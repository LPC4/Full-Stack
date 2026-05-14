//! Live debug session: wraps a running VirtualMachine and exposes a plain-data
//! snapshot that all debug panels render from.

use std::collections::HashMap;

use crate::assembly_language::assembler::output::AssembledOutput;
use crate::virtual_machine::bus::{
    CLINT_BASE, ELF_LOAD_BASE, PLIC_BASE, RAM_BASE, ROM_BASE, UART_BASE,
};
use crate::virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

pub use snapshot::{
    CpuSnapshot, DebugSnapshot, PipelineCycleSnapshot, PipelineEntry, PipelineHistory, SlotState,
};

pub mod cache_view;
pub mod cpu_state_view;
pub mod disassembly_view;
pub mod framebuffer_view;
pub mod io_view;
pub mod memory_view;
pub mod pipeline_view;
pub mod snapshot;

pub use cache_view::CacheView;
pub use cpu_state_view::CpuStateView;
pub use disassembly_view::DisassemblyView;
pub use framebuffer_view::FramebufferView;
pub use io_view::IoView;
pub use memory_view::MemoryView;
pub use pipeline_view::PipelineView;

// ---------------------------------------------------------------------------
// Session status
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionStatus {
    /// Ready to step; execution has not finished.
    Running,
    /// Program exited with the given code.
    Halted(i64),
    /// The VM hit an unrecoverable error.
    Error(String),
}

// ---------------------------------------------------------------------------
// Well-known address presets the memory view can jump to
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct AddressPreset {
    pub label: &'static str,
    pub addr: u64,
}

pub const ADDRESS_PRESETS: &[AddressPreset] = &[
    AddressPreset {
        label: "ROM base",
        addr: ROM_BASE,
    },
    AddressPreset {
        label: "UART MMIO",
        addr: UART_BASE,
    },
    AddressPreset {
        label: "CLINT",
        addr: CLINT_BASE,
    },
    AddressPreset {
        label: "PLIC",
        addr: PLIC_BASE,
    },
    AddressPreset {
        label: "RAM base",
        addr: RAM_BASE,
    },
];

// ---------------------------------------------------------------------------
// DebugSession
// ---------------------------------------------------------------------------

pub struct DebugSession {
    vm: VirtualMachine,
    assembled: AssembledOutput,
    pub step_count: u64,
    pub status: SessionStatus,
    pub snapshot: DebugSnapshot,
    /// Symbol table from the linker (label -> absolute address).
    pub symbols: HashMap<String, u64>,
    /// Cumulative UART output across all steps.
    pub uart_output: Vec<u8>,
    /// Pending bytes to send to the VM's UART RX buffer.
    uart_tx_pending: Vec<u8>,
}

impl DebugSession {
    pub fn new(assembled: &AssembledOutput) -> Self {
        let symbols: HashMap<String, u64> = assembled
            .symbol_table
            .iter()
            .map(|(name, &offset)| (name.clone(), RAM_BASE + offset))
            .collect();
        let elf = assembled.to_elf(ELF_LOAD_BASE);

        // Build the initial snapshot with dynamic section presets.
        let mut initial_snapshot = DebugSnapshot::default();
        fill_section_presets(&mut initial_snapshot, &symbols);

        let vm = VirtualMachine::from_elf(&elf)
            .unwrap_or_else(|e| panic!("failed to load debug ELF: {e}"));

        let mut session = Self {
            vm,
            assembled: assembled.clone(),
            step_count: 0,
            status: SessionStatus::Running,
            snapshot: initial_snapshot,
            symbols,
            uart_output: Vec::new(),
            uart_tx_pending: Vec::new(),
        };

        // Capture the initial CPU state (before any steps).
        session.refresh_snapshot();
        session
    }

    // -----------------------------------------------------------------------
    // Control
    // -----------------------------------------------------------------------

    /// Execute a single instruction and update the snapshot.
    pub fn step(&mut self) {
        if self.status != SessionStatus::Running {
            return;
        }

        // Push any pending TX bytes before stepping.
        for byte in self.uart_tx_pending.drain(..) {
            self.vm.push_uart_rx(byte);
        }

        match self.vm.step() {
            Ok(StepOutcome::Continue) => {
                self.step_count += 1;
            }
            Ok(StepOutcome::Halted(code)) => {
                self.step_count += 1;
                self.status = SessionStatus::Halted(code);
            }
            Err(e) => {
                self.step_count += 1;
                self.status = SessionStatus::Error(format!("{e:?}"));
            }
        }

        let new_bytes = self.vm.drain_uart_output();
        self.uart_output.extend_from_slice(&new_bytes);

        self.refresh_snapshot();
    }

    /// Execute up to `n` pipeline cycles, stopping early on halt/error.
    ///
    /// Runs the inner loop without snapshotting — snapshots once at the end.
    /// Use `step()` for single-step interactive debugging.
    pub fn step_n(&mut self, n: u64) {
        if self.status != SessionStatus::Running {
            return;
        }
        self.run_inner(n, None);
        let new_bytes = self.vm.drain_uart_output();
        self.uart_output.extend_from_slice(&new_bytes);
        self.refresh_snapshot();
    }

    /// Execute until `n` instructions have retired through WB, stopping early on halt/error.
    ///
    /// Runs the inner loop without snapshotting — snapshots once at the end.
    pub fn step_n_instructions(&mut self, n: u64) {
        if self.status != SessionStatus::Running {
            return;
        }
        let target = self.vm.insns_retired().saturating_add(n);
        self.run_inner(u64::MAX, Some(target));
        let new_bytes = self.vm.drain_uart_output();
        self.uart_output.extend_from_slice(&new_bytes);
        self.refresh_snapshot();
    }

    /// Tight inner loop: step the VM up to `max` times, stopping early if halted/errored
    /// or if `insn_target` retired instructions have been reached.
    /// Does NOT snapshot or drain UART — callers handle that.
    fn run_inner(&mut self, max: u64, insn_target: Option<u64>) {
        for _ in 0..max {
            for byte in self.uart_tx_pending.drain(..) {
                self.vm.push_uart_rx(byte);
            }

            match self.vm.step() {
                Ok(StepOutcome::Continue) => {
                    self.step_count += 1;
                }
                Ok(StepOutcome::Halted(code)) => {
                    self.step_count += 1;
                    self.status = SessionStatus::Halted(code);
                    return;
                }
                Err(e) => {
                    self.step_count += 1;
                    self.status = SessionStatus::Error(format!("{e:?}"));
                    return;
                }
            }

            if let Some(target) = insn_target {
                if self.vm.insns_retired() >= target {
                    return;
                }
            }
        }
    }

    /// Rebuild the VM from the original assembled output and reset all state.
    pub fn reset(&mut self) {
        self.symbols = self
            .assembled
            .symbol_table
            .iter()
            .map(|(name, &offset)| (name.clone(), RAM_BASE + offset))
            .collect();
        let elf = self.assembled.to_elf(ELF_LOAD_BASE);
        self.vm = VirtualMachine::from_elf(&elf)
            .unwrap_or_else(|e| panic!("failed to reload debug ELF: {e}"));
        self.step_count = 0;
        self.status = SessionStatus::Running;
        self.uart_output.clear();
        self.uart_tx_pending.clear();
        self.snapshot = DebugSnapshot::default();
        fill_section_presets(&mut self.snapshot, &self.symbols);
        self.refresh_snapshot();
    }

    /// Queue bytes to be sent into the VM's UART RX on the next step.
    pub fn send_uart(&mut self, bytes: impl IntoIterator<Item = u8>) {
        self.uart_tx_pending.extend(bytes);
    }

    // -----------------------------------------------------------------------
    // Memory inspection
    // -----------------------------------------------------------------------

    /// Read up to `len` bytes from the VM's address space.
    pub fn peek_bytes(&mut self, addr: u64, len: usize) -> Vec<u8> {
        self.vm.peek_bytes(addr, len)
    }

    /// Full cache snapshots (params + per-line state + stats) for the cache view.
    /// Only call this from the render path — it allocates ~1.3MB for L3.
    pub fn cache_snapshots(
        &self,
    ) -> (
        crate::virtual_machine::memory::cache::CacheSnapshot,
        crate::virtual_machine::memory::cache::CacheSnapshot,
        crate::virtual_machine::memory::cache::CacheSnapshot,
    ) {
        self.vm.get_cache_snapshots()
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    fn refresh_snapshot(&mut self) {
        let pc = self.vm.peek_pc();
        let xregs = self.vm.peek_all_xregs();
        let fregs = self.vm.peek_all_fregs();
        let csrs = self.vm.peek_csrs();
        let (l1_stats, l2_stats, l3_stats) = self.vm.get_cache_stats();

        let prev_pc = self.snapshot.cpu.pc;
        let prev_xregs = self.snapshot.cpu.xregs;

        self.snapshot.cpu = CpuSnapshot {
            pc,
            xregs,
            fregs,
            csrs,
            prev_pc,
            prev_xregs,
        };

        // Build a pipeline cycle snapshot from the pipelined CPU state.
        let pipe = self.vm.pipeline_snapshot();
        let stats = self.vm.pipeline_stats();

        let make_slot = |entry: &Option<(u64, &'static str)>,
                         stalled: bool,
                         flushed: bool,
                         is_id_stage: bool|
         -> SlotState {
            match entry {
                Some((addr, mnem)) => SlotState::Normal(PipelineEntry {
                    pc: *addr,
                    mnemonic: mnem.to_string(),
                }),
                None => {
                    if flushed {
                        SlotState::FlushBubble
                    } else if stalled && is_id_stage {
                        SlotState::StallBubble
                    } else {
                        SlotState::Empty
                    }
                }
            }
        };

        let cycle_snap = PipelineCycleSnapshot {
            cycle: stats.cycles,
            stages: [
                make_slot(&pipe.stages[0], pipe.stalled, pipe.flushed, false), // IF
                make_slot(&pipe.stages[1], pipe.stalled, pipe.flushed, true),  // ID
                make_slot(&pipe.stages[2], false, false, false),               // EX
                make_slot(&pipe.stages[3], false, false, false),               // MEM
                make_slot(&pipe.stages[4], false, false, false),               // WB
            ],
            stalled: pipe.stalled,
            flushed: pipe.flushed,
        };

        self.snapshot.pipeline.push(cycle_snap);
        self.snapshot.pipeline.total_cycles = stats.cycles;
        self.snapshot.pipeline.stall_cycles = stats.stall_cycles;
        self.snapshot.pipeline.flush_cycles = stats.flush_cycles;
        self.snapshot.pipeline.branches_seen = stats.branches_seen;
        self.snapshot.pipeline.branches_mispredicted = stats.branches_mispredicted;

        self.snapshot.l1_stats = l1_stats;
        self.snapshot.l2_stats = l2_stats;
        self.snapshot.l3_stats = l3_stats;
    }
}

// ---------------------------------------------------------------------------
// Helper: populate section-start presets from the linker symbol table
// ---------------------------------------------------------------------------

fn fill_section_presets(snapshot: &mut DebugSnapshot, symbols: &HashMap<String, u64>) {
    // Named section start labels emitted by the assembler.
    for (sym, preset_label) in &[
        ("_text_start", ".text start"),
        ("_rodata_start", ".rodata start"),
        ("_data_start", ".data start"),
        ("_bss_start", ".bss start"),
    ] {
        if let Some(&addr) = symbols.get(*sym) {
            snapshot.section_presets.push((*preset_label, addr));
        }
    }
    // Always include the RAM base as a fallback for .text if no symbol exists.
    if snapshot.section_presets.is_empty() {
        snapshot
            .section_presets
            .push((".text (RAM base)", RAM_BASE));
    }
}
