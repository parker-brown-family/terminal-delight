"""Paired process CPU with start-time, geometry and output-rate checks."""
import json
import os
import sys
import subprocess
import socket
import time
import tomllib
from datetime import datetime, timezone
from pathlib import Path
a_win, a_host, b_win, b_host, condition, pair, out, counters, session_a, session_b = sys.argv[1:]
hz = os.sysconf("SC_CLK_TCK")
def process(pid):
    path = Path("/proc") / pid
    stat = (path / "stat").read_text().rsplit(")", 1)[1].split()
    return {"ticks": int(stat[11]) + int(stat[12]), "start": int(stat[19]),
            "exe": os.readlink(path / "exe"), "stat": stat}
def geometry():
    clients = json.loads(subprocess.check_output(["hyprctl", "clients", "-j"]))
    selected = {str(c["pid"]): {k: c[k] for k in ("size", "monitor", "floating", "workspace")}
                for c in clients if str(c["pid"]) in (a_win, b_win)}
    assert len(selected) == 2
    a, b = selected[a_win], selected[b_win]
    assert a["size"] == b["size"] == [1000, 700]
    assert a["monitor"] == b["monitor"] and a["floating"] and b["floating"]
    assert a["workspace"] == b["workspace"]
    return selected
def take():
    return {"geometry": geometry(), "processes": [process(p) for p in (a_win, a_host, b_win, b_host)],
            "counts": {label: {p.name: int(p.read_text()) for p in (Path(counters) / label).iterdir() if p.name.isdigit()}
                       for label in ("baseline", "candidate")},
            "pressure": Path("/proc/pressure/cpu").read_text(),
            "machine": Path("/proc/stat").read_text().splitlines()[0]}
start = time.monotonic()
before = take()
time.sleep(10)
after = take()
seconds = time.monotonic() - start
assert before["geometry"] == after["geometry"], "geometry changed during the pair"
rates = {}
for label in ("baseline", "candidate"):
    x, y = before["counts"][label], after["counts"][label]
    assert len(x) == 30 and x.keys() == y.keys(), ("fixture identity", label, x, y)
    delta = {k: y[k] - x[k] for k in x}
    assert all(v > 100 if condition == "working" else v == 0 for v in delta.values()), (condition, delta)
    rates[label] = sum(delta.values()) / seconds
assert condition != "working" or abs(rates["baseline"] / rates["candidate"] - 1) < 0.05, rates
def parsed_output(window, counts):
    request = {"jsonrpc": "2.0", "id": 1, "method": "tools/call",
               "params": {"name": "grep", "arguments": {"query": "still thinking", "scrollback": 15}}}
    with socket.socket(socket.AF_UNIX) as client:
        client.settimeout(10)
        client.connect("/run/user/1000/terminal-delight/ctl-" + window + ".sock")
        client.sendall(("mcp rpc " + json.dumps(request) + "\n").encode())
        data = bytearray()
        while not data.endswith(b"\n"):
            block = client.recv(65536)
            assert block, "window closed while verifying parsed output"
            data.extend(block)
    reply = json.loads(bytes(data)[bytes(data).index(b"{"):])
    assert not reply.get("error") and not reply["result"].get("isError"), reply
    rows = reply["result"]["structuredContent"]["panes"]
    assert len(rows) == 30, ("not every hidden pane parsed working output", len(rows))
    seen = {str(row["tab"]): max(int(m["text"].strip().rsplit(" ", 1)[-1])
                               for m in row["matches"]) for row in rows}
    assert seen.keys() == counts.keys()
    lags = {k: counts[k] - seen[k] for k in counts}
    assert all(-30 <= lag <= 30 for lag in lags.values()), ("hidden parser fell behind", lags)
    return {"parsed_counter": seen, "counter_file_minus_parsed": lags}
# This read happens after the timed process snapshots; writer counts alone
# cannot prove that a window is still draining and parsing hidden output.
parsed = ({label: parsed_output(window, after["counts"][label])
           for label, window in (("baseline", a_win), ("candidate", b_win))}
          if condition == "working" else None)
cpu = {}
for name, a, b in zip(("baseline_window", "baseline_host", "candidate_window", "candidate_host"),
                      before["processes"], after["processes"]):
    assert a["start"] == b["start"]
    cpu[name] = 100 * (b["ticks"] - a["ticks"]) / (hz * seconds)
layouts = []
for session in (session_a, session_b):
    layout = tomllib.loads(Path("/home/parker/.config/terminal-delight/sessions", session + ".toml").read_text())
    assert layout["active"] == 0 and len(layout["tabs"]) == 31, (session, layout["active"])
    assert layout["theme"]["grade"]["crt"] is False, "CRT must be off in both windows"
    layouts.append({"active": layout["active"], "tabs": len(layout["tabs"]), "win": layout.get("win")})
record = {"at": datetime.now(timezone.utc).isoformat(), "condition": condition,
          "pair": int(pair), "seconds": seconds, "clock_ticks_per_second": hz,
          "logical_cpus": os.cpu_count(), "cpu": cpu, "output_per_second": rates,
          "layouts": layouts, "parsed_output_proof": parsed, "before": before, "after": after}
with Path(out).open("a") as f:
    f.write(json.dumps(record) + "\n")
print(json.dumps({k: v for k, v in record.items() if k not in ("before", "after")}), flush=True)
