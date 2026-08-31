pub(crate) fn parse_int_reg(name: &str) -> Option<u8> {
    riscv_model::IntReg::parse(name).map(Into::into)
}

#[cfg(test)]
pub(crate) fn parse_float_reg(name: &str) -> Option<u8> {
    riscv_model::FpReg::parse(name).map(Into::into)
}
