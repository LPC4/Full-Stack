// Executable compiler conformance across the VM and QEMU.

use crate::{conformance_support, qemu_support};
use conformance_support::{Feature, VARIANTS};
use std::collections::HashSet;
use std::path::PathBuf;

#[test]
fn conformance_vm_corpus_all_variants_match_baseline() {
    let mut failures = Vec::new();
    for case in conformance_support::CASES {
        let baseline = match conformance_support::run_case_vm(case, conformance_support::BASELINE_VARIANT)
        {
            Ok(outcome) => outcome,
            Err(e) => {
                failures.push(format!("{} baseline: {e}", case.id));
                continue;
            }
        };
        if baseline.halted != Some(case.expected_exit as i64) || baseline.uart != case.expected_stdout
        {
            failures.push(format!(
                "{} baseline: expected exit {} stdout {:?}, got {:?} stdout {:?}",
                case.id, case.expected_exit, case.expected_stdout, baseline.halted, baseline.uart
            ));
        }
        for (idx, variant) in VARIANTS.iter().enumerate() {
            if idx == conformance_support::BASELINE_VARIANT {
                continue;
            }
            match conformance_support::run_case_vm(case, idx) {
                Ok(outcome) => {
                    if outcome != baseline {
                        failures.push(format!(
                            "{} variant {}: expected exit {:?} stdout {:?}, got exit {:?} stdout {:?}",
                            case.id, variant.name, baseline.halted, baseline.uart, outcome.halted, outcome.uart
                        ));
                    }
                }
                Err(e) => failures.push(format!("{} variant {}: {e}", case.id, variant.name)),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "conformance corpus mismatches:\n{}",
        failures.join("\n")
    );
}

#[test]
fn conformance_ir_optimization_is_deterministic() {
    let mut failures = Vec::new();
    for case in conformance_support::CASES {
        for (idx, variant) in VARIANTS.iter().enumerate() {
            if !variant.ir_opt.any() {
                continue;
            }
            match (
                conformance_support::canonical_ir(case, idx),
                conformance_support::canonical_ir(case, idx),
            ) {
                (Ok(first), Ok(second)) => {
                    if first != second {
                        failures.push(format!(
                            "{} variant {}: canonical IR differs between compiles",
                            case.id, variant.name
                        ));
                    }
                }
                (Err(e), _) | (_, Err(e)) => {
                    failures.push(format!("{} variant {}: {e}", case.id, variant.name));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "IR determinism mismatches:\n{}",
        failures.join("\n")
    );
}

#[test]
fn conformance_qemu_corpus_matches_manifest_and_baseline_vm() {
    let mut failures = Vec::new();
    let mut unavailable = None;
    for case in conformance_support::CASES {
        let baseline =
            match conformance_support::run_case_vm(case, conformance_support::BASELINE_VARIANT) {
                Ok(baseline) => baseline,
                Err(e) => {
                    failures.push(format!("{} baseline: {e}", case.id));
                    continue;
                }
            };
        let elf = match conformance_support::compile_case_elf(case, conformance_support::BASELINE_VARIANT)
        {
            Ok(elf) => elf,
            Err(e) => {
                failures.push(format!("{}: {e}", case.id));
                continue;
            }
        };
        match qemu_support::run_elf(&elf) {
            Ok(result) => {
                if result.exit_code != case.expected_exit || result.stdout != case.expected_stdout {
                    failures.push(format!(
                        "{}: qemu exit {} stdout {:?} != manifest exit {} stdout {:?}; stderr {:?}",
                        case.id,
                        result.exit_code,
                        result.stdout,
                        case.expected_exit,
                        case.expected_stdout,
                        result.stderr
                    ));
                }
                let baseline_exit = baseline.halted.unwrap_or(-1);
                if result.exit_code as i64 != baseline_exit || result.stdout != baseline.uart {
                    failures.push(format!(
                        "{}: qemu exit {} stdout {:?} != baseline VM exit {} stdout {:?}; stderr {:?}",
                        case.id,
                        result.exit_code,
                        result.stdout,
                        baseline_exit,
                        baseline.uart,
                        result.stderr
                    ));
                }
            }
            Err(qemu_support::QemuError::Unavailable(reason)) => {
                if unavailable.is_none() {
                    unavailable = Some(reason);
                }
                break;
            }
            Err(qemu_support::QemuError::Failed(msg)) => {
                failures.push(format!("{}: {msg}", case.id));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "QEMU corpus mismatches:\n{}",
        failures.join("\n")
    );
    if let Some(reason) = unavailable {
        assert!(
            !qemu_support::qemu_required(),
            "required QEMU corpus failed: {reason}"
        );
        eprintln!("[SKIP] conformance_qemu_corpus - {reason}");
    }
}

#[test]
fn conformance_matrix_matches_committed_file() {
    let rendered = conformance_support::render_matrix();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(conformance_support::MATRIX_REL_PATH);
    if conformance_support::should_update_matrix() {
        std::fs::write(&path, &rendered).unwrap_or_else(|e| {
            panic!("failed to write {}: {e}", path.display())
        });
        println!("updated {}", path.display());
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing committed matrix at {}; rerun with UPDATE_CONFORMANCE_MATRIX=1 to bootstrap it: {e}",
            path.display()
        )
    });
    assert_eq!(
        rendered,
        committed,
        "conformance matrix drifted; rerun with UPDATE_CONFORMANCE_MATRIX=1"
    );
}

#[test]
fn conformance_manifest_is_well_formed() {
    let mut seen = HashSet::new();
    let mut failures = Vec::new();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for case in conformance_support::CASES {
        if !seen.insert(case.id) {
            failures.push(format!("duplicate case id {}", case.id));
        }
        if !(0..=255).contains(&case.expected_exit) {
            failures.push(format!(
                "{}: expected exit {} out of 0..=255",
                case.id, case.expected_exit
            ));
        }
        if case.step_limit == 0 {
            failures.push(format!("{}: step limit must be positive", case.id));
        }
        if case.features.is_empty() {
            failures.push(format!("{}: case must cover at least one feature", case.id));
        }
        match std::fs::read_to_string(root.join(case.source_path)) {
            Ok(source) if source == case.source => {}
            Ok(_) => failures.push(format!("{}: source_path does not match source", case.id)),
            Err(e) => failures.push(format!("{}: invalid source_path: {e}", case.id)),
        }
    }
    let mut seen_features = HashSet::new();
    for feature in Feature::ALL {
        if !conformance_support::FEATURE_COVERAGE
            .iter()
            .any(|row| row.feature == feature)
        {
            failures.push(format!("{:?} has no feature row", feature));
        }
    }
    for row in conformance_support::FEATURE_COVERAGE {
        if !seen_features.insert(row.feature) {
            failures.push(format!("{:?} has duplicate feature rows", row.feature));
        }
        if row.golden.is_empty() {
            failures.push(format!("{:?} has no golden evidence", row.feature));
        }
        if !conformance_support::CASES
            .iter()
            .any(|case| case.features.contains(&row.feature))
        {
            failures.push(format!("{:?} has no VM/QEMU evidence", row.feature));
        }
        for key in row.golden {
            let ir_path = root.join("programs/test").join(format!("{key}.ir"));
            if !ir_path.exists() {
                failures.push(format!("golden evidence {key} has no committed .ir snapshot"));
            }
            let asm_path = root.join("programs/test").join(format!("{key}.s"));
            if !asm_path.exists() {
                failures.push(format!("golden evidence {key} has no committed .s snapshot"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "malformed conformance manifest:\n{}",
        failures.join("\n")
    );
}

#[test]
fn conformance_runner_preserves_exact_stdout_and_exit() {
    let case = conformance_support::case_by_id("imports_strings_stdout_trap");
    let vm = conformance_support::run_case_vm(case, conformance_support::PRODUCTION_VARIANT)
        .expect("production VM run");
    assert_eq!(vm.uart, "hello\n", "exact stdout including trailing newline");
    assert_eq!(vm.halted, Some(101), "nonzero trap exit preserved");

    let elf = qemu_support::compile_hll_to_elf(case.source);
    let Some(result) = qemu_support::require_qemu_with(
        qemu_support::qemu_required(),
        "conformance_runner_preserves_exact_stdout_and_exit",
        qemu_support::run_elf(&elf),
    ) else {
        return;
    };
    assert_eq!(result.stdout, "hello\n", "exact stdout under QEMU");
    assert_eq!(result.exit_code, 101, "nonzero trap exit under QEMU");
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
#[test]
fn conformance_runner_timeout_kills_child() {
    const INFINITE_LOOP: &str = "main: () -> i32 { while true {} return 0 }\n";
    let elf = qemu_support::compile_hll_to_elf(INFINITE_LOOP);
    match qemu_support::run_elf_with_timeout(&elf, std::time::Duration::from_secs(2)) {
        Err(qemu_support::QemuError::Unavailable(qemu_support::QemuUnavailable::TimedOut(
            secs,
        ))) => assert_eq!(secs, 2),
        Err(qemu_support::QemuError::Unavailable(reason)) => {
            assert!(
                !qemu_support::qemu_required(),
                "required QEMU run failed: {reason}"
            );
            eprintln!("[SKIP] conformance_runner_timeout_kills_child - {reason}");
        }
        Err(qemu_support::QemuError::Failed(msg)) => panic!("{msg}"),
        Ok(result) => panic!("an infinite loop must not halt; got {result:?}"),
    }
}

#[test]
fn conformance_runner_required_vs_optional_tool_absence() {
    use qemu_support::{QemuError, QemuResult, QemuUnavailable};
    let result = QemuResult {
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
    };
    assert_eq!(
        qemu_support::apply_qemu_policy(false, "t", Ok(result.clone())),
        Ok(Some(result.clone()))
    );
    assert_eq!(
        qemu_support::apply_qemu_policy(
            false,
            "t",
            Err(QemuError::Unavailable(QemuUnavailable::MissingBinary(
                "none".to_owned()
            )))
        ),
        Ok(None)
    );
    let required = qemu_support::apply_qemu_policy(
        true,
        "t",
        Err(QemuError::Unavailable(QemuUnavailable::MissingBinary(
            "none".to_owned(),
        ))),
    );
    assert!(required.is_err(), "required mode must fail on missing tools");
    let failed =
        qemu_support::apply_qemu_policy(false, "t", Err(QemuError::Failed("boom".to_owned())));
    assert!(failed.is_err(), "a real QEMU failure must never skip");
}

#[test]
fn conformance_runner_rejects_non_utf8_output() {
    let error = qemu_support::decode_utf8("stdout", vec![0xff]).expect_err("invalid UTF-8");
    match error {
        qemu_support::QemuError::Failed(message) => {
            assert!(message.contains("stdout was not UTF-8"));
        }
        qemu_support::QemuError::Unavailable(reason) => {
            panic!("invalid output must be a run failure, not unavailable: {reason}");
        }
    }
}

#[test]
fn conformance_all_cases_compile_under_all_gates() {
    let all_on = VARIANTS
        .iter()
        .position(|variant| variant.name == "all_on")
        .expect("all_on variant");
    let mut failures = Vec::new();
    for case in conformance_support::CASES {
        if let Err(e) = conformance_support::canonical_ir(case, all_on) {
            failures.push(format!("{} all_on: {e}", case.id));
        }
    }
    assert!(
        failures.is_empty(),
        "cases that fail under all transformations:\n{}",
        failures.join("\n")
    );
}
