#!/usr/bin/env python3
"""The two safety questions a worktree/branch cleanup has to answer first.

1. Is a live process STANDING IN this worktree? Removing a directory that is some
   agent's cwd breaks that agent in a way it cannot diagnose — every later command
   fails with a path error and nothing says why.
2. How old is each residue branch? "Every commit is already upstream" is true of a
   branch someone pushed ten minutes ago and is still building on. Age is the cheap
   proxy for "nobody is mid-thought here".
"""
import os
import subprocess
import time
from pathlib import Path

R = "/home/parker/Work/terminal-delight"


def sh(*a):
    return subprocess.run(a, capture_output=True, text=True, cwd=R, timeout=180).stdout


worktrees = [l.split(" ", 1)[1] for l in sh("git", "worktree", "list", "--porcelain").splitlines()
             if l.startswith("worktree ")]

occupancy = {w: [] for w in worktrees}
for pid in os.listdir("/proc"):
    if not pid.isdigit():
        continue
    try:
        cwd = os.readlink(f"/proc/{pid}/cwd")
        comm = Path(f"/proc/{pid}/comm").read_text().strip()
    except OSError:
        continue
    for w in worktrees:
        if cwd == w or cwd.startswith(w + "/"):
            occupancy[w].append(f"{comm}({pid})")

print("════ WORKTREE OCCUPANCY — is anyone standing in it? ════")
for w in sorted(worktrees):
    who = occupancy[w]
    tag = "OCCUPIED" if who else "empty   "
    print(f"  {tag}  {os.path.basename(w):<22} {', '.join(who[:4]) if who else '—'}"
          + (f" +{len(who)-4} more" if len(who) > 4 else ""))

print()
print("════ RESIDUE REMOTE BRANCHES BY AGE ════")
now = time.time()
refs = [r for r in sh("git", "for-each-ref", "--format=%(refname:short)",
                      "refs/remotes/origin/").split()
        if r not in ("origin/HEAD", "origin/main")]
old, recent = [], []
for r in refs:
    cnt = sh("git", "rev-list", "--count", f"origin/main..{r}").strip()
    if cnt and cnt != "0":
        if sh("git", "cherry", "origin/main", r).count("\n+"):
            continue                                   # holds real work — not residue
        if sh("git", "cherry", "origin/main", r).startswith("+"):
            continue
    ts = sh("git", "log", "-1", "--format=%ct", r).strip()
    age_h = (now - int(ts)) / 3600 if ts.isdigit() else 1e9
    (recent if age_h < 24 else old).append((r[len("origin/"):], age_h))

print(f"  older than 24h  : {len(old)}   — safe to delete")
print(f"  touched today   : {len(recent)} — leave them, somebody may still be pushing")
for b, a in sorted(recent, key=lambda x: x[1])[:12]:
    print(f"      {b:<48} {a:.1f}h")
