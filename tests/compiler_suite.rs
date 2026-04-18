use full_stack::high_level_language::compiler::HighLevelCompiler;
use full_stack::high_level_language::{lexer::Lexer, parser::Parser, token::Token};
use std::fs;
use std::path::{PathBuf};

fn suite_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("programs/test/compiler_suite")
}

#[test]
fn execute_compiler_test_suite() {
    let root = suite_root();
    let mut entries: Vec<_> = fs::read_dir(&root)
        .expect("failed to read test directory")
        .filter_map(Result::ok)
        .collect();

    entries.sort_by_key(|e| e.path());

    let mut tests_run = 0;

    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("hll") {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("failed to read fixture {path:?}: {err}"));

            // Lex
            let mut lexer = Lexer::new(&source);
            let mut tokens = Vec::new();
            loop {
                let token = lexer.next_token();
                let is_eof = matches!(token, Token::Eof);
                tokens.push(token);
                if is_eof {
                    break;
                }
            }

            // Parse
            let mut parser = Parser::new(tokens);
            let ast = parser.parse_program().unwrap_or_else(|e| {
                panic!(
                    "Parse error in {:?} at pos {}: {}",
                    path.file_name().unwrap(),
                    e.pos,
                    e.message
                )
            });

            // Compile
            let mut compiler = HighLevelCompiler::new();
            let ir_program = compiler.compile_program(&ast).unwrap_or_else(|e| {
                panic!("Compile error in {:?}: {:?}", path.file_name().unwrap(), e)
            });

            let actual_ir = format!("{}", ir_program)
                .replace("\r\n", "\n")
                .trim()
                .to_string();

            let ir_path = path.with_extension("ir");
            if ir_path.exists() {
                let expected_ir = fs::read_to_string(&ir_path)
                    .unwrap()
                    .replace("\r\n", "\n")
                    .trim()
                    .to_string();
                assert_eq!(
                    actual_ir,
                    expected_ir,
                    "\n=== IR MISMATCH in {:?} ===\nEXPECTED:\n{}\n\nGOT:\n{}\n================\n",
                    path.file_name().unwrap(),
                    expected_ir,
                    actual_ir
                );
                tests_run += 1;
            } else {
                fs::write(&ir_path, actual_ir).expect("Failed to write golden ir file");
                println!(
                    "Created new golden IR file for {:?}",
                    path.file_name().unwrap()
                );
                tests_run += 1;
            }
        }
    }

    assert!(tests_run > 0, "No tests found in compiler_suite");
    println!(
        "Successfully ran {} golden master compilation tests",
        tests_run
    );
}
