/// Every concrete instruction type implements this trait.
pub trait Instruction: std::fmt::Debug + Clone {
    /// Encode to a 32-bit machine word.
    fn encode(&self) -> u32;

    /// Produce the canonical assembly-language string for this instruction.
    fn to_asm(&self) -> String;

    /// Human-readable name of the instruction (e.g. `"add"`, `"lw"`).
    fn mnemonic(&self) -> &'static str;
}
