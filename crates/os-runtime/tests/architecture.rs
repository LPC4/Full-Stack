use std::collections::{BTreeMap, BTreeSet};

type KernelModules = BTreeMap<String, (&'static str, &'static str)>;
type ImportGraph = BTreeMap<String, BTreeSet<String>>;

fn kernel_modules() -> KernelModules {
    let mut modules = BTreeMap::new();
    for (path, source) in os_runtime::SOURCE_TREE {
        let Some(relative) = path
            .strip_prefix("kernel/")
            .and_then(|path| path.strip_suffix(".hll"))
        else {
            continue;
        };
        let name = relative.rsplit('/').next().expect("kernel module name");
        assert!(
            modules.insert(name.to_owned(), (*path, *source)).is_none(),
            "duplicate kernel module stem `{name}`"
        );
    }
    modules
}

fn module_key(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .strip_suffix(".hll")
        .unwrap_or(path)
}

fn import_graph(modules: &KernelModules) -> ImportGraph {
    modules
        .iter()
        .map(|(name, (_, source))| {
            let imports = hll_to_ir::imports::collect_module_imports(source)
                .unwrap_or_else(|error| panic!("parse imports for `{name}`: {error}"))
                .into_iter()
                .map(|(_, path)| module_key(&path).to_owned())
                .filter(|dependency| modules.contains_key(dependency))
                .collect();
            (name.clone(), imports)
        })
        .collect()
}

fn reaches(graph: &ImportGraph, start: &str, target: &str) -> bool {
    let mut pending = vec![start];
    let mut visited = BTreeSet::new();
    while let Some(current) = pending.pop() {
        if !visited.insert(current) {
            continue;
        }
        for dependency in &graph[current] {
            if dependency == target {
                return true;
            }
            pending.push(dependency);
        }
    }
    false
}

fn import_cycles(graph: &ImportGraph) -> Vec<Vec<String>> {
    let mut cycles = Vec::new();
    let mut assigned = BTreeSet::new();
    for name in graph.keys() {
        if assigned.contains(name) {
            continue;
        }
        let component: Vec<String> = graph
            .keys()
            .filter(|candidate| reaches(graph, name, candidate) && reaches(graph, candidate, name))
            .cloned()
            .collect();
        if component.len() > 1 || graph[name].contains(name) {
            assigned.extend(component.iter().cloned());
            cycles.push(component);
        }
    }
    cycles
}

fn exports(source: &str) -> BTreeSet<String> {
    hll_to_ir::imports::collect_exports(source)
        .expect("parse exports")
        .into_iter()
        .collect()
}

fn names(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn generated_registry_uses_canonical_kernel_ids() {
    for (name, (path, source)) in kernel_modules() {
        assert_eq!(os_runtime::module_source(path), Some(source));
        assert_eq!(
            os_runtime::module_path(path),
            Some(format!("crates/os-runtime/{path}").as_str())
        );
        assert_eq!(
            os_runtime::module_source(&name),
            None,
            "legacy stem alias `{name}` must not resolve"
        );
    }
}

#[test]
fn production_kernel_has_no_import_cycles() {
    assert!(import_cycles(&import_graph(&kernel_modules())).is_empty());
}

#[test]
fn kernel_imports_use_canonical_registry_ids() {
    let modules = kernel_modules();
    for (name, (_, source)) in &modules {
        for (_, path) in hll_to_ir::imports::collect_module_imports(source).expect("parse imports")
        {
            if modules.contains_key(module_key(&path)) {
                assert!(
                    path.starts_with("kernel/") && path.ends_with(".hll"),
                    "kernel module `{name}` imports legacy ID `{path}`"
                );
                assert_eq!(
                    modules[module_key(&path)].0,
                    path,
                    "kernel module `{name}` imports a noncanonical path"
                );
            }
        }
    }
}

#[test]
fn internal_modules_have_approved_consumers() {
    let graph = import_graph(&kernel_modules());
    let consumers = |dependency: &str| -> BTreeSet<String> {
        graph
            .iter()
            .filter(|(_, dependencies)| dependencies.contains(dependency))
            .map(|(name, _)| name.clone())
            .collect()
    };

    assert_eq!(
        consumers("page_tables"),
        names(&["address_space", "user_memory"])
    );
    assert_eq!(consumers("file_syscalls"), names(&["syscalls"]));
    assert_eq!(consumers("io_syscalls"), names(&["syscalls"]));
    assert_eq!(consumers("memory_syscalls"), names(&["syscalls"]));
    assert_eq!(consumers("process_syscalls"), names(&["syscalls"]));
    assert_eq!(consumers("syscall_trace"), names(&["syscalls"]));
    assert_eq!(consumers("process"), names(&["tasks"]));
    assert_eq!(consumers("scheduler"), names(&["tasks"]));
    assert_eq!(consumers("fs_descriptors"), names(&["files"]));
    assert_eq!(
        consumers("fs_directory"),
        names(&["files", "fs_descriptors"])
    );
    assert_eq!(
        consumers("fs_allocator"),
        names(&["fs_descriptors", "fs_directory"])
    );
    assert_eq!(
        consumers("fs_image"),
        names(&["files", "fs_allocator", "fs_descriptors", "fs_directory"])
    );
    assert_eq!(
        consumers("fs_types"),
        names(&[
            "files",
            "fs_allocator",
            "fs_descriptors",
            "fs_directory",
            "fs_image",
        ])
    );
    assert_eq!(
        consumers("tasks"),
        names(&[
            "file_syscalls",
            "io_syscalls",
            "memory_syscalls",
            "my_kernel",
            "process_syscalls",
            "syscalls",
            "trap_handler",
        ])
    );
    assert_eq!(
        graph["trap_handler"],
        names(&[
            "csr",
            "interrupt_controller",
            "kernel_abi",
            "syscalls",
            "tasks",
            "timer",
            "uart",
        ])
    );
}

#[test]
fn guide_modules_have_approved_exports() {
    let modules = kernel_modules();
    assert_eq!(
        exports(modules["syscalls"].1),
        names(&["syscalls_dispatch"])
    );
    assert_eq!(
        exports(modules["files"].1),
        names(&[
            "files_close",
            "files_fd_size",
            "files_get_pos",
            "files_init",
            "files_mkdir",
            "files_open",
            "files_read",
            "files_readdir",
            "files_rename",
            "files_rmdir",
            "files_set_pos",
            "files_stat",
            "files_truncate",
            "files_unlink",
            "files_write",
        ])
    );
    assert_eq!(
        exports(modules["file_syscalls"].1),
        names(&[
            "file_syscalls_ftruncate",
            "file_syscalls_mkdir",
            "file_syscalls_open",
            "file_syscalls_readdir",
            "file_syscalls_rename",
            "file_syscalls_rmdir",
            "file_syscalls_stat",
            "file_syscalls_unlink",
        ])
    );
    assert_eq!(
        exports(modules["io_syscalls"].1),
        names(&[
            "io_syscalls_poll_key",
            "io_syscalls_read_character",
            "io_syscalls_write",
        ])
    );
    assert_eq!(
        exports(modules["tasks"].1),
        names(&[
            "tasks_block_on_input",
            "tasks_build",
            "tasks_build_empty",
            "tasks_current",
            "tasks_current_root",
            "tasks_enqueue",
            "tasks_exit_current",
            "tasks_has_children",
            "tasks_init",
            "tasks_kill_child",
            "tasks_kill_pid",
            "tasks_pid_alive",
            "tasks_reap_child",
            "tasks_reap_pid",
            "tasks_schedule",
            "tasks_wake_input",
        ])
    );
}

fn layer(path: &str) -> u8 {
    match path {
        "kernel/core/kernel_abi.hll" | "kernel/core/kernel_config.hll" => 0,
        path if path.starts_with("kernel/arch/") => 1,
        path if path.starts_with("kernel/mm/") => 2,
        path if path.starts_with("kernel/fs/") || path.starts_with("kernel/exec/") => 3,
        path if path.starts_with("kernel/sched/") => 4,
        path if path.starts_with("kernel/syscall/") => 5,
        path if path.starts_with("kernel/core/") || path.starts_with("kernel/trap/") => 6,
        _ => panic!("unclassified kernel module {path}"),
    }
}

fn upward_imports(modules: &KernelModules, graph: &ImportGraph) -> Vec<(String, String)> {
    graph
        .iter()
        .flat_map(|(name, dependencies)| {
            dependencies.iter().filter_map(move |dependency| {
                (layer(modules[name].0) < layer(modules[dependency].0))
                    .then(|| (name.clone(), dependency.clone()))
            })
        })
        .collect()
}

#[test]
fn kernel_dependencies_point_down_the_layers() {
    let modules = kernel_modules();
    assert_eq!(upward_imports(&modules, &import_graph(&modules)), vec![]);
}

#[test]
fn architecture_gates_detect_cycles_and_upward_imports() {
    let modules = kernel_modules();
    let mut graph = import_graph(&modules);
    graph
        .get_mut("user_memory")
        .unwrap()
        .insert("tasks".to_owned());
    assert_eq!(
        upward_imports(&modules, &graph),
        vec![("user_memory".to_owned(), "tasks".to_owned())]
    );
    graph
        .get_mut("tasks")
        .unwrap()
        .insert("user_memory".to_owned());
    assert!(!import_cycles(&graph).is_empty());
}

#[test]
fn facade_gate_detects_an_exported_helper() {
    let source = kernel_modules()["syscalls"].1;
    let leaked = format!("{source}\nexport accidental_helper: () {{}}\n");
    assert_ne!(exports(&leaked), names(&["syscalls_dispatch"]));
}

#[test]
fn public_functions_precede_private_implementation() {
    for (name, (_, source)) in kernel_modules() {
        assert!(
            late_public_functions(source).is_empty(),
            "{name}: public API follows a helper"
        );
    }
}

fn late_public_functions(source: &str) -> Vec<String> {
    let mut implementation_started = false;
    hll_to_ir::imports::collect_function_definitions(source)
        .expect("parse kernel functions")
        .into_iter()
        .filter_map(|(function, exported)| {
            if exported || function == "kmain" {
                implementation_started.then_some(function)
            } else {
                implementation_started = true;
                None
            }
        })
        .collect()
}

#[test]
fn function_order_gate_allows_state_but_rejects_early_helpers() {
    assert!(late_public_functions("state: u64 = 0\nexport api: () {}\nhelper: () {}").is_empty());
    assert_eq!(
        late_public_functions("helper: () {}\nexport api: () {}"),
        vec!["api"]
    );
}
