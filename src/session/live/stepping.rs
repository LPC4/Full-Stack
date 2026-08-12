use super::device_io::append_terminal_bytes;
use super::{
    CLOCK_CHECK_INTERVAL, DecodedInsn, Duration, INSTRUCTION_STEP_CYCLE_CAP, LiveMachine,
    MachineState, SOURCE_LINE_STEP_CYCLE_CAP, SOURCE_LINE_STEP_TIME_CAP, StepOutcome, UART_LOG_CAP,
    decoder,
};

impl LiveMachine {
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
    pub(super) fn step_one(&mut self) {
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
    pub(super) fn step_instruction(&mut self) {
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
        self.step_line_impl_with_budget(over_calls, SOURCE_LINE_STEP_TIME_CAP);
    }

    pub(super) fn step_line_impl_with_budget(&mut self, over_calls: bool, budget: Duration) {
        if !self.state.is_alive() {
            return;
        }
        if !self.has_line_table() {
            self.step_instruction();
            return;
        }
        if self.current_instruction_is_self_jump() {
            self.step_instruction();
            return;
        }
        let origin = self
            .source_for_pc(self.registers().pc)
            .filter(|(file, _)| !file.is_empty())
            .map(|(file, line)| (file.to_owned(), line));
        self.prev_step_regs = Some(self.registers());
        let clock = SliceClock::start();
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
            if cycles.is_multiple_of(CLOCK_CHECK_INTERVAL) && clock.expired(budget) {
                break;
            }
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
}

impl LiveMachine {
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

    fn current_instruction_is_self_jump(&self) -> bool {
        let pc = self.vm.peek_pc();
        let bytes = self.vm.peek_bytes_virtual(pc, 4);
        read_word(&bytes, 0).is_some_and(is_self_jump)
    }
}

pub(super) fn read_word(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

pub(super) fn step_over_target_for_words(pc: u64, word: u32, next: Option<u32>) -> Option<u64> {
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

fn is_self_jump(word: u32) -> bool {
    matches!(decoder::decode(word), Ok(DecodedInsn::Jal { imm: 0, .. }))
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
