use crate::view::{CompilationState, CompilerView, ProgramCatalog};
use egui::{RichText, ScrollArea};
#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
use virtual_machine::bus::ELF_LOAD_BASE;

#[derive(Default)]
pub struct ExecutionView {
    #[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
    wsl_receiver: Option<std::sync::mpsc::Receiver<String>>,
}

impl Clone for ExecutionView {
    fn clone(&self) -> Self {
        Self {
            #[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
            wsl_receiver: None,
        }
    }
}

impl CompilerView for ExecutionView {
    fn title(&self) -> &'static str {
        "Execution (QEMU)"
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &egui::Context,
        state: &mut CompilationState,
        _catalog: &mut ProgramCatalog,
    ) {
        #[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
        {
            ui.horizontal(|ui| {
                if ui.button("Run in QEMU").clicked() {
                    let elf_bytes_opt = state.assembled().map(|a| a.to_elf(ELF_LOAD_BASE));
                    let Some(elf_bytes) = elf_bytes_opt else {
                        state.execution_output = "Please compile first.".to_owned();
                        return;
                    };

                    state.execution_output =
                        "Running in QEMU... please wait.\n(Executing qemu-riscv64 in background)"
                            .to_owned();

                    let (tx, rx) = std::sync::mpsc::channel();
                    self.wsl_receiver = Some(rx);

                    std::thread::spawn(move || {
                        let result = run_in_wsl(&elf_bytes);
                        let _ = tx.send(result);
                    });
                }
            });
            ui.separator();

            // Poll for WSL output
            if let Some(rx) = &self.wsl_receiver {
                match rx.try_recv() {
                    Ok(result) => {
                        state.execution_output = result;
                        self.wsl_receiver = None;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        ctx.request_repaint();
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        state.execution_output =
                            "Error: WSL thread disconnected unexpectedly.".to_owned();
                        self.wsl_receiver = None;
                    }
                }
            }
        }

        #[cfg(not(all(not(target_arch = "wasm32"), target_os = "windows")))]
        {
            let theme = crate::view::ui_theme();
            egui::Frame::NONE
                .fill(theme.warning.gamma_multiply(0.10))
                .stroke(egui::Stroke::new(1.0, theme.warning.gamma_multiply(0.40)))
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("QEMU execution requires Windows with WSL.")
                            .color(theme.warning),
                    );
                });
            ui.add_space(8.0);
        }

        if state.execution_output.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("Click 'Run in QEMU' to execute via WSL/QEMU.").weak());
            });
            return;
        }

        ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
            ui.label(RichText::new(&state.execution_output).monospace());
        });
    }

    fn clone_box(&self) -> Box<dyn CompilerView> {
        Box::new(self.clone())
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
fn run_in_wsl(elf: &[u8]) -> String {
    use std::io::Write as _;
    use std::os::windows::process::CommandExt as _;
    use std::process::{Command, Stdio};

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let script = r#"
echo "=== Connected to WSL ==="
export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH"
WORKDIR="$HOME/assembly"
mkdir -p "$WORKDIR"
cd "$WORKDIR"
QEMU="$(which qemu-riscv64 2>/dev/null)"
if [ -z "$QEMU" ]; then
    echo "ERROR: qemu-riscv64 not found."
    exit 1
fi
set -e
cat > program
chmod +x program
set +e
"$QEMU" ./program
EXIT_CODE=$?
echo ""
echo "--- Process Exited with Code: $EXIT_CODE ---"
"#;

    let mut child = match Command::new("wsl")
        .args(["--exec", "bash", "-lc", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return format!("Failed to start WSL: {e}"),
    };

    if let Some(mut stdin) = child.stdin.take()
        && let Err(e) = stdin.write_all(elf)
    {
        return format!("Failed to write to WSL stdin: {e}");
    }

    let output = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => return format!("Failed to wait on WSL process: {e}"),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut result = String::new();
    if !stdout.is_empty() {
        result.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !result.is_empty() {
            result.push_str("\n\n[STDERR]\n");
        }
        result.push_str(&stderr);
    }

    result
}
