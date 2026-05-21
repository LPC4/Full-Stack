//! 5-stage pipeline: orchestrates Fetch -> Decode -> Execute -> Memory -> Writeback.
//!
//! Implements data forwarding, load-use stall detection, and 2-bit bimodal
//! branch prediction with BTB; 2-cycle flush on mispredict.

pub mod decode;
pub mod execute;
pub mod fetch;
pub mod memory;
pub mod registers;
pub mod writeback;

use crate::bus::SystemBus;
use crate::cpu::csr::{CsrFile, CsrSnapshot};
use crate::cpu::decoder::{DecodedInsn, decode as decode_insn};
use crate::cpu::hazard_unit::{
    compute_forwarding, insn_is_atomic, insn_is_fp_dest, insn_is_load, insn_rd, insn_rs1, insn_rs2,
    load_use_hazard,
};
use crate::cpu::pipeline::execute::ExecResult;
use crate::cpu::pipeline::memory::MemResult;
use crate::cpu::pipeline::registers::{EXMEMReg, IDEXReg, IFIDReg, MEMWBReg};
use crate::cpu::predictor::BranchPredictor;
use crate::cpu::registers::{PrivilegeMode, Registers};
use crate::cpu::traps;
use crate::error::VmError;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum TickOutcome {
    Continue,
    Halted(i64),
    /// An ecall was serviced in WB. The pipeline was squashed; remaining stages
    /// must not commit their results. tick() early-returns after seeing this.
    EcallSquash,
}

#[derive(Default, Debug, Clone)]
pub struct PipelineStats {
    pub cycles: u64,
    pub insns_retired: u64,
    pub stall_cycles: u64,
    pub flush_cycles: u64,
    pub branches_seen: u64,
    pub branches_mispredicted: u64,
}

/// Per-cycle pipeline stage snapshot: (pc, mnemonic) or None for a bubble.
pub type StageEntry = Option<(u64, &'static str)>;

/// Raw pipeline state feed captured after each tick (one entry per stage).
#[derive(Clone, Debug)]
pub struct CpuPipelineFeed {
    /// `stages[0]` = IF ... `stages[4]` = WB.  `None` means bubble/empty.
    pub stages: [StageEntry; 5],
    /// IF stage was held (load-use stall).
    pub stalled: bool,
    /// IF/ID were squashed (branch mispredict).
    pub flushed: bool,
}

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------

pub struct Pipeline {
    regs: Registers,
    csrs: CsrFile,
    reservation: Option<u64>,
    /// PC of the next instruction to fetch.
    fetch_pc: u64,

    if_id: Option<IFIDReg>,
    id_ex: Option<IDEXReg>,
    ex_mem: Option<EXMEMReg>,
    mem_wb: Option<MEMWBReg>,

    /// Flag to prevent speculative fetch when MRET/SRET is in flight.
    /// Set in ID stage when MRET/SRET is decoded, cleared after WB flushes.
    mret_in_flight: bool,

    predictor: BranchPredictor,
    pub stats: PipelineStats,
    /// Snapshot of the pipeline state produced by the most recent tick.
    pub last_cycle: CpuPipelineFeed,
}

impl Pipeline {
    pub fn new(start_pc: u64, stack_ptr: u64) -> Self {
        let mut regs = Registers::new();
        regs.pc = start_pc;
        regs.write_x(2, stack_ptr);

        let mut csrs = CsrFile::new();
        csrs.mtvec = crate::rom::M_TRAP_ADDR; // _m_trap at ROM_BASE + 0x100
        csrs.mscratch = stack_ptr - 4096; // M-mode stack top, 4 KB below user stack
        Self {
            regs,
            csrs,
            reservation: None,
            fetch_pc: start_pc,
            if_id: None,
            id_ex: None,
            ex_mem: None,
            mem_wb: None,
            mret_in_flight: false,
            predictor: BranchPredictor::new(),
            stats: PipelineStats::default(),
            last_cycle: CpuPipelineFeed {
                stages: [None; 5],
                stalled: false,
                flushed: false,
            },
        }
    }

    pub fn set_return_addr(&mut self, ra: u64) {
        self.regs.write_x(1, ra);
    }

    /// Reset the program counter and flush the pipeline.
    ///
    /// Used by `VirtualMachine::new_kernel` to redirect the CPU to ROM_BASE
    /// after the ELF has been loaded into RAM, so that the ROM's `_start`
    /// boot stub runs first.
    pub fn reset_pc(&mut self, pc: u64) {
        self.regs.pc = pc;
        self.fetch_pc = pc;
        self.if_id = None;
        self.id_ex = None;
        self.ex_mem = None;
        self.mem_wb = None;
    }

    /// Set register a0 (x10) to the kernel entry point so ROM `_start` can
    /// `csrw mepc, a0` without knowing the address at assembly time.
    pub fn set_boot_entry(&mut self, entry: u64) {
        self.regs.write_x(10, entry);
    }

    pub fn write_csr_mtvec(&mut self, val: u64) {
        self.csrs.mtvec = val;
    }

    pub fn peek_reg(&self, r: usize) -> u64 {
        self.regs.read_x(r)
    }

    pub fn peek_fp_reg(&self, r: usize) -> u64 {
        self.regs.read_f_bits(r)
    }

    pub fn peek_pc(&self) -> u64 {
        self.regs.pc
    }

    pub fn peek_csr_mcause(&self) -> u64 {
        self.csrs.mcause
    }

    pub fn peek_csr_mtvec(&self) -> u64 {
        self.csrs.mtvec
    }

    pub fn peek_all_xregs(&self) -> [u64; 32] {
        std::array::from_fn(|i| self.regs.read_x(i))
    }

    pub fn peek_all_fregs(&self) -> [u64; 32] {
        std::array::from_fn(|i| self.regs.read_f_bits(i))
    }

    pub fn peek_csrs(&self) -> CsrSnapshot {
        self.csrs.snapshot()
    }

    pub fn predictor_stats(&self) -> &crate::cpu::predictor::PredictorStats {
        self.predictor.stats()
    }

    // -----------------------------------------------------------------------
    // Main tick, advance the pipeline by one clock cycle
    // -----------------------------------------------------------------------

    /// Advance every pipeline stage by one cycle.
    ///
    /// Stages execute in reverse order (WB -> MEM -> EX -> ID -> IF) so each
    /// stage can pass its results forward within the same cycle.
    pub fn tick(&mut self, bus: &mut SystemBus) -> Result<TickOutcome, VmError> {
        self.stats.cycles += 1;
        self.csrs.increment_cycle();

        // Advance CLINT timer and sync hardware interrupt bits into mip.
        {
            let clint = bus.clint_mut();
            clint.tick();
            if clint.timer_irq_pending() {
                self.csrs.mip |= 1u64 << 7; // MTIP
            } else {
                self.csrs.mip &= !(1u64 << 7);
            }
            if clint.software_irq_pending() {
                self.csrs.mip |= 1u64 << 3; // MSIP
            } else {
                self.csrs.mip &= !(1u64 << 3);
            }
        }

        // Snapshot pipeline state at the START of this cycle (before any stage runs).
        let snap_wb_entry: StageEntry = self.mem_wb.as_ref().map(|r| (r.pc, r.mnemonic));
        let snap_mem_entry: StageEntry = self.ex_mem.as_ref().map(|r| (r.pc, r.mnemonic));
        let snap_ex_entry: StageEntry = self.id_ex.as_ref().map(|r| (r.pc, r.mnemonic));
        let snap_id_entry: StageEntry = self.if_id.as_ref().map(|r| {
            let mnem = decode_insn(r.raw).map(|i| i.mnemonic()).unwrap_or("???");
            (r.pc, mnem)
        });

        let old_mem_wb = self.mem_wb.take();
        let old_ex_mem = self.ex_mem.take();
        let old_id_ex = self.id_ex.take();
        let old_if_id = self.if_id.take();

        // ---- WB stage -------------------------------------------------------
        let wb_outcome = self.stage_wb(old_mem_wb.as_ref(), bus)?;
        match wb_outcome {
            TickOutcome::Halted(code) => return Ok(TickOutcome::Halted(code)),
            TickOutcome::EcallSquash => {
                self.flush_pipeline();
                return Ok(TickOutcome::Continue);
            }
            TickOutcome::Continue => {}
        }

        // ---- Interrupt check (precise: after WB retires, before MEM executes) ----
        // In-flight instructions (old_ex_mem, old_id_ex, old_if_id) are squashed if
        // an interrupt fires; their pipeline registers are already taken out of self.
        if let Some((irq_cause, irq_tval, irq_pc)) = self.check_pending_interrupt() {
            // old_ex_mem / old_id_ex / old_if_id drop here - no memory side-effects yet.
            self.take_trap(irq_cause, irq_tval, irq_pc);
            self.last_cycle = CpuPipelineFeed {
                stages: [
                    None,
                    snap_id_entry,
                    snap_ex_entry,
                    snap_mem_entry,
                    snap_wb_entry,
                ],
                stalled: false,
                flushed: true,
            };
            return Ok(TickOutcome::Continue);
        }

        // ---- MEM stage ------------------------------------------------------
        let new_mem_wb = self.stage_mem(old_ex_mem.as_ref(), bus)?;

        // ---- EX stage -------------------------------------------------------
        let (new_ex_mem, flush, redirect_pc) =
            self.stage_ex(old_id_ex.as_ref(), old_ex_mem.as_ref(), old_mem_wb.as_ref())?;

        // ---- Hazard detection -----------------------------------------------
        let stall = match (old_id_ex.as_ref(), old_if_id.as_ref()) {
            (Some(id_ex), Some(if_id)) => load_use_hazard(id_ex, if_id.rs1, if_id.rs2),
            _ => false,
        };

        // ---- ID stage -------------------------------------------------------
        let new_id_ex = if flush || stall {
            None
        } else {
            self.stage_id(old_if_id.as_ref())?
        };

        // ---- IF stage -------------------------------------------------------
        let new_if_id = if stall || self.mret_in_flight {
            // Stall IF when there's a load-use hazard OR when MRET/SRET is in flight
            // This prevents speculative fetch past control-flow changing instructions
            old_if_id
        } else if flush {
            self.fetch_pc = redirect_pc;
            None
        } else {
            self.stage_if(bus)?
        };

        // ---- Update pipeline registers --------------------------------------
        self.mem_wb = new_mem_wb;
        self.ex_mem = new_ex_mem;
        self.id_ex = new_id_ex;
        self.if_id = new_if_id;

        // ---- Stats ----------------------------------------------------------
        if stall {
            self.stats.stall_cycles += 1;
        }
        if flush {
            self.stats.flush_cycles += 2;
        }

        // ---- Capture cycle snapshot -----------------------------------------
        let snap_if_entry: StageEntry = if stall {
            snap_id_entry
        } else if flush {
            None
        } else {
            self.if_id.as_ref().map(|r| {
                let mnem = decode_insn(r.raw).map(|i| i.mnemonic()).unwrap_or("???");
                (r.pc, mnem)
            })
        };

        self.last_cycle = CpuPipelineFeed {
            stages: [
                snap_if_entry,  // IF
                snap_id_entry,  // ID
                snap_ex_entry,  // EX
                snap_mem_entry, // MEM
                snap_wb_entry,  // WB
            ],
            stalled: stall,
            flushed: flush,
        };

        Ok(TickOutcome::Continue)
    }

    // -----------------------------------------------------------------------
    // Stage implementations
    // -----------------------------------------------------------------------

    fn stage_if(&mut self, bus: &mut SystemBus) -> Result<Option<IFIDReg>, VmError> {
        let pc = self.fetch_pc;

        let raw = match fetch::fetch_with_pmp(
            bus,
            pc,
            self.csrs.satp,
            self.regs.priv_mode,
            self.csrs.mstatus,
            self.csrs.pmpcfg0,
            self.csrs.pmpaddr0,
        ) {
            Ok(r) => r,
            Err(e) => {
                self.flush_and_trap(e, pc);
                return Ok(None);
            }
        };

        let rs1 = ((raw >> 15) & 0x1f) as usize;
        let rs2 = ((raw >> 20) & 0x1f) as usize;

        let (predicted_taken, predicted_target) = self.predictor.predict(pc);
        self.fetch_pc = if predicted_taken {
            predicted_target
        } else {
            pc.wrapping_add(4)
        };

        Ok(Some(IFIDReg {
            pc,
            raw,
            rs1,
            rs2,
            predicted_taken,
            predicted_target,
        }))
    }

    fn stage_id(&mut self, if_id: Option<&IFIDReg>) -> Result<Option<IDEXReg>, VmError> {
        let if_id = match if_id {
            Some(r) => r,
            None => return Ok(None),
        };

        let insn = match decode::decode(if_id.raw) {
            Ok(i) => i,
            Err(e) => {
                self.flush_and_trap(e, if_id.pc);
                return Ok(None);
            }
        };

        let mnemonic = insn.mnemonic();
        let rs1 = insn_rs1(&insn);
        let rs2 = insn_rs2(&insn);
        let rd = insn_rd(&insn);
        let is_fp_dest = insn_is_fp_dest(&insn);
        let is_load = insn_is_load(&insn);
        let is_fp_load = matches!(insn, DecodedInsn::FLoad { .. });

        // Detect MRET/SRET to prevent speculative fetch past these instructions
        if matches!(insn, DecodedInsn::Mret | DecodedInsn::Sret) {
            self.mret_in_flight = true;
        }

        let (frs1, frs2) = match &insn {
            DecodedInsn::FStore { rs2, .. } => (rs1, *rs2),
            DecodedInsn::FOp {
                rs1: r1, rs2: r2, ..
            } => (*r1, *r2),
            DecodedInsn::FMac {
                rs1: r1, rs2: r2, ..
            } => (*r1, *r2),
            _ => (0, 0),
        };

        Ok(Some(IDEXReg {
            pc: if_id.pc,
            mnemonic,
            rs1_val: self.regs.read_x(rs1),
            rs2_val: self.regs.read_x(rs2),
            frs1_val: self.regs.read_f_bits(frs1),
            frs2_val: self.regs.read_f_bits(frs2),
            insn,
            rs1,
            rs2,
            frs1,
            frs2,
            rd,
            is_fp_dest,
            is_load,
            is_fp_load,
            predicted_taken: if_id.predicted_taken,
            predicted_target: if_id.predicted_target,
        }))
    }

    fn stage_ex(
        &mut self,
        id_ex: Option<&IDEXReg>,
        old_ex_mem: Option<&EXMEMReg>,
        old_mem_wb: Option<&MEMWBReg>,
    ) -> Result<(Option<EXMEMReg>, bool, u64), VmError> {
        let id_ex = match id_ex {
            Some(r) => r,
            None => return Ok((None, false, 0)),
        };

        let fwd = compute_forwarding(old_ex_mem, old_mem_wb, id_ex);

        let save_x_rs1 = self.regs.read_x(id_ex.rs1);
        let save_x_rs2 = self.regs.read_x(id_ex.rs2);
        let save_f_rs1 = self.regs.read_f_bits(id_ex.frs1);
        let save_f_rs2 = self.regs.read_f_bits(id_ex.frs2);
        self.regs.write_x(id_ex.rs1, fwd.rs1);
        self.regs.write_x(id_ex.rs2, fwd.rs2);
        self.regs.write_f_bits(id_ex.frs1, fwd.frs1);
        self.regs.write_f_bits(id_ex.frs2, fwd.frs2);

        let exec_result = execute::execute(&id_ex.insn, &self.regs, &self.csrs, id_ex.pc);

        self.regs.write_x(id_ex.rs1, save_x_rs1);
        self.regs.write_x(id_ex.rs2, save_x_rs2);
        self.regs.write_f_bits(id_ex.frs1, save_f_rs1);
        self.regs.write_f_bits(id_ex.frs2, save_f_rs2);

        let exec_result = match exec_result {
            Ok(r) => r,
            Err(e) => {
                self.flush_and_trap(e, id_ex.pc);
                return Ok((None, false, 0));
            }
        };

        let (actual_next_pc, actual_taken, is_branch) =
            self.resolve_control_flow(&exec_result, id_ex.pc, fwd.rs1);

        let is_load = id_ex.is_load || insn_is_atomic(&id_ex.insn);
        let (rd, is_fp_dest, fwd_val) = ex_forwarding_info(&exec_result, id_ex);

        if is_branch {
            self.stats.branches_seen += 1;
            self.predictor.update(
                id_ex.pc,
                actual_taken,
                actual_next_pc,
                id_ex.predicted_taken,
            );
        }

        let mispredict = is_branch
            && (actual_taken != id_ex.predicted_taken
                || (actual_taken && actual_next_pc != id_ex.predicted_target));

        if mispredict {
            self.stats.branches_mispredicted += 1;
        }

        Ok((
            Some(EXMEMReg {
                pc: id_ex.pc,
                mnemonic: id_ex.mnemonic,
                exec_result,
                rd,
                is_fp_dest,
                fwd_val,
                is_load,
                actual_next_pc,
                actual_taken,
                predicted_taken: id_ex.predicted_taken,
                predicted_target: id_ex.predicted_target,
            }),
            mispredict,
            actual_next_pc,
        ))
    }

    fn stage_mem(
        &mut self,
        ex_mem: Option<&EXMEMReg>,
        bus: &mut SystemBus,
    ) -> Result<Option<MEMWBReg>, VmError> {
        let ex_mem = match ex_mem {
            Some(r) => r,
            None => return Ok(None),
        };

        let mem_result = match memory::memory_stage_with_pmp(
            ex_mem.exec_result.clone(),
            bus,
            &mut self.reservation,
            self.csrs.satp,
            self.regs.priv_mode,
            self.csrs.mstatus,
            self.csrs.pmpcfg0,
            self.csrs.pmpaddr0,
        ) {
            Ok(r) => r,
            Err(e) => {
                self.flush_and_trap(e, ex_mem.pc);
                return Ok(None);
            }
        };

        let (fwd_rd, is_fp_dest, fwd_val) = mem_forwarding_info(&mem_result, ex_mem);

        Ok(Some(MEMWBReg {
            pc: ex_mem.pc,
            mnemonic: ex_mem.mnemonic,
            rd: fwd_rd,
            is_fp_dest,
            fwd_val,
            mem_result,
        }))
    }

    fn stage_wb(
        &mut self,
        mem_wb: Option<&MEMWBReg>,
        bus: &mut SystemBus,
    ) -> Result<TickOutcome, VmError> {
        let mem_wb = match mem_wb {
            Some(r) => r,
            None => return Ok(TickOutcome::Continue),
        };

        let next_pc =
            match writeback::writeback(mem_wb.mem_result.clone(), &mut self.regs, &mut self.csrs) {
                Ok(pc) => pc,
                Err(VmError::Ecall) => return self.handle_ecall(bus),
                Err(VmError::Ebreak) => {
                    let pc = self.regs.pc;
                    self.flush_pipeline();
                    self.take_trap(traps::CAUSE_EBREAK, 0, pc);
                    return Ok(TickOutcome::Continue);
                }
                Err(VmError::Mret) => {
                    self.handle_mret();
                    return Ok(TickOutcome::EcallSquash);
                }
                Err(VmError::Sret) => {
                    self.handle_sret();
                    return Ok(TickOutcome::Continue);
                }
                Err(e) => {
                    let pc = self.regs.pc;
                    self.flush_and_trap(e, pc);
                    return Ok(TickOutcome::Continue);
                }
            };

        self.regs.pc = next_pc;
        self.csrs.increment_instret();
        self.stats.insns_retired += 1;
        Ok(TickOutcome::Continue)
    }

    // -----------------------------------------------------------------------
    // Control flow resolution
    // -----------------------------------------------------------------------

    fn resolve_control_flow(
        &self,
        result: &ExecResult,
        pc: u64,
        rs1_val: u64,
    ) -> (u64, bool, bool) {
        match result {
            ExecResult::WriteInt { next_pc, .. }
            | ExecResult::WriteFp { next_pc, .. }
            | ExecResult::WriteIntFlags { next_pc, .. }
            | ExecResult::WriteFpFlags { next_pc, .. }
            | ExecResult::Load { next_pc, .. }
            | ExecResult::Store { next_pc, .. }
            | ExecResult::FLoad { next_pc, .. }
            | ExecResult::FStore { next_pc, .. }
            | ExecResult::Csr { next_pc, .. }
            | ExecResult::Fence { next_pc }
            | ExecResult::FenceI { next_pc } => {
                let sequential = pc.wrapping_add(4);
                let taken = *next_pc != sequential;
                (*next_pc, taken, taken)
            }
            ExecResult::Jump { next_pc } => {
                let _ = rs1_val;
                (*next_pc, true, true)
            }
            ExecResult::Atomic { next_pc, .. } => {
                let taken = *next_pc != pc.wrapping_add(4);
                (*next_pc, taken, false)
            }
            ExecResult::Ecall
            | ExecResult::Ebreak
            | ExecResult::Mret
            | ExecResult::Sret
            | ExecResult::SfenceVma => (pc.wrapping_add(4), false, false),
        }
    }

    // -----------------------------------------------------------------------
    // Trap / interrupt helpers
    // -----------------------------------------------------------------------

    fn flush_pipeline(&mut self) {
        self.if_id = None;
        self.id_ex = None;
        self.ex_mem = None;
        self.mem_wb = None;
    }

    fn flush_and_trap(&mut self, e: VmError, pc: u64) {
        self.flush_pipeline();
        if let Some((cause, tval)) = traps::error_to_trap_cause(&e) {
            self.take_trap(cause, tval, pc);
        }
    }

    fn take_trap(&mut self, cause: u64, tval: u64, pc: u64) {
        let new_pc = traps::take_trap(&mut self.regs, &mut self.csrs, cause, tval, pc);
        self.fetch_pc = new_pc;
        self.regs.pc = new_pc;
    }

    fn handle_mret(&mut self) {
        let new_pc = traps::handle_mret(&mut self.regs, &mut self.csrs);
        self.fetch_pc = new_pc;
        self.regs.pc = new_pc;
        self.flush_pipeline();
        self.mret_in_flight = false; // Clear the flag after MRET completes
        self.csrs.increment_instret();
        self.stats.insns_retired += 1;
    }

    fn handle_sret(&mut self) {
        match traps::handle_sret(&mut self.regs, &mut self.csrs) {
            Ok(new_pc) => {
                self.fetch_pc = new_pc;
                self.regs.pc = new_pc;
                self.flush_pipeline();
                self.mret_in_flight = false; // Clear the flag after SRET completes
                self.csrs.increment_instret();
                self.stats.insns_retired += 1;
            }
            Err(e) => {
                self.flush_pipeline();
                if let Some((cause, tval)) = traps::error_to_trap_cause(&e) {
                    self.take_trap(cause, tval, self.regs.pc);
                }
            }
        }
    }

    fn handle_ecall(&mut self, _bus: &mut SystemBus) -> Result<TickOutcome, VmError> {
        let ecall_pc = self.regs.pc;
        let cause = match self.regs.priv_mode {
            PrivilegeMode::User => traps::CAUSE_ECALL_U,
            PrivilegeMode::Supervisor => traps::CAUSE_ECALL_S,
            PrivilegeMode::Machine => traps::CAUSE_ECALL_M,
        };
        self.take_trap(cause, 0, ecall_pc);
        self.flush_pipeline();
        Ok(TickOutcome::EcallSquash)
    }

    /// Check whether a pending interrupt should be taken right now.
    ///
    /// Returns `Some((cause, tval, pc))` when an interrupt is ready to fire.
    /// Priority order per spec: MEI(11) > MSI(3) > MTI(7), then SEI(9) > SSI(1) > STI(5).
    fn check_pending_interrupt(&self) -> Option<(u64, u64, u64)> {
        let pending = self.csrs.mip & self.csrs.mie;
        if pending == 0 {
            return None;
        }

        let in_m = self.regs.priv_mode == PrivilegeMode::Machine;
        let in_s = self.regs.priv_mode == PrivilegeMode::Supervisor;
        let mie_global = (self.csrs.mstatus >> 3) & 1; // mstatus.MIE
        let sie_global = (self.csrs.mstatus >> 1) & 1; // mstatus.SIE

        // M-mode interrupts: not delegated to S-mode.
        // Taken if: not in M-mode (lower modes are always preempted), or MIE=1 in M-mode.
        let m_pending = pending & !self.csrs.mideleg;
        if m_pending != 0 && (!in_m || mie_global == 1) {
            let cause_idx = if m_pending & (1 << 11) != 0 {
                11u64
            } else if m_pending & (1 << 3) != 0 {
                3u64
            } else if m_pending & (1 << 7) != 0 {
                7u64
            } else {
                m_pending.trailing_zeros() as u64
            };
            return Some(((1u64 << 63) | cause_idx, 0, self.regs.pc));
        }

        // S-mode interrupts: delegated; only relevant when not in M-mode.
        let s_pending = pending & self.csrs.mideleg;
        if s_pending != 0 && !in_m && (!in_s || sie_global == 1) {
            let cause_idx = if s_pending & (1 << 9) != 0 {
                9u64
            } else if s_pending & (1 << 1) != 0 {
                1u64
            } else if s_pending & (1 << 5) != 0 {
                5u64
            } else {
                s_pending.trailing_zeros() as u64
            };
            return Some(((1u64 << 63) | cause_idx, 0, self.regs.pc));
        }

        None
    }

    // -----------------------------------------------------------------------
    // run() helper
    // -----------------------------------------------------------------------

    pub fn run(&mut self, bus: &mut SystemBus, max_cycles: u64) -> (TickOutcome, String) {
        let mut outcome = TickOutcome::Continue;
        for _ in 0..max_cycles {
            match self.tick(bus) {
                Ok(TickOutcome::Continue | TickOutcome::EcallSquash) => {
                    if let Some(code) = bus.take_syscon_exit() {
                        outcome = TickOutcome::Halted(code);
                        break;
                    }
                }
                Ok(TickOutcome::Halted(code)) => {
                    outcome = TickOutcome::Halted(code);
                    break;
                }
                Err(e) => {
                    eprintln!("[pipeline] tick error: {e:?}");
                    outcome = TickOutcome::Halted(-1);
                    break;
                }
            }
        }
        let uart_bytes = bus.uart_mut().drain_output();
        let uart_output = String::from_utf8_lossy(&uart_bytes).into_owned();
        (outcome, uart_output)
    }
}

// ---------------------------------------------------------------------------
// Forwarding value helpers
// ---------------------------------------------------------------------------

fn ex_forwarding_info(result: &ExecResult, id_ex: &IDEXReg) -> (usize, bool, u64) {
    match result {
        ExecResult::WriteInt { rd, val, .. } => (*rd, false, *val),
        ExecResult::WriteFp { rd, bits, .. } => (*rd, true, *bits),
        ExecResult::WriteIntFlags { rd, val, .. } => (*rd, false, *val),
        ExecResult::WriteFpFlags { rd, bits, .. } => (*rd, true, *bits),
        ExecResult::Jump { .. } => (0, false, 0),
        ExecResult::Load { rd, .. } => (*rd, false, 0),
        ExecResult::FLoad { rd, .. } => (*rd, true, 0),
        ExecResult::Atomic { rd, .. } => (*rd, false, 0),
        ExecResult::Csr { rd, old_val, .. } => (*rd, false, *old_val),
        _ => (id_ex.rd, id_ex.is_fp_dest, 0),
    }
}

fn mem_forwarding_info(result: &MemResult, ex_mem: &EXMEMReg) -> (usize, bool, u64) {
    match result {
        MemResult::WriteInt { rd, val, .. } => (*rd, false, *val),
        MemResult::WriteFp { rd, bits, .. } => (*rd, true, *bits),
        MemResult::WriteIntFlags { rd, val, .. } => (*rd, false, *val),
        MemResult::WriteFpFlags { rd, bits, .. } => (*rd, true, *bits),
        MemResult::Jump { .. } => (0, false, 0),
        MemResult::Csr { rd, old_val, .. } => (*rd, false, *old_val),
        _ => (ex_mem.rd, ex_mem.is_fp_dest, ex_mem.fwd_val),
    }
}
