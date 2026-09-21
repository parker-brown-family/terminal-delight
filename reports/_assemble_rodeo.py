#!/usr/bin/env python3
"""Assemble the rodeo checklist.

The board moves every few minutes and five panes are finishing at once, so nothing
about the state is typed into the body. Every checklist item is GENERATED from a live
pull request; `_rodeo_items.json` is a curated overlay of real gesture-level test steps
keyed by PR number. A pull request with no overlay entry still gets an item, marked as
auto-derived — because a test step nobody wrote is not the same as one somebody wrote.

Re-run it and the page is current. Notes live in the browser and survive the rebuild.
"""
import html
import json
import os
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

SKILL = Path("/home/parker/.claude/skills/decision-brief")
HERE = Path(__file__).parent
REPO = HERE.parent
NAME = "2026-09-19-the-rodeo-checklist.html"
OUT = HERE / NAME
GH = "/home/parker/bin/gh"
E = html.escape

LIB = "/home/parker/.local/lib/terminal-delight"


def live_windows():
    """Every Terminal Delight WINDOW process, and the binary each one actually is.

    Measured off /proc, not declared. A window is a td process whose command line
    carries no subcommand — `mcp`, `serve` and `ctl` are helpers, not windows.

    This exists because a relaunch on 2026-09-18 landed on a debug build out of a
    worktree: the window was drawing pre-#555 chrome while the installed binary was
    correct, and every "just relaunch" note on the box was wrong as a result. A window
    started BY PATH ignores the symlink, adopts the live session, and draws every pane
    in whatever branch that worktree happens to be on.
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
            with open(f"/proc/{pid}/cmdline", "rb") as f:
                args = [a for a in f.read().decode().split("\0") if a]
        except OSError:
            continue
        if "terminal-delight" not in exe or len(args) < 1:
            continue
        if any(a in ("mcp", "serve", "ctl", "surface", "skin", "probe") for a in args[1:]):
            continue
        started = subprocess.run(["ps", "-o", "lstart=", "-p", pid],
                                 capture_output=True, text=True).stdout.strip()
        # field 22 of /proc/<pid>/stat is start time in clock ticks since boot. Monotonic,
        # and the only honest way to order these: pids WRAP, and on this box the debug
        # window that took the session had a LOWER pid than the older window it displaced,
        # so "newest = highest pid" picked the wrong one and hid the fault.
        try:
            with open(f"/proc/{pid}/stat") as f:
                ticks = int(f.read().rsplit(")", 1)[1].split()[19])
        except (OSError, IndexError, ValueError):
            ticks = 0
        base = os.path.basename(exe).replace(" (deleted)", "")
        installed_build = exe.startswith(LIB) and base.startswith("td-")
        out.append({"pid": int(pid), "exe": exe, "build": base, "started": started,
                    "ticks": ticks, "installed": installed_build})
    return sorted(out, key=lambda w: w["ticks"])


def live_hosts():
    """Every session host — a THIRD build each, and the one nothing here reported.

    `serve --session <n>` owns every pane's pseudoterminal, is parented by systemd
    rather than by any window, and therefore survives a window restart. Which is
    the point of it: the window is swappable and the host is not. The consequence
    is that it only ever upgrades by dying, and it cannot die without taking every
    pane with it — so it drifts, quietly, for days.

    This page compared the window against the installed binary and against main and
    called that the answer. It was two thirds of one. On 2026-09-19 the window was
    exactly main and its host was 76 merges behind it, and nothing on the page said so.

    Returns ALL of them, each tagged with the session it serves, because there is one
    host per session and taking the first match answers for the wrong one: the live
    session's host and the `tdclip` and `attention` hosts are three different builds
    on this box, and the first `pgrep` hit was none of the one being asked about.
    """
    out = []
    try:
        pids = subprocess.run(["pgrep", "-f", "serve --session"],
                              capture_output=True, text=True, timeout=30).stdout.split()
    except Exception:
        return out
    for pid in pids:
        try:
            exe = os.path.realpath(f"/proc/{pid}/exe")
            with open(f"/proc/{pid}/cmdline", "rb") as f:
                args = [a for a in f.read().decode().split("\0") if a]
        except OSError:
            continue
        if "terminal-delight" not in exe or "serve" not in args:
            continue
        session = args[args.index("--session") + 1] if "--session" in args[:-1] else "?"
        base = os.path.basename(exe).replace(" (deleted)", "")
        started = subprocess.run(["ps", "-o", "lstart=", "-p", pid],
                                 capture_output=True, text=True).stdout.strip()
        sha = base.split("-")[1] if base.count("-") >= 1 else ""
        behind = git("rev-list", "--count", "--merges", f"{sha}..origin/main") if sha else ""
        out.append({"pid": int(pid), "exe": exe, "build": base, "started": started,
                    "session": session, "behind": behind or "?",
                    "installed": exe.startswith(LIB) and base.startswith("td-")})
    return sorted(out, key=lambda h: h["session"])


WINDOWS = live_windows()
# Which session is the one being asked about. The window's own instance names it;
# $TD_SESSION names the pane this script is running in, which is the same thing here
# and the honest fallback when there is no window to ask.
OUR_SESSION = os.environ.get("TD_SESSION", "1")
# Which window is drawing THIS page's reader? The newest one, unless told otherwise.
_want = os.environ.get("TD_WINDOW_PID")
_mine = ([w for w in WINDOWS if str(w["pid"]) == _want] or
         sorted(WINDOWS, key=lambda w: w["ticks"])[-1:] or [None])[0]
RUNNING = (_mine or {}).get("build") or os.environ.get("TD_RUNNING_BUILD", "unknown")

# A window running out of a worktree ADOPTS THE LIVE SESSION — that is the whole finding
# above. So it is not enough for the window we measured to be an installed build: any
# stray worktree window alive on this box can be the one holding the panes. Reporting
# only the newest made the exact hazard this code exists to catch invisible, because
# picking by highest pid happened to pick the good one while the bad one kept running.
STRAY = [w for w in WINDOWS if not w["installed"] and w is not _mine]
RUNNING_OK = bool(_mine and _mine["installed"]) and not STRAY


def git(*a, default=""):
    try:
        return subprocess.run(["git", "-C", str(REPO), *a], capture_output=True,
                              text=True, timeout=90).stdout.strip()
    except Exception:
        return default


def gh_json(*a, default=None):
    try:
        out = subprocess.run([GH, *a], capture_output=True, text=True,
                             cwd=str(REPO), timeout=180).stdout
        return json.loads(out or "null") if out.strip() else (default or [])
    except Exception:
        return default if default is not None else []


def merges_clean(branch):
    p = subprocess.run(["git", "-C", str(REPO), "merge-tree", "--write-tree",
                        "origin/main", f"origin/{branch}"],
                       capture_output=True, text=True, timeout=180)
    if p.returncode == 0:
        return True, []
    return False, [l.split("Merge conflict in ", 1)[1]
                   for l in p.stdout.splitlines() if "Merge conflict in " in l]


git("fetch", "origin", "-q", "--prune")
HOSTS = live_hosts()          # needs git(), so it runs after the fetch, not at import
items_cfg = json.loads((HERE / "_rodeo_items.json").read_text())
PANES = items_cfg.get("_panes", {})

main_sha = git("rev-parse", "--short", "origin/main")
main_tree = git("rev-parse", "origin/main^{tree}")
commits24 = git("rev-list", "--count", "--since=24 hours ago", "origin/main") or "?"

installed = os.path.basename(os.path.realpath("/home/parker/.local/bin/terminal-delight"))
inst_sha = installed.split("-")[1] if installed.count("-") >= 1 else ""
inst_tree = git("rev-parse", f"{inst_sha}^{{tree}}") if inst_sha else ""
same_tree = bool(inst_tree) and inst_tree == main_tree

def running_commit():
    """The commit the drawing window was actually built from.

    An installed build carries it in its name (`td-<sha>-<label>`). A worktree build
    does not — its name is the bare crate name — so resolve it from the checkout the
    binary sits in. Getting this wrong is not cosmetic: run_sha decides which merged
    pull requests count as "installed but not yet on screen", and an unresolvable sha
    silently promoted all 41 of them into section A.
    """
    if _mine and _mine["installed"]:
        parts = _mine["build"].split("-")
        return (parts[1] if len(parts) > 1 else ""), "the build's own name"
    if _mine:
        p = _mine["exe"]
        for _ in range(6):                      # …/<worktree>/app/target/debug/<bin>
            p = os.path.dirname(p)
            if os.path.exists(os.path.join(p, ".git")):
                sha = subprocess.run(["git", "-C", p, "rev-parse", "HEAD"],
                                     capture_output=True, text=True).stdout.strip()
                if sha:
                    return sha, f"HEAD of {os.path.basename(p)}, the worktree it was built in"
    return "", ""


run_sha, run_sha_from = running_commit()
# Unknown is not zero and it is not "everything" either: with no resolvable commit we
# cannot say what the window is missing, so we say that rather than inventing a list.
run_known = bool(run_sha) and git("cat-file", "-t", run_sha) == "commit"
behind_merges = (git("rev-list", "--count", "--merges", f"{run_sha}..origin/main")
                 if run_known else "unknown")


def _ppid(pid):
    try:
        with open(f"/proc/{pid}/status") as f:
            for line in f:
                if line.startswith("PPid:"):
                    return int(line.split()[1])
    except OSError:
        pass
    return None


def resolve_panes():
    """resume uuid -> {pane pid, agent pid, peer name}, read live.

    Three hops, all exact: ~/.claude/sessions/<agent_pid>.json carries `sessionId`, the
    agent process is a direct child of the pane shell, and the same file carries the
    `name` that SendMessage takes. Resolving at build time is the point — pids do not
    survive a window restart and the uuid does.
    """
    found = {}
    sess = os.path.expanduser("~/.claude/sessions")
    try:
        files = os.listdir(sess)
    except OSError:
        return found
    for fn in files:
        if not fn.endswith(".json"):
            continue
        try:
            with open(os.path.join(sess, fn)) as f:
                d = json.load(f)
        except (OSError, ValueError):
            continue
        sid, apid = d.get("sessionId"), d.get("pid")
        if not sid or not apid or not os.path.isdir(f"/proc/{apid}"):
            continue
        # One sessionId can have TWO live agents at once: the session host keeps the old
        # window's panes running while a new window spawns fresh ones on the same resume
        # uuid. Both are alive, so "is it running" cannot pick between them — order by
        # the file's own startedAt. NOT by pid: these pids wrap, and the stale agent's
        # (2058113) is numerically larger than the current one's (396727), so a pid
        # comparison confidently picked the window Parker is not looking at.
        started = d.get("startedAt") or 0
        prev = found.get(sid)
        if prev and prev["started"] >= started:
            continue
        found[sid] = {"agent": apid, "pane": _ppid(apid), "peer": d.get("name"),
                      "started": started}
    return found


LIVE = resolve_panes()
for _k, _v in PANES.items():
    if _k.startswith("_"):
        continue
    _v.update(LIVE.get(_v.get("resume", ""), {}))
# A peer name that resolves to two panes cannot be used to address either of them.
_seen = {}
for _k, _v in PANES.items():
    if not _k.startswith("_") and _v.get("peer"):
        _seen.setdefault(_v["peer"], []).append(_k)
AMBIG = {n for n, ks in _seen.items() if len(ks) > 1}


def owner_html(key):
    """Parker names panes in the left bar; pids are what actually address one."""
    p = PANES.get(key or "")
    if not p:
        return '<span class="mut">owner not established</span>'
    tag = {"measured": "v", "inferred": "w"}.get(p.get("confidence", ""), "c")
    grp = f' &middot; {E(p["group"])}' if p.get("group") else ""
    where = (f' &middot; pid {p["pane"]}' if p.get("pane")
             else ' &middot; <span class="mut">not running</span>')
    return (f'<b>{E(key)}</b>{grp}{where} '
            f'<span class="tag {tag}">{E(p.get("confidence", "unknown"))}</span>')


def first_para(body):
    """The pull request's own first claim, not my paraphrase."""
    for chunk in (body or "").split("\n\n"):
        t = " ".join(chunk.split())
        if t and not t.startswith(("#", "|", "-", "*", ">", "`")) and len(t) > 40:
            return t[:420]
    return ""


ROUTE = {}          # owner tab name -> [item numbers], built as items render
SECTION = {}        # item number -> A|B|C|D, so the fleet map can colour a tab


def render_item(n, pr, state, blocked, badge, cfg_key=None):
    """One checklist item. `state` is the line under the title; `badge` gates the number."""
    cfg = items_cfg.get(str(cfg_key if cfg_key is not None else n), {})
    ROUTE.setdefault(cfg.get("owner") or "", []).append(n)
    auto = "do" not in cfg
    title = cfg.get("title") or pr.get("title", f"Pull request {n}")
    why = cfg.get("why") or (first_para(pr.get("body", "")) if auto else "")
    do = cfg.get("do") or "No hand-written steps for this one yet. Read the pull request, exercise what it claims, and tell me what you did — I will write the step from your words."
    passq = cfg.get("pass", "")
    cls = "finding blocked" if blocked else ("finding live" if badge == "live" else "finding")
    out = [f'<div class="{cls}">',
           f'  <div class="hd"><span class="num">{n}</span><div>',
           f'    <h4>{title}</h4>',
           f'    <p class="who">{owner_html(cfg.get("owner"))} &middot; {state}</p>',
           '  </div></div>', '  <div class="bd">']
    if why:
        out.append(f'    <p>{why}</p>')
    if auto:
        out.append('    <p class="mut" style="font-size:12.5px">The claim above is the pull '
                   'request\'s own first paragraph. <b>The steps below are auto-derived, not '
                   'written by anyone</b> — treat them as a prompt, not a script.</p>')
    out.append(f'    <div class="do"><b>Do:</b> {do}</div>')
    if passq:
        out.append(f'    <p class="pass"><b>Pass:</b> {passq}</p>')
    if cfg.get("note"):
        out.append(f'    <p class="mut">{cfg["note"]}</p>')
    if cfg.get("relayed"):
        out.append(f'    <p class="mut" style="font-size:12.5px">{cfg["relayed"]}</p>')
    out += ['  </div>', '  <div class="verdicts">',
            '    <button class="vp" data-v="PASS">&#10003; pass</button>'
            '<button class="vf" data-v="FAIL &mdash; ">&#10007; fail</button>'
            '<button class="vo" data-v="ODD &mdash; ">~ odd</button>'
            '<button class="vs" data-v="SKIPPED &mdash; ">skip</button>',
            '    <span class="mark"></span>', '  </div>', '</div>']
    return "\n".join(out)


# ---- gather -----------------------------------------------------------------------
openprs = gh_json("pr", "list", "--limit", "60", "--json",
                  "number,headRefName,title,body,files")
mergedprs = gh_json("pr", "list", "--state", "merged", "--limit", "40", "--json",
                    "number,headRefName,title,body,mergedAt,mergeCommit")

# A merged pull request whose commit is in main but NOT in the running build is a
# feature that exists, is installed, and is invisible until the window restarts.
relaunch = []
for pr in mergedprs:
    oid = (pr.get("mergeCommit") or {}).get("oid", "")
    if not oid or not run_known:
        continue
    in_main = subprocess.run(["git", "-C", str(REPO), "merge-base", "--is-ancestor",
                              oid, "origin/main"], capture_output=True).returncode == 0
    in_run = subprocess.run(["git", "-C", str(REPO), "merge-base", "--is-ancestor",
                             oid, run_sha], capture_output=True).returncode == 0
    if in_main and not in_run:
        relaunch.append(pr)
relaunch.sort(key=lambda p: p.get("mergedAt", ""))

clean, conflicted, rows = [], [], []
for pr in sorted(openprs, key=lambda p: p["number"]):
    ok, paths = merges_clean(pr["headRefName"])
    pr["_clean"], pr["_conf"] = ok, paths
    (clean if ok else conflicted).append(pr)
    src = [f["path"] for f in pr.get("files", []) if f["path"].startswith("app/src/")]
    shown = ", ".join(os.path.basename(f) for f in src[:4]) or "docs only"
    if len(src) > 4:
        shown += f", +{len(src) - 4}"
    cell = ('<td class="ok">clean</td>' if ok
            else f'<td class="bad">conflict: {E(", ".join(paths))}</td>')
    rows.append(f'<tr><td><b>#{pr["number"]}</b></td>'
                f'<td><code>{E(pr["headRefName"])}</code></td>{cell}'
                f'<td class="mut">{E(shown)}</td></tr>')

# Spot-checks: curated entries for pull requests already in the running build.
running_extra = [n for n in items_cfg
                 if n.isdigit()
                 and not any(int(n) == p["number"] for p in openprs)
                 and not any(int(n) == p["number"] for p in relaunch)]
running_extra.sort(key=int, reverse=True)

_gate_why = ("" if RUNNING_OK else f"""
    <p><b>This already went wrong once tonight, which is why the step is worded like this.</b>
      A relaunch landed on a <b>debug build out of a worktree</b>. That window ignored the
      symlink, adopted the live session, and drew every pane in whatever branch that worktree
      was on, which was pre-#555. The chrome then photographed and complained about was the old
      chrome. <b>The install was never the problem.</b></p>""")

_winlist = "".join(
    f'<tr><td><code>{w["pid"]}</code></td>'
    f'<td>{"<b>measured as yours</b>" if w is _mine else "<span class=\'mut\'>also running</span>"}</td>'
    f'<td class="{"ok" if w["installed"] else "bad"}">{E(w["build"])}</td>'
    f'<td class="mut"><code>{E(w["exe"])}</code></td></tr>' for w in WINDOWS)
_gate_windows = (f"""
    <p><b>Every Terminal Delight window alive on this box right now</b>, read from
      <code>/proc/&lt;pid&gt;/exe</code> rather than declared. A window whose binary is not under
      <code>~/.local/lib/terminal-delight</code> can hold your panes:</p>
    <div class="scroller"><table class="wide" style="min-width:460px"><thead><tr>
      <th>pid</th><th>which</th><th>build</th><th>binary</th></tr></thead>
      <tbody>{_winlist}</tbody></table></div>
    {'<p class="mut">' + str(len(STRAY)) + ' of these is a worktree build. Close it before you judge anything on this page.</p>' if STRAY else ''}""")

GATE_ZERO = f"""<div class="finding {'live' if RUNNING_OK else 'blocked'}">
  <div class="hd"><span class="num">1</span><div>
    <h4>Reopen the window <em>from the launcher</em>, not from a worktree</h4>
    <p class="who"><b>OVERSEER!</b> hands this one to you &middot; gates the rest of section A
      &middot; costs a restart, not a build</p>
  </div></div>
  <div class="bd">{_gate_why}{_gate_windows}
    <div class="do"><b>Do:</b> quit Terminal Delight, then start it again <b>by running
      <code>terminal-delight</code> on your PATH</b> — never <code>./target/debug/terminal-delight</code>
      or any path inside a worktree. The session host owns the PTYs, so your agent panes survive it.</div>
    <p class="pass"><b>Pass:</b> ask any pane's agent for a Terminal Delight MCP call and read the
      <code>instance.build</code> it answers with. It must read <code>{E(installed)}</code>.
      <b>If it reads the bare word <code>terminal-delight</code> with no sha, a worktree build has
      the session</b> and nothing below this line is testing what you think it is.</p>
  </div>
  <div class="verdicts">
    <button class="vp" data-v="PASS">&#10003; pass</button><button class="vf" data-v="FAIL &mdash; ">&#10007; fail</button><button class="vo" data-v="ODD &mdash; ">~ odd</button><button class="vs" data-v="SKIPPED &mdash; ">skip</button>
    <span class="mark"></span>
  </div>
</div>"""

k = 1
SECTION[1] = "A"
sec_a = [GATE_ZERO]
for pr in relaunch:
    k += 1
    SECTION[k] = "A"
    sec_a.append(render_item(
        k, pr, f'merged as #{pr["number"]} &middot; installed, waiting on your restart',
        False, "live", cfg_key=pr["number"]))

sec_b, sec_c = [], []
for pr in clean:
    k += 1
    SECTION[k] = "B"
    sec_b.append(render_item(
        k, pr, f'#{pr["number"]} &middot; <code>{E(pr["headRefName"])}</code> '
        f'&middot; merges clean, needs a build', True, "", cfg_key=pr["number"]))
for pr in conflicted:
    k += 1
    SECTION[k] = "C"
    sec_c.append(render_item(
        k, pr, f'#{pr["number"]} &middot; <code>{E(pr["headRefName"])}</code> '
        f'&middot; conflicts on {E(", ".join(pr["_conf"]))}', True, "", cfg_key=pr["number"]))

sec_d = []
for n in running_extra:
    k += 1
    SECTION[k] = "D"
    sec_d.append(render_item(
        k, {}, f'pull request {n} &middot; already in the build you are running',
        False, "", cfg_key=n))

# ---- the fleet map: your own left bar, with what each tab shipped tonight ----------
STATE = {
    "A": ("shipped", "s-a", "merged and installed — relaunch to see it"),
    "B": ("waiting", "s-b", "pull request open, merges clean"),
    "C": ("stuck", "s-c", "pull request open, conflicting"),
    "D": ("landed", "s-d", "already in the build you are running"),
}
groups, seen = [], set()
for name, meta in PANES.items():
    if name.startswith("_"):
        continue
    g = meta.get("group") or "— ungrouped"
    if g not in seen:
        seen.add(g)
        groups.append(g)
fleet = []
for g in groups:
    rows = []
    for name, meta in PANES.items():
        if name.startswith("_") or (meta.get("group") or "— ungrouped") != g:
            continue
        nums = sorted(ROUTE.get(name, []))
        secs = [SECTION.get(n) for n in nums]
        pick = next((s for s in ("C", "B", "A", "D") if s in secs), None)
        if name == "OVERSEER!":
            label, cls, _ = ("you are here", "s-me", "")
        elif pick:
            label, cls, _ = STATE[pick]
            label = f'{label} &middot; {", ".join(str(n) for n in nums)}'
        else:
            label, cls = "nothing on this page", "s-none"
        rows.append(f'<div class="fl-tab {cls}"><span class="fl-n">{E(name)}</span>'
                    f'<span class="fl-s">{label}</span></div>')
    fleet.append(f'<div class="fl-g"><div class="fl-gh">{E(g)}</div>{"".join(rows)}</div>')
fleet_html = "".join(fleet)

# routing: which items land on which pane, by the name Parker typed in the left bar,
# and — since 2026-09-18 — by the peer name SendMessage actually takes.
# resolve_panes() above already did the three hops off the resume uuid, so the peer name
# arrives with the pane rather than being looked up again from a pid that may be dead.
peer_of, peer_count = {}, {}
for _name, _meta in PANES.items():
    if _name.startswith("_"):
        continue
    _n = _meta.get("peer")
    _why = ("measured" if _n else
            "this pane is not running — its uuid resolves to no live agent")
    peer_of[_name] = (_n, _why)
    if _n:
        peer_count[_n] = peer_count.get(_n, 0) + 1

route_rows = []
for name, meta in PANES.items():
    if name.startswith("_") or name == "OVERSEER!":
        continue
    nums = ROUTE.get(name, [])
    if not nums:
        continue
    tag = {"measured": "v", "inferred": "w"}.get(meta.get("confidence", ""), "c")
    pname, pwhy = peer_of.get(name, (None, ""))
    if pname and peer_count.get(pname, 0) > 1:
        # a name two panes answer to is a coin flip; say so instead of sending
        peer_cell = (f'<code>{E(pname)}</code> '
                     f'<span class="tag c">shared by {peer_count[pname]}</span>')
        how = "note + surface only"
    elif pname:
        peer_cell = f'<code>{E(pname)}</code>'
        how = "note, surface, or a prompt"
    else:
        peer_cell = f'<span class="mut">{E(pwhy)}</span>'
        how = "note + surface"
    pane_pid = meta.get("pane")
    pid_cell = (f'<code>{pane_pid}</code>' if pane_pid
                else '<span class="mut">not running</span>')
    route_rows.append(
        f'<tr><td>{", ".join(str(x) for x in sorted(nums))}</td>'
        f'<td><b>{E(name)}</b></td><td class="mut">{E(meta.get("group") or "—")}</td>'
        f'<td>{pid_cell}</td>'
        f'<td>{peer_cell}</td>'
        f'<td>{how}</td>'
        f'<td><span class="tag {tag}">{E(meta.get("confidence", "unknown"))}</span></td></tr>')
orphans = sorted(ROUTE.get("", []))
if orphans:
    route_rows.append(
        f'<tr><td class="mut">{", ".join(str(x) for x in orphans)}</td>'
        f'<td class="mut">no pane established</td><td class="mut">—</td>'
        f'<td class="mut">—</td><td class="mut">—</td>'
        f'<td class="mut">a falsifiable issue</td>'
        f'<td><span class="tag c">unattributed</span></td></tr>')

n_peer = sum(1 for n, _ in peer_of.values() if n)
n_amb = sum(1 for n, _ in peer_of.values() if n and peer_count.get(n, 0) > 1)

tiles = "".join([
    f'<div class="tile r"><div class="n">{E(RUNNING.replace("td-", "").split("-")[0])}</div>'
    f'<div class="k">the build this window is running</div></div>',
    f'<div class="tile g"><div class="n">{E(inst_sha or "?")}</div>'
    f'<div class="k">installed on disk, one relaunch away</div></div>',
    f'<div class="tile b"><div class="n">{E(main_sha)}</div><div class="k">tip of main</div></div>',
    f'<div class="tile g"><div class="n">{len(clean)}</div>'
    f'<div class="k">open pull requests that merge clean</div></div>',
    f'<div class="tile a"><div class="n">{len(conflicted)}</div>'
    f'<div class="k">blocked on a conflict</div></div>',
    f'<div class="tile m"><div class="n">{commits24}</div>'
    f'<div class="k">commits onto main in 24h</div></div>',
])

n_auto = sum(1 for pr in openprs + relaunch if str(pr["number"]) not in items_cfg)
stamp = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")

sub = {
    "__STAMP__": stamp, "__TILES__": tiles, "__PRROWS__": "\n      ".join(rows),
    "__RUNNING__": E(RUNNING), "__INSTALLED__": E(installed), "__MAIN__": E(main_sha),
    "__BEHINDN__": behind_merges, "__COMMITS24__": commits24,
    "__TREEMSG__": "same program as installed" if same_tree else "differs from installed",
    "__TREEVERDICT__": ("identical trees — a build of one is a build of the other" if same_tree
                        else "the installed binary is NOT the tip of main — a build is needed"),
    "__SEC_A__": "\n".join(sec_a), "__SEC_B__": "\n".join(sec_b) or
        '<p class="sub">Nothing here right now — every open pull request either conflicts or has already merged.</p>',
    "__SEC_C__": "\n".join(sec_c) or
        '<p class="sub">Nothing blocked. Every open pull request merges onto main without help.</p>',
    "__SEC_D__": "\n".join(sec_d),
    "__ROUTEROWS__": "\n      ".join(route_rows),
    "__NA__": str(len(relaunch) + 1), "__NB__": str(len(sec_b)), "__NC__": str(len(sec_c)),
    "__ND__": str(len(sec_d)), "__NAUTO__": str(n_auto),
    "__NPEER__": str(n_peer), "__NPANES__": str(len([k for k in PANES if not k.startswith("_")])),
    "__NAMB__": str(n_amb), "__FLEETMAP__": fleet_html,
    # Section A means two different things and the heading has to say which. While
    # something is merged and installed but not in the running window, A is a queue
    # behind a restart. Once the window IS the tip of main, A is empty and telling
    # the reader to relaunch is simply wrong.
    "__AHEAD__": ("Quit and reopen, then test these" if relaunch
                  else "Nothing is hiding behind a restart"),
    "__ALEAD__": (f"Already built, already installed. The only thing between you and these "
                  f"{len(relaunch)} is a restart."
                  if relaunch else
                  f"Your window is running <code>{E(RUNNING)}</code>, which is the tip of main — "
                  f"same tree, compared by oid rather than by sha. <b>Every merged feature is in "
                  f"front of you right now</b>, so go to section D, which is the {len(sec_d)} "
                  f"things you can put a finger on this second. This section keeps one item: the "
                  f"check that the sentence above is still true."),
    "__WINDOWROWS__": "\n      ".join(
        f'<tr><td><code>{w["pid"]}</code>{" &larr; yours" if _mine and w["pid"] == _mine["pid"] else ""}</td>'
        f'<td class="mut">{E(w["started"][4:20] if len(w["started"]) > 20 else w["started"])}</td>'
        f'<td class="{"ok" if w["installed"] else "bad"}"><code>{E(w["build"])}</code></td>'
        f'<td class="{"ok" if w["installed"] else "bad"}">'
        f'{"from the launcher" if w["installed"] else "FROM A WORKTREE — ignores the symlink"}</td>'
        f'</tr>' for w in WINDOWS) or
        '<tr><td colspan="4" class="mut">no window process found — this page was generated '
        'somewhere without one</td></tr>',
    # The host is a THIRD build and it is not a window. It owns every pane's
    # pseudoterminal, survives a window restart by design, and therefore only
    # upgrades by dying — which it cannot do without taking every pane with it.
    # Reporting the window alone made a page that said "you are current" while a
    # third of the running program was days old.
    "__HOSTROW__": "\n      ".join(
        f'<tr><td><code>{h["pid"]}</code> '
        f'<span class="mut">host &middot; {E(h["session"])}</span>'
        f'{" &larr; yours" if h["session"] == OUR_SESSION else ""}</td>'
        f'<td class="mut">{E(h["started"][4:20] if len(h["started"]) > 20 else h["started"])}</td>'
        f'<td class="{"ok" if h["behind"] == "0" else "warn"}"><code>{E(h["build"])}</code></td>'
        f'<td class="{"ok" if h["behind"] == "0" else "warn"}">'
        + ("current" if h["behind"] == "0" else
           f'<b>{E(h["behind"])} merges behind</b> — owns every PTY in session '
           f'<code>{E(h["session"])}</code>, so it only upgrades by killing every pane '
           f'in it. A window restart does not touch it.')
        + '</td></tr>' for h in HOSTS) or
        '<tr><td colspan="4" class="mut">no <code>serve --session</code> process — either '
        'this window owns its panes directly, or the host could not be read</td></tr>',
    "__NWINDOWS__": str(len(WINDOWS)),
    "__WINVERDICT__": ("the window drawing your panes is an installed build" if RUNNING_OK else
                       "the window drawing your panes is NOT an installed build"),
}

body = (HERE / "_rodeo_body.html").read_text()
for t, v in sub.items():
    body = body.replace(t, v)
left = sorted(set(re.findall(r"__[A-Z_0-9]+__", body)))
if left:
    raise SystemExit(f"unsubstituted tokens in the body: {left}")

base_css = (SKILL / "assets/base.css").read_text()
notes_css = (SKILL / "assets/notes.css").read_text()
notes_js = (SKILL / "assets/notes.js").read_text()
local_css = (HERE / "_rodeo_style.css").read_text()
markup = (SKILL / "reference/notes-markup.html").read_text()
if "</script" in notes_js.lower():
    raise SystemExit("notes.js contains a literal closing script tag; escape it before inlining")
notes_markup = markup[markup.index('<div class="notebar"'):
                      markup.index('<script>', markup.index('<div class="notebar"'))].rstrip()

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

OUT.write_text(f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>The rodeo — live-test checklist, terminal-delight</title>
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
{verdict_js}
</script>
</body>
</html>
""")
print(f"wrote {OUT} ({OUT.stat().st_size} bytes)")
print(f"  main {main_sha} · installed {installed} · running {RUNNING} ({behind_merges} merges behind)"
      f" · trees {'MATCH' if same_tree else 'DIFFER'}")
print(f"  A relaunch-and-test {len(relaunch) + 1} · B needs-a-build {len(sec_b)} ·"
      f" C blocked {len(sec_c)} · D spot-checks {len(sec_d)}   ({n_auto} auto-derived)")
