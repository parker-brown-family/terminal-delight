#!/usr/bin/env python3
"""Assemble the consolidation report: inline every asset so the file survives being emailed."""
from pathlib import Path

SKILL = Path("/home/parker/.claude/skills/decision-brief")
HERE = Path(__file__).parent
OUT = HERE / "2026-09-22-the-consolidation.html"

base_css = (SKILL / "assets/base.css").read_text()
notes_css = (SKILL / "assets/notes.css").read_text()
notes_js = (SKILL / "assets/notes.js").read_text()
local_css = (HERE / "_consolidation_style.css").read_text()
body = (HERE / "_consolidation_body.html").read_text()
markup = (SKILL / "reference/notes-markup.html").read_text()

# The inlining trap: an HTML parser ends a <script> at the first literal closing tag,
# even inside a JS string or comment. Refuse to ship a silently broken notes system.
if "</script" in notes_js.lower():
    raise SystemExit("notes.js contains a literal closing script tag; escape it before inlining")

start = markup.index('<div class="notebar"')
end = markup.index('<script>', start)
notes_markup = markup[start:end].rstrip()

OUT.write_text(f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Everything that was in flight is now in main — terminal-delight, 2026-09-22</title>
<style>
{base_css}
{notes_css}
{local_css}
</style>
</head>
<body>
{body}

{notes_markup}

<script>window.NOTES_FILE = '2026-09-22-the-consolidation.html';</script>
<script>
{notes_js}
</script>
</body>
</html>
""")
print(f"wrote {OUT}  ({OUT.stat().st_size} bytes)")
