use crate::encode_decode::Reg;

pub const INTEGER_REGISTER_NAMES: [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4",
    "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4",
    "t5", "t6",
];

pub const FLOAT_REGISTER_NAMES: [&str; 32] = [
    "ft0", "ft1", "ft2", "ft3", "ft4", "ft5", "ft6", "ft7", "fs0", "fs1", "fa0", "fa1", "fa2",
    "fa3", "fa4", "fa5", "fa6", "fa7", "fs2", "fs3", "fs4", "fs5", "fs6", "fs7", "fs8", "fs9",
    "fs10", "fs11", "ft8", "ft9", "ft10", "ft11",
];

pub fn integer_register_name(register: usize) -> Option<&'static str> {
    INTEGER_REGISTER_NAMES.get(register).copied()
}

pub fn float_register_name(register: usize) -> Option<&'static str> {
    FLOAT_REGISTER_NAMES.get(register).copied()
}

pub fn register_name(register: Reg, float: bool) -> String {
    let name = if float {
        float_register_name(register.into())
    } else {
        integer_register_name(register.into())
    };
    name.map_or_else(
        || {
            if float {
                format!("f{register}")
            } else {
                format!("x{register}")
            }
        },
        str::to_owned,
    )
}

pub fn parse_integer_register(name: &str) -> Option<Reg> {
    let name = name.trim();
    if name == "fp" {
        return Some(8);
    }
    let register = parse_numbered_register(name, 'x').or_else(|| {
        INTEGER_REGISTER_NAMES
            .iter()
            .position(|candidate| *candidate == name)
    })?;
    register.try_into().ok()
}

pub fn parse_float_register(name: &str) -> Option<Reg> {
    let name = name.trim();
    let register = parse_numbered_register(name, 'f').or_else(|| {
        FLOAT_REGISTER_NAMES
            .iter()
            .position(|candidate| *candidate == name)
    })?;
    register.try_into().ok()
}

pub fn is_register_name(name: &str) -> bool {
    parse_integer_register(name).is_some() || parse_float_register(name).is_some()
}

fn parse_numbered_register(name: &str, prefix: char) -> Option<usize> {
    let number = name.strip_prefix(prefix)?.parse::<usize>().ok()?;
    (number < 32).then_some(number)
}

pub fn pcrel_split(offset: i64) -> (i32, i32) {
    let low = ((offset & 0xfff) as i32).wrapping_sub(if offset & 0x800 == 0 { 0 } else { 0x1000 });
    let high = ((offset - i64::from(low)) >> 12) as i32;
    (high, low)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_names_round_trip() {
        for register in 0..32 {
            assert_eq!(
                parse_integer_register(integer_register_name(register).expect("integer name")),
                Some(register as Reg)
            );
            assert_eq!(
                parse_float_register(float_register_name(register).expect("float name")),
                Some(register as Reg)
            );
            assert_eq!(
                parse_integer_register(&format!("x{register}")),
                Some(register as Reg)
            );
            assert_eq!(
                parse_float_register(&format!("f{register}")),
                Some(register as Reg)
            );
        }
        assert_eq!(parse_integer_register("x32"), None);
        assert_eq!(parse_float_register("f32"), None);
    }

    #[test]
    fn pc_relative_split_handles_rounding_boundaries() {
        for offset in [-4097, -4096, -2049, -2048, -1, 0, 2047, 2048, 4095, 4096] {
            let (high, low) = pcrel_split(offset);
            assert!((-2048..=2047).contains(&low));
            assert_eq!((i64::from(high) << 12) + i64::from(low), offset);
        }
    }
}
