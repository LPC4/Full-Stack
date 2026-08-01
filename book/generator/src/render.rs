// Markdown to HTML, with our own code fences, figures and link rewriting.

use crate::code;
use crate::split::slugify;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd, html};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub struct Context<'a> {
    pub path_to_root: &'a str,
    pub repo_url: Option<&'a str>,
    pub figures: &'a Path,
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

// Repo-relative links only resolve on GitHub, so point them there.
fn rewrite_link(dest: &str, repo_url: Option<&str>) -> String {
    let external = dest.starts_with("http://")
        || dest.starts_with("https://")
        || dest.starts_with('#')
        || dest.starts_with("mailto:");
    if external {
        return dest.to_owned();
    }
    let Some(repo) = repo_url else {
        return dest.to_owned();
    };
    // The path was authored relative to a file that has since moved; treat
    // whatever remains after the `../` hops as relative to the repo root.
    let mut rest = dest;
    while let Some(stripped) = rest.strip_prefix("../") {
        rest = stripped;
    }
    let rest = rest.strip_prefix("./").unwrap_or(rest);
    format!("{}/blob/main/{rest}", repo.trim_end_matches('/'))
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
    let root = ctx.path_to_root;
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

pub fn render(markdown: &str, ctx: &Context<'_>) -> Rendered {
    let mut out = Vec::new();
    let mut outline = Vec::new();
    let mut used_ids = HashSet::new();

    let mut fence: Option<(String, String)> = None;
    let mut image: Option<(String, Vec<Event<'_>>)> = None;
    let mut heading: Option<(HeadingLevel, Vec<Event<'_>>)> = None;
    // A <figure> cannot live inside <p>, so a paragraph that opens with one loses its tags.
    let mut paragraph_open = false;
    let mut figure_paragraph = false;

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
                out.push(Event::Start(Tag::Link {
                    link_type,
                    dest_url: rewrite_link(&dest_url, ctx.repo_url).into(),
                    title,
                    id,
                }));
            }
            other => out.push(other),
        }
    }

    let mut html_out = String::new();
    html::push_html(&mut html_out, out.into_iter());
    Rendered {
        html: html_out,
        outline,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Context<'static> {
        Context {
            path_to_root: "../",
            repo_url: Some("https://github.com/LPC4/Full-Stack"),
            figures: Path::new("does-not-exist"),
        }
    }

    #[test]
    fn code_fences_use_our_highlighter() {
        let out = render("```asm\nli t0, 5\n```\n", &ctx());
        assert!(out.html.contains("class=\"code wrapped\""));
        assert!(out.html.contains("tk-kw"));
        // cmark's own escaping path would emit language-asm instead.
        assert!(!out.html.contains("language-asm"));
    }

    #[test]
    fn plain_fences_stay_unhighlighted() {
        let out = render("```text\n+---+\n| a |\n+---+\n```\n", &ctx());
        assert!(out.html.contains("data-lang=\"text\""));
        assert!(!out.html.contains("<span class=\"tk"));
    }

    #[test]
    fn runnable_fences_get_sandbox_controls() {
        let out = render("```asm,runnable\nli t0, 5\n```\n", &ctx());
        assert!(out.html.contains("class=\"sandbox\""));
        assert!(out.html.contains("sandbox-run"));
    }

    #[test]
    fn repo_relative_links_point_at_github() {
        let out = render(
            "[spec](../crates/asm-to-binary/_RISCV_SPECIFICATIONS.md)\n",
            &ctx(),
        );
        assert!(
            out.html.contains(
                "https://github.com/LPC4/Full-Stack/blob/main/crates/asm-to-binary/_RISCV_SPECIFICATIONS.md"
            ),
            "{}",
            out.html
        );
    }

    #[test]
    fn external_links_are_untouched() {
        let out = render("[qemu](https://www.qemu.org/docs/master/)\n", &ctx());
        assert!(out.html.contains("https://www.qemu.org/docs/master/"));
        assert!(!out.html.contains("blob/main"));
    }

    #[test]
    fn images_become_figures_with_captions() {
        let out = render("![The stack grows down](figures/stack.svg)\n", &ctx());
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
        let out = render("before\n\n![](figures/stack.svg)\n\nafter\n", &ctx());
        assert!(!out.html.contains("<p><figure"), "{}", out.html);
        // Text paragraphs around it keep their tags.
        assert!(out.html.contains("<p>before</p>"), "{}", out.html);
        assert!(out.html.contains("<p>after</p>"), "{}", out.html);
    }

    #[test]
    fn text_sharing_a_paragraph_with_an_image_survives() {
        let out = render("see ![](figures/stack.svg) here\n", &ctx());
        assert!(out.html.contains("see "), "{}", out.html);
        assert!(out.html.contains(" here"), "{}", out.html);
    }

    #[test]
    fn tables_get_a_box_to_scroll_in() {
        let out = render("| a | b |\n| --- | --- |\n| 1 | 2 |\n", &ctx());
        assert!(
            out.html.contains("<div class=\"table-wrap\"><table>"),
            "{}",
            out.html
        );
        assert!(out.html.contains("</table>\n</div>"), "{}", out.html);
    }

    #[test]
    fn headings_get_stable_ids_and_an_outline() {
        let out = render("#### First Part\ntext\n\n#### First Part\nmore\n", &ctx());
        assert_eq!(out.outline.len(), 2);
        assert_eq!(out.outline[0].id, "first-part");
        assert_eq!(out.outline[1].id, "first-part-");
        assert!(out.html.contains("<h4 id=\"first-part\">"));
    }
}
