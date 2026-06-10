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
            self.render_program_section(ui, ProgramKind::Stdlib, "Standard Library");
            ui.separator();
            self.render_program_section(ui, ProgramKind::Os, "OS");
            ui.separator();
            self.render_program_section(ui, ProgramKind::User, "Userspace Programs");
            ui.separator();
            self.render_program_section(ui, ProgramKind::Example, "Examples");
            ui.separator();
            self.render_program_section(ui, ProgramKind::Custom, "Your programs");

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

    fn render_program_section(&mut self, ui: &mut egui::Ui, kind: ProgramKind, title: &str) {
        let entries: Vec<(String, String)> = self
            .catalog
            .get_programs_by_kind(kind)
            .iter()
            .map(|p| (p.id.clone(), p.name.clone()))
            .collect();

        if entries.is_empty() {
            return;
        }

        let header_label = format!("{title} ({})", entries.len());
        egui::CollapsingHeader::new(header_label)
            .default_open(true)
            .show(ui, |ui| {
                for (id, name) in &entries {
                    let is_rename_active = self.rename_id.as_deref() == Some(id.as_str());
                    if is_rename_active {
                        let response = ui.text_edit_singleline(&mut self.rename_buffer);
                        response.request_focus();
                        let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if response.lost_focus() || enter_pressed {
                            if let Some(program) = self.catalog.current_program_mut() {
                                if program.id == *id {
                                    program.name = self.rename_buffer.trim().to_owned();
                                }
                            }
                            self.rename_id = None;
                            ui.ctx().request_repaint();
                        }
                    } else {
                        let selected = *id == self.catalog.selected_program_id;
                        let can_rename = kind == ProgramKind::Custom;
                        let response = if can_rename {
                            ui.selectable_label(selected, name)
                                .on_hover_text("double-click to rename")
                        } else {
                            ui.selectable_label(selected, name)
                        };
                        if response.clicked() {
                            self.catalog.select_program(id);
                            self.compile();
                        }
                        if response.double_clicked() && can_rename {
                            self.rename_buffer = name.clone();
                            self.rename_id = Some(id.clone());
                            ui.ctx().request_repaint();
                        }
                    }
                }
            });
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
