/// QEMU execution integration tests.
///
/// Each test compiles an HLL program all the way to RISC-V assembly, then
/// runs it under qemu-riscv64 inside WSL and verifies the exit code and
/// (where applicable) stdout output.
///
/// If the WSL toolchain (riscv64-linux-gnu-gcc + qemu-riscv64) is absent
/// the tests print a diagnostic and return without failing, so CI on
/// machines without the cross-toolchain stays green.
use full_stack::high_level_language::compilation_pipeline::CompilationPipeline;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn qemu_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("programs/test/qemu")
}

fn read_program(filename: &str) -> String {
    let path = qemu_dir().join(filename);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {path:?}: {e}"))
}

/// Compile HLL source → assembly text, stripping inline comments so the
/// assembly can be passed safely through a shell heredoc.
fn compile_to_asm(source: &str) -> String {
    let pipeline = CompilationPipeline::new();
    let result = pipeline
        .compile(source)
        .unwrap_or_else(|e| panic!("HLL compilation failed: {e}"));
    let asm = pipeline.compile_ir_to_assembly(&result.ir_program);

    // Mirror the comment-strip that app.rs does before sending to WSL.
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

/// Run a pre-compiled assembly string through WSL → riscv64-linux-gnu-gcc
/// → qemu-riscv64.  Returns `None` when the toolchain is unavailable so
/// callers can skip gracefully.
fn run_asm_via_qemu(asm: &str) -> Option<QemuResult> {
    // The bash script is identical in structure to the one in app.rs but it
    // also prints a sentinel line so we can reliably extract the exit code
    // even when the program itself produces output.
    let script = r#"
export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH"

CC="$(which riscv64-linux-gnu-gcc 2>/dev/null)"
if [ -z "$CC" ]; then
    echo "TOOLCHAIN_UNAVAILABLE: riscv64-linux-gnu-gcc not found"
    exit 0
fi
QEMU="$(which qemu-riscv64 2>/dev/null)"
if [ -z "$QEMU" ]; then
    echo "TOOLCHAIN_UNAVAILABLE: qemu-riscv64 not found"
    exit 0
fi

WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT
cd "$WORKDIR"

cat > program.s

# Compile; capture stderr so a failed compile is surfaced in the Rust output.
COMPILE_OUT="$("$CC" -static program.s -o program 2>&1)"
if [ $? -ne 0 ]; then
    echo "LINK_FAILED: $COMPILE_OUT"
    exit 0
fi

"$QEMU" ./program
echo "---EXIT:$?---"
"#;

    #[cfg(not(target_os = "windows"))]
    {
        // On non-Windows hosts wsl is not available; skip.
        eprintln!("[qemu_execution] not on Windows, skipping QEMU tests");
        return None;
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut child = match Command::new("wsl")
            .args(["--exec", "bash", "-lc", script])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[qemu_execution] failed to start WSL: {e}; skipping");
                return None;
            }
        };

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(asm.as_bytes());
        }

        let output = match child.wait_with_output() {
            Ok(o) => o,
            Err(e) => {
                eprintln!("[qemu_execution] WSL wait failed: {e}; skipping");
                return None;
            }
        };

        let combined = String::from_utf8_lossy(&output.stdout).into_owned();

        // Detect toolchain or link failures and skip instead of panicking.
        if combined.contains("TOOLCHAIN_UNAVAILABLE") {
            eprintln!("[qemu_execution] {}", combined.trim());
            return None;
        }
        if combined.contains("LINK_FAILED") {
            panic!("assembly link step failed:\n{combined}");
        }

        // Parse the sentinel line "---EXIT:N---".
        let sentinel_prefix = "---EXIT:";
        let exit_code = combined
            .lines()
            .rev()
            .find_map(|line| {
                let trimmed = line.trim();
                if trimmed.starts_with(sentinel_prefix) && trimmed.ends_with("---") {
                    let inner = &trimmed[sentinel_prefix.len()..trimmed.len() - 3];
                    inner.parse::<i32>().ok()
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                panic!("could not find exit-code sentinel in WSL output:\n{combined}")
            });

        // Stdout = everything before the sentinel line.
        let stdout = combined
            .lines()
            .filter(|l| {
                let t = l.trim();
                !t.starts_with(sentinel_prefix)
            })
            .collect::<Vec<_>>()
            .join("\n");
        // Trim leading/trailing blank lines but preserve internal newlines.
        let stdout = stdout.trim_matches('\n').to_string();

        Some(QemuResult { exit_code, stdout })
    }
}

/// Compile HLL and run through QEMU in one step.
fn run_hll_via_qemu(source: &str) -> Option<QemuResult> {
    let asm = compile_to_asm(source);
    run_asm_via_qemu(&asm)
}

// ── Test 1: Arithmetic & Types ────────────────────────────────────────────────

/// Verifies i32 mul/div/mod/sub, i64 cast roundtrip, u32 unsigned arithmetic,
/// abs/clamp/gcd helpers, signed vs unsigned comparisons, and operator
/// precedence.  The program returns 42 if every check passes.
#[test]
fn qemu_01_arithmetic_and_types() {
    let source = read_program("01_arithmetic_and_types.hll");
    let result = match run_hll_via_qemu(&source) {
        Some(r) => r,
        None => {
            eprintln!("[SKIP] qemu_01_arithmetic_and_types – toolchain unavailable");
            return;
        }
    };
    assert_eq!(
        result.exit_code, 42,
        "01_arithmetic_and_types: expected exit 42 (all arithmetic checks pass); \
         a non-42 code names the failing assertion (1=prod, 2=diff, 3=quot, 4=rem, \
         5=cast, 6=udiv, 7=umod, 8-9=abs, 10-12=clamp, 13-14=gcd, 15-16=sign, \
         17=overflow, 18=precedence)"
    );
}

// ── Test 2: Control Flow ──────────────────────────────────────────────────────

/// Verifies if/else chains (categorise helper), while accumulation, break,
/// continue, nested loops, compile-time const, and boolean infix `and`.
/// The five sub-results sum to exactly 100.
#[test]
fn qemu_02_control_flow() {
    let source = read_program("02_control_flow.hll");
    let result = match run_hll_via_qemu(&source) {
        Some(r) => r,
        None => {
            eprintln!("[SKIP] qemu_02_control_flow – toolchain unavailable");
            return;
        }
    };
    assert_eq!(
        result.exit_code, 100,
        "02_control_flow: expected exit 100 (category=3 + sum=55 + break=7 + \
         evens=10 + inner=25); codes 201-206 name the failing sub-check"
    );
}

// ── Test 3: Structs & Destructuring ──────────────────────────────────────────

/// Verifies inline struct literals, named type aliases, struct pass-by-value,
/// dot product, scaling, the small-struct RISC-V ABI return path
/// (destructuring from a function call result), local-variable destructuring,
/// partial destructuring, order-independent field binding, nested struct
/// field access, and anonymous inline struct literals.
#[test]
fn qemu_03_structs_and_destructuring() {
    let source = read_program("03_structs_and_destructuring.hll");
    let result = match run_hll_via_qemu(&source) {
        Some(r) => r,
        None => {
            eprintln!("[SKIP] qemu_03_structs_and_destructuring – toolchain unavailable");
            return;
        }
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

// ── Test 4: Pointers & Memory ─────────────────────────────────────────────────

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
    let result = match run_hll_via_qemu(&source) {
        Some(r) => r,
        None => {
            eprintln!("[SKIP] qemu_04_pointers_and_memory – toolchain unavailable");
            return;
        }
    };
    assert_eq!(
        result.exit_code, 0,
        "04_pointers_and_memory: non-zero exit names the failing assertion \
         (1-2=basic new/write/read, 3=defer write, 4=address-of local, \
         5-6=swap, 7=increment, 8=stack array sum, 9=overwrite+sum, \
         10-11=variable-index addr, 12-13=chained deref, 14=dot product)"
    );
}

// ── Test 5: Functions & I/O ───────────────────────────────────────────────────

/// Verifies iterative factorial and Fibonacci (with boundary values),
/// is_prime (including edge cases 1, 2, even numbers), prime counting,
/// power function, function composition (fib∘count_primes), and external
/// C FFI via putchar.  On success the program writes "PASS\n" to stdout
/// and returns 0.
#[test]
fn qemu_05_functions_and_io() {
    let source = read_program("05_functions_and_io.hll");
    let result = match run_hll_via_qemu(&source) {
        Some(r) => r,
        None => {
            eprintln!("[SKIP] qemu_05_functions_and_io – toolchain unavailable");
            return;
        }
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

// ── Compile-only smoke tests ──────────────────────────────────────────────────
//
// These run on every platform (no WSL needed) and confirm the full
// HLL → IR → assembly pipeline doesn't panic or error on any of the five
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
            asm.contains(".globl main") || asm.contains(".globl "),
            "{filename}: expected at least one .globl directive in output"
        );
    }
}
