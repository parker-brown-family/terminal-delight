#!/usr/bin/env python3
"""Join a Terminal Delight pane pid to the Claude peer NAME you can SendMessage to.

Three exact hops, no inference:

  1. pane pid                      — from list_panes
  2. /run/user/1000/cc-socks/<agent_pid>.sock, where PPid(agent_pid) == pane pid
  3. ~/.claude/sessions/<agent_pid>.json      — carries the peer name

Verified against a live cross-session message: pane 90 (pid 3604902) messaged this
session from uds:/run/user/1000/cc-socks/3609265.sock naming itself terminal-delight-81,
and PPid(3609265) == 3604902.

Run it with no arguments for the whole window; pass pids to limit it.
"""
import glob
import json
import os
import sys

SOCKS = "/run/user/1000/cc-socks"
SESSIONS = os.path.expanduser("~/.claude/sessions")
NAME_KEYS = ("name", "peerName", "agentName", "peer_name", "sessionName")


def ppid(pid):
    try:
        with open(f"/proc/{pid}/status") as f:
            for line in f:
                if line.startswith("PPid:"):
                    return int(line.split()[1])
    except OSError:
        pass
    return None


def agent_sockets_by_pane():
    """pane pid -> [agent pids]. A pane with none is not exposed to the peer bus."""
    out = {}
    for s in glob.glob(f"{SOCKS}/*.sock"):
        try:
            apid = int(os.path.basename(s)[:-5])
        except ValueError:
            continue
        parent = ppid(apid)
        if parent:
            out.setdefault(parent, []).append(apid)
    return out


def peer_name(agent_pid):
    """The name SendMessage takes, or None if this session never registered one."""
    path = f"{SESSIONS}/{agent_pid}.json"
    try:
        with open(path) as f:
            d = json.load(f)
    except (OSError, ValueError):
        return None
    for k in NAME_KEYS:
        v = d.get(k)
        if isinstance(v, str) and v:
            return v
    # the shape may differ; fall back to any short string field whose key mentions name
    for k, v in d.items():
        if "name" in k.lower() and isinstance(v, str) and 0 < len(v) < 80:
            return v
    return None


def resolve(pane_pid):
    """(peer_name, agent_pid, why) — 'why' says how confident the answer is."""
    socks = agent_sockets_by_pane().get(pane_pid, [])
    if not socks:
        return None, None, "no agent socket under this pane — not on the peer bus"
    if len(socks) > 1:
        return None, None, f"{len(socks)} agent sockets under one pane — ambiguous, do not guess"
    n = peer_name(socks[0])
    if not n:
        return None, socks[0], "socket found but no session file names it"
    return n, socks[0], "measured"


def main(argv):
    pids = [int(a) for a in argv[1:]] if len(argv) > 1 else None
    by = agent_sockets_by_pane()
    if pids is None:
        pids = sorted(by)
    print(f"{'PANE PID':>9}  {'AGENT PID':>9}  {'PEER NAME':<28} HOW")
    named = 0
    for p in pids:
        n, a, why = resolve(p)
        if n:
            named += 1
        print(f"{p:>9}  {(a or '—'):>9}  {(n or '—'):<28} {why}")
    print(f"\n{named} of {len(pids)} panes resolved to a peer name")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
