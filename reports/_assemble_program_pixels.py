#!/usr/bin/env python3
"""Assemble reports/2026-09-24-program-pixels.html from its parts.

The body and this brief's styles live beside this script. The notes system
(base.css, notes.css, notes.js and the markup block) is read from the
decision-brief skill every time, never copied into the repo, so the brief
always carries the skill's current notes format.

    python3 reports/_assemble_program_pixels.py
"""
from pathlib import Path
import re
import sys

HERE = Path(__file__).resolve().parent
SKILL = Path.home() / ".claude/skills/decision-brief"
OUT = HERE / "2026-09-24-program-pixels.html"
NAME = OUT.name

base = (SKILL / "assets/base.css").read_text()
notes_css = (SKILL / "assets/notes.css").read_text()
notes_js = (SKILL / "assets/notes.js").read_text()
markup = (SKILL / "reference/notes-markup.html").read_text()
css = (HERE / "_program_pixels.css").read_text()
body = (HERE / "_program_pixels_body.html").read_text()

# An HTML parser ends a <script> at the first closing tag it meets, even
# inside a comment or a string, and everything after it becomes page text.
if re.search(r"</script", notes_js, re.I):
    sys.exit("notes.js contains a closing script tag; inlining it would break the page")

markup = markup.replace("<YYYY-MM-DD>-<topic-slug>.html", NAME)
markup = markup.replace(
    "<!-- then paste the contents of assets/notes.js inline, inside a <script> tag -->",
    "<script>\n" + notes_js + "\n</script>",
)
if NAME not in markup or notes_js[:40] not in markup:
    sys.exit("the notes markup block changed shape; update this assembler")

dialogs = """<script>
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
</script>"""

page = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Program Pixels</title>
<style>
{base}
{notes_css}
{css}
</style>
</head>
<body>
{body}
{markup}
{dialogs}
</body>
</html>
"""
OUT.write_text(page)
print(f"wrote {OUT} ({len(page):,} bytes)")
