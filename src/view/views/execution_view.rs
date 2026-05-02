use crate::view::{CompilationState, CompilerView, ProgramCatalog};
use egui::{RichText, ScrollArea};

#[derive(Default)]
pub struct ExecutionView {
    #[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
    wsl_receiver: Option<std::sync::mpsc::Receiver<String>>,
}

impl Clone for ExecutionView {
    fn clone(&self) -> Self {
        Self {
            #[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
            wsl_receiver: None, // Don't clone the receiver
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
        ctx: &egui::Context,
        state: &mut CompilationState,
        _catalog: &mut ProgramCatalog,
    ) {
        // Button to run in QEMU via WSL
        #[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
        {
            ui.horizontal(|ui| {
                if ui.button("Run in QEMU").clicked() {
                    if state.asm.is_empty() {
                        state.execution_output = "Please compile first.".to_string();
                        return;
                    }

                    state.execution_output = "Running in QEMU... please wait.\n(Executing cross-compiler and QEMU in background)".to_string();

                    let (tx, rx) = std::sync::mpsc::channel();
                    self.wsl_receiver = Some(rx);
                    let asm_copy = state.asm.clone();

                    std::thread::spawn(move || {
                        let result = run_in_wsl(&asm_copy);
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
                            "Error: WSL thread disconnected unexpectedly.".to_string();
                        self.wsl_receiver = None;
                    }
                }
            }
        }

        #[cfg(not(all(not(target_arch = "wasm32"), target_os = "windows")))]
        {
            ui.label(RichText::new("QEMU execution requires Windows with WSL.").weak());
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
fn run_in_wsl(asm: &str) -> String {
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let clean_asm = asm
        .lines()
        .map(|line| line.split(';').next().unwrap_or("").trim_end())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    let script = r#"
echo "=== Connected to WSL ==="
export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH"
WORKDIR="$HOME/assembly"
mkdir -p "$WORKDIR"
cd "$WORKDIR"
CC="$(which riscv64-linux-gnu-gcc 2>/dev/null)"
if [ -z "$CC" ]; then
    echo "ERROR: riscv64-linux-gnu-gcc not found."
    exit 1
fi
QEMU="$(which qemu-riscv64 2>/dev/null)"
if [ -z "$QEMU" ]; then
    echo "ERROR: qemu-riscv64 not found."
    exit 1
fi
set -e
cat > program.s
echo >> program.s
"$CC" -static program.s -o program
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
        Err(e) => return format!("Failed to start WSL: {}", e),
    };

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(clean_asm.as_bytes()) {
            return format!("Failed to write to WSL stdin: {}", e);
        }
    }

    let output = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => return format!("Failed to wait on WSL process: {}", e),
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
