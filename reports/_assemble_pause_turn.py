#!/usr/bin/env python3
"""Assemble the pausing-a-turn brief into one self-contained file.

Everything is inlined — base.css, notes.css, notes.js, the brief's own style and body —
because a brief has to survive being emailed as one file. Nothing loads from a network
or from a sibling path.
"""
import pathlib
import sys

SKILL = pathlib.Path("/home/parker/.claude/skills/decision-brief")
HERE = pathlib.Path(__file__).parent
OUT = HERE / "2026-09-21-pausing-a-turn.html"
NOTES_FILE = OUT.name


def read(p: pathlib.Path) -> str:
    return p.read_text(encoding="utf-8")


base_css = read(SKILL / "assets" / "base.css")
notes_css = read(SKILL / "assets" / "notes.css")
notes_js = read(SKILL / "assets" / "notes.js")
own_css = read(HERE / "_pause_turn_style.css")
body = read(HERE / "_pause_turn_body.html")
markup = read(SKILL / "reference" / "notes-markup.html")

# The trap the skill warns about: a literal closing script tag anywhere inside the
# inlined JS ends the <script> element early and silently turns the rest into page
# content. Refuse to assemble rather than ship a brief whose notes never run.
for name, blob in (("notes.js", notes_js), ("body", body)):
    if "</script" in blob.lower() and name == "notes.js":
        sys.exit(f"REFUSING: {name} contains a literal closing script tag")

# The markup block carries its own commentary and a placeholder filename. Keep only the
# elements, and bind the filename to this brief.
start = markup.index('<div class="notebar"')
end = markup.index("<!-- then paste the contents")
markup = markup[start:end].replace(
    "window.NOTES_FILE = '<YYYY-MM-DD>-<topic-slug>.html';",
    f"window.NOTES_FILE = '{NOTES_FILE}';",
)

html = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Pausing a turn &middot; terminal-delight</title>
<style>
{base_css}
{notes_css}
{own_css}
</style>
</head>
<body>
{body}

{markup}
<script>
{notes_js}
</script>
<script>
document.querySelectorAll('[data-dlg]').forEach(function (b) {{
  b.addEventListener('click', function () {{
    var d = document.getElementById(b.dataset.dlg);
    if (d) d.showModal();
  }});
}});
document.querySelectorAll('[data-close]').forEach(function (b) {{
  b.addEventListener('click', function () {{
    var d = b.closest('dialog');
    if (d) d.close();
  }});
}});
</script>
</body>
</html>
"""

OUT.write_text(html, encoding="utf-8")
print(f"{OUT}  ({len(html):,} bytes)")
