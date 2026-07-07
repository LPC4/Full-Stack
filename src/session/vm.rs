/// Outcome of running an assembled program: captured UART output, exit code,
/// step count, whether the budget ran out, and the final CPU register file.
#[derive(Clone)]
pub struct VmExecutionResult {
    pub uart_output: String,
    pub exit_code: Option<i32>,
    pub steps: u64,
    pub max_steps_reached: bool,
    /// Final register file, or `None` if the image never loaded.
    pub registers: Option<RegisterSnapshot>,
}

/// A snapshot of the integer register file plus the program counter, taken when the
/// run stops. Decoding stays out of the VM: the UI reads these plain fields.
#[derive(Clone)]
pub struct RegisterSnapshot {
    pub pc: u64,
    pub x: [u64; 32],
}

impl RegisterSnapshot {
    /// RISC-V integer register ABI names, indexed by register number x0..x31.
    pub const ABI_NAMES: [&'static str; 32] = [
        "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4",
        "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4",
        "t5", "t6",
    ];
}
