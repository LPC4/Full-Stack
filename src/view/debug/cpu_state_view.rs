use crate::view::debug::snapshot::CpuSnapshot;
use crate::view::{auto_grid_columns_with_min_width, CompilationState, CompilerView, ProgramCatalog};
use egui::{Color32, FontId, Grid, RichText, ScrollArea, Ui, vec2};

const ABI_NAMES: [&str; 32] = [
    "zero", "ra",  "sp",  "gp",  "tp",  "t0",  "t1",  "t2",
    "s0",   "s1",  "a0",  "a1",  "a2",  "a3",  "a4",  "a5",
    "a6",   "a7",  "s2",  "s3",  "s4",  "s5",  "s6",  "s7",
    "s8",   "s9",  "s10", "s11", "t3",  "t4",  "t5",  "t6",
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
                ui.add_space(8.0);

                // Use a single global grid for all registers so their major columns perfectly align.
                let num_cols = auto_grid_columns_with_min_width(ui, 320.0, 1, 4);
                let col_width = ((available_w - (num_cols - 1) as f32 * 16.0) / num_cols as f32).max(10.0);

                Grid::new("global_cpu_state_grid")
                    .striped(true)
                    .num_columns(num_cols)
                    .spacing([16.0, 6.0])
                    .min_col_width(col_width)
                    .show(ui, |ui| {
                        // --- Integer Registers Header ---
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Integer Registers").strong().color(Color32::from_gray(160)));
                        });
                        for _ in 1..num_cols { ui.label(""); }
                        ui.end_row();

                        // --- Integer Registers ---
                        integer_regs(ui, &cpu.xregs, &cpu.prev_xregs, num_cols);

                        // Spacer Row
                        for _ in 0..num_cols { ui.label(""); }
                        ui.end_row();

                        // --- FP Registers Header ---
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("FP Registers").strong().color(Color32::from_gray(160)));
                            ui.checkbox(&mut self.show_fp_as_float, "as float");
                        });
                        for _ in 1..num_cols { ui.label(""); }
                        ui.end_row();

                        // --- FP Registers ---
                        fp_regs(ui, &cpu.fregs, self.show_fp_as_float, num_cols);

                        // Spacer Row
                        for _ in 0..num_cols { ui.label(""); }
                        ui.end_row();

                        // --- CSRs Header ---
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("CSRs").strong().color(Color32::from_gray(160)));
                        });
                        for _ in 1..num_cols { ui.label(""); }
                        ui.end_row();

                        // --- CSRs ---
                        csr_table(ui, cpu, num_cols);
                    });
            });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}

fn pc_bar(ui: &mut Ui, pc: u64, w: f32) {
    let h = 28.0;
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), egui::Sense::hover());
    ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(50, 50, 70));
    ui.painter().text(
        rect.left_center() + vec2(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        "PC",
        FontId::monospace(11.0),
        Color32::from_gray(140),
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
    let changed = Color32::from_rgb(255, 215, 0);
    let dim = Color32::from_gray(130);
    let normal = Color32::from_gray(220);
    let dec_color = Color32::from_gray(100);

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
    let dim = Color32::from_gray(130);
    let normal = Color32::from_gray(210);

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
        ("mstatus", c.mstatus), ("mtvec",   c.mtvec),
        ("mepc",    c.mepc),    ("mcause",  c.mcause),
        ("mtval",   c.mtval),   ("mie",     c.mie),
        ("mip",     c.mip),     ("mscratch", c.mscratch),
        ("stvec",   c.stvec),   ("sepc",    c.sepc),
        ("scause",  c.scause),  ("stval",   c.stval),
        ("satp",    c.satp),    ("cycle",   c.cycle),
        ("instret", c.instret), ("fcsr",    ((c.frm as u64) << 5) | (c.fflags as u64)),
    ];

    let dim = Color32::from_gray(130);
    let val_color = Color32::from_gray(210);

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