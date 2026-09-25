// Program file management and catalog

use hll_to_ir::TargetMode;
use hll_to_ir::stdlib::get_stdlib_modules_for_mode;

/// What a catalog entry is, for badge + grouping in the file list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CatalogBadge {
    /// Compiles + links into something runnable (tools, demos, examples, kernel).
    Runnable,
    /// Read-only reference source that does not link into anything (stdlib).
    Reference,
    /// A translation unit that is part of a runnable program (helper module, kernel fragment).
    Fragment,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProgramKind {
    Example,
    Stdlib, // Read-only stdlib programs
    Os,     // Read-only OS / kernel programs
    User,   // Read-only userspace programs (shell, editor, assembler, demos)
}

#[derive(Clone, Debug)]
pub struct ProgramFile {
    pub id: String,
    pub name: String,
    pub kind: ProgramKind,
    pub source: String,
    pub source_path: Option<String>,
    pub build_path: Option<String>,
    pub standalone: bool, // compile without linking stdlib (set for runtime/stdlib reference files)
    pub parent_id: Option<String>, // set for helper modules and kernel fragments; their owner's id
    pub layout: String,   // shared HLL header prepended before compile (empty if none)
    pub description: String,
}

impl ProgramFile {
    fn display_name_from_file_name(file_name: &str) -> String {
        file_name
            .trim_end_matches(".hll")
            .replace('_', " ")
            .split_whitespace()
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn example(
        id: &str,
        folder_name: &str,
        file_name: &str,
        description: &str,
        source: &str,
    ) -> Self {
        Self {
            id: id.to_owned(),
            name: Self::display_name_from_file_name(file_name),
            kind: ProgramKind::Example,
            source: source.to_owned(),
            source_path: Some(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("programs")
                    .join("example")
                    .join(folder_name)
                    .join(file_name)
                    .display()
                    .to_string(),
            ),
            build_path: Some(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("programs")
                    .join("example")
                    .join(folder_name)
                    .join(format!("{folder_name}.build"))
                    .display()
                    .to_string(),
            ),
            standalone: false,
            parent_id: None,
            layout: String::new(),
            description: description.to_owned(),
        }
    }

    pub fn stdlib(id: &str, name: &str, description: &str, source: &str) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            kind: ProgramKind::Stdlib,
            source: source.to_owned(),
            source_path: None,
            build_path: None,
            standalone: false,
            parent_id: None,
            layout: String::new(),
            description: description.to_owned(),
        }
    }

    pub fn os(id: &str, name: &str, description: &str, source: &str) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            kind: ProgramKind::Os,
            source: source.to_owned(),
            source_path: None,
            build_path: None,
            standalone: false,
            parent_id: None,
            layout: String::new(),
            description: description.to_owned(),
        }
    }

    pub fn user(id: &str, name: &str, description: &str, source: &str) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            kind: ProgramKind::User,
            source: source.to_owned(),
            source_path: None,
            build_path: None,
            standalone: false,
            parent_id: None,
            layout: String::new(),
            description: description.to_owned(),
        }
    }

    fn is_stdlib(&self) -> bool {
        matches!(self.kind, ProgramKind::Stdlib)
    }

    /// Runnability classification for the catalog badge + grouping.
    pub fn badge(&self) -> CatalogBadge {
        if self.parent_id.is_some() {
            CatalogBadge::Fragment
        } else if self.is_stdlib() || self.standalone {
            CatalogBadge::Reference
        } else {
            CatalogBadge::Runnable
        }
    }
}

fn runtime_source(path: &str) -> &'static str {
    os_runtime::module_source(path).unwrap_or_else(|| panic!("missing runtime source `{path}`"))
}

fn built_in_programs() -> Vec<ProgramFile> {
    let mut programs = vec![
        // Stdlib parent. The actual stdlib files are child fragments below; the
        // compiler also builds them per-module, so the GUI must not concatenate them.
        ProgramFile::stdlib(
            "stdlib",
            "Standard Library",
            "Read-only standard library modules.",
            "; Select a standard library module below.\n",
        ),
        // OS / kernel source files (read-only, for reference; not individually compilable)
        {
            let mut p = ProgramFile::os(
                "os-kernel-entry",
                "Entry",
                "Kernel entry point: _kernel_start -> kmain.",
                runtime_source("stdlib/platform/kernel/entry.hll"),
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        {
            let mut p = ProgramFile::os(
                "os-kernel-checks",
                "Checks",
                "Boot-time diagnostics: memory self-test and system validation.",
                runtime_source("kernel/core/checks.hll"),
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        {
            let mut p = ProgramFile::os(
                "os-kernel-utilities",
                "System",
                "Kernel shutdown boundary.",
                runtime_source("kernel/arch/riscv64/system.hll"),
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        {
            let mut p = ProgramFile::os(
                "os-kernel-trap-entry",
                "Trap Entry",
                "S-mode trap entry, register save/restore, and trap-stack initialization.",
                runtime_source("kernel/trap/trap_entry.hll"),
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        {
            let mut p = ProgramFile::os(
                "os-kernel-trap-handler",
                "Trap Handler",
                "S-mode trap dispatcher: scause-based routing to interrupt/exception handlers.",
                runtime_source("kernel/trap/trap_handler.hll"),
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        {
            let mut p = ProgramFile::os(
                "os-kernel-pmm",
                "Physical Pages",
                "Owned 4 KiB physical-page allocator with reuse.",
                runtime_source("kernel/mm/physical_pages.hll"),
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        {
            let mut p = ProgramFile::os(
                "os-kernel-vmm",
                "Address Spaces",
                "Sv39 address-space construction, activation, and fork copying.",
                runtime_source("kernel/mm/address_space.hll"),
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        {
            let mut p = ProgramFile::os(
                "os-kernel-process",
                "Process",
                "Process control-block allocation, PID assignment, and user-stack construction.",
                runtime_source("kernel/sched/process.hll"),
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        {
            let mut p = ProgramFile::os(
                "os-kernel-syscall",
                "Syscall",
                "Syscall dispatch, tracing, process control, and file/device request handlers.",
                runtime_source("kernel/syscall/syscalls.hll"),
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        {
            let mut p = ProgramFile::os(
                "os-kernel-scheduler",
                "Scheduler",
                "Round-robin scheduling with ready queues, zombies, and input-wait bookkeeping.",
                runtime_source("kernel/sched/scheduler.hll"),
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        {
            let mut p = ProgramFile::os(
                "os-kernel-fs",
                "Filesystem",
                "Inode-based read-write filesystem facade for descriptors and directory operations.",
                runtime_source("kernel/fs/files.hll"),
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        // Compilable kernel: select this to build the full OS
        {
            let mut p = ProgramFile::os(
                "os-my-kernel",
                "My Kernel",
                "Compilable kernel: coordinates hardware, memory, tasks, files, and traps. Select Kernel target mode to run.",
                runtime_source("kernel/core/my_kernel.hll"),
            );
            p.build_path = Some(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("crates")
                    .join("os-runtime")
                    .join("kernel")
                    .join("kernel.build")
                    .display()
                    .to_string(),
            );
            p.source_path = Some(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("crates")
                    .join("os-runtime")
                    .join("kernel")
                    .join("core")
                    .join("my_kernel.hll")
                    .display()
                    .to_string(),
            );
            p
        },
    ];

    for (module_name, module_source) in get_stdlib_modules_for_mode(TargetMode::Hosted) {
        let mut module = ProgramFile::stdlib(
            &format!("stdlib-{module_name}"),
            &format!("{module_name}.hll"),
            "Read-only hosted standard library translation unit.",
            module_source,
        );
        module.parent_id = Some("stdlib".to_owned());
        programs.push(module);
    }

    // Read-only userspace programs from the generated catalog: hosted tools and
    // demos the shell boots as pid 1 or via bare-name execution.
    for prog in crate::userspace::PROGRAMS
        .iter()
        .filter(|p| p.is_compiled())
    {
        let parent_id = format!("user-{}", prog.name);
        let mut primary = ProgramFile::user(&parent_id, prog.title, prog.description, prog.source);
        primary.layout = prog.layout.to_owned();
        // Carry the on-disk paths so relative imports (`import("./ld_link.hll")`) resolve
        // from the tool's own directory and its `.build` manifest drives mangling/closure.
        primary.source_path = Some(prog.source_path.to_owned());
        primary.build_path = Some(prog.build_path.to_owned());
        programs.push(primary);
    }

    programs.extend([
        // Example programs: one per feature family, each printing labeled output
        // and returning exit 0 only when its self-checks pass.
        ProgramFile::example(
            "example-core-basics",
            "core_basics",
            "core_basics.hll",
            "Typed and inferred declarations, const evaluation, functions, and loops.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/core_basics/core_basics.hll"
            )),
        ),
        ProgramFile::example(
            "example-operators",
            "operators",
            "operators.hll",
            "Arithmetic, logical, bitwise, shift, cast precedence, and compound assignment.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/operators/operators.hll"
            )),
        ),
        ProgramFile::example(
            "example-pointers-and-places",
            "pointers_and_places",
            "pointers_and_places.hll",
            "Address-of, @ deref, member auto-deref, element-scaled pointer math, casts, defer.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/pointers_and_places/pointers_and_places.hll"
            )),
        ),
        ProgramFile::example(
            "example-arrays-slices-and-ranges",
            "arrays_slices_and_ranges",
            "arrays_slices_and_ranges.hll",
            "Array literals, zero fill, slices, .len, ranges, and `for` over each.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/arrays_slices_and_ranges/arrays_slices_and_ranges.hll"
            )),
        ),
        ProgramFile::example(
            "example-structs-and-binding",
            "structs_and_binding",
            "structs_and_binding.hll",
            "Named and contextual struct literals, aggregate calls/returns, and destructuring.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/structs_and_binding/structs_and_binding.hll"
            )),
        ),
        ProgramFile::example(
            "example-generics",
            "generics",
            "generics.hll",
            "Explicit and inferred generic functions, generic structs, and recursive records.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/generics/generics.hll"
            )),
        ),
        ProgramFile::example(
            "example-hll2-completion-showcase",
            "hll2_completion_showcase",
            "hll2_completion_showcase.hll",
            "Living HLL2 feature showcase: function pointers, manual vtables, impl methods, interfaces with generic bounds, and literal match patterns.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/hll2_completion_showcase/hll2_completion_showcase.hll"
            )),
        ),
        ProgramFile::example(
            "example-enums-match-and-result",
            "enums_match_and_result",
            "enums_match_and_result.hll",
            "Enums, statement and value match, a generic enum, Option, Result, and `?`.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/enums_match_and_result/enums_match_and_result.hll"
            )),
        ),
        ProgramFile::example(
            "example-strings-and-iteration",
            "strings_and_iteration",
            "strings_and_iteration.hll",
            "Strings as u8[] slices: indexing, range slicing, byte iteration, and char literals.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/strings_and_iteration/strings_and_iteration.hll"
            )),
        ),
        ProgramFile::example(
            "example-compile-time-math",
            "compile_time_math",
            "compile_time_math.hll",
            "Pure recursive functions folded into constants before runtime.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/compile_time_math/compile_time_math.hll"
            )),
        ),
    ]);

    programs
}

#[derive(Clone, Debug)]
pub struct ProgramCatalog {
    programs: Vec<ProgramFile>,
}

impl Default for ProgramCatalog {
    fn default() -> Self {
        Self {
            programs: built_in_programs(),
        }
    }
}

impl ProgramCatalog {
    pub fn all_programs(&self) -> &[ProgramFile] {
        &self.programs
    }

    pub fn get_programs_by_kind(&self, kind: ProgramKind) -> Vec<&ProgramFile> {
        self.programs.iter().filter(|p| p.kind == kind).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(programs: &'a [ProgramFile], id: &str) -> &'a ProgramFile {
        programs
            .iter()
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("missing catalog entry {id}"))
    }

    #[test]
    fn bundled_user_tool_helpers_are_imported_by_source_path() {
        let programs = built_in_programs();
        let as_prog = find(&programs, "user-as");
        assert_eq!(as_prog.badge(), CatalogBadge::Runnable);
        assert!(as_prog.source.contains("import(\"./as_object.hll\")"));
        assert!(
            programs
                .iter()
                .all(|program| program.id != "user-as-as_object")
        );

        // The tool's on-disk paths must be carried so relative imports resolve from its own
        // directory and its `.build` manifest drives the closure (mangling off).
        let ld = find(&programs, "user-ld");
        assert!(ld.source.contains("import(\"./ld_link.hll\")"));
        let source_path = ld.source_path.as_deref().expect("ld source_path");
        assert!(
            source_path
                .replace('\\', "/")
                .ends_with("programs/user/bin/ld/ld.hll"),
            "unexpected ld source_path: {source_path}"
        );
        let build_path = ld.build_path.as_deref().expect("ld build_path");
        assert!(
            build_path
                .replace('\\', "/")
                .ends_with("programs/user/bin/ld/ld.build"),
            "unexpected ld build_path: {build_path}"
        );
    }

    #[test]
    fn kernel_fragments_nest_under_my_kernel() {
        let programs = built_in_programs();
        let pmm = find(&programs, "os-kernel-pmm");
        assert_eq!(pmm.parent_id.as_deref(), Some("os-my-kernel"));
        assert_eq!(pmm.badge(), CatalogBadge::Fragment);
        assert_eq!(
            find(&programs, "os-my-kernel").badge(),
            CatalogBadge::Runnable
        );
    }

    #[test]
    fn stdlib_is_reference() {
        assert_eq!(
            find(&built_in_programs(), "stdlib").badge(),
            CatalogBadge::Reference
        );
    }

    #[test]
    fn stdlib_modules_nest_under_standard_library() {
        let programs = built_in_programs();
        let parent = find(&programs, "stdlib");
        assert_eq!(parent.badge(), CatalogBadge::Reference);
        assert!(parent.source.contains("Select a standard library module"));

        for (module_name, module_source) in get_stdlib_modules_for_mode(TargetMode::Hosted) {
            let module = find(&programs, &format!("stdlib-{module_name}"));
            assert_eq!(module.parent_id.as_deref(), Some("stdlib"));
            assert_eq!(module.badge(), CatalogBadge::Fragment);
            assert_eq!(module.source, module_source);
        }
    }
}
