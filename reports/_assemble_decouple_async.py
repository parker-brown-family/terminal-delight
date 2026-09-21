#!/usr/bin/env python3
"""Assemble reports/2026-09-21-decouple-async-the-agent-channel.html.

One self-contained file: the decision-brief skill's base.css, notes.css and
notes.js inlined beside the brief's own styles and body, with the notes markup
the notes system needs. No external src anywhere, so the file survives being
emailed as one file. Re-run after editing the body.
"""
from pathlib import Path

HERE = Path(__file__).resolve().parent
SKILL = Path.home() / ".claude/skills/decision-brief"
OUT = HERE / "2026-09-21-decouple-async-the-agent-channel.html"

base_css = (SKILL / "assets/base.css").read_text()
notes_css = (SKILL / "assets/notes.css").read_text()
notes_js = (SKILL / "assets/notes.js").read_text()
notes_markup = (SKILL / "reference/notes-markup.html").read_text()
body = (HERE / "_decouple_async_body.html").read_text()

# The inlining trap: a literal closing script tag inside the JS would end the
# element early and the notes system would silently never run.
assert "</script" not in notes_js.lower(), "notes.js holds a closing script tag"

# The markup block ends with a placeholder script that sets NOTES_FILE and a
# comment asking for notes.js to follow; both are replaced here.
notes_markup = notes_markup.replace("<YYYY-MM-DD>-<topic-slug>.html", OUT.name)
notes_markup = notes_markup.replace(
    "<!-- then paste the contents of assets/notes.js inline, inside a <script> tag -->",
    "<script>\n" + notes_js + "\n</script>",
)

brief_css = r"""
/* ---- the brief's own components (base.css leaves these to the brief) ---- */
header.top .sub { color: var(--text-secondary); font-size: 13.5px; margin: 6px 0 0; }
.headline { margin: 26px 0 8px; }
.lede { font-size: 17px; line-height: 1.5; color: var(--text-primary); }
.lede b { color: #fff; }
kbd { font-family: var(--mono); font-size: 11px; padding: 1px 5px; border: 1px solid var(--line); border-radius: 4px; background: var(--surface-2); }
.cards { display: grid; gap: 14px; margin: 8px 0 6px; }
.cards.three { grid-template-columns: repeat(3, minmax(0, 1fr)); }
.card { border: 1px solid var(--line); border-radius: 10px; background: var(--surface-1); padding: 14px 16px; }
.card h3 { font-size: 14.5px; margin: 6px 0 8px; line-height: 1.35; }
.card p { font-size: 13px; color: var(--text-secondary); margin: 0 0 8px; }
.card .why { color: var(--text-primary); }
.card .badge { color: var(--text-muted); font-size: 10.5px; letter-spacing: .08em; text-transform: uppercase; font-family: var(--mono); }
.card.rec { border-color: var(--s3); box-shadow: 0 0 0 1px var(--s3) inset; }
.card.rec .badge { color: var(--s3); }
.esc-list { list-style: none; padding: 0; margin: 0; counter-reset: none; }
.esc-list li { display: flex; gap: 12px; align-items: flex-start; border: 1px solid var(--line); border-radius: 8px; background: var(--surface-1); padding: 10px 12px; margin: 0 0 8px; font-size: 13.5px; }
.esc-list li b { color: #fff; }
.esc-list li em { color: var(--text-muted); font-style: normal; }
.esc-n { flex: none; width: 22px; height: 22px; border-radius: 50%; background: var(--critical); color: #fff; display: inline-flex; align-items: center; justify-content: center; font-family: var(--mono); font-size: 11px; }
figure { margin: 0 0 22px; border: 1px solid var(--line); border-radius: 10px; background: var(--surface-1); padding: 16px 18px 14px; }
figure > .lbl { display: block; font-family: var(--mono); font-size: 10.5px; letter-spacing: .1em; text-transform: uppercase; color: var(--text-muted); margin: 0 0 13px; }
figcaption { margin-top: 13px; font-size: 13.5px; color: var(--text-secondary); }
figcaption b { color: var(--text-primary); }
.scroller { overflow-x: auto; }
svg { display: block; width: 100%; height: auto; }
/* architecture frames */
.frames { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 14px; }
.frame { border: 1px solid var(--line); border-radius: 8px; padding: 10px 12px; background: var(--surface-0); }
.frame-t { font-family: var(--mono); font-size: 11px; color: var(--text-primary); margin-bottom: 8px; }
.arch { display: flex; flex-direction: column; gap: 8px; }
.bound { position: relative; border: 1px solid var(--s1); border-radius: 8px; padding: 18px 8px 8px; }
.bound.dashed { border-style: dashed; }
.bound.files { border-color: var(--text-muted); border-style: dotted; }
.bl { position: absolute; top: 4px; left: 8px; font-family: var(--mono); font-size: 9.5px; letter-spacing: .08em; text-transform: uppercase; color: var(--s1); }
.bound.files .bl { color: var(--text-muted); }
.node { border-radius: 6px; padding: 6px 8px; font-family: var(--mono); font-size: 11px; margin: 4px 0; background: var(--surface-2); color: var(--text-primary); border: 1px solid var(--line); }
.node.g { border-color: var(--s3); }
.node.b { border-color: var(--s1); }
.node.r { border-color: var(--critical); }
.node.a { border-color: var(--s4); background: #2a2410; }
.node.inner { margin: 6px 0 0 10px; }
.fc { font-size: 12.5px; color: var(--text-secondary); margin: 8px 0 0; }
.amber { color: var(--s4); }
/* triptych / run */
.trip, .run { display: grid; gap: 12px; }
.trip { grid-template-columns: repeat(3, minmax(0, 1fr)); }
.run { grid-template-columns: repeat(2, minmax(0, 1fr)); }
.state, .run-col { border: 1px solid var(--line); border-radius: 8px; padding: 8px 10px; background: var(--surface-0); min-width: 0; }
.state.now { border-color: var(--s3); }
.st-t { font-family: var(--mono); font-size: 10.5px; letter-spacing: .08em; text-transform: uppercase; color: var(--text-muted); margin-bottom: 6px; }
pre.mock { margin: 0; font-size: 11px; line-height: 1.45; white-space: pre; overflow-x: auto; background: transparent; border: 0; padding: 0; color: var(--text-primary); }
/* timeline */
.tl { display: flex; flex-direction: column; gap: 6px; }
.tls { display: grid; grid-template-columns: 26px 1fr; gap: 8px; align-items: baseline; border-left: 3px solid var(--line); padding: 6px 10px; background: var(--surface-0); border-radius: 0 6px 6px 0; font-size: 13px; }
.tls.done { border-left-color: var(--s3); }
.tls.held { border-left-color: var(--s4); }
.tls .n { font-family: var(--mono); font-size: 11px; color: var(--text-muted); }
.tls .ev { display: block; grid-column: 2; color: var(--text-muted); font-family: var(--mono); font-size: 11px; }
/* fence */
.fence { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
.side { border: 1px solid var(--line); border-radius: 8px; padding: 10px 12px; background: var(--surface-0); }
.side.in { border-color: var(--s3); }
.side.out { border-color: var(--text-muted); }
.side ul { margin: 0; padding-left: 18px; font-size: 13px; }
.side li { margin: 4px 0; }
/* findings, grill */
.findings { list-style: none; padding: 0; margin: 0; }
.finding { border: 1px solid var(--line); border-radius: 8px; background: var(--surface-1); padding: 10px 12px; margin: 0 0 8px; font-size: 13.5px; }
.finding b { color: #fff; }
.finding em { color: var(--text-muted); font-style: normal; }
.grill .ask { border: 1px solid var(--critical); border-radius: 10px; background: var(--surface-1); padding: 12px 14px; margin: 0 0 10px; }
.grill .ask h3 { font-size: 14px; margin: 0 0 6px; }
.grill .ask p { font-size: 13px; color: var(--text-secondary); margin: 0 0 6px; }
.grill .ask .rec { color: var(--text-primary); }
.grill .ask .rec b { color: var(--s3); }
table.wide { width: 100%; font-size: 12.5px; }
table.wide td .dim { color: var(--text-muted); font-size: 11px; }
.modals-row { display: flex; flex-wrap: wrap; gap: 8px; }
.dlg-open { font: inherit; font-size: 12.5px; padding: 6px 10px; border-radius: 6px; border: 1px solid var(--line); background: var(--surface-2); color: var(--text-primary); cursor: pointer; }
.dlg-open:hover { border-color: var(--s1); }
@media (max-width: 900px) { .cards.three, .trip, .frames, .run, .fence { grid-template-columns: 1fr; } }
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
<title>The agent channel — decoupling the workbench from the terminal view — terminal-delight</title>
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
