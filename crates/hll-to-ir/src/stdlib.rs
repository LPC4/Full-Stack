use crate::TargetMode;
use crate::ir::{IntWidth, IrType};

const STDLIB_BUILD: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../os-runtime/stdlib/stdlib.build"
));

/// Return the stdlib as a list of (`module_name`, source) tuples for the given mode.
///
/// This makes it possible to compile each HLL file independently so that
/// `.ir`, `.s` and `.o` artifacts exist per original source file instead of
/// concatenating everything into one big bundle.
pub fn get_stdlib_modules_for_mode(mode: TargetMode) -> Vec<(&'static str, &'static str)> {
    let manifest = build_manifest::parse(STDLIB_BUILD).expect("parse stdlib.build");
    let key = match mode {
        TargetMode::Hosted => "hosted",
        TargetMode::Freestanding => "freestanding",
        TargetMode::Kernel => "kernel",
    };
    let entries = manifest
        .list(key)
        .expect("parse stdlib module list")
        .unwrap_or_else(|| panic!("stdlib.build missing `{key}` list"));

    entries
        .into_iter()
        .map(|entry| {
            let (name, source_key) = parse_module_entry(&entry);
            let source = os_runtime::module_source(source_key)
                .unwrap_or_else(|| panic!("stdlib.build references unknown module `{source_key}`"));
            (leak(name), source)
        })
        .collect()
}

fn parse_module_entry(entry: &str) -> (String, &str) {
    match entry.split_once('=') {
        Some((name, source_key)) => (name.to_owned(), source_key),
        None => (entry.to_owned(), entry),
    }
}

fn leak(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

/// Shared named types required by independent stdlib modules.
///
/// These are registered directly in the compiler context so that modules like
/// `memory_allocator.hll` can resolve `HeapBlock` without concatenating `types.hll`.
pub fn get_stdlib_type_prelude() -> Vec<(String, IrType)> {
    let heap_block = IrType::Aggregate(vec![
        (
            "next".to_owned(),
            IrType::Pointer(Box::new(IrType::Named("HeapBlock".to_owned()))),
        ),
        (
            "ptr".to_owned(),
            IrType::Pointer(Box::new(IrType::Integer(IntWidth::I8))),
        ),
        ("size".to_owned(), IrType::Integer(IntWidth::I64)),
        ("is_free".to_owned(), IrType::Integer(IntWidth::I64)),
    ]);

    vec![("HeapBlock".to_owned(), heap_block)]
}

#[cfg(test)]
mod tests {
    use super::get_stdlib_modules_for_mode;
    use crate::TargetMode;

    #[test]
    fn hosted_order_uses_hosted_allocator_and_runtime_sources() {
        let modules = get_stdlib_modules_for_mode(TargetMode::Hosted);
        let names: Vec<_> = modules.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            [
                "types",
                "memory_allocator",
                "string_utils",
                "runtime",
                "syscalls"
            ]
        );

        let allocator = modules
            .iter()
            .find(|(name, _)| *name == "memory_allocator")
            .expect("hosted allocator")
            .1;
        assert!(
            allocator.contains("heap_brk"),
            "hosted allocator should be backed by brk"
        );
    }

    #[test]
    fn kernel_order_uses_kernel_entry_source() {
        let modules = get_stdlib_modules_for_mode(TargetMode::Kernel);
        let names: Vec<_> = modules.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            [
                "types",
                "memory_allocator",
                "string_utils",
                "mem",
                "runtime",
                "console",
                "klog",
                "entry",
            ]
        );

        let entry = modules
            .iter()
            .find(|(name, _)| *name == "entry")
            .expect("kernel entry")
            .1;
        assert!(entry.contains("_kernel_start"));
    }
}
