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
    /// A translation unit that is part of a runnable program (aux module, kernel fragment).
    Fragment,
}

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Debug)]
pub enum ProgramKind {
    Example,
    Custom,
    Stdlib, // Read-only stdlib programs
    Os,     // Read-only OS / kernel programs
    User,   // Read-only userspace programs (shell, editor, assembler, demos)
}

#[derive(Clone, serde::Deserialize, serde::Serialize, Debug)]
pub struct ProgramFile {
    pub id: String,
    pub name: String,
    pub kind: ProgramKind,
    pub source: String,
    #[serde(default)]
    pub standalone: bool, // compile without linking stdlib (set for runtime/stdlib reference files)
    #[serde(default)]
    pub parent_id: Option<String>, // set for aux translation units and kernel fragments; their owner's id
    #[serde(default)]
    pub layout: String, // shared HLL header prepended to this program's primary + aux units at compile (empty if none)
    #[serde(default)]
    pub undo_stack: Vec<String>,
    #[serde(default)]
    pub redo_stack: Vec<String>,
    #[serde(skip)]
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

    pub fn example(id: &str, file_name: &str, description: &str, source: &str) -> Self {
        Self {
            id: id.to_owned(),
            name: Self::display_name_from_file_name(file_name),
            kind: ProgramKind::Example,
            source: source.to_owned(),
            standalone: false,
            parent_id: None,
            layout: String::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            description: description.to_owned(),
        }
    }

    pub fn custom(id: String, name: String, source: String) -> Self {
        Self {
            id,
            name,
            kind: ProgramKind::Custom,
            source,
            standalone: false,
            parent_id: None,
            layout: String::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            description: String::from("Your personal in-memory program."),
        }
    }

    pub fn stdlib(id: &str, name: &str, description: &str, source: &str) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            kind: ProgramKind::Stdlib,
            source: source.to_owned(),
            standalone: false,
            parent_id: None,
            layout: String::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            description: description.to_owned(),
        }
    }

    pub fn os(id: &str, name: &str, description: &str, source: &str) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            kind: ProgramKind::Os,
            source: source.to_owned(),
            standalone: false,
            parent_id: None,
            layout: String::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            description: description.to_owned(),
        }
    }

    pub fn user(id: &str, name: &str, description: &str, source: &str) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            kind: ProgramKind::User,
            source: source.to_owned(),
            standalone: false,
            parent_id: None,
            layout: String::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            description: description.to_owned(),
        }
    }

    pub fn is_custom(&self) -> bool {
        matches!(self.kind, ProgramKind::Custom)
    }

    pub fn is_stdlib(&self) -> bool {
        matches!(self.kind, ProgramKind::Stdlib)
    }

    pub fn is_os(&self) -> bool {
        matches!(self.kind, ProgramKind::Os)
    }

    pub fn is_user(&self) -> bool {
        matches!(self.kind, ProgramKind::User)
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

fn built_in_programs() -> Vec<ProgramFile> {
    // Read-only reference view of the hosted stdlib (display only, not a build
    // input): join the per-module sources the pipeline compiles separately.
    let stdlib_combined = get_stdlib_modules_for_mode(TargetMode::Hosted)
        .iter()
        .map(|(name, src)| format!("; --- {name} ---\n{src}"))
        .collect::<Vec<_>>()
        .join("\n");

    let mut programs = vec![
        // Stdlib program (read-only, single combined file)
        ProgramFile::stdlib(
            "stdlib",
            "Standard Library",
            "Read-only standard library (types, memory, strings, io, runtime)",
            &stdlib_combined,
        ),
        // OS / kernel source files (read-only, for reference -- not individually compilable)
        {
            let mut p = ProgramFile::os(
                "os-kernel-entry",
                "Entry",
                "Kernel entry point: _kernel_start -> kmain.",
                os_runtime::kernel::RUNTIME,
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
                os_runtime::kernel::CHECKS,
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        {
            let mut p = ProgramFile::os(
                "os-kernel-utilities",
                "Utilities",
                "Kernel platform helpers: kmalloc, kshutdown, timer, PLIC init.",
                os_runtime::kernel::UTILITIES,
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        {
            let mut p = ProgramFile::os(
                "os-kernel-trap-entry",
                "Trap Entry",
                "S-mode trap entry: stvec prologue/epilogue, trap_init, sscratch helpers.",
                os_runtime::kernel::TRAP_ENTRY,
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
                os_runtime::kernel::TRAP_HANDLER,
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        {
            let mut p = ProgramFile::os(
                "os-kernel-pmm",
                "PMM",
                "Physical Memory Manager: pmm_init, pmm_alloc, pmm_free (4 KiB pages).",
                os_runtime::kernel::PMM,
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        {
            let mut p = ProgramFile::os(
                "os-kernel-vmm",
                "VMM",
                "Sv39 Virtual Memory Manager: vmm_init, vmm_enable, vmm_map, vmm_map_range.",
                os_runtime::kernel::VMM,
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        {
            let mut p = ProgramFile::os(
                "os-kernel-process",
                "Process",
                "Process Control Block and lifecycle: process_init, process_create, fork helpers.",
                os_runtime::kernel::PROCESS,
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        {
            let mut p = ProgramFile::os(
                "os-kernel-syscall",
                "Syscall",
                "Syscall dispatch: syscall_dispatch and the sys_* implementations (exit, exec, fork, wait, file I/O).",
                os_runtime::kernel::SYSCALL,
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        {
            let mut p = ProgramFile::os(
                "os-kernel-scheduler",
                "Scheduler",
                "Round-robin scheduler: scheduler_init, scheduler_add, schedule, zombie/reap bookkeeping.",
                os_runtime::kernel::SCHEDULER,
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        {
            let mut p = ProgramFile::os(
                "os-kernel-fs",
                "Filesystem",
                "Inode-based read-write filesystem: fs_open, fs_read, fs_write, fs_create, fs_mkdir, fs_rename, fs_unlink, fs_rmdir.",
                os_runtime::kernel::FS,
            );
            p.standalone = true;
            p.parent_id = Some("os-my-kernel".to_owned());
            p
        },
        // Compilable kernel: select this to build the full OS
        ProgramFile::os(
            "os-my-kernel",
            "My Kernel",
            "Compilable kernel: ties Entry, Checks, Utilities, PMM, VMM, and trap modules together. Select Kernel target mode to run.",
            os_runtime::kernel::MY_KERNEL,
        ),
    ];

    // Userspace programs (read-only) derive from the single os_runtime::user
    // catalog: the hosted tools and demos the shell boots. Compile in Hosted
    // target mode; the shell boots them as pid 1 or via bare-name execution.
    for prog in os_runtime::user::PROGRAMS
        .iter()
        .filter(|p| p.is_compiled())
    {
        let parent_id = format!("user-{}", prog.name);
        let mut primary = ProgramFile::user(&parent_id, prog.title, prog.description, prog.source);
        primary.layout = prog.layout.to_owned();
        programs.push(primary);
        // Each aux translation unit is an editable child module of its program.
        for (aux_name, aux_source) in prog.aux_modules() {
            let mut module = ProgramFile::user(
                &format!("{parent_id}-{aux_name}"),
                &format!("{aux_name}.hll"),
                "Linked translation unit of the parent program.",
                aux_source,
            );
            module.parent_id = Some(parent_id.clone());
            programs.push(module);
        }
    }

    programs.extend([
        // Example programs: one per feature family, each printing labeled output
        // and returning exit 0 only when its self-checks pass.
        ProgramFile::example(
            "example-core-basics",
            "core_basics.hll",
            "Typed and inferred declarations, const evaluation, functions, and loops.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/core_basics.hll"
            )),
        ),
        ProgramFile::example(
            "example-operators",
            "operators.hll",
            "Arithmetic, logical, bitwise, shift, cast precedence, and compound assignment.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/operators.hll"
            )),
        ),
        ProgramFile::example(
            "example-pointers-and-places",
            "pointers_and_places.hll",
            "Address-of, @ deref, member auto-deref, element-scaled pointer math, casts, defer.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/pointers_and_places.hll"
            )),
        ),
        ProgramFile::example(
            "example-arrays-slices-and-ranges",
            "arrays_slices_and_ranges.hll",
            "Array literals, zero fill, slices, .len, ranges, and `for` over each.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/arrays_slices_and_ranges.hll"
            )),
        ),
        ProgramFile::example(
            "example-structs-and-binding",
            "structs_and_binding.hll",
            "Named and contextual struct literals, aggregate calls/returns, and destructuring.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/structs_and_binding.hll"
            )),
        ),
        ProgramFile::example(
            "example-generics",
            "generics.hll",
            "Explicit and inferred generic functions, generic structs, and recursive records.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/generics.hll"
            )),
        ),
        ProgramFile::example(
            "example-hll2-completion-showcase",
            "hll2_completion_showcase.hll",
            "Living HLL2 feature showcase, starting with function pointer aliases, values, calls, and arrays.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/hll2_completion_showcase.hll"
            )),
        ),
        ProgramFile::example(
            "example-enums-match-and-result",
            "enums_match_and_result.hll",
            "Enums, statement and value match, a generic enum, Option, Result, and `?`.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/enums_match_and_result.hll"
            )),
        ),
        ProgramFile::example(
            "example-strings-and-iteration",
            "strings_and_iteration.hll",
            "Strings as u8[] slices: indexing, range slicing, byte iteration, and char literals.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/strings_and_iteration.hll"
            )),
        ),
        ProgramFile::example(
            "example-compile-time-math",
            "compile_time_math.hll",
            "Pure recursive functions folded into constants before runtime.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/compile_time_math.hll"
            )),
        ),
    ]);

    programs
}

pub fn blank_custom_program_source() -> String {
    ["main: () -> i32 {", "    return 0", "}", ""].join("\n")
}

#[derive(Clone, serde::Deserialize, serde::Serialize, Debug)]
pub struct ProgramCatalog {
    programs: Vec<ProgramFile>,
    pub selected_program_id: String,
    pub next_custom_program_id: u32,
}

impl Default for ProgramCatalog {
    fn default() -> Self {
        let programs = built_in_programs();
        let selected_program_id = programs
            .first()
            .map(|program| program.id.clone())
            .unwrap_or_default();

        Self {
            programs,
            selected_program_id,
            next_custom_program_id: 1,
        }
    }
}

impl ProgramCatalog {
    pub fn ensure_consistency(&mut self) {
        let mut merged_programs = Vec::with_capacity(self.programs.len().max(1) + 8);

        for built_in in built_in_programs() {
            if let Some(existing) = self
                .programs
                .iter()
                .find(|program| program.id == built_in.id)
            {
                let mut updated = existing.clone();
                if built_in.kind == ProgramKind::Custom {
                    // Custom programs: preserve everything - user-managed
                } else {
                    // Stdlib, Example, Os: always refresh from embedded (read-only)
                    updated.name = built_in.name;
                    updated.kind = built_in.kind;
                    updated.source = built_in.source;
                    updated.description = built_in.description;
                    updated.standalone = built_in.standalone;
                    updated.parent_id = built_in.parent_id;
                    updated.layout = built_in.layout;
                }
                merged_programs.push(updated);
            } else {
                merged_programs.push(built_in);
            }
        }

        merged_programs.extend(
            self.programs
                .iter()
                .filter(|program| program.is_custom())
                .cloned(),
        );

        self.programs = merged_programs;

        if self.next_custom_program_id == 0 {
            self.next_custom_program_id = 1;
        }

        if self.selected_program_id.is_empty()
            || !self
                .programs
                .iter()
                .any(|program| program.id == self.selected_program_id)
        {
            self.selected_program_id = self
                .programs
                .first()
                .map(|program| program.id.clone())
                .unwrap_or_default();
        }
    }

    pub fn current_program_index(&self) -> Option<usize> {
        self.programs
            .iter()
            .position(|program| program.id == self.selected_program_id)
    }

    pub fn current_program(&self) -> Option<&ProgramFile> {
        self.current_program_index()
            .map(|index| &self.programs[index])
    }

    pub fn current_program_mut(&mut self) -> Option<&mut ProgramFile> {
        let index = self.current_program_index()?;
        self.programs.get_mut(index)
    }

    pub fn get_selected_source(&self) -> String {
        self.current_program()
            .map(|program| program.source.clone())
            .unwrap_or_default()
    }

    pub fn set_selected_source(&mut self, source: String) {
        if let Some(program) = self.current_program_mut() {
            program.source = source;
            program.undo_stack.clear();
            program.redo_stack.clear();
        }
    }

    pub fn replace_selected_source_with_history(&mut self, source: String) {
        if let Some(program) = self.current_program_mut()
            && program.source != source
        {
            program.undo_stack.push(program.source.clone());
            program.redo_stack.clear();
            program.source = source;
        }
    }

    pub fn undo_selected_source(&mut self) -> bool {
        let Some(program) = self.current_program_mut() else {
            return false;
        };

        let Some(previous) = program.undo_stack.pop() else {
            return false;
        };

        program.redo_stack.push(program.source.clone());
        program.source = previous;
        true
    }

    pub fn can_undo_selected_source(&self) -> bool {
        self.current_program()
            .map(|program| !program.undo_stack.is_empty())
            .unwrap_or(false)
    }

    pub fn can_redo_selected_source(&self) -> bool {
        self.current_program()
            .map(|program| !program.redo_stack.is_empty())
            .unwrap_or(false)
    }

    pub fn redo_selected_source(&mut self) -> bool {
        let Some(program) = self.current_program_mut() else {
            return false;
        };

        let Some(next) = program.redo_stack.pop() else {
            return false;
        };

        program.undo_stack.push(program.source.clone());
        program.source = next;
        true
    }

    pub fn select_program(&mut self, program_id: &str) {
        if self.selected_program_id == program_id {
            return;
        }
        self.selected_program_id = program_id.to_owned();
    }

    pub fn create_custom_program(&mut self, source: String, name: String) {
        let program_id = format!("custom-{}", self.next_custom_program_id);
        self.next_custom_program_id = self
            .next_custom_program_id
            .checked_add(1)
            .unwrap_or(self.next_custom_program_id);

        self.programs
            .push(ProgramFile::custom(program_id.clone(), name, source));
        self.selected_program_id = program_id;
    }

    pub fn create_blank_program(&mut self) {
        let name = format!("Untitled {}", self.next_custom_program_id);
        self.create_custom_program(blank_custom_program_source(), name);
    }

    pub fn duplicate_current_program(&mut self) {
        let duplicate_name = self
            .current_program()
            .map(|program| format!("Copy of {}", program.name))
            .unwrap_or_else(|| String::from("Copy of current file"));

        let source = self.get_selected_source();
        self.create_custom_program(source, duplicate_name);
    }

    pub fn delete_current_custom_program(&mut self) {
        let Some(current) = self.current_program().cloned() else {
            return;
        };

        if !current.is_custom() {
            return;
        }

        self.programs.retain(|program| program.id != current.id);

        if let Some(next_program) = self.programs.first() {
            self.selected_program_id = next_program.id.clone();
        } else {
            self.selected_program_id.clear();
        }
    }

    pub fn all_programs(&self) -> &[ProgramFile] {
        &self.programs
    }

    pub fn get_programs_by_kind(&self, kind: ProgramKind) -> Vec<&ProgramFile> {
        self.programs.iter().filter(|p| p.kind == kind).collect()
    }

    /// Catalog entries whose `parent_id` is `parent_id`, in catalog order.
    pub fn children_of(&self, parent_id: &str) -> Vec<&ProgramFile> {
        self.programs
            .iter()
            .filter(|p| p.parent_id.as_deref() == Some(parent_id))
            .collect()
    }

    /// The (possibly edited) source of each aux module of `parent_id`, in order.
    /// Drives the link path so edits to aux units take effect on compile.
    pub fn child_sources(&self, parent_id: &str) -> Vec<String> {
        self.children_of(parent_id)
            .iter()
            .map(|p| p.source.clone())
            .collect()
    }

    /// The shared layout header for the program `id` (prepended to its primary and
    /// aux units at compile), or `""` if it has none.
    pub fn layout_of(&self, id: &str) -> &str {
        self.programs
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.layout.as_str())
            .unwrap_or("")
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
    fn aux_modules_nest_under_their_program() {
        let programs = built_in_programs();
        let aux = find(&programs, "user-as-as_object");
        assert_eq!(aux.parent_id.as_deref(), Some("user-as"));
        assert_eq!(aux.badge(), CatalogBadge::Fragment);
        // The primary program is runnable; the linker pulls the aux from the catalog.
        assert_eq!(find(&programs, "user-as").badge(), CatalogBadge::Runnable);
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
    fn child_sources_returns_aux_units() {
        let catalog = ProgramCatalog::default();
        let sources = catalog.child_sources("user-cc");
        assert_eq!(sources.len(), 1, "cc has one aux translation unit");
        assert!(!sources[0].is_empty());
    }

    // A persisted catalog from before the `layout` field defaults it to empty;
    // ensure_consistency must restore it for read-only split tools (cc/as).
    #[test]
    fn ensure_consistency_restores_layout_for_read_only_programs() {
        let mut catalog = ProgramCatalog::default();
        for program in &mut catalog.programs {
            program.layout.clear();
        }
        catalog.ensure_consistency();
        assert_eq!(catalog.layout_of("user-cc"), os_runtime::user::CC_LAYOUT);
        assert_eq!(catalog.layout_of("user-as"), os_runtime::user::AS_LAYOUT);
    }
}
