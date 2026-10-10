// Each kernel module compiles in isolation, so `external` declarations resolve
// at link time. Only a truly undefined identifier fails here.
use full_stack::compilation_pipeline::{CompilationPipeline, TargetMode};
use hll_to_ir::stdlib::get_stdlib_modules_for_mode;

fn kernel_hll_modules() -> Vec<(String, &'static str)> {
    os_runtime::SOURCE_TREE
        .iter()
        .filter_map(|(path, source)| {
            let name = path
                .strip_prefix("kernel/")?
                .strip_suffix(".hll")?
                .rsplit('/')
                .next()?;
            Some((name.to_owned(), *source))
        })
        .collect()
}

// Compile one kernel HLL source as a standalone module and assert it succeeds.
fn assert_kernel_module_compiles(name: &str, source: &str) {
    eprintln!("compiling kernel module {name}");
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
    p.compile_modules(&modules)
        .unwrap_or_else(|e| panic!("kernel stdlib modules failed to compile:\n{e}"));
}

#[test]
fn kernel_stdlib_full_bundle_compiles() {
    CompilationPipeline::compile_stdlib_objects(TargetMode::Kernel)
        .unwrap_or_else(|e| panic!("kernel stdlib failed to compile:\n{e}"));
}

#[test]
fn all_kernel_hll_modules_compile() {
    for (name, source) in kernel_hll_modules() {
        if !hll_to_ir::imports::defines_code(source).expect("parse kernel module") {
            continue;
        }
        assert_kernel_module_compiles(&name, source);
    }
}

#[derive(Debug, PartialEq, Eq)]
struct RecordLayout {
    size: usize,
    alignment: usize,
    fields: Vec<(String, usize)>,
}

fn record_layout(source: &str, name: &str) -> RecordLayout {
    use hll_to_ir::ir::{IrType, type_layout};

    // Layout is target-independent. Hosted mode avoids reinjecting the kernel ABI.
    let mut pipeline = CompilationPipeline::new();
    pipeline.set_write_artifacts(false);
    let program = pipeline
        .compile(source)
        .expect("compile kernel record")
        .ir_program;
    let aliases = program
        .type_aliases
        .into_iter()
        .map(|alias| (alias.name, alias.ty))
        .collect::<std::collections::HashMap<_, _>>();
    let record = aliases
        .get(name)
        .unwrap_or_else(|| panic!("missing record {name}"));
    let IrType::Aggregate(fields) = record else {
        panic!("{name} is not a record");
    };
    let layout = type_layout(record, &aliases).expect("kernel record layout");
    let mut offset = 0usize;
    let fields = fields
        .iter()
        .map(|(field, ty)| {
            let layout = type_layout(ty, &aliases).expect("kernel field layout");
            offset = offset.next_multiple_of(layout.alignment);
            let field = (field.clone(), offset);
            offset += layout.size;
            field
        })
        .collect();
    RecordLayout {
        size: layout.size,
        alignment: layout.alignment,
        fields,
    }
}

fn expected_record(size: usize, alignment: usize, fields: &[(&str, usize)]) -> RecordLayout {
    RecordLayout {
        size,
        alignment,
        fields: fields
            .iter()
            .map(|(name, offset)| ((*name).to_owned(), *offset))
            .collect(),
    }
}

#[test]
fn pcb_and_trap_frame_struct_layout_matches_offsets() {
    let source = os_runtime::module_source("kernel/core/kernel_abi.hll").unwrap();
    let mut trap_fields: Vec<_> = (0..32)
        .map(|index| (format!("x{index}"), index * 8))
        .collect();
    trap_fields.extend([
        ("sepc".to_owned(), 256),
        ("scause".to_owned(), 264),
        ("stval".to_owned(), 272),
        ("sstatus".to_owned(), 280),
    ]);
    assert_eq!(
        record_layout(source, "TrapFrame"),
        RecordLayout {
            size: 288,
            alignment: 8,
            fields: trap_fields,
        }
    );
    assert_eq!(
        record_layout(source, "Pcb"),
        expected_record(
            384,
            8,
            &[
                ("pid", 0),
                ("state", 8),
                ("next", 16),
                ("user_stack_pa", 24),
                ("entry_pc", 32),
                ("frame", 40),
                ("page_root", 328),
                ("parent_pid", 336),
                ("exit_code", 344),
                ("stdout_fd", 352),
                ("stdin_fd", 360),
                ("fb_mapped", 368),
                ("heap_brk", 376),
            ]
        )
    );
}

#[test]
fn filesystem_struct_layout_matches_disk_format() {
    let source = os_runtime::module_source("kernel/fs/fs_types.hll").unwrap();
    assert_eq!(
        record_layout(source, "DiskInode"),
        expected_record(
            128,
            4,
            &[
                ("node_type", 0),
                ("parent", 2),
                ("size", 4),
                ("name", 8),
                ("blocks", 40),
            ]
        )
    );
    assert_eq!(
        record_layout(source, "DiskDirEntry"),
        expected_record(36, 2, &[("name", 0), ("inode", 32), ("reserved", 34),])
    );
    assert_eq!(
        record_layout(source, "OpenFile"),
        expected_record(
            32,
            8,
            &[("valid", 0), ("inode", 8), ("flags", 16), ("pos", 24),]
        )
    );
}

#[test]
fn syscall_trace_entry_layout_matches_inspector_format() {
    let source = os_runtime::module_source("kernel/syscall/syscall_trace.hll").unwrap();
    assert_eq!(
        record_layout(source, "TraceEntry"),
        expected_record(
            80,
            8,
            &[
                ("seq", 0),
                ("pid", 8),
                ("num", 16),
                ("a0", 24),
                ("a1", 32),
                ("a2", 40),
                ("a3", 48),
                ("a4", 56),
                ("a5", 64),
                ("result", 72),
            ]
        )
    );
}

#[test]
fn kernel_layout_gate_detects_size_and_field_order_changes() {
    let source = os_runtime::module_source("kernel/core/kernel_abi.hll").unwrap();
    let original = record_layout(source, "Pcb");
    let enlarged = source.replace("heap_brk: u64,", "heap_brk: u64[2],");
    assert_ne!(record_layout(&enlarged, "Pcb").size, original.size);
    let reordered = source.replace("pid: u64,\n    state: u64,", "state: u64,\n    pid: u64,");
    let changed = record_layout(&reordered, "Pcb");
    assert_eq!(changed.size, original.size);
    assert_ne!(changed.fields, original.fields);
}

// --- Frame-size guard ---
// Every block-local gets its own slot, so a big function can push a frame past
// the RV immediate range and panic the assembler (syscall_dispatch hit this).

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

    let mut sources: Vec<(String, String)> = get_stdlib_modules_for_mode(TargetMode::Kernel)
        .iter()
        .map(|(n, s)| ((*n).to_owned(), (*s).to_owned()))
        .collect();
    sources.extend(
        kernel_hll_modules()
            .into_iter()
            .filter_map(|(name, source)| {
                hll_to_ir::imports::defines_code(source)
                    .expect("parse kernel module")
                    .then(|| (name, source.to_owned()))
            }),
    );

    let mut offenders: Vec<(String, u64)> = Vec::new();
    let mut peak = (String::new(), 0);
    let mut parsed_any = false;
    for (name, source) in &sources {
        let ir = p
            .compile(source)
            .unwrap_or_else(|e| panic!("{name} failed to compile:\n{e}"))
            .ir_program;
        let asm = p.compile_ir_to_assembly(&ir).expect("valid IR layout");
        for (func, size) in frame_sizes(&asm) {
            parsed_any = true;
            if size > peak.1 {
                peak = (format!("{name}::{func}"), size);
            }
            if size > FRAME_WARN_BYTES {
                offenders.push((func, size));
            }
        }
    }

    eprintln!("peak kernel frame: {} bytes ({})", peak.1, peak.0);

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

#[test]
fn kernel_manifest_preserves_flat_abi_edges_and_boots() {
    use full_stack::build::{BuildExecutor, BuildManifest};
    use virtual_machine::virtual_machine::{RunOutcome, VirtualMachine};

    let manifest = BuildManifest::from_file(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("crates/os-runtime/kernel/kernel.build"),
    )
    .expect("kernel manifest");
    let started = std::time::Instant::now();
    let artifacts = BuildExecutor::build(&manifest).expect("build kernel");
    let build_time = started.elapsed();
    for symbol in ["_kernel_start", "kmain", "_s_trap_host", "trap_handler"] {
        assert!(
            artifacts.linked.has_symbol(symbol),
            "missing ABI edge {symbol}"
        );
    }
    let mut machine = VirtualMachine::new_kernel(&artifacts.linked);
    let run = machine.run(5_000_000);
    assert!(
        matches!(run.termination, RunOutcome::Halted(0)),
        "{:?}",
        run.termination
    );
    assert!(run.uart_output.contains("no user binary present"));
    eprintln!(
        "kernel build: {build_time:?}, {} objects, {} flat bytes, {} no-user boot steps",
        artifacts.units.len(),
        artifacts.linked.to_flat_binary().len(),
        run.steps,
    );
}

#[test]
fn kernel_module_closure_matches_generated_sources() {
    use std::collections::BTreeSet;

    let modules = get_stdlib_modules_for_mode(TargetMode::Kernel);
    assert!(
        modules.len() >= 8,
        "expected at least 8 kernel stdlib modules, got {}",
        modules.len()
    );

    let kernel_objs = CompilationPipeline::compile_kernel_module_objects()
        .expect("kernel module closure compile");
    let my_kernel = kernel_objs
        .iter()
        .find(|(name, _)| name == "my_kernel")
        .expect("kernel root object");
    assert!(my_kernel.1.has_symbol("kmain"));
    assert!(my_kernel.1.has_symbol("my_kernel__boot_console"));
    assert!(!my_kernel.1.has_symbol("boot_console"));

    let trap = kernel_objs
        .iter()
        .find(|(name, _)| name == "kernel/trap/trap_handler.hll")
        .expect("trap handler object");
    assert!(trap.1.has_symbol("trap_handler"));
    assert!(trap.1.has_symbol("trap_handler__kernel_tick"));

    let actual: BTreeSet<String> = kernel_objs.into_iter().map(|(name, _)| name).collect();
    let expected: BTreeSet<String> = os_runtime::SOURCE_TREE
        .iter()
        .filter_map(|(path, source)| {
            if !path.starts_with("kernel/") || !path.ends_with(".hll") {
                return None;
            }
            hll_to_ir::imports::defines_code(source)
                .expect("parse kernel module")
                .then(|| {
                    if *path == "kernel/core/my_kernel.hll" {
                        "my_kernel".to_owned()
                    } else {
                        (*path).to_owned()
                    }
                })
        })
        .collect();

    assert_eq!(
        actual, expected,
        "kernel closure does not match source registry"
    );
}
