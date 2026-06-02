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
use hll_to_ir::stdlib::{get_kernel_stdlib_source, get_stdlib_modules_for_mode, get_stdlib_type_prelude};
use os_runtime::kernel;

/// Compile one kernel HLL source as a standalone module and assert it succeeds.
fn assert_kernel_module_compiles(name: &str, source: &str) {
    let mut p = CompilationPipeline::new();
    p.set_target_mode(TargetMode::Kernel);
    p.set_write_artifacts(false);
    p.compile_modules(&[(name, source)])
        .unwrap_or_else(|e| panic!("kernel module `{name}` failed to compile:\n{e}"));
}

// ---------------------------------------------------------------------------
// Stdlib modules used by the kernel
// ---------------------------------------------------------------------------

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
    let mut p = CompilationPipeline::new();
    p.set_target_mode(TargetMode::Kernel);
    p.set_write_artifacts(false);
    p.set_string_prefix(Some("__kern_str_".to_owned()));
    p.compile(&get_kernel_stdlib_source())
        .unwrap_or_else(|e| panic!("kernel stdlib bundle failed to compile:\n{e}"));
}

// ---------------------------------------------------------------------------
// Individual kernel HLL modules
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Regression: my_kernel.hll must compile with all identifiers resolved.
//
// This test directly prevents the `computed_binary_pa` class of bug: a local
// variable renamed during refactoring leaves a stale reference that the
// compiler must catch.  If this test fails, inspect spawn_user_process (or
// any function using undefined names) before running slower runtime tests.
// ---------------------------------------------------------------------------

#[test]
fn my_kernel_compiles() {
    assert_kernel_module_compiles("my_kernel", kernel::MY_KERNEL);
}

// ---------------------------------------------------------------------------
// Module count sanity checks
// ---------------------------------------------------------------------------

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
