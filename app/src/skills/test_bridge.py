"""Run with python3 -m unittest discover -s app/src/skills -p 'test_*.py'."""
import hashlib
import fcntl
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import json
import os
from pathlib import Path
import pty
import select
import shutil
import socket
import struct
import subprocess
import tempfile
import threading
import time
import termios
import unittest


HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("td_skill_bridge", HERE / "bridge.py")
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)


class Fixture(unittest.TestCase):
    def setUp(self):
        # Codex deliberately refuses its helper home under /tmp. Use an
        # isolated cache directory for native fixtures, never the real home.
        native_base = Path.home() / ".cache" if os.environ.get("TD_NATIVE_SKILL_TESTS") == "1" else None
        if native_base:
            native_base.mkdir(exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="td-skills-test-", dir=native_base)
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.package = self.root / "package"
        self.package.mkdir()
        shutil.copy(HERE / "bridge.py", self.package / "bridge.py")
        (self.package / "bin").mkdir()
        for name in ("claude", "codex", "gemini"):
            (self.package / "bin" / name).symlink_to("../bridge.py")
        self.extra = self.skill(self.root / "extra", "td-extra")
        self.work = self.root / "work"
        self.work.mkdir()
        self.home = self.root / "home"
        self.home.mkdir()
        (self.home / ".codex").mkdir()
        self.config = self.root / "skills.json"
        self.fake = self.root / "fake-provider"
        self.fake.write_text("#!/usr/bin/env python3\nimport json,os,sys\nfrom pathlib import Path\nPath(os.environ['ARGV_LOG']).write_text(json.dumps(sys.argv[1:]))\nsys.exit(27)\n")
        self.fake.chmod(0o755)
        self.env = dict(os.environ, HOME=str(self.home), XDG_STATE_HOME=str(self.root / "state"),
                        XDG_CONFIG_HOME=str(self.root / "config"), TD_SESSION="fixture/session", TD_PANE_ID="9",
                        TD_SKILLS_SESSION="fixture/session", TD_SKILLS_PANE_ID="9",
                        TD_SKILLS_CONFIG=str(self.config), TD_SKILLS_REAL_CLAUDE=str(self.fake),
                        ARGV_LOG=str(self.root / "argv.json"))
        self.configure()

    def skill(self, path, name):
        path.mkdir(parents=True, exist_ok=True)
        (path / "SKILL.md").write_text(f"---\nname: {name}\ndescription: A fixture skill with supporting files.\n---\nFollow the fixture.\n")
        (path / "helper.txt").write_text("supporting file")
        return path

    def configure(self, **values):
        source = dict(bridge.manifest(self.extra), origin=str(self.extra), revision="fixture-v1", enabled=True)
        config = dict(active=True, briefing=False, factory=False, sources=[source])
        config.update(values)
        self.config.write_text(json.dumps(config))

    def run_provider(self, provider="claude", args=(), **kwargs):
        return subprocess.run(["python3", str(self.package / "bin" / provider), *args], cwd=self.work,
                              env=self.env, text=True, capture_output=True, timeout=25, **kwargs)

    def receipt(self):
        key = hashlib.sha256(b"fixture/session").hexdigest()[:24]
        return json.loads((self.root / "state/terminal-delight/skill-receipts" / key / "9.json").read_text())

    def test_inactive_is_an_exact_passthrough_including_empty_and_shell_metacharacters(self):
        self.configure(active=False)
        args = ["--model", "user model", "", "$(touch should-not-exist)", "quote'and\"newline\n"]
        result = self.run_provider(args=args)
        self.assertEqual(result.returncode, 27)
        self.assertEqual(json.loads((self.root / "argv.json").read_text()), args)
        self.assertFalse((self.work / "should-not-exist").exists())
        self.assertEqual(self.receipt()["status"], "not_attached")

    def test_claude_receives_native_directory_and_preserves_user_settings_and_hooks(self):
        supplied = {"model": "personal-model", "permissions": {"deny": ["Read(secret)"]},
                    "hooks": {"SessionStart": [{"hooks": [{"type": "command", "command": "echo user-hook"}]}]}}
        self.fake.write_text('''#!/usr/bin/env python3
import json,os,subprocess,sys
from pathlib import Path
args=sys.argv[1:]; settings=json.loads(Path(args[args.index('--settings')+1]).read_text())
directory=Path(args[args.index('--add-dir')+1])
assert (directory/'.claude/skills/td-extra/helper.txt').read_text()=='supporting file'
assert settings['model']=='personal-model'
assert settings['permissions']['deny']==['Read(secret)']
hooks=settings['hooks']['SessionStart']
assert hooks[0]['hooks'][0]['command']=='echo user-hook'
command=hooks[-1]['hooks'][0]['command']
ack=subprocess.run(command,shell=True,input=json.dumps({'session_id':'native-fixture'}),text=True,capture_output=True,check=True)
context=json.loads(ack.stdout)['hookSpecificOutput']['additionalContext']
assert 'td-extra' in context and 'SKILL.md' in context
assert not settings['hooks'].get('SessionEnd'), 'conversation end must not remove a live native directory'
subprocess.run(command,shell=True,input=json.dumps({'session_id':'next-conversation'}),text=True,capture_output=True,check=True)
assert (directory/'.claude/skills/td-extra/SKILL.md').is_file()
Path(os.environ['ARGV_LOG']).write_text(json.dumps(args))
''')
        result = self.run_provider(args=["--settings", json.dumps(supplied), "--model", "explicit-model", "my task"])
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        received = json.loads((self.root / "argv.json").read_text())
        self.assertEqual(received[-3:], ["--model", "explicit-model", "my task"])
        self.assertEqual(self.receipt()["status"], "attached")
        self.assertEqual(self.receipt()["agent_session"], "next-conversation")
        self.assertEqual(self.receipt()["available"][0]["name"], "td-extra")

    def test_explicit_claude_disable_wins_and_is_not_reenabled(self):
        settings = {"skillOverrides": {"td-extra": "off"}}
        args = ["--settings", json.dumps(settings), "task"]
        result = self.run_provider(args=args)
        self.assertEqual(result.returncode, 27)
        self.assertEqual(json.loads((self.root / "argv.json").read_text()), args)
        self.assertEqual(self.receipt()["status"], "not_attached")
        self.assertIn("disabled", self.receipt()["missing"][0]["reason"])

    def test_missing_startup_hook_confirmation_remains_unknown(self):
        result = self.run_provider(args=["--settings", '{"disableAllHooks":true}', "task"])
        self.assertEqual(result.returncode, 27)
        self.assertEqual(self.receipt()["status"], "unknown")
        self.assertIsNone(self.receipt()["available"])

    def test_remote_invocation_is_preserved_and_never_claims_attachment(self):
        args = ["--remote=unix:///someones/server", "task"]
        self.assertEqual(self.run_provider(args=args).returncode, 27)
        self.assertEqual(json.loads((self.root / "argv.json").read_text()), args)
        self.assertEqual(self.receipt()["status"], "unknown")

    def test_subcommand_after_config_is_preserved(self):
        args = ["-c", "features.hooks=false", "exec", "my task"]
        self.env["TD_SKILLS_REAL_CODEX"] = str(self.fake)
        self.assertEqual(self.run_provider("codex", args=args).returncode, 27)
        self.assertEqual(json.loads((self.root / "argv.json").read_text()), args)
        self.assertEqual(self.receipt()["status"], "unknown")

    def test_codex_named_profile_is_preserved_with_unknown_attachment(self):
        args = ["--profile", "personal", "my task"]
        self.env["TD_SKILLS_REAL_CODEX"] = str(self.fake)
        self.assertEqual(self.run_provider("codex", args=args).returncode, 27)
        self.assertEqual(json.loads((self.root / "argv.json").read_text()), args)
        self.assertEqual(self.receipt()["status"], "unknown")

    def test_unknown_or_invalid_settings_do_not_break_the_ordinary_command(self):
        self.config.write_text('{"active":"yes"}')
        self.assertEqual(self.run_provider(args=["my task"]).returncode, 27)
        self.assertEqual(self.receipt()["status"], "not_attached")
        self.assertIn("invalid TD skill switch", self.receipt()["reason"])

    def test_an_unwritable_status_path_cannot_prevent_the_agent_from_starting(self):
        blocked = self.root / "file-instead-of-state"
        blocked.write_text("a file")
        self.env["XDG_STATE_HOME"] = str(blocked)
        self.configure(active=False)
        result = self.run_provider(args=["task"])
        self.assertEqual(result.returncode, 27, result.stderr)
        self.assertEqual(json.loads((self.root / "argv.json").read_text()), ["task"])

    def test_existing_claude_skill_wins_without_replacing_personal_files(self):
        personal = self.skill(self.home / ".claude/skills/td-extra", "td-extra")
        (personal / "helper.txt").write_text("personal version")
        self.fake.write_text('''#!/usr/bin/env python3
import json,os,subprocess,sys
from pathlib import Path
args=sys.argv[1:]; settings=json.loads(Path(args[args.index('--settings')+1]).read_text())
directory=Path(args[args.index('--add-dir')+1])
assert not (directory/'.claude/skills/td-extra').exists()
hook=settings['hooks']['SessionStart'][-1]['hooks'][0]['command']
subprocess.run(hook,shell=True,input='{"session_id":"fixture"}',text=True,check=True)
''')
        self.assertEqual(self.run_provider(args=["task"]).returncode, 0)
        value = self.receipt()
        self.assertEqual(value["available"][0]["path"], str(personal))
        self.assertEqual(value["available"][0]["origin"], "existing agent skill")
        self.assertEqual((personal / "helper.txt").read_text(), "personal version")

    def test_cleanup_cannot_remove_a_source_or_another_directory(self):
        bridge.clean_run(self.extra)
        self.assertTrue((self.extra / "SKILL.md").is_file())

    def test_failed_source_reports_known_missing_instead_of_claiming_success(self):
        self.configure(sources=[dict(path=str(self.root / "missing"), name="gone", enabled=True)])
        result = self.run_provider(args=["task"])
        self.assertEqual(result.returncode, 27)
        self.assertEqual(self.receipt()["status"], "not_attached")
        self.assertEqual(len(self.receipt()["missing"]), 1)

    def test_local_import_keeps_supporting_files_and_versions_them(self):
        first = bridge.import_source(str(self.extra / "SKILL.md"))
        self.assertEqual(Path(first[0]["path"]), self.extra)
        self.assertEqual((Path(first[0]["path"]) / "helper.txt").read_text(), "supporting file")
        (self.extra / "helper.txt").write_text("changed helper")
        self.assertNotEqual(first[0]["revision"], bridge.import_source(str(self.extra))[0]["revision"])
        with self.assertRaises(ValueError):
            bridge.import_source(str(self.extra), "..")

    def test_one_source_accepts_file_urls_and_quoted_paths_with_spaces(self):
        folder = self.skill(self.root / "my skills", "with-spaces")
        for reference in ((folder / "SKILL.md").as_uri(), str(folder), '"' + str(folder) + '"'):
            with self.subTest(reference=reference):
                result = bridge.import_source(reference)
                self.assertEqual(result[0]["name"], "with-spaces")
                self.assertEqual(Path(result[0]["path"]), folder)
                self.assertTrue(result[0]["url"].startswith("file://"))

    def test_one_source_keeps_a_revision_with_its_repository(self):
        self.assertEqual(bridge.source_reference("https://example.invalid/repo.git#v1:skills"),
                         ("https://example.invalid/repo.git", "skills", "v1", None))
        for reference in ("abcdef123456", "refs/heads/main"):
            with self.assertRaisesRegex(ValueError, "revision needs a repository"):
                bridge.source_reference(reference)
        with self.assertRaisesRegex(ValueError, "one source"):
            bridge.source_reference("https://example.invalid/one\nhttps://example.invalid/two")
        with self.assertRaisesRegex(ValueError, "credentials"):
            bridge.source_reference("https://person:secret@example.invalid/repo")
        with self.assertRaisesRegex(ValueError, "on this computer"):
            bridge.source_reference("file://some-server/skills/SKILL.md")

    def test_pasted_github_skill_links_resolve_refs_and_keep_the_whole_package(self):
        repo = self.root / "github-repo"
        for name in ("one", "two", "three"):
            self.skill(repo / "skills" / name, "repo-" + name)
        (repo / "reference.txt").write_text("shared supporting resource")
        def git(*args):
            return subprocess.check_output(["git", "-C", str(repo), *args], stderr=subprocess.DEVNULL, text=True).strip()
        git("init", "--quiet", "--initial-branch=main")
        git("add", ".")
        git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "fixture")
        git("branch", "feature/loadout")
        git("tag", "v1")
        commit = git("rev-parse", "HEAD")
        origin = "https://github.com/td-fixture/skills"
        env = dict(self.env, GIT_CONFIG_COUNT="1", GIT_CONFIG_KEY_0="url." + repo.as_uri() + ".insteadOf", GIT_CONFIG_VALUE_0=origin)
        cases = [
            (origin, 3),
            (origin + "#v1:skills", 3),
            (origin + "/tree/feature/loadout/skills", 3),
            (origin + "/blob/" + commit + "/skills/one/SKILL.md", 1),
            ("https://raw.githubusercontent.com/td-fixture/skills/main/skills/one/SKILL.md", 1),
        ]
        for reference, count in cases:
            with self.subTest(reference=reference):
                result = subprocess.run(["python3", str(self.package / "bridge.py"), "import", reference],
                                        env=env, cwd=self.work, text=True, capture_output=True, timeout=25)
                self.assertEqual(result.returncode, 0, result.stderr)
                imported = json.loads(result.stdout)
                self.assertEqual(len(imported), count)
                for source in imported:
                    self.assertEqual(source["revision"], commit)
                    self.assertEqual((Path(source["path"]).parents[1] / "reference.txt").read_text(), "shared supporting resource")
                    self.assertIn("/tree/" + commit + "/skills/", source["url"])
        escaped = origin + "/tree/main/../../../escape"
        result = subprocess.run(["python3", str(self.package / "bridge.py"), "import", escaped],
                                env=env, cwd=self.work, text=True, capture_output=True, timeout=25)
        self.assertNotEqual(result.returncode, 0)

    def test_a_local_skill_receipt_versions_the_files_present_at_startup(self):
        imported = bridge.import_source(str(self.extra))[0]
        (self.extra / "helper.txt").write_text("changed since import")
        good, missing = bridge.selected(dict(briefing=False, factory=False, sources=[imported]))
        self.assertFalse(missing)
        self.assertNotEqual(good[0]["revision"], imported["revision"])

    def test_personal_settings_file_is_never_modified(self):
        personal = self.home / ".claude/settings.json"
        personal.parent.mkdir()
        original = '{"skillOverrides":{"td-extra":"off"},"model":"personal"}'
        personal.write_text(original)
        self.run_provider(args=["task"])
        self.assertEqual(personal.read_text(), original)
        self.assertEqual(self.receipt()["status"], "not_attached")

    def test_real_repository_import_pins_commit_and_preserves_sibling_resources(self):
        repo = self.root / "repository"
        self.skill(repo / "skills/one", "repo-one")
        (repo / "reference.txt").write_text("sibling reference")
        def git(*args):
            return subprocess.check_output(["git", "-C", str(repo), *args], stderr=subprocess.DEVNULL, text=True).strip()
        git("init", "--quiet")
        git("add", ".")
        git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "fixture")
        commit = git("rev-parse", "HEAD")
        env = dict(self.env, GIT_CONFIG_COUNT="1", GIT_CONFIG_KEY_0="url." + repo.as_uri() + ".insteadOf", GIT_CONFIG_VALUE_0="https://td.fixture/repo")
        result = subprocess.run(["python3", str(self.package / "bridge.py"), "import", "https://td.fixture/repo", "skills", commit],
                                env=env, cwd=self.work, text=True, capture_output=True, timeout=25)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        source = json.loads(result.stdout)[0]
        self.assertEqual(source["revision"], commit)
        self.assertEqual((Path(source["path"]).parents[1] / "reference.txt").read_text(), "sibling reference")
        original_path = Path(source["path"])
        (repo / "reference.txt").write_text("updated reference")
        git("add", ".")
        git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "update")
        update = subprocess.run(["python3", str(self.package / "bridge.py"), "import", "https://td.fixture/repo", "skills", "HEAD"], env=env, cwd=self.work, text=True, capture_output=True, timeout=25)
        self.assertEqual(update.returncode, 0, update.stderr)
        refreshed = json.loads(update.stdout)[0]
        self.assertNotEqual(refreshed["path"], source["path"])
        self.assertEqual((original_path.parents[1] / "reference.txt").read_text(), "sibling reference")
        self.assertEqual((Path(refreshed["path"]).parents[1] / "reference.txt").read_text(), "updated reference")

    @unittest.skipUnless(os.environ.get("TD_NATIVE_SKILL_TESTS") == "1" and shutil.which("codex"), "opt-in native Codex discovery")
    def test_native_codex_catalog_keeps_personal_skill_and_adds_td_skill(self):
        self.skill(self.work / ".agents/skills/td-person", "td-person")
        self.env.update(TD_SKILLS_CATALOG_PROBE="1", TD_SKILLS_REAL_CODEX=shutil.which("codex"))
        result = self.run_provider("codex", args=["-c", "features.hooks=false"])
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        value = json.loads(result.stdout)
        self.assertEqual(value["status"], "attached")
        self.assertEqual(value["method"], "Codex skills/list")
        self.assertEqual([s["name"] for s in value["available"]], ["td-extra"])
        self.assertIn("td-person", value["native_catalog"])
        self.assertIn("td-extra", value["native_catalog"])

    @unittest.skipUnless(os.environ.get("TD_NATIVE_SKILL_TESTS") == "1" and shutil.which("codex"), "opt-in native Codex exclusion")
    def test_native_codex_name_exclusion_wins_over_the_added_root(self):
        self.env.update(TD_SKILLS_CATALOG_PROBE="1", TD_SKILLS_REAL_CODEX=shutil.which("codex"))
        result = self.run_provider("codex", args=["-c", 'skills.config=[{name="td-extra",enabled=false}]', "-c", "features.hooks=false"])
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        value = json.loads(result.stdout)
        self.assertEqual(value["status"], "not_attached")
        self.assertEqual(value["available"], [])
        self.assertEqual(value["missing"][0]["name"], "td-extra")

    @unittest.skipUnless(os.environ.get("TD_NATIVE_SKILL_TESTS") == "1" and shutil.which("codex"), "opt-in native Codex terminal")
    def test_native_codex_terminal_connects_and_shows_both_skills(self):
        self.skill(self.work / ".agents/skills/td-person", "td-person")
        profile = '''model = "fixture"
model_provider = "fixture"
[features]
daemon_auto_start = false
hooks = false
[model_providers.fixture]
name = "Local fixture"
base_url = "http://127.0.0.1:1/v1"
requires_openai_auth = false
'''
        (self.home / ".codex/config.toml").write_text(profile)
        self.env.update(TD_SKILLS_REAL_CODEX=shutil.which("codex"), TERM="xterm-256color", CODEX_HOME=str(self.home / ".codex"))
        pid, master = pty.fork()
        if pid == 0:
            fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
            os.chdir(self.work)
            os.execve("/usr/bin/python3", ["python3", str(self.package / "bin/codex"), "--no-alt-screen"], self.env)
        output = bytearray()
        try:
            deadline, sent, entered, listed = time.monotonic() + 15, None, False, False
            while time.monotonic() < deadline:
                if select.select([master], [], [], .1)[0]:
                    try:
                        chunk = os.read(master, 65536)
                    except OSError:
                        break
                    if not chunk:
                        break
                    output.extend(chunk)
                    if b'\x1b[6n' in chunk:
                        os.write(master, b'\x1b[1;1R')
                if sent is None and b'fixture default' in output and time.monotonic() > deadline - 12:
                    self.assertEqual(self.receipt()["status"], "attached", self.receipt())
                    os.write(master, b'/skills')
                    sent = time.monotonic()
                if sent is not None and not entered and time.monotonic() - sent > .5:
                    os.write(master, b'\r')
                    entered = True
                if not listed and b'List skills' in output:
                    os.write(master, b'\r')
                    listed = True
                if b'td-extra' in output and b'td-person' in output:
                    break
            self.assertTrue(b'td-extra' in output, output.decode(errors="replace")[-3000:])
            self.assertTrue(b'td-person' in output, output.decode(errors="replace")[-3000:])
            self.assertEqual(self.receipt()["status"], "attached")
        finally:
            children = Path(f'/proc/{pid}/task/{pid}/children')
            children = children.read_text().split() if children.exists() else []
            try:
                os.kill(pid, 15)
            except ProcessLookupError:
                pass
            os.waitpid(pid, 0)
            os.close(master)
            self.assertFalse([p for p in children if Path(f'/proc/{p}').exists()], "the private native children must exit with their invocation")

    @unittest.skipUnless(os.environ.get("TD_NATIVE_SKILL_TESTS") == "1" and shutil.which("claude"), "opt-in native Claude discovery")
    def test_native_claude_catalog_and_model_context_receive_both_skill_sources(self):
        self.skill(self.work / ".claude/skills/td-person", "td-person")
        requests = []
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_args):
                pass

            def do_POST(self):
                request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                if "count_tokens" in self.path:
                    self.send_response(200)
                    self.send_header("Content-Type", "application/json")
                    self.end_headers()
                    self.wfile.write(b'{"input_tokens":100}')
                    return
                requests.append(request)
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.end_headers()
                message = {"id": "msg_fixture", "type": "message", "role": "assistant", "content": [], "model": request.get("model", "fixture"), "stop_reason": None, "stop_sequence": None, "usage": {"input_tokens": 10, "output_tokens": 0}}
                events = [dict(type="message_start", message=message),
                          dict(type="content_block_start", index=0, content_block={"type": "text", "text": ""}),
                          dict(type="content_block_delta", index=0, delta={"type": "text_delta", "text": "Fixture complete."}),
                          dict(type="content_block_stop", index=0),
                          dict(type="message_delta", delta={"stop_reason": "end_turn", "stop_sequence": None}, usage={"output_tokens": 3}),
                          dict(type="message_stop")]
                for event in events:
                    self.wfile.write(("event: " + event["type"] + "\ndata: " + json.dumps(event) + "\n\n").encode())
                self.wfile.flush()
        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        self.env.update(TD_SKILLS_REAL_CLAUDE=shutil.which("claude"), ANTHROPIC_API_KEY="fixture-only",
                        ANTHROPIC_BASE_URL=f"http://127.0.0.1:{server.server_port}",
                        CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC="1", DISABLE_AUTOUPDATER="1")
        result = self.run_provider(args=["--setting-sources", "project", "--print", "--verbose", "--output-format", "stream-json", "--model", "claude-sonnet-4-5", "catalog fixture"])
        self.assertEqual(result.returncode, 0, result.stderr[-2000:])
        events = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
        initial = next(e for e in events if e.get("subtype") == "init")
        self.assertIn("td-extra", initial["skills"])
        self.assertIn("td-person", initial["skills"])
        self.assertTrue(requests)
        self.assertIn("td-extra", json.dumps(requests[-1]))
        self.assertIn("td-person", json.dumps(requests[-1]))
        self.assertEqual(self.receipt()["status"], "attached")


class Frames(unittest.TestCase):
    def setUp(self):
        self.client, self.peer = socket.socketpair()
        self.addCleanup(self.client.close)
        self.addCleanup(self.peer.close)
        self.client.settimeout(1)
        self.native = bridge.NativeSocket.__new__(bridge.NativeSocket)
        self.native.sock = self.client

    def test_fragmented_native_json_and_ping_are_handled(self):
        self.peer.sendall(b'\x01\x05{"ok"' + b'\x89\x01x' + b'\x80\x06:true}')
        self.assertEqual(self.native.receive(), {"ok": True})
        frame = self.peer.recv(128)
        self.assertEqual(frame[0], 138)
        self.assertTrue(frame[1] & 128)

    def test_oversized_or_masked_native_frames_are_rejected_before_allocation(self):
        self.peer.sendall(bytes([129, 127]) + struct.pack("!Q", bridge.MAX_MESSAGE + 1))
        with self.assertRaises(ValueError):
            self.native.receive()


if __name__ == "__main__":
    unittest.main()
