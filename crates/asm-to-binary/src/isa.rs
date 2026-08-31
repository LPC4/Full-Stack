use crate::encode_decode::Reg;
pub use riscv_model::{
    FLOAT_REGISTER_NAMES, INTEGER_REGISTER_NAMES, float_register_name, integer_register_name,
    is_register_name, pcrel_split,
};

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
    riscv_model::IntReg::parse(name).map(Into::into)
}

pub fn parse_float_register(name: &str) -> Option<Reg> {
    riscv_model::FpReg::parse(name).map(Into::into)
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
}
