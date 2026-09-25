use asm_to_binary::assembler::link_layout::LinkLayout;
use asm_to_binary::{AssembledOutput, Assembler, ObjectLinker};
use hll_to_ir::stdlib::get_stdlib_modules_for_mode;
use hll_to_ir::{CompileConfig, HllCompiler, TargetMode};
use ir_to_asm::CompilerRv64;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use virtual_machine::virtual_machine::{RunOutcome, VirtualMachine};

// --- Helpers ---

static STDLIB_OBJS: OnceLock<Vec<(String, AssembledOutput)>> = OnceLock::new();

fn runtime_source(path: &str) -> &'static str {
    os_runtime::module_source(path).unwrap_or_else(|| panic!("missing runtime source `{path}`"))
}

fn bundled_module_source(name: &str) -> Option<&'static str> {
    os_runtime::module_source(name).or_else(|| {
        get_stdlib_modules_for_mode(TargetMode::Kernel)
            .into_iter()
            .find_map(|(module_name, source)| (module_name == name).then_some(source))
    })
}

fn direct_imports(source: &str) -> (String, HashMap<String, hll_to_ir::imports::ModuleAlias>) {
    let module_imports =
        hll_to_ir::imports::collect_module_imports(source).expect("module imports");
    let mut prelude = String::new();
    let mut aliases = HashMap::new();
    let mut emitted = HashSet::new();
    let mut visiting = Vec::new();
    append_import(
        "abi",
        "kernel/core/kernel_abi.hll",
        &mut prelude,
        &mut aliases,
        &mut emitted,
        &mut visiting,
    );
    for (alias, path) in module_imports {
        append_import(
            &alias,
            &path,
            &mut prelude,
            &mut aliases,
            &mut emitted,
            &mut visiting,
        );
    }
    (prelude, aliases)
}

fn append_import(
    alias: &str,
    path: &str,
    prelude: &mut String,
    aliases: &mut HashMap<String, hll_to_ir::imports::ModuleAlias>,
    emitted: &mut HashSet<String>,
    visiting: &mut Vec<String>,
) {
    let key = module_key(path);
    let module =
        bundled_module_source(&key).unwrap_or_else(|| panic!("missing bundled module `{key}`"));
    assert!(
        !visiting.contains(&key),
        "cyclic bundled import through `{key}`"
    );
    if !emitted.contains(&key) {
        visiting.push(key.clone());
        for (dependency_alias, dependency_path) in
            hll_to_ir::imports::collect_interface_imports(module).expect("interface imports")
        {
            append_import(
                &dependency_alias,
                &dependency_path,
                prelude,
                aliases,
                emitted,
                visiting,
            );
        }
        visiting.pop();
        prelude.push_str(
            &hll_to_ir::imports::extract_interface(module).expect("extract import interface"),
        );
        prelude.push('\n');
        emitted.insert(key.clone());
    }

    let exports = hll_to_ir::imports::collect_exports(module).expect("collect exports");
    let declared =
        hll_to_ir::imports::collect_declaration_names(module).expect("collect declarations");
    let namespaced =
        hll_to_ir::imports::collect_exported_interfaces(module).expect("collect interfaces");
    aliases.insert(
        alias.to_owned(),
        hll_to_ir::imports::ModuleAlias {
            prefix: String::new(),
            exports: exports.clone(),
            mangled: HashSet::new(),
            namespaced,
            member_aliases: module_member_aliases(alias, &key, &exports),
            declared,
        },
    );
}

fn module_key(path: &str) -> String {
    path.trim_start_matches("./").to_owned()
}

fn module_member_aliases(
    alias: &str,
    key: &str,
    exports: &HashSet<String>,
) -> HashMap<String, String> {
    let mut aliases = HashMap::new();
    // Sort for deterministic alias resolution (the export set is unordered).
    let mut sorted_exports: Vec<&String> = exports.iter().collect();
    sorted_exports.sort();
    let stem = key.rsplit('/').next().unwrap_or(key);
    for prefix in [alias, key, stem] {
        let prefix = prefix.trim_end_matches(".hll");
        if prefix.is_empty() {
            continue;
        }
        let prefix = format!("{prefix}_");
        for export in &sorted_exports {
            let Some(short) = export.strip_prefix(&prefix) else {
                continue;
            };
            if short.is_empty() {
                continue;
            }
            match aliases.get(short) {
                Some(existing) if existing != *export => {
                    aliases.remove(short);
                }
                Some(_) => {}
                None => {
                    aliases.insert(short.to_owned(), (*export).clone());
                }
            }
        }
    }
    for export in &sorted_exports {
        if let Some(short) = export.strip_prefix('k')
            && short
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_lowercase())
        {
            aliases
                .entry(short.to_owned())
                .or_insert_with(|| (*export).clone());
        }
    }
    aliases
}

fn kernel_snippet_abi_exports() -> Vec<String> {
    let manifest = include_str!("../../../tests/fixtures/kernel_snippet.build");
    for raw_line in manifest.lines() {
        let line = raw_line.trim();
        let Some(value) = line.strip_prefix("abi_exports") else {
            continue;
        };
        let Some((_eq, value)) = value.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) else {
            continue;
        };
        return inner
            .split(',')
            .map(str::trim)
            .filter_map(|item| item.strip_prefix('"')?.strip_suffix('"'))
            .map(str::to_owned)
            .collect();
    }
    panic!("kernel_snippet.build must declare abi_exports");
}

fn kernel_hll_modules() -> Vec<(&'static str, &'static str)> {
    os_runtime::SOURCE_TREE
        .iter()
        .filter_map(|(path, source)| {
            let name = path
                .strip_prefix("kernel/")?
                .strip_suffix(".hll")?
                .rsplit('/')
                .next()?;
            Some((name, *source))
        })
        .collect()
}

fn kernel_code_modules() -> Vec<(&'static str, &'static str)> {
    let mut mods = get_stdlib_modules_for_mode(TargetMode::Kernel);
    mods.extend(kernel_hll_modules().into_iter().filter(|(name, source)| {
        *name != "my_kernel"
            && hll_to_ir::imports::defines_code(source).expect("parse kernel module")
    }));
    mods
}

// Compile the kernel stdlib + modules as independent per-module objects (no concatenation).
fn compiled_stdlib() -> &'static [(String, AssembledOutput)] {
    STDLIB_OBJS.get_or_init(|| {
        kernel_code_modules()
            .iter()
            .map(|(name, src)| {
                let (source_prelude, module_aliases) = direct_imports(src);
                let compiler = HllCompiler::new(CompileConfig {
                    target: TargetMode::Kernel,
                    strict: true,
                    string_prefix: Some("__kern_str_".to_owned()),
                    type_prelude: Vec::new(),
                    source_prelude: Some(source_prelude),
                    module_aliases,
                    module_mangle_prefix: None,
                    source_file: Some(format!("{name}.hll")),
                });
                let out = compiler.compile(src).unwrap_or_else(|diags| {
                    panic!("kernel stdlib `{name}` compile failed: {diags:?}")
                });
                let mut rv = CompilerRv64::new();
                let (_, tokens) = rv
                    .compile_with_tokens(&out.ir)
                    .expect("stdlib IR type layout");
                let obj = Assembler::assemble(&tokens)
                    .unwrap_or_else(|e| panic!("stdlib `{name}` assemble failed: {e}"));
                ((*name).to_owned(), obj)
            })
            .collect()
    })
}

fn run_kernel_hll(user_src: &str) -> (String, Option<i64>) {
    let stdlib_objs = compiled_stdlib();
    let (source_prelude, module_aliases) = direct_imports(user_src);

    let user_compiler = HllCompiler::new(CompileConfig {
        target: TargetMode::Kernel,
        strict: true,
        string_prefix: None,
        type_prelude: Vec::new(),
        source_prelude: Some(source_prelude),
        module_aliases,
        module_mangle_prefix: None,
        source_file: Some("user.hll".to_owned()),
    });
    let user_out = user_compiler
        .compile(user_src)
        .unwrap_or_else(|diags| panic!("user compile failed: {diags:?}"));
    let mut user_rv = CompilerRv64::new();
    let (_, user_tokens) = user_rv
        .compile_with_tokens(&user_out.ir)
        .expect("user IR type layout");

    let mut user_obj =
        Assembler::assemble(&user_tokens).unwrap_or_else(|e| panic!("user assemble failed: {e}"));
    for export in kernel_snippet_abi_exports() {
        user_obj.mark_entry_global(&export);
    }
    let mut modules: Vec<(&str, &AssembledOutput)> =
        stdlib_objs.iter().map(|(n, o)| (n.as_str(), o)).collect();
    modules.push(("user", &user_obj));
    let mut assembled = ObjectLinker::link(&modules).unwrap_or_else(|e| panic!("link failed: {e}"));
    let layout = LinkLayout::freestanding_kernel();
    if layout.emit_layout_symbols {
        assembled.inject_layout_symbols(&layout);
    }

    let mut vm = VirtualMachine::new_kernel(&assembled);
    let run = vm.run(10_000_000);
    let exit = match run.termination {
        RunOutcome::Halted(code) => Some(code),
        RunOutcome::TimedOut => None,
        RunOutcome::Fault(error) => panic!("VM fault: {error}"),
    };
    (run.uart_output, exit)
}

// --- ROM / boot assembly source content ---

#[test]
fn explicit_roots_keep_user_mappings_and_permissions_separate() {
    let (uart, exit) = run_kernel_hll(
        r#"
pages := import("kernel/mm/physical_pages.hll")
spaces := import("kernel/mm/address_space.hll")
user := import("kernel/mm/user_memory.hll")
system := import("kernel/arch/riscv64/system.hll")

kmain: () {
    pages.init(0x84000000, 0x84100000)
    first: u64 = spaces.create()
    second: u64 = spaces.create()
    spaces.map(first, 0x40000000, 0x84200000, 18)
    spaces.map(second, 0x40001000, 0x84201000, 22)
    if user.validate(first, 0x40000000 as u8*, 4096, 0) != 1 { system.shutdown(1) }
    if user.validate(first, 0x40000000 as u8*, 1, 1) != 0 { system.shutdown(2) }
    if user.validate(second, 0x40000000 as u8*, 1, 0) != 0 { system.shutdown(3) }
    if user.validate(first, 0x40000FFF as u8*, 2, 0) != 0 { system.shutdown(4) }
    if user.validate(second, 0x40001000 as u8*, 4096, 1) != 1 { system.shutdown(5) }
    if user.validate(first, 0xFFFFFFFFFFFFFFF0 as u8*, 32, 0) != 0 { system.shutdown(6) }
    if user.validate(first, 0x80000000 as u8*, 1, 0) != 0 { system.shutdown(7) }
    if user.validate(0, null, 0, 0) != 0 { system.shutdown(8) }
    system.shutdown(0)
}
"#,
    );
    assert_eq!(exit, Some(0), "{uart}");
}

#[test]
fn physical_page_exhaustion_and_reuse_preserve_the_boundary() {
    let (uart, exit) = run_kernel_hll(
        r#"
pages := import("kernel/mm/physical_pages.hll")
system := import("kernel/arch/riscv64/system.hll")
kmain: () {
    pages.init(0x84000000, 0x84001000)
    page: u8* = pages.alloc()
    if page == null || pages.alloc() != null { system.shutdown(1) }
    pages.free(page)
    if pages.alloc() != page || pages.alloc() != null { system.shutdown(2) }
    system.shutdown(0)
}
"#,
    );
    assert_eq!(exit, Some(0), "{uart}");
}

#[test]
fn rom_source_is_startup_concatenated_with_trap() {
    let expected = format!("{}{}", os_runtime::BOOT_STARTUP, os_runtime::BOOT_TRAP);
    assert_eq!(
        os_runtime::ROM_SOURCE,
        expected,
        "ROM_SOURCE must equal BOOT_STARTUP + BOOT_TRAP"
    );
}

#[test]
fn boot_startup_mrets_into_smode() {
    assert!(
        os_runtime::BOOT_STARTUP.contains("mret"),
        "startup stub must mret into S-mode"
    );
    assert!(
        os_runtime::BOOT_STARTUP.contains("medeleg"),
        "startup stub must delegate exceptions via medeleg"
    );
    assert!(
        os_runtime::BOOT_STARTUP.contains("mideleg"),
        "startup stub must delegate interrupts via mideleg"
    );
}

#[test]
fn boot_trap_handles_ecalls() {
    assert!(
        os_runtime::BOOT_TRAP.contains("_dispatch_ecall"),
        "M-mode trap handler must dispatch ecalls"
    );
    assert!(
        os_runtime::BOOT_TRAP.contains("sys_exit"),
        "M-mode trap handler must implement sys_exit"
    );
    assert!(
        os_runtime::BOOT_TRAP.contains("sys_write"),
        "M-mode trap handler must implement sys_write"
    );
}

// --- End-to-end kernel boot execution ---

#[test]
fn kernel_boots_and_exits_cleanly() {
    let (uart, exit) = run_kernel_hll(runtime_source("kernel/core/my_kernel.hll"));
    assert_eq!(exit, Some(0), "kernel must exit with code 0; uart={uart:?}");
}

#[test]
fn trap_handler_installed_at_boot() {
    let (uart, exit) = run_kernel_hll(runtime_source("kernel/core/my_kernel.hll"));
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains("[  OK  ] trap handler installed\n"),
        "uart must confirm trap handler install; uart={uart:?}"
    );
}

#[test]
fn timer_armed_at_boot() {
    let (uart, exit) = run_kernel_hll(runtime_source("kernel/core/my_kernel.hll"));
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains("[  OK  ] timer armed\n"),
        "uart must confirm timer was armed; uart={uart:?}"
    );
}

#[test]
fn memory_self_test_passes() {
    let (uart, exit) = run_kernel_hll(runtime_source("kernel/core/my_kernel.hll"));
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains("[  OK  ] memory self-test passed\n"),
        "memory self-test must pass; uart={uart:?}"
    );
    assert!(
        !uart.contains("memory self-test failed"),
        "memory self-test must not fail; uart={uart:?}"
    );
}

#[test]
fn heap_smoke_test_passes() {
    let (uart, exit) = run_kernel_hll(runtime_source("kernel/core/my_kernel.hll"));
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains("[  OK  ] heap ready\n"),
        "heap smoke-test must pass; uart={uart:?}"
    );
}

#[test]
fn unimplemented_subsystems_warn() {
    let (uart, exit) = run_kernel_hll(runtime_source("kernel/core/my_kernel.hll"));
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains("[ WARN ] device tree:"),
        "device tree stub must emit warn; uart={uart:?}"
    );
    assert!(
        uart.contains("[  OK  ] interrupt controller online\n"),
        "interrupt controller must initialize and report online; uart={uart:?}"
    );
}

#[test]
fn pmm_smoke_test_passes() {
    let (uart, exit) = run_kernel_hll(runtime_source("kernel/core/my_kernel.hll"));
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains("[  OK  ] pmm ready\n"),
        "PMM smoke-test must pass; uart={uart:?}"
    );
}

#[test]
fn kernel_modules_export_required_boundaries() {
    let required = [
        (
            runtime_source("kernel/core/kernel_abi.hll"),
            &["PCB_SIZE", "PROC_READY", "SysAction"][..],
        ),
        (
            runtime_source("kernel/sched/process.hll"),
            &["process_create", "process_init"][..],
        ),
        (
            runtime_source("kernel/sched/scheduler.hll"),
            &[
                "schedule",
                "scheduler_add",
                "scheduler_current",
                "scheduler_init",
            ][..],
        ),
        (
            runtime_source("kernel/syscall/syscalls.hll"),
            &["syscalls_dispatch"][..],
        ),
    ];
    for (source, names) in required {
        let exports = hll_to_ir::imports::collect_exports(source).expect("collect exports");
        for name in names {
            assert!(exports.contains(*name), "missing kernel export `{name}`");
        }
    }
}

#[test]
fn kernel_modules_reference_imports_only_through_aliases() {
    // Quality gate: a kernel module must reach another module's functions/globals through its
    // qualified import alias, never by bare flat name. Inline asm is the sole exempt path (6.3).
    let mut modules = vec![("entry", runtime_source("stdlib/platform/kernel/entry.hll"))];
    modules.extend(kernel_hll_modules());
    for (name, source) in modules {
        // The link symbols (functions + non-extern globals) of every imported module, minus this
        // module's own declarations. Const/type/struct exports fold flat by design and are skipped.
        let local = hll_to_ir::imports::collect_declaration_names(source).expect("decl names");
        let mut imported: HashSet<String> = HashSet::new();
        for (_alias, path) in
            hll_to_ir::imports::collect_module_imports(source).expect("module imports")
        {
            let key = module_key(&path);
            let module_src = bundled_module_source(&key)
                .unwrap_or_else(|| panic!("missing bundled module `{key}`"));
            imported.extend(
                hll_to_ir::imports::collect_exported_link_symbols(module_src)
                    .expect("link symbols"),
            );
        }
        imported.retain(|symbol| !local.contains(symbol));

        let refs = hll_to_ir::imports::collect_bare_references(source).expect("bare references");
        let mut leaks: Vec<&String> = refs.intersection(&imported).collect();
        leaks.sort();
        assert!(
            leaks.is_empty(),
            "{name}.hll references imported symbols by bare flat name (use the import alias, \
             e.g. `fs.read`): {leaks:?}"
        );
    }
}

// --- Compile / boot smoke test ---

#[test]
fn kernel_boots_with_process_and_scheduler() {
    // Minimal kernel that initialises process + scheduler alongside normal boot.
    let test_kernel = "
external klog_ok:               (msg: u8*)
external klog_warn:             (msg: u8*)
external klog_error:            (msg: u8*)
external klog_int:              (label: u8*, val: i64)
external kernel_heap_alloc_bytes: (size: u64) -> u8*
external mem_set:               (dst: u8*, value: u8, n: u64) -> u8*
external system_shutdown:       (code: i64)
external trap_init:             ()
external interrupt_controller_init: ()
external timer_rearm:           (interval: u64)
external physical_pages_init:   (start: u64, end: u64)
external address_space_boot:    ()
external memory_self_test:      (size: u64) -> i64
external pmm_ops_test:          ()
external process_init:          ()
external process_create:        (entry_pc: u64) -> u64*
external scheduler_init:        ()
external scheduler_add:         (pcb: u64*)

kmain: () {
    klog_ok(\"kernel starting\".ptr)
    trap_init()
    timer_rearm(1000000)
    interrupt_controller_init()
    klog_ok(\"interrupt controller online\".ptr)
    memory_self_test(256)
    kernel_heap_alloc_bytes(64)
    physical_pages_init(0x80100000, 0x87F00000)
    address_space_boot()
    process_init()
    scheduler_init()
    klog_ok(\"boot complete\".ptr)
    system_shutdown(0)
}
";
    let (uart, exit) = run_kernel_hll(test_kernel);
    assert_eq!(exit, Some(0), "kernel must exit with code 0; uart={uart:?}");
    assert!(
        uart.contains("[  OK  ] process subsystem ready\n"),
        "process subsystem must initialise; uart={uart:?}"
    );
    assert!(
        uart.contains("[  OK  ] scheduler ready\n"),
        "scheduler must initialise; uart={uart:?}"
    );
    assert!(
        uart.contains("[  OK  ] boot complete\n"),
        "kernel must complete boot; uart={uart:?}"
    );
}

// --- malloc / free unit tests ---

const MALLOC_PRELUDE: &str = r#"
system := import("kernel/arch/riscv64/system.hll")
external malloc:    (size: u64) -> u8*
external free:      (ptr: u8*)
"#;

#[test]
fn malloc_returns_non_null_for_small_allocation() {
    let src = [
        MALLOC_PRELUDE,
        r#"
kmain: () {
    p: u8* = malloc(8)
    if p == null {
        system.shutdown(1)
        return
    }
    system.shutdown(0)
}
"#,
    ]
    .concat();
    let (_, exit) = run_kernel_hll(&src);
    assert_eq!(exit, Some(0), "malloc(8) must return a non-null pointer");
}

#[test]
fn malloc_zero_size_returns_non_null() {
    let src = [
        MALLOC_PRELUDE,
        r#"
kmain: () {
    p: u8* = malloc(0)
    if p == null {
        system.shutdown(1)
        return
    }
    system.shutdown(0)
}
"#,
    ]
    .concat();
    let (_, exit) = run_kernel_hll(&src);
    assert_eq!(
        exit,
        Some(0),
        "malloc(0) must be coerced to malloc(1) and return non-null"
    );
}

#[test]
fn malloc_write_read_roundtrip() {
    let src = [
        MALLOC_PRELUDE,
        r#"
kmain: () {
    p: i64* = malloc(8) as i64*
    if p == null {
        system.shutdown(1)
        return
    }
    @p = 12345
    if @p != 12345 {
        system.shutdown(2)
        return
    }
    system.shutdown(0)
}
"#,
    ]
    .concat();
    let (_, exit) = run_kernel_hll(&src);
    assert_eq!(
        exit,
        Some(0),
        "write to and read back from malloc'd memory must round-trip"
    );
}

#[test]
fn malloc_multiple_allocations_are_distinct() {
    let src = [
        MALLOC_PRELUDE,
        r#"
kmain: () {
    a: u8* = malloc(8)
    b: u8* = malloc(8)
    c: u8* = malloc(8)
    if a == null {
        system.shutdown(1)
        return
    }
    if b == null {
        system.shutdown(2)
        return
    }
    if c == null {
        system.shutdown(3)
        return
    }
    if a == b {
        system.shutdown(4)
        return
    }
    if b == c {
        system.shutdown(5)
        return
    }
    if a == c {
        system.shutdown(6)
        return
    }
    system.shutdown(0)
}
"#,
    ]
    .concat();
    let (_, exit) = run_kernel_hll(&src);
    assert_eq!(
        exit,
        Some(0),
        "three consecutive malloc calls must return distinct pointers"
    );
}

#[test]
fn free_marks_block_reusable_on_next_same_size_malloc() {
    let src = [
        MALLOC_PRELUDE,
        r#"
kmain: () {
    p: u8* = malloc(64)
    if p == null {
        system.shutdown(1)
        return
    }
    free(p)
    q: u8* = malloc(64)
    if q == null {
        system.shutdown(2)
        return
    }
    if q != p {
        system.shutdown(3)
        return
    }
    system.shutdown(0)
}
"#,
    ]
    .concat();
    let (_, exit) = run_kernel_hll(&src);
    assert_eq!(
        exit,
        Some(0),
        "malloc after free of same size must reuse the freed block"
    );
}

#[test]
fn free_null_is_a_noop() {
    let src = [
        MALLOC_PRELUDE,
        r#"
kmain: () {
    free(null)
    system.shutdown(0)
}
"#,
    ]
    .concat();
    let (_, exit) = run_kernel_hll(&src);
    assert_eq!(
        exit,
        Some(0),
        "free(null) must be a silent no-op and not crash"
    );
}

#[test]
fn malloc_large_allocation_succeeds() {
    let src = [
        MALLOC_PRELUDE,
        r#"
kmain: () {
    p: u8* = malloc(1024)
    if p == null {
        system.shutdown(1)
        return
    }
    system.shutdown(0)
}
"#,
    ]
    .concat();
    let (_, exit) = run_kernel_hll(&src);
    assert_eq!(
        exit,
        Some(0),
        "malloc(1024) must succeed within the 64 KiB heap"
    );
}

#[test]
fn malloc_exhaustion_returns_null() {
    let src = [
        MALLOC_PRELUDE,
        r#"
kmain: () {
    got_null: u64 = 0
    i: u64 = 0
    while i < 20 {
        p: u8* = malloc(4096)
        if p == null {
            got_null = 1
        }
        i = i + 1
    }
    if got_null == 0 {
        system.shutdown(1)
        return
    }
    system.shutdown(0)
}
"#,
    ]
    .concat();
    let (_, exit) = run_kernel_hll(&src);
    assert_eq!(
        exit,
        Some(0),
        "malloc must return null once the 64 KiB heap is exhausted"
    );
}

// --- console / UART output unit tests ---

// Guard a VA/PA mismatch: console_putchar via ecall with a stack buffer let M-mode
// read the user-virtual sp as physical; the fix writes the UART MMIO directly.

#[test]
fn freestanding_console_contains_no_ecall() {
    // Direct MMIO only: an ecall makes M-mode treat the caller's sp as physical.
    // HLL ';' comment lines are stripped so prose "ecall" mentions do not trip it.
    let non_comment: String = os_runtime::stdlib::FREESTANDING_CONSOLE
        .lines()
        .filter(|l| !l.trim_start().starts_with(';'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !non_comment.contains("ecall"),
        "freestanding console.hll must not use ecall as an instruction -- direct UART MMIO only"
    );
}

#[test]
fn freestanding_console_references_uart_mmio_address() {
    assert!(
        os_runtime::stdlib::FREESTANDING_CONSOLE.contains("0x10000000"),
        "freestanding console.hll must reference the NS16550A UART MMIO address 0x10000000"
    );
}

#[test]
fn freestanding_console_putchar_uses_store_byte() {
    // sb (store-byte) is the correct instruction for writing a single char to UART TX;
    // a wider store would corrupt adjacent UART registers.
    assert!(
        os_runtime::stdlib::FREESTANDING_CONSOLE.contains("sb a0"),
        "console_putchar must use 'sb a0' to write a single byte to the UART TX register"
    );
}

#[test]
fn console_putchar_writes_exact_bytes_to_uart() {
    let src = r#"
system := import("kernel/arch/riscv64/system.hll")
external console_putchar: (c: i32)
kmain: () {
    console_putchar(65)
    console_putchar(66)
    console_putchar(67)
    system.shutdown(0)
}
"#;
    let (uart, exit) = run_kernel_hll(src);
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains("ABC"),
        "console_putchar must produce the exact character value in UART; uart={uart:?}"
    );
}

#[test]
fn console_write_writes_exact_string_to_uart() {
    let src = r#"
system := import("kernel/arch/riscv64/system.hll")
external console_write: (str: u8*)
kmain: () {
    console_write("uart-check".ptr)
    system.shutdown(0)
}
"#;
    let (uart, exit) = run_kernel_hll(src);
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains("uart-check"),
        "console_write must produce the exact string in UART; uart={uart:?}"
    );
}

#[test]
fn console_print_int_produces_correct_decimal_digits() {
    // Guards the digit path: the old code handed a (possibly user-virtual)
    // stack slot address to M-mode via ecall, and M-mode read garbage.
    let src = r#"
system := import("kernel/arch/riscv64/system.hll")
external console_print_int: (n: i64)
kmain: () {
    console_print_int(0)
    console_print_int(42)
    console_print_int(-7)
    system.shutdown(0)
}
"#;
    let (uart, exit) = run_kernel_hll(src);
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains('0'),
        "console_print_int(0) must output '0'; uart={uart:?}"
    );
    assert!(
        uart.contains("42"),
        "console_print_int(42) must output '42'; uart={uart:?}"
    );
    assert!(
        uart.contains("-7"),
        "console_print_int(-7) must output '-7'; uart={uart:?}"
    );
}

#[test]
fn console_print_hex_produces_correct_hex_digits() {
    let src = r#"
system := import("kernel/arch/riscv64/system.hll")
external console_print_hex: (n: u64)
kmain: () {
    console_print_hex(255)
    console_print_hex(0)
    system.shutdown(0)
}
"#;
    let (uart, exit) = run_kernel_hll(src);
    assert_eq!(exit, Some(0), "kernel must exit cleanly; uart={uart:?}");
    assert!(
        uart.contains("0x00000000000000ff"),
        "console_print_hex(255) must output '0x00000000000000ff'; uart={uart:?}"
    );
    assert!(
        uart.contains("0x0000000000000000"),
        "console_print_hex(0) must output '0x0000000000000000'; uart={uart:?}"
    );
}
