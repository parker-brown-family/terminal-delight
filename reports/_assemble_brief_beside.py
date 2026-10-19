#!/usr/bin/env python3
"""Assemble reports/2026-09-24-brief-beside.html — the plan for a brief and its agent talking.

One self-contained file, the same way the gate pages are built: the decision-brief
skill's base.css, notes.css and notes.js inlined beside this page's own styles and
body. The soak chart is drawn here from the committed measurements in
docs/plans/gui-in-the-tui/evidence/, so the picture cannot drift from the numbers.
Placeholders: {{soakchart}}. Re-run after editing.
"""
import csv
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
SKILL = Path.home() / ".claude/skills/decision-brief"
EVIDENCE = HERE.parent / "docs/plans/gui-in-the-tui/evidence"
OUT = HERE / "2026-09-24-brief-beside.html"
TITLE = "Brief Beside"

base_css = (SKILL / "assets/base.css").read_text()
notes_css = (SKILL / "assets/notes.css").read_text()
notes_js = (SKILL / "assets/notes.js").read_text()
notes_markup = (SKILL / "reference/notes-markup.html").read_text()
shared_css = (HERE / "_documents_in_the_pane_style.css").read_text()
page_css = (HERE / "_floating_square_style.css").read_text()
body = (HERE / "_brief_beside_body.html").read_text()

assert "</script" not in notes_js.lower(), "notes.js holds a closing script tag"
assert "</script" not in body.lower(), "the body holds a script tag; keep behaviour in dialog_js"


def closed_series(name: str) -> list[tuple[int, int]]:
    with open(EVIDENCE / name) as f:
        return [(int(r["cycle"]), int(r["closed_gpu_mib"])) for r in csv.DictReader(f)]


def soak_chart() -> str:
    fixed = closed_series("slice-1-soak-fixed.csv")
    control = closed_series("slice-1-soak-control.csv")
    x0, x1, y0, y1 = 70.0, 730.0, 250.0, 26.0  # plot box: left, right, bottom, top
    ymax = 7000.0

    def px(c: float) -> float:
        return x0 + (x1 - x0) * c / 100.0

    def py(m: float) -> float:
        return y0 - (y0 - y1) * m / ymax

    def pts(series):
        return " ".join(f"{px(c):.1f},{py(m):.1f}" for c, m in series)

    last_c, last_m = control[-1]
    step = (control[-1][1] - control[0][1]) / (control[-1][0] - control[0][0])
    proj_end = last_m + step * (100 - last_c)
    grid = []
    for m in (0, 2000, 4000, 6000):
        grid.append(
            f'<line x1="{x0}" y1="{py(m):.1f}" x2="{x1}" y2="{py(m):.1f}" stroke="#2c2c29"/>'
            f'<text x="{x0 - 8}" y="{py(m) + 4:.1f}" font-size="11" fill="#8b8a80" text-anchor="end">'
            f"{m:,}</text>"
        )
    for c in (0, 25, 50, 75, 100):
        grid.append(
            f'<text x="{px(c):.1f}" y="{y0 + 18}" font-size="11" fill="#8b8a80" text-anchor="middle">{c}</text>'
        )
    fixed_lo = min(m for _, m in fixed)
    fixed_hi = max(m for _, m in fixed)
    return f"""<svg viewBox="0 0 760 300" role="img" aria-label="GPU memory held by the window after each close. The build with the fix stays between {fixed_lo} and {fixed_hi} MiB for all {len(fixed)} cycles. The control build, with the texture release removed, climbs {step:.0f} MiB every cycle, from {control[0][1]} to {last_m} MiB over {len(control)} cycles; a dashed projection continues that rate to {proj_end:,.0f} MiB at cycle 100." font-family="ui-monospace, monospace">
  {''.join(grid)}
  <text x="{x0 - 52}" y="{y1 - 8}" font-size="11" fill="#8b8a80">MiB on the GPU</text>
  <text x="{x1}" y="{y0 + 36}" font-size="11" fill="#8b8a80" text-anchor="end">open-and-close cycle</text>
  <line x1="{px(last_c):.1f}" y1="{py(last_m):.1f}" x2="{px(100):.1f}" y2="{py(proj_end):.1f}" stroke="#e6b54a" stroke-width="2" stroke-dasharray="6 5"/>
  <polyline points="{pts(control)}" fill="none" stroke="#ec835a" stroke-width="2.5"/>
  <polyline points="{pts(fixed)}" fill="none" stroke="#3ecf8e" stroke-width="2.5"/>
  <text x="{px(62):.1f}" y="{py(proj_end) + 4:.1f}" font-size="11.5" fill="#e6b54a">projected: {proj_end / 1024:.1f} GiB by cycle 100</text>
  <text x="{px(23):.1f}" y="{py(last_m) + 10:.1f}" font-size="11.5" fill="#ec835a">control, release removed: +{step:.0f} MiB a cycle</text>
  <text x="{px(40):.1f}" y="{py(fixed_hi) - 10:.1f}" font-size="11.5" fill="#3ecf8e">this build: {fixed_lo}&#8211;{fixed_hi} MiB after every close</text>
</svg>"""


pass
assert "{{" not in body, re.findall(r"\{\{\w+\}\}", body)

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
<title>{TITLE}</title>
<style>
{base_css}
{notes_css}
{shared_css}
{page_css}
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
