# Full-Stack Guide

The Guide is generated from Markdown in `guide/src/`. It is published beside the browser IDE
and uses the same syntax highlighter and virtual machine.

## Preview and build

Preview the Guide at `http://localhost:8000/guide/`:

```sh
cargo run -p full_stack_guide -- serve --root guide
```

The preview server rebuilds changed Guide pages. Add `--port N` to change the port,
`--no-sandbox` to skip WASM checks, or `--rebuild-sandbox` to force a sandbox rebuild.

Build the site without serving it:

```sh
cargo run -p full_stack_guide -- build --root guide --out dist
```

The build writes the Guide to `dist/guide/` and repository sources to `dist/_source/` without
removing the IDE files already in `dist/`. Add `--include-worktree` to include untracked,
nonignored files.

## Authoring

Sources are listed in `guide/guide.toml`. In the first source, `##` starts a chapter and `###`
starts a page. Deeper headings form that page's outline. Later sources become their own chapters.

Generated routes are lowercase directories with trailing slashes:

```text
/guide/
/guide/cpus-and-assembly/
/guide/cpus-and-assembly/registers/
```

Use `repo:` links for files in the repository:

```markdown
[bus implementation](repo:crates/virtual-machine/src/bus.rs)
[UART base address](repo:crates/virtual-machine/src/bus.rs#L48)
```

Links to `.s`, `.ir`, and `.hll` files open in the IDE. Other repository files open on GitHub.
Inline code labels in repository links wrap long paths to fit narrow screens.
The build rejects missing files, invalid paths, and invalid line fragments.

Include a complete repository file without duplicating it in the Guide source:

```text
{{#include programs/lessons/01_sum/sum.s}}
```

The directive must occupy its own line. Paths are repository-relative, and absolute or traversing
paths are rejected. Put the directive inside a code fence when the included file is source code.

Code fences tagged `asm`, `hll`, `ir`, `ast`, `sh`, `ld`, or `x86` use the shared highlighter.
Add `runnable` to run an assembly fragment or an HLL program with `main` on the page:

````markdown
```asm,runnable expect=t0:55|a0:0
li t0, 55
```

```hll,runnable registers=false
main: () -> i32 {
	return 0
}
```
````

Use `runnable=bare` only for a complete assembly program. `registers=false` hides the register
dump when it is not useful.

Markdown image descriptions provide the diagram's accessible alternative and native fallback
text when an image cannot load. They are not displayed as captions below loaded diagrams.
A matching `.dark.svg` file is selected in dark mode:

```markdown
![Loads move data between RAM and registers.](figures/load_store_data_flow.svg)
```

Edit individual Excalidraw scenes under `guide/figures/scenes/`. Follow the drawing conventions,
palette, and review checklist in [the figure authoring guide](figures/README.md).
Use Markdown tables for simple comparisons. Diagrams should show a relationship through
position, boundaries, or arrows. Keep labels concise and explanations in surrounding prose.
Save matching light/navy masters and export both SVGs after changing a scene. Use square storage
shapes, outlined triangle arrowheads, and dashed sides for gaps. Follow the figure specification
when setting export dimensions and checking the result in the Guide.

The TikZ sources under `guide/figures/source/` remain technical references. The empty legacy
manifest prevents the old generator from replacing the manual exports.

Figures retain a universal 1.9 CSS pixels per SVG/PDF unit, including on phones. A focusable
viewport contains horizontal scrolling. The focused viewport
keeps Left/Right arrow keys for scrolling, including at its edges.
Page-navigation shortcuts apply outside figure viewports. The persistent image
role exposes one alternative in either theme; both native images carry the same fallback text
and are hidden from assistive technology to avoid duplication. Print uses a light page palette
and the light twin, regardless of the selected screen theme, and fits the page. Printed code
uses the text color so screen-theme syntax colors remain readable on white.

## Layout

```text
src/          Markdown sources and generated page summary
generator/    Static-site generator and preview server
wasm/         In-page assembly and HLL sandbox
theme/        Guide styles and browser behavior
figures/      Editable scenes and light/navy SVG exports
tools/        Legacy TikZ figure tools
```
