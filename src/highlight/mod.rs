// egui adapter over the shared syntax highlighters.

pub mod theme;

use egui::text::LayoutJob;
use fs_utils::syntax_highlight::{self, Class, Lang};

pub use theme::SyntaxPalette;

/// Highlights RISC-V assembly (RV64IMAFD) with proper mnemonics, registers, and directives.
pub fn highlight_assembly(theme: &egui::Style, code: &str) -> LayoutJob {
    layout(theme, code, Lang::Asm)
}

/// Highlights source code with basic syntax highlighting for the HLL.
pub fn highlight_code(theme: &egui::Style, code: &str) -> LayoutJob {
    layout(theme, code, Lang::Hll)
}

/// Highlights IR (Intermediate Representation) output with IR-specific keywords.
pub fn highlight_ir(theme: &egui::Style, code: &str) -> LayoutJob {
    layout(theme, code, Lang::Ir)
}

/// Highlights AST and token output.
pub fn highlight_ast(theme: &egui::Style, code: &str) -> LayoutJob {
    layout(theme, code, Lang::Ast)
}

fn layout(theme: &egui::Style, code: &str, lang: Lang) -> LayoutJob {
    let palette = SyntaxPalette::dark();
    let font_id = egui::TextStyle::Monospace.resolve(theme);
    let mut job = LayoutJob::default();

    for span in syntax_highlight::highlight(lang, code) {
        let mut format = egui::TextFormat {
            font_id: font_id.clone(),
            ..Default::default()
        };
        if let Some(color) = color_for(span.class, &palette, theme) {
            format.color = color;
        }
        job.append(&code[span.start..span.end], 0.0, format);
    }
    job
}

// `Plain` keeps egui's own default color; every other class is explicit.
fn color_for(class: Class, palette: &SyntaxPalette, theme: &egui::Style) -> Option<egui::Color32> {
    match class {
        Class::Plain => None,
        Class::Text => Some(theme.visuals.text_color()),
        Class::Keyword => Some(palette.keyword),
        Class::String => Some(palette.string),
        Class::Number => Some(palette.number),
        Class::Comment => Some(palette.comment),
        Class::Label => Some(palette.label),
        Class::Register => Some(palette.register),
        Class::Bracket => Some(palette.bracket),
        Class::Directive => Some(palette.directive),
        Class::Identifier => Some(palette.identifier),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Guards the keyword list against going stale as the language grows.
    #[test]
    fn hll2_era_keywords_are_highlighted() {
        let style = egui::Style::default();
        let palette = SyntaxPalette::dark();
        let hll2 = [
            "interface",
            "dyn",
            "impl",
            "struct",
            "enum",
            "match",
            "for",
            "in",
            "fn",
            "as",
            "import",
            "export",
            "break",
            "continue",
            "not",
            "Self",
        ];
        for keyword in hll2 {
            let job = highlight_code(&style, keyword);
            assert!(
                job.sections
                    .iter()
                    .any(|s| s.format.color == palette.keyword),
                "keyword `{keyword}` not highlighted"
            );
        }
    }

    #[test]
    fn plain_identifiers_are_not_highlighted() {
        let style = egui::Style::default();
        let palette = SyntaxPalette::dark();
        for word in ["interfaces", "formatter", "matcher", "implement"] {
            let job = highlight_code(&style, word);
            assert!(
                job.sections
                    .iter()
                    .all(|s| s.format.color != palette.keyword),
                "identifier `{word}` wrongly highlighted as a keyword"
            );
        }
    }

    fn word_color(job: &LayoutJob, word: &str) -> Option<egui::Color32> {
        job.sections.iter().find_map(|section| {
            (&job.text[section.byte_range.clone()] == word).then_some(section.format.color)
        })
    }

    #[test]
    fn declared_and_generic_types_use_the_identifier_color() {
        let style = egui::Style::default();
        let palette = SyntaxPalette::dark();
        let source = r#"
interface Measured { measure: (self: Self*) -> i32 }
struct Box<T> { value: T }
read: <U: Measured>(value: Box<U>*, erased: dyn Measured) -> U { return value.value }
const OP_ADD = 1
"#;
        let job = highlight_code(&style, source);

        for ty in ["Measured", "Box", "T", "U"] {
            assert_eq!(
                word_color(&job, ty),
                Some(palette.identifier),
                "type `{ty}`"
            );
        }
        assert_ne!(word_color(&job, "OP_ADD"), Some(palette.identifier));
    }

    // The job must reproduce the input exactly, or the editor would show altered text.
    #[test]
    fn layout_preserves_source_text() {
        let style = egui::Style::default();
        let cases = [
            (Lang::Asm, "_start:\n  li t0, 5 # go\n  ret\n"),
            (
                Lang::Hll,
                "main: () -> i32 {\n  asm { li a0, 1 }\n} ; done\n",
            ),
            (Lang::Ir, "define @main() {\nentry:\n  $1 = const 5\n}\n"),
            (Lang::Ast, "Fn { name: \"main\", args: [1, 2] }"),
        ];
        for (lang, src) in cases {
            assert_eq!(layout(&style, src, lang).text, src, "{lang:?}");
        }
    }
}
