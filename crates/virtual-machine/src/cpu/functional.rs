//! Deterministic single-instruction execution for architectural comparison.

use crate::bus::SystemBus;
use crate::cpu::architectural::ArchitecturalState;
use crate::cpu::decoder;
use crate::cpu::hazard_unit::{insn_frs1, insn_frs2, insn_rs1, insn_rs2};
use crate::cpu::interrupts;
use crate::cpu::mmu;
use crate::cpu::pipeline::execute::{self, ExecuteOperands};
use crate::cpu::pipeline::fetch;
use crate::cpu::pipeline::memory::{self, MemResult};
use crate::cpu::pipeline::writeback;
use crate::cpu::registers::PrivilegeMode;
use crate::cpu::retirement::{
    EventState, MemoryWrite, RETIRE_EVENT_VERSION, RetireEvent, TrapRecord, TrapStage,
};
use crate::cpu::traps;
use crate::error::VmError;
use riscv_model::InstructionId;

#[derive(Clone)]
pub struct FunctionalCore {
    architectural: ArchitecturalState,
    tlb: mmu::Tlb,
    next_sequence: u64,
}

impl FunctionalCore {
    pub fn new(start_pc: u64, stack_ptr: u64) -> Self {
        Self {
            architectural: ArchitecturalState::new(start_pc, stack_ptr),
            tlb: mmu::Tlb::new(),
            next_sequence: 0,
        }
    }

    pub fn architectural_state(&self) -> &ArchitecturalState {
        &self.architectural
    }

    pub fn reset_pc(&mut self, pc: u64) {
        self.architectural.set_pc(pc);
        self.tlb.flush();
    }

    pub fn set_boot_entry(&mut self, entry: u64) {
        self.architectural.registers_mut().write_x(10, entry);
    }

    pub fn step(&mut self, bus: &mut SystemBus) -> Result<RetireEvent, VmError> {
        self.architectural.csrs_mut().increment_cycle();
        interrupts::sample_inputs(&mut self.architectural, bus);

        let before = EventState::capture(&self.architectural);
        let pc = self.architectural.pc();
        if let Some((cause, value, trap_pc)) = interrupts::pending(&self.architectural) {
            return Ok(self.take_trap(
                before,
                trap_pc,
                None,
                None,
                cause,
                value,
                TrapStage::Interrupt,
            ));
        }

        let translate = self.translation_context();
        let raw = match fetch::fetch_with_pmp(bus, pc, translate, &mut self.tlb) {
            Ok(raw) => raw,
            Err(error) => {
                return self.trap_error(before, pc, None, None, error, TrapStage::Fetch);
            }
        };
        let instruction = match decoder::decode(raw) {
            Ok(instruction) => instruction,
            Err(error) => {
                return self.trap_error(before, pc, Some(raw), None, error, TrapStage::Decode);
            }
        };
        let instruction_id = instruction.instruction_id();
        let registers = self.architectural.registers();
        let rs1 = insn_rs1(&instruction);
        let rs2 = insn_rs2(&instruction);
        let frs1 = insn_frs1(&instruction);
        let frs2 = insn_frs2(&instruction);
        let operands = ExecuteOperands::new(
            registers,
            (rs1, registers.read_x(rs1)),
            (rs2, registers.read_x(rs2)),
            (frs1, registers.read_f_bits(frs1)),
            (frs2, registers.read_f_bits(frs2)),
        );
        let executed =
            match execute::execute(&instruction, &operands, self.architectural.csrs(), pc) {
                Ok(executed) => executed,
                Err(error) => {
                    return self.trap_error(
                        before,
                        pc,
                        Some(raw),
                        instruction_id,
                        error,
                        TrapStage::Execute,
                    );
                }
            };

        let translate = self.translation_context();
        let (memory_result, memory_write) = match memory::memory_stage_with_trace(
            executed,
            bus,
            self.architectural.reservation_mut(),
            translate,
            &mut self.tlb,
        ) {
            Ok(result) => result,
            Err(error) => {
                return self.trap_error(
                    before,
                    pc,
                    Some(raw),
                    instruction_id,
                    error,
                    TrapStage::Memory,
                );
            }
        };

        if matches!(memory_result, MemResult::SfenceVma) {
            self.tlb.flush();
        }

        let writeback_result = {
            let (registers, csrs) = self.architectural.registers_and_csrs_mut();
            writeback::writeback(memory_result, registers, csrs)
        };
        match writeback_result {
            Ok(next_pc) => {
                self.architectural.set_pc(next_pc);
                self.retire(before, pc, raw, instruction_id, memory_write)
            }
            Err(VmError::Ecall) => {
                let cause = match self.architectural.privilege_mode() {
                    PrivilegeMode::User => traps::CAUSE_ECALL_U,
                    PrivilegeMode::Supervisor => traps::CAUSE_ECALL_S,
                    PrivilegeMode::Machine => traps::CAUSE_ECALL_M,
                };
                Ok(self.take_trap(
                    before,
                    pc,
                    Some(raw),
                    instruction_id,
                    cause,
                    0,
                    TrapStage::Writeback,
                ))
            }
            Err(VmError::Ebreak) => Ok(self.take_trap(
                before,
                pc,
                Some(raw),
                instruction_id,
                traps::CAUSE_EBREAK,
                0,
                TrapStage::Writeback,
            )),
            Err(VmError::Mret) => {
                let (registers, csrs) = self.architectural.registers_and_csrs_mut();
                let next_pc = traps::handle_mret(registers, csrs);
                self.architectural.set_pc(next_pc);
                self.retire(before, pc, raw, instruction_id, memory_write)
            }
            Err(VmError::Sret) => {
                let result = {
                    let (registers, csrs) = self.architectural.registers_and_csrs_mut();
                    traps::handle_sret(registers, csrs)
                };
                match result {
                    Ok(next_pc) => {
                        self.architectural.set_pc(next_pc);
                        self.retire(before, pc, raw, instruction_id, memory_write)
                    }
                    Err(error) => self.trap_error(
                        before,
                        pc,
                        Some(raw),
                        instruction_id,
                        error,
                        TrapStage::Writeback,
                    ),
                }
            }
            Err(error) => self.trap_error(
                before,
                pc,
                Some(raw),
                instruction_id,
                error,
                TrapStage::Writeback,
            ),
        }
    }

    fn translation_context(&self) -> mmu::TranslationContext {
        let csrs = self.architectural.csrs();
        mmu::TranslationContext {
            satp: csrs.satp,
            priv_mode: self.architectural.privilege_mode(),
            mstatus: csrs.mstatus,
            pmpcfg0: csrs.pmpcfg0,
            pmpaddr0: csrs.pmpaddr0,
        }
    }

    fn retire(
        &mut self,
        before: EventState,
        pc: u64,
        raw: u32,
        instruction: Option<InstructionId>,
        memory_write: Option<MemoryWrite>,
    ) -> Result<RetireEvent, VmError> {
        self.architectural.csrs_mut().increment_instret();
        Ok(self.event(before, pc, Some(raw), instruction, true, memory_write, None))
    }

    fn trap_error(
        &mut self,
        before: EventState,
        pc: u64,
        raw: Option<u32>,
        instruction: Option<InstructionId>,
        error: VmError,
        stage: TrapStage,
    ) -> Result<RetireEvent, VmError> {
        let Some((cause, value)) = traps::error_to_trap_cause(&error) else {
            return Err(error);
        };
        Ok(self.take_trap(before, pc, raw, instruction, cause, value, stage))
    }

    fn take_trap(
        &mut self,
        before: EventState,
        pc: u64,
        raw: Option<u32>,
        instruction: Option<InstructionId>,
        cause: u64,
        value: u64,
        stage: TrapStage,
    ) -> RetireEvent {
        let next_pc = {
            let (registers, csrs) = self.architectural.registers_and_csrs_mut();
            traps::take_trap(registers, csrs, cause, value, pc)
        };
        self.architectural.set_pc(next_pc);
        self.event(
            before,
            pc,
            raw,
            instruction,
            false,
            None,
            Some(TrapRecord {
                cause,
                value,
                stage,
            }),
        )
    }

    fn event(
        &mut self,
        before: EventState,
        pc: u64,
        raw_instruction: Option<u32>,
        instruction: Option<InstructionId>,
        retired: bool,
        memory_write: Option<MemoryWrite>,
        trap: Option<TrapRecord>,
    ) -> RetireEvent {
        let (integer_writes, floating_writes, csr_writes, privilege_transition) =
            before.effects(&self.architectural);
        let event = RetireEvent {
            version: RETIRE_EVENT_VERSION,
            sequence: self.next_sequence,
            pc,
            raw_instruction,
            instruction,
            retired,
            integer_writes,
            floating_writes,
            csr_writes,
            memory_write,
            privilege_transition,
            trap,
            next_pc: self.architectural.pc(),
        };
        self.next_sequence = self.next_sequence.wrapping_add(1);
        event
    }
}
