// HTML rendering of spans, shared by the Guide generator and the WASM sandbox.

use super::{Class, Lang};

/// CSS class for a token, or `None` for text that takes the default color.
pub fn class_name(class: Class) -> Option<&'static str> {
    match class {
        Class::Plain | Class::Text => None,
        Class::Keyword => Some("tk-kw"),
        Class::String => Some("tk-str"),
        Class::Number => Some("tk-num"),
        Class::Comment => Some("tk-cmt"),
        Class::Label => Some("tk-lbl"),
        Class::Register => Some("tk-reg"),
        Class::Bracket => Some("tk-brk"),
        Class::Directive => Some("tk-dir"),
        Class::Identifier => Some("tk-id"),
    }
}

/// Escapes text for use in HTML element content or a quoted attribute.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// The language a fence tag or file extension selects, if any.
pub fn lang_for(tag: &str) -> Option<Lang> {
    match tag {
        "asm" | "s" | "riscv" => Some(Lang::Asm),
        "hll" => Some(Lang::Hll),
        "ir" => Some(Lang::Ir),
        "ast" => Some(Lang::Ast),
        "sh" | "bash" | "shell" | "console" => Some(Lang::Sh),
        "ld" | "linker" => Some(Lang::Ld),
        "x86" | "x86asm" | "nasm" => Some(Lang::X86),
        _ => None,
    }
}

// Classified byte ranges of `source`; one unclassified range for an unknown tag.
fn classify(tag: &str, source: &str) -> Vec<(usize, usize, Option<&'static str>)> {
    match lang_for(tag) {
        Some(lang) => super::highlight(lang, source)
            .into_iter()
            .map(|span| (span.start, span.end, class_name(span.class)))
            .collect(),
        None => vec![(0, source.len(), None)],
    }
}

fn push_piece(out: &mut String, text: &str, class: Option<&str>) {
    match class {
        Some(class) => {
            out.push_str("<span class=\"");
            out.push_str(class);
            out.push_str("\">");
            out.push_str(&escape(text));
            out.push_str("</span>");
        }
        None => out.push_str(&escape(text)),
    }
}

/// Highlighted inner HTML for `source`, or plain escaped text for unknown tags.
pub fn highlight_html(tag: &str, source: &str) -> String {
    let mut out = String::with_capacity(source.len() * 2);
    for (start, end, class) in classify(tag, source) {
        push_piece(&mut out, &source[start..end], class);
    }
    out
}

/// The same, with every source line in its own block element, which is what
/// lets a wrapping listing hang-indent the folded part and nothing else.
pub fn highlight_lines_html(tag: &str, source: &str) -> String {
    let body = source.strip_suffix('\n').unwrap_or(source);
    let mut out = String::with_capacity(body.len() * 2);

    out.push_str("<span class=\"ln\">");
    for (start, end, class) in classify(tag, body) {
        let mut rest = &body[start..end];
        // One span can cover several lines, so break it at every newline.
        while let Some(at) = rest.find('\n') {
            push_piece(&mut out, &rest[..at], class);
            out.push_str("</span><span class=\"ln\">");
            rest = &rest[at + 1..];
        }
        push_piece(&mut out, rest, class);
    }
    out.push_str("</span>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // What a reader would see: markup dropped, entities read back.
    fn visible_text(html: &str) -> String {
        let mut text = String::new();
        let mut depth = 0;
        for c in html.chars() {
            match c {
                '<' => depth += 1,
                '>' => depth -= 1,
                _ if depth == 0 => text.push(c),
                _ => {}
            }
        }
        text.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&amp;", "&")
    }

    #[test]
    fn unknown_tags_fall_back_to_escaped_text() {
        assert_eq!(highlight_html("text", "a < b & c"), "a &lt; b &amp; c");
    }

    #[test]
    fn shell_and_linker_scripts_are_highlighted() {
        assert!(highlight_html("sh", "gcc -o a.elf b.s\n").contains("tk-dir"));
        assert!(highlight_html("ld", "SECTIONS { }\n").contains("tk-kw"));
    }

    #[test]
    fn every_source_line_gets_its_own_box() {
        let html = highlight_lines_html("asm", "li t0, 5\n\nret\n");
        assert_eq!(html.matches("<span class=\"ln\">").count(), 3);
        // The trailing newline must not add a fourth, empty line.
        assert!(html.ends_with("</span>"));
    }

    #[test]
    fn line_boxes_keep_a_comment_that_spans_lines_together() {
        let html = highlight_lines_html("ld", "/* one\ntwo */\n");
        assert_eq!(html.matches("<span class=\"ln\">").count(), 2);
        assert_eq!(html.matches("tk-cmt").count(), 2);
    }

    // Markup must never change what the reader sees, one line at a time.
    #[test]
    fn line_boxes_preserve_the_source_text() {
        let source = "_start:\n\n  li t0, 5 # <go> & \"back\"\n";
        let html = highlight_lines_html("asm", source);
        let lines: Vec<String> = html
            .trim_start_matches("<span class=\"ln\">")
            .trim_end_matches("</span>")
            .split("</span><span class=\"ln\">")
            .map(visible_text)
            .collect();
        assert_eq!(
            lines,
            source.trim_end_matches('\n').lines().collect::<Vec<_>>()
        );
    }

    #[test]
    fn assembly_gets_token_classes() {
        let html = highlight_html("asm", "_start:\n  li t0, 5 # go\n");
        for class in ["tk-lbl", "tk-kw", "tk-reg", "tk-num", "tk-cmt"] {
            assert!(html.contains(class), "missing {class}");
        }
    }

    // Markup must never change what the reader sees as source.
    #[test]
    fn rendering_preserves_visible_text() {
        let source = "li t0, 5 # \"quoted\" & <angled>\n";
        assert_eq!(visible_text(&highlight_html("asm", source)), source);
    }
}
