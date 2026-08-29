use super::*;
use egui_kittest::{Harness, kittest::Queryable as _};
use full_stack::workspace::MemoryFs;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn demo_workspace_uses_the_application_storage_directory() {
    let storage = std::path::PathBuf::from("app-data");
    assert_eq!(
        demo_workspace_dir(&storage),
        std::path::PathBuf::from("app-data/workspaces/demo")
    );
}

// A hosted preview must resolve `_start`->`main`, and a kernel preview must link
// against the kernel stdlib (`_kernel_start`/`kmain`) with no phantom undefined `main`.
#[test]
fn preview_links_hosted_and_kernel_without_phantom_main() {
    let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));

    let hosted = BuildManifest::hosted("prev_hosted", "main: () -> i32 { return 0 }");
    let hosted_bin = app
        .build_preview_binary(&hosted, "prev_hosted.hll", "main: () -> i32 { return 0 }")
        .expect("hosted preview links");
    assert!(hosted_bin.assembled.has_symbol("main"));

    let kernel_build =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/os-runtime/kernel/kernel.build");
    let manifest = BuildManifest::from_file(&kernel_build).expect("kernel manifest");
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/os-runtime/kernel/core/my_kernel.hll");
    let source = fs::read_to_string(&root).expect("read kernel root");
    let kernel_bin = app
        .build_preview_binary(&manifest, &root.display().to_string(), &source)
        .expect("kernel preview links without phantom main");
    assert!(kernel_bin.assembled.has_symbol("kmain"));
}

#[test]
fn open_native_workspace_replaces_tree_configs_and_transient_state() {
    let root = temp_root("open_workspace_replaces");
    write_native_workspace(&root);
    let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
    app.vm_result = Some(VmExecutionResult {
        uart_output: "old".to_owned(),
        exit_code: Some(0),
        fault: None,
        steps: 1,
        max_steps_reached: false,
        registers: None,
    });
    app.machine_texture_fill_count = Some(7);
    app.manifest_cache = Some((1.0, vec![VPath::parse("old.build").expect("path")]));
    app.quick_open = Some(quick_open::QuickOpenState::default());
    app.find.request_open();
    app.launch_mode = LaunchMode::Debug;
    app.selected_config = 12;

    app.open_native_workspace_folder(&root);

    assert_eq!(app.configs().len(), 1);
    assert_eq!(app.selected_config, 0);
    assert_eq!(
        app.active_path().expect("active root").as_str(),
        "programs/demo/main.hll"
    );
    assert!(
        app.workspace
            .tree()
            .flatten()
            .iter()
            .any(|entry| entry.path.as_str() == "programs/demo/demo.build")
    );
    assert!(app.vm_result.is_none());
    assert!(app.machine_texture.is_none());
    assert!(app.machine_texture_fill_count.is_none());
    assert!(app.manifest_cache.is_none());
    assert!(app.quick_open.is_none());
    assert!(!app.find.open);
    assert_eq!(app.launch_mode, LaunchMode::Run);
    assert!(app.status_message.expect("status").starts_with("opened "));

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn reopen_last_workspace_restores_the_folder_only_when_enabled() {
    let root = temp_root("reopen_last");
    write_native_workspace(&root);
    let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));

    // Opening a folder records it as the last workspace. The stored path is
    // the canonicalized root, so match on the unique temp-folder name.
    app.open_native_workspace_folder(&root);
    let label = app.settings.last_workspace.clone().expect("last recorded");
    let dir_name = root.file_name().expect("dir name").to_string_lossy();
    assert!(label.contains(dir_name.as_ref()), "stored path was {label}");

    // Off (the default): startup falls back to the bundled sample workspace.
    let mut settings = settings::AppSettings {
        last_workspace: Some(label.clone()),
        ..settings::AppSettings::default()
    };
    assert!(reopen_last_workspace(&settings).is_none());

    // On with a live path: startup reopens that folder's configs.
    settings.reopen_last_workspace = true;
    let reopened = reopen_last_workspace(&settings).expect("reopened");
    assert_eq!(reopened.run_configs().configs.len(), 1);

    // On but the path is gone: fall back rather than fail startup.
    fs::remove_dir_all(&root).expect("cleanup");
    assert!(reopen_last_workspace(&settings).is_none());
}

#[test]
fn open_native_workspace_blocks_when_documents_are_dirty() {
    let root = temp_root("open_workspace_dirty");
    write_native_workspace(&root);
    let mut fs = MemoryFs::new();
    let old_path = VPath::parse("old/main.hll").expect("path");
    fs.write_text(&old_path, "old").expect("write old");
    let mut app = test_app(IdeWorkspace::new(Box::new(fs)).expect("workspace"));
    app.workspace
        .open_document(old_path.clone())
        .expect("open old");
    app.workspace.set_active_text("dirty").expect("edit old");

    app.open_native_workspace_folder(&root);

    assert_eq!(
        app.status_message.as_deref(),
        Some("save or close unsaved files before opening a folder")
    );
    assert_eq!(
        app.active_path().expect("active old").as_str(),
        old_path.as_str()
    );
    assert_eq!(app.workspace.dirty_documents().count(), 1);
    assert!(app.configs().is_empty());

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn dirty_close_confirmation_blocks_discard_until_confirmed() {
    let mut fs = MemoryFs::new();
    let path = VPath::parse("main.hll").expect("path");
    fs.write_text(&path, "main: () -> i32 { return 0 }")
        .expect("write source");
    let mut app = test_app(IdeWorkspace::new(Box::new(fs)).expect("workspace"));
    app.workspace
        .open_document(path.clone())
        .expect("open source");
    app.workspace
        .set_active_text("main: () -> i32 { return 1 }")
        .expect("edit source");
    app.settings.confirm_discard_on_close = true;

    app.close_path(&path);

    assert_eq!(app.pending_close.as_ref(), Some(&path));
    assert!(app.workspace.document(&path).is_some());

    app.close_path_confirmed(&path);

    assert!(app.pending_close.is_none());
    assert!(app.workspace.document(&path).is_none());
}

#[test]
fn open_terminal_on_run_setting_can_keep_terminal_closed() {
    let root = temp_root("terminal_closed");
    write_native_workspace(&root);
    let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
    app.open_native_workspace_folder(&root);
    app.settings.open_terminal_on_run = false;

    app.run_selected_config(1.0);

    assert!(!app.terminal_open);
    assert!(app.live.is_some(), "run still starts a machine session");

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn run_ignores_debug_defaults_and_persisted_breakpoints() {
    let root = temp_root("run_without_debugger");
    write_native_workspace(&root);
    let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
    app.open_native_workspace_folder(&root);
    let config = &mut app.workspace.run_configs_mut().configs[0];
    config.debug.enabled = true;
    config.debug.breakpoints.push("main".to_owned());

    app.run_selected_config(1.0);

    let live = app.live.as_ref().expect("run starts a session");
    assert_eq!(live.state, MachineState::Running);
    assert!(live.breakpoints.is_empty());
    assert_eq!(app.launch_mode, LaunchMode::Run);
    // No run surface: the canvas keeps the document and the console lands in
    // the bottom terminal's Machine tab.
    assert_eq!(app.main_surface, MainSurface::Document);
    assert!(!app.debugger_open);
    assert!(matches!(app.terminal.active, TerminalKind::Machine));

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn restart_reuses_the_last_build_until_sources_change() {
    let root = temp_root("restart_reuse");
    write_native_workspace(&root);
    let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
    app.open_native_workspace_folder(&root);

    app.run_selected_config(1.0);
    assert!(app.launch_cache.is_some(), "first run fills the cache");
    assert_eq!(drive_app_to_exit(&mut app), 0);

    app.restart_selected_config(2.0);
    let output = app.terminal.output_text();
    assert!(
        output.contains("Sources unchanged; restarting from the last build."),
        "restart reused the build: {output}"
    );
    assert!(!output.contains("Building run target."));
    assert_eq!(drive_app_to_exit(&mut app), 0, "cached artifacts still run");

    fs::write(
        root.join("programs/demo/main.hll"),
        "main: () -> i32 { return 9 }",
    )
    .expect("edit source");
    app.restart_selected_config(3.0);
    let output = app.terminal.output_text();
    assert!(
        output.contains("Building run target."),
        "source edit forces a rebuild: {output}"
    );
    assert_eq!(drive_app_to_exit(&mut app), 9, "rebuild picked up the edit");

    fs::remove_dir_all(root).expect("cleanup");
}

// Debug owns a separate surface. Closing it drops back to the document while
// the session stays alive.
#[test]
fn debug_tab_focuses_switches_and_closes() {
    let root = temp_root("debug_tab_lifecycle");
    write_native_workspace(&root);
    let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
    app.open_native_workspace_folder(&root);

    app.debug_selected_config(1.0);
    assert!(app.live.is_some(), "debug launch starts a session");
    assert!(app.debugger_focused());
    assert!(app.debugger_open);
    assert_eq!(app.session_tab_label.as_deref(), Some("main.hll"));

    app.document_focus_changed();
    assert!(!app.debugger_focused(), "file tab takes the canvas back");
    assert!(app.live.is_some(), "switching tabs keeps the session");

    app.focus_debugger();
    assert!(app.debugger_focused());

    app.close_debugger_surface();
    assert!(app.live.is_some(), "closing the debugger keeps the session");
    assert_eq!(
        app.main_surface,
        MainSurface::Document,
        "the document takes the canvas back"
    );
    assert!(!app.debugger_open);

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn instruction_mode_retires_one_instruction_and_enters_calls() {
    let root = temp_root("instruction_step");
    write_native_workspace(&root);
    let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
    app.open_native_workspace_folder(&root);
    app.debug_selected_config(1.0);

    assert_eq!(app.step_count, 1, "single-step is the default");
    app.step_unit = machine_dock::StepUnit::Instruction;
    let call_pc = {
        let live = app.live.as_mut().expect("debug session");
        let mut call_pc = None;
        for _ in 0..128 {
            let pc = live.registers().pc;
            let bytes = live.read_virtual(pc, 4);
            let word = u32::from_le_bytes(bytes.try_into().expect("instruction word"));
            if matches!(
                virtual_machine::cpu::decoder::decode(word),
                Ok(
                    virtual_machine::cpu::decoder::DecodedInsn::Jal { rd: 1, .. }
                        | virtual_machine::cpu::decoder::DecodedInsn::Jalr { rd: 1, .. }
                )
            ) {
                call_pc = Some(pc);
                break;
            }
            live.step_in();
        }
        call_pc.expect("hosted entry calls main")
    };
    let retired = app
        .live
        .as_ref()
        .expect("debug session")
        .vm()
        .insns_retired();

    app.step_selected_live_machine();

    let live = app.live.as_ref().expect("debug session");
    assert_eq!(live.vm().insns_retired(), retired + 1);
    assert_ne!(
        live.registers().pc,
        call_pc + 4,
        "instruction mode entered the call instead of stepping over it"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn stopping_a_debug_session_hides_its_surface() {
    let root = temp_root("debug_stop_surface");
    write_native_workspace(&root);
    let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
    app.open_native_workspace_folder(&root);

    app.debug_selected_config(1.0);
    assert!(app.debugger_focused());
    assert!(app.debugger_dock_visible());

    app.stop_live_machine();

    assert_eq!(
        app.live.as_ref().expect("retained session").state,
        MachineState::Stopped
    );
    assert!(!app.debugger_open);
    assert!(!app.debugger_dock_visible());
    assert_eq!(app.main_surface, MainSurface::Document);

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn run_brings_the_machine_console_forward_without_a_surface() {
    let root = temp_root("run_machine_console");
    write_native_workspace(&root);
    let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
    app.open_native_workspace_folder(&root);

    app.run_selected_config(1.0);
    assert!(app.live.is_some());
    assert_eq!(app.main_surface, MainSurface::Document);
    assert!(matches!(app.terminal.active, TerminalKind::Machine));

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn framebuffer_run_keeps_console_and_graphics_in_the_ide() {
    use full_stack::runconfig::PresentMode;

    let root = temp_root("framebuffer_run_surface");
    write_native_workspace(&root);
    let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
    app.open_native_workspace_folder(&root);
    app.workspace.run_configs_mut().configs[0].run.present = PresentMode::Framebuffer;

    app.run_selected_config(1.0);

    // The document stays behind the embedded screen while the terminal remains usable.
    assert_eq!(app.main_surface, MainSurface::Document);
    assert!(app.config_has_screen());
    assert!(matches!(app.terminal.active, TerminalKind::Machine));
    assert!(!app.debugger_open);
    assert_eq!(app.launch_mode, LaunchMode::Run);

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn kittest_debug_tabs_focus_close_and_keep_the_session() {
    let root = temp_root("kittest_debug_tabs");
    write_native_workspace(&root);
    let app_root = root.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(move |cc| {
            theme::apply(&cc.egui_ctx);
            let mut app =
                test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
            app.open_native_workspace_folder(&app_root);
            app.debug_selected_config(1.0);
            app
        });

    assert!(harness.state().debugger_focused());

    // A file tab takes the canvas back; the debug tab returns focus to it.
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "main.hll")
        .click();
    harness.step();
    assert!(!harness.state().debugger_focused());

    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "[debug] main.hll")
        .click();
    harness.step();
    assert!(harness.state().debugger_focused());

    harness.get_by_label("close [debug] main.hll").click();
    harness.step();
    assert!(harness.state().live.is_some());
    assert!(!harness.state().debugger_open);
    assert_eq!(harness.state().main_surface, MainSurface::Document);

    drop(harness);
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn kittest_debug_dock_steps_and_source_gutter_toggles_breakpoint() {
    let root = temp_root("kittest_debug_controls");
    write_native_workspace(&root);
    let app_root = root.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(move |cc| {
            theme::apply(&cc.egui_ctx);
            let mut app =
                test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
            app.open_native_workspace_folder(&app_root);
            app.debug_selected_config(1.0);
            app
        });

    let before = harness.state().live.as_ref().expect("session").steps;
    harness.get_by_label("Step one instruction (F10)").click();
    harness.step();
    assert!(harness.state().live.as_ref().expect("session").steps > before);

    let before = harness
        .state()
        .live
        .as_ref()
        .expect("session")
        .breakpoints
        .len();
    harness
        .get_by_label("toggle breakpoint line 6")
        .click_accesskit();
    harness.step();
    assert_eq!(
        harness
            .state()
            .live
            .as_ref()
            .expect("session")
            .breakpoints
            .len(),
        before + 1
    );

    drop(harness);
    fs::remove_dir_all(root).expect("cleanup");
}

// Debug launch lands the left tool on Inspect > Call Stack, where the
// paused entry frame renders as a clickable row.
#[test]
fn kittest_inspector_call_stack_lists_the_paused_frame() {
    let root = temp_root("kittest_call_stack");
    write_native_workspace(&root);
    let app_root = root.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(move |cc| {
            theme::apply(&cc.egui_ctx);
            let mut app =
                test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
            app.open_native_workspace_folder(&app_root);
            app.debug_selected_config(1.0);
            app
        });

    assert_eq!(harness.state().tool, Tool::Inspect);
    assert_eq!(
        harness.state().inspector.pane,
        inspector::InspectorPane::CallStack
    );
    let expected = {
        let live = harness.state().live.as_ref().expect("session");
        let pc = live.registers().pc;
        let name = live
            .symbol_near(pc)
            .map(|(name, _)| name.to_owned())
            .expect("symbol at the entry pc");
        format!("frame 0 {}", inspector::frame_symbol(&name))
    };
    harness.step();
    harness.get_by_label(expected.as_str());

    drop(harness);
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn kittest_output_link_jumps_to_the_diagnostic_source() {
    let line = terminal::linked_test_line(
        "  --> programs/demo/main.hll:2:12",
        "programs/demo/main.hll",
        2,
    );
    let mut harness = Harness::new_ui_state(
        move |ui, jump| {
            if let Some(clicked) = terminal::console_line(ui, &line) {
                *jump = Some(clicked);
            }
        },
        None,
    );
    harness.get_by_role(egui::accesskit::Role::Link).click();
    harness.run();
    assert_eq!(
        harness
            .state()
            .as_ref()
            .map(|(path, line)| (path.as_str(), *line)),
        Some(("programs/demo/main.hll", 2))
    );
}

#[test]
fn restart_reuse_setting_off_forces_rebuild() {
    let root = temp_root("restart_reuse_off");
    write_native_workspace(&root);
    let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
    app.open_native_workspace_folder(&root);

    app.run_selected_config(1.0);
    assert!(app.launch_cache.is_some());
    assert_eq!(drive_app_to_exit(&mut app), 0);

    app.settings.restart_reuses_build = false;
    app.restart_selected_config(2.0);

    let output = app.terminal.output_text();
    assert!(
        output.contains("Building run target."),
        "restart should rebuild when reuse is disabled: {output}"
    );
    assert!(
        !output.contains("Sources unchanged; restarting from the last build."),
        "reuse message should not appear when disabled: {output}"
    );

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn persist_breakpoints_setting_off_leaves_run_config_unchanged() {
    let root = temp_root("persist_breakpoints_off");
    write_native_workspace(&root);
    let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
    app.open_native_workspace_folder(&root);
    app.settings.persist_breakpoints = false;

    app.debug_selected_config(1.0);
    let live = app.live.as_mut().expect("live session");
    let main = live.resolve_breakpoint("main").expect("main symbol");
    live.breakpoints.insert(main);
    app.persist_live_breakpoints();

    assert!(
        app.selected_config()
            .expect("selected config")
            .debug
            .breakpoints
            .is_empty()
    );

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn idle_check_compiles_the_dirty_buffer_without_saving() {
    let root = temp_root("idle_check");
    write_native_workspace(&root);
    let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
    app.open_native_workspace_folder(&root);
    app.workspace
        .open_document(VPath::parse("programs/demo/main.hll").expect("path"))
        .expect("open");
    app.workspace
        .set_active_text("main: () -> i32 {\n    return )\n}\n")
        .expect("edit");

    let ctx = egui::Context::default();
    app.poll_idle_check(&ctx, 0.0);
    assert!(app.stage_cache.is_none(), "debounce holds until idle");
    app.poll_idle_check(&ctx, 2.0);
    let cache = app.stage_cache.as_ref().expect("idle check ran");
    assert!(cache.result.has_errors(), "unsaved parse error detected");

    // The same edit is not rechecked; a new edit re-arms the debounce.
    app.stage_cache = None;
    app.poll_idle_check(&ctx, 4.0);
    assert!(app.stage_cache.is_none(), "unchanged buffer not rechecked");

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn build_failure_marks_the_editor_and_links_output() {
    let root = temp_root("diag_markers");
    write_native_workspace(&root);
    fs::write(
        root.join("programs/demo/main.hll"),
        "main: () -> i32 {\n    return )\n}\n",
    )
    .expect("break source");
    let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
    app.open_native_workspace_folder(&root);

    app.build_selected_config(1.0);
    assert_eq!(app.status_message.as_deref(), Some("build failed"));
    let text = app
        .workspace
        .active_document()
        .expect("active doc")
        .text()
        .to_owned();
    let marks = app.active_diagnostic_marks(&text);
    assert!(!marks.is_empty(), "parse error produced editor marks");
    assert!(marks[0].error);
    assert_eq!(marks[0].line, 2, "span points at the broken line");
    let output = app.terminal.output_text();
    assert!(
        output.contains("programs/demo/main.hll:2:"),
        "output line carries the location: {output}"
    );
    assert!(
        output.contains("return )") && output.contains('^'),
        "output shows the rendered block with source line and caret: {output}"
    );
    assert!(
        !output.contains("Freestanding errors:"),
        "the stringified compile error does not repeat the block: {output}"
    );

    // An Output link click routes through jump_to_source without reopening
    // the already-active file (the stage cache and marks must survive).
    app.jump_to_source("programs/demo/main.hll", 2);
    assert_eq!(
        app.pending_jump
            .as_ref()
            .map(|(path, line)| (path.as_str(), *line)),
        Some(("programs/demo/main.hll", 2))
    );
    assert!(!app.active_diagnostic_marks(&text).is_empty());

    fs::remove_dir_all(root).expect("cleanup");
}

// A build without a run must leave artifacts inspectable, and a pane that
// lost its backing state must fall back to Overview instead of going dead.
#[test]
fn build_retains_artifacts_and_inspector_falls_back() {
    let root = temp_root("inspector_last_build");
    write_native_workspace(&root);
    let mut app = test_app(IdeWorkspace::new(Box::new(MemoryFs::new())).expect("workspace"));
    app.open_native_workspace_folder(&root);
    assert!(app.last_build.is_none());

    app.build_selected_config(1.0);
    let build = app.last_build.as_ref().expect("build retained");
    assert!(build.artifacts.linked.has_symbol("main"));

    app.inspector.pane = inspector::InspectorPane::Processes;
    app.ensure_inspector_pane_available();
    assert_eq!(app.inspector.pane, inspector::InspectorPane::Overview);
    app.inspector.pane = inspector::InspectorPane::Artifacts;
    app.ensure_inspector_pane_available();
    assert_eq!(app.inspector.pane, inspector::InspectorPane::Artifacts);

    fs::remove_dir_all(root).expect("cleanup");
}

fn drive_app_to_exit(app: &mut FullStackApp) -> i32 {
    let live = app.live.as_mut().expect("live session");
    for _ in 0..1_000 {
        match live.state {
            MachineState::Exited(code) => return code,
            MachineState::Running => {
                live.drive(10_000, std::time::Duration::from_secs(5));
            }
            _ => break,
        }
    }
    panic!("session did not exit: {:?}", live.state);
}

#[test]
fn config_from_current_file_writes_manifest_and_selects_config() {
    let mut fs = MemoryFs::new();
    fs.write_text(
        &VPath::parse("lesson/loop.s").expect("path"),
        ".globl _start\n_start:\n  li a0, 0\n",
    )
    .expect("write source");
    let mut app = test_app(IdeWorkspace::new(Box::new(fs)).expect("workspace"));
    app.open_path(VPath::parse("lesson/loop.s").expect("path"));

    app.add_config_for_active_file();

    let manifest = app
        .workspace
        .fs()
        .read_file(&VPath::parse("lesson/loop.build").expect("path"))
        .expect("manifest written");
    let manifest = String::from_utf8(manifest).expect("utf-8 manifest");
    assert!(manifest.contains("target = \"freestanding\""));
    assert!(manifest.contains("entry = \"_start\""));
    let config = app.selected_config().expect("config selected");
    assert_eq!(config.kind, RunKind::Freestanding);

    // A second click reselects the existing config instead of duplicating it.
    app.add_config_for_active_file();
    assert_eq!(app.configs().len(), 1);
}

fn test_app(workspace: IdeWorkspace) -> FullStackApp {
    let mut app = FullStackApp::with_workspace(workspace, settings::AppSettings::default());
    app.build_glow_start = Some(1.0);
    app.build_lit_until = Some(Layer::Elf);
    app.cursor_pos = Some((1, 1));
    app.step_unit = machine_dock::StepUnit::Instruction;
    app.inspector = inspector::InspectorState {
        pane: inspector::InspectorPane::Trace,
        pid: Some(1),
        inode: Some(2),
        ..Default::default()
    };
    app
}

#[test]
fn repository_address_state_drops_stale_lines_and_non_repository_files() {
    let mut fs = MemoryFs::new();
    let demo = VPath::parse("demo.hll").expect("path");
    fs.write_text(&demo, "main: () -> i32 { return 0 }")
        .expect("write");
    let mut workspace = IdeWorkspace::new(Box::new(fs)).expect("workspace");
    workspace.open_document(demo.clone()).expect("open demo");
    let reference = VPath::parse("README.md").expect("path");
    workspace.inject_read_only_document(reference.clone(), b"# Read me\n".to_vec(), false);
    let mut app = test_app(workspace);
    let repository_path = VPath::parse("repository/README.md").expect("repository path");
    app.repository_paths
        .insert(reference.clone(), repository_path.clone());
    app.reference_jump = Some((reference.clone(), 42));

    assert_eq!(
        app.reference_address_state(),
        (Some(repository_path.clone()), Some(42))
    );
    app.activate_path(&demo);
    assert_eq!(app.reference_address_state(), (None, None));
    app.activate_path(&reference);
    assert_eq!(app.reference_address_state(), (Some(repository_path), None));
}

#[test]
fn repository_locations_choose_the_bundled_workspace_path() {
    let lesson = reference_location(
        &VPath::parse("programs/lessons/04_functions/max3.s").expect("lesson path"),
    )
    .expect("lesson location");
    assert_eq!(lesson.preset, WorkspacePreset::Lessons);
    assert_eq!(lesson.workspace_path.as_str(), "04_functions/max3.s");

    let demo = reference_location(
        &VPath::parse("programs/example/core_basics/core_basics.hll").expect("demo path"),
    )
    .expect("demo location");
    assert_eq!(demo.preset, WorkspacePreset::Demo);
    assert_eq!(
        demo.workspace_path.as_str(),
        "programs/example/core_basics/core_basics.hll"
    );

    assert!(reference_location(&VPath::parse("README.md").expect("other path")).is_none());
}

#[test]
fn lesson_reference_selects_its_matching_build_configuration() {
    let workspace = IdeWorkspace::lessons().expect("lessons workspace");
    let mut app = test_app(workspace);
    let repository_path =
        VPath::parse("programs/lessons/04_functions/max3.s").expect("repository path");
    app.open_repository_reference(
        crate::web_reference::ReferenceDocument {
            path: repository_path.clone(),
            bytes: b".globl _start\n_start:\n".to_vec(),
            binary: false,
            line: Some(2),
        },
        None,
    );

    assert_eq!(
        app.active_path().expect("active source").as_str(),
        "04_functions/max3.s"
    );
    assert!(
        !app.workspace
            .active_document()
            .expect("active document")
            .is_read_only()
    );
    assert_eq!(
        app.selected_config().expect("selected config").build,
        BuildRef::Manifest(VPath::parse("04_functions/max3.build").expect("manifest"))
    );
    assert_eq!(
        app.reference_address_state(),
        (Some(repository_path), Some(2))
    );
}

#[test]
fn hll_lesson_reference_opens_the_lessons_workspace_and_hosted_config() {
    let workspace = IdeWorkspace::lessons().expect("lessons workspace");
    let mut app = test_app(workspace);
    let repository_path =
        VPath::parse("programs/lessons/08_hll/compiler_pipeline.hll").expect("path");
    app.open_repository_reference(
        crate::web_reference::ReferenceDocument {
            path: repository_path.clone(),
            bytes: b"main: () -> i32 { return 1 + 1 }\n".to_vec(),
            binary: false,
            line: None,
        },
        None,
    );

    assert_eq!(
        app.active_path().expect("active source").as_str(),
        "08_hll/compiler_pipeline.hll"
    );
    assert!(
        !app.workspace
            .active_document()
            .expect("active document")
            .is_read_only()
    );
    let config = app.selected_config().expect("selected config");
    assert_eq!(config.kind, RunKind::Hosted);
    assert_eq!(
        config.build,
        BuildRef::Manifest(VPath::parse("08_hll/compiler_pipeline.build").expect("manifest"))
    );
    assert_eq!(app.reference_address_state(), (Some(repository_path), None));
}

#[test]
fn example_reference_selects_its_demo_build_configuration() {
    let workspace = IdeWorkspace::sample().expect("demo workspace");
    let mut app = test_app(workspace);
    let repository_path =
        VPath::parse("programs/example/core_basics/core_basics.hll").expect("repository path");
    app.open_repository_reference(
        crate::web_reference::ReferenceDocument {
            path: repository_path.clone(),
            bytes: b"main: () -> i32 { return 0 }\n".to_vec(),
            binary: false,
            line: None,
        },
        None,
    );

    assert_eq!(app.active_path().expect("active source"), repository_path);
    assert!(
        !app.workspace
            .active_document()
            .expect("active document")
            .is_read_only()
    );
    assert_eq!(
        app.selected_config().expect("selected config").build,
        BuildRef::Manifest(
            VPath::parse("programs/example/core_basics/core_basics.build").expect("manifest")
        )
    );
}

#[test]
fn lesson_reference_without_a_build_leaves_run_configuration_unselected() {
    let workspace = IdeWorkspace::lessons().expect("lessons workspace");
    let mut app = test_app(workspace);
    app.open_repository_reference(
        crate::web_reference::ReferenceDocument {
            path: VPath::parse("programs/lessons/07_qemu/hello_world.s").expect("repository path"),
            bytes: b".globl _start\n_start:\n".to_vec(),
            binary: false,
            line: None,
        },
        None,
    );

    assert_eq!(
        app.active_path().expect("active source").as_str(),
        "07_qemu/hello_world.s"
    );
    assert!(app.selected_config().is_none());
}

#[test]
fn unbundled_repository_reference_remains_read_only() {
    let workspace = IdeWorkspace::sample().expect("demo workspace");
    let mut app = test_app(workspace);
    let repository_path = VPath::parse("README.md").expect("repository path");
    app.open_repository_reference(
        crate::web_reference::ReferenceDocument {
            path: repository_path.clone(),
            bytes: b"# Full-Stack\n".to_vec(),
            binary: false,
            line: None,
        },
        None,
    );

    assert_eq!(app.active_path().expect("active source"), repository_path);
    assert!(
        app.workspace
            .active_document()
            .expect("active document")
            .is_read_only()
    );
}

#[test]
fn rust_repository_references_never_open_in_the_ide() {
    let workspace = IdeWorkspace::sample().expect("demo workspace");
    let mut app = test_app(workspace);
    let original = app.active_path();
    let repository_path = VPath::parse("src/main.rs").expect("repository path");

    app.open_repository_reference(
        crate::web_reference::ReferenceDocument {
            path: repository_path.clone(),
            bytes: b"fn main() {}\n".to_vec(),
            binary: false,
            line: Some(1),
        },
        None,
    );

    assert_eq!(app.active_path(), original);
    assert!(app.workspace.document(&repository_path).is_none());
    assert_eq!(
        app.status_message.as_deref(),
        Some("Rust source `src/main.rs` is available on GitHub")
    );
}

fn write_native_workspace(root: &Path) {
    fs::create_dir_all(root.join(".fullstack")).expect("create config dir");
    fs::create_dir_all(root.join("programs/demo")).expect("create program dir");
    fs::write(
        root.join(".fullstack/run.toml"),
        "\
[[config]]
name = \"Temp Demo\"
type = \"hosted\"
build = \"programs/demo/demo.build\"
present = \"terminal\"
max_steps_per_frame = 200000
debug = false
",
    )
    .expect("write run config");
    fs::write(
        root.join("programs/demo/demo.build"),
        "\
name = \"temp_demo\"
target = \"hosted\"
root = \"main.hll\"
stdlib = \"hosted\"
",
    )
    .expect("write manifest");
    fs::write(
        root.join("programs/demo/main.hll"),
        "main: () -> i32 { return 0 }",
    )
    .expect("write source");
}

fn temp_root(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("full_stack_{name}_{nanos}"))
}
