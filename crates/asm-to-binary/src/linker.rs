/// Primitive linker for RISC-V assembly programs.
///
/// Combines assembly sources with runtime glue (`_start`, `putchar`, `puts`,
/// `print_int`, `printf`) and produces a single linked assembly output ready
/// for compilation to an executable via a RISC-V cross-compiler.
///
/// Future: replace with a proper ELF linker that links object files directly.

/// Hand-written RISC-V assembly glue that is prepended to every linked program:
///
/// `_start` - called by the Linux kernel; calls `main` and exits with its
///            return value via syscall 93 (sys_exit_group).
///
/// `putchar` - writes one byte to stdout (fd=1) via syscall 64 (sys_write),
///             then returns the character, matching the C libc signature.
///
/// `puts` - writes a null-terminated string + newline via sys_write.
///
/// `printf` - formatted output via sys_write (simple implementation).
pub fn runtime_glue() -> &'static str {
    "\t.text\n\
     .globl _start\n\
     _start:\n\
     \tcall main\n\
     \tli a7, 93\n\
     \tecall\n\
     \n\
     .globl putchar\n\
     putchar:\n\
     \taddi sp, sp, -16\n\
     \tsd ra, 8(sp)\n\
     \tsd a0, 0(sp)\n\
     \tsb a0, 7(sp)\n\
     \tli a0, 1\n\
     \taddi a1, sp, 7\n\
     \tli a2, 1\n\
     \tli a7, 64\n\
     \tecall\n\
     \tld a0, 0(sp)\n\
     \tld ra, 8(sp)\n\
     \taddi sp, sp, 16\n\
     \tret\n\
     \n\
     .globl puts\n\
     puts:\n\
     \taddi sp, sp, -16\n\
     \tsd ra, 8(sp)\n\
     \tsd s0, 0(sp)\n\
     \tmv s0, a0\n\
     __puts_loop:\n\
     \tlbu a0, 0(s0)\n\
     \tbeq a0, x0, __puts_done\n\
     \tcall putchar\n\
     \taddi s0, s0, 1\n\
     \tj __puts_loop\n\
     __puts_done:\n\
     \tli a0, 10\n\
     \tcall putchar\n\
     \tli a0, 0\n\
     \tld s0, 0(sp)\n\
     \tld ra, 8(sp)\n\
     \taddi sp, sp, 16\n\
     \tret\n\
     \n\
     .globl print_int\n\
     print_int:\n\
     \taddi sp, sp, -48\n\
     \tsd ra, 40(sp)\n\
     \tsd s0, 32(sp)\n\
     \tsd s1, 24(sp)\n\
     \tli s0, 0\n\
     \tli s1, 0\n\
     \tbge a0, x0, __pi_pos\n\
     \tli s1, 1\n\
     \tsub a0, x0, a0\n\
     __pi_pos:\n\
     \tbeq a0, x0, __pi_zero\n\
     __pi_digit_loop:\n\
     \tli t0, 10\n\
     \tremu t1, a0, t0\n\
     \tdivu a0, a0, t0\n\
     \taddi t1, t1, 48\n\
     \tadd t2, sp, s0\n\
     \tsb t1, 0(t2)\n\
     \taddi s0, s0, 1\n\
     \tbne a0, x0, __pi_digit_loop\n\
     \tj __pi_output\n\
     __pi_zero:\n\
     \tli t0, 48\n\
     \tsb t0, 0(sp)\n\
     \tli s0, 1\n\
     __pi_output:\n\
     \tbeq s1, x0, __pi_print\n\
     \tli a0, 45\n\
     \tcall putchar\n\
     __pi_print:\n\
     \taddi s0, s0, -1\n\
     __pi_loop:\n\
     \tadd t0, sp, s0\n\
     \tlbu a0, 0(t0)\n\
     \tcall putchar\n\
     \taddi s0, s0, -1\n\
     \tbge s0, x0, __pi_loop\n\
     \tld s1, 24(sp)\n\
     \tld s0, 32(sp)\n\
     \tld ra, 40(sp)\n\
     \taddi sp, sp, 48\n\
     \tret\n\
     \n\
     .globl printf\n\
     printf:\n\
     \taddi sp, sp, -32\n\
     \tsd ra, 24(sp)\n\
     \tsd s0, 16(sp)\n\
     \tsd s1, 8(sp)\n\
     \tmv s0, a0\n\
     \tmv s1, a1\n\
     __printf_loop:\n\
     \tlbu a0, 0(s0)\n\
     \tbeq a0, x0, __printf_done\n\
     \tli t0, 37\n\
     \tbne a0, t0, __printf_char\n\
     \tlbu t1, 1(s0)\n\
     \tli t0, 100\n\
     \tbne t1, t0, __printf_char\n\
     \taddi s0, s0, 2\n\
     \tmv a0, s1\n\
     \tcall print_int\n\
     \tj __printf_loop\n\
     __printf_char:\n\
     \tcall putchar\n\
     \taddi s0, s0, 1\n\
     \tj __printf_loop\n\
     __printf_done:\n\
     \tli a0, 0\n\
     \tld s1, 8(sp)\n\
     \tld s0, 16(sp)\n\
     \tld ra, 24(sp)\n\
     \taddi sp, sp, 32\n\
     \tret\n"
}

/// Links multiple assembly sources into a single combined assembly output.
///
/// The linking order is:
/// 1. Runtime glue (_start, putchar)
/// 2. Additional assembly sources (stdlib, user code, etc.)
pub fn link_assembly(sources: &[&str]) -> String {
    let mut result = String::new();

    // Start with runtime glue
    result.push_str(runtime_glue());
    result.push('\n');

    // Append all additional sources
    for (i, source) in sources.iter().enumerate() {
        if !source.is_empty() {
            result.push_str(source);
            if i < sources.len() - 1 {
                result.push('\n');
            }
        }
    }

    result
}

/// Strips inline comments from assembly source (lines starting with ';').
///
/// This is useful for preparing assembly for shell heredoc processing or
/// other environments where comments might cause issues.
///
/// # Arguments
/// * `asm` - Assembly source with potential inline comments
///
/// # Returns
/// Assembly source with inline comments removed
pub fn strip_comments(asm: &str) -> String {
    asm.lines()
        .map(|line| line.split(';').next().unwrap_or("").trim_end())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Represents a linked assembly program ready for compilation.
#[derive(Debug, Clone)]
pub struct LinkedProgram {
    /// The combined assembly source
    pub assembly: String,
}

impl LinkedProgram {
    /// Create a new linked program from assembly sources
    pub fn new(sources: &[&str]) -> Self {
        Self {
            assembly: link_assembly(sources),
        }
    }

    /// Create a new linked program and strip comments
    pub fn new_stripped(sources: &[&str]) -> Self {
        let linked = link_assembly(sources);
        Self {
            assembly: strip_comments(&linked),
        }
    }

    /// Get the assembly source as a string slice
    pub fn as_str(&self) -> &str {
        &self.assembly
    }

    /// Convert into owned String
    pub fn into_string(self) -> String {
        self.assembly
    }
}

impl std::fmt::Display for LinkedProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.assembly)
    }
}
