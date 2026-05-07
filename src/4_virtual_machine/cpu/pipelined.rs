//! 5-stage pipelined CPU simulation for RV64IMAFD.
//!
//! Implements the Fetch -> Decode -> Execute -> Memory -> Writeback pipeline with:
//! - Data forwarding (EX/MEM -> EX, MEM/WB -> EX) for integer and FP registers.
//! - Load-use stall detection (1-cycle bubble on load-use hazard).
//! - 2-bit bimodal branch prediction with BTB; 2-cycle flush on mispredict.
//!
//! Traps are handled imprecisely but safely: on any exception the pipeline is
//! fully flushed and the trap handler is invoked.

use crate::virtual_machine::bus::SystemBus;
use crate::virtual_machine::cpu::csr::{CsrFile, CsrSnapshot};
use crate::virtual_machine::cpu::decoder::{DecodedInsn, decode as decode_insn};
use crate::virtual_machine::cpu::pipeline::execute::ExecResult;
use crate::virtual_machine::cpu::pipeline::hazard::{
    compute_forwarding, insn_is_atomic, insn_is_fp_dest, insn_is_load, insn_rd, insn_rs1, insn_rs2,
    load_use_hazard,
};
use crate::virtual_machine::cpu::pipeline::memory::MemResult;
use crate::virtual_machine::cpu::pipeline::predictor::BranchPredictor;
use crate::virtual_machine::cpu::pipeline::registers::{EXMEMReg, IDEXReg, IFIDReg, MEMWBReg};
use crate::virtual_machine::cpu::pipeline::{decode, execute, fetch, memory, writeback};
use crate::virtual_machine::cpu::registers::{PrivilegeMode, Registers};
use crate::virtual_machine::error::VmError;
use crate::virtual_machine::memory::MemoryAccess;

// ---------------------------------------------------------------------------
// mcause constants (duplicated from cpu_impl)
// ---------------------------------------------------------------------------

const CAUSE_INSN_ACCESS_FAULT: u64 = 1;
const CAUSE_ILLEGAL_INSN: u64 = 2;
const CAUSE_EBREAK: u64 = 3;
const CAUSE_LOAD_ACCESS_FAULT: u64 = 5;
const CAUSE_STORE_ACCESS_FAULT: u64 = 7;
#[allow(dead_code)]
const CAUSE_ECALL_U: u64 = 8;
#[allow(dead_code)]
const CAUSE_ECALL_S: u64 = 9;
#[allow(dead_code)]
const CAUSE_ECALL_M: u64 = 11;

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

// ---------------------------------------------------------------------------
// PipelinedCpu
// ---------------------------------------------------------------------------

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

pub struct PipelinedCpu {
    regs: Registers,
    csrs: CsrFile,
    reservation: Option<u64>,
    /// PC of the next instruction to fetch.
    fetch_pc: u64,

    if_id: Option<IFIDReg>,
    id_ex: Option<IDEXReg>,
    ex_mem: Option<EXMEMReg>,
    mem_wb: Option<MEMWBReg>,

    predictor: BranchPredictor,
    pub stats: PipelineStats,
    /// Snapshot of the pipeline state produced by the most recent tick.
    pub last_cycle: CpuPipelineFeed,
}

impl PipelinedCpu {
    pub fn new(start_pc: u64, stack_ptr: u64) -> Self {
        let mut regs = Registers::new();
        regs.pc = start_pc;
        regs.write_x(2, stack_ptr);
        Self {
            regs,
            csrs: CsrFile::new(),
            reservation: None,
            fetch_pc: start_pc,
            if_id: None,
            id_ex: None,
            ex_mem: None,
            mem_wb: None,
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

    pub fn predictor_stats(
        &self,
    ) -> &crate::virtual_machine::cpu::pipeline::predictor::PredictorStats {
        self.predictor.stats()
    }

    // -----------------------------------------------------------------------
    // Main tick, advance the pipeline by one clock cycle
    // -----------------------------------------------------------------------

    /// Advance every pipeline stage by one cycle.  Returns `Continue` until the
    /// program halts via ecall(93) or the pipeline drains after a halt.
    pub fn tick(&mut self, bus: &mut SystemBus) -> Result<TickOutcome, VmError> {
        self.stats.cycles += 1;
        self.csrs.increment_cycle();

        // Snapshot pipeline state at the START of this cycle (before any stage runs).
        // stages: [IF, ID, EX, MEM, WB] = what each stage is processing this cycle.
        let snap_wb_entry: StageEntry = self.mem_wb.as_ref().map(|r| (r.pc, r.mnemonic));
        let snap_mem_entry: StageEntry = self.ex_mem.as_ref().map(|r| (r.pc, r.mnemonic));
        let snap_ex_entry: StageEntry = self.id_ex.as_ref().map(|r| (r.pc, r.mnemonic));
        let snap_id_entry: StageEntry = self.if_id.as_ref().map(|r| {
            let mnem = decode_insn(r.raw).map(|i| i.mnemonic()).unwrap_or("???");
            (r.pc, mnem)
        });

        // Take snapshot of the current (old) pipeline state
        let old_mem_wb = self.mem_wb.take();
        let old_ex_mem = self.ex_mem.take();
        let old_id_ex = self.id_ex.take();
        let old_if_id = self.if_id.take();

        // ---- WB stage -------------------------------------------------------
        let wb_outcome = self.stage_wb(old_mem_wb.as_ref(), bus)?;
        match wb_outcome {
            TickOutcome::Halted(code) => return Ok(TickOutcome::Halted(code)),
            TickOutcome::EcallSquash => {
                // An ecall was serviced. fetch_pc is already set to the next
                // instruction after the ecall (the `jalr` ret stub). The old
                // pipeline latches (old_ex_mem / old_id_ex / old_if_id) contain
                // instructions that were speculatively fetched past the ecall and
                // must be squashed -- we do that by early-returning here so their
                // results never get written to self.{mem_wb, ex_mem, id_ex, if_id}.
                self.flush_pipeline();
                return Ok(TickOutcome::Continue);
            }
            TickOutcome::Continue => {}
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
            None // bubble
        } else {
            self.stage_id(old_if_id.as_ref())?
        };

        // ---- IF stage -------------------------------------------------------
        let new_if_id = if stall {
            old_if_id // preserve stalled instruction
        } else if flush {
            self.fetch_pc = redirect_pc;
            None // fetch from redirected PC next cycle
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
        // IF slot: what was fetched this cycle (or held/squashed).
        let snap_if_entry: StageEntry = if stall {
            // IF was held -- same instruction as before (show it as stalled).
            snap_id_entry // the instruction that was in IF is still there
        } else if flush {
            None // squashed
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

        let raw = match fetch::fetch(bus, pc, self.csrs.satp, self.regs.priv_mode) {
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

        // For FP instructions the source FP registers sit in the same bit positions
        // as integer rs1/rs2 but are read from the FP register file.
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

        // Compute forwarded register values
        let fwd = compute_forwarding(old_ex_mem, old_mem_wb, id_ex);

        // Apply forwarded values by temporarily overriding the register file
        let save_x_rs1 = self.regs.read_x(id_ex.rs1);
        let save_x_rs2 = self.regs.read_x(id_ex.rs2);
        let save_f_rs1 = self.regs.read_f_bits(id_ex.frs1);
        let save_f_rs2 = self.regs.read_f_bits(id_ex.frs2);
        self.regs.write_x(id_ex.rs1, fwd.rs1);
        self.regs.write_x(id_ex.rs2, fwd.rs2);
        self.regs.write_f_bits(id_ex.frs1, fwd.frs1);
        self.regs.write_f_bits(id_ex.frs2, fwd.frs2);

        let exec_result = execute::execute(&id_ex.insn, &self.regs, &self.csrs, id_ex.pc);

        // Restore register file
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

        // Determine the actual next PC and whether a branch was taken
        let (actual_next_pc, actual_taken, is_branch) =
            self.resolve_control_flow(&exec_result, id_ex.pc, fwd.rs1);

        let is_load = id_ex.is_load || insn_is_atomic(&id_ex.insn);
        let (rd, is_fp_dest, fwd_val) = ex_forwarding_info(&exec_result, id_ex);

        // Update the branch predictor
        if is_branch {
            self.stats.branches_seen += 1;
            self.predictor.update(
                id_ex.pc,
                actual_taken,
                actual_next_pc,
                id_ex.predicted_taken,
            );
        }

        // Detect mispredict: wrong taken/not-taken OR wrong target
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

        let mem_result = match memory::memory_stage(
            ex_mem.exec_result.clone(),
            bus,
            &mut self.reservation,
            self.csrs.satp,
            self.regs.priv_mode,
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
                    self.take_trap(CAUSE_EBREAK, 0, pc);
                    return Ok(TickOutcome::Continue);
                }
                Err(VmError::Other(ref msg)) if msg == "MRET" => {
                    self.handle_mret();
                    return Ok(TickOutcome::Continue);
                }
                Err(VmError::Other(ref msg)) if msg == "SRET" => {
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

    /// Returns (actual_next_pc, was_taken, is_branch_or_jump).
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
                // Check if this is a Jal/Jalr (which changes flow unconditionally)
                let sequential = pc.wrapping_add(4);
                let taken = *next_pc != sequential;
                (
                    *next_pc, taken, taken, // only flag jumps as "branches" when they deviate
                )
            }
            ExecResult::Jump { next_pc } => {
                // Unconditional Jal/Jalr already computed to a non-PC+4 target
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
        let (cause, tval) = match &e {
            VmError::InstructionAccessFault(a) => (CAUSE_INSN_ACCESS_FAULT, *a),
            VmError::IllegalInstruction(i) => (CAUSE_ILLEGAL_INSN, *i as u64),
            VmError::LoadAccessFault(a) | VmError::BusError(a) => (CAUSE_LOAD_ACCESS_FAULT, *a),
            VmError::StoreAccessFault(a) => (CAUSE_STORE_ACCESS_FAULT, *a),
            _ => return, // fatal,caller will see Err if needed
        };
        self.take_trap(cause, tval, pc);
    }

    fn take_trap(&mut self, cause: u64, tval: u64, pc: u64) {
        self.csrs.mepc = pc & !0x3u64;
        self.csrs.mcause = cause;
        self.csrs.mtval = tval;

        let mie_bit = (self.csrs.mstatus >> 3) & 1;
        self.csrs.mstatus &= !(1u64 << 7);
        self.csrs.mstatus |= mie_bit << 7;
        self.csrs.mstatus &= !(1u64 << 3);
        let mode = self.regs.priv_mode as u64;
        self.csrs.mstatus &= !(0x3u64 << 11);
        self.csrs.mstatus |= mode << 11;

        let mtvec = self.csrs.mtvec;
        let vmode = mtvec & 0x3;
        let base = mtvec & !0x3u64;
        self.fetch_pc = if vmode == 1 && (cause & (1u64 << 63)) != 0 {
            base + 4 * (cause & !(1u64 << 63))
        } else {
            base
        };
        self.regs.pc = self.fetch_pc;
        self.regs.priv_mode = PrivilegeMode::Machine;
    }

    fn handle_mret(&mut self) {
        let mpie = (self.csrs.mstatus >> 7) & 1;
        self.csrs.mstatus &= !(1u64 << 3);
        self.csrs.mstatus |= mpie << 3;
        self.csrs.mstatus |= 1u64 << 7;
        let mpp = (self.csrs.mstatus >> 11) & 0x3;
        self.regs.priv_mode = match mpp {
            0 => PrivilegeMode::User,
            1 => PrivilegeMode::Supervisor,
            _ => PrivilegeMode::Machine,
        };
        self.csrs.mstatus &= !(0x3u64 << 11);
        self.regs.pc = self.csrs.mepc;
        self.fetch_pc = self.regs.pc;
        self.flush_pipeline();
        self.csrs.increment_instret();
        self.stats.insns_retired += 1;
    }

    fn handle_sret(&mut self) {
        self.regs.priv_mode = PrivilegeMode::User;
        self.regs.pc = self.csrs.sepc;
        self.fetch_pc = self.regs.pc;
        self.flush_pipeline();
        self.csrs.increment_instret();
        self.stats.insns_retired += 1;
    }

    fn handle_ecall(&mut self, bus: &mut SystemBus) -> Result<TickOutcome, VmError> {
        // NOTE: do NOT call flush_pipeline() here. tick() has already snapshotted
        // the old pipeline latches into local old_* variables, so flushing self.*
        // has no effect on the stages that run after WB in the same tick. Instead,
        // tick() detects EcallSquash and early-returns before committing any
        // new stage results, which is the correct squash mechanism.
        let syscall = self.regs.read_x(17);
        match syscall {
            64 => {
                // Linux sys_write(fd, buf, len)
                let fd = self.regs.read_x(10);
                let buf = self.regs.read_x(11);
                let len = self.regs.read_x(12) as usize;

                // Only support stdout (fd=1)
                if fd == 1 {
                    let mut written = 0usize;
                    for i in 0..len {
                        let byte = bus.read_byte(buf + i as u64).unwrap_or(0);
                        let _ = bus.uart_mut().write_byte(0, byte);
                        written += 1;
                    }
                    self.regs.write_x(10, written as u64);
                } else {
                    // Unsupported file descriptor
                    self.regs.write_x(10, u64::MAX);
                }
                self.regs.pc = self.regs.pc.wrapping_add(4);
                self.fetch_pc = self.regs.pc;
                self.csrs.increment_instret();
                self.stats.insns_retired += 1;
                Ok(TickOutcome::EcallSquash)
            }
            93 | 94 => {
                // Linux sys_exit / sys_exit_group
                let code = self.regs.read_x(10) as i64;
                Ok(TickOutcome::Halted(code))
            }
            _ => {
                // Unknown syscall - return error
                self.regs.write_x(10, u64::MAX);
                self.regs.pc = self.regs.pc.wrapping_add(4);
                self.fetch_pc = self.regs.pc;
                self.csrs.increment_instret();
                self.stats.insns_retired += 1;
                Ok(TickOutcome::EcallSquash)
            }
        }
    }

    // -----------------------------------------------------------------------
    // run() helper
    // -----------------------------------------------------------------------

    pub fn run(&mut self, bus: &mut SystemBus, max_cycles: u64) -> (TickOutcome, String) {
        let mut outcome = TickOutcome::Continue;
        for _ in 0..max_cycles {
            match self.tick(bus) {
                Ok(TickOutcome::Continue | TickOutcome::EcallSquash) => {}
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

/// Extract (rd, is_fp_dest, fwd_val) from an ExecResult for the EX/MEM register.
fn ex_forwarding_info(result: &ExecResult, id_ex: &IDEXReg) -> (usize, bool, u64) {
    match result {
        ExecResult::WriteInt { rd, val, .. } => (*rd, false, *val),
        ExecResult::WriteFp { rd, bits, .. } => (*rd, true, *bits),
        ExecResult::WriteIntFlags { rd, val, .. } => (*rd, false, *val),
        ExecResult::WriteFpFlags { rd, bits, .. } => (*rd, true, *bits),
        // JAL/JALR: rd gets pc+4 (return address); ExecResult::WriteInt covers this case
        ExecResult::Jump { .. } => (0, false, 0),
        ExecResult::Load { rd, .. } => (*rd, false, 0), // value not available until MEM
        ExecResult::FLoad { rd, .. } => (*rd, true, 0),
        ExecResult::Atomic { rd, .. } => (*rd, false, 0),
        ExecResult::Csr { rd, .. } => (*rd, false, 0), // value computed at WB
        _ => (id_ex.rd, id_ex.is_fp_dest, 0),
    }
}

/// Extract (rd, is_fp_dest, fwd_val) from a MemResult for the MEM/WB register.
fn mem_forwarding_info(result: &MemResult, ex_mem: &EXMEMReg) -> (usize, bool, u64) {
    match result {
        MemResult::WriteInt { rd, val, .. } => (*rd, false, *val),
        MemResult::WriteFp { rd, bits, .. } => (*rd, true, *bits),
        MemResult::WriteIntFlags { rd, val, .. } => (*rd, false, *val),
        MemResult::WriteFpFlags { rd, bits, .. } => (*rd, true, *bits),
        MemResult::Jump { .. } => (0, false, 0),
        MemResult::Csr { rd, .. } => (*rd, false, 0), // WB computes old value
        _ => (ex_mem.rd, ex_mem.is_fp_dest, ex_mem.fwd_val),
    }
}
