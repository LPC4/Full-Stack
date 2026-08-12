use super::{
    CCursor, CCursorRange, DiagMark, FindHighlight, FontId, Highlighter, Layer, LayoutJob,
    LayoutSection, Sense, SourceDebug, TextStyle, egui, metrics, pos2, theme, vec2,
};

pub(super) fn read_only_code(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    text: &str,
    highlighter: Option<Highlighter>,
    jump_to: Option<usize>,
) -> CodeViewOutput {
    code_view(
        ui,
        CodeViewConfig {
            id_salt: ("stage", id_salt),
            text,
            highlighter,
            interactive: false,
            show_current_line: false,
            find: None,
            marks: &[],
            jump_to,
            debug: &SourceDebug::default(),
        },
    )
}

pub(super) fn enclosing_function(text: &str, line: usize) -> Option<String> {
    text.lines()
        .take(line.max(1))
        .filter_map(fn_def_name)
        .last()
        .map(str::to_owned)
}

// An unindented `name: (` or `name<..>: (`; excludes struct/const/impl declarations.
pub(super) fn fn_def_name(line: &str) -> Option<&str> {
    let name_end = line.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))?;
    if name_end == 0 || line.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let name = &line[..name_end];
    let mut rest = line[name_end..].trim_start();
    if let Some(generics) = rest.strip_prefix('<') {
        rest = generics[generics.find('>')?..][1..].trim_start();
    }
    rest.strip_prefix(':')
        .is_some_and(|sig| sig.trim_start().starts_with('('))
        .then_some(name)
}

// IR locates the `define name(..)` header; assembly locates the `name:` label.
pub(super) fn locate_in_stage(text: &str, layer: Layer, name: &str) -> Option<usize> {
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let hit = match layer {
            Layer::Ir => {
                let t = line.trim_start();
                let t = t.strip_prefix("export ").unwrap_or(t);
                t.strip_prefix("define ")
                    .is_some_and(|signature| signature.starts_with(&format!("{name}(")))
            }
            Layer::Asm => line
                .strip_prefix(name)
                .is_some_and(|rest| rest.trim_end() == ":"),
            Layer::Hll | Layer::Elf => false,
        };
        if hit {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

#[derive(Default)]
pub(super) struct CodeViewOutput {
    pub(super) edited: Option<String>,
    pub(super) cursor: Option<(usize, usize)>,
    pub(super) bp_toggle: Option<usize>,
    pub(super) run_to: Option<usize>,
    // True once the jump target is inside the viewport, so callers re-jump until it lands.
    pub(super) jump_landed: bool,
}

pub(super) struct CodeViewConfig<'a, Id> {
    pub(super) id_salt: Id,
    pub(super) text: &'a str,
    pub(super) highlighter: Option<Highlighter>,
    pub(super) interactive: bool,
    pub(super) show_current_line: bool,
    pub(super) find: Option<&'a FindHighlight>,
    pub(super) marks: &'a [DiagMark],
    pub(super) jump_to: Option<usize>,
    pub(super) debug: &'a SourceDebug,
}

pub(super) fn code_view<Id>(ui: &mut egui::Ui, config: CodeViewConfig<'_, Id>) -> CodeViewOutput
where
    Id: std::hash::Hash + Clone,
{
    let CodeViewConfig {
        id_salt,
        text,
        highlighter,
        interactive,
        show_current_line,
        find,
        marks,
        jump_to,
        debug,
    } = config;
    let font = TextStyle::Monospace.resolve(ui.style());
    let row_height = ui.fonts_mut(|f| f.row_height(&font));
    let rows = (ui.available_height() / row_height).floor().max(1.0) as usize;
    let line_count = text.matches('\n').count() + 1;

    let mut buf = text.to_owned();
    let mut out = CodeViewOutput::default();
    egui::ScrollArea::both()
        .id_salt(("code_scroll", id_salt.clone()))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.horizontal_top(|ui| {
                match gutter(ui, line_count, &font, marks, row_height, &debug.breakpoints) {
                    Some((line, true)) => out.run_to = Some(line),
                    Some((line, false)) => out.bp_toggle = Some(line),
                    None => {}
                }
                // Paint last frame's bands first, so this frame's text lands on
                // top of them (egui has no per-shape z within a layer).
                let dbg_id = ui.make_persistent_id(("dbg_line", &id_salt));
                paint_debug_band(ui, dbg_id);
                let hl_id = ui.make_persistent_id(("cur_line", &id_salt));
                let show_cursor_band =
                    interactive && show_current_line && debug.current_line.is_none();
                if show_cursor_band && let Some(band) = ui.data(|d| d.get_temp::<[f32; 4]>(hl_id)) {
                    let rect =
                        egui::Rect::from_min_max(pos2(band[2], band[0]), pos2(band[3], band[1]));
                    ui.painter().rect_filled(rect, 0.0, theme::CURRENT_LINE);
                    ui.ctx().request_repaint();
                }
                let find_marks = find
                    .as_ref()
                    .map(|f| (f.ranges.clone(), f.current))
                    .unwrap_or_default();
                let diag_spans: Vec<(usize, usize, egui::Color32)> = marks
                    .iter()
                    .filter_map(|mark| mark.range.map(|(start, end)| (start, end, mark.color())))
                    .collect();
                let mut layouter = highlighted_layouter(highlighter, &font, find_marks, diag_spans);
                if interactive {
                    handle_code_tab(ui, ("code_edit", id_salt.clone()), &mut buf, &mut out);
                }
                let output = egui::TextEdit::multiline(&mut buf)
                    .id_salt(("code_edit", id_salt.clone()))
                    .font(font.clone())
                    // A TextEdit drops Key::Tab by default (it is egui's focus key), so
                    // without this Tab never reaches the branch that indents.
                    .lock_focus(true)
                    .frame(egui::Frame::NONE)
                    .margin(vec2(metrics::CODE_PAD_X, 0.0))
                    .interactive(interactive)
                    .desired_width(f32::INFINITY)
                    .desired_rows(rows)
                    .layouter(&mut layouter)
                    .show(ui);
                if let Some(byte) = find.as_ref().and_then(|f| f.jump_to).or(jump_to) {
                    let chars = buf[..byte.min(buf.len())].chars().count();
                    let row = output.galley.pos_from_cursor(CCursor::new(chars));
                    let target = egui::Rect::from_min_max(
                        pos2(
                            output.galley_pos.x + row.min.x,
                            output.galley_pos.y + row.min.y,
                        ),
                        pos2(
                            output.galley_pos.x + row.max.x,
                            output.galley_pos.y + row.max.y,
                        ),
                    );
                    // Landed only once the row is inside the visible viewport;
                    // a fresh scroll area can clamp the first jump to the top.
                    let clip = ui.clip_rect();
                    out.jump_landed = target.min.y >= clip.top() && target.max.y <= clip.bottom();
                    if !out.jump_landed {
                        ui.scroll_to_rect(target.expand2(vec2(40.0, 80.0)), None);
                    }
                }
                if interactive {
                    let band = output.cursor_range.map(|range| {
                        let row = output.galley.pos_from_cursor(range.primary);
                        let clip = output.text_clip_rect;
                        [
                            output.galley_pos.y + row.min.y,
                            output.galley_pos.y + row.max.y,
                            clip.left(),
                            clip.right(),
                        ]
                    });
                    ui.data_mut(|d| match band {
                        Some(band) => {
                            d.insert_temp(hl_id, band);
                        }
                        None => {
                            d.remove::<[f32; 4]>(hl_id);
                        }
                    });
                    out.cursor = output
                        .cursor_range
                        .map(|range| line_col(&buf, range.primary.index));
                    if output.response.changed() {
                        out.edited = Some(buf.clone());
                    }
                }
                hover_diagnostic_spans(ui, &output, &buf, marks);
                // Stash the paused line's row band to paint under next frame's text.
                let dbg_band = debug
                    .current_line
                    .map(|line| line_band(&output, &buf, line));
                ui.data_mut(|d| match dbg_band {
                    Some(band) => {
                        d.insert_temp(dbg_id, band);
                    }
                    None => {
                        d.remove::<[f32; 4]>(dbg_id);
                    }
                });
            });
        });
    out
}

// Consume Shift+Tab first because egui's plain Tab match also accepts held Shift.
// Edits use spaces and preserve four-column tab stops.
pub(super) fn handle_code_tab<Id>(
    ui: &egui::Ui,
    id_salt: Id,
    buf: &mut String,
    out: &mut CodeViewOutput,
) where
    Id: std::hash::Hash + Clone,
{
    // The TextEdit derives its id from `.id_salt(x)` as `Id::new(x)` hashed
    // into the parent Ui, so mirror that exactly or focus/state lookups miss.
    let edit_id = ui.make_persistent_id(egui::Id::new(id_salt));
    if ui.ctx().memory(|m| m.focused() != Some(edit_id)) {
        return;
    }
    let outdent = ui.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab));
    let indent = !outdent && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
    if !indent && !outdent {
        return;
    }
    let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), edit_id) else {
        return;
    };
    let (primary, secondary) = match state.cursor.char_range() {
        Some(range) => (range.primary.index, range.secondary.index),
        None => (0, 0),
    };
    let (primary, secondary) = apply_tab_edit(buf, primary, secondary, outdent);
    state.cursor.set_char_range(Some(CCursorRange {
        primary: CCursor::new(primary),
        secondary: CCursor::new(secondary),
        h_pos: None,
    }));
    egui::TextEdit::store_state(ui.ctx(), edit_id, state);
    out.edited = Some(buf.clone());
    out.cursor = Some(line_col(buf, primary));
}

pub(super) fn hover_diagnostic_spans(
    ui: &egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    buf: &str,
    marks: &[DiagMark],
) {
    for (i, mark) in marks.iter().enumerate() {
        let Some((start, end)) = mark.range else {
            continue;
        };
        // The live buffer may be newer than the marks; skip stale ranges.
        if end > buf.len() || !buf.is_char_boundary(start) || !buf.is_char_boundary(end) {
            continue;
        }
        let from = buf[..start].chars().count();
        let to = from + buf[start..end].chars().count();
        let head = output.galley.pos_from_cursor(CCursor::new(from));
        let tail = output.galley.pos_from_cursor(CCursor::new(to));
        let rect = egui::Rect::from_min_max(
            output.galley_pos + head.min.to_vec2(),
            output.galley_pos + tail.max.to_vec2(),
        );
        ui.interact(rect, ui.id().with(("diag_span", i)), Sense::hover())
            .on_hover_text(theme::mono(
                mark.message.clone(),
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT,
            ));
    }
}

// Painted from coords stashed last frame, so this frame's text lands on top.
pub(super) fn paint_debug_band(ui: &egui::Ui, dbg_id: egui::Id) {
    let Some(band) = ui.data(|d| d.get_temp::<[f32; 4]>(dbg_id)) else {
        return;
    };
    let rect = egui::Rect::from_min_max(pos2(band[2], band[0]), pos2(band[3], band[1]));
    ui.painter().rect_filled(rect, 0.0, theme::SURFACE);
    let bar = egui::Rect::from_min_max(rect.left_top(), pos2(band[2] + 2.0, band[1]));
    ui.painter().rect_filled(bar, 0.0, theme::ELECTRIC);
}

pub(super) fn line_band(
    output: &egui::text_edit::TextEditOutput,
    buf: &str,
    line: usize,
) -> [f32; 4] {
    let byte = super::super::diagnostics::byte_offset_of_line(buf, line);
    let chars = buf[..byte.min(buf.len())].chars().count();
    let row = output.galley.pos_from_cursor(CCursor::new(chars));
    let clip = output.text_clip_rect;
    [
        output.galley_pos.y + row.min.y,
        output.galley_pos.y + row.max.y,
        clip.left(),
        clip.right(),
    ]
}

const TAB_WIDTH: usize = 4;

// Cursor offsets are character indices, not byte offsets.
pub(super) fn apply_tab_edit(
    buf: &mut String,
    primary: usize,
    secondary: usize,
    outdent: bool,
) -> (usize, usize) {
    if primary == secondary {
        let at = primary;
        if !outdent {
            let column = at - line_start_char(buf, at);
            let spaces = TAB_WIDTH - (column % TAB_WIDTH);
            buf.insert_str(char_to_byte(buf, at), &" ".repeat(spaces));
            (at + spaces, at + spaces)
        } else {
            let line_start = line_start_char(buf, at);
            let removed = leading_outdent_len(buf, line_start);
            if removed == 0 {
                (at, at)
            } else {
                let from = char_to_byte(buf, line_start);
                let to = char_to_byte(buf, line_start + removed);
                buf.replace_range(from..to, "");
                let new_at = if at >= line_start + removed {
                    at - removed
                } else {
                    line_start
                };
                (new_at, new_at)
            }
        }
    } else {
        let (min, max) = if primary < secondary {
            (primary, secondary)
        } else {
            (secondary, primary)
        };
        let starts = line_starts_in(buf, min, max);
        if !outdent {
            let n = starts.len();
            for &line_start in starts.iter().rev() {
                buf.insert_str(char_to_byte(buf, line_start), "    ");
            }
            let new_min = min + TAB_WIDTH;
            let new_max = max + TAB_WIDTH * n;
            if primary < secondary {
                (new_min, new_max)
            } else {
                (new_max, new_min)
            }
        } else {
            let removals: Vec<(usize, usize)> = starts
                .iter()
                .map(|&line_start| (line_start, leading_outdent_len(buf, line_start)))
                .filter(|&(_, removed)| removed > 0)
                .collect();
            for &(line_start, removed) in removals.iter().rev() {
                let from = char_to_byte(buf, line_start);
                let to = char_to_byte(buf, line_start + removed);
                buf.replace_range(from..to, "");
            }
            let (mut new_min, mut new_max) = (min, max);
            for &(line_start, removed) in removals.iter().rev() {
                new_min = remap_after_removal(new_min, line_start, removed);
                new_max = remap_after_removal(new_max, line_start, removed);
            }
            if primary < secondary {
                (new_min, new_max)
            } else {
                (new_max, new_min)
            }
        }
    }
}

pub(super) fn remap_after_removal(edge: usize, at: usize, removed: usize) -> usize {
    if edge >= at + removed {
        edge - removed
    } else if edge > at {
        at
    } else {
        edge
    }
}

pub(super) fn char_to_byte(text: &str, char_idx: usize) -> usize {
    text.char_indices()
        .nth(char_idx)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len())
}

pub(super) fn line_start_char(text: &str, char_idx: usize) -> usize {
    let byte = char_to_byte(text, char_idx);
    text[..byte]
        .rfind('\n')
        .map(|newline| text[..newline].chars().count() + 1)
        .unwrap_or(0)
}

// Char index of the first newline at or after `from`.
pub(super) fn next_newline_char(text: &str, from: usize) -> Option<usize> {
    text.char_indices()
        .skip(from)
        .find(|(_, c)| *c == '\n')
        .map(|(byte, _)| text[..byte].chars().count())
}

// Starts (in chars) of the lines a selection [min, max) touches: the line
// holding `min`, plus every later line whose start lies inside the selection.
pub(super) fn line_starts_in(text: &str, min: usize, max: usize) -> Vec<usize> {
    let limit = if max > min && text.chars().nth(max - 1) == Some('\n') {
        max - 1
    } else {
        max
    };
    let mut starts = vec![line_start_char(text, min)];
    let mut newline = next_newline_char(text, starts[0]);
    while let Some(nl) = newline {
        if nl >= limit {
            break;
        }
        starts.push(nl + 1);
        newline = next_newline_char(text, nl + 1);
    }
    starts
}

// Aligned indentation removes a full tab stop; partial indentation returns to its boundary.
pub(super) fn leading_outdent_len(text: &str, line_start: usize) -> usize {
    let mut rest = text[char_to_byte(text, line_start)..].chars();
    match rest.next() {
        Some('\t') => 1,
        Some(' ') => {
            let spaces = 1 + rest.take_while(|c| *c == ' ').count();
            let rem = spaces % TAB_WIDTH;
            if rem == 0 { TAB_WIDTH.min(spaces) } else { rem }
        }
        _ => 0,
    }
}

pub(super) fn line_col(text: &str, char_idx: usize) -> (usize, usize) {
    let (mut line, mut col) = (1, 1);
    for c in text.chars().take(char_idx) {
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

pub(super) fn gutter(
    ui: &mut egui::Ui,
    line_count: usize,
    font: &FontId,
    marks: &[DiagMark],
    row_height: f32,
    breakpoints: &[usize],
) -> Option<(usize, bool)> {
    let digits = line_count.to_string().len().max(2);
    let mut nums = String::with_capacity(line_count * (digits + 1));
    for i in 1..=line_count {
        if i > 1 {
            nums.push('\n');
        }
        nums.push_str(&format!("{i:>digits$}"));
    }
    let galley = ui.fonts_mut(|f| f.layout_no_wrap(nums, font.clone(), theme::TEXT_FAINT));
    // Rows are pixel-snapped, so their pitch can differ from the font's nominal
    // row height; measure it or gutter marks drift below the code lines.
    let pitch = match galley.rows.get(1) {
        Some(row) => row.pos.y - galley.rows[0].pos.y,
        None => row_height,
    };
    let width = galley.size().x + metrics::CODE_PAD_X + metrics::GUTTER_GAP;
    let (rect, _) = ui.allocate_exact_size(vec2(width, galley.size().y), Sense::hover());
    ui.painter().galley(
        pos2(rect.left() + metrics::CODE_PAD_X, rect.top()),
        galley,
        theme::TEXT_FAINT,
    );

    let dot_x = rect.left() + 6.0;
    let dot_y = |line: usize| rect.top() + (line as f32 - 0.5) * pitch;
    for &line in breakpoints {
        if line >= 1 && line <= line_count {
            ui.painter()
                .circle_filled(pos2(dot_x, dot_y(line)), 3.0, theme::DANGER);
        }
    }
    let mut toggled = None;
    for line in 1..=line_count {
        let line_rect = egui::Rect::from_min_size(
            pos2(rect.left(), rect.top() + (line - 1) as f32 * pitch),
            vec2(rect.width(), pitch),
        );
        let resp = ui.interact(line_rect, ui.id().with(("gutter_bp", line)), Sense::click());
        resp.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                format!("toggle breakpoint line {line}"),
            )
        });
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            if !breakpoints.contains(&line) {
                ui.painter().circle_stroke(
                    pos2(dot_x, dot_y(line)),
                    3.0,
                    egui::Stroke::new(1.0, theme::TEXT_FAINT),
                );
            }
        }
        if resp.clicked() {
            toggled = Some((line, ui.input(|i| i.modifiers.command)));
        }
    }

    for (i, mark) in marks.iter().enumerate() {
        if mark.line == 0 || mark.line > line_count {
            continue;
        }
        let top = rect.top() + (mark.line - 1) as f32 * pitch;
        let bar = egui::Rect::from_min_size(pos2(rect.left(), top + 1.0), vec2(3.0, pitch - 2.0));
        ui.painter().rect_filled(bar, 0.0, mark.color());
        let hover = egui::Rect::from_min_size(pos2(rect.left(), top), vec2(width, pitch));
        ui.interact(hover, ui.id().with(("diag_mark", i)), Sense::hover())
            .on_hover_text(theme::mono(
                mark.message.clone(),
                metrics::STATUS_TEXT_SIZE,
                theme::TEXT,
            ));
    }
    toggled
}

pub(super) fn highlighted_layouter(
    highlighter: Option<Highlighter>,
    font: &FontId,
    find_marks: (Vec<(usize, usize)>, usize),
    diag_spans: Vec<(usize, usize, egui::Color32)>,
) -> impl FnMut(&egui::Ui, &dyn egui::TextBuffer, f32) -> std::sync::Arc<egui::Galley> {
    let font = font.clone();
    move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
        let mut job = highlighter
            .map(|highlight| highlight(ui.style(), buf.as_str()))
            .unwrap_or_else(|| {
                LayoutJob::simple(buf.as_str().to_owned(), font.clone(), theme::TEXT, wrap)
            });
        apply_match_backgrounds(&mut job, &find_marks.0, find_marks.1);
        apply_diagnostic_underlines(&mut job, &diag_spans);
        job.wrap.max_width = wrap;
        ui.fonts_mut(|f| f.layout_job(job))
    }
}

pub(super) fn apply_match_backgrounds(
    job: &mut LayoutJob,
    ranges: &[(usize, usize)],
    current: usize,
) {
    let ranges = valid_ranges(&job.text, ranges.iter().copied());
    restyle_ranges(job, &ranges, |idx, format| {
        format.background = if idx == current {
            theme::FIND_MATCH_CURRENT
        } else {
            theme::FIND_MATCH
        };
    });
}

pub(super) fn apply_diagnostic_underlines(
    job: &mut LayoutJob,
    spans: &[(usize, usize, egui::Color32)],
) {
    let ranges = valid_ranges(&job.text, spans.iter().map(|&(start, end, _)| (start, end)));
    let colors: Vec<egui::Color32> = spans.iter().map(|&(_, _, color)| color).collect();
    restyle_ranges(job, &ranges, |idx, format| {
        format.underline = egui::Stroke::new(1.0, colors[idx]);
        format.background = if colors[idx] == theme::DANGER {
            theme::DANGER_WASH
        } else {
            theme::WARNING_WASH
        };
    });
}

// Ranges are validated against the live buffer, which may be newer than them.
pub(super) fn valid_ranges(
    text: &str,
    ranges: impl Iterator<Item = (usize, usize)>,
) -> Vec<(usize, usize)> {
    ranges
        .filter(|&(start, end)| {
            end <= text.len() && text.is_char_boundary(start) && text.is_char_boundary(end)
        })
        .collect()
}

pub(super) fn restyle_ranges(
    job: &mut LayoutJob,
    ranges: &[(usize, usize)],
    mut restyle: impl FnMut(usize, &mut egui::text::TextFormat),
) {
    if ranges.is_empty() {
        return;
    }
    let mut sections = Vec::with_capacity(job.sections.len() + ranges.len() * 2);
    for section in job.sections.drain(..) {
        let (start, end) = (section.byte_range.start, section.byte_range.end);
        let mut cuts = vec![start];
        for &(range_start, range_end) in ranges {
            if range_start > start && range_start < end {
                cuts.push(range_start);
            }
            if range_end > start && range_end < end {
                cuts.push(range_end);
            }
        }
        cuts.push(end);
        cuts.sort_unstable();
        cuts.dedup();

        let mut leading_space = section.leading_space;
        for window in cuts.windows(2) {
            let (a, b) = (window[0], window[1]);
            let mut format = section.format.clone();
            // Every window is fully inside or fully outside a range, because the
            // cuts include all range boundaries that land within the section.
            if let Some(idx) = ranges
                .iter()
                .position(|&(range_start, range_end)| a >= range_start && b <= range_end)
            {
                restyle(idx, &mut format);
            }
            sections.push(LayoutSection {
                leading_space,
                byte_range: a..b,
                format,
            });
            leading_space = 0.0;
        }
    }
    job.sections = sections;
}

pub(in crate::app) fn console_view(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    text: &str,
    stick_to_bottom: bool,
) {
    let font = TextStyle::Monospace.resolve(ui.style());
    let mut buf = text.to_owned();
    egui::ScrollArea::both()
        .id_salt(("console_scroll", id_salt))
        .stick_to_bottom(stick_to_bottom)
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut buf)
                    .id_salt(("console_edit", id_salt))
                    .font(font)
                    .frame(egui::Frame::NONE)
                    .margin(vec2(metrics::CODE_PAD_X, 0.0))
                    .interactive(false)
                    .desired_width(f32::INFINITY),
            );
        });
}
