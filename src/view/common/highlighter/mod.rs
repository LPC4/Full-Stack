pub mod asm;
pub mod ast;
pub mod hll;
pub mod ir;

pub use asm::highlight_assembly;
pub use ast::highlight_ast;
pub use hll::highlight_code;
pub use ir::highlight_ir;
