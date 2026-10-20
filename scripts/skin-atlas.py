#!/usr/bin/env python3
"""Capture one real TD skin fixture in an isolated session.

The default backend is a hidden Hyprland 0.56 workspace. --visible is the
portable fallback: launch a private window and supply --capture as an argv
array, or inspect the window with --hold. No normal session is adopted.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import signal
import socket
import subprocess
import sys
import time
import uuid


def digest(path):
    h = hashlib.sha256()
    with open(path, "rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def run(argv, **kwargs):
    return subprocess.check_output(argv, text=True, **kwargs).strip()


def request(pid, line):
    path = Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")) / "terminal-delight" / f"ctl-{pid}.sock"
    with socket.socket(socket.AF_UNIX) as peer:
        peer.settimeout(15)
        peer.connect(str(path))
        peer.sendall((line + "\n").encode())
        return peer.makefile().readline().strip()


def rpc(pid, name, arguments, origin=None):
    prefix = "mcp rpc " if origin is None else f"mcp from {origin[0]} {origin[1]} rpc "
    answer = json.loads(request(pid, prefix + json.dumps({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": arguments}})))
    if answer.get("error") or answer.get("result", {}).get("isError"):
        raise RuntimeError(f"{name}: {answer}")
    return answer["result"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--skin", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--scene", default="workspace", help="fixture id from this binary's skin --catalogue export")
    parser.add_argument("--theme", default="hacker")
    parser.add_argument("--palette", help="bundled palette id; overrides the design's default colours")
    parser.add_argument("--size", default="1200x800")
    parser.add_argument("--ui-scale", type=float, default=1.)
    parser.add_argument("--text-scale", type=float, default=1.)
    parser.add_argument("--warp", type=float, default=0.)
    parser.add_argument("--lang", default="en")
    parser.add_argument("--grade-json", type=Path, help="partial MCP grade in 0..100 percent units, applied only to the private preview")
    parser.add_argument("--visible", action="store_true")
    parser.add_argument("--capture", help='JSON argv array; {pid}, {window} and {output} are replaced, never shell-evaluated')
    parser.add_argument("--hold", action="store_true", help="keep the private window until Enter, for manual inspection")
    parser.add_argument("--verify-reload", action="store_true", help="exercise valid/invalid edits and restore in the private session")
    parser.add_argument("--idle-seconds", type=float, default=0., help="sample this private process's idle CPU and resident memory after capture")
    args = parser.parse_args()
    binary, skin, out = args.binary.resolve(), args.skin.resolve(), args.out.resolve()
    if args.palette and ("/" in args.palette or "\\" in args.palette):
        parser.error("native --palette takes a bundled id; the headless resolver also accepts TD palette files")
    width, height = map(int, args.size.split("x"))
    if not (300 <= width <= 4000 and 250 <= height <= 3000):
        parser.error("size must fit a native review window")
    # Validation happens before a window is launched or an output is created.
    resolve_args = [str(binary), "skin", "--check", "--skin", str(skin), "--theme", args.theme, "--scale", str(args.ui_scale), "--text-size", str(args.text_scale)]
    if args.palette:
        resolve_args += ["--palette", args.palette]
    resolved = json.loads(run(resolve_args))
    catalogue = json.loads(run([str(binary), "skin", "--catalogue"]))
    fixtures = {fixture["id"]: fixture for fixture in catalogue["fixtures"]}
    if args.scene not in fixtures:
        parser.error("scene must be one of: " + ", ".join(fixtures))
    fixture = fixtures[args.scene]
    if out.exists() and any(out.iterdir()):
        parser.error("out must be a new or empty directory; captures never overwrite a review")
    out.mkdir(parents=True, exist_ok=True)
    for directory in ["config/terminal-delight/sessions", "cache", "state", "tmp"]:
        (out / directory).mkdir(parents=True)
    shutil.copyfile(__file__, out / "runner.py")
    shutil.copyfile(skin, out / "skin.toml")
    (out / "resolved.json").write_text(json.dumps(resolved, indent=2) + "\n")
    (out / "catalogue.json").write_text(json.dumps(catalogue, indent=2) + "\n")
    session = "skin-atlas-" + uuid.uuid4().hex[:12]
    emitter = out / "emitter.py"
    emitter.write_text("import time\nprint('\\033[36mCustom surface native fixture\\033[0m\\n\\n✓ measured zero: 0%\\n◇ reading unavailable: —\\n\\n> review the native controls',flush=True)\nwhile True: time.sleep(30)\n")
    val = json.dumps
    head = f'active = 0\nlang = {val(args.lang)}\nleft_bar = true\nleft_bar_w = 210.0\nskin = "custom"\nwin = [0.0, 0.0, {width}.0, {height}.0]\n\n[theme]\nid = {val(args.theme)}\ncolor = "default"\nfollow_omarchy = false\n'
    if args.palette:
        head += f'palette = {val(args.palette)}\n'
    head += '\n[theme.grade]\n'
    for key, value in dict(crt=args.warp != 0, warp=args.warp, scale=args.ui_scale, text_size=args.text_scale, brightness=.5, contrast=.5, colour=.5, text=.5, background=.5, gamma=.5, phosphor=1.).items():
        head += f'{key} = {val(value)}\n'
    for i, (title, group) in enumerate([("Custom surface", 1), ("Unknown readings", 1), ("Native fixtures", 1)]):
        if i == 0:
            head += '\n[[projects]]\nid = 1\nname = "SKIN ATLAS"\ncollapsed = false\ncolor = "#8C94A6"\n'
        head += f'\n[[tabs]]\nname = {val(title)}\nproject = {group}\n[tabs.node.Leaf]\ncwd = {val(str(out))}\nresume = {val(shlex.join([sys.executable, str(emitter)]))}\n'
    saved = out / "config/terminal-delight/sessions" / f"{session}.toml"
    saved.write_text(head)
    desktop_keys = {"HOME", "USER", "LOGNAME", "PATH", "LANG", "DISPLAY", "WAYLAND_DISPLAY", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS", "XDG_SESSION_TYPE", "XDG_CURRENT_DESKTOP", "HYPRLAND_INSTANCE_SIGNATURE", "WGPU_BACKEND", "VK_ICD_FILENAMES", "LIBGL_ALWAYS_SOFTWARE"}
    env = {key: value for key, value in os.environ.items() if key in desktop_keys or key.startswith("LC_")}
    env.update(TD_SESSION=session, TD_SKIN=str(out / "skin.toml"), XDG_CONFIG_HOME=str(out / "config"), XDG_CACHE_HOME=str(out / "cache"), XDG_STATE_HOME=str(out / "state"), TMPDIR=str(out / "tmp"), SHELL="/bin/bash")
    env.update(fixture["environment"])
    launch = out / "launch.sh"
    launch.write_text("#!/bin/sh\nexec env -i " + shlex.join([f"{key}={value}" for key, value in env.items()]) + " " + shlex.quote(str(binary)) + " > " + shlex.quote(str(out / "window.log")) + " 2>&1\n")
    pid = None
    process = None
    receipt = {"schema": "td-skin-proof/1", "scene": args.scene, "session": session, "binary_sha256": digest(binary), "skin_sha256": digest(out / "skin.toml"), "catalogue_schema": catalogue["schema"], "theme": args.theme, "palette": args.palette, "geometry_requested": [width, height], "geometry_observed": None, "grade": {"warp": args.warp, "ui_scale": args.ui_scale, "text_scale": args.text_scale}, "lang": args.lang, "capture": None, "visual_verdict": None, "states_reviewed": None}
    try:
        if args.visible:
            process = subprocess.Popen([str(binary)], env=env, stdout=(out / "window.log").open("w"), stderr=subprocess.STDOUT)
            pid = process.pid
        else:
            monitors = json.loads(run(["hyprctl", "monitors", "-j"]))
            monitor = next((m["name"] for m in monitors if m.get("focused")), monitors[0]["name"])
            workspace = "special:" + session
            cmd = "sh " + shlex.quote(str(launch))
            result = run(["hyprctl", "dispatch", f"hl.dsp.exec_cmd({val(cmd)}, {{ monitor = {val(monitor)}, workspace = {val(workspace + ' silent')}, render_unfocused = true, no_initial_focus = true }})"])
            if result not in ("ok", ""):
                raise RuntimeError("hidden launch refused: " + result + "; use --visible on another compositor")
            deadline = time.monotonic() + 35
            client = None
            while time.monotonic() < deadline:
                clients = json.loads(run(["hyprctl", "clients", "-j"]))
                client = next((c for c in clients if c["title"] == "terminal-delight — " + session), None)
                if client:
                    pid = client["pid"]
                    if client["workspace"]["name"] != workspace:
                        raise RuntimeError("private window was not hidden; aborting without adopting another window")
                    break
                time.sleep(.3)
            if not client:
                raise RuntimeError("native window did not appear; see window.log")
            address = client["address"]
            if not client["floating"]:
                run(["hyprctl", "dispatch", f"hl.dsp.window.float({{ window = 'address:{address}' }})"])
            run(["hyprctl", "dispatch", f"hl.dsp.window.resize({{ x = {width}, y = {height}, window = 'address:{address}' }})"])
            client = next(c for c in json.loads(run(["hyprctl", "clients", "-j"])) if c["pid"] == pid)
            receipt["geometry_observed"] = client["size"]
        deadline = time.monotonic() + 25
        while time.monotonic() < deadline:
            try:
                if request(pid, "ping") == "pong":
                    break
            except (OSError, TimeoutError):
                pass
            time.sleep(.3)
        else:
            raise RuntimeError("private control socket did not answer")
        if digest(Path(f"/proc/{pid}/exe")) != receipt["binary_sha256"]:
            raise RuntimeError("binary changed during launch; capture is not attributed to the validated build")
        receipt["runner_sha256"] = digest(out / "runner.py")
        for command in ["mcp on", "mcp writes on", "mcp expose all", "skin custom"]:
            answer = request(pid, command)
            if answer.startswith("err"):
                raise RuntimeError(f"{command}: {answer}")
        # The control socket acknowledges enqueueing. Wait for the UI's mirror
        # to confirm application before the MCP server's exposure guard runs.
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            status = request(pid, "mcp status")
            if all(part in status for part in ["enabled=on", "writes=on", "expose=all"]):
                receipt["mcp_status"] = status
                break
            time.sleep(.2)
        else:
            raise RuntimeError("private MCP policy was not applied: " + status)
        if fixture["action"] == "paint":
            request(pid, "paint on")
        panes = rpc(pid, "list_panes", {})["structuredContent"]["panes"]
        (out / "panes.json").write_text(json.dumps(panes, indent=2) + "\n")
        target = next(p for p in panes if p["tab"] == 0)
        if args.grade_json:
            grade = json.loads(args.grade_json.read_text())
            receipt["grade_override"] = grade
            rpc(pid, "set_pane_config", {"updates": [{"target": "outer", "config": grade}]})
        receipt["pane_config"] = rpc(pid, "get_pane_config", {"targets": [target["pid"], "outer"]})
        rpc(pid, "leave_note", {"pid": target["pid"], "title": "NATIVE FIXTURE", "text": "Reference skin; unknown readings stay unknown.", "pin": False})
        if fixture["action"] == "document":
            document = out / "content.md"
            document.write_text("# Authored content\n\nNative controls follow the skin; this document keeps its reading typography.\n\n```rust\nlet reading: Option<f32> = None;\n```\n" + "\n\n".join(f"## Reading {i}\n\nA measured zero and an unavailable reading mean different things. The surrounding native rail reports progress through this long document." for i in range(1, 21)))
            rpc(pid, "open_document", {"path": str(document), "placement": "here"}, origin=(session, target["pane_id"]))
        if fixture["action"] == "bench":
            surface = out / "state/terminal-delight/surfaces" / session / str(target["pane_id"])
            surface.mkdir(parents=True, exist_ok=True)
            (surface / "fixture.json").write_text(json.dumps({"td": "0.4", "kind": "response", "title": "A custom surface", "model": {"layman": "Native controls follow the skin file.", "brief": "A real workbench fixture.", "evidence": ["Unknown is shown as unavailable."]}}))
            request(pid, "bench on")
        time.sleep(2)
        receipt["skin_status"] = request(pid, "skin status")
        receipt["live_skin"] = json.loads(request(pid, "skin snapshot"))
        if args.verify_reload:
            original = (out / "skin.toml").read_bytes()
            before = receipt["live_skin"]

            def await_skin(predicate):
                deadline = time.monotonic() + 8
                while time.monotonic() < deadline:
                    live = json.loads(request(pid, "skin snapshot"))
                    if predicate(live):
                        return live
                    time.sleep(.15)
                raise RuntimeError("private live skin did not reach the expected state")

            try:
                (out / "skin.toml").write_text('name = "atlas reload witness"\n[font]\nfamily = "Share Tech Mono"\n')
                valid = await_skin(lambda s: s["skin"] == "atlas reload witness")
                (out / "skin.toml").write_text("name = [invalid\n")
                deadline = time.monotonic() + 8
                while time.monotonic() < deadline:
                    if "skin reload error (keeping current)" in (out / "window.log").read_text():
                        break
                    time.sleep(.15)
                else:
                    raise RuntimeError("invalid edit was not observed by the live watcher")
                invalid = json.loads(request(pid, "skin snapshot"))
                if invalid != valid:
                    raise RuntimeError("invalid edit changed the previous valid skin")
            finally:
                (out / "skin.toml").write_bytes(original)
            restored = await_skin(lambda s: s == before)
            request(pid, "skin default")
            await_skin(lambda s: s["active_skin"] == "default")

            def await_saved(selection):
                deadline = time.monotonic() + 8
                while time.monotonic() < deadline:
                    if f'skin = "{selection}"' in saved.read_text():
                        return
                    time.sleep(.15)
                raise RuntimeError("private skin selection was not saved")

            await_saved("default")
            previous_pid = pid
            os.kill(previous_pid, signal.SIGTERM)
            if process:
                process.wait(timeout=10)
            if args.visible:
                process = subprocess.Popen([str(binary)], env=env, stdout=(out / "restart.log").open("w"), stderr=subprocess.STDOUT)
                pid = process.pid
            else:
                run(["hyprctl", "dispatch", f"hl.dsp.exec_cmd({val(cmd)}, {{ monitor = {val(monitor)}, workspace = {val(workspace + ' silent')}, render_unfocused = true, no_initial_focus = true }})"])
                deadline = time.monotonic() + 35
                while time.monotonic() < deadline:
                    client = next((c for c in json.loads(run(["hyprctl", "clients", "-j"])) if c["title"] == "terminal-delight — " + session and c["pid"] != previous_pid), None)
                    if client:
                        pid = client["pid"]
                        if client["workspace"]["name"] != workspace:
                            raise RuntimeError("restarted private preview was not hidden")
                        break
                    time.sleep(.3)
                else:
                    raise RuntimeError("private persistence restart did not appear")
            deadline = time.monotonic() + 25
            while time.monotonic() < deadline:
                try:
                    if request(pid, "ping") == "pong":
                        break
                except (OSError, TimeoutError):
                    pass
                time.sleep(.3)
            else:
                raise RuntimeError("restarted private control socket did not answer")
            persisted = await_skin(lambda s: s["active_skin"] == "default")
            request(pid, "skin custom")
            await_skin(lambda s: s == restored)
            await_saved("custom")
            if digest(Path(f"/proc/{pid}/exe")) != receipt["binary_sha256"]:
                raise RuntimeError("private restart did not use the validated binary")
            receipt["reload"] = {"valid_edit": "applied", "invalid_edit": "kept previous valid", "file_restore": "exact bytes and live tokens", "selection_restore": "custom", "saved_selection": "custom", "private_restart_selection": persisted["active_skin"]}
            if not args.visible:
                if not client["floating"]:
                    run(["hyprctl", "dispatch", f"hl.dsp.window.float({{ window = 'address:{client['address']}' }})"])
                run(["hyprctl", "dispatch", f"hl.dsp.window.resize({{ x = {width}, y = {height}, window = 'address:{client['address']}' }})"])
                client = next(c for c in json.loads(run(["hyprctl", "clients", "-j"])) if c["pid"] == pid)
                receipt["geometry_observed"] = client["size"]
            time.sleep(1)
        capture = out / "native.png"
        if args.capture:
            argv = json.loads(args.capture)
            run([str(arg).format(pid=pid, window=client.get("stableId") if not args.visible else pid, output=capture) for arg in argv])
        elif not args.visible:
            run(["grim", "-T", str(client["stableId"]), str(capture)])
        if capture.exists():
            receipt["capture"] = {"file": capture.name, "sha256": digest(capture)}
        if args.idle_seconds > 0:
            def usage():
                fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
                return int(fields[11]) + int(fields[12]), int(fields[21]) * os.sysconf("SC_PAGE_SIZE")
            start_ticks, start_rss = usage()
            start_time = time.monotonic()
            time.sleep(min(args.idle_seconds, 60.))
            end_ticks, end_rss = usage()
            elapsed = time.monotonic() - start_time
            receipt["idle_sample"] = {"elapsed_seconds": elapsed, "cpu_seconds": (end_ticks - start_ticks) / os.sysconf("SC_CLK_TCK"), "rss_bytes_start": start_rss, "rss_bytes_end": end_rss, "method": "proc stat, private GUI process only; excludes child hosts and compositor; one sample, not a regression bound"}
        if args.hold:
            print(f"Private window {pid}; press Enter to close. Output: {out}", flush=True)
            input()
    finally:
        (out / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
        if pid:
            try:
                os.kill(pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
        # Own hosts only, identified by the unguessable session and the full
        # serve argv, never by an app name or a broad killall.
        for proc in Path("/proc").iterdir():
            if not proc.name.isdigit():
                continue
            try:
                argv = (proc / "cmdline").read_bytes().split(b"\0")
                if b"serve" in argv and b"--session" in argv and session.encode() in argv:
                    os.kill(int(proc.name), signal.SIGTERM)
            except (OSError, ProcessLookupError):
                pass
        if process:
            process.wait(timeout=10)
    print(out / "receipt.json")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, RuntimeError, subprocess.SubprocessError) as error:
        sys.exit(str(error))
