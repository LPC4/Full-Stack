//! Virtual machine error types.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmError {
    BusError(u64),
    WriteToRom,
    InstructionAccessFault(u64),
    LoadAccessFault(u64),
    StoreAccessFault(u64),
    IllegalInstruction(u32),
    Ecall,
    Ebreak,
    PageFault(u64),
    Mret,
    Sret,
    /// Other runtime error with a descriptive message.
    Other(String),
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BusError(addr) => write!(f, "bus error at address {addr:#010x}"),
            Self::WriteToRom => write!(f, "write to read-only memory"),
            Self::InstructionAccessFault(addr) => {
                write!(f, "instruction access fault at {addr:#010x}")
            }
            Self::LoadAccessFault(addr) => write!(f, "load access fault at {addr:#010x}"),
            Self::StoreAccessFault(addr) => write!(f, "store access fault at {addr:#010x}"),
            Self::IllegalInstruction(word) => write!(f, "illegal instruction {word:#010x}"),
            Self::Ecall => write!(f, "environment call"),
            Self::Ebreak => write!(f, "breakpoint"),
            Self::PageFault(addr) => write!(f, "page fault at {addr:#010x}"),
            Self::Mret => write!(f, "return from machine-mode trap (MRET)"),
            Self::Sret => write!(f, "return from supervisor-mode trap (SRET)"),
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for VmError {}
