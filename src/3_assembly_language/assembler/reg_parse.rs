/// Inverse of `utils::reg_name` — map an ABI register name back to its number (0-31).

use crate::assembly_language::encode_decode::Reg;

/// Parse an ABI integer register name (e.g. `"a0"`, `"x10"`, `"zero"`) to a
/// hardware register number in `0..=31`.  Returns `None` on unrecognised names.
pub fn parse_int_reg(name: &str) -> Option<Reg> {
    match name.trim() {
        "zero" | "x0" => Some(0),
        "ra" | "x1" => Some(1),
        "sp" | "x2" => Some(2),
        "gp" | "x3" => Some(3),
        "tp" | "x4" => Some(4),
        "t0" | "x5" => Some(5),
        "t1" | "x6" => Some(6),
        "t2" | "x7" => Some(7),
        "s0" | "fp" | "x8" => Some(8),
        "s1" | "x9" => Some(9),
        "a0" | "x10" => Some(10),
        "a1" | "x11" => Some(11),
        "a2" | "x12" => Some(12),
        "a3" | "x13" => Some(13),
        "a4" | "x14" => Some(14),
        "a5" | "x15" => Some(15),
        "a6" | "x16" => Some(16),
        "a7" | "x17" => Some(17),
        "s2" | "x18" => Some(18),
        "s3" | "x19" => Some(19),
        "s4" | "x20" => Some(20),
        "s5" | "x21" => Some(21),
        "s6" | "x22" => Some(22),
        "s7" | "x23" => Some(23),
        "s8" | "x24" => Some(24),
        "s9" | "x25" => Some(25),
        "s10" | "x26" => Some(26),
        "s11" | "x27" => Some(27),
        "t3" | "x28" => Some(28),
        "t4" | "x29" => Some(29),
        "t5" | "x30" => Some(30),
        "t6" | "x31" => Some(31),
        _ => None,
    }
}

/// Parse an ABI float register name (e.g. `"fa0"`, `"ft0"`, `"f10"`) to a
/// hardware float register number in `0..=31`.
pub fn parse_float_reg(name: &str) -> Option<Reg> {
    match name.trim() {
        "ft0" | "f0" => Some(0),
        "ft1" | "f1" => Some(1),
        "ft2" | "f2" => Some(2),
        "ft3" | "f3" => Some(3),
        "ft4" | "f4" => Some(4),
        "ft5" | "f5" => Some(5),
        "ft6" | "f6" => Some(6),
        "ft7" | "f7" => Some(7),
        "fs0" | "f8" => Some(8),
        "fs1" | "f9" => Some(9),
        "fa0" | "f10" => Some(10),
        "fa1" | "f11" => Some(11),
        "fa2" | "f12" => Some(12),
        "fa3" | "f13" => Some(13),
        "fa4" | "f14" => Some(14),
        "fa5" | "f15" => Some(15),
        "fa6" | "f16" => Some(16),
        "fa7" | "f17" => Some(17),
        "fs2" | "f18" => Some(18),
        "fs3" | "f19" => Some(19),
        "fs4" | "f20" => Some(20),
        "fs5" | "f21" => Some(21),
        "fs6" | "f22" => Some(22),
        "fs7" | "f23" => Some(23),
        "fs8" | "f24" => Some(24),
        "fs9" | "f25" => Some(25),
        "fs10" | "f26" => Some(26),
        "fs11" | "f27" => Some(27),
        "ft8" | "f28" => Some(28),
        "ft9" | "f29" => Some(29),
        "ft10" | "f30" => Some(30),
        "ft11" | "f31" => Some(31),
        _ => None,
    }
}
