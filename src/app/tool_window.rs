use super::{FullStackApp, Tool, chrome, metrics};
use crate::ui::theme;
use eframe::egui;
use egui::{FontId, Sense, pos2, vec2};
use full_stack::build::is_asm_path;
use full_stack::compilation_pipeline::TargetMode;
use full_stack::runconfig::{
    BuildRef, FsMount, PresentMode, RunConfigSeverity, RunKind, name_conflicts,
    run_config_from_manifest, validate_config,
};
use full_stack::target_mode::infer_target_mode_for_source;
use full_stack::workspace::VPath;

impl FullStackApp {
    pub(super) fn tool_window(&mut self, ui: &mut egui::Ui) {
        // Wash the well a step below the editor so the columns read as distinct.
        let rect = ui.max_rect();
        theme::vertical_gradient(&ui.painter_at(rect), rect, theme::WELL, theme::WELL_FLOOR);
        match self.tool {
            Tool::Files => self.explorer_panel(ui),
            Tool::Search => self.search_panel(ui),
            Tool::Run => self.run_panel(ui),
            Tool::Inspect => self.inspect_panel(ui),
        }
    }

    fn run_panel(&mut self, ui: &mut egui::Ui) {
        chrome::panel_header(ui, "Run");
        let now = ui.input(|i| i.time);
        // Discover manifests once per frame; both the add combo and the editor read them.
        let manifests = self.discovered_manifests(now);

        let selected = self.selected_config;
        let mut chosen = None;
        for (idx, config) in self.configs().iter().enumerate() {
            if config_row(ui, &config.name, kind_tag(config.kind), idx == selected).clicked() {
                chosen = Some(idx);
            }
        }
        if self.configs().is_empty() {
            chrome::indented(ui, |ui| {
                ui.label(theme::mono(
                    "No run configs yet. Add one\nfrom a .build manifest below.",
                    metrics::STATUS_TEXT_SIZE,
                    theme::TEXT_FAINT,
                ));
            });
        }
        if let Some(idx) = chosen {
            self.select_config(idx);
        }

        ui.add_space(metrics::SECTION_GAP);
        self.add_config_control(ui, &manifests);
        chrome::indented(ui, |ui| {
            if ui
                .add(egui::Button::new(theme::mono(
                    "+ config from current file",
                    metrics::STATUS_TEXT_SIZE,
                    theme::TEXT_DIM,
                )))
                .on_hover_text("Write a .build for the active file and add a run config for it")
                .clicked()
            {
                self.add_config_for_active_file();
            }
        });

        if self.selected_config().is_some() {
            ui.add_space(metrics::SECTION_GAP);
            chrome::hairline(ui);
            ui.add_space(metrics::SECTION_GAP);
            self.config_editor(ui, &manifests);
        }
    }

    fn add_config_control(&mut self, ui: &mut egui::Ui, manifests: &[VPath]) {
        let mut added = None;
        chrome::indented(ui, |ui| {
            egui::ComboBox::from_id_salt("add_run_config")
                .selected_text(theme::mono(
                    "+ add from .build",
                    metrics::STATUS_TEXT_SIZE,
                    theme::TEXT_DIM,
                ))
                .show_ui(ui, |ui| {
                    for path in manifests {
                        let label =
                            theme::mono(path.as_str(), metrics::STATUS_TEXT_SIZE, theme::TEXT);
                        if ui.selectable_label(false, label).clicked() {
                            added = Some(path.clone());
                        }
                    }
                });
        });
        if let Some(path) = added {
            match run_config_from_manifest(self.workspace.fs(), path) {
                Ok(config) => {
                    self.workspace.run_configs_mut().configs.push(config);
                    let idx = self.configs().len() - 1;
                    self.persist_run_configs();
                    self.select_config(idx);
                }
                Err(err) => {
                    self.status_message = Some(format!("add config failed: {err}"));
                }
            }
        }
    }

    pub(super) fn add_config_for_active_file(&mut self) {
        let Some(path) = self.workspace.active_document().map(|d| d.path().clone()) else {
            self.status_message = Some("open a source file to create a config".to_owned());
            return;
        };
        let file = path.file_name().unwrap_or_default().to_owned();
        let Some(stem) = file
            .strip_suffix(".hll")
            .or_else(|| file.strip_suffix(".s"))
            .map(str::to_owned)
        else {
            self.status_message = Some("configs come from .hll or .s files".to_owned());
            return;
        };
        let manifest_ref = format!("{stem}.build");
        let manifest_path = match path.parent() {
            Some(parent) => parent.join(&manifest_ref),
            None => VPath::parse(&manifest_ref),
        };
        let Ok(manifest_path) = manifest_path else {
            self.status_message = Some(format!("invalid manifest path {manifest_ref}"));
            return;
        };

        if self.workspace.fs().metadata(&manifest_path).is_err() {
            let target = if is_asm_path(&file) {
                TargetMode::Freestanding
            } else {
                let source = self
                    .workspace
                    .document(&path)
                    .map(|doc| doc.text().to_owned())
                    .unwrap_or_default();
                infer_target_mode_for_source(&source, false, TargetMode::Hosted)
            };
            let text = generated_manifest(&file, &stem, target);
            if let Err(err) = self
                .workspace
                .fs_mut()
                .write_file(&manifest_path, text.as_bytes())
            {
                self.status_message = Some(format!("writing {manifest_path} failed: {err}"));
                return;
            }
            if let Err(err) = self.workspace.refresh_tree() {
                self.status_message = Some(format!("refreshing the file tree failed: {err}"));
            }
            self.manifest_cache = None;
        }

        if let Some(idx) = self
            .configs()
            .iter()
            .position(|c| matches!(&c.build, BuildRef::Manifest(p) if p == &manifest_path))
        {
            self.select_config(idx);
            self.status_message = Some(format!("selected the existing config for {file}"));
            return;
        }
        match run_config_from_manifest(self.workspace.fs(), manifest_path) {
            Ok(config) => {
                let name = config.name.clone();
                self.workspace.run_configs_mut().configs.push(config);
                let idx = self.configs().len() - 1;
                self.persist_run_configs();
                self.select_config(idx);
                self.status_message = Some(format!("added run config {name}"));
            }
            Err(err) => self.status_message = Some(format!("add config failed: {err}")),
        }
    }

    fn config_editor(&mut self, ui: &mut egui::Ui, manifests: &[VPath]) {
        let idx = self.selected_config;
        let Some(original) = self.configs().get(idx).cloned() else {
            return;
        };
        let mut config = original.clone();
        // Warn if the name (as last persisted) collides with another config.
        let name_conflict = name_conflicts(&original.name, self.configs(), idx);
        let mut host_buf = std::mem::take(&mut self.mount_host_entry);
        let mut guest_buf = std::mem::take(&mut self.mount_guest_entry);
        let mut remove = false;
        let mut duplicate = false;

        {
            let fs = self.workspace.fs();
            chrome::indented(ui, |ui| {
                config_fields(ui, &mut config, manifests);
                mount_editor(ui, &mut config, &mut host_buf, &mut guest_buf);
                breakpoints_editor(ui, &mut config);
                config_details(ui, &config, fs, name_conflict);
                ui.add_space(metrics::SECTION_GAP);
                ui.horizontal(|ui| {
                    if editor_button(ui, "duplicate") {
                        duplicate = true;
                    }
                    if editor_button(ui, "remove config") {
                        remove = true;
                    }
                });
            });
        }
        self.mount_host_entry = host_buf;
        self.mount_guest_entry = guest_buf;

        if duplicate {
            self.duplicate_config(idx);
            return;
        }
        if remove {
            self.workspace.run_configs_mut().configs.remove(idx);
            self.selected_config = idx.min(self.configs().len().saturating_sub(1));
            self.persist_run_configs();
            self.status_message = Some(format!("removed config {}", original.name));
            return;
        }
        // A kernel/hosted flip must follow into a source-based build target.
        if let BuildRef::SourceWithDefaults { target, .. } = &mut config.build {
            *target = match config.kind {
                RunKind::Hosted => TargetMode::Hosted,
                RunKind::KernelBoot => TargetMode::Kernel,
                RunKind::Freestanding => TargetMode::Freestanding,
            };
        }
        if config != original {
            self.workspace.run_configs_mut().configs[idx] = config;
            self.persist_run_configs();
        }
    }

    fn duplicate_config(&mut self, idx: usize) {
        let Some(mut copy) = self.configs().get(idx).cloned() else {
            return;
        };
        copy.name = format!("{} copy", copy.name);
        copy.name_from_manifest = false;
        self.workspace
            .run_configs_mut()
            .configs
            .insert(idx + 1, copy);
        self.persist_run_configs();
        self.select_config(idx + 1);
    }
}

fn config_row(ui: &mut egui::Ui, name: &str, tag: &str, active: bool) -> egui::Response {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, metrics::ROW_HEIGHT), Sense::click());
    let painter = ui.painter_at(rect);
    if active {
        painter.rect_filled(rect, 0.0, theme::SURFACE);
        let bar = egui::Rect::from_min_max(rect.left_top(), pos2(rect.left() + 2.0, rect.bottom()));
        painter.rect_filled(bar, 0.0, theme::ELECTRIC);
    } else if resp.hovered() {
        painter.rect_filled(rect, 0.0, theme::SURFACE_ALT);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let color = if active { theme::ELECTRIC } else { theme::TEXT };
    let tag_galley = painter.layout_no_wrap(
        tag.to_owned(),
        FontId::monospace(metrics::STATUS_TEXT_SIZE),
        theme::TEXT_FAINT,
    );
    // Elide long names so they never run under the right-aligned kind tag.
    let name_max =
        (rect.width() - 2.0 * metrics::PANEL_INSET_X - tag_galley.size().x - metrics::SECTION_GAP)
            .max(0.0);
    let mut job = egui::text::LayoutJob::simple_singleline(
        name.to_owned(),
        FontId::monospace(metrics::LABEL_TEXT_SIZE),
        color,
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(name_max);
    let name_galley = painter.layout_job(job);
    let elided = name_galley.elided;
    painter.galley(
        pos2(
            rect.left() + metrics::PANEL_INSET_X,
            rect.center().y - name_galley.size().y / 2.0,
        ),
        name_galley,
        color,
    );
    painter.galley(
        pos2(
            rect.right() - metrics::PANEL_INSET_X - tag_galley.size().x,
            rect.center().y - tag_galley.size().y / 2.0,
        ),
        tag_galley,
        theme::TEXT_FAINT,
    );
    if elided {
        resp.on_hover_text(theme::mono(name, metrics::STATUS_TEXT_SIZE, theme::TEXT))
    } else {
        resp
    }
}

fn field_row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.label(theme::label(label));
        add(ui);
    });
}

fn config_fields(
    ui: &mut egui::Ui,
    config: &mut full_stack::runconfig::RunConfig,
    manifests: &[VPath],
) {
    field_row(ui, "name", |ui| {
        // Typing a name overrides the manifest title, so run.toml starts storing it.
        let response = ui.add(
            egui::TextEdit::singleline(&mut config.name)
                .id_salt("cfg_name")
                .font(egui::TextStyle::Monospace)
                .text_color(theme::TEXT)
                .desired_width(150.0),
        );
        if response.changed() {
            config.name_from_manifest = false;
        }
    });
    field_row(ui, "kind", |ui| {
        combo(ui, "cfg_kind", kind_tag(config.kind), |ui| {
            for kind in [RunKind::Hosted, RunKind::KernelBoot, RunKind::Freestanding] {
                if combo_item(ui, kind_tag(kind), config.kind == kind) {
                    config.kind = kind;
                }
            }
        });
    });
    field_row(ui, "build", |ui| {
        combo(ui, "cfg_build", &build_ref_label(config), |ui| {
            for path in manifests {
                let active = matches!(&config.build, BuildRef::Manifest(p) if p == path);
                if combo_item(ui, path.as_str(), active) {
                    config.build = BuildRef::Manifest(path.clone());
                }
            }
        });
    });
    field_row(ui, "present", |ui| {
        combo(ui, "cfg_present", present_tag(config.run.present), |ui| {
            for mode in [
                PresentMode::Terminal,
                PresentMode::Framebuffer,
                PresentMode::Both,
            ] {
                if combo_item(ui, present_tag(mode), config.run.present == mode) {
                    config.run.present = mode;
                }
            }
        });
    });
    field_row(ui, "autorun", |ui| {
        ui.add(
            egui::TextEdit::singleline(&mut config.run.autorun)
                .id_salt("cfg_autorun")
                .font(egui::TextStyle::Monospace)
                .text_color(theme::TEXT)
                .desired_width(150.0),
        );
    });
    field_row(ui, "steps/frame", |ui| {
        ui.add(
            egui::DragValue::new(&mut config.run.max_steps_per_frame)
                .range(1_000..=5_000_000)
                .speed(1_000),
        );
    });
}

fn mount_editor(
    ui: &mut egui::Ui,
    config: &mut full_stack::runconfig::RunConfig,
    host_buf: &mut String,
    guest_buf: &mut String,
) {
    ui.add_space(metrics::SECTION_GAP);
    ui.label(theme::label("mounts"));
    let mut remove_at = None;
    for (i, mount) in config.fs.mounts.iter().enumerate() {
        ui.horizontal(|ui| {
            if remove_glyph(ui, "mount", i) {
                remove_at = Some(i);
            }
            ui.label(theme::mono(
                format!("{} \u{2192} {}", mount.host, mount.guest),
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT_DIM,
            ));
        });
    }
    if let Some(i) = remove_at {
        config.fs.mounts.remove(i);
    }
    ui.horizontal(|ui| {
        mount_field(ui, "mount_host", host_buf, "host path");
        mount_field(ui, "mount_guest", guest_buf, "/guest");
        if editor_button(ui, "add")
            && !host_buf.trim().is_empty()
            && let Ok(host) = VPath::parse(host_buf.trim())
        {
            config.fs.mounts.push(FsMount {
                host,
                guest: guest_buf.trim().to_owned(),
            });
            host_buf.clear();
            guest_buf.clear();
        }
    });
}

// Persisted breakpoint count only; editing lives in the Debugger dock.
fn breakpoints_editor(ui: &mut egui::Ui, config: &mut full_stack::runconfig::RunConfig) {
    if config.debug.breakpoints.is_empty() {
        return;
    }
    ui.add_space(metrics::SECTION_GAP);
    ui.horizontal(|ui| {
        ui.label(theme::label("breakpoints"));
        ui.label(theme::mono(
            config.debug.breakpoints.len().to_string(),
            metrics::STATUS_TEXT_SIZE,
            theme::TEXT_DIM,
        ));
        if editor_button(ui, "clear all") {
            config.debug.breakpoints.clear();
        }
    });
}

fn config_details(
    ui: &mut egui::Ui,
    config: &full_stack::runconfig::RunConfig,
    fs: &dyn full_stack::workspace::WorkspaceFs,
    name_conflict: bool,
) {
    if name_conflict {
        ui.add_space(metrics::SECTION_GAP);
        ui.label(theme::mono(
            "another config shares this name",
            metrics::STATUS_TEXT_SIZE,
            theme::WARNING,
        ));
    }
    for diag in validate_config(config, fs) {
        let color = match diag.severity {
            RunConfigSeverity::Error => theme::DANGER,
            RunConfigSeverity::Warning => theme::WARNING,
        };
        ui.label(theme::mono(diag.message, metrics::STATUS_TEXT_SIZE, color));
    }
}

fn mount_field(ui: &mut egui::Ui, id: &str, buf: &mut String, hint: &str) {
    ui.add(
        egui::TextEdit::singleline(buf)
            .id_salt(id)
            .font(egui::TextStyle::Monospace)
            .text_color(theme::TEXT)
            .hint_text(theme::mono(
                hint,
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT_FAINT,
            ))
            .desired_width(96.0),
    );
}

fn remove_glyph(ui: &mut egui::Ui, kind: &str, i: usize) -> bool {
    ui.add(
        egui::Button::new(theme::mono(
            "\u{00D7}",
            metrics::LABEL_TEXT_SIZE,
            theme::TEXT_DIM,
        ))
        .frame(false),
    )
    .on_hover_text(format!("remove {kind} {i}"))
    .clicked()
}

fn editor_button(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(egui::Button::new(theme::mono(
        label,
        metrics::STATUS_TEXT_SIZE,
        theme::TEXT_DIM,
    )))
    .clicked()
}

fn combo(ui: &mut egui::Ui, id: &str, current: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(theme::mono(current, metrics::STATUS_TEXT_SIZE, theme::TEXT))
        .show_ui(ui, add);
}

fn combo_item(ui: &mut egui::Ui, label: &str, active: bool) -> bool {
    ui.selectable_label(
        active,
        theme::mono(label, metrics::STATUS_TEXT_SIZE, theme::TEXT),
    )
    .clicked()
}

pub(super) fn kind_tag(kind: RunKind) -> &'static str {
    match kind {
        RunKind::Hosted => "hosted",
        RunKind::KernelBoot => "kernel",
        RunKind::Freestanding => "freestanding",
    }
}

pub(super) fn present_tag(mode: PresentMode) -> &'static str {
    match mode {
        PresentMode::Terminal => "terminal",
        PresentMode::Framebuffer => "framebuffer",
        PresentMode::Both => "both",
    }
}

// The lesson `sum.build` documents every field.
fn generated_manifest(file: &str, stem: &str, target: TargetMode) -> String {
    let target_tag = match target {
        TargetMode::Hosted => "hosted",
        TargetMode::Kernel => "kernel",
        TargetMode::Freestanding => "freestanding",
    };
    let mut text = format!("name = \"{stem}\"\nroot = \"{file}\"\ntarget = \"{target_tag}\"\n");
    if is_asm_path(file) {
        text.push_str("entry = \"_start\"\n");
    }
    text
}

fn build_ref_label(config: &full_stack::runconfig::RunConfig) -> String {
    let path = match &config.build {
        full_stack::runconfig::BuildRef::Manifest(path) => path.as_str(),
        full_stack::runconfig::BuildRef::SourceWithDefaults { root, .. } => root.as_str(),
    };
    path.rsplit('/').next().unwrap_or(path).to_owned()
}
