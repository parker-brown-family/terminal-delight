#!/usr/bin/env python3
"""Clean up what a long multi-agent session leaves behind. DRY RUN unless --apply.

Removes only things that are provably dead:

  binaries   every installed build except the newest N, the symlink target, and
             anything a live process is executing (old MCP relays hold old builds)
  sockets    ctl-<pid>.sock whose pid is gone
  worktrees  clean AND fully contained in main AND with no process standing in it
  branches   remote: patch-equivalent to main, no open PR, untouched for 24h
             local:  contained in main, not checked out anywhere

Two guards are the whole point, and each caught something real on first run:

  OCCUPANCY  a worktree that is some agent's cwd must never be removed. Deleting
             it breaks that agent in a way it cannot diagnose — every later
             command fails on a missing path and nothing says why. `td-outer-paint`
             was clean, merged, and had fifteen live processes in it.
  AGE        "every commit is already upstream" is equally true of a branch
             somebody pushed ten minutes ago and is still building on. 31 of the
             68 residue branches had been touched that day.
"""
import os
import subprocess
import sys
import time
from pathlib import Path

R = "/home/parker/Work/terminal-delight"
LIB = Path("/home/parker/.local/lib/terminal-delight")
SOCKS = Path("/run/user/1000/terminal-delight")
KEEP = 5
MIN_AGE_H = 24
APPLY = "--apply" in sys.argv


def sh(*a, cwd=R):
    return subprocess.run(a, capture_output=True, text=True, cwd=cwd, timeout=300).stdout


def act(did, would, *cmd, cwd=R):
    if APPLY:
        subprocess.run(cmd, capture_output=True, text=True, cwd=cwd, timeout=300)
        print(f"  ✓ {did}")
    else:
        print(f"  · would {would}")


sh("git", "fetch", "origin", "-q", "--prune")
freed = 0

# ── 1. binaries ──────────────────────────────────────────────────────────────
print("════ INSTALLED BINARIES ════")
live = os.path.realpath("/home/parker/.local/bin/terminal-delight")
in_use = {live}
for pid in os.listdir("/proc"):
    if pid.isdigit():
        try:
            in_use.add(os.path.realpath(f"/proc/{pid}/exe"))
        except OSError:
            pass
held = sorted(p.name for p in LIB.iterdir() if str(p) in in_use)
print(f"  protected: the live symlink plus {len(held)} build(s) a process is still executing")

cands = sorted((p for p in LIB.iterdir() if str(p) not in in_use),
               key=lambda p: p.stat().st_mtime, reverse=True)
for p in cands[KEEP:]:
    freed += p.stat().st_size
    if APPLY:
        p.unlink()
print(f"  {'removed' if APPLY else 'would remove'} {max(0, len(cands) - KEEP)} of "
      f"{len(list(LIB.iterdir())) if not APPLY else '…'} binaries, "
      f"freeing {freed / 1073741824:.2f} GB")

# ── 2. sockets ───────────────────────────────────────────────────────────────
print("\n════ DEAD CTL SOCKETS ════")
dead = [s for s in SOCKS.glob("ctl-*.sock")
        if not Path(f"/proc/{s.stem.removeprefix('ctl-')}").exists()]
for s in dead:
    if APPLY:
        s.unlink()
print(f"  {'removed' if APPLY else 'would remove'} {len(dead)} socket(s) whose window is gone")

# ── 3. worktrees ─────────────────────────────────────────────────────────────
print("\n════ WORKTREES ════")
trees = [l.split(" ", 1)[1] for l in sh("git", "worktree", "list", "--porcelain").splitlines()
         if l.startswith("worktree ")]
busy = {w: 0 for w in trees}
for pid in os.listdir("/proc"):
    if not pid.isdigit():
        continue
    try:
        cwd = os.readlink(f"/proc/{pid}/cwd")
    except OSError:
        continue
    for w in trees:
        if cwd == w or cwd.startswith(w + "/"):
            busy[w] += 1

for w in sorted(trees):
    if w == R:
        continue
    name = os.path.basename(w)
    dirty = len([l for l in sh("git", "status", "--porcelain", cwd=w).splitlines() if l])
    head = sh("git", "rev-parse", "HEAD", cwd=w).strip()
    inmain = subprocess.run(["git", "merge-base", "--is-ancestor", head, "origin/main"],
                            cwd=R, capture_output=True).returncode == 0
    if busy[w]:
        print(f"  KEEP {name:<22} {busy[w]} live process(es) standing in it")
    elif dirty:
        print(f"  KEEP {name:<22} {dirty} uncommitted file(s) — not mine to discard")
    elif not inmain:
        print(f"  KEEP {name:<22} HEAD is not in main")
    else:
        act(f"removed {name}", f"remove {name} (clean, merged, empty)",
            "git", "worktree", "remove", "--force", w)

# ── 4. branches ──────────────────────────────────────────────────────────────
print("\n════ REMOTE BRANCHES — residue, no open PR, untouched 24h ════")
try:
    open_prs = set(sh("/home/parker/bin/gh", "pr", "list", "--limit", "80",
                      "--json", "headRefName", "--jq", ".[].headRefName").split())
except Exception:
    open_prs = set()
checked_out = {l.split("refs/heads/")[1] for l in sh("git", "worktree", "list", "--porcelain").splitlines()
               if l.startswith("branch ")}
now, gone = time.time(), 0
for r in sh("git", "for-each-ref", "--format=%(refname:short)", "refs/remotes/origin/").split():
    if r in ("origin/HEAD", "origin/main"):
        continue
    br = r[len("origin/"):]
    if br in open_prs or br in checked_out:
        continue
    cnt = sh("git", "rev-list", "--count", f"origin/main..{r}").strip()
    if cnt and cnt != "0" and any(l.startswith("+") for l in sh("git", "cherry", "origin/main", r).splitlines()):
        continue                                        # holds real work
    ts = sh("git", "log", "-1", "--format=%ct", r).strip()
    if not ts.isdigit() or (now - int(ts)) / 3600 < MIN_AGE_H:
        continue                                        # somebody may still be pushing
    gone += 1
    if APPLY:
        subprocess.run(["git", "push", "-q", "origin", "--delete", br],
                       cwd=R, capture_output=True, timeout=300)
print(f"  {'deleted' if APPLY else 'would delete'} {gone} remote branch(es)")

print("\n════ LOCAL BRANCHES CONTAINED IN MAIN ════")
cur = sh("git", "branch", "--show-current").strip()
n = 0
for b in sh("git", "for-each-ref", "--format=%(refname:short)", "refs/heads/").split():
    if b in (cur, "main") or b in checked_out:
        continue
    if subprocess.run(["git", "merge-base", "--is-ancestor", b, "origin/main"],
                      cwd=R, capture_output=True).returncode != 0:
        continue
    n += 1
    if APPLY:
        subprocess.run(["git", "branch", "-D", b], cwd=R, capture_output=True, timeout=120)
print(f"  {'deleted' if APPLY else 'would delete'} {n} local branch(es)")

print("""
════ DELIBERATELY UNTOUCHED ════
  · five orphan branches holding real unlanded work — issue #588
  · shorts-pipeline — the filming rig, held out of main by decision
  · the 2026-09-15 stash — deliberately parked WIP
  · every worktree with uncommitted changes, or with anyone standing in it
  · every branch with an open pull request, or touched in the last 24h""")
