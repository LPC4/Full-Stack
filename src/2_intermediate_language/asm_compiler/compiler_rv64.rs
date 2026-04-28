use super::{
    assembly_emitter::AssemblyEmitter, data_section::DataSection,
    function_context::FunctionContext, register_allocator::RegisterAllocator,
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
use log::warn;
use std::collections::{HashMap, HashSet};

const ZERO: Reg = 0;
const RA: Reg = 1;
const SP: Reg = 2;
const S0: Reg = 8;
const A0: Reg = 10;

// Float register constants
const FT0: Reg = 0;  // ft0
const FA0: Reg = 10; // fa0

pub struct CompilerRv64 {
    emitter: AssemblyEmitter,
    data: DataSection,
    type_aliases: HashMap<String, IrType>,
    function_return_types: HashMap<String, IrType>,
    float_temp_counter: usize, // Separate counter for float temp registers
}

impl CompilerRv64 {
    pub fn new() -> Self {
        Self {
            emitter: AssemblyEmitter::new(),
            data: DataSection::new(),
            type_aliases: HashMap::new(),
            function_return_types: HashMap::new(),
            float_temp_counter: 0,
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
        self.emitter.emit_functions();

        self.emitter.finish()
    }

    fn compile_function(&mut self, func: &crate::intermediate_language::IrFunction) {
        let mut ctx = FunctionContext::new(&func.name, &self.type_aliases);
        let mut alloc = RegisterAllocator::new();
        alloc.allocate_slots(func, &mut ctx, &self.function_return_types);
        
        // Check if function returns an aggregate type - if so, determine return strategy
        let return_type = self.resolve_ir_type(&func.return_type);
        let is_aggregate = matches!(return_type, IrType::Aggregate(_) | IrType::Array { .. });
        
        // Small structs (≤16 bytes) are returned in registers, larger ones use sret
        let needs_sret = is_aggregate && !self.can_return_in_registers(&return_type);
        
        // If we need sret, save an extra callee-saved register to hold the sret pointer
        if needs_sret {
            ctx.save_reg(9); // Save s1 (callee-saved) to hold sret pointer
        }
        
        ctx.save_ra();
        ctx.save_reg(S0);
        ctx.finalize();

        for block in &func.blocks {
            ctx.map_label(&block.label, format!("{}__{}", func.name, block.label.0));
        }

        self.emitter.start_function(&func.name);
        ctx.emit_prologue(&mut self.emitter);
        
        // If this function returns a large aggregate, save the sret pointer from a0 to s1
        if needs_sret {
            self.emit_mv(9, A0); // Save sret pointer in s1
        }
        
        // FIXED: Use emit_parameter_spills_with_sret when needs_sret is true
        if needs_sret {
            // Allocate a slot for the sret pointer parameter
            let sret_slot = ctx.frame.alloc_slot(8, 8);
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

        // REMOVED: Epilogue now emitted per-return in lower_terminator
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
                self.emitter.reset_temp_counter();
                self.float_temp_counter = 0;
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                let ptr_tmp = self.load_pointer_operand_to_temp(ptr, ctx);
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
                self.emitter.reset_temp_counter();
                self.float_temp_counter = 0;
                // Compute the destination address.
                // If ptr is a stack_address register, its "value" IS the address
                // (sp + slot). We must NOT dereference it a second time.
                let addr_tmp = self.resolve_ptr_to_addr(ptr, ctx, offset.map(|o| o as i32));

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
                self.emitter.reset_temp_counter();
                self.float_temp_counter = 0;
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                let ptr_tmp = self.load_pointer_operand_to_temp(ptr, ctx);
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
                self.emitter.reset_temp_counter();
                self.float_temp_counter = 0;
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                let base_tmp = self.load_pointer_operand_to_temp(base_ptr, ctx);
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
                ty,
                lhs,
                rhs,
            } => {
                self.emitter.reset_temp_counter();
                self.float_temp_counter = 0;
                // CRITICAL FIX: Ensure math operations only work with scalar types
                let resolved_ty = self.resolve_ir_type(ty);
                if matches!(resolved_ty, IrType::Aggregate(_) | IrType::Array { .. }) {
                    panic!(
                        "Math operations cannot be performed on aggregate/array type {:?}",
                        resolved_ty
                    );
                }
                
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                
                // FIXED: Dispatch to float or integer path based on type
                if matches!(resolved_ty, IrType::Float(crate::intermediate_language::FloatWidth::F32)) {
                    // Float arithmetic path
                    let lhs_fp = self.load_float_value_to_temp(lhs, ctx);
                    let rhs_fp = self.load_float_value_to_temp(rhs, ctx);
                    let result_fp = self.alloc_float_temp_reg();
                    match op {
                        IrMathOp::Add => self.emit_fadd_s(result_fp, lhs_fp, rhs_fp),
                        IrMathOp::Sub => self.emit_fsub_s(result_fp, lhs_fp, rhs_fp),
                        IrMathOp::Mul => self.emit_fmul_s(result_fp, lhs_fp, rhs_fp),
                        IrMathOp::Div | IrMathOp::SDiv => self.emit_fdiv_s(result_fp, lhs_fp, rhs_fp),
                        _ => panic!("Unsupported float math op {:?}", op),
                    }
                    // Store float result back to slot via fsw
                    self.emit_fsw(SP, result_fp, dest_slot as i32);
                } else {
                    // Integer arithmetic path (existing)
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
                    // FIXED: Use type-aware store instead of always sd
                    self.emit_store_from_tmp(SP, result_tmp, &resolved_ty, dest_slot as i32);
                }
            }
            Unary {
                dest,
                op,
                ty,
                value,
            } => {
                self.emitter.reset_temp_counter();
                self.float_temp_counter = 0;
                // CRITICAL FIX: Ensure unary operations only work with scalar types
                let resolved_ty = self.resolve_ir_type(ty);
                if matches!(resolved_ty, IrType::Aggregate(_) | IrType::Array { .. }) {
                    panic!(
                        "Unary operations cannot be performed on aggregate/array type {:?}",
                        resolved_ty
                    );
                }
                
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                
                // FIXED: Dispatch to float or integer path based on type
                if matches!(resolved_ty, IrType::Float(crate::intermediate_language::FloatWidth::F32)) {
                    // Float unary operations
                    let val_fp = self.load_float_value_to_temp(value, ctx);
                    let result_fp = self.alloc_float_temp_reg();
                    match op {
                        IrUnaryOp::Neg => {
                            // Float negation using fsgnjn.s (negate sign bit)
                            use crate::assembly_language::riscv::rv64fd::Fsgnjn;
                            self.emitter.emit_inst(RealInstruction::Fsgnjn(Fsgnjn::new(result_fp, val_fp, val_fp)));
                        }
                        IrUnaryOp::Not => panic!("Bitwise not not supported for floats"),
                    }
                    self.emit_fsw(SP, result_fp, dest_slot as i32);
                } else {
                    // Integer unary operations (existing)
                    let val_tmp = self.load_value_to_temp(value, ctx);
                    let result_tmp = self.alloc_temp_reg();
                    match op {
                        IrUnaryOp::Neg => self.emit_neg(result_tmp, val_tmp),
                        IrUnaryOp::Not => self.emit_not(result_tmp, val_tmp),
                    }
                    // FIXED: Use type-aware store instead of always sd
                    self.emit_store_from_tmp(SP, result_tmp, &resolved_ty, dest_slot as i32);
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
                self.float_temp_counter = 0;
                // CRITICAL FIX: Ensure comparison operations only work with scalar types
                let resolved_ty = self.resolve_ir_type(ty);
                if matches!(resolved_ty, IrType::Aggregate(_) | IrType::Array { .. }) {
                    panic!(
                        "Comparison operations cannot be performed on aggregate/array type {:?}",
                        resolved_ty
                    );
                }
                
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                
                // FIXED: Dispatch to float or integer path based on type
                if matches!(resolved_ty, IrType::Float(crate::intermediate_language::FloatWidth::F32)) {
                    // Float comparison - result is always i1 (stored as i8)
                    let lhs_fp = self.load_float_value_to_temp(lhs, ctx);
                    let rhs_fp = self.load_float_value_to_temp(rhs, ctx);
                    let result_tmp = self.alloc_temp_reg(); // Integer register for comparison result
                    match op {
                        IrCmpOp::Eq => self.emit_feq_s(result_tmp, lhs_fp, rhs_fp),
                        IrCmpOp::Ne => {
                            // feq then not
                            let tmp = self.alloc_temp_reg();
                            self.emit_feq_s(tmp, lhs_fp, rhs_fp);
                            self.emit_not(result_tmp, tmp);
                        }
                        IrCmpOp::Slt | IrCmpOp::Ult => self.emit_flt_s(result_tmp, lhs_fp, rhs_fp),
                        IrCmpOp::Sle | IrCmpOp::Ule => self.emit_fle_s(result_tmp, lhs_fp, rhs_fp),
                        IrCmpOp::Sgt | IrCmpOp::Ugt => self.emit_flt_s(result_tmp, rhs_fp, lhs_fp),
                        IrCmpOp::Sge | IrCmpOp::Uge => self.emit_fle_s(result_tmp, rhs_fp, lhs_fp),
                    }
                    // Comparison results are always i1 (stored as i8/sb)
                    self.emit_store_from_tmp(SP, result_tmp, &IrType::Integer(crate::intermediate_language::IntWidth::I1), dest_slot as i32);
                } else {
                    // Integer comparison (existing)
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
                    // FIXED: Comparison results are always i1 (stored as i8/sb)
                    self.emit_store_from_tmp(SP, result_tmp, &IrType::Integer(crate::intermediate_language::IntWidth::I1), dest_slot as i32);
                }
            }
            Cast {
                dest,
                mode,
                value,
                ty,
            } => {
                self.emitter.reset_temp_counter();
                self.float_temp_counter = 0;
                // CRITICAL FIX: Ensure casts only work with scalar types
                let resolved_ty = self.resolve_ir_type(ty);
                if matches!(resolved_ty, IrType::Aggregate(_) | IrType::Array { .. }) {
                    panic!(
                        "Cast operations cannot be performed on aggregate/array type {:?}",
                        resolved_ty
                    );
                }
                
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                let src_tmp = self.load_value_to_temp(value, ctx);
                let result_tmp = self.alloc_temp_reg();
                self.lower_cast(result_tmp, src_tmp, *mode, ty);
                // FIXED: Use type-aware store instead of always sd
                self.emit_store_from_tmp(SP, result_tmp, &resolved_ty, dest_slot as i32);
            }
            Call {
                dest,
                function,
                args,
            } => {
                self.emitter.reset_temp_counter();
                self.float_temp_counter = 0;
                // Check if the called function returns an aggregate type
                let func_return_type = self.function_return_types
                    .get(function)
                    .cloned()
                    .unwrap_or(IrType::Integer(crate::intermediate_language::IntWidth::I64));
                let resolved_ret_ty = self.resolve_ir_type(&func_return_type);
                let is_agg_return = matches!(resolved_ret_ty, IrType::Aggregate(_) | IrType::Array { .. });
                
                // Small structs (≤16 bytes) are returned in registers, larger ones use sret
                let needs_sret = is_agg_return && !self.can_return_in_registers(&resolved_ret_ty);
                
                let mut arg_index = 0;
                
                // If the function returns a large aggregate, we need to pass a hidden sret pointer as the first argument
                if needs_sret {
                    if let Some(dest_reg) = dest {
                        // The destination should be a stack_address register pointing to allocated memory
                        let dest_slot = ctx.slot_for_reg(dest_reg).expect("dest slot for sret");
                        // Compute the address: sp + slot
                        let sret_ptr = self.alloc_temp_reg();
                        self.emit_add_imm(sret_ptr, SP, dest_slot as i64);
                        // Pass it as the first argument (a0)
                        self.emit_mv(reg_for_arg(0), sret_ptr);
                        arg_index = 1; // Start regular args from a1
                    } else {
                        panic!("Call with aggregate return must have a destination");
                    }
                }
                
                // Pass regular arguments
                for arg in args.iter() {
                    if arg_index >= 8 {
                        break;
                    }
                    let arg_tmp = self.load_value_to_temp(arg, ctx);
                    self.emit_mv(reg_for_arg(arg_index), arg_tmp);
                    arg_index += 1;
                }
                
                self.emit_jal(RA, function.as_str());
                
                // For small aggregate returns, load the result from a0/a1 into the destination slot
                if is_agg_return && !needs_sret {
                    if let Some(dest_reg) = dest {
                        let dest_slot = ctx.slot_for_reg(dest_reg).expect("dest slot");
                        
                        // FIXED: Unpack fields from a0/a1 based on field layout
                        if let IrType::Aggregate(fields) = &resolved_ret_ty {
                            let mut field_offset = 0;
                            for (i, (_, field_ty)) in fields.iter().enumerate() {
                                let resolved_field_ty = self.resolve_ir_type(field_ty);
                                let field_size = self.type_size(&resolved_field_ty);
                                let reg = if i == 0 { A0 } else { 11 }; // a0 or a1
                                
                                // Store field based on its size
                                if field_size >= 8 {
                                    self.emit_sd(SP, reg, (dest_slot + field_offset) as i32);
                                } else if field_size >= 4 {
                                    self.emitter.emit_inst(RealInstruction::Sw(Sw::new(SP, reg, (dest_slot + field_offset) as i32)));
                                } else if field_size >= 2 {
                                    self.emitter.emit_inst(RealInstruction::Sh(Sh::new(SP, reg, (dest_slot + field_offset) as i32)));
                                } else if field_size >= 1 {
                                    self.emitter.emit_inst(RealInstruction::Sb(Sb::new(SP, reg, (dest_slot + field_offset) as i32)));
                                }
                                
                                // Align to next field
                                let align = self.type_alignment(&resolved_field_ty);
                                field_offset = ((field_offset + align - 1) / align * align) + field_size;
                            }
                        } else {
                            // Fallback for non-aggregate types
                            let size = self.type_size(&resolved_ret_ty);
                            if size >= 8 {
                                self.emit_sd(SP, A0, dest_slot as i32);
                            } else if size >= 4 {
                                self.emitter.emit_inst(RealInstruction::Sw(Sw::new(SP, A0, dest_slot as i32)));
                            }
                        }
                    }
                } else if !is_agg_return {
                    // For scalar returns, store a0 to destination
                    if let Some(dest) = dest {
                        let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                        self.emit_sd(SP, A0, dest_slot as i32);
                    }
                }
                // For sret returns, the result is already in the memory pointed to by sret
            }
            Phi { .. } => {}
            HeapAlloc { dest, ty, count } => {
                self.emitter.reset_temp_counter();
                self.float_temp_counter = 0;
                let dest_slot = ctx.slot_for_reg(dest).expect("dest slot");
                let elem_count = count.unwrap_or(1);
                let bytes = self.type_size(ty).saturating_mul(elem_count);

                self.emit_li(A0, bytes as i64);
                self.emitter.emit_raw("\tcall malloc");
                self.emit_sd(SP, A0, dest_slot as i32);
            }
            HeapFree { ptr } => {
                self.emitter.reset_temp_counter();
                self.float_temp_counter = 0;
                let ptr_tmp = self.load_value_to_temp(&IrValue::Register(ptr.clone()), ctx);
                self.emit_mv(A0, ptr_tmp);
                self.emitter.emit_raw("\tcall free");
            }
        }
    }

    fn lower_terminator(&mut self, term: &IrTerminator, ctx: &mut FunctionContext, needs_sret: bool, is_aggregate: bool) {
        match term {
            IrTerminator::Return(val) => {
                if let Some(val) = val {
                    let resolved_val = self.resolve_value_type(val, ctx);
                    let is_agg_return = matches!(resolved_val, IrType::Aggregate(_) | IrType::Array { .. });
                    
                    if needs_sret && is_agg_return {
                        // For large aggregate returns using sret pattern:
                        // The value should be a stack_address register pointing to the aggregate data
                        // We need to copy it to the sret pointer passed by the caller (saved in s1)
                        match val {
                            IrValue::Register(reg) => {
                                // Get the address of the aggregate data
                                let src_addr = if ctx.is_stack_address(reg) {
                                    // It's already an address (sp + slot)
                                    let slot = ctx.slot_for_reg(reg).expect("reg slot");
                                    let addr_tmp = self.alloc_temp_reg();
                                    self.emit_add_imm(addr_tmp, SP, slot as i64);
                                    addr_tmp
                                } else {
                                    // Load the pointer from the slot
                                    self.load_pointer_operand_to_temp(reg, ctx)
                                };
                                
                                // The sret pointer was saved in s1 (callee-saved register)
                                let sret_ptr = 9; // s1
                                
                                // Copy the aggregate data from src_addr to sret location
                                let size = self.type_size(&resolved_val);
                                self.copy_bytes_from_addr_to_addr(sret_ptr, 0, src_addr, 0, size);
                            }
                            _ => panic!("Aggregate return must be a register"),
                        }
                        // Don't set A0 for sret returns - caller already has the pointer
                    } else if is_aggregate && !needs_sret {
                        // Small struct returned in registers (a0/a1)
                        // FIXED: Load each field into separate registers based on field layout
                        match val {
                            IrValue::Register(reg) => {
                                let slot = ctx.slot_for_reg(reg).expect("reg slot");
                                
                                // For a two-field struct of i32s like {quotient: i32, remainder: i32}:
                                // Field 0 at offset 0 -> a0 via lw
                                // Field 1 at offset 4 -> a1 via lw
                                if let IrType::Aggregate(fields) = &resolved_val {
                                    let mut field_offset = 0;
                                    for (i, (_, field_ty)) in fields.iter().enumerate() {
                                        let resolved_field_ty = self.resolve_ir_type(field_ty);
                                        let field_size = self.type_size(&resolved_field_ty);
                                        let reg = if i == 0 { A0 } else { 11 }; // a0 or a1
                                        
                                        // Load field based on its size
                                        if field_size >= 8 {
                                            self.emit_ld(reg, SP, (slot + field_offset) as i32);
                                        } else if field_size >= 4 {
                                            self.emit_lw(reg, SP, (slot + field_offset) as i32);
                                        } else if field_size >= 2 {
                                            self.emit_lh(reg, SP, (slot + field_offset) as i32);
                                        } else if field_size >= 1 {
                                            self.emit_lb(reg, SP, (slot + field_offset) as i32);
                                        }
                                        
                                        // Align to next field (natural alignment)
                                        let align = self.type_alignment(&resolved_field_ty);
                                        field_offset = ((field_offset + align - 1) / align * align) + field_size;
                                    }
                                } else {
                                    // Fallback for non-aggregate types (shouldn't happen here)
                                    let size = self.type_size(&resolved_val);
                                    if size >= 8 {
                                        self.emit_ld(A0, SP, slot as i32);
                                    } else if size >= 4 {
                                        self.emit_lw(A0, SP, slot as i32);
                                    }
                                }
                            }
                            _ => panic!("Small aggregate return must be a register"),
                        }
                    } else {
                        // Scalar return value
                        let resolved_val = self.resolve_value_type(val, ctx);
                        if matches!(resolved_val, IrType::Float(_)) {
                            // FIXED: Float returns go in fa0, not a0
                            let val_fp = self.load_float_value_to_temp(val, ctx);
                            // Use appropriate move based on float width
                            match resolved_val {
                                IrType::Float(crate::intermediate_language::FloatWidth::F32) => {
                                    self.emit_fmv_s(FA0, val_fp);
                                }
                                IrType::Float(crate::intermediate_language::FloatWidth::F64) => {
                                    // For f64, we still use fa0 (f10) but need fmv.d
                                    use crate::assembly_language::riscv::rv64fd::fmv_d;
                                    self.emitter.emit_inst(RealInstruction::FsgnjD(fmv_d(FA0, val_fp)));
                                }
                                _ => unreachable!(),
                            }
                        } else {
                            let val_tmp = self.load_value_to_temp(val, ctx);
                            self.emit_mv(A0, val_tmp);
                        }
                    }
                }
                // FIXED: Emit epilogue before every return
                ctx.emit_epilogue(&mut self.emitter);
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
                // FIXED: Branch to then_label when condition is non-zero
                self.emit_bne(cond_tmp, ZERO, then_lbl);
                self.emit_jal(ZERO, else_lbl);
            }
        }
    }

    // -------------------------------------------------------------------------
    // Key helper: resolve a pointer register + optional immediate offset
    // into an address held in a temporary register.
    //
    // The critical distinction:
    //   - stack_address registers: their VALUE is already an address computed
    //     as (sp + slot). We use `addi` to offset from the stack pointer.
    //   - normal pointer registers: their VALUE is a pointer stored in the
    //     stack slot. We `ld` the pointer, then optionally add the offset.
    // -------------------------------------------------------------------------
    fn resolve_ptr_to_addr(
        &mut self,
        ptr: &crate::intermediate_language::IrRegister,
        ctx: &FunctionContext,
        byte_offset: Option<i32>,
    ) -> Reg {
        let slot = ctx.slot_for_reg(ptr).expect("ptr slot");
        let tmp = self.alloc_temp_reg();

        if ctx.is_stack_address(ptr) {
            // The address IS (sp + slot). Apply any extra byte offset immediately.
            let total_offset = slot as i64 + byte_offset.unwrap_or(0) as i64;
            self.emit_add_imm(tmp, SP, total_offset);
        } else {
            // Load the pointer value from the stack slot.
            self.emit_ld(tmp, SP, slot as i32);
            // Then add the byte offset if present.
            if let Some(off) = byte_offset {
                if off != 0 {
                    self.emit_add_imm(tmp, tmp, off as i64);
                }
            }
        }
        tmp
    }

    // -------------------------------------------------------------------------
    // RISC‑V instruction emission helpers
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
    fn emit_add_imm(&mut self, rd: Reg, rs: Reg, imm: i64) {
        if (-2048..=2047).contains(&imm) {
            self.emit_addi(rd, rs, imm as i32);
        } else {
            let tmp = self.alloc_temp_reg();
            self.emit_li(tmp, imm);
            self.emit_add(rd, rs, tmp);
        }
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
    fn emit_fmv_w_x(&mut self, fd: Reg, rs: Reg) {
        self.emitter
            .emit_inst(RealInstruction::FmvWX(FmvWX::new(fd, rs)));
    }

    // Float arithmetic emission helpers
    fn emit_fadd_s(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emitter
            .emit_inst(RealInstruction::Fadd(Fadd::new(rd, rs1, rs2)));
    }

    fn emit_fsub_s(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emitter
            .emit_inst(RealInstruction::Fsub(Fsub::new(rd, rs1, rs2)));
    }

    fn emit_fmul_s(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emitter
            .emit_inst(RealInstruction::Fmul(Fmul::new(rd, rs1, rs2)));
    }

    fn emit_fdiv_s(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        self.emitter
            .emit_inst(RealInstruction::Fdiv(Fdiv::new(rd, rs1, rs2)));
    }

    fn emit_fsw(&mut self, base: Reg, src: Reg, offset: i32) {
        self.emitter
            .emit_inst(RealInstruction::Fsw(Fsw::new(base, src, offset)));
    }

    // Float move (copy between FP registers)
    fn emit_fmv_s(&mut self, rd: Reg, rs: Reg) {
        use crate::assembly_language::riscv::rv64fd::fmv_s;
        self.emitter.emit_inst(RealInstruction::Fsgnj(fmv_s(rd, rs)));
    }

    // Float comparison emission helpers
    fn emit_feq_s(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        use crate::assembly_language::riscv::rv64fd::FeqS;
        self.emitter.emit_inst(RealInstruction::FeqS(FeqS::new(rd, rs1, rs2)));
    }

    fn emit_flt_s(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        use crate::assembly_language::riscv::rv64fd::FltS;
        self.emitter.emit_inst(RealInstruction::FltS(FltS::new(rd, rs1, rs2)));
    }

    fn emit_fle_s(&mut self, rd: Reg, rs1: Reg, rs2: Reg) {
        use crate::assembly_language::riscv::rv64fd::FleqS;
        self.emitter.emit_inst(RealInstruction::FleqS(FleqS::new(rd, rs1, rs2)));
    }

    fn copy_bytes_from_addr_to_slot(
        &mut self,
        slot: usize,
        addr_reg: Reg,
        offset: i32,
        size: usize,
    ) {
        // OPTIMIZED: Use 64-bit loads/stores when possible instead of byte-by-byte
        let mut remaining = size;
        let mut current_offset = offset;
        let mut current_slot = slot;
        
        // Copy in 8-byte chunks using ld/sd
        while remaining >= 8 {
            let tmp = self.alloc_temp_reg();
            self.emitter.emit_inst(RealInstruction::Ld(Ld::new(
                tmp,
                addr_reg,
                current_offset,
            )));
            self.emitter.emit_inst(RealInstruction::Sd(Sd::new(
                SP,
                tmp,
                current_slot as i32,
            )));
            remaining -= 8;
            current_offset += 8;
            current_slot += 8;
        }
        
        // Copy in 4-byte chunks using lw/sw
        while remaining >= 4 {
            let tmp = self.alloc_temp_reg();
            self.emitter.emit_inst(RealInstruction::Lw(Lw::new(
                tmp,
                addr_reg,
                current_offset,
            )));
            self.emitter.emit_inst(RealInstruction::Sw(Sw::new(
                SP,
                tmp,
                current_slot as i32,
            )));
            remaining -= 4;
            current_offset += 4;
            current_slot += 4;
        }
        
        // Copy remaining bytes individually
        let byte_tmp = self.alloc_temp_reg();
        for i in 0..remaining {
            self.emitter.emit_inst(RealInstruction::Lb(Lb::new(
                byte_tmp,
                addr_reg,
                current_offset + i as i32,
            )));
            self.emitter.emit_inst(RealInstruction::Sb(Sb::new(
                SP,
                byte_tmp,
                current_slot as i32 + i as i32,
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
        // OPTIMIZED: Use 64-bit loads/stores when possible instead of byte-by-byte
        let mut remaining = size;
        let mut current_offset = offset;
        let mut current_slot = slot;
        
        // Copy in 8-byte chunks using ld/sd
        while remaining >= 8 {
            let tmp = self.alloc_temp_reg();
            self.emitter.emit_inst(RealInstruction::Ld(Ld::new(
                tmp,
                SP,
                current_slot as i32,
            )));
            self.emitter.emit_inst(RealInstruction::Sd(Sd::new(
                addr_reg,
                tmp,
                current_offset,
            )));
            remaining -= 8;
            current_offset += 8;
            current_slot += 8;
        }
        
        // Copy in 4-byte chunks using lw/sw
        while remaining >= 4 {
            let tmp = self.alloc_temp_reg();
            self.emitter.emit_inst(RealInstruction::Lw(Lw::new(
                tmp,
                SP,
                current_slot as i32,
            )));
            self.emitter.emit_inst(RealInstruction::Sw(Sw::new(
                addr_reg,
                tmp,
                current_offset,
            )));
            remaining -= 4;
            current_offset += 4;
            current_slot += 4;
        }
        
        // Copy remaining bytes individually
        let byte_tmp = self.alloc_temp_reg();
        for i in 0..remaining {
            self.emitter.emit_inst(RealInstruction::Lb(Lb::new(
                byte_tmp,
                SP,
                current_slot as i32 + i as i32,
            )));
            self.emitter.emit_inst(RealInstruction::Sb(Sb::new(
                addr_reg,
                byte_tmp,
                current_offset + i as i32,
            )));
        }
    }

    /// Copy bytes from one memory address to another (both in registers)
    fn copy_bytes_from_addr_to_addr(
        &mut self,
        dst_addr: Reg,
        dst_offset: i32,
        src_addr: Reg,
        src_offset: i32,
        size: usize,
    ) {
        // OPTIMIZED: Use 64-bit loads/stores when possible instead of byte-by-byte
        let mut remaining = size;
        let mut current_dst_offset = dst_offset;
        let mut current_src_offset = src_offset;
        
        // Copy in 8-byte chunks using ld/sd
        while remaining >= 8 {
            let tmp = self.alloc_temp_reg();
            self.emitter.emit_inst(RealInstruction::Ld(Ld::new(
                tmp,
                src_addr,
                current_src_offset,
            )));
            self.emitter.emit_inst(RealInstruction::Sd(Sd::new(
                dst_addr,
                tmp,
                current_dst_offset,
            )));
            remaining -= 8;
            current_dst_offset += 8;
            current_src_offset += 8;
        }
        
        // Copy in 4-byte chunks using lw/sw
        while remaining >= 4 {
            let tmp = self.alloc_temp_reg();
            self.emitter.emit_inst(RealInstruction::Lw(Lw::new(
                tmp,
                src_addr,
                current_src_offset,
            )));
            self.emitter.emit_inst(RealInstruction::Sw(Sw::new(
                dst_addr,
                tmp,
                current_dst_offset,
            )));
            remaining -= 4;
            current_dst_offset += 4;
            current_src_offset += 4;
        }
        
        // Copy remaining bytes individually
        let byte_tmp = self.alloc_temp_reg();
        for i in 0..remaining {
            self.emitter.emit_inst(RealInstruction::Lb(Lb::new(
                byte_tmp,
                src_addr,
                current_src_offset + i as i32,
            )));
            self.emitter.emit_inst(RealInstruction::Sb(Sb::new(
                dst_addr,
                byte_tmp,
                current_dst_offset + i as i32,
            )));
        }
    }

    fn load_value_to_temp(&mut self, val: &IrValue, ctx: &FunctionContext) -> Reg {
        let temp = self.alloc_temp_reg();
        match val {
            IrValue::Register(reg) => {
                let slot = ctx.slot_for_reg(reg).expect("reg slot");
                if ctx.is_stack_address(reg) {
                    // The register represents the address of a stack slot,
                    // so produce (sp + slot) rather than dereferencing it.
                    self.emit_add_imm(temp, SP, slot as i64);
                } else {
                    let ty = ctx
                        .type_for_reg(reg)
                        .unwrap_or(IrType::Integer(crate::intermediate_language::IntWidth::I64));
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

    /// Load a float value into a float temporary register
    fn load_float_value_to_temp(&mut self, val: &IrValue, ctx: &FunctionContext) -> Reg {
        let temp = self.alloc_float_temp_reg();
        match val {
            IrValue::Register(reg) => {
                let slot = ctx.slot_for_reg(reg).expect("reg slot");
                // For floats, we need to use flw/fld instead of lw/ld
                let ty = ctx
                    .type_for_reg(reg)
                    .unwrap_or(IrType::Float(crate::intermediate_language::FloatWidth::F32));
                match ty {
                    IrType::Float(crate::intermediate_language::FloatWidth::F32) => {
                        self.emit_flw(temp, SP, slot as i32);
                    }
                    IrType::Float(crate::intermediate_language::FloatWidth::F64) => {
                        self.emit_fld(temp, SP, slot as i32);
                    }
                    _ => panic!("Expected float type for float register load"),
                }
            }
            IrValue::Float(f) => {
                // Load float constant by first loading bits as integer, then converting
                let int_tmp = self.alloc_temp_reg();
                self.emit_li(int_tmp, f.to_bits() as i64);
                self.emit_fmv_w_x(temp, int_tmp);
            }
            _ => panic!("Unsupported float value: {:?}", val),
        }
        temp
    }

    /// Load the pointer held by `reg` into a fresh temp.
    ///
    /// - stack_address: the "pointer" is `sp + slot`  →  use `addi`
    /// - normal:        the pointer is stored in the slot  →  use `ld`
    fn load_pointer_operand_to_temp(
        &mut self,
        reg: &crate::intermediate_language::IrRegister,
        ctx: &FunctionContext,
    ) -> Reg {
        let temp = self.alloc_temp_reg();
        let slot = ctx.slot_for_reg(reg).expect("reg slot");
        if ctx.is_stack_address(reg) {
            self.emit_add_imm(temp, SP, slot as i64);
        } else {
            self.emit_ld(temp, SP, slot as i32);
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

    fn emit_srai(&mut self, rd: Reg, rs1: Reg, shamt: u8) {
        self.emitter
            .emit_inst(RealInstruction::Srai(Srai::new(rd, rs1, shamt)));
    }

    fn alloc_temp_reg(&mut self) -> Reg {
        self.emitter.alloc_temp_reg()
    }

    fn alloc_float_temp_reg(&mut self) -> Reg {
        // Cycle through ft0-ft7 (regs 0-7)
        let reg = FT0 + (self.float_temp_counter as Reg % 8);
        self.float_temp_counter += 1;
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

    /// Resolve the type of an IR value
    fn resolve_value_type(&self, val: &IrValue, ctx: &FunctionContext) -> IrType {
        match val {
            IrValue::Register(reg) => {
                ctx.type_for_reg(reg)
                    .unwrap_or(IrType::Integer(crate::intermediate_language::IntWidth::I64))
            }
            IrValue::Integer(_) => IrType::Integer(crate::intermediate_language::IntWidth::I64),
            IrValue::Bool(_) => IrType::Integer(crate::intermediate_language::IntWidth::I1),
            IrValue::Float(_) => IrType::Float(crate::intermediate_language::FloatWidth::F64),
            IrValue::Null => IrType::Pointer(Box::new(IrType::Void)),
            IrValue::GlobalString(_) => IrType::Pointer(Box::new(IrType::Integer(crate::intermediate_language::IntWidth::I8))),
        }
    }

    /// Return the natural alignment of a (resolved) type, in bytes.
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

    /// Return the padded size of a type, respecting natural alignment of every
    /// field and the overall struct alignment (matching the C ABI).
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
            IrType::Array { len, element } => {
                // Each element occupies exactly its padded size.
                len * self.type_size(&element)
            }
            IrType::Aggregate(fields) => {
                // Walk fields respecting natural alignment, then round the
                // total up to the struct's own alignment so that arrays of
                // structs are correctly strided.
                let mut offset: usize = 0;
                let mut max_align: usize = 1;
                for (_, field_ty) in &fields {
                    let align = self.type_alignment(field_ty);
                    max_align = max_align.max(align);
                    // Pad to field alignment.
                    offset = (offset + align - 1) & !(align - 1);
                    offset += self.type_size(field_ty);
                }
                // Pad total to struct alignment.
                (offset + max_align - 1) & !(max_align - 1)
            }
            IrType::Named(_) => {
                warn!("Cannot compute size of unresolved named type; defaulting to 8");
                8
            }
        }
    }

    /// Determine if a struct can be returned in registers (a0/a1) instead of using sret.
    /// Small structs (≤16 bytes) are returned directly in registers according to RISC-V ABI.
    fn can_return_in_registers(&self, ty: &IrType) -> bool {
        let size = self.type_size(ty);
        size <= 16 && size > 0
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
