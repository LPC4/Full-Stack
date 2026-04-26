#[macro_use]
mod macros;

pub mod assembler;
pub mod encode_decode;
pub mod pseudo;
pub mod real;
pub mod rv64a;
pub mod rv64fd;
pub mod rv64i;
pub mod rv64m;
pub mod rv64zicsr;
pub mod rv_instruction;
pub mod traits;
pub mod utils;

pub use pseudo::PseudoInstruction;
pub use real::RealInstruction;
pub use rv_instruction::RvInstruction;
pub use traits::Instruction;
