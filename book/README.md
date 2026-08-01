# fsbook

**The learning frontend for Full-Stack.**

A static book generated from `src/written.md` and published beside the browser IDE.
It teaches the system from machine instructions upward, using the same syntax
highlighter and virtual machine as the application.

## Flow

```
written.md + book.toml + figures
  -> fsbook
  -> static HTML, CSS, and SVG pages
  -> optional in-page assembly sandbox through fsbook_wasm
```

Each generated chapter is a complete HTML document, so the book remains readable
without JavaScript. The browser enhancement makes chapter changes immediate while
preserving normal links and history.

## Running

From the repository root:

```sh
cargo run -p fsbook -- serve --root book
```

The book is served at `http://localhost:8000/`. The command rebuilds the pages and
rebuilds the WASM sandbox when its sources have changed.

Build without serving:

```sh
cargo run -p fsbook -- build --root book
```

Use `--out DIR` to select an output directory, `--port N` to change the preview
port, `--no-sandbox` to skip WASM checks, or `--rebuild-sandbox` to force a rebuild.

## Authoring

Write the lesson in `src/written.md`. A `##` heading starts a chapter, `###` starts
a page, and deeper headings form the on-page outline.

Code fences tagged `asm`, `hll`, `ir`, `ast`, `sh`, `ld`, or `x86` use the shared
syntax highlighter. An assembly fence can run in the page:

````markdown
```asm,runnable
li a0, 5
```

```asm,runnable=bare
.globl _start
```
````

Use ordinary Markdown image syntax for figures. The alt text becomes the accessible
caption, and a matching `name.dark.svg` is selected by the dark theme.

```markdown
![Loads move data between RAM and registers.](figures/load_store_data_flow.svg)
```

Regenerate the committed SVGs after changing the TikZ source:

```sh
python book/tools/tikz_to_svg.py --list
python book/tools/tikz_to_svg.py
```

This requires `tectonic` and PyMuPDF.

## Module layout

```
src/          authored Markdown and generated page summary
generator/    fsbook static-site generator and local preview server
wasm/         fsbook_wasm assembly sandbox
theme/        book CSS and progressive browser behavior
figures/      generated light and dark SVG assets plus metadata
tools/        figure-generation tooling
```

The GitHub Pages workflow builds the book into `dist/book`, alongside the IDE.
