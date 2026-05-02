// Layout utilities for UI grid and sizing calculations

/// Estimates the character width for monospace fonts.
pub fn estimated_monospace_char_width(ui: &egui::Ui) -> f32 {
    let font_id = egui::TextStyle::Monospace.resolve(ui.style());
    let sample = "W".repeat(40);
    let galley = ui
        .painter()
        .layout_no_wrap(sample, font_id, ui.visuals().text_color());
    (galley.size().x / 40.0).max(1.0)
}

/// Automatically calculates the number of grid columns based on available space
/// and desired pane sizes.
pub fn auto_grid_columns(ui: &egui::Ui, pane_count: usize) -> usize {
    let available = ui.available_rect_before_wrap();
    let char_width = estimated_monospace_char_width(ui);
    let target_pane_width = char_width * 40.0;

    let monospace_line_height = ui.text_style_height(&egui::TextStyle::Monospace);
    let target_pane_height = monospace_line_height * 18.0 + 72.0;

    let columns_by_width = (available.width() / target_pane_width).floor().max(1.0) as usize;
    let rows_by_height = (available.height() / target_pane_height).floor().max(1.0) as usize;
    let columns_needed_for_height = pane_count.div_ceil(rows_by_height);

    columns_needed_for_height
        .min(columns_by_width)
        .clamp(1, pane_count.max(1))
}

/// Calculates responsive column count based on minimum column width.
/// Returns a value between min_cols and max_cols based on available width.
pub fn auto_grid_columns_with_min_width(
    ui: &egui::Ui,
    min_column_width: f32,
    min_cols: usize,
    max_cols: usize,
) -> usize {
    let available_width = ui.available_width();
    let cols_by_width = (available_width / min_column_width).floor() as usize;
    cols_by_width.clamp(min_cols, max_cols)
}

/// Splits a rectangle into a grid of equally-sized cells.
/// Returns a vector of rectangles representing each cell's bounds.
pub fn split_rect_into_grid(
    rect: egui::Rect,
    pane_count: usize,
    columns: usize,
) -> Vec<egui::Rect> {
    let columns = columns.max(1);
    let rows = pane_count.div_ceil(columns).max(1);
    let cell_width = rect.width() / columns as f32;
    let cell_height = rect.height() / rows as f32;

    let mut rects = Vec::with_capacity(pane_count);
    for index in 0..pane_count {
        let row = index / columns;
        let column = index % columns;

        let min_x = rect.min.x + column as f32 * cell_width;
        let min_y = rect.min.y + row as f32 * cell_height;
        let max_x = if column + 1 == columns {
            rect.max.x
        } else {
            rect.min.x + (column + 1) as f32 * cell_width
        };
        let max_y = if row + 1 == rows {
            rect.max.y
        } else {
            rect.min.y + (row + 1) as f32 * cell_height
        };

        rects.push(egui::Rect::from_min_max(
            egui::pos2(min_x, min_y),
            egui::pos2(max_x, max_y),
        ));
    }

    rects
}
