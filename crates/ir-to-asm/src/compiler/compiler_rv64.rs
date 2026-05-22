use super::{
    assembly_emitter::AssemblyEmitter,
    data_section::DataSection,
    function_context::FunctionContext,
    register_allocator::{Allocation, RegisterAllocator},
    type_utils,
};
use asm_to_binary::encode_decode::Reg;
use asm_to_binary::real::RealInstruction;
use asm_to_binary::riscv::rv64fd::{Fsgnjn, fmv_d};
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

// Float register constants
const FA0: Reg = 10; // fa0

pub struct CompilerRv64 {
    emitter: AssemblyEmitter,
    data: DataSection,
    type_aliases: HashMap<String, IrType>,
    function_return_types: HashMap<String, IrType>,
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
    ) -> (String, Vec<asm_to_binary::rv_instruction::RvInstruction>) {
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
        let mut ctx = FunctionContext::new(&func.name, &self.type_aliases);
        let mut alloc = RegisterAllocator::new();
        alloc.allocate_stack_slots(func, &mut ctx, &self.function_return_types);

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
                self.lower_instruction(inst, &mut ctx, &alloc);
            }
            if let Some(term) = &block.terminator {
                self.lower_terminator(term, &mut ctx, &alloc, needs_sret, is_aggregate);
            }
        }

        self.emitter.end_function();
    }

    fn lower_instruction(
        &mut self,
        inst: &IrInstruction,
        ctx: &mut FunctionContext,
        alloc: &RegisterAllocator,
    ) {
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
            ReadReg { dest, reg } => self.lower_read_reg(dest, reg, ctx, alloc),
            GlobalRef { dest, name } => self.lower_global_ref(dest, name, ctx, alloc),
            Load {
                dest,
                ty,
                ptr,
                offset,
            } => self.lower_load(dest, ty, ptr, *offset, ctx, alloc),
            Store {
                ty,
                value,
                ptr,
                offset,
            } => self.lower_store(ty, value, ptr, *offset, ctx, alloc),
            Offset {
                dest, ptr, bytes, ..
            } => self.lower_offset(dest, ptr, bytes, ctx, alloc),
            Index {
                dest,
                ty,
                base_ptr,
                idx,
            } => self.lower_index(dest, ty, base_ptr, idx, ctx, alloc),
            Math {
                dest,
                op,
                ty,
                lhs,
                rhs,
            } => self.lower_math(dest, op, ty, lhs, rhs, ctx, alloc),
            Unary {
                dest,
                op,
                ty,
                value,
            } => self.lower_unary(dest, op, ty, value, ctx, alloc),
            Cmp {
                dest,
                op,
                ty,
                lhs,
                rhs,
            } => self.lower_cmp(dest, op, ty, lhs, rhs, ctx, alloc),
            Cast {
                dest,
                mode,
                value,
                ty,
            } => self.lower_cast_inst(dest, *mode, value, ty, ctx, alloc),
            Call {
                dest,
                function,
                args,
            } => self.lower_call(dest, function, args, ctx, alloc),
            HeapAlloc { dest, ty, count } => self.lower_heap_alloc(dest, ty, *count, ctx, alloc),
            HeapFree { ptr } => self.lower_heap_free(ptr, ctx, alloc),
        }
    }

    fn lower_load(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        ty: &IrType,
        ptr: &hll_to_ir::IrRegister,
        offset: Option<i64>,
        ctx: &mut FunctionContext,
        alloc: &RegisterAllocator,
    ) {
        self.emitter.reset_temp_counter();
        let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
        self.emitter
            .emit_comment(&format!("Load {ty} from memory into ${dest}"));
        let ptr_tmp = self.load_pointer_operand_to_temp(ptr, ctx, alloc);
        let addr_tmp = if let Some(off) = offset {
            let tmp = self.emitter.alloc_temp_reg();
            self.emitter.emit_addi(tmp, ptr_tmp, off as i32);
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
            // Load value into a temp register first
            let loaded_val = self.emitter.alloc_temp_reg();
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
            // Store to stack slot
            self.emitter
                .emit_store_from_tmp(SP, loaded_val, &resolved_ty, dest_slot as i32);
            // If dest has a physical register allocation, also store there
            if let Some(Allocation::Physical(phys_reg)) = alloc.get_allocation(dest) {
                self.emitter.emit_mv(*phys_reg, loaded_val);
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
        alloc: &RegisterAllocator,
    ) {
        self.emitter.reset_temp_counter();
        let addr_tmp = self.resolve_ptr_to_addr(ptr, ctx, offset.map(|o| o as i32), alloc);
        let resolved_ty = self.resolve_ir_type(ty);
        self.emitter.emit_comment(&format!("Store {ty} to memory"));
        if matches!(resolved_ty, IrType::Array { .. } | IrType::Aggregate(_)) {
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
            let val_fp = self.load_float_value_to_temp(value, ctx, alloc);
            self.emitter
                .emit_store_from_tmp(addr_tmp, val_fp, &resolved_ty, 0);
        } else {
            let val_tmp = self.load_value_to_temp(value, ctx, alloc);
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
        alloc: &RegisterAllocator,
    ) {
        self.emitter.reset_temp_counter();
        let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
        let ptr_tmp = self.load_pointer_operand_to_temp(ptr, ctx, alloc);
        let byte_val_reg = self.load_value_to_temp(bytes, ctx, alloc);
        let off_tmp = self.emitter.alloc_temp_reg();
        self.emitter.emit_mv(off_tmp, byte_val_reg);
        let result_tmp = self.emitter.alloc_temp_reg();
        self.emitter.emit_add(result_tmp, ptr_tmp, off_tmp);
        self.emitter.emit_sd(SP, result_tmp, dest_slot as i32);
    }

    fn lower_index(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        ty: &IrType,
        base_ptr: &hll_to_ir::IrRegister,
        idx: &IrValue,
        ctx: &mut FunctionContext,
        alloc: &RegisterAllocator,
    ) {
        self.emitter.reset_temp_counter();
        let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
        let base_tmp = self.load_pointer_operand_to_temp(base_ptr, ctx, alloc);
        let idx_tmp = self.load_value_to_temp(idx, ctx, alloc);
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

    fn lower_math(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        op: &IrMathOp,
        ty: &IrType,
        lhs: &IrValue,
        rhs: &IrValue,
        ctx: &mut FunctionContext,
        alloc: &RegisterAllocator,
    ) {
        self.emitter.reset_temp_counter();
        let resolved_ty = self.resolve_ir_type(ty);
        if matches!(resolved_ty, IrType::Aggregate(_) | IrType::Array { .. }) {
            panic!("Math operations cannot be performed on aggregate/array type {resolved_ty:?}");
        }
        let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
        self.emitter
            .emit_comment(&format!("{op} operation on {ty}"));
        if matches!(resolved_ty, IrType::Float(hll_to_ir::FloatWidth::F32)) {
            let lhs_fp = self.load_float_value_to_temp(lhs, ctx, alloc);
            let rhs_fp = self.load_float_value_to_temp(rhs, ctx, alloc);
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
            let lhs_tmp = self.load_value_to_temp(lhs, ctx, alloc);
            let rhs_tmp = self.load_value_to_temp(rhs, ctx, alloc);
            let result_tmp = self.emitter.alloc_temp_reg();
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
            self.emitter
                .emit_store_from_tmp(SP, result_tmp, &resolved_ty, dest_slot as i32);
            // If dest has a physical register allocation, also store there for future uses
            if let Some(Allocation::Physical(phys_reg)) = alloc.get_allocation(dest) {
                self.emitter.emit_mv(*phys_reg, result_tmp);
            }
        }
    }

    fn lower_unary(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        op: &IrUnaryOp,
        ty: &IrType,
        value: &IrValue,
        ctx: &mut FunctionContext,
        alloc: &RegisterAllocator,
    ) {
        self.emitter.reset_temp_counter();
        let resolved_ty = self.resolve_ir_type(ty);
        if matches!(resolved_ty, IrType::Aggregate(_) | IrType::Array { .. }) {
            panic!("Unary operations cannot be performed on aggregate/array type {resolved_ty:?}");
        }
        let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
        if matches!(resolved_ty, IrType::Float(hll_to_ir::FloatWidth::F32)) {
            let val_fp = self.load_float_value_to_temp(value, ctx, alloc);
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
            let val_tmp = self.load_value_to_temp(value, ctx, alloc);
            let result_tmp = self.emitter.alloc_temp_reg();
            match op {
                IrUnaryOp::Neg => self.emitter.emit_neg(result_tmp, val_tmp),
                IrUnaryOp::Not => self.emitter.emit_not(result_tmp, val_tmp),
            }
            self.emitter
                .emit_store_from_tmp(SP, result_tmp, &resolved_ty, dest_slot as i32);
            // If dest has a physical register allocation, also store there
            if let Some(Allocation::Physical(phys_reg)) = alloc.get_allocation(dest) {
                self.emitter.emit_mv(*phys_reg, result_tmp);
            }
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
        alloc: &RegisterAllocator,
    ) {
        self.emitter.reset_temp_counter();
        let resolved_ty = self.resolve_ir_type(ty);
        if matches!(resolved_ty, IrType::Aggregate(_) | IrType::Array { .. }) {
            panic!(
                "Comparison operations cannot be performed on aggregate/array type {resolved_ty:?}"
            );
        }
        let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
        let bool_ty = IrType::Integer(hll_to_ir::IntWidth::I1);
        if matches!(resolved_ty, IrType::Float(hll_to_ir::FloatWidth::F32)) {
            let lhs_fp = self.load_float_value_to_temp(lhs, ctx, alloc);
            let rhs_fp = self.load_float_value_to_temp(rhs, ctx, alloc);
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
            self.emitter
                .emit_store_from_tmp(SP, result_tmp, &bool_ty, dest_slot as i32);
        } else {
            let lhs_tmp = self.load_value_to_temp(lhs, ctx, alloc);
            let rhs_tmp = self.load_value_to_temp(rhs, ctx, alloc);
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
            self.emitter
                .emit_store_from_tmp(SP, result_tmp, &bool_ty, dest_slot as i32);
            // If dest has a physical register allocation, also store there
            if let Some(Allocation::Physical(phys_reg)) = alloc.get_allocation(dest) {
                self.emitter.emit_mv(*phys_reg, result_tmp);
            }
        }
    }

    fn lower_cast_inst(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        mode: IrCastMode,
        value: &IrValue,
        ty: &IrType,
        ctx: &mut FunctionContext,
        alloc: &RegisterAllocator,
    ) {
        self.emitter.reset_temp_counter();
        let resolved_ty = self.resolve_ir_type(ty);
        if matches!(resolved_ty, IrType::Aggregate(_) | IrType::Array { .. }) {
            panic!("Cast operations cannot be performed on aggregate/array type {resolved_ty:?}");
        }
        let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
        let src_tmp = self.load_value_to_temp(value, ctx, alloc);
        let result_tmp = self.emitter.alloc_temp_reg();
        self.lower_cast(result_tmp, src_tmp, mode, &resolved_ty);
        self.emitter
            .emit_store_from_tmp(SP, result_tmp, &resolved_ty, dest_slot as i32);
        // If dest has a physical register allocation, also store there
        if let Some(Allocation::Physical(phys_reg)) = alloc.get_allocation(dest) {
            self.emitter.emit_mv(*phys_reg, result_tmp);
        }
    }

    fn lower_call(
        &mut self,
        dest: &Option<hll_to_ir::IrRegister>,
        function: &str,
        args: &[IrValue],
        ctx: &mut FunctionContext,
        alloc: &RegisterAllocator,
    ) {
        self.emitter.reset_temp_counter();
        let func_return_type = self
            .function_return_types
            .get(function)
            .cloned()
            .unwrap_or(IrType::Integer(hll_to_ir::IntWidth::I64));
        let resolved_ret_ty = self.resolve_ir_type(&func_return_type);
        let is_agg_return = matches!(resolved_ret_ty, IrType::Aggregate(_) | IrType::Array { .. });
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
            let arg_tmp = self.load_value_to_temp(arg, ctx, alloc);
            self.emitter.emit_mv(reg_for_arg(arg_index), arg_tmp);
            arg_index += 1;
        }

        self.emitter.emit_jal(RA, function);

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
                        let reg = if i == 0 { A0 } else { A1 };
                        if field_size >= 8 {
                            self.emitter
                                .emit_sd(SP, reg, (dest_slot + field_offset) as i32);
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
                        field_offset = (field_offset.div_ceil(align) * align) + field_size;
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
        } else if !is_agg_return && let Some(dest) = dest {
            let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
            let resolved_return_ty = self.resolve_ir_type(&func_return_type);
            self.emitter
                .emit_store_from_tmp(SP, A0, &resolved_return_ty, dest_slot as i32);
            // If dest has a physical register allocation, also store there
            if let Some(Allocation::Physical(phys_reg)) = alloc.get_allocation(dest) {
                self.emitter.emit_mv(*phys_reg, A0);
            }
        }
        self.emitter
            .emit_comment(&format!("--- End Function Call: {function} ---"));
    }

    fn lower_global_ref(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        name: &str,
        ctx: &mut FunctionContext,
        alloc: &RegisterAllocator,
    ) {
        self.emitter.reset_temp_counter();
        let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
        let temp = self.emitter.alloc_temp_reg();
        self.emitter
            .emit_raw(&format!("\tla {}, {}", reg_name(temp, false), name));
        self.emitter.emit_sd(SP, temp, dest_slot as i32);
        if let Some(Allocation::Physical(phys_reg)) = alloc.get_allocation(dest) {
            self.emitter.emit_mv(*phys_reg, temp);
        }
    }

    fn lower_heap_alloc(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        ty: &IrType,
        count: Option<usize>,
        ctx: &mut FunctionContext,
        alloc: &RegisterAllocator,
    ) {
        self.emitter.reset_temp_counter();
        let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
        let bytes = self.type_size(ty).saturating_mul(count.unwrap_or(1));
        self.emitter.emit_li(A0, bytes as i64);
        self.emitter.emit_raw("\tcall malloc");
        self.emitter.emit_sd(SP, A0, dest_slot as i32);
        // If dest has a physical register allocation, also store there
        if let Some(Allocation::Physical(phys_reg)) = alloc.get_allocation(dest) {
            self.emitter.emit_mv(*phys_reg, A0);
        }
    }

    fn lower_heap_free(
        &mut self,
        ptr: &hll_to_ir::IrRegister,
        ctx: &mut FunctionContext,
        alloc: &RegisterAllocator,
    ) {
        self.emitter.reset_temp_counter();
        let ptr_tmp = self.load_value_to_temp(&IrValue::Register(ptr.clone()), ctx, alloc);
        self.emitter.emit_mv(A0, ptr_tmp);
        self.emitter.emit_raw("\tcall free");
    }

    fn lower_read_reg(
        &mut self,
        dest: &hll_to_ir::IrRegister,
        reg: &str,
        ctx: &mut FunctionContext,
        alloc: &RegisterAllocator,
    ) {
        use asm_to_binary::parse_int_reg;
        self.emitter.reset_temp_counter();
        let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
        let src_hw = parse_int_reg(reg).expect("asm_reg: register validated by semantic analysis");
        let tmp = self.emitter.alloc_temp_reg();
        self.emitter.emit_mv(tmp, src_hw);
        self.emitter.emit_sd(SP, tmp, dest_slot as i32);
        if let Some(Allocation::Physical(phys_reg)) = alloc.get_allocation(dest) {
            self.emitter.emit_mv(*phys_reg, src_hw);
        }
    }

    fn lower_terminator(
        &mut self,
        term: &IrTerminator,
        ctx: &mut FunctionContext,
        alloc: &RegisterAllocator,
        needs_sret: bool,
        _is_aggregate: bool,
    ) {
        match term {
            IrTerminator::Return(val) => self.lower_return(val.as_ref(), needs_sret, ctx, alloc),
            IrTerminator::Jump(label) => {
                let lbl = ctx.get_label(label).unwrap();
                self.emitter.emit_jal(ZERO, lbl);
            }
            IrTerminator::Branch {
                cond,
                then_label,
                else_label,
            } => {
                let cond_tmp = self.load_value_to_temp(cond, ctx, alloc);
                let then_lbl = ctx.get_label(then_label).unwrap();
                let else_lbl = ctx.get_label(else_label).unwrap();
                self.emitter.emit_bne(cond_tmp, ZERO, then_lbl);
                self.emitter.emit_jal(ZERO, else_lbl);
            }
        }
    }

    fn lower_return(
        &mut self,
        val: Option<&IrValue>,
        needs_sret: bool,
        ctx: &mut FunctionContext,
        alloc: &RegisterAllocator,
    ) {
        if let Some(val) = val {
            let raw_val_type = self.resolve_value_type(val, ctx);
            let resolved_val = match (&raw_val_type, val) {
                (IrType::Pointer(inner), IrValue::Register(reg)) if ctx.is_stack_address(reg) => {
                    self.resolve_ir_type(inner)
                }
                _ => raw_val_type.clone(),
            };
            let is_agg_return = matches!(resolved_val, IrType::Aggregate(_) | IrType::Array { .. });

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
                    self.load_pointer_operand_to_temp(reg, ctx, alloc)
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
                if let IrType::Aggregate(fields) = &resolved_val {
                    let mut field_offset = 0;
                    for (i, (_, field_ty)) in fields.iter().enumerate() {
                        let resolved_field_ty = self.resolve_ir_type(field_ty);
                        let field_size = self.type_size(&resolved_field_ty);
                        let ret_reg = if i == 0 { A0 } else { A1 };
                        if field_size >= 8 {
                            self.emitter
                                .emit_ld(ret_reg, SP, (slot + field_offset) as i32);
                        } else if field_size >= 4 {
                            self.emitter
                                .emit_lw(ret_reg, SP, (slot + field_offset) as i32);
                        } else if field_size >= 2 {
                            self.emitter
                                .emit_lh(ret_reg, SP, (slot + field_offset) as i32);
                        } else if field_size >= 1 {
                            self.emitter
                                .emit_lb(ret_reg, SP, (slot + field_offset) as i32);
                        }
                        let align = self.type_alignment(&resolved_field_ty);
                        field_offset = (field_offset.div_ceil(align) * align) + field_size;
                    }
                } else {
                    let size = self.type_size(&resolved_val);
                    if size >= 8 {
                        self.emitter.emit_ld(A0, SP, slot as i32);
                    } else if size >= 4 {
                        self.emitter.emit_lw(A0, SP, slot as i32);
                    }
                }
            } else {
                let resolved_val = self.resolve_value_type(val, ctx);
                if matches!(resolved_val, IrType::Float(_)) {
                    let val_fp = self.load_float_value_to_temp(val, ctx, alloc);
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
                    let val_tmp = self.load_value_to_temp(val, ctx, alloc);
                    self.emitter.emit_mv(A0, val_tmp);
                }
            }
        }
        ctx.emit_epilogue(&mut self.emitter);
    }

    // ---------- helpers that remain in the compiler (use emitter only) ----------
    fn resolve_ptr_to_addr(
        &mut self,
        ptr: &hll_to_ir::IrRegister,
        ctx: &FunctionContext,
        byte_offset: Option<i32>,
        alloc: &RegisterAllocator,
    ) -> Reg {
        // Check if pointer is in a physical register
        if let Some(alloc_result) = alloc.get_allocation(ptr)
            && let Allocation::Physical(phys_reg) = alloc_result
        {
            let tmp = self.emitter.alloc_temp_reg();
            if let Some(off) = byte_offset {
                if off != 0 {
                    self.emitter.emit_add_imm(tmp, *phys_reg, off as i64);
                } else {
                    self.emitter.emit_mv(tmp, *phys_reg);
                }
            } else {
                self.emitter.emit_mv(tmp, *phys_reg);
            }
            return tmp;
        }

        // Fall back to stack slot
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

    fn load_value_to_temp(
        &mut self,
        val: &IrValue,
        ctx: &FunctionContext,
        alloc: &RegisterAllocator,
    ) -> Reg {
        let temp = self.emitter.alloc_temp_reg();
        match val {
            IrValue::Register(reg) => {
                // Check if this register has a physical register allocation
                if let Some(alloc_result) = alloc.get_allocation(reg) {
                    match alloc_result {
                        Allocation::Physical(phys_reg) => {
                            // Value is in a physical register, copy it to our temp
                            self.emitter.emit_mv(temp, *phys_reg);
                            return temp;
                        }
                        Allocation::StackSlot(slot) => {
                            // Load from stack slot
                            if ctx.is_stack_address(reg) {
                                self.emitter.emit_add_imm(temp, SP, *slot as i64);
                            } else {
                                let ty = ctx
                                    .type_for_reg(reg)
                                    .unwrap_or(IrType::Integer(hll_to_ir::IntWidth::I64));
                                self.emitter.emit_load_from_slot(temp, *slot, &ty);
                            }
                            return temp;
                        }
                    }
                }

                // Fallback to old behavior if no allocation found
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

    fn load_float_value_to_temp(
        &mut self,
        val: &IrValue,
        ctx: &FunctionContext,
        alloc: &RegisterAllocator,
    ) -> Reg {
        let temp = self.emitter.alloc_float_temp_reg();
        match val {
            IrValue::Register(reg) => {
                // Check if this register has a physical register allocation
                if let Some(alloc_result) = alloc.get_allocation(reg)
                    && let Allocation::Physical(phys_reg) = alloc_result
                {
                    // Value is in a physical register, copy it to our temp
                    // For floats, we need to use the appropriate move instruction
                    let ty = ctx
                        .type_for_reg(reg)
                        .unwrap_or(IrType::Float(hll_to_ir::FloatWidth::F32));
                    match ty {
                        IrType::Float(hll_to_ir::FloatWidth::F32) => {
                            self.emitter.emit_fmv_s(temp, *phys_reg);
                        }
                        IrType::Float(hll_to_ir::FloatWidth::F64) => {
                            self.emitter
                                .emit_inst(RealInstruction::FsgnjD(fmv_d(temp, *phys_reg)));
                        }
                        _ => panic!("Expected float type"),
                    }
                    return temp;
                }

                // Load from stack
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
                self.emitter.emit_li(int_tmp, f.to_bits() as i64);
                self.emitter.emit_fmv_w_x(temp, int_tmp);
            }
            _ => panic!("Unsupported float value: {val:?}"),
        }
        temp
    }

    fn load_pointer_operand_to_temp(
        &mut self,
        reg: &hll_to_ir::IrRegister,
        ctx: &FunctionContext,
        alloc: &RegisterAllocator,
    ) -> Reg {
        let temp = self.emitter.alloc_temp_reg();
        // Check if this register has a physical register allocation
        if let Some(alloc_result) = alloc.get_allocation(reg)
            && let Allocation::Physical(phys_reg) = alloc_result
        {
            // Value is in a physical register, copy it to our temp
            self.emitter.emit_mv(temp, *phys_reg);
            return temp;
        }

        // Load from stack
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

    fn type_alignment(&self, ty: &IrType) -> usize {
        type_utils::type_alignment(ty, &self.type_aliases)
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
