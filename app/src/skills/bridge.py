#!/usr/bin/env python3
"""TD-owned, additive agent startup. No provider config or user skill is written."""
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import time
import uuid
from urllib.parse import quote, unquote, urlsplit


ROOT = Path(__file__).resolve().parent
MAX_MESSAGE = 8 * 1024 * 1024


def config_path():
    return Path(os.environ.get("TD_SKILLS_CONFIG", str(Path(os.environ.get(
        "XDG_CONFIG_HOME", str(Path.home() / ".config"))) / "terminal-delight/skills.json")))


def state_root():
    return Path(os.environ.get("XDG_STATE_HOME", str(Path.home() / ".local/state"))) / "terminal-delight/skill-receipts"


def read_config():
    try:
        value = json.loads(config_path().read_text())
    except FileNotFoundError:
        return {}
    if not isinstance(value, dict):
        raise ValueError("TD skill settings must be an object")
    for key in ("active", "briefing", "factory"):
        if value.get(key) is not None and not isinstance(value[key], bool):
            raise ValueError("invalid TD skill switch")
    if not isinstance(value.get("sources", []), list):
        raise ValueError("invalid TD skill sources")
    return value


def config_revision():
    try:
        return hashlib.sha256(config_path().read_bytes()).hexdigest()
    except FileNotFoundError:
        return "absent"
    except OSError:
        return "unreadable"


def start_tick(pid):
    try:
        return int(Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[19])
    except (OSError, ValueError, IndexError):
        return None


def receipt_path():
    session = os.environ.get("TD_SKILLS_SESSION", os.environ.get("TD_SESSION"))
    pane = os.environ.get("TD_SKILLS_PANE_ID", os.environ.get("TD_PANE_ID"))
    if not session or not pane or not pane.isdecimal():
        return None
    key = hashlib.sha256(session.encode()).hexdigest()[:24]
    return state_root() / key / (str(int(pane)) + ".json")


def atomic_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd, name = tempfile.mkstemp(prefix=".receipt-", dir=path.parent)
    try:
        with os.fdopen(fd, "w") as stream:
            json.dump(value, stream)
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def receipt(provider, status="unknown", **extra):
    result = {"provider": provider, "status": status, "pid": os.getpid(),
              "start_tick": start_tick(os.getpid()), "run": uuid.uuid4().hex,
              "revision": config_revision(), "available": None, "missing": [],
              "created_ms": int(time.time() * 1000)}
    result.update(extra)
    return result


def publish(value):
    target = receipt_path()
    if target:
        try:
            atomic_json(target, value)
        except OSError as error:
            # Observability must never prevent the person's agent from starting.
            print("TD Skills: could not record session status: " + str(error)[:200], file=sys.stderr)


def manifest(path):
    path = Path(path).resolve()
    text = (path / "SKILL.md").read_text()
    front = re.match(r"\A---\s*\n(.*?)\n---(?:\s*\n|$)", text, re.S)
    if not front:
        raise ValueError("skill has no frontmatter: " + str(path))
    fields = {}
    for key in ("name", "description"):
        match = re.search(r"^" + key + r":\s*(.+)$", front[1], re.M)
        if not match:
            raise ValueError("skill needs " + key + ": " + str(path))
        value = match[1].strip()
        if value in ("|", ">", "|-", ">-"):
            tail = front[1][match.end():]
            lines = []
            for line in tail.splitlines():
                if line and not line[0].isspace():
                    break
                if line.strip():
                    lines.append(line.strip())
            value = " ".join(lines)
        fields[key] = value.strip("\"'")
    if not re.fullmatch(r"[a-z0-9][a-z0-9:_-]{0,127}", fields["name"]):
        raise ValueError("invalid skill name: " + str(path))
    fields["path"] = str(path)
    return fields


def selected(config):
    choices = []
    if config.get("briefing") is not False:
        choices.append({"path": str(ROOT / "bundle/briefing"), "origin": "TD bundle"})
    if config.get("factory") is not False:
        choices.append({"path": str(ROOT / "bundle/software-factory"), "origin": "TD bundle"})
    choices.extend(s for s in config.get("sources", []) if s.get("enabled") is not False)
    good, missing, seen = [], [], set()
    for source in choices:
        try:
            entry = dict(source, **manifest(source["path"]))
            if entry["name"] in seen:
                raise ValueError("selected skill name occurs twice: " + entry["name"])
            seen.add(entry["name"])
            if entry.get("revision", "").startswith("local:"):
                entry["revision"] = local_revision(Path(entry["path"]))
            entry.setdefault("revision", hashlib.sha256((Path(entry["path"]) / "SKILL.md").read_bytes()).hexdigest())
            good.append(entry)
        except (OSError, ValueError, KeyError, TypeError) as error:
            missing.append({"name": source.get("name", source.get("path", "unavailable")), "reason": str(error)[:400]})
    return good, missing


def discover(root):
    root = Path(root).resolve()
    found = []
    for directory, dirs, files in os.walk(root, followlinks=False):
        relative = Path(directory).relative_to(root)
        dirs[:] = sorted(d for d in dirs if d not in (".git", "node_modules", "target", ".venv"))
        if len(relative.parts) >= 5:
            dirs[:] = []
        if "SKILL.md" in files:
            found.append(manifest(directory))
            dirs[:] = []
        if len(found) > 128:
            raise ValueError("select a smaller skill subfolder (at most 128 skills per import)")
    if not found:
        raise ValueError("no SKILL.md found in this folder")
    return found


def local_revision(root):
    digest, count, size = hashlib.sha256(), 0, 0
    for directory, dirs, files in os.walk(root, followlinks=False):
        dirs[:] = sorted(d for d in dirs if d not in (".git", "node_modules", "target", ".venv"))
        for name in sorted(files):
            path = Path(directory) / name
            if path.is_symlink():
                data = os.readlink(path).encode()
            else:
                count += 1
                size += path.stat().st_size
                if count > 4000 or size > 64 * 1024 * 1024:
                    raise ValueError("select a smaller local skill folder")
                data = path.read_bytes()
            digest.update(str(path.relative_to(root)).encode() + b"\0" + data)
    return "local:" + digest.hexdigest()


def source_reference(source, subfolder="", revision="HEAD"):
    """Recognize a pasted reference; Git resolves ambiguous branch/path splits."""
    source = source.strip()
    if len(source) > 2 and source[0] == source[-1] and source[0] in "\"'":
        source = source[1:-1]
    if not source:
        raise ValueError("paste a skill file, folder or repository URL")
    if any(ord(c) < 32 for c in source):
        raise ValueError("paste one source at a time")
    if source.startswith("file://"):
        parsed = urlsplit(source)
        if parsed.netloc not in ("", "localhost"):
            raise ValueError("a file URL must name a file on this computer")
        source = unquote(parsed.path)
    if Path(source).expanduser().exists():
        return source, subfolder, revision, None
    if re.fullmatch(r"[a-fA-F0-9]{7,40}", source) or source.startswith("refs/"):
        raise ValueError("a revision needs a repository; paste its URL followed by #branch, #tag or #commit")
    if not source.startswith(("https://", "ssh://", "git@")):
        raise ValueError("this path does not exist; paste an existing file/folder or a repository URL")
    source, marker, pin = source.partition("#")
    if marker:
        if revision != "HEAD":
            raise ValueError("the repository has two revisions; keep just one")
        revision, colon, folder = unquote(pin).partition(":")
        if colon:
            if subfolder:
                raise ValueError("the repository has two skill folders; keep just one")
            subfolder = folder
    tree = None
    if not source.startswith("git@"):
        parsed = urlsplit(source)
        if parsed.password or (parsed.scheme == "https" and parsed.username):
            raise ValueError("use Git's credential helper, not credentials in the URL")
        parts = [unquote(p) for p in parsed.path.strip("/").split("/")]
        if parsed.hostname == "raw.githubusercontent.com" and len(parts) >= 4:
            source = "https://github.com/" + "/".join(parts[:2])
            tree = parts[2:]
        elif parsed.hostname == "github.com" and len(parts) >= 4 and parts[2] in ("tree", "blob"):
            source = "https://github.com/" + "/".join(parts[:2])
            tree = parts[3:]
        if tree is not None and (marker or subfolder or revision != "HEAD"):
            raise ValueError("the GitHub link already names its revision and folder")
    return source, subfolder, revision, tree


def github_target(repository, revision, relative):
    parsed = urlsplit(repository)
    if parsed.hostname == "github.com":
        path = parsed.path.removesuffix(".git").rstrip("/")
        return "https://github.com" + path + "/tree/" + quote(revision, safe="") + "/" + quote(str(relative), safe="/")
    if repository.startswith("git@github.com:"):
        return github_target("https://github.com/" + repository.split(":", 1)[1], revision, relative)
    return repository if repository.startswith("https://") else None


def local_target(base, path):
    try:
        repo = Path(subprocess.check_output(["git", "-C", str(base), "rev-parse", "--show-toplevel"], text=True, stderr=subprocess.DEVNULL, timeout=3).strip())
        remote = subprocess.check_output(["git", "-C", str(repo), "remote", "get-url", "origin"], text=True, stderr=subprocess.DEVNULL, timeout=3).strip()
        rev = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True, stderr=subprocess.DEVNULL, timeout=3).strip()
        return github_target(remote, rev, Path(path).relative_to(repo)) or Path(path).as_uri()
    except (OSError, ValueError, subprocess.SubprocessError):
        return Path(path).as_uri()


def import_source(source, subfolder="", revision="HEAD"):
    source, subfolder, revision, tree = source_reference(source, subfolder, revision)
    local = Path(source).expanduser()
    if local.exists():
        base = local.resolve()
        if base.is_file() and base.name == "SKILL.md":
            base = base.parent
        if not base.is_dir():
            raise ValueError("select a skill folder or SKILL.md")
        selected_root = (base / subfolder).resolve()
        selected_root.relative_to(base)
        entries = discover(selected_root)
        rev = local_revision(selected_root)
        origin = str(base)
        for entry in entries:
            entry["url"] = local_target(base, entry["path"])
    else:
        if not (source.startswith("https://") or source.startswith("ssh://") or source.startswith("git@")):
            raise ValueError("use an HTTPS/SSH repository URL or an existing local folder")
        if source.startswith("https://") and "@" in source.split("/", 3)[2]:
            raise ValueError("use Git's credential helper, not credentials in the URL")
        if not revision or revision.startswith("-") or any(ord(c) < 32 for c in revision):
            raise ValueError("invalid repository revision")
        cache = ROOT.parent / "sources"
        cache.mkdir(parents=True, exist_ok=True, mode=0o700)
        # Import is an explicit update action. Resolve its ref once, then keep
        # an immutable commit directory so running sessions retain their files.
        temporary = Path(tempfile.mkdtemp(prefix=".import-", dir=cache))
        try:
            subprocess.run(["git", "clone", "--filter=blob:none", "--no-checkout", "--", source, str(temporary / "repo")],
                           check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120)
            if tree is not None:
                # Longest existing ref wins, including refs containing slashes.
                # The remaining path is then checked against the checkout root.
                for count in range(len(tree), 0, -1):
                    candidate = "/".join(tree[:count])
                    for ref in (candidate, "origin/" + candidate):
                        check = subprocess.run(["git", "-C", str(temporary / "repo"), "rev-parse", "--verify", "--end-of-options", ref + "^{commit}"],
                                               text=True, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=10)
                        if check.returncode == 0:
                            revision = check.stdout.strip()
                            subfolder = "/".join(tree[count:])
                            break
                    else:
                        continue
                    break
                else:
                    raise ValueError("the GitHub link names a revision that this repository does not contain")
                if subfolder.endswith("/SKILL.md") or subfolder == "SKILL.md":
                    subfolder = str(Path(subfolder).parent)
            subprocess.run(["git", "-C", str(temporary / "repo"), "checkout", "--detach", revision],
                           check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120)
            rev = subprocess.check_output(["git", "-C", str(temporary / "repo"), "rev-parse", "HEAD"], text=True, timeout=10).strip()
            key = hashlib.sha256((source + "\0" + rev).encode()).hexdigest()
            base = cache / key
            if not base.exists():
                try:
                    os.rename(temporary / "repo", base)
                except FileExistsError:
                    pass
        finally:
            shutil.rmtree(temporary, ignore_errors=True)
        selected_root = (base / subfolder).resolve()
        selected_root.relative_to(base.resolve())
        # A repository-supplied link must not escape its fetched package.
        for directory, dirs, files in os.walk(base, followlinks=False):
            dirs[:] = [d for d in dirs if d != ".git"]
            for name in dirs + files:
                path = Path(directory) / name
                if path.is_symlink():
                    path.resolve().relative_to(base.resolve())
        entries, origin = discover(selected_root), source
        for entry in entries:
            entry["url"] = github_target(source, rev, Path(entry["path"]).relative_to(base))
    for entry in entries:
        entry.update(origin=origin, revision=rev, enabled=True)
    return entries


class NativeSocket:
    """Bounded JSON-RPC over a private local WebSocket; no TCP listener."""
    def __init__(self, path):
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.settimeout(6)
        try:
            self.sock.connect(str(path))
            key = base64.b64encode(os.urandom(16)).decode()
            self.sock.sendall(("GET /rpc HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
                               "Sec-WebSocket-Key: " + key + "\r\nSec-WebSocket-Version: 13\r\n\r\n").encode())
            headers = b""
            while not headers.endswith(b"\r\n\r\n"):
                if len(headers) >= 16384:
                    raise ValueError("oversized native handshake")
                headers += self.exact(1)
            accept = base64.b64encode(hashlib.sha1((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()).digest()).decode()
            if not headers.startswith(b"HTTP/1.1 101 ") or accept.encode() not in headers:
                raise ValueError("native server rejected the handshake")
        except Exception:
            self.sock.close()
            raise
        self.counter = 0

    def close(self):
        self.sock.close()

    def exact(self, n):
        data = bytearray()
        while len(data) < n:
            chunk = self.sock.recv(n - len(data))
            if not chunk:
                raise OSError("native connection closed")
            data.extend(chunk)
        return bytes(data)

    def send(self, value, opcode=1):
        data = json.dumps(value).encode() if opcode == 1 else value
        size, mask = len(data), os.urandom(4)
        if size > MAX_MESSAGE:
            raise ValueError("native message too large")
        header = bytes([128 | opcode, 128 | size]) if size < 126 else bytes([128 | opcode, 254]) + struct.pack("!H", size) if size < 65536 else bytes([128 | opcode, 255]) + struct.pack("!Q", size)
        self.sock.sendall(header + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))

    def receive(self):
        data = bytearray()
        while True:
            first, second = self.exact(2)
            length, opcode = second & 127, first & 15
            if length == 126:
                length = struct.unpack("!H", self.exact(2))[0]
            elif length == 127:
                length = struct.unpack("!Q", self.exact(8))[0]
            if second & 128 or length + len(data) > MAX_MESSAGE:
                raise ValueError("invalid native frame")
            payload = self.exact(length)
            if opcode == 8:
                raise OSError("native server closed")
            if opcode == 9:
                self.send(payload, 10)
                continue
            if opcode == 10:
                continue
            if opcode not in (0, 1):
                raise ValueError("unsupported native frame")
            data.extend(payload)
            if first & 128:
                return json.loads(data)

    def call(self, method, params):
        self.counter += 1
        target, deadline = self.counter, time.monotonic() + 12
        self.send({"jsonrpc": "2.0", "id": target, "method": method, "params": params})
        while time.monotonic() < deadline:
            value = self.receive()
            if value.get("id") == target:
                if "error" in value:
                    raise ValueError(str(value["error"].get("message", "native request failed"))[:400])
                return value.get("result")
            if "id" in value and "method" in value:
                self.send({"jsonrpc": "2.0", "id": value["id"], "error": {"code": -32601, "message": "TD catalog client does not execute tools"}})
        raise TimeoutError("native catalog request timed out")


def merge_dict(base, extra):
    for key, value in extra.items():
        if isinstance(value, dict) and isinstance(base.get(key), dict):
            merge_dict(base[key], value)
        else:
            base[key] = value
    return base


def claude_settings(args):
    files = [Path(os.environ.get("CLAUDE_CONFIG_DIR", str(Path.home() / ".claude"))) / "settings.json"]
    ancestors = list(reversed([Path.cwd(), *Path.cwd().parents]))
    for directory in ancestors:
        files.extend([directory / ".claude/settings.json", directory / ".claude/settings.local.json"])
    effective = {}
    for path in files:
        if path.exists():
            merge_dict(effective, json.loads(path.read_text()))
    supplied, forwarded, index = {}, [], 0
    while index < len(args):
        arg = args[index]
        if arg == "--settings":
            if index + 1 == len(args):
                raise ValueError("--settings needs a value")
            text = args[index + 1]
            index += 2
        elif arg.startswith("--settings="):
            text, index = arg.split("=", 1)[1], index + 1
        else:
            forwarded.append(arg)
            index += 1
            continue
        supplied = json.loads(text) if text.lstrip().startswith("{") else json.loads(Path(text).expanduser().read_text())
    merge_dict(effective, supplied)
    # Managed policy outranks the command line.
    managed = Path("/etc/claude-code/managed-settings.json")
    if managed.exists():
        merge_dict(effective, json.loads(managed.read_text()))
    return effective, supplied, forwarded


def existing_claude_skills():
    roots = [Path("/etc/claude-code/skills"), Path(os.environ.get("CLAUDE_CONFIG_DIR", str(Path.home() / ".claude"))) / "skills"]
    roots.extend(p / ".claude/skills" for p in reversed([Path.cwd(), *Path.cwd().parents]))
    entries = {}
    for root in roots:
        if not root.is_dir():
            continue
        for path in root.iterdir():
            if (path / "SKILL.md").is_file():
                try:
                    item = manifest(path)
                    item.update(origin="existing agent skill", revision=hashlib.sha256((path / "SKILL.md").read_bytes()).hexdigest())
                    # Managed and personal skills win over project skills.
                    entries.setdefault(item["name"], item)
                except (OSError, ValueError):
                    pass
    return entries


def clean_run(path):
    path = Path(path)
    allowed = (state_root() / "runs").resolve()
    if path.resolve().parent == allowed and re.fullmatch(r"\d+-[A-Za-z0-9_]+", path.name) and not path.is_symlink():
        shutil.rmtree(path, ignore_errors=True)


def wake_named_runs():
    """Run folders a sleeping agent's wake script still names.

    A window older than the wake-through-the-bridge fix replays a sleeper's
    launch line word for word, folder and all, so pruning that folder leaves
    the agent unable to wake ("Settings file not found"). Unreadable scripts
    name nothing; a folder they might have named is pruned as before."""
    named = set()
    try:
        scripts = sorted((state_root().parent / "wake").glob("*.sh"))[:4096]
    except OSError:
        return named
    for script in scripts:
        try:
            text = script.read_text(errors="replace")
        except OSError:
            continue
        named.update(re.findall(r"skill-receipts/runs/(\d+-[A-Za-z0-9_]+)", text))
    return named


def prune_runs(runs):
    keep = None
    for index, path in enumerate(runs.iterdir()):
        if index >= 256:
            break
        match = re.fullmatch(r"(\d+)-[A-Za-z0-9_]+", path.name)
        if match and start_tick(int(match[1])) is None:
            if keep is None:
                keep = wake_named_runs()
            if path.name not in keep:
                clean_run(path)


def stage(entries, destination):
    destination.mkdir(parents=True)
    for item in entries:
        (destination / item["name"]).symlink_to(item["path"], target_is_directory=True)


def outcome(value):
    if value["available"] is None:
        value["available"] = []
    count = len(value["available"])
    value["status"] = "partial" if value["missing"] and count else "not_attached" if not count else "attached"
    publish(value)


def claude_ack(path):
    packet = json.loads(Path(path).read_text())
    event = json.load(sys.stdin)
    value = packet["receipt"]
    # PID identity is the exec'd Claude process, not this hook subprocess.
    if start_tick(value["pid"]) != value["start_tick"]:
        return 0
    value["agent_session"] = event.get("session_id")
    value["method"] = "native-directory and SessionStart context"
    value["available"] = packet["entries"]
    lines = ["Terminal Delight has added these optional skills for this session. Use a relevant skill by reading its SKILL.md; the person's instructions and exclusions retain priority."]
    for item in packet["entries"]:
        lines.append(f'- {item["name"]}: {item["description"]} ({item["path"]}/SKILL.md)')
    print(json.dumps({"hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": "\n".join(lines)}}), flush=True)
    outcome(value)
    return 0


def run_claude(real, args, entries, value, run_dir):
    effective, supplied, forwarded = claude_settings(args)
    overrides = effective.get("skillOverrides", {})
    allowed, added, existing = [], [], existing_claude_skills()
    denied = effective.get("permissions", {}).get("deny", [])
    for item in entries:
        denied_skill = any(rule == "Skill" or rule in (f'Skill({item["name"]})', f'Skill({item["name"]} *)') for rule in denied)
        if overrides.get(item["name"]) == "off" or denied_skill:
            value["missing"].append({"name": item["name"], "reason": "disabled in Claude settings"})
        else:
            allowed.append(existing.get(item["name"], item))
            if item["name"] not in existing:
                added.append(item)
    if not allowed:
        outcome(value)
        os.execv(real, [real, *args])
    stage(added, run_dir / ".claude/skills")
    packet = run_dir / "ack.json"
    atomic_json(packet, {"receipt": value, "entries": allowed})
    hook = f"python3 {shlex.quote(str(ROOT / 'bridge.py'))} acknowledge {shlex.quote(str(packet))}"
    hooks = supplied.setdefault("hooks", {}).setdefault("SessionStart", [])
    hooks.append({"hooks": [{"type": "command", "command": hook, "timeout": 10}]})
    # A conversation ending is not proof that its process exited. Keep the
    # native directory valid across clear/resume; prune only dead PIDs later.
    settings = run_dir / "settings.json"
    atomic_json(settings, supplied)
    value["reason"] = "waiting for Claude's SessionStart confirmation"
    publish(value)
    os.execv(real, [real, "--add-dir", str(run_dir), "--settings", str(settings), *forwarded])


def codex_options(args):
    server, cwd, index = [], Path.cwd(), 0
    while index < len(args):
        arg = args[index]
        if arg in ("-c", "--config", "-p", "--profile", "--enable", "--disable", "-C", "--cd"):
            if index + 1 == len(args):
                raise ValueError("missing Codex option value")
            option = args[index + 1]
            if arg in ("-C", "--cd"):
                cwd = Path(option).expanduser().absolute()
            else:
                server.extend([arg, option])
            index += 2
        elif any(arg.startswith(name + "=") for name in ("--config", "--profile", "--enable", "--disable")):
            server.append(arg)
            index += 1
        elif arg.startswith("--cd="):
            cwd = Path(arg.split("=", 1)[1]).expanduser().absolute()
            index += 1
        else:
            index += 1
    return server, cwd


def run_codex(real, args, entries, value, run_dir):
    server_options, cwd = codex_options(args)
    endpoint = run_dir / "codex.sock"
    server = client = tui = None
    def terminate(_signum, _frame):
        raise KeyboardInterrupt()
    previous = {s: signal.signal(s, terminate) for s in (signal.SIGTERM, signal.SIGHUP, signal.SIGINT)}
    try:
        with (run_dir / "server.log").open("wb") as log:
            server = subprocess.Popen([real, *server_options, "app-server", "--listen", "unix://" + str(endpoint)],
                                      cwd=cwd, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=log)
            deadline = time.monotonic() + 8
            while not endpoint.exists() and server.poll() is None and time.monotonic() < deadline:
                time.sleep(.025)
            if not endpoint.exists():
                log.flush()
                detail = (run_dir / "server.log").read_text(errors="replace")[-600:]
                raise RuntimeError("Codex did not open its private skill endpoint: " + detail)
            client = NativeSocket(endpoint)
            client.call("initialize", {"clientInfo": {"name": "terminal-delight-skills", "version": "1"}, "capabilities": {"experimentalApi": True}})
            client.send({"jsonrpc": "2.0", "method": "initialized"})
            def catalog():
                result = client.call("skills/list", {"cwds": [str(cwd)], "forceReload": True})
                return [s for entry in result["data"] for s in entry["skills"]]
            before = catalog()
            value["available"] = []
            additions, inherited = [], []
            for item in entries:
                matches = [s for s in before if s["name"] == item["name"]]
                if any(s.get("enabled") is False for s in matches):
                    value["missing"].append({"name": item["name"], "reason": "disabled in Codex settings"})
                elif any(s.get("enabled") is True for s in matches):
                    existing = next(s for s in matches if s.get("enabled") is True)
                    owned = dict(item)
                    native_path = Path(existing["path"])
                    owned.update(path=str(native_path.parent), origin="existing agent skill",
                                 revision=hashlib.sha256(native_path.read_bytes()).hexdigest())
                    inherited.append(owned)
                else:
                    additions.append(item)
            stage(additions, run_dir / "native")
            client.call("skills/extraRoots/set", {"extraRoots": [str(run_dir / "native")]})
            after = catalog()
            baseline = {(s["name"], s["path"]) for s in before if s.get("enabled") is True}
            remaining = {(s["name"], s["path"]) for s in after if s.get("enabled") is True}
            if not baseline.issubset(remaining):
                raise RuntimeError("Codex's existing skill catalog changed during attachment")
            for item in inherited + additions:
                if any(s["name"] == item["name"] and s.get("enabled") is True for s in after):
                    value["available"].append(item)
                else:
                    value["missing"].append({"name": item["name"], "reason": "not enabled in native Codex catalog"})
            value["method"] = "Codex skills/list"
            outcome(value)
            if os.environ.get("TD_SKILLS_CATALOG_PROBE") == "1":
                value["native_catalog"] = [s["name"] for s in after if s.get("enabled") is True]
                print(json.dumps(value))
                return 0
            tui = subprocess.Popen([real, "--remote", "unix://" + str(endpoint), *args])
            return tui.wait()
    finally:
        # Closing the PTY can deliver HUP immediately after TERM/INT. A second
        # signal must not interrupt reaping our own server and leave it behind.
        for signum in previous:
            signal.signal(signum, signal.SIG_IGN)
        if client:
            client.close()
        for child in (tui, server):
            if child and child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()
        for signum, handler in previous.items():
            signal.signal(signum, handler)


def real_binary(provider):
    candidate = os.environ.get("TD_SKILLS_REAL_" + provider.upper())
    if candidate:
        return str(Path(candidate).resolve())
    for directory in os.get_exec_path():
        candidate = Path(directory) / provider
        if candidate.is_file() and os.access(candidate, os.X_OK):
            resolved = candidate.resolve()
            if resolved == Path(__file__).resolve():
                continue
            if resolved.name == "bridge.py" and (resolved.parent / "bundle/software-factory/SKILL.md").is_file():
                continue
            return str(candidate)
    raise FileNotFoundError(provider + " is not on PATH")


def run(provider, args):
    real = real_binary(provider)
    if not receipt_path():
        os.execv(real, [real, *args])
    value = receipt(provider)
    try:
        config = read_config()
        if config.get("active") is not True:
            value.update(status="not_attached", available=[], reason="TD Skills is disabled")
            publish(value)
            os.execv(real, [real, *args])
        if any(a in ("--help", "-h", "--version", "-V") for a in args):
            os.execv(real, [real, *args])
        bypass = ("--remote", "--remote-auth-token-env", "--bare", "--safe-mode", "--disable-slash-commands")
        # Native skills/list currently has no profile selection, and app-server
        # refuses --profile. Preserve it instead of attaching the wrong catalog.
        if provider == "codex":
            bypass += ("-p", "--profile")
        commands = {"exec", "e", "review", "mcp", "plugin", "plugins", "login", "logout", "auth", "install", "update", "app-server", "remote-control", "features", "doctor", "completion", "debug", "setup-token"}
        if any(a.split("=", 1)[0] in bypass for a in args) or any(a in commands for a in args):
            value["reason"] = "this agent invocation bypasses TD skill attachment"
            publish(value)
            os.execv(real, [real, *args])
        entries, value["missing"] = selected(config)
        value["requested"] = [s["name"] for s in entries] + [s["name"] for s in value["missing"]]
        if not entries:
            outcome(value)
            os.execv(real, [real, *args])
        runs = state_root() / "runs"
        runs.mkdir(parents=True, exist_ok=True, mode=0o700)
        prune_runs(runs)
        run_dir = Path(tempfile.mkdtemp(prefix=f"{os.getpid()}-", dir=runs))
        # Unix socket paths are limited to 108 bytes. Use a private short directory.
        if provider == "codex":
            shutil.rmtree(run_dir)
            with tempfile.TemporaryDirectory(prefix="td-skills-") as scratch:
                try:
                    return run_codex(real, args, entries, value, Path(scratch))
                except (OSError, ValueError, RuntimeError, KeyError, TypeError) as error:
                    value.update(status="not_attached", available=[], reason="Codex skill attachment failed: " + str(error)[:400])
                    publish(value)
                    if os.environ.get("TD_SKILLS_CATALOG_PROBE") == "1":
                        print(json.dumps(value))
                        return 1
                    print("TD Skills: attachment failed; starting your ordinary Codex.", file=sys.stderr)
            os.execv(real, [real, *args])
        if provider == "claude":
            run_claude(real, args, entries, value, run_dir)
        else:
            value["reason"] = "no native TD skills adapter for this agent"
            publish(value)
            os.execv(real, [real, *args])
    except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        value.update(status="not_attached", reason="TD skill attachment failed: " + str(error)[:400])
        publish(value)
        print("TD Skills: attachment failed; starting your ordinary agent.", file=sys.stderr)
        os.execv(real, [real, *args])
    return 1


def main():
    provider = Path(sys.argv[0]).name
    if provider in ("claude", "codex", "gemini", "opencode", "crush"):
        return run(provider, sys.argv[1:])
    command, *args = sys.argv[1:]
    if command == "acknowledge":
        return claude_ack(args[0])
    if command == "cleanup":
        clean_run(args[0])
        return 0
    if command == "import":
        print(json.dumps(import_source(*args)))
        return 0
    if command == "run":
        return run(args[0], args[1:])
    raise ValueError("unknown TD skills command")


if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        sys.exit(130)
    except (OSError, ValueError, subprocess.SubprocessError) as exc:
        print("TD Skills: " + str(exc)[:800], file=sys.stderr)
        sys.exit(1)
