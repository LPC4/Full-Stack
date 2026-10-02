# Guide figures

Every teaching diagram has a stable ID, a small TikZ source, and committed light/navy SVGs.
Readers and the site build need no Python, fonts, or LaTeX. The RISC-V logo is an identity
asset and is not regenerated.

## Make a figure

1. Copy the closest family example in `source/`:
   - Fields: `how_the_cpu_reads_instructions_3.tex`.
   - Memory cells: `the_frame_pointer_2.tex`.
   - Signal paths: `load_store_data_flow.tex`.
   - Before/after: `heap_fragmentation.tex`.
   - Timelines: `pipeline_stages.tex`.
2. Choose one teaching claim. Check numbers against the governing specification and code.
3. Add an entry to `figures.toml`:

   ```toml
   [[figure]]
   id = "my_diagram"
   source = "source/my_diagram.tex"
   ```

4. Use shared styles from `source/styles.tex` and paint roles from `palette.toml`.
5. Generate and inspect both variants:

   ```sh
   python guide/tools/tikz_to_svg.py --only my_diagram
   python guide/tools/tikz_to_svg.py --report
   ```

6. Add a useful Markdown image description for accessibility and failed-image fallback:

   ```markdown
   ![Explain what the reader should learn.](figures/my_diagram.svg)
   ```

Review on the actual page with neighboring prose, in both themes, on desktop and at 320px.
Inspect arrows, arithmetic, type, and keyboard scrolling. Update the description when the claim
changes. Keep essential numerical explanations in surrounding prose as well as the drawing.

## Drawing vocabulary

The design uses precise computer architecture notation with quiet supporting chrome.
Drawings sit directly on the canvas. Reserve boxes for storage, fields, states, and meaningful
groups. Use native Markdown tables for comparisons and aligned cells for before/after examples.
A diagram should add spatial meaning: flow, transformation, boundaries, or correspondence.

| Primitive | Purpose |
| --- | --- |
| `panel` | Neutral object with consistent padding and a strong boundary |
| `focus` | Current object, using the primary accent |
| `category=<role>` | Labeled object with a semantic fill and boundary |
| `region=<role>` | Square memory cells and instruction fields |
| `pathof=<role>` | Arrow that preserves the identity of its source |
| `unused` | Dashed empty space, explicitly labeled as mapped or conceptual |
| `signal` | Directed data or execution path |
| `guideedge` | Secondary directed relationship |
| `literal` | Numbers, addresses, registers, and instruction text |
| `note` | Supporting explanation with a controlled text width |

Use `ink` for text, `muted` for supporting text, `stroke` for essential boundaries, `surface`
for neutral regions, and `accent` for focus. `data`, `operand`, `caution`, `danger`, `violet`,
and `teal` identify categories. Use these conventions across related figures:

| Role | Meaning |
| --- | --- |
| `accent` | Code, execution paths, instruction destination register |
| `data` | Data sections, local values, instruction opcode |
| `operand` | Instruction source registers |
| `violet` | Stack storage and saved registers |
| `teal` | Heap storage and device interfaces |
| `caution` | Signed immediate constraints and pending work |
| `danger` | Invalid values, discarded work, or explicitly missing features |
| `difference` | Orange accent for changed machine interfaces |
| `stroke` / `surface` | Padding, neutral structure, and unused storage |

Stack-pointer and frame-pointer arrows use `violet`, matching their storage; `teal` remains
for heap and device paths. Use neutral panels for alternative interpretations of the same
value, with the shared source highlighted once. Device diagrams distinguish instruction,
operand, and device roles instead of tinting the entire flow as a device.

Assembly snippets use shared `\asmop`, `\asmreg`, `\asmimm`, and `\asmlabel` helpers:
blue mnemonics, pink registers, amber immediates, and purple symbols. Keep syntax coloring
consistent even inside instruction encodings; field fills independently identify bit roles.
The machine comparison uses labeled green/same, orange/different, and red/missing states.

Center standalone headings and footers on the complete drawing, including side annotations.
Use a `local bounding box=diagram` scope for the body, then place centered text at
`diagram.north` or `diagram.south` with explicit clearance. Keep component headings centered
on their own component, and preserve intentional side labels and byte-end annotations.

Byte-order and allocation diagrams may assign colors to individual identities. Preserve each
identity across transformations and label it explicitly. Never highlight an object merely
because it is the first row or stage. Equal stages receive equal visual weight. Use arrowheads
at exact boundaries, keep paths clear of labels, and label dashed reference relationships.
Use `border` only for decorative dividers. Terminal foreground/background are explicitly
paired as `terminalink` and `terminal` in both themes.

Tint fills explicitly toward `paper`, such as `accent!8!paper`. Do not mix implicitly toward
white or use theme-specific body geometry. Both variants compile the same body and styles;
only the palette changes. There are no dark-theme source substitutions.

Source Sans Pro carries explanations; Source Code Pro carries literals. `\figlabel` is
8.5 TeX points with 11-point leading. At the universal 1.9 PDF-unit-to-CSS-pixel conversion,
sans labels are about 16.1px and the 0.88-scale mono face about 14.2px. Do not use smaller
fonts, local scaling, or `transform shape`. Essential strokes are 0.75-0.9 points
(about 1.4-1.7px). Use about 0.15cm clearance from label edges and more near arrowheads.

Aim for a drawing below 700 CSS pixels. Split explanations or use a vertical flow before
making a wider plate. On narrow screens, drawings retain their scale in a keyboard-focusable
scroll viewport. Keep concise labels inside the drawing and extended teaching explanations
in surrounding Guide prose. Do not add a caption repeating those labels. Both twins reserve
identical intrinsic dimensions. A persistent image role exposes the Markdown alternative in either theme. Each native image also carries
that description as fallback text, but is hidden from assistive technology to avoid duplication.

Memory diagrams must state address direction and address space. Mark conceptual layouts
and omissions as not to scale. Bit fields need ranges and widths; timelines need a time
direction and explicit stalls or discarded work. Dashed reference paths need a legend.
Do not use a plausible-looking arrow in place of checking the actual control flow.

## Toolchain and commands

Tested toolchain: Python 3.10.11, Tectonic 0.16.9, PyMuPDF 1.28.0 / MuPDF 1.29.0.
Install Python dependencies with:

```sh
python -m pip install -r guide/tools/requirements-figures.txt
python guide/tools/tikz_to_svg.py --list
python guide/tools/tikz_to_svg.py
python -m unittest discover -s guide/tools -p test_tikz_to_svg.py
```

Tectonic must be on PATH. Its first run downloads TeX packages and the Source Sans Pro /
Source Code Pro fonts into its bundle cache; this is not an offline bootstrap. Keep the
same Tectonic bundle/cache when comparing byte-for-byte output. Package/font versions are
recorded in local review evidence after rendering. Outlined SVG labels avoid browser font
dependencies. Python 3.11+ uses `tomllib`; older supported Python uses `tomli`.

`--list` and `--report` never render. `--only` accepts a stable ID, not an old section heading.
Public SVG names are preserved. Metadata is authored and is never rewritten by generation.
The old `--no-dark` switch is removed: a generation always produces a complete pair.

## Installation and recovery

All selected outputs render into an ignored staging directory under `guide/out/`. Four
independent compiler jobs run concurrently. Every SVG must parse, have positive finite
dimensions and a viewBox, and contain no scripts, event handlers, embedded images, text-font
dependencies, or external references. Twin dimensions and viewBoxes must match. Only after
the whole selection validates does installation begin. Unselected files are never pruned.

The installer saves previous files and a `journal.json` in `guide/out/figure-recovery-*`.
On an installation error it restores replaced files and removes newly installed files.
This is recoverable batch installation, not filesystem-wide atomicity. If the process or
machine is forcibly stopped, the recovery directory remains: restore each name marked
`true` in the journal from its backup, and remove each destination marked `false`. Keep the
journal until restoration finishes. Never run two generators against the same output set
at once. Successful installation or rollback removes its own recovery directory.

Retirement is an explicit edit: audit all source, HTML, CSS, and repository references first,
then remove only the named source and pair and update the manifest and review record.

## Review artifacts

`--report` creates `guide/out/figure-review/contact-sheet.html` and a JSON size report.
Open the HTML locally for actual-size light/navy previews, including unpublished designs.
Review screenshots belong in this ignored directory, not among site assets.
Use `serve --root guide --out guide/out/preview --include-worktree` for full previews during
figure work. Trunk replaces its output directory, so serving into `guide/out` can remove review
artifacts and recovery directories stored beneath it.
The historical implementation record is in `.ai/current_plan/guide_figures/archive/REVIEW.md`.

Plain comparisons retired from unpublished designs are in [reference-tables.md](reference-tables.md).
[diagram-notes.md](diagram-notes.md) retains context for unpublished diagrams.
