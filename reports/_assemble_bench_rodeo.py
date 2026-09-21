#!/usr/bin/env python3
"""Assemble the bench-rodeo brief.

Body, local styles and the skill's own assets, inlined into one file — because a brief
has to survive being emailed, and an external `src` is a picture that vanishes on the
reader's machine.

Re-run it after editing `_bench_rodeo_body.html`; notes live in the browser and in the
JSON island, and the island is re-read on load, so a rebuild does not lose them.
"""
from pathlib import Path

SKILL = Path("/home/parker/.claude/skills/decision-brief")
HERE = Path(__file__).parent
NAME = "2026-09-19-the-bench-acts-on-your-click.html"
OUT = HERE / NAME

body = (HERE / "_bench_rodeo_body.html").read_text()
local_css = (HERE / "_bench_rodeo_style.css").read_text()
base_css = (SKILL / "assets/base.css").read_text()
notes_css = (SKILL / "assets/notes.css").read_text()
notes_js = (SKILL / "assets/notes.js").read_text()

# The inlining trap: a parser ends a <script> at the first literal closing tag it sees,
# including one inside a comment or a string — and everything after it silently becomes
# page content, with no console error.
if "</script" in notes_js.lower():
    raise SystemExit("notes.js contains a literal closing script tag; escape it before inlining")

markup = (SKILL / "reference/notes-markup.html").read_text()
start = markup.index('<div class="notebar"')
notes_markup = markup[start:markup.index("<script>", start)].rstrip()

# Modals are opened by anything carrying data-dlg, so a card and its button both work.
dialog_js = """
document.addEventListener('click', function (ev) {
  var t = ev.target.closest('[data-dlg]');
  if (t) {
    var d = document.getElementById(t.dataset.dlg);
    if (d && !d.open) { d.showModal(); return; }
  }
  if (ev.target.closest('[data-close]')) {
    var open = ev.target.closest('dialog');
    if (open) open.close();
  }
});
"""

OUT.write_text(f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>The bench acts on your click — five workbench defects, terminal-delight</title>
<style>
{base_css}
{notes_css}
{local_css}
</style>
</head>
<body>
{body}

{notes_markup}

<script>window.NOTES_FILE = '{NAME}';</script>
<script>
{notes_js}
</script>
<script>
{dialog_js}
</script>
</body>
</html>
""")
print(f"wrote {OUT} ({OUT.stat().st_size // 1024} KB)")
