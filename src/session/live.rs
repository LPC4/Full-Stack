//! A resident VM session the shell drives per frame: step budgets on the UI
//! thread keep the app responsive while the machine stays pausable and inspectable.

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
// A line with no distinct next entry (or a never-returning call) must not spin the UI.
const SOURCE_LINE_STEP_CYCLE_CAP: u64 = 2_000_000;

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

impl LiveMachine {
    /// Boot a hosted program from its linked artifacts; the session starts running.
    pub fn new_hosted(artifacts: &BuildArtifacts) -> Result<Self, String> {
        let elf = artifacts
            .linked
            .to_elf_with_entry(LinkLayout::hosted().load_base, "_start");
        let vm = VirtualMachine::from_elf(&elf).map_err(|err| format!("ELF load failed: {err}"))?;
        Ok(Self {
            vm,
            state: MachineState::Running,
            kind: RunKind::Hosted,
            steps: 0,
            breakpoints: BTreeSet::new(),
            uart_log: String::new(),
            steps_per_sec: 0.0,
            symbol_table: runtime_symbol_table(&artifacts.linked),
            symbols: artifacts.linked.clone(),
            os_sym: None,
            fs_sym: None,
            resumed_from: None,
            temp_breakpoint: None,
            prev_step_regs: None,
            timeline: TimeMachine::new(50_000, 8, 64),
        })
    }

    /// Boot a bare-metal VM: no hosted runtime, execution starts at the config's
    /// entry symbol (falling back to `_start`).
    pub fn new_freestanding(artifacts: &BuildArtifacts) -> Result<Self, String> {
        let entry = artifacts.plan.entry.as_deref().unwrap_or("_start");
        let elf = artifacts
            .linked
            .to_elf_with_entry(virtual_machine::bus::ELF_LOAD_BASE, entry);
        let vm = VirtualMachine::from_elf(&elf).map_err(|err| format!("ELF load failed: {err}"))?;
        Ok(Self {
            vm,
            state: MachineState::Running,
            kind: RunKind::Freestanding,
            steps: 0,
            breakpoints: BTreeSet::new(),
            uart_log: String::new(),
            steps_per_sec: 0.0,
            symbol_table: runtime_symbol_table(&artifacts.linked),
            symbols: artifacts.linked.clone(),
            os_sym: None,
            fs_sym: None,
            resumed_from: None,
            temp_breakpoint: None,
            prev_step_regs: None,
            timeline: TimeMachine::new(50_000, 8, 64),
        })
    }

    /// Boot a kernel VM from the prepared kernel image and optional boot assets.
    pub fn new_kernel(
        kernel: &BuildArtifacts,
        user_init: Option<&AssembledOutput>,
        fs_image: Option<&[u8]>,
        autorun: &str,
    ) -> Result<Self, String> {
        let mut vm = VirtualMachine::new_kernel(&kernel.linked);
        write_kernel_boot_assets(&mut vm, user_init, fs_image)?;
        for byte in autorun.bytes() {
            vm.push_uart_rx(byte);
        }

        Ok(Self {
            vm,
            state: MachineState::Running,
            kind: RunKind::KernelBoot,
            steps: 0,
            breakpoints: BTreeSet::new(),
            uart_log: String::new(),
            steps_per_sec: 0.0,
            symbol_table: runtime_symbol_table(&kernel.linked),
            symbols: kernel.linked.clone(),
            os_sym: OsSymbols::from_kernel(&kernel.linked),
            fs_sym: FsSymbols::from_kernel(&kernel.linked),
            resumed_from: None,
            temp_breakpoint: None,
            prev_step_regs: None,
            timeline: TimeMachine::new(50_000, 8, 64),
        })
    }

    /// Apply an explicitly requested timing model to this session.
    pub fn set_memory_latency_config(&mut self, config: virtual_machine::bus::MemoryLatencyConfig) {
        self.vm.set_memory_latency_config(config);
    }

    /// Step while `Running` until a budget is spent, a breakpoint is hit, the
    /// program exits, or a fault occurs. Returns the steps executed this slice.
    pub fn drive(&mut self, max_steps: u64, budget: Duration) -> u64 {
        if self.state != MachineState::Running {
            return 0;
        }
        let clock = SliceClock::start();
        let mut stepped = 0u64;
        while stepped < max_steps {
            if self.breakpoint_hit() {
                self.state = MachineState::Paused;
                self.temp_breakpoint = None;
                break;
            }
            if !self.step_once() {
                stepped += 1;
                break;
            }
            stepped += 1;
            if stepped.is_multiple_of(CLOCK_CHECK_INTERVAL) && clock.expired(budget) {
                break;
            }
        }
        self.drain_uart();
        let elapsed = clock.elapsed_secs();
        if elapsed > 0.0 && stepped > 0 {
            self.steps_per_sec = stepped as f64 / elapsed;
        }
        // Force a frame where the slice paused (breakpoint/exit); otherwise let
        // the interval throttle continuous-run captures.
        if self.state == MachineState::Running {
            self.maybe_capture();
        } else {
            self.capture_now();
        }
        stepped
    }

    fn maybe_capture(&mut self) {
        let retired = self.vm.insns_retired();
        if self.timeline.should_capture(retired) {
            let snap = self.vm.snapshot();
            self.timeline.capture(&snap, retired);
        }
    }

    // Unthrottled, so the scrubber has a tick exactly where the user stopped.
    fn capture_now(&mut self) {
        if self.timeline.enabled() {
            let retired = self.vm.insns_retired();
            let snap = self.vm.snapshot();
            self.timeline.capture(&snap, retired);
        }
    }

    /// Restore the machine to time-travel frame `index`, pausing there. Returns
    /// false if the index is out of range or history is empty.
    pub fn restore_timepoint(&mut self, index: usize) -> bool {
        let Some(snap) = self.timeline.snapshot_at(index) else {
            return false;
        };
        self.vm.restore(&snap);
        // Rebase the capture throttle on the restored (rewound) retired counter;
        // otherwise no frame is due again until execution re-passes the old count.
        self.timeline.rewind_to(index);
        if self.state.is_alive() {
            self.state = MachineState::Paused;
        }
        self.prev_step_regs = None;
        self.resumed_from = None;
        self.temp_breakpoint = None;
        true
    }

    // Execute exactly one cycle and pause, regardless of current state.
    fn step_one(&mut self) {
        self.step_cycles(1);
    }

    /// Execute exactly one cycle and pause.
    pub fn step_cycle(&mut self) {
        self.step_one();
    }

    /// Step one retired instruction, entering calls.
    pub fn step_in(&mut self) {
        self.step_instruction();
    }

    /// Step over a call-like instruction, otherwise step in once.
    pub fn step_over(&mut self) {
        if !self.state.is_alive() {
            return;
        }
        if let Some(return_pc) = self.step_over_target() {
            self.prev_step_regs = Some(self.registers());
            self.run_to(return_pc);
        } else {
            self.step_instruction();
        }
    }

    /// Continue until the current frame's return address is reached.
    pub fn step_out(&mut self) {
        if !self.state.is_alive() {
            return;
        }
        let ra = self.registers().x[1];
        if ra == 0 {
            return;
        }
        self.prev_step_regs = Some(self.registers());
        self.run_to(ra);
    }

    /// Execute up to `count` cycles and pause. Records the register file
    /// beforehand so the inspector can highlight what the step(s) changed.
    pub fn step_cycles(&mut self, count: u64) {
        if !self.state.is_alive() || count == 0 {
            return;
        }
        self.prev_step_regs = Some(self.registers());
        for _ in 0..count {
            if !self.step_once() {
                break;
            }
        }
        if self.state.is_alive() {
            self.state = MachineState::Paused;
        }
        self.drain_uart();
        self.capture_now();
    }

    // Execute until one instruction retires, then pause.
    fn step_instruction(&mut self) {
        self.step_instructions(1);
    }

    /// Step to a different source line in this frame or a caller; calls are
    /// stepped over, not into. Without a line table this is one instruction.
    pub fn step_line(&mut self) {
        self.step_line_impl(true);
    }

    /// Step to a different source line, entering calls: a call line pauses on
    /// the callee's first line. Without a line table this is one instruction.
    pub fn step_line_in(&mut self) {
        self.step_line_impl(false);
    }

    fn step_line_impl(&mut self, over_calls: bool) {
        if !self.state.is_alive() {
            return;
        }
        if !self.has_line_table() {
            self.step_instruction();
            return;
        }
        let origin = self
            .source_for_pc(self.registers().pc)
            .filter(|(file, _)| !file.is_empty())
            .map(|(file, line)| (file.to_owned(), line));
        self.prev_step_regs = Some(self.registers());
        let mut cycles = 0;
        let mut retired = self.vm.insns_retired();
        // While set, we are inside a call: run until the pc returns here so
        // line checks never observe callee lines (notably the stdlib's).
        let mut skip_until = over_calls.then(|| self.step_over_target()).flatten();
        while cycles < SOURCE_LINE_STEP_CYCLE_CAP {
            if !self.step_once() {
                break;
            }
            cycles += 1;
            // Re-check the source line only on an instruction boundary.
            let now_retired = self.vm.insns_retired();
            if now_retired == retired {
                continue;
            }
            retired = now_retired;
            // A user breakpoint inside a skipped call still stops the step.
            if self.breakpoint_hit() {
                self.temp_breakpoint = None;
                break;
            }
            if let Some(return_pc) = skip_until
                && self.registers().pc != return_pc
            {
                continue;
            }
            let here = self
                .source_for_pc(self.registers().pc)
                .filter(|(file, _)| !file.is_empty())
                .map(|(file, line)| (file.to_owned(), line));
            if here.is_some() && here != origin {
                break;
            }
            skip_until = over_calls.then(|| self.step_over_target()).flatten();
        }
        if self.state.is_alive() {
            self.state = MachineState::Paused;
        }
        self.drain_uart();
        self.capture_now();
    }

    /// Execute until `count` instructions retire, then pause. Pipeline fill,
    /// stalls, and flushes may consume more cycles than the requested count.
    pub fn step_instructions(&mut self, count: u64) {
        if !self.state.is_alive() || count == 0 {
            return;
        }
        self.prev_step_regs = Some(self.registers());
        let target = self
            .vm
            .insns_retired()
            .saturating_add(count)
            .max(self.vm.insns_retired() + 1);
        let cycle_cap = count.saturating_mul(INSTRUCTION_STEP_CYCLE_CAP);
        let mut cycles = 0;
        while self.vm.insns_retired() < target && cycles < cycle_cap {
            if !self.step_once() {
                break;
            }
            cycles += 1;
        }
        if self.state.is_alive() {
            self.state = MachineState::Paused;
        }
        self.drain_uart();
        self.capture_now();
    }

    pub fn pause(&mut self) {
        if self.state == MachineState::Running {
            self.state = MachineState::Paused;
            self.temp_breakpoint = None;
            // Interval captures can be tens of thousands of instructions stale;
            // give the scrubber a tick exactly where the user paused.
            self.capture_now();
        }
    }

    pub fn resume(&mut self) {
        if self.state == MachineState::Paused {
            self.resumed_from = Some(self.vm.peek_pc());
            self.prev_step_regs = None;
            self.state = MachineState::Running;
        }
    }

    /// Run until `pc` via a one-shot breakpoint (run-to-cursor). Works from
    /// `Paused` (resume) and `Running` (retargets the temporary stop).
    pub fn run_to(&mut self, pc: u64) {
        if !self.state.is_alive() {
            return;
        }
        self.temp_breakpoint = Some(pc);
        self.resume();
    }

    /// End the session; the machine keeps its final state for inspection.
    pub fn stop(&mut self) {
        if self.state.is_alive() {
            self.state = MachineState::Stopped;
        }
    }

    /// Feed one byte to the guest UART receive queue.
    pub fn push_uart(&mut self, byte: u8) {
        self.vm.push_uart_rx(byte);
    }

    /// Feed a whole line to the guest UART, terminated with a newline.
    pub fn push_uart_line(&mut self, text: &str) {
        for byte in text.bytes() {
            self.vm.push_uart_rx(byte);
        }
        self.vm.push_uart_rx(b'\n');
    }

    pub fn push_key_event(&mut self, scancode: u16, pressed: bool) {
        self.vm.keyboard_push(scancode, pressed);
    }

    pub fn framebuffer(&self) -> &[u8] {
        self.vm.peek_framebuffer()
    }

    pub fn framebuffer_fill_count(&self) -> u64 {
        self.vm.framebuffer_fill_count()
    }

    /// Visible-pixel change counter; the GUI re-uploads its texture when it moves.
    pub fn framebuffer_version(&self) -> u64 {
        self.vm.framebuffer_content_version()
    }

    /// Runtime pc for an image-relative symbol offset (`AssembledOutput`
    /// symbol addresses); the VM maps program images at `RAM_BASE`.
    pub fn runtime_address(image_offset: u64) -> u64 {
        virtual_machine::bus::RAM_BASE + image_offset
    }

    pub fn registers(&self) -> RegisterSnapshot {
        RegisterSnapshot {
            pc: self.vm.peek_pc(),
            x: self.vm.peek_all_xregs(),
        }
    }

    pub fn symbols(&self) -> &AssembledOutput {
        &self.symbols
    }

    /// The machine itself, for read-only inspection probes (`inspect::*`).
    pub fn vm(&self) -> &VirtualMachine {
        &self.vm
    }

    /// Read guest memory through the MMU at the current translation context.
    /// Unmapped pages come back zero-filled.
    pub fn read_virtual(&self, addr: u64, len: usize) -> Vec<u8> {
        self.vm.peek_bytes_virtual(addr, len)
    }

    /// Read guest-physical memory, bypassing translation.
    pub fn read_physical(&self, addr: u64, len: usize) -> Vec<u8> {
        self.vm.peek_bytes_raw(addr, len)
    }

    /// Sv39 walk of `vaddr` in the current context, `None` when unmapped.
    pub fn translate(&self, vaddr: u64) -> Option<u64> {
        self.vm.debug_translate(vaddr)
    }

    /// Registers before the last step, for delta highlighting.
    pub fn prev_step_registers(&self) -> Option<&RegisterSnapshot> {
        self.prev_step_regs.as_ref()
    }

    /// Nearest symbol at or below `addr` (runtime addresses), with the offset
    /// into it. Only useful for text in the linked image's own address range.
    pub fn symbol_near(&self, addr: u64) -> Option<(&str, u64)> {
        let idx = self
            .symbol_table
            .partition_point(|(sym_addr, _)| *sym_addr <= addr);
        let (sym_addr, name) = self.symbol_table.get(idx.checked_sub(1)?)?;
        Some((name.as_str(), addr - sym_addr))
    }

    /// Exact-address symbol, for labeling disassembly rows.
    pub fn symbol_at(&self, addr: u64) -> Option<&str> {
        let idx = self
            .symbol_table
            .binary_search_by_key(&addr, |(sym_addr, _)| *sym_addr)
            .ok()?;
        Some(self.symbol_table[idx].1.as_str())
    }

    /// Resolve a persisted breakpoint like `main` or `main+0x10` to a runtime pc.
    pub fn resolve_breakpoint(&self, spec: &str) -> Result<u64, String> {
        let (symbol, offset) = parse_breakpoint_spec(spec)?;
        let Some(addr) = self.symbols.symbol_address(symbol) else {
            return Err(format!("unknown symbol `{symbol}`"));
        };
        Ok(Self::runtime_address(addr) + offset)
    }

    /// Convert a runtime pc back to a stable symbol+offset breakpoint spec.
    pub fn breakpoint_spec_for_addr(&self, addr: u64) -> Option<String> {
        let (symbol, offset) = self.symbol_near(addr)?;
        if offset == 0 {
            Some(symbol.to_owned())
        } else {
            Some(format!("{symbol}+{offset:#x}"))
        }
    }

    /// Whether the linked image carries a pc-to-source line table.
    pub fn has_line_table(&self) -> bool {
        self.symbols.line_table_len() > 0
    }

    /// Source location for a runtime pc via the linked image's line table.
    /// `None` outside the image's own text range or without a table.
    pub fn source_for_pc(&self, pc: u64) -> Option<(&str, u32)> {
        let offset = pc.checked_sub(virtual_machine::bus::RAM_BASE)?;
        self.symbols.source_for_addr(offset)
    }

    /// Lowest runtime pc attributed to `line` of `file`, if the table has it.
    pub fn pc_for_source(&self, file: &str, line: u32) -> Option<u64> {
        self.symbols
            .addr_for_source(file, line)
            .map(Self::runtime_address)
    }

    /// Like `pc_for_source`, but slides forward to the next line of `file`
    /// that carries code, so labels and blank lines are valid targets.
    pub fn pc_at_or_after_source(&self, file: &str, line: u32) -> Option<u64> {
        self.symbols
            .addr_for_source_at_or_after(file, line)
            .map(Self::runtime_address)
    }

    /// Best-effort call stack: the live pc, then return addresses found via ra
    /// and a bounded stack scan (there is no fp chain to walk).
    pub fn call_stack(&self, max_frames: usize) -> Vec<u64> {
        const STACK_SCAN_BYTES: usize = 4096;
        let regs = self.registers();
        let mut frames = vec![regs.pc];
        let ra = regs.x[1];
        if frames.len() < max_frames && ra != regs.pc && self.is_return_address(ra) {
            frames.push(ra);
        }
        let sp = regs.x[2];
        if sp == 0 {
            return frames;
        }
        for chunk in self.read_virtual(sp, STACK_SCAN_BYTES).chunks_exact(8) {
            if frames.len() >= max_frames {
                break;
            }
            let word = u64::from_le_bytes(chunk.try_into().expect("8-byte chunk"));
            if frames.last() != Some(&word) && self.is_return_address(word) {
                frames.push(word);
            }
        }
        frames
    }

    // Heuristic: aligned, near a known symbol, preceded by a jal/jalr that linked ra.
    fn is_return_address(&self, addr: u64) -> bool {
        const MAX_SYMBOL_OFFSET: u64 = 0x10_0000;
        if addr < 8 || !addr.is_multiple_of(4) {
            return false;
        }
        if !matches!(self.symbol_near(addr), Some((_, off)) if off < MAX_SYMBOL_OFFSET) {
            return false;
        }
        let bytes = self.read_virtual(addr - 4, 4);
        matches!(
            read_word(&bytes, 0).map(decoder::decode),
            Some(Ok(
                DecodedInsn::Jal { rd: 1, .. } | DecodedInsn::Jalr { rd: 1, .. }
            ))
        )
    }

    pub fn os_symbols(&self) -> Option<&OsSymbols> {
        self.os_sym.as_ref()
    }

    pub fn fs_symbols(&self) -> Option<&FsSymbols> {
        self.fs_sym.as_ref()
    }

    /// Post-mortem summary in the shape the one-shot run path produced, so the
    /// existing dock/Inspect/terminal surfaces keep working unchanged.
    pub fn result(&self) -> VmExecutionResult {
        VmExecutionResult {
            uart_output: self.uart_log.clone(),
            exit_code: match self.state {
                MachineState::Exited(code) => Some(code),
                _ => None,
            },
            steps: self.steps,
            max_steps_reached: false,
            registers: Some(self.registers()),
        }
    }

    // Returns false when the session left Running/Paused and stepping must stop.
    fn step_once(&mut self) -> bool {
        match self.vm.step() {
            Ok(StepOutcome::Continue) => {
                self.steps += 1;
                true
            }
            Ok(StepOutcome::Halted(code)) => {
                self.steps += 1;
                self.state = MachineState::Exited(code as i32);
                false
            }
            Err(err) => {
                self.steps += 1;
                self.state = MachineState::Faulted(format!("{err} at pc {:#x}", self.vm.peek_pc()));
                false
            }
        }
    }

    fn breakpoint_hit(&mut self) -> bool {
        let pc = self.vm.peek_pc();
        if self.resumed_from == Some(pc) {
            return false;
        }
        self.resumed_from = None;
        self.temp_breakpoint == Some(pc)
            || (!self.breakpoints.is_empty() && self.breakpoints.contains(&pc))
    }

    fn drain_uart(&mut self) {
        let bytes = self.vm.drain_uart_output();
        append_terminal_bytes(&mut self.uart_log, &bytes);
        if self.uart_log.len() > UART_LOG_CAP {
            let cut = self.uart_log.len() - UART_LOG_CAP * 3 / 4;
            let cut = (cut..self.uart_log.len())
                .find(|&i| self.uart_log.is_char_boundary(i))
                .unwrap_or(0);
            self.uart_log.drain(..cut);
        }
    }

    fn step_over_target(&self) -> Option<u64> {
        let pc = self.vm.peek_pc();
        let bytes = self.vm.peek_bytes_virtual(pc, 8);
        let current = read_word(&bytes, 0)?;
        let next = read_word(&bytes, 4);
        step_over_target_for_words(pc, current, next)
    }
}

fn read_word(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn step_over_target_for_words(pc: u64, word: u32, next: Option<u32>) -> Option<u64> {
    match decoder::decode(word).ok()? {
        DecodedInsn::Jal { rd: 1, .. } | DecodedInsn::Jalr { rd: 1, .. } => pc.checked_add(4),
        DecodedInsn::Auipc { rd: 1, .. } if next.is_some_and(is_ra_jalr) => pc.checked_add(8),
        _ => None,
    }
}

fn is_ra_jalr(word: u32) -> bool {
    matches!(
        decoder::decode(word),
        Ok(DecodedInsn::Jalr { rd: 1, rs1: 1, .. })
    )
}

// Minimal terminal discipline: `\b` erases the previous character (guests echo
// "\b \b") and `\r` is dropped.
fn append_terminal_bytes(log: &mut String, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    for ch in String::from_utf8_lossy(bytes).chars() {
        match ch {
            '\u{8}' => {
                if log.chars().next_back().is_some_and(|last| last != '\n') {
                    log.pop();
                }
            }
            '\r' => {}
            _ => log.push(ch),
        }
    }
}

fn runtime_symbol_table(linked: &AssembledOutput) -> Vec<(u64, String)> {
    let mut table: Vec<(u64, String)> = linked
        .symbols_iter()
        .map(|(name, offset)| (LiveMachine::runtime_address(offset), name.to_owned()))
        .collect();
    table.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    table
}

fn parse_breakpoint_spec(spec: &str) -> Result<(&str, u64), String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err("empty breakpoint".to_owned());
    }
    let Some((symbol, offset)) = spec.split_once('+') else {
        return Ok((spec, 0));
    };
    let symbol = symbol.trim();
    if symbol.is_empty() {
        return Err(format!("missing symbol in `{spec}`"));
    }
    let offset = offset.trim();
    let value = if let Some(hex) = offset
        .strip_prefix("0x")
        .or_else(|| offset.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16)
    } else {
        offset.parse()
    }
    .map_err(|err| format!("invalid breakpoint offset `{offset}`: {err}"))?;
    Ok((symbol, value))
}

const FS_META_PA: u64 = 0x87BFF000;
const FS_IMAGE_PA: u64 = 0x87C00000;
const USER_BINARY_PA: u64 = 0x87F00000;
const USER_META_PA: u64 = 0x87EFF000;
const USER_CODE_VA: u64 = 0x4000_0000;

fn write_kernel_boot_assets(
    vm: &mut VirtualMachine,
    user: Option<&AssembledOutput>,
    fs_image: Option<&[u8]>,
) -> Result<(), String> {
    if let Some(image) = fs_image {
        vm.write_ram(FS_META_PA, &FS_IMAGE_PA.to_le_bytes())
            .map_err(|err| format!("write FS image address failed: {err:?}"))?;
        vm.write_ram(FS_META_PA + 8, &(image.len() as u64).to_le_bytes())
            .map_err(|err| format!("write FS image size failed: {err:?}"))?;
        vm.write_ram(FS_IMAGE_PA, image)
            .map_err(|err| format!("write FS image failed: {err:?}"))?;
    }

    if let Some(user) = user {
        let mut flat = user.to_flat_binary();
        let page = 4096usize;
        flat.resize(flat.len().div_ceil(page) * page, 0u8);
        let entry_off = user
            .symbol_address("_start")
            .ok_or_else(|| "user init binary is missing `_start`".to_owned())?;
        let entry_va = USER_CODE_VA + entry_off;
        vm.write_ram(USER_META_PA, &entry_va.to_le_bytes())
            .map_err(|err| format!("write user entry failed: {err:?}"))?;
        vm.write_ram(USER_META_PA + 8, &(flat.len() as u64).to_le_bytes())
            .map_err(|err| format!("write user size failed: {err:?}"))?;
        vm.write_ram(USER_BINARY_PA, &flat)
            .map_err(|err| format!("write user binary failed: {err:?}"))?;
    }

    Ok(())
}

// `web_time::Instant` is std Instant on native and performance.now() on wasm,
// so both targets honor the slice budget.
struct SliceClock {
    start: web_time::Instant,
}

impl SliceClock {
    fn start() -> Self {
        Self {
            start: web_time::Instant::now(),
        }
    }

    fn expired(&self, budget: Duration) -> bool {
        self.start.elapsed() >= budget
    }

    fn elapsed_secs(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runconfig::RunConfig;
    use crate::session::SessionEngine;
    use crate::workspace::{MemoryFs, VPath};
    use asm_to_binary::riscv::rv64i::{Addi, Auipc, Jal, Jalr};
    use asm_to_binary::traits::Instruction as _;

    const LONG_BUDGET: Duration = Duration::from_secs(5);

    fn build_artifacts(source: &str) -> BuildArtifacts {
        let mut fs = MemoryFs::new();
        fs.write_text(
            &VPath::parse("demo/demo.build").expect("path"),
            "name = \"demo\"\ntarget = \"hosted\"\nroot = \"demo.hll\"\nstdlib = \"hosted\"\n",
        )
        .expect("write manifest");
        fs.write_text(&VPath::parse("demo/demo.hll").expect("path"), source)
            .expect("write source");
        let config = RunConfig::hosted("Demo", VPath::parse("demo/demo.build").expect("path"));
        SessionEngine::new(&fs)
            .build_config(&config)
            .expect("build config")
            .artifacts
    }

    fn drive_to_completion(live: &mut LiveMachine) {
        for _ in 0..1_000 {
            if live.state != MachineState::Running {
                return;
            }
            live.drive(10_000, LONG_BUDGET);
        }
        panic!("machine did not finish within the test budget");
    }

    #[test]
    fn hosted_program_exits_through_drive() {
        let artifacts = build_artifacts("main: () -> i32 { return 7 }");
        let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
        drive_to_completion(&mut live);
        assert_eq!(live.state, MachineState::Exited(7));
        assert!(live.steps > 0);
        let result = live.result();
        assert_eq!(result.exit_code, Some(7));
        assert_eq!(result.steps, live.steps);
        assert!(result.registers.is_some());
    }

    #[test]
    fn time_travel_restores_earlier_cpu_state() {
        let artifacts = build_artifacts(
            "main: () -> i32 {\n    a: i32 = 1\n    a = a + 10\n    a = a + 100\n    return a\n}",
        );
        let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
        live.pause();
        live.timeline.set_enabled(true);

        // Step a few instructions; each pause captures a frame.
        live.step_instruction();
        live.step_instruction();
        let early_frame = live.timeline.len() - 1;
        let early_pc = live.registers().pc;
        let early_x = live.registers().x;

        // Advance further so the live pc moves on.
        for _ in 0..4 {
            live.step_instruction();
        }
        assert_ne!(live.registers().pc, early_pc, "pc should have advanced");
        assert!(live.timeline.len() > early_frame + 1);

        // Rewind to the earlier frame; CPU state must match exactly.
        assert!(live.restore_timepoint(early_frame));
        assert_eq!(live.registers().pc, early_pc);
        assert_eq!(live.registers().x, early_x);
        assert_eq!(live.state, MachineState::Paused);
    }

    #[test]
    fn drive_respects_the_step_budget() {
        let artifacts = build_artifacts("main: () -> i32 { return 7 }");
        let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
        let stepped = live.drive(3, LONG_BUDGET);
        assert_eq!(stepped, 3);
        assert_eq!(live.state, MachineState::Running);
        assert_eq!(live.steps, 3);
    }

    #[test]
    fn pause_blocks_drive_and_step_one_advances() {
        let artifacts = build_artifacts("main: () -> i32 { return 7 }");
        let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
        live.pause();
        assert_eq!(live.drive(100, LONG_BUDGET), 0);
        assert_eq!(live.steps, 0);
        live.step_one();
        live.step_one();
        assert_eq!(live.steps, 2);
        assert_eq!(live.state, MachineState::Paused);
        live.resume();
        drive_to_completion(&mut live);
        assert_eq!(live.state, MachineState::Exited(7));
    }

    #[test]
    fn call_stack_reports_pc_and_the_calling_frames() {
        let artifacts = build_artifacts(
            "inner: () -> i32 { return 5 }\n\
             outer: () -> i32 { return inner() + 1 }\n\
             main: () -> i32 { return outer() }",
        );
        let inner_pc = LiveMachine::runtime_address(
            artifacts
                .linked
                .symbol_address("demo__inner")
                .expect("inner symbol"),
        );
        let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
        live.breakpoints.insert(inner_pc);
        for _ in 0..1_000 {
            if live.state != MachineState::Running {
                break;
            }
            live.drive(1_000, LONG_BUDGET);
        }
        assert_eq!(live.state, MachineState::Paused);

        let frames = live.call_stack(8);
        assert_eq!(frames[0], inner_pc);
        let names: Vec<&str> = frames
            .iter()
            .filter_map(|&pc| live.symbol_near(pc).map(|(name, _)| name))
            .collect();
        assert!(
            names.iter().any(|n| n.starts_with("demo__outer")),
            "outer missing from {names:?}"
        );
        assert!(
            names.iter().any(|n| n.starts_with("main")),
            "main missing from {names:?}"
        );
    }

    #[test]
    fn breakpoint_pauses_at_the_same_pc_and_step_count() {
        let artifacts = build_artifacts("main: () -> i32 { return 7 }");
        let main_pc = LiveMachine::runtime_address(
            artifacts
                .linked
                .symbol_address("main")
                .expect("main symbol"),
        );

        let run_to_breakpoint = |artifacts: &BuildArtifacts| {
            let mut live = LiveMachine::new_hosted(artifacts).expect("boot");
            live.breakpoints.insert(main_pc);
            for _ in 0..1_000 {
                if live.state != MachineState::Running {
                    break;
                }
                live.drive(1_000, LONG_BUDGET);
            }
            live
        };

        let first = run_to_breakpoint(&artifacts);
        assert_eq!(first.state, MachineState::Paused);
        assert_eq!(first.registers().pc, main_pc);

        let second = run_to_breakpoint(&artifacts);
        assert_eq!(second.steps, first.steps, "breakpoint hit is deterministic");

        // Resuming steps off the breakpoint instead of re-triggering it.
        let mut live = first;
        live.resume();
        drive_to_completion(&mut live);
        assert_eq!(live.state, MachineState::Exited(7));
    }

    #[test]
    fn terminal_bytes_honor_backspace_and_carriage_return() {
        let mut log = String::new();
        append_terminal_bytes(&mut log, b"/$ helq");
        append_terminal_bytes(&mut log, b"\x08 \x08p\r\n");
        assert_eq!(log, "/$ help\n");
        // Backspace never erases past a line break.
        append_terminal_bytes(&mut log, b"\x08\x08x");
        assert_eq!(log, "/$ help\nx");
    }

    #[test]
    fn step_cycles_advances_and_records_the_previous_registers() {
        let artifacts = build_artifacts("main: () -> i32 { return 7 }");
        let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
        live.pause();
        live.step_cycles(5);
        assert_eq!(live.steps, 5);
        assert_eq!(live.state, MachineState::Paused);
        let prev = live.prev_step_registers().expect("prev registers");
        assert_ne!(prev.pc, live.registers().pc, "pc moved across the steps");
        live.resume();
        assert!(live.prev_step_registers().is_none(), "cleared on resume");
    }

    #[test]
    fn cycle_and_instruction_steps_are_distinct() {
        let artifacts = build_artifacts("main: () -> i32 { return 7 }");
        let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
        live.pause();

        live.step_cycle();
        assert_eq!(live.steps, 1, "cycle step is one pipeline tick");
        let retired_after_cycle = live.vm.insns_retired();

        live.step_instruction();
        assert_eq!(
            live.vm.insns_retired(),
            retired_after_cycle + 1,
            "instruction step waits for one retirement"
        );
        assert!(
            live.steps > 2,
            "instruction stepping may consume pipeline fill cycles"
        );
    }

    #[test]
    fn step_over_classifies_direct_and_indirect_calls() {
        let pc = 0x8000_1000;
        let addi = Addi::new(5, 5, 1).encode();
        let direct_call = Jal::new(1, 0x40).encode();
        let jump = Jal::new(0, 0x40).encode();
        let indirect_call = Jalr::new(1, 5, 0).encode();
        let ret = Jalr::new(0, 1, 0).encode();

        assert_eq!(
            step_over_target_for_words(pc, direct_call, None),
            Some(pc + 4)
        );
        assert_eq!(
            step_over_target_for_words(pc, indirect_call, None),
            Some(pc + 4)
        );
        assert_eq!(step_over_target_for_words(pc, addi, None), None);
        assert_eq!(step_over_target_for_words(pc, jump, None), None);
        assert_eq!(step_over_target_for_words(pc, ret, None), None);
    }

    #[test]
    fn step_over_classifies_auipc_jalr_call_pair() {
        let pc = 0x8000_1000;
        let auipc_ra = Auipc::new(1, 0).encode();
        let jalr_ra = Jalr::new(1, 1, 0).encode();
        let jalr_other = Jalr::new(1, 5, 0).encode();

        assert_eq!(
            step_over_target_for_words(pc, auipc_ra, Some(jalr_ra)),
            Some(pc + 8)
        );
        assert_eq!(
            step_over_target_for_words(pc, auipc_ra, Some(jalr_other)),
            None
        );
    }

    #[test]
    fn breakpoint_specs_resolve_against_runtime_symbols() {
        let artifacts = build_artifacts("main: () -> i32 { return 7 }");
        let live = LiveMachine::new_hosted(&artifacts).expect("boot");
        let main_pc = LiveMachine::runtime_address(
            artifacts
                .linked
                .symbol_address("main")
                .expect("main symbol"),
        );

        assert_eq!(live.resolve_breakpoint("main").expect("resolve"), main_pc);
        assert_eq!(
            live.resolve_breakpoint("main+0x10").expect("resolve"),
            main_pc + 0x10
        );
        let spec = live
            .breakpoint_spec_for_addr(main_pc + 0x10)
            .expect("format spec");
        assert_eq!(
            live.resolve_breakpoint(&spec).expect("resolve formatted"),
            main_pc + 0x10
        );
        assert!(live.resolve_breakpoint("missing").is_err());
    }

    #[test]
    fn run_to_stops_at_the_target_and_does_not_stick() {
        let artifacts = build_artifacts("main: () -> i32 { return 7 }");
        let main_pc = LiveMachine::runtime_address(
            artifacts
                .linked
                .symbol_address("main")
                .expect("main symbol"),
        );
        let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
        live.pause();
        live.run_to(main_pc);
        drive_to_completion(&mut live);
        assert_eq!(live.state, MachineState::Paused);
        assert_eq!(live.registers().pc, main_pc);
        // The one-shot breakpoint is consumed: resuming runs to completion.
        live.resume();
        drive_to_completion(&mut live);
        assert_eq!(live.state, MachineState::Exited(7));
    }

    #[test]
    fn symbols_and_memory_reads_serve_the_debugger() {
        let artifacts = build_artifacts("main: () -> i32 { return 7 }");
        let main_pc = LiveMachine::runtime_address(
            artifacts
                .linked
                .symbol_address("main")
                .expect("main symbol"),
        );
        let live = LiveMachine::new_hosted(&artifacts).expect("boot");
        assert_eq!(live.symbol_at(main_pc), Some("main"));
        let (name, offset) = live.symbol_near(main_pc).expect("nearest symbol");
        assert_eq!((name, offset), ("main", 0));
        let word = live.read_virtual(main_pc, 4);
        assert_ne!(word, vec![0u8; 4], "code bytes are readable at main");
        assert_eq!(live.read_physical(main_pc, 4), word, "identity-mapped");
    }

    #[test]
    fn line_table_maps_runtime_pc_to_source_lines() {
        let artifacts = build_artifacts("main: () -> i32 {\n    return 7\n}");
        let live = LiveMachine::new_hosted(&artifacts).expect("boot");
        assert!(live.has_line_table(), "hosted build carries a line table");

        // Round-trip a real table entry through the runtime-pc mapping the
        // debugger uses (this build also links stdlib, so entries are merged).
        let entry = artifacts
            .linked
            .line_table_iter()
            .next()
            .expect("at least one line entry");
        let file = artifacts
            .linked
            .file_name(entry.file_id)
            .expect("file name")
            .to_owned();
        let pc = LiveMachine::runtime_address(entry.addr);
        assert_eq!(live.source_for_pc(pc), Some((file.as_str(), entry.line)));
        assert_eq!(
            live.pc_for_source(&file, entry.line),
            Some(LiveMachine::runtime_address(
                artifacts
                    .linked
                    .addr_for_source(&file, entry.line)
                    .expect("addr for source"),
            )),
        );
    }

    #[test]
    fn step_line_advances_to_a_different_source_line() {
        let artifacts =
            build_artifacts("main: () -> i32 {\n    a := 1\n    b := 2\n    return a + b\n}");
        let main_pc = LiveMachine::runtime_address(
            artifacts
                .linked
                .symbol_address("main")
                .expect("main symbol"),
        );
        let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
        live.pause();
        live.run_to(main_pc);
        drive_to_completion(&mut live);
        assert_eq!(live.state, MachineState::Paused);

        let origin = live
            .source_for_pc(live.registers().pc)
            .map(|(_, line)| line)
            .expect("main pc has a source line");
        live.step_line();
        assert!(live.state.is_alive(), "step_line paused, not ran away");
        let landed = live
            .source_for_pc(live.registers().pc)
            .map(|(_, line)| line)
            .expect("landed pc has a source line");
        assert_ne!(landed, origin, "stepped onto a different source line");
    }

    #[test]
    fn step_line_captures_a_time_travel_frame() {
        let artifacts =
            build_artifacts("main: () -> i32 {\n    a := 1\n    b := 2\n    return a + b\n}");
        let main_pc = LiveMachine::runtime_address(
            artifacts
                .linked
                .symbol_address("main")
                .expect("main symbol"),
        );
        let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
        live.timeline.set_enabled(true);
        live.pause();
        live.run_to(main_pc);
        drive_to_completion(&mut live);
        assert_eq!(live.state, MachineState::Paused);

        let before = live.timeline.len();
        live.step_line();
        assert_eq!(
            live.timeline.len(),
            before + 1,
            "step_line left a scrubber tick where it paused"
        );
        assert_eq!(
            live.timeline.retired_at(live.timeline.len() - 1),
            Some(live.vm.insns_retired()),
        );
    }

    #[test]
    fn restore_timepoint_rebases_history_and_keeps_capturing() {
        let artifacts =
            build_artifacts("main: () -> i32 {\n    a := 1\n    b := 2\n    return a + b\n}");
        let main_pc = LiveMachine::runtime_address(
            artifacts
                .linked
                .symbol_address("main")
                .expect("main symbol"),
        );
        let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
        live.timeline.set_enabled(true);
        live.pause();
        live.run_to(main_pc);
        drive_to_completion(&mut live);
        for _ in 0..3 {
            live.step_instruction();
        }
        let frames = live.timeline.len();
        assert!(frames >= 4, "expected a frame per pause, got {frames}");

        assert!(live.restore_timepoint(1));
        assert_eq!(
            live.vm.insns_retired(),
            live.timeline.retired_at(1).expect("frame 1 retired count")
        );
        // Scrubbing alone drops nothing; the stale future goes on the next capture.
        assert_eq!(live.timeline.len(), frames);
        live.step_instruction();
        assert_eq!(
            live.timeline.len(),
            3,
            "truncated to frame 1 plus the new frame"
        );
        assert_eq!(
            live.timeline.retired_at(2),
            Some(live.vm.insns_retired()),
            "capture resumed from the rewound retired counter"
        );
    }

    #[test]
    fn step_line_steps_over_calls_without_pausing_in_the_callee() {
        let artifacts = build_artifacts(
            "add: (a: i32, b: i32) -> i32 {\n    c := a + b\n    return c\n}\n\nmain: () -> i32 {\n    x := add(1, 2)\n    y := x + 4\n    return x + y\n}",
        );
        let main_pc = LiveMachine::runtime_address(
            artifacts
                .linked
                .symbol_address("main")
                .expect("main symbol"),
        );
        let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
        live.pause();
        live.run_to(main_pc);
        drive_to_completion(&mut live);
        assert_eq!(live.state, MachineState::Paused);

        let origin = live
            .source_for_pc(live.registers().pc)
            .map(|(_, line)| line)
            .expect("main pc has a source line");
        assert!(origin >= 6, "paused in main (line {origin}), not add");
        live.step_line();
        assert!(live.state.is_alive(), "step_line paused, not ran away");
        let landed = live
            .source_for_pc(live.registers().pc)
            .map(|(_, line)| line)
            .expect("landed pc has a source line");
        assert_ne!(landed, origin, "stepped onto a different source line");
        assert!(landed >= 6, "landed in main (line {landed}), not in add");
    }

    #[test]
    fn step_line_in_pauses_on_the_callees_first_line() {
        let artifacts = build_artifacts(
            "add: (a: i32, b: i32) -> i32 {\n    c := a + b\n    return c\n}\n\nmain: () -> i32 {\n    x := add(1, 2)\n    y := x + 4\n    return x + y\n}",
        );
        let main_pc = LiveMachine::runtime_address(
            artifacts
                .linked
                .symbol_address("main")
                .expect("main symbol"),
        );
        let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
        live.pause();
        live.run_to(main_pc);
        drive_to_completion(&mut live);
        assert_eq!(live.state, MachineState::Paused);

        live.step_line_in();
        assert!(live.state.is_alive(), "step_line_in paused, not ran away");
        let landed = live
            .source_for_pc(live.registers().pc)
            .map(|(_, line)| line)
            .expect("landed pc has a source line");
        assert!(landed <= 4, "landed inside add (line {landed}), not main");
    }

    #[test]
    fn kernel_session_reads_user_virtual_addresses_through_the_mmu() {
        use crate::runconfig::RunKind;
        use crate::workspace::sample_workspace;

        let fs = sample_workspace().expect("sample workspace");
        let store = crate::runconfig::RunConfigStore::load(&fs).expect("run config store");
        let config = store
            .configs
            .iter()
            .find(|config| config.kind == RunKind::KernelBoot)
            .expect("kernel config");
        let boot = SessionEngine::new(&fs)
            .boot_kernel_config(config)
            .expect("boot kernel config");
        let mut live = LiveMachine::new_kernel(
            &boot.build.artifacts,
            Some(&boot.init),
            Some(&boot.fs_image),
            "",
        )
        .expect("live kernel");
        for _ in 0..1_000 {
            live.drive(100_000, LONG_BUDGET);
            if live.uart_log.contains("HLL shell ready") {
                break;
            }
            assert!(live.state.is_alive(), "kernel died: {:?}", live.state);
        }
        assert!(live.uart_log.contains("HLL shell ready"), "shell booted");

        // The shell (pid 1) runs at USER_CODE_VA behind Sv39; the debugger read
        // must translate, not treat the VA as physical (which reads zeros).
        let pa = live.translate(USER_CODE_VA).expect("user code VA maps");
        assert_ne!(pa, USER_CODE_VA, "user code is not identity-mapped");
        let virt = live.read_virtual(USER_CODE_VA, 4);
        assert_ne!(virt, vec![0u8; 4], "instruction bytes visible via the MMU");
        assert_eq!(virt, live.read_physical(pa, 4), "walk matches physical");
    }

    // A raw-asm root boots bare-metal and exits through the ROM ecall handler;
    // the session must reach Exited(0) without any hosted runtime.
    #[test]
    fn freestanding_asm_session_runs_to_exit() {
        let mut fs = MemoryFs::new();
        fs.write_text(
            &VPath::parse("asm/exit.build").expect("path"),
            "name = \"exit\"\ntarget = \"freestanding\"\nroot = \"exit.s\"\nentry = \"_start\"\n",
        )
        .expect("write manifest");
        fs.write_text(
            &VPath::parse("asm/exit.s").expect("path"),
            ".globl _start\n_start:\n  li a0, 0\n  li a7, 93\n  ecall\n",
        )
        .expect("write source");
        let config = RunConfig::hosted("Exit", VPath::parse("asm/exit.build").expect("path"));
        let artifacts = SessionEngine::new(&fs)
            .build_config(&config)
            .expect("build config")
            .artifacts;
        let mut live = LiveMachine::new_freestanding(&artifacts).expect("boot");
        drive_to_completion(&mut live);
        assert_eq!(live.state, MachineState::Exited(0));
    }

    #[test]
    fn uart_output_lands_in_the_log_and_stop_keeps_state() {
        let artifacts = build_artifacts(
            "console := import(\"console\")\n\
             main: () -> i32 {\n    console.println(\"live hello\")\n    return 0\n}",
        );
        let mut live = LiveMachine::new_hosted(&artifacts).expect("boot");
        drive_to_completion(&mut live);
        assert_eq!(live.state, MachineState::Exited(0));
        assert!(
            live.uart_log.contains("live hello"),
            "uart drained into log"
        );

        let mut stopped = LiveMachine::new_hosted(&artifacts).expect("boot");
        stopped.drive(3, LONG_BUDGET);
        stopped.stop();
        assert_eq!(stopped.state, MachineState::Stopped);
        assert_eq!(stopped.result().exit_code, None);
        assert_eq!(
            stopped.drive(100, LONG_BUDGET),
            0,
            "stopped machines stay put"
        );
    }
}
