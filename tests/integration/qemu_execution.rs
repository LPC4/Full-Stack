/// QEMU execution integration tests.
///
/// Each test compiles an HLL program all the way to RISC-V assembly or ELF,
/// then runs it under qemu-riscv64 inside WSL and verifies the exit code and
/// (where applicable) stdout output.
///
/// If the WSL toolchain is absent the tests print a diagnostic with the exact
/// missing prerequisite and return without failing, so CI on machines without
/// the cross-toolchain stays green.
use asm_to_binary::AssembledOutput;
use full_stack::compilation_pipeline::CompilationPipeline;
use hll_to_ir::TargetMode;
use hll_to_ir::stdlib::{get_stdlib_modules_for_mode, get_stdlib_type_prelude};
use std::fmt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use virtual_machine::bus::ELF_LOAD_BASE;

fn qemu_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("programs/test/qemu")
}

fn read_program(filename: &str) -> String {
    let path = qemu_dir().join(filename);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {path:?}: {e}"))
}

/// Compile HLL source -> assembly text, stripping inline comments so the
/// assembly can be passed safely through a shell heredoc.
/// Uses two-stage compilation: compile stdlib and user code independently,
/// then assemble them into objects and link the objects before generating ELF.
fn compile_to_asm(source: &str) -> String {
    // Stage 1: compile each stdlib module independently (no source concatenation),
    // each with a distinct rodata prefix, and join their assembly text for `as`.
    let mut stdlib_asm = String::new();
    for (i, (_, src)) in get_stdlib_modules_for_mode(TargetMode::Hosted)
        .iter()
        .enumerate()
    {
        let mut p = CompilationPipeline::new();
        p.set_string_prefix(Some(format!("std{i}_str_")));
        p.set_type_prelude(get_stdlib_type_prelude());
        let r = p
            .compile(src)
            .unwrap_or_else(|e| panic!("stdlib compilation failed: {e}"));
        let (asm, _) = p.compile_ir_to_assembly_with_tokens(&r.ir_program);
        stdlib_asm.push_str(&asm);
        stdlib_asm.push('\n');
    }

    // Stage 2: Compile user code
    let user_pipeline = CompilationPipeline::new();
    let user_result = user_pipeline
        .compile(source)
        .unwrap_or_else(|e| panic!("HLL compilation failed: {e}"));
    let (user_asm, _) = user_pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);

    let combined = format!("{}\n{}", stdlib_asm, user_asm);
    strip_comments(&combined)
}

fn strip_comments(asm: &str) -> String {
    asm.lines()
        .map(|line| line.split(';').next().unwrap_or("").trim_end())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

struct QemuResult {
    exit_code: i32,
    /// Everything the program wrote to stdout (after stripping our sentinel).
    stdout: String,
}

#[expect(
    dead_code,
    reason = "variants document platform-specific skip outcomes"
)]
#[derive(Debug)]
enum QemuSkipReason {
    NotWindows,
    WslUnavailable(String),
    WslLaunchFailed(String),
    WslWaitFailed(String),
    WslTimedOut(u64),
    MissingRiscv64Gcc,
    MissingQemuRiscv64,
}

/// Bounded wait for WSL + qemu-riscv64; a broken or unstarted WSL can block
/// `wsl.exe` indefinitely, so a timeout degrades to a skip rather than hanging.
const QEMU_WAIT_TIMEOUT_SECS: u64 = 60;

impl fmt::Display for QemuSkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotWindows => write!(
                f,
                "QEMU integration tests currently require Windows + WSL2. Install WSL2 and run the suite on Windows to enable them."
            ),
            Self::WslUnavailable(msg) => write!(
                f,
                "failed to start WSL ({msg}). Install WSL2 and make sure `wsl.exe` is available on PATH"
            ),
            Self::WslLaunchFailed(msg) => write!(
                f,
                "WSL could not launch the QEMU test shell ({msg}). Check that your WSL distro is installed and healthy"
            ),
            Self::WslWaitFailed(msg) => write!(
                f,
                "WSL terminated unexpectedly while waiting for QEMU ({msg}). Reinstall or repair WSL2 if this persists"
            ),
            Self::WslTimedOut(secs) => write!(
                f,
                "WSL/QEMU did not respond within {secs}s and was terminated. WSL is likely not started or is unhealthy; run `wsl --shutdown` then retry, or repair WSL2"
            ),
            Self::MissingRiscv64Gcc => write!(
                f,
                "missing `riscv64-linux-gnu-gcc` in WSL. Install the RISC-V cross toolchain, for example `sudo apt install gcc-riscv64-linux-gnu qemu-user` on Ubuntu/Debian WSL"
            ),
            Self::MissingQemuRiscv64 => write!(
                f,
                "missing `qemu-riscv64` in WSL. Install the RISC-V user-mode emulator, for example `sudo apt install qemu-user` on Ubuntu/Debian WSL"
            ),
        }
    }
}

fn qemu_skip_reason_from_output(combined: &str) -> Option<QemuSkipReason> {
    if combined.contains("TOOLCHAIN_UNAVAILABLE: qemu-riscv64 not found") {
        return Some(QemuSkipReason::MissingQemuRiscv64);
    }
    None
}

fn report_qemu_skip(test_name: &str, reason: QemuSkipReason) -> bool {
    eprintln!("[SKIP] {test_name} - {reason}");
    false
}

fn require_qemu_result(
    test_name: &'static str,
    result: Result<QemuResult, QemuSkipReason>,
) -> Option<QemuResult> {
    match result {
        Ok(value) => Some(value),
        Err(reason) => {
            let _ = report_qemu_skip(test_name, reason);
            None
        }
    }
}

/// Compile HLL source to the final assembled output and export it as an ELF
/// image ready for qemu-riscv64.
/// Uses two-stage compilation: compile stdlib and user code independently,
/// then assemble and link through the pipeline.
fn compile_to_elf(source: &str) -> Vec<u8> {
    let pipeline = CompilationPipeline::new();

    // Stage 1: compile the stdlib as independent per-module objects.
    let stdlib_objs = CompilationPipeline::compile_stdlib_objects(TargetMode::Hosted)
        .unwrap_or_else(|e| panic!("stdlib compilation failed: {e}"));

    // Stage 2: Compile user code
    let user_result = pipeline
        .compile(source)
        .unwrap_or_else(|e| panic!("HLL compilation failed: {e}"));
    let (_, user_tokens) = pipeline.compile_ir_to_assembly_with_tokens(&user_result.ir_program);
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

/// Convert a Windows absolute path to its WSL /mnt/... equivalent.
/// e.g. `C:\Users\foo\bar.elf` -> `/mnt/c/Users/foo/bar.elf`
#[cfg(target_os = "windows")]
fn win_path_to_wsl(path: &std::path::Path) -> String {
    let s = path.to_string_lossy();
    // Expect "X:\rest\of\path"
    let (drive, rest) = if s.len() >= 3 && s.as_bytes()[1] == b':' {
        let drive = s[..1].to_lowercase();
        let rest = s[2..].replace('\\', "/");
        (drive, rest)
    } else {
        // Not a drive-rooted path - best-effort conversion
        return s.replace('\\', "/");
    };
    format!("/mnt/{drive}{rest}")
}

/// Run a pre-compiled ELF image through WSL -> qemu-riscv64.
fn run_elf_via_qemu(elf: &[u8]) -> Result<QemuResult, QemuSkipReason> {
    #[cfg(not(target_os = "windows"))]
    {
        return Err(QemuSkipReason::NotWindows);
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        // Write ELF to a unique Windows temp file - avoids both binary-mode
        // issues with stdin piping and races between parallel test threads.
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let unique = format!("fst_{}_{}", std::process::id(), seq);
        let win_elf_path = std::env::temp_dir().join(format!("{unique}.elf"));
        std::fs::write(&win_elf_path, elf)
            .unwrap_or_else(|e| panic!("failed to write ELF to temp file: {e}"));
        let wsl_elf_path = win_path_to_wsl(&win_elf_path);

        let script = format!(
            r#"
export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH"

QEMU="$(command -v qemu-riscv64 2>/dev/null)"
if [ -z "$QEMU" ]; then
    echo "TOOLCHAIN_UNAVAILABLE: qemu-riscv64 not found"
    exit 0
fi

ELF_PATH="{wsl_elf_path}"
chmod +x "$ELF_PATH"
"$QEMU" "$ELF_PATH" 2>&1
echo "---EXIT:$?---"
"#
        );

        let child = match Command::new("wsl")
            .args(["--exec", "bash", "-lc", &script])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                return Err(QemuSkipReason::WslUnavailable(e.to_string()));
            }
        };

        // Wait on a helper thread so a hung WSL cannot block the test forever; on
        // timeout, kill the wsl.exe process tree and report it as an unavailable toolchain.
        let pid = child.id();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(child.wait_with_output());
        });
        let output = match rx.recv_timeout(std::time::Duration::from_secs(QEMU_WAIT_TIMEOUT_SECS)) {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return Err(QemuSkipReason::WslWaitFailed(e.to_string())),
            Err(_) => {
                // Best-effort, fire-and-forget: an unhealthy WSL can make `taskkill`
                // itself block, so do not wait on it or the test would hang again.
                let _ = Command::new("taskkill")
                    .args(["/T", "/F", "/PID", &pid.to_string()])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn();
                return Err(QemuSkipReason::WslTimedOut(QEMU_WAIT_TIMEOUT_SECS));
            }
        };

        let combined = String::from_utf8_lossy(&output.stdout).into_owned();
        if let Some(reason) = qemu_skip_reason_from_output(&combined) {
            return Err(reason);
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            eprintln!("[QEMU stderr]: {stderr}");
        }

        let sentinel_prefix = "---EXIT:";
        let exit_code = combined.lines().rev().find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with(sentinel_prefix) && trimmed.ends_with("---") {
                let inner = &trimmed[sentinel_prefix.len()..trimmed.len() - 3];
                inner.parse::<i32>().ok()
            } else {
                None
            }
        });
        let Some(exit_code) = exit_code else {
            // bash always echoes the sentinel, so its absence means WSL never ran the
            // script (stopped/unhealthy distro); skip, per the toolchain-absent policy.
            return Err(QemuSkipReason::WslLaunchFailed(format!(
                "no exit-code sentinel in WSL output (stdout={:?}, stderr={:?})",
                combined.trim(),
                stderr.trim()
            )));
        };

        let stdout = combined
            .lines()
            .filter(|l| !l.trim().starts_with(sentinel_prefix))
            .collect::<Vec<_>>()
            .join("\n");
        let stdout = stdout.trim_matches('\n').to_string();

        let _ = std::fs::remove_file(&win_elf_path);

        Ok(QemuResult { exit_code, stdout })
    }
}

/// Compile HLL and run through QEMU in one step.
fn run_hll_via_qemu(source: &str) -> Result<QemuResult, QemuSkipReason> {
    run_hll_elf_via_qemu(source)
}

/// Compile HLL to ELF and run through QEMU in one step.
fn run_hll_elf_via_qemu(source: &str) -> Result<QemuResult, QemuSkipReason> {
    let elf = compile_to_elf(source);
    run_elf_via_qemu(&elf)
}

/// Verifies i32 mul/div/mod/sub, i64 cast roundtrip, u32 unsigned arithmetic,
/// abs/clamp/gcd helpers, signed vs unsigned comparisons, and operator
/// precedence.  The program returns 42 if every check passes.
#[test]
fn qemu_01_arithmetic_and_types() {
    let source = read_program("01_arithmetic_and_types.hll");
    let Some(result) =
        require_qemu_result("qemu_01_arithmetic_and_types", run_hll_via_qemu(&source))
    else {
        return;
    };
    assert_eq!(
        result.exit_code, 42,
        "01_arithmetic_and_types: expected exit 42 (all arithmetic checks pass); \
         a non-42 code names the failing assertion (1=prod, 2=diff, 3=quot, 4=rem, \
         5=cast, 6=udiv, 7=umod, 8-9=abs, 10-12=clamp, 13-14=gcd, 15-16=sign, \
         17=overflow, 18=precedence)"
    );
}

/// Verifies if/else chains (categorise helper), while accumulation, break,
/// continue, nested loops, compile-time const, and boolean infix `and`.
/// The five sub-results sum to exactly 100.
#[test]
fn qemu_02_control_flow() {
    let source = read_program("02_control_flow.hll");
    let Some(result) = require_qemu_result("qemu_02_control_flow", run_hll_via_qemu(&source))
    else {
        return;
    };
    assert_eq!(
        result.exit_code, 100,
        "02_control_flow: expected exit 100 (category=3 + sum=55 + break=7 + \
         evens=10 + inner=25); codes 201-206 name the failing sub-check"
    );
}

/// Verifies inline struct literals, named type aliases, struct pass-by-value,
/// dot product, scaling, the small-struct RISC-V ABI return path
/// (destructuring from a function call result), local-variable destructuring,
/// partial destructuring, order-independent field binding, nested struct
/// field access, and anonymous inline struct literals.
#[test]
fn qemu_03_structs_and_destructuring() {
    let source = read_program("03_structs_and_destructuring.hll");
    let Some(result) = require_qemu_result(
        "qemu_03_structs_and_destructuring",
        run_hll_via_qemu(&source),
    ) else {
        return;
    };
    assert_eq!(
        result.exit_code, 0,
        "03_structs_and_destructuring: non-zero exit names the failing assertion \
         (1-2=field access, 3=dot, 4-5=scaled fields, 6-7=div_rem first, \
         8-9=div_rem second, 10-11=local destructure, 12=partial, \
         13-14=order-independent, 15-17=Range fields/len, 18-19=range_contains, \
         20-22=clamp_to, 23-24=add_vec, 25-26=inline, 27-28=struct pointer param)"
    );
}

/// Verifies new/free, defer free (guaranteed cleanup), address-of stack
/// variables, pointer mutation via function parameters, pointer swap,
/// stack arrays (@arr[i] read/write), variable-index array element address,
/// chained pointer dereference (@@), and passing array pointers to functions.
///
/// Every heap allocation is explicitly freed before the program exits;
/// a clean exit-0 confirms no corruption or missing free.
#[test]
fn qemu_04_pointers_and_memory() {
    let source = read_program("04_pointers_and_memory.hll");
    let Some(result) =
        require_qemu_result("qemu_04_pointers_and_memory", run_hll_via_qemu(&source))
    else {
        return;
    };
    assert_eq!(
        result.exit_code, 0,
        "04_pointers_and_memory: non-zero exit names the failing assertion \
         (1-2=basic new/write/read, 3=defer write, 4=address-of local, \
         5-6=swap, 7=increment, 8=stack array sum, 9=overwrite+sum, \
         10-11=variable-index addr, 12-13=chained deref, 14=dot product)"
    );
}

/// Verifies iterative factorial and Fibonacci (with boundary values),
/// is_prime (including edge cases 1, 2, even numbers), prime counting,
/// power function, function composition (fibcount_primes), and external
/// putchar I/O.  On success the program writes "PASS\n" to stdout and
/// returns 0.
#[test]
fn qemu_05_functions_and_io() {
    let source = read_program("05_functions_and_io.hll");
    let Some(result) = require_qemu_result("qemu_05_functions_and_io", run_hll_via_qemu(&source))
    else {
        return;
    };

    // Verify the I/O first so a mis-printed output gets its own message.
    assert_eq!(
        result.stdout, "PASS",
        "05_functions_and_io: expected stdout \"PASS\" (putchar wrote wrong bytes or \
         the program exited before reaching the I/O section)"
    );

    assert_eq!(
        result.exit_code, 0,
        "05_functions_and_io: non-zero exit names the failing assertion \
         (1-4=factorial, 5-8=fibonacci, 9-12=is_prime, 13-14=count_primes, \
         15-17=power, 18-19=composition)"
    );
}

/// Verifies that the assembled output can be exported as an ELF image and run
/// directly under qemu-riscv64 without going through the GCC linker path.
/// The arithmetic-and-types program is pure compute, so it avoids any libc or
/// UART assumptions and gives a stable exit code.
#[test]
fn qemu_06_elf_export_and_execution() {
    let source = read_program("01_arithmetic_and_types.hll");
    let Some(result) = require_qemu_result(
        "qemu_06_elf_export_and_execution",
        run_hll_elf_via_qemu(&source),
    ) else {
        return;
    };
    assert_eq!(
        result.exit_code, 42,
        "06_elf_export_and_execution: expected exit 42 after running the exported ELF under qemu-riscv64"
    );
}

// These run on every platform (no WSL needed) and confirm the full
// HLL -> IR -> assembly pipeline doesn't panic or error on any of the five
// programs.  They are cheap and always active.

#[test]
fn qemu_programs_compile_to_asm_without_error() {
    let files = [
        "01_arithmetic_and_types.hll",
        "02_control_flow.hll",
        "03_structs_and_destructuring.hll",
        "04_pointers_and_memory.hll",
        "05_functions_and_io.hll",
    ];
    for filename in files {
        let source = read_program(filename);
        // compile_to_asm panics on error, which becomes a test failure.
        let asm = compile_to_asm(&source);
        assert!(
            asm.contains("main:"),
            "{filename}: expected a main label in output"
        );
    }
}
