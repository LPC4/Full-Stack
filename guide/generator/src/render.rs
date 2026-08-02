// Markdown to HTML, with our own code fences, figures, and repository links.

use crate::code;
use crate::repository::{Catalog, is_ide_source, validate_path};
use crate::split::slugify;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd, html};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub struct Context<'a> {
    pub path_to_guide_root: &'a str,
    pub path_to_site_root: &'a str,
    pub figures: &'a Path,
    pub repository: &'a Catalog,
    pub repository_url: Option<&'a str>,
}

#[derive(Debug)]
pub struct Entry {
    pub level: u8,
    pub title: String,
    pub id: String,
}

#[derive(Debug, Default)]
pub struct Rendered {
    pub html: String,
    pub outline: Vec<Entry>,
}

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
}

fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn repository_link(dest: &str, ctx: &Context<'_>) -> Result<Option<String>, String> {
    let Some(reference) = dest.strip_prefix("repo:") else {
        return Ok(None);
    };
    let (path, fragment) = reference
        .split_once('#')
        .map_or((reference, None), |(path, fragment)| (path, Some(fragment)));
    validate_path(path)?;
    if !ctx.repository.contains(path) {
        return Err(format!("repository link names missing file `{path}`"));
    }
    let line = fragment
        .map(|fragment| {
            if fragment.contains('#') {
                return Err(format!("invalid repository line fragment `#{fragment}`"));
            }
            let number = fragment
                .strip_prefix('L')
                .ok_or_else(|| format!("invalid repository line fragment `#{fragment}`"))?;
            let line = number
                .parse::<usize>()
                .map_err(|_error| format!("invalid repository line fragment `#{fragment}`"))?;
            if line == 0 {
                return Err("repository line numbers are one-based".to_owned());
            }
            Ok(line)
        })
        .transpose()?;
    let ide_source = is_ide_source(path);
    let mut href = if ide_source {
        format!("{}?file={}", ctx.path_to_site_root, percent_encode(path))
    } else {
        let repository_url = ctx
            .repository_url
            .ok_or_else(|| format!("repository link `{path}` requires `repo_url`"))?
            .trim_end_matches('/');
        let encoded_path = path
            .split('/')
            .map(percent_encode)
            .collect::<Vec<_>>()
            .join("/");
        format!("{repository_url}/blob/main/{encoded_path}")
    };
    if let Some(line) = line {
        if ide_source {
            href.push_str(&format!("&amp;line={line}"));
        } else {
            href.push_str(&format!("#L{line}"));
        }
    }
    Ok(Some(format!(
        "<a class=\"source-link\" href=\"{href}\" target=\"_blank\" rel=\"noopener\">"
    )))
}

/// LaTeX points to CSS pixels. One factor for every figure, so a diagram's
/// labels are the same size wherever it appears; the CSS caps the width.
const FIGURE_SCALE: f32 = 1.9;

// The SVGs carry their PDF size, which renders far too small next to 16px body text.
fn svg_dimension(svg: &str, name: &str) -> Option<u32> {
    let marker = format!("{name}=\"");
    let rest = svg.split_once(&marker)?.1;
    let value: f32 = rest.split_once('"')?.0.parse().ok()?;
    Some((value * FIGURE_SCALE).round() as u32)
}

fn figure_size(path: &Path) -> Option<(u32, u32)> {
    let svg = fs::read_to_string(path).ok()?;
    Some((
        svg_dimension(&svg, "width")?,
        svg_dimension(&svg, "height")?,
    ))
}

fn figure_html(dest: &str, alt: &str, ctx: &Context<'_>) -> String {
    let root = ctx.path_to_guide_root;
    let name = dest.rsplit('/').next().unwrap_or(dest);
    let stem = name.strip_suffix(".svg").unwrap_or(name);
    let has_dark = ctx.figures.join(format!("{stem}.dark.svg")).is_file();
    let sizing = figure_size(&ctx.figures.join(name))
        .map(|(width, height)| {
            format!(" width=\"{width}\" height=\"{height}\" style=\"width:{width}px\"")
        })
        .unwrap_or_default();

    let alt_attr = code::escape(alt);
    let light = format!(
        "<img class=\"fig-light\" src=\"{root}figures/{name}\" alt=\"{alt_attr}\"{sizing} loading=\"lazy\" decoding=\"async\" fetchpriority=\"low\">"
    );
    // The class, not a `:has()` rule, is what tells the theme a twin exists.
    let (dark, kind) = if has_dark {
        (
            format!(
                "<img class=\"fig-dark\" src=\"{root}figures/{stem}.dark.svg\" alt=\"\" aria-hidden=\"true\"{sizing} loading=\"lazy\" decoding=\"async\" fetchpriority=\"low\">"
            ),
            " has-dark",
        )
    } else {
        (String::new(), "")
    };
    let caption = if alt.is_empty() {
        String::new()
    } else {
        format!("<figcaption>{alt_attr}</figcaption>")
    };
    format!("<figure class=\"figure{kind}\">{light}{dark}{caption}</figure>")
}

fn plain_text(events: &[Event<'_>]) -> String {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Text(text) | Event::Code(text) => Some(text.as_ref()),
            _ => None,
        })
        .collect()
}

// Every heading gets an id for the outline to link to, suffixed until unique.
fn heading_html(
    level: HeadingLevel,
    inner: Vec<Event<'_>>,
    used_ids: &mut HashSet<String>,
) -> (String, Entry) {
    let title = plain_text(&inner);
    let mut id = slugify(&title);
    while !used_ids.insert(id.clone()) {
        id.push('-');
    }
    let mut inner_html = String::new();
    html::push_html(&mut inner_html, inner.into_iter());
    let tag = level as u8;
    (
        format!("<h{tag} id=\"{id}\">{inner_html}</h{tag}>"),
        Entry {
            level: tag,
            title,
            id,
        },
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "Markdown events are rewritten in one stateful pass"
)]
pub fn render(markdown: &str, ctx: &Context<'_>) -> Result<Rendered, String> {
    let mut out = Vec::new();
    let mut outline = Vec::new();
    let mut used_ids = HashSet::new();

    let mut fence: Option<(String, String)> = None;
    let mut image: Option<(String, Vec<Event<'_>>)> = None;
    let mut heading: Option<(HeadingLevel, Vec<Event<'_>>)> = None;
    // A <figure> cannot live inside <p>, so a paragraph that opens with one loses its tags.
    let mut paragraph_open = false;
    let mut figure_paragraph = false;
    let mut repository_link_open = false;

    for event in Parser::new_ext(markdown, options()) {
        if paragraph_open {
            paragraph_open = false;
            figure_paragraph = matches!(event, Event::Start(Tag::Image { .. }));
            if !figure_paragraph {
                out.push(Event::Start(Tag::Paragraph));
            }
        }

        // Code fences are rendered by our own highlighter, not by cmark.
        if let Some((_, buffer)) = fence.as_mut() {
            match event {
                Event::Text(text) => {
                    buffer.push_str(&text);
                    continue;
                }
                Event::End(TagEnd::CodeBlock) => {
                    let (info, source) = fence.take().expect("inside a fence");
                    let parsed = code::parse_fence(&info);
                    out.push(Event::Html(code::render(&parsed, &source).into()));
                    continue;
                }
                _ => continue,
            }
        }

        if let Some((_, buffer)) = image.as_mut() {
            if matches!(event, Event::End(TagEnd::Image)) {
                let (dest, inner) = image.take().expect("inside an image");
                let alt = plain_text(&inner);
                out.push(Event::Html(figure_html(&dest, &alt, ctx).into()));
            } else {
                buffer.push(event);
            }
            continue;
        }

        if let Some((_, buffer)) = heading.as_mut() {
            if matches!(event, Event::End(TagEnd::Heading(_))) {
                let (level, inner) = heading.take().expect("inside a heading");
                let (html, entry) = heading_html(level, inner, &mut used_ids);
                out.push(Event::Html(html.into()));
                outline.push(entry);
            } else {
                buffer.push(event);
            }
            continue;
        }

        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                fence = Some((info.to_string(), String::new()));
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)) => {
                fence = Some((String::new(), String::new()));
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                image = Some((dest_url.to_string(), Vec::new()));
            }
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some((level, Vec::new()));
            }
            // A table needs an outer box to scroll in; scrolling the table
            // itself would make it shrink-wrap instead of filling the column.
            Event::Start(Tag::Table(alignments)) => {
                out.push(Event::Html("<div class=\"table-wrap\">".into()));
                out.push(Event::Start(Tag::Table(alignments)));
            }
            Event::End(TagEnd::Table) => {
                out.push(Event::End(TagEnd::Table));
                out.push(Event::Html("</div>".into()));
            }
            Event::Start(Tag::Paragraph) => paragraph_open = true,
            Event::End(TagEnd::Paragraph) if figure_paragraph => figure_paragraph = false,
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                if let Some(open) = repository_link(&dest_url, ctx)? {
                    repository_link_open = true;
                    out.push(Event::Html(open.into()));
                } else {
                    out.push(Event::Start(Tag::Link {
                        link_type,
                        dest_url,
                        title,
                        id,
                    }));
                }
            }
            Event::End(TagEnd::Link) if repository_link_open => {
                repository_link_open = false;
                out.push(Event::Html("</a>".into()));
            }
            other => out.push(other),
        }
    }

    let mut html_out = String::new();
    html::push_html(&mut html_out, out.into_iter());
    Ok(Rendered {
        html: html_out,
        outline,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::LazyLock;

    fn ctx() -> Context<'static> {
        static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
            Catalog::testing(&[
                "crates/asm-to-binary/_RISCV_SPECIFICATIONS.md",
                "crates/virtual-machine/src/bus.rs",
                "programs/example/output.ir",
                "programs/example/source.hll",
                "programs/lessons/01_sum/sum.s",
                "programs/lessons/07_qemu/link.ld",
                "directory/file with spaces.rs",
            ])
        });
        Context {
            path_to_guide_root: "../../",
            path_to_site_root: "../../../",
            figures: Path::new("does-not-exist"),
            repository: &CATALOG,
            repository_url: Some("https://github.com/LPC4/Full-Stack"),
        }
    }

    #[test]
    fn code_fences_use_our_highlighter() {
        let out = render("```asm\nli t0, 5\n```\n", &ctx()).expect("render");
        assert!(out.html.contains("class=\"code wrapped\""));
        assert!(out.html.contains("tk-kw"));
        // cmark's own escaping path would emit language-asm instead.
        assert!(!out.html.contains("language-asm"));
    }

    #[test]
    fn plain_fences_stay_unhighlighted() {
        let out = render("```text\n+---+\n| a |\n+---+\n```\n", &ctx()).expect("render");
        assert!(out.html.contains("data-lang=\"text\""));
        assert!(!out.html.contains("<span class=\"tk"));
    }

    #[test]
    fn runnable_fences_get_sandbox_controls() {
        let out = render("```asm,runnable\nli t0, 5\n```\n", &ctx()).expect("render");
        assert!(out.html.contains("class=\"sandbox\""));
        assert!(out.html.contains("sandbox-run"));
    }

    #[test]
    fn assembly_ir_and_hll_repository_links_open_the_ide_in_a_new_tab() {
        let out = render(
            "[asm](repo:programs/lessons/01_sum/sum.s#L2) \
             [ir](repo:programs/example/output.ir) \
             [hll](repo:programs/example/source.hll)\n",
            &ctx(),
        )
        .expect("render");
        assert!(
            out.html
                .contains("../../../?file=programs%2Flessons%2F01_sum%2Fsum.s&amp;line=2")
        );
        assert!(
            out.html
                .contains("../../../?file=programs%2Fexample%2Foutput.ir")
        );
        assert!(
            out.html
                .contains("../../../?file=programs%2Fexample%2Fsource.hll")
        );
        assert!(out.html.contains("class=\"source-link\""));
        assert!(out.html.contains("target=\"_blank\" rel=\"noopener\""));
    }

    #[test]
    fn documentation_vm_code_and_other_repository_files_open_github() {
        let out = render(
            "[spec](repo:crates/asm-to-binary/_RISCV_SPECIFICATIONS.md#L42) \
             [bus](repo:crates/virtual-machine/src/bus.rs#L45) \
             [script](repo:programs/lessons/07_qemu/link.ld)\n",
            &ctx(),
        )
        .expect("render");
        assert!(out.html.contains(
            "href=\"https://github.com/LPC4/Full-Stack/blob/main/crates/asm-to-binary/_RISCV_SPECIFICATIONS.md#L42\""
        ));
        assert!(out.html.contains(
            "href=\"https://github.com/LPC4/Full-Stack/blob/main/crates/virtual-machine/src/bus.rs#L45\""
        ));
        assert!(out.html.contains(
            "href=\"https://github.com/LPC4/Full-Stack/blob/main/programs/lessons/07_qemu/link.ld\""
        ));
        assert!(!out.html.contains("?file="));
        assert!(out.html.contains("target=\"_blank\" rel=\"noopener\""));
    }

    #[test]
    fn external_links_are_untouched() {
        let out = render("[qemu](https://www.qemu.org/docs/master/)\n", &ctx()).expect("render");
        assert!(out.html.contains("https://www.qemu.org/docs/master/"));
    }

    #[test]
    fn local_links_and_images_are_untouched() {
        let out = render("[nearby](other-page/) [part](#part)\n", &ctx()).expect("render");
        assert!(out.html.contains("href=\"other-page/\""));
        assert!(out.html.contains("href=\"#part\""));
    }

    #[test]
    fn invalid_repository_links_fail_rendering() {
        assert!(render("[missing](repo:no/such.rs)\n", &ctx()).is_err());
        assert!(render("[bad](repo:../secret#L0)\n", &ctx()).is_err());
        assert!(render("[bad](repo:crates/virtual-machine/src/bus.rs#42)\n", &ctx()).is_err());
    }

    #[test]
    fn images_become_figures_with_captions() {
        let out = render("![The stack grows down](figures/stack.svg)\n", &ctx()).expect("render");
        assert!(out.html.contains("<figure class=\"figure\">"));
        assert!(out.html.contains("../figures/stack.svg"));
        assert!(
            out.html
                .contains("<figcaption>The stack grows down</figcaption>")
        );
        // No dark twin on disk, so none is referenced.
        assert!(!out.html.contains("stack.dark.svg"));
        assert!(out.html.contains("loading=\"lazy\""));
        assert!(out.html.contains("decoding=\"async\""));
        assert!(out.html.contains("fetchpriority=\"low\""));
    }

    #[test]
    fn svg_dimensions_are_scaled_for_intrinsic_layout() {
        let svg = r#"<svg width="285.98" height="114.84" viewBox="0 0 285.98 114.84">"#;
        assert_eq!(svg_dimension(svg, "width"), Some(543));
        assert_eq!(svg_dimension(svg, "height"), Some(218));
    }

    #[test]
    fn a_lone_figure_is_not_wrapped_in_a_paragraph() {
        let out = render("before\n\n![](figures/stack.svg)\n\nafter\n", &ctx()).expect("render");
        assert!(!out.html.contains("<p><figure"), "{}", out.html);
        // Text paragraphs around it keep their tags.
        assert!(out.html.contains("<p>before</p>"), "{}", out.html);
        assert!(out.html.contains("<p>after</p>"), "{}", out.html);
    }

    #[test]
    fn text_sharing_a_paragraph_with_an_image_survives() {
        let out = render("see ![](figures/stack.svg) here\n", &ctx()).expect("render");
        assert!(out.html.contains("see "), "{}", out.html);
        assert!(out.html.contains(" here"), "{}", out.html);
    }

    #[test]
    fn tables_get_a_box_to_scroll_in() {
        let out = render("| a | b |\n| --- | --- |\n| 1 | 2 |\n", &ctx()).expect("render");
        assert!(
            out.html.contains("<div class=\"table-wrap\"><table>"),
            "{}",
            out.html
        );
        assert!(out.html.contains("</table>\n</div>"), "{}", out.html);
    }

    #[test]
    fn headings_get_stable_ids_and_an_outline() {
        let out =
            render("#### First Part\ntext\n\n#### First Part\nmore\n", &ctx()).expect("render");
        assert_eq!(out.outline.len(), 2);
        assert_eq!(out.outline[0].id, "first-part");
        assert_eq!(out.outline[1].id, "first-part-");
        assert!(out.html.contains("<h4 id=\"first-part\">"));
    }
}
