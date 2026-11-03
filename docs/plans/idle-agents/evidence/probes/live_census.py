#!/usr/bin/env python3
"""Read-only census of a session's pane groups: which are frozen, and what each
pane's processes hold in RAM (PSS) and swap (SwapPss). Changes nothing.
    live_census.py [session]   (default 1)"""
import glob, os, re, sys

session = sys.argv[1] if len(sys.argv) > 1 else "1"
base = f"/sys/fs/cgroup/user.slice/user-{os.getuid()}.slice/user@{os.getuid()}.service/app.slice"

def read(p):
    try:
        return open(p).read()
    except OSError:
        return ""

def pss(pid):
    t = read(f"/proc/{pid}/smaps_rollup")
    g = lambda k: int((re.search(rf"^{k}:\s+(\d+)", t, re.M) or [0, 0])[1])
    return g("Pss") * 1024, g("SwapPss") * 1024

def agent(procs):
    for pid in procs:
        c = read(f"/proc/{pid}/comm").strip()
        if c in ("claude", "codex"):
            return c
    return "-"

rows = []
for d in sorted(glob.glob(f"{base}/td-pane-{session}-*.scope")):
    procs = [int(x) for x in read(f"{d}/cgroup.procs").split()]
    frozen = "frozen 1" in read(f"{d}/cgroup.events")
    ram = sum(pss(p)[0] for p in procs)
    swp = sum(pss(p)[1] for p in procs)
    pane = os.path.basename(d).split("-")[3]
    rows.append((pane, agent(procs), frozen, ram, swp, len(procs)))

mb = lambda b: f"{b / 1048576:7.0f} MB"
print(f"{'pane':>5} {'agent':7} {'state':8} {'ram':>10} {'swap':>10} procs")
for pane, ag, frozen, ram, swp, n in rows:
    print(f"{pane:>5} {ag:7} {'RESTING' if frozen else 'awake':8} {mb(ram)} {mb(swp)} {n}")
tot = lambda f: sum(r[3] + r[4] for r in rows if f(r))
print(f"panes {len(rows)} · resting {sum(r[2] for r in rows)} · "
      f"agents' RAM {mb(sum(r[3] for r in rows if r[1] != '-')).strip()} · "
      f"agents' swap {mb(sum(r[4] for r in rows if r[1] != '-')).strip()} · "
      f"resting panes' RAM {mb(sum(r[3] for r in rows if r[2])).strip()}")
