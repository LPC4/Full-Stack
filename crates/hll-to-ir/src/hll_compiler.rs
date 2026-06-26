use crate::TargetMode;
use crate::ast::{Block, DeclNode, Program, Statement};
use crate::compiler::{Diagnostic, DiagnosticLevel, HighLevelCompiler, SemanticAnalyzer};
use crate::ir::{IrProgram, IrType};
use crate::lexer::Lexer;
use crate::monomorphize::monomorphize_program;
use crate::parser::Parser;
use crate::token::Token;

// --- Public types ---

pub struct CompileConfig {
    pub target: TargetMode,
    pub strict: bool,
    pub string_prefix: Option<String>,
    pub type_prelude: Vec<(String, IrType)>,
    /// HLL source prepended to the unit before lexing (a shared definitions header).
    /// `None` falls back to the kernel `layout.hll` in kernel mode, else nothing, so
    /// every kernel TU shares one copy of the PCB / trap-frame / VMM consts (in HLL).
    pub source_prelude: Option<String>,
}

impl Default for CompileConfig {
    fn default() -> Self {
        Self {
            target: TargetMode::Hosted,
            strict: false,
            string_prefix: None,
            type_prelude: Vec::new(),
            source_prelude: None,
        }
    }
}

pub struct HllOutput {
    pub ir: IrProgram,
    pub diagnostics: Vec<Diagnostic>,
    /// Debug-formatted token list for visualizer display.
    pub tokens_display: String,
    /// Debug-formatted AST for visualizer display.
    pub ast_display: String,
}

pub struct HllCompiler {
    config: CompileConfig,
}

impl HllCompiler {
    pub fn new(config: CompileConfig) -> Self {
        Self { config }
    }

    /// `source` with the effective prelude prepended: an explicit `source_prelude`,
    /// else the shared kernel `layout.hll` in kernel mode, else unchanged.
    fn with_source_prelude(&self, source: &str) -> String {
        let prelude: &str = match &self.config.source_prelude {
            Some(p) => p.as_str(),
            None if self.config.target == TargetMode::Kernel => os_runtime::kernel::LAYOUT,
            None => "",
        };
        if prelude.is_empty() {
            source.to_owned()
        } else {
            format!("{prelude}\n{source}")
        }
    }

    /// Lex, parse, (optionally) analyse, and lower `source` to IR.
    ///
    /// Returns `Err` only on hard failures (lex errors, parse errors, IR
    /// lowering errors).  Warnings are surfaced via `HllOutput::diagnostics`.
    pub fn compile(&self, source: &str) -> Result<HllOutput, Vec<Diagnostic>> {
        // Prepend the shared definitions header (kernel layout, or an explicit prelude)
        // so separately-compiled TUs share one HLL definition of their common consts.
        let prepended = self.with_source_prelude(source);
        let source = prepended.as_str();

        // Phase 1: Lex
        let token_spans = Lexer::tokenize(source);

        // Check for lex errors before consuming the token stream.
        if let Some((Token::Error(msg), _)) = token_spans
            .iter()
            .find(|(t, _)| matches!(t, Token::Error(_)))
        {
            let tokens_display = format!("LEXER ERROR: {msg}");
            return Err(vec![
                Diagnostic::new(DiagnosticLevel::Error, format!("lexer error: {msg}"))
                    .with_note(tokens_display),
            ]);
        }

        let tokens_display = format!("{token_spans:#?}");

        // Phase 2: Parse
        let mut ast = Parser::new_with_spans(token_spans)
            .parse_program()
            .map_err(|e| vec![Diagnostic::new(DiagnosticLevel::Error, e.to_string())])?;

        ast = monomorphize_program(&ast)
            .map_err(|message| vec![Diagnostic::new(DiagnosticLevel::Error, message)])?;

        let ast_display = format!("{ast:#?}");

        // Phase 3: Semantic analysis (when strict mode enabled)
        if self.config.strict {
            let mut analyzer = SemanticAnalyzer::new();
            analyzer.seed_types(&self.config.type_prelude);
            if analyzer.analyze_program(&ast).is_err()
                || analyzer
                    .diagnostics()
                    .iter()
                    .any(|d| matches!(d.level, DiagnosticLevel::Error))
            {
                let errors: Vec<Diagnostic> = analyzer
                    .diagnostics()
                    .iter()
                    .filter(|d| matches!(d.level, DiagnosticLevel::Error))
                    .cloned()
                    .collect();
                return Err(errors);
            }
        }

        // Phase 4: IR lowering
        let prefix = self.config.string_prefix.as_deref().unwrap_or("str_");
        let mut compiler = HighLevelCompiler::with_string_prefix(prefix);
        compiler.set_type_prelude(self.config.type_prelude.clone());
        let ir = compiler
            .compile_program(&ast)
            .map_err(|e| vec![Diagnostic::new(DiagnosticLevel::Error, format!("{e:?}"))])?;

        let mut diagnostics = compiler.diagnostics().to_vec();

        // Hard-error on any error-level diagnostic from IR lowering.
        let ir_errors: Vec<String> = diagnostics
            .iter()
            .filter(|d| matches!(d.level, DiagnosticLevel::Error))
            .map(|d| d.message.clone())
            .collect();
        if !ir_errors.is_empty() {
            return Err(diagnostics
                .into_iter()
                .filter(|d| matches!(d.level, DiagnosticLevel::Error))
                .collect());
        }

        // Phase 5: Freestanding / kernel validation (warnings/errors from asm blocks)
        if matches!(
            self.config.target,
            TargetMode::Freestanding | TargetMode::Kernel
        ) {
            let freestanding_diags = check_freestanding_asm(&ast);
            let fs_errors: Vec<Diagnostic> = freestanding_diags
                .iter()
                .filter(|d| matches!(d.level, DiagnosticLevel::Error))
                .cloned()
                .collect();
            if !fs_errors.is_empty() {
                return Err(fs_errors);
            }
            diagnostics.extend(freestanding_diags);
        }

        Ok(HllOutput {
            ir,
            diagnostics,
            tokens_display,
            ast_display,
        })
    }
}

// --- Freestanding asm-block validator (moved from root compilation_pipeline.rs) ---

/// Linux RV64 userspace syscall numbers that are invalid in freestanding mode.
const LINUX_USERSPACE_SYSCALLS: &[(u64, &str)] = &[
    (17, "sys_getcwd"),
    (23, "sys_dup"),
    (29, "sys_ioctl"),
    (34, "sys_mkdirat"),
    (35, "sys_unlinkat"),
    (48, "sys_faccessat"),
    (49, "sys_chdir"),
    (56, "sys_openat"),
    (57, "sys_close"),
    (62, "sys_lseek"),
    (63, "sys_read"),
    (64, "sys_write"),
    (65, "sys_readv"),
    (66, "sys_writev"),
    (80, "sys_fstat"),
    (93, "sys_exit"),
    (94, "sys_exit_group"),
    (96, "sys_set_tid_address"),
    (160, "sys_uname"),
    (172, "sys_getpid"),
    (214, "sys_brk"),
    (222, "sys_mmap"),
    (226, "sys_mprotect"),
];

fn check_freestanding_asm(program: &Program) -> Vec<Diagnostic> {
    let mut diags: Vec<Diagnostic> = Vec::new();
    let mut has_external_main = false;

    for decl in &program.declarations {
        if let DeclNode::Function {
            name,
            is_extern,
            body,
            ..
        } = &decl.decl
        {
            if *is_extern && name == "main" {
                has_external_main = true;
            }
            if let Some(block) = body {
                check_asm_in_block(block, &mut diags);
            }
        }
    }

    if has_external_main {
        diags.push(
            Diagnostic::new(
                DiagnosticLevel::Warning,
                "`external main` declaration found in freestanding mode",
            )
            .with_note(
                "freestanding builds do not call `main`; \
                 use a custom entry point (e.g., `kmain`) instead",
            ),
        );
    }

    diags
}

fn check_asm_in_block(block: &Block, diags: &mut Vec<Diagnostic>) {
    for stmt in &block.statements {
        check_asm_in_stmt(stmt, diags);
    }
}

fn check_asm_in_stmt(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    match stmt {
        Statement::AsmBlock { lines } => check_asm_lines(lines, diags),
        Statement::Block(b) => check_asm_in_block(b, diags),
        Statement::If {
            then_block,
            else_branch,
            ..
        } => {
            check_asm_in_block(then_block, diags);
            if let Some(else_stmt) = else_branch {
                check_asm_in_stmt(else_stmt, diags);
            }
        }
        Statement::While { body, .. } => check_asm_in_block(body, diags),
        _ => {}
    }
}

fn check_asm_lines(lines: &[String], diags: &mut Vec<Diagnostic>) {
    let has_ecall = lines
        .iter()
        .any(|l| l.split_whitespace().next() == Some("ecall"));

    if !has_ecall {
        return;
    }

    for line in lines {
        let trimmed = line.trim();
        if let Some(rest) = trimmed
            .to_lowercase()
            .strip_prefix("li")
            .and_then(|s| s.trim_start().strip_prefix("a7"))
            .and_then(|s| s.trim_start().strip_prefix(','))
        {
            let num_str = rest.trim();
            if let Ok(n) = num_str.parse::<u64>()
                && let Some(&(_, name)) = LINUX_USERSPACE_SYSCALLS.iter().find(|&&(id, _)| id == n)
            {
                diags.push(
                    Diagnostic::new(
                        DiagnosticLevel::Warning,
                        format!("asm block invokes Linux userspace syscall {n} ({name}) via ecall"),
                    )
                    .with_note(
                        "freestanding builds run without an OS; use MMIO or SBI ecalls \
                             (extension IDs  0x10) instead of Linux userspace syscall numbers",
                    ),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use crate::compiler::HighLevelCompiler;
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::token::Token;
    use crate::{CompileConfig, Diagnostic, HllCompiler, HllOutput, TargetMode};

    fn compile_hll(source: &str) -> Result<HllOutput, Vec<Diagnostic>> {
        HllCompiler::new(CompileConfig {
            target: TargetMode::Hosted,
            strict: true,
            ..CompileConfig::default()
        })
        .compile(source)
    }

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../programs/test/fixtures")
    }

    fn collect_hll_fixtures(root: &Path) -> Vec<PathBuf> {
        let mut fixtures = Vec::new();
        let mut stack = vec![root.to_path_buf()];

        while let Some(dir) = stack.pop() {
            let entries = fs::read_dir(&dir)
                .unwrap_or_else(|err| panic!("failed to read fixture directory {dir:?}: {err}"));

            for entry in entries {
                let entry = entry.unwrap_or_else(|err| {
                    panic!("failed to read directory entry in {dir:?}: {err}")
                });
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

    fn lex_source(source: &str) -> Vec<Token<'_>> {
        let mut lexer = Lexer::new(source);
        let mut tokens = Vec::new();

        loop {
            let token = lexer.next_token();
            let is_eof = matches!(token, Token::Eof);
            tokens.push(token);
            if is_eof {
                break;
            }
        }

        tokens
    }

    fn lex_fixture(file_name: &str) -> Vec<Token<'static>> {
        let path = fixture_root().join(file_name);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read fixture {path:?}: {err}"));
        let source: &'static str = Box::leak(source.into_boxed_str());
        lex_source(source)
    }

    fn parse_fixture(file_name: &str) -> Result<crate::ast::Program, String> {
        let path = fixture_root().join(file_name);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read fixture {path:?}: {err}"));
        let source: &'static str = Box::leak(source.into_boxed_str());

        let tokens = lex_source(source);
        let mut parser = Parser::new(tokens);
        parser.parse_program().map_err(|err| format!("{err}"))
    }

    fn contains_token<F>(tokens: &[Token<'_>], predicate: F) -> bool
    where
        F: Fn(&Token<'_>) -> bool,
    {
        tokens.iter().any(predicate)
    }

    #[test]
    fn all_high_level_language_fixtures_lex_to_eof() {
        let fixtures = collect_hll_fixtures(&fixture_root());
        assert!(!fixtures.is_empty(), "expected at least one .hll fixture");

        for path in fixtures {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("failed to read fixture {path:?}: {err}"));
            let tokens = lex_source(&source);

            assert!(
                matches!(tokens.last(), Some(Token::Eof)),
                "{path:?} did not end with EOF"
            );
            assert!(
                tokens
                    .iter()
                    .any(|token| matches!(token, Token::StatementTerminator)),
                "{path:?} did not contain any statement terminators"
            );
        }
    }

    #[test]
    fn test1_hll_lexes_comments_newlines_and_return() {
        let tokens = lex_fixture("lexer/01_comments_and_newlines.hll");

        assert!(contains_token(&tokens, |t| matches!(t, Token::Ident("x"))));
        assert!(contains_token(&tokens, |t| matches!(t, Token::Ident("y"))));
        assert!(contains_token(&tokens, |t| matches!(t, Token::Ident("z"))));
        assert!(contains_token(&tokens, |t| matches!(t, Token::Return)));
        assert!(
            tokens
                .iter()
                .filter(|t| matches!(t, Token::StatementTerminator))
                .count()
                >= 4
        );
        assert!(matches!(tokens.last(), Some(Token::Eof)));
    }

    #[test]
    fn test2_hll_lexes_struct_and_pointer_syntax() {
        let tokens = lex_fixture("parser/02_structs_and_pointers.hll");

        assert!(contains_token(&tokens, |t| matches!(t, Token::Struct)));
        assert!(contains_token(&tokens, |t| matches!(t, Token::LBrace)));
        assert!(contains_token(&tokens, |t| matches!(t, Token::Ampersand)));
        assert!(contains_token(&tokens, |t| matches!(t, Token::Defer)));
        assert!(contains_token(&tokens, |t| matches!(t, Token::If)));
        assert!(contains_token(&tokens, |t| matches!(t, Token::Return)));
        assert!(matches!(tokens.last(), Some(Token::Eof)));
    }

    #[test]
    fn test3_hll_lexes_nested_access_and_control_flow() {
        let tokens = lex_fixture("parser/03_nested_access_and_control_flow.hll");

        assert!(contains_token(&tokens, |t| matches!(t, Token::Struct)));
        assert!(contains_token(&tokens, |t| matches!(t, Token::While)));
        assert!(contains_token(&tokens, |t| matches!(t, Token::Break)));
        assert!(contains_token(&tokens, |t| matches!(t, Token::Or)));
        assert!(contains_token(&tokens, |t| matches!(t, Token::And)));
        assert!(contains_token(&tokens, |t| matches!(t, Token::Not)));
        assert!(matches!(tokens.last(), Some(Token::Eof)));
    }

    #[test]
    fn test4_hll_lexes_multi_return_and_destructuring() {
        let tokens = lex_fixture("parser/04_tuple_returns.hll");

        assert!(contains_token(&tokens, |t| matches!(
            t,
            Token::Ident("divide")
        )));
        assert!(contains_token(&tokens, |t| matches!(t, Token::Comma)));
        assert!(contains_token(&tokens, |t| matches!(t, Token::LBrace)));
        assert!(contains_token(&tokens, |t| matches!(t, Token::RBrace)));
        assert!(contains_token(&tokens, |t| matches!(t, Token::Colon)));
        assert!(contains_token(&tokens, |t| matches!(t, Token::If)));
        assert!(contains_token(&tokens, |t| matches!(t, Token::Return)));
        assert!(matches!(tokens.last(), Some(Token::Eof)));
    }

    #[test]
    fn test1_hll_parser_success_and_ast_validation() {
        let program =
            parse_fixture("lexer/01_comments_and_newlines.hll").expect("failed to parse test1.hll");

        assert_eq!(
            program.declarations.len(),
            3,
            "expected 3 declarations in test1.hll"
        );
        assert_eq!(
            program.statements.len(),
            1,
            "expected 1 statement in test1.hll"
        );

        use crate::ast::*;

        match &program.declarations[0].decl {
            DeclNode::Variable { name, .. } => assert_eq!(name, "x"),
            _ => panic!("Expected VariableDecl for x"),
        }

        match &program.declarations[1].decl {
            DeclNode::Variable { name, .. } => assert_eq!(name, "y"),
            _ => panic!("Expected VariableDecl for y"),
        }

        match &program.declarations[2].decl {
            DeclNode::Variable { name, init, .. } => {
                assert_eq!(name, "z");
                assert!(init.is_some(), "z should have an initializer");
                if let Expression::Binary { op, .. } = init.as_ref().unwrap() {
                    assert_eq!(*op, BinaryOp::Add, "Expected Add operation for z");
                } else {
                    panic!("Expected Binary expression for z init");
                }
            }
            _ => panic!("Expected VariableDecl for z"),
        }

        match &program.statements[0] {
            Statement::Return(Some(expr)) => {
                if let Expression::Primary(PrimaryExpr::Identifier(id)) = expr {
                    assert_eq!(id, "z");
                } else {
                    panic!("Expected return to yield identifier 'z'");
                }
            }
            _ => panic!("Expected Return statement"),
        }
    }

    #[test]
    fn test2_hll_parser_success_and_ast_validation() {
        let program =
            parse_fixture("parser/02_structs_and_pointers.hll").expect("failed to parse test2.hll");

        assert_eq!(
            program.declarations.len(),
            2,
            "Expected 2 declarations (Node struct, main function)"
        );

        use crate::ast::*;

        match &program.declarations[0].decl {
            DeclNode::Struct { name, .. } => assert_eq!(name, "Node"),
            _ => panic!("Expected Struct Node declaration"),
        }

        match &program.declarations[1].decl {
            DeclNode::Function { name, body, .. } => {
                assert_eq!(name, "main");
                assert!(body.is_some(), "main should have a block");
                let statements = &body.as_ref().unwrap().statements;
                assert_eq!(statements.len(), 7, "Expected 7 statements in main");
            }
            _ => panic!("Expected Function main declaration"),
        }
    }

    #[test]
    fn test3_hll_parser_success_and_ast_validation() {
        let program = parse_fixture("parser/03_nested_access_and_control_flow.hll")
            .expect("failed to parse test3.hll");

        assert_eq!(
            program.declarations.len(),
            2,
            "Expected Container, stress_test"
        );

        use crate::ast::*;

        match &program.declarations[0].decl {
            DeclNode::Struct { name, .. } => assert_eq!(name, "Container"),
            _ => panic!("Expected Struct Container declaration"),
        }

        match &program.declarations[1].decl {
            DeclNode::Function { name, .. } => assert_eq!(name, "stress_test"),
            _ => panic!("Expected Function stress_test declaration"),
        }
    }

    #[test]
    fn test4_hll_parser_success_and_ast_validation() {
        let program =
            parse_fixture("parser/04_tuple_returns.hll").expect("failed to parse test4.hll");

        assert_eq!(program.declarations.len(), 2, "Expected divide, start");

        use crate::ast::*;

        match &program.declarations[0].decl {
            DeclNode::Function {
                name, return_type, ..
            } => {
                assert_eq!(name, "divide");
                assert!(
                    matches!(return_type, Some(ReturnType::Single(Type::Struct(_)))),
                    "divide should return an inline struct"
                );
            }
            _ => panic!("Expected Function divide declaration"),
        }

        match &program.declarations[1].decl {
            DeclNode::Function { name, body, .. } => {
                assert_eq!(name, "start");
                let block = body.as_ref().expect("start should have a body");
                let statements = &block.statements;
                assert_eq!(statements.len(), 2, "Expected 2 statements in start");
                assert!(matches!(
                    statements[0],
                    Statement::Expression(Expression::Assignment { .. })
                ));
                assert!(matches!(statements[1], Statement::If { .. }));
            }
            _ => panic!("Expected Function start declaration"),
        }
    }

    #[test]
    fn test5_hll_parser_reordered_and_partial_struct_destructuring() {
        let program = parse_fixture("parser/05_destructuring_order_and_partial_binding.hll")
            .expect("failed to parse test5.hll");

        assert_eq!(program.declarations.len(), 3, "Expected Pair, pair, main");

        use crate::ast::*;

        match &program.declarations[2].decl {
            DeclNode::Function { name, body, .. } => {
                assert_eq!(name, "main");
                let block = body.as_ref().expect("main should have a body");
                assert_eq!(
                    block.statements.len(),
                    3,
                    "Expected two destructures and a return"
                );

                let first_assign = match &block.statements[0] {
                    Statement::Expression(Expression::Assignment { target, .. }) => target,
                    other => panic!("expected first destructuring assignment, got {other:?}"),
                };
                match first_assign.as_ref() {
                    AssignTarget::StructDestructure(fields) => {
                        assert_eq!(fields.len(), 2);
                        assert_eq!(fields[0].name, Some("second".to_string()));
                        assert_eq!(fields[1].name, Some("first".to_string()));
                    }
                    other => panic!("unexpected first destructuring target: {other:?}"),
                }

                let second_assign = match &block.statements[1] {
                    Statement::Expression(Expression::Assignment { target, .. }) => target,
                    other => panic!("expected second destructuring assignment, got {other:?}"),
                };
                match second_assign.as_ref() {
                    AssignTarget::StructDestructure(fields) => {
                        assert_eq!(fields.len(), 1);
                        assert_eq!(fields[0].name, Some("first".to_string()));
                    }
                    other => panic!("unexpected second destructuring target: {other:?}"),
                }
            }
            _ => panic!("Expected Function main declaration"),
        }
    }

    #[test]
    fn test1_hll_compiles_to_ir_with_arithmetic() {
        let path = fixture_root().join("lexer/01_comments_and_newlines.hll");
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read fixture {path:?}: {err}"));
        let source: &'static str = Box::leak(source.into_boxed_str());

        let tokens = lex_source(source);
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().expect("failed to parse test1.hll");

        let mut compiler = HighLevelCompiler::new();
        let ir_program = compiler
            .compile_program(&program)
            .expect("failed to compile test1.hll");

        let ir_text = format!("{ir_program}");
        println!(
            "=== test1.hll IR OUTPUT ===\n{}",
            if ir_text.is_empty() {
                "(empty - only declarations)"
            } else {
                &ir_text
            }
        );
    }

    #[test]
    fn test2_hll_compiles_to_ir_with_pointers_and_structs() {
        let path = fixture_root().join("parser/02_structs_and_pointers.hll");
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read fixture {path:?}: {err}"));
        let source: &'static str = Box::leak(source.into_boxed_str());

        let tokens = lex_source(source);
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().expect("failed to parse test2.hll");

        let mut compiler = HighLevelCompiler::new();
        let ir_program = compiler
            .compile_program(&program)
            .expect("failed to compile test2.hll");

        let ir_text = format!("{ir_program}");
        assert!(
            ir_text.contains("define i32 main("),
            "IR should contain main function"
        );
        assert!(
            ir_text.contains("*") || ir_text.contains("Node"),
            "IR should contain pointer types or struct references"
        );
        println!("=== test2.hll IR OUTPUT ===\n{ir_text}");
    }

    #[test]
    fn test3_hll_compiles_to_ir_with_control_flow() {
        let path = fixture_root().join("parser/03_nested_access_and_control_flow.hll");
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read fixture {path:?}: {err}"));
        let source: &'static str = Box::leak(source.into_boxed_str());

        let tokens = lex_source(source);
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().expect("failed to parse test3.hll");

        let mut compiler = HighLevelCompiler::new();
        let ir_program = compiler
            .compile_program(&program)
            .expect("failed to compile test3.hll");

        let ir_text = format!("{ir_program}");
        assert!(
            ir_text.contains("define i32 stress_test("),
            "IR should contain stress_test function"
        );
        assert!(
            ir_text.contains("jump") || ir_text.contains("branch"),
            "IR should contain control flow instructions"
        );
        println!("=== test3.hll IR OUTPUT ===\n{ir_text}");
    }

    #[test]
    fn test4_hll_compiles_to_ir_with_multiple_returns() {
        let path = fixture_root().join("parser/04_tuple_returns.hll");
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read fixture {path:?}: {err}"));
        let source: &'static str = Box::leak(source.into_boxed_str());

        let tokens = lex_source(source);
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().expect("failed to parse test4.hll");

        let mut compiler = HighLevelCompiler::new();
        let ir_program = compiler
            .compile_program(&program)
            .expect("failed to compile test4.hll");

        let ir_text = format!("{ir_program}");
        assert!(
            ir_text.contains(" divide("),
            "IR should contain divide function"
        );
        assert!(
            ir_text.contains("define void start("),
            "IR should contain start function"
        );
        println!("=== test4.hll IR OUTPUT ===\n{ir_text}");
    }

    #[test]
    fn array_index_is_a_value_place_and_addressable() {
        let output = compile_hll(
            r#"
main: () -> i32 {
    arr: i32[2] = [4, 5]
    arr[1] = 7
    first: i32* = &arr[0]
    return arr[0] + arr[1] + @first
}
"#,
        )
        .expect("indexed places should compile");

        let ir = format!("{}", output.ir);
        assert!(
            ir.contains("index i32"),
            "expected indexed address IR: {ir}"
        );
        assert!(
            ir.contains("read i32"),
            "expected indexed value load IR: {ir}"
        );
        assert!(
            ir.contains("write i32"),
            "expected indexed assignment IR: {ir}"
        );
    }

    #[test]
    fn pointer_member_access_loads_the_field_value() {
        compile_hll(
            r#"
struct Point {
    x: i32
    y: i32
}

main: () -> i32 {
    point: Point* = new(Point)
    point.x = 9
    return point.x
}
"#,
        )
        .expect("pointer member auto-deref should compile");
    }

    #[test]
    fn address_of_rejects_non_place_temporaries() {
        let result = compile_hll(
            r#"
main: () -> i32 {
    ptr: i32* = &(1 + 2)
    return 0
}
"#,
        );
        let errors = match result {
            Ok(_) => panic!("taking the address of a temporary must fail"),
            Err(errors) => errors,
        };

        assert!(
            errors
                .iter()
                .any(|d| d.message.contains("address-of requires a place")),
            "unexpected diagnostics: {errors:?}"
        );
    }

    #[test]
    fn inferred_bindings_lower_with_concrete_types() {
        let output = compile_hll(
            r#"
noop: () {
    return
}

main: () -> i32 {
    number := 40
    values := [1, 2]
    pointer := &values[1]
    noop()
    return number + @pointer
}
"#,
        )
        .expect("inferred bindings should compile");

        let ir = format!("{}", output.ir);
        assert!(ir.contains("inferred local var: number"));
        assert!(ir.contains("inferred local var: values"));
        assert!(ir.contains("inferred local var: pointer"));
    }

    #[test]
    fn rejects_duplicate_inferred_binding_in_same_scope() {
        let result = compile_hll(
            r#"
main: () -> i32 {
    value := 1
    value := 2
    return value
}
"#,
        );
        let errors = match result {
            Ok(_) => panic!("duplicate inferred declarations must fail"),
            Err(errors) => errors,
        };

        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("duplicate inferred binding `value`"))
        );
    }

    #[test]
    fn infers_top_level_binding_type() {
        compile_hll(
            r#"
answer := 42

main: () -> i32 {
    return answer
}
"#,
        )
        .expect("top-level inferred bindings should compile");
    }

    #[test]
    fn infers_pointer_named_struct_array_and_generic_types() {
        let output = compile_hll(
            r#"
struct Point { x: i32 }
struct Box<T> { value: T }

main: () -> i32 {
    point_ptr := new(Point)
    point := @point_ptr
    values := [1, 2, 3]
    box_ptr := new(Box<i32>)
    point.x = values[0]
    return point.x
}
"#,
        )
        .expect("inference should preserve aggregate and generic types");

        let ir = output.ir.to_string();
        assert!(ir.contains("type Box<i32>"), "missing specialization: {ir}");
    }

    #[test]
    fn assignment_cannot_create_a_binding() {
        let result = compile_hll(
            r#"
main: () -> i32 {
    typo = 1
    return 0
}
"#,
        );
        let errors = match result {
            Ok(_) => panic!("assignment to an undeclared name must fail"),
            Err(errors) => errors,
        };
        assert!(errors.iter().any(|error| {
            error
                .message
                .contains("assignment target `typo` is not declared")
        }));
    }

    #[test]
    fn explicit_binding_requires_initializer() {
        let result = compile_hll(
            r#"
main: () -> i32 {
    value: i32
    return 0
}
"#,
        );
        let errors = match result {
            Ok(_) => panic!("uninitialized binding must fail"),
            Err(errors) => errors,
        };
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("require an initializer"))
        );
    }

    #[test]
    fn type_is_restricted_to_aliases() {
        let result = compile_hll(
            r#"
type Point = { x: i32 }
main: () -> i32 { return 0 }
"#,
        );
        let errors = match result {
            Ok(_) => panic!("record declarations must use `struct`"),
            Err(errors) => errors,
        };
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("`type` is only for aliases"))
        );
    }

    #[test]
    fn named_and_contextual_struct_literals_compile() {
        compile_hll(
            r#"
struct Point {
    x: i32
    y: i32
}

main: () -> i32 {
    named := Point { .y = 2, .x = 1 }
    contextual: Point = { .x = 3, .y = 4 }
    return named.x + named.y + contextual.x + contextual.y
}
"#,
        )
        .expect("canonical struct literals should compile");
    }

    #[test]
    fn named_struct_literal_reports_field_errors() {
        for (literal, expected) in [
            ("Point { .x = 1, .x = 2 }", "duplicate field `x`"),
            ("Point { .x = 1 }", "missing field `y`"),
            ("Point { .x = 1, .y = 2, .z = 3 }", "unknown field `z`"),
        ] {
            let source = format!(
                "struct Point {{ x: i32, y: i32 }}\nmain: () -> i32 {{ value := {literal}\nreturn 0\n}}"
            );
            let errors = match compile_hll(&source) {
                Ok(_) => panic!("invalid literal unexpectedly compiled: {literal}"),
                Err(errors) => errors,
            };
            assert!(
                errors.iter().any(|error| error.message.contains(expected)),
                "expected `{expected}` for `{literal}`, got {errors:?}"
            );
        }
    }

    #[test]
    fn contextual_struct_literal_zero_fills_missing_fields() {
        let output = compile_hll(
            r#"
struct Point { x: i32, y: i32 }

main: () -> i32 {
    point: Point = { .x = 7 }
    point = { .y = 9 }
    return point.x + point.y
}
"#,
        )
        .expect("contextual literals should default omitted fields to zero");
        let ir = output.ir.to_string();
        assert!(ir.contains("write i32 0"), "missing zero fill in IR: {ir}");
    }

    #[test]
    fn contextual_struct_literal_validates_fields() {
        for (literal, expected) in [
            ("{ .x = 1, .x = 2 }", "duplicate field `x`"),
            ("{ .z = 1 }", "unknown field `z`"),
            ("{ .x = true }", "field `x` expects `i32`"),
        ] {
            let source = format!(
                "struct Point {{ x: i32, y: i32 }}\nmain: () -> i32 {{ value: Point = {literal}\nreturn 0\n}}"
            );
            let errors = match compile_hll(&source) {
                Ok(_) => panic!("invalid contextual literal compiled: {literal}"),
                Err(errors) => errors,
            };
            assert!(
                errors.iter().any(|error| error.message.contains(expected)),
                "expected `{expected}`, got {errors:?}"
            );
        }
    }

    #[test]
    fn propagates_struct_context_through_arguments_and_returns() {
        compile_hll(
            r#"
struct Point { x: i32, y: i32 }

make_point: () -> Point {
    return { .x = 5 }
}

sum_point: (point: Point) -> i32 {
    return point.x + point.y
}

main: () -> i32 {
    return sum_point({ .y = 7 }) + sum_point(make_point())
}
"#,
        )
        .expect("argument and return types should contextualize anonymous literals");
    }

    #[test]
    fn rejects_anonymous_literal_without_context() {
        let errors = match compile_hll(
            r#"
main: () -> i32 {
    value := { .x = 1 }
    return 0
}
"#,
        ) {
            Ok(_) => panic!("anonymous literal cannot infer its own nominal type"),
            Err(errors) => errors,
        };
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("cannot infer `value`"))
        );
    }

    #[test]
    fn monomorphizes_explicit_generic_function_calls() {
        let output = compile_hll(
            r#"
identity: <T>(value: T) -> T {
    return value
}

main: () -> i32 {
    return identity<i32>(42)
}
"#,
        )
        .expect("explicit generic call should compile");
        let ir = output.ir.to_string();
        assert!(ir.contains("identity__i32"), "missing specialization: {ir}");
        assert!(
            !ir.contains("identity<T>"),
            "generic template leaked to IR: {ir}"
        );
    }

    #[test]
    fn lowers_function_pointer_assignment_and_call() {
        let output = compile_hll(
            r#"
type BinaryOp = fn(i32, i32) -> i32

add: (a: i32, b: i32) -> i32 {
    return a + b
}

main: () -> i32 {
    op: BinaryOp = add
    return op(2, 3)
}
"#,
        )
        .expect("function pointer assignment and indirect call should compile");
        let ir = output.ir.to_string();
        assert!(
            ir.contains("function_addr add"),
            "missing function address: {ir}"
        );
        assert!(ir.contains("indirect_call"), "missing indirect call: {ir}");
    }

    #[test]
    fn rejects_wrong_generic_function_arity() {
        let errors = match compile_hll(
            r#"
identity: <T>(value: T) -> T {
    return value
}

main: () -> i32 {
    return identity<i32, i64>(42)
}
"#,
        ) {
            Ok(_) => panic!("wrong generic arity must fail"),
            Err(errors) => errors,
        };
        assert!(
            errors
                .iter()
                .any(|error| { error.message.contains("expects 1 type arguments, got 2") })
        );
    }
}
