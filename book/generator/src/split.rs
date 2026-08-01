// Splits one authored markdown file into book pages, one per `###` section.

use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Index,
    Chapter,
    Section,
}

#[derive(Debug)]
pub struct Page {
    pub level: Level,
    pub title: String,
    // Output path without extension, e.g. "cpus-and-assembly/registers".
    pub slug: String,
    pub body: String,
}

impl Page {
    // How far to climb to reach the output root, for depth-independent asset links.
    pub fn path_to_root(&self) -> String {
        "../".repeat(self.slug.matches('/').count())
    }

    pub fn href(&self) -> String {
        format!("{}.html", self.slug)
    }
}

pub fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut pending_dash = false;
    for c in title.trim().chars() {
        if c.is_ascii_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.extend(c.to_lowercase());
        } else {
            pending_dash = true;
        }
    }
    if slug.is_empty() {
        slug.push_str("section");
    }
    slug
}

// `### Exercise` appears twice in one chapter, so slugs must be made unique.
fn unique(used: &mut HashSet<String>, base: String) -> String {
    if used.insert(base.clone()) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|candidate| used.insert(candidate.clone()))
        .expect("an unused suffix always exists")
}

struct Heading {
    line: usize,
    level: Level,
    title: String,
}

fn headings_of(lines: &[&str]) -> Vec<Heading> {
    let mut headings = Vec::new();
    let mut in_fence = false;

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        // `###` first: a section line also matches the `##` prefix.
        let found = trimmed
            .strip_prefix("### ")
            .map(|title| (Level::Section, title))
            .or_else(|| trimmed.strip_prefix("## ").map(|t| (Level::Chapter, t)));
        if let Some((level, title)) = found {
            headings.push(Heading {
                line: index,
                level,
                title: title.trim().to_owned(),
            });
        }
    }
    headings
}

pub fn split(index_title: &str, markdown: &str) -> Vec<Page> {
    let lines: Vec<&str> = markdown.lines().collect();
    let headings = headings_of(&lines);

    let mut pages = Vec::new();
    let mut used = HashSet::new();
    let mut used_dirs = HashSet::new();

    // Everything above the first chapter is the book's landing page.
    let first = headings.first().map_or(lines.len(), |h| h.line);
    pages.push(Page {
        level: Level::Index,
        title: index_title.to_owned(),
        slug: unique(&mut used, "index".to_owned()),
        body: lines[..first].join("\n").trim().to_owned(),
    });

    let mut chapter_dir = String::new();
    for (index, heading) in headings.iter().enumerate() {
        let end = headings
            .get(index + 1)
            .map_or(lines.len(), |next| next.line);
        let body = lines[heading.line + 1..end].join("\n").trim().to_owned();

        let slug = match heading.level {
            Level::Chapter => {
                chapter_dir = unique(&mut used_dirs, slugify(&heading.title));
                let slug = format!("{chapter_dir}/index");
                used.insert(slug.clone());
                slug
            }
            Level::Section => {
                let base = if chapter_dir.is_empty() {
                    slugify(&heading.title)
                } else {
                    format!("{chapter_dir}/{}", slugify(&heading.title))
                };
                unique(&mut used, base)
            }
            Level::Index => unreachable!("headings never produce an index page"),
        };

        pages.push(Page {
            level: heading.level,
            title: heading.title.clone(),
            slug,
            body,
        });
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "intro prose\n\n## First Chapter\nchapter intro\n\n### Alpha\nalpha body\n\n### Exercise\nfirst\n\n### Exercise\nsecond\n\n## Second Chapter\n\n### Beta\nbeta body\n";

    #[test]
    fn splits_into_index_chapters_and_sections() {
        let pages = split("Introduction", SAMPLE);
        let slugs: Vec<&str> = pages.iter().map(|p| p.slug.as_str()).collect();
        assert_eq!(
            slugs,
            [
                "index",
                "first-chapter/index",
                "first-chapter/alpha",
                "first-chapter/exercise",
                "first-chapter/exercise-2",
                "second-chapter/index",
                "second-chapter/beta",
            ]
        );
        assert_eq!(pages[0].body, "intro prose");
        assert_eq!(pages[1].body, "chapter intro");
        assert_eq!(pages[3].body, "first");
        assert_eq!(pages[4].body, "second");
    }

    // A `###` inside a fence is sample text, not a new page.
    #[test]
    fn headings_inside_fences_are_ignored() {
        let source = "## Chapter\n\n```text\n### not a heading\n```\n\n### Real\nbody\n";
        let pages = split("Intro", source);
        let titles: Vec<&str> = pages.iter().map(|p| p.title.as_str()).collect();
        assert_eq!(titles, ["Intro", "Chapter", "Real"]);
    }

    #[test]
    fn trailing_spaces_do_not_leak_into_slugs() {
        assert_eq!(slugify("Exercise "), "exercise");
        assert_eq!(
            slugify("Memory Mapped Input / Output"),
            "memory-mapped-input-output"
        );
        assert_eq!(slugify("The same code on QEMU"), "the-same-code-on-qemu");
    }

    #[test]
    fn path_to_root_matches_nesting() {
        let pages = split("Introduction", SAMPLE);
        assert_eq!(pages[0].path_to_root(), "");
        assert_eq!(pages[2].path_to_root(), "../");
    }
}
