// Program file management and catalog

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Debug)]
pub enum ProgramKind {
    Example,
    Custom,
    Stdlib, // Read-only stdlib programs
}

#[derive(Clone, serde::Deserialize, serde::Serialize, Debug)]
pub struct ProgramFile {
    pub id: String,
    pub name: String,
    pub kind: ProgramKind,
    pub source: String,
    #[serde(default)]
    pub undo_stack: Vec<String>,
    #[serde(default)]
    pub redo_stack: Vec<String>,
    #[serde(skip)]
    pub description: String,
}

impl ProgramFile {
    pub fn example(id: &str, name: &str, description: &str, source: &str) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            kind: ProgramKind::Example,
            source: source.to_owned(),
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
}

fn built_in_programs() -> Vec<ProgramFile> {
    // Load stdlib sources and combine into one file
    let std_types = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/programs/stdlib/types.hll"
    ));
    let std_memory = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/programs/stdlib/memory_allocator.hll"
    ));
    let std_strings = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/programs/stdlib/string_utils.hll"
    ));

    // Combine all stdlib into one source
    let stdlib_combined = format!("{}\n\n{}\n\n{}", std_types, std_memory, std_strings);

    vec![
        // Stdlib program (read-only, single combined file)
        ProgramFile::stdlib(
            "stdlib",
            "stdlib.hll",
            "Standard library (types, memory allocation, strings)",
            &stdlib_combined,
        ),
        // Example programs
        ProgramFile::example(
            "example-core-syntax",
            "Core Syntax",
            "Basic declarations, constants, functions, and primitive values.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/core_syntax.hll"
            )),
        ),
        ProgramFile::example(
            "example-pointers-arrays",
            "Pointers & Arrays",
            "Address-of, dereference, array indexing, and heap cleanup.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/pointers_arrays.hll"
            )),
        ),
        ProgramFile::example(
            "example-array-literals",
            "Array Literals",
            "Fixed-size array literals, element access, and IR/ASM array copies.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/array_literals.hll"
            )),
        ),
        ProgramFile::example(
            "example-structs-destructuring",
            "Structs & Destructuring",
            "Named structs, shorthand literals, and reordered/partial destructuring.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/structs_destructuring.hll"
            )),
        ),
        ProgramFile::example(
            "example-control-flow-functions",
            "Control Flow & Functions",
            "Function calls, loops, branching, and defer-based cleanup.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/control_flow_functions.hll"
            )),
        ),
        ProgramFile::example(
            "example-casts-and-pointers",
            "Casts & Pointers",
            "Explicit type casts, calling printf, and pointer casts.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/casts_and_pointers.hll"
            )),
        ),
        ProgramFile::example(
            "example-constexpr-functions",
            "Constexpr Functions",
            "Pure functions evaluated at compile time to produce constants.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/constexpr_functions.hll"
            )),
        ),
        ProgramFile::example(
            "example-generics-strings",
            "Generics & Strings",
            "Generic Box<T>, string pointers, and external puts from C.",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/programs/example/generics_strings.hll"
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
                // For stdlib and examples, always update from embedded source (read-only)
                if built_in.kind != ProgramKind::Custom {
                    updated.name = built_in.name;
                    updated.kind = built_in.kind;
                    updated.source = built_in.source;
                    updated.description = built_in.description;
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
        if let Some(program) = self.current_program_mut() {
            if program.source != source {
                program.undo_stack.push(program.source.clone());
                program.redo_stack.clear();
                program.source = source;
            }
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
