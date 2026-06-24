// Execution-level coverage for the HLL V2 front end (PLAN.md Milestones 1-6).
//
// The V2 conformance tests in `hll-to-ir` stop at compile/IR shape. This suite
// closes the gate the V2 plan names before repository migration: every
// implemented V2 surface (place/value access, inferred bindings, canonical
// struct literals, pointer arithmetic, slices, and explicit generics) must compile,
// assemble, link against the V1 hosted stdlib, and produce the right exit code
// in the VM.

use asm_to_binary::AssembledOutput;
use full_stack::compilation_pipeline::CompilationPipeline;
use hll_to_ir::stdlib::get_stdlib_source;
use hll_to_ir::LanguageVersion;
use std::sync::OnceLock;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

// The hosted stdlib is version-agnostic (V1 source); compile and assemble it
// once for the whole suite and link every V2 user program against it.
fn cached_stdlib_obj() -> &'static AssembledOutput {
    static STDLIB: OnceLock<AssembledOutput> = OnceLock::new();
    STDLIB.get_or_init(|| {
        let mut pipeline = CompilationPipeline::new();
        pipeline.set_write_artifacts(false);
        let stdlib_result = pipeline
            .compile(&get_stdlib_source())
            .expect("stdlib compile failed");
        let (_, stdlib_tokens) =
            pipeline.compile_ir_to_assembly_with_tokens(&stdlib_result.ir_program);
        pipeline
            .assemble(&stdlib_tokens)
            .expect("stdlib assemble failed")
    })
}

/// Compile a V2 user program, link it against the cached stdlib, run in the VM.
fn run_v2(src: &str) -> StepOutcome {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_language_version(LanguageVersion::V2);

    let user_result = pipeline.compile(src).expect("V2 user compile failed");
    let (_, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);
    let user_obj = pipeline
        .assemble(&user_tokens)
        .expect("user assemble failed");

    let assembled = pipeline
        .link_assembled_objects(&[("stdlib", cached_stdlib_obj()), ("user", &user_obj)])
        .expect("link failed");
    let mut vm = VirtualMachine::new(&assembled);
    vm.run(5_000_000).outcome
}

fn assert_exit(src: &str, code: i64) {
    let outcome = run_v2(src);
    assert!(
        matches!(outcome, StepOutcome::Halted(c) if c == code),
        "expected Halted({code}), got {outcome:?}"
    );
}

/// Assert a V2 program fails to compile (used for rejected diagnostics).
fn assert_compile_fails(src: &str) {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_language_version(LanguageVersion::V2);
    assert!(
        pipeline.compile(src).is_err(),
        "expected the program to be rejected, but it compiled"
    );
}

// --- Milestone 1: place / value access model ---

#[test]
fn v2_array_index_read_write_and_address() {
    // Index in value context reads; on the LHS it writes; &arr[i] takes its
    // address; @ reads the whole pointee. 10 + 5 + 30 = 45.
    assert_exit(
        r#"
main: () -> i32 {
    arr: i32[3] = [10, 20, 30]
    arr[1] = 5
    p: i32* = &arr[2]
    return arr[0] + arr[1] + @p
}
"#,
        45,
    );
}

#[test]
fn v2_pointer_member_auto_deref() {
    // `.` auto-dereferences one pointer level for field read and write.
    assert_exit(
        r#"
struct Point {
    x: i32
    y: i32
}

main: () -> i32 {
    pt: Point* = new(Point)
    pt.x = 12
    pt.y = 30
    return pt.x + pt.y
}
"#,
        42,
    );
}

#[test]
fn v2_array_of_struct_element_place() {
    // Indexing an array of structs yields an element place; selecting a field
    // and taking &arr[i] both work without `@arr[i]`.
    assert_exit(
        r#"
struct Point {
    x: i32
    y: i32
}

main: () -> i32 {
    pts: Point[2] = [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }]
    pts[1].x = 36
    first: Point* = &pts[0]
    return pts[1].x + first.y
}
"#,
        38,
    );
}

// --- Milestone 2: inferred binding syntax (`:=`) ---

#[test]
fn v2_inferred_bindings_execute() {
    // `:=` infers primitive, array, and pointer types and runs end to end.
    assert_exit(
        r#"
main: () -> i32 {
    n := 40
    arr := [1, 2]
    p := &arr[1]
    return n + @p
}
"#,
        42,
    );
}

#[test]
fn v2_inferred_struct_pointer_binding() {
    // `:=` over new(T) infers T*; field access through it works.
    assert_exit(
        r#"
struct Box {
    value: i32
}

main: () -> i32 {
    b := new(Box)
    b.value = 99
    return b.value
}
"#,
        99,
    );
}

// --- Milestone 3: canonical struct literals ---

#[test]
fn v2_named_and_contextual_literals_execute() {
    // Named literal (reordered fields) and a contextual literal from the
    // annotation. 1 + 2 + 3 + 4 = 10.
    assert_exit(
        r#"
struct Point {
    x: i32
    y: i32
}

main: () -> i32 {
    named := Point { y: 2, x: 1 }
    contextual: Point = { x: 3, y: 4 }
    return named.x + named.y + contextual.x + contextual.y
}
"#,
        10,
    );
}

#[test]
fn v2_contextual_literal_zero_fills() {
    // Omitted fields in an anonymous literal default to zero at runtime.
    assert_exit(
        r#"
struct Point {
    x: i32
    y: i32
}

main: () -> i32 {
    point: Point = { x: 7 }
    return point.x + point.y
}
"#,
        7,
    );
}

#[test]
fn v2_literal_context_through_return() {
    // An anonymous literal contextualizes from the function's return type and
    // the returned struct (by value, via sret) reads back correctly.
    // NOTE: struct-by-value *arguments* are a separate ABI gap (PLAN.md 4.3),
    // so this exercises only the return-context path.
    assert_exit(
        r#"
struct Point {
    x: i32
    y: i32
}

make_point: () -> Point {
    return { x: 5, y: 7 }
}

main: () -> i32 {
    p := make_point()
    return p.x + p.y
}
"#,
        12,
    );
}

// --- Milestone 5: `for` over ranges (lowers to `while`) ---

#[test]
fn v2_for_range_sums() {
    // Half-open `0..5` iterates 0,1,2,3,4 -> sum 10.
    assert_exit(
        r#"
main: () -> i32 {
    total: i32 = 0
    for i in 0..5 {
        total = total + i
    }
    return total
}
"#,
        10,
    );
}

#[test]
fn v2_for_inclusive_range() {
    // Inclusive `1..=4` iterates 1,2,3,4 -> sum 10.
    assert_exit(
        r#"
main: () -> i32 {
    total: i32 = 0
    for i in 1..=4 {
        total = total + i
    }
    return total
}
"#,
        10,
    );
}

#[test]
fn v2_for_continue_still_steps() {
    // `continue` must still advance the counter, else this would hang.
    // Sum of odd i in 0..6 = 1 + 3 + 5 = 9.
    assert_exit(
        r#"
main: () -> i32 {
    total: i32 = 0
    for i in 0..6 {
        if i % 2 == 0 {
            continue
        }
        total = total + i
    }
    return total
}
"#,
        9,
    );
}

#[test]
fn v2_for_break_exits() {
    // `break` leaves the loop: sum 0..5 before i == 5 = 10.
    assert_exit(
        r#"
main: () -> i32 {
    total: i32 = 0
    for i in 0..100 {
        if i == 5 {
            break
        }
        total = total + i
    }
    return total
}
"#,
        10,
    );
}

#[test]
fn v2_for_end_evaluated_once() {
    // The range end is captured once; mutating `n` in the body must not extend
    // the loop. 3 iterations regardless of `n` growing.
    assert_exit(
        r#"
main: () -> i32 {
    n: i32 = 3
    count: i32 = 0
    for i in 0..n {
        n = n + 10
        count = count + 1
    }
    return count
}
"#,
        3,
    );
}

#[test]
fn v2_nested_for_loops() {
    // 3x3 nested loop body runs 9 times.
    assert_exit(
        r#"
main: () -> i32 {
    count: i32 = 0
    for i in 0..3 {
        for j in 0..3 {
            count = count + 1
        }
    }
    return count
}
"#,
        9,
    );
}

// --- Milestone 5: `for` over a fixed array ---

#[test]
fn v2_for_each_array_sums() {
    // `for x in arr` binds each element by value: 3 + 5 + 7 + 9 = 24.
    assert_exit(
        r#"
main: () -> i32 {
    arr: i32[4] = [3, 5, 7, 9]
    total: i32 = 0
    for x in arr {
        total = total + x
    }
    return total
}
"#,
        24,
    );
}

#[test]
fn v2_for_each_array_with_continue() {
    // Skip values below 5: 7 + 9 = 16.
    assert_exit(
        r#"
main: () -> i32 {
    arr: i32[4] = [3, 5, 7, 9]
    total: i32 = 0
    for x in arr {
        if x < 6 {
            continue
        }
        total = total + x
    }
    return total
}
"#,
        16,
    );
}

#[test]
fn v2_for_each_struct_array() {
    // Element is a struct value; field reads through the by-value binding.
    // (1+2) + (3+4) + (5+6) = 21.
    assert_exit(
        r#"
struct Point {
    x: i32
    y: i32
}

main: () -> i32 {
    pts: Point[3] = [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }, Point { x: 5, y: 6 }]
    total: i32 = 0
    for p in pts {
        total = total + p.x + p.y
    }
    return total
}
"#,
        21,
    );
}

// --- Milestone 4: typed, element-scaled pointer arithmetic ---

#[test]
fn v2_pointer_arithmetic_is_element_scaled() {
    // `p + 2` over an i32* advances by 2 elements (8 bytes), not 2 bytes.
    assert_exit(
        r#"
main: () -> i32 {
    arr: i32[4] = [10, 20, 30, 40]
    p: i32* = &arr[0]
    q: i32* = p + 2
    return @q
}
"#,
        30,
    );
}

#[test]
fn v2_pointer_arithmetic_walks_struct_array() {
    // Element scaling uses sizeof(Point), so p + 1 lands on the next record.
    assert_exit(
        r#"
struct Point {
    x: i32
    y: i32
}

main: () -> i32 {
    pts: Point[3] = [Point { x: 1, y: 1 }, Point { x: 5, y: 6 }, Point { x: 9, y: 9 }]
    base: Point* = &pts[0]
    mid: Point* = base + 1
    return mid.x + mid.y
}
"#,
        11,
    );
}

#[test]
fn v2_pointer_subtraction_is_element_scaled() {
    // Element-scaled subtraction steps back one i32.
    assert_exit(
        r#"
main: () -> i32 {
    arr: i32[3] = [7, 42, 13]
    last: i32* = &arr[2]
    prev: i32* = last - 1
    return @prev
}
"#,
        42,
    );
}

// --- Milestone 5: slices (T[] fat pointer) ---

#[test]
fn v2_slice_from_array_indexes_and_len() {
    // A fixed array coerces to a slice; indexing reads elements and .len gives
    // the count. 10 + 30 + 3 = 43.
    assert_exit(
        r#"
main: () -> i32 {
    arr: i32[3] = [10, 20, 30]
    view: i32[] = arr
    return view[0] + view[2] + view.len as i32
}
"#,
        43,
    );
}

#[test]
fn v2_slice_element_write() {
    // Writing through a slice element place mutates the backing array. 5 + 20 = 25.
    assert_exit(
        r#"
main: () -> i32 {
    arr: i32[3] = [10, 20, 30]
    view: i32[] = arr
    view[0] = 5
    return view[0] + arr[1]
}
"#,
        25,
    );
}

#[test]
fn v2_for_over_slice_sums() {
    // `for x in slice` iterates each element. 1 + 2 + 3 + 4 = 10.
    assert_exit(
        r#"
main: () -> i32 {
    arr: i32[4] = [1, 2, 3, 4]
    view: i32[] = arr
    sum: i32 = 0
    for x in view {
        sum = sum + x
    }
    return sum
}
"#,
        10,
    );
}

#[test]
fn v2_slice_out_of_bounds_traps() {
    // Indexing past len fails the bounds check and aborts with the slice-bounds
    // diagnostic code (134) instead of reading out of bounds.
    assert_exit(
        r#"
main: () -> i32 {
    arr: i32[3] = [10, 20, 30]
    view: i32[] = arr
    i: i32 = 5
    return view[i]
}
"#,
        134,
    );
}

#[test]
fn v2_slice_in_bounds_does_not_trap() {
    // A runtime index that is in bounds passes the check and reads the element.
    assert_exit(
        r#"
main: () -> i32 {
    arr: i32[3] = [10, 20, 30]
    view: i32[] = arr
    i: i32 = 2
    return view[i]
}
"#,
        30,
    );
}

// --- Milestone 5: range slicing (arr[a..b]) ---

#[test]
fn v2_range_slice_from_array() {
    // arr[1..4] is the half-open sub-slice {20, 30, 40}. 20 + 40 + 3 = 63.
    assert_exit(
        r#"
main: () -> i32 {
    arr: i32[5] = [10, 20, 30, 40, 50]
    view := arr[1..4]
    return view[0] + view[2] + view.len as i32
}
"#,
        63,
    );
}

#[test]
fn v2_range_slice_inclusive() {
    // arr[1..=3] includes index 3, so {20, 30, 40} -- same as arr[1..4].
    assert_exit(
        r#"
main: () -> i32 {
    arr: i32[5] = [10, 20, 30, 40, 50]
    view := arr[1..=3]
    return view[0] + view[2] + view.len as i32
}
"#,
        63,
    );
}

#[test]
fn v2_range_slice_open_endpoints() {
    // arr[..2] = {10,20} (len 2); arr[3..] = {40,50} (len 2); arr[..] = all (len 5).
    assert_exit(
        r#"
main: () -> i32 {
    arr: i32[5] = [10, 20, 30, 40, 50]
    head := arr[..2]
    tail := arr[3..]
    whole := arr[..]
    return (head.len + tail.len + whole.len) as i32
}
"#,
        9,
    );
}

#[test]
fn v2_for_over_range_subslice() {
    // Iterate a range-produced sub-slice. 20 + 30 + 40 = 90.
    assert_exit(
        r#"
main: () -> i32 {
    arr: i32[5] = [10, 20, 30, 40, 50]
    sum: i32 = 0
    for x in arr[1..4] {
        sum = sum + x
    }
    return sum
}
"#,
        90,
    );
}

#[test]
fn v2_range_slice_of_slice() {
    // Re-slicing a slice indexes relative to the sub-slice. view = {20,30,40},
    // sub = view[1..3] = {30,40}. 30 + 40 = 70.
    assert_exit(
        r#"
main: () -> i32 {
    arr: i32[5] = [10, 20, 30, 40, 50]
    view := arr[1..4]
    sub := view[1..3]
    return sub[0] + sub[1]
}
"#,
        70,
    );
}

#[test]
fn v2_range_slice_end_past_len_traps() {
    // An end beyond the source length fails the slice bounds check (code 134).
    assert_exit(
        r#"
main: () -> i32 {
    arr: i32[5] = [10, 20, 30, 40, 50]
    b: i32 = 10
    view := arr[1..b]
    return view[0]
}
"#,
        134,
    );
}

#[test]
fn v2_range_slice_start_after_end_traps() {
    // start > end is an invalid range and traps (code 134).
    assert_exit(
        r#"
main: () -> i32 {
    arr: i32[5] = [10, 20, 30, 40, 50]
    a: i32 = 3
    view := arr[a..1]
    return view[0]
}
"#,
        134,
    );
}

// --- Milestone 3: contextual struct literals inside array literals ---

#[test]
fn v2_contextual_struct_literals_in_array() {
    // Bare `{ .. }` elements take the declared element type as context, so the
    // named `Point` prefix can be omitted. (1+2) + (3+4) + (5+6) = 21.
    assert_exit(
        r#"
struct Point {
    x: i32
    y: i32
}

main: () -> i32 {
    pts: Point[3] = [{ x: 1, y: 2 }, { x: 3, y: 4 }, { x: 5, y: 6 }]
    total: i32 = 0
    for p in pts {
        total = total + p.x + p.y
    }
    return total
}
"#,
        21,
    );
}

#[test]
fn v2_contextual_array_literal_zero_fills() {
    // A contextual element with a missing field zero-fills it, like a contextual
    // struct literal in any other context. y defaults to 0: (1) + (2+7) = 10.
    assert_exit(
        r#"
struct Point {
    x: i32
    y: i32
}

main: () -> i32 {
    pts: Point[2] = [{ x: 1 }, { x: 2, y: 7 }]
    return pts[0].x + pts[0].y + pts[1].x + pts[1].y
}
"#,
        10,
    );
}

#[test]
fn v2_array_literal_scalar_width_flexible() {
    // Bare integer literals adopt the declared element width (i64 here).
    assert_exit(
        r#"
main: () -> i64 {
    nums: i64[3] = [10, 20, 30]
    return nums[0] + nums[1] + nums[2]
}
"#,
        60,
    );
}

#[test]
fn v2_array_literal_wrong_element_count_rejected() {
    // The literal length must match the declared array length.
    assert_compile_fails(
        r#"
main: () -> i32 {
    nums: i32[3] = [1, 2]
    return nums[0]
}
"#,
    );
}

#[test]
fn v2_contextual_array_element_unknown_field_rejected() {
    // A contextual element naming a field the struct does not have is rejected.
    assert_compile_fails(
        r#"
struct Point {
    x: i32
    y: i32
}

main: () -> i32 {
    pts: Point[1] = [{ x: 1, z: 2 }]
    return pts[0].x
}
"#,
    );
}

// --- Milestone 6: monomorphized generics ---

#[test]
fn v2_explicit_generic_function_specializations_execute() {
    assert_exit(
        r#"
identity: <T>(value: T) -> T {
    return value
}

main: () -> i32 {
    left: i32 = identity<i32>(17)
    right: i64 = identity<i64>(25)
    return left + right as i32
}
"#,
        42,
    );
}

#[test]
fn v2_generic_function_specialization_is_cached() {
    assert_exit(
        r#"
add_one: <T>(value: T) -> T {
    return value + 1
}

main: () -> i32 {
    return add_one<i32>(20) + add_one<i32>(21)
}
"#,
        43,
    );
}

#[test]
fn v2_generic_function_infers_literal_type_argument() {
    assert_exit(
        r#"
identity: <T>(value: T) -> T {
    return value
}

main: () -> i32 {
    return identity(42)
}
"#,
        42,
    );
}

#[test]
fn v2_generic_function_infers_local_binding_type() {
    assert_exit(
        r#"
identity: <T>(value: T) -> T {
    return value
}

main: () -> i32 {
    value := 42
    return identity(value)
}
"#,
        42,
    );
}

#[test]
fn v2_generic_function_requires_explicit_unconstrained_type() {
    assert_compile_fails(
        r#"
make: <T>() -> T {
    value: T = 0
    return value
}

main: () -> i32 {
    return make()
}
"#,
    );
}

#[test]
fn v2_generic_record_specialization_executes() {
    assert_exit(
        r#"
struct Box<T> {
    value: T
}

main: () -> i32 {
    boxed: Box<i32>* = new(Box<i32>)
    boxed.value = 42
    return boxed.value
}
"#,
        42,
    );
}

#[test]
fn v2_nested_generic_record_specialization_executes() {
    assert_exit(
        r#"
struct Pair<T> {
    first: T
    second: T
}

struct Box<T> {
    value: T
}

main: () -> i32 {
    boxed: Box<Pair<i32>>* = new(Box<Pair<i32>>)
    boxed.value.first = 19
    boxed.value.second = 23
    return boxed.value.first + boxed.value.second
}
"#,
        42,
    );
}

#[test]
fn v2_generic_function_uses_generic_record() {
    assert_exit(
        r#"
struct Box<T> {
    value: T
}

unbox: <T>(boxed: Box<T>*) -> T {
    return boxed.value
}

main: () -> i32 {
    boxed: Box<i32>* = new(Box<i32>)
    boxed.value = 42
    return unbox<i32>(boxed)
}
"#,
        42,
    );
}

#[test]
fn v2_generic_function_infers_nested_record_argument() {
    assert_exit(
        r#"
struct Box<T> {
    value: T
}

unbox: <T>(boxed: Box<T>*) -> T {
    return boxed.value
}

main: () -> i32 {
    boxed: Box<i32>* = new(Box<i32>)
    boxed.value = 42
    return unbox(boxed)
}
"#,
        42,
    );
}

#[test]
fn v2_generic_function_rejects_conflicting_inference() {
    assert_compile_fails(
        r#"
first: <T>(left: T, right: T) -> T {
    return left
}

main: () -> i32 {
    return first(42, true)
}
"#,
    );
}

#[test]
fn v2_generic_record_specializations_have_distinct_layouts() {
    assert_exit(
        r#"
struct Box<T> {
    value: T
}

main: () -> i32 {
    small: Box<i32>* = new(Box<i32>)
    wide: Box<i64>* = new(Box<i64>)
    small.value = 17
    wide.value = 25
    return small.value + wide.value as i32
}
"#,
        42,
    );
}

#[test]
fn v2_recursive_generic_record_specialization_executes() {
    assert_exit(
        r#"
struct Node<T> {
    value: T
    next: Node<T>*
}

main: () -> i32 {
    node: Node<i32>* = new(Node<i32>)
    node.value = 42
    return node.value
}
"#,
        42,
    );
}

// --- Milestone 7: enums, patterns, and `match` ---

#[test]
fn v2_match_payload_variant_dispatch() {
    // Rect is tag 1; its two payload slots bind to w and h. 6 * 7 = 42.
    assert_exit(
        r#"
enum Shape {
    Circle(i32)
    Rect(i32, i32)
    Empty
}

main: () -> i32 {
    s: Shape = Rect(6, 7)
    match s {
        Circle(r) -> {
            return r * r
        }
        Rect(w, h) -> {
            return w * h
        }
        Empty -> {
            return 0
        }
    }
    return -1
}
"#,
        42,
    );
}

#[test]
fn v2_match_selects_single_payload_arm() {
    // Circle is tag 0; binding r reads its one payload slot. 7 * 7 = 49.
    assert_exit(
        r#"
enum Shape {
    Circle(i32)
    Rect(i32, i32)
    Empty
}

main: () -> i32 {
    s: Shape = Circle(7)
    match s {
        Circle(r) -> {
            return r * r
        }
        Rect(w, h) -> {
            return w * h
        }
        Empty -> {
            return 0
        }
    }
    return -1
}
"#,
        49,
    );
}

#[test]
fn v2_match_unit_variants_with_wildcard() {
    // Unit variants carry no payload; the wildcard covers the rest. Green is tag 1.
    assert_exit(
        r#"
enum Color {
    Red
    Green
    Blue
}

main: () -> i32 {
    c: Color = Green
    match c {
        Red -> {
            return 1
        }
        Green -> {
            return 2
        }
        _ -> {
            return 99
        }
    }
    return -1
}
"#,
        2,
    );
}

#[test]
fn v2_match_unit_variant_falls_to_wildcard() {
    // Blue (tag 2) is not named explicitly, so the wildcard arm runs.
    assert_exit(
        r#"
enum Color {
    Red
    Green
    Blue
}

main: () -> i32 {
    c: Color = Blue
    match c {
        Red -> {
            return 1
        }
        _ -> {
            return 99
        }
    }
    return -1
}
"#,
        99,
    );
}

#[test]
fn v2_match_mixed_payload_widths() {
    // Pair packs an i64 then an i32 into the payload area; both read back.
    assert_exit(
        r#"
enum Packet {
    Ping
    Pair(i64, i32)
}

main: () -> i32 {
    p: Packet = Pair(40, 2)
    match p {
        Ping -> {
            return 0
        }
        Pair(a, b) -> {
            return a as i32 + b
        }
    }
    return -1
}
"#,
        42,
    );
}

#[test]
fn v2_match_non_exhaustive_is_rejected() {
    assert_compile_fails(
        r#"
enum E {
    A
    B
}

main: () -> i32 {
    e: E = A
    match e {
        A -> {
            return 1
        }
    }
    return 0
}
"#,
    );
}

#[test]
fn v2_enum_variant_wrong_arity_is_rejected() {
    assert_compile_fails(
        r#"
enum Shape {
    Circle(i32)
    Empty
}

main: () -> i32 {
    s: Shape = Circle(1, 2)
    return 0
}
"#,
    );
}

#[test]
fn v2_match_unknown_variant_is_rejected() {
    assert_compile_fails(
        r#"
enum Shape {
    Circle(i32)
    Empty
}

main: () -> i32 {
    s: Shape = Empty
    match s {
        Circle(r) -> {
            return r
        }
        Square -> {
            return 0
        }
    }
    return -1
}
"#,
    );
}

// --- Milestone 7: generic enums (Option / Result prelude) ---

#[test]
fn v2_option_some_match_extracts_payload() {
    // The Option<i32> prelude enum: Some(41) binds v, returns v + 1.
    assert_exit(
        r#"
main: () -> i32 {
    o: Option<i32> = Some(41)
    match o {
        Some(v) -> {
            return v + 1
        }
        None -> {
            return 0
        }
    }
    return -1
}
"#,
        42,
    );
}

#[test]
fn v2_option_none_takes_unit_arm() {
    assert_exit(
        r#"
main: () -> i32 {
    o: Option<i32> = None
    match o {
        Some(v) -> {
            return v
        }
        None -> {
            return 7
        }
    }
    return -1
}
"#,
        7,
    );
}

#[test]
fn v2_result_ok_and_err_dispatch() {
    // Result<i32, i32> returned from a function and matched at the call site.
    assert_exit(
        r#"
parse: (n: i32) -> Result<i32, i32> {
    if n < 0 {
        return Err(n)
    }
    return Ok(n * 2)
}

main: () -> i32 {
    r: Result<i32, i32> = parse(21)
    match r {
        Ok(v) -> {
            return v
        }
        Err(e) -> {
            return e
        }
    }
    return -1
}
"#,
        42,
    );
}

#[test]
fn v2_result_err_arm_executes() {
    assert_exit(
        r#"
parse: (n: i32) -> Result<i32, i32> {
    if n < 0 {
        return Err(0 - n)
    }
    return Ok(n)
}

main: () -> i32 {
    r: Result<i32, i32> = parse(0 - 5)
    match r {
        Ok(v) -> {
            return v
        }
        Err(e) -> {
            return e
        }
    }
    return -1
}
"#,
        5,
    );
}

#[test]
fn v2_distinct_option_specializations_coexist() {
    // Option<i32> and Option<i64> are separate enums with their own constructors;
    // both must lower and run in the same program.
    assert_exit(
        r#"
main: () -> i32 {
    a: Option<i32> = Some(40)
    b: Option<i64> = Some(2)
    total: i32 = 0
    match a {
        Some(v) -> {
            total = total + v
        }
        None -> {
            total = total + 0
        }
    }
    match b {
        Some(w) -> {
            total = total + w as i32
        }
        None -> {
            total = total + 0
        }
    }
    return total
}
"#,
        42,
    );
}

#[test]
fn v2_user_generic_enum_specializes() {
    // A user-declared generic enum, not just the prelude ones.
    assert_exit(
        r#"
enum Pair<T> {
    One(T)
    Two(T, T)
}

main: () -> i32 {
    p: Pair<i32> = Two(15, 27)
    match p {
        One(a) -> {
            return a
        }
        Two(a, b) -> {
            return a + b
        }
    }
    return -1
}
"#,
        42,
    );
}

#[test]
fn v2_bare_constructor_without_context_is_rejected() {
    // `:=` gives no expected type, so a bare generic-enum constructor is ambiguous.
    assert_compile_fails(
        r#"
main: () -> i32 {
    o := None
    return 0
}
"#,
    );
}

#[test]
fn v2_result_try_extracts_success_value() {
    assert_exit(
        r#"
parse: (n: i32) -> Result<i32, i32> {
    return Ok(n)
}

widen: (n: i32) -> Result<i64, i32> {
    value := parse(n)?
    return Ok((value + 1) as i64)
}

main: () -> i32 {
    result: Result<i64, i32> = widen(41)
    match result {
        Ok(value) -> {
            return value as i32
        }
        Err(error) -> {
            return error
        }
    }
    return -1
}
"#,
        42,
    );
}

#[test]
fn v2_result_try_propagates_error() {
    assert_exit(
        r#"
parse: (n: i32) -> Result<i32, i32> {
    return Err(n)
}

widen: (n: i32) -> Result<i64, i32> {
    value := parse(n)?
    return Ok(value as i64)
}

main: () -> i32 {
    result: Result<i64, i32> = widen(42)
    match result {
        Ok(value) -> {
            return value as i32
        }
        Err(error) -> {
            return error
        }
    }
    return -1
}
"#,
        42,
    );
}

#[test]
fn v2_option_try_extracts_and_propagates() {
    assert_exit(
        r#"
increment: () -> Option<i32> {
    value: Option<i32> = Some(41)
    inner := value?
    return Some(inner + 1)
}

main: () -> i32 {
    result: Option<i32> = increment()
    match result {
        Some(value) -> {
            return value
        }
        None -> {
            return 0
        }
    }
    return -1
}
"#,
        42,
    );
}

#[test]
fn v2_option_try_propagates_none() {
    assert_exit(
        r#"
increment: () -> Option<i32> {
    value: Option<i32> = None
    inner := value?
    return Some(inner + 1)
}

main: () -> i32 {
    result: Option<i32> = increment()
    match result {
        Some(value) -> {
            return value
        }
        None -> {
            return 42
        }
    }
    return -1
}
"#,
        42,
    );
}

#[test]
fn v2_try_rejects_non_carrier_operand() {
    assert_compile_fails(
        r#"
main: () -> i32 {
    value := 1?
    return value
}
"#,
    );
}

#[test]
fn v2_try_rejects_incompatible_error_type() {
    assert_compile_fails(
        r#"
parse: () -> Result<i32, i64> {
    return Err(1)
}

run: () -> Result<i32, i32> {
    value := parse()?
    return Ok(value)
}

main: () -> i32 {
    return 0
}
"#,
    );
}

// --- Aggregate by-value function ABI ---

#[test]
fn v2_enum_argument_is_passed_by_value() {
    assert_exit(
        r#"
read: (value: Option<i32>) -> i32 {
    match value {
        Some(inner) -> {
            return inner
        }
        None -> {
            return 0
        }
    }
    return -1
}

main: () -> i32 {
    value: Option<i32> = Some(42)
    return read(value)
}
"#,
        42,
    );
}

#[test]
fn v2_struct_argument_is_passed_by_value() {
    assert_exit(
        r#"
struct Pair {
    left: i32
    right: i32
}

sum: (pair: Pair) -> i32 {
    return pair.left + pair.right
}

main: () -> i32 {
    pair: Pair = Pair { left: 19, right: 23 }
    return sum(pair)
}
"#,
        42,
    );
}

#[test]
fn v2_slice_argument_is_passed_by_value() {
    assert_exit(
        r#"
sum: (values: i32[]) -> i32 {
    return values[0] + values[1]
}

main: () -> i32 {
    values: i32[2] = [19, 23]
    view: i32[] = values
    return sum(view)
}
"#,
        42,
    );
}

#[test]
fn v2_large_struct_argument_is_copied() {
    assert_exit(
        r#"
struct Triple {
    first: i64
    second: i64
    third: i64
}

mutate: (value: Triple) -> i32 {
    value.first = 0
    return (value.second + value.third) as i32
}

main: () -> i32 {
    value: Triple = Triple { first: 10, second: 12, third: 20 }
    ignored: i32 = mutate(value)
    return (value.first + value.second + value.third) as i32
}
"#,
        42,
    );
}

#[test]
fn v2_aggregate_argument_can_overflow_to_stack() {
    assert_exit(
        r#"
struct Pair {
    left: i32
    right: i32
}

sum: (a: i32, b: i32, c: i32, d: i32, e: i32, f: i32, g: i32, h: i32, pair: Pair) -> i32 {
    return a + b + c + d + e + f + g + h + pair.left + pair.right
}

main: () -> i32 {
    pair: Pair = Pair { left: 15, right: 19 }
    return sum(1, 1, 1, 1, 1, 1, 1, 1, pair)
}
"#,
        42,
    );
}

#[test]
fn v2_slice_round_trips_through_function_return() {
    assert_exit(
        r#"
identity: (values: i32[]) -> i32[] {
    return values
}

main: () -> i32 {
    values: i32[2] = [19, 23]
    view: i32[] = values
    returned: i32[] = identity(view)
    return returned[0] + returned[1]
}
"#,
        42,
    );
}

// --- Milestone 7: value-producing match ---

#[test]
fn v2_value_match_inferred_binding() {
    // `:=` infers i32 from the arms; Rect(6,7) -> 42.
    assert_exit(
        r#"
enum Shape {
    Circle(i32)
    Rect(i32, i32)
    Empty
}

main: () -> i32 {
    s: Shape = Rect(6, 7)
    area := match s {
        Circle(r) -> r * r
        Rect(w, h) -> w * h
        Empty -> 0
    }
    return area
}
"#,
        42,
    );
}

#[test]
fn v2_value_match_typed_binding() {
    assert_exit(
        r#"
enum Shape {
    Circle(i32)
    Rect(i32, i32)
    Empty
}

main: () -> i32 {
    s: Shape = Circle(7)
    area: i32 = match s {
        Circle(r) -> r * r
        Rect(w, h) -> w * h
        Empty -> 0
    }
    return area
}
"#,
        49,
    );
}

#[test]
fn v2_value_match_as_return_value() {
    assert_exit(
        r#"
enum Shape {
    Circle(i32)
    Rect(i32, i32)
    Empty
}

classify: (s: Shape) -> i32 {
    return match s {
        Circle(r) -> r
        Rect(w, h) -> w + h
        Empty -> -1
    }
}

main: () -> i32 {
    return classify(Rect(40, 2))
}
"#,
        42,
    );
}

#[test]
fn v2_value_match_assigned_to_existing_binding() {
    assert_exit(
        r#"
enum Color {
    Red
    Green
    Blue
}

main: () -> i32 {
    c: Color = Green
    n: i32 = 0
    n = match c {
        Red -> 1
        Green -> 2
        Blue -> 3
    }
    return n * 21
}
"#,
        42,
    );
}

#[test]
fn v2_value_match_with_wildcard_arm() {
    assert_exit(
        r#"
enum Color {
    Red
    Green
    Blue
}

main: () -> i32 {
    c: Color = Blue
    n := match c {
        Red -> 10
        _ -> 42
    }
    return n
}
"#,
        42,
    );
}

// --- Milestone 8: empty-literal array zero-fill ---

#[test]
fn v2_empty_array_literal_zero_fills() {
    // `buf: i32[4] = []` zeroes every element; summing them is 0, then we write one.
    assert_exit(
        r#"
main: () -> i32 {
    buf: i32[4] = []
    sum: i32 = 0
    for v in buf {
        sum = sum + v
    }
    buf[2] = 42
    return sum + buf[2]
}
"#,
        42,
    );
}

#[test]
fn v2_empty_byte_buffer_zero_fills() {
    assert_exit(
        r#"
main: () -> i32 {
    buf: u8[8] = []
    total: i32 = 0
    for b in buf {
        total = total + b as i32
    }
    return total
}
"#,
        0,
    );
}

// --- Milestone 8: strings are u8[] slices ---

#[test]
fn v2_string_literal_has_slice_len() {
    assert_exit(
        r#"
main: () -> i32 {
    s := "hello"
    return s.len as i32
}
"#,
        5,
    );
}

#[test]
fn v2_string_literal_indexes_bytes() {
    // 'e' is 101; element access is bounds-checked u8 indexing.
    assert_exit(
        r#"
main: () -> i32 {
    s := "hello"
    return s[1] as i32
}
"#,
        101,
    );
}

#[test]
fn v2_string_for_loop_sums_bytes() {
    // 'A' (65) + 'B' (66) = 131.
    assert_exit(
        r#"
main: () -> i32 {
    s := "AB"
    total: i32 = 0
    for c in s {
        total = total + c as i32
    }
    return total
}
"#,
        131,
    );
}

#[test]
fn v2_string_range_slice() {
    // "hello"[1..3] is "el"; its length is 2 and first byte is 'e' (101).
    assert_exit(
        r#"
main: () -> i32 {
    s := "hello"
    t := s[1..3]
    return (t.len as i32) * 1000 + t[0] as i32
}
"#,
        2101,
    );
}

#[test]
fn v2_value_match_mixed_value_and_block_arms_is_rejected() {
    assert_compile_fails(
        r#"
enum Color {
    Red
    Green
}

main: () -> i32 {
    c: Color = Red
    n := match c {
        Red -> 1
        Green -> {
            return 2
        }
    }
    return n
}
"#,
    );
}
