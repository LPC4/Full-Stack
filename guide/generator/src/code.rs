// Fenced code blocks: highlighted through the same scanners the IDE uses.

pub use fs_utils::syntax_highlight::html::{escape, highlight_html, highlight_lines_html};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Fence<'a> {
    pub tag: &'a str,
    pub runnable: bool,
    // A runnable snippet that is already a whole program, needing no wrapper.
    pub bare: bool,
    pub expected: Option<&'a str>,
}

// Info strings look like "asm", "asm,runnable" or "asm,runnable expect=t0:5|t1:8".
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
            value if value.starts_with("expect=") => {
                fence.expected = value.strip_prefix("expect=");
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

    // The textarea sits over this highlighted copy, keeping native editing and undo.
    let inner = highlight_html(fence.tag, source);
    let wrap = if fence.bare { "bare" } else { "snippet" };
    let expected = escape(fence.expected.unwrap_or(""));
    let raw = escape(source);
    format!(
        "<div class=\"sandbox\" data-lang=\"{tag}\" data-wrap=\"{wrap}\" data-expect=\"{expected}\">\
<div class=\"sandbox-bar\"><span class=\"sandbox-title\">{tag}</span>\
<button class=\"sandbox-run\" type=\"button\">Run</button>\
<button class=\"sandbox-reset\" type=\"button\">Reset</button></div>\
<div class=\"sandbox-editor\">\
<pre class=\"code sandbox-highlight\" aria-hidden=\"true\"><code>{inner}</code></pre>\
<textarea class=\"sandbox-code\" aria-label=\"Editable assembly example\" spellcheck=\"false\" wrap=\"off\">{raw}</textarea>\
</div>\
<div class=\"sandbox-output\" aria-live=\"polite\" hidden></div></div>\n"
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

        let expected = parse_fence("asm,runnable expect=t0:5|a0:12");
        assert_eq!(expected.expected, Some("t0:5|a0:12"));
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
        assert!(html.contains("sandbox-editor"));
        assert!(html.contains("sandbox-highlight"));
        assert!(html.contains("textarea class=\"sandbox-code\""));
        assert!(!html.contains("contenteditable"));
        assert!(html.contains("data-wrap=\"snippet\""));
        assert!(html.contains("data-expect=\"\""));
        assert!(html.contains("aria-live=\"polite\""));
        assert!(html.contains("tk-kw"), "still highlighted");
    }

    #[test]
    fn expected_registers_are_attached_to_the_sandbox() {
        let fence = parse_fence("asm,runnable expect=t0:5|a0:12");
        let html = render(&fence, "li t0, 5\n");
        assert!(html.contains("data-expect=\"t0:5|a0:12\""));
    }

    #[test]
    fn bare_runnable_blocks_are_marked_as_whole_programs() {
        let html = render(&parse_fence("asm,runnable=bare"), ".globl _start\n");
        assert!(html.contains("data-wrap=\"bare\""));
    }
}
