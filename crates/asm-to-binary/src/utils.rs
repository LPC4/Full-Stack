use crate::encode_decode::Reg;

pub(crate) fn reg_name(register: Reg, float: bool) -> String {
    let name = if float {
        riscv_model::float_register_name(register.into())
    } else {
        riscv_model::integer_register_name(register.into())
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
