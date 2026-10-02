import os
import tempfile
import unittest
from pathlib import Path

import tikz_to_svg as figures

SVG = '<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20" viewBox="0 0 10 20"/>'


class FigureTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "figures"
        (self.root / "source").mkdir(parents=True)
        (self.root / "source/a.tex").write_text("body")
        self.manifest = self.root / "figures.toml"
        self.manifest.write_text('[[figure]]\nid="a"\nsource="source/a.tex"\n')

    def test_selection_and_metadata_are_read_only(self):
        before = self.manifest.read_bytes()
        selected = figures.select(figures.read_manifest(self.root), "a")
        self.assertEqual(selected, [{"id": "a", "source": "source/a.tex"}])
        with self.assertRaisesRegex(ValueError, "unknown"):
            figures.select(selected, "nope")
        self.assertEqual(before, self.manifest.read_bytes())

    def test_rejects_invalid_manifests(self):
        valid = self.manifest.read_text()
        for contents in [valid + valid, valid.replace('"a"', '"../a"'),
                         valid.replace('source/a.tex', '../outside.tex'),
                         valid.replace('source/a.tex', 'source/missing.tex'),
                         valid + 'caption="unowned"\n']:
            with self.subTest(contents=contents):
                self.manifest.write_text(contents)
                with self.assertRaises(ValueError):
                    figures.read_manifest(self.root)

    def test_svg_rejects_unsafe_content_and_invalid_geometry(self):
        path = self.root / "a.svg"
        for content in [SVG.replace('width="10"', 'width="-1"'),
                        SVG.replace('width="10"', 'width="nan"'),
                        SVG.replace('/>', '><script/></svg>'),
                        SVG.replace('/>', '><use href="https://example.test/a"/></svg>'),
                        SVG.replace('/>', '><path onclick="x()"/></svg>')]:
            path.write_text(content)
            with self.assertRaises(ValueError):
                figures.validate_svg(path)
        path.write_text(SVG)
        self.assertEqual(figures.validate_svg(path)[0], (10, 20))

    def test_render_failure_preserves_entire_asset_set(self):
        old = self.root / "a.svg"
        old.write_text("previous")

        def fail_on_dark(figure, theme, workdir, root):
            if theme == "navy":
                raise RuntimeError("render failed")
            path = workdir / "a.svg"
            path.write_text(SVG)
            return path

        with self.assertRaisesRegex(RuntimeError, "render failed"):
            figures.generate(figures.read_manifest(self.root), self.root, fail_on_dark)
        self.assertEqual(old.read_text(), "previous")
        self.assertFalse((self.root / "a.dark.svg").exists())

    def test_failed_install_restores_old_files_and_removes_new_files(self):
        stage = self.root / "stage"
        stage.mkdir()
        paths = [stage / name for name in ["new.svg", "a.svg", "a.dark.svg"]]
        for path in paths:
            path.write_text(SVG)
        (self.root / "a.svg").write_text("previous")
        (self.root / "unrelated.svg").write_text("untouched")

        def fail_last(source, target):
            if target.name == "a.dark.svg":
                raise OSError("injected install failure")
            os.replace(source, target)

        with self.assertRaisesRegex(OSError, "injected"):
            figures.install(paths, self.root, fail_last)
        self.assertEqual((self.root / "a.svg").read_text(), "previous")
        self.assertEqual((self.root / "unrelated.svg").read_text(), "untouched")
        self.assertFalse((self.root / "new.svg").exists())

    def test_invalid_later_pair_preserves_the_entire_batch(self):
        self.manifest.write_text(self.manifest.read_text()
                                 + '[[figure]]\nid="b"\nsource="source/b.tex"\n')
        (self.root / "source/b.tex").write_text("body")
        previous = {name: "previous " + name for name in
                    ["a.svg", "a.dark.svg", "b.svg", "b.dark.svg"]}
        for name, content in previous.items():
            (self.root / name).write_text(content)

        for invalid in [SVG.replace('width="10"', 'width="11"'),
                        SVG.replace('/>', '><path d="M0 0L2 2"/></svg>'),
                        SVG.replace('/>', '><text>font dependency</text></svg>')]:
            with self.subTest(invalid=invalid):
                def render_pair(figure, theme, workdir, root):
                    suffix = ".dark" if theme == "navy" else ""
                    path = workdir / (figure["id"] + suffix + ".svg")
                    path.write_text(invalid if figure["id"] == "b" and suffix else SVG)
                    return path

                with self.assertRaises(ValueError):
                    figures.generate(figures.read_manifest(self.root), self.root, render_pair)
                self.assertEqual({name: (self.root / name).read_text() for name in previous},
                                 previous)
                self.assertEqual(list((self.root.parent / "out").iterdir()), [])

    def test_selective_install_preserves_other_assets(self):
        stage = self.root / "stage"
        stage.mkdir()
        path = stage / "a.svg"
        path.write_text(SVG)
        other = self.root / "b.svg"
        other.write_text("other")
        figures.install([path], self.root)
        self.assertEqual(other.read_text(), "other")
        self.assertEqual((self.root / "a.svg").read_text(), SVG)

    def test_same_bounds_do_not_hide_changed_twin_geometry(self):
        first, second = self.root / "a.svg", self.root / "a.dark.svg"
        first.write_text(SVG.replace('/>', '><path d="M0 0L1 1" fill="#fff"/></svg>'))
        second.write_text(SVG.replace('/>', '><path d="M0 0L2 2" fill="#000"/></svg>'))
        self.assertEqual(figures.validate_svg(first), figures.validate_svg(second))
        self.assertNotEqual(figures.geometry(first), figures.geometry(second))

    def test_palette_has_readable_text_and_essential_lines(self):
        with (figures.OUT / "palette.toml").open("rb") as stream:
            palettes = figures.tomllib.load(stream)

        def rgb(value):
            return [int(value[i:i + 2], 16) / 255 for i in (0, 2, 4)]

        def luminance(color):
            linear = [c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
                      for c in color]
            return sum(c * weight for c, weight in zip(linear, [0.2126, 0.7152, 0.0722]))

        def contrast(first, second):
            a, b = sorted([luminance(first), luminance(second)])
            return (b + 0.05) / (a + 0.05)

        for theme, palette in palettes.items():
            grounds = [rgb(palette[name]) for name in ['paper', 'surface']]
            paper = rgb(palette['paper'])
            for role in ['accent', 'operand', 'data', 'caution', 'violet']:
                grounds.append([a * 0.08 + b * 0.92
                                for a, b in zip(rgb(palette[role]), paper)])
            for ground in grounds:
                for role in ['ink', 'muted']:
                    with self.subTest(theme=theme, role=role, ground=ground):
                        self.assertGreaterEqual(contrast(rgb(palette[role]), ground), 4.5)
                self.assertGreaterEqual(contrast(rgb(palette['stroke']), ground), 3)


if __name__ == "__main__":
    unittest.main()
