#!/usr/bin/env python3
"""Exercise TD's actual reveal generator on private D-Bus, never the desktop.

Dependencies: rustc, dbus-run-session, dbus-send, Python3 + PyGObject (Gio).
Run: python3 reveal-poc.py --repo /path/to/terminal-delight --out /tmp/reveal-proof
This injects protocol failures. It does NOT reproduce Strata's internal freeze.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time

NAME = "org.freedesktop.FileManager1"
OBJECT = "/org/freedesktop/FileManager1"


def event(path, **fields):
    with open(path, "a") as output:
        output.write(json.dumps({"monotonic": time.monotonic(), **fields}) + "\n")


def fixture(mode, directory):
    # Parent creates a new bus; refusing its old address prevents accidental use
    # of a live desktop even if this private entry point is invoked by mistake.
    assert os.environ["DBUS_SESSION_BUS_ADDRESS"] != os.environ.get("POC_OLD_BUS")
    assert not os.environ.get("DISPLAY") and not os.environ.get("WAYLAND_DISPLAY")
    from gi.repository import Gio, GLib
    connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    xml = '''<node><interface name="org.freedesktop.FileManager1">
    <method name="ShowItems"><arg type="as" direction="in"/>
    <arg type="s" direction="in"/></method></interface></node>'''
    interface = Gio.DBusNodeInfo.new_for_xml(xml).interfaces[0]
    pending = []

    def receive(_connection, _sender, _path, _interface, method, args, invocation):
        event(directory / "events.jsonl", kind="request", method=method,
              args=args.unpack())
        pending.append(invocation)
        if mode == "reply":
            invocation.return_value(GLib.Variant("()", ()))
        elif mode == "reject":
            invocation.return_dbus_error(NAME + ".Rejected", "injected refusal")
        elif mode == "late":
            def reply_later():
                event(directory / "events.jsonl", kind="late_reply")
                invocation.return_value(GLib.Variant("()", ()))
                return False
            GLib.timeout_add(5000, reply_later)
        # no-reply retains the invocation but never answers it.

    connection.register_object(OBJECT, interface, receive, None, None)
    connection.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus",
                         "org.freedesktop.DBus", "RequestName",
                         GLib.Variant("(su)", (NAME, 0)), None,
                         Gio.DBusCallFlags.NONE, 1000, None)
    (directory / "ready").touch()
    GLib.MainLoop().run()


def private_case(mode, directory):
    directory = Path(directory)
    fixture_process = subprocess.Popen(
        [sys.executable, __file__, "--fixture", mode, str(directory)])
    try:
        deadline = time.monotonic() + 5
        while not (directory / "ready").exists():
            if fixture_process.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError("private fixture failed to own its name")
            time.sleep(.01)
        command = (directory / "command.txt").read_text()
        started = time.monotonic()
        child = subprocess.Popen(["sh", "-c", command], start_new_session=True)
        timed_out = False
        try:
            status = child.wait(timeout=float(os.environ["POC_DEADLINE"]))
        except subprocess.TimeoutExpired:
            timed_out = True
            import signal
            os.killpg(child.pid, signal.SIGTERM)
            status = child.wait(timeout=2)
        elapsed = time.monotonic() - started
        if mode == "late":
            # Observe the late success after the fallback already ran.
            time.sleep(max(0, 5.3 - elapsed))
        (directory / "case.json").write_text(json.dumps({
            "elapsed_seconds": elapsed, "status": status,
            "watchdog_terminated": timed_out,
            "bus_is_private": os.environ["DBUS_SESSION_BUS_ADDRESS"] !=
                              os.environ.get("POC_OLD_BUS"),
        }, indent=2) + "\n")
    finally:
        fixture_process.terminate()
        fixture_process.wait(timeout=2)


def source_generator(repo, temporary):
    source_path = repo / "app/src/pane.rs"
    source = source_path.read_text()
    functions = []
    # Current functions have column-zero closing braces. Fail on a changed
    # shape instead of silently benchmarking a hand-maintained replacement.
    for name in ("path_to_file_uri", "shell_quote", "reveal_script"):
        found = re.findall(r"^fn " + name + r"\([^\n]+\{\n.*?^\}",
                           source, re.M | re.S)
        if len(found) != 1:
            raise ValueError(f"expected exactly one production function: {name}")
        functions.append(found[0])
    constant = re.findall(r"^const REVEAL_REPLY_TIMEOUT_MS:.*?;$", source, re.M)
    if len(constant) != 1:
        raise ValueError("missing timeout constant")
    program = "\n\n".join(functions + constant) + '''
fn main() {
    let args: Vec<String> = std::env::args().collect();
    println!("{}", reveal_script(&args[1], args[2] == "uwsm"));
}
'''
    path = temporary / "production-generator.rs"
    path.write_text(program)
    binary = temporary / "generator"
    subprocess.run(["rustc", "--edition=2021", str(path), "-o", str(binary)], check=True)
    return binary, program, hashlib.sha256(source_path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    repo, output = args.repo.resolve(), args.out.resolve()
    output.mkdir(parents=True, exist_ok=False)
    for tool in ("rustc", "dbus-run-session", "dbus-send"):
        if not shutil.which(tool):
            raise RuntimeError(f"missing dependency: {tool}")
    results = []
    with tempfile.TemporaryDirectory(prefix="td-reveal-proof-") as scratch:
        temporary = Path(scratch)
        generator, program, source_sha = source_generator(repo, temporary)
        (output / "production-generator.rs").write_text(program)
        bin_dir = temporary / "bin"
        bin_dir.mkdir()
        # The stubs only record launch arguments. No real desktop opener is run.
        stub = '''#!PYTHON
import os,sys,json,time
from pathlib import Path
kind=Path(sys.argv[0]).name
with open(os.environ['POC_EVENTS'],'a') as f:
 f.write(json.dumps({'kind':kind,'args':sys.argv[1:],'monotonic':time.monotonic()})+'\\n')
if kind=='uwsm-app':
 assert sys.argv[1:3]==['--','xdg-open']
 os.execv(sys.executable,[sys.executable,str(Path(sys.argv[0]).with_name('xdg-open')),*sys.argv[3:]])
if os.environ.get('POC_HANG_FALLBACK')=='1': time.sleep(60)
'''.replace("PYTHON", sys.executable)
        for name in ("xdg-open", "uwsm-app"):
            path = bin_dir / name
            path.write_text(stub)
            path.chmod(0o755)
        target_dir = temporary / "folder ' # % , $HOME $(touch SHOULD_NOT_EXIST)"
        target_dir.mkdir()
        target = target_dir / "video ' # % , ü.mp4"
        target.touch()  # Protocol fixture only; no media decoding occurs.
        cases = [(f"healthy-{route}", "reply", route, "none", 5)
                 for route in ("direct", "uwsm")]
        cases += [(f"silent-{route}-{n}", "silent", route, "none", 6)
                  for route in ("direct", "uwsm") for n in range(3)]
        cases += [("refused", "reject", "direct", "none", 5),
                  ("late-reply", "late", "direct", "none", 7),
                  ("fallback-also-hangs", "silent", "uwsm", "hang", 5),
                  ("old-uncapped", "silent", "direct", "uncapped", 30),
                  ("wrong-fallback-target", "reject", "direct", "target", 5)]
        for name, mode, route, mutation, deadline in cases:
            directory = output / name
            directory.mkdir()
            command = subprocess.check_output([str(generator), str(target), route], text=True).strip()
            if mutation == "uncapped":
                command = re.sub(r" --reply-timeout=\d+", "", command)
            elif mutation == "target":
                # Change only the fallback operand, leaving the D-Bus target alone.
                import shlex
                old = "'" + str(target_dir).replace("'", "'\\''") + "'"
                command = command.rsplit(old, 1)
                if len(command) != 2:
                    raise ValueError("fallback mutation did not match")
                command = command[0] + shlex.quote(str(target)) + command[1]
            (directory / "command.txt").write_text(command + "\n")
            env = dict(os.environ)
            env["POC_OLD_BUS"] = env.get("DBUS_SESSION_BUS_ADDRESS", "")
            for key in ("DBUS_SESSION_BUS_ADDRESS", "DBUS_STARTER_ADDRESS", "DBUS_STARTER_BUS_TYPE",
                        "DISPLAY", "WAYLAND_DISPLAY", "AT_SPI_BUS_ADDRESS"):
                env.pop(key, None)
            env.update(PATH=str(bin_dir) + os.pathsep + env["PATH"],
                       POC_EVENTS=str(directory / "events.jsonl"),
                       POC_DEADLINE=str(deadline), POC_HANG_FALLBACK=str(int(mutation == "hang")))
            log = directory / "runner.log"
            with log.open("w") as stream:
                subprocess.run(["dbus-run-session", "--", sys.executable, __file__,
                                "--case", mode, str(directory)], env=env, cwd=temporary,
                               stdout=stream, stderr=subprocess.STDOUT,
                               timeout=deadline + 10, check=True)
            row = json.loads((directory / "case.json").read_text())
            events = [json.loads(line) for line in (directory / "events.jsonl").read_text().splitlines()]
            requests = [e for e in events if e["kind"] == "request"]
            opens = [e for e in events if e["kind"] == "xdg-open"]
            wrappers = [e for e in events if e["kind"] == "uwsm-app"]
            assert len(requests) == 1 and requests[0]["args"] == [[target.as_uri()], ""], name
            assert not (temporary / "SHOULD_NOT_EXIST").exists(), "shell interpolation occurred"
            assert row["bus_is_private"], name
            expected_opens = 0 if mode == "reply" else 1
            assert len(opens) == expected_opens, name
            expected_wrappers = expected_opens if route == "uwsm" else 0
            assert len(wrappers) == expected_wrappers, name
            parent_ok = all(e["args"] == [str(target_dir)] for e in opens)
            if mutation == "target":
                assert not parent_ok, "wrong-target mutation escaped the oracle"
            else:
                assert parent_ok, name
            if mutation == "hang":
                assert row["watchdog_terminated"] and row["elapsed_seconds"] >= 5, name
            else:
                assert row["status"] == 0 and not row["watchdog_terminated"], name
            if mutation == "uncapped":
                assert 24 <= row["elapsed_seconds"] < 29, name
            elif mode in ("silent", "late") and mutation != "hang":
                assert 2.8 <= row["elapsed_seconds"] < 4.5, name
            elif mode in ("reply", "reject"):
                assert row["elapsed_seconds"] < 2, name
            if mode == "late":
                late = next(e for e in events if e["kind"] == "late_reply")
                assert late["monotonic"] > opens[0]["monotonic"], name
            row.update(name=name, mode=mode, route=route, mutation=mutation,
                       parent_argument_correct=parent_ok, events=events)
            results.append(row)
            print(f"{name}: {row['elapsed_seconds']:.3f}s, assertions passed", flush=True)
        receipt = {
            "kind": "injected protocol failure; not a Strata root-cause reproducer",
            "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
            "pane_source_sha256": source_sha,
            "harness_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
            "cases": results,
        }
        (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "--fixture":
        fixture(sys.argv[2], Path(sys.argv[3]))
    elif len(sys.argv) == 4 and sys.argv[1] == "--case":
        private_case(sys.argv[2], sys.argv[3])
    else:
        main()
