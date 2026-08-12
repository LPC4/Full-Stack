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

Image alt text becomes the visible caption. A matching `.dark.svg` file is selected in dark mode:

```markdown
![Loads move data between RAM and registers.](figures/load_store_data_flow.svg)
```

Regenerate committed figures after changing `guide/figures/figures.tex`:

```sh
python guide/tools/tikz_to_svg.py
```

This requires `tectonic` and PyMuPDF.

## Layout

```text
src/          Markdown sources and generated page summary
generator/    Static-site generator and preview server
wasm/         In-page assembly and HLL sandbox
theme/        Guide styles and browser behavior
figures/      Generated light and dark SVGs
tools/        Figure-generation tools
```
