// Fenced code blocks: highlighted through the same scanners the IDE uses.

pub use syntax_highlight::html::{escape, highlight_html, highlight_lines_html};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Fence<'a> {
    pub tag: &'a str,
    pub runnable: bool,
    // A runnable snippet that is already a whole program, needing no wrapper.
    pub bare: bool,
}

// Info strings look like "asm", "asm,runnable" or "asm,runnable=bare".
pub fn parse_fence(info: &str) -> Fence<'_> {
    let mut parts = info.split(&[',', ' '][..]).filter(|p| !p.is_empty());
    let tag = parts.next().unwrap_or("");
    let mut fence = Fence {
        tag,
        ..Default::default()
    };
    for part in parts {
        match part.trim() {
            "runnable" => fence.runnable = true,
            "runnable=bare" => {
                fence.runnable = true;
                fence.bare = true;
            }
            _ => {}
        }
    }
    fence
}

/// Full `<pre>` block for a fence, including the sandbox controls when runnable.
pub fn render(fence: &Fence<'_>, source: &str) -> String {
    let tag = escape(fence.tag);

    if !fence.runnable {
        let inner = highlight_lines_html(fence.tag, source);
        return format!(
            "<pre class=\"code wrapped\" data-lang=\"{tag}\"><code>{inner}</code></pre>\n"
        );
    }

    // The editable pane is repainted from `highlight_html` as you type, and it
    // scrolls rather than wraps, so it stays one flat run of spans.
    let inner = highlight_html(fence.tag, source);
    let wrap = if fence.bare { "bare" } else { "snippet" };
    let raw = escape(source);
    format!(
        "<div class=\"sandbox\" data-lang=\"{tag}\" data-wrap=\"{wrap}\">\
<div class=\"sandbox-bar\"><span class=\"sandbox-title\">{tag}</span>\
<button class=\"sandbox-run\" type=\"button\">Run</button>\
<button class=\"sandbox-reset\" type=\"button\">Reset</button></div>\
<pre class=\"code sandbox-code\" contenteditable=\"true\" spellcheck=\"false\"><code>{inner}</code></pre>\
<textarea class=\"sandbox-source\" hidden>{raw}</textarea>\
<div class=\"sandbox-output\" hidden></div></div>\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fence_attributes() {
        assert_eq!(parse_fence("asm").tag, "asm");
        assert!(!parse_fence("asm").runnable);

        let runnable = parse_fence("asm,runnable");
        assert!(runnable.runnable && !runnable.bare);

        let bare = parse_fence("asm,runnable=bare");
        assert!(bare.runnable && bare.bare);
        assert_eq!(bare.tag, "asm");
    }

    // Lines share one box in the sandbox and get their own in a listing, so the
    // hanging indent lands on wrapped text instead of on every line but the first.
    #[test]
    fn listings_box_each_line_and_the_sandbox_does_not() {
        let listing = render(&parse_fence("asm"), "_start:\n  ret\n");
        assert_eq!(listing.matches("class=\"ln\"").count(), 2);

        let sandbox = render(&parse_fence("asm,runnable"), "_start:\n  ret\n");
        assert!(!sandbox.contains("class=\"ln\""));
    }

    #[test]
    fn runnable_blocks_carry_the_original_source() {
        let fence = parse_fence("asm,runnable");
        let html = render(&fence, "li t0, 5\n");
        assert!(html.contains("sandbox-source"));
        assert!(html.contains("data-wrap=\"snippet\""));
        assert!(html.contains("tk-kw"), "still highlighted");
    }

    #[test]
    fn bare_runnable_blocks_are_marked_as_whole_programs() {
        let html = render(&parse_fence("asm,runnable=bare"), ".globl _start\n");
        assert!(html.contains("data-wrap=\"bare\""));
    }
}
