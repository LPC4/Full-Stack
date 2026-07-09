//! Explorer: the workspace file tree with type chips, dirty dots, and
//! right-click file operations (new file/folder, rename, delete).

use super::{FullStackApp, chrome, metrics};
use crate::ui::theme;
use eframe::egui;
use egui::collapsing_header::CollapsingState;
use egui::{Align2, FontId, Sense, Stroke, pos2, vec2};
use full_stack::workspace::{EntryKind, TreeEntry, VPath};

/// Per-workspace explorer UI state: the in-flight inline edit and delete confirm.
#[derive(Default)]
pub(crate) struct ExplorerState {
    edit: Option<EditState>,
    confirm_delete: Option<VPath>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditAction {
    NewFile,
    NewDir,
    Rename,
}

impl EditAction {
    fn hint(self) -> &'static str {
        match self {
            Self::NewFile => "file name",
            Self::NewDir => "folder name",
            Self::Rename => "new name",
        }
    }

    fn verb(self) -> &'static str {
        match self {
            Self::NewFile | Self::NewDir => "create",
            Self::Rename => "rename",
        }
    }
}

/// An inline naming row: creating inside `target` (a directory) or renaming
/// `target` itself.
struct EditState {
    action: EditAction,
    target: VPath,
    buffer: String,
    focus: bool,
    /// Set by the edit row: Some(true) commits, Some(false) cancels.
    done: Option<bool>,
}

impl EditState {
    fn new(action: EditAction, target: VPath, buffer: String) -> Self {
        Self {
            action,
            target,
            buffer,
            focus: true,
            done: None,
        }
    }

    /// True when this edit inserts a new entry inside directory `dir`.
    fn creates_in(&self, dir: &VPath) -> bool {
        self.action != EditAction::Rename && self.target == *dir
    }

    fn renames(&self, path: &VPath) -> bool {
        self.action == EditAction::Rename && self.target == *path
    }
}

/// A row interaction, applied after the tree walk releases its borrows.
enum ExplorerAction {
    Open(VPath),
    StartNewFile(VPath),
    StartNewDir(VPath),
    StartRename(VPath),
    AskDelete(VPath),
}

impl FullStackApp {
    pub(super) fn explorer_panel(&mut self, ui: &mut egui::Ui) {
        chrome::panel_header(ui, "Explorer");

        let active = self.workspace.active_document().map(|d| d.path().clone());
        let dirty: Vec<VPath> = self
            .workspace
            .dirty_documents()
            .map(|doc| doc.path().clone())
            .collect();
        let can_edit = !self.workspace.fs().read_only();
        // The edit state leaves `self` for the walk so the tree can stay borrowed.
        let mut edit = self.explorer.edit.take();
        let mut actions: Vec<ExplorerAction> = Vec::new();

        egui::ScrollArea::vertical()
            .id_salt("explorer_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if let Some(edit) = edit.as_mut().filter(|edit| edit.creates_in(&VPath::root())) {
                    edit_row(ui, 0, edit);
                }
                let tree = self.workspace.tree();
                let mut tree_ctx = TreeNodeContext {
                    active: active.as_ref(),
                    dirty: &dirty,
                    can_edit,
                    edit: &mut edit,
                    actions: &mut actions,
                };
                for entry in &tree.root {
                    tree_node(ui, entry, 0, &mut tree_ctx);
                }
                // The empty well below the tree offers root-level creation.
                let leftover = ui.available_height().max(metrics::ROW_HEIGHT * 2.0);
                let (_, resp) =
                    ui.allocate_exact_size(vec2(ui.available_width(), leftover), Sense::click());
                if can_edit {
                    resp.context_menu(|ui| {
                        if menu_item(ui, "new file").clicked() {
                            actions.push(ExplorerAction::StartNewFile(VPath::root()));
                        }
                        if menu_item(ui, "new folder").clicked() {
                            actions.push(ExplorerAction::StartNewDir(VPath::root()));
                        }
                    });
                }
            });

        self.explorer.edit = edit;
        for action in actions {
            self.apply_explorer_action(action);
        }
        self.finish_explorer_edit();
    }

    fn apply_explorer_action(&mut self, action: ExplorerAction) {
        match action {
            ExplorerAction::Open(path) => self.open_path(path),
            ExplorerAction::StartNewFile(dir) => {
                self.explorer.edit = Some(EditState::new(EditAction::NewFile, dir, String::new()));
            }
            ExplorerAction::StartNewDir(dir) => {
                self.explorer.edit = Some(EditState::new(EditAction::NewDir, dir, String::new()));
            }
            ExplorerAction::StartRename(path) => {
                let name = path.file_name().unwrap_or_default().to_owned();
                self.explorer.edit = Some(EditState::new(EditAction::Rename, path, name));
            }
            ExplorerAction::AskDelete(path) => self.explorer.confirm_delete = Some(path),
        }
    }

    /// Resolve a finished inline edit into the matching workspace operation.
    fn finish_explorer_edit(&mut self) {
        if self.explorer.edit.as_ref().is_none_or(|e| e.done.is_none()) {
            return;
        }
        let edit = self.explorer.edit.take().expect("checked above");
        if edit.done != Some(true) {
            return;
        }
        let name = edit.buffer.trim();
        if name.is_empty() {
            return;
        }
        if name.contains('/') || name.contains('\\') {
            self.status_message = Some("names cannot contain path separators".to_owned());
            return;
        }
        let base = match edit.action {
            EditAction::NewFile | EditAction::NewDir => edit.target.clone(),
            EditAction::Rename => edit.target.parent().unwrap_or_else(VPath::root),
        };
        let path = match base.join(name) {
            Ok(path) => path,
            Err(err) => {
                self.status_message = Some(format!("{} failed: {err}", edit.action.verb()));
                return;
            }
        };
        let result = match edit.action {
            EditAction::NewFile => self.workspace.create_file(&path),
            EditAction::NewDir => self.workspace.create_dir(&path),
            EditAction::Rename => {
                if path == edit.target {
                    return;
                }
                self.workspace.rename_entry(&edit.target, &path)
            }
        };
        match result {
            Ok(()) => {
                if edit.action == EditAction::NewFile {
                    self.open_path(path.clone());
                }
                if edit.action == EditAction::Rename {
                    // The active document may now live at a new path; resync
                    // the layer, stage cache, and build target.
                    self.document_focus_changed();
                }
                self.status_message = Some(match edit.action {
                    EditAction::NewFile | EditAction::NewDir => format!("created {path}"),
                    EditAction::Rename => format!("renamed to {path}"),
                });
            }
            Err(err) => {
                self.status_message = Some(format!("{} failed: {err}", edit.action.verb()));
            }
        }
    }

    /// The delete confirmation, floating above the shell like quick open.
    pub(super) fn explorer_confirm_overlay(&mut self, ctx: &egui::Context) {
        let Some(path) = self.explorer.confirm_delete.clone() else {
            return;
        };
        let mut decided: Option<bool> = None;
        egui::Area::new(egui::Id::new("explorer_confirm_delete"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_TOP, vec2(0.0, 120.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::CHROME)
                    .stroke(Stroke::new(1.0, theme::HAIRLINE))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(340.0);
                        ui.label(theme::label("delete"));
                        ui.add_space(metrics::SECTION_GAP);
                        ui.label(theme::mono(
                            path.to_string(),
                            metrics::LABEL_TEXT_SIZE,
                            theme::TEXT,
                        ));
                        ui.label(theme::mono(
                            "This cannot be undone; open tabs under it close.",
                            metrics::STATUS_TEXT_SIZE,
                            theme::TEXT_DIM,
                        ));
                        ui.add_space(metrics::SECTION_GAP);
                        ui.horizontal(|ui| {
                            if ui
                                .button(theme::mono(
                                    "delete",
                                    metrics::STATUS_TEXT_SIZE,
                                    theme::DANGER,
                                ))
                                .clicked()
                            {
                                decided = Some(true);
                            }
                            if ui
                                .button(theme::mono(
                                    "cancel",
                                    metrics::STATUS_TEXT_SIZE,
                                    theme::TEXT_DIM,
                                ))
                                .clicked()
                            {
                                decided = Some(false);
                            }
                        });
                    });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            decided = Some(false);
        }
        match decided {
            Some(true) => {
                self.explorer.confirm_delete = None;
                match self.workspace.remove_entry(&path) {
                    Ok(()) => {
                        self.status_message = Some(format!("deleted {path}"));
                        self.document_focus_changed();
                    }
                    Err(err) => {
                        self.status_message = Some(format!("delete failed: {err}"));
                    }
                }
            }
            Some(false) => self.explorer.confirm_delete = None,
            None => {}
        }
    }
}

struct TreeNodeContext<'a> {
    active: Option<&'a VPath>,
    dirty: &'a [VPath],
    can_edit: bool,
    edit: &'a mut Option<EditState>,
    actions: &'a mut Vec<ExplorerAction>,
}

fn tree_node(ui: &mut egui::Ui, entry: &TreeEntry, depth: usize, ctx: &mut TreeNodeContext<'_>) {
    match entry.kind {
        EntryKind::Dir => {
            let id = ui.make_persistent_id(("explorer_dir", entry.path.as_str()));
            let mut state = CollapsingState::load_with_default_open(
                ui.ctx(),
                id,
                entry.path.as_str().is_empty(),
            );
            // A pending "new" edit inside this directory forces it open so the
            // naming row is visible where the entry will appear.
            if ctx.edit.as_ref().is_some_and(|e| e.creates_in(&entry.path)) {
                state.set_open(true);
            }
            let openness = state.openness(ui.ctx());
            if let Some(edit) = ctx.edit.as_mut().filter(|e| e.renames(&entry.path)) {
                edit_row(ui, depth, edit);
            } else {
                let resp = row(
                    ui,
                    depth,
                    &entry.name,
                    RowKind::Dir { openness },
                    false,
                    false,
                );
                if resp.clicked() {
                    state.toggle(ui);
                }
                if ctx.can_edit {
                    resp.context_menu(|ui| {
                        if menu_item(ui, "new file").clicked() {
                            ctx.actions
                                .push(ExplorerAction::StartNewFile(entry.path.clone()));
                        }
                        if menu_item(ui, "new folder").clicked() {
                            ctx.actions
                                .push(ExplorerAction::StartNewDir(entry.path.clone()));
                        }
                        ui.separator();
                        if menu_item(ui, "rename").clicked() {
                            ctx.actions
                                .push(ExplorerAction::StartRename(entry.path.clone()));
                        }
                        if menu_item_danger(ui, "delete").clicked() {
                            ctx.actions
                                .push(ExplorerAction::AskDelete(entry.path.clone()));
                        }
                    });
                }
            }
            state.store(ui.ctx());
            if state.is_open() {
                if let Some(edit) = ctx.edit.as_mut().filter(|e| e.creates_in(&entry.path)) {
                    edit_row(ui, depth + 1, edit);
                }
                for child in &entry.children {
                    tree_node(ui, child, depth + 1, ctx);
                }
            }
        }
        EntryKind::File => {
            if let Some(edit) = ctx.edit.as_mut().filter(|e| e.renames(&entry.path)) {
                edit_row(ui, depth, edit);
                return;
            }
            let is_active = ctx.active == Some(&entry.path);
            let is_dirty = ctx.dirty.contains(&entry.path);
            let resp = row(ui, depth, &entry.name, RowKind::File, is_active, is_dirty);
            if resp.clicked() {
                ctx.actions.push(ExplorerAction::Open(entry.path.clone()));
            }
            if ctx.can_edit {
                resp.context_menu(|ui| {
                    if menu_item(ui, "rename").clicked() {
                        ctx.actions
                            .push(ExplorerAction::StartRename(entry.path.clone()));
                    }
                    if menu_item_danger(ui, "delete").clicked() {
                        ctx.actions
                            .push(ExplorerAction::AskDelete(entry.path.clone()));
                    }
                });
            }
        }
    }
}

/// The inline naming row. Enter commits; Escape or clicking away cancels.
fn edit_row(ui: &mut egui::Ui, depth: usize, edit: &mut EditState) {
    let indent = metrics::PANEL_INSET_X + depth as f32 * metrics::TREE_INDENT;
    ui.horizontal(|ui| {
        ui.add_space(indent + 13.0);
        let resp = ui.add(
            egui::TextEdit::singleline(&mut edit.buffer)
                .id_salt(("explorer_edit", edit.target.as_str(), edit.action.verb()))
                .font(egui::TextStyle::Monospace)
                .text_color(theme::TEXT)
                .hint_text(theme::mono(
                    edit.action.hint(),
                    metrics::LABEL_TEXT_SIZE,
                    theme::TEXT_FAINT,
                ))
                .desired_width(
                    (ui.available_width() - metrics::PANEL_INSET_X).max(metrics::TREE_INDENT),
                ),
        );
        if edit.focus {
            resp.request_focus();
            edit.focus = false;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            edit.done = Some(false);
        } else if resp.lost_focus() {
            let commit = ui.input(|i| i.key_pressed(egui::Key::Enter));
            edit.done = Some(commit);
        }
    });
}

fn menu_item(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.button(theme::mono(label, metrics::STATUS_TEXT_SIZE, theme::TEXT))
}

fn menu_item_danger(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.button(theme::mono(label, metrics::STATUS_TEXT_SIZE, theme::DANGER))
}

#[derive(Clone, Copy)]
enum RowKind {
    /// A directory row; `openness` in 0..1 drives the chevron rotation.
    Dir {
        openness: f32,
    },
    File,
}

/// The file-type chip color: source kinds pick up their depth-spectrum color, so
/// the explorer quietly restates the identity the spine carries.
pub(super) fn file_chip_color(name: &str) -> egui::Color32 {
    if name.ends_with(".hll") {
        crate::ui::Layer::Hll.color()
    } else if name.ends_with(".ir") {
        crate::ui::Layer::Ir.color()
    } else if name.ends_with(".s") {
        crate::ui::Layer::Asm.color()
    } else if name.ends_with(".elf") || name.ends_with(".o") {
        crate::ui::Layer::Elf.color()
    } else if name.ends_with(".build") || name.ends_with(".toml") {
        theme::TEXT_DIM
    } else {
        theme::TEXT_FAINT
    }
}

/// One full-width tree row: hover/active fill, indent guides, chevron or type
/// chip, label, and an unsaved dot for dirty open documents.
fn row(
    ui: &mut egui::Ui,
    depth: usize,
    name: &str,
    kind: RowKind,
    active: bool,
    dirty: bool,
) -> egui::Response {
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

    let indent = metrics::PANEL_INSET_X + depth as f32 * metrics::TREE_INDENT;
    for d in 0..depth {
        let gx = rect.left() + metrics::PANEL_INSET_X + d as f32 * metrics::TREE_INDENT + 3.0;
        painter.line_segment(
            [pos2(gx, rect.top()), pos2(gx, rect.bottom())],
            Stroke::new(1.0, theme::HAIRLINE_SOFT),
        );
    }

    let mid_y = rect.center().y;
    let (text_x, color) = match kind {
        RowKind::Dir { openness } => {
            chevron(&painter, pos2(rect.left() + indent + 4.0, mid_y), openness);
            (rect.left() + indent + 14.0, theme::TEXT_DIM)
        }
        RowKind::File => {
            let chip = egui::Rect::from_center_size(
                pos2(rect.left() + indent + 4.5, mid_y),
                vec2(5.0, 5.0),
            );
            painter.rect_filled(chip, 0.0, file_chip_color(name));
            let color = if active { theme::ELECTRIC } else { theme::TEXT };
            (rect.left() + indent + 13.0, color)
        }
    };
    let text_rect = painter.text(
        pos2(text_x, mid_y),
        Align2::LEFT_CENTER,
        name,
        FontId::monospace(metrics::LABEL_TEXT_SIZE),
        color,
    );
    if dirty {
        painter.circle_filled(pos2(text_rect.right() + 7.0, mid_y), 2.5, theme::WARNING);
    }
    resp
}

/// A disclosure chevron rotating from ">" (closed) to "v" (open) by `openness`.
fn chevron(painter: &egui::Painter, c: egui::Pos2, openness: f32) {
    let ang = openness * std::f32::consts::FRAC_PI_2;
    let (s, co) = ang.sin_cos();
    let rot = |dx: f32, dy: f32| pos2(c.x + dx * co - dy * s, c.y + dx * s + dy * co);
    let a = 3.0;
    let (p1, p2, p3) = (rot(-a * 0.5, -a), rot(a * 0.5, 0.0), rot(-a * 0.5, a));
    let stroke = Stroke::new(1.2, theme::TEXT_DIM);
    painter.line_segment([p1, p2], stroke);
    painter.line_segment([p2, p3], stroke);
}
