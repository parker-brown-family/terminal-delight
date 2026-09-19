#!/usr/bin/env python3
"""Stamp the integration brief with what has changed under it since it was written.

A stale artifact is worse than an absent one, and this board moves every few minutes.
The brief's structural findings still hold; its board does not.
"""
from pathlib import Path

F = Path(__file__).parent / "2026-09-19-overseer-the-pass.html"
html = F.read_text()

BANNER = """
<div class="wrap"><div class="callout crit" style="margin-top:18px">
  <span class="lbl">Superseded in part &mdash; read this first</span>
  <p><b>The board in this brief is out of date; its structural findings are not.</b> Since it was
  written, <b>#555 merged</b> as <code>c63d95b</code> after someone merged main into the branch and the
  changelog conflict resolved itself, and three more pull requests opened (#560, #561, #562). The
  hazard it names as the loudest &mdash; pane 88's feature loose in the shared worktree with no branch
  &mdash; <b>closed itself</b>: that work is now #560.</p>
  <p>Still true, and still the point of the document: <b>28 of the branches ahead of main are squash
  residue</b> and only nine hold new commits; five of those nine are orphaned with no pull request;
  <code>CHANGELOG.md</code> is the only thing anything ever conflicts on, which it has now done
  <b>five times in one day</b>; and the merge-order simulation stands.</p>
  <p>For live board state use the rodeo checklist, which reads git and GitHub at generation time
  instead of quoting a moment:
  <code>reports/2026-09-19-the-rodeo-checklist.html</code>.</p>
</div></div>
"""

MARK = '<h2 class="sec">The board, in numbers</h2>'
if "Superseded in part" in html:
    print("already stamped")
elif MARK in html:
    # close the open .wrap, drop the banner in its own wrap, reopen
    html = html.replace(MARK, "</div>" + BANNER + '<div class="wrap">' + MARK, 1)
    F.write_text(html)
    print(f"stamped {F}")
else:
    raise SystemExit("anchor not found — refusing to guess where the banner goes")
