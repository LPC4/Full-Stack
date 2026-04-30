/// Unit tests for struct destructuring lowering.
///
/// Verifies that the HLL → IR pipeline handles every variant of struct
/// destructuring: small-struct ABI (function return), local variable
/// address-mode path, partial destructuring, mixed field types, type aliases,
/// and the single-evaluation guarantee (no double-evaluation of the rvalue).
use full_stack::high_level_language::compilation_pipeline::CompilationPipeline;
use full_stack::intermediate_language::IrInstruction;

fn compile_ok(source: &str) -> full_stack::high_level_language::compilation_pipeline::CompilationResult {
    CompilationPipeline::new()
        .compile(source)
        .unwrap_or_else(|e| panic!("compilation failed: {e}"))
}

fn call_count(result: &full_stack::high_level_language::compilation_pipeline::CompilationResult, name: &str) -> usize {
    result
        .ir_program
        .functions
        .iter()
        .flat_map(|f| f.blocks.iter())
        .flat_map(|b| b.instructions.iter())
        .filter(|i| matches!(i, IrInstruction::Call { function, .. } if function == name))
        .count()
}

// ── Small-struct ABI (function return) ───────────────────────────────────────

/// Core regression: the callee must be invoked exactly once.
/// Before the fix, the rvalue was evaluated twice (once for Address mode, once
/// for the spill fallback), producing two call instructions.
#[test]
fn function_return_struct_called_exactly_once() {
    let result = compile_ok(r#"
divide: (a: i32, b: i32) -> { quotient: i32, remainder: i32 } {
    return { .quotient = a / b, .remainder = a % b }
}
main: () -> i32 {
    { quotient: i32, remainder: i32 } = divide(10, 3)
    return quotient
}
"#);
    assert_eq!(call_count(&result, "divide"), 1, "divide() should be called exactly once");
}

#[test]
fn both_fields_accessible_after_function_return_destructure() {
    compile_ok(r#"
minmax: (a: i32, b: i32) -> { lo: i32, hi: i32 } {
    return { .lo = a, .hi = b }
}
main: () -> i32 {
    { lo: i32, hi: i32 } = minmax(1, 9)
    return hi - lo
}
"#);
}

#[test]
fn single_field_struct_return_destructured() {
    compile_ok(r#"
wrap: (x: i32) -> { val: i32 } { return { .val = x } }
main: () -> i32 {
    { val: i32 } = wrap(7)
    return val
}
"#);
}

#[test]
fn struct_with_bool_field_destructured() {
    compile_ok(r#"
check: (x: i32) -> { value: i32, ok: bool } {
    return { .value = x, .ok = true }
}
main: () -> i32 {
    { value: i32, ok: bool } = check(5)
    return value
}
"#);
}

#[test]
fn type_alias_struct_return_destructured() {
    compile_ok(r#"
type Pair = { first: i32, second: i32 }
make_pair: (a: i32, b: i32) -> Pair { return { .first = a, .second = b } }
main: () -> i32 {
    { first: i32, second: i32 } = make_pair(3, 4)
    return first + second
}
"#);
}

#[test]
fn three_field_struct_return_destructured() {
    compile_ok(r#"
triple: (a: i32, b: i32, c: i32) -> { x: i32, y: i32, z: i32 } {
    return { .x = a, .y = b, .z = c }
}
main: () -> i32 {
    { x: i32, y: i32, z: i32 } = triple(1, 2, 3)
    return x + y + z
}
"#);
}

// ── Local variable (address-mode path) ───────────────────────────────────────

#[test]
fn local_struct_variable_destructured() {
    compile_ok(r#"
main: () -> i32 {
    p: { x: i32, y: i32 } = { .x = 10, .y = 20 }
    { x: i32, y: i32 } = p
    return x + y
}
"#);
}

// ── Composition and context ───────────────────────────────────────────────────

#[test]
fn destructure_inside_if_branch() {
    compile_ok(r#"
get_val: () -> { n: i32 } { return { .n = 99 } }
main: () -> i32 {
    result: i32 = 0
    if true {
        { n: i32 } = get_val()
        result = n
    }
    return result
}
"#);
}

#[test]
fn destructured_field_used_in_arithmetic() {
    compile_ok(r#"
dims: () -> { w: i32, h: i32 } { return { .w = 6, .h = 7 } }
main: () -> i32 {
    { w: i32, h: i32 } = dims()
    return w * h
}
"#);
}

#[test]
fn chained_call_destructure() {
    compile_ok(r#"
inner: () -> { val: i32 } { return { .val = 42 } }
outer: () -> i32 {
    { val: i32 } = inner()
    return val
}
main: () -> i32 { return outer() }
"#);
}

#[test]
fn struct_return_with_computed_call_args() {
    compile_ok(r#"
pair: (a: i32, b: i32) -> { sum: i32, product: i32 } {
    return { .sum = a + b, .product = a * b }
}
main: () -> i32 {
    x: i32 = 3
    y: i32 = 4
    { sum: i32, product: i32 } = pair(x + 1, y - 1)
    return sum
}
"#);
}
