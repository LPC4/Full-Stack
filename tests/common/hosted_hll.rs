use asm_to_binary::AssembledOutput;
use full_stack::compilation_pipeline::{CompilationPipeline, TargetMode};
use std::sync::OnceLock;
use virtual_machine::virtual_machine::{RunResult, VirtualMachine};

pub struct HostedOptions<'a> {
    pub max_steps: u64,
    pub run_semantic_analysis: bool,
    pub string_prefix: Option<&'a str>,
}

impl Default for HostedOptions<'_> {
    fn default() -> Self {
        Self {
            max_steps: 5_000_000,
            run_semantic_analysis: true,
            string_prefix: None,
        }
    }
}

fn stdlib_objects() -> &'static [(String, AssembledOutput)] {
    static STDLIB: OnceLock<Vec<(String, AssembledOutput)>> = OnceLock::new();
    STDLIB.get_or_init(|| {
        CompilationPipeline::compile_stdlib_objects(TargetMode::Hosted)
            .expect("stdlib compile failed")
    })
}

pub fn run(source: &str, options: HostedOptions<'_>) -> RunResult {
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_target_mode(TargetMode::Hosted);
    pipeline.set_run_semantic_analysis(options.run_semantic_analysis);
    pipeline.set_write_artifacts(false);
    if let Some(prefix) = options.string_prefix {
        pipeline.set_string_prefix(Some(prefix.to_owned()));
    }

    let user = pipeline.compile(source).expect("user compile failed");
    let (_, tokens) = pipeline
        .compile_ir_to_assembly_with_tokens(&user.ir_program)
        .expect("valid IR layout");
    let user = pipeline.assemble(&tokens).expect("user assemble failed");
    let mut modules: Vec<(&str, &AssembledOutput)> = stdlib_objects()
        .iter()
        .map(|(name, object)| (name.as_str(), object))
        .collect();
    modules.push(("user", &user));
    let linked = pipeline
        .link_assembled_objects(&modules)
        .expect("object link failed");
    VirtualMachine::new(&linked).run(options.max_steps)
}
