// Program file management and catalog

use hll_to_ir::stdlib::get_stdlib_source;

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Debug)]
pub enum ProgramKind {
    Example,
    Custom,
    Stdlib, // Read-only stdlib programs
    Os,     // Read-only OS / kernel programs
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
}

fn built_in_programs() -> Vec<ProgramFile> {
    // Keep the catalog's "Standard Library" source in lock-step with the compiler pipeline.
    let stdlib_combined = get_stdlib_source();

    vec![
        // Stdlib program (read-only, single combined file)
        ProgramFile::stdlib(
            "stdlib",
            "Standard Library",
            "Read-only standard library (types, memory, strings, io, runtime)",
            &stdlib_combined,
        ),
        // OS / kernel programs (read-only)
        {
                let mut p = ProgramFile::os(
                "os-kernel-runtime",
                "Kernel Runtime",
                "Read-only kernel boot runtime: _kernel_start, kmalloc, kshutdown.",
                concat!(
                    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/crates/os-runtime/stdlib/kernel/utilities.hll")),
                    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/crates/os-runtime/kernel/entry.hll")),
                ),
            );
            p.standalone = true;
            p
        },
        ProgramFile::os(
            "os-my-kernel",
            "My Kernel",
            "Minimal kernel: boot log, heap smoke-test, and shutdown. Select Kernel target mode to run.",
            os_runtime::kernel::MY_KERNEL,
        ),
        // Example programs
        ProgramFile::example(
            "example-core-basics",
            "core_basics.hll",
            "A compact starter program with constants, a helper, and one branch.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/core_basics.hll"
            )),
        ),
        ProgramFile::example(
            "example-pointer-arrays",
            "pointer_arrays.hll",
            "Address-of, dereference, and heap cleanup with distinct pointer math.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/pointer_arrays.hll"
            )),
        ),
        ProgramFile::example(
            "example-array-initialization",
            "array_initialization.hll",
            "Stack arrays mirrored into heap storage and read back through indexing.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/array_initialization.hll"
            )),
        ),
        ProgramFile::example(
            "example-struct-binding",
            "struct_binding.hll",
            "Named structs, reordered fields, and partial destructuring.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/struct_binding.hll"
            )),
        ),
        ProgramFile::example(
            "example-control-flow-basics",
            "control_flow_basics.hll",
            "Loops, continue, and defer-based cleanup around a reusable helper.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/control_flow_basics.hll"
            )),
        ),
        ProgramFile::example(
            "example-casting-and-pointers",
            "casting_and_pointers.hll",
            "Explicit type casts, pointer reinterpretation, and formatted output.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/casting_and_pointers.hll"
            )),
        ),
        ProgramFile::example(
            "example-compile-time-math",
            "compile_time_math.hll",
            "Pure functions folded into constants before runtime.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/compile_time_math.hll"
            )),
        ),
        ProgramFile::example(
            "example-generics-and-strings",
            "generics_and_strings.hll",
            "A larger demo mixing generics, heap values, strings, and output.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/generics_and_strings.hll"
            )),
        ),
    ]
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
}
