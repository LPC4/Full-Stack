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
    pub const ABI_NAMES: [&'static str; 32] = asm_to_binary::INTEGER_REGISTER_NAMES;
}
