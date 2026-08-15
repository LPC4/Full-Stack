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
        Class::BuiltinType => Some(palette.builtin_type),
        Class::Type => Some(palette.ty),
        Class::Generic => Some(palette.generic),
        Class::Variant => Some(palette.variant),
        Class::Function => Some(palette.function),
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
        let identifier = word
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
        let start = job.text.match_indices(word).find_map(|(start, _)| {
            let bytes = job.text.as_bytes();
            let end = start + word.len();
            let left_boundary =
                start == 0 || !bytes[start - 1].is_ascii_alphanumeric() && bytes[start - 1] != b'_';
            let right_boundary =
                end == bytes.len() || !bytes[end].is_ascii_alphanumeric() && bytes[end] != b'_';
            (!identifier || left_boundary && right_boundary).then_some(start)
        })?;
        let end = start + word.len();
        job.sections
            .iter()
            .find(|section| section.byte_range.start.0 <= start && section.byte_range.end.0 >= end)
            .map(|section| section.format.color)
    }

    #[test]
    fn hll_semantic_roles_use_distinct_colors() {
        let style = egui::Style::default();
        let palette = SyntaxPalette::dark();
        let source = r#"
; semantic palette
interface Measured { measure: (self: Self*) -> i32 }
struct Box<T> { value: T }
enum Shape { Unit, Circle(i32) }
read: <U: Measured>(value: Box<U>*, erased: dyn Measured) -> U { return value.value }
main: () -> i32 {
    result: Box<i32> = factory.make<T>(value)
    shape: Shape = Shape.Circle(7)
    text := "sample"
    return result.value
}
"#;
        let job = highlight_code(&style, source);

        assert_eq!(word_color(&job, "enum"), Some(palette.keyword));
        assert_eq!(word_color(&job, "\"sample\""), Some(palette.string));
        assert_eq!(word_color(&job, "7"), Some(palette.number));
        assert_eq!(word_color(&job, "semantic palette"), Some(palette.comment));
        for ty in ["Measured", "Box", "Shape"] {
            assert_eq!(word_color(&job, ty), Some(palette.ty), "type `{ty}`");
        }
        for generic in ["Self", "T", "U"] {
            assert_eq!(
                word_color(&job, generic),
                Some(palette.generic),
                "generic `{generic}`"
            );
        }
        assert_eq!(word_color(&job, "i32"), Some(palette.builtin_type));
        assert_eq!(word_color(&job, "Circle"), Some(palette.variant));
        assert_eq!(word_color(&job, "main"), Some(palette.function));
        assert_eq!(word_color(&job, "make"), Some(palette.function));
        for variable in ["result", "factory", "value", "shape", "text"] {
            assert_eq!(
                word_color(&job, variable),
                Some(style.visuals.text_color()),
                "variable `{variable}`"
            );
        }
        assert_ne!(palette.ty, palette.builtin_type);
        assert_ne!(palette.generic, palette.ty);
        assert_ne!(palette.variant, palette.function);
        assert_ne!(palette.variant, palette.builtin_type);
        assert_ne!(palette.function, palette.generic);
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
