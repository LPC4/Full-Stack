// RISC-V assembly (RV64IMAFD) highlighting with proper mnemonics, registers, and directives

use crate::view::ui_theme;
use egui::text::LayoutJob;

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
            let c = line[start..]
                .chars()
                .next()
                .expect("start is within the non-empty source line");
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
