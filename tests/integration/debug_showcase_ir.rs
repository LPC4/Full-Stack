use full_stack::high_level_language::compilation_pipeline::CompilationPipeline;
use std::fs;
use std::path::PathBuf;

fn normalize_ir(text: &str) -> String {
    text.replace("\r\n", "\n").trim().to_string()
}

#[test]
fn debug_showcase_ir_matches_snapshot() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source_path = root.join("programs/debug/debug.hll");
    let snapshot_path = root.join("programs/debug/debug.ir");

    let source = fs::read_to_string(&source_path)
        .unwrap_or_else(|err| panic!("failed to read showcase source {source_path:?}: {err}"));
    let expected = fs::read_to_string(&snapshot_path)
        .unwrap_or_else(|err| panic!("failed to read showcase snapshot {snapshot_path:?}: {err}"));

    let pipeline = CompilationPipeline::new();
    let result = pipeline
        .compile(&source)
        .unwrap_or_else(|err| panic!("debug showcase failed to compile: {err}"));

    let actual = normalize_ir(&format!("{}", result.ir_program));
    let expected = normalize_ir(&expected);

    assert_eq!(actual, expected, "debug showcase IR changed unexpectedly");
}
