use super::{
    assembly_emitter::AssemblyEmitter, data_section::DataSection,
    function_context::{FunctionContext, Rv64Backend},
    register_allocator::RegisterAllocator,
};
use crate::assembly_language::encode_decode::Reg;
use crate::assembly_language::real::RealInstruction;
use crate::assembly_language::riscv::rv64fd::*;
use crate::assembly_language::riscv::rv64i::*;
use crate::assembly_language::riscv::rv64m::*;
use crate::assembly_language::utils::reg_name;
use crate::intermediate_language::{
    IrCastMode, IrCmpOp, IrInstruction, IrMathOp, IrProgram, IrTerminator, IrType, IrUnaryOp,
    IrValue,
};
use std::collections::{HashMap, HashSet};

const ZERO: Reg = 0;
const RA: Reg = 1;
const SP: Reg = 2;
const S0: Reg = 8;
const T0: Reg = 5;
const T1: Reg = 6;
const T2: Reg = 7;
const T3: Reg = 28;
const T4: Reg = 29;
const T5: Reg = 30;
const T6: Reg = 31;
const A0: Reg = 10;

pub struct CompilerRv64 {
    emitter: AssemblyEmitter,
    data: DataSection,
    temp_counter: usize,
    type_aliases: HashMap<String, IrType>,
    function_return_types: HashMap<String, IrType>,
}

impl Rv64Backend for CompilerRv64 {
    fn emit_add_imm(&mut self, rd: Reg, rs: Reg, imm: i64) {
        self.emit_add_imm(rd, rs, imm);
    }
    fn emit_sd(&mut self, base: Reg, src: Reg, offset: i32) {
        self.emit_sd(base, src, offset);
    }
    fn emit_ld(&mut self, rd: Reg, base: Reg, offset: i32) {
        self.emit_ld(rd, base, offset);
    }
    fn emit_mv(&mut self, rd: Reg, rs: Reg) {
        self.emit_mv(rd, rs);
    }
    fn emit_jalr(&mut self, rd: Reg, rs1: Reg, imm: i32) {
        self.emit_jalr(rd, rs1, imm);
    }
    fn emit_li(&mut self, rd: Reg, imm: i64) {
        self.emit_li(rd, imm);
    }
    fn emit_store_from_tmp(&mut self, addr_reg: Reg, val_reg: Reg, ty: &IrType, offset: i32) {
        self.emit_store_from_tmp(addr_reg, val_reg, ty, offset);
    }
    fn emit_load_to_slot(&mut self, slot: usize, addr_reg: Reg, ty: &IrType, offset: i32) {
        self.emit_load_to_slot(slot, addr_reg, ty, offset);
    }
}

impl CompilerRv64 {
    pub fn new() -> Self {
        Self {
            emitter: AssemblyEmitter::new(),
            data: DataSection::new(),
            temp_counter: 0,
            type_aliases: HashMap::new(),
            function_return_types: HashMap::new(),
        }
    }

    pub fn compile(&mut self, program: &IrProgram) -> String {
        self.emitter.reset();
        self.data.reset();
        self.type_aliases.clear();
        self.function_return_types.clear();

        for s in &program.global_strings {
            self.data.add_global_string(s);
        }
        for alias in &program.type_aliases {
            self.type_aliases.insert(alias.name.clone(), alias.ty.clone());
            self.data.add_type_alias(alias);
        }
        for func in &program.functions {
            self.function_return_types
                .insert(func.name.clone(), func.return_type.clone());
        }

        for func in &program.functions {
            self.compile_function(func);
        }

        self.emitter.emit_data_section(&self.data);
        self.emitter.emit_text_section();
        self.emitter.emit_functions();

        self.emitter.finish()
    }

    fn compile_function(&mut self, func: &crate::intermediate_language::IrFunction) {
        let mut ctx = FunctionContext::new(&func.name, &self.type_aliases);
        let mut alloc = RegisterAllocator::new();
        alloc.allocate_slots(func, &mut ctx, &self.function_return_types);
        ctx.frame.save_ra();
        ctx.frame.save_reg(S0);
        ctx.finalize();

        for block in &func.blocks {
            ctx.map_label(&block.label, format!("{}__{}", func.name, block.label.0));
        }

        self.emitter.start_function(&func.name);
        ctx.emit_prologue(self);
        ctx.emit_parameter_spills(self, func);

        for block in &func.blocks {
            let label = ctx.get_label(&block.label).unwrap();
            self.emitter.emit_label(label);
            for inst in &block.instructions {
                self.lower_instruction(inst, &mut ctx);
            }
            if let Some(term) = &block.terminator {
                self.lower_terminator(term, &mut ctx);
            }
        }

        ctx.emit_epilogue(self);
        self.emitter.end_function();
    }


    fn lower_instruction(&mut self, inst: &IrInstruction, ctx: &mut FunctionContext) {
        use IrInstruction::*;
        match inst {
            Comment(s) => self.emitter.emit_comment(s),
            Alloc { .. } => {}
            Load {
                dest,
                ty,
                ptr,
                offset,
            } => {
                let ptr_slot = ctx.slot_for_reg(ptr).expect("ptr slot");
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                let ptr_tmp = self.alloc_temp_reg();
                self.emit_ld(ptr_tmp, SP, ptr_slot as i32);
                let addr_tmp = if let Some(off) = offset {
                    let tmp = self.alloc_temp_reg();
                    self.emit_addi(tmp, ptr_tmp, *off as i32);
                    tmp
                } else {
                    ptr_tmp
                };
                let resolved_ty = self.resolve_ir_type(ty);
                if matches!(resolved_ty, IrType::Array { .. } | IrType::Aggregate(_)) {
                    self.copy_bytes_from_addr_to_slot(
                        dest_slot,
                        addr_tmp,
                        0,
                        self.type_size(&resolved_ty),
                    );
                } else {
                    self.emit_load_to_slot(dest_slot, addr_tmp, &resolved_ty, 0);
                }
            }
            Store {
                ty,
                value,
                ptr,
                offset,
            } => {
                let ptr_slot = ctx.slot_for_reg(ptr).expect("ptr slot");
                let ptr_tmp = self.alloc_temp_reg();
                self.emit_ld(ptr_tmp, SP, ptr_slot as i32);
                let addr_tmp = if let Some(off) = offset {
                    let tmp = self.alloc_temp_reg();
                    self.emit_addi(tmp, ptr_tmp, *off as i32);
                    tmp
                } else {
                    ptr_tmp
                };
                let resolved_ty = self.resolve_ir_type(ty);
                if matches!(resolved_ty, IrType::Array { .. } | IrType::Aggregate(_)) {
                    let IrValue::Register(reg) = value else {
                        unimplemented!("composite stores require a register source")
                    };
                    let val_slot = ctx.slot_for_reg(reg).expect("value slot");
                    self.copy_bytes_from_slot_to_addr(
                        val_slot,
                        addr_tmp,
                        0,
                        self.type_size(&resolved_ty),
                    );
                } else {
                    let val_tmp = self.load_value_to_temp(value, ctx);
                    self.emit_store_from_tmp(addr_tmp, val_tmp, &resolved_ty, 0);
                }
            }
            Offset {
                dest,
                ty: _,
                ptr,
                bytes,
            } => {
                let ptr_slot = ctx.slot_for_reg(ptr).expect("ptr slot");
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                let ptr_tmp = self.alloc_temp_reg();
                self.emit_ld(ptr_tmp, SP, ptr_slot as i32);
                let byte_val_reg = self.load_value_to_temp(bytes, ctx);
                let off_tmp = self.alloc_temp_reg();
                self.emit_mv(off_tmp, byte_val_reg);
                let result_tmp = self.alloc_temp_reg();
                self.emit_add(result_tmp, ptr_tmp, off_tmp);
                self.emit_sd(SP, result_tmp, dest_slot as i32);
            }
            Index {
                dest,
                ty,
                base_ptr,
                idx,
            } => {
                let base_slot = ctx.slot_for_reg(base_ptr).expect("base slot");
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                let base_tmp = self.alloc_temp_reg();
                self.emit_ld(base_tmp, SP, base_slot as i32);
                let idx_tmp = self.load_value_to_temp(idx, ctx);
                let scale = self.type_size(ty);
                let scaled_tmp = self.alloc_temp_reg();
                if scale == 1 {
                    self.emit_mv(scaled_tmp, idx_tmp);
                } else {
                    self.emit_mul_imm(scaled_tmp, idx_tmp, scale as i32);
                }
                let result_tmp = self.alloc_temp_reg();
                self.emit_add(result_tmp, base_tmp, scaled_tmp);
                self.emit_sd(SP, result_tmp, dest_slot as i32);
            }
            Math {
                dest,
                op,
                ty: _,
                lhs,
                rhs,
            } => {
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                let lhs_tmp = self.load_value_to_temp(lhs, ctx);
                let rhs_tmp = self.load_value_to_temp(rhs, ctx);
                let result_tmp = self.alloc_temp_reg();
                match op {
                    IrMathOp::Add => self.emit_add(result_tmp, lhs_tmp, rhs_tmp),
                    IrMathOp::Sub => self.emit_sub(result_tmp, lhs_tmp, rhs_tmp),
                    IrMathOp::Mul => self.emit_mul(result_tmp, lhs_tmp, rhs_tmp),
                    IrMathOp::Div => self.emit_div(result_tmp, lhs_tmp, rhs_tmp),
                    IrMathOp::SDiv => self.emit_div(result_tmp, lhs_tmp, rhs_tmp),
                    IrMathOp::Mod => self.emit_rem(result_tmp, lhs_tmp, rhs_tmp),
                    IrMathOp::Shl => self.emit_sll(result_tmp, lhs_tmp, rhs_tmp),
                    IrMathOp::Shr => self.emit_srl(result_tmp, lhs_tmp, rhs_tmp),
                    IrMathOp::And => self.emit_and(result_tmp, lhs_tmp, rhs_tmp),
                    IrMathOp::Or => self.emit_or(result_tmp, lhs_tmp, rhs_tmp),
                    IrMathOp::Xor => self.emit_xor(result_tmp, lhs_tmp, rhs_tmp),
                }
                self.emit_sd(SP, result_tmp, dest_slot as i32);
            }
            Unary {
                dest,
                op,
                ty: _,
                value,
            } => {
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                let val_tmp = self.load_value_to_temp(value, ctx);
                let result_tmp = self.alloc_temp_reg();
                match op {
                    IrUnaryOp::Neg => self.emit_neg(result_tmp, val_tmp),
                    IrUnaryOp::Not => self.emit_not(result_tmp, val_tmp),
                }
                self.emit_sd(SP, result_tmp, dest_slot as i32);
            }
            Cmp {
                dest,
                op,
                ty: _,
                lhs,
                rhs,
            } => {
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                let lhs_tmp = self.load_value_to_temp(lhs, ctx);
                let rhs_tmp = self.load_value_to_temp(rhs, ctx);
                let result_tmp = self.alloc_temp_reg();
                match op {
                    IrCmpOp::Eq => self.emit_seq(result_tmp, lhs_tmp, rhs_tmp),
                    IrCmpOp::Ne => self.emit_sne(result_tmp, lhs_tmp, rhs_tmp),
                    IrCmpOp::Slt => self.emit_slt(result_tmp, lhs_tmp, rhs_tmp),
                    IrCmpOp::Ult => self.emit_sltu(result_tmp, lhs_tmp, rhs_tmp),
                    IrCmpOp::Sle => self.emit_cmp_sle(result_tmp, lhs_tmp, rhs_tmp),
                    IrCmpOp::Ule => self.emit_cmp_ule(result_tmp, lhs_tmp, rhs_tmp),
                    IrCmpOp::Sgt => self.emit_slt(result_tmp, rhs_tmp, lhs_tmp),
                    IrCmpOp::Ugt => self.emit_sltu(result_tmp, rhs_tmp, lhs_tmp),
                    IrCmpOp::Sge => self.emit_cmp_sge(result_tmp, lhs_tmp, rhs_tmp),
                    IrCmpOp::Uge => self.emit_cmp_uge(result_tmp, lhs_tmp, rhs_tmp),
                }
                self.emit_sd(SP, result_tmp, dest_slot as i32);
            }
            Cast { dest, mode, value, ty } => {
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                let src_tmp = self.load_value_to_temp(value, ctx);
                let result_tmp = self.alloc_temp_reg();
                self.lower_cast(result_tmp, src_tmp, *mode, ty);
                self.emit_sd(SP, result_tmp, dest_slot as i32);
            }
            Call {
                dest,
                function,
                args,
            } => {
                for (i, arg) in args.iter().enumerate() {
                    if i >= 8 {
                        break;
                    }
                    let arg_tmp = self.load_value_to_temp(arg, ctx);
                    self.emit_mv(reg_for_arg(i), arg_tmp);
                }
                self.emit_jal(RA, function.as_str());
                if let Some(dest) = dest {
                    let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                    self.emit_sd(SP, A0, dest_slot as i32);
                }
            }
            Phi { .. } => {}
            HeapAlloc { dest, ty, count } => {
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                let elem_count = count.unwrap_or(1);
                let bytes = self.type_size(ty).saturating_mul(elem_count);

                self.emit_li(A0, bytes as i64);
                self.emitter.emit_raw("\tcall malloc");
                self.emit_sd(SP, A0, dest_slot as i32);
            }
            HeapFree { ptr } => {
                let ptr_slot = ctx.slot_for_reg(ptr).expect("ptr slot");
                let ptr_tmp = self.alloc_temp_reg();
                self.emit_ld(ptr_tmp, SP, ptr_slot as i32);
                self.emit_mv(A0, ptr_tmp);
                self.emitter.emit_raw("\tcall free");
            }
        }
    }

    fn lower_terminator(&mut self, term: &IrTerminator, ctx: &mut FunctionContext) {
        match term {
            IrTerminator::Return(val) => {
                if let Some(val) = val {
                    let val_tmp = self.load_value_to_temp(val, ctx);
                    self.emit_mv(A0, val_tmp);
                }
            }
            IrTerminator::Jump(label) => {
                let lbl = ctx.get_label(label).unwrap();
                self.emit_jal(ZERO, lbl);
            }
            IrTerminator::Branch {
                cond,
                then_label,
                else_label,
            } => {
                let cond_tmp = self.load_value_to_temp(cond, ctx);
                let then_lbl = ctx.get_label(then_label).unwrap();
                let else_lbl = ctx.get_label(else_label).unwrap();
                self.emit_bne(cond_tmp, ZERO, else_lbl);
                self.emit_jal(ZERO, then_lbl);
            }
        }
    }

    // -------------------------------------------------------------------------
    // RISC‑V instruction emission helpers (unchanged)
    // -------------------------------------------------------------------------
    fn emit_addi(&mut self, rd: Reg, rs1: Reg, imm: i32) {
        self.emitter
            .emit_inst(RealInstruction::Addi(Addi::new(rd, rs1, imm)));
    }
    fn emit_sd(&mut self, base: Reg, src: Reg, offset: i32) {
        self.emitter
            .emit_inst(RealInstruction::Sd(Sd::new(base, src, offset)));
    }
    fn emit_ld(&mut self, rd: Reg, base: Reg, offset: i32) {
        self.emitter
            .emit_inst(RealInstruction::Ld(Ld::new(rd, base, offset)));
    }
    fn emit_lw(&mut self, rd: Reg, base: Reg, offset: i32) {
        self.emitter
            .emit_inst(RealInstruction::Lw(Lw::new(rd, base, offset)));
    }
    fn emit_lh(&mut self, rd: Reg, base: Reg, offset: i32) {
        self.emitter
            .emit_inst(RealInstruction::Lh(Lh::new(rd, base, offset)));
    }
    fn emit_lb(&mut self, rd: Reg, base: Reg, offset: i32) {
        self.emitter
            .emit_inst(RealInstruction::Lb(Lb::new(rd, base, offset)));
    }
    fn emit_flw(&mut self, rd: Reg, base: Reg, offset: i32) {
        self.emitter
            .emit_inst(RealInstruction::Flw(Flw::new(rd, base, offset)));
    }
    fn emit_fld(&mut self, rd: Reg, base: Reg, offset: i32) {
        self.emitter
            .emit_inst(RealInstruction::Fld(Fld::new(rd, base, offset)));
    }
    fn emit_add(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emitter
            .emit_inst(RealInstruction::Add(Add::new(rd, rs1, rs2)));
    }
    fn emit_addiw(&mut self, rd: Reg, rs1: Reg, imm: i32) {
        self.emitter
            .emit_inst(RealInstruction::Addiw(Addiw::new(rd, rs1, imm)));
    }
    fn emit_sub(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emitter
            .emit_inst(RealInstruction::Sub(Sub::new(rd, rs1, rs2)));
    }
    fn emit_mul(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emitter
            .emit_inst(RealInstruction::Mul(Mul::new(rd, rs1, rs2)));
    }
    fn emit_div(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emitter
            .emit_inst(RealInstruction::Div(Div::new(rd, rs1, rs2)));
    }
    fn emit_rem(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emitter
            .emit_inst(RealInstruction::Rem(Rem::new(rd, rs1, rs2)));
    }
    fn emit_and(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emitter
            .emit_inst(RealInstruction::And(And::new(rd, rs1, rs2)));
    }
    fn emit_or(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emitter
            .emit_inst(RealInstruction::Or(Or::new(rd, rs1, rs2)));
    }
    fn emit_xor(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emitter
            .emit_inst(RealInstruction::Xor(Xor::new(rd, rs1, rs2)));
    }
    fn emit_neg(&mut self, rd: Reg, rs: Reg) {
        self.emit_sub(rd, ZERO, rs);
    }
    fn emit_not(&mut self, rd: Reg, rs: Reg) {
        self.emit_xori(rd, rs, -1);
    }
    fn emit_xori(&mut self, rd: Reg, rs1: Reg, imm: i32) {
        self.emitter
            .emit_inst(RealInstruction::Xori(Xori::new(rd, rs1, imm)));
    }
    fn emit_li(&mut self, rd: Reg, imm: i64) {
        if imm >= -2048 && imm <= 2047 {
            self.emit_addi(rd, ZERO, imm as i32);
        } else {
            let hi = ((imm >> 12) & 0xFFFFF) as i32;
            let lo = (imm & 0xFFF) as i32;
            let lo_signed = if lo >= 0x800 { lo - 0x1000 } else { lo };
            let hi_adj = if lo_signed < 0 { hi + 1 } else { hi };
            self.emitter
                .emit_inst(RealInstruction::Lui(Lui::new(rd, hi_adj << 12)));
            if lo_signed != 0 {
                self.emit_addi(rd, rd, lo_signed);
            }
        }
    }
    fn emit_mv(&mut self, rd: Reg, rs: Reg) {
        self.emit_addi(rd, rs, 0);
    }
    fn emit_mul_imm(&mut self, rd: Reg, rs: Reg, imm: i32) {
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
    fn emit_seq(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        let tmp = self.alloc_temp_reg();
        self.emit_sub(tmp, rs1, rs2);
        self.emit_sltiu(rd, tmp, 1);
    }
    fn emit_sltiu(&mut self, rd: Reg, rs1: Reg, imm: i32) {
        self.emitter
            .emit_inst(RealInstruction::Sltiu(Sltiu::new(rd, rs1, imm)));
    }
    fn emit_sne(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        let tmp = self.alloc_temp_reg();
        self.emit_sub(tmp, rs1, rs2);
        self.emit_sltu(rd, ZERO, tmp);
    }
    fn emit_sltu(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emitter
            .emit_inst(RealInstruction::Sltu(Sltu::new(rd, rs1, rs2)));
    }
    fn emit_slt(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emitter
            .emit_inst(RealInstruction::Slt(Slt::new(rd, rs1, rs2)));
    }
    fn emit_bne(&mut self, rs1: Reg, rs2: Reg, _target: &str) {
        self.emitter.emit_raw(&format!(
            "\tbne {}, {}, {}",
            reg_name(rs1, false),
            reg_name(rs2, false),
            _target
        ));
    }
    fn emit_jal(&mut self, rd: Reg, _target: &str) {
        if rd == ZERO {
            self.emitter.emit_raw(&format!("\tj {}", _target));
        } else {
            self.emitter
                .emit_raw(&format!("\tjal {}, {}", reg_name(rd, false), _target));
        }
    }
    fn emit_jalr(&mut self, rd: Reg, rs1: Reg, imm: i32) {
        self.emitter
            .emit_inst(RealInstruction::Jalr(Jalr::new(rd, rs1, imm)));
    }
    fn emit_fmv_w_x(&mut self, fd: Reg, rs: Reg) {
        self.emitter
            .emit_inst(RealInstruction::FmvWX(FmvWX::new(fd, rs)));
    }

    fn copy_bytes_from_addr_to_slot(&mut self, slot: usize, addr_reg: Reg, offset: i32, size: usize) {
        for i in 0..size {
            let tmp = self.alloc_temp_reg();
            self.emitter.emit_inst(RealInstruction::Lb(Lb::new(
                tmp,
                addr_reg,
                offset + i as i32,
            )));
            self.emitter.emit_inst(RealInstruction::Sb(Sb::new(
                SP,
                tmp,
                slot as i32 + i as i32,
            )));
        }
    }

    fn copy_bytes_from_slot_to_addr(
        &mut self,
        slot: usize,
        addr_reg: Reg,
        offset: i32,
        size: usize,
    ) {
        for i in 0..size {
            let tmp = self.alloc_temp_reg();
            self.emitter.emit_inst(RealInstruction::Lb(Lb::new(
                tmp,
                SP,
                slot as i32 + i as i32,
            )));
            self.emitter.emit_inst(RealInstruction::Sb(Sb::new(
                addr_reg,
                tmp,
                offset + i as i32,
            )));
        }
    }

    // Load a value into a temporary register
    fn load_value_to_temp(&mut self, val: &IrValue, ctx: &FunctionContext) -> Reg {
        let temp = self.alloc_temp_reg();
        match val {
            IrValue::Register(reg) => {
                let slot = ctx.slot_for_reg(reg).expect("reg slot");
                if ctx.is_stack_address(reg) {
                    self.emit_addi(temp, SP, slot as i32);
                } else {
                    let ty = ctx
                        .type_for_reg(reg)
                        .unwrap_or(IrType::Integer(crate::intermediate_language::IntWidth::I32));
                    self.emit_load_from_slot(temp, slot, &ty);
                }
            }
            IrValue::Integer(i) => self.emit_li(temp, *i),
            IrValue::Bool(b) => self.emit_li(temp, if *b { 1 } else { 0 }),
            IrValue::Float(f) => {
                let int_tmp = self.alloc_temp_reg();
                self.emit_li(int_tmp, f.to_bits() as i64);
                self.emit_fmv_w_x(temp, int_tmp);
            }
            IrValue::Null => self.emit_li(temp, 0),
            IrValue::GlobalString(symbol) => {
                self.emitter
                    .emit_raw(&format!("\tla {}, {}", reg_name(temp, false), symbol));
            }
        }
        temp
    }

    fn emit_load_to_slot(&mut self, slot: usize, addr_reg: Reg, ty: &IrType, offset: i32) {
        let tmp = self.alloc_temp_reg();
        match ty {
            IrType::Integer(w) => match w {
                crate::intermediate_language::IntWidth::I1
                | crate::intermediate_language::IntWidth::I8 => {
                    self.emitter
                        .emit_inst(RealInstruction::Lb(Lb::new(tmp, addr_reg, offset)));
                }
                crate::intermediate_language::IntWidth::I16 => {
                    self.emitter
                        .emit_inst(RealInstruction::Lh(Lh::new(tmp, addr_reg, offset)));
                }
                crate::intermediate_language::IntWidth::I32 => {
                    self.emitter
                        .emit_inst(RealInstruction::Lw(Lw::new(tmp, addr_reg, offset)));
                }
                crate::intermediate_language::IntWidth::I64 => {
                    self.emitter
                        .emit_inst(RealInstruction::Ld(Ld::new(tmp, addr_reg, offset)));
                }
            },
            IrType::Float(w) => match w {
                crate::intermediate_language::FloatWidth::F32 => {
                    self.emitter
                        .emit_inst(RealInstruction::Flw(Flw::new(tmp, addr_reg, offset)));
                }
                crate::intermediate_language::FloatWidth::F64 => {
                    self.emitter
                        .emit_inst(RealInstruction::Fld(Fld::new(tmp, addr_reg, offset)));
                }
            },
            IrType::Pointer(_) => {
                self.emitter
                    .emit_inst(RealInstruction::Ld(Ld::new(tmp, addr_reg, offset)));
            }
            IrType::Named(_) => {
                self.emitter
                    .emit_inst(RealInstruction::Ld(Ld::new(tmp, addr_reg, offset)));
            }
            _ => {
                self.emitter
                    .emit_inst(RealInstruction::Ld(Ld::new(tmp, addr_reg, offset)));
            }
        }
        self.emit_store_from_tmp(SP, tmp, ty, slot as i32);
    }

    fn emit_load_from_slot(&mut self, rd: Reg, slot: usize, ty: &IrType) {
        match ty {
            IrType::Integer(w) => match w {
                crate::intermediate_language::IntWidth::I1
                | crate::intermediate_language::IntWidth::I8 => self.emit_lb(rd, SP, slot as i32),
                crate::intermediate_language::IntWidth::I16 => self.emit_lh(rd, SP, slot as i32),
                crate::intermediate_language::IntWidth::I32 => self.emit_lw(rd, SP, slot as i32),
                crate::intermediate_language::IntWidth::I64 => self.emit_ld(rd, SP, slot as i32),
            },
            IrType::Float(w) => match w {
                crate::intermediate_language::FloatWidth::F32 => self.emit_flw(rd, SP, slot as i32),
                crate::intermediate_language::FloatWidth::F64 => self.emit_fld(rd, SP, slot as i32),
            },
            IrType::Pointer(_) | IrType::Named(_) => self.emit_ld(rd, SP, slot as i32),
            _ => self.emit_ld(rd, SP, slot as i32),
        }
    }

    fn emit_store_from_tmp(&mut self, addr_reg: Reg, val_reg: Reg, ty: &IrType, offset: i32) {
        match ty {
            IrType::Integer(w) => match w {
                crate::intermediate_language::IntWidth::I1
                | crate::intermediate_language::IntWidth::I8 => {
                    self.emitter
                        .emit_inst(RealInstruction::Sb(Sb::new(addr_reg, val_reg, offset)));
                }
                crate::intermediate_language::IntWidth::I16 => {
                    self.emitter
                        .emit_inst(RealInstruction::Sh(Sh::new(addr_reg, val_reg, offset)));
                }
                crate::intermediate_language::IntWidth::I32 => {
                    self.emitter
                        .emit_inst(RealInstruction::Sw(Sw::new(addr_reg, val_reg, offset)));
                }
                crate::intermediate_language::IntWidth::I64 => {
                    self.emitter
                        .emit_inst(RealInstruction::Sd(Sd::new(addr_reg, val_reg, offset)));
                }
            },
            IrType::Float(w) => match w {
                crate::intermediate_language::FloatWidth::F32 => {
                    self.emitter
                        .emit_inst(RealInstruction::Fsw(Fsw::new(addr_reg, val_reg, offset)));
                }
                crate::intermediate_language::FloatWidth::F64 => {
                    self.emitter
                        .emit_inst(RealInstruction::Fsd(Fsd::new(addr_reg, val_reg, offset)));
                }
            },
            IrType::Pointer(_) => {
                self.emitter
                    .emit_inst(RealInstruction::Sd(Sd::new(addr_reg, val_reg, offset)));
            }
            IrType::Named(_) => {
                self.emitter
                    .emit_inst(RealInstruction::Sd(Sd::new(addr_reg, val_reg, offset)));
            }
            _ => {
                self.emitter
                    .emit_inst(RealInstruction::Sd(Sd::new(addr_reg, val_reg, offset)));
            }
        }
    }

    fn emit_sll(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emitter
            .emit_inst(RealInstruction::Sll(Sll::new(rd, rs1, rs2)));
    }

    fn emit_srl(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emitter
            .emit_inst(RealInstruction::Srl(Srl::new(rd, rs1, rs2)));
    }

    fn emit_cmp_sle(&mut self, rd: Reg, lhs: Reg, rhs: Reg) {
        let tmp = self.alloc_temp_reg();
        self.emit_slt(tmp, rhs, lhs);
        self.emit_seqz(rd, tmp);
    }

    fn emit_cmp_sge(&mut self, rd: Reg, lhs: Reg, rhs: Reg) {
        let tmp = self.alloc_temp_reg();
        self.emit_slt(tmp, lhs, rhs);
        self.emit_seqz(rd, tmp);
    }

    fn emit_cmp_ule(&mut self, rd: Reg, lhs: Reg, rhs: Reg) {
        let tmp = self.alloc_temp_reg();
        self.emit_sltu(tmp, rhs, lhs);
        self.emit_seqz(rd, tmp);
    }

    fn emit_cmp_uge(&mut self, rd: Reg, lhs: Reg, rhs: Reg) {
        let tmp = self.alloc_temp_reg();
        self.emit_sltu(tmp, lhs, rhs);
        self.emit_seqz(rd, tmp);
    }

    fn emit_seqz(&mut self, rd: Reg, rs: Reg) {
        self.emit_sltiu(rd, rs, 1);
    }

    fn lower_cast(&mut self, rd: Reg, rs: Reg, mode: IrCastMode, ty: &IrType) {
        match mode {
            IrCastMode::Bitcast | IrCastMode::Trunc | IrCastMode::Zext => {
                self.emit_mv(rd, rs);
            }
            IrCastMode::Sext => match ty {
                IrType::Integer(crate::intermediate_language::IntWidth::I32) => {
                    self.emit_addiw(rd, rs, 0)
                }
                IrType::Integer(crate::intermediate_language::IntWidth::I64)
                | IrType::Pointer(_) => self.emit_mv(rd, rs),
                IrType::Integer(crate::intermediate_language::IntWidth::I16) => {
                    self.emit_slli(rd, rs, 48);
                    self.emit_srai(rd, rd, 48);
                }
                IrType::Integer(crate::intermediate_language::IntWidth::I8) => {
                    self.emit_slli(rd, rs, 56);
                    self.emit_srai(rd, rd, 56);
                }
                _ => self.emit_mv(rd, rs),
            },
            IrCastMode::F2i | IrCastMode::I2f => self.emit_mv(rd, rs),
        }
    }

    fn emit_slli(&mut self, rd: Reg, rs1: Reg, shamt: u8) {
        self.emitter
            .emit_inst(RealInstruction::Slli(Slli::new(rd, rs1, shamt)));
    }

    fn emit_add_imm(&mut self, rd: Reg, rs: Reg, imm: i64) {
        if (-2048..=2047).contains(&imm) {
            self.emit_addi(rd, rs, imm as i32);
        } else {
            let tmp = self.alloc_temp_reg();
            self.emit_li(tmp, imm);
            self.emit_add(rd, rs, tmp);
        }
    }

    fn emit_srai(&mut self, rd: Reg, rs1: Reg, shamt: u8) {
        self.emitter
            .emit_inst(RealInstruction::Srai(Srai::new(rd, rs1, shamt)));
    }

    fn alloc_temp_reg(&mut self) -> Reg {
        let temps = [T0, T1, T2, T3, T4, T5, T6];
        let reg = temps[self.temp_counter % temps.len()];
        self.temp_counter += 1;
        reg
    }

    fn resolve_ir_type(&self, ty: &IrType) -> IrType {
        self.resolve_ir_type_inner(ty, &mut HashSet::new())
    }

    fn resolve_ir_type_inner(&self, ty: &IrType, seen: &mut HashSet<String>) -> IrType {
        match ty {
            IrType::Named(name) => self
                .type_aliases
                .get(name)
                .cloned()
                .map(|resolved| {
                    if !seen.insert(name.clone()) {
                        IrType::Named(name.clone())
                    } else {
                        let out = self.resolve_ir_type_inner(&resolved, seen);
                        seen.remove(name);
                        out
                    }
                })
                .unwrap_or_else(|| IrType::Named(name.clone())),
            IrType::Pointer(inner) => {
                IrType::Pointer(Box::new(self.resolve_ir_type_inner(inner, seen)))
            }
            IrType::Array { len, element } => IrType::Array {
                len: *len,
                element: Box::new(self.resolve_ir_type_inner(element, seen)),
            },
            IrType::Aggregate(fields) => IrType::Aggregate(
                fields
                    .iter()
                    .map(|(name, field_ty)| {
                        (name.clone(), self.resolve_ir_type_inner(field_ty, seen))
                    })
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    fn type_size(&self, ty: &IrType) -> usize {
        match self.resolve_ir_type(ty) {
            IrType::Void => 0,
            IrType::Integer(w) => match w {
                crate::intermediate_language::IntWidth::I1 => 1,
                crate::intermediate_language::IntWidth::I8 => 1,
                crate::intermediate_language::IntWidth::I16 => 2,
                crate::intermediate_language::IntWidth::I32 => 4,
                crate::intermediate_language::IntWidth::I64 => 8,
            },
            IrType::Float(w) => match w {
                crate::intermediate_language::FloatWidth::F32 => 4,
                crate::intermediate_language::FloatWidth::F64 => 8,
            },
            IrType::Pointer(_) => 8,
            IrType::Array { len, element } => len * self.type_size(&element),
            IrType::Aggregate(fields) => fields.iter().map(|(_, t)| self.type_size(t)).sum(),
            IrType::Named(_) => 8,
        }
    }
}

fn reg_for_arg(i: usize) -> Reg {
    match i {
        0 => 10, // a0
        1 => 11, // a1
        2 => 12, // a2
        3 => 13, // a3
        4 => 14, // a4
        5 => 15, // a5
        6 => 16, // a6
        7 => 17, // a7
        _ => 0,
    }
}