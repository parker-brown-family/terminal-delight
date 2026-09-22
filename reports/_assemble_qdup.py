#!/usr/bin/env python3
"""Assemble reports/2026-09-21-one-ask-four-cards.html.

The duplicate-question diagnosis: why one AskUserQuestion drew four rows on
pane 16's decisions rail, and why the road that answers a whole round at once
was closed before anybody pressed anything.

One self-contained file. The decision-brief skill's base.css / notes.css /
notes.js are inlined beside this brief's own component CSS. Nothing external,
so it survives being emailed as one file.

The build chain and the installed hook version are read at assemble time
rather than typed into the body: both are things that change under the reader,
and both are findings in the brief.
"""
import html
import re
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
SKILL = Path.home() / ".claude/skills/decision-brief"
NAME = "2026-09-21-one-ask-four-cards.html"
OUT = HERE / NAME
E = html.escape


def git(*args, default=""):
    try:
        return subprocess.run(["git", "-C", str(REPO), *args],
                              capture_output=True, text=True, timeout=60).stdout.strip()
    except Exception:
        return default


head = git("rev-parse", "--short", "HEAD") or "unknown"
branch = git("rev-parse", "--abbrev-ref", "HEAD") or "unknown"

launcher = Path.home() / ".local/bin/terminal-delight"
try:
    installed = launcher.resolve().name
except Exception:
    installed = "unknown"

hook = Path.home() / ".local/bin/td-agent-hooks"
hook_ver = "unreadable"
if hook.is_file():
    txt = hook.read_text(errors="replace")
    vers = sorted(set(re.findall(r'td:"(\d+\.\d+)"', txt)))
    hook_ver = ", ".join(vers) if vers else "no version string"

repo_hook = REPO / "scripts/td-agent-hooks"
repo_ver = "unreadable"
if repo_hook.is_file():
    vers = sorted(set(re.findall(r'td:"(\d+\.\d+)"', repo_hook.read_text(errors="replace"))))
    repo_ver = ", ".join(vers) if vers else "no version string"

base_css = (SKILL / "assets/base.css").read_text()
notes_css = (SKILL / "assets/notes.css").read_text()
notes_js = (SKILL / "assets/notes.js").read_text()
body = (HERE / "_qdup_body.html").read_text()

# The state line is read here so the page cannot go quietly out of date about
# the two things it tells the reader to install.
state = f"""
<p class="statebar">branch <code>{E(branch)}</code> at <code>{E(head)}</code>
&middot; launcher points at <code>{E(installed)}</code>
&middot; installed hook adapter TDAC <code>{E(hook_ver)}</code>, repo ships <code>{E(repo_ver)}</code>
&middot; assembled by <code>reports/_assemble_qdup.py</code>, re-run it and this line is current</p>
"""
body = body.replace("</header>", state + "</header>", 1)

local_css = """
figure { margin: 0 0 22px; border: 1px solid var(--line); border-radius: 10px;
  background: var(--surface-1); padding: 16px 18px 14px; }
figure > .lbl { display: block; font-family: var(--mono); font-size: 10.5px; letter-spacing: .1em;
  text-transform: uppercase; color: var(--text-muted); margin: 0 0 13px; }
figcaption { margin-top: 13px; font-size: 13.5px; color: var(--text-secondary); line-height: 1.55; }
figcaption b { color: var(--text-primary); }
.scroller { overflow-x: auto; }
svg { display: block; width: 100%; height: auto; }

.hero { margin-bottom: 30px; }
.kicker { font-family: var(--mono); font-size: 11px; letter-spacing: .09em;
  text-transform: uppercase; color: var(--text-muted); margin: 0 0 8px; }
.hero h1 { margin: 0 0 14px; }
.lede { font-size: 15.5px; line-height: 1.62; color: var(--text-secondary); margin: 0 0 12px; }
.lede b { color: var(--text-primary); }
.statebar { font-family: var(--mono); font-size: 11px; color: var(--text-muted);
  border-top: 1px solid var(--line); padding-top: 10px; margin: 16px 0 0; line-height: 1.8; }

.esc-list { padding-left: 20px; }
.esc-list li { margin-bottom: 16px; line-height: 1.55; }
.esc-list li em { color: var(--text-muted); font-size: 13px; }

/* ---- the rail mockup ---- */
.railmock { max-width: 320px; border: 1px solid var(--line); border-radius: 8px;
  background: var(--surface-0); padding: 8px; }
.rr { border-left: 3px solid var(--line); background: var(--surface-2); border-radius: 5px;
  padding: 7px 9px; margin-bottom: 7px; }
.rr.dupe { border-left-color: var(--critical); background: #2a1a1a; }
.rlane { font-family: var(--mono); font-size: 9.5px; letter-spacing: .1em; margin-bottom: 4px; }
.rlane.crit { color: #f08a8a; } .rlane.warn { color: #f5c95f; } .rlane.ok { color: #6cd46c; }
.rtitle { font-size: 13px; color: var(--text-primary); line-height: 1.35; }
.rsub { font-size: 11px; color: var(--text-muted); margin-top: 3px; }
.rid { font-family: var(--mono); font-size: 9.5px; color: #8b8a80; margin-top: 5px;
  border-top: 1px dotted var(--line); padding-top: 4px; }
.mockkey { font-size: 11.5px; color: var(--text-muted); margin: 9px 0 0; }
.sw { display: inline-block; width: 10px; height: 10px; border-radius: 2px;
  background: var(--critical); margin-right: 6px; vertical-align: -1px; }
.sw4 { background: var(--s4); }

/* ---- the built-up architecture ---- */
.arch { display: flex; flex-direction: column; align-items: center; min-width: 470px; }
.zone { position: relative; border: 1px solid var(--line); border-radius: 9px;
  padding: 24px 16px 14px; background: var(--surface-0); width: 100%;
  display: flex; flex-direction: column; align-items: center; }
.zlbl { position: absolute; top: 7px; left: 12px; font-family: var(--mono); font-size: 9.5px;
  letter-spacing: .1em; color: var(--text-muted); }
.node { border: 1px solid var(--line); border-radius: 6px; background: var(--surface-2);
  padding: 8px 12px; font-family: var(--mono); font-size: 11.5px; color: var(--text-primary);
  display: flex; gap: 10px; align-items: center; justify-content: space-between;
  min-width: 300px; }
.node.s1 { border-color: var(--s1); }
.node.s3 { border-color: var(--s3); }
.node.s4 { border-color: var(--s4); }
.node.dim { border-color: var(--line); color: var(--text-muted); background: var(--surface-1); }
.node.bench { border-color: var(--s3); background: #16231d; }
.node.bench.bad { border-color: var(--critical); background: #261818; }
.node .pill { font-size: 9.5px; color: var(--text-muted); border: 1px solid var(--line);
  border-radius: 20px; padding: 1px 7px; white-space: nowrap; }
.node .pill.mono { font-family: var(--mono); }
.conn.v { width: 1px; height: 16px; background: var(--line); }
.side, .side2, .side3, .side4 { margin-top: 10px; }
.edge-note { font-family: var(--mono); font-size: 10px; color: var(--s3); margin: 6px 0 0;
  text-align: center; }
.edge-note.dim { color: var(--text-muted); }
.edge-note.none { color: var(--critical); }
.gutter { display: block; margin-top: 9px; font-size: 12px; color: var(--text-muted);
  border-top: 1px dotted var(--line); padding-top: 7px; }

/* ---- the roads table ---- */
table.wide { width: 100%; border-collapse: collapse; font-size: 13px; }
table.wide th { text-align: left; font-family: var(--mono); font-size: 10px; letter-spacing: .09em;
  text-transform: uppercase; color: var(--text-muted); border-bottom: 1px solid var(--line);
  padding: 0 10px 7px 0; vertical-align: bottom; }
table.wide td { padding: 10px 10px 10px 0; border-bottom: 1px solid var(--line);
  vertical-align: top; line-height: 1.5; color: var(--text-secondary); }
table.wide td b { color: var(--text-primary); }
td.yes { color: #6cd46c; } td.no { color: #f08a8a; }
tr.rec td { background: rgba(25,158,112,.07); }
.pillrec { font-family: var(--mono); font-size: 9px; letter-spacing: .07em; text-transform: uppercase;
  color: #6cd46c; border: 1px solid rgba(25,158,112,.45); border-radius: 20px; padding: 1px 7px;
  white-space: nowrap; margin-left: 6px; }

/* ---- the timeline ---- */
.tl { border-left: 2px solid var(--line); padding-left: 0; margin: 0; }
.ti { display: flex; gap: 12px; padding: 8px 0 8px 14px; border-left: 3px solid transparent;
  margin-left: -2px; }
.ti.bad { border-left-color: var(--critical); background: rgba(208,59,59,.07); }
.tt { font-family: var(--mono); font-size: 11px; color: var(--text-muted); flex: none; width: 62px; }
.td-b { font-size: 13px; color: var(--text-secondary); line-height: 1.5; }
.td-b b { color: var(--text-primary); }
.ok { color: #6cd46c; } .crit { color: #f08a8a; }

/* ---- the target card mockup ---- */
.cardmock { max-width: 420px; border: 1px solid var(--line); border-radius: 8px;
  background: var(--surface-0); padding: 12px 14px; margin-bottom: 14px; }
.cm-head { font-size: 14px; color: var(--text-primary); margin-bottom: 9px; }
.cm-chip { font-family: var(--mono); font-size: 9px; letter-spacing: .08em; text-transform: uppercase;
  border: 1px solid var(--line); border-radius: 4px; padding: 1px 5px; color: var(--text-muted);
  margin-right: 7px; }
.cm-steps { display: flex; gap: 6px; flex-wrap: wrap; margin-bottom: 6px; }
.st { font-family: var(--mono); font-size: 10px; padding: 2px 7px; border-radius: 4px;
  border: 1px solid var(--line); color: var(--text-muted); }
.st.done { color: #6cd46c; border-color: rgba(25,158,112,.45); }
.st.open { color: var(--text-primary); border-color: var(--s1); }
.cm-prog { font-size: 11px; color: var(--text-muted); margin-bottom: 10px; }
.cm-opts { display: flex; gap: 6px; flex-wrap: wrap; margin-bottom: 11px; }
.op { font-size: 11.5px; border: 1px solid var(--line); border-radius: 5px; padding: 4px 9px;
  color: var(--text-secondary); }
.op.picked { border-color: var(--s5); color: var(--text-primary); background: rgba(213,81,129,.12); }
.cm-foot { border-top: 1px solid var(--line); padding-top: 9px; text-align: right; }
.cm-submit { font-family: var(--mono); font-size: 10.5px; letter-spacing: .08em;
  border: 1px solid var(--s4); border-radius: 5px; padding: 4px 10px; color: var(--s4); }
.cm-submit.dim { border-color: var(--line); color: var(--text-muted); }
.cm-submit.live { border-color: var(--s4); color: #f5c95f; background: rgba(201,133,0,.12); }

/* ---- the grill ---- */
.grill .q { border: 1px solid var(--line); border-radius: 9px; background: var(--surface-1);
  padding: 15px 17px; margin-bottom: 16px; }
.grill .ask { margin: 0 0 12px; font-size: 14.5px; color: var(--text-primary); }
.rationale { margin: 12px 0 0; font-size: 13px; color: var(--text-muted); line-height: 1.55; }
.callout { border: 1px solid var(--s4); border-left-width: 3px; border-radius: 8px;
  background: rgba(201,133,0,.07); padding: 13px 16px; margin: 0 0 22px; }
.callout p { margin: 0; font-size: 14px; line-height: 1.58; color: var(--text-secondary); }
.callout b { color: var(--text-primary); }

@media (max-width: 620px) {
  .arch { min-width: 0; }
  .node { min-width: 0; flex-wrap: wrap; }
}
"""

dialog_js = """
(function () {
  document.querySelectorAll('[data-dlg]').forEach(function (b) {
    b.addEventListener('click', function () {
      var d = document.getElementById(b.dataset.dlg);
      if (d && typeof d.showModal === 'function') d.showModal();
    });
  });
  document.querySelectorAll('dialog [data-close]').forEach(function (b) {
    b.addEventListener('click', function () {
      var d = b.closest('dialog');
      if (d && d.open) d.close();
    });
  });
})();
"""

notes_markup = (SKILL / "reference/notes-markup.html").read_text()
# The reference block carries its own NOTES_FILE placeholder and a trailing
# instruction comment; both are replaced here so the page ships one real value.
notes_markup = notes_markup.split("<script>\n  /* Set before notes.js runs.")[0]

OUT.write_text(f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>One ask, four cards &mdash; terminal-delight, 2026-09-21</title>
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
print(f"wrote {OUT} ({OUT.stat().st_size} bytes)")
print(f"  {branch} @ {head} · launcher {installed} · hook TDAC {hook_ver} (repo {repo_ver})")
