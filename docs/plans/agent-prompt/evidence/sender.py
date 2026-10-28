#!/usr/bin/env python3
"""The coordinator in the prompt rig: an MCP client inside a seated pane.

It talks to its own window through `terminal-delight mcp`, the way an agent's
MCP server does, so the window names it by its process. It prompts a real
Claude worker and a real Codex worker along the prompt lines the rig's layout
drew, retries with a key, tries a second prompt while the first is open, sends
long multi-line prompts, and writes every request and answer to the log.
"""
import json, subprocess, sys, time, random

TD, LOG = sys.argv[1], sys.argv[2]
log = open(LOG, "a", buffering=1)
def say(*a):
    line = " ".join(str(x) for x in a)
    print(line, flush=True)
    log.write(line + "\n")

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
        line = p.stdout.readline()
        if not line:
            raise SystemExit("mcp server closed")
        try:
            v = json.loads(line)
        except Exception:
            continue
        if v.get("id") == seq:
            return v
def tool(name, args):
    v = rpc("tools/call", {"name": name, "arguments": args})
    r = v.get("result", {})
    text = " ".join(c.get("text", "") for c in r.get("content", []))
    log.write(json.dumps({"call": name, "args": args, "isError": r.get("isError"), "data": r.get("structuredContent")}) + "\n")
    return r.get("isError"), text, r.get("structuredContent") or {}

rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "prompt-rig", "version": "1"}})
p.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n"); p.stdin.flush()

workers = {}
for _ in range(240):
    err, text, data = tool("list_panes", {})
    for pane in data.get("panes", []):
        if pane.get("mode") in ("CLAUDE", "CODEX"):
            workers[pane["mode"]] = pane["pid"]
    if len(workers) == 2:
        break
    time.sleep(0.5)
say("workers", workers, "reach", [(x.get("mode"), x.get("reach")) for x in data.get("panes", [])])
if len(workers) < 2:
    say("SENDER DONE: workers never appeared"); sys.exit(1)

tag = random.randint(1000, 9999)
WAIT = 1
def prompt(mode, text, key=None):
    args = {"pid": workers[mode], "prompt": text}
    if key:
        args["key"] = key
    err, said, data = tool("prompt_agent", args)
    say(f"prompt_agent {mode} key={key} err={err}:", said[:300].replace("\n", " | "))
    return data.get("prompt_id")

def follow(pid_id, mode, limit=420):
    end = time.time() + limit
    last = None
    while time.time() < end:
        err, said, data = tool("prompt_status", {"prompt_id": pid_id, "wait_seconds": WAIT})
        stage = data.get("stage")
        if (stage, data.get("worker"), data.get("pid")) != last:
            say(f"  {mode} prompt {pid_id}: {stage} {data.get('worker') or ''} (pid {data.get('pid')}, {data.get('age_ms')} ms)".rstrip())
            last = (stage, data.get("worker"), data.get("pid"))
        if stage in ("finished", "failed"):
            say(f"  {mode} END:", said[:600].replace("\n", " | "))
            return data
    say(f"  {mode} prompt {pid_id}: still open after {limit}s")
    return {}

# 1. A short prompt to each, with a key; a retry by key; a second while open.
c1 = prompt("CLAUDE", f"Reply with exactly one line: PROMPT-OK-CLAUDE-{tag}. Use no tools.", key="c1")
again = prompt("CLAUDE", "this must not be typed", key="c1")
say("retry by key names the same prompt:", again == c1, again, c1)
busy = prompt("CLAUDE", "a second prompt while the first is open")
say("second prompt while open was refused:", busy is None)
x1 = prompt("CODEX", f"Reply with exactly one line: PROMPT-OK-CODEX-{tag}. Run no commands.", key="x1")
rc = follow(c1, "CLAUDE")
rx = follow(x1, "CODEX")
err, said, data = tool("grep", {"query": f"PROMPT-OK-CLAUDE-{tag}"})
say("grep claude screen for its answer:", [(m.get("pid"), [t.get("text") for t in m.get("matches", [])][:3]) for m in data.get("panes", [])])
err, said, data = tool("grep", {"query": "Threshing Floor prompt"})
say("grep for the header on the workers' screens:", [(m.get("mode"), [t.get("text")[:120] for t in m.get("matches", [])][:2]) for m in data.get("panes", [])])

# 2. A queue: a slow prompt from here, and the second coordinator prompts the
# same worker while it works. Its prompt must wait, then go in after.
c3 = prompt("CLAUDE", f"Write a 200-word story about a lighthouse keeper. Use no tools. End with a final line: QUEUE-FIRST-{tag}", key="c3")
say("PHASE2", c3, tag)
follow(c3, "CLAUDE")
for _ in range(240):
    if "SENDER2 DONE" in open(LOG).read():
        break
    time.sleep(0.5)

# 3. Rest: wait past the rig's one-minute rest, then prompt the resting worker.
say("PHASE3 waiting for the claude worker to rest")
time.sleep(95)
c4 = prompt("CLAUDE", f"Reply with exactly one line: RESTED-OK-{tag}. Use no tools.", key="c4")
follow(c4, "CLAUDE")

# 4. Sleep: past rest and the rig's one-minute sleep, prompt the asleep worker.
say("PHASE4 waiting for the claude worker to sleep")
time.sleep(170)
c5 = prompt("CLAUDE", f"Reply with exactly one line: ASLEEP-OK-{tag}. Use no tools.", key="c5")
follow(c5, "CLAUDE", limit=300)
err, said, data = tool("list_panes", {})
say("panes after", [(x.get("mode"), x.get("pid"), x.get("reach")) for x in data.get("panes", [])])
say("SENDER DONE", tag)
time.sleep(900)
