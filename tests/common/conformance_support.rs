//! Compiler conformance manifest and shared VM runner.

use asm_to_binary::AssembledOutput;
use full_stack::compilation_pipeline::CompilationPipeline;
use hll_to_ir::stdlib::{
    get_stdlib_modules_for_mode, get_stdlib_type_prelude, stdlib_module_source_path,
};
use hll_to_ir::{OptOptions, TargetMode};
use std::fmt::Write as _;
use std::sync::OnceLock;
use virtual_machine::bus::ELF_LOAD_BASE;
use virtual_machine::virtual_machine::{StepOutcome, VirtualMachine};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Feature {
    LexicalConventions,
    TypesAndBindings,
    StringsAndChars,
    Pointers,
    Arrays,
    StructsAndDestructuring,
    SlicesRangesAndBounds,
    StringInterpolation,
    Functions,
    Methods,
    Generics,
    ModulesAndImports,
    InterfacesAndBounds,
    DynamicInterfaces,
    ControlFlow,
    EnumsMatchQuestion,
    Defer,
    TrapBehavior,
    InlineAssembly,
    FormalGrammar,
    Operators,
    MemorySafety,
    Stdlib,
    Hll0Subset,
}

impl Feature {
    pub const ALL: [Self; 24] = [
        Self::LexicalConventions,
        Self::TypesAndBindings,
        Self::StringsAndChars,
        Self::Pointers,
        Self::Arrays,
        Self::StructsAndDestructuring,
        Self::SlicesRangesAndBounds,
        Self::StringInterpolation,
        Self::Functions,
        Self::Methods,
        Self::Generics,
        Self::ModulesAndImports,
        Self::InterfacesAndBounds,
        Self::DynamicInterfaces,
        Self::ControlFlow,
        Self::EnumsMatchQuestion,
        Self::Defer,
        Self::TrapBehavior,
        Self::InlineAssembly,
        Self::FormalGrammar,
        Self::Operators,
        Self::MemorySafety,
        Self::Stdlib,
        Self::Hll0Subset,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::LexicalConventions => "Lexical conventions",
            Self::TypesAndBindings => "Types and bindings",
            Self::StringsAndChars => "Strings and characters",
            Self::Pointers => "Pointers",
            Self::Arrays => "Arrays",
            Self::StructsAndDestructuring => "Structs and destructuring",
            Self::SlicesRangesAndBounds => "Slices, ranges, bounds",
            Self::StringInterpolation => "String interpolation",
            Self::Functions => "Functions",
            Self::Methods => "Methods (impl)",
            Self::Generics => "Generics",
            Self::ModulesAndImports => "Modules and imports",
            Self::InterfacesAndBounds => "Interfaces and bounds",
            Self::DynamicInterfaces => "Dynamic interfaces",
            Self::ControlFlow => "Control flow",
            Self::EnumsMatchQuestion => "Enums, match, ?",
            Self::Defer => "defer",
            Self::TrapBehavior => "Trap behavior",
            Self::InlineAssembly => "Inline assembly",
            Self::FormalGrammar => "Formal grammar",
            Self::Operators => "Operators",
            Self::MemorySafety => "Memory safety",
            Self::Stdlib => "Standard library",
            Self::Hll0Subset => "HLL-0 subset",
        }
    }

    pub const fn spec(self) -> &'static str {
        match self {
            Self::LexicalConventions => "2",
            Self::TypesAndBindings => "3.1-3.4",
            Self::StringsAndChars => "3.5",
            Self::Pointers => "4",
            Self::Arrays => "5.1",
            Self::StructsAndDestructuring => "5.2, 5.4",
            Self::SlicesRangesAndBounds => "5.3",
            Self::StringInterpolation => "5.5",
            Self::Functions => "6.1",
            Self::Methods => "6.2",
            Self::Generics => "6.3",
            Self::ModulesAndImports => "6.4",
            Self::InterfacesAndBounds => "6.5",
            Self::DynamicInterfaces => "6.6",
            Self::ControlFlow => "7.1",
            Self::EnumsMatchQuestion => "7.2",
            Self::Defer => "7.3",
            Self::TrapBehavior => "7.4",
            Self::InlineAssembly => "8",
            Self::FormalGrammar => "9",
            Self::Operators => "9.3-9.4",
            Self::MemorySafety => "10",
            Self::Stdlib => "11",
            Self::Hll0Subset => "Appendix A",
        }
    }
}

pub struct ConformanceCase {
    pub id: &'static str,
    pub source_path: &'static str,
    pub source: &'static str,
    pub expected_exit: i32,
    pub expected_stdout: &'static str,
    pub step_limit: u64,
    pub features: &'static [Feature],
}

pub struct CompilerVariant {
    pub name: &'static str,
    pub peephole: bool,
    pub regalloc: bool,
    pub omit_frame_pointer: bool,
    pub ir_opt: OptOptions,
}

pub const BASELINE_VARIANT: usize = 0;
pub const PRODUCTION_VARIANT: usize = 1;

pub const VARIANTS: &[CompilerVariant] = &[
    CompilerVariant {
        name: "baseline",
        peephole: false,
        regalloc: false,
        omit_frame_pointer: false,
        ir_opt: OptOptions {
            const_fold: false,
            dce: false,
            phi_simplify: false,
        },
    },
    CompilerVariant {
        name: "production",
        peephole: true,
        regalloc: true,
        omit_frame_pointer: true,
        ir_opt: OptOptions {
            const_fold: false,
            dce: false,
            phi_simplify: false,
        },
    },
    CompilerVariant {
        name: "opt_const_fold",
        peephole: false,
        regalloc: false,
        omit_frame_pointer: false,
        ir_opt: OptOptions {
            const_fold: true,
            dce: false,
            phi_simplify: false,
        },
    },
    CompilerVariant {
        name: "opt_dce",
        peephole: false,
        regalloc: false,
        omit_frame_pointer: false,
        ir_opt: OptOptions {
            const_fold: false,
            dce: true,
            phi_simplify: false,
        },
    },
    CompilerVariant {
        name: "opt_phi_simplify",
        peephole: false,
        regalloc: false,
        omit_frame_pointer: false,
        ir_opt: OptOptions {
            const_fold: false,
            dce: false,
            phi_simplify: true,
        },
    },
    CompilerVariant {
        name: "register_allocation",
        peephole: false,
        regalloc: true,
        omit_frame_pointer: false,
        ir_opt: OptOptions {
            const_fold: false,
            dce: false,
            phi_simplify: false,
        },
    },
    CompilerVariant {
        name: "peephole",
        peephole: true,
        regalloc: false,
        omit_frame_pointer: false,
        ir_opt: OptOptions {
            const_fold: false,
            dce: false,
            phi_simplify: false,
        },
    },
    CompilerVariant {
        name: "omit_frame_pointer",
        peephole: false,
        regalloc: false,
        omit_frame_pointer: true,
        ir_opt: OptOptions {
            const_fold: false,
            dce: false,
            phi_simplify: false,
        },
    },
    CompilerVariant {
        name: "all_on",
        peephole: true,
        regalloc: true,
        omit_frame_pointer: true,
        ir_opt: OptOptions {
            const_fold: true,
            dce: true,
            phi_simplify: true,
        },
    },
];

pub const CASES: &[ConformanceCase] = &[
    ConformanceCase {
        id: "qemu_01_arithmetic_and_types",
        source_path: "programs/test/qemu/01_arithmetic_and_types.hll",
        source: include_str!("../../programs/test/qemu/01_arithmetic_and_types.hll"),
        expected_exit: 42,
        expected_stdout: "",
        step_limit: 50_000_000,
        features: &[Feature::TypesAndBindings, Feature::Operators],
    },
    ConformanceCase {
        id: "qemu_02_control_flow",
        source_path: "programs/test/qemu/02_control_flow.hll",
        source: include_str!("../../programs/test/qemu/02_control_flow.hll"),
        expected_exit: 100,
        expected_stdout: "",
        step_limit: 50_000_000,
        features: &[Feature::ControlFlow],
    },
    ConformanceCase {
        id: "qemu_03_structs_and_destructuring",
        source_path: "programs/test/qemu/03_structs_and_destructuring.hll",
        source: include_str!("../../programs/test/qemu/03_structs_and_destructuring.hll"),
        expected_exit: 0,
        expected_stdout: "",
        step_limit: 50_000_000,
        features: &[Feature::StructsAndDestructuring, Feature::Functions],
    },
    ConformanceCase {
        id: "qemu_04_pointers_and_memory",
        source_path: "programs/test/qemu/04_pointers_and_memory.hll",
        source: include_str!("../../programs/test/qemu/04_pointers_and_memory.hll"),
        expected_exit: 0,
        expected_stdout: "",
        step_limit: 50_000_000,
        features: &[
            Feature::Pointers,
            Feature::Arrays,
            Feature::Defer,
            Feature::Functions,
            Feature::MemorySafety,
        ],
    },
    ConformanceCase {
        id: "qemu_05_functions_and_io",
        source_path: "programs/test/qemu/05_functions_and_io.hll",
        source: include_str!("../../programs/test/qemu/05_functions_and_io.hll"),
        expected_exit: 0,
        expected_stdout: "PASS\n",
        step_limit: 50_000_000,
        features: &[
            Feature::Functions,
            Feature::ModulesAndImports,
            Feature::Stdlib,
        ],
    },
    ConformanceCase {
        id: "slices_ranges_bounds",
        source_path: "programs/test/compiler_suite/conformance/16_slices_ranges_bounds.hll",
        source: include_str!(
            "../../programs/test/compiler_suite/conformance/16_slices_ranges_bounds.hll"
        ),
        expected_exit: 134,
        expected_stdout: "",
        step_limit: 20_000_000,
        features: &[
            Feature::SlicesRangesAndBounds,
            Feature::Arrays,
            Feature::TrapBehavior,
            Feature::TypesAndBindings,
            Feature::MemorySafety,
        ],
    },
    ConformanceCase {
        id: "generic_enums_match_question",
        source_path: "programs/test/compiler_suite/conformance/17_generic_enums_match_question.hll",
        source: include_str!(
            "../../programs/test/compiler_suite/conformance/17_generic_enums_match_question.hll"
        ),
        expected_exit: 42,
        expected_stdout: "",
        step_limit: 20_000_000,
        features: &[
            Feature::EnumsMatchQuestion,
            Feature::Generics,
            Feature::Stdlib,
            Feature::Functions,
        ],
    },
    ConformanceCase {
        id: "methods_bounds_dynamic_interfaces",
        source_path: "programs/test/compiler_suite/conformance/18_methods_bounds_dynamic_interfaces.hll",
        source: include_str!(
            "../../programs/test/compiler_suite/conformance/18_methods_bounds_dynamic_interfaces.hll"
        ),
        expected_exit: 42,
        expected_stdout: "",
        step_limit: 20_000_000,
        features: &[
            Feature::Methods,
            Feature::InterfacesAndBounds,
            Feature::DynamicInterfaces,
            Feature::Generics,
            Feature::StructsAndDestructuring,
            Feature::Functions,
        ],
    },
    ConformanceCase {
        id: "imports_strings_stdout_trap",
        source_path: "programs/test/compiler_suite/conformance/19_imports_strings_stdout_trap.hll",
        source: include_str!(
            "../../programs/test/compiler_suite/conformance/19_imports_strings_stdout_trap.hll"
        ),
        expected_exit: 101,
        expected_stdout: "hello\n",
        step_limit: 20_000_000,
        features: &[
            Feature::ModulesAndImports,
            Feature::StringsAndChars,
            Feature::Stdlib,
            Feature::TrapBehavior,
        ],
    },
    ConformanceCase {
        id: "lexical_grammar",
        source_path: "programs/test/compiler_suite/conformance/20_lexical_grammar.hll",
        source: include_str!(
            "../../programs/test/compiler_suite/conformance/20_lexical_grammar.hll"
        ),
        expected_exit: 42,
        expected_stdout: "",
        step_limit: 20_000_000,
        features: &[Feature::LexicalConventions, Feature::FormalGrammar],
    },
    ConformanceCase {
        id: "string_interpolation",
        source_path: "programs/test/compiler_suite/conformance/21_string_interpolation.hll",
        source: include_str!(
            "../../programs/test/compiler_suite/conformance/21_string_interpolation.hll"
        ),
        expected_exit: 42,
        expected_stdout: "value=42\n",
        step_limit: 20_000_000,
        features: &[Feature::StringInterpolation],
    },
    ConformanceCase {
        id: "inline_assembly",
        source_path: "programs/test/compiler_suite/conformance/22_inline_assembly.hll",
        source: include_str!(
            "../../programs/test/compiler_suite/conformance/22_inline_assembly.hll"
        ),
        expected_exit: 42,
        expected_stdout: "",
        step_limit: 20_000_000,
        features: &[Feature::InlineAssembly],
    },
    ConformanceCase {
        id: "hll0_subset",
        source_path: "programs/test/compiler_suite/conformance/23_hll0_subset.hll",
        source: include_str!("../../programs/test/compiler_suite/conformance/23_hll0_subset.hll"),
        expected_exit: 42,
        expected_stdout: "",
        step_limit: 20_000_000,
        features: &[Feature::Hll0Subset],
    },
];

pub struct FeatureCoverage {
    pub feature: Feature,
    pub golden: &'static [&'static str],
}

pub const FEATURE_COVERAGE: &[FeatureCoverage] = &[
    FeatureCoverage {
        feature: Feature::LexicalConventions,
        golden: &["compiler_suite/conformance/20_lexical_grammar"],
    },
    FeatureCoverage {
        feature: Feature::TypesAndBindings,
        golden: &[
            "compiler_suite/arithmetic/01_basic_arithmetic",
            "compiler_suite/types/15_signed_unsigned_casts",
        ],
    },
    FeatureCoverage {
        feature: Feature::StringsAndChars,
        golden: &["compiler_suite/types/09_string_literals"],
    },
    FeatureCoverage {
        feature: Feature::Pointers,
        golden: &[
            "compiler_suite/pointers/03_basic_pointers",
            "compiler_suite/pointers/08_chained_deref",
        ],
    },
    FeatureCoverage {
        feature: Feature::Arrays,
        golden: &["compiler_suite/arithmetic/01_basic_arithmetic"],
    },
    FeatureCoverage {
        feature: Feature::StructsAndDestructuring,
        golden: &[
            "compiler_suite/types/04_struct_types",
            "compiler_suite/types/06_tuple_destructuring",
        ],
    },
    FeatureCoverage {
        feature: Feature::SlicesRangesAndBounds,
        golden: &["compiler_suite/conformance/16_slices_ranges_bounds"],
    },
    FeatureCoverage {
        feature: Feature::StringInterpolation,
        golden: &["compiler_suite/conformance/21_string_interpolation"],
    },
    FeatureCoverage {
        feature: Feature::Functions,
        golden: &[
            "compiler_suite/functions/07_simple_assign",
            "compiler_suite/functions/11_constexpr_pure_functions",
            "compiler_suite/functions/12_constexpr_while_loops",
        ],
    },
    FeatureCoverage {
        feature: Feature::Methods,
        golden: &["compiler_suite/conformance/18_methods_bounds_dynamic_interfaces"],
    },
    FeatureCoverage {
        feature: Feature::Generics,
        golden: &["compiler_suite/types/10_generic_naming_collision_free"],
    },
    FeatureCoverage {
        feature: Feature::ModulesAndImports,
        golden: &[
            "compiler_suite/conformance/19_imports_strings_stdout_trap",
            "compiler_suite/conformance/21_string_interpolation",
        ],
    },
    FeatureCoverage {
        feature: Feature::InterfacesAndBounds,
        golden: &["compiler_suite/conformance/18_methods_bounds_dynamic_interfaces"],
    },
    FeatureCoverage {
        feature: Feature::DynamicInterfaces,
        golden: &["compiler_suite/conformance/18_methods_bounds_dynamic_interfaces"],
    },
    FeatureCoverage {
        feature: Feature::ControlFlow,
        golden: &[
            "compiler_suite/control_flow/02_conditional_and_loop",
            "compiler_suite/control_flow/05_constants_and_loops",
        ],
    },
    FeatureCoverage {
        feature: Feature::EnumsMatchQuestion,
        golden: &["compiler_suite/conformance/17_generic_enums_match_question"],
    },
    FeatureCoverage {
        feature: Feature::Defer,
        golden: &["compiler_suite/pointers/03_basic_pointers"],
    },
    FeatureCoverage {
        feature: Feature::TrapBehavior,
        golden: &[
            "compiler_suite/conformance/16_slices_ranges_bounds",
            "compiler_suite/conformance/19_imports_strings_stdout_trap",
        ],
    },
    FeatureCoverage {
        feature: Feature::InlineAssembly,
        golden: &["compiler_suite/conformance/22_inline_assembly"],
    },
    FeatureCoverage {
        feature: Feature::FormalGrammar,
        golden: &["compiler_suite/conformance/20_lexical_grammar"],
    },
    FeatureCoverage {
        feature: Feature::Operators,
        golden: &["compiler_suite/arithmetic/01_basic_arithmetic"],
    },
    FeatureCoverage {
        feature: Feature::MemorySafety,
        golden: &[
            "compiler_suite/pointers/03_basic_pointers",
            "compiler_suite/conformance/16_slices_ranges_bounds",
        ],
    },
    FeatureCoverage {
        feature: Feature::Stdlib,
        golden: &[
            "compiler_suite/conformance/19_imports_strings_stdout_trap",
            "compiler_suite/conformance/21_string_interpolation",
        ],
    },
    FeatureCoverage {
        feature: Feature::Hll0Subset,
        golden: &["compiler_suite/conformance/23_hll0_subset"],
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmOutcome {
    pub halted: Option<i64>,
    pub uart: String,
}

pub fn case_by_id(id: &str) -> &'static ConformanceCase {
    CASES
        .iter()
        .find(|case| case.id == id)
        .unwrap_or_else(|| panic!("unknown conformance case {id}"))
}

pub fn pipeline_for_variant(idx: usize) -> CompilationPipeline {
    let variant = &VARIANTS[idx];
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    pipeline.set_peephole(variant.peephole);
    pipeline.set_register_allocation(variant.regalloc);
    pipeline.set_omit_frame_pointer(variant.omit_frame_pointer);
    pipeline.set_optimize(variant.ir_opt);
    pipeline
}

static STDLIB_BY_VARIANT: [OnceLock<Vec<(String, AssembledOutput)>>; VARIANTS.len()] =
    [const { OnceLock::new() }; VARIANTS.len()];

pub fn stdlib_for_variant(idx: usize) -> &'static [(String, AssembledOutput)] {
    STDLIB_BY_VARIANT[idx].get_or_init(|| {
        let mut pipeline = pipeline_for_variant(idx);
        pipeline.set_target_mode(TargetMode::Hosted);
        pipeline.set_type_prelude(get_stdlib_type_prelude());
        let modules = get_stdlib_modules_for_mode(TargetMode::Hosted);
        let mut objects = Vec::new();
        for (name, source) in &modules {
            pipeline.set_current_source_path(stdlib_module_source_path(TargetMode::Hosted, name));
            objects.extend(
                pipeline
                    .compile_modules(&[(*name, *source)])
                    .unwrap_or_else(|e| {
                        panic!(
                            "stdlib compile failed for variant {}: {e}",
                            VARIANTS[idx].name
                        )
                    }),
            );
        }
        for object in &mut objects {
            mark_legacy_exports_global(object);
        }
        modules
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .zip(objects)
            .collect()
    })
}

fn mark_legacy_exports_global(object: &mut AssembledOutput) {
    let symbols: Vec<String> = object
        .symbols_iter()
        .filter(|(name, _)| !name.starts_with('.') && !name.contains("__"))
        .map(|(name, _)| name.to_owned())
        .collect();
    for symbol in symbols {
        object.mark_entry_global(&symbol);
    }
}

pub fn run_case_vm(case: &ConformanceCase, variant_idx: usize) -> Result<VmOutcome, String> {
    run_source_vm(case.source, case.step_limit, variant_idx)
}

pub fn run_source_vm(
    source: &str,
    step_limit: u64,
    variant_idx: usize,
) -> Result<VmOutcome, String> {
    let pipeline = pipeline_for_variant(variant_idx);
    let compiled = pipeline
        .compile(source)
        .map_err(|e| format!("compile failed: {e}"))?;
    let (_, tokens) = pipeline
        .compile_ir_to_assembly_with_tokens(&compiled.ir_program)
        .map_err(|e| format!("codegen failed: {e}"))?;
    let user = pipeline
        .assemble(&tokens)
        .map_err(|e| format!("assemble failed: {e}"))?;
    let mut modules: Vec<(&str, &AssembledOutput)> = stdlib_for_variant(variant_idx)
        .iter()
        .map(|(name, object)| (name.as_str(), object))
        .collect();
    modules.push(("user", &user));
    let linked = pipeline
        .link_assembled_objects(&modules)
        .map_err(|e| format!("link failed: {e}"))?;
    let run = VirtualMachine::new(&linked).run(step_limit);
    match run.outcome {
        StepOutcome::Halted(code) => Ok(VmOutcome {
            halted: Some(code),
            uart: run.uart_output,
        }),
        StepOutcome::Continue => Err(format!("timeout: did not halt within {} steps", step_limit)),
    }
}

pub fn canonical_ir(case: &ConformanceCase, variant_idx: usize) -> Result<String, String> {
    canonical_source_ir(case.source, variant_idx)
}

pub fn canonical_source_ir(source: &str, variant_idx: usize) -> Result<String, String> {
    let pipeline = pipeline_for_variant(variant_idx);
    let compiled = pipeline
        .compile(source)
        .map_err(|e| format!("compile failed: {e}"))?;
    Ok(format!("{}", compiled.ir_program))
}

pub fn compile_case_elf(case: &ConformanceCase, variant_idx: usize) -> Result<Vec<u8>, String> {
    compile_source_elf(case.source, variant_idx)
}

pub fn compile_source_elf(source: &str, variant_idx: usize) -> Result<Vec<u8>, String> {
    let pipeline = pipeline_for_variant(variant_idx);
    let compiled = pipeline
        .compile(source)
        .map_err(|e| format!("compile failed: {e}"))?;
    let (_, tokens) = pipeline
        .compile_ir_to_assembly_with_tokens(&compiled.ir_program)
        .map_err(|e| format!("codegen failed: {e}"))?;
    let user = pipeline
        .assemble(&tokens)
        .map_err(|e| format!("assemble failed: {e}"))?;
    let mut modules: Vec<(&str, &AssembledOutput)> = stdlib_for_variant(variant_idx)
        .iter()
        .map(|(name, object)| (name.as_str(), object))
        .collect();
    modules.push(("user", &user));
    let linked = pipeline
        .link_assembled_objects(&modules)
        .map_err(|e| format!("link failed: {e}"))?;
    Ok(linked.to_elf(ELF_LOAD_BASE))
}

pub const MATRIX_REL_PATH: &str = "tests/CONFORMANCE.md";
pub const MATRIX_UPDATE_VAR: &str = "UPDATE_CONFORMANCE_MATRIX";

pub fn should_update_matrix() -> bool {
    std::env::var_os(MATRIX_UPDATE_VAR).is_some()
}

pub fn render_matrix() -> String {
    let mut out = String::new();
    out.push_str(
        "Generated by:\n\
         \n\
         ```powershell\n\
         $env:UPDATE_CONFORMANCE_MATRIX = \"1\"\n\
         cargo test -p full_stack --test all conformance_matrix_matches_committed_file\n\
         ```\n",
    );

    out.push_str("\n| Feature | Spec | Golden | VM | QEMU |\n");
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for row in FEATURE_COVERAGE {
        let golden = row
            .golden
            .iter()
            .map(|path| path.strip_prefix("compiler_suite/").unwrap_or(path))
            .collect::<Vec<_>>()
            .join("<br>");
        let vm = evidence_ids(row.feature);
        writeln!(
            out,
            "| {} | {} | {} | {} | {} |",
            row.feature.name(),
            row.feature.spec(),
            golden,
            vm,
            vm
        )
        .expect("write feature row");
    }
    out
}

fn evidence_ids(feature: Feature) -> String {
    CASES
        .iter()
        .filter(|case| case.features.contains(&feature))
        .map(|case| case.id)
        .collect::<Vec<_>>()
        .join("<br>")
}
