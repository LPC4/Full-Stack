# Guide figures

Teaching figures use editable Excalidraw scenes and committed light/navy SVG exports.
All 48 editable drawings live in `scenes/`, with one light and one navy master per figure.
The original hand-drawn references are `scenes/load_store_data_flow.excalidraw` and
`scenes/the_stack_and_the_heap.excalidraw`. The other figures share their handwritten labels,
colored hatching, square storage shapes, and outlined triangle arrowheads. Gaps retain their
two dashed sides. There are no separate collection copies to sync.

See [_FIGURE_SPECIFICATION.md](_FIGURE_SPECIFICATION.md) for drawing conventions, export
settings, light/navy colors, exact dimensions, page placement, and acceptance checks.
The Guide still applies a 1.9 sizing factor for compatibility with the original TikZ SVGs.
Exports set root dimensions to native size times `0.75 / 1.9`, giving a 75% display size.
Their viewBoxes and editable scene geometry remain unchanged.

Stacks, memory columns, and vertical flows use named export frames centered on their main
body. Export those frames so side arrows and labels do not shift the visual center. Preserve
the balanced whitespace around the body when editing or re-exporting.

Keep labels to a word or a short phrase. Put explanations in `guide/src/written.md`, or in
`diagram-notes.md` for unpublished figures. Keep the numbers and code needed to read each drawing.

## Replace or add a drawing

1. Edit the individual scene in Excalidraw and save it.
2. Apply the same geometry changes to the navy scene.
3. Export both SVGs, adjust root dimensions as specified, and replace `<id>.svg` and
   `<id>.dark.svg` in this directory.
4. Reference only the light filename from `guide/src/written.md`; the Guide selects the
   matching dark twin automatically.
5. Preview at desktop and phone widths in both themes, and inspect print output.

The site only copies SVG/PNG assets. It does not need Excalidraw, Node, Python, or LaTeX.
The RISC-V logo is a separate identity asset.

## Retained TikZ references

The `source/*.tex` files remain as technical references. All 48 installed SVG pairs now come
from the individual Excalidraw masters in `scenes/`. The legacy `figures.toml` contains an empty list so the old generator
cannot overwrite the new assets. Its historical colors and styles remain in `palette.toml`
and `source/styles.tex`; it does not process Excalidraw files.

[diagram-notes.md](diagram-notes.md) describes unpublished diagrams.
[reference-tables.md](reference-tables.md) retains comparisons better expressed as tables.
