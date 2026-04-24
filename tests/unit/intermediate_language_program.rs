use full_stack::intermediate_language::block::IrBlock;
use full_stack::intermediate_language::instruction::{IrInstruction, IrTerminator};
use full_stack::intermediate_language::ops::IrMathOp;
use full_stack::intermediate_language::program::{
    IrFunction, IrGlobalString, IrParam, IrProgram, IrTypeAlias,
};
use full_stack::intermediate_language::types::{FloatWidth, IntWidth, IrType};
use full_stack::intermediate_language::values::{IrRegister, IrValue};

#[test]
fn pretty_print_program_has_registers_labels_and_tabs() {
    let mut program = IrProgram::new("demo");
    program.push_type_alias(IrTypeAlias {
        name: "Point".to_owned(),
        ty: IrType::Aggregate(vec![
            ("x".to_owned(), IrType::Float(FloatWidth::F32)),
            ("y".to_owned(), IrType::Float(FloatWidth::F32)),
        ]),
    });
    program.push_global_string(IrGlobalString {
        name: "hello".to_owned(),
        content: "hi".to_owned(),
    });

    let mut function = IrFunction::new("add_one", IrType::Integer(IntWidth::I32));
    function.push_param(IrParam {
        ty: IrType::Integer(IntWidth::I32),
        register: IrRegister::Named("value".to_owned()),
    });

    let mut entry = IrBlock::new("entry");
    entry.push_instruction(IrInstruction::Math {
        dest: IrRegister::Temp(0),
        op: IrMathOp::Add,
        ty: IrType::Integer(IntWidth::I32),
        lhs: IrValue::Register(IrRegister::Named("value".to_owned())),
        rhs: IrValue::Integer(1),
    });
    entry.set_terminator(IrTerminator::Return(Some(IrValue::Register(
        IrRegister::Temp(0),
    ))));

    function.push_block(entry);
    program.push_function(function);

    let output = format!("{program}");
    assert!(output.contains("type Point = {x: f32, y: f32}"));
    assert!(output.contains("const hello = c\"hi\""));
    assert!(output.contains("define i32 add_one(i32 $value) {"));
    assert!(output.contains("entry:"));
    assert!(output.contains("\t$0 = math add i32 $value, 1"));
    assert!(output.contains("\tret $0"));
}
