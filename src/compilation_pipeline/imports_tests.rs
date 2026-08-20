use super::*;

// A unit's direct `import` pulls in the target's exported interface: types/consts
// fold locally and exported functions resolve as `external` references.
#[test]
fn import_pulls_exported_interface_into_importer() {
    let provider = r#"
export struct P { x: i32 }
export const K = 5
export helper: () -> i32 { return 7 }
private_helper: () -> i32 { return 99 }
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_mangling(true);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| {
        (name == "provider").then(|| provider.to_owned())
    })));

    let importer = r#"
provider := import("provider")
main: () -> i32 {
    p: provider.P = { .x = provider.K }
    return p.x + provider.helper()
}
"#;

    let result = pipeline
        .compile(importer)
        .expect("import should resolve and compile");
    assert!(
        result.diagnostics.iter().all(|diagnostic| {
            !diagnostic
                .message
                .contains("extern function `helper` - definition omitted")
        }),
        "import-generated extern should not warn: {:?}",
        result.diagnostics
    );
    let ir = result.ir_program.to_string();
    assert!(ir.contains("main"), "missing main: {ir}");
    assert!(ir.contains("helper"), "exported fn not referenced: {ir}");
    // The private helper is not part of the interface and must not leak in.
    assert!(!ir.contains("private_helper"), "private decl leaked: {ir}");
}

#[test]
fn imported_exported_impl_resolves_method_call() {
    let provider = r#"
export struct Rect { width: i32, height: i32 }
export impl Rect {
    area: (self: Rect*) -> i32 { return self.width * self.height }
}
"#;
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| {
        (name == "geometry").then(|| provider.to_owned())
    })));

    let importer = r#"
geometry := import("geometry")
main: () -> i32 {
    rect: geometry.Rect = { .width = 6, .height = 7 }
    return rect.area()
}
"#;
    let result = pipeline
        .compile(importer)
        .expect("exported impl should resolve in importer");
    let ir = result.ir_program.to_string();
    assert!(
        ir.contains("Rect_area"),
        "missing imported method call: {ir}"
    );
}

#[test]
fn imported_interface_can_bound_a_local_generic() {
    let provider = r#"
export interface Measurable {
    area: (self: Self*) -> i32
}
export struct Rect { width: i32, height: i32 }
export impl Rect {
    area: (self: Rect*) -> i32 { return self.width * self.height }
}
"#;
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| {
        (name == "geometry").then(|| provider.to_owned())
    })));

    let importer = r#"
geometry := import("geometry")
measure: <T: geometry.Measurable>(value: T*) -> i32 { return value.area() }
main: () -> i32 {
    rect: geometry.Rect = { .width = 6, .height = 7 }
    return measure<geometry.Rect>(&rect)
}
"#;
    pipeline
        .compile_program_closure("main", importer)
        .expect("qualified imported interface bound should compile");
}

#[test]
fn imported_bounded_struct_preserves_its_interface_contract() {
    let provider = r#"
export interface Measurable {
    area: (self: Self*) -> i32
}
export struct Holder<T: Measurable> { value: T }
"#;
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| {
        (name == "geometry").then(|| provider.to_owned())
    })));

    let importer = r#"
geometry := import("geometry")
struct Point { x: i32 }
main: () -> i32 {
    holder: geometry.Holder<Point> = { .value = Point { .x = 1 } }
    return holder.value.x
}
"#;
    let errors = pipeline
        .compile(importer)
        .expect_err("imported aggregate bound must reject a nonconforming type");
    assert!(
        errors.to_string().contains("does not implement interface"),
        "unexpected diagnostics: {errors:?}"
    );
}

// An exported generic travels as a body, not an `external` signature: the importer
// specializes it into its own object (spec 6.3).
#[test]
fn imported_generic_function_specializes_in_the_importer() {
    let provider = r#"
export max: <T>(a: T, b: T) -> T {
    if a > b { return a }
    return b
}
"#;
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_mangling(true);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| {
        (name == "util").then(|| provider.to_owned())
    })));

    let importer = r#"
util := import("util")
main: () -> i32 {
    return util.max<i32>(19, 23) - 23
}
"#;
    let result = pipeline
        .compile(importer)
        .expect("qualified generic call should compile");
    let ir = result.ir_program.to_string();
    assert!(
        ir.contains("define max__i32"),
        "the template should specialize into the importer's own object: {ir}"
    );
}

// Type arguments may still be inferred through the module alias.
#[test]
fn imported_generic_infers_its_type_arguments() {
    let provider = r#"
export first: <T>(values: T*) -> T { return values[0] }
"#;
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_mangling(true);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| {
        (name == "util").then(|| provider.to_owned())
    })));

    let importer = r#"
util := import("util")
main: () -> i32 {
    values: i32[2] = [42, 7]
    return util.first(&values[0])
}
"#;
    pipeline
        .compile(importer)
        .expect("inferred qualified generic call should compile");
}

// A generic body an importer specializes cannot reach the provider's private symbols:
// they are local object symbols with no link visibility.
#[test]
fn exported_generic_referencing_a_private_symbol_is_rejected() {
    let provider = r#"
secret: (x: i32) -> i32 { return x + 1 }
export bump: <T>(value: i32) -> i32 { return secret(value) }
"#;
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| {
        (name == "util").then(|| provider.to_owned())
    })));

    let importer = r#"
util := import("util")
main: () -> i32 { return util.bump<i32>(1) }
"#;
    let errors = pipeline
        .compile(importer)
        .expect_err("private reference in a generic body must be rejected");
    assert!(
        errors.to_string().contains("references `secret`"),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn exported_generic_retains_another_modules_function_interface() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| match name {
        "util" => Some(
            "inner := import(\"inner\")
                 export bump: <T>(value: i32) -> i32 { return inner.step(value) }
"
            .to_owned(),
        ),
        "inner" => Some(
            "export step: (x: i32) -> i32 { return x + 1 }
"
            .to_owned(),
        ),
        _ => None,
    })));

    let importer = r#"
util := import("util")
main: () -> i32 { return util.bump<i32>(1) }
"#;
    pipeline
        .compile(importer)
        .expect("a generic body should retain its qualified imported call");
}

#[test]
fn exported_generic_rejects_a_private_imported_member() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| match name {
        "util" => Some(
            "inner := import(\"inner\")
             export bump: <T>(value: i32) -> i32 { return inner.step(value) }
"
            .to_owned(),
        ),
        "inner" => Some("step: (x: i32) -> i32 { return x + 1 }\n".to_owned()),
        _ => None,
    })));

    let importer = r#"
util := import("util")
main: () -> i32 { return util.bump<i32>(1) }
"#;
    let error = pipeline
        .compile(importer)
        .expect_err("a private transitive member must remain inaccessible");
    assert!(
        error.to_string().contains("member `step`") && error.to_string().contains("private"),
        "unexpected diagnostics: {error:?}"
    );
}

#[test]
fn exported_generic_retains_another_modules_type_interface() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| match name {
        "boxes" => Some(
            "types := import(\"types\")
             export unwrap: <T>(item: types.Box<T>) -> T { return item.value }
"
            .to_owned(),
        ),
        "types" => Some("export struct Box<T> { value: T }\n".to_owned()),
        _ => None,
    })));

    let importer = r#"
boxes := import("boxes")
types := import("types")
main: () -> i32 {
    item: types.Box<i32> = { .value = 42 }
    return boxes.unwrap<i32>(item) - 42
}
"#;
    pipeline
        .compile(importer)
        .expect("an imported generic should retain qualified imported types");
}

#[test]
fn transitive_import_alias_does_not_collide_with_importer_alias() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| match name {
        "wrapper" => Some(
            "types := import(\"inner_types\")\n\
             export unwrap: <T>(item: types.Box<T>) -> T { return item.value }\n"
                .to_owned(),
        ),
        "inner_types" => Some("export struct Box<T> { value: T }\n".to_owned()),
        "outer_types" => Some("export type Number = i32\n".to_owned()),
        _ => None,
    })));

    let importer = r#"
types := import("outer_types")
wrapper := import("wrapper")
main: () -> types.Number {
    item: Box<i32> = { .value = 42 }
    return wrapper.unwrap<i32>(item)
}
"#;
    pipeline
        .compile(importer)
        .expect("a transitive alias must not occupy the importer's alias scope");
}

#[test]
fn chained_imported_type_aliases_resolve_in_each_module_scope() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| match name {
        "leaf" => Some("export type Scalar = i32\n".to_owned()),
        "middle" => {
            Some("types := import(\"leaf\")\nexport type Number = types.Scalar\n".to_owned())
        }
        "top" => {
            Some("types := import(\"middle\")\nexport type Result = types.Number\n".to_owned())
        }
        _ => None,
    })));

    let importer = r#"
types := import("top")
main: () -> types.Result { return 42 }
"#;
    pipeline
        .compile(importer)
        .expect("chained imported aliases should resolve through their declaring modules");
}

// An exported generic keeps its bound: the importer checks it at specialization.
#[test]
fn imported_generic_bound_is_checked_at_the_importer() {
    let provider = r#"
export interface Measurable {
    area: (self: Self*) -> i32
}
export double_area: <T: Measurable>(value: T*) -> i32 { return value.area() + value.area() }
"#;
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| {
        (name == "geometry").then(|| provider.to_owned())
    })));

    let importer = r#"
geometry := import("geometry")
struct Point { x: i32 }
main: () -> i32 { return geometry.double_area<Point>(&Point { .x = 1 }) }
"#;
    let errors = pipeline
        .compile(importer)
        .expect_err("a nonconforming type must fail the imported bound");
    assert!(
        errors.to_string().contains("does not implement interface"),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn private_member_diagnostic_distinguishes_private_from_missing() {
    let provider = r#"
export helper: () -> i32 { return 7 }
secret: () -> i32 { return 99 }
"#;
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| {
        (name == "provider").then(|| provider.to_owned())
    })));

    let private_caller = r#"
provider := import("provider")
main: () -> i32 { return provider.secret() }
"#;
    let errors = pipeline
        .compile(private_caller)
        .expect_err("private member access must fail");
    assert!(
        errors
            .to_string()
            .contains("member `secret` of module `provider` is private"),
        "unexpected diagnostics: {errors:?}"
    );

    let missing_caller = r#"
provider := import("provider")
main: () -> i32 { return provider.absent() }
"#;
    let errors = pipeline
        .compile(missing_caller)
        .expect_err("missing member access must fail");
    assert!(
        errors
            .to_string()
            .contains("module `provider` has no exported member `absent`"),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn lambda_synthesized_functions_stay_module_private() {
    let provider = r#"
export bump: (value: i32) -> i32 {
    adjust: (i32) -> i32 = (v: i32) -> i32 { return v + 1 }
    return adjust(value)
}
"#;
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| {
        (name == "provider").then(|| provider.to_owned())
    })));

    // The importer synthesizes its own __hll_lambda_0; the provider's must
    // neither leak into the interface nor collide at link.
    let importer = r#"
provider := import("provider")
main: () -> i32 {
    shrink: (i32) -> i32 = (v: i32) -> i32 { return v - 1 }
    return shrink(provider.bump(42))
}
"#;
    pipeline
        .compile_program_closure("main", importer)
        .expect("module-private lambdas must not collide across modules");
}

#[test]
fn transitive_re_export_resolves_through_middle_module() {
    let base = r#"
export add: (a: i32, b: i32) -> i32 { return a + b }
"#;
    let middle = r#"
base := import("base")
export external add: (a: i32, b: i32) -> i32
"#;
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| match name {
        "base" => Some(base.to_owned()),
        "middle" => Some(middle.to_owned()),
        _ => None,
    })));

    let importer = r#"
middle := import("middle")
main: () -> i32 { return middle.add(20, 22) }
"#;
    pipeline
        .compile_program_closure("main", importer)
        .expect("re-exported member should resolve through the middle module");
}

#[test]
fn same_named_interfaces_from_two_modules_remain_distinct() {
    let first = r#"
export interface Capability { first: (self: Self*) -> i32 }
"#;
    let second = r#"
export interface Capability { second: (self: Self*) -> i32 }
"#;
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_mangling(true);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| match name {
        "first" => Some(first.to_owned()),
        "second" => Some(second.to_owned()),
        _ => None,
    })));

    let importer = r#"
first := import("first")
second := import("second")
struct Both { value: i32 }
impl Both {
    first: (self: Both*) -> i32 { return self.value }
    second: (self: Both*) -> i32 { return self.value }
}
use_both: <T: first.Capability + second.Capability>(value: T*) -> i32 {
    return value.first() + value.second()
}
main: () -> i32 {
    value: Both = Both { .value = 21 }
    return use_both<Both>(&value)
}
"#;
    pipeline
        .compile_program_closure("main", importer)
        .expect("same-named imported interfaces should not collide");
}

#[test]
fn user_written_external_function_still_warns() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);

    let result = pipeline
        .compile(
            r#"
external helper: () -> i32
main: () -> i32 { return helper() }
"#,
        )
        .expect("external function program should compile");

    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.level == DiagnosticLevel::Warning
                && diagnostic
                    .message
                    .contains("extern function `helper` - definition omitted")
        }),
        "user-written external should still warn: {:?}",
        result.diagnostics
    );
}

// A qualified `alias := import("path")` binding reaches exported functions and consts
// through the alias; the importer's bare namespace stays clean.
#[test]
fn qualified_import_resolves_calls_and_consts() {
    let provider = r#"
export const ANSWER = 42
export add: (a: i32, b: i32) -> i32 { return a + b }
private_helper: () -> i32 { return 99 }
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| {
        (name == "math").then(|| provider.to_owned())
    })));

    let importer = r#"
math := import("math")
main: () -> i32 {
    return math.add(math.ANSWER, 0)
}
"#;

    let result = pipeline
        .compile(importer)
        .expect("qualified import should resolve and compile");
    let ir = result.ir_program.to_string();
    assert!(ir.contains("call add("), "qualified call not lowered: {ir}");
    assert!(!ir.contains("private_helper"), "private decl leaked: {ir}");
}

#[test]
fn qualified_import_resolves_exported_types() {
    let provider = r#"
export struct Point { x: i32, y: i32 }
"#;

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| {
        (name == "geom").then(|| provider.to_owned())
    })));

    let importer = r#"
geom := import("geom")
main: () -> i32 {
    p: geom.Point = { .x = 40, .y = 2 }
    return p.x + p.y
}
"#;

    let result = pipeline
        .compile(importer)
        .expect("qualified imported type should resolve and compile");
    let ir = result.ir_program.to_string();
    assert!(ir.contains("type Point"), "imported type missing: {ir}");
}

#[test]
fn normalize_lexical_resolves_dot_segments() {
    assert_eq!(
        normalize_lexical(Path::new("programs/user/bin/shell/./../as/as_object.hll")),
        PathBuf::from("programs/user/bin/as/as_object.hll")
    );
}

#[test]
fn embedded_source_files_include_tool_helpers() {
    for key in [
        "programs/user/bin/shell/shell_fileops.hll",
        "programs/user/bin/as/as_object.hll",
        "programs/user/bin/cc/cc_codegen.hll",
        "programs/user/bin/ld/ld_link.hll",
    ] {
        assert!(
            crate::userspace::source_by_path(key).is_some(),
            "missing embedded helper: {key}"
        );
    }
}

#[test]
fn relative_import_resolves_from_embedded_table_without_fs() {
    let pipeline = CompilationPipeline::new();
    // A virtual base under programs/ stands in for the WASM case with no filesystem.
    let base = PathBuf::from("programs/user/bin/shell/shell.hll");
    let resolved = pipeline
        .resolve_module_source("./shell_fileops.hll", Some(base.as_path()))
        .expect("resolve ok")
        .expect("embedded helper present");
    assert_eq!(resolved.key, "programs/user/bin/shell/shell_fileops.hll");
    assert!(!resolved.source.is_empty());
    assert_eq!(
        resolved.source_path.as_deref(),
        Some(Path::new("programs/user/bin/shell/shell_fileops.hll"))
    );
}

#[test]
fn filesystem_relative_import_resolves_from_source_path() {
    let root = std::env::temp_dir().join(format!(
        "full_stack_import_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("create import test dir");
    let main_path = root.join("main.hll");
    let math_path = root.join("math.hll");
    std::fs::write(
        &math_path,
        "export struct Point { x: i32, y: i32 }\n\
             export add: (a: i32, b: i32) -> i32 { return a + b }\n",
    )
    .expect("write math module");
    let main_src = r#"
math := import("./math.hll")
main: () -> i32 {
    p: math.Point = { .x = 20, .y = 22 }
    return math.add(p.x, p.y)
}
"#;
    std::fs::write(&main_path, main_src).expect("write main module");

    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_current_source_path(Some(main_path.clone()));
    let objects = pipeline
        .compile_program_closure("main", main_src)
        .expect("filesystem import closure should compile");
    assert_eq!(objects.len(), 2, "expected imported module plus main");

    let _ = std::fs::remove_dir_all(root);
}

// Stdlib objects attribute lines to real repo files, so the debugger can
// show stdlib source while stepping through it.
#[test]
fn stdlib_objects_attribute_lines_to_real_files() {
    let objects =
        CompilationPipeline::compile_stdlib_objects(TargetMode::Hosted).expect("stdlib objects");
    let mut entries = 0usize;
    for (module, object) in &objects {
        for entry in object.line_table_iter() {
            entries += 1;
            let file = object.file_name(entry.file_id).unwrap_or("");
            assert!(
                file.starts_with("crates/os-runtime/"),
                "{module}: line entry attributed to `{file}`"
            );
        }
    }
    assert!(entries > 0, "stdlib should carry line info");
}

// Kernel modules resolved by bare `import("name")` land in the line table
// under their real `crates/os-runtime/kernel/...` paths.
#[test]
fn kernel_module_objects_attribute_lines_to_kernel_files() {
    let objects = CompilationPipeline::compile_kernel_module_objects().expect("kernel objects");
    let mut entries = 0usize;
    for (module, object) in &objects {
        for entry in object.line_table_iter() {
            entries += 1;
            let file = object.file_name(entry.file_id).unwrap_or("");
            assert!(
                file.starts_with("crates/os-runtime/kernel/"),
                "{module}: line entry attributed to `{file}`"
            );
        }
    }
    assert!(entries > 0, "kernel modules should carry line info");
}

#[test]
fn import_closure_rejects_cycles() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(|name: &str| match name {
        "a" => Some(
            "b := import(\"b\")\n\
                 export fa: () -> i32 { return b.fb() }\n"
                .to_owned(),
        ),
        "b" => Some(
            "a := import(\"a\")\n\
                 export fb: () -> i32 { return 2 }\n"
                .to_owned(),
        ),
        _ => None,
    })));

    let primary = "a := import(\"a\")\nmain: () -> i32 { return a.fa() }\n";
    let error = pipeline
        .compile_program_closure("main", primary)
        .expect_err("cyclic import closure must fail");
    let message = error.to_string();
    assert!(
        message.contains("cyclic import"),
        "unexpected error: {message}"
    );
}

// Naming a missing or private export is a compile error that names the module.
#[test]
fn qualified_import_rejects_unknown_member() {
    let provider = "export add: (a: i32, b: i32) -> i32 { return a + b }\n";
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(move |name: &str| {
        (name == "math").then(|| provider.to_owned())
    })));

    let importer = r#"
math := import("math")
main: () -> i32 {
    return math.subtract(1, 2)
}
"#;
    let error = pipeline
        .compile(importer)
        .expect_err("unknown qualified member must fail");
    let message = error.to_string();
    assert!(
        message.contains("module `math`") && message.contains("subtract"),
        "unexpected error: {message}"
    );
}

#[test]
fn qualified_import_rejects_duplicate_export_names() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(|name: &str| match name {
        "a" => Some("export add: (x: i32) -> i32 { return x + 1 }\n".to_owned()),
        "b" => Some("export add: (x: i32) -> i32 { return x + 2 }\n".to_owned()),
        _ => None,
    })));

    let importer = r#"
a := import("a")
b := import("b")
main: () -> i32 { return 0 }
"#;
    let error = pipeline
        .compile(importer)
        .expect_err("duplicate flat exports must fail before rewrite");
    let message = error.to_string();
    assert!(
        message.contains("exports `add`") && message.contains("conflicts"),
        "unexpected error: {message}"
    );
}

#[test]
fn qualified_import_rejects_duplicate_aliases() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(|name: &str| match name {
        "a" => Some("export const A = 1\n".to_owned()),
        "b" => Some("export const B = 2\n".to_owned()),
        _ => None,
    })));

    let importer = r#"
m := import("a")
m := import("b")
main: () -> i32 { return 0 }
"#;
    let error = pipeline
        .compile(importer)
        .expect_err("duplicate aliases must fail");
    let message = error.to_string();
    assert!(
        message.contains("duplicate module import alias `m`"),
        "unexpected error: {message}"
    );
}

#[test]
fn qualified_import_rejects_export_that_collides_with_local_declaration() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(|name: &str| {
        (name == "math").then(|| "export const add = 1\n".to_owned())
    })));

    let importer = r#"
math := import("math")
add: () -> i32 { return 2 }
main: () -> i32 { return add() }
"#;
    let error = pipeline
        .compile(importer)
        .expect_err("imported flat export must not collide with local declaration");
    let message = error.to_string();
    assert!(
        message.contains("exports `add`") && message.contains("top-level declaration"),
        "unexpected error: {message}"
    );
}

// An unknown import name is a compile error naming the missing module.
#[test]
fn unresolved_import_is_an_error() {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_module_resolver(Some(Box::new(|_name: &str| None)));
    let importer = "x := import(\"does_not_exist\")\nmain: () -> i32 { return 0 }\n";
    let error = pipeline
        .compile(importer)
        .expect_err("unknown import must fail");
    let message = error.to_string();
    assert!(
        message.contains("does_not_exist"),
        "unexpected error: {message}"
    );
}

#[test]
fn bundled_kernel_imports_resolve_and_extract_interfaces() {
    const NAMES: &[&str] = &[
        "layout",
        "pmm",
        "vmm",
        "process",
        "scheduler",
        "fs",
        "syscall",
        "trap_entry",
        "trap_handler",
        "utilities",
        "checks",
        "stdlib/platform/kernel/klog.hll",
        "stdlib/core/mem.hll",
        "stdlib/core/string.hll",
        "stdlib/core/alloc.hll",
        "stdlib/platform/baremetal/runtime.hll",
        "stdlib/core/console.hll",
    ];

    for name in NAMES {
        let source = bundled_module_source(name)
            .unwrap_or_else(|| panic!("bundled resolver missed `{name}`"));
        hll_to_ir::imports::extract_interface(source)
            .unwrap_or_else(|e| panic!("interface extraction failed for `{name}`:\n{e}"));
    }
}
