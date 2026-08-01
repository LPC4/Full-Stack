#!/usr/bin/env python3
"""Render the TikZ figures in teaching/written.tex to SVG for the book.

Run locally, not in CI: the generated SVGs are committed so the Pages build
needs no LaTeX. Requires `tectonic` on PATH and PyMuPDF (`pip install pymupdf`).

    python book/tools/tikz_to_svg.py            # rebuild every figure
    python book/tools/tikz_to_svg.py --list     # just show what would be built
    python book/tools/tikz_to_svg.py --only sv39_walk
"""

import argparse
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
TEX = REPO / "teaching" / "written.tex"
OUT = REPO / "book" / "figures"
NAMES = REPO / "book" / "figures" / "figures.toml"

# Dark-theme overrides. Only the ink/surface colors change; the accent hues are
# already legible on the dark page, so the figures keep the palette the PDF uses.
DARK = {
    "ink": "E6EDF3",
    "gray7": "C9D1D9",
    "gray5": "8B949E",
    "gray3": "30363D",
    "codebg": "161B22",
}

# `gblue!12` mixes toward the model's implicit white, which redefining the named
# `white` does not change, so the mix target has to be written out explicitly.
# The digit run must be terminal, or `a!70!b` backtracks to `a!7` and strands a `0`.
TINT = re.compile(r"\b([a-zA-Z][a-zA-Z0-9]*)!(\d+)(?![\d!])")


def read_tex():
    if not TEX.is_file():
        sys.exit(f"missing {TEX}. teaching/ is git-ignored; it must exist locally.")
    return TEX.read_text(encoding="utf-8").splitlines()


def preamble_blocks(lines):
    """Packages, palette and tikz styles, lifted verbatim so figures cannot drift."""
    end = next(i for i, l in enumerate(lines) if l.startswith(r"\begin{document}"))
    head = lines[:end]

    packages = [
        l
        for l in head
        if l.startswith(r"\usepackage") or l.startswith(r"\usetikzlibrary")
    ]
    # Page furniture is meaningless in a standalone figure and pulls in fonts.
    drop = ("geometry", "fancyhdr", "titlesec", "hyperref", "xurl", "parskip", "enumitem")
    packages = [l for l in packages if not any(f"{{{d}}}" in l or f"]{{{d}}}" in l for d in drop)]

    colors = [l for l in head if l.startswith(r"\definecolor")]

    start = next(i for i, l in enumerate(head) if l.startswith(r"\tikzset{"))
    depth, stop = 0, start
    for i in range(start, len(head)):
        depth += head[i].count("{") - head[i].count("}")
        if depth == 0:
            stop = i
            break
    styles = head[start : stop + 1]
    return packages, colors, styles


def figures(lines):
    """Every tikzpicture that belongs to the body, with its section context."""
    doc = next(i for i, l in enumerate(lines) if l.startswith(r"\begin{document}"))
    section = subsection = ""
    found, depth, start = [], 0, None

    for i, line in enumerate(lines):
        stripped = line.strip()
        m = re.match(r"\\section\*?\{(.+?)\}", stripped)
        if m:
            section, subsection = m.group(1), ""
        m = re.match(r"\\subsection\*?\{(.+?)\}", stripped)
        if m:
            subsection = m.group(1)

        if stripped.startswith(r"\begin{tikzpicture}"):
            start, depth = i, 0
        if start is not None:
            depth += line.count(r"\begin{tikzpicture}") - line.count(r"\end{tikzpicture}")
            if depth == 0 and stripped.startswith(r"\end{tikzpicture}"):
                # Anything before the first section is title-page decoration.
                if i > doc and section:
                    found.append(
                        {
                            "line": start + 1,
                            "section": section,
                            "subsection": subsection,
                            "body": "\n".join(lines[start : i + 1]),
                        }
                    )
                start = None
    return found


def slugify(text):
    text = re.sub(r"\\[a-zA-Z]+", " ", text)
    text = re.sub(r"[^0-9A-Za-z]+", "_", text).strip("_").lower()
    return re.sub(r"_+", "_", text)


def assign_names(found):
    """Name each figure after its subsection, numbering repeats within one."""
    counts = {}
    for figure in found:
        base = slugify(figure["subsection"] or figure["section"]) or "figure"
        counts[base] = counts.get(base, 0) + 1
        figure["name"] = base if counts[base] == 1 else f"{base}_{counts[base]}"
    return found


def figure_metadata():
    if not NAMES.is_file():
        return {}
    metadata = {}
    for block in NAMES.read_text(encoding="utf-8").split("[[figure]]"):
        fields = {
            key: value
            for key, value in re.findall(r'^(source|name|caption)\s*=\s*"([^"]*)"', block, re.M)
        }
        if "name" in fields:
            metadata[fields.get("source", fields["name"])] = fields
    return metadata


def apply_metadata(found, metadata):
    for figure in found:
        source = figure["name"]
        entry = metadata.get(source, {})
        figure["source"] = source
        figure["name"] = entry.get("name", source)
        figure["caption"] = entry.get("caption", "")
    return found


def toml_string(value):
    return value.replace("\\", "\\\\").replace('"', '\\"')


def darken(text):
    # A box filled with `ink` is a terminal screen, not body text: it must stay
    # dark in both themes, or its white label becomes unreadable.
    text = text.replace("fill=ink", "fill=terminal").replace("draw=ink", "draw=terminal")
    # Text on a saturated accent fill stays light; paper surfaces go dark.
    text = text.replace("text=white", "text=oncolor")
    text = text.replace(r"\color{white}", r"\color{oncolor}")
    text = re.sub(r"\bwhite\b", "paper", text)
    return TINT.sub(r"\1!\2!paper", text)


def document(packages, colors, styles, body, dark):
    palette = list(colors)
    if dark:
        for name, value in DARK.items():
            palette = [c for c in palette if not c.startswith(rf"\definecolor{{{name}}}")]
            palette.append(rf"\definecolor{{{name}}}{{HTML}}{{{value}}}")
        palette.insert(0, r"\definecolor{oncolor}{HTML}{FFFFFF}")
        palette.insert(1, r"\definecolor{paper}{HTML}{0D1117}")
        palette.insert(2, r"\definecolor{terminal}{HTML}{161B22}")
        styles = [darken(line) for line in styles]
        body = darken(body)

    return "\n".join(
        [r"\documentclass[border=4pt]{standalone}", *packages, *palette, *styles]
        + [r"\color{ink}", r"\begin{document}", body, r"\end{document}", ""]
    )


def to_svg(pdf: Path) -> str:
    import fitz  # PyMuPDF

    with fitz.open(pdf) as doc:
        # Text as paths: no font loading in the browser, and no metric drift.
        return doc[0].get_svg_image(text_as_path=True)


def build(figure, packages, colors, styles, dark, workdir):
    suffix = ".dark" if dark else ""
    tex = workdir / f"{figure['name']}{suffix}.tex"
    tex.write_text(document(packages, colors, styles, figure["body"], dark), encoding="utf-8")

    result = subprocess.run(
        ["tectonic", "-X", "compile", "--outdir", str(workdir), "--keep-logs", str(tex)],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        tail = (result.stderr or result.stdout).strip().splitlines()[-12:]
        raise RuntimeError(f"tectonic failed for {tex.name}:\n  " + "\n  ".join(tail))

    pdf = tex.with_suffix(".pdf")
    target = OUT / f"{figure['name']}{suffix}.svg"
    target.write_text(to_svg(pdf), encoding="utf-8")
    return target


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--list", action="store_true", help="show figures and exit")
    parser.add_argument("--only", help="build a single figure by name")
    parser.add_argument("--no-dark", action="store_true", help="skip the dark variants")
    args = parser.parse_args()

    lines = read_tex()
    packages, colors, styles = preamble_blocks(lines)
    found = apply_metadata(assign_names(figures(lines)), figure_metadata())
    selected = found

    if args.only:
        selected = [f for f in found if args.only in (f["source"], f["name"])]
        if not selected:
            sys.exit(f"no figure named {args.only!r}")

    if args.list:
        for figure in selected:
            print(f"{figure['name']:34} line {figure['line']:5}  {figure['subsection']}")
        print(f"\n{len(selected)} figures")
        return

    if not shutil.which("tectonic"):
        sys.exit("tectonic is not on PATH")

    OUT.mkdir(parents=True, exist_ok=True)
    written = []
    with tempfile.TemporaryDirectory(prefix="fsbook-tikz-") as tmp:
        workdir = Path(tmp)
        for figure in selected:
            variants = [False] if args.no_dark else [False, True]
            for dark in variants:
                target = build(figure, packages, colors, styles, dark, workdir)
                written.append(target)
                print(f"  {target.name} ({target.stat().st_size // 1024} KB)")

    manifest = ["# Generated by book/tools/tikz_to_svg.py. Captions are authored here.\n"]
    for figure in found:
        source = toml_string(figure["source"])
        name = toml_string(figure["name"])
        caption = toml_string(figure["caption"])
        manifest.append(
            f'[[figure]]\nsource = "{source}"\nname = "{name}"\n'
            f'source_line = {figure["line"]}\ncaption = "{caption}"\n'
        )
    expected = {
        f'{figure["name"]}{suffix}.svg'
        for figure in found
        for suffix in ("", ".dark")
    }
    for stale in OUT.glob("*.svg"):
        if stale.name not in expected:
            stale.unlink()
    NAMES.write_text("\n".join(manifest), encoding="utf-8")

    print(f"\n{len(written)} files -> {OUT}")


if __name__ == "__main__":
    main()
