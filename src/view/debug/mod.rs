//! Live debug session: wraps a running VirtualMachine and exposes a plain-data
//! snapshot that all debug panels render from.

use std::collections::HashMap;

use crate::assembly_language::assembler::output::AssembledOutput;
use crate::virtual_machine::bus::{CLINT_BASE, PLIC_BASE, RAM_BASE, ROM_BASE, UART_BASE};
use crate::virtual_machine::cpu::decoder::{DecodedInsn, FMacOp, decode as decode_insn};
use crate::virtual_machine::linker::{self, LinkerConfig};
use crate::virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

pub use snapshot::{CpuSnapshot, DebugSnapshot, PipelineEntry, PipelineHistory};

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
    /// Symbol table from the linker (label → absolute address).
    pub symbols: HashMap<String, u64>,
    /// Cumulative UART output across all steps.
    pub uart_output: Vec<u8>,
    /// Pending bytes to send to the VM's UART RX buffer.
    uart_tx_pending: Vec<u8>,
}

impl DebugSession {
    pub fn new(assembled: &AssembledOutput) -> Self {
        let program = linker::link(assembled, &LinkerConfig::default());
        let symbols = program.symbols.clone();

        // Build the initial snapshot with dynamic section presets.
        let mut initial_snapshot = DebugSnapshot::default();
        fill_section_presets(&mut initial_snapshot, &symbols);

        let vm = VirtualMachine::from_linked(&program);

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

    /// Execute up to `n` instructions, stopping early on halt/error.
    pub fn step_n(&mut self, n: u64) {
        for _ in 0..n {
            self.step();
            if self.status != SessionStatus::Running {
                break;
            }
        }
    }

    /// Rebuild the VM from the original assembled output and reset all state.
    pub fn reset(&mut self) {
        let program = linker::link(&self.assembled, &LinkerConfig::default());
        self.symbols = program.symbols.clone();
        self.vm = VirtualMachine::from_linked(&program);
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

        // Decode mnemonic at current fetch PC and record it in the pipeline log.
        let mnemonic = {
            let bytes = self.vm.peek_bytes(pc, 4);
            if bytes.len() == 4 {
                let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                decode_insn(raw)
                    .map(|insn| insn_mnemonic(&insn).to_string())
                    .unwrap_or_else(|_| "???".into())
            } else {
                "---".into()
            }
        };
        self.snapshot.pipeline.push(snapshot::PipelineEntry { pc, mnemonic });

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

// ---------------------------------------------------------------------------
// Instruction mnemonic helper
// ---------------------------------------------------------------------------

fn insn_mnemonic(insn: &DecodedInsn) -> &'static str {
    match insn {
        DecodedInsn::Lui { .. }      => "lui",
        DecodedInsn::Auipc { .. }    => "auipc",
        DecodedInsn::Jal { .. }      => "jal",
        DecodedInsn::Jalr { .. }     => "jalr",
        DecodedInsn::Branch { funct3, .. } => match funct3 {
            0 => "beq", 1 => "bne", 4 => "blt", 5 => "bge", 6 => "bltu", 7 => "bgeu", _ => "b??",
        },
        DecodedInsn::Load { funct3, .. } => match funct3 {
            0 => "lb",  1 => "lh",  2 => "lw",  3 => "ld",
            4 => "lbu", 5 => "lhu", 6 => "lwu",  _ => "l??",
        },
        DecodedInsn::Store { funct3, .. } => match funct3 {
            0 => "sb", 1 => "sh", 2 => "sw", 3 => "sd", _ => "s??",
        },
        DecodedInsn::AluImm { funct3, funct7, .. } => match funct3 {
            0 => "addi", 1 => "slli", 2 => "slti", 3 => "sltiu",
            4 => "xori",
            5 => if funct7 & 0x20 != 0 { "srai" } else { "srli" },
            6 => "ori",  7 => "andi", _ => "imm??",
        },
        DecodedInsn::AluImm32 { funct3, funct7, .. } => match funct3 {
            0 => "addiw", 1 => "slliw",
            5 => if funct7 & 0x20 != 0 { "sraiw" } else { "srliw" },
            _ => "immw??",
        },
        DecodedInsn::Alu { funct3, funct7, .. } => {
            if *funct7 == 1 {
                match funct3 {
                    0 => "mul", 1 => "mulh", 2 => "mulhsu", 3 => "mulhu",
                    4 => "div", 5 => "divu",  6 => "rem",   7 => "remu",
                    _ => "m??",
                }
            } else {
                let alt = funct7 & 0x20 != 0;
                match (funct3, alt) {
                    (0, true)  => "sub", (0, false) => "add",
                    (1, _)     => "sll", (2, _)     => "slt",
                    (3, _)     => "sltu",(4, _)     => "xor",
                    (5, true)  => "sra", (5, false) => "srl",
                    (6, _)     => "or",  (7, _)     => "and",
                    _          => "alu??",
                }
            }
        },
        DecodedInsn::Alu32 { funct3, funct7, .. } => {
            if *funct7 == 1 {
                match funct3 {
                    0 => "mulw", 4 => "divw", 5 => "divuw", 6 => "remw", 7 => "remuw", _ => "mw??",
                }
            } else {
                let alt = funct7 & 0x20 != 0;
                match (funct3, alt) {
                    (0, true)  => "subw", (0, false) => "addw",
                    (1, _)     => "sllw",
                    (5, true)  => "sraw", (5, false) => "srlw",
                    _          => "alu32??",
                }
            }
        },
        DecodedInsn::Fence { .. }    => "fence",
        DecodedInsn::FenceI          => "fence.i",
        DecodedInsn::Ecall           => "ecall",
        DecodedInsn::Ebreak          => "ebreak",
        DecodedInsn::Mret            => "mret",
        DecodedInsn::Sret            => "sret",
        DecodedInsn::SfenceVma       => "sfence.vma",
        DecodedInsn::Csr { funct3, .. } => match funct3 {
            1 => "csrrw", 2 => "csrrs", 3 => "csrrc",
            5 => "csrrwi",6 => "csrrsi",7 => "csrrci",
            _ => "csr??",
        },
        DecodedInsn::FLoad  { funct3, .. } => if *funct3 == 2 { "flw" } else { "fld" },
        DecodedInsn::FStore { funct3, .. } => if *funct3 == 2 { "fsw" } else { "fsd" },
        DecodedInsn::FOp    { .. }         => "f.op",
        DecodedInsn::FMac   { op, .. }     => match op {
            FMacOp::Fmadd  => "fmadd",  FMacOp::Fmsub  => "fmsub",
            FMacOp::Fnmsub => "fnmsub", FMacOp::Fnmadd => "fnmadd",
        },
        DecodedInsn::Atomic { funct5, .. } => match funct5 {
            2  => "lr.w",    3  => "sc.w",
            1  => "amoswap", 0  => "amoadd",
            4  => "amoxor",  8  => "amoor",   12 => "amoand",
            16 => "amomin",  20 => "amomax",  24 => "amominu", 28 => "amomaxu",
            _  => "amo??",
        },
    }
}
