// file: src/2_intermediate_language/asm_compiler/compiler_rv64.rs

use super::{
    assembly_emitter::AssemblyEmitter, data_section::DataSection,
    function_context::FunctionContext, register_allocator::RegisterAllocator,
};
use crate::assembly_language::encode_decode::Reg;
use crate::assembly_language::real::RealInstruction;
use crate::assembly_language::riscv::rv64fd::{Fsgnjn, fmv_d};
use crate::assembly_language::riscv::rv64i::{Sb, Sh, Sw};
use crate::assembly_language::utils::reg_name;
use crate::intermediate_language::{
    IrCastMode, IrCmpOp, IrInstruction, IrMathOp, IrProgram, IrTerminator, IrType, IrUnaryOp,
    IrValue,
};
use log::warn;
use std::collections::{HashMap, HashSet};

const ZERO: Reg = 0;
const RA: Reg = 1;
const SP: Reg = 2;
const S0: Reg = 8;
const A0: Reg = 10;
const A1: Reg = 11;

// Float register constants
const FA0: Reg = 10; // fa0

pub struct CompilerRv64 {
    emitter: AssemblyEmitter,
    data: DataSection,
    type_aliases: HashMap<String, IrType>,
    function_return_types: HashMap<String, IrType>,
}

impl CompilerRv64 {
    pub fn new() -> Self {
        Self {
            emitter: AssemblyEmitter::new(),
            data: DataSection::new(),
            type_aliases: HashMap::new(),
            function_return_types: HashMap::new(),
        }
    }

    pub fn compile(&mut self, program: &IrProgram) -> String {
        self.compile_inner(program);
        self.emitter.finish()
    }

    /// Compile and return both the text assembly and the structured token stream.
    pub fn compile_with_tokens(
        &mut self,
        program: &IrProgram,
    ) -> (
        String,
        Vec<crate::assembly_language::rv_instruction::RvInstruction>,
    ) {
        self.compile_inner(program);
        (self.emitter.finish(), self.emitter.finish_tokens())
    }

    fn compile_inner(&mut self, program: &IrProgram) {
        self.emitter.reset();
        self.data.reset();
        self.type_aliases.clear();
        self.function_return_types.clear();

        for s in &program.global_strings {
            self.data.add_global_string(s);
        }
        for alias in &program.type_aliases {
            self.type_aliases
                .insert(alias.name.clone(), alias.ty.clone());
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
    }

    fn compile_function(&mut self, func: &crate::intermediate_language::IrFunction) {
        let mut ctx = FunctionContext::new(&func.name, &self.type_aliases);
        let mut alloc = RegisterAllocator::new();
        alloc.allocate_slots(func, &mut ctx, &self.function_return_types);

        let return_type = self.resolve_ir_type(&func.return_type);
        let is_aggregate = matches!(return_type, IrType::Aggregate(_) | IrType::Array { .. });
        let needs_sret = is_aggregate && !self.can_return_in_registers(&return_type);

        let sret_slot = if needs_sret {
            ctx.save_reg(9); // s1 for sret pointer
            Some(ctx.frame.alloc_slot(8, 8))
        } else {
            None
        };

        ctx.save_ra();
        ctx.save_reg(S0);
        ctx.finalize();

        for block in &func.blocks {
            ctx.map_label(&block.label, format!("{}__{}", func.name, block.label.0));
        }

        self.emitter.start_function(&func.name);
        ctx.emit_prologue(&mut self.emitter);

        if needs_sret {
            self.emitter.emit_mv(9, A0);
        }

        if let Some(sret_slot) = sret_slot {
            ctx.emit_parameter_spills_with_sret(&mut self.emitter, func, sret_slot);
        } else {
            ctx.emit_parameter_spills(&mut self.emitter, func);
        }

        for block in &func.blocks {
            let label = ctx.get_label(&block.label).unwrap();
            self.emitter.emit_label(label);
            for inst in &block.instructions {
                self.lower_instruction(inst, &mut ctx);
            }
            if let Some(term) = &block.terminator {
                self.lower_terminator(term, &mut ctx, needs_sret, is_aggregate);
            }
        }

        self.emitter.end_function();
    }

    // ---------- instruction lowering ----------
    fn lower_instruction(&mut self, inst: &IrInstruction, ctx: &mut FunctionContext) {
        use IrInstruction::{
            Alloc, Call, Cast, Cmp, Comment, HeapAlloc, HeapFree, Index, Load, Math, Offset, Phi,
            Store, Unary,
        };
        match inst {
            Comment(s) => self.emitter.emit_comment(s),
            Alloc { .. } => {}
            Load {
                dest,
                ty,
                ptr,
                offset,
            } => {
                self.emitter.reset_temp_counter();
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                self.emitter
                    .emit_comment(&format!("Load {ty} from memory into ${dest}"));
                let ptr_tmp = self.load_pointer_operand_to_temp(ptr, ctx);
                let addr_tmp = if let Some(off) = offset {
                    let tmp = self.emitter.alloc_temp_reg();
                    self.emitter.emit_addi(tmp, ptr_tmp, *off as i32);
                    tmp
                } else {
                    ptr_tmp
                };
                let resolved_ty = self.resolve_ir_type(ty);
                if matches!(resolved_ty, IrType::Array { .. } | IrType::Aggregate(_)) {
                    self.emitter.copy_bytes_from_addr_to_slot(
                        dest_slot,
                        addr_tmp,
                        0,
                        self.type_size(&resolved_ty),
                    );
                } else {
                    self.emitter
                        .emit_load_to_slot(dest_slot, addr_tmp, &resolved_ty, 0);
                }
            }
            Store {
                ty,
                value,
                ptr,
                offset,
            } => {
                self.emitter.reset_temp_counter();
                let addr_tmp = self.resolve_ptr_to_addr(ptr, ctx, offset.map(|o| o as i32));
                let resolved_ty = self.resolve_ir_type(ty);
                self.emitter.emit_comment(&format!("Store {ty} to memory"));
                if matches!(resolved_ty, IrType::Array { .. } | IrType::Aggregate(_)) {
                    let IrValue::Register(reg) = value else {
                        unimplemented!("composite stores require a register source")
                    };
                    let val_slot = ctx.slot_for_reg(reg).expect("value slot");
                    self.emitter.copy_bytes_from_slot_to_addr(
                        val_slot,
                        addr_tmp,
                        0,
                        self.type_size(&resolved_ty),
                    );
                } else {
                    if matches!(resolved_ty, IrType::Float(_)) {
                        let val_fp = self.load_float_value_to_temp(value, ctx);
                        self.emitter
                            .emit_store_from_tmp(addr_tmp, val_fp, &resolved_ty, 0);
                    } else {
                        let val_tmp = self.load_value_to_temp(value, ctx);
                        self.emitter
                            .emit_store_from_tmp(addr_tmp, val_tmp, &resolved_ty, 0);
                    }
                }
            }
            Offset {
                dest,
                ty: _,
                ptr,
                bytes,
            } => {
                self.emitter.reset_temp_counter();
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                let ptr_tmp = self.load_pointer_operand_to_temp(ptr, ctx);
                let byte_val_reg = self.load_value_to_temp(bytes, ctx);
                let off_tmp = self.emitter.alloc_temp_reg();
                self.emitter.emit_mv(off_tmp, byte_val_reg);
                let result_tmp = self.emitter.alloc_temp_reg();
                self.emitter.emit_add(result_tmp, ptr_tmp, off_tmp);
                self.emitter.emit_sd(SP, result_tmp, dest_slot as i32);
            }
            Index {
                dest,
                ty,
                base_ptr,
                idx,
            } => {
                self.emitter.reset_temp_counter();
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                let base_tmp = self.load_pointer_operand_to_temp(base_ptr, ctx);
                let idx_tmp = self.load_value_to_temp(idx, ctx);
                let scale = self.type_size(ty);
                let scaled_tmp = self.emitter.alloc_temp_reg();
                if scale == 1 {
                    self.emitter.emit_mv(scaled_tmp, idx_tmp);
                } else {
                    self.emitter.emit_mul_imm(scaled_tmp, idx_tmp, scale as i32);
                }
                let result_tmp = self.emitter.alloc_temp_reg();
                self.emitter.emit_add(result_tmp, base_tmp, scaled_tmp);
                self.emitter.emit_sd(SP, result_tmp, dest_slot as i32);
            }
            Math {
                dest,
                op,
                ty,
                lhs,
                rhs,
            } => {
                self.emitter.reset_temp_counter();
                let resolved_ty = self.resolve_ir_type(ty);
                if matches!(resolved_ty, IrType::Aggregate(_) | IrType::Array { .. }) {
                    panic!(
                        "Math operations cannot be performed on aggregate/array type {resolved_ty:?}"
                    );
                }
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                self.emitter
                    .emit_comment(&format!("{op} operation on {ty}"));

                if matches!(
                    resolved_ty,
                    IrType::Float(crate::intermediate_language::FloatWidth::F32)
                ) {
                    let lhs_fp = self.load_float_value_to_temp(lhs, ctx);
                    let rhs_fp = self.load_float_value_to_temp(rhs, ctx);
                    let result_fp = self.emitter.alloc_float_temp_reg();
                    match op {
                        IrMathOp::Add => self.emitter.emit_fadd_s(result_fp, lhs_fp, rhs_fp),
                        IrMathOp::Sub => self.emitter.emit_fsub_s(result_fp, lhs_fp, rhs_fp),
                        IrMathOp::Mul => self.emitter.emit_fmul_s(result_fp, lhs_fp, rhs_fp),
                        IrMathOp::Div | IrMathOp::SDiv => {
                            self.emitter.emit_fdiv_s(result_fp, lhs_fp, rhs_fp);
                        }
                        _ => panic!("Unsupported float math op {op:?}"),
                    }
                    self.emitter.emit_fsw(SP, result_fp, dest_slot as i32);
                } else {
                    let lhs_tmp = self.load_value_to_temp(lhs, ctx);
                    let rhs_tmp = self.load_value_to_temp(rhs, ctx);
                    let result_tmp = self.emitter.alloc_temp_reg();
                    match op {
                        IrMathOp::Add => self.emitter.emit_add(result_tmp, lhs_tmp, rhs_tmp),
                        IrMathOp::Sub => self.emitter.emit_sub(result_tmp, lhs_tmp, rhs_tmp),
                        IrMathOp::Mul => self.emitter.emit_mul(result_tmp, lhs_tmp, rhs_tmp),
                        IrMathOp::Div => self.emitter.emit_div(result_tmp, lhs_tmp, rhs_tmp),
                        IrMathOp::SDiv => self.emitter.emit_div(result_tmp, lhs_tmp, rhs_tmp),
                        IrMathOp::Mod => self.emitter.emit_rem(result_tmp, lhs_tmp, rhs_tmp),
                        IrMathOp::Shl => self.emitter.emit_sll(result_tmp, lhs_tmp, rhs_tmp),
                        IrMathOp::Shr => self.emitter.emit_srl(result_tmp, lhs_tmp, rhs_tmp),
                        IrMathOp::And => self.emitter.emit_and(result_tmp, lhs_tmp, rhs_tmp),
                        IrMathOp::Or => self.emitter.emit_or(result_tmp, lhs_tmp, rhs_tmp),
                        IrMathOp::Xor => self.emitter.emit_xor(result_tmp, lhs_tmp, rhs_tmp),
                    }
                    self.emitter.emit_store_from_tmp(
                        SP,
                        result_tmp,
                        &resolved_ty,
                        dest_slot as i32,
                    );
                }
            }
            Unary {
                dest,
                op,
                ty,
                value,
            } => {
                self.emitter.reset_temp_counter();
                let resolved_ty = self.resolve_ir_type(ty);
                if matches!(resolved_ty, IrType::Aggregate(_) | IrType::Array { .. }) {
                    panic!(
                        "Unary operations cannot be performed on aggregate/array type {resolved_ty:?}"
                    );
                }
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                if matches!(
                    resolved_ty,
                    IrType::Float(crate::intermediate_language::FloatWidth::F32)
                ) {
                    let val_fp = self.load_float_value_to_temp(value, ctx);
                    let result_fp = self.emitter.alloc_float_temp_reg();
                    match op {
                        IrUnaryOp::Neg => {
                            self.emitter.emit_inst(RealInstruction::Fsgnjn(Fsgnjn::new(
                                result_fp, val_fp, val_fp,
                            )));
                        }
                        IrUnaryOp::Not => panic!("Bitwise not not supported for floats"),
                    }
                    self.emitter.emit_fsw(SP, result_fp, dest_slot as i32);
                } else {
                    let val_tmp = self.load_value_to_temp(value, ctx);
                    let result_tmp = self.emitter.alloc_temp_reg();
                    match op {
                        IrUnaryOp::Neg => self.emitter.emit_neg(result_tmp, val_tmp),
                        IrUnaryOp::Not => self.emitter.emit_not(result_tmp, val_tmp),
                    }
                    self.emitter.emit_store_from_tmp(
                        SP,
                        result_tmp,
                        &resolved_ty,
                        dest_slot as i32,
                    );
                }
            }
            Cmp {
                dest,
                op,
                ty,
                lhs,
                rhs,
            } => {
                self.emitter.reset_temp_counter();
                let resolved_ty = self.resolve_ir_type(ty);
                if matches!(resolved_ty, IrType::Aggregate(_) | IrType::Array { .. }) {
                    panic!(
                        "Comparison operations cannot be performed on aggregate/array type {resolved_ty:?}"
                    );
                }
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");

                if matches!(
                    resolved_ty,
                    IrType::Float(crate::intermediate_language::FloatWidth::F32)
                ) {
                    let lhs_fp = self.load_float_value_to_temp(lhs, ctx);
                    let rhs_fp = self.load_float_value_to_temp(rhs, ctx);
                    let result_tmp = self.emitter.alloc_temp_reg();
                    match op {
                        IrCmpOp::Eq => self.emitter.emit_feq_s(result_tmp, lhs_fp, rhs_fp),
                        IrCmpOp::Ne => {
                            let tmp = self.emitter.alloc_temp_reg();
                            self.emitter.emit_feq_s(tmp, lhs_fp, rhs_fp);
                            self.emitter.emit_not(result_tmp, tmp);
                        }
                        IrCmpOp::Slt | IrCmpOp::Ult => {
                            self.emitter.emit_flt_s(result_tmp, lhs_fp, rhs_fp);
                        }
                        IrCmpOp::Sle | IrCmpOp::Ule => {
                            self.emitter.emit_fle_s(result_tmp, lhs_fp, rhs_fp);
                        }
                        IrCmpOp::Sgt | IrCmpOp::Ugt => {
                            self.emitter.emit_flt_s(result_tmp, rhs_fp, lhs_fp);
                        }
                        IrCmpOp::Sge | IrCmpOp::Uge => {
                            self.emitter.emit_fle_s(result_tmp, rhs_fp, lhs_fp);
                        }
                    }
                    self.emitter.emit_store_from_tmp(
                        SP,
                        result_tmp,
                        &IrType::Integer(crate::intermediate_language::IntWidth::I1),
                        dest_slot as i32,
                    );
                } else {
                    let lhs_tmp = self.load_value_to_temp(lhs, ctx);
                    let rhs_tmp = self.load_value_to_temp(rhs, ctx);
                    let result_tmp = self.emitter.alloc_temp_reg();
                    match op {
                        IrCmpOp::Eq => self.emitter.emit_seq(result_tmp, lhs_tmp, rhs_tmp),
                        IrCmpOp::Ne => self.emitter.emit_sne(result_tmp, lhs_tmp, rhs_tmp),
                        IrCmpOp::Slt => self.emitter.emit_slt(result_tmp, lhs_tmp, rhs_tmp),
                        IrCmpOp::Ult => self.emitter.emit_sltu(result_tmp, lhs_tmp, rhs_tmp),
                        IrCmpOp::Sle => self.emitter.emit_cmp_sle(result_tmp, lhs_tmp, rhs_tmp),
                        IrCmpOp::Ule => self.emitter.emit_cmp_ule(result_tmp, lhs_tmp, rhs_tmp),
                        IrCmpOp::Sgt => self.emitter.emit_slt(result_tmp, rhs_tmp, lhs_tmp),
                        IrCmpOp::Ugt => self.emitter.emit_sltu(result_tmp, rhs_tmp, lhs_tmp),
                        IrCmpOp::Sge => self.emitter.emit_cmp_sge(result_tmp, lhs_tmp, rhs_tmp),
                        IrCmpOp::Uge => self.emitter.emit_cmp_uge(result_tmp, lhs_tmp, rhs_tmp),
                    }
                    self.emitter.emit_store_from_tmp(
                        SP,
                        result_tmp,
                        &IrType::Integer(crate::intermediate_language::IntWidth::I1),
                        dest_slot as i32,
                    );
                }
            }
            Cast {
                dest,
                mode,
                value,
                ty,
            } => {
                self.emitter.reset_temp_counter();
                let resolved_ty = self.resolve_ir_type(ty);
                if matches!(resolved_ty, IrType::Aggregate(_) | IrType::Array { .. }) {
                    panic!(
                        "Cast operations cannot be performed on aggregate/array type {resolved_ty:?}"
                    );
                }
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                let src_tmp = self.load_value_to_temp(value, ctx);
                let result_tmp = self.emitter.alloc_temp_reg();
                self.lower_cast(result_tmp, src_tmp, *mode, &resolved_ty);
                self.emitter
                    .emit_store_from_tmp(SP, result_tmp, &resolved_ty, dest_slot as i32);
            }
            Call {
                dest,
                function,
                args,
            } => {
                self.emitter.reset_temp_counter();
                let func_return_type = self
                    .function_return_types
                    .get(function)
                    .cloned()
                    .unwrap_or(IrType::Integer(crate::intermediate_language::IntWidth::I64));
                let resolved_ret_ty = self.resolve_ir_type(&func_return_type);
                let is_agg_return =
                    matches!(resolved_ret_ty, IrType::Aggregate(_) | IrType::Array { .. });
                let needs_sret = is_agg_return && !self.can_return_in_registers(&resolved_ret_ty);

                self.emitter
                    .emit_comment(&format!("--- Function Call: {function} ---"));
                if needs_sret {
                    self.emitter
                        .emit_comment("Using sret convention for large aggregate return");
                }

                let mut arg_index = 0;
                if needs_sret {
                    if let Some(dest_reg) = dest {
                        let dest_slot = ctx.slot_for_reg(dest_reg).expect("dest slot for sret");
                        let sret_ptr = self.emitter.alloc_temp_reg();
                        self.emitter.emit_add_imm(sret_ptr, SP, dest_slot as i64);
                        self.emitter.emit_mv(reg_for_arg(0), sret_ptr);
                        arg_index = 1;
                    } else {
                        panic!("Call with aggregate return must have a destination");
                    }
                }

                self.emitter
                    .emit_comment(&format!("Passing {} arguments", args.len()));
                for arg in args {
                    if arg_index >= 8 {
                        break;
                    }
                    let arg_tmp = self.load_value_to_temp(arg, ctx);
                    self.emitter.emit_mv(reg_for_arg(arg_index), arg_tmp);
                    arg_index += 1;
                }

                self.emitter.emit_jal(RA, function.as_str());

                if is_agg_return && !needs_sret {
                    self.emitter
                        .emit_comment("Unpacking small aggregate return from a0/a1");
                    if let Some(dest_reg) = dest {
                        let dest_slot = ctx.slot_for_reg(dest_reg).expect("dest slot");
                        if let IrType::Aggregate(fields) = &resolved_ret_ty {
                            let mut field_offset = 0;
                            for (i, (_, field_ty)) in fields.iter().enumerate() {
                                let resolved_field_ty = self.resolve_ir_type(field_ty);
                                let field_size = self.type_size(&resolved_field_ty);
                                let reg = if i == 0 { A0 } else { 11 }; // a0 or a1
                                if field_size >= 8 {
                                    self.emitter.emit_sd(
                                        SP,
                                        reg,
                                        (dest_slot + field_offset) as i32,
                                    );
                                } else if field_size >= 4 {
                                    self.emitter.emit_inst(RealInstruction::Sw(Sw::new(
                                        SP,
                                        reg,
                                        (dest_slot + field_offset) as i32,
                                    )));
                                } else if field_size >= 2 {
                                    self.emitter.emit_inst(RealInstruction::Sh(Sh::new(
                                        SP,
                                        reg,
                                        (dest_slot + field_offset) as i32,
                                    )));
                                } else if field_size >= 1 {
                                    self.emitter.emit_inst(RealInstruction::Sb(Sb::new(
                                        SP,
                                        reg,
                                        (dest_slot + field_offset) as i32,
                                    )));
                                }
                                let align = self.type_alignment(&resolved_field_ty);
                                field_offset =
                                    ((field_offset + align - 1) / align * align) + field_size;
                            }
                        } else {
                            let size = self.type_size(&resolved_ret_ty);
                            if size >= 8 {
                                self.emitter.emit_sd(SP, A0, dest_slot as i32);
                            } else if size >= 4 {
                                self.emitter.emit_inst(RealInstruction::Sw(Sw::new(
                                    SP,
                                    A0,
                                    dest_slot as i32,
                                )));
                            }
                        }
                    }
                } else if !is_agg_return {
                    if let Some(dest) = dest {
                        let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                        let resolved_return_ty = self.resolve_ir_type(&func_return_type);
                        self.emitter.emit_store_from_tmp(
                            SP,
                            A0,
                            &resolved_return_ty,
                            dest_slot as i32,
                        );
                    }
                }
                self.emitter
                    .emit_comment(&format!("--- End Function Call: {function} ---"));
            }
            Phi { .. } => {}
            HeapAlloc { dest, ty, count } => {
                self.emitter.reset_temp_counter();
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                let elem_count = count.unwrap_or(1);
                let bytes = self.type_size(ty).saturating_mul(elem_count);

                self.emitter.emit_li(A0, bytes as i64);
                self.emitter.emit_raw("\tcall malloc");
                self.emitter.emit_sd(SP, A0, dest_slot as i32);
            }
            HeapFree { ptr } => {
                self.emitter.reset_temp_counter();
                let ptr_tmp = self.load_value_to_temp(&IrValue::Register(ptr.clone()), ctx);
                self.emitter.emit_mv(A0, ptr_tmp);
                self.emitter.emit_raw("\tcall free");
            }
        }
    }

    fn lower_terminator(
        &mut self,
        term: &IrTerminator,
        ctx: &mut FunctionContext,
        needs_sret: bool,
        _is_aggregate: bool,
    ) {
        match term {
            IrTerminator::Return(val) => {
                if let Some(val) = val {
                    let raw_val_type = self.resolve_value_type(val, ctx);
                    let resolved_val = match (&raw_val_type, val) {
                        (IrType::Pointer(inner), IrValue::Register(reg))
                            if ctx.is_stack_address(reg) =>
                        {
                            self.resolve_ir_type(inner)
                        }
                        _ => raw_val_type.clone(),
                    };
                    let is_agg_return =
                        matches!(resolved_val, IrType::Aggregate(_) | IrType::Array { .. });

                    if needs_sret && is_agg_return {
                        match val {
                            IrValue::Register(reg) => {
                                let src_addr = if ctx.is_stack_address(reg) {
                                    let slot = ctx.slot_for_reg(reg).expect("reg slot");
                                    let addr_tmp = self.emitter.alloc_temp_reg();
                                    self.emitter.emit_add_imm(addr_tmp, SP, slot as i64);
                                    addr_tmp
                                } else {
                                    self.load_pointer_operand_to_temp(reg, ctx)
                                };
                                let sret_ptr = 9; // s1
                                let size = self.type_size(&resolved_val);
                                self.emitter
                                    .copy_bytes_from_addr_to_addr(sret_ptr, 0, src_addr, 0, size);
                            }
                            _ => panic!("Aggregate return must be a register"),
                        }
                    } else if is_agg_return && !needs_sret {
                        match val {
                            IrValue::Register(reg) => {
                                let slot = ctx.slot_for_reg(reg).expect("reg slot");
                                if let IrType::Aggregate(fields) = &resolved_val {
                                    let mut field_offset = 0;
                                    for (i, (_, field_ty)) in fields.iter().enumerate() {
                                        let resolved_field_ty = self.resolve_ir_type(field_ty);
                                        let field_size = self.type_size(&resolved_field_ty);
                                        let reg = if i == 0 { A0 } else { A1 };
                                        if field_size >= 8 {
                                            self.emitter.emit_ld(
                                                reg,
                                                SP,
                                                (slot + field_offset) as i32,
                                            );
                                        } else if field_size >= 4 {
                                            self.emitter.emit_lw(
                                                reg,
                                                SP,
                                                (slot + field_offset) as i32,
                                            );
                                        } else if field_size >= 2 {
                                            self.emitter.emit_lh(
                                                reg,
                                                SP,
                                                (slot + field_offset) as i32,
                                            );
                                        } else if field_size >= 1 {
                                            self.emitter.emit_lb(
                                                reg,
                                                SP,
                                                (slot + field_offset) as i32,
                                            );
                                        }
                                        let align = self.type_alignment(&resolved_field_ty);
                                        field_offset = ((field_offset + align - 1) / align * align)
                                            + field_size;
                                    }
                                } else {
                                    let size = self.type_size(&resolved_val);
                                    if size >= 8 {
                                        self.emitter.emit_ld(A0, SP, slot as i32);
                                    } else if size >= 4 {
                                        self.emitter.emit_lw(A0, SP, slot as i32);
                                    }
                                }
                            }
                            _ => panic!("Small aggregate return must be a register"),
                        }
                    } else {
                        let resolved_val = self.resolve_value_type(val, ctx);
                        if matches!(resolved_val, IrType::Float(_)) {
                            let val_fp = self.load_float_value_to_temp(val, ctx);
                            match resolved_val {
                                IrType::Float(crate::intermediate_language::FloatWidth::F32) => {
                                    self.emitter.emit_fmv_s(FA0, val_fp);
                                }
                                IrType::Float(crate::intermediate_language::FloatWidth::F64) => {
                                    self.emitter
                                        .emit_inst(RealInstruction::FsgnjD(fmv_d(FA0, val_fp)));
                                }
                                _ => unreachable!(),
                            }
                        } else {
                            let val_tmp = self.load_value_to_temp(val, ctx);
                            self.emitter.emit_mv(A0, val_tmp);
                        }
                    }
                }
                ctx.emit_epilogue(&mut self.emitter);
            }
            IrTerminator::Jump(label) => {
                let lbl = ctx.get_label(label).unwrap();
                self.emitter.emit_jal(ZERO, lbl);
            }
            IrTerminator::Branch {
                cond,
                then_label,
                else_label,
            } => {
                let cond_tmp = self.load_value_to_temp(cond, ctx);
                let then_lbl = ctx.get_label(then_label).unwrap();
                let else_lbl = ctx.get_label(else_label).unwrap();
                self.emitter.emit_bne(cond_tmp, ZERO, then_lbl);
                self.emitter.emit_jal(ZERO, else_lbl);
            }
        }
    }

    // ---------- helpers that remain in the compiler (use emitter only) ----------
    fn resolve_ptr_to_addr(
        &mut self,
        ptr: &crate::intermediate_language::IrRegister,
        ctx: &FunctionContext,
        byte_offset: Option<i32>,
    ) -> Reg {
        let slot = ctx.slot_for_reg(ptr).expect("ptr slot");
        let tmp = self.emitter.alloc_temp_reg();

        if ctx.is_stack_address(ptr) {
            let total_offset = slot as i64 + byte_offset.unwrap_or(0) as i64;
            self.emitter.emit_add_imm(tmp, SP, total_offset);
        } else {
            self.emitter.emit_ld(tmp, SP, slot as i32);
            if let Some(off) = byte_offset {
                if off != 0 {
                    self.emitter.emit_add_imm(tmp, tmp, off as i64);
                }
            }
        }
        tmp
    }

    fn load_value_to_temp(&mut self, val: &IrValue, ctx: &FunctionContext) -> Reg {
        let temp = self.emitter.alloc_temp_reg();
        match val {
            IrValue::Register(reg) => {
                let slot = ctx.slot_for_reg(reg).expect("reg slot");
                if ctx.is_stack_address(reg) {
                    self.emitter.emit_add_imm(temp, SP, slot as i64);
                } else {
                    let ty = ctx
                        .type_for_reg(reg)
                        .unwrap_or(IrType::Integer(crate::intermediate_language::IntWidth::I64));
                    self.emitter.emit_load_from_slot(temp, slot, &ty);
                }
            }
            IrValue::Integer(i) => self.emitter.emit_li(temp, *i),
            IrValue::Bool(b) => self.emitter.emit_li(temp, i64::from(*b)),
            IrValue::Float(_) => panic!("Float values must use load_float_value_to_temp"),
            IrValue::Null => self.emitter.emit_li(temp, 0),
            IrValue::GlobalString(symbol) => {
                self.emitter
                    .emit_raw(&format!("\tla {}, {}", reg_name(temp, false), symbol));
            }
        }
        temp
    }

    fn load_float_value_to_temp(&mut self, val: &IrValue, ctx: &FunctionContext) -> Reg {
        let temp = self.emitter.alloc_float_temp_reg();
        match val {
            IrValue::Register(reg) => {
                let slot = ctx.slot_for_reg(reg).expect("reg slot");
                let ty = ctx
                    .type_for_reg(reg)
                    .unwrap_or(IrType::Float(crate::intermediate_language::FloatWidth::F32));
                match ty {
                    IrType::Float(crate::intermediate_language::FloatWidth::F32) => {
                        self.emitter.emit_flw(temp, SP, slot as i32);
                    }
                    IrType::Float(crate::intermediate_language::FloatWidth::F64) => {
                        self.emitter.emit_fld(temp, SP, slot as i32);
                    }
                    _ => panic!("Expected float type for float register load"),
                }
            }
            IrValue::Float(f) => {
                let int_tmp = self.emitter.alloc_temp_reg();
                self.emitter.emit_li(int_tmp, f.to_bits() as i64);
                self.emitter.emit_fmv_w_x(temp, int_tmp);
            }
            _ => panic!("Unsupported float value: {val:?}"),
        }
        temp
    }

    fn load_pointer_operand_to_temp(
        &mut self,
        reg: &crate::intermediate_language::IrRegister,
        ctx: &FunctionContext,
    ) -> Reg {
        let temp = self.emitter.alloc_temp_reg();
        let slot = ctx.slot_for_reg(reg).expect("reg slot");
        if ctx.is_stack_address(reg) {
            self.emitter.emit_add_imm(temp, SP, slot as i64);
        } else {
            self.emitter.emit_ld(temp, SP, slot as i32);
        }
        temp
    }

    fn lower_cast(&mut self, rd: Reg, rs: Reg, mode: IrCastMode, ty: &IrType) {
        match mode {
            IrCastMode::Bitcast | IrCastMode::Trunc | IrCastMode::Zext => {
                self.emitter.emit_mv(rd, rs);
            }
            IrCastMode::Sext => match ty {
                IrType::Integer(crate::intermediate_language::IntWidth::I32) => {
                    self.emitter.emit_addiw(rd, rs, 0);
                }
                IrType::Integer(crate::intermediate_language::IntWidth::I64)
                | IrType::Pointer(_) => self.emitter.emit_mv(rd, rs),
                IrType::Integer(crate::intermediate_language::IntWidth::I16) => {
                    self.emitter.emit_slli(rd, rs, 48);
                    self.emitter.emit_srai(rd, rd, 48);
                }
                IrType::Integer(crate::intermediate_language::IntWidth::I8) => {
                    self.emitter.emit_slli(rd, rs, 56);
                    self.emitter.emit_srai(rd, rd, 56);
                }
                _ => self.emitter.emit_mv(rd, rs),
            },
            IrCastMode::F2i | IrCastMode::I2f => self.emitter.emit_mv(rd, rs),
        }
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

    fn resolve_value_type(&self, val: &IrValue, ctx: &FunctionContext) -> IrType {
        match val {
            IrValue::Register(reg) => ctx
                .type_for_reg(reg)
                .unwrap_or(IrType::Integer(crate::intermediate_language::IntWidth::I64)),
            IrValue::Integer(_) => IrType::Integer(crate::intermediate_language::IntWidth::I64),
            IrValue::Bool(_) => IrType::Integer(crate::intermediate_language::IntWidth::I1),
            IrValue::Float(_) => IrType::Float(crate::intermediate_language::FloatWidth::F64),
            IrValue::Null => IrType::Pointer(Box::new(IrType::Void)),
            IrValue::GlobalString(_) => IrType::Pointer(Box::new(IrType::Integer(
                crate::intermediate_language::IntWidth::I8,
            ))),
        }
    }

    fn type_alignment(&self, ty: &IrType) -> usize {
        match self.resolve_ir_type(ty) {
            IrType::Void => 1,
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
            IrType::Pointer(_) | IrType::Named(_) => 8,
            IrType::Array { element, .. } => self.type_alignment(&element),
            IrType::Aggregate(fields) => fields
                .iter()
                .map(|(_, ft)| self.type_alignment(ft))
                .max()
                .unwrap_or(1),
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
            IrType::Aggregate(fields) => {
                let mut offset: usize = 0;
                let mut max_align: usize = 1;
                for (_, field_ty) in &fields {
                    let align = self.type_alignment(field_ty);
                    max_align = max_align.max(align);
                    offset = (offset + align - 1) & !(align - 1);
                    offset += self.type_size(field_ty);
                }
                (offset + max_align - 1) & !(max_align - 1)
            }
            IrType::Named(_) => {
                warn!("Cannot compute size of unresolved named type; defaulting to 8");
                8
            }
        }
    }

    fn can_return_in_registers(&self, ty: &IrType) -> bool {
        let size = self.type_size(ty);
        size <= 16 && size > 0
    }
}

fn reg_for_arg(i: usize) -> Reg {
    match i {
        0 => 10,
        1 => 11,
        2 => 12,
        3 => 13,
        4 => 14,
        5 => 15,
        6 => 16,
        7 => 17,
        _ => 0,
    }
}
