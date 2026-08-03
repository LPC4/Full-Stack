# Full-Stack Guide

The learning frontend for Full-Stack is a static Guide generated from the Markdown sources in
`guide.toml` and published beside the browser IDE. It uses the same syntax highlighter and
virtual machine as the application.

## Flow

```text
Markdown sources + guide.toml + figures
  -> full_stack_guide
  -> clean directory-routed HTML, CSS, SVG, and repository source assets
  -> optional in-page assembly sandbox through full_stack_guide_wasm
```

Each generated page is a complete HTML document, so the Guide remains readable without
JavaScript. The browser enhancement makes page changes immediate while preserving ordinary
links and history.

## Running

From the repository root:

```sh
cargo run -p full_stack_guide -- serve --root guide
```

The Guide is served at `http://localhost:8000/guide/`. The command rebuilds the pages and keeps
a release build of the browser IDE beside them so source permalinks work locally. The browser IDE
and the WASM sandbox are rebuilt only when their source inputs have changed, so editing Guide
Markdown reuses both existing WASM bundles.

The preview server supports `GET` and `HEAD`, redirects directory resources to their trailing-slash
canonical route, and returns `Allow: GET, HEAD` for unsupported methods. Production response and
cache headers remain controlled by GitHub Pages.

Build without serving:

```sh
cargo run -p full_stack_guide -- build --root guide --out dist
```

`--out` names the overall site root. The generator replaces its owned `guide/` and `_source/`
children and publishes the root icon assets, leaving Trunk's other IDE files in place.
Production catalogs Git-tracked regular files. `--include-worktree` additionally
includes nonignored worktree additions and skips tracked paths removed from disk, which is
useful while testing an unstaged rename.

Use `--port N` to change the preview port, `--no-sandbox` to skip WASM checks, or
`--rebuild-sandbox` to force a rebuild.

## Routes and repository links

Guide pages use lowercase directory routes with trailing slashes:

```text
/guide/
/guide/cpus-and-assembly/
/guide/cpus-and-assembly/registers/
```

Link to a Git-tracked repository file with the explicit `repo:` destination. Add a one-based
line fragment when the link should jump to a particular line:

```markdown
[bus implementation](repo:crates/virtual-machine/src/bus.rs)
[UART base address](repo:crates/virtual-machine/src/bus.rs#L48)
```

The build rejects malformed paths, invalid line fragments, and missing files. Only `.s`, `.ir`,
and `.hll` repository links open in the IDE. Valid links to bundled lesson, runtime, and example
sources open editable files in their matching IDE workspace; other source files open as read-only
references. The matching `.build` run configuration is selected when one exists. Every other
repository file, including Rust VM code and Markdown documentation, opens its `blob/main` page on
GitHub. Ordinary Guide-local, fragment, figure, and external links retain their normal Markdown
meaning.

## Authoring

List authored sources as `[[parts]]` in `guide.toml`. The first source supplies the landing page
and its `##` chapters. Each later source is appended as a top-level chapter named by its part
title. Within every source, `###` starts a page and deeper headings form the on-page outline.

Code fences tagged `asm`, `hll`, `ir`, `ast`, `sh`, `ld`, or `x86` use the shared syntax
highlighter. Use `runnable` for an assembly fragment that should receive an entry point and clean
halt automatically, or for a standalone HLL program with a `main` function. Use `runnable=bare`
only for a complete assembly program:

````markdown
```asm,runnable
li a0, 5
```

```asm,runnable=bare
.globl _start
```

```hll,runnable
main: () -> i32 {
    return 5
}
```
````

Expected register values are optional and remain visible after the reader edits a snippet:

````markdown
```asm,runnable expect=t0:55|a0:0
```
````

Runnable editors highlight changes as they are typed, wrap long lines, and grow with their source
instead of adding an internal scrollbar. Tab and Shift+Tab indent or unindent with two spaces while
keeping focus in the editor.

Use ordinary Markdown image syntax for figures. The alt text becomes the accessible caption,
and a matching `name.dark.svg` is selected by the dark theme.

```markdown
![Loads move data between RAM and registers.](figures/load_store_data_flow.svg)
```

Regenerate the committed SVGs after changing the TikZ source:

```sh
python guide/tools/tikz_to_svg.py --list
python guide/tools/tikz_to_svg.py
```

This requires `tectonic` and PyMuPDF.

## Module layout

```text
src/          authored Markdown and generated page summary
generator/    full_stack_guide static-site generator and preview server
wasm/         full_stack_guide_wasm assembly sandbox
theme/        Guide CSS and progressive browser behavior
figures/      generated light and dark SVG assets plus metadata
tools/        figure-generation tooling
```

The Pages workflow builds the IDE into `dist/`, then adds `dist/guide/` and `dist/_source/`
before deploying the combined artifact.
