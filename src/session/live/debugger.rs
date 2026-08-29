use super::stepping::read_word;
use super::{
    AssembledOutput, DecodedInsn, FsSymbols, LiveMachine, MachineState, OsSymbols,
    RegisterSnapshot, VirtualMachine, VmExecutionResult, decoder,
};

impl LiveMachine {
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
            fault: match &self.state {
                MachineState::Faulted(message) => Some(message.clone()),
                _ => None,
            },
            steps: self.steps,
            max_steps_reached: false,
            registers: Some(self.registers()),
        }
    }
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
