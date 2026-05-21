use crate::view::debug::snapshot::CpuSnapshot;
use crate::view::{
    CompilationState, CompilerView, ProgramCatalog, auto_grid_columns_with_min_width, ui_theme,
};
use egui::{Color32, FontId, Frame, Grid, Margin, RichText, ScrollArea, Ui, vec2};

const ABI_NAMES: [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4",
    "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4",
    "t5", "t6",
];

#[derive(Clone, Default)]
pub struct CpuStateView {
    show_fp_as_float: bool,
}

impl CompilerView for CpuStateView {
    fn title(&self) -> &'static str {
        "CPU State"
    }

    fn ui(
        &mut self,
        ui: &mut Ui,
        _ctx: &egui::Context,
        state: &mut CompilationState,
        _catalog: &mut ProgramCatalog,
    ) {
        let theme = ui_theme();
        let Some(session) = &state.debug_session else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("No debug session active").weak());
            });
            return;
        };

        let snap = session.snapshot.clone();
        let cpu = &snap.cpu;

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let available_w = ui.available_width();
                ui.set_max_width(available_w);

                pc_bar(ui, cpu.pc, available_w);
                ui.add_space(6.0);

                let num_cols = auto_grid_columns_with_min_width(ui, 320.0, 1, 4);
                let col_width =
                    ((available_w - (num_cols - 1) as f32 * 16.0) / num_cols as f32).max(10.0);

                section_bar(ui, "Integer Registers", available_w);
                Grid::new("cpu_int_regs_grid")
                    .striped(true)
                    .num_columns(num_cols)
                    .spacing([16.0, 6.0])
                    .min_col_width(col_width)
                    .show(ui, |ui| {
                        integer_regs(ui, &cpu.xregs, &cpu.prev_xregs, num_cols);
                    });

                ui.add_space(4.0);

                let fp_bar_margin = Margin {
                    left: 8,
                    right: 4,
                    top: 4,
                    bottom: 4,
                };
                Frame::NONE
                    .fill(theme.surface_alt)
                    .inner_margin(fp_bar_margin)
                    .show(ui, |ui| {
                        ui.set_min_width(available_w);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("FP Registers")
                                    .strong()
                                    .small()
                                    .color(theme.text_dim),
                            );
                            ui.add_space(8.0);
                            ui.checkbox(&mut self.show_fp_as_float, "as float");
                        });
                    });
                Grid::new("cpu_fp_regs_grid")
                    .striped(true)
                    .num_columns(num_cols)
                    .spacing([16.0, 6.0])
                    .min_col_width(col_width)
                    .show(ui, |ui| {
                        fp_regs(ui, &cpu.fregs, self.show_fp_as_float, num_cols);
                    });

                ui.add_space(4.0);

                section_bar(ui, "CSRs", available_w);
                Grid::new("cpu_csr_grid")
                    .striped(true)
                    .num_columns(num_cols)
                    .spacing([16.0, 6.0])
                    .min_col_width(col_width)
                    .show(ui, |ui| {
                        csr_table(ui, cpu, num_cols);
                    });
            });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}

fn section_bar(ui: &mut Ui, label: &str, available_w: f32) {
    let theme = ui_theme();
    Frame::NONE
        .fill(theme.surface_alt)
        .inner_margin(Margin {
            left: 8,
            right: 4,
            top: 4,
            bottom: 4,
        })
        .show(ui, |ui| {
            ui.set_min_width(available_w);
            ui.label(RichText::new(label).strong().small().color(theme.text_dim));
        });
}

fn pc_bar(ui: &mut Ui, pc: u64, w: f32) {
    let h = 28.0;
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), egui::Sense::hover());
    let theme = ui_theme();
    ui.painter().rect_filled(rect, 4.0, theme.surface_alt);
    ui.painter().text(
        rect.left_center() + vec2(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        "PC",
        FontId::monospace(11.0),
        theme.text_dim,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{pc:#018x}"),
        FontId::monospace(13.0),
        Color32::WHITE,
    );
}

fn integer_regs(ui: &mut Ui, regs: &[u64; 32], prev: &[u64; 32], num_cols: usize) {
    let theme = ui_theme();
    let changed = theme.highlight;
    let dim = theme.text_dim;
    let normal = theme.text;
    let dec_color = theme.text_soft;

    let regs_per_col = (32 + num_cols - 1) / num_cols;

    for row in 0..regs_per_col {
        for col in 0..num_cols {
            let i = col * regs_per_col + row;
            if i < 32 {
                let val = regs[i];
                let value_color = if val != prev[i] { changed } else { normal };

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;

                    // Pad alias to exactly 8 characters to match longest CSR ("mscratch")
                    let name_text = format!("{:<8}", ABI_NAMES[i]);
                    ui.label(RichText::new(name_text).monospace().color(dim));

                    let hex_text = format!("{val:#018x}");
                    ui.label(RichText::new(hex_text).monospace().color(value_color));

                    let dec_text = format!("{}", val as i64);
                    ui.label(RichText::new(dec_text).monospace().color(dec_color));
                });
            } else {
                ui.label("");
            }
        }
        ui.end_row();
    }
}

fn fp_regs(ui: &mut Ui, regs: &[u64; 32], as_float: bool, num_cols: usize) {
    let theme = ui_theme();
    let dim = theme.text_dim;
    let normal = theme.text_soft;

    let regs_per_col = (32 + num_cols - 1) / num_cols;

    for row in 0..regs_per_col {
        for col in 0..num_cols {
            let i = col * regs_per_col + row;
            if i < 32 {
                let bits = regs[i];
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;

                    // Pad 'f' + number out to exactly 8 characters to perfectly align with Int/CSR
                    let name_text = format!("f{:<7}", i);
                    ui.label(RichText::new(name_text).monospace().color(dim));

                    if as_float {
                        let v = f64::from_bits(bits);
                        ui.label(RichText::new(format!("{v:.6}")).monospace().color(normal));
                    } else {
                        let is_boxed = (bits >> 32) == 0xFFFF_FFFF;
                        let text = if is_boxed {
                            let f = f32::from_bits(bits as u32);
                            format!("{bits:#018x} {f:.4}")
                        } else {
                            format!("{bits:#018x}")
                        };
                        ui.label(RichText::new(text).monospace().color(normal));
                    }
                });
            } else {
                ui.label("");
            }
        }
        ui.end_row();
    }
}

fn csr_table(ui: &mut Ui, cpu: &CpuSnapshot, num_cols: usize) {
    let c = &cpu.csrs;
    let csrs: &[(&str, u64)] = &[
        ("mstatus", c.mstatus),
        ("mtvec", c.mtvec),
        ("mepc", c.mepc),
        ("mcause", c.mcause),
        ("mtval", c.mtval),
        ("mie", c.mie),
        ("mip", c.mip),
        ("mscratch", c.mscratch),
        ("stvec", c.stvec),
        ("sepc", c.sepc),
        ("scause", c.scause),
        ("stval", c.stval),
        ("satp", c.satp),
        ("cycle", c.cycle),
        ("instret", c.instret),
        ("fcsr", ((c.frm as u64) << 5) | (c.fflags as u64)),
    ];

    let theme = ui_theme();
    let dim = theme.text_dim;
    let val_color = theme.text_soft;

    let items_per_col = (csrs.len() + num_cols - 1) / num_cols;

    for row in 0..items_per_col {
        for col in 0..num_cols {
            let i = col * items_per_col + row;
            if i < csrs.len() {
                let (name, val) = csrs[i];
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;

                    let name_text = format!("{name:<8}"); // Pad names out to matching length
                    ui.label(RichText::new(name_text).monospace().color(dim));

                    let hex_text = format!("{val:#018x}");
                    ui.label(RichText::new(hex_text).monospace().color(val_color));
                });
            } else {
                ui.label("");
            }
        }
        ui.end_row();
    }
}
