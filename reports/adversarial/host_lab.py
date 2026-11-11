#!/usr/bin/env python3
"""Headless TD session-host experiments against an explicitly selected binary.

No live socket discovery, no GUI, no agent launches, no external services.
Each host receives private XDG directories and runs /bin/cat in its PTYs.
"""
from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
import resource
import selectors
import signal
import socket
import statistics
import subprocess
import tempfile
import threading
import time
import tomllib
from pathlib import Path

GEOM = {"cols": 160, "rows": 24, "cell_width": 8, "cell_height": 16}
FENCE = b"\x1b[?7"
TIMEOUT = 15


def quantile(values, p):
    if not values:
        return None
    return sorted(values)[min(len(values) - 1, int((len(values) - 1) * p))]


def process_stats(pid):
    try:
        status = Path(f"/proc/{pid}/status").read_text().splitlines()
        fields = dict(line.split(":", 1) for line in status if ":" in line)
        raw = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        return {"rss_kib": int(fields["VmRSS"].split()[0]),
                "threads": int(fields["Threads"]),
                "cpu_s": (int(raw[11]) + int(raw[12])) / os.sysconf("SC_CLK_TCK")}
    except (OSError, KeyError, ValueError):
        return None


class Host:
    def __init__(self, binary, name):
        self.tmp = tempfile.TemporaryDirectory(prefix="td-adversarial-")
        self.root = Path(self.tmp.name)
        self.name = name
        self.logpath = self.root / "host.log"
        self.log = self.logpath.open("w+")
        self.panes = []
        self.streams = []
        self.sockpath = self.root / "run/terminal-delight" / f"session-{name}.sock"
        env = {k: v for k, v in os.environ.items() if not k.startswith(("TD_", "CLAUDE_", "CODEX_", "ANTHROPIC_", "OPENAI_", "OPENROUTER_"))}
        for suffix in ("RUNTIME", "CONFIG", "STATE", "DATA", "CACHE"):
            d = self.root / ("run" if suffix == "RUNTIME" else suffix.lower())
            d.mkdir(mode=0o700)
            env[f"XDG_{suffix}_HOME" if suffix != "RUNTIME" else "XDG_RUNTIME_DIR"] = str(d)
        env.update(TD_HOST_SHELL="/bin/cat", SHELL="/bin/cat", RUST_BACKTRACE="1")
        self.env = env
        self.proc = subprocess.Popen([str(binary), "serve", "--session", name], env=env,
                                     cwd=self.root, stdin=subprocess.DEVNULL, stdout=self.log,
                                     stderr=subprocess.STDOUT, start_new_session=True)
        deadline = time.monotonic() + TIMEOUT
        while time.monotonic() < deadline:
            if self.proc.poll() is not None:
                raise RuntimeError("host exited during startup: " + self.logpath.read_text()[-2000:])
            if self.sockpath.exists():
                try:
                    hello = self.ask({"verb": "hello", "proto": 1, "kind": "tool"})
                    if hello.get("reply") == "hello":
                        return
                except OSError:
                    pass
            time.sleep(.01)
        raise TimeoutError("isolated host never answered hello")

    def connect(self):
        s = socket.socket(socket.AF_UNIX)
        s.settimeout(TIMEOUT)
        try:
            s.connect(str(self.sockpath))
        except BaseException:
            s.close()
            raise
        return s

    def ask(self, request):
        with self.connect() as s:
            s.sendall(json.dumps(request).encode() + b"\n")
            with s.makefile("rb") as f:
                line = f.readline(2_000_000)
            if not line:
                raise RuntimeError("control socket closed without a reply")
            return json.loads(line)

    def spawn(self):
        r = self.ask({"verb": "spawn-pane", "cwd": str(self.root), "geom": GEOM})
        p = r.get("outcome", {}).get("ok")
        if not p:
            raise RuntimeError("spawn refused: " + json.dumps(r))
        self.panes.append(p)
        return p

    def attach(self, pane):
        s = self.connect()
        self.streams.append(s)
        s.sendall(f"stream {pane}\n".encode())
        snapshot = receive_until(s, lambda b: FENCE in b and b[-1:] in (b"h", b"l"))
        return s, snapshot

    def close(self):
        for s in self.streams:
            s.close()
        try:
            if self.proc.poll() is None:
                self.ask({"verb": "shutdown"})
                self.proc.wait(timeout=5)
        except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired):
            self.proc.kill()
            self.proc.wait(timeout=5)
        # The harness owns these exact children. Never scan/kill by program name.
        for p in self.panes:
            pid = p["shell_pid"]
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        self.log.close()
        self.tmp.cleanup()


def receive_until(s, satisfied):
    data = bytearray()
    deadline = time.monotonic() + TIMEOUT
    while time.monotonic() < deadline:
        s.settimeout(max(.01, deadline - time.monotonic()))
        block = s.recv(65536)
        if not block:
            break
        data.extend(block)
        if satisfied(data):
            return bytes(data)
        if len(data) > 8_000_000:
            raise RuntimeError("probe read cap reached")
    raise TimeoutError(f"stream condition absent; received {bytes(data[-400:])!r}")


def scaling(host, count, rounds):
    empty = process_stats(host.proc.pid)
    started = time.monotonic()
    for _ in range(count):
        host.spawn()
    spawn_wall = time.monotonic() - started
    streams = [host.attach(p["pane"])[0] for p in host.panes]
    latencies = []
    byte_count = 0
    traffic_started = time.monotonic()
    for iteration in range(rounds):
        with selectors.DefaultSelector() as sel:
            pending = {}
            for idx, s in enumerate(streams):
                marker = f"PANE{idx:03d}-ROUND{iteration:03d}-".encode() + b"X" * 64
                begin = time.monotonic()
                s.sendall(marker + b"\n")
                pending[s] = [marker, bytearray(), begin]
                sel.register(s, selectors.EVENT_READ)
                byte_count += len(marker) + 1
            deadline = time.monotonic() + TIMEOUT
            while pending:
                if time.monotonic() > deadline:
                    raise TimeoutError(f"{len(pending)} panes did not return two copies")
                for key, _ in sel.select(.1):
                    s = key.fileobj
                    block = s.recv(65536)
                    if not block:
                        raise RuntimeError("PTY stream closed during traffic")
                    marker, data, begin = pending[s]
                    data.extend(block)
                    if data.count(marker) >= 2:
                        latencies.append((time.monotonic() - begin) * 1000)
                        sel.unregister(s)
                        del pending[s]
    traffic_wall = time.monotonic() - traffic_started
    live = process_stats(host.proc.pid)
    # A detach is not a close. Every child must retain its same process id.
    for s in streams:
        s.close()
    listed = host.ask({"verb": "list-panes"})["panes"]
    same = sorted((p["pane"], p["shell_pid"]) for p in listed) == sorted((p["pane"], p["shell_pid"]) for p in host.panes)
    s, snapshot = host.attach(host.panes[0]["pane"])
    wanted = f"PANE000-ROUND{rounds - 1:03d}-".encode()
    snapshot_ok = wanted in snapshot
    s.sendall(b"AFTER-RECONNECT\n")
    receive_until(s, lambda b: b.count(b"AFTER-RECONNECT") >= 2)
    return same and snapshot_ok, {"panes": count, "rounds": rounds,
        "spawn_wall_s": round(spawn_wall, 4), "traffic_wall_s": round(traffic_wall, 4),
        "input_bytes": byte_count, "echo_samples": len(latencies),
        "echo_ms_p50": statistics.median(latencies), "echo_ms_p95": quantile(latencies, .95),
        "echo_ms_max": max(latencies), "host_empty": empty, "host_active": live,
        "host_cpu_delta_s": live["cpu_s"] - empty["cpu_s"] if empty and live else None,
        "children_retained_same_identity": same, "snapshot_contains_last_round": snapshot_ok,
        "renderer_frame_ms": None, "crt_overhead_ms": None, "gpu_memory_kib": None,
        "scope": "Host+PTY echo, including line discipline and cat; no window or rendering. Host RSS excludes children."}


def fragmented_wire(host):
    outcomes = []
    cases = [b"{bad-json}\n", b'{"verb":"not-a-verb"}\n', b'{"verb":"hello","proto":999,"kind":"tool"}\n']
    for payload in cases:
        with host.connect() as s:
            for part in (payload[:1], payload[1:7], payload[7:]):
                s.sendall(part)
            with s.makefile("rb") as f:
                error = json.loads(f.readline())
                s.sendall(b'{"verb":"list-panes"}\n')
                recovered = json.loads(f.readline())
            outcomes.append({"payload": payload.decode().strip(), "reply": error, "following_reply": recovered})
    # Partial request from a client that disappears must not poison the next connection.
    with host.connect() as s:
        s.sendall(b'{"verb":"spawn-pane","geom":')
    hello = host.ask({"verb": "hello", "proto": 1, "kind": "tool"})
    return all(x["reply"].get("reply") == "error" and x["following_reply"].get("reply") == "panes" for x in outcomes) and hello["panes"] == 0, {
        "cases": outcomes, "hello_after_interrupted_request": hello}


def concurrent_lifecycle(host):
    with concurrent.futures.ThreadPoolExecutor(max_workers=12) as ex:
        panes = list(ex.map(lambda _: host.spawn(), range(32)))
    unique = len({p["pane"] for p in panes}) == len(panes)
    seen = host.ask({"verb": "list-panes"})["panes"]
    for p in panes[:16]:
        os.kill(p["shell_pid"], signal.SIGKILL)
    with concurrent.futures.ThreadPoolExecutor(max_workers=12) as ex:
        replies = list(ex.map(lambda p: host.ask({"verb": "close-pane", "pane": p["pane"]}), panes))
    after = host.ask({"verb": "list-panes"})["panes"]
    return unique and len(seen) == 32 and not after and all("ok" in r.get("outcome", {}) for r in replies), {
        "spawn_count": len(panes), "unique_ids": unique, "census_count": len(seen),
        "killed_owned_children": 16, "close_replies": replies, "census_after_close": after}


def stalled_clients(host):
    before = process_stats(host.proc.pid)
    clients = []
    try:
        for _ in range(32):
            s = host.connect()
            clients.append(s)
            s.sendall(b'{"verb":')
        time.sleep(.2)
        during = process_stats(host.proc.pid)
        hello = host.ask({"verb": "hello", "proto": 1, "kind": "tool"})
    finally:
        for s in clients:
            s.close()
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        after = process_stats(host.proc.pid)
        if before and after and after["threads"] <= before["threads"] + 2:
            break
        time.sleep(.05)
    return bool(before and after and after["threads"] <= before["threads"] + 2 and hello["reply"] == "hello"), {
        "stalled_connections": 32, "before": before, "during": during, "after_disconnect": after,
        "host_still_answers": hello.get("reply") == "hello",
        "scope": "Bounded 32-client test; no claim that connection count or line size is capped."}


def host_duplicate(host):
    log = host.root / "duplicate.log"
    with log.open("w") as f:
        child = subprocess.Popen([str(BINARY), "serve", "--session", host.name], env=host.env,
                                 cwd=host.root, stdout=f, stderr=subprocess.STDOUT)
        try:
            rc = child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()
            rc = None
    hello = host.ask({"verb": "hello", "proto": 1, "kind": "tool"})
    # "Already served" is an idempotent success in the CLI, not a promised
    # nonzero exit. The invariant is that the original process/socket survives.
    return rc is not None and hello.get("host") == host.proc.pid and "already served" in log.read_text(), {
        "duplicate_exit": rc, "duplicate_log": log.read_text()[-2000:],
        "original_host_retained": hello.get("host") == host.proc.pid}


def zero_geometry(host):
    outcomes = []
    for geom in ({**GEOM, "cols": 0}, {**GEOM, "rows": 0}, {**GEOM, "cell_width": 0, "cell_height": 0}):
        try:
            r = host.ask({"verb": "spawn-pane", "cwd": str(host.root), "geom": geom})
            p = r.get("outcome", {}).get("ok")
            if p:
                host.panes.append(p)
                close = host.ask({"verb": "close-pane", "pane": p["pane"]})
            else:
                close = None
            outcomes.append({"geom": geom, "reply": r, "close": close})
        except (OSError, RuntimeError, ValueError) as exc:
            return False, {"cases": outcomes, "failure": str(exc), "host_exit": host.proc.poll()}
    hello = host.ask({"verb": "hello", "proto": 1, "kind": "tool"})
    return hello["reply"] == "hello", {"cases": outcomes, "host_still_answers": True,
        "scope": "Zero dimensions only; no enormous allocation request."}


def interrupted_saves(host):
    pane = host.spawn()
    state = host.root / "config/terminal-delight/sessions" / f"{host.name}.toml"
    def body(n):
        return f'active = 0\ngeneration = {n}\n[[tabs]]\nname = "generation-{n}"\n[tabs.node.Leaf]\npane_id = {pane["pane"]}\ncwd = "/intentionally-stale"\n'
    baseline = host.ask({"verb": "save", "schema": 1, "body": body(0)})
    if "written" not in baseline.get("outcome", {}).get("ok", {}):
        raise RuntimeError("baseline save failed: " + json.dumps(baseline))
    original = state.read_bytes()
    with host.connect() as s:
        partial = json.dumps({"verb": "save", "schema": 1, "body": body(999)}).encode()
        s.sendall(partial[:len(partial)//2])
    # A cold hello is also a barrier showing another request still progresses.
    host.ask({"verb": "hello", "proto": 1, "kind": "tool"})
    partial_changed = state.read_bytes() != original
    errors = []
    stop = threading.Event()
    samples = []
    def observe():
        while not stop.is_set():
            try:
                value = tomllib.loads(state.read_text())
                generation = value["generation"]
                if value["tabs"][0]["name"] != f"generation-{generation}":
                    errors.append("mixed generations")
                samples.append(generation)
            except (OSError, ValueError, KeyError) as exc:
                errors.append(type(exc).__name__ + ": " + str(exc))
    observer = threading.Thread(target=observe)
    observer.start()
    try:
        with concurrent.futures.ThreadPoolExecutor(max_workers=8) as ex:
            replies = list(ex.map(lambda n: host.ask({"verb": "save", "schema": 1, "body": body(n)}), range(1, 41)))
    finally:
        stop.set()
        observer.join(timeout=5)
    value = tomllib.loads(state.read_text())
    writes_ok = all("written" in r.get("outcome", {}).get("ok", {}) for r in replies)
    good = not partial_changed and not errors and bool(samples) and writes_ok and 1 <= value["generation"] <= 40
    return good, {"interrupted_request_changed_disk": partial_changed,
        "concurrent_saves": len(replies), "all_acknowledged_written": writes_ok,
        "observer_samples": len(samples), "observed_generations": sorted(set(samples)),
        "torn_read_errors": errors, "final_generation": value["generation"],
        "host_corrected_stale_cwd": value["tabs"][0]["node"]["Leaf"]["cwd"] == str(host.root),
        "scope": "Concurrent acknowledged saves and an interrupted request; not power-loss durability."}


def resize_and_unicode(host):
    pane = host.spawn()
    s, _ = host.attach(pane["pane"])
    outcomes = []
    for n in range(32):
        geom = {"cols": 40 + (n * 17) % 120, "rows": 4 + (n * 7) % 48,
                "cell_width": 8, "cell_height": 16}
        r = host.ask({"verb": "resize", "pane": pane["pane"], "geom": geom})
        outcomes.append("ok" in r.get("outcome", {}))
        # Split in the middle of a UTF-8 scalar. The marker stays short enough
        # to fit at the narrowest width and proves progress after each resize.
        marker = f"U{n:02d}-".encode() + "å🦀".encode()
        payload = marker + b"\n"
        s.sendall(payload[:5]); s.sendall(payload[5:7]); s.sendall(payload[7:])
        receive_until(s, lambda b: b.count(marker) >= 2)
    s.close()
    _, snapshot = host.attach(pane["pane"])
    last = b"U31-" + "å🦀".encode()
    return all(outcomes) and last in snapshot, {"resize_and_utf8_rounds": len(outcomes),
        "all_resize_outcomes_ok": all(outcomes), "last_wide_unicode_marker_in_snapshot": last in snapshot,
        "scope": "Progress and snapshot text after bounded geometry changes; no client renderer hash comparison."}


def main():
    global BINARY
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--out", type=Path, default=Path(__file__).with_name("host-results.json"))
    parser.add_argument("--counts", default="2,5,20,50")
    parser.add_argument("--rounds", type=int, default=12)
    args = parser.parse_args()
    BINARY = args.binary.resolve(strict=True)
    counts = [int(x) for x in args.counts.split(",")]
    if not counts or any(x < 1 or x > 50 for x in counts) or not 1 <= args.rounds <= 50:
        parser.error("counts must be 1..50; rounds 1..50")
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    cases = [(f"scaling-{n}", lambda h, n=n: scaling(h, n, args.rounds)) for n in counts]
    cases += [("fragmented-wire", fragmented_wire), ("concurrent-lifecycle", concurrent_lifecycle),
              ("stalled-clients", stalled_clients), ("duplicate-host", host_duplicate),
              ("zero-geometry", zero_geometry), ("interrupted-concurrent-saves", interrupted_saves),
              ("resize-unicode", resize_and_unicode)]
    results = []
    for name, experiment in cases:
        host = None
        started = time.monotonic()
        try:
            host = Host(BINARY, "adversarial")
            passed, evidence = experiment(host)
            status = "corroborated" if passed else "falsified"
        except Exception as exc:
            status, evidence = "inconclusive", {"type": type(exc).__name__, "error": str(exc)}
        finally:
            if host:
                evidence["host_log_tail"] = host.logpath.read_text()[-3000:]
                host.close()
        results.append({"id": name, "status": status, "wall_s": time.monotonic() - started, "evidence": evidence})
        print(f"{status:12} {name}", flush=True)
    doc = {"schema": "td-host-adversarial-v1", "binary": str(BINARY),
           "binary_sha256": hashlib.sha256(BINARY.read_bytes()).hexdigest(),
           "source_revision": subprocess.run(["git", "rev-parse", "HEAD"], cwd=Path(__file__).resolve().parents[2], capture_output=True, text=True).stdout.strip(),
           "source_binary_match": None, "paid_model_calls": 0, "gui_opened": False,
           "results": results}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(doc, indent=2) + "\n")
    return int(any(r["status"] != "corroborated" for r in results))


if __name__ == "__main__":
    raise SystemExit(main())
