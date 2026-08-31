//! Versioned architectural events shared by CPU execution models.

use crate::cpu::csr::{self, CsrSnapshot};
use crate::cpu::registers::PrivilegeMode;
use riscv_model::InstructionId;

pub const RETIRE_EVENT_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegisterWrite {
    pub index: u8,
    pub value: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CsrWrite {
    pub address: u16,
    pub value: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryWrite {
    pub physical_address: u64,
    pub size: u8,
    pub value: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrivilegeTransition {
    pub from: PrivilegeMode,
    pub to: PrivilegeMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapStage {
    Interrupt,
    Fetch,
    Decode,
    Execute,
    Memory,
    Writeback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrapRecord {
    pub cause: u64,
    pub value: u64,
    pub stage: TrapStage,
}

impl TrapRecord {
    pub fn is_interrupt(self) -> bool {
        self.cause >> 63 != 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetireEvent {
    pub version: u16,
    pub sequence: u64,
    pub pc: u64,
    pub raw_instruction: Option<u32>,
    pub instruction: Option<InstructionId>,
    pub retired: bool,
    pub integer_writes: Vec<RegisterWrite>,
    pub floating_writes: Vec<RegisterWrite>,
    pub csr_writes: Vec<CsrWrite>,
    pub memory_write: Option<MemoryWrite>,
    pub privilege_transition: Option<PrivilegeTransition>,
    pub trap: Option<TrapRecord>,
    pub next_pc: u64,
}

pub(crate) struct EventState {
    integer_registers: [u64; 32],
    floating_registers: [u64; 32],
    csrs: CsrSnapshot,
    privilege: PrivilegeMode,
}

impl EventState {
    pub(crate) fn capture(state: &super::ArchitecturalState) -> Self {
        Self {
            integer_registers: std::array::from_fn(|i| state.registers().read_x(i)),
            floating_registers: std::array::from_fn(|i| state.registers().read_f_bits(i)),
            csrs: state.csrs().snapshot(),
            privilege: state.privilege_mode(),
        }
    }

    pub(crate) fn effects(
        &self,
        state: &super::ArchitecturalState,
    ) -> (
        Vec<RegisterWrite>,
        Vec<RegisterWrite>,
        Vec<CsrWrite>,
        Option<PrivilegeTransition>,
    ) {
        let integer_writes =
            changed_registers(&self.integer_registers, |i| state.registers().read_x(i));
        let floating_writes = changed_registers(&self.floating_registers, |i| {
            state.registers().read_f_bits(i)
        });
        let csr_writes = changed_csrs(&self.csrs, &state.csrs().snapshot());
        let privilege_transition =
            (self.privilege != state.privilege_mode()).then_some(PrivilegeTransition {
                from: self.privilege,
                to: state.privilege_mode(),
            });
        (
            integer_writes,
            floating_writes,
            csr_writes,
            privilege_transition,
        )
    }
}

fn changed_registers(before: &[u64; 32], after: impl Fn(usize) -> u64) -> Vec<RegisterWrite> {
    (0..32)
        .filter_map(|index| {
            let value = after(index);
            (before[index] != value).then_some(RegisterWrite {
                index: index as u8,
                value,
            })
        })
        .collect()
}

fn changed_csrs(before: &CsrSnapshot, after: &CsrSnapshot) -> Vec<CsrWrite> {
    let values = [
        (
            csr::addr::FFLAGS,
            u64::from(before.fflags),
            u64::from(after.fflags),
        ),
        (csr::addr::FRM, u64::from(before.frm), u64::from(after.frm)),
        (csr::addr::MSTATUS, before.mstatus, after.mstatus),
        (csr::addr::MISA, before.misa, after.misa),
        (csr::addr::MEDELEG, before.medeleg, after.medeleg),
        (csr::addr::MIDELEG, before.mideleg, after.mideleg),
        (csr::addr::MIE, before.mie, after.mie),
        (csr::addr::MTVEC, before.mtvec, after.mtvec),
        (csr::addr::MSCRATCH, before.mscratch, after.mscratch),
        (csr::addr::MEPC, before.mepc, after.mepc),
        (csr::addr::MCAUSE, before.mcause, after.mcause),
        (csr::addr::MTVAL, before.mtval, after.mtval),
        (csr::addr::MIP, before.mip, after.mip),
        (0x3A0, before.pmpcfg0, after.pmpcfg0),
        (0x3B0, before.pmpaddr0, after.pmpaddr0),
        (csr::addr::STVEC, before.stvec, after.stvec),
        (csr::addr::SSCRATCH, before.sscratch, after.sscratch),
        (csr::addr::SEPC, before.sepc, after.sepc),
        (csr::addr::SCAUSE, before.scause, after.scause),
        (csr::addr::STVAL, before.stval, after.stval),
        (csr::addr::SATP, before.satp, after.satp),
    ];

    values
        .into_iter()
        .filter_map(|(address, old, value)| (old != value).then_some(CsrWrite { address, value }))
        .collect()
}
