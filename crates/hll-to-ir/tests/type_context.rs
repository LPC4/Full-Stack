use hll_to_ir::ast::{BinaryOp, UnaryOp};
use hll_to_ir::compiler::utility::type_context::{
    TypeCheckError, TypeContext,
};
use hll_to_ir::IrType;

#[test]
fn allows_placeholder_arithmetic() {
    let ctx = TypeContext::new();
    assert_eq!(ctx.check_binary_op(&BinaryOp::Add, "T", "T").unwrap(), "T");
}

#[test]
fn still_rejects_non_numeric_named_types() {
    let ctx = TypeContext::new();
    assert!(matches!(
        ctx.check_binary_op(&BinaryOp::Add, "Point", "Point"),
        Err(TypeCheckError::InvalidOperation { .. })
    ));
}

#[test]
fn unary_dereference_and_address_of_round_trip() {
    let ctx = TypeContext::new();
    assert_eq!(
        ctx.check_unary_op(&UnaryOp::AddressOf, "i32").unwrap(),
        "*i32"
    );
    assert_eq!(
        ctx.check_unary_op(&UnaryOp::Dereference, "*i32").unwrap(),
        "i32"
    );
}

#[test]
fn get_type_name_formats_aggregates() {
    let ctx = TypeContext::new();
    let ty = IrType::Aggregate(vec![
        ("x".to_string(), IrType::Named("i32".to_string())),
        ("y".to_string(), IrType::Named("i32".to_string())),
    ]);
    assert_eq!(ctx.get_type_name(&ty), "{ x: i32, y: i32 }");
}
