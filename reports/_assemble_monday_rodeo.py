#!/usr/bin/env python3
"""Assemble reports/2026-09-21-the-monday-rodeo.html.

A live-test checklist for the twenty-four pull requests that landed on 2026-09-21,
drawn. One self-contained file: the decision-brief skill's base.css / notes.css /
notes.js inlined beside the rodeo's component CSS (read from `_rodeo_style.css`
rather than copied, so the two checklists keep one look) and this brief's body.

Nothing about the machine's state is typed into the body. The build chain, the
live windows and the hook adapter's version are read at assemble time, because
every one of them is a thing that changes under the reader — the whole point of
the first two checklist items. Re-run it and the page is current; notes live in
the browser and survive the rebuild.
"""
import html
import os
import re
import subprocess
from datetime import datetime
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
SKILL = Path.home() / ".claude/skills/decision-brief"
NAME = "2026-09-21-the-monday-rodeo.html"
OUT = HERE / NAME
LIB = Path.home() / ".local/lib/terminal-delight"
LAUNCHER = Path.home() / ".local/bin/terminal-delight"
DAY = "2026-09-21"
E = html.escape


def git(*args, default=""):
    try:
        return subprocess.run(["git", "-C", str(REPO), *args],
                              capture_output=True, text=True, timeout=60).stdout.strip()
    except Exception:
        return default


# ---- the day's numbers -----------------------------------------------------
commits = git("log", f"--since={DAY} 00:00", "--oneline")
n_commits = len([l for l in commits.splitlines() if l])
merges = git("log", f"--since={DAY} 00:00", "--merges", "--oneline")
n_prs = len([l for l in merges.splitlines() if "Merge pull request" in l])

first_today = git("log", f"--since={DAY} 00:00", "--format=%h").splitlines()
first_today = first_today[-1] if first_today else "HEAD"
rust = git("diff", "--shortstat", f"{first_today}^", "HEAD", "--", "app/src", "app/tests")
m = re.search(r"(\d+) insertions", rust)
n_rust = f"{int(m.group(1)):,}" if m else "unknown"

# The test count is the gate line the head commit wrote about itself, not a run
# from here: a shared CARGO_TARGET_DIR serves another worktree's binary and a
# count read that way has already been wrong by sixteen tests.
recent = git("log", "-40", "--format=%B")
found = re.findall(r"([\d,]+) tests passing", recent)
n_tests = f"{int(found[0].replace(',', '')):,}" if found else "unknown"

main_sha = git("rev-parse", "--short", "HEAD") or "unknown"

# ---- the build chain -------------------------------------------------------
installed = os.path.basename(os.path.realpath(LAUNCHER)) if LAUNCHER.exists() else "no launcher"


def sha_of(build: str):
    m = re.match(r"td-([0-9a-f]{7,})-", build or "")
    return m.group(1) if m else None


def behind(build: str) -> str:
    """How many merges onto main a build is missing. Unknown is not zero."""
    sha = sha_of(build)
    if not sha:
        return "unknown &mdash; not a versioned build"
    if not git("cat-file", "-t", sha):
        return "unknown &mdash; that sha is not in this tree"
    n = git("rev-list", "--merges", "--count", f"{sha}..HEAD")
    if n == "0":
        return "up to date"
    return f"{n} merges"


def windows():
    """Every terminal-delight WINDOW process and the binary it actually is.

    A window is a td process whose command line carries no subcommand; `mcp`,
    `serve` and `ctl` are helpers. Read off /proc rather than declared, because
    a process does not follow a symlink that moved after it started, and that
    is the entire failure this figure exists to catch.
    """
    out = []
    try:
        pids = subprocess.run(["pgrep", "-f", "terminal-delight"],
                              capture_output=True, text=True, timeout=30).stdout.split()
    except Exception:
        return out
    for pid in pids:
        try:
            exe = os.path.realpath(f"/proc/{pid}/exe")
            args = [a for a in open(f"/proc/{pid}/cmdline").read().split("\0") if a]
        except OSError:
            continue
        if "terminal-delight" not in exe or not args:
            continue
        if any(a in ("mcp", "serve", "ctl", "surface", "skin", "probe", "bindings") for a in args[1:]):
            continue
        age = subprocess.run(["ps", "-o", "etime=", "-p", pid],
                             capture_output=True, text=True).stdout.strip()
        out.append((pid, os.path.basename(exe), age))
    out.sort(key=lambda r: r[0])
    return out


WINS = windows()
if WINS:
    rows = []
    for pid, build, age in WINS:
        b = behind(build)
        cls = "ok" if b == "up to date" else ("warn" if "unknown" in b else "bad")
        rows.append(f'<tr><td>pid <b>{E(pid)}</b><div class="dim">up {E(age)}</div></td>'
                    f'<td><code>{E(build)}</code></td>'
                    f'<td class="{cls}">{b}</td></tr>')
    winrows = "\n".join(rows)
else:
    winrows = ('<tr><td colspan="3">No window process was readable from /proc when this page was '
               'built. That is not the same as none running &mdash; it is an unread field.</td></tr>')

# ---- the hook adapter ------------------------------------------------------
adapter = Path.home() / ".local/bin/td-agent-hooks"
repo_adapter = REPO / "scripts/td-agent-hooks"
if not adapter.exists():
    hook_ver, hook_state = "absent", "not installed at all &mdash; the channel is off for every pane"
else:
    txt = adapter.read_text(errors="replace")
    m = re.search(r'TDAC (\d+\.\d+)', txt)
    hook_ver = m.group(1) if m else "unreadable"
    same = repo_adapter.exists() and repo_adapter.read_text(errors="replace") == txt
    hook_state = ("already the current one &mdash; mark it PASS and move on" if same
                  else f"the copy on PATH is TDAC {hook_ver}, the repository's is 0.2")

# ---- assemble --------------------------------------------------------------
sub = {
    "__STAMP__": datetime.now().strftime("%Y-%m-%d %H:%M"),
    "__COMMITS__": str(n_commits),
    "__PRS__": str(n_prs),
    "__RUST__": n_rust,
    "__TESTS__": n_tests,
    "__MAIN__": E(main_sha),
    "__INSTALLED__": E(installed),
    "__WINROWS__": winrows,
    "__HOOKVER__": E(hook_ver),
    "__HOOKSTATE__": hook_state,
}

body = (HERE / "_monday_rodeo_body.html").read_text()
for tok, val in sub.items():
    body = body.replace(tok, val)
left = sorted(set(re.findall(r"__[A-Z_0-9]+__", body)))
if left:
    raise SystemExit(f"unsubstituted tokens in the body: {left}")

base_css = (SKILL / "assets/base.css").read_text()
notes_css = (SKILL / "assets/notes.css").read_text()
notes_js = (SKILL / "assets/notes.js").read_text()
local_css = (HERE / "_rodeo_style.css").read_text()
markup = (SKILL / "reference/notes-markup.html").read_text()

# An HTML parser ends a <script> at the first literal closing tag it sees, including
# one inside a JS string — everything after it silently becomes page content and the
# notes system never runs, with no console error.
if "</script" in notes_js.lower():
    raise SystemExit("notes.js contains a literal closing script tag; escape it before inlining")

notes_markup = markup[markup.index('<div class="notebar"'):
                      markup.index('<script>', markup.index('<div class="notebar"'))].rstrip()

extra_css = r"""
/* ---- this page's additions on top of the rodeo's components ---- */
.esc-list { list-style: none; padding: 0; margin: 0 0 6px; }
.esc-list li { display: flex; gap: 12px; align-items: flex-start; border: 1px solid var(--line);
  border-radius: 8px; background: var(--surface-1); padding: 11px 13px; margin: 0 0 8px; font-size: 13.5px; }
.esc-list li b { color: #fff; }
.esc-list li em { color: var(--text-muted); font-style: normal; }
.esc-t { min-width: 0; flex: 1 1 auto; }
.esc-n { flex: none; width: 22px; height: 22px; border-radius: 50%; background: var(--critical);
  color: #fff; display: inline-flex; align-items: center; justify-content: center;
  font-family: var(--mono); font-size: 11px; }
.frames { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 14px; }
.frame { border: 1px solid var(--line); border-radius: 8px; padding: 10px 12px; background: var(--surface-0); }
.frame-t { font-family: var(--mono); font-size: 11px; color: var(--text-primary); margin-bottom: 8px; }
.arch { display: flex; flex-direction: column; gap: 8px; }
.bound { position: relative; border: 1px solid var(--s1); border-radius: 8px; padding: 18px 8px 8px; }
.bound.dashed { border-style: dashed; }
.bound.files { border-color: var(--text-muted); border-style: dotted; }
.bl { position: absolute; top: 4px; left: 8px; font-family: var(--mono); font-size: 9.5px;
  letter-spacing: .08em; text-transform: uppercase; color: var(--s1); }
.bound.files .bl { color: var(--text-muted); }
.node { border-radius: 6px; padding: 6px 8px; font-family: var(--mono); font-size: 11px; margin: 4px 0;
  background: var(--surface-2); color: var(--text-primary); border: 1px solid var(--line); }
.node.g { border-color: var(--s3); } .node.b { border-color: var(--s1); }
.node.a { border-color: var(--s4); background: #2a2410; }
.node.inner { margin: 6px 0 0 10px; }
.fc { font-size: 12.5px; color: var(--text-secondary); margin: 8px 0 0; }
.amber { color: var(--s4); }
.trip, .run, .quad { display: grid; gap: 12px; }
.trip { grid-template-columns: repeat(3, minmax(0, 1fr)); }
.run { grid-template-columns: repeat(2, minmax(0, 1fr)); }
/* Four states side by side need ~250px each to read; a tiled pane is 968px, which is
   above the 900px breakpoint below, so this one drops to 2x2 of its own accord first. */
.quad { grid-template-columns: repeat(4, minmax(0, 1fr)); }
@media (max-width: 1100px) { .quad { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
@media (max-width: 620px) { .quad { grid-template-columns: 1fr; } }
.state, .run-col { border: 1px solid var(--line); border-radius: 8px; padding: 9px 11px;
  background: var(--surface-0); min-width: 0; }
.state.now { border-color: var(--s3); }
.st-t { font-family: var(--mono); font-size: 10.5px; letter-spacing: .08em; text-transform: uppercase;
  color: var(--text-muted); margin-bottom: 6px; }
pre.mock { margin: 0; font-size: 11px; line-height: 1.5; white-space: pre; overflow-x: auto;
  background: transparent; border: 0; padding: 0; color: var(--text-primary); }
.fence { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
.side { border: 1px solid var(--line); border-radius: 8px; padding: 10px 12px; background: var(--surface-0); }
.side.in { border-color: var(--s3); } .side.out { border-color: var(--text-muted); }
.side ul { margin: 0; padding-left: 18px; font-size: 13px; } .side li { margin: 4px 0; }
kbd { font-family: var(--mono); font-size: 11px; padding: 1px 5px; border: 1px solid var(--line);
  border-radius: 4px; background: var(--surface-2); }
.modals-row { display: flex; flex-wrap: wrap; gap: 8px; }
.dlg-open { font: inherit; font-size: 12.5px; padding: 6px 10px; border-radius: 6px;
  border: 1px solid var(--line); background: var(--surface-2); color: var(--text-primary); cursor: pointer; }
.dlg-open:hover { border-color: var(--s1); }
table.wide td .dim { color: var(--text-muted); font-size: 11px; }
@media (max-width: 900px) {
  .frames, .run, .fence, .trip { grid-template-columns: 1fr !important; }
}
"""

verdict_js = r"""
(function () {
  var KEY = 'notes:' + window.NOTES_FILE;
  function store() {
    try { return JSON.parse(localStorage.getItem(KEY) || '{}'); } catch (e) { return {}; }
  }
  function verdictOf(nid, N) {
    var list = (N || store())[nid] || [];
    for (var i = list.length - 1; i >= 0; i--) {
      var t = (list[i].text || '').trim().toUpperCase();
      if (t.indexOf('PASS') === 0) return 'pass';
      if (t.indexOf('FAIL') === 0) return 'fail';
      if (t.indexOf('ODD') === 0) return 'odd';
      if (t.indexOf('SKIP') === 0) return 'skip';
    }
    return null;
  }
  function paint() {
    var N = store(), c = { pass: 0, fail: 0, odd: 0, skip: 0, todo: 0 };
    document.querySelectorAll('.finding').forEach(function (f) {
      f.classList.remove('v-pass', 'v-fail', 'v-odd', 'v-skip');
      var v = f.dataset.nid ? verdictOf(f.dataset.nid, N) : null;
      var mark = f.querySelector('.verdicts .mark');
      if (v) { f.classList.add('v-' + v); c[v]++; if (mark) mark.textContent = v.toUpperCase(); }
      else { c.todo++; if (mark) mark.textContent = ''; }
    });
    var set = function (id, n) { var e = document.getElementById(id); if (e) e.textContent = n; };
    set('c-pass', c.pass); set('c-fail', c.fail); set('c-odd', c.odd);
    set('c-todo', c.todo + c.skip);
  }
  document.addEventListener('click', function (ev) {
    var b = ev.target.closest('.verdicts button[data-v]');
    if (!b) return;
    var item = b.closest('.finding');
    var nb = item && item.querySelector(':scope > .note-btn');
    if (!nb) return;
    var v = b.dataset.v;
    var d = document.getElementById('d-note');
    if (d && d.open) d.close();     // a second verdict must not showModal() on an open dialog
    nb.click();
    // a timer, never requestAnimationFrame: rAF does not fire in a backgrounded tab,
    // which silently swallowed the prefill and made a one-click PASS store nothing.
    setTimeout(function () {
      var ta = document.getElementById('note-text');
      if (!ta) return;
      ta.value = v;
      if (v !== 'PASS') {           // a verdict that needs your words stays open, prefilled
        ta.focus();
        ta.setSelectionRange(ta.value.length, ta.value.length);
        return;
      }
      document.querySelector('[data-note-action="add"]').click();
      var dd = document.getElementById('d-note');
      if (dd && dd.open) dd.close();
      paint();
    }, 0);
  });
  document.addEventListener('click', function (ev) {
    if (ev.target.closest('[data-note-action], [data-close], .ndel')) setTimeout(paint, 60);
  });
  document.addEventListener('keydown', function (ev) {
    if (ev.key === 'Enter' && (ev.ctrlKey || ev.metaKey)) setTimeout(paint, 60);
  });
  paint();
})();
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

OUT.write_text(f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>The Monday rodeo &mdash; terminal-delight, {DAY}</title>
<style>
{base_css}
{notes_css}
{local_css}
{extra_css}
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
{verdict_js}
</script>
<script>
{dialog_js}
</script>
</body>
</html>
""")
print(f"wrote {OUT} ({OUT.stat().st_size} bytes)")
print(f"  main {main_sha} · installed {installed} · {len(WINS)} live window(s)")
print(f"  {n_commits} commits · {n_prs} PRs · {n_rust} rust lines · adapter TDAC {hook_ver}")
