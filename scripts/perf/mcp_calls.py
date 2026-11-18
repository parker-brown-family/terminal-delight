"""Time the advertised tools over a private ctl socket, from a real fixture pane."""
import json
import os
from pathlib import Path
import socket
import time


def measure(directory, repetitions):
    root = Path(directory)
    pid = int((root / "window-pid").read_text())
    path = root / "run/terminal-delight" / f"ctl-{pid}.sock"
    my_id = int(os.environ["TD_PANE_ID"])
    serial = 0

    def call(method, params):
        nonlocal serial
        serial += 1
        request = json.dumps(dict(jsonrpc="2.0", id=serial, method=method, params=params))
        packet = f"mcp from bench {my_id} rpc {request}\n".encode()
        start = time.perf_counter_ns()
        with socket.socket(socket.AF_UNIX) as connection:
            connection.settimeout(12)
            connection.connect(str(path))
            connection.sendall(packet)
            with connection.makefile("rb") as reader:
                raw = reader.readline()
        elapsed = time.perf_counter_ns() - start
        reply = json.loads(raw)
        if reply["id"] != serial:
            raise RuntimeError("wrong RPC id")
        return reply["result"], dict(id=serial, roundtrip_ns=elapsed, wire_bytes=len(raw))

    catalogue, _ = call("tools/list", {})
    verbs = {t["name"] for t in catalogue["tools"]}
    listing, _ = call("tools/call", {"name": "list_panes", "arguments": {}})
    visible = listing["structuredContent"]["panes"]
    if len(visible) != 20:
        raise RuntimeError(f"expected 20 exposed fixture panes, found {len(visible)}")
    own = next(p["pid"] for p in visible if p["pane_id"] == my_id)
    # These are private shells, with writes and high-trust tools disabled.
    # Stateful actions exercise their real refusal path and never start an agent.
    args = {
        "grep": {"query": "history", "limit": 2},
        "pane_events": {"pid": own},
        "get_pane_config": {"targets": ["outer", own]},
        "ask_agent": {"pid": own, "question": "Fixture?"},
        "ask_answer": {"ask_id": 1},
        "prompt_agent": {"pid": own, "prompt": "Fixture"},
        "prompt_status": {"prompt_id": 1},
        "leave_note": {"pid": own, "text": "Fixture"},
        "declare_deliverable": {},
        "open_document": {"path": str(root / "missing.md")},
        "request_access": {},
        "start_team": {},
    }
    cases = [(name, name, args.get(name, {})) for name in sorted(verbs)]
    if "whoami" in verbs:
        cases += [("list_compact", "list_panes", {"compact": True}),
                  ("list_mine", "list_panes", {"mine": True, "compact": True})]
    rows = []
    for repeat in range(repetitions):
        for case, verb, arguments in cases:
            result, row = call("tools/call", {"name": verb, "arguments": arguments})
            data = result.get("structuredContent", {})
            row.update(case=case, verb=verb, repeat=repeat, error=bool(result.get("isError")),
                       code=data.get("code") if isinstance(data, dict) else None)
            rows.append(row)
    (root / "calls.json").write_text(json.dumps(rows))
