#!/usr/bin/env python3
"""Bounded real-release reveal probe. Run ONLY in the pinned test container.

Requires a private Xvfb, D-Bus, throwaway home and read-only release archives.
No QA mode, desktop bus, user profile, GPU or installed application is used.
Healthy results do not establish multi-day, Wayland or NVIDIA reliability.
"""
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import time


def run(version):
    assert os.environ.get("STRATA_PROBE_CONTAINER") == "1"
    assert Path("/.dockerenv").exists()
    assert os.environ["DBUS_SESSION_BUS_ADDRESS"].startswith("unix:")
    assert not os.environ.get("WAYLAND_DISPLAY")
    root = Path("/tmp") / ("strata-probe-" + version)
    root.mkdir()
    output = Path("/evidence") / (version + os.environ.get("POC_LABEL", ""))
    output.mkdir(exist_ok=False)
    env = dict(os.environ)
    for name in ("HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_RUNTIME_DIR"):
        directory = root / name.lower()
        directory.mkdir(mode=0o700)
        env[name] = str(directory)
    env.update(DISPLAY=":91", GDK_BACKEND="x11", GSK_RENDERER="cairo",
               GTK_A11Y="none", NO_AT_BRIDGE="1")
    config = Path(env["XDG_CONFIG_HOME"]) / "strata"
    config.mkdir()
    config.joinpath("settings.toml").write_text(
        'single_click_previews = true\npreview_autoplay = false\n'
        'browser_mode = "columns"\ncheck_for_updates = false\n')
    target_dir = root / "media with spaces # percent %"
    target_dir.mkdir()
    text = target_dir / "example.txt"
    text.write_text("Local reveal fixture\n")
    video = target_dir / "example.mp4"
    subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i",
                    "color=c=blue:s=64x64:d=1", "-c:v", "libx264",
                    "-pix_fmt", "yuv420p", str(video)], check=True)
    binary = Path("/releases") / (f"strata-{version}-x86_64-unknown-linux-gnu") / "strata"
    rows = []
    with (output / "display.log").open("w") as xlog, (output / "application.log").open("w") as log:
        display = subprocess.Popen(["Xvfb", ":91", "-screen", "0", "1280x900x24",
                                    "-nolisten", "tcp", "-noreset"], stdout=xlog, stderr=xlog)
        app = None
        try:
            for _ in range(100):
                if Path("/tmp/.X11-unix/X91").exists():
                    break
                if display.poll() is not None:
                    raise RuntimeError("private display failed")
                time.sleep(.05)
            else:
                raise RuntimeError("private display unavailable")
            app = subprocess.Popen([str(binary), str(target_dir)], env=env, stdout=log, stderr=log)
            for _ in range(100):
                query = subprocess.run(["dbus-send", "--session", "--print-reply",
                    "--dest=org.freedesktop.DBus", "/org/freedesktop/DBus",
                    "org.freedesktop.DBus.NameHasOwner", "string:org.freedesktop.FileManager1"],
                    capture_output=True, text=True)
                if "boolean true" in query.stdout:
                    break
                if app.poll() is not None:
                    raise RuntimeError("Strata exited before owning FileManager1")
                time.sleep(.1)
            else:
                raise RuntimeError("Strata did not own FileManager1")
            for n in range(60):
                target = video if n % 2 == 0 else text
                started = time.monotonic()
                response = subprocess.run(["dbus-send", "--session", "--print-reply",
                    "--reply-timeout=3000", "--dest=org.freedesktop.FileManager1",
                    "/org/freedesktop/FileManager1", "org.freedesktop.FileManager1.ShowItems",
                    "array:string:" + target.as_uri(), "string:"],
                    capture_output=True, text=True, timeout=5)
                row = {"index": n, "file_kind": target.suffix,
                       "seconds": time.monotonic() - started, "status": response.returncode,
                       "stdout": response.stdout, "stderr": response.stderr}
                rows.append(row)
                with (output / "requests.jsonl").open("a") as stream:
                    stream.write(json.dumps(row) + "\n")
                if response.returncode:
                    break
                time.sleep(.25)
            recovery = None
            if rows[-1]["status"]:
                time.sleep(5)
                started = time.monotonic()
                response = subprocess.run(["dbus-send", "--session", "--print-reply",
                    "--reply-timeout=10000", "--dest=org.freedesktop.FileManager1",
                    "/org/freedesktop/FileManager1", "org.freedesktop.FileManager1.ShowItems",
                    "array:string:" + text.as_uri(), "string:"],
                    capture_output=True, text=True, timeout=12)
                recovery = {"idle_seconds": 5, "deadline_seconds": 10,
                            "seconds": time.monotonic() - started, "status": response.returncode,
                            "stdout": response.stdout, "stderr": response.stderr}
            status_text = Path(f"/proc/{app.pid}/status").read_text() if app.poll() is None else None
            package_versions = subprocess.check_output(
                ["pkg-config", "--modversion", "gtk4", "glib-2.0"], text=True).splitlines()
            log.flush()
            receipt = {"version": version, "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                       "harness_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                       "fixture_sha256": hashlib.sha256(video.read_bytes()).hexdigest(),
                       "gtk4": package_versions[0], "glib2": package_versions[1], "requests": len(rows),
                       "failures": sum(row["status"] != 0 for row in rows),
                       "median_seconds": statistics.median(row["seconds"] for row in rows),
                       "max_seconds": max(row["seconds"] for row in rows),
                       "app_running_at_end": app.poll() is None, "process_status": status_text,
                       "presented_window_log_entries": (output / "application.log").read_text().count("application window presented"),
                       "recovery_probe": recovery,
                       "limits": "60 requests, synthetic 1-second MP4/text, new process, Xvfb/X11/Cairo, no GPU; not the installed Arch build or a multi-day soak"}
            (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
            print(json.dumps({k: v for k, v in receipt.items() if k != "process_status"}), flush=True)
        finally:
            if app is not None and app.poll() is None:
                app.terminate()
                try:
                    app.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    app.kill()
                    app.wait()
            display.terminate()
            display.wait(timeout=5)


if __name__ == "__main__":
    run(sys.argv[1])
