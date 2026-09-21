#!/usr/bin/env python3
"""Assemble reports/2026-09-21-through-the-eye-of-the-needle.html.

One self-contained file: the decision-brief skill's base.css, notes.css and
notes.js inlined beside the brief's own styles and body, with the notes markup
the notes system needs. The component CSS is shared with the agent-channel
brief assembled earlier the same day (`_assemble_decouple_async.py`), read from
that file rather than copied, so the two briefs keep one look. Re-run after
editing the body.
"""
from pathlib import Path

HERE = Path(__file__).resolve().parent
SKILL = Path.home() / ".claude/skills/decision-brief"
OUT = HERE / "2026-09-21-through-the-eye-of-the-needle.html"

base_css = (SKILL / "assets/base.css").read_text()
notes_css = (SKILL / "assets/notes.css").read_text()
notes_js = (SKILL / "assets/notes.js").read_text()
notes_markup = (SKILL / "reference/notes-markup.html").read_text()
body = (HERE / "_needle_body.html").read_text()

sibling = (HERE / "_assemble_decouple_async.py").read_text()
brief_css = sibling.split('brief_css = r"""', 1)[1].split('"""', 1)[0]

assert "</script" not in notes_js.lower(), "notes.js holds a closing script tag"

notes_markup = notes_markup.replace("<YYYY-MM-DD>-<topic-slug>.html", OUT.name)
notes_markup = notes_markup.replace(
    "<!-- then paste the contents of assets/notes.js inline, inside a <script> tag -->",
    "<script>\n" + notes_js + "\n</script>",
)

extra_css = r"""
/* ---- this brief's additions ---- */
.stat-row { display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); gap: 10px; margin: 6px 0 4px; }
.stat { border: 1px solid var(--line); border-radius: 8px; padding: 10px 12px; background: var(--surface-0); }
.stat .k { font-family: var(--mono); font-size: 10px; letter-spacing: .08em; text-transform: uppercase; color: var(--text-muted); }
.stat .v { font-size: 22px; font-weight: 600; color: var(--text-primary); margin-top: 2px; }
.stat .v small { font-size: 12px; color: var(--text-muted); font-weight: 400; }
.tls.needle { border-left-color: var(--s1); }
.tls.peer { border-left-color: var(--s3); }
.tls.pending { border-left-color: var(--s4); }
.fate { font-family: var(--mono); font-size: 10.5px; padding: 1px 6px; border-radius: 4px; border: 1px solid var(--line); white-space: nowrap; }
.fate.m { border-color: var(--s3); color: var(--s3); }
.fate.k { border-color: var(--s1); color: var(--s1); }
.fate.p { border-color: var(--s4); color: var(--s4); }
.fate.d { border-color: var(--text-muted); color: var(--text-muted); }
table.wide th { text-align: left; font-family: var(--mono); font-size: 10.5px; letter-spacing: .06em; text-transform: uppercase; color: var(--text-muted); }
table.wide td { vertical-align: top; padding: 5px 8px 5px 0; border-top: 1px solid var(--line); }
table.wide code { font-size: 11.5px; }
.gutter { font-family: var(--mono); font-size: 10.5px; color: var(--text-muted); margin-top: 10px; }
@media (max-width: 900px) { .stat-row { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
"""

dialog_js = """
(function () {
  document.querySelectorAll('[data-dlg]').forEach(function (b) {
    b.addEventListener('click', function () {
      var d = document.getElementById(b.dataset.dlg);
      if (d && typeof d.showModal === 'function') d.showModal();
    });
  });
})();
"""

html = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Through the eye of the needle — where terminal-delight landed on 2026-09-21</title>
<style>
{base_css}
{notes_css}
{brief_css}
{extra_css}
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
