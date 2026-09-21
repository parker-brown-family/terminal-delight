#!/usr/bin/env python3
"""Assemble the rename-box brief: inline every asset so the file survives being emailed."""
from pathlib import Path

SKILL = Path("/home/parker/.claude/skills/decision-brief")
HERE = Path(__file__).parent
NAME = "2026-09-19-the-bench-keeps-your-keystrokes.html"
OUT = HERE / NAME

base_css = (SKILL / "assets/base.css").read_text()
notes_css = (SKILL / "assets/notes.css").read_text()
notes_js = (SKILL / "assets/notes.js").read_text()
local_css = (HERE / "_rename_style.css").read_text()
body = (HERE / "_rename_body.html").read_text()
markup = (SKILL / "reference/notes-markup.html").read_text()

# The inlining trap: an HTML parser ends a <script> at the first literal closing tag,
# even inside a JS string or comment. Refuse to ship a silently broken notes system.
if "</script" in notes_js.lower():
    raise SystemExit("notes.js contains a literal closing script tag; escape it before inlining")

start = markup.index('<div class="notebar"')
end = markup.index('<script>', start)
notes_markup = markup[start:end].rstrip()

# notes.js wires only its own two dialogs. The brief's sub-reports need their own
# opener, and every dialog needs its close affordance.
dialogs_js = """
(function () {
  function wire() {
    Array.prototype.forEach.call(document.querySelectorAll('[data-dlg]'), function (b) {
      b.addEventListener('click', function () {
        var d = document.getElementById(b.dataset.dlg);
        if (d) d.showModal();
      });
    });
    Array.prototype.forEach.call(document.querySelectorAll('dialog'), function (d) {
      Array.prototype.forEach.call(d.querySelectorAll('[data-close]'), function (x) {
        x.addEventListener('click', function () { d.close(); });
      });
      d.addEventListener('click', function (e) { if (e.target === d) d.close(); });
    });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', wire);
  else wire();
})();
"""

OUT.write_text(f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Typing into the pane's rename box — terminal-delight, 2026-09-19</title>
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
{dialogs_js}
</script>
</body>
</html>
""")
print(f"wrote {OUT}  ({OUT.stat().st_size} bytes)")
