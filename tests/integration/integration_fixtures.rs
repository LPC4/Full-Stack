use full_stack::compilation_pipeline::CompilationPipeline;
use std::fs;
use std::path::{Path, PathBuf};
fn integration_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("programs/test/integration")
}
fn collect_hll_fixtures(root: &Path) -> Vec<PathBuf> {
    let mut fixtures = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir)
            .unwrap_or_else(|err| panic!("failed to read integration fixtures at {dir:?}: {err}"));
        for entry in entries {
            let entry = entry
                .unwrap_or_else(|err| panic!("failed to read directory entry in {dir:?}: {err}"));
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("hll") {
                fixtures.push(path);
            }
        }
    }
    fixtures.sort();
    fixtures
}
#[test]
fn integration_hll_fixtures_compile() {
    let fixtures = collect_hll_fixtures(&integration_root());
    assert!(
        !fixtures.is_empty(),
        "expected at least one integration fixture"
    );
    let mut pipeline = CompilationPipeline::new();
    // Integration fixtures exercise parser/lowering breadth; semantic generic resolution is still incomplete.
    pipeline.set_run_semantic_analysis(false);
    for fixture in fixtures {
        let source = fs::read_to_string(&fixture)
            .unwrap_or_else(|err| panic!("failed to read fixture {fixture:?}: {err}"));
        pipeline.compile(&source).unwrap_or_else(|err| {
            panic!(
                "integration fixture {:?} failed to compile: {err}",
                fixture.file_name().unwrap()
            )
        });
    }
}
