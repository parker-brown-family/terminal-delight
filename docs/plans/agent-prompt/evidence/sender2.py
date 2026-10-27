#!/usr/bin/env python3
"""The second coordinator: waits for PHASE2 in the first one's log, then prompts
the Claude worker while it works on the first coordinator's slow prompt."""
import json, subprocess, sys, time, re
TD, LOG = sys.argv[1], sys.argv[2]
for _ in range(1200):
    m = re.search(r"PHASE2 (\d+) (\d+)", open(LOG).read()) if __import__("os").path.exists(LOG) else None
    if m:
        break
    time.sleep(0.5)
tag = m.group(2)
p = subprocess.Popen([TD, "mcp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
seq = 0
def rpc(method, params=None):
    global seq
    seq += 1
    msg = {"jsonrpc": "2.0", "id": seq, "method": method}
    if params is not None:
        msg["params"] = params
    p.stdin.write(json.dumps(msg) + "\n"); p.stdin.flush()
    while True:
        v = json.loads(p.stdout.readline())
        if v.get("id") == seq:
            return v
def tool(name, args):
    r = rpc("tools/call", {"name": name, "arguments": args}).get("result", {})
    return r.get("isError"), " ".join(c.get("text", "") for c in r.get("content", [])), r.get("structuredContent") or {}
def say(*a):
    with open(LOG, "a") as f:
        f.write(" ".join(str(x) for x in a) + "\n")
rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "rig2", "version": "1"}})
err, text, data = tool("list_panes", {})
claude = [x["pid"] for x in data.get("panes", []) if x.get("mode") == "CLAUDE"][0]
err, said, data = tool("prompt_agent", {"pid": claude, "prompt": f"Reply with exactly one line: QUEUE-SECOND-{tag}. Use no tools.", "key": "s2"})
pid_id = data.get("prompt_id")
say("SENDER2 prompt_agent:", err, said[:160].replace("\n", " | "))
last = None
for _ in range(600):
    err, said, data = tool("prompt_status", {"prompt_id": pid_id, "wait_seconds": 1})
    st = (data.get("stage"), data.get("worker"))
    if st != last:
        say(f"  SENDER2 prompt {pid_id}: {st[0]} {st[1] or ''} ({data.get('age_ms')} ms)")
        last = st
    if st[0] in ("finished", "failed"):
        say("  SENDER2 END:", said[-200:].replace("\n", " | "))
        break
say("SENDER2 DONE")
time.sleep(900)
