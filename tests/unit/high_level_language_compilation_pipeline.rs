use full_stack::high_level_language::compilation_pipeline::CompilationPipeline;
use full_stack::intermediate_language::{IrCmpOp, IrInstruction, IrMathOp};


fn assert_semantic_error_contains(source: &str, expected: &str) {
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source);

    let err = match result {
        Ok(ok) => panic!("expected compilation to fail, but it succeeded: {ok:?}"),
        Err(err) => err,
    };

    match err {
        full_stack::high_level_language::compilation_pipeline::CompilationError::SemanticErrors(
            errors,
        ) => {
            assert!(
                errors.iter().any(|msg| msg.contains(expected)),
                "expected a semantic error containing `{expected}`, got: {errors:?}"
            );
        }
        other => panic!("expected semantic errors, got: {other:?}"),
    }
}

#[test]
fn test_pipeline_compiles_valid_program() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.run_semantic_analysis = false;

    let source = r#"
main: () -> i32 {
    return 42;
}
"#;

    let result = pipeline.compile(source);
    if let Err(ref e) = result {
        eprintln!("Compilation failed with error: {}", e);
    }
    assert!(result.is_ok());
}

#[test]
fn test_pipeline_catches_lexer_error() {
    let pipeline = CompilationPipeline::new();
    let source = "@invalid_token!@#";

    let result = pipeline.compile(source);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        full_stack::high_level_language::compilation_pipeline::CompilationError::LexerError(_)
    ));
}

#[test]
fn rejects_address_of_dereference_expression() {
    assert_semantic_error_contains(
        r#"
main: () -> i32* {
    ptr: i32* = new(i32)
    return &@ptr
}
"#,
        "cannot take address of a dereference expression",
    );
}

#[test]
fn rejects_returning_stack_addresses() {
    assert_semantic_error_contains(
        r#"
main: () -> i32* {
    x: i32 = 5
    return &x
}
"#,
        "Returning address of local `x` is not allowed",
    );
}

#[test]
fn rejects_returning_address_of_local_field() {
    assert_semantic_error_contains(
        r#"
type Point = {
    x: i32,
    y: i32
}

main: () -> i32* {
    p: { x: i32, y: i32 } = { .x = 1, .y = 2 }
    return &(p.x)
}
"#,
        "Returning address of local `p` is not allowed",
    );
}

#[test]
fn rejects_returning_address_of_local_array_element() {
    assert_semantic_error_contains(
        r#"
main: () -> i32* {
    arr: i32[4]
    return &(arr[0])
}
"#,
        "Returning address of local `arr` is not allowed",
    );
}

#[test]
fn allows_mixed_boolean_precedence() {
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(
        r#"
main: () -> bool {
    return true or false and true
}
"#,
    );

    assert!(
        result.is_ok(),
        "expected mixed boolean precedence to compile successfully: {:?}",
        result.err()
    );
}

#[test]
fn rejects_invalid_pointer_arithmetic() {
    assert_semantic_error_contains(
        r#"
main: () -> i32* {
    left: i32* = new(i32)
    right: i32* = new(i32)
    return left + right
}
"#,
        "Type error in binary operation",
    );
}

#[test]
fn test_unsigned_division_uses_udiv() {
    let source = r#"
        main: () -> i32 {
            a: u32 = 10
            b: u32 = 2
            c: u32 = a / b
            return i32(c)
        }
    "#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).unwrap();
    let program = &result.ir_program;

    
    let has_unsigned_div = program.functions.iter().any(|func| {
        func.blocks.iter().any(|block| {
            block.instructions.iter().any(|inst| {
                matches!(inst, IrInstruction::Math { op, .. } if op == &IrMathOp::Div)
            })
        })
    });

    assert!(has_unsigned_div, "Expected unsigned division (IrMathOp::Div) for u32 types");
}

#[test]
fn test_signed_division_uses_sdiv() {
    let source = r#"
        main: () -> i32 {
            a: i32 = 10
            b: i32 = 2
            c: i32 = a / b
            return c
        }
    "#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).unwrap();
    let program = &result.ir_program;

    
    let has_signed_div = program.functions.iter().any(|func| {
        func.blocks.iter().any(|block| {
            block.instructions.iter().any(|inst| {
                matches!(inst, IrInstruction::Math { op, .. } if op == &IrMathOp::SDiv)
            })
        })
    });

    assert!(has_signed_div, "Expected signed division (IrMathOp::SDiv) for i32 types");
}

#[test]
fn test_unsigned_comparison_uses_unsigned_ops() {
    let source = r#"
        main: () -> bool {
            a: u32 = 10
            b: u32 = 20
            result: bool = a < b
            return result
        }
    "#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).unwrap();
    let program = &result.ir_program;

    
    let has_unsigned_cmp = program.functions.iter().any(|func| {
        func.blocks.iter().any(|block| {
            block.instructions.iter().any(|inst| {
                matches!(inst, IrInstruction::Cmp { op, .. } 
                    if matches!(op, IrCmpOp::Ult | IrCmpOp::Ule | IrCmpOp::Ugt | IrCmpOp::Uge))
            })
        })
    });

    assert!(has_unsigned_cmp, "Expected unsigned comparison operator for u32 types");
}

#[test]
fn test_signed_comparison_uses_signed_ops() {
    let source = r#"
        main: () -> bool {
            a: i32 = 10
            b: i32 = 20
            result: bool = a < b
            return result
        }
    "#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).unwrap();
    let program = &result.ir_program;

    
    let has_signed_cmp = program.functions.iter().any(|func| {
        func.blocks.iter().any(|block| {
            block.instructions.iter().any(|inst| {
                matches!(inst, IrInstruction::Cmp { op, .. } 
                    if matches!(op, IrCmpOp::Slt | IrCmpOp::Sle | IrCmpOp::Sgt | IrCmpOp::Sge))
            })
        })
    });

    assert!(has_signed_cmp, "Expected signed comparison operator for i32 types");
}

#[test]
fn test_free_builtin_emits_heap_free() {
    let source = r#"
        external print: (value: i32) -> i32
        
        main: () -> i32 {
            ptr: i32* = new(i32)
            @ptr = 42
            value: i32 = @ptr
            free(ptr)
            return value
        }
    "#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).unwrap();
    let program = &result.ir_program;

    
    let has_heap_free = program.functions.iter().any(|func| {
        func.blocks.iter().any(|block| {
            block.instructions.iter().any(|inst| {
                matches!(inst, IrInstruction::HeapFree { .. })
            })
        })
    });

    assert!(has_heap_free, "Expected HeapFree instruction for free() call");
}

#[test]
fn test_free_with_wrong_arg_count_fails() {
    let source = r#"
        main: () -> i32 {
            ptr: i32* = new(i32)
            free()  ; Error: no arguments
            return 0
        }
    "#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source);

    assert!(result.is_err(), "Expected error for free() with no arguments");
}

#[test]
fn test_type_cast_i32_to_i64() {
    let source = r#"
        main: () -> i64 {
            a: i32 = 42
            b: i64 = i64(a)
            return b
        }
    "#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).unwrap();
    let program = &result.ir_program;

    
    let has_cast = program.functions.iter().any(|func| {
        func.blocks.iter().any(|block| {
            block.instructions.iter().any(|inst| {
                matches!(inst, IrInstruction::Cast { .. })
            })
        })
    });

    assert!(has_cast, "Expected Cast instruction for i32 to i64 conversion");
}

#[test]
fn test_type_cast_u32_to_u64() {
    let source = r#"
        main: () -> u64 {
            a: u32 = 42
            b: u64 = u64(a)
            return b
        }
    "#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).unwrap();
    let program = &result.ir_program;

    
    let has_cast = program.functions.iter().any(|func| {
        func.blocks.iter().any(|block| {
            block.instructions.iter().any(|inst| {
                matches!(inst, IrInstruction::Cast { .. })
            })
        })
    });

    assert!(has_cast, "Expected Cast instruction for u32 to u64 conversion");
}

#[test]
fn test_type_cast_i64_to_i32() {
    let source = r#"
        main: () -> i32 {
            a: i64 = 42
            b: i32 = i32(a)
            return b
        }
    "#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).unwrap();
    let program = &result.ir_program;

    
    let has_cast = program.functions.iter().any(|func| {
        func.blocks.iter().any(|block| {
            block.instructions.iter().any(|inst| {
                matches!(inst, IrInstruction::Cast { .. })
            })
        })
    });

    assert!(has_cast, "Expected Cast instruction for i64 to i32 truncation");
}

#[test]
fn test_type_cast_i32_to_f64() {
    let source = r#"
        main: () -> f64 {
            a: i32 = 42
            b: f64 = f64(a)
            return b
        }
    "#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).unwrap();
    let program = &result.ir_program;

    
    let has_cast = program.functions.iter().any(|func| {
        func.blocks.iter().any(|block| {
            block.instructions.iter().any(|inst| {
                matches!(inst, IrInstruction::Cast { .. })
            })
        })
    });

    assert!(has_cast, "Expected Cast instruction for i32 to f64 conversion");
}

#[test]
fn test_type_cast_pointer_to_pointer() {
    let source = r#"
        main: () -> i8* {
            a: i32* = new(i32)
            b: i8* = i8*(a)
            return b
        }
    "#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).unwrap();
    let program = &result.ir_program;

    
    let has_cast = program.functions.iter().any(|func| {
        func.blocks.iter().any(|block| {
            block.instructions.iter().any(|inst| {
                matches!(inst, IrInstruction::Cast { .. })
            })
        })
    });

    assert!(has_cast, "Expected Cast instruction for pointer to pointer conversion");
}

#[test]
fn test_multiple_casts_in_expression() {
    
    let source = r#"
        main: () -> i64 {
            a: i32 = 10
            b: i64 = i64(a) + i64(20)
            return b
        }
    "#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).unwrap();
    let program = &result.ir_program;

    
    let cast_count = program.functions.iter().fold(0, |acc, func| {
        acc + func.blocks.iter().fold(0, |block_acc, block| {
            block_acc + block.instructions.iter().filter(|inst| {
                matches!(inst, IrInstruction::Cast { .. })
            }).count()
        })
    });

    assert!(cast_count >= 2, "Expected at least 2 Cast instructions, found {}", cast_count);
}

#[test]
fn test_cast_followed_by_arithmetic() {
    
    let source = r#"
        main: () -> i64 {
            small: i32 = 100
            big: i64 = i64(small) * i64(1000)
            return big
        }
    "#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source);

    assert!(result.is_ok(), "Cast followed by arithmetic should compile successfully");
}

#[test]
fn test_unsigned_and_signed_mixed_operations() {
    
    let source = r#"
        main: () -> i32 {
            signed_val: i32 = 10 / 2      ; Should use SDiv
            ua: u32 = 5
            ub: u32 = 10
            unsigned_val: u32 = ua / ub   ; Should use Div
            
            signed_cmp: bool = 5 < 10     ; Should use Slt
            unsigned_cmp: bool = ua < ub  ; Should use Ult
            
            return signed_val
        }
    "#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).unwrap();
    let program = &result.ir_program;

    
    let has_both_div = program.functions.iter().any(|func| {
        let has_sdiv = func.blocks.iter().any(|block| {
            block.instructions.iter().any(|inst| {
                matches!(inst, IrInstruction::Math { op, .. } if op == &IrMathOp::SDiv)
            })
        });
        let has_udiv = func.blocks.iter().any(|block| {
            block.instructions.iter().any(|inst| {
                matches!(inst, IrInstruction::Math { op, .. } if op == &IrMathOp::Div)
            })
        });
        has_sdiv && has_udiv
    });

    assert!(has_both_div, "Expected both signed and unsigned division in mixed operations");
}

#[test]
fn test_free_after_new_pattern() {
    
    let source = r#"
        external print: (value: i32) -> i32
        
        main: () -> i32 {
            ptr: i32* = new(i32)
            @ptr = 100
            result: i32 = @ptr
            free(ptr)
            return result
        }
    "#;

    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(source).unwrap();
    let program = &result.ir_program;

    
    let has_alloc = program.functions.iter().any(|func| {
        func.blocks.iter().any(|block| {
            block.instructions.iter().any(|inst| {
                matches!(inst, IrInstruction::HeapAlloc { .. })
            })
        })
    });

    let has_free = program.functions.iter().any(|func| {
        func.blocks.iter().any(|block| {
            block.instructions.iter().any(|inst| {
                matches!(inst, IrInstruction::HeapFree { .. })
            })
        })
    });

    assert!(has_alloc, "Expected HeapAlloc for new()");
    assert!(has_free, "Expected HeapFree for free()");
}

// ── Struct destructuring ────────────────────────────────────────────────────

fn compile_ok(source: &str) -> full_stack::high_level_language::compilation_pipeline::CompilationResult {
    CompilationPipeline::new()
        .compile(source)
        .unwrap_or_else(|e| panic!("expected compilation to succeed, got: {e}"))
}

fn instruction_count<F>(result: &full_stack::high_level_language::compilation_pipeline::CompilationResult, pred: F) -> usize
where
    F: Fn(&IrInstruction) -> bool,
{
    result.ir_program.functions.iter().flat_map(|f| f.blocks.iter()).flat_map(|b| b.instructions.iter()).filter(|i| pred(i)).count()
}

/// Small struct (2 × i32) returned by value from a function and destructured.
/// The fix that landed this test: function-call results are rvalues — they
/// must be spilled before field extraction, not treated as addressable.
#[test]
fn small_struct_return_destructured_compiles() {
    let result = compile_ok(r#"
divide: (a: i32, b: i32) -> { quotient: i32, remainder: i32 } {
    return { .quotient = a / b, .remainder = a % b }
}

main: () -> i32 {
    { quotient: i32, remainder: i32 } = divide(10, 3)
    return quotient
}
"#);
    // divide() must be called exactly once — the old double-evaluation bug emitted two calls.
    let call_count = instruction_count(&result, |i| {
        matches!(i, IrInstruction::Call { function, .. } if function == "divide")
    });
    assert_eq!(call_count, 1, "divide() should be called exactly once, got {call_count}");
}

/// Struct returned by value where all fields are used after destructuring.
#[test]
fn small_struct_both_fields_accessible_after_destructure() {
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

/// A single-field struct returned by value.
#[test]
fn single_field_struct_return_destructured() {
    compile_ok(r#"
wrap: (x: i32) -> { val: i32 } {
    return { .val = x }
}

main: () -> i32 {
    { val: i32 } = wrap(7)
    return val
}
"#);
}

/// Struct with mixed field types (i32 + bool).
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

/// Type-aliased struct returned and destructured — exercises the named-type
/// resolution path inside lower_struct_destructuring_from_addr.
#[test]
fn type_alias_struct_return_destructured() {
    compile_ok(r#"
type Pair = { first: i32, second: i32 }

make_pair: (a: i32, b: i32) -> Pair {
    return { .first = a, .second = b }
}

main: () -> i32 {
    { first: i32, second: i32 } = make_pair(3, 4)
    return first + second
}
"#);
}

/// Destructuring a local struct variable (address-mode path, not the spill path).
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

/// Nested struct returned by inner call; outer function destructures it.
#[test]
fn chained_function_call_struct_destructure() {
    compile_ok(r#"
inner: () -> { val: i32 } {
    return { .val = 42 }
}

outer: () -> i32 {
    { val: i32 } = inner()
    return val
}

main: () -> i32 {
    return outer()
}
"#);
}

/// Struct returned by a function called with non-trivial (computed) arguments.
#[test]
fn struct_return_with_computed_args() {
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

/// Destructuring from a function call inside an if-branch.
#[test]
fn struct_destructure_inside_if() {
    compile_ok(r#"
get_val: () -> { n: i32 } {
    return { .n = 99 }
}

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

/// Destructuring result is used in a subsequent expression, not just returned.
#[test]
fn struct_field_used_in_arithmetic() {
    compile_ok(r#"
dims: () -> { w: i32, h: i32 } {
    return { .w = 6, .h = 7 }
}

main: () -> i32 {
    { w: i32, h: i32 } = dims()
    area: i32 = w * h
    return area
}
"#);
}

/// Three-field struct — exceeds the two-register ABI window so the spill path
/// must handle more than two loads.
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
