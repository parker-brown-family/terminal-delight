#!/usr/bin/env python3
"""Capture a real attach snapshot from a throwaway session host.

The encoder runs HOST-side, so the only way to test it against real terminal
content is to ask a real host for a real snapshot. Session 1 is not available
for that — stealing a pane's byte stream takes it away from the window that is
showing it — so this stands up a host of its own, feeds a pane the patterns the
generated sweep found, and takes the snapshot as the only client.

Writes three files, which the Rust side reads back:
    <out>.bin    the exact bytes the host sent
    <out>.json   the pane's geometry and the host's own grid hash

Usage: probe-capture.py <binary> <session-key> <out-prefix>
"""
import json
import os
import socket
import subprocess
import sys
import time

binary, key, out = sys.argv[1], sys.argv[2], sys.argv[3]
sock_path = f"/run/user/{os.getuid()}/terminal-delight/session-{key}.sock"

# --- stand up a host of our own ------------------------------------------
if os.path.exists(sock_path):
    os.unlink(sock_path)
host = subprocess.Popen([binary, "serve", "--session", key],
                        stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT)
for _ in range(60):
    time.sleep(0.25)
    if os.path.exists(sock_path):
        break
else:
    sys.exit("probe-capture: host never opened its socket")


def control():
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(10)
    s.connect(sock_path)
    f = s.makefile("rwb")

    def call(o):
        f.write((json.dumps(o) + "\n").encode())
        f.flush()
        return json.loads(f.readline().decode())

    call({"verb": "hello", "proto": 1, "kind": "tool"})
    return s, call


sock, call = control()

COLS, ROWS = 80, 24
# The host types the `resume` line into the pane itself. That is the reliable
# injection path — writing to the byte stream as a client did not reach the
# shell, and produced a capture of an empty grid that passed every check by
# testing nothing.
CONTENT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "probe-content.sh")
spawned = call({"verb": "spawn-pane", "cwd": "/tmp",
                "resume": f"bash {CONTENT}",
                "geom": {"cols": COLS, "rows": ROWS,
                         "cell_width": 8, "cell_height": 20}})
info = spawned.get("outcome", {}).get("ok") or spawned.get("info")
if isinstance(info, dict) and "info" in info:
    info = info["info"]
pane = info["pane"] if isinstance(info, dict) else spawned["pane"]
print(f"spawned pane {pane} on {binary.split('/')[-1]}")

# --- open the byte stream BEFORE generating content, as a window would ----
stream = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
stream.settimeout(10)
stream.connect(sock_path)
stream.sendall(f"stream {pane}\n".encode())
time.sleep(0.5)

# --- feed the pane the shapes the sweep found -----------------------------
# Wrapped prose ending in a space, colour painted to the margin, wide
# characters at the margin, and erases that tear them in half. Written through
# the shell the host spawned, so it is a real pty carrying real output.
# The content is already running: the host typed the recipe when it spawned the
# pane, before this stream was opened. Give it time to finish printing.
time.sleep(4.0)

# --- drain until the pane goes quiet --------------------------------------
buf = bytearray()
stream.settimeout(1.5)
while True:
    try:
        chunk = stream.recv(65536)
    except socket.timeout:
        break
    if not chunk:
        break
    buf += chunk

check = call({"verb": "grid-check", "pane": pane})
outcome = check.get("outcome", {})
got = outcome.get("ok") if isinstance(outcome, dict) else None
if got is None:
    print("grid-check said:", json.dumps(check)[:400])
    sys.exit("probe-capture: the host would not answer grid-check")

panes = {p["pane"]: p for p in call({"verb": "list-panes"})["panes"]}
geom = panes[pane]["geom"]

open(f"{out}.bin", "wb").write(bytes(buf))
json.dump({"hash": got["hash"], "stream_offset": got["stream_offset"],
           "bytes_read": len(buf), "cols": geom["cols"], "rows": geom["rows"],
           "binary": binary},
          open(f"{out}.json", "w"), indent=2)

print(f"captured {len(buf)} bytes; host says offset {got['stream_offset']}, "
      f"hash {got['hash']:#x}; geom {geom['cols']}x{geom['rows']}")
print("aligned" if len(buf) == got["stream_offset"]
      else "NOT ALIGNED — the pane was still printing; hash is not comparable")

call({"verb": "shutdown"})
time.sleep(0.5)
host.terminate()
