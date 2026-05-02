// Syntax highlighting for different language representations

use egui::Color32;
use egui::text::LayoutJob;

/// Highlights source code with basic syntax highlighting for the HLL.
pub fn highlight_code(theme: &egui::Style, code: &str) -> LayoutJob {
    let mut job = LayoutJob::default();
    let font_id = egui::TextStyle::Monospace.resolve(theme);

    let keywords = [
        "type", "const", "if", "else", "while", "return", "defer", "new", "free", "and", "or",
        "true", "false", "null", "main", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64",
        "f32", "f64", "bool",
    ];

    for segment in code.split_inclusive('\n') {
        let (line, has_newline) = if let Some(without_newline) = segment.strip_suffix('\n') {
            (without_newline, true)
        } else {
            (segment, false)
        };

        if line.trim_start().starts_with(';') {
            job.append(
                line,
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: Color32::from_rgb(100, 150, 100),
                    ..Default::default()
                },
            );
            if has_newline {
                job.append(
                    "\n",
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        ..Default::default()
                    },
                );
            }
            continue;
        }

        let mut start = 0;
        let bytes = line.as_bytes();
        let len = bytes.len();

        while start < len {
            let mut end = start;
            if bytes[start].is_ascii_alphabetic() || bytes[start] == b'_' {
                while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                    end += 1;
                }
                let word = &line[start..end];
                let color = if keywords.contains(&word) {
                    Color32::from_rgb(200, 100, 200) // matched keywords (purple)
                } else {
                    theme.visuals.text_color() // generic
                };

                job.append(
                    word,
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color,
                        ..Default::default()
                    },
                );
            } else if bytes[start].is_ascii_digit() {
                while end < len && bytes[end].is_ascii_digit() {
                    end += 1;
                }
                job.append(
                    &line[start..end],
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color: Color32::from_rgb(100, 150, 200), // numbers (blue)
                        ..Default::default()
                    },
                );
            } else {
                while end < len && !bytes[end].is_ascii_alphanumeric() && bytes[end] != b'_' {
                    end += 1;
                }
                job.append(
                    &line[start..end],
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color: theme.visuals.text_color(),
                        ..Default::default()
                    },
                );
            }
            start = end;
        }
        if has_newline {
            job.append(
                "\n",
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    ..Default::default()
                },
            );
        }
    }
    job
}

/// Highlights AST and token output.
pub fn highlight_ast(theme: &egui::Style, code: &str) -> LayoutJob {
    let mut job = LayoutJob::default();
    let font_id = egui::TextStyle::Monospace.resolve(theme);

    let mut start = 0;
    let bytes = code.as_bytes();
    let len = bytes.len();

    while start < len {
        let mut end = start;
        let c = bytes[start];

        if c.is_ascii_alphabetic() || c == b'_' {
            // Identifiers/Types/Enums
            while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            job.append(
                &code[start..end],
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: Color32::from_rgb(180, 220, 180), // pale green
                    ..Default::default()
                },
            );
        } else if c.is_ascii_digit() {
            // Numbers
            while end < len && bytes[end].is_ascii_digit() {
                end += 1;
            }
            job.append(
                &code[start..end],
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: Color32::from_rgb(100, 150, 200), // blue
                    ..Default::default()
                },
            );
        } else if c == b'{' || c == b'}' || c == b'[' || c == b']' || c == b'(' || c == b')' {
            // Brackets
            end += 1;
            job.append(
                &code[start..end],
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: Color32::from_rgb(220, 200, 100), // yellow
                    ..Default::default()
                },
            );
        } else if c == b'"' || c == b'\'' {
            // Strings
            end += 1;
            while end < len && bytes[end] != c {
                if bytes[end] == b'\\' && end + 1 < len {
                    end += 2;
                } else {
                    end += 1;
                }
            }
            if end < len {
                end += 1;
            }
            job.append(
                &code[start..end],
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: Color32::from_rgb(200, 150, 100), // orange
                    ..Default::default()
                },
            );
        } else {
            // Symbols/Spaces
            while end < len
                && !bytes[end].is_ascii_alphanumeric()
                && bytes[end] != b'_'
                && bytes[end] != b'{'
                && bytes[end] != b'}'
                && bytes[end] != b'['
                && bytes[end] != b']'
                && bytes[end] != b'('
                && bytes[end] != b')'
                && bytes[end] != b'"'
                && bytes[end] != b'\''
            {
                end += 1;
            }
            job.append(
                &code[start..end],
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: theme.visuals.text_color(),
                    ..Default::default()
                },
            );
        }
        start = end;
    }
    job
}

/// Highlights IR (Intermediate Representation) output with IR-specific keywords.
pub fn highlight_ir(theme: &egui::Style, code: &str) -> LayoutJob {
    let mut job = LayoutJob::default();
    let font_id = egui::TextStyle::Monospace.resolve(theme);

    let ir_keywords = [
        "type",
        "define",
        "entry",
        "branch",
        "jump",
        "ret",
        "call",
        "read",
        "write",
        "index",
        "stack_alloc",
        "heap_alloc",
        "heap_free",
        "offset",
        "math",
        "cmp",
        "cast",
        "unary",
    ];

    for segment in code.split_inclusive('\n') {
        let (line, has_newline) = if let Some(without_newline) = segment.strip_suffix('\n') {
            (without_newline, true)
        } else {
            (segment, false)
        };

        // Comments
        if line.trim_start().starts_with(';') {
            job.append(
                line,
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: Color32::from_rgb(100, 150, 100),
                    ..Default::default()
                },
            );
            if has_newline {
                job.append(
                    "\n",
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        ..Default::default()
                    },
                );
            }
            continue;
        }

        let mut start = 0;
        let bytes = line.as_bytes();
        let len = bytes.len();

        while start < len {
            let mut end = start;
            if bytes[start].is_ascii_alphabetic() || bytes[start] == b'_' || bytes[start] == b'@' {
                if bytes[start] == b'@' {
                    end += 1;
                    while end < len
                        && (bytes[end].is_ascii_alphanumeric()
                            || bytes[end] == b'_'
                            || bytes[end] == b'.'
                            || bytes[end] == b'<'
                            || bytes[end] == b'>')
                    {
                        end += 1;
                    }
                    // Labels/functions
                    job.append(
                        &line[start..end],
                        0.0,
                        egui::TextFormat {
                            font_id: font_id.clone(),
                            color: Color32::from_rgb(150, 200, 255), // light blue
                            ..Default::default()
                        },
                    );
                } else {
                    while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                        end += 1;
                    }
                    let word = &line[start..end];
                    let color = if ir_keywords.contains(&word) {
                        Color32::from_rgb(200, 100, 200) // keywords (purple)
                    } else {
                        theme.visuals.text_color()
                    };

                    job.append(
                        word,
                        0.0,
                        egui::TextFormat {
                            font_id: font_id.clone(),
                            color,
                            ..Default::default()
                        },
                    );
                }
            } else if bytes[start].is_ascii_digit() {
                while end < len && bytes[end].is_ascii_digit() {
                    end += 1;
                }
                job.append(
                    &line[start..end],
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color: Color32::from_rgb(100, 200, 100), // green
                        ..Default::default()
                    },
                );
            } else if bytes[start] == b'$' {
                // Registers
                end += 1;
                while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                    end += 1;
                }
                job.append(
                    &line[start..end],
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color: Color32::from_rgb(255, 200, 100), // orange
                        ..Default::default()
                    },
                );
            } else {
                while end < len
                    && !bytes[end].is_ascii_alphanumeric()
                    && bytes[end] != b'_'
                    && bytes[end] != b'$'
                    && bytes[end] != b'@'
                {
                    end += 1;
                }
                job.append(
                    &line[start..end],
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color: theme.visuals.text_color(),
                        ..Default::default()
                    },
                );
            }
            start = end;
        }
        if has_newline {
            job.append(
                "\n",
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    ..Default::default()
                },
            );
        }
    }
    job
}

/// Highlights RISC‑V assembly (RV64IMAFD) with proper mnemonics, registers, and directives.
pub fn highlight_assembly(theme: &egui::Style, code: &str) -> LayoutJob {
    let mut job = LayoutJob::default();
    let font_id = egui::TextStyle::Monospace.resolve(theme);

    // RISC‑V integer / FP instruction mnemonics (full set for RV64IMAFD)
    let instructions: &[&str] = &[
        "lb",
        "lh",
        "lw",
        "ld",
        "lbu",
        "lhu",
        "lwu",
        "sb",
        "sh",
        "sw",
        "sd",
        "addi",
        "slti",
        "sltiu",
        "xori",
        "ori",
        "andi",
        "slli",
        "srli",
        "srai",
        "addiw",
        "slliw",
        "srliw",
        "sraiw",
        "add",
        "sub",
        "sll",
        "slt",
        "sltu",
        "xor",
        "srl",
        "sra",
        "or",
        "and",
        "addw",
        "subw",
        "sllw",
        "srlw",
        "sraw",
        "lui",
        "auipc",
        "jal",
        "jalr",
        "beq",
        "bne",
        "blt",
        "bge",
        "bltu",
        "bgeu",
        "ecall",
        "ebreak",
        "fence",
        "fence.i",
        "mul",
        "mulh",
        "mulhsu",
        "mulhu",
        "div",
        "divu",
        "rem",
        "remu",
        "mulw",
        "divw",
        "divuw",
        "remw",
        "remuw",
        // A‑extension
        "lr.w",
        "sc.w",
        "amoadd.w",
        "amoswap.w",
        "amoxor.w",
        "amoand.w",
        "amoor.w",
        "amomin.w",
        "amomax.w",
        "amominu.w",
        "amomaxu.w",
        "lr.d",
        "sc.d",
        "amoadd.d",
        "amoswap.d",
        "amoxor.d",
        "amoand.d",
        "amoor.d",
        "amomin.d",
        "amomax.d",
        "amominu.d",
        "amomaxu.d",
        // F/D‑extension
        "flw",
        "fld",
        "fsw",
        "fsd",
        "fadd.s",
        "fsub.s",
        "fmul.s",
        "fdiv.s",
        "fsqrt.s",
        "fsgnj.s",
        "fsgnjn.s",
        "fsgnjx.s",
        "fmin.s",
        "fmax.s",
        "fadd.d",
        "fsub.d",
        "fmul.d",
        "fdiv.d",
        "fsqrt.d",
        "fsgnj.d",
        "fsgnjn.d",
        "fsgnjx.d",
        "fmin.d",
        "fmax.d",
        "feq.s",
        "flt.s",
        "fle.s",
        "feq.d",
        "flt.d",
        "fle.d",
        "fclass.s",
        "fclass.d",
        "fmv.x.w",
        "fmv.w.x",
        "fmv.x.d",
        "fmv.d.x",
        "fcvt.w.s",
        "fcvt.wu.s",
        "fcvt.l.s",
        "fcvt.lu.s",
        "fcvt.w.d",
        "fcvt.wu.d",
        "fcvt.l.d",
        "fcvt.lu.d",
        "fcvt.s.w",
        "fcvt.s.wu",
        "fcvt.s.l",
        "fcvt.s.lu",
        "fcvt.d.w",
        "fcvt.d.wu",
        "fcvt.d.l",
        "fcvt.d.lu",
        "fcvt.s.d",
        "fcvt.d.s",
        "fmadd.s",
        "fmsub.s",
        "fnmsub.s",
        "fnmadd.s",
        "fmadd.d",
        "fmsub.d",
        "fnmsub.d",
        "fnmadd.d",
        // pseudo‑instructions (treated as instructions for highlighting)
        "nop",
        "li",
        "la",
        "mv",
        "not",
        "neg",
        "negw",
        "sext.w",
        "seqz",
        "snez",
        "sltz",
        "sgtz",
        "beqz",
        "bnez",
        "blez",
        "bgez",
        "bltz",
        "bgtz",
        "bgt",
        "ble",
        "bgtu",
        "bleu",
        "j",
        "jr",
        "ret",
        "call",
        "tail",
        "fmv.s",
        "fmv.d",
        "fneg.s",
        "fneg.d",
        "fabs.s",
        "fabs.d",
    ];

    // ABI register names (integer)
    let int_regs: &[&str] = &[
        "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "t3", "t4", "t5", "t6", "s0", "s1", "s2",
        "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "a0", "a1", "a2", "a3", "a4", "a5",
        "a6", "a7",
    ];
    // Floating‑point ABI names
    let fp_regs: &[&str] = &[
        "ft0", "ft1", "ft2", "ft3", "ft4", "ft5", "ft6", "ft7", "fs0", "fs1", "fs2", "fs3", "fs4",
        "fs5", "fs6", "fs7", "fs8", "fs9", "fs10", "fs11", "fa0", "fa1", "fa2", "fa3", "fa4",
        "fa5", "fa6", "fa7",
    ];
    // Common assembler directives
    let directives: &[&str] = &[
        ".text", ".data", ".bss", ".rodata", ".globl", ".local", ".weak", ".align", ".balign",
        ".p2align", ".byte", ".half", ".word", ".dword", ".float", ".double", ".asciz", ".ascii",
        ".string", ".skip", ".space", ".zero", ".equ", ".set", ".type", ".size", ".section",
    ];

    for segment in code.split_inclusive('\n') {
        let (line, has_newline) = if let Some(stripped) = segment.strip_suffix('\n') {
            (stripped, true)
        } else {
            (segment, false)
        };

        // Whole‑line comment (; …)
        if line.trim_start().starts_with(';') {
            job.append(
                line,
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: Color32::from_rgb(100, 150, 100), // comment green
                    ..Default::default()
                },
            );
            if has_newline {
                job.append(
                    "\n",
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        ..Default::default()
                    },
                );
            }
            continue;
        }

        let mut start = 0;
        let bytes = line.as_bytes();
        let len = bytes.len();

        while start < len {
            let mut end = start;

            // Skip leading whitespace
            if bytes[start].is_ascii_whitespace() {
                while end < len && bytes[end].is_ascii_whitespace() {
                    end += 1;
                }
                job.append(
                    &line[start..end],
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        ..Default::default()
                    },
                );
                start = end;
                continue;
            }

            // Labels (identifier followed by ':')
            if bytes[start].is_ascii_alphabetic() || bytes[start] == b'_' || bytes[start] == b'.' {
                while end < len
                    && (bytes[end].is_ascii_alphanumeric()
                        || bytes[end] == b'_'
                        || bytes[end] == b'.')
                {
                    end += 1;
                }
                let word = &line[start..end];

                // Check if it's an instruction, register, directive, or a label
                if end < len && bytes[end] == b':' {
                    // It's a label definition (e.g. "label:")
                    job.append(
                        word,
                        0.0,
                        egui::TextFormat {
                            font_id: font_id.clone(),
                            color: Color32::from_rgb(255, 200, 100), // label orange
                            ..Default::default()
                        },
                    );
                    // append the ':'
                    job.append(
                        ":",
                        0.0,
                        egui::TextFormat {
                            font_id: font_id.clone(),
                            color: theme.visuals.text_color(),
                            ..Default::default()
                        },
                    );
                    start = end + 1;
                } else if instructions.contains(&word) {
                    // Instruction mnemonic
                    job.append(
                        word,
                        0.0,
                        egui::TextFormat {
                            font_id: font_id.clone(),
                            color: Color32::from_rgb(200, 100, 200), // purple
                            ..Default::default()
                        },
                    );
                    start = end;
                } else if int_regs.contains(&word)
                    || fp_regs.contains(&word)
                    || word.starts_with('x')
                    || word.starts_with('f')
                {
                    // Register name (numeric or ABI)
                    job.append(
                        word,
                        0.0,
                        egui::TextFormat {
                            font_id: font_id.clone(),
                            color: Color32::from_rgb(100, 200, 255), // light blue
                            ..Default::default()
                        },
                    );
                    start = end;
                } else if directives.contains(&word) {
                    // Assembler directive (e.g. .text)
                    job.append(
                        word,
                        0.0,
                        egui::TextFormat {
                            font_id: font_id.clone(),
                            color: Color32::from_rgb(255, 150, 100), // orange directive
                            ..Default::default()
                        },
                    );
                    start = end;
                } else {
                    // Other identifier (maybe function name or symbol)
                    job.append(
                        word,
                        0.0,
                        egui::TextFormat {
                            font_id: font_id.clone(),
                            ..Default::default()
                        },
                    );
                    start = end;
                }
                continue;
            }

            // Numbers (decimal, hex 0x..., binary 0b...)
            if bytes[start].is_ascii_digit()
                || (bytes[start] == b'0'
                    && start + 1 < len
                    && (bytes[start + 1] == b'x' || bytes[start + 1] == b'b'))
            {
                while end < len && (bytes[end].is_ascii_alphanumeric()) {
                    end += 1;
                }
                job.append(
                    &line[start..end],
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color: Color32::from_rgb(100, 200, 100), // green numbers
                        ..Default::default()
                    },
                );
                start = end;
                continue;
            }

            // Characters like ',' '(' ')' etc.
            // Just consume one char
            let ch = line[start..start + 1].to_owned();
            job.append(
                &ch,
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    ..Default::default()
                },
            );
            start += 1;
        }

        if has_newline {
            job.append(
                "\n",
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    ..Default::default()
                },
            );
        }
    }
    job
}
