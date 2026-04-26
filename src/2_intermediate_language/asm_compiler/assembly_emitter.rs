use super::data_section::DataSection;
use crate::assembly_language::real::RealInstruction;

pub struct AssemblyEmitter {
    lines: Vec<String>,
    current_section: Option<String>,
}

impl AssemblyEmitter {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            current_section: None,
        }
    }

    pub fn reset(&mut self) {
        self.lines.clear();
        self.current_section = None;
    }

    pub fn switch_section(&mut self, name: &str) {
        if self.current_section.as_deref() != Some(name) {
            self.current_section = Some(name.to_string());
            self.lines.push(format!(".section {}", name));
        }
    }

    pub fn emit_raw(&mut self, line: &str) {
        self.lines.push(line.to_string());
    }

    pub fn emit_data_section(&mut self, data: &DataSection) {
        data.emit(self);
    }

    pub fn emit_text_section(&mut self) {
        self.switch_section(".text");
    }

    pub fn emit_functions(&mut self) {
        // Already emitted via start_function; nothing extra needed.
    }

    pub fn start_function(&mut self, name: &str) {
        self.switch_section(".text");
        self.lines.push(format!(".globl {}", name));
        self.lines.push(format!("{}:", name));
    }

    pub fn end_function(&mut self) {}

    pub fn emit_label(&mut self, label: &str) {
        self.lines.push(format!("{}:", label));
    }

    pub fn emit_inst(&mut self, inst: RealInstruction) {
        self.lines.push(format!("\t{}", inst.to_asm()));
    }

    pub fn emit_comment(&mut self, text: &str) {
        self.lines.push(format!("\t; {}", text));
    }

    pub fn finish(&mut self) -> String {
        self.lines.join("\n")
    }
}

impl Default for AssemblyEmitter {
    fn default() -> Self {
        Self::new()
    }
}