use hll_to_ir::{IrGlobalString, IrTypeAlias};

pub struct DataSection {
    rodata: Vec<String>,
    data: Vec<String>,
    bss: Vec<String>,
}

impl DataSection {
    pub fn new() -> Self {
        Self {
            rodata: Vec::new(),
            data: Vec::new(),
            bss: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.rodata.clear();
        self.data.clear();
        self.bss.clear();
    }

    pub fn add_global_string(&mut self, s: &IrGlobalString) {
        let escaped = s
            .content
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\t', "\\t")
            .replace('\r', "\\r");
        self.rodata.push(format!("{}:", s.name));
        self.rodata.push(format!("\t.asciz \"{escaped}\""));
        self.rodata.push("\t.align 1".to_owned());
    }

    /// No-op; type aliases are not emitted to assembly.
    pub fn add_type_alias(&mut self, _alias: &IrTypeAlias) {}

    pub fn add_bss_symbol(&mut self, name: &str, size: usize, align: usize) {
        self.bss.push(format!(".globl {name}"));
        self.bss.push(format!(".balign {align}"));
        self.bss.push(format!("{name}:"));
        self.bss.push(format!("\t.space {size}"));
    }

    pub fn add_data_symbol(&mut self, name: &str, _size: usize, align: usize, init: &[u8]) {
        self.data.push(format!(".globl {name}"));
        self.data.push(format!(".balign {align}"));
        self.data.push(format!("{name}:"));
        let mut bytes = String::new();
        for (i, b) in init.iter().enumerate() {
            if i % 8 == 0 {
                if !bytes.is_empty() {
                    self.data.push(bytes);
                    bytes = String::new();
                }
                bytes.push_str("\t.byte ");
            }
            bytes.push_str(&format!("{b:#04x}"));
            if i != init.len() - 1 {
                bytes.push_str(", ");
            }
        }
        if !bytes.is_empty() {
            self.data.push(bytes);
        }
    }

    pub fn emit(&self, emitter: &mut super::assembly_emitter::AssemblyEmitter) {
        if !self.rodata.is_empty() {
            emitter.switch_section(".rodata");
            for line in &self.rodata {
                emitter.emit_raw(line);
            }
        }
        if !self.data.is_empty() {
            emitter.switch_section(".data");
            for line in &self.data {
                emitter.emit_raw(line);
            }
        }
        if !self.bss.is_empty() {
            emitter.switch_section(".bss");
            for line in &self.bss {
                emitter.emit_raw(line);
            }
        }
    }
}
