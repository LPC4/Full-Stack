//! Build-output panes: the retained build plan and linked units (Artifacts),
//! and a filterable symbol table with line-table summary (Symbols).

use super::FullStackApp;
use super::widgets::{human_size, section_label, stat_row};
use crate::app::{chrome, metrics};
use crate::ui::theme;
use eframe::egui;
use full_stack::build::{ImportClosurePolicy, StdlibPolicy};
use full_stack::session::LiveMachine;
use std::collections::BTreeSet;

/// Symbol rows rendered before the list truncates; filtering narrows past it.
const SYMBOL_ROW_CAP: usize = 400;

impl FullStackApp {
    /// What the source became: the build plan, the linked units, and where the
    /// `.ir`/`.s`/`.elf` artifacts were written when that was enabled.
    pub(super) fn artifacts_pane(&self, ui: &mut egui::Ui) {
        let Some(build) = &self.last_build else {
            chrome::centered_hint(ui, "Build a config to inspect artifacts.");
            return;
        };
        let plan = &build.artifacts.plan;
        egui::ScrollArea::vertical()
            .id_salt("artifacts_scroll")
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                section_label(ui, "plan");
                stat_row(ui, "name", &plan.name);
                stat_row(ui, "target", super::target_tag(plan.target));
                stat_row(ui, "entry", &build.entry);
                stat_row(ui, "root", &plan.root);
                stat_row(ui, "stdlib", stdlib_tag(plan.stdlib));
                stat_row(ui, "modules", closure_tag(plan.import_closure));
                if !plan.abi_exports.is_empty() {
                    stat_row(ui, "abi exports", &plan.abi_exports.join(", "));
                }
                if plan.has_source_prelude {
                    stat_row(ui, "prelude", "shared source prelude");
                }

                ui.add_space(metrics::SECTION_GAP);
                section_label(ui, "units");
                for unit in &build.artifacts.units {
                    stat_row(
                        ui,
                        &unit.name,
                        &human_size(unit.assembled.total_bytes() as u64),
                    );
                }

                ui.add_space(metrics::SECTION_GAP);
                section_label(ui, "linked");
                let linked = &build.artifacts.linked;
                stat_row(ui, "size", &human_size(linked.total_bytes() as u64));
                stat_row(ui, "sections", &linked.sections_iter().count().to_string());
                stat_row(ui, "symbols", &linked.symbol_count().to_string());
                stat_row(ui, "line entries", &linked.line_table_len().to_string());
                if let Some(dir) = &build.artifact_dir {
                    stat_row(ui, "out", &dir.display().to_string());
                }
            });
    }

    /// The linked symbol table: filterable rows sorted by address, image
    /// offsets when static, runtime addresses when a session exists.
    pub(super) fn symbols_pane(&mut self, ui: &mut egui::Ui) {
        let live = self.live.is_some();
        chrome::indented(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::label("filter"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.inspector.symbol_filter)
                        .id_salt("symbol_filter")
                        .font(egui::TextStyle::Monospace)
                        .text_color(theme::TEXT)
                        .desired_width(f32::INFINITY),
                );
            });
        });
        ui.add_space(metrics::SECTION_GAP);

        let symbols = match (&self.live, &self.last_build) {
            (Some(live), _) => Some(live.symbols()),
            (None, Some(build)) => Some(&build.artifacts.linked),
            (None, None) => None,
        };
        let Some(output) = symbols else {
            chrome::centered_hint(ui, "Build or run to inspect symbols.");
            return;
        };

        let filter = self.inspector.symbol_filter.trim().to_lowercase();
        let mut rows: Vec<(&str, u64)> = output
            .symbols_iter()
            .filter(|(name, _)| filter.is_empty() || name.to_lowercase().contains(&filter))
            .collect();
        rows.sort_by_key(|(name, addr)| (*addr, name.to_owned()));

        let source_files = output
            .line_table_iter()
            .map(|entry| entry.file_id)
            .collect::<BTreeSet<_>>()
            .len();

        egui::ScrollArea::vertical()
            .id_salt("symbols_scroll")
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                stat_row(
                    ui,
                    "addresses",
                    if live { "runtime" } else { "image offsets" },
                );
                stat_row(
                    ui,
                    "line table",
                    &format!(
                        "{} in {}",
                        crate::app::diagnostics::tally(output.line_table_len() as u64, "entry"),
                        crate::app::diagnostics::tally(source_files as u64, "file")
                    ),
                );
                ui.add_space(metrics::SECTION_GAP);
                chrome::hairline(ui);
                ui.add_space(metrics::SECTION_GAP);
                let total = rows.len();
                for (name, offset) in rows.into_iter().take(SYMBOL_ROW_CAP) {
                    let addr = if live {
                        LiveMachine::runtime_address(offset)
                    } else {
                        offset
                    };
                    stat_row(ui, name, &format!("{addr:08x}"));
                }
                if total > SYMBOL_ROW_CAP {
                    chrome::indented(ui, |ui| {
                        ui.label(theme::mono(
                            format!("... {} more; filter to narrow", total - SYMBOL_ROW_CAP),
                            metrics::STATUS_TEXT_SIZE,
                            theme::TEXT_FAINT,
                        ));
                    });
                } else if total == 0 {
                    chrome::indented(ui, |ui| {
                        ui.label(theme::mono(
                            "no symbols match",
                            metrics::STATUS_TEXT_SIZE,
                            theme::TEXT_FAINT,
                        ));
                    });
                }
            });
    }
}

fn stdlib_tag(policy: StdlibPolicy) -> &'static str {
    match policy {
        StdlibPolicy::None => "none",
        StdlibPolicy::ForTarget => "for target",
        StdlibPolicy::Explicit(mode) => super::target_tag(mode),
    }
}

fn closure_tag(policy: ImportClosurePolicy) -> &'static str {
    match policy {
        ImportClosurePolicy::Disabled => "root only",
        ImportClosurePolicy::Enabled {
            mangle_symbols: true,
        } => "closure, mangled",
        ImportClosurePolicy::Enabled {
            mangle_symbols: false,
        } => "closure",
    }
}
