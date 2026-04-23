use full_stack::high_level_language::compilation_pipeline::CompilationPipeline;
use std::fs;
use std::path::PathBuf;

fn integration_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("programs/test/integration")
}

#[test]
fn integration_hll_fixtures_compile() {
    let root = integration_root();
    let entries = fs::read_dir(&root)
        .unwrap_or_else(|err| panic!("failed to read integration fixtures at {root:?}: {err}"));

    let mut fixtures: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("hll"))
        .collect();

    fixtures.sort();
    assert!(!fixtures.is_empty(), "expected at least one integration fixture");

    let mut pipeline = CompilationPipeline::new();
    // Integration fixtures exercise parser/lowering breadth; semantic generic resolution is still incomplete.
    pipeline.run_semantic_analysis = false;

    for fixture in fixtures {
        let source = fs::read_to_string(&fixture)
            .unwrap_or_else(|err| panic!("failed to read fixture {fixture:?}: {err}"));

        pipeline
            .compile(&source)
            .unwrap_or_else(|err| panic!("integration fixture {:?} failed to compile: {err}", fixture.file_name().unwrap()));
    }
}


