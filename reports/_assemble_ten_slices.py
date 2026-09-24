#!/usr/bin/env python3
"""Assemble reports/2026-09-24-ten-slices.html — Gates 3 and 4 of the GUI-in-the-TUI plan.

One self-contained file: the decision-brief skill's base.css, notes.css and notes.js
inlined beside this page's own styles and body. The Gate 1 mockups under
docs/plans/gui-in-the-tui/mockups/ are embedded unchanged as srcdoc iframes, their
stylesheet inlined, so the page still survives being emailed as one file. A body
placeholder {{mock:<name>:<height>}} becomes one iframe. Re-run after editing.
"""
import html
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
SKILL = Path.home() / ".claude/skills/decision-brief"
MOCKS = HERE.parent / "docs/plans/gui-in-the-tui/mockups"
OUT = HERE / "2026-09-24-ten-slices.html"

base_css = (SKILL / "assets/base.css").read_text()
notes_css = (SKILL / "assets/notes.css").read_text()
notes_js = (SKILL / "assets/notes.js").read_text()
notes_markup = (SKILL / "reference/notes-markup.html").read_text()
brief_css = (HERE / "_documents_in_the_pane_style.css").read_text()
body = (HERE / "_ten_slices_body.html").read_text()
mock_css = (MOCKS / "td-mock.css").read_text()

assert "</script" not in notes_js.lower(), "notes.js holds a closing script tag"
assert "</script" not in body.lower(), "the body holds a script tag; keep behaviour in dialog_js"


def mock_iframe(name: str, height: str) -> str:
    src = (MOCKS / f"{name}.html").read_text()
    src = src.replace('<link rel="stylesheet" href="td-mock.css">', f"<style>{mock_css}</style>")
    src = re.sub(r'<h1 class="mock">.*?</h1>', "", src, flags=re.S)
    src = re.sub(r'<p class="say">.*?</p>', "", src, flags=re.S)
    assert "<script" not in src.lower(), f"{name} carries a script"
    return (f'<iframe class="mock" style="height:{height}px" title="{html.escape(name)}" '
            f'srcdoc="{html.escape(src, quote=True)}"></iframe>')


body = re.sub(r"\{\{mock:([\w-]+):(\d+)\}\}", lambda m: mock_iframe(m.group(1), m.group(2)), body)
assert "{{mock:" not in body

notes_markup = notes_markup.replace("<YYYY-MM-DD>-<topic-slug>.html", OUT.name)
notes_markup = notes_markup.replace(
    "<!-- then paste the contents of assets/notes.js inline, inside a <script> tag -->",
    "<script>\n" + notes_js + "\n</script>",
)

dialog_js = """
(function () {
  document.querySelectorAll('[data-dlg]').forEach(function (b) {
    b.addEventListener('click', function () {
      var d = document.getElementById(b.dataset.dlg);
      if (d && typeof d.showModal === 'function') d.showModal();
    });
  });
  document.querySelectorAll('dialog.sub').forEach(function (d) {
    d.querySelectorAll('[data-close]').forEach(function (x) {
      x.addEventListener('click', function () { d.close(); });
    });
    d.addEventListener('click', function (e) { if (e.target === d) d.close(); });
  });
})();
"""

page = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Ten Slices</title>
<style>
{base_css}
{notes_css}
{brief_css}
</style>
</head>
<body>
{body}
{notes_markup}
<script>{dialog_js}</script>
</body>
</html>
"""
OUT.write_text(page)
print(OUT, len(page), "bytes")
