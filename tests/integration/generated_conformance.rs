// Deterministic valid programs exercise control-flow shapes beyond the hand-written corpus.

use crate::{conformance_support, generated_conformance_support, qemu_support};
use conformance_support::{VARIANTS, VmOutcome};
use generated_conformance_support::{
    GENERATED_STEP_LIMIT, GeneratedProgram, configured_seeds, load_regressions, persist_failure,
};

#[derive(Debug)]
struct DifferentialFailure {
    variant_idx: usize,
    details: String,
}

enum QemuProbe {
    Match,
    Mismatch(String),
    Unavailable(qemu_support::QemuUnavailable),
}

#[test]
fn generated_conformance_vm_all_variants() {
    for seed in configured_seeds().expect("valid generated conformance case count") {
        let program = GeneratedProgram::generate(seed);
        let source = program.render();
        if let Some(failure) = vm_failure(&source) {
            let reduced = program.minimize(|candidate| {
                vm_failure(&candidate.render())
                    .is_some_and(|candidate_failure| candidate_failure.variant_idx == failure.variant_idx)
            });
            fail_with_artifact(
                &format!("vm_seed_{seed:016x}_{}", VARIANTS[failure.variant_idx].name),
                &reduced.render(),
                &failure.details,
            );
        }
    }

    for (name, source) in load_regressions().expect("load generated conformance regressions") {
        if let Some(failure) = vm_failure(&source) {
            panic!("retained regression {name}: {}", failure.details);
        }
    }
}

#[test]
fn generated_conformance_qemu_all_variants() {
    for seed in configured_seeds().expect("valid generated conformance case count") {
        let program = GeneratedProgram::generate(seed);
        let source = program.render();
        for (variant_idx, variant) in VARIANTS.iter().enumerate() {
            match qemu_probe(&source, variant_idx) {
                QemuProbe::Match => {}
                QemuProbe::Mismatch(details) => {
                    let reduced = program.minimize(|candidate| {
                        matches!(
                            qemu_probe(&candidate.render(), variant_idx),
                            QemuProbe::Mismatch(_)
                        )
                    });
                    fail_with_artifact(
                        &format!("qemu_seed_{seed:016x}_{}", variant.name),
                        &reduced.render(),
                        &details,
                    );
                }
                QemuProbe::Unavailable(reason) => {
                    handle_qemu_unavailable(reason);
                    return;
                }
            }
        }
    }

    for (name, source) in load_regressions().expect("load generated conformance regressions") {
        for (variant_idx, variant) in VARIANTS.iter().enumerate() {
            match qemu_probe(&source, variant_idx) {
                QemuProbe::Match => {}
                QemuProbe::Mismatch(details) => {
                    panic!(
                        "retained regression {name}, variant {}: {details}",
                        variant.name
                    );
                }
                QemuProbe::Unavailable(reason) => {
                    handle_qemu_unavailable(reason);
                    return;
                }
            }
        }
    }
}

#[test]
fn generated_conformance_ir_is_deterministic() {
    let mut failures = Vec::new();
    for seed in configured_seeds().expect("valid generated conformance case count") {
        let source = GeneratedProgram::generate(seed).render();
        for (variant_idx, variant) in VARIANTS.iter().enumerate() {
            let first = conformance_support::canonical_source_ir(&source, variant_idx);
            let second = conformance_support::canonical_source_ir(&source, variant_idx);
            match (first, second) {
                (Ok(first), Ok(second)) if first == second => {}
                (Ok(_), Ok(_)) => failures.push(format!(
                    "seed {seed:016x}, variant {}: canonical IR changed between compiles",
                    variant.name
                )),
                (Err(error), _) | (_, Err(error)) => failures.push(format!(
                    "seed {seed:016x}, variant {}: {error}",
                    variant.name
                )),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "generated IR determinism failures:\n{}",
        failures.join("\n")
    );
}

fn vm_failure(source: &str) -> Option<DifferentialFailure> {
    let baseline = match run_vm(source, conformance_support::BASELINE_VARIANT) {
        Ok(outcome) => outcome,
        Err(error) => {
            return Some(DifferentialFailure {
                variant_idx: conformance_support::BASELINE_VARIANT,
                details: format!("baseline: {error}"),
            });
        }
    };
    for (variant_idx, variant) in VARIANTS.iter().enumerate().skip(1) {
        match run_vm(source, variant_idx) {
            Ok(outcome) if outcome == baseline => {}
            Ok(outcome) => {
                return Some(DifferentialFailure {
                    variant_idx,
                    details: format!(
                        "variant {} produced {outcome:?}; baseline produced {baseline:?}",
                        variant.name
                    ),
                });
            }
            Err(error) => {
                return Some(DifferentialFailure {
                    variant_idx,
                    details: format!("variant {}: {error}", variant.name),
                });
            }
        }
    }
    None
}

fn qemu_probe(source: &str, variant_idx: usize) -> QemuProbe {
    let vm = match run_vm(source, variant_idx) {
        Ok(outcome) => outcome,
        Err(error) => {
            return QemuProbe::Mismatch(format!(
                "variant {} VM run failed: {error}",
                VARIANTS[variant_idx].name
            ));
        }
    };
    let elf = match conformance_support::compile_source_elf(source, variant_idx) {
        Ok(elf) => elf,
        Err(error) => {
            return QemuProbe::Mismatch(format!(
                "variant {} ELF compile failed: {error}",
                VARIANTS[variant_idx].name
            ));
        }
    };
    match qemu_support::run_elf(&elf) {
        Ok(qemu) if outcomes_match(&vm, &qemu) => QemuProbe::Match,
        Ok(qemu) => QemuProbe::Mismatch(format!(
            "variant {} QEMU exit {} stdout {:?}; VM exit {:?} stdout {:?}; stderr {:?}",
            VARIANTS[variant_idx].name,
            qemu.exit_code,
            qemu.stdout,
            vm.halted,
            vm.uart,
            qemu.stderr
        )),
        Err(qemu_support::QemuError::Unavailable(reason)) => QemuProbe::Unavailable(reason),
        Err(qemu_support::QemuError::Failed(error)) => QemuProbe::Mismatch(format!(
            "variant {} QEMU run failed: {error}",
            VARIANTS[variant_idx].name
        )),
    }
}

fn run_vm(source: &str, variant_idx: usize) -> Result<VmOutcome, String> {
    conformance_support::run_source_vm(source, GENERATED_STEP_LIMIT, variant_idx)
}

fn outcomes_match(vm: &VmOutcome, qemu: &qemu_support::QemuResult) -> bool {
    vm.halted == Some(i64::from(qemu.exit_code)) && vm.uart == qemu.stdout
}

fn handle_qemu_unavailable(reason: qemu_support::QemuUnavailable) {
    assert!(
        !qemu_support::qemu_required(),
        "required generated QEMU corpus failed: {reason}"
    );
    eprintln!("[SKIP] generated_conformance_qemu_all_variants - {reason}");
}

fn fail_with_artifact(name: &str, source: &str, details: &str) -> ! {
    let path = persist_failure(name, source, details)
        .unwrap_or_else(|error| panic!("{details}; failure artifact could not be saved: {error}"));
    panic!(
        "{details}; minimized source saved to {} (set {}=1 to promote on rerun)",
        path.display(),
        generated_conformance_support::PROMOTE_FAILURES_VAR
    );
}
