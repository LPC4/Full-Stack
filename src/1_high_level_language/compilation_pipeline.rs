use crate::high_level_language::ast::Program;
use crate::high_level_language::compiler::{
    CompilerError, Diagnostic, HighLevelCompiler, SemanticAnalyzer,
};
use crate::high_level_language::lexer::Lexer;
use crate::high_level_language::parser::{Parser, ParserError};
use crate::high_level_language::token::Token;
use crate::intermediate_language::IrProgram;

#[derive(Debug, Clone)]
pub enum CompilationError {
    LexerError(String),
    ParseError(ParserError),
    CompilerError(CompilerError),
    SemanticErrors(Vec<String>),
}

impl std::fmt::Display for CompilationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompilationError::LexerError(msg) => write!(f, "Lexer error: {}", msg),
            CompilationError::ParseError(err) => {
                write!(f, "Parse error at pos {}: {}", err.pos, err.message)
            }
            CompilationError::CompilerError(err) => write!(f, "Compiler error: {:?}", err),
            CompilationError::SemanticErrors(errors) => {
                writeln!(f, "Semantic errors:")?;
                for error in errors {
                    writeln!(f, "  - {}", error)?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for CompilationError {}

#[derive(Debug)]
pub struct CompilationResult {
    pub ast: Program,
    pub ir_program: IrProgram,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct CompilationPipeline {
    pub run_semantic_analysis: bool,
    pub strict_semantics: bool,
}

impl Default for CompilationPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl CompilationPipeline {
    pub fn new() -> Self {
        Self {
            run_semantic_analysis: true,
            strict_semantics: false,
        }
    }

    /// source → tokens → AST → IR
    pub fn compile(&self, source: &str) -> Result<CompilationResult, CompilationError> {
        log::info!("Starting compilation pipeline");

        // Phase 1: Lexing
        log::info!("Phase 1: Lexing source code");
        let tokens = self.lex_internal(source)?;
        log::info!("Lexed {} tokens", tokens.len());

        // Phase 2: Parsing
        log::info!("Phase 2: Parsing tokens to AST");
        let ast = self.parse(tokens)?;
        log::info!(
            "Parsed program with {} declarations",
            ast.declarations.len()
        );

        // Phase 3: Semantic Analysis
        if self.run_semantic_analysis {
            log::info!("Phase 3: Running semantic analysis");
            self.semantic_analysis(&ast)?;
        }

        // Phase 4: Compilation to IR
        log::info!("Phase 4: Compiling to intermediate representation");
        let (ir_program, diagnostics) = self.compile_to_ir(&ast)?;
        log::info!("Compilation complete");

        Ok(CompilationResult {
            ast,
            ir_program,
            diagnostics,
        })
    }

    fn lex_internal<'a>(&self, source: &'a str) -> Result<Vec<Token<'a>>, CompilationError> {
        let mut lexer = Lexer::new(source);
        let mut tokens = Vec::new();

        loop {
            let token = lexer.next_token();
            if let Token::Error(ref msg) = token {
                return Err(CompilationError::LexerError(msg.clone()));
            }
            let is_eof = matches!(token, Token::Eof);
            tokens.push(token);
            if is_eof {
                break;
            }
        }

        Ok(tokens)
    }

    pub fn parse(&self, tokens: Vec<Token<'_>>) -> Result<Program, CompilationError> {
        let mut parser = Parser::new(tokens);
        parser.parse_program().map_err(CompilationError::ParseError)
    }

    pub fn semantic_analysis(&self, ast: &Program) -> Result<(), CompilationError> {
        let mut semantic_analyzer = SemanticAnalyzer::new();

        match semantic_analyzer.analyze_program(ast) {
            Ok(_) => {
                // Check if there are any errors in diagnostics
                let errors: Vec<_> = semantic_analyzer
                    .diagnostics()
                    .iter()
                    .filter(|d| {
                        matches!(
                            d.level,
                            crate::high_level_language::compiler::DiagnosticLevel::Error
                        )
                    })
                    .map(|d| d.message.clone())
                    .collect();

                if !errors.is_empty() {
                    return Err(CompilationError::SemanticErrors(errors));
                }
                Ok(())
            }
            Err(_) => {
                // Semantic analysis failed completely
                let errors: Vec<_> = semantic_analyzer
                    .diagnostics()
                    .iter()
                    .map(|d| d.message.clone())
                    .collect();
                Err(CompilationError::SemanticErrors(errors))
            }
        }
    }

    pub fn compile_to_ir(
        &self,
        ast: &Program,
    ) -> Result<(IrProgram, Vec<Diagnostic>), CompilationError> {
        let mut compiler = HighLevelCompiler::new();
        let ir_program = compiler
            .compile_program(ast)
            .map_err(CompilationError::CompilerError)?;

        let diagnostics = compiler.diagnostics().to_vec();

        // Check for semantic errors in compiler diagnostics
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| {
                matches!(
                    d.level,
                    crate::high_level_language::compiler::DiagnosticLevel::Error
                )
            })
            .map(|d| d.message.clone())
            .collect();

        if !errors.is_empty() {
            return Err(CompilationError::SemanticErrors(errors));
        }

        Ok((ir_program, diagnostics))
    }

    /// Compile and return only the IR program (convenience method)
    pub fn compile_to_ir_only(&self, source: &str) -> Result<IrProgram, CompilationError> {
        let result = self.compile(source)?;
        Ok(result.ir_program)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_compiles_valid_program() {
        let mut pipeline = CompilationPipeline::new();
        // Disable semantic analysis to isolate parsing/compilation issues
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
            CompilationError::LexerError(_)
        ));
    }
}
