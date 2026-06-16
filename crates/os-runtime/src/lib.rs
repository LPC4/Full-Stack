//! Firmware: all bare-metal source files embedded as compile-time string constants.
//!
//! ## Boot sequence
//!
//! ```text
//! Power-on
//!   boot/startup.s       M-mode ROM: PMP, delegation, mret to S-mode
//!   boot/trap.s          M-mode ROM: trap handler, syscall dispatch
//!   kernel/entry.hll     S-mode kernel entry (_kernel_start -> kmain)
//!
//! Hosted / userspace programs
//!   stdlib/hosted/runtime.hll     _start -> main() -> sys_exit
//!   stdlib/freestanding/          bare-metal programs (no Linux syscalls)
//!   stdlib/common/                shared: types, malloc, string utils, mem, klog
//! ```
//!
//! Consumers:
//!   - [`virtual_machine`] assembles [`ROM_SOURCE`] into the ROM image at startup.
//!   - [`hll_to_ir`] uses [`stdlib`] and [`kernel`] constants to build stdlib bundles.

/// M-mode boot stub (RISC-V assembly): PMP, delegation, mret to S-mode.
/// Placed at ROM offset 0x000, padded to 0x100 bytes.
pub const BOOT_STARTUP: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/boot/startup.s"));

/// M-mode trap handler (RISC-V assembly): ecall dispatch and syscall implementations.
/// Placed at ROM offset 0x100, immediately after the `_start` padding.
pub const BOOT_TRAP: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/boot/trap.s"));

/// Full ROM source: `BOOT_STARTUP` concatenated with `BOOT_TRAP`.
/// `_start` at 0x000, `_m_trap` at 0x100.
pub const ROM_SOURCE: &str = concat!(
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/boot/startup.s")),
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/boot/trap.s")),
);

/// HLL standard library source fragments, one constant per file.
///
/// Consumers assemble these in order to build a complete stdlib bundle.
/// See `hll_to_ir::stdlib` for the three supported link orders.
pub mod stdlib {
    /// Shared type definitions (`Str`, `HeapBlock`).
    pub const TYPES: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/common/types.hll"
    ));

    /// Bump-pointer allocator over a fixed 64 KB `.bss` buffer (`malloc`, `free`,
    /// `heap_raw_alloc`). Used by the kernel and freestanding builds, which have
    /// no syscalls. Userspace uses `MEMORY_ALLOCATOR_HOSTED` instead.
    pub const MEMORY_ALLOCATOR: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/common/memory_allocator.hll"
    ));

    /// Growable userspace allocator (`malloc`, `free`, `heap_raw_alloc`) backed by
    /// the `brk` syscall, so U-mode programs can grow the heap past 64 KB. Used
    /// only in hosted mode; same API as `MEMORY_ALLOCATOR`.
    pub const MEMORY_ALLOCATOR_HOSTED: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/hosted/memory_allocator.hll"
    ));

    /// String utilities (`str_len`, `str_equals`, `str_copy`, `str_concat`).
    pub const STRING_UTILS: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/common/string_utils.hll"
    ));

    /// Low-level memory primitives (`memset`, `memcpy`, `memmove`, `memcmp`).
    pub const MEM: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/common/mem.hll"
    ));

    /// Kernel logging helpers (`klog`, `klog_ok`, `klog_warn`, `klog_error`, `klog_int`, `klog_hex`).
    pub const KLOG: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/common/klog.hll"
    ));

    /// Hosted (Linux userspace) runtime: `_start`, `putchar`, `puts`, `print_int`, `exit`.
    pub const HOSTED_RUNTIME: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/hosted/runtime.hll"
    ));

    /// Hosted userspace syscall wrappers (`sc_open`, `sc_read`, ...) and C-string
    /// helpers (`cstr_len`, `cstr_eq`, ...), shared by the bundled user programs.
    pub const HOSTED_SYSCALLS: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/hosted/syscalls.hll"
    ));

    /// Freestanding runtime: `_kpanic` / `kpanic` (UART direct-write, no syscalls).
    pub const FREESTANDING_RUNTIME: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/freestanding/runtime.hll"
    ));

    /// Freestanding console: `console_putchar`, `console_write`, `console_writeln`,
    /// `console_print_int`, `console_print_hex` (NS16550A UART at 0x10000000).
    pub const FREESTANDING_CONSOLE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/freestanding/console.hll"
    ));

    /// Freestanding entry wrapper: `_start` calls `main`, then halts via SYSCON.
    /// ONLY included in pure freestanding mode, kernel has its own `_kernel_start`.
    pub const FREESTANDING_ENTRY: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/stdlib/freestanding/entry.hll"
    ));
}

/// Kernel-mode HLL source fragments.
pub mod kernel {
    /// Kernel entry: minimal kernel entrypoint (`_kernel_start` -> `kmain`).
    pub const RUNTIME: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/entry.hll"));

    /// S-mode trap entry: stvec prologue/epilogue, trap_init, sscratch helpers.
    /// The entry-point for all S-mode traps and interrupts.
    pub const TRAP_ENTRY: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/kernel/trap_entry.hll"
    ));

    /// Kernel platform helpers: kmalloc, kshutdown, timer, PLIC init.
    /// Core kernel infrastructure functions that use externs.
    pub const UTILITIES: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/utilities.hll"));

    /// Kernel checks and diagnostics: memory_self_test, etc.
    /// Called during boot to validate kernel systems.
    pub const CHECKS: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/checks.hll"));

    /// S-mode trap dispatcher: `trap_handler(frame: u64*)`.
    /// Reads scause from the trap frame and dispatches to timer/external/software
    /// interrupt handlers or exception handlers.  Depends on `kpanic`, `klog_hex`,
    /// and `timer_set` (all provided by the kernel stdlib bundle).
    pub const TRAP_HANDLER: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/kernel/trap_handler.hll"
    ));

    /// Physical Memory Manager: `pmm_init`, `pmm_alloc`, `pmm_free`.
    /// 4 KiB page granularity; free-list + bump allocator.
    pub const PMM: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/pmm.hll"));

    /// Sv39 Virtual Memory Manager: `vmm_init`, `vmm_enable`, `vmm_map`,
    /// `vmm_map_1gib`, `vmm_map_range`. Depends on `pmm_alloc` and `memset`.
    pub const VMM: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/vmm.hll"));

    /// Process Control Block and lifecycle: `process_init`, `process_create`.
    /// Depends on `pmm_alloc`, `vmm_map`, `kmalloc`, `memset`, `memcpy`.
    pub const PROCESS: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/process.hll"));

    /// Syscall dispatch: `syscall_dispatch`, `sys_write_impl`.
    /// Depends on `klog_int`, `klog_error`.
    pub const SYSCALL: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/syscall.hll"));

    /// Round-robin scheduler: `scheduler_init`, `scheduler_add`, `schedule`.
    /// Depends on `memcpy`, `kpanic`, `klog_*`.
    pub const SCHEDULER: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/scheduler.hll"));

    /// Inode-based read-write filesystem: `fs_init`, `fs_open`, `fs_read`, `fs_write`,
    /// `fs_close`, `fs_create`, `fs_mkdir`, `fs_rename`, `fs_unlink`, `fs_rmdir`.
    pub const FS: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/fs.hll"));

    /// Reference kernel: full boot sequence demonstrating real and stub subsystems.
    /// Defines `kmain`; depends on the kernel stdlib bundle.
    pub const MY_KERNEL: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/kernel/my_kernel.hll"));
}

/// User-space programs and example sources. The source tree mirrors the runtime
/// boot FS: `user/bin` -> `/bin` tools, `user/demo` -> `/home/demo` demos,
/// `user/examples` -> `/home/src` sample sources. `user/fixtures` holds frozen
/// test inputs that are not installed.
pub mod user {
    // --- bin: tools installed under /bin ---

    /// Interactive shell: reads UART input and runs built-in commands
    /// (`ls`, `cd`, `run`, `exit`). Compiled in hosted mode and booted as pid 1.
    pub const SHELL: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/user/bin/shell.hll"));

    /// File-management builtins (`touch`/`mkdir`/`rm`/`rmdir`/`mv`) for the shell,
    /// split into their own translation unit and linked with `SHELL` by the host
    /// toolchain. They share `sh_join_path` via an `external` decl.
    pub const SHELL_FILEOPS: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/user/bin/shell_fileops.hll"
    ));

    /// Tiny line editor (ed-like). Reads its target path from USER_ARG_BASE,
    /// loads the file, and edits it with append/print/clear/write/quit commands.
    /// Compiled in hosted mode and launched by the shell's `edit` command.
    pub const EDIT: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/user/bin/edit.hll"));

    /// Minimal in-VM RV64I assembler. Reads a `.s` file, assembles a small
    /// instruction subset, and writes a runnable ELF. Installed at `/bin/as.elf`
    /// and launched by the shell's `as <src> <out>` command.
    pub const AS: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/user/bin/as.hll"));

    /// ET_REL object serializer for `as`, split into its own translation unit and
    /// linked with `AS` by the host toolchain. Shares the assembler state with
    /// `as.hll` via `external` globals.
    pub const AS_OBJECT: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/user/bin/as_object.hll"
    ));

    /// Minimal in-VM HLL-0 compiler. Reads an `.hll` source, parses the
    /// HLL-0 subset, and writes naive stack-machine assembly in the `/bin/as`
    /// subset. Installed at `/bin/cc.elf` and launched by the shell's
    /// `cc <src.hll> <out.s>` command; pairs with `as` for the self-hosting demo.
    pub const CC: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/user/bin/cc.hll"));

    /// HLL-0 code generator for `cc`, split into its own translation unit and
    /// linked with `CC` by the host toolchain. Walks the shared AST
    /// tables via `external` globals and emits the stack-machine assembly.
    pub const CC_CODEGEN: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/user/bin/cc_codegen.hll"
    ));

    /// In-VM static linker. Reads N relocatable `ET_REL` objects
    /// produced by `as <src> <out>.o`, merges their sections, resolves the global
    /// symbol table, applies relocations, and writes a runnable ELF. Installed at
    /// `/bin/ld.elf` and launched by the shell's `ld <obj>... <out>` command.
    pub const LD: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/user/bin/ld.hll"));

    /// Relocation patching + executable emission for `ld`, split into its own
    /// translation unit and linked with `LD` by the host toolchain.
    /// Reads the merged sections and symbol table via `external` globals.
    pub const LD_LINK: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/user/bin/ld_link.hll"));

    // --- demo: programs installed under /home/demo ---

    /// Framebuffer demo: maps the framebuffer and renders a Mandelbrot set.
    /// Installed at `/home/demo/mandelbrot.elf`.
    pub const MANDELBROT: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/user/demo/mandelbrot.hll"
    ));

    /// Spinning 3D wireframe cube demo: maps the framebuffer and animates a
    /// rotating cube. Installed at `/home/demo/cube.elf`.
    pub const CUBE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/user/demo/cube.hll"));

    /// Conway's Game of Life demo: a toroidal grid animated on the framebuffer
    /// with P/R/space keyboard control. Installed at `/home/demo/life.elf`.
    pub const LIFE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/user/demo/life.hll"));

    /// Hello-world user program: writes a greeting via ecall, then yields forever.
    pub const USER_HELLO: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/user/demo/user_hello.hll"
    ));

    // --- examples: sample sources installed under /home/src ---

    /// Example assembly source: sum a stack-built array, exit with the total (42).
    /// Exercises the expanded assembler subset (sd/ld with offset(reg), slli, bge).
    /// Installed at `/home/src/array.s` so `as` can be tried out of the box.
    pub const EXAMPLE_ARRAY_S: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/user/examples/array.s"
    ));

    /// HLL-0 sample for the in-VM `cc`. Calls `putc` but does not define it; the
    /// symbol is resolved at link time against `EXAMPLE_STDLIB_S`, so this is the
    /// headline `cc`+`as`+`ld` client. Installed at `/home/src/hello.hll`.
    pub const CC_DEMO_HLL: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/user/examples/hello.hll"
    ));

    /// Tiny user-space stdlib (`putc`/`puts`/`exit`) as assembly, meant to be
    /// assembled to an object and linked with a client program by `ld`. Installed
    /// at `/home/src/stdlib.s`.
    pub const EXAMPLE_STDLIB_S: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/user/examples/stdlib.s"
    ));

    // --- fixtures: frozen test inputs, not installed ---

    /// HLL-0 reference source for the in-VM `cc`; host-compilable.
    pub const CC_HELLO_HLL: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/user/fixtures/hello.hll"
    ));

    /// The assembly `cc` must emit for `CC_HELLO_HLL`; the frozen codegen target.
    pub const CC_HELLO_S: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/user/fixtures/hello.s"
    ));

    // --- Catalog: one source of truth for "what user programs exist" ---

    /// Role of a bundled user program. Determines where (and whether) it is
    /// installed into the boot filesystem image.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum UserProgramKind {
        /// HLL source compiled to an ELF and installed under `/bin`.
        Tool,
        /// HLL source compiled to an ELF and installed under `/home/demo`.
        Demo,
        /// Verbatim source installed under `/home/src` so the toolchain can be
        /// tried out of the box.
        Example,
        /// Frozen test input; not installed into the boot image.
        Fixture,
    }

    /// One bundled user program: catalog identity, role, boot-FS install path
    /// (`None` when not auto-installed), and embedded source. `PROGRAMS` is the
    /// single list every consumer (boot FS image, GUI catalog, tests) iterates.
    #[derive(Clone, Copy, Debug)]
    pub struct UserProgram {
        /// Stable key: cache key and (for Tool/Demo) catalog id `user-<name>`.
        pub name: &'static str,
        /// Human-facing display name for the GUI catalog.
        pub title: &'static str,
        /// One-line catalog description.
        pub description: &'static str,
        pub kind: UserProgramKind,
        /// Boot-FS install path, or `None` for the init shell / fixtures / the
        /// on-demand hello demo.
        pub install_path: Option<&'static str>,
        /// Primary translation unit.
        pub source: &'static str,
        /// Additional translation units linked with `source`. Empty for single-file
        /// programs; each is compiled to its own object and linked with the primary
        /// one and the stdlib.
        pub aux_sources: &'static [&'static str],
        /// Display names for `aux_sources`, parallel by index. Used by the GUI
        /// catalog to show each aux unit as a named, editable module.
        pub aux_names: &'static [&'static str],
    }

    impl UserProgram {
        /// HLL programs the host compiles to an ELF (tools + demos), as opposed
        /// to verbatim example sources and uninstalled fixtures.
        pub fn is_compiled(&self) -> bool {
            matches!(self.kind, UserProgramKind::Tool | UserProgramKind::Demo)
        }

        /// The aux translation units paired with their display names, in order.
        pub fn aux_modules(&self) -> impl Iterator<Item = (&'static str, &'static str)> {
            self.aux_names
                .iter()
                .copied()
                .zip(self.aux_sources.iter().copied())
        }
    }

    use UserProgramKind::{Demo, Example, Fixture, Tool};

    /// Every bundled user program, in catalog display order. Adding a program
    /// means appending one row here -- the boot FS image, the GUI catalog, and
    /// the userspace compile test all derive from this single list.
    pub const PROGRAMS: &[UserProgram] = &[
        // Tools (/bin). The shell is the init process, compiled but not installed.
        UserProgram {
            name: "shell",
            title: "Shell",
            description: "Interactive shell (pid 1): ls, cd, run, cat, edit, as, file management.",
            kind: Tool,
            install_path: None,
            source: SHELL,
            aux_sources: &[SHELL_FILEOPS],
            aux_names: &["shell_fileops"],
        },
        UserProgram {
            name: "edit",
            title: "Editor",
            description: "ed-style line editor launched by the shell's `edit` command.",
            kind: Tool,
            install_path: Some("/bin/edit.elf"),
            source: EDIT,
            aux_sources: &[],
            aux_names: &[],
        },
        UserProgram {
            name: "as",
            title: "Assembler",
            description: "In-VM RV64I assembler launched by the shell's `as` command.",
            kind: Tool,
            install_path: Some("/bin/as.elf"),
            source: AS,
            aux_sources: &[AS_OBJECT],
            aux_names: &["as_object"],
        },
        UserProgram {
            name: "cc",
            title: "Compiler",
            description: "In-VM HLL-0 compiler launched by the shell's `cc` command.",
            kind: Tool,
            install_path: Some("/bin/cc.elf"),
            source: CC,
            aux_sources: &[CC_CODEGEN],
            aux_names: &["cc_codegen"],
        },
        UserProgram {
            name: "ld",
            title: "Linker",
            description: "In-VM static linker launched by the shell's `ld` command.",
            kind: Tool,
            install_path: Some("/bin/ld.elf"),
            source: LD,
            aux_sources: &[LD_LINK],
            aux_names: &["ld_link"],
        },
        // Demos (/home/demo). `hello` is injected on demand, not auto-installed.
        UserProgram {
            name: "cube",
            title: "Cube Demo",
            description: "Spinning 3D wireframe cube on the framebuffer device.",
            kind: Demo,
            install_path: Some("/home/demo/cube.elf"),
            source: CUBE,
            aux_sources: &[],
            aux_names: &[],
        },
        UserProgram {
            name: "mandelbrot",
            title: "Mandelbrot Demo",
            description: "Framebuffer Mandelbrot renderer.",
            kind: Demo,
            install_path: Some("/home/demo/mandelbrot.elf"),
            source: MANDELBROT,
            aux_sources: &[],
            aux_names: &[],
        },
        UserProgram {
            name: "life",
            title: "Game of Life Demo",
            description: "Conway's Game of Life on the framebuffer (P pause, R reseed, space step).",
            kind: Demo,
            install_path: Some("/home/demo/life.elf"),
            source: LIFE,
            aux_sources: &[],
            aux_names: &[],
        },
        UserProgram {
            name: "hello",
            title: "Hello",
            description: "Minimal user program: prints a greeting, then yields forever.",
            kind: Demo,
            install_path: None,
            source: USER_HELLO,
            aux_sources: &[],
            aux_names: &[],
        },
        // Example sources (/home/src): installed verbatim, not compiled here.
        UserProgram {
            name: "ex_array",
            title: "array.s",
            description: "Example assembly: sum a stack array, exit 42. Try with `as`.",
            kind: Example,
            install_path: Some("/home/src/array.s"),
            source: EXAMPLE_ARRAY_S,
            aux_sources: &[],
            aux_names: &[],
        },
        UserProgram {
            name: "ex_hello_hll",
            title: "hello.hll",
            description: "HLL-0 sample for `cc`, linked against stdlib.s by `ld`.",
            kind: Example,
            install_path: Some("/home/src/hello.hll"),
            source: CC_DEMO_HLL,
            aux_sources: &[],
            aux_names: &[],
        },
        UserProgram {
            name: "ex_stdlib",
            title: "stdlib.s",
            description: "Tiny user-space stdlib (putc/puts/exit) linked into cc programs by `ld`.",
            kind: Example,
            install_path: Some("/home/src/stdlib.s"),
            source: EXAMPLE_STDLIB_S,
            aux_sources: &[],
            aux_names: &[],
        },
        // Fixtures: frozen test inputs, not installed.
        UserProgram {
            name: "fx_hello_hll",
            title: "hello.hll (fixture)",
            description: "HLL-0 reference source for `cc`.",
            kind: Fixture,
            install_path: None,
            source: CC_HELLO_HLL,
            aux_sources: &[],
            aux_names: &[],
        },
        UserProgram {
            name: "fx_hello_s",
            title: "hello.s (fixture)",
            description: "Frozen codegen target `cc` must emit for the reference source.",
            kind: Fixture,
            install_path: None,
            source: CC_HELLO_S,
            aux_sources: &[],
            aux_names: &[],
        },
    ];

    /// Look up a program by its stable `name` key.
    pub fn program(name: &str) -> Option<&'static UserProgram> {
        PROGRAMS.iter().find(|p| p.name == name)
    }
}
