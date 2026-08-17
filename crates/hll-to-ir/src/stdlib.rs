use crate::TargetMode;

const STDLIB_BUILD: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../os-runtime/stdlib/stdlib.build"
));

/// The stdlib as (`module_name`, source) tuples for `mode`. Modules stay separate so
/// each one yields its own `.ir`, `.s` and `.o` instead of one concatenated bundle.
pub fn get_stdlib_modules_for_mode(mode: TargetMode) -> Vec<(&'static str, &'static str)> {
    let manifest = fs_utils::build_manifest::parse(STDLIB_BUILD).expect("parse stdlib.build");
    let groups: &[&str] = match mode {
        TargetMode::Hosted => &["hosted_platform", "common", "hosted"],
        TargetMode::Freestanding => &["baremetal", "common", "freestanding"],
        TargetMode::Kernel => &["baremetal", "common", "kernel"],
    };
    groups
        .iter()
        .flat_map(|key| {
            manifest
                .list(key)
                .expect("parse stdlib module list")
                .unwrap_or_else(|| panic!("stdlib.build missing `{key}` list"))
        })
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
    let manifest = fs_utils::build_manifest::parse(STDLIB_BUILD).ok()?;
    let groups: &[&str] = match mode {
        TargetMode::Hosted => &["hosted_platform", "common", "hosted"],
        TargetMode::Freestanding => &["baremetal", "common", "freestanding"],
        TargetMode::Kernel => &["baremetal", "common", "kernel"],
    };
    groups.iter().find_map(|key| {
        manifest.list(key).ok()??.into_iter().find_map(|entry| {
            let (module_name, source_key) = parse_module_entry(&entry);
            (module_name == name)
                .then(|| os_runtime::module_path(source_key))
                .flatten()
        })
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

#[cfg(test)]
mod tests {
    use super::{get_stdlib_modules_for_mode, stdlib_module_source_path};
    use crate::TargetMode;

    #[test]
    fn hosted_order_uses_hosted_allocator_and_runtime_sources() {
        let modules = get_stdlib_modules_for_mode(TargetMode::Hosted);
        let names: Vec<_> = modules.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            [
                "sys",
                "heap_backend",
                "console_backend",
                "alloc",
                "vec",
                "owned_string",
                "arena",
                "pool",
                "string",
                "mem",
                "ascii",
                "format_float",
                "format",
                "console",
                "cstr",
                "runtime",
            ]
        );

        let source = modules
            .iter()
            .find(|(name, _)| *name == "heap_backend")
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
                "heap_backend",
                "console_backend",
                "runtime",
                "alloc",
                "vec",
                "owned_string",
                "arena",
                "pool",
                "string",
                "mem",
                "ascii",
                "format_float",
                "format",
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

    #[test]
    fn freestanding_composes_baremetal_common_and_entry_groups() {
        let modules = get_stdlib_modules_for_mode(TargetMode::Freestanding);
        let names: Vec<_> = modules.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            [
                "heap_backend",
                "console_backend",
                "runtime",
                "alloc",
                "vec",
                "owned_string",
                "arena",
                "pool",
                "string",
                "mem",
                "ascii",
                "format_float",
                "format",
                "console",
                "entry",
            ]
        );
        assert_eq!(
            stdlib_module_source_path(TargetMode::Freestanding, "entry"),
            Some("crates/os-runtime/stdlib/platform/freestanding/entry.hll")
        );
    }
}
