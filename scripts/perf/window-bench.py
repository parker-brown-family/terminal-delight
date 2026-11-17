#!/usr/bin/env python3
"""Measure a production window on a new Xvfb with lavapipe. One visible tab,
N open PTYs. Raw samples are retained. CPU is percent of one logical core."""

import argparse
import contextlib
import json
import os
from pathlib import Path
import shlex
import signal
import statistics
import subprocess as sp
import sys
import tempfile
import time
import tty

HERE = Path(__file__).resolve()
ROOT = HERE.parents[2]
CLOCK = time.monotonic


def wait_for(predicate, seconds, label):
    deadline = CLOCK() + seconds
    while CLOCK() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(0.005)
    raise TimeoutError(label)


def worker(directory, index, history, size):
    # OPOST is off so our byte count is the count written to the PTY.
    tty.setraw(0)
    for n in range(history):
        os.write(1, f"pane {index} history {n:05d} 0123456789abcdef\r\n".encode())
    if index == 0 and os.getenv("TD_BENCH_CAPTURE"):
        # Populate the photograph with the checkout's actual release work.
        rows = sp.check_output(["git", "-C", str(ROOT), "log", "-4", "--format=%h  %s"])
        os.write(
            1, b"\033[2J\033[H$ git log -4 --oneline\r\n" + rows.replace(b"\n", b"\r\n")
        )
    Path(directory, f"ready-{index}").touch()
    if index == 0:
        with open(Path(directory, "go"), "rb") as trigger:
            trigger.read(1)
        block = b"".join(
            f"\033[38;5;{n}mrow {n:03} 0123456789abcdef\033[0m\r\n".encode()
            for n in range(256)
        )
        start = CLOCK()
        sent = 0
        while sent < size:
            sent += os.write(1, block[: min(len(block), size - sent)])
        # A cursor-position report fences the final byte at the parser.
        os.write(1, b"\033[0m\r\n\033[6n")
        response = b""
        while not response.endswith(b"R"):
            response += os.read(0, 64)
        Path(directory, "done").write_text(json.dumps([start, CLOCK(), sent]))
    signal.pause()


def proc_stats(pid):
    # /proc stat's command field may contain spaces or parentheses.
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return (
        (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK"),
        int(fields[21]) * os.sysconf("SC_PAGE_SIZE") / 1048576,
    )


def tagged(directory):
    marker = f"TD_BENCH_RUN={directory}".encode()
    for item in Path("/proc").glob("[0-9]*/environ"):
        try:
            if marker in item.read_bytes().split(b"\0"):
                yield int(item.parent.name)
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            pass


def clean(directory):
    for sig in (signal.SIGTERM, signal.SIGKILL):
        for pid in tagged(directory):
            with contextlib.suppress(ProcessLookupError):
                os.kill(pid, sig)
        time.sleep(0.15)


def summary(values):
    if not values:
        return None
    return dict(
        n=len(values),
        median=statistics.median(values),
        minimum=min(values),
        maximum=max(values),
    )


def run(a, display_env, probe, n, skin, repeat):
    with tempfile.TemporaryDirectory(prefix="td-window-") as temp:
        d = Path(temp)
        env = dict(
            display_env,
            HOME=temp,
            TD_BENCH_RUN=temp,
            TD_SESSION="bench",
            TD_SKIN=str(a.skin_file or ROOT / "app/skins" / f"{skin}.toml"),
            SHELL="/bin/bash",
            TD_HOST_SHELL="/bin/bash",
            LP_NUM_THREADS=str(a.threads),
        )
        if a.capture:
            env["TD_BENCH_CAPTURE"] = "1"
        if a.audit:
            env["TD_FRAME_AUDIT"] = "1"
        if a.scene == "floor":
            env["TD_THRESHING_FLOOR_DEMO"] = "1"
        elif a.scene:
            env["TD_SKIN_SCENE"] = a.scene
        for key, sub in [
            ("XDG_CONFIG_HOME", ".config"),
            ("XDG_RUNTIME_DIR", "run"),
            ("XDG_STATE_HOME", "state"),
            ("XDG_DATA_HOME", "data"),
            ("XDG_CACHE_HOME", "cache"),
        ]:
            (d / sub).mkdir(mode=0o700)
            env[key] = str(d / sub)
        sessions = d / ".config/terminal-delight/sessions"
        sessions.mkdir(parents=True)
        os.mkfifo(d / "go")
        layout = f"active = 0\npanes = {n}\nwin = [0,0,900,600]\n"
        for i in range(n):
            cmd = shlex.join(
                [
                    "exec",
                    sys.executable,
                    str(HERE),
                    "worker",
                    temp,
                    str(i),
                    str(a.history),
                    str(a.bytes),
                ]
            )
            layout += f'[[tabs]]\n[tabs.node.Leaf]\nname = "pane {i}"\ncwd = {json.dumps(temp)}\nresume = {json.dumps(cmd)}\n'
        (sessions / "bench.toml").write_text(layout)
        events_path = d / "frames"
        log_path = a.out / f"{skin}-{n}-{repeat}.log"
        with events_path.open("w") as events, log_path.open("w") as log:
            listener = sp.Popen([probe], env=env, stdout=events, stderr=log)
            try:
                wait_for(
                    lambda: events_path.read_text().startswith("ready"),
                    4,
                    "X11 observer",
                )
                start = CLOCK()
                win = sp.Popen([a.binary], env=env, cwd=temp, stdout=log, stderr=log)
                try:
                    # Xvfb has no window manager. Supply the ConfigureNotify that
                    # starts gpui drawing here. Keep the final fixture size fixed.
                    xid = sp.check_output(
                        [
                            "timeout",
                            "30",
                            "xdotool",
                            "search",
                            "--sync",
                            "--onlyvisible",
                            "--name",
                            "terminal-delight",
                        ],
                        env=env,
                        text=True,
                    ).splitlines()[0]
                    for width in [899, 900]:
                        sp.run(
                            ["xdotool", "windowsize", "--sync", xid, str(width), "600"],
                            env=env,
                            check=True,
                            timeout=5,
                        )

                    def first_paint():
                        if win.poll() is not None:
                            raise RuntimeError(
                                f"window exited {win.returncode}: {log_path}"
                            )
                        return next(
                            (
                                float(l.split()[2])
                                for l in events_path.read_text().splitlines()
                                if l.startswith("paint ")
                            ),
                            None,
                        )

                    first_ms = 1000 * (
                        wait_for(first_paint, 30, "first coloured frame") - start
                    )
                    wait_for(
                        lambda: all((d / f"ready-{i}").exists() for i in range(n)),
                        80,
                        "restored PTYs",
                    )
                    time.sleep(a.quiet)
                    if a.capture:
                        sp.run(
                            [
                                "import",
                                "-window",
                                xid,
                                str(a.out / f"{skin}-{n}-{repeat}.png"),
                            ],
                            env=env,
                            check=True,
                            timeout=20,
                        )
                    host = [
                        p
                        for p in tagged(temp)
                        if b"serve"
                        in Path(f"/proc/{p}/cmdline").read_bytes().split(b"\0")
                    ]
                    if len(host) != 1:
                        raise RuntimeError(f"expected one private host, found {host}")
                    t = CLOCK()
                    cpu0 = proc_stats(win.pid)[0]
                    time.sleep(a.sample)
                    cpu1, rss = proc_stats(win.pid)
                    idle = 100 * (cpu1 - cpu0) / (CLOCK() - t)
                    host_rss = proc_stats(host[0])[1]
                    if a.profile:
                        record = sp.Popen(
                            [
                                "perf",
                                "record",
                                "-q",
                                "-e",
                                "cpu-clock:u",
                                "-F",
                                "99",
                                "-p",
                                str(win.pid),
                                "-o",
                                str(a.out / f"{skin}-{n}-{repeat}.perf.data"),
                            ]
                        )
                    try:
                        with (d / "go").open("wb", buffering=0) as trigger:
                            trigger.write(b"1")
                        wait_for((d / "done").exists, 240, "stream parser fence")
                        begin, end, sent = json.loads((d / "done").read_text())
                    finally:
                        if a.profile:
                            record.send_signal(signal.SIGINT)
                            record.wait(timeout=15)
                    window_id = int(xid)
                    time.sleep(0.05)
                    lines = events_path.read_text().splitlines()[1:]
                    frames = [
                        int(ust) / 1e6
                        for kind, xid, ts, ust in map(str.split, lines)
                        if kind == "present"
                        and int(xid) == window_id
                        and begin <= float(ts) <= end
                    ]
                    intervals = [1000 * (y - x) for x, y in zip(frames, frames[1:])]
                    damages = [
                        float(ts)
                        for kind, xid, ts, _ in map(str.split, lines)
                        if kind == "damage"
                        and int(xid) == window_id
                        and begin <= float(ts) <= end
                    ]
                    (a.out / f"{skin}-{n}-{repeat}.frames").write_text(
                        "\n".join(lines) + "\n"
                    )
                    if a.audit:
                        draws = [
                            line
                            for line in log_path.read_text().splitlines()
                            if line.startswith("td-frame ")
                        ]
                        if not draws:
                            raise RuntimeError(f"renderer audit missing: {log_path}")
                        if skin == "baremetal" and any(
                            line != "td-frame direct=true post=false shadows=0"
                            for line in draws
                        ):
                            raise RuntimeError(
                                f"optional draw with BAREMETAL: {log_path}"
                            )
                    return dict(
                        repeat=repeat,
                        skin=skin,
                        panes=n,
                        history_lines_per_pane=a.history,
                        cold_process_paint_ms=first_ms,
                        idle_window_cpu_pct=idle,
                        window_rss_mib=rss,
                        host_rss_mib=host_rss,
                        stream_mib_s=sent / 1048576 / (end - begin),
                        present_interval_ms=summary(intervals),
                        damage_events=len(damages),
                        full_update_interval_ms=summary(
                            [1000 * (b - a) for a, b in zip(damages, damages[1:])]
                        ),
                        stream_begin=begin,
                        stream_end=end,
                    )
                finally:
                    clean(temp)
                    win.wait(timeout=5)
            finally:
                listener.terminate()
                listener.wait(timeout=5)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--binary", type=Path, required=True)
    p.add_argument("--icd", type=Path, required=True, help="lavapipe Vulkan ICD JSON")
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--display", type=int, default=96)
    p.add_argument("--panes", type=int, nargs="+", default=[1, 8, 16])
    p.add_argument("--skins", nargs="+", default=["default"])
    p.add_argument("--runs", type=int, default=5)
    p.add_argument("--quiet", type=float, default=60)
    p.add_argument("--sample", type=float, default=8)
    p.add_argument("--history", type=int, default=0)
    p.add_argument("--bytes", type=int, default=50 * 1024 * 1024)
    p.add_argument("--threads", type=int, default=4)
    p.add_argument("--profile", action="store_true")
    p.add_argument(
        "--smoke",
        action="store_true",
        help="permit debug binary for rig qualification only",
    )
    p.add_argument(
        "--audit", action="store_true", help="verify actual optional draw submissions"
    )
    p.add_argument(
        "--capture",
        action="store_true",
        help="save the settled native window before streaming",
    )
    p.add_argument(
        "--scene",
        choices=["settings", "help", "floor"],
        help="show a native built-in capture fixture",
    )
    p.add_argument(
        "--skin-file",
        type=Path,
        help="override the one named skin, for a matched control",
    )
    a = p.parse_args()
    if a.skin_file:
        if len(a.skins) != 1:
            p.error("--skin-file needs exactly one named skin")
        a.skin_file = a.skin_file.resolve(strict=True)
    a.binary = a.binary.resolve(strict=True)
    a.icd = a.icd.resolve(strict=True)
    a.out = a.out.resolve()
    if "lvp" not in json.loads(a.icd.read_text())["ICD"]["library_path"]:
        p.error("expected a lavapipe ICD")
    if (
        min(a.panes) < 1
        or min(a.sample, a.runs, a.bytes, a.threads) <= 0
        or min(a.history, a.quiet) < 0
    ):
        p.error("invalid benchmark dimensions")
    for skin in a.skins:
        if not (ROOT / "app/skins" / f"{skin}.toml").is_file():
            p.error(f"unknown skin {skin}")
    info = json.loads(sp.check_output([a.binary, "--build-info"]))
    if not a.smoke and info["opt_level"] not in ["2", "3"]:
        p.error("expected optimized release binary")
    a.out.mkdir(parents=True, exist_ok=False)
    (a.out / "method.json").write_text(
        json.dumps(dict(build=info, argv=sys.argv), indent=2)
    )
    env = dict(
        PATH="/usr/bin:/bin",
        DISPLAY=f":{a.display}",
        LANG="C.UTF-8",
        VK_DRIVER_FILES=str(a.icd),
        VK_ICD_FILENAMES=str(a.icd),
    )
    with tempfile.TemporaryDirectory() as temp:
        probe = Path(temp) / "observe"
        sp.run(
            [
                "cc",
                "-O2",
                "-Wall",
                "-Werror",
                HERE.with_name("x11-frames.c"),
                "-lxcb",
                "-lxcb-damage",
                "-lxcb-present",
                "-o",
                probe,
            ],
            check=True,
        )
        xv = sp.Popen(
            [
                "Xvfb",
                env["DISPLAY"],
                "-screen",
                "0",
                "1000x700x24",
                "-fbdir",
                temp,
                "-nolisten",
                "tcp",
                "-noreset",
            ],
            env=env,
        )
        try:
            fbfile = Path(temp) / "Xvfb_screen0"
            wait_for(
                lambda: (
                    xv.poll() is None
                    and fbfile.exists()
                    and fbfile.stat().st_size >= 1000 * 700 * 4
                    and Path(f"/tmp/.X11-unix/X{a.display}").exists()
                ),
                5,
                "owned Xvfb",
            )
            with (a.out / "samples.jsonl").open("w", buffering=1) as output:
                for rep in range(a.runs):
                    for n in a.panes:
                        for skin in a.skins[:: 1 if rep % 2 == 0 else -1]:
                            row = run(a, env, probe, n, skin, rep)
                            print(json.dumps(row), file=output)
                            print(json.dumps(row), flush=True)
        finally:
            xv.terminate()
            xv.wait(timeout=5)


if __name__ == "__main__":
    if sys.argv[1:2] == ["worker"]:
        worker(sys.argv[2], *map(int, sys.argv[3:]))
    else:
        main()
