use super::{
    assembly_emitter::AssemblyEmitter, data_section::DataSection,
    function_context::FunctionContext, peephole, stack_slots, type_utils,
};
use asm_to_binary::encode_decode::Reg;
use asm_to_binary::real::RealInstruction;
use asm_to_binary::riscv::rv64fd::{
    FcvtDL, FcvtDS, FcvtDW, FcvtLD, FcvtLS, FcvtSD, FcvtSL, FcvtSW, FcvtWD, FcvtWS, Fsgnjn,
    FsgnjnD, fmv_d,
};
use asm_to_binary::riscv::rv64i::{Lb, Ld, Lh, Lw, Sb, Sh, Sw};
use asm_to_binary::utils::reg_name;
use hll_to_ir::{
    IrCastMode, IrCmpOp, IrInstruction, IrMathOp, IrProgram, IrTerminator, IrType, IrUnaryOp,
    IrValue,
};
use std::collections::HashMap;

const ZERO: Reg = 0;
const RA: Reg = 1;
const SP: Reg = 2;
const S0: Reg = 8;
const A0: Reg = 10;
const A1: Reg = 11;
const FA0: Reg = 10;

pub struct CompilerRv64 {
    emitter: AssemblyEmitter,
    data: DataSection,
    type_aliases: HashMap<String, IrType>,
    function_return_types: HashMap<String, IrType>,
    peephole: bool,
    regalloc: bool,
    omit_frame_pointer: bool,
}

impl Default for CompilerRv64 {
    fn default() -> Self {
        Self::new()
    }
}

impl CompilerRv64 {
    pub fn new() -> Self {
        Self {
            emitter: AssemblyEmitter::new(),
            data: DataSection::new(),
            type_aliases: HashMap::new(),
            function_return_types: HashMap::new(),
            peephole: false,
            regalloc: false,
            omit_frame_pointer: false,
        }
    }

    pub fn set_peephole(&mut self, enabled: bool) {
        self.peephole = enabled;
    }

    pub fn set_register_allocation(&mut self, enabled: bool) {
        self.regalloc = enabled;
    }

    /// Omit the redundant frame pointer (s0); locals are addressed via sp.
    pub fn set_omit_frame_pointer(&mut self, enabled: bool) {
        self.omit_frame_pointer = enabled;
    }

    pub fn compile(&mut self, program: &IrProgram) -> String {
        self.compile_with_tokens(program).0
    }

    /// Compile and return both the text assembly and the structured token stream.
    pub fn compile_with_tokens(
        &mut self,
        program: &IrProgram,
    ) -> (String, Vec<asm_to_binary::rv_instruction::RvInstruction>) {
        self.compile_inner(program);
        if self.peephole {
            // The peephole runs on the token stream the assembler consumes; render
            // the text from the optimized tokens so the `.s` view matches the
            // bytes that will actually be assembled.
            let tokens = peephole::optimize(&self.emitter.finish_tokens());
            let text = tokens
                .iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            (text, tokens)
        } else {
            (self.emitter.finish(), self.emitter.finish_tokens())
        }
    }

    fn compile_inner(&mut self, program: &IrProgram) {
        self.emitter.reset();
        self.data.reset();
        self.type_aliases.clear();
        self.function_return_types.clear();

        for s in &program.global_strings {
            self.data.add_global_string(s);
        }
        for gv in &program.global_vars {
            let size = self.type_size(&gv.ty).max(1);
            let align = size.min(8);
            match &gv.init {
                None => self.data.add_bss_symbol(&gv.name, size, align),
                Some(bytes) => self.data.add_data_symbol(&gv.name, size, align, bytes),
            }
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

    fn compile_function(&mut self, func: &hll_to_ir::IrFunction) {
        let return_type = self.resolve_ir_type(&func.return_type);
        let is_aggregate = matches!(
            return_type,
            IrType::Aggregate(_) | IrType::Array { .. } | IrType::Slice(_)
        );
        let needs_sret = is_aggregate && !self.can_return_in_registers(&return_type);

        let mut ctx = FunctionContext::new(&self.type_aliases);
        ctx.set_omit_frame_pointer(self.omit_frame_pointer);
        stack_slots::assign_stack_slots(
            func,
            &mut ctx,
            &self.function_return_types,
            self.regalloc,
            needs_sret,
        );

        let sret_slot = if needs_sret {
            ctx.save_reg(9); // s1 holds the sret pointer.
            Some(ctx.frame.alloc_slot(8, 8))
        } else {
            None
        };

        ctx.save_ra();
        if !self.omit_frame_pointer {
            ctx.save_reg(S0);
        }
        ctx.finalize();

        for (index, param) in func.params.iter().enumerate() {
            ctx.set_param_index(&param.register, index + usize::from(needs_sret));
        }

        for block in &func.blocks {
            ctx.map_label(&block.label, format!("{}__{}", func.name, block.label.0));
        }

        self.emitter.start_function(&func.name);
        ctx.emit_prologue(&mut self.emitter);

        if needs_sret {
            self.emitter.emit_mv(9, A0);
        }

        // Inline-asm-only functions keep their params in a0-a7 / fa0-fa7 for the
        // asm to read, so the normal spills are skipped.
        let has_inline_asm = func.blocks.iter().any(|block| {
            block
                .instructions
                .iter()
                .any(|inst| matches!(inst, IrInstruction::InlineAsm { .. }))
        });
        let is_asm_only = has_inline_asm
            && func.blocks.iter().all(|block| {
                block.instructions.iter().all(|inst| {
                    matches!(
                        inst,
                        IrInstruction::InlineAsm { .. }
                            | IrInstruction::Comment(_)
                            | IrInstruction::Alloc { .. }
                            | IrInstruction::Store { .. }
                            | IrInstruction::Phi { .. }
                    )
                })
            });

        ctx.set_preserve_param_registers(is_asm_only);

        if is_asm_only {
            if let Some(sret_slot) = sret_slot {
                ctx.emit_parameter_spills_with_sret_for_inline_asm(
                    &mut self.emitter,
                    func,
                    sret_slot,
                );
            } else {
                ctx.emit_parameter_spills_for_inline_asm(&mut self.emitter, func);
            }
        } else if let Some(sret_slot) = sret_slot {
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

    fn lower_instruction(&mut self, inst: &IrInstruction, ctx: &mut FunctionContext) {
        use IrInstruction::{
            Alloc, Call, Cast, Cmp, Comment, GlobalRef, HeapAlloc, HeapFree, Index, InlineAsm,
            Load, Math, Offset, Phi, ReadReg, Store, Unary,
        };
        match inst {
            Comment(s) => self.emitter.emit_comment(s),
            Alloc { .. } | Phi { .. } => {}
            InlineAsm { lines } => {
                for line in lines {
                    self.emitter.emit_raw(&format!("\t{line}"));
                }
            }
            ReadReg { dest, reg } => self.lower_read_reg(dest, reg, ctx),
            GlobalRef { dest, name } => self.lower_global_ref(dest, name, ctx),
            Load {
                dest,
                ty,
                ptr,
                offset,
            } => self.lower_load(dest, ty, ptr, *offset, ctx),
            Store {
                ty,
                value,
                ptr,
                offset,
            } => self.lower_store(ty, value, ptr, *offset, ctx),
            Offset {
                dest, ptr, bytes, ..
            } => self.lower_offset(dest, ptr, bytes, ctx),
            Index {
                dest,
                ty,
                base_ptr,
                idx,
            } => self.lower_index(dest, ty, base_ptr, idx, ctx),
            Math {
                dest,
                op,
                ty,
                lhs,
                rhs,
            } => self.lower_math(dest, op, ty, lhs, rhs, ctx),
            Unary {
                dest,
                op,
                ty,
                value,
            } => self.lower_unary(dest, op, ty, value, ctx),
            Cmp {
                dest,
                op,
                ty,
                lhs,
                rhs,
            } => self.lower_cmp(dest, op, ty, lhs, rhs, ctx),
            Cast {
                dest,
                mode,
                value,
                ty,
            } => self.lower_cast_inst(dest, *mode, value, ty, ctx),
            Call {
                dest,
                function,
                args,
            } => self.lower_call(dest, function, args, ctx),
            HeapAlloc { dest, ty, count } => self.lower_heap_alloc(dest, ty, count, ctx),
            HeapFree { ptr } => self.lower_heap_free(ptr, ctx),
        }
    }

    fn lower_load(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        ty: &IrType,
        ptr: &hll_to_ir::IrRegister,
        offset: Option<i64>,
        ctx: &mut FunctionContext,
    ) {
        self.emitter.reset_temp_counter();
        self.emitter
            .emit_comment(&format!("Load {ty} from memory into ${dest}"));
        let ptr_tmp = self.load_pointer_operand_to_temp(ptr, ctx);
        let addr_tmp = if let Some(off) = offset {
            let tmp = self.emitter.alloc_temp_reg();
            self.emitter.emit_addi(tmp, ptr_tmp, off as i32);
            tmp
        } else {
            ptr_tmp
        };
        let resolved_ty = self.resolve_ir_type(ty);
        if matches!(
            resolved_ty,
            IrType::Array { .. } | IrType::Aggregate(_) | IrType::Slice(_)
        ) {
            let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
            self.emitter.copy_bytes_from_addr_to_slot(
                dest_slot,
                addr_tmp,
                0,
                self.type_size(&resolved_ty),
            );
        } else if let IrType::Float(width) = resolved_ty {
            // Floats must load into an FP register and store back with fsw/fsd;
            // routing through a GP temp would reinterpret the bit pattern.
            let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
            let fp_tmp = self.emitter.alloc_float_temp_reg();
            match width {
                hll_to_ir::FloatWidth::F32 => {
                    self.emitter.emit_flw(fp_tmp, addr_tmp, 0);
                    self.emitter.emit_fsw(SP, fp_tmp, dest_slot as i32);
                }
                hll_to_ir::FloatWidth::F64 => {
                    self.emitter.emit_fld(fp_tmp, addr_tmp, 0);
                    self.emitter.emit_fsd(SP, fp_tmp, dest_slot as i32);
                }
            }
        } else {
            // A register-resident dest takes the typed load directly (the load
            // already sign-extends to width); slot dests go through a temp.
            let dest_phys = ctx.phys_reg_for(dest);
            let loaded_val = dest_phys.unwrap_or_else(|| self.emitter.alloc_temp_reg());
            match &resolved_ty {
                IrType::Integer(w) => match w {
                    hll_to_ir::IntWidth::I1 | hll_to_ir::IntWidth::I8 => {
                        self.emitter
                            .emit_inst(RealInstruction::Lb(Lb::new(loaded_val, addr_tmp, 0)));
                    }
                    hll_to_ir::IntWidth::I16 => {
                        self.emitter
                            .emit_inst(RealInstruction::Lh(Lh::new(loaded_val, addr_tmp, 0)));
                    }
                    hll_to_ir::IntWidth::I32 => {
                        self.emitter
                            .emit_inst(RealInstruction::Lw(Lw::new(loaded_val, addr_tmp, 0)));
                    }
                    hll_to_ir::IntWidth::I64 => {
                        self.emitter
                            .emit_inst(RealInstruction::Ld(Ld::new(loaded_val, addr_tmp, 0)));
                    }
                },
                IrType::Pointer(_) => {
                    self.emitter
                        .emit_inst(RealInstruction::Ld(Ld::new(loaded_val, addr_tmp, 0)));
                }
                _ => {
                    self.emitter
                        .emit_inst(RealInstruction::Ld(Ld::new(loaded_val, addr_tmp, 0)));
                }
            }
            if dest_phys.is_none() {
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                self.emitter
                    .emit_store_from_tmp(SP, loaded_val, &resolved_ty, dest_slot as i32);
            }
        }
    }

    fn lower_store(
        &mut self,
        ty: &IrType,
        value: &IrValue,
        ptr: &hll_to_ir::IrRegister,
        offset: Option<i64>,
        ctx: &mut FunctionContext,
    ) {
        self.emitter.reset_temp_counter();
        let addr_tmp = self.resolve_ptr_to_addr(ptr, ctx, offset.map(|o| o as i32));
        let resolved_ty = self.resolve_ir_type(ty);
        self.emitter.emit_comment(&format!("Store {ty} to memory"));
        if matches!(
            resolved_ty,
            IrType::Array { .. } | IrType::Aggregate(_) | IrType::Slice(_)
        ) {
            let IrValue::Register(reg) = value else {
                unreachable!("IR invariant: composite/array stores always have a register source")
            };
            let val_slot = ctx.slot_for_reg(reg).expect("value slot");
            self.emitter.copy_bytes_from_slot_to_addr(
                val_slot,
                addr_tmp,
                0,
                self.type_size(&resolved_ty),
            );
        } else if matches!(resolved_ty, IrType::Float(_)) {
            let val_fp = self.load_float_value_to_temp(value, &resolved_ty, ctx);
            self.emitter
                .emit_store_from_tmp(addr_tmp, val_fp, &resolved_ty, 0);
        } else {
            let val_tmp = self.load_value_to_temp(value, ctx);
            self.emitter
                .emit_store_from_tmp(addr_tmp, val_tmp, &resolved_ty, 0);
        }
    }

    fn lower_offset(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        ptr: &hll_to_ir::IrRegister,
        bytes: &IrValue,
        ctx: &mut FunctionContext,
    ) {
        self.emitter.reset_temp_counter();
        let ptr_tmp = self.load_pointer_operand_to_temp(ptr, ctx);
        let byte_val_reg = self.load_value_to_temp(bytes, ctx);
        let off_tmp = self.emitter.alloc_temp_reg();
        self.emitter.emit_mv(off_tmp, byte_val_reg);
        let result_tmp = self.result_reg_for(dest, ctx);
        self.emitter.emit_add(result_tmp, ptr_tmp, off_tmp);
        let ptr_ty = IrType::Pointer(Box::new(IrType::Void));
        self.commit_int_result(dest, result_tmp, &ptr_ty, ctx);
    }

    fn lower_index(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        ty: &IrType,
        base_ptr: &hll_to_ir::IrRegister,
        idx: &IrValue,
        ctx: &mut FunctionContext,
    ) {
        self.emitter.reset_temp_counter();
        let base_tmp = self.load_pointer_operand_to_temp(base_ptr, ctx);
        let idx_tmp = self.load_value_to_temp(idx, ctx);
        let scale = self.type_size(ty);
        let scaled_tmp = self.emitter.alloc_temp_reg();
        if scale == 1 {
            self.emitter.emit_mv(scaled_tmp, idx_tmp);
        } else {
            self.emitter.emit_mul_imm(scaled_tmp, idx_tmp, scale as i32);
        }
        let result_tmp = self.result_reg_for(dest, ctx);
        self.emitter.emit_add(result_tmp, base_tmp, scaled_tmp);
        let ptr_ty = IrType::Pointer(Box::new(IrType::Void));
        self.commit_int_result(dest, result_tmp, &ptr_ty, ctx);
    }

    fn lower_math(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        op: &IrMathOp,
        ty: &IrType,
        lhs: &IrValue,
        rhs: &IrValue,
        ctx: &mut FunctionContext,
    ) {
        self.emitter.reset_temp_counter();
        let resolved_ty = self.resolve_ir_type(ty);
        if matches!(
            resolved_ty,
            IrType::Aggregate(_) | IrType::Array { .. } | IrType::Slice(_)
        ) {
            panic!("Math operations cannot be performed on aggregate/array type {resolved_ty:?}");
        }
        self.emitter
            .emit_comment(&format!("{op} operation on {ty}"));
        if let IrType::Float(width) = resolved_ty {
            let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
            let is_f64 = matches!(width, hll_to_ir::FloatWidth::F64);
            let lhs_fp = self.load_float_value_to_temp(lhs, &resolved_ty, ctx);
            let rhs_fp = self.load_float_value_to_temp(rhs, &resolved_ty, ctx);
            let result_fp = self.emitter.alloc_float_temp_reg();
            match op {
                IrMathOp::Add if is_f64 => self.emitter.emit_fadd_d(result_fp, lhs_fp, rhs_fp),
                IrMathOp::Sub if is_f64 => self.emitter.emit_fsub_d(result_fp, lhs_fp, rhs_fp),
                IrMathOp::Mul if is_f64 => self.emitter.emit_fmul_d(result_fp, lhs_fp, rhs_fp),
                IrMathOp::Div | IrMathOp::SDiv if is_f64 => {
                    self.emitter.emit_fdiv_d(result_fp, lhs_fp, rhs_fp);
                }
                IrMathOp::Add => self.emitter.emit_fadd_s(result_fp, lhs_fp, rhs_fp),
                IrMathOp::Sub => self.emitter.emit_fsub_s(result_fp, lhs_fp, rhs_fp),
                IrMathOp::Mul => self.emitter.emit_fmul_s(result_fp, lhs_fp, rhs_fp),
                IrMathOp::Div | IrMathOp::SDiv => {
                    self.emitter.emit_fdiv_s(result_fp, lhs_fp, rhs_fp);
                }
                _ => panic!("Unsupported float math op {op:?}"),
            }
            if is_f64 {
                self.emitter.emit_fsd(SP, result_fp, dest_slot as i32);
            } else {
                self.emitter.emit_fsw(SP, result_fp, dest_slot as i32);
            }
        } else {
            let lhs_tmp = self.load_value_to_temp(lhs, ctx);
            let rhs_tmp = self.load_value_to_temp(rhs, ctx);
            let result_tmp = self.result_reg_for(dest, ctx);
            match op {
                IrMathOp::Add => self.emitter.emit_add(result_tmp, lhs_tmp, rhs_tmp),
                IrMathOp::Sub => self.emitter.emit_sub(result_tmp, lhs_tmp, rhs_tmp),
                IrMathOp::Mul => self.emitter.emit_mul(result_tmp, lhs_tmp, rhs_tmp),
                IrMathOp::Div | IrMathOp::SDiv => {
                    self.emitter.emit_div(result_tmp, lhs_tmp, rhs_tmp);
                }
                IrMathOp::Mod => self.emitter.emit_rem(result_tmp, lhs_tmp, rhs_tmp),
                IrMathOp::UDiv => self.emitter.emit_divu(result_tmp, lhs_tmp, rhs_tmp),
                IrMathOp::UMod => self.emitter.emit_remu(result_tmp, lhs_tmp, rhs_tmp),
                IrMathOp::Shl => self.emitter.emit_sll(result_tmp, lhs_tmp, rhs_tmp),
                IrMathOp::Shr => self.emitter.emit_srl(result_tmp, lhs_tmp, rhs_tmp),
                IrMathOp::And => self.emitter.emit_and(result_tmp, lhs_tmp, rhs_tmp),
                IrMathOp::Or => self.emitter.emit_or(result_tmp, lhs_tmp, rhs_tmp),
                IrMathOp::Xor => self.emitter.emit_xor(result_tmp, lhs_tmp, rhs_tmp),
            }
            self.commit_int_result(dest, result_tmp, &resolved_ty, ctx);
        }
    }

    fn lower_unary(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        op: &IrUnaryOp,
        ty: &IrType,
        value: &IrValue,
        ctx: &mut FunctionContext,
    ) {
        self.emitter.reset_temp_counter();
        let resolved_ty = self.resolve_ir_type(ty);
        if matches!(
            resolved_ty,
            IrType::Aggregate(_) | IrType::Array { .. } | IrType::Slice(_)
        ) {
            panic!("Unary operations cannot be performed on aggregate/array type {resolved_ty:?}");
        }
        if let IrType::Float(width) = resolved_ty {
            let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
            let is_f64 = matches!(width, hll_to_ir::FloatWidth::F64);
            let val_fp = self.load_float_value_to_temp(value, &resolved_ty, ctx);
            let result_fp = self.emitter.alloc_float_temp_reg();
            match op {
                IrUnaryOp::Neg if is_f64 => {
                    self.emitter
                        .emit_inst(RealInstruction::FsgnjnD(FsgnjnD::new(
                            result_fp, val_fp, val_fp,
                        )));
                }
                IrUnaryOp::Neg => {
                    self.emitter.emit_inst(RealInstruction::Fsgnjn(Fsgnjn::new(
                        result_fp, val_fp, val_fp,
                    )));
                }
                IrUnaryOp::Not => panic!("Bitwise not not supported for floats"),
            }
            if is_f64 {
                self.emitter.emit_fsd(SP, result_fp, dest_slot as i32);
            } else {
                self.emitter.emit_fsw(SP, result_fp, dest_slot as i32);
            }
        } else {
            let val_tmp = self.load_value_to_temp(value, ctx);
            let result_tmp = self.result_reg_for(dest, ctx);
            match op {
                IrUnaryOp::Neg => self.emitter.emit_neg(result_tmp, val_tmp),
                // HLL `!`/`not` is logical negation (no bitwise-not operator exists),
                // so `!x` is `x == 0`, not a bitwise complement.
                IrUnaryOp::Not => self.emitter.emit_seqz(result_tmp, val_tmp),
            }
            self.commit_int_result(dest, result_tmp, &resolved_ty, ctx);
        }
    }

    fn lower_cmp(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        op: &IrCmpOp,
        ty: &IrType,
        lhs: &IrValue,
        rhs: &IrValue,
        ctx: &mut FunctionContext,
    ) {
        self.emitter.reset_temp_counter();
        let resolved_ty = self.resolve_ir_type(ty);
        if matches!(
            resolved_ty,
            IrType::Aggregate(_) | IrType::Array { .. } | IrType::Slice(_)
        ) {
            panic!(
                "Comparison operations cannot be performed on aggregate/array type {resolved_ty:?}"
            );
        }
        let bool_ty = IrType::Integer(hll_to_ir::IntWidth::I1);
        if let IrType::Float(width) = resolved_ty {
            let is_f64 = matches!(width, hll_to_ir::FloatWidth::F64);
            let lhs_fp = self.load_float_value_to_temp(lhs, &resolved_ty, ctx);
            let rhs_fp = self.load_float_value_to_temp(rhs, &resolved_ty, ctx);
            let result_tmp = self.result_reg_for(dest, ctx);
            // Emit the comparison appropriate to the float width.
            let feq = |s: &mut Self, rd, a, b| {
                if is_f64 {
                    s.emitter.emit_feq_d(rd, a, b)
                } else {
                    s.emitter.emit_feq_s(rd, a, b)
                }
            };
            let flt = |s: &mut Self, rd, a, b| {
                if is_f64 {
                    s.emitter.emit_flt_d(rd, a, b)
                } else {
                    s.emitter.emit_flt_s(rd, a, b)
                }
            };
            let fle = |s: &mut Self, rd, a, b| {
                if is_f64 {
                    s.emitter.emit_fle_d(rd, a, b)
                } else {
                    s.emitter.emit_fle_s(rd, a, b)
                }
            };
            match op {
                IrCmpOp::Eq => feq(self, result_tmp, lhs_fp, rhs_fp),
                IrCmpOp::Ne => {
                    let tmp = self.emitter.alloc_temp_reg();
                    feq(self, tmp, lhs_fp, rhs_fp);
                    self.emitter.emit_not(result_tmp, tmp);
                }
                IrCmpOp::Slt | IrCmpOp::Ult => flt(self, result_tmp, lhs_fp, rhs_fp),
                IrCmpOp::Sle | IrCmpOp::Ule => fle(self, result_tmp, lhs_fp, rhs_fp),
                IrCmpOp::Sgt | IrCmpOp::Ugt => flt(self, result_tmp, rhs_fp, lhs_fp),
                IrCmpOp::Sge | IrCmpOp::Uge => fle(self, result_tmp, rhs_fp, lhs_fp),
            }
            self.commit_canonical_result(dest, result_tmp, &bool_ty, ctx);
        } else {
            let lhs_tmp = self.load_value_to_temp(lhs, ctx);
            let rhs_tmp = self.load_value_to_temp(rhs, ctx);
            let result_tmp = self.result_reg_for(dest, ctx);
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
            self.commit_canonical_result(dest, result_tmp, &bool_ty, ctx);
        }
    }

    fn lower_cast_inst(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        mode: IrCastMode,
        value: &IrValue,
        ty: &IrType,
        ctx: &mut FunctionContext,
    ) {
        self.emitter.reset_temp_counter();
        let resolved_ty = self.resolve_ir_type(ty);
        if matches!(
            resolved_ty,
            IrType::Aggregate(_) | IrType::Array { .. } | IrType::Slice(_)
        ) {
            panic!("Cast operations cannot be performed on aggregate/array type {resolved_ty:?}");
        }
        let src_ty = self.resolve_value_type(value, ctx);

        // Casts that cross or stay within the FP register file need fcvt, not a
        // plain integer move. The IR `Cast` carries only the target type, so the
        // source width is recovered from the operand's type.
        let src_is_float = matches!(src_ty, IrType::Float(_));
        let dst_is_float = matches!(resolved_ty, IrType::Float(_));
        match mode {
            IrCastMode::F2i => {
                self.lower_cast_f2i(dest, value, &src_ty, &resolved_ty, ctx);
                return;
            }
            IrCastMode::I2f => {
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                self.lower_cast_i2f(dest_slot, value, &resolved_ty, ctx);
                return;
            }
            // A float-to-float Bitcast is really a width conversion.
            IrCastMode::Bitcast if src_is_float && dst_is_float => {
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                self.lower_cast_f2f(dest_slot, value, &src_ty, &resolved_ty, ctx);
                return;
            }
            _ => {}
        }

        let src_tmp = self.load_value_to_temp(value, ctx);
        let result_tmp = self.result_reg_for(dest, ctx);
        self.lower_cast(result_tmp, src_tmp, mode, &src_ty, &resolved_ty);
        self.commit_int_result(dest, result_tmp, &resolved_ty, ctx);
    }

    /// Lower a float-to-integer cast via the appropriate `fcvt.{w,l}.{s,d}`.
    /// Signedness is not tracked in the IR cast mode, so signed conversion is
    /// used (the common case).
    fn lower_cast_f2i(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        value: &IrValue,
        src_ty: &IrType,
        target_ty: &IrType,
        ctx: &FunctionContext,
    ) {
        let src_fp = self.load_float_value_to_temp(value, src_ty, ctx);
        let result_tmp = self.result_reg_for(dest, ctx);
        let from_f64 = matches!(src_ty, IrType::Float(hll_to_ir::FloatWidth::F64));
        let to_i64 = matches!(target_ty, IrType::Integer(hll_to_ir::IntWidth::I64));
        match (from_f64, to_i64) {
            (false, false) => self
                .emitter
                .emit_inst(RealInstruction::FcvtWS(FcvtWS::new(result_tmp, src_fp))),
            (false, true) => self
                .emitter
                .emit_inst(RealInstruction::FcvtLS(FcvtLS::new(result_tmp, src_fp))),
            (true, false) => self
                .emitter
                .emit_inst(RealInstruction::FcvtWD(FcvtWD::new(result_tmp, src_fp))),
            (true, true) => self
                .emitter
                .emit_inst(RealInstruction::FcvtLD(FcvtLD::new(result_tmp, src_fp))),
        }
        self.commit_int_result(dest, result_tmp, target_ty, ctx);
    }

    /// Lower an integer-to-float cast via `fcvt.{s,d}.{w,l}`. Signed source is
    /// assumed (signedness is not carried in the IR cast mode).
    fn lower_cast_i2f(
        &mut self,
        dest_slot: usize,
        value: &IrValue,
        target_ty: &IrType,
        ctx: &FunctionContext,
    ) {
        let src_ty = self.resolve_value_type(value, ctx);
        let src_tmp = self.load_value_to_temp(value, ctx);
        let result_fp = self.emitter.alloc_float_temp_reg();
        let to_f64 = matches!(target_ty, IrType::Float(hll_to_ir::FloatWidth::F64));
        let from_i64 = matches!(src_ty, IrType::Integer(hll_to_ir::IntWidth::I64));
        match (to_f64, from_i64) {
            (false, false) => self
                .emitter
                .emit_inst(RealInstruction::FcvtSW(FcvtSW::new(result_fp, src_tmp))),
            (false, true) => self
                .emitter
                .emit_inst(RealInstruction::FcvtSL(FcvtSL::new(result_fp, src_tmp))),
            (true, false) => self
                .emitter
                .emit_inst(RealInstruction::FcvtDW(FcvtDW::new(result_fp, src_tmp))),
            (true, true) => self
                .emitter
                .emit_inst(RealInstruction::FcvtDL(FcvtDL::new(result_fp, src_tmp))),
        }
        if to_f64 {
            self.emitter.emit_fsd(SP, result_fp, dest_slot as i32);
        } else {
            self.emitter.emit_fsw(SP, result_fp, dest_slot as i32);
        }
    }

    /// Lower a float-to-float cast: f32<->f64 width conversion, or a plain move
    /// when the widths match.
    fn lower_cast_f2f(
        &mut self,
        dest_slot: usize,
        value: &IrValue,
        src_ty: &IrType,
        target_ty: &IrType,
        ctx: &FunctionContext,
    ) {
        let src_fp = self.load_float_value_to_temp(value, src_ty, ctx);
        let result_fp = self.emitter.alloc_float_temp_reg();
        let from_f64 = matches!(src_ty, IrType::Float(hll_to_ir::FloatWidth::F64));
        let to_f64 = matches!(target_ty, IrType::Float(hll_to_ir::FloatWidth::F64));
        match (from_f64, to_f64) {
            (true, false) => self
                .emitter
                .emit_inst(RealInstruction::FcvtSD(FcvtSD::new(result_fp, src_fp))),
            (false, true) => self
                .emitter
                .emit_inst(RealInstruction::FcvtDS(FcvtDS::new(result_fp, src_fp))),
            _ => self.emitter.emit_fmv_d(result_fp, src_fp),
        }
        if to_f64 {
            self.emitter.emit_fsd(SP, result_fp, dest_slot as i32);
        } else {
            self.emitter.emit_fsw(SP, result_fp, dest_slot as i32);
        }
    }

    fn lower_call(
        &mut self,
        dest: &Option<hll_to_ir::IrRegister>,
        function: &str,
        args: &[IrValue],
        ctx: &mut FunctionContext,
    ) {
        self.emitter.reset_temp_counter();
        // For externals (absent from the IR) fall back to the dest register's
        // type so aggregate/slice returns stay on the aggregate path.
        let mut func_return_type = self.function_return_types.get(function).cloned();
        if func_return_type.is_none()
            && let Some(d) = dest.as_ref()
        {
            func_return_type = ctx.type_for_reg(d);
        }
        let func_return_type =
            func_return_type.unwrap_or(IrType::Integer(hll_to_ir::IntWidth::I64));
        let resolved_ret_ty = self.resolve_ir_type(&func_return_type);
        let is_agg_return = matches!(
            resolved_ret_ty,
            IrType::Aggregate(_) | IrType::Array { .. } | IrType::Slice(_)
        );
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

        // Aggregates use one indirect integer ABI argument. This gives every
        // aggregate value-copy semantics without splitting its fields across the
        // scalar register paths; the callee copies from the pointer into its slot.
        let total_abi_args = arg_index + args.len();
        let excess_count = total_abi_args.saturating_sub(8);
        let aligned_bytes = if excess_count == 0 {
            0
        } else {
            let stack_bytes = (excess_count * 8) as i64;
            if stack_bytes % 16 == 0 {
                stack_bytes
            } else {
                stack_bytes + (16 - stack_bytes % 16)
            }
        };
        if aligned_bytes > 0 {
            self.emitter
                .emit_comment("Pushing excess arguments to stack");
        }

        // Store overflow values below sp before moving sp, because aggregate
        // source slots are addressed relative to the caller's current frame.
        for arg in args {
            let ty = self.resolve_ir_type(&self.resolve_value_type(arg, ctx));
            let is_aggregate = matches!(
                &ty,
                IrType::Aggregate(_) | IrType::Array { .. } | IrType::Slice(_)
            );
            let value_reg = if is_aggregate {
                let IrValue::Register(reg) = arg else {
                    panic!("aggregate call argument must be a register")
                };
                let slot = ctx.slot_for_reg(reg).expect("aggregate argument slot");
                let address = self.emitter.alloc_temp_reg();
                self.emitter.emit_add_imm(address, SP, slot as i64);
                address
            } else if let IrType::Float(width) = &ty {
                let width = *width;
                let value = self.load_float_value_to_temp(arg, &IrType::Float(width), ctx);
                if arg_index < 8 {
                    self.emit_float_move(reg_for_arg(arg_index), value, width);
                    arg_index += 1;
                    continue;
                }
                value
            } else {
                self.load_value_to_temp(arg, ctx)
            };

            if arg_index < 8 {
                self.emitter.emit_mv(reg_for_arg(arg_index), value_reg);
            } else {
                let offset = (((arg_index - 8) * 8) as i64 - aligned_bytes) as i32;
                if matches!(ty, IrType::Float(_)) {
                    match ty {
                        IrType::Float(hll_to_ir::FloatWidth::F32) => {
                            self.emitter.emit_fsw(SP, value_reg, offset);
                        }
                        IrType::Float(hll_to_ir::FloatWidth::F64) => {
                            self.emitter.emit_fsd(SP, value_reg, offset);
                        }
                        _ => unreachable!(),
                    }
                } else {
                    self.emitter.emit_sd(SP, value_reg, offset);
                }
            }
            arg_index += 1;
        }
        if aligned_bytes > 0 {
            self.emitter.emit_add_imm(SP, SP, -aligned_bytes);
        }

        self.emitter.emit_jal(RA, function);

        // Reclaim the stack space used for the excess arguments.
        if aligned_bytes > 0 {
            self.emitter.emit_add_imm(SP, SP, aligned_bytes);
        }

        if is_agg_return && !needs_sret {
            self.emitter
                .emit_comment("Unpacking small aggregate return from a0/a1");
            if let Some(dest_reg) = dest {
                let dest_slot = ctx.slot_for_reg(dest_reg).expect("dest slot");
                {
                    let total_size = self.type_size(&resolved_ret_ty);
                    let chunk0 = total_size.min(8);
                    if chunk0 == 8 {
                        self.emitter.emit_sd(SP, A0, dest_slot as i32);
                    } else if chunk0 >= 4 {
                        self.emitter.emit_inst(RealInstruction::Sw(Sw::new(
                            SP,
                            A0,
                            dest_slot as i32,
                        )));
                    } else if chunk0 >= 2 {
                        self.emitter.emit_inst(RealInstruction::Sh(Sh::new(
                            SP,
                            A0,
                            dest_slot as i32,
                        )));
                    } else {
                        self.emitter.emit_inst(RealInstruction::Sb(Sb::new(
                            SP,
                            A0,
                            dest_slot as i32,
                        )));
                    }
                    if total_size > 8 {
                        let remaining = total_size - 8;
                        if remaining >= 8 {
                            self.emitter.emit_sd(SP, A1, (dest_slot + 8) as i32);
                        } else if remaining >= 4 {
                            self.emitter.emit_inst(RealInstruction::Sw(Sw::new(
                                SP,
                                A1,
                                (dest_slot + 8) as i32,
                            )));
                        } else if remaining >= 2 {
                            self.emitter.emit_inst(RealInstruction::Sh(Sh::new(
                                SP,
                                A1,
                                (dest_slot + 8) as i32,
                            )));
                        } else {
                            self.emitter.emit_inst(RealInstruction::Sb(Sb::new(
                                SP,
                                A1,
                                (dest_slot + 8) as i32,
                            )));
                        }
                    }
                }
            }
        } else if !is_agg_return && let Some(dest) = dest {
            let resolved_return_ty = self.resolve_ir_type(&func_return_type);
            self.store_int_result_from(dest, A0, &resolved_return_ty, ctx);
        }
        self.emitter
            .emit_comment(&format!("--- End Function Call: {function} ---"));
    }

    fn lower_global_ref(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        name: &str,
        ctx: &mut FunctionContext,
    ) {
        self.emitter.reset_temp_counter();
        let temp = self.result_reg_for(dest, ctx);
        self.emitter
            .emit_raw(&format!("\tla {}, {}", reg_name(temp, false), name));
        let ptr_ty = IrType::Pointer(Box::new(IrType::Void));
        self.commit_canonical_result(dest, temp, &ptr_ty, ctx);
    }

    fn lower_heap_alloc(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        ty: &IrType,
        count: &Option<IrValue>,
        ctx: &mut FunctionContext,
    ) {
        self.emitter.reset_temp_counter();
        let type_size = self.type_size(ty);

        match count {
            None => {
                self.emitter.emit_li(A0, type_size as i64);
            }
            Some(IrValue::Integer(n)) => {
                let bytes = type_size.saturating_mul(*n as usize);
                self.emitter.emit_li(A0, bytes as i64);
            }
            Some(count_val) => {
                // Dynamic count: compute sizeof(T) * count at runtime.
                let count_tmp = self.load_value_to_temp(count_val, ctx);
                let size_tmp = self.emitter.alloc_temp_reg();
                self.emitter.emit_li(size_tmp, type_size as i64);
                self.emitter.emit_raw(&format!(
                    "\tmul {}, {}, {}",
                    reg_name(A0, false),
                    reg_name(count_tmp, false),
                    reg_name(size_tmp, false)
                ));
            }
        }

        self.emitter.emit_raw("\tcall malloc");
        let ptr_ty = IrType::Pointer(Box::new(IrType::Void));
        self.store_int_result_from(dest, A0, &ptr_ty, ctx);
    }

    fn lower_heap_free(&mut self, ptr: &hll_to_ir::IrRegister, ctx: &mut FunctionContext) {
        self.emitter.reset_temp_counter();
        let ptr_tmp = self.load_value_to_temp(&IrValue::Register(ptr.clone()), ctx);
        self.emitter.emit_mv(A0, ptr_tmp);
        self.emitter.emit_raw("\tcall free");
    }

    fn lower_read_reg(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        reg: &str,
        ctx: &mut FunctionContext,
    ) {
        use asm_to_binary::parse_int_reg;
        self.emitter.reset_temp_counter();
        let src_hw = parse_int_reg(reg).expect("asm_reg: register validated by semantic analysis");
        let tmp = self.result_reg_for(dest, ctx);
        self.emitter.emit_mv(tmp, src_hw);
        let i64_ty = IrType::Integer(hll_to_ir::IntWidth::I64);
        self.commit_canonical_result(dest, tmp, &i64_ty, ctx);
    }

    fn lower_terminator(
        &mut self,
        term: &IrTerminator,
        ctx: &mut FunctionContext,
        needs_sret: bool,
        _is_aggregate: bool,
    ) {
        match term {
            IrTerminator::Return(val) => self.lower_return(val.as_ref(), needs_sret, ctx),
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
            IrTerminator::Trap { code } => {
                // Exit with the diagnostic code in a0 (syscall 93). The firmware
                // turns this into a clean halt, so a failed check shows as that code.
                self.emitter.emit_raw(&format!("\tli a0, {code}"));
                self.emitter.emit_raw("\tli a7, 93");
                self.emitter.emit_raw("\tecall");
            }
        }
    }

    fn lower_return(&mut self, val: Option<&IrValue>, needs_sret: bool, ctx: &mut FunctionContext) {
        if let Some(val) = val {
            let raw_val_type = self.resolve_value_type(val, ctx);
            let resolved_val = match (&raw_val_type, val) {
                (IrType::Pointer(inner), IrValue::Register(reg)) if ctx.is_stack_address(reg) => {
                    self.resolve_ir_type(inner)
                }
                _ => raw_val_type.clone(),
            };
            let is_agg_return = matches!(
                resolved_val,
                IrType::Aggregate(_) | IrType::Array { .. } | IrType::Slice(_)
            );

            if needs_sret && is_agg_return {
                let IrValue::Register(reg) = val else {
                    panic!("Aggregate return must be a register")
                };
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
            } else if is_agg_return {
                let IrValue::Register(reg) = val else {
                    panic!("Small aggregate return must be a register")
                };
                let slot = ctx.slot_for_reg(reg).expect("reg slot");
                {
                    let total_size = self.type_size(&resolved_val);
                    let chunk0 = total_size.min(8);
                    if chunk0 == 8 {
                        self.emitter.emit_ld(A0, SP, slot as i32);
                    } else if chunk0 >= 4 {
                        self.emitter.emit_lw(A0, SP, slot as i32);
                    } else if chunk0 >= 2 {
                        self.emitter.emit_lh(A0, SP, slot as i32);
                    } else if chunk0 >= 1 {
                        self.emitter.emit_lb(A0, SP, slot as i32);
                    }
                    if total_size > 8 {
                        let remaining = total_size - 8;
                        if remaining >= 8 {
                            self.emitter.emit_ld(A1, SP, (slot + 8) as i32);
                        } else if remaining >= 4 {
                            self.emitter.emit_lw(A1, SP, (slot + 8) as i32);
                        } else if remaining >= 2 {
                            self.emitter.emit_lh(A1, SP, (slot + 8) as i32);
                        } else {
                            self.emitter.emit_lb(A1, SP, (slot + 8) as i32);
                        }
                    }
                }
            } else {
                let resolved_val = self.resolve_value_type(val, ctx);
                if matches!(resolved_val, IrType::Float(_)) {
                    let val_fp = self.load_float_value_to_temp(val, &resolved_val, ctx);
                    match resolved_val {
                        IrType::Float(hll_to_ir::FloatWidth::F32) => {
                            self.emitter.emit_fmv_s(FA0, val_fp);
                        }
                        IrType::Float(hll_to_ir::FloatWidth::F64) => {
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

    // --- Result-routing helpers (register allocation) ---

    /// The register an instruction result is computed into: the dest's
    /// assigned physical register, or a fresh scratch temp for slot-based dests.
    fn result_reg_for(&mut self, dest: &hll_to_ir::IrRegister, ctx: &FunctionContext) -> Reg {
        ctx.phys_reg_for(dest)
            .unwrap_or_else(|| self.emitter.alloc_temp_reg())
    }

    /// Finish an integer-producing instruction whose result is in `result`
    /// (obtained from `result_reg_for`): register-resident dests are
    /// width-normalized in place, slot-based dests are stored to their slot.
    fn commit_int_result(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        result: Reg,
        ty: &IrType,
        ctx: &FunctionContext,
    ) {
        if ctx.phys_reg_for(dest).is_some() {
            self.emitter.emit_normalize_width(result, ty);
        } else {
            let slot = ctx.slot_for_reg(dest).expect("dest slot");
            self.emitter
                .emit_store_from_tmp(SP, result, ty, slot as i32);
        }
    }

    /// Like `commit_int_result`, but skips width normalization for results
    /// already in canonical sign-extended form (e.g. comparison results,
    /// always 0 or 1).
    fn commit_canonical_result(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        result: Reg,
        ty: &IrType,
        ctx: &FunctionContext,
    ) {
        if ctx.phys_reg_for(dest).is_none() {
            let slot = ctx.slot_for_reg(dest).expect("dest slot");
            self.emitter
                .emit_store_from_tmp(SP, result, ty, slot as i32);
        }
    }

    /// Store a value already sitting in `src` (e.g. a0 after a call) into the
    /// dest register or slot.
    fn store_int_result_from(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        src: Reg,
        ty: &IrType,
        ctx: &FunctionContext,
    ) {
        if let Some(phys) = ctx.phys_reg_for(dest) {
            self.emitter.emit_move_typed(phys, src, ty);
        } else {
            let slot = ctx.slot_for_reg(dest).expect("dest slot");
            self.emitter.emit_store_from_tmp(SP, src, ty, slot as i32);
        }
    }

    // --- Operand-loading helpers ---

    fn resolve_ptr_to_addr(
        &mut self,
        ptr: &hll_to_ir::IrRegister,
        ctx: &FunctionContext,
        byte_offset: Option<i32>,
    ) -> Reg {
        if let Some(phys) = ctx.phys_reg_for(ptr) {
            // The pointer already lives in a register. Apply any offset into a
            // scratch temp; the allocated register must not be mutated.
            return match byte_offset {
                Some(off) if off != 0 => {
                    let tmp = self.emitter.alloc_temp_reg();
                    self.emitter.emit_add_imm(tmp, phys, off as i64);
                    tmp
                }
                _ => phys,
            };
        }

        let slot = ctx.slot_for_reg(ptr).expect("ptr slot");
        let tmp = self.emitter.alloc_temp_reg();

        if ctx.is_stack_address(ptr) {
            let total_offset = slot as i64 + byte_offset.unwrap_or(0) as i64;
            self.emitter.emit_add_imm(tmp, SP, total_offset);
        } else {
            self.emitter.emit_ld(tmp, SP, slot as i32);
            if let Some(off) = byte_offset
                && off != 0
            {
                self.emitter.emit_add_imm(tmp, tmp, off as i64);
            }
        }
        tmp
    }

    fn load_value_to_temp(&mut self, val: &IrValue, ctx: &FunctionContext) -> Reg {
        // Register-resident values are used in place; no temp, no load.
        if let IrValue::Register(reg) = val
            && !ctx.preserve_param_registers()
            && let Some(phys) = ctx.phys_reg_for(reg)
        {
            return phys;
        }

        let temp = self.emitter.alloc_temp_reg();
        match val {
            IrValue::Register(reg) => {
                if ctx.preserve_param_registers()
                    && let Some(index) = ctx.param_index(reg)
                    && index < 8
                {
                    self.emitter.emit_mv(temp, reg_for_arg(index));
                    return temp;
                }

                // Load the register's value from its stack slot.
                let slot = ctx.slot_for_reg(reg).expect("reg slot");
                if ctx.is_stack_address(reg) {
                    self.emitter.emit_add_imm(temp, SP, slot as i64);
                } else {
                    let ty = ctx
                        .type_for_reg(reg)
                        .unwrap_or(IrType::Integer(hll_to_ir::IntWidth::I64));
                    self.emitter.emit_load_from_slot(temp, slot, &ty);
                }
            }
            IrValue::Integer(i) => {
                self.emitter.emit_li(temp, *i);
            }
            IrValue::Bool(b) => {
                self.emitter.emit_li(temp, i64::from(*b));
            }
            IrValue::Float(_) => panic!("Float values must use load_float_value_to_temp"),
            IrValue::Null => {
                self.emitter.emit_li(temp, 0);
            }
            IrValue::GlobalString(symbol) => {
                self.emitter
                    .emit_raw(&format!("\tla {}, {}", reg_name(temp, false), symbol));
            }
        }
        temp
    }

    /// Load a float value into an FP temp. `float_ty` is the operand's float
    /// width, used to materialize constants with the correct bit pattern and
    /// move instruction (`fmv.w.x` for f32, `fmv.d.x` for f64).
    fn load_float_value_to_temp(
        &mut self,
        val: &IrValue,
        float_ty: &IrType,
        ctx: &FunctionContext,
    ) -> Reg {
        let temp = self.emitter.alloc_float_temp_reg();
        match val {
            IrValue::Register(reg) => {
                if ctx.preserve_param_registers()
                    && let Some(index) = ctx.param_index(reg)
                    && index < 8
                {
                    let ty = ctx
                        .type_for_reg(reg)
                        .unwrap_or(IrType::Float(hll_to_ir::FloatWidth::F32));
                    match ty {
                        IrType::Float(hll_to_ir::FloatWidth::F32) => {
                            self.emitter.emit_fmv_s(temp, reg_for_arg(index));
                        }
                        IrType::Float(hll_to_ir::FloatWidth::F64) => {
                            self.emitter.emit_inst(RealInstruction::FsgnjD(fmv_d(
                                temp,
                                reg_for_arg(index),
                            )));
                        }
                        _ => panic!("Expected float type"),
                    }
                    return temp;
                }

                let slot = ctx.slot_for_reg(reg).expect("reg slot");
                let ty = ctx
                    .type_for_reg(reg)
                    .unwrap_or(IrType::Float(hll_to_ir::FloatWidth::F32));
                match ty {
                    IrType::Float(hll_to_ir::FloatWidth::F32) => {
                        self.emitter.emit_flw(temp, SP, slot as i32);
                    }
                    IrType::Float(hll_to_ir::FloatWidth::F64) => {
                        self.emitter.emit_fld(temp, SP, slot as i32);
                    }
                    _ => panic!("Expected float type for float register load"),
                }
            }
            IrValue::Float(f) => {
                let int_tmp = self.emitter.alloc_temp_reg();
                match float_ty {
                    IrType::Float(hll_to_ir::FloatWidth::F64) => {
                        // Move the full 64-bit pattern via fmv.d.x.
                        self.emitter.emit_li(int_tmp, f.to_bits() as i64);
                        self.emitter.emit_fmv_d_x(temp, int_tmp);
                    }
                    _ => {
                        // f32: round the f64 literal to f32 and move its 32-bit
                        // pattern. fmv.w.x only consumes the low 32 bits, so the
                        // value must already be the f32 encoding.
                        let bits = (*f as f32).to_bits();
                        self.emitter.emit_li(int_tmp, i64::from(bits));
                        self.emitter.emit_fmv_w_x(temp, int_tmp);
                    }
                }
            }
            _ => panic!("Unsupported float value: {val:?}"),
        }
        temp
    }

    fn load_pointer_operand_to_temp(
        &mut self,
        reg: &hll_to_ir::IrRegister,
        ctx: &FunctionContext,
    ) -> Reg {
        if !ctx.preserve_param_registers()
            && let Some(phys) = ctx.phys_reg_for(reg)
        {
            return phys;
        }

        let temp = self.emitter.alloc_temp_reg();
        if ctx.preserve_param_registers()
            && let Some(index) = ctx.param_index(reg)
            && index < 8
        {
            self.emitter.emit_mv(temp, reg_for_arg(index));
            return temp;
        }

        let slot = ctx.slot_for_reg(reg).expect("reg slot");
        if ctx.is_stack_address(reg) {
            self.emitter.emit_add_imm(temp, SP, slot as i64);
        } else {
            self.emitter.emit_ld(temp, SP, slot as i32);
        }
        temp
    }

    fn lower_cast(&mut self, rd: Reg, rs: Reg, mode: IrCastMode, src_ty: &IrType, ty: &IrType) {
        match mode {
            IrCastMode::Bitcast | IrCastMode::Trunc => {
                self.emitter.emit_mv(rd, rs);
            }
            // Zero-extend from the SOURCE width: the operand may have been
            // produced by a sign-extending load (lb/lh/lw), so the high bits
            // must be cleared, not merely copied. The IR cast carries only the
            // target type, so the source width comes from `src_ty`.
            IrCastMode::Zext => match self.resolve_ir_type(src_ty) {
                IrType::Integer(hll_to_ir::IntWidth::I1) => {
                    self.emitter.emit_slli(rd, rs, 63);
                    self.emitter.emit_srli(rd, rd, 63);
                }
                IrType::Integer(hll_to_ir::IntWidth::I8) => {
                    self.emitter.emit_slli(rd, rs, 56);
                    self.emitter.emit_srli(rd, rd, 56);
                }
                IrType::Integer(hll_to_ir::IntWidth::I16) => {
                    self.emitter.emit_slli(rd, rs, 48);
                    self.emitter.emit_srli(rd, rd, 48);
                }
                IrType::Integer(hll_to_ir::IntWidth::I32) => {
                    self.emitter.emit_slli(rd, rs, 32);
                    self.emitter.emit_srli(rd, rd, 32);
                }
                // I64 (and any non-narrow source) already fills the register.
                _ => self.emitter.emit_mv(rd, rs),
            },
            IrCastMode::Sext => match ty {
                IrType::Integer(hll_to_ir::IntWidth::I32) => {
                    self.emitter.emit_addiw(rd, rs, 0);
                }
                IrType::Integer(hll_to_ir::IntWidth::I64) | IrType::Pointer(_) => {
                    self.emitter.emit_mv(rd, rs);
                }
                IrType::Integer(hll_to_ir::IntWidth::I16) => {
                    self.emitter.emit_slli(rd, rs, 48);
                    self.emitter.emit_srai(rd, rd, 48);
                }
                IrType::Integer(hll_to_ir::IntWidth::I8) => {
                    self.emitter.emit_slli(rd, rs, 56);
                    self.emitter.emit_srai(rd, rd, 56);
                }
                _ => self.emitter.emit_mv(rd, rs),
            },
            IrCastMode::F2i | IrCastMode::I2f => self.emitter.emit_mv(rd, rs),
        }
    }

    fn resolve_ir_type(&self, ty: &IrType) -> IrType {
        type_utils::resolve_ir_type(ty, &self.type_aliases)
    }

    fn resolve_value_type(&self, val: &IrValue, ctx: &FunctionContext) -> IrType {
        match val {
            IrValue::Register(reg) => ctx
                .type_for_reg(reg)
                .unwrap_or(IrType::Integer(hll_to_ir::IntWidth::I64)),
            IrValue::Integer(_) => IrType::Integer(hll_to_ir::IntWidth::I64),
            IrValue::Bool(_) => IrType::Integer(hll_to_ir::IntWidth::I1),
            IrValue::Float(_) => IrType::Float(hll_to_ir::FloatWidth::F64),
            IrValue::Null => IrType::Pointer(Box::new(IrType::Void)),
            IrValue::GlobalString(_) => {
                IrType::Pointer(Box::new(IrType::Integer(hll_to_ir::IntWidth::I8)))
            }
        }
    }

    /// Move a float temp into a destination float register for the given width
    /// (`fmv.s` for f32, `fmv.d` for f64). Used to place float call arguments.
    fn emit_float_move(&mut self, dst: Reg, src: Reg, width: hll_to_ir::FloatWidth) {
        match width {
            hll_to_ir::FloatWidth::F32 => self.emitter.emit_fmv_s(dst, src),
            hll_to_ir::FloatWidth::F64 => self.emitter.emit_fmv_d(dst, src),
        }
    }

    fn type_size(&self, ty: &IrType) -> usize {
        type_utils::type_size(ty, &self.type_aliases)
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

#[cfg(test)]
mod tests {
    use super::{CompilerRv64, reg_for_arg};
    use hll_to_ir::{
        IntWidth, IrBlock, IrCastMode, IrFunction, IrInstruction, IrProgram, IrRegister,
        IrTerminator, IrType, IrValue,
    };

    // A zero-extending cast from a narrow unsigned integer must clear the high
    // bits the sign-extending load (lb/lh/lw) left behind -- it cannot be a bare
    // `mv`. Regression for the u8->u64 widening miscompile that read 0xA4 as
    // 0xFFFF_FFFF_FFFF_FFA4. The lowering emits `slli`/`srli` to mask to the
    // source width.
    #[test]
    fn zext_from_loaded_narrow_unsigned_masks_high_bits() {
        let mask_widths = [
            (IntWidth::I8, 56u32),
            (IntWidth::I16, 48),
            (IntWidth::I32, 32),
        ];
        for (width, shift) in mask_widths {
            let slot = IrRegister::Named("slot".to_owned());
            let loaded = IrRegister::Named("loaded".to_owned());
            let widened = IrRegister::Named("widened".to_owned());

            let mut entry = IrBlock::new("entry");
            entry.push_instruction(IrInstruction::Alloc {
                dest: slot.clone(),
                ty: IrType::Integer(width),
                count: None,
            });
            entry.push_instruction(IrInstruction::Store {
                ty: IrType::Integer(width),
                value: IrValue::Integer(-1),
                ptr: slot.clone(),
                offset: None,
            });
            entry.push_instruction(IrInstruction::Load {
                dest: loaded.clone(),
                ty: IrType::Integer(width),
                ptr: slot.clone(),
                offset: None,
            });
            entry.push_instruction(IrInstruction::Cast {
                dest: widened.clone(),
                mode: IrCastMode::Zext,
                value: IrValue::Register(loaded),
                ty: IrType::Integer(IntWidth::I64),
            });
            entry.set_terminator(IrTerminator::Return(Some(IrValue::Register(widened))));

            let mut func = IrFunction::new("widen", IrType::Integer(IntWidth::I64));
            func.push_block(entry);
            let mut program = IrProgram::new("test");
            program.push_function(func);

            let asm = CompilerRv64::new().compile(&program);
            assert!(
                asm.contains("slli") && asm.contains("srli"),
                "zext from {width:?} must emit slli/srli to zero the high bits; asm:\n{asm}"
            );
            assert!(
                asm.contains(&format!(", {shift}\n")),
                "zext from {width:?} must mask with a {shift}-bit logical shift; asm:\n{asm}"
            );
        }
    }

    #[test]
    fn reg_for_arg_maps_first_eight_to_a_regs() {
        for (index, expected_reg) in [10u8, 11, 12, 13, 14, 15, 16, 17].iter().enumerate() {
            assert_eq!(
                reg_for_arg(index),
                *expected_reg,
                "arg {index} should map to register {expected_reg} (a{index})"
            );
        }
    }

    #[test]
    fn reg_for_arg_ninth_and_beyond_returns_zero() {
        for overflow_index in [8, 9, 15, 100] {
            assert_eq!(
                reg_for_arg(overflow_index),
                0,
                "arg {overflow_index} (beyond a7) should return x0 (stack-passed marker)"
            );
        }
    }
}
