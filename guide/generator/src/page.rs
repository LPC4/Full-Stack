// The HTML shell: sidebar, chapter nav, on-page outline.

use crate::code::escape;
use crate::config::Config;
use crate::render::Rendered;
use crate::split::{Level, Page};

// Chevrons point out of the page: back on the left, on on the right.
const CHEVRON_PREV: &str = "M15.5 4.5 7.5 12l8 7.5";
const CHEVRON_NEXT: &str = "M8.5 4.5 16.5 12l-8 7.5";

fn nav_link(page: &Page, current: &Page, label: &str) -> String {
    let href = page.route.href_from(&current.route);
    let title = escape(&page.title);
    let (path, kicker) = if label == "prev" {
        (CHEVRON_PREV, "Previous")
    } else {
        (CHEVRON_NEXT, "Next")
    };
    format!(
        "<a class=\"nav-{label}\" href=\"{href}\" rel=\"{label}\" title=\"{kicker}: {title}\" \
aria-label=\"{kicker} page: {title}\">\
<span class=\"nav-arrow\" aria-hidden=\"true\">\
<svg viewBox=\"0 0 24 24\"><path d=\"{path}\"/></svg></span>\
<span class=\"nav-label\" aria-hidden=\"true\">\
<span class=\"nav-kicker\">{kicker}</span><span class=\"nav-title\">{title}</span></span></a>"
    )
}

fn sidebar(pages: &[Page], current: &Page) -> String {
    let mut out = String::from("<nav class=\"chapters\"><ol class=\"chapter-list\">");
    let mut in_section_list = false;

    for page in pages {
        let is_current = page.route == current.route;
        let aria = if is_current {
            " aria-current=\"page\""
        } else {
            ""
        };
        let active = if is_current { " class=\"active\"" } else { "" };
        let href = page.route.href_from(&current.route);
        let title = escape(&page.title);

        match page.level {
            Level::Section => {
                if !in_section_list {
                    out.push_str("<li class=\"sections\"><ol>");
                    in_section_list = true;
                } else {
                    out.push_str("</li>");
                }
                out.push_str(&format!("<li{active}><a href=\"{href}\"{aria}>{title}</a>"));
            }
            Level::Index | Level::Chapter => {
                if in_section_list {
                    out.push_str("</li></ol></li>");
                    in_section_list = false;
                }
                let kind = if page.level == Level::Index {
                    "index"
                } else {
                    "chapter"
                };
                out.push_str(&format!(
                    "<li class=\"{kind}{}\"><a href=\"{href}\"{aria}>{title}</a></li>",
                    if is_current { " active" } else { "" }
                ));
            }
        }
    }
    if in_section_list {
        out.push_str("</li></ol></li>");
    }
    out.push_str("</ol></nav>");
    out
}

fn outline(rendered: &Rendered) -> String {
    if rendered.outline.is_empty() {
        return String::new();
    }
    let mut out =
        String::from("<aside class=\"outline\"><p class=\"outline-title\">On this page</p><ul>");
    for entry in &rendered.outline {
        out.push_str(&format!(
            "<li class=\"lvl-{}\"><a href=\"#{}\">{}</a></li>",
            entry.level,
            entry.id,
            escape(&entry.title)
        ));
    }
    out.push_str("</ul></aside>");
    out
}

pub fn build(config: &Config, pages: &[Page], index: usize, rendered: &Rendered) -> String {
    let current = &pages[index];
    let guide_root = current.route.path_to_guide_root();
    let site_root = current.route.path_to_site_root();
    let canonical = current.route.href_from(&current.route);
    let title = escape(&current.title);
    let guide_title = escape(&config.title);

    let head_title = if current.level == Level::Index {
        guide_title.clone()
    } else {
        format!("{title} - {guide_title}")
    };
    let description = config
        .description
        .as_deref()
        .map(escape)
        .unwrap_or_default();
    let author = config
        .author
        .as_deref()
        .map(|name| format!("\n<meta name=\"author\" content=\"{}\">", escape(name)))
        .unwrap_or_default();

    let prev = index
        .checked_sub(1)
        .map(|i| nav_link(&pages[i], current, "prev"))
        .unwrap_or_default();
    let next = pages
        .get(index + 1)
        .map(|page| nav_link(page, current, "next"))
        .unwrap_or_default();

    let repo_link = config
        .repo_url
        .as_deref()
        .map(|url| {
            format!("<a class=\"repo\" href=\"{url}\" title=\"Source on GitHub\">GitHub</a>")
        })
        .unwrap_or_default();

    format!(
        r##"<!DOCTYPE html>
<html lang="en" data-theme="navy">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{head_title}</title>
<meta name="description" content="{description}">{author}
<link rel="canonical" href="{canonical}">
<link rel="icon" href="{site_root}favicon.ico"><link rel="icon" href="{guide_root}theme/favicon.svg" type="image/svg+xml"><link rel="apple-touch-icon" href="{site_root}apple-touch-icon.png">
<link rel="stylesheet" href="{guide_root}theme/guide.css">
<script>
  // Applied before paint so a stored theme never flashes the wrong colors.
  try {{
    var t = localStorage.getItem('full-stack-guide-theme');
    if (!t) {{
      t = localStorage.getItem('fsbook-theme');
      if (t) localStorage.setItem('full-stack-guide-theme', t);
    }}
    if (t) document.documentElement.dataset.theme = t;
  }} catch (e) {{}}
</script>
</head>
<body class="{sidebar_class}{outline_class}">
<a class="skip-link" href="#content">Skip to content</a>
<div class="page">
<header class="topbar">
<button class="menu-toggle" type="button" aria-label="Toggle chapters" aria-expanded="false"><span></span></button>
<a class="brand" href="{guide_root}">{guide_title}</a>
<div class="topbar-right">
<button class="theme-toggle" type="button" aria-label="Toggle theme" title="Toggle theme">
<svg class="icon-sun" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="4.1"/><path d="M12 2.6v2.1M12 19.3v2.1M4.7 4.7l1.5 1.5M17.8 17.8l1.5 1.5M2.6 12h2.1M19.3 12h2.1M4.7 19.3l1.5-1.5M17.8 6.2l1.5-1.5"/></svg>
<svg class="icon-moon" viewBox="0 0 24 24" aria-hidden="true"><path d="M20.4 14.7A8.7 8.7 0 0 1 9.3 3.6 8.7 8.7 0 1 0 20.4 14.7z"/></svg>
</button>
<a class="ide" href="{site_root}" title="Open the browser IDE">Open IDE</a>
{repo_link}
</div>
</header>
<div class="body">
<div class="sidebar" id="sidebar">{sidebar}</div>
<main class="content" id="content">
<article class="chapter">
<h1>{title}</h1>
{content}
</article>
</main>
{outline}
</div>
<nav class="pagenav">{prev}{next}</nav>
</div>
<script src="{guide_root}theme/guide.js"></script>
</body>
</html>
"##,
        sidebar_class = config.sidebar_class(),
        // Without an outline there is nothing on the right to leave room for.
        outline_class = if rendered.outline.is_empty() {
            " no-outline"
        } else {
            ""
        },
        sidebar = sidebar(pages, current),
        content = rendered.html,
        outline = outline(rendered),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Part, Sidebar};
    use crate::split;
    use std::path::PathBuf;

    fn config() -> Config {
        Config {
            title: "Full-Stack Guide".to_owned(),
            author: None,
            description: None,
            repo_url: None,
            sidebar: Sidebar::Left,
            parts: vec![Part {
                title: "Intro".to_owned(),
                source: PathBuf::from("src/written.md"),
            }],
        }
    }

    #[test]
    fn generated_navigation_uses_clean_urls() {
        let pages = split::split("Intro", "intro\n\n## CPU\nchapter\n\n### Registers\npage\n");
        let html = build(&config(), &pages, 2, &Rendered::default());
        assert!(html.contains("href=\"../../cpu/\""));
        assert!(html.contains("href=\"../../\""));
        assert!(html.contains("href=\"../../../\" title=\"Open the browser IDE\""));
        assert!(html.contains("rel=\"canonical\" href=\"../../cpu/registers/\""));
        assert!(!html.contains("href=\"index.html\""));
        assert!(!html.contains("href=\"../../cpu/registers/index\""));
        assert!(!html.contains(".html\" rel=\"next\""));
    }
}
