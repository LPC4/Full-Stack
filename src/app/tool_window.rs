use super::{FullStackApp, Tool, chrome, metrics};
use crate::ui::theme;
use eframe::egui;
use egui::{Align2, FontId, Sense, pos2, vec2};
use full_stack::compilation_pipeline::TargetMode;
use full_stack::runconfig::{
    BuildRef, FsMount, PresentMode, RunConfigSeverity, RunKind, name_conflicts,
    run_config_from_manifest, validate_config,
};
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

        if self.selected_config().is_some() {
            ui.add_space(metrics::SECTION_GAP);
            chrome::hairline(ui);
            ui.add_space(metrics::SECTION_GAP);
            self.config_editor(ui, &manifests);
        }
    }

    /// Append a run config seeded from a workspace `.build` manifest.
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

    /// Edit the selected config in place; changes persist to run.toml.
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

    /// Insert a copy of config `idx` right after it and select the copy.
    fn duplicate_config(&mut self, idx: usize) {
        let Some(mut copy) = self.configs().get(idx).cloned() else {
            return;
        };
        copy.name = format!("{} copy", copy.name);
        self.workspace
            .run_configs_mut()
            .configs
            .insert(idx + 1, copy);
        self.persist_run_configs();
        self.select_config(idx + 1);
    }
}

/// A full-width selectable config row: name at the left, kind tag at the right.
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
    painter.text(
        pos2(rect.left() + metrics::PANEL_INSET_X, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::monospace(metrics::LABEL_TEXT_SIZE),
        color,
    );
    painter.text(
        pos2(rect.right() - metrics::PANEL_INSET_X, rect.center().y),
        Align2::RIGHT_CENTER,
        tag,
        FontId::monospace(metrics::STATUS_TEXT_SIZE),
        theme::TEXT_FAINT,
    );
    resp
}

/// A labeled editor row: eyebrow label at the left, widget to its right.
fn field_row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.label(theme::label(label));
        add(ui);
    });
}

/// The editable fields of a run config (name, kind, build, run behavior).
fn config_fields(
    ui: &mut egui::Ui,
    config: &mut full_stack::runconfig::RunConfig,
    manifests: &[VPath],
) {
    field_row(ui, "name", |ui| {
        ui.add(
            egui::TextEdit::singleline(&mut config.name)
                .id_salt("cfg_name")
                .font(egui::TextStyle::Monospace)
                .text_color(theme::TEXT)
                .desired_width(150.0),
        );
    });
    field_row(ui, "kind", |ui| {
        combo(ui, "cfg_kind", kind_tag(config.kind), |ui| {
            for kind in [RunKind::Hosted, RunKind::KernelBoot] {
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
    ui.checkbox(
        &mut config.debug.enabled,
        theme::mono(
            "debug: start paused",
            metrics::STATUS_TEXT_SIZE,
            theme::TEXT,
        ),
    );
}

/// Editable host->guest mounts: a removable row each, plus an add row.
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

/// Persisted breakpoints for the config, with a clear-all control.
fn breakpoints_editor(ui: &mut egui::Ui, config: &mut full_stack::runconfig::RunConfig) {
    if config.debug.breakpoints.is_empty() {
        return;
    }
    ui.add_space(metrics::SECTION_GAP);
    let mut clear = false;
    ui.horizontal(|ui| {
        ui.label(theme::label("breakpoints"));
        if editor_button(ui, "clear all") {
            clear = true;
        }
    });
    if clear {
        config.debug.breakpoints.clear();
        return;
    }
    for breakpoint in &config.debug.breakpoints {
        ui.label(theme::mono(
            breakpoint,
            metrics::STATUS_TEXT_SIZE,
            theme::TEXT_DIM,
        ));
    }
}

/// The duplicate-name warning and live validation for the selected config.
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

/// A short mono text field for the mount add row.
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

/// A small frameless `x` glyph button for removing a list row; true on click.
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

/// A dim, mono text button used across the config editor; true on click.
fn editor_button(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(egui::Button::new(theme::mono(
        label,
        metrics::STATUS_TEXT_SIZE,
        theme::TEXT_DIM,
    )))
    .clicked()
}

/// A mono-styled `ComboBox` shell shared by the config editor fields.
fn combo(ui: &mut egui::Ui, id: &str, current: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(theme::mono(current, metrics::STATUS_TEXT_SIZE, theme::TEXT))
        .show_ui(ui, add);
}

/// One mono-styled selectable option inside a `combo`; true when clicked.
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

fn present_tag(mode: PresentMode) -> &'static str {
    match mode {
        PresentMode::Terminal => "terminal",
        PresentMode::Framebuffer => "framebuffer",
        PresentMode::Both => "both",
    }
}

fn build_ref_label(config: &full_stack::runconfig::RunConfig) -> String {
    let path = match &config.build {
        full_stack::runconfig::BuildRef::Manifest(path) => path.as_str(),
        full_stack::runconfig::BuildRef::SourceWithDefaults { root, .. } => root.as_str(),
    };
    path.rsplit('/').next().unwrap_or(path).to_owned()
}
