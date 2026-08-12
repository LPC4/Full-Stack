//! A resident VM session the shell drives per frame: step budgets on the UI
//! thread keep the app responsive while the machine stays pausable and inspectable.

mod boot;
mod debugger;
mod device_io;
mod stepping;
#[cfg(test)]
mod tests;

use crate::build::BuildArtifacts;
use crate::inspect::{fs_view::FsSymbols, os_view::OsSymbols};
use crate::runconfig::RunKind;
use crate::session::timeline::TimeMachine;
use crate::session::vm::{RegisterSnapshot, VmExecutionResult};
use asm_to_binary::AssembledOutput;
use asm_to_binary::assembler::link_layout::LinkLayout;
use std::collections::BTreeSet;
use std::time::Duration;
use virtual_machine::cpu::{
    StepOutcome,
    decoder::{self, DecodedInsn},
};
use virtual_machine::virtual_machine::VirtualMachine;

const CLOCK_CHECK_INTERVAL: u64 = 1024;
const UART_LOG_CAP: usize = 256 * 1024;
// Guards against a stalled pipeline; normal instructions retire within a few cycles.
const INSTRUCTION_STEP_CYCLE_CAP: u64 = 10_000;
// Source stepping is synchronous, so loops and missing line entries need both limits.
const SOURCE_LINE_STEP_CYCLE_CAP: u64 = 2_000_000;
const SOURCE_LINE_STEP_TIME_CAP: Duration = Duration::from_millis(8);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MachineState {
    Running,
    Paused,
    Exited(i32),
    /// The user ended the session before the program finished.
    Stopped,
    /// A VM error surfaced; the message carries the cause and faulting pc.
    Faulted(String),
}

impl MachineState {
    /// True while the machine can still execute (running or paused).
    pub fn is_alive(&self) -> bool {
        matches!(self, Self::Running | Self::Paused)
    }
}

pub struct LiveMachine {
    vm: VirtualMachine,
    pub state: MachineState,
    pub kind: RunKind,
    pub steps: u64,
    pub breakpoints: BTreeSet<u64>,
    /// Append-only console transcript, drained from the UART every drive.
    pub uart_log: String,
    /// Steps per second measured over recent drive slices (0 until measured).
    pub steps_per_sec: f64,
    // Kernel sessions keep the kernel image; hosted sessions the linked hosted program.
    symbols: AssembledOutput,
    symbol_table: Vec<(u64, String)>,
    os_sym: Option<OsSymbols>,
    fs_sym: Option<FsSymbols>,
    // Breakpoints at the resume pc stay quiet until it moves; the pipelined CPU
    // holds the architectural pc for a few ticks.
    resumed_from: Option<u64>,
    // One-shot run-to-cursor breakpoint; cleared whenever the machine pauses.
    temp_breakpoint: Option<u64>,
    prev_step_regs: Option<RegisterSnapshot>,
    /// Time-travel history; off by default, enabled from the machine dock.
    pub timeline: TimeMachine,
}
