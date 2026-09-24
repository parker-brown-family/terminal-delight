#!/usr/bin/env python3
"""Assemble reports/2026-09-24-gui-in-the-tui.html.

One self-contained file: the decision-brief skill's base.css, notes.css and
notes.js inlined beside this brief's own styles and body, with the notes markup
the notes system needs. Re-run after editing the body or the style.
"""
from pathlib import Path

HERE = Path(__file__).resolve().parent
SKILL = Path.home() / ".claude/skills/decision-brief"
OUT = HERE / "2026-09-24-gui-in-the-tui.html"

base_css = (SKILL / "assets/base.css").read_text()
notes_css = (SKILL / "assets/notes.css").read_text()
notes_js = (SKILL / "assets/notes.js").read_text()
notes_markup = (SKILL / "reference/notes-markup.html").read_text()
brief_css = (HERE / "_gui_in_the_tui_style.css").read_text()
body = (HERE / "_gui_in_the_tui_body.html").read_text()

assert "</script" not in notes_js.lower(), "notes.js holds a closing script tag"
assert "</script" not in body.lower(), "the body holds a script tag; keep behaviour in dialog_js"

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

html = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>GUI in the TUI</title>
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
OUT.write_text(html)
print(OUT, len(html), "bytes")
