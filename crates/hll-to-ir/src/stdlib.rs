use crate::TargetMode;
use crate::ir::{IntWidth, IrType};

const STDLIB_BUILD: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../os-runtime/stdlib/stdlib.build"
));

/// The stdlib as (`module_name`, source) tuples for `mode`. Modules stay separate so
/// each one yields its own `.ir`, `.s` and `.o` instead of one concatenated bundle.
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

/// Repo-relative source path of stdlib module `name` in `mode`, for the
/// pc-to-source line table. `None` when the module or its path is unknown.
pub fn stdlib_module_source_path(mode: TargetMode, name: &str) -> Option<&'static str> {
    let manifest = build_manifest::parse(STDLIB_BUILD).ok()?;
    let key = match mode {
        TargetMode::Hosted => "hosted",
        TargetMode::Freestanding => "freestanding",
        TargetMode::Kernel => "kernel",
    };
    let entries = manifest.list(key).ok()??;
    entries.into_iter().find_map(|entry| {
        let (module_name, source_key) = parse_module_entry(&entry);
        (module_name == name)
            .then(|| os_runtime::module_path(source_key))
            .flatten()
    })
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

/// Shared named types registered directly in the compiler context, so a module like
/// `memory_allocator.hll` resolves `HeapBlock` without concatenating `types.hll`.
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
                "heap_source",
                "memory_allocator",
                "string",
                "mem",
                "cstr",
                "sys",
                "console",
                "runtime",
            ]
        );

        let source = modules
            .iter()
            .find(|(name, _)| *name == "heap_source")
            .expect("hosted heap source")
            .1;
        assert!(
            source.contains("heap_brk"),
            "hosted heap source should be backed by brk"
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
                "heap_source",
                "memory_allocator",
                "string",
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
