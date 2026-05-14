//! System call handling for the RISC-V virtual machine.
//!
//! This module implements Linux-compatible syscalls for RV64, including:
//! - I/O operations (write, putchar, puts, printf)
//! - Process control (exit, exit_group)
//! - Memory management (to be implemented)
//!
//! Syscall numbers follow the Linux RISC-V ABI:
//! - 64: write(fd, buf, len)
//! - 93: exit(code)
//! - 94: exit_group(code)
//!
//! Custom syscalls (1000+):
//! - 1000: putchar(byte)
//! - 1001: puts(string_ptr)
//! - 1002: printf(fmt_ptr, ...)

use crate::virtual_machine::bus::SystemBus;
use crate::virtual_machine::cpu::csr::CsrFile;
use crate::virtual_machine::cpu::registers::Registers;
use crate::virtual_machine::error::VmError;
use crate::virtual_machine::memory::MemoryAccess;

/// Syscall outcome indicating what should happen after the syscall
#[derive(Debug)]
pub enum SyscallOutcome {
    /// Continue execution normally
    Continue,
    /// Halt the VM with the given exit code
    Halted(i64),
}

/// Handle an ecall instruction as a syscall
///
/// # Arguments
/// * `regs` - CPU registers containing syscall arguments
/// * `csrs` - CSR file for tracking statistics
/// * `bus` - System bus for memory and device access
///
/// # Returns
/// The outcome of the syscall (continue or halt)
pub fn handle_syscall(
    regs: &mut Registers,
    csrs: &mut CsrFile,
    bus: &mut SystemBus,
) -> Result<SyscallOutcome, VmError> {
    let syscall_number = regs.read_x(17); // a7 holds syscall number

    match syscall_number {
        // Linux sys_write(fd, buf, len)
        64 => sys_write(regs, csrs, bus),
        // Linux sys_exit / sys_exit_group
        93 | 94 => sys_exit(regs, csrs),
        // Custom putchar(a0 = byte)
        1000 => sys_putchar(regs, csrs, bus),
        // Custom puts(a0 = ptr to null-terminated string)
        1001 => sys_puts(regs, csrs, bus),
        // Custom printf(a0 = fmt, a1..a7 = varargs)
        1002 => sys_printf(regs, csrs, bus),
        // Unknown syscall - return error
        _ => unknown_syscall(regs, csrs),
    }
}

/// sys_write: Write data to a file descriptor
/// Currently only supports stdout (fd=1) via UART
fn sys_write(
    regs: &mut Registers,
    csrs: &mut CsrFile,
    bus: &mut SystemBus,
) -> Result<SyscallOutcome, VmError> {
    let fd = regs.read_x(10); // a0
    let buf = regs.read_x(11); // a1
    let len = regs.read_x(12) as usize; // a2

    // Only support stdout (fd=1)
    if fd != 1 {
        // Unsupported file descriptor - return error
        regs.write_x(10, u64::MAX);
        advance_pc(regs, csrs);
        return Ok(SyscallOutcome::Continue);
    }

    let mut written = 0usize;
    for i in 0..len {
        let byte = bus.read_byte(buf + i as u64).unwrap_or(0);
        let _ = bus.uart_mut().write_byte(0, byte);
        written += 1;
    }

    // Return number of bytes written
    regs.write_x(10, written as u64);
    advance_pc(regs, csrs);
    Ok(SyscallOutcome::Continue)
}

/// sys_exit: Terminate the process with an exit code
fn sys_exit(regs: &mut Registers, csrs: &mut CsrFile) -> Result<SyscallOutcome, VmError> {
    let exit_code = regs.read_x(10) as i64; // a0
    Ok(SyscallOutcome::Halted(exit_code))
}

/// sys_putchar: Write a single character to stdout
fn sys_putchar(
    regs: &mut Registers,
    csrs: &mut CsrFile,
    bus: &mut SystemBus,
) -> Result<SyscallOutcome, VmError> {
    let c = regs.read_x(10) as u8; // a0
    let _ = bus.uart_mut().write_byte(0, c);
    regs.write_x(10, 0); // Return 0 on success
    advance_pc(regs, csrs);
    Ok(SyscallOutcome::Continue)
}

/// sys_puts: Write a null-terminated string followed by newline to stdout
fn sys_puts(
    regs: &mut Registers,
    csrs: &mut CsrFile,
    bus: &mut SystemBus,
) -> Result<SyscallOutcome, VmError> {
    let mut ptr = regs.read_x(10); // a0

    // Write string characters until null terminator
    loop {
        let byte = bus.read_byte(ptr).unwrap_or(0);
        if byte == 0 {
            break;
        }
        let _ = bus.uart_mut().write_byte(0, byte);
        ptr += 1;
    }

    // Write newline
    let _ = bus.uart_mut().write_byte(0, b'\n');
    regs.write_x(10, 0); // Return 0 on success
    advance_pc(regs, csrs);
    Ok(SyscallOutcome::Continue)
}

/// sys_printf: Format and write a string to stdout
/// Supports format specifiers: %d, %i, %u, %x, %X, %p, %c, %s, %f, %g, %e, %%
fn sys_printf(
    regs: &mut Registers,
    csrs: &mut CsrFile,
    bus: &mut SystemBus,
) -> Result<SyscallOutcome, VmError> {
    let output = vm_printf(regs, bus);
    let len = output.len();

    for byte in output {
        let _ = bus.uart_mut().write_byte(0, byte);
    }

    regs.write_x(10, len as u64); // Return number of bytes written
    advance_pc(regs, csrs);
    Ok(SyscallOutcome::Continue)
}

/// Handle unknown syscall - return error code
fn unknown_syscall(regs: &mut Registers, csrs: &mut CsrFile) -> Result<SyscallOutcome, VmError> {
    regs.write_x(10, u64::MAX); // Return -1 (error)
    advance_pc(regs, csrs);
    Ok(SyscallOutcome::Continue)
}

/// Advance PC and increment counters after successful syscall
fn advance_pc(regs: &mut Registers, csrs: &mut CsrFile) {
    regs.pc = regs.pc.wrapping_add(4);
    csrs.increment_instret();
    csrs.increment_cycle();
}

// ---------------------------------------------------------------------------
// printf implementation
// ---------------------------------------------------------------------------

/// Minimal printf: handles %d %i %u %x %X %c %s %f %% and width-less %p.
fn vm_printf(regs: &Registers, bus: &mut SystemBus) -> Vec<u8> {
    let fmt_ptr = regs.read_x(10);
    let mut arg_reg = 11u32; // a1..a7 supply arguments

    let mut out = Vec::<u8>::new();
    let mut addr = fmt_ptr;

    loop {
        let c = bus.read_byte(addr).unwrap_or(0);
        addr += 1;
        if c == 0 {
            break;
        }

        if c != b'%' {
            out.push(c);
            continue;
        }

        // Read the format specifier (skip simple flags/width for now)
        let spec = bus.read_byte(addr).unwrap_or(0);
        addr += 1;

        let arg = if arg_reg <= 17 {
            let v = regs.read_x(arg_reg as usize);
            arg_reg += 1;
            v
        } else {
            0
        };

        match spec {
            b'd' | b'i' => {
                let s = (arg as i64).to_string();
                out.extend_from_slice(s.as_bytes());
            }
            b'u' => {
                out.extend_from_slice(arg.to_string().as_bytes());
            }
            b'x' => {
                out.extend_from_slice(format!("{arg:x}").as_bytes());
            }
            b'X' => {
                out.extend_from_slice(format!("{arg:X}").as_bytes());
            }
            b'p' => {
                out.extend_from_slice(format!("0x{arg:x}").as_bytes());
            }
            b'c' => {
                out.push(arg as u8);
            }
            b's' => {
                let mut ptr = arg;
                loop {
                    let byte = bus.read_byte(ptr).unwrap_or(0);
                    if byte == 0 {
                        break;
                    }
                    out.push(byte);
                    ptr += 1;
                }
            }
            b'f' | b'g' | b'e' => {
                let f = f64::from_bits(arg);
                out.extend_from_slice(format!("{f}").as_bytes());
            }
            b'%' => {
                out.push(b'%');
                // No argument consumed for %%
                if arg_reg > 11 {
                    arg_reg -= 1;
                }
            }
            other => {
                out.push(b'%');
                out.push(other);
            }
        }
    }

    out
}
