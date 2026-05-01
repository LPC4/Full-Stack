//! Virtual machine error types.

use std::fmt;

/// All errors that can occur inside the virtual machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmError {
    /// Physical address is not mapped to any device.
    BusError(u64),
    /// Attempt to write to ROM or a read‑only cache.
    WriteToRom,
    /// Instruction fetch from non‑executable memory or protection fault.
    InstructionAccessFault(u64),
    /// Load from memory caused an exception.
    LoadAccessFault(u64),
    /// Store to memory caused an exception.
    StoreAccessFault(u64),
    /// Unrecognised or unsupported opcode.
    IllegalInstruction(u32),
    /// Environment call (ecall).
    Ecall,
    /// Breakpoint (ebreak).
    Ebreak,
    /// Other runtime error with a descriptive message.
    Other(String),
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VmError::BusError(addr) => write!(f, "bus error at address {addr:#010x}"),
            VmError::WriteToRom => write!(f, "write to read‑only memory"),
            VmError::InstructionAccessFault(addr) => write!(f, "instruction access fault at {addr:#010x}"),
            VmError::LoadAccessFault(addr) => write!(f, "load access fault at {addr:#010x}"),
            VmError::StoreAccessFault(addr) => write!(f, "store access fault at {addr:#010x}"),
            VmError::IllegalInstruction(word) => write!(f, "illegal instruction {word:#010x}"),
            VmError::Ecall => write!(f, "environment call"),
            VmError::Ebreak => write!(f, "breakpoint"),
            VmError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for VmError {}