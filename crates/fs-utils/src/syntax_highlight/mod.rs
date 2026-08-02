//! Syntax highlighting for the languages this project reads, as plain span data.
//! No UI dependency: the IDE, the Guide, and the sandbox share one tokenizer.

mod asm;
mod ast;
mod hll;
pub mod html;
mod ir;
mod ld;
mod sh;
mod x86;

/// A language the highlighters understand.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    /// RISC-V assembly (`.s`).
    Asm,
    /// The high-level language (`.hll`).
    Hll,
    /// Compiler IR dumps (`.ir`).
    Ir,
    /// AST and token dumps.
    Ast,
    /// Shell command lines, as the Guide shows build commands in.
    Sh,
    /// GNU linker scripts (`.ld`).
    Ld,
    /// x86-64 in Intel syntax, which the Guide shows only to contrast with RISC-V.
    X86,
}

/// What a span of source text is, independent of any palette.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    /// Text the scanner leaves unstyled; renders in the renderer's default color.
    Plain,
    /// Text the scanner explicitly assigns the theme's body text color.
    Text,
    /// Language keyword, or an assembly mnemonic.
    Keyword,
    /// String or character literal.
    String,
    /// Numeric literal.
    Number,
    /// Comment, to end of line.
    Comment,
    /// Label definition or an IR block/function name.
    Label,
    /// Machine register.
    Register,
    /// Bracket the scanner calls out, such as an inline `asm` block's braces.
    Bracket,
    /// Assembler directive, such as `.text`.
    Directive,
    /// A declared type name, or an identifier in an AST dump.
    Identifier,
}

/// A classified, half-open byte range of the source that was highlighted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Span {
    /// Byte offset of the first byte, from the start of the source.
    pub start: usize,
    /// Byte offset one past the last byte.
    pub end: usize,
    /// What the range is.
    pub class: Class,
}

/// Classify `src` as `lang`. Spans are ordered, non-overlapping, and cover
/// every byte, so concatenating their text reproduces the input.
pub fn highlight(lang: Lang, src: &str) -> Vec<Span> {
    let mut out = Out::default();
    match lang {
        Lang::Asm => asm::highlight(src, &mut out),
        Lang::Hll => hll::highlight(src, &mut out),
        Lang::Ir => ir::highlight(src, &mut out),
        Lang::Ast => ast::highlight(src, &mut out),
        Lang::Sh => sh::highlight(src, &mut out),
        Lang::Ld => ld::highlight(src, &mut out),
        Lang::X86 => x86::highlight(src, &mut out),
    }
    out.spans
}

// Scanners push through this so empty ranges never reach callers.
#[derive(Default)]
pub(crate) struct Out {
    spans: Vec<Span>,
}

impl Out {
    pub(crate) fn push(&mut self, start: usize, end: usize, class: Class) {
        if start < end {
            self.spans.push(Span { start, end, class });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LANGS: [Lang; 7] = [
        Lang::Asm,
        Lang::Hll,
        Lang::Ir,
        Lang::Ast,
        Lang::Sh,
        Lang::Ld,
        Lang::X86,
    ];

    // The tiling guarantee is what lets renderers emit spans without tracking gaps.
    fn assert_tiles(lang: Lang, src: &str) {
        let spans = highlight(lang, src);
        let mut at = 0;
        for span in &spans {
            assert_eq!(span.start, at, "{lang:?} gap or overlap in {src:?}");
            assert!(span.end > span.start, "{lang:?} empty span in {src:?}");
            assert!(src.is_char_boundary(span.start), "{lang:?} split char");
            assert!(src.is_char_boundary(span.end), "{lang:?} split char");
            at = span.end;
        }
        assert_eq!(at, src.len(), "{lang:?} did not cover {src:?}");
    }

    #[test]
    fn spans_tile_every_language() {
        let sources = [
            "",
            "\n",
            "\n\n\n",
            "   ",
            "li t0, 5 # set it\n_start:\n  ret\n",
            "main: () -> i32 { return 7 }\n; trailing comment",
            "define main() -> i32 {\nentry:\n    $1 = add i32 2, 3\n}\n",
            "Fn { name: \"main\", args: [1, 2] }",
            "\"unterminated",
            "0x1f 0b1010 0o17 12_345",
            "gcc -T link.ld -o a.elf \\\n    b.s # build\n",
            "/* unterminated\nSECTIONS { . = 0x80000000; *(.text*) }\n",
        ];
        for lang in LANGS {
            for src in sources {
                assert_tiles(lang, src);
            }
        }
    }

    // Multi-byte input used to be able to split a char and panic on slicing.
    #[test]
    fn multibyte_text_never_splits_a_char() {
        let sources = [
            "li t0, 5 # em-dash \u{2014} here\n",
            "\"caf\u{e9}\" \u{4e16}\u{754c}\n",
            "; \u{2192} arrow\n\u{1f600}\n",
        ];
        for lang in LANGS {
            for src in sources {
                assert_tiles(lang, src);
            }
        }
    }
}
