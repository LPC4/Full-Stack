/// Identifies a standard ELF section or a custom one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SectionKind {
    Text,
    Data,
    RoData,
    Bss,
    Custom(String),
}

impl SectionKind {
    pub fn from_str(s: &str) -> Self {
        match s.trim() {
            ".text" => Self::Text,
            ".data" => Self::Data,
            ".rodata" => Self::RoData,
            ".bss" => Self::Bss,
            other => Self::Custom(other.to_owned()),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Text => ".text",
            Self::Data => ".data",
            Self::RoData => ".rodata",
            Self::Bss => ".bss",
            Self::Custom(s) => s.as_str(),
        }
    }

    /// True for sections that hold executable machine code.
    pub fn is_executable(&self) -> bool {
        matches!(self, Self::Text)
    }
}

/// Accumulated bytes and local symbol table for one section.
#[derive(Debug, Default, Clone)]
pub struct SectionData {
    pub kind: Option<SectionKind>,
    /// Raw bytes emitted so far (instructions + data directives).
    pub bytes: Vec<u8>,
    /// Labels defined within this section: (name, byte-offset within section).
    pub symbols: Vec<(String, u64)>,
    /// Names exported via `.globl` (may be defined here or in another section).
    pub globals: Vec<String>,
}

impl SectionData {
    pub fn new(kind: SectionKind) -> Self {
        Self {
            kind: Some(kind),
            bytes: Vec::new(),
            symbols: Vec::new(),
            globals: Vec::new(),
        }
    }

    pub fn current_offset(&self) -> u64 {
        self.bytes.len() as u64
    }

    pub fn push_u8(&mut self, byte: u8) {
        self.bytes.push(byte);
    }

    pub fn push_u32_le(&mut self, word: u32) {
        self.bytes.extend_from_slice(&word.to_le_bytes());
    }

    pub fn push_u64_le(&mut self, word: u64) {
        self.bytes.extend_from_slice(&word.to_le_bytes());
    }

    /// Align the section to `alignment` bytes by zero-padding.
    pub fn align_to(&mut self, alignment: usize) {
        if alignment <= 1 {
            return;
        }
        let rem = self.bytes.len() % alignment;
        if rem != 0 {
            let padding = alignment - rem;
            self.bytes.extend(std::iter::repeat(0u8).take(padding));
        }
    }

    pub fn define_label(&mut self, name: String) {
        self.symbols.push((name, self.current_offset()));
    }

    pub fn export_global(&mut self, name: String) {
        if !self.globals.contains(&name) {
            self.globals.push(name);
        }
    }
}
