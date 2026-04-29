use super::{data_section::DataSection, function_context::Rv64Backend};
use crate::assembly_language::encode_decode::Reg;
use crate::assembly_language::real::RealInstruction;
use crate::assembly_language::riscv::rv64fd::{
    Fadd, Fdiv, FeqS, Fld, FleqS, FltS, Flw, Fmul, FmvWX, Fsd, Fsub, Fsw, fmv_s,
};
use crate::assembly_language::riscv::rv64i::{
    Add, Addi, Addiw, And, Jalr, Lb, Ld, Lh, Lui, Lw, Or, Sb, Sd, Sh, Sll, Slli, Slt, Sltiu, Sltu,
    Srai, Srl, Sub, Sw, Xor, Xori,
};
use crate::assembly_language::riscv::rv64m::{Div, Mul, Rem};
use crate::assembly_language::utils::reg_name;
use crate::intermediate_language::IrType;

const ZERO: Reg = 0;
const T0: Reg = 5;
const T1: Reg = 6;
const T2: Reg = 7;
const T3: Reg = 28;
const T4: Reg = 29;
const T5: Reg = 30;
const T6: Reg = 31;

pub struct AssemblyEmitter {
    lines: Vec<String>,
    current_section: Option<String>,
    temp_counter: usize,
    float_temp_counter: usize,
}

impl AssemblyEmitter {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            current_section: None,
            temp_counter: 0,
            float_temp_counter: 0,
        }
    }

    pub fn reset(&mut self) {
        self.lines.clear();
        self.current_section = None;
        self.temp_counter = 0;
        self.float_temp_counter = 0;
    }

    pub fn reset_temp_counter(&mut self) {
        self.temp_counter = 0;
    }

    // ---------- section / label / comment utilities ----------
    pub fn switch_section(&mut self, name: &str) {
        if self.current_section.as_deref() != Some(name) {
            self.current_section = Some(name.to_owned());
            self.lines.push(format!(".section {name}"));
        }
    }

    pub fn emit_raw(&mut self, line: &str) {
        self.lines.push(line.to_owned());
    }

    pub fn emit_data_section(&mut self, data: &DataSection) {
        data.emit(self);
    }

    pub fn emit_text_section(&mut self) {
        self.switch_section(".text");
    }

    pub fn start_function(&mut self, name: &str) {
        self.switch_section(".text");
        self.lines
            .push(format!("\t; ========================================"));
        self.lines.push(format!("\t; Function: {name}"));
        self.lines
            .push(format!("\t; ========================================"));
        self.lines.push(format!(".globl {name}"));
        self.lines.push(format!("{name}:"));
    }

    pub fn end_function(&mut self) {
        self.lines.push(format!("\t; End of function"));
        self.lines.push(format!(""));
    }

    pub fn emit_label(&mut self, label: &str) {
        if label.contains("__") {
            let parts: Vec<&str> = label.splitn(2, "__").collect();
            if parts.len() == 2 {
                self.lines
                    .push(format!("\t; --- Basic Block: {} ---", parts[1]));
            }
        }
        self.lines.push(format!("{label}:"));
    }

    pub fn emit_inst(&mut self, inst: RealInstruction) {
        self.lines.push(format!("\t{}", inst.to_asm()));
    }

    pub fn emit_comment(&mut self, text: &str) {
        self.lines.push(format!("\t; {text}"));
    }

    // ---------- register allocation helpers ----------
    pub fn alloc_temp_reg(&mut self) -> Reg {
        let temps = [T0, T1, T2, T3, T4, T5, T6];
        let reg = temps[self.temp_counter % temps.len()];
        self.temp_counter += 1;
        reg
    }

    pub fn alloc_float_temp_reg(&mut self) -> Reg {
        // Cycle through ft0-ft7 (regs 0-7)
        let reg = self.float_temp_counter as Reg % 8; // ft0..ft7 are 0..7
        self.float_temp_counter += 1;
        reg
    }

    // ---------- base integer instructions ----------
    pub fn emit_addi(&mut self, rd: Reg, rs1: Reg, imm: i32) {
        self.emit_inst(RealInstruction::Addi(Addi::new(rd, rs1, imm)));
    }
    pub fn emit_sd(&mut self, base: Reg, src: Reg, offset: i32) {
        self.emit_inst(RealInstruction::Sd(Sd::new(base, src, offset)));
    }
    pub fn emit_ld(&mut self, rd: Reg, base: Reg, offset: i32) {
        self.emit_inst(RealInstruction::Ld(Ld::new(rd, base, offset)));
    }
    pub fn emit_lw(&mut self, rd: Reg, base: Reg, offset: i32) {
        self.emit_inst(RealInstruction::Lw(Lw::new(rd, base, offset)));
    }
    pub fn emit_lh(&mut self, rd: Reg, base: Reg, offset: i32) {
        self.emit_inst(RealInstruction::Lh(Lh::new(rd, base, offset)));
    }
    pub fn emit_lb(&mut self, rd: Reg, base: Reg, offset: i32) {
        self.emit_inst(RealInstruction::Lb(Lb::new(rd, base, offset)));
    }
    pub fn emit_add(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emit_inst(RealInstruction::Add(Add::new(rd, rs1, rs2)));
    }
    pub fn emit_sub(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emit_inst(RealInstruction::Sub(Sub::new(rd, rs1, rs2)));
    }
    pub fn emit_mul(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emit_inst(RealInstruction::Mul(Mul::new(rd, rs1, rs2)));
    }
    pub fn emit_div(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emit_inst(RealInstruction::Div(Div::new(rd, rs1, rs2)));
    }
    pub fn emit_rem(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emit_inst(RealInstruction::Rem(Rem::new(rd, rs1, rs2)));
    }
    pub fn emit_and(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emit_inst(RealInstruction::And(And::new(rd, rs1, rs2)));
    }
    pub fn emit_or(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emit_inst(RealInstruction::Or(Or::new(rd, rs1, rs2)));
    }
    pub fn emit_xor(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emit_inst(RealInstruction::Xor(Xor::new(rd, rs1, rs2)));
    }
    pub fn emit_xori(&mut self, rd: Reg, rs1: Reg, imm: i32) {
        self.emit_inst(RealInstruction::Xori(Xori::new(rd, rs1, imm)));
    }
    pub fn emit_sltiu(&mut self, rd: Reg, rs1: Reg, imm: i32) {
        self.emit_inst(RealInstruction::Sltiu(Sltiu::new(rd, rs1, imm)));
    }
    pub fn emit_sltu(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emit_inst(RealInstruction::Sltu(Sltu::new(rd, rs1, rs2)));
    }
    pub fn emit_slt(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emit_inst(RealInstruction::Slt(Slt::new(rd, rs1, rs2)));
    }
    pub fn emit_sll(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emit_inst(RealInstruction::Sll(Sll::new(rd, rs1, rs2)));
    }
    pub fn emit_srl(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emit_inst(RealInstruction::Srl(Srl::new(rd, rs1, rs2)));
    }
    pub fn emit_slli(&mut self, rd: Reg, rs1: Reg, shamt: u8) {
        self.emit_inst(RealInstruction::Slli(Slli::new(rd, rs1, shamt)));
    }
    pub fn emit_srai(&mut self, rd: Reg, rs1: Reg, shamt: u8) {
        self.emit_inst(RealInstruction::Srai(Srai::new(rd, rs1, shamt)));
    }
    pub fn emit_addiw(&mut self, rd: Reg, rs1: Reg, imm: i32) {
        self.emit_inst(RealInstruction::Addiw(Addiw::new(rd, rs1, imm)));
    }
    pub fn emit_jalr(&mut self, rd: Reg, rs1: Reg, imm: i32) {
        self.emit_inst(RealInstruction::Jalr(Jalr::new(rd, rs1, imm)));
    }

    // ---------- convenience compound operations ----------
    pub fn emit_li(&mut self, rd: Reg, imm: i64) {
        if imm >= -2048 && imm <= 2047 {
            self.emit_addi(rd, ZERO, imm as i32);
        } else {
            let hi = ((imm >> 12) & 0xFFFFF) as i32;
            let lo = (imm & 0xFFF) as i32;
            let lo_signed = if lo >= 0x800 { lo - 0x1000 } else { lo };
            let hi_adj = if lo_signed < 0 { hi + 1 } else { hi };
            self.emit_inst(RealInstruction::Lui(Lui::new(rd, hi_adj << 12)));
            if lo_signed != 0 {
                self.emit_addi(rd, rd, lo_signed);
            }
        }
    }
    pub fn emit_mv(&mut self, rd: Reg, rs: Reg) {
        self.emit_addi(rd, rs, 0);
    }
    pub fn emit_neg(&mut self, rd: Reg, rs: Reg) {
        self.emit_sub(rd, ZERO, rs);
    }
    pub fn emit_not(&mut self, rd: Reg, rs: Reg) {
        self.emit_xori(rd, rs, -1);
    }
    pub fn emit_add_imm(&mut self, rd: Reg, rs: Reg, imm: i64) {
        if (-2048..=2047).contains(&imm) {
            self.emit_addi(rd, rs, imm as i32);
        } else {
            let tmp = self.alloc_temp_reg();
            self.emit_li(tmp, imm);
            self.emit_add(rd, rs, tmp);
        }
    }
    pub fn emit_mul_imm(&mut self, rd: Reg, rs: Reg, imm: i32) {
        if imm == 1 {
            self.emit_mv(rd, rs);
        } else if imm == 2 {
            self.emit_add(rd, rs, rs);
        } else {
            let tmp = self.alloc_temp_reg();
            self.emit_li(tmp, imm as i64);
            self.emit_mul(rd, rs, tmp);
        }
    }
    pub fn emit_seq(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        let tmp = self.alloc_temp_reg();
        self.emit_sub(tmp, rs1, rs2);
        self.emit_sltiu(rd, tmp, 1);
    }
    pub fn emit_sne(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        let tmp = self.alloc_temp_reg();
        self.emit_sub(tmp, rs1, rs2);
        self.emit_sltu(rd, ZERO, tmp);
    }
    pub fn emit_seqz(&mut self, rd: Reg, rs: Reg) {
        self.emit_sltiu(rd, rs, 1);
    }
    pub fn emit_cmp_sle(&mut self, rd: Reg, lhs: Reg, rhs: Reg) {
        let tmp = self.alloc_temp_reg();
        self.emit_slt(tmp, rhs, lhs);
        self.emit_seqz(rd, tmp);
    }
    pub fn emit_cmp_sge(&mut self, rd: Reg, lhs: Reg, rhs: Reg) {
        let tmp = self.alloc_temp_reg();
        self.emit_slt(tmp, lhs, rhs);
        self.emit_seqz(rd, tmp);
    }
    pub fn emit_cmp_ule(&mut self, rd: Reg, lhs: Reg, rhs: Reg) {
        let tmp = self.alloc_temp_reg();
        self.emit_sltu(tmp, rhs, lhs);
        self.emit_seqz(rd, tmp);
    }
    pub fn emit_cmp_uge(&mut self, rd: Reg, lhs: Reg, rhs: Reg) {
        let tmp = self.alloc_temp_reg();
        self.emit_sltu(tmp, lhs, rhs);
        self.emit_seqz(rd, tmp);
    }

    // ---------- branches / jumps ----------
    pub fn emit_bne(&mut self, rs1: Reg, rs2: Reg, target: &str) {
        self.emit_raw(&format!(
            "\tbne {}, {}, {}",
            reg_name(rs1, false),
            reg_name(rs2, false),
            target
        ));
    }
    pub fn emit_jal(&mut self, rd: Reg, target: &str) {
        if rd == ZERO {
            self.emit_raw(&format!("\tj {target}"));
        } else {
            self.emit_raw(&format!("\tjal {}, {}", reg_name(rd, false), target));
        }
    }

    // ---------- floating-point instructions ----------
    pub fn emit_fmv_w_x(&mut self, fd: Reg, rs: Reg) {
        self.emit_inst(RealInstruction::FmvWX(FmvWX::new(fd, rs)));
    }
    pub fn emit_fadd_s(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emit_inst(RealInstruction::Fadd(Fadd::new(rd, rs1, rs2)));
    }
    pub fn emit_fsub_s(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emit_inst(RealInstruction::Fsub(Fsub::new(rd, rs1, rs2)));
    }
    pub fn emit_fmul_s(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emit_inst(RealInstruction::Fmul(Fmul::new(rd, rs1, rs2)));
    }
    pub fn emit_fdiv_s(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emit_inst(RealInstruction::Fdiv(Fdiv::new(rd, rs1, rs2)));
    }
    pub fn emit_fsw(&mut self, base: Reg, src: Reg, offset: i32) {
        self.emit_inst(RealInstruction::Fsw(Fsw::new(base, src, offset)));
    }
    pub fn emit_fsd(&mut self, base: Reg, src: Reg, offset: i32) {
        self.emit_inst(RealInstruction::Fsd(Fsd::new(base, src, offset)));
    }
    pub fn emit_flw(&mut self, rd: Reg, base: Reg, offset: i32) {
        self.emit_inst(RealInstruction::Flw(Flw::new(rd, base, offset)));
    }
    pub fn emit_fld(&mut self, rd: Reg, base: Reg, offset: i32) {
        self.emit_inst(RealInstruction::Fld(Fld::new(rd, base, offset)));
    }
    pub fn emit_fmv_s(&mut self, rd: Reg, rs: Reg) {
        self.emit_inst(RealInstruction::Fsgnj(fmv_s(rd, rs)));
    }
    pub fn emit_feq_s(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emit_inst(RealInstruction::FeqS(FeqS::new(rd, rs1, rs2)));
    }
    pub fn emit_flt_s(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emit_inst(RealInstruction::FltS(FltS::new(rd, rs1, rs2)));
    }
    pub fn emit_fle_s(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emit_inst(RealInstruction::FleqS(FleqS::new(rd, rs1, rs2)));
    }

    // ---------- typed memory helpers ----------
    pub fn emit_load_from_slot(&mut self, rd: Reg, slot: usize, ty: &IrType) {
        match ty {
            IrType::Integer(w) => match w {
                crate::intermediate_language::IntWidth::I1
                | crate::intermediate_language::IntWidth::I8 => self.emit_lb(rd, 2, slot as i32),
                crate::intermediate_language::IntWidth::I16 => self.emit_lh(rd, 2, slot as i32),
                crate::intermediate_language::IntWidth::I32 => self.emit_lw(rd, 2, slot as i32),
                crate::intermediate_language::IntWidth::I64 => self.emit_ld(rd, 2, slot as i32),
            },
            IrType::Float(w) => match w {
                crate::intermediate_language::FloatWidth::F32 => self.emit_flw(rd, 2, slot as i32),
                crate::intermediate_language::FloatWidth::F64 => self.emit_fld(rd, 2, slot as i32),
            },
            IrType::Pointer(_) | IrType::Named(_) => self.emit_ld(rd, 2, slot as i32),
            _ => self.emit_ld(rd, 2, slot as i32),
        }
    }

    pub fn emit_load_to_slot(&mut self, slot: usize, addr_reg: Reg, ty: &IrType, offset: i32) {
        let tmp = self.alloc_temp_reg();
        match ty {
            IrType::Integer(w) => match w {
                crate::intermediate_language::IntWidth::I1
                | crate::intermediate_language::IntWidth::I8 => {
                    self.emit_inst(RealInstruction::Lb(Lb::new(tmp, addr_reg, offset)));
                }
                crate::intermediate_language::IntWidth::I16 => {
                    self.emit_inst(RealInstruction::Lh(Lh::new(tmp, addr_reg, offset)));
                }
                crate::intermediate_language::IntWidth::I32 => {
                    self.emit_inst(RealInstruction::Lw(Lw::new(tmp, addr_reg, offset)));
                }
                crate::intermediate_language::IntWidth::I64 => {
                    self.emit_inst(RealInstruction::Ld(Ld::new(tmp, addr_reg, offset)));
                }
            },
            IrType::Float(w) => match w {
                crate::intermediate_language::FloatWidth::F32 => {
                    self.emit_inst(RealInstruction::Flw(Flw::new(tmp, addr_reg, offset)));
                }
                crate::intermediate_language::FloatWidth::F64 => {
                    self.emit_inst(RealInstruction::Fld(Fld::new(tmp, addr_reg, offset)));
                }
            },
            IrType::Pointer(_) | IrType::Named(_) => {
                self.emit_inst(RealInstruction::Ld(Ld::new(tmp, addr_reg, offset)));
            }
            _ => {
                self.emit_inst(RealInstruction::Ld(Ld::new(tmp, addr_reg, offset)));
            }
        }
        self.emit_store_from_tmp(2, tmp, ty, slot as i32);
    }

    pub fn emit_store_from_tmp(&mut self, addr_reg: Reg, val_reg: Reg, ty: &IrType, offset: i32) {
        match ty {
            IrType::Integer(w) => match w {
                crate::intermediate_language::IntWidth::I1
                | crate::intermediate_language::IntWidth::I8 => {
                    self.emit_inst(RealInstruction::Sb(Sb::new(addr_reg, val_reg, offset)));
                }
                crate::intermediate_language::IntWidth::I16 => {
                    self.emit_inst(RealInstruction::Sh(Sh::new(addr_reg, val_reg, offset)));
                }
                crate::intermediate_language::IntWidth::I32 => {
                    self.emit_inst(RealInstruction::Sw(Sw::new(addr_reg, val_reg, offset)));
                }
                crate::intermediate_language::IntWidth::I64 => {
                    self.emit_inst(RealInstruction::Sd(Sd::new(addr_reg, val_reg, offset)));
                }
            },
            IrType::Float(w) => match w {
                crate::intermediate_language::FloatWidth::F32 => {
                    self.emit_inst(RealInstruction::Fsw(Fsw::new(addr_reg, val_reg, offset)));
                }
                crate::intermediate_language::FloatWidth::F64 => {
                    self.emit_inst(RealInstruction::Fsd(Fsd::new(addr_reg, val_reg, offset)));
                }
            },
            IrType::Pointer(_) | IrType::Named(_) => {
                self.emit_inst(RealInstruction::Sd(Sd::new(addr_reg, val_reg, offset)));
            }
            _ => {
                self.emit_inst(RealInstruction::Sd(Sd::new(addr_reg, val_reg, offset)));
            }
        }
    }

    // ---------- block-level memory copy helpers ----------
    pub fn copy_bytes_from_addr_to_slot(
        &mut self,
        slot: usize,
        addr_reg: Reg,
        offset: i32,
        size: usize,
    ) {
        let mut remaining = size;
        let mut current_offset = offset;
        let mut current_slot = slot;

        while remaining >= 8 {
            let tmp = self.alloc_temp_reg();
            self.emit_inst(RealInstruction::Ld(Ld::new(tmp, addr_reg, current_offset)));
            self.emit_inst(RealInstruction::Sd(Sd::new(2, tmp, current_slot as i32)));
            remaining -= 8;
            current_offset += 8;
            current_slot += 8;
        }
        while remaining >= 4 {
            let tmp = self.alloc_temp_reg();
            self.emit_inst(RealInstruction::Lw(Lw::new(tmp, addr_reg, current_offset)));
            self.emit_inst(RealInstruction::Sw(Sw::new(2, tmp, current_slot as i32)));
            remaining -= 4;
            current_offset += 4;
            current_slot += 4;
        }
        let byte_tmp = self.alloc_temp_reg();
        for i in 0..remaining {
            self.emit_inst(RealInstruction::Lb(Lb::new(
                byte_tmp,
                addr_reg,
                current_offset + i as i32,
            )));
            self.emit_inst(RealInstruction::Sb(Sb::new(
                2,
                byte_tmp,
                current_slot as i32 + i as i32,
            )));
        }
    }

    pub fn copy_bytes_from_slot_to_addr(
        &mut self,
        slot: usize,
        addr_reg: Reg,
        offset: i32,
        size: usize,
    ) {
        let mut remaining = size;
        let mut current_offset = offset;
        let mut current_slot = slot;

        while remaining >= 8 {
            let tmp = self.alloc_temp_reg();
            self.emit_inst(RealInstruction::Ld(Ld::new(tmp, 2, current_slot as i32)));
            self.emit_inst(RealInstruction::Sd(Sd::new(addr_reg, tmp, current_offset)));
            remaining -= 8;
            current_offset += 8;
            current_slot += 8;
        }
        while remaining >= 4 {
            let tmp = self.alloc_temp_reg();
            self.emit_inst(RealInstruction::Lw(Lw::new(tmp, 2, current_slot as i32)));
            self.emit_inst(RealInstruction::Sw(Sw::new(addr_reg, tmp, current_offset)));
            remaining -= 4;
            current_offset += 4;
            current_slot += 4;
        }
        let byte_tmp = self.alloc_temp_reg();
        for i in 0..remaining {
            self.emit_inst(RealInstruction::Lb(Lb::new(
                byte_tmp,
                2,
                current_slot as i32 + i as i32,
            )));
            self.emit_inst(RealInstruction::Sb(Sb::new(
                addr_reg,
                byte_tmp,
                current_offset + i as i32,
            )));
        }
    }

    pub fn copy_bytes_from_addr_to_addr(
        &mut self,
        dst_addr: Reg,
        dst_offset: i32,
        src_addr: Reg,
        src_offset: i32,
        size: usize,
    ) {
        let mut remaining = size;
        let mut current_dst_offset = dst_offset;
        let mut current_src_offset = src_offset;

        while remaining >= 8 {
            let tmp = self.alloc_temp_reg();
            self.emit_inst(RealInstruction::Ld(Ld::new(
                tmp,
                src_addr,
                current_src_offset,
            )));
            self.emit_inst(RealInstruction::Sd(Sd::new(
                dst_addr,
                tmp,
                current_dst_offset,
            )));
            remaining -= 8;
            current_dst_offset += 8;
            current_src_offset += 8;
        }
        while remaining >= 4 {
            let tmp = self.alloc_temp_reg();
            self.emit_inst(RealInstruction::Lw(Lw::new(
                tmp,
                src_addr,
                current_src_offset,
            )));
            self.emit_inst(RealInstruction::Sw(Sw::new(
                dst_addr,
                tmp,
                current_dst_offset,
            )));
            remaining -= 4;
            current_dst_offset += 4;
            current_src_offset += 4;
        }
        let byte_tmp = self.alloc_temp_reg();
        for i in 0..remaining {
            self.emit_inst(RealInstruction::Lb(Lb::new(
                byte_tmp,
                src_addr,
                current_src_offset + i as i32,
            )));
            self.emit_inst(RealInstruction::Sb(Sb::new(
                dst_addr,
                byte_tmp,
                current_dst_offset + i as i32,
            )));
        }
    }

    pub fn finish(&mut self) -> String {
        let result = self.lines.join("\n");
        result
    }
}

impl Default for AssemblyEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl Rv64Backend for AssemblyEmitter {
    fn alloc_temp_reg(&mut self) -> Reg {
        Self::alloc_temp_reg(self)
    }

    fn emit_add_imm(&mut self, rd: Reg, rs: Reg, imm: i64) {
        Self::emit_add_imm(self, rd, rs, imm);
    }

    fn emit_sd(&mut self, base: Reg, src: Reg, offset: i32) {
        Self::emit_sd(self, base, src, offset);
    }

    fn emit_ld(&mut self, rd: Reg, base: Reg, offset: i32) {
        Self::emit_ld(self, rd, base, offset);
    }

    fn emit_mv(&mut self, rd: Reg, rs: Reg) {
        Self::emit_mv(self, rd, rs);
    }

    fn emit_jalr(&mut self, rd: Reg, rs1: Reg, imm: i32) {
        Self::emit_jalr(self, rd, rs1, imm);
    }

    fn emit_li(&mut self, rd: Reg, imm: i64) {
        Self::emit_li(self, rd, imm);
    }

    fn emit_store_from_tmp(&mut self, addr_reg: Reg, val_reg: Reg, ty: &IrType, offset: i32) {
        Self::emit_store_from_tmp(self, addr_reg, val_reg, ty, offset);
    }

    fn emit_load_to_slot(&mut self, slot: usize, addr_reg: Reg, ty: &IrType, offset: i32) {
        Self::emit_load_to_slot(self, slot, addr_reg, ty, offset);
    }

    fn emit_comment(&mut self, text: &str) {
        Self::emit_comment(self, text);
    }
}
