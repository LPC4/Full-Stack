use crate::high_level_language::compilation_pipeline::CompilationPipeline;
use crate::high_level_language::stdlib::get_kernel_stdlib_source;
use crate::view::ide::vm_execution_view::VmExecutionResult;
use crate::view::{CompilationState, CompilerView, ProgramCatalog};
use egui::{Color32, Frame, RichText};

const TERM_BG: Color32 = Color32::from_rgb(7, 9, 12);
const TERM_TEXT: Color32 = Color32::from_rgb(185, 210, 185);
const TERM_OK: Color32 = Color32::from_rgb(72, 200, 100);
const TERM_WARN: Color32 = Color32::from_rgb(220, 178, 60);
const TERM_ERR: Color32 = Color32::from_rgb(230, 80, 80);
const TERM_DIM: Color32 = Color32::from_rgb(100, 120, 100);
const TERM_PANIC: Color32 = Color32::from_rgb(255, 60, 80);

fn run_kernel_boot(user_source: &str) -> VmExecutionResult {
    use crate::virtual_machine::cpu::StepOutcome;
    use crate::virtual_machine::virtual_machine::VirtualMachine;

    const MAX_STEPS: u64 = 10_000_000;

    let mut kern = CompilationPipeline::new();
    kern.string_prefix = Some("__kern_str_".to_owned());

    let stdlib_ir = match kern.compile(&get_kernel_stdlib_source()) {
        Ok(r) => r,
        Err(e) => {
            return VmExecutionResult {
                uart_output: format!("PANIC: kernel stdlib compile failed: {e:?}"),
                exit_code: None,
                steps: 0,
                max_steps_reached: false,
            };
        }
    };
    let (_, stdlib_tokens) = kern.compile_ir_to_assembly_with_tokens(&stdlib_ir.ir_program);

    let user = CompilationPipeline::new();
    let user_ir = match user.compile(user_source) {
        Ok(r) => r,
        Err(e) => {
            return VmExecutionResult {
                uart_output: format!("PANIC: user compile failed: {e:?}"),
                exit_code: None,
                steps: 0,
                max_steps_reached: false,
            };
        }
    };
    let (_, user_tokens) = user.compile_ir_to_assembly_with_tokens(&user_ir.ir_program);

    let mut linked = stdlib_tokens;
    linked.extend(user_tokens);

    let assembled = match user.assemble(&linked) {
        Ok(a) => a,
        Err(e) => {
            let msg = if e.message.contains("kmain") {
                "undefined label `kmain` — your kernel must define:\n  kmain: () -> () { ... }"
                    .to_owned()
            } else {
                e.message.clone()
            };
            return VmExecutionResult {
                uart_output: format!("PANIC: assemble failed: {msg}"),
                exit_code: None,
                steps: 0,
                max_steps_reached: false,
            };
        }
    };

    let mut vm = VirtualMachine::new_kernel(&assembled);
    let result = vm.run(MAX_STEPS);
    VmExecutionResult {
        uart_output: result.uart_output,
        exit_code: match result.outcome {
            StepOutcome::Halted(code) => Some(code as i32),
            _ => None,
        },
        steps: result.steps,
        max_steps_reached: matches!(result.outcome, StepOutcome::Continue),
    }
}

#[derive(Default, Clone)]
pub struct KernelView {
    boot_result: Option<VmExecutionResult>,
}

impl CompilerView for KernelView {
    fn title(&self) -> &'static str {
        "Kernel"
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &egui::Context,
        _state: &mut CompilationState,
        catalog: &mut ProgramCatalog,
    ) {
        let is_kernel = catalog
            .current_program()
            .map(|p| p.is_kernel())
            .unwrap_or(false);

        if !is_kernel {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("Select a kernel program to use this panel.")
                        .weak(),
                );
            });
            return;
        }

        let source = catalog.get_selected_source();

        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::Button::new(RichText::new("Boot").strong())
                        .fill(Color32::from_rgb(20, 60, 100))
                        .min_size(egui::vec2(72.0, 28.0)),
                )
                .on_hover_text("Compile and boot this kernel program")
                .clicked()
            {
                self.boot_result = Some(run_kernel_boot(&source));
            }
            if ui.button("Clear").clicked() {
                self.boot_result = None;
            }
            if let Some(r) = &self.boot_result {
                ui.separator();
                let (txt, col) = if r.max_steps_reached {
                    ("TIMEOUT", TERM_WARN)
                } else if r.exit_code == Some(0) {
                    ("OK", TERM_OK)
                } else if r.exit_code.is_some() {
                    ("ERR", TERM_ERR)
                } else {
                    ("???", TERM_DIM)
                };
                ui.colored_label(col, txt);
                ui.colored_label(TERM_DIM, format!("{} steps", r.steps));
            }
        });

        ui.add_space(4.0);

        let avail = ui.available_height();
        Frame::NONE
            .fill(TERM_BG)
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(30, 50, 30)))
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.set_min_size(egui::vec2(ui.available_width(), avail));
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        match &self.boot_result {
                            None => {
                                ui.colored_label(
                                    TERM_DIM,
                                    "Press Boot to compile and run this kernel program.",
                                );
                            }
                            Some(r) if r.uart_output.is_empty() => {
                                ui.colored_label(TERM_DIM, "(no output)");
                            }
                            Some(r) => {
                                let font = egui::FontId::monospace(12.0);
                                let mut job = egui::text::LayoutJob::default();
                                for line in r.uart_output.split('\n') {
                                    let (tag, tag_col, rest_col) =
                                        if line.starts_with("[  OK  ]") {
                                            (Some("[  OK  ]"), TERM_OK, TERM_TEXT)
                                        } else if line.starts_with("[ WARN ]") {
                                            (Some("[ WARN ]"), TERM_WARN, TERM_WARN)
                                        } else if line.starts_with("[ ERR  ]") {
                                            (Some("[ ERR  ]"), TERM_ERR, TERM_ERR)
                                        } else if line.starts_with("PANIC")
                                            || line.starts_with("panic")
                                        {
                                            (None, TERM_PANIC, TERM_PANIC)
                                        } else {
                                            (None, TERM_TEXT, TERM_TEXT)
                                        };
                                    let fmt = |col: Color32| egui::TextFormat {
                                        font_id: font.clone(),
                                        color: col,
                                        ..Default::default()
                                    };
                                    if let Some(t) = tag {
                                        job.append(t, 0.0, fmt(tag_col));
                                        job.append(&line[t.len()..], 0.0, fmt(rest_col));
                                    } else {
                                        job.append(line, 0.0, fmt(rest_col));
                                    }
                                    job.append("\n", 0.0, fmt(TERM_TEXT));
                                }
                                ui.label(job);
                            }
                        }
                    });
            });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}
