use crate::view::{CompilationState, CompilerView, ProgramCatalog};
use egui::epaint::PathStroke;
use egui::{Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};
use std::collections::HashMap;

const PAGE_SIZE: usize = 4096; // 4KB Pages

#[derive(Clone)]
struct VmSegment {
    name: String,
    va_start: u64,
    size_bytes: usize,
    color: Color32,
    target_pa: Option<u64>,
    description: String,
}

#[derive(Clone)]
struct PmBlock {
    name: String,
    pa_start: u64,
    size_bytes: usize,
    color: Color32,
    owner_pid: u32,
}

#[derive(Clone)]
struct Process {
    pid: u32,
    name: String,
    segments: Vec<VmSegment>,
}

#[derive(Clone)]
struct MmuState {
    processes: Vec<Process>,
    phys_mem: Vec<PmBlock>,
}

#[derive(Clone)]
pub struct MemoryMapView {
    selected_pid: u32,
}

impl Default for MemoryMapView {
    fn default() -> Self {
        Self { selected_pid: 1 }
    }
}

// Helper to format bytes into KB/MB nicely
fn format_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{} MB", bytes / (1024 * 1024))
    } else if bytes >= 1024 {
        format!("{} KB", bytes / 1024)
    } else {
        format!("{} B", bytes)
    }
}

impl CompilerView for MemoryMapView {
    fn title(&self) -> &'static str {
        "MMU & Virtual Memory"
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &egui::Context,
        state: &mut CompilationState,
        _catalog: &mut ProgramCatalog,
    ) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.heading("MMU Page Translation");
                if state.asm.is_empty() {
                    ui.label(RichText::new("(Schematic)").weak().italics());
                } else {
                    ui.label(RichText::new("(Live Program Mappings)").color(Color32::from_rgb(100, 200, 100)));
                }
            });
            ui.add_space(4.0);
            ui.label(RichText::new("Showing Sv39 Virtual Memory Translation. The MMU uses Page Tables to map isolated Virtual Spaces into shared Physical RAM.").weak());
            ui.separator();

            let mmu_state = build_mmu_state(&state.asm);

            ui.horizontal(|ui| {
                ui.label(RichText::new("Active Context (satp register):").strong());
                for process in &mmu_state.processes {
                    if ui.selectable_label(self.selected_pid == process.pid, &process.name).clicked() {
                        self.selected_pid = process.pid;
                    }
                }
            });
            ui.add_space(8.0);

            let active_process = mmu_state.processes.iter().find(|p| p.pid == self.selected_pid).unwrap();

            // Use vertical scroll ONLY so ui.available_width() functions properly
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(10.0);
                    draw_mmu_visualization(ui, active_process, &mmu_state.phys_mem);
                });
        });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}

/// Parses the user's assembly and mocks an OS environment around it.
fn build_mmu_state(asm: &str) -> MmuState {
    let mut text_sz = 0;
    let mut data_sz = 0;
    let mut bss_sz = 0;
    let mut current_sec = ".text";

    for line in asm.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with(".text") {
            current_sec = ".text";
            continue;
        } else if line.starts_with(".data") || line.starts_with(".rodata") {
            current_sec = ".data";
            continue;
        } else if line.starts_with(".bss") {
            current_sec = ".bss";
            continue;
        }

        if line.starts_with('.') {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let mut size = 0;
            match parts[0] {
                ".word" => size = 4 * parts.len().saturating_sub(1),
                ".half" => size = 2 * parts.len().saturating_sub(1),
                ".byte" => size = 1 * parts.len().saturating_sub(1),
                ".dword" => size = 8 * parts.len().saturating_sub(1),
                ".space" | ".skip" | ".zero" => {
                    if parts.len() > 1 {
                        let val = parts[1]
                            .strip_prefix("0x")
                            .map(|s| usize::from_str_radix(s, 16).unwrap_or(0))
                            .unwrap_or_else(|| parts[1].parse().unwrap_or(0));
                        size = val;
                    }
                }
                ".string" | ".asciz" | ".ascii" => {
                    if let (Some(s), Some(e)) = (line.find('"'), line.rfind('"')) {
                        if e > s {
                            size = e - s;
                        }
                    }
                }
                _ => {}
            }
            if current_sec == ".data" {
                data_sz += size;
            } else if current_sec == ".bss" {
                bss_sz += size;
            }
        } else if !line.ends_with(':') && current_sec == ".text" {
            text_sz += 4;
        }
    }

    let align_page = |sz: usize| -> usize {
        if sz == 0 {
            PAGE_SIZE
        } else {
            (sz + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
        }
    };
    text_sz = align_page(text_sz);
    data_sz = align_page(data_sz);
    bss_sz = align_page(bss_sz);
    let stack_sz = PAGE_SIZE * 4;

    let pa_kernel = 0x8000_0000;

    let pa_t1_text = 0x8100_0000;
    let pa_t1_data = pa_t1_text + text_sz as u64;
    let pa_t1_bss = pa_t1_data + data_sz as u64;
    let pa_t1_stack = 0x8150_0000;

    let pa_t2_text = 0x8200_0000;
    let pa_t2_data = 0x8200_4000;
    let pa_t2_stack = 0x8250_0000;

    let t1_segments = vec![
        VmSegment {
            name: "OS Kernel".into(),
            va_start: 0xFFFFFFC0_00000000,
            size_bytes: 0x1000000,
            color: Color32::from_rgb(100, 100, 100),
            target_pa: Some(pa_kernel),
            description: "Mapped into all tasks. Handles syscalls and interrupts.".into(),
        },
        VmSegment {
            name: "Stack".into(),
            va_start: 0x0000003F_FFF00000,
            size_bytes: stack_sz,
            color: Color32::from_rgb(220, 90, 90),
            target_pa: Some(pa_t1_stack),
            description: "Task 1 Local Stack".into(),
        },
        VmSegment {
            name: ".bss".into(),
            va_start: 0x00000000_00012000,
            size_bytes: bss_sz,
            color: Color32::from_rgb(220, 180, 70),
            target_pa: Some(pa_t1_bss),
            description: "Uninitialized Data".into(),
        },
        VmSegment {
            name: ".data".into(),
            va_start: 0x00000000_00011000,
            size_bytes: data_sz,
            color: Color32::from_rgb(190, 130, 230),
            target_pa: Some(pa_t1_data),
            description: "Initialized Data".into(),
        },
        VmSegment {
            name: ".text".into(),
            va_start: 0x00000000_00010000,
            size_bytes: text_sz,
            color: Color32::from_rgb(90, 160, 255),
            target_pa: Some(pa_t1_text),
            description: "Program Instructions".into(),
        },
    ];

    let t2_segments = vec![
        VmSegment {
            name: "OS Kernel".into(),
            va_start: 0xFFFFFFC0_00000000,
            size_bytes: 0x1000000,
            color: Color32::from_rgb(100, 100, 100),
            target_pa: Some(pa_kernel),
            description: "Shared Kernel Space".into(),
        },
        VmSegment {
            name: "Stack".into(),
            va_start: 0x0000003F_FFF00000,
            size_bytes: stack_sz,
            color: Color32::from_rgb(200, 100, 140),
            target_pa: Some(pa_t2_stack),
            description: "Task 2 Stack (Isolated)".into(),
        },
        VmSegment {
            name: ".data".into(),
            va_start: 0x00000000_00011000,
            size_bytes: 8192,
            color: Color32::from_rgb(160, 100, 160),
            target_pa: Some(pa_t2_data),
            description: "Task 2 Data".into(),
        },
        VmSegment {
            name: ".text".into(),
            va_start: 0x00000000_00010000,
            size_bytes: 16384,
            color: Color32::from_rgb(80, 200, 200),
            target_pa: Some(pa_t2_text),
            description: "Background Daemon Code".into(),
        },
    ];

    let phys_mem = vec![
        PmBlock {
            name: "Kernel Image".into(),
            pa_start: pa_kernel,
            size_bytes: 0x1000000,
            color: Color32::from_rgb(100, 100, 100),
            owner_pid: 0,
        },
        PmBlock {
            name: "T1 .text".into(),
            pa_start: pa_t1_text,
            size_bytes: text_sz,
            color: Color32::from_rgb(90, 160, 255),
            owner_pid: 1,
        },
        PmBlock {
            name: "T1 .data".into(),
            pa_start: pa_t1_data,
            size_bytes: data_sz,
            color: Color32::from_rgb(190, 130, 230),
            owner_pid: 1,
        },
        PmBlock {
            name: "T1 .bss".into(),
            pa_start: pa_t1_bss,
            size_bytes: bss_sz,
            color: Color32::from_rgb(220, 180, 70),
            owner_pid: 1,
        },
        PmBlock {
            name: "T1 Stack".into(),
            pa_start: pa_t1_stack,
            size_bytes: stack_sz,
            color: Color32::from_rgb(220, 90, 90),
            owner_pid: 1,
        },
        PmBlock {
            name: "T2 .text".into(),
            pa_start: pa_t2_text,
            size_bytes: 16384,
            color: Color32::from_rgb(80, 200, 200),
            owner_pid: 2,
        },
        PmBlock {
            name: "T2 .data".into(),
            pa_start: pa_t2_data,
            size_bytes: 8192,
            color: Color32::from_rgb(160, 100, 160),
            owner_pid: 2,
        },
        PmBlock {
            name: "T2 Stack".into(),
            pa_start: pa_t2_stack,
            size_bytes: stack_sz,
            color: Color32::from_rgb(200, 100, 140),
            owner_pid: 2,
        },
    ];

    MmuState {
        processes: vec![
            Process {
                pid: 1,
                name: "Task 1 (Your Program)".into(),
                segments: t1_segments,
            },
            Process {
                pid: 2,
                name: "Task 2 (Background)".into(),
                segments: t2_segments,
            },
        ],
        phys_mem,
    }
}

fn draw_mmu_visualization(ui: &mut egui::Ui, process: &Process, phys_mem: &[PmBlock]) {
    // 1. Subtract a little extra right-side buffer just to be safe with the scrollbar
    let available_width = ui.available_width().max(600.0) - 10.0;

    let padding = 8.0;
    let inner_width = available_width - (padding * 2.0);

    // 2. Distribute the perfectly clamped inner width
    let col_width = (inner_width * 0.35).clamp(180.0, 300.0);
    let routing_width = inner_width - (col_width * 2.0);
    let total_height = 680.0;

    let (rect, _response) =
        ui.allocate_exact_size(Vec2::new(available_width, total_height), Sense::hover());

    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter_at(rect);

    // 3. Perfect edge-to-edge math including padding
    let va_x_left = rect.left() + padding;
    let va_x_right = va_x_left + col_width;
    let pa_x_left = va_x_right + routing_width;
    let pa_x_right = pa_x_left + col_width;

    // Draw Column Backgrounds (they will perfectly touch the allocated rect edges)
    let va_bg_rect = Rect::from_min_max(
        Pos2::new(va_x_left - padding, rect.top() + 30.0),
        Pos2::new(va_x_right + padding, rect.bottom()),
    );
    let pa_bg_rect = Rect::from_min_max(
        Pos2::new(pa_x_left - padding, rect.top() + 30.0),
        Pos2::new(pa_x_right + padding, rect.bottom()),
    );

    painter.rect_filled(
        va_bg_rect,
        CornerRadius::same(6),
        Color32::from_white_alpha(8),
    );
    painter.rect_filled(
        pa_bg_rect,
        CornerRadius::same(6),
        Color32::from_white_alpha(8),
    );

    // Headers
    painter.text(
        Pos2::new(va_x_left + (col_width / 2.0), rect.top()),
        egui::Align2::CENTER_TOP,
        format!("Virtual Address Space\n(Process {})", process.pid),
        egui::FontId::proportional(15.0),
        Color32::WHITE,
    );
    painter.text(
        Pos2::new(pa_x_left + (col_width / 2.0), rect.top()),
        egui::Align2::CENTER_TOP,
        "Physical RAM\n(Shared by Hardware)",
        egui::FontId::proportional(15.0),
        Color32::WHITE,
    );

    let mut current_va_y = rect.top() + 50.0;
    let mut current_pa_y = rect.top() + 50.0;

    let block_height = 48.0;

    // Pass 1: Draw Physical Memory & Store positions for routing lines
    let mut pa_rects = HashMap::new();

    let draw_block = |ui: &mut egui::Ui,
                      painter: &egui::Painter,
                      x1: f32,
                      x2: f32,
                      y: f32,
                      name: &str,
                      sub: &str,
                      color: Color32,
                      tooltip: &str|
     -> Rect {
        let r = Rect::from_min_max(Pos2::new(x1, y), Pos2::new(x2, y + block_height));

        // Block style
        painter.rect_filled(r, CornerRadius::same(6), color.linear_multiply(0.25));
        painter.rect_stroke(
            r,
            CornerRadius::same(6),
            Stroke::new(1.5, color),
            StrokeKind::Middle,
        );

        // Text
        painter.text(
            Pos2::new(x1 + 10.0, y + 8.0),
            egui::Align2::LEFT_TOP,
            name,
            egui::FontId::proportional(14.0),
            Color32::WHITE,
        );
        painter.text(
            Pos2::new(x1 + 10.0, y + 26.0),
            egui::Align2::LEFT_TOP,
            sub,
            egui::FontId::monospace(11.0),
            Color32::from_gray(180),
        );

        // Interaction
        let response = ui.interact(r, ui.id().with(y.to_bits()), Sense::hover());
        response.on_hover_ui(|ui| {
            ui.heading(name);
            ui.separator();
            ui.label(RichText::new(tooltip).size(13.0));
        });

        r
    };

    for pm in phys_mem {
        let alpha = if pm.owner_pid == 0 || pm.owner_pid == process.pid {
            1.0
        } else {
            0.2
        };
        let color = Color32::from_rgba_premultiplied(
            (pm.color.r() as f32 * alpha) as u8,
            (pm.color.g() as f32 * alpha) as u8,
            (pm.color.b() as f32 * alpha) as u8,
            255,
        );
        let pa_str = format!("PA: 0x{:08X}", pm.pa_start);
        let tooltip = format!("Physical Size: {}", format_size(pm.size_bytes));

        let r = draw_block(
            ui,
            &painter,
            pa_x_left,
            pa_x_right,
            current_pa_y,
            &pm.name,
            &pa_str,
            color,
            &tooltip,
        );
        pa_rects.insert(pm.pa_start, r);
        current_pa_y += block_height + 10.0;
    }

    // Pass 2: Draw Virtual Memory & Routing Lines
    for (i, vm) in process.segments.iter().enumerate() {
        let va_str = format!("VA: 0x{:012X}", vm.va_start);
        let tooltip = format!(
            "{}\nVirtual Size: {}",
            vm.description,
            format_size(vm.size_bytes)
        );

        let r_va = draw_block(
            ui,
            &painter,
            va_x_left,
            va_x_right,
            current_va_y,
            &vm.name,
            &va_str,
            vm.color,
            &tooltip,
        );

        // Draw routing line to Physical Memory
        if let Some(target_pa) = vm.target_pa {
            if let Some(r_pa) = pa_rects.get(&target_pa) {
                let start_pt = Pos2::new(va_x_right + 2.0, r_va.center().y);
                let end_pt = Pos2::new(pa_x_left - 2.0, r_pa.center().y);

                let control_dist = (routing_width * 0.5).max(40.0);
                let shape = egui::epaint::CubicBezierShape {
                    points: [
                        start_pt,
                        start_pt + Vec2::new(control_dist, 0.0),
                        end_pt - Vec2::new(control_dist, 0.0),
                        end_pt,
                    ],
                    closed: false,
                    fill: Color32::TRANSPARENT,
                    stroke: PathStroke::new(2.5, vm.color.linear_multiply(0.75)),
                };

                painter.add(shape);

                // Connection nodes
                painter.circle_filled(start_pt, 3.5, vm.color);
                painter.circle_filled(end_pt, 3.5, vm.color);
            }
        }

        current_va_y += block_height + 10.0;

        // Visual gap between Kernel High Mem and User Low Mem
        if i == 0 {
            current_va_y += 15.0;

            let gap_rect = Rect::from_min_max(
                Pos2::new(va_x_left, current_va_y),
                Pos2::new(va_x_right, current_va_y + 30.0),
            );
            painter.rect_stroke(
                gap_rect,
                CornerRadius::same(4),
                Stroke::new(1.0, Color32::from_gray(60)),
                StrokeKind::Middle,
            );
            painter.text(
                gap_rect.center(),
                egui::Align2::CENTER_CENTER,
                "Unmapped / Page Fault",
                egui::FontId::monospace(11.0),
                Color32::from_gray(120),
            );

            current_va_y += 45.0;
        }
    }
}
