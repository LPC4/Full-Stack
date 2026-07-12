//! New IDE UI layer (egui). Visuals only; state comes from the `full_stack` backend
//! (`IdeWorkspace`, `SessionEngine`, inspectors). Signature element: the Stack Spine.

pub mod spine;
pub mod theme;

use egui::Color32;

/// One rung of the compilation stack, from human source (L0) to linked ELF (L3).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Hll,
    Ir,
    Asm,
    Elf,
}

impl Layer {
    /// Short rung tag shown on the spine.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Hll => "L0",
            Self::Ir => "L1",
            Self::Asm => "L2",
            Self::Elf => "L3",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Hll => "HLL",
            Self::Ir => "IR",
            Self::Asm => "ASM",
            Self::Elf => "ELF",
        }
    }

    /// The depth-spectrum color: warm source at L0 through cool linked output at L3.
    pub fn color(self) -> Color32 {
        match self {
            Self::Hll => Color32::from_rgb(0xF2, 0xA6, 0x5A),
            Self::Ir => Color32::from_rgb(0xC9, 0x9A, 0x77),
            Self::Asm => Color32::from_rgb(0x8F, 0xA0, 0xB3),
            Self::Elf => Color32::from_rgb(0x5E, 0x86, 0xC9),
        }
    }
}
