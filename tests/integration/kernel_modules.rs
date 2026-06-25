/// Verify every kernel HLL module compiles without errors.
///
/// Each test here is a fast, build-time check: compile one module and assert
/// no diagnostics.  Any undefined identifier in a kernel file (like the
/// `computed_binary_pa` bug in `my_kernel.hll`) is caught here before slower
/// runtime tests run.
///
/// The module is compiled in isolation -- `external` declarations resolve at
/// link time, so they don't cause errors. Only truly undefined identifiers
/// (used but never declared) produce a compile-time error.
use full_stack::compilation_pipeline::{CompilationPipeline, TargetMode};
use hll_to_ir::stdlib::{get_stdlib_modules_for_mode, get_stdlib_type_prelude};
use os_runtime::kernel;

/// Compile one kernel HLL source as a standalone module and assert it succeeds.
fn assert_kernel_module_compiles(name: &str, source: &str) {
    let mut p = CompilationPipeline::new();
    p.set_target_mode(TargetMode::Kernel);
    p.set_write_artifacts(false);
    p.compile_modules(&[(name, source)])
        .unwrap_or_else(|e| panic!("kernel module `{name}` failed to compile:\n{e}"));
}

// --- Stdlib modules used by the kernel ---

#[test]
fn kernel_stdlib_all_modules_compile() {
    let modules = get_stdlib_modules_for_mode(TargetMode::Kernel);
    let mut p = CompilationPipeline::new();
    p.set_target_mode(TargetMode::Kernel);
    p.set_write_artifacts(false);
    p.set_string_prefix(Some("__kern_str_".to_owned()));
    p.set_type_prelude(get_stdlib_type_prelude());
    p.compile_modules(&modules)
        .unwrap_or_else(|e| panic!("kernel stdlib modules failed to compile:\n{e}"));
}

#[test]
fn kernel_stdlib_full_bundle_compiles() {
    CompilationPipeline::compile_stdlib_objects(TargetMode::Kernel)
        .unwrap_or_else(|e| panic!("kernel stdlib failed to compile:\n{e}"));
}

// --- Individual kernel HLL modules ---

#[test]
fn kernel_entry_compiles() {
    assert_kernel_module_compiles("entry", kernel::RUNTIME);
}

#[test]
fn kernel_trap_entry_compiles() {
    assert_kernel_module_compiles("trap_entry", kernel::TRAP_ENTRY);
}

#[test]
fn kernel_trap_handler_compiles() {
    assert_kernel_module_compiles("trap_handler", kernel::TRAP_HANDLER);
}

#[test]
fn kernel_utilities_compiles() {
    assert_kernel_module_compiles("utilities", kernel::UTILITIES);
}

#[test]
fn kernel_checks_compiles() {
    assert_kernel_module_compiles("checks", kernel::CHECKS);
}

#[test]
fn kernel_pmm_compiles() {
    assert_kernel_module_compiles("pmm", kernel::PMM);
}

#[test]
fn kernel_vmm_compiles() {
    assert_kernel_module_compiles("vmm", kernel::VMM);
}

#[test]
fn kernel_process_compiles() {
    assert_kernel_module_compiles("process", kernel::PROCESS);
}

#[test]
fn kernel_syscall_compiles() {
    assert_kernel_module_compiles("syscall", kernel::SYSCALL);
}

#[test]
fn kernel_scheduler_compiles() {
    assert_kernel_module_compiles("scheduler", kernel::SCHEDULER);
}

#[test]
fn kernel_fs_compiles() {
    assert_kernel_module_compiles("fs", kernel::FS);
}

// --- Regression: my_kernel.hll must compile with all identifiers resolved ---
// This test directly prevents the `computed_binary_pa` class of bug: a local
// variable renamed during refactoring leaves a stale reference that the
// compiler must catch.  If this test fails, inspect spawn_user_process (or
// any function using undefined names) before running slower runtime tests.

#[test]
fn my_kernel_compiles() {
    assert_kernel_module_compiles("my_kernel", kernel::MY_KERNEL);
}

// --- Frame-size guard ---
// HLL gives every block-local its own stack slot, so a giant function can grow
// a frame past the RV immediate range [-2048, 2047], at which point an `sd`/`ld`
// offset panics the assembler at compile time (this bit syscall_dispatch twice).
// Flag any kernel function whose frame exceeds a safe threshold here, so the
// problem surfaces as a clear test failure rather than a deep compile panic.

// Safe ceiling: leaves headroom below the 2047-byte immediate limit for the
// largest slot offset within a frame.
const FRAME_WARN_BYTES: u64 = 1800;

// Parse `<label>:` / `; Allocate stack frame: N bytes` pairs out of asm text.
fn frame_sizes(asm: &str) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    for line in asm.lines() {
        let trimmed = line.trim();
        if let Some(label) = trimmed.strip_suffix(':') {
            if !label.is_empty() && !label.starts_with('.') && !label.contains(' ') {
                current = Some(label.to_owned());
            }
        } else if let Some(rest) = trimmed.strip_prefix("; Allocate stack frame:") {
            if let (Some(name), Some(size)) = (
                current.take(),
                rest.trim()
                    .strip_suffix(" bytes")
                    .and_then(|s| s.parse().ok()),
            ) {
                out.push((name, size));
            }
        }
    }
    out
}

// Compile every kernel function and assert no frame exceeds the safe ceiling.
#[test]
fn kernel_frames_stay_within_immediate_range() {
    let mut p = CompilationPipeline::new();
    p.set_target_mode(TargetMode::Kernel);
    p.set_write_artifacts(false);
    p.set_string_prefix(Some("__kern_str_".to_owned()));

    p.set_type_prelude(get_stdlib_type_prelude());
    let mut sources: Vec<(&str, String)> = get_stdlib_modules_for_mode(TargetMode::Kernel)
        .iter()
        .map(|(n, s)| (*n, (*s).to_owned()))
        .collect();
    sources.push(("my_kernel", kernel::MY_KERNEL.to_owned()));

    let mut offenders: Vec<(String, u64)> = Vec::new();
    let mut parsed_any = false;
    for (name, source) in &sources {
        let ir = p
            .compile(source)
            .unwrap_or_else(|e| panic!("{name} failed to compile:\n{e}"))
            .ir_program;
        let asm = p.compile_ir_to_assembly(&ir);
        for (func, size) in frame_sizes(&asm) {
            parsed_any = true;
            if size > FRAME_WARN_BYTES {
                offenders.push((func, size));
            }
        }
    }

    // Guard against the parser silently matching nothing (asm format drift).
    assert!(
        parsed_any,
        "no frame-size comments parsed; the asm format changed"
    );

    assert!(
        offenders.is_empty(),
        "kernel functions exceed the safe frame ceiling ({FRAME_WARN_BYTES} bytes); \
         factor them into helpers:\n{}",
        offenders
            .iter()
            .map(|(f, s)| format!("  {f}: {s} bytes"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

// --- Module count sanity checks ---

#[test]
fn kernel_stdlib_has_expected_module_count() {
    let modules = get_stdlib_modules_for_mode(TargetMode::Kernel);
    // Kernel stdlib: types, memory_allocator, string_utils, mem, freestanding
    // runtime, console, klog, trap_entry, utilities, checks, entry, trap_handler,
    // pmm, vmm, process, syscall, scheduler, fs -- at least 18 modules.
    assert!(
        modules.len() >= 18,
        "expected at least 18 kernel stdlib modules, got {}",
        modules.len()
    );
}
