//! Static benchmark registry for the tracked benchmark suite.

/// Pipeline knob set for one variant. Defaults mirror `CompilationPipeline`:
/// regalloc/peephole/omit-fp ON, IR optimization OFF.
#[derive(Clone, Copy, Debug)]
pub struct Knobs {
    pub ir_opt: bool,
    pub peephole: bool,
    pub regalloc: bool,
    pub omit_frame_pointer: bool,
}

impl Knobs {
    pub const DEFAULT: Self = Self {
        ir_opt: false,
        peephole: true,
        regalloc: true,
        omit_frame_pointer: true,
    };
    pub const fn ir_opt_on() -> Self {
        Self {
            ir_opt: true,
            ..Self::DEFAULT
        }
    }
    pub const fn regalloc_off() -> Self {
        Self {
            regalloc: false,
            ..Self::DEFAULT
        }
    }
    /// Peephole folds stack-slot reloads, so its A/B runs with regalloc off;
    /// with regalloc on the pass finds nothing (values never touch the stack).
    pub const fn noreg_peephole_off() -> Self {
        Self {
            regalloc: false,
            peephole: false,
            ..Self::DEFAULT
        }
    }
    pub const fn noreg_peephole_on() -> Self {
        Self {
            regalloc: false,
            peephole: true,
            ..Self::DEFAULT
        }
    }
    pub const fn fp_kept() -> Self {
        Self {
            omit_frame_pointer: false,
            ..Self::DEFAULT
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Variant {
    pub name: &'static str,
    pub knobs: Knobs,
}

/// One benchmark; `{{N}}` in the source becomes `base_n * input_size`.
/// The first variant is the baseline for downstream speedup computations.
#[derive(Clone, Copy, Debug)]
pub struct BenchDef {
    pub name: &'static str,
    pub file: &'static str,
    pub group: &'static str,
    pub base_n: u64,
    pub max_steps: u64,
    pub variants: &'static [Variant],
}

const DEFAULT_ONLY: &[Variant] = &[Variant {
    name: "default",
    knobs: Knobs::DEFAULT,
}];

pub const SUITE_NAME: &str = "teaching";

pub const BENCHES: &[BenchDef] = &[
    BenchDef {
        name: "const_fold_dce",
        file: "const_fold_dce.hll",
        group: "opt",
        base_n: 200_000,
        max_steps: 200_000_000,
        variants: &[
            Variant {
                name: "opt_off",
                knobs: Knobs::DEFAULT,
            },
            Variant {
                name: "opt_on",
                knobs: Knobs::ir_opt_on(),
            },
        ],
    },
    BenchDef {
        name: "regalloc_pressure",
        file: "regalloc_pressure.hll",
        group: "opt",
        base_n: 100_000,
        max_steps: 200_000_000,
        variants: &[
            Variant {
                name: "regalloc_off",
                knobs: Knobs::regalloc_off(),
            },
            Variant {
                name: "regalloc_on",
                knobs: Knobs::DEFAULT,
            },
        ],
    },
    BenchDef {
        name: "peephole_addressing",
        file: "peephole_addressing.hll",
        group: "opt",
        // i64[64] on purpose: frames > 2047 bytes use legalized (li+add)
        // addressing that the slot-reload fold does not match -> zero delta.
        base_n: 2000,
        max_steps: 200_000_000,
        variants: &[
            Variant {
                name: "peephole_off",
                knobs: Knobs::noreg_peephole_off(),
            },
            Variant {
                name: "peephole_on",
                knobs: Knobs::noreg_peephole_on(),
            },
        ],
    },
    BenchDef {
        name: "frame_pointer_calls",
        file: "frame_pointer_calls.hll",
        group: "opt",
        base_n: 200_000,
        max_steps: 200_000_000,
        variants: &[
            Variant {
                name: "fp_kept",
                knobs: Knobs::fp_kept(),
            },
            Variant {
                name: "fp_omitted",
                knobs: Knobs::DEFAULT,
            },
        ],
    },
    BenchDef {
        name: "load_use_chain",
        file: "load_use_chain.hll",
        group: "pipeline",
        base_n: 500_000,
        max_steps: 200_000_000,
        variants: DEFAULT_ONLY,
    },
    BenchDef {
        name: "load_use_spaced",
        file: "load_use_spaced.hll",
        group: "pipeline",
        base_n: 500_000,
        max_steps: 200_000_000,
        variants: DEFAULT_ONLY,
    },
    BenchDef {
        name: "branch_predictable",
        file: "branch_predictable.hll",
        group: "pipeline",
        base_n: 500_000,
        max_steps: 200_000_000,
        variants: DEFAULT_ONLY,
    },
    BenchDef {
        name: "branch_random",
        file: "branch_random.hll",
        group: "pipeline",
        base_n: 500_000,
        max_steps: 200_000_000,
        variants: DEFAULT_ONLY,
    },
    BenchDef {
        name: "mem_sequential",
        file: "mem_sequential.hll",
        group: "memory",
        base_n: 30,
        max_steps: 400_000_000,
        variants: DEFAULT_ONLY,
    },
    BenchDef {
        name: "mem_strided",
        file: "mem_strided.hll",
        group: "memory",
        base_n: 240,
        max_steps: 400_000_000,
        variants: DEFAULT_ONLY,
    },
    BenchDef {
        name: "mem_random",
        file: "mem_random.hll",
        group: "memory",
        base_n: 245_760,
        max_steps: 400_000_000,
        variants: DEFAULT_ONLY,
    },
    BenchDef {
        name: "sieve",
        file: "sieve.hll",
        group: "kernel",
        base_n: 10,
        max_steps: 400_000_000,
        variants: DEFAULT_ONLY,
    },
    BenchDef {
        name: "matmul",
        file: "matmul.hll",
        group: "kernel",
        base_n: 5,
        // regalloc_off spills every temp AND pays large-offset materialization
        // on its 24 KiB frame; generous step budget for that variant.
        max_steps: 2_000_000_000,
        variants: &[
            Variant {
                name: "default",
                knobs: Knobs::DEFAULT,
            },
            Variant {
                name: "opt_on",
                knobs: Knobs::ir_opt_on(),
            },
            Variant {
                name: "regalloc_off",
                knobs: Knobs::regalloc_off(),
            },
            Variant {
                name: "fp_kept",
                knobs: Knobs::fp_kept(),
            },
        ],
    },
    BenchDef {
        name: "fib_recursive",
        file: "fib_recursive.hll",
        group: "kernel",
        base_n: 1,
        max_steps: 400_000_000,
        variants: DEFAULT_ONLY,
    },
    BenchDef {
        name: "strsearch",
        file: "strsearch.hll",
        group: "kernel",
        base_n: 60,
        max_steps: 200_000_000,
        variants: DEFAULT_ONLY,
    },
];

pub fn find(name: &str) -> Option<&'static BenchDef> {
    BENCHES.iter().find(|b| b.name == name)
}

/// Absolute path of a bench source, anchored to this crate's manifest dir.
pub fn source_path(file: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../suite")
        .join(file)
}

/// Substitute the `{{N}}` trip-count token. Array capacities stay literal.
pub fn substitute_n(src: &str, n: u64) -> String {
    src.replace("{{N}}", &n.to_string())
}
