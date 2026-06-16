use egui::RichText;
use full_stack::view::{ProgramKind, ui_theme};

use super::{CatalogExportKind, FullStackApp};

#[cfg(not(target_arch = "wasm32"))]
use std::{fs, path::Path};

impl FullStackApp {
    pub(super) fn catalog_ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.heading("Files");
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                if ui.button("New File").clicked() {
                    self.catalog.create_blank_program();
                    self.rename_id = None;
                    self.compile();
                }
                if ui.button("Duplicate").clicked() {
                    self.catalog.duplicate_current_program();
                    self.rename_id = None;
                    self.compile();
                }
            });

            #[cfg(not(target_arch = "wasm32"))]
            ui.horizontal(|ui| {
                let import_label = if self.show_import_controls {
                    "Import v"
                } else {
                    "Import"
                };
                if ui
                    .button(import_label)
                    .on_hover_text("Import a .hll file from disk")
                    .clicked()
                {
                    self.show_import_controls = !self.show_import_controls;
                    if self.show_import_controls {
                        self.show_export_controls = false;
                    }
                }
                let export_label = if self.show_export_controls {
                    "Export v"
                } else {
                    "Export"
                };
                if ui
                    .button(export_label)
                    .on_hover_text("Export the current program, assembly, or ELF image")
                    .clicked()
                {
                    self.show_export_controls = !self.show_export_controls;
                    if self.show_export_controls {
                        self.show_import_controls = false;
                    }
                }
            });

            #[cfg(not(target_arch = "wasm32"))]
            {
                if self.show_import_controls {
                    ui.separator();
                    ui.small("Import a .hll file from disk:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.import_disk_path)
                            .hint_text("Path to .hll file")
                            .desired_width(f32::INFINITY),
                    );
                    let path_ready = !self.import_disk_path.trim().is_empty();
                    if ui
                        .add_enabled(path_ready, egui::Button::new("Import .hll"))
                        .clicked()
                    {
                        self.import_program_from_disk();
                    }
                }

                if self.show_export_controls {
                    ui.separator();
                    ui.small("Export the current program, assembly, or ELF image:");
                    ui.horizontal(|ui| {
                        ui.label("Format:");
                        egui::ComboBox::from_id_salt("catalog_export_format")
                            .selected_text(self.export_kind.label())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.export_kind,
                                    CatalogExportKind::Hll,
                                    ".hll",
                                );
                                ui.selectable_value(
                                    &mut self.export_kind,
                                    CatalogExportKind::Asm,
                                    ".s",
                                );
                                ui.selectable_value(
                                    &mut self.export_kind,
                                    CatalogExportKind::Elf,
                                    ".elf",
                                );
                                ui.selectable_value(
                                    &mut self.export_kind,
                                    CatalogExportKind::Bin,
                                    ".bin (flat binary)",
                                );
                            });
                    });
                    ui.add(
                        egui::TextEdit::singleline(&mut self.export_disk_path)
                            .hint_text(self.export_kind.hint())
                            .desired_width(f32::INFINITY),
                    );
                    let path_ready = !self.export_disk_path.trim().is_empty();
                    let can_export = path_ready
                        && match self.export_kind {
                            CatalogExportKind::Hll => self.catalog.current_program().is_some(),
                            CatalogExportKind::Asm
                            | CatalogExportKind::Elf
                            | CatalogExportKind::Bin => {
                                self.compilation_state.just_compiled
                                    && self.compilation_state.assembled().is_some()
                            }
                        };
                    let export_label = match self.export_kind {
                        CatalogExportKind::Hll => "Export .hll",
                        CatalogExportKind::Asm => "Export .s",
                        CatalogExportKind::Elf => "Export .elf",
                        CatalogExportKind::Bin => "Export .bin",
                    };
                    if ui
                        .add_enabled(can_export, egui::Button::new(export_label))
                        .clicked()
                    {
                        self.export_selected_output_to_disk();
                    }
                }

                if let Some(message) = &self.catalog_message {
                    let theme = ui_theme();
                    let lower = message.to_lowercase();
                    let is_err = lower.starts_with("failed")
                        || lower.starts_with("error")
                        || lower.starts_with("no program")
                        || lower.starts_with("enter a");
                    let color = if is_err { theme.error } else { theme.success };
                    ui.label(RichText::new(message).small().color(color));
                }
            }

            ui.add_space(8.0);
            // Intent groups, top-level entries only; aux modules and kernel
            // fragments render nested under their parent (see render_catalog_group).
            let runnable_tools = self.group_ids(|p| p.is_user() && p.parent_id.is_none());
            let kernel = self.group_ids(|p| p.id == "os-my-kernel");
            let reference = self.group_ids(|p| p.is_stdlib());
            let examples = self.group_ids(|p| p.kind == ProgramKind::Example);
            let custom = self.group_ids(|p| p.kind == ProgramKind::Custom);

            self.render_catalog_group(ui, "Programs", true, &runnable_tools);
            ui.separator();
            self.render_catalog_group(ui, "Operating System", true, &kernel);
            ui.separator();
            self.render_catalog_group(ui, "Standard Library", false, &reference);
            ui.separator();
            self.render_catalog_group(ui, "Examples", false, &examples);
            ui.separator();
            self.render_catalog_group(ui, "My Files", true, &custom);

            let is_custom = self
                .catalog
                .current_program()
                .map(|p| p.is_custom())
                .unwrap_or(false);
            if is_custom && ui.input(|i| i.key_pressed(egui::Key::Delete)) {
                self.catalog.delete_current_custom_program();
                self.rename_id = None;
                self.compile();
            }
        });
    }

    /// Catalog entry ids (top-level, in display order) matching `pred`.
    fn group_ids(&self, pred: impl Fn(&full_stack::view::ProgramFile) -> bool) -> Vec<String> {
        self.catalog
            .all_programs()
            .iter()
            .filter(|p| pred(p))
            .map(|p| p.id.clone())
            .collect()
    }

    /// Render one collapsible intent group. A program that has child modules
    /// looks like a normal file with a trailing "…"; clicking it selects the
    /// program and reveals its modules (no separate expander arrow).
    fn render_catalog_group(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        default_open: bool,
        top_level: &[String],
    ) {
        if top_level.is_empty() {
            return;
        }
        let header_label = format!("{title} ({})", top_level.len());
        egui::CollapsingHeader::new(header_label)
            .default_open(default_open)
            .show(ui, |ui| {
                for id in top_level {
                    let child_ids: Vec<String> = self
                        .catalog
                        .children_of(id)
                        .iter()
                        .map(|c| c.id.clone())
                        .collect();
                    if child_ids.is_empty() {
                        self.render_catalog_row(ui, id, 0, false);
                        continue;
                    }
                    // Click the program (which carries a "…") to select it and
                    // reveal its modules; click again to hide them. State lives in
                    // egui temp memory, closed by default.
                    let expand_id = ui.make_persistent_id(("catalog_expand", id));
                    let mut expanded = ui.data(|d| d.get_temp::<bool>(expand_id)).unwrap_or(false);
                    if self.render_catalog_row(ui, id, 0, !expanded) {
                        expanded = !expanded;
                        ui.data_mut(|d| d.insert_temp(expand_id, expanded));
                    }
                    if expanded {
                        for child_id in &child_ids {
                            self.render_catalog_row(ui, child_id, 1, false);
                        }
                    }
                }
            });
    }

    /// Render a single catalog row at the given nesting depth: indent, selectable
    /// name (with a trailing "…" when `has_more`), and a runnability badge chip.
    /// Handles selection + custom-file rename; returns whether the row was clicked.
    fn render_catalog_row(
        &mut self,
        ui: &mut egui::Ui,
        id: &str,
        depth: usize,
        has_more: bool,
    ) -> bool {
        let Some(program) = self.catalog.all_programs().iter().find(|p| p.id == id) else {
            return false;
        };
        let name = program.name.clone();
        let badge = program.badge();
        let can_rename = program.is_custom();
        let selected = id == self.catalog.selected_program_id;

        if self.rename_id.as_deref() == Some(id) {
            let response = ui.text_edit_singleline(&mut self.rename_buffer);
            response.request_focus();
            let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
            if response.lost_focus() || enter_pressed {
                if let Some(program) = self.catalog.current_program_mut() {
                    if program.id == id {
                        program.name = self.rename_buffer.trim().to_owned();
                    }
                }
                self.rename_id = None;
                ui.ctx().request_repaint();
            }
            return false;
        }

        let label = if has_more {
            format!("{name} …")
        } else {
            name.clone()
        };
        let mut response = ui
            .horizontal(|ui| {
                if depth > 0 {
                    ui.add_space(depth as f32 * 14.0);
                }
                let resp = ui.selectable_label(selected, label);
                Self::badge_chip(ui, badge);
                resp
            })
            .inner;

        if can_rename {
            response = response.on_hover_text("double-click to rename");
        }
        let clicked = response.clicked();
        if clicked {
            self.catalog.select_program(id);
            self.compile();
        }
        if can_rename && response.double_clicked() {
            self.rename_buffer = name;
            self.rename_id = Some(id.to_owned());
            ui.ctx().request_repaint();
        }
        clicked
    }

    /// Small colored chip indicating an entry's runnability.
    fn badge_chip(ui: &mut egui::Ui, badge: full_stack::view::CatalogBadge) {
        use full_stack::view::CatalogBadge;
        let theme = ui_theme();
        let (label, color) = match badge {
            CatalogBadge::Runnable => ("run", theme.accent),
            CatalogBadge::Reference => ("ref", theme.text_dim),
            CatalogBadge::Fragment => ("frag", theme.text_dim),
        };
        ui.label(RichText::new(label).small().weak().color(color));
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn export_selected_output_to_disk(&mut self) {
        let path = self.export_disk_path.trim().to_owned();
        if path.is_empty() {
            self.catalog_message = Some("enter a file path to export the selected file".to_owned());
            return;
        }

        let result = match self.export_kind {
            CatalogExportKind::Hll => {
                let Some(program) = self.catalog.current_program() else {
                    self.catalog_message = Some("no program selected".to_owned());
                    return;
                };
                fs::write(&path, &program.source)
                    .map(|_| format!("exported `{}` to `{path}`", program.name))
            }
            CatalogExportKind::Asm => {
                if self.compilation_state.assembled().is_none() {
                    self.catalog_message =
                        Some("compile successfully before exporting assembly".to_owned());
                    return;
                }
                if !self.compilation_state.just_compiled {
                    self.catalog_message =
                        Some("recompile successfully before exporting assembly".to_owned());
                    return;
                }
                fs::write(&path, self.compilation_state.asm().as_bytes())
                    .map(|_| format!("exported assembly to `{path}`"))
            }
            CatalogExportKind::Elf => {
                let Some(assembled) = self.compilation_state.assembled() else {
                    self.catalog_message =
                        Some("compile successfully before exporting an ELF image".to_owned());
                    return;
                };
                if !self.compilation_state.just_compiled {
                    self.catalog_message =
                        Some("recompile successfully before exporting an ELF image".to_owned());
                    return;
                }
                let entry = self.compilation_state.entry_symbol.clone();
                let base = self.compilation_state.load_base;
                let elf = assembled.to_elf_with_entry(base, &entry);
                fs::write(&path, elf)
                    .map(|_| format!("exported ELF image to `{path}` (load base {base:#010x})"))
            }
            CatalogExportKind::Bin => {
                let Some(assembled) = self.compilation_state.assembled() else {
                    self.catalog_message =
                        Some("compile successfully before exporting a flat binary".to_owned());
                    return;
                };
                if !self.compilation_state.just_compiled {
                    self.catalog_message =
                        Some("recompile successfully before exporting a flat binary".to_owned());
                    return;
                }
                let bin = assembled.to_flat_binary();
                fs::write(&path, bin).map(|_| format!("exported flat binary to `{path}`"))
            }
        };

        match result {
            Ok(message) => self.catalog_message = Some(message),
            Err(err) => {
                self.catalog_message = Some(format!("failed to export to `{path}`: {err}"));
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn import_program_from_disk(&mut self) {
        let path = self.import_disk_path.trim().to_owned();
        if path.is_empty() {
            self.catalog_message = Some("enter a file path to import a program".to_owned());
            return;
        }

        match fs::read_to_string(&path) {
            Ok(source) => {
                let name = Path::new(&path)
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .filter(|stem| !stem.trim().is_empty())
                    .map(|stem| stem.to_owned())
                    .unwrap_or_else(|| String::from("Imported Program"));
                self.catalog.create_custom_program(source, name.clone());
                self.rename_id = None;
                self.catalog_message = Some(format!("imported `{name}` from `{path}`"));
                self.compile();
            }
            Err(err) => {
                self.catalog_message = Some(format!("failed to import from `{path}`: {err}"));
            }
        }
    }
}
