use std::collections::HashMap;
use crate::intermediate_language::{IrFunction, IrProgram, IrRegister, IrType, IrTypeAlias};

// ---------------------------------------------------------------------------
// Stdlib registries — extracted once at startup, injected into user compiles
// ---------------------------------------------------------------------------

pub struct FunctionSignature {
    pub params: Vec<IrType>,
    pub return_type: IrType,
}

#[derive(Default)]
pub struct FunctionRegistry {
    pub functions: HashMap<String, FunctionSignature>,
}

#[derive(Default)]
pub struct TypeRegistry {
    pub aliases: Vec<IrTypeAlias>,
}

/// For sret functions (large aggregate return), `IrFunction.return_type` is `Void`
/// and the real type is the inner type of the first `__sret` pointer parameter.
fn effective_return_type(func: &IrFunction) -> IrType {
    if func.return_type == IrType::Void {
        if let Some(first) = func.params.first() {
            if let IrRegister::Named(n) = &first.register {
                if n == "__sret" {
                    if let IrType::Pointer(inner) = &first.ty {
                        return *inner.clone();
                    }
                }
            }
        }
    }
    func.return_type.clone()
}

/// Walk a compiled stdlib `IrProgram` and extract the public function signatures
/// and type aliases needed to seed user-code compilation.
pub fn extract_registries(ir: &IrProgram) -> (FunctionRegistry, TypeRegistry) {
    let mut functions = HashMap::new();
    for func in &ir.functions {
        let return_type = effective_return_type(func);
        let params: Vec<IrType> = func
            .params
            .iter()
            .filter(|p| !matches!(&p.register, IrRegister::Named(n) if n == "__sret"))
            .map(|p| p.ty.clone())
            .collect();
        functions.insert(func.name.clone(), FunctionSignature { params, return_type });
    }
    (
        FunctionRegistry { functions },
        TypeRegistry { aliases: ir.type_aliases.clone() },
    )
}

// ---------------------------------------------------------------------------

const STD_TYPES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/stdlib/types.hll"
));
const STD_MEMORY: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/stdlib/memory_allocator.hll"
));
const STD_STRINGS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/stdlib/string_utils.hll"
));
const STD_IO: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/programs/stdlib/io.hll"
));

fn append_section(buf: &mut String, header: &str, content: &str) {
    buf.push_str(header);
    buf.push_str(content);
    if !buf.ends_with('\n') {
        buf.push('\n');
    }
}

/// Return the complete stdlib source, ready to prepend to any user program.
pub fn get_stdlib_source() -> String {
    let capacity = STD_TYPES.len() + STD_MEMORY.len() + STD_STRINGS.len() + STD_IO.len() + 256;
    let mut combined = String::with_capacity(capacity);
    append_section(&mut combined, "; --- stdlib: types ---\n", STD_TYPES);
    append_section(
        &mut combined,
        "; --- stdlib: memory_allocator ---\n",
        STD_MEMORY,
    );
    append_section(
        &mut combined,
        "; --- stdlib: string_utils ---\n",
        STD_STRINGS,
    );
    append_section(&mut combined, "; --- stdlib: io ---\n", STD_IO);
    combined
}

pub fn prepend_stdlib(source: &str) -> String {
    let mut combined = get_stdlib_source();
    combined.push('\n');
    combined.push_str(source);
    combined
}
