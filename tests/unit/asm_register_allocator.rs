use full_stack::high_level_language::compilation_pipeline::CompilationPipeline;
use full_stack::intermediate_language::{IrBlock, IrFunction, IrInstruction, IrMathOp, IrProgram, IrRegister, IrTerminator, IrType, IrValue, IntWidth};
use full_stack::intermediate_language::asm_compiler::compiler_rv64::CompilerRv64;

fn int32() -> IrType {
    IrType::Integer(IntWidth::I32)
}

#[test]
fn test_register_allocation_output() {
    let mut program = IrProgram::new("test");
    let mut func = IrFunction::new("main", int32());

    let mut entry = IrBlock::new("entry");
    
    // Allocate local variables
    entry.push_instruction(IrInstruction::Alloc {
        dest: IrRegister::Named("a".into()),
        ty: int32(),
        count: None,
    });
    entry.push_instruction(IrInstruction::Alloc {
        dest: IrRegister::Named("b".into()),
        ty: int32(),
        count: None,
    });
    
    // Store values
    entry.push_instruction(IrInstruction::Store {
        ty: int32(),
        value: IrValue::Integer(6),
        ptr: IrRegister::Named("a".into()),
        offset: None,
    });
    entry.push_instruction(IrInstruction::Store {
        ty: int32(),
        value: IrValue::Integer(7),
        ptr: IrRegister::Named("b".into()),
        offset: None,
    });
    
    // Load values for multiplication
    entry.push_instruction(IrInstruction::Load {
        dest: IrRegister::Named("x".into()),
        ty: int32(),
        ptr: IrRegister::Named("a".into()),
        offset: None,
    });
    entry.push_instruction(IrInstruction::Load {
        dest: IrRegister::Named("y".into()),
        ty: int32(),
        ptr: IrRegister::Named("b".into()),
        offset: None,
    });
    
    // Multiply
    entry.push_instruction(IrInstruction::Math {
        dest: IrRegister::Named("result".into()),
        op: IrMathOp::Mul,
        ty: int32(),
        lhs: IrValue::Register(IrRegister::Named("x".into())),
        rhs: IrValue::Register(IrRegister::Named("y".into())),
    });
    
    // Return result
    entry.set_terminator(IrTerminator::Return(Some(IrValue::Register(IrRegister::Named("result".into())))));
    func.push_block(entry);
    program.push_function(func);

    let mut compiler = CompilerRv64::new();
    let asm = compiler.compile(&program);
    
    println!("Generated Assembly:\n{}", asm);
    
    // Check that we have some register allocations (fewer stack operations)
    assert!(asm.contains("mul"), "Should contain mul instruction");
}

#[test]
fn test_dot_product_assembly() {
    let source = r#"
type Vec2 = { x: i32, y: i32 }

make_vec: (x: i32, y: i32) -> Vec2 {
    return { .x = x, .y = y }
}

dot: (a: Vec2, b: Vec2) -> i32 {
    return a.x * b.x + a.y * b.y
}

main: () -> i32 {
    a: Vec2 = make_vec(3, 4)
    b: Vec2 = make_vec(1, 2)
    d: i32 = dot(a, b)
    return d
}
"#;
    
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).expect("compile failed");
    let (asm_text, _) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
    
    println!("Generated Assembly:\n{}", asm_text);
}

#[test]
fn test_array_sum_assembly() {
    let source = r#"
main: () -> i32 {
    arr: i32[5]
    @arr[0] = 2
    @arr[1] = 4
    @arr[2] = 6
    @arr[3] = 8
    @arr[4] = 10
    stack_sum: i32 = @arr[0] + @arr[1] + @arr[2] + @arr[3] + @arr[4]
    return stack_sum
}
"#;
    
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).expect("compile failed");
    let (asm_text, _) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
    
    println!("Generated Assembly:\n{}", asm_text);
}

#[test]
fn test_function_call_assembly() {
    let source = r#"
add: (a: i32, b: i32) -> i32 {
    return a + b
}
main: () -> i32 {
    return add(10, 32)
}
"#;
    
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).expect("compile failed");
    let (asm_text, toks) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
    
    println!("Generated Assembly:\n{}", asm_text);
}

#[test]
fn test_pointer_assembly() {
    let source = r#"
main: () -> i32 {
    p: i32* = new(i32)
    @p = 99
    if @p != 99 {
        free(p)
        return 1
    }
    free(p)
    return 0
}
"#;
    
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).expect("compile failed");
    let (asm_text, _) = pipeline.compile_ir_to_assembly_with_tokens(&result.ir_program);
    
    println!("Generated Assembly:\n{}", asm_text);
}
