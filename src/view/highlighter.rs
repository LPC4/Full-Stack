// Syntax highlighting for different language representations

use crate::view::ui_theme;
use egui::text::LayoutJob;

const HLL_KEYWORDS: &[&str] = &[
    "type", "const", "if", "else", "while", "return", "defer", "new", "free", "and", "or", "true",
    "false", "null", "main", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64",
    "bool", "asm",
];

const ABI_INT_REGS: &[&str] = &[
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "t3", "t4", "t5", "t6", "s0", "s1", "s2",
    "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "a0", "a1", "a2", "a3", "a4", "a5",
    "a6", "a7",
];

const ABI_FP_REGS: &[&str] = &[
    "ft0", "ft1", "ft2", "ft3", "ft4", "ft5", "ft6", "ft7", "fs0", "fs1", "fs2", "fs3", "fs4",
    "fs5", "fs6", "fs7", "fs8", "fs9", "fs10", "fs11", "fa0", "fa1", "fa2", "fa3", "fa4", "fa5",
    "fa6", "fa7",
];

fn find_comment_start_outside_string(line: &str) -> Option<usize> {
    let mut comment_start = None;
    let mut in_string = false;
    let mut string_quote = b'\0';
    let mut escape = false;

    for (i, &b) in line.as_bytes().iter().enumerate() {
        if escape {
            escape = false;
            continue;
        }
        if in_string {
            if b == b'\\' {
                escape = true;
            } else if b == string_quote {
                in_string = false;
            }
        } else if b == b';' {
            comment_start = Some(i);
            break;
        } else if b == b'"' || b == b'\'' {
            in_string = true;
            string_quote = b;
        }
    }

    comment_start
}

fn append_hll_code_part(job: &mut LayoutJob, theme: &egui::Style, code_part: &str) {
    let palette = ui_theme().syntax;
    let font_id = egui::TextStyle::Monospace.resolve(theme);
    let mut start = 0;
    let bytes = code_part.as_bytes();
    let len = bytes.len();

    while start < len {
        let mut end = start;

        // string literal
        if bytes[start] == b'"' || bytes[start] == b'\'' {
            let quote = bytes[start];
            end = start + 1;
            while end < len {
                if bytes[end] == b'\\' && end + 1 < len {
                    end += 2; // skip escaped char
                } else if bytes[end] == quote {
                    end += 1;
                    break;
                } else {
                    end += 1;
                }
            }
            job.append(
                &code_part[start..end],
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: palette.string,
                    ..Default::default()
                },
            );
            start = end;
            continue;
        }

        // identifier / keyword
        if bytes[start].is_ascii_alphabetic() || bytes[start] == b'_' {
            while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            let word = &code_part[start..end];
            let color = if HLL_KEYWORDS.contains(&word) {
                palette.keyword
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
            start = end;
            continue;
        }

        // number
        if bytes[start].is_ascii_digit() {
            while end < len && bytes[end].is_ascii_digit() {
                end += 1;
            }
            job.append(
                &code_part[start..end],
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: palette.number,
                    ..Default::default()
                },
            );
            start = end;
            continue;
        }

        // other symbols (operators, whitespace, etc.)
        while end < len
            && !bytes[end].is_ascii_alphanumeric()
            && bytes[end] != b'_'
            && bytes[end] != b'"'
            && bytes[end] != b'\''
        {
            end += 1;
        }
        job.append(
            &code_part[start..end],
            0.0,
            egui::TextFormat {
                font_id: font_id.clone(),
                color: theme.visuals.text_color(),
                ..Default::default()
            },
        );
        start = end;
    }
}

fn is_register_word(word: &str) -> bool {
    ABI_INT_REGS.contains(&word)
        || ABI_FP_REGS.contains(&word)
        || (word.starts_with('x') && word[1..].chars().all(|c| c.is_ascii_digit()))
        || (word.starts_with('f') && word[1..].chars().all(|c| c.is_ascii_digit()))
}

fn append_inline_asm_code_part(job: &mut LayoutJob, theme: &egui::Style, code_part: &str) {
    let palette = ui_theme().syntax;
    let font_id = egui::TextStyle::Monospace.resolve(theme);
    let mut start = 0;
    let bytes = code_part.as_bytes();
    let len = bytes.len();
    let mut seen_mnemonic = false;

    while start < len {
        let mut end = start;
        let b = bytes[start];

        if b.is_ascii_whitespace() {
            while end < len && bytes[end].is_ascii_whitespace() {
                end += 1;
            }
            job.append(
                &code_part[start..end],
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: theme.visuals.text_color(),
                    ..Default::default()
                },
            );
            start = end;
            continue;
        }

        if b.is_ascii_alphabetic() || b == b'_' || b == b'.' {
            while end < len
                && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_' || bytes[end] == b'.')
            {
                end += 1;
            }
            let word = &code_part[start..end];
            let is_label = end < len && bytes[end] == b':';
            let color = if is_label {
                palette.label
            } else if is_register_word(word) {
                palette.register
            } else if word.starts_with('.') {
                palette.directive
            } else if !seen_mnemonic {
                palette.keyword
            } else {
                theme.visuals.text_color()
            };
            if !is_label && !is_register_word(word) && !word.starts_with('.') {
                seen_mnemonic = true;
            }
            job.append(
                word,
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color,
                    ..Default::default()
                },
            );
            start = end;
            continue;
        }

        if b.is_ascii_digit() || (b == b'-' && start + 1 < len && bytes[start + 1].is_ascii_digit())
        {
            end += 1;
            while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'x') {
                end += 1;
            }
            job.append(
                &code_part[start..end],
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: palette.number,
                    ..Default::default()
                },
            );
            start = end;
            continue;
        }

        let color = if b == b'{' || b == b'}' {
            palette.bracket
        } else {
            theme.visuals.text_color()
        };
        job.append(
            &code_part[start..start + 1],
            0.0,
            egui::TextFormat {
                font_id: font_id.clone(),
                color,
                ..Default::default()
            },
        );
        start += 1;
    }
}

fn find_inline_asm_open(code_part: &str) -> Option<(usize, usize)> {
    let bytes = code_part.as_bytes();
    if bytes.len() < 4 {
        return None;
    }

    let mut i = 0;
    let mut in_string = false;
    let mut string_quote = b'\0';
    let mut escape = false;

    while i + 3 <= bytes.len() {
        let b = bytes[i];
        if escape {
            escape = false;
            i += 1;
            continue;
        }
        if in_string {
            if b == b'\\' {
                escape = true;
            } else if b == string_quote {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            in_string = true;
            string_quote = b;
            i += 1;
            continue;
        }

        if i + 3 <= bytes.len() && &bytes[i..i + 3] == b"asm" {
            let left_ok = i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
            let right_ok = i + 3 == bytes.len()
                || !(bytes[i + 3].is_ascii_alphanumeric() || bytes[i + 3] == b'_');
            if left_ok && right_ok {
                let mut j = i + 3;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'{' {
                    return Some((i, j));
                }
            }
        }
        i += 1;
    }

    None
}

/// Highlights source code with basic syntax highlighting for the HLL.
pub fn highlight_code(theme: &egui::Style, code: &str) -> LayoutJob {
    let palette = ui_theme().syntax;
    let mut job = LayoutJob::default();
    let font_id = egui::TextStyle::Monospace.resolve(theme);
    let mut in_inline_asm_block = false;

    for segment in code.split_inclusive('\n') {
        let (line, has_newline) = if let Some(without_newline) = segment.strip_suffix('\n') {
            (without_newline, true)
        } else {
            (segment, false)
        };

        let comment_start = find_comment_start_outside_string(line);

        // split into code part and optional comment part
        let (code_part, comment_part) = if let Some(idx) = comment_start {
            (&line[..idx], Some(&line[idx..]))
        } else {
            (line, None)
        };

        if in_inline_asm_block {
            if let Some(close_idx) = code_part.find('}') {
                let asm_part = &code_part[..close_idx];
                append_inline_asm_code_part(&mut job, theme, asm_part);
                job.append(
                    "}",
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color: palette.bracket,
                        ..Default::default()
                    },
                );
                append_hll_code_part(&mut job, theme, &code_part[close_idx + 1..]);
                in_inline_asm_block = false;
            } else {
                append_inline_asm_code_part(&mut job, theme, code_part);
            }
        } else if let Some((asm_idx, open_idx)) = find_inline_asm_open(code_part) {
            append_hll_code_part(&mut job, theme, &code_part[..asm_idx]);
            job.append(
                "asm",
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: palette.keyword,
                    ..Default::default()
                },
            );
            if open_idx > asm_idx + 3 {
                append_hll_code_part(&mut job, theme, &code_part[asm_idx + 3..open_idx]);
            }
            job.append(
                "{",
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: palette.bracket,
                    ..Default::default()
                },
            );

            let after_open = &code_part[open_idx + 1..];
            if let Some(close_rel) = after_open.find('}') {
                append_inline_asm_code_part(&mut job, theme, &after_open[..close_rel]);
                job.append(
                    "}",
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color: palette.bracket,
                        ..Default::default()
                    },
                );
                append_hll_code_part(&mut job, theme, &after_open[close_rel + 1..]);
            } else {
                append_inline_asm_code_part(&mut job, theme, after_open);
                in_inline_asm_block = true;
            }
        } else {
            append_hll_code_part(&mut job, theme, code_part);
        }

        // ---------- append the comment part (if any) ----------
        if let Some(comment) = comment_part {
            job.append(
                comment,
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: palette.comment,
                    ..Default::default()
                },
            );
        }

        // append newline
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
    let palette = ui_theme().syntax;
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
                    color: palette.identifier,
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
                    color: palette.number,
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
                    color: palette.bracket,
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
                    color: palette.string,
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
    let palette = ui_theme().syntax;
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
                    color: palette.comment,
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
                            color: palette.label,
                            ..Default::default()
                        },
                    );
                } else {
                    while end < len && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                        end += 1;
                    }
                    let word = &line[start..end];
                    let color = if ir_keywords.contains(&word) {
                        palette.keyword
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
                        color: palette.number,
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
                        color: palette.register,
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

/// Highlights RISC-V assembly (RV64IMAFD) with proper mnemonics, registers, and directives.
pub fn highlight_assembly(theme: &egui::Style, code: &str) -> LayoutJob {
    let palette = ui_theme().syntax;
    let mut job = LayoutJob::default();
    let font_id = egui::TextStyle::Monospace.resolve(theme);

    // RISC-V integer / FP instruction mnemonics (full set for RV64IMAFD)
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
        // A-extension
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
        // F/D-extension
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
        // pseudo-instructions (treated as instructions for highlighting)
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
    // Floating-point ABI names
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

        // Whole-line comment (; ...)
        if line.trim_start().starts_with(';') {
            job.append(
                line,
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: palette.comment,
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
                            color: palette.label,
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
                            color: palette.keyword,
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
                            color: palette.register,
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
                            color: palette.directive,
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
                        color: palette.number,
                        ..Default::default()
                    },
                );
                start = end;
                continue;
            }

            // Fallthrough: consume one Unicode code point so multi-byte chars
            // (e.g. em-dash in string literals) never cause a byte-boundary panic.
            let c = line[start..].chars().next().unwrap();
            let char_len = c.len_utf8();
            job.append(
                &line[start..start + char_len],
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    ..Default::default()
                },
            );
            start += char_len;
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
