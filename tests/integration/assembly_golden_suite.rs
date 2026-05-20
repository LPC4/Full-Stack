#[path = "../common/golden_support.rs"]
mod golden_support;

use full_stack::compilation_pipeline::CompilationPipeline;
use std::fs;
use std::path::PathBuf;

fn suite_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("programs/test/compiler_suite")
}

/// Recursively collect all .hll files from a directory tree
fn collect_hll_files(dir: &PathBuf, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                collect_hll_files(&path, files);
            } else if path.extension().and_then(|e| e.to_str()) == Some("hll") {
                files.push(path);
            }
        }
    }
}

#[test]
fn execute_assembly_golden_suite() {
    let root = suite_root();
    let mut hll_files = Vec::new();
    collect_hll_files(&root, &mut hll_files);

    // Sort for consistent test execution order
    hll_files.sort();

    let mut tests_run = 0;
    let pipeline = CompilationPipeline::new();
    let update_goldens = golden_support::should_update_goldens("UPDATE_ASM_GOLDENS");

    for path in hll_files {
        if path.extension().and_then(|e| e.to_str()) == Some("hll") {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("failed to read fixture {path:?}: {err}"));

            // Compile HLL -> IR -> Assembly using shared pipeline
            let result = pipeline.compile(&source).unwrap_or_else(|e| {
                panic!(
                    "Compilation error in {:?}: {}",
                    path.file_name().unwrap(),
                    e
                )
            });

            let actual_asm = pipeline.compile_ir_to_assembly(&result.ir_program);
            let actual_asm = actual_asm.replace("\r\n", "\n").trim().to_string();

            let asm_path = path.with_extension("s");
            if update_goldens {
                fs::write(&asm_path, &actual_asm).expect("Failed to write golden assembly file");
                println!(
                    "Updated golden assembly file for {:?}",
                    path.file_name().unwrap()
                );
                tests_run += 1;
            } else if asm_path.exists() {
                let expected_asm = fs::read_to_string(&asm_path)
                    .unwrap()
                    .replace("\r\n", "\n")
                    .trim()
                    .to_string();
                assert_eq!(
                    actual_asm,
                    expected_asm,
                    "\n=== ASSEMBLY MISMATCH in {:?} ===\nEXPECTED:\n{}\n\nGOT:\n{}\n================\n",
                    path.file_name().unwrap(),
                    expected_asm,
                    actual_asm
                );
                tests_run += 1;
            } else {
                panic!(
                    "Missing golden assembly file for {:?}; rerun with UPDATE_ASM_GOLDENS=1 to bootstrap it",
                    path.file_name().unwrap()
                );
            }
        }
    }

    assert!(tests_run > 0, "No tests found in assembly_golden_suite");
    println!(
        "\nSuccessfully ran {} golden master assembly tests across all categories",
        tests_run
    );
}
