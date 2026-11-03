#!/usr/bin/env python3
"""The controller and assertions for permissions-native.sh; no live targets."""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time
import tomllib
from receipt import source, digest

mode, repo, out, binary, session = sys.argv[1:6]
repo, out = Path(repo), Path(out)
runtime = Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")) / "terminal-delight"
state = Path.home() / ".config/terminal-delight/sessions" / f"{session}.toml"


def ctl(line, window=None):
    window = window or int((out / "window").read_text())
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(12)
        stream.connect(str(runtime / f"ctl-{window}.sock"))
        stream.sendall((line + "\n").encode())
        return stream.makefile().readline().strip()


def rpc(name, arguments, pane=None):
    frame = {"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": arguments}}
    prefix = f"mcp from {session} {pane if pane is not None else '-'} rpc "
    return json.loads(ctl(prefix + json.dumps(frame)))


def failed(frame):
    return "error" in frame or frame.get("result", {}).get("isError") is True


def save(name, value):
    (out / name).write_text(json.dumps(value, indent=2) + "\n")


if mode == "prepare":
    assert not state.exists(), "scratch session already exists"
    state.parent.mkdir(parents=True, exist_ok=True)
    state.write_text(
        'active = 0\npanes = 1\n[mcp]\nenabled = true\nwritable = true\nevents = false\nexpose = "all"\n'
        '[[tabs]]\nname = "PRIVATE CANARY"\n[tabs.node.Leaf]\noff_limits = true\n'
        + 'cwd = ' + json.dumps(str(out)) + '\nresume = '
        + json.dumps(f'exec python3 "{repo}/scripts/security/permissions-native.py" caller "{repo}" "{out}" "{binary}" "{session}" > "{out}/caller.log" 2>&1') + '\n'
    )
    # A known commentable page, copied so no real document's notes can change.
    (out / "own.html").write_bytes((repo / "reports/2026-10-21-restricted-commands.html").read_bytes())
    (out / "launch.sh").write_text(
        '#!/bin/sh\nunset TD_SEED_CWD TD_SEED_RESUME TD_SCRATCH TD_NO_SESSIOND TD_DEMO_STATE\n'
        f'export TD_SESSION={session} TD_MCP_SCRIPTED=1 TD_WORKBENCH_DEMO=1\nexec "{binary}" > "{out}/window.log" 2>&1\n'
    )
elif mode == "caller":
    until = time.monotonic() + 20
    while not (out / "window").exists():
        assert time.monotonic() < until, "window never identified itself"
        time.sleep(.05)
    pane = int(os.environ["TD_PANE_ID"])
    note = rpc("leave_note", {"text": "Own hidden pane still works"}, pane)
    assert not failed(note), note
    opened = rpc("open_document", {"path": str(out / "own.html"), "placement": "here"}, pane)
    assert not failed(opened), opened
    while True:
        notes = rpc("document_notes", {}, pane)
        if not failed(notes):
            break
        assert time.monotonic() < until, notes
        time.sleep(.1)
    assert notes["result"]["structuredContent"]["path"] == str(out / "own.html"), notes
    save("caller-receipt.json", {"pane": pane, "shell": os.getpid(), "note": note, "open": opened, "notes": notes})
    # Keep this exact pane generation alive for the foreign caller assertions.
    while True:
        time.sleep(1)
elif mode == "check":
    until = time.monotonic() + 20
    while "demo surfaces on pane" not in (out / "window.log").read_text(errors="replace"):
        assert time.monotonic() < until, "the positive Workbench fixture never appeared"
        time.sleep(.05)
    caller = json.loads((out / "caller-receipt.json").read_text())
    pane, pid = caller["pane"], caller["shell"]
    assert rpc("list_panes", {})["result"]["structuredContent"]["panes"] == []
    attempts = {}
    for name, args in [
        ("leave_note", {"pid": pid, "text": "Foreign canary"}),
        ("declare_deliverable", {"pid": pid, "href": str(out / "own.html")}),
        ("present_surface", {"pid": pid, "surface": {"td": "0.4", "kind": "response", "title": "Foreign", "model": {"layman": "Must be refused"}}}),
        ("request_access", {"change": "events_on", "why": "Forged caller must not ask"}),
        ("document_notes", {}),
    ]:
        result = rpc(name, args, pane)
        assert failed(result), ("forged caller reached hidden pane", name, result)
        attempts[name] = result
    # These route to a real Workbench and real open document, created by the
    # genuine caller above. Removing a picker check must expose this canary.
    routes = {}
    for line in ["bench probe", "doc notes", "doc close", "doc scroll 10", "doc note add x forged", "doc concur x", "doc save", f"doc here {out / 'own.html'}", f"doc beside {out / 'own.html'}"]:
        result = ctl(line)
        assert result.startswith("err") and "off-limits" in result, (line, result)
        routes[line] = result
    assert ctl('tabs ' + json.dumps([{"op": "name", "pane": pid, "name": "FORGED"}])).startswith("ok")
    assert tomllib.loads(state.read_text())["tabs"][0]["name"] == "PRIVATE CANARY"
    assert ctl('tabs ' + json.dumps([{"op": "name", "tab": 0, "name": "FORGED BY INDEX"}])).startswith("ok")
    assert tomllib.loads(state.read_text())["tabs"][0]["name"] == "PRIVATE CANARY"
    save("permission-receipt.json", {"source": source(repo), "binary_sha256": digest(binary), "own_hidden_note_open_read": True,
         "forged_callers": attempts, "routes": routes, "hidden_tab_unchanged": True})
elif mode == "ordinary":
    window = int(sys.argv[6])
    replies = {}
    for line in ["mcp on", "mcp off", "mcp writes on", "mcp writes off", "mcp expose all", "mcp expose agents"]:
        reply = ctl(line, window)
        assert reply.startswith("err") and "TD_MCP_SCRIPTED=1" in reply, (line, reply)
        replies[line] = reply
    save("ordinary-receipt.json", {"source": source(repo), "binary_sha256": digest(binary), "six_startup_refusals": replies})
else:
    raise SystemExit("unknown native phase")
