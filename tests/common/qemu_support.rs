// Shared qemu-riscv64 execution for integration tests.

use asm_to_binary::AssembledOutput;
use full_stack::compilation_pipeline::CompilationPipeline;
use hll_to_ir::TargetMode;
use std::fmt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use virtual_machine::bus::ELF_LOAD_BASE;

pub const QEMU_WAIT_TIMEOUT_SECS: u64 = 60;
static QEMU_RUN_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct QemuResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub enum QemuError {
    Unavailable(QemuUnavailable),
    Failed(String),
}

#[derive(Debug)]
pub enum QemuUnavailable {
    MissingBinary(String),
    #[cfg(target_os = "linux")]
    SpawnFailed(String),
    TimedOut(u64),
    #[cfg(target_os = "windows")]
    WslBroken(String),
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    UnsupportedPlatform(String),
}

impl fmt::Display for QemuUnavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TimedOut(secs) => write!(
                f,
                "qemu did not exit within {secs}s and was killed; is the program stuck?"
            ),
            Self::MissingBinary(msg) => write!(f, "{msg}"),
            #[cfg(target_os = "linux")]
            Self::SpawnFailed(msg) => write!(f, "{msg}"),
            #[cfg(target_os = "windows")]
            Self::WslBroken(msg) => write!(f, "{msg}"),
            #[cfg(not(any(target_os = "linux", target_os = "windows")))]
            Self::UnsupportedPlatform(msg) => write!(f, "{msg}"),
        }
    }
}

pub fn qemu_required() -> bool {
    std::env::var("FULL_STACK_REQUIRE_QEMU").is_ok_and(|v| v != "0")
}

pub fn apply_qemu_policy(
    required: bool,
    test_name: &str,
    result: Result<QemuResult, QemuError>,
) -> Result<Option<QemuResult>, String> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(QemuError::Failed(msg)) => Err(format!("{test_name}: {msg}")),
        Err(QemuError::Unavailable(reason)) if required => {
            Err(format!("{test_name}: required QEMU run failed: {reason}"))
        }
        Err(QemuError::Unavailable(reason)) => {
            eprintln!("[SKIP] {test_name} - {reason}");
            Ok(None)
        }
    }
}

pub fn require_qemu_with(
    required: bool,
    test_name: &str,
    result: Result<QemuResult, QemuError>,
) -> Option<QemuResult> {
    apply_qemu_policy(required, test_name, result).unwrap_or_else(|e| panic!("{e}"))
}

pub fn run_elf(elf: &[u8]) -> Result<QemuResult, QemuError> {
    run_elf_with_timeout(elf, Duration::from_secs(QEMU_WAIT_TIMEOUT_SECS))
}

pub fn run_elf_with_timeout(elf: &[u8], timeout: Duration) -> Result<QemuResult, QemuError> {
    // Concurrent WSL launches can fail at the service boundary instead of starting QEMU.
    let _guard = QEMU_RUN_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    platform_run_elf(elf, timeout)
}

pub fn decode_utf8(stream: &str, bytes: Vec<u8>) -> Result<String, QemuError> {
    String::from_utf8(bytes)
        .map_err(|e| QemuError::Failed(format!("qemu {stream} was not UTF-8: {e}")))
}

#[cfg(target_os = "linux")]
fn platform_run_elf(elf: &[u8], timeout: Duration) -> Result<QemuResult, QemuError> {
    run_elf_linux(elf, timeout)
}

#[cfg(target_os = "windows")]
fn platform_run_elf(elf: &[u8], timeout: Duration) -> Result<QemuResult, QemuError> {
    run_elf_wsl(elf, timeout)
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn platform_run_elf(_elf: &[u8], _timeout: Duration) -> Result<QemuResult, QemuError> {
    Err(QemuError::Unavailable(
        QemuUnavailable::UnsupportedPlatform(
            "QEMU runs on Linux (direct) or Windows (WSL)".to_owned(),
        ),
    ))
}

pub fn compile_hll_to_elf(source: &str) -> Vec<u8> {
    let pipeline = CompilationPipeline::new();
    let stdlib_objs = CompilationPipeline::compile_stdlib_objects(TargetMode::Hosted)
        .unwrap_or_else(|e| panic!("stdlib compilation failed: {e}"));
    let user_result = pipeline
        .compile(source)
        .unwrap_or_else(|e| panic!("HLL compilation failed: {e}"));
    let (_, user_tokens) = pipeline
        .compile_ir_to_assembly_with_tokens(&user_result.ir_program)
        .expect("valid IR layout");
    let user_obj = pipeline
        .assemble(&user_tokens)
        .unwrap_or_else(|e| panic!("user assembly failed: {e}"));
    let mut modules: Vec<(&str, &AssembledOutput)> =
        stdlib_objs.iter().map(|(n, o)| (n.as_str(), o)).collect();
    modules.push(("user", &user_obj));
    let assembled = pipeline
        .link_assembled_objects(&modules)
        .unwrap_or_else(|e| panic!("link failed: {e}"));
    assembled.to_elf(ELF_LOAD_BASE)
}

#[cfg_attr(
    target_os = "windows",
    expect(dead_code, reason = "used by the Linux runner")
)]
fn write_temp_elf(elf: &[u8]) -> Result<PathBuf, QemuError> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("fst_qemu_{}_{}.elf", std::process::id(), seq));
    std::fs::write(&path, elf)
        .map_err(|e| QemuError::Failed(format!("failed to write ELF {path:?}: {e}")))?;
    Ok(path)
}

#[cfg(target_os = "linux")]
fn run_elf_linux(elf: &[u8], timeout: Duration) -> Result<QemuResult, QemuError> {
    let path = write_temp_elf(elf)?;
    let child = match Command::new("qemu-riscv64")
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            drop(std::fs::remove_file(&path));
            return Err(QemuError::Unavailable(
                if e.kind() == std::io::ErrorKind::NotFound {
                    QemuUnavailable::MissingBinary(format!("qemu-riscv64 not found: {e}"))
                } else {
                    QemuUnavailable::SpawnFailed(e.to_string())
                },
            ));
        }
    };
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        drop(tx.send(child.wait_with_output()));
    });
    let output = match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            drop(std::fs::remove_file(&path));
            return Err(QemuError::Unavailable(QemuUnavailable::SpawnFailed(
                e.to_string(),
            )));
        }
        Err(_) => {
            drop(
                Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn(),
            );
            drop(std::fs::remove_file(&path));
            return Err(QemuError::Unavailable(QemuUnavailable::TimedOut(
                timeout.as_secs(),
            )));
        }
    };
    drop(std::fs::remove_file(&path));
    let exit_code = output.status.code().ok_or_else(|| {
        QemuError::Failed(format!("qemu terminated by signal: {}", output.status))
    })?;
    let stdout = decode_utf8("stdout", output.stdout)?;
    let stderr = decode_utf8("stderr", output.stderr)?;
    Ok(QemuResult {
        exit_code,
        stdout,
        stderr,
    })
}

#[cfg(target_os = "windows")]
fn run_elf_wsl(elf: &[u8], timeout: Duration) -> Result<QemuResult, QemuError> {
    use std::io::Write as _;
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    // Creating the ELF inside WSL avoids stale 9p directory entries under parallel tests.
    let base64 = base64_encode(elf);
    let script = r#"
export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH"

QEMU="$(command -v qemu-riscv64 2>/dev/null)"
if [ -z "$QEMU" ]; then
    echo "TOOLCHAIN_UNAVAILABLE: qemu-riscv64 not found"
    exit 0
fi

ELF_PATH="/tmp/fst_$$.elf"
base64 -d > "$ELF_PATH"
chmod +x "$ELF_PATH"
"$QEMU" "$ELF_PATH"
STATUS=$?
rm -f "$ELF_PATH"
exit "$STATUS"
"#;

    let mut child = match Command::new("wsl")
        .args(["--exec", "bash", "-lc", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            return Err(QemuError::Unavailable(QemuUnavailable::WslBroken(format!(
                "failed to start WSL ({e}). Install WSL2 and make sure `wsl.exe` is on PATH"
            ))));
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        drop(stdin.write_all(base64.as_bytes()));
    }

    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        drop(tx.send(child.wait_with_output()));
    });
    let output = match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return Err(QemuError::Unavailable(QemuUnavailable::WslBroken(format!(
                "WSL terminated while waiting for QEMU ({e})"
            ))));
        }
        Err(_) => {
            drop(
                Command::new("taskkill")
                    .args(["/T", "/F", "/PID", &pid.to_string()])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn(),
            );
            return Err(QemuError::Unavailable(QemuUnavailable::TimedOut(
                timeout.as_secs(),
            )));
        }
    };

    let stdout = decode_utf8("stdout", output.stdout)?;
    if stdout.contains("TOOLCHAIN_UNAVAILABLE: qemu-riscv64 not found") {
        return Err(QemuError::Unavailable(QemuUnavailable::MissingBinary(
            "qemu-riscv64 not found in WSL".to_owned(),
        )));
    }
    let stderr = decode_utf8("stderr", output.stderr)?;
    let exit_code = output.status.code().ok_or_else(|| {
        QemuError::Unavailable(QemuUnavailable::WslBroken(format!(
            "WSL terminated without an exit status (stderr={stderr:?})"
        )))
    })?;
    if exit_code < 0 {
        return Err(QemuError::Unavailable(QemuUnavailable::WslBroken(format!(
            "WSL failed with status {exit_code} (stdout={stdout:?}, stderr={stderr:?})"
        ))));
    }
    Ok(QemuResult {
        exit_code,
        stdout,
        stderr,
    })
}

#[cfg(target_os = "windows")]
fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}
