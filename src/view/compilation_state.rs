#[derive(Default)]
pub struct CompilationState {
    pub tokens: String,
    pub ast: String,
    pub ir: String,
    pub asm: String,
    pub error: Option<String>,
    pub just_compiled: bool,
}