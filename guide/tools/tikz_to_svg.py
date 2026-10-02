"""Build paired Guide SVGs from explicit, independently editable TikZ sources."""

import argparse
import json
import math
import os
import re
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "guide/figures"
REVIEW = ROOT / "guide/out/figure-review"
ID = re.compile(r"[a-z][a-z0-9_]*\Z")


def read_manifest(root=OUT):
    with (root / "figures.toml").open("rb") as stream:
        data = tomllib.load(stream)
    if set(data) != {"figure"} or not isinstance(data["figure"], list):
        raise ValueError("figures.toml: expected [[figure]] entries")
    ids, sources, figures = set(), set(), []
    for entry in data["figure"]:
        if set(entry) != {"id", "source"}:
            raise ValueError(f"figure requires only id and source: {entry}")
        name, source = entry["id"], entry["source"]
        if not isinstance(name, str) or not ID.fullmatch(name):
            raise ValueError(f"invalid figure ID: {name!r}")
        if name.casefold() in ids:
            raise ValueError(f"duplicate output ID: {name}")
        if not isinstance(source, str) or not re.fullmatch(r"source/[a-z][a-z0-9_]*\.tex", source):
            raise ValueError(f"{name}: source must be source/<stable-id>.tex")
        path = (root / source).resolve()
        if not path.is_relative_to(root.resolve()) or not path.is_file():
            raise ValueError(f"{name}: missing or unsafe source {source}")
        if path in sources:
            raise ValueError(f"{name}: source already owned: {source}")
        ids.add(name.casefold())
        sources.add(path)
        figures.append(dict(id=name, source=source))
    return figures


def select(figures, only):
    if only is None:
        return figures
    selected = [figure for figure in figures if figure["id"] == only]
    if not selected:
        raise ValueError(f"unknown figure ID {only!r}; use --list")
    return selected


def palette(theme, root=OUT):
    with (root / "palette.toml").open("rb") as stream:
        roles = tomllib.load(stream)
    colors = roles[theme]
    if set(colors) != set(roles["light"]) or set(colors) != set(roles["navy"]):
        raise ValueError("palette themes must define the same roles")
    for name, value in colors.items():
        if not re.fullmatch(r"[a-z][a-z0-9]*", name) or not re.fullmatch(r"[0-9A-F]{6}", value):
            raise ValueError(f"invalid palette role {name}={value}")
    return "\n".join(rf"\definecolor{{{name}}}{{HTML}}{{{value}}}" for name, value in colors.items())


def document(figure, theme, root=OUT):
    return "\n".join([
        r"\documentclass[border=8pt]{standalone}",
        r"\usepackage[table]{xcolor}", palette(theme, root),
        (root / "source/styles.tex").read_text(encoding="utf-8"),
        r"\color{ink}\begin{document}",
        (root / figure["source"]).read_text(encoding="utf-8"), r"\end{document}",
    ])


def validate_svg(path):
    svg = ET.parse(path).getroot()
    if svg.tag != "{http://www.w3.org/2000/svg}svg":
        raise ValueError(f"{path.name}: not an SVG")
    dimensions = tuple(float(svg.attrib[key]) for key in ("width", "height"))
    box = tuple(float(value) for value in svg.attrib["viewBox"].split())
    if len(box) != 4 or not all(math.isfinite(v) for v in (*box, *dimensions)):
        raise ValueError(f"{path.name}: invalid dimensions")
    if min(*dimensions, *box[2:]) <= 0:
        raise ValueError(f"{path.name}: nonpositive dimensions")
    for node in svg.iter():
        tag = node.tag.rsplit("}", 1)[-1]
        if tag in {"script", "foreignObject", "image", "text", "style", "animate", "set"}:
            raise ValueError(f"{path.name}: forbidden SVG element {tag}")
        for key, value in node.attrib.items():
            local = key.rsplit("}", 1)[-1]
            if local.lower().startswith("on"):
                raise ValueError(f"{path.name}: event handler")
            if local == "href" and not value.startswith("#"):
                raise ValueError(f"{path.name}: external reference")
            if "url(" in value and not re.fullmatch(r"url\(#[A-Za-z0-9_-]+\)", value):
                raise ValueError(f"{path.name}: external paint reference")
    return dimensions, box


def geometry(path):
    return [(node.tag, {key: value for key, value in node.attrib.items()
                       if key not in {"fill", "stroke"}})
            for node in ET.parse(path).getroot().iter()]


def render(figure, theme, workdir, root=OUT):
    import fitz

    suffix = ".dark" if theme == "navy" else ""
    tex = workdir / f"{figure['id']}{suffix}.tex"
    tex.write_text(document(figure, theme, root), encoding="utf-8")
    process = subprocess.run(
        ["tectonic", "-X", "compile", "--outdir", str(workdir), "--keep-logs", str(tex)],
        capture_output=True, text=True,
    )
    if process.returncode:
        raise RuntimeError(f"{tex.name}:\n" + "\n".join(process.stderr.splitlines()[-16:]))
    with fitz.open(tex.with_suffix(".pdf")) as pdf:
        if len(pdf) != 1:
            raise ValueError(f"{tex.name}: expected one page, got {len(pdf)}")
        svg = pdf[0].get_svg_image(text_as_path=True)
    path = tex.with_suffix(".svg")
    path.write_text(svg, encoding="utf-8", newline="\n")
    validate_svg(path)
    return path


def install(staged, destination=OUT, replace=os.replace):
    destination.mkdir(parents=True, exist_ok=True)
    (destination.parent / "out").mkdir(parents=True, exist_ok=True)
    recovery = Path(tempfile.mkdtemp(prefix="figure-recovery-", dir=destination.parent / "out"))
    previous = {}
    for path in staged:
        target = destination / path.name
        previous[path.name] = target.exists()
        if target.exists():
            shutil.copy2(target, recovery / path.name)
    (recovery / "journal.json").write_text(json.dumps(previous, indent=2))
    changed = []
    try:
        for path in staged:
            target = destination / path.name
            changed.append(target)
            replace(path, target)
    except BaseException:
        for target in reversed(changed):
            if previous[target.name]:
                shutil.copy2(recovery / target.name, target)
            else:
                target.unlink(missing_ok=True)
        shutil.rmtree(recovery)
        raise
    shutil.rmtree(recovery)


def generate(figures, root=OUT, renderer=render):
    (root.parent / "out").mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="figure-stage-", dir=root.parent / "out") as tmp:
        staged = []
        with ThreadPoolExecutor(max_workers=4) as executor:
            pending = [(figure, [executor.submit(renderer, figure, theme, Path(tmp), root)
                                for theme in ("light", "navy")]) for figure in figures]
            for figure, futures in pending:
                pair = [future.result() for future in futures]
                if (validate_svg(pair[0]) != validate_svg(pair[1])
                        or geometry(pair[0]) != geometry(pair[1])):
                    raise ValueError(f"{figure['id']}: twin geometry differs")
                staged.extend(pair)
                print(f"  {figure['id']}: light + navy", flush=True)
        install(staged, root)


def report(figures, root=OUT, output=REVIEW / "contact-sheet.html"):
    from html import escape

    output.parent.mkdir(parents=True, exist_ok=True)
    rows, measurements = [], []
    for figure in figures:
        name = figure["id"]
        images = []
        for suffix in ("", ".dark"):
            path = root / f"{name}{suffix}.svg"
            if not path.exists():
                images.append("<p>Missing pair member</p>")
                continue
            (width, height), _ = validate_svg(path)
            measurements.append(dict(id=name, theme="navy" if suffix else "light",
                                     width=width, height=height, bytes=path.stat().st_size))
            relative = os.path.relpath(path, output.parent).replace(os.sep, "/")
            images.append(f'<div class="viewport {"navy" if suffix else "light"}" tabindex="0">'
                          f'<img alt="{escape(name)}" src="{relative}" '
                          f'width="{round(width * 1.9)}" height="{round(height * 1.9)}"></div>')
        rows.append(f'<section id="{name}"><h2>{name}</h2>{"".join(images)}</section>')
    output.write_text('<!doctype html><meta charset="utf-8"><title>Figure review</title>'
                      '<meta name="viewport" content="width=device-width,initial-scale=1">'
                      '<style>body{font:16px system-ui;margin:24px;background:#f6f7f9}'
                      'h2{font-size:16px}section{margin:32px 0}.viewport{overflow:auto;padding:16px}'
                      '.light{background:white}.navy{background:#0d1117}img{max-width:none}</style>'
                      + "".join(rows), encoding="utf-8")
    output.with_suffix(".json").write_text(json.dumps(measurements, indent=2))
    print(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--list", action="store_true")
    parser.add_argument("--only", help="one stable output ID")
    parser.add_argument("--report", action="store_true", help="inspect pairs without rendering")
    args = parser.parse_args()
    try:
        figures = select(read_manifest(), args.only)
        if args.list:
            for figure in figures:
                print(f"{figure['id']:38} {figure['source']}")
            print(f"{len(figures)} figures")
        elif args.report:
            report(figures)
        else:
            if not shutil.which("tectonic"):
                raise ValueError("tectonic is not on PATH; see guide/figures/README.md")
            generate(figures)
    except (ValueError, KeyError, OSError, RuntimeError, ET.ParseError) as error:
        sys.exit(str(error))


if __name__ == "__main__":
    main()
