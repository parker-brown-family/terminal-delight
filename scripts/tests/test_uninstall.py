#!/usr/bin/env python3
"""Legacy cleanup regressions. Every mutable path is confined to a temp fixture.

HOME and every XDG path are fixtures; desktop tools are stubs. No TD launch or
process signal is used, including when testing the historical installers.
TD_TEST_BASELINE=<git commit> exercises the same oracles on historical scripts.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[2]


class Cleanup(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="td-uninstall-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.repo = self.root / "repo"
        shutil.copytree(REPO / "scripts", self.repo / "scripts")
        (self.repo / "docsite/pages").mkdir(parents=True)
        shutil.copyfile(REPO / "docsite/pages/keys.html", self.repo / "docsite/pages/keys.html")
        shutil.copytree(REPO / "packaging", self.repo / "packaging")
        shutil.copytree(REPO / "plugins", self.repo / "plugins")
        (self.repo / "app/src/uninstall").mkdir(parents=True)
        shutil.copyfile(REPO / "app/src/uninstall/known_hashes.txt", self.repo / "app/src/uninstall/known_hashes.txt")
        if baseline := os.environ.get("TD_TEST_BASELINE"):
            for name in ("install-agent-hooks.sh", "install-recovery-hook.sh", "install-keys-hotkey.sh", "install-hotkey.sh", "install-desktop.sh", "install-plugins.sh", "lib/uninstall.sh"):
                (self.repo / "scripts" / name).write_bytes(subprocess.check_output(
                    ["git", "show", f"{baseline}:scripts/{name}"], cwd=REPO))
            generator = subprocess.run(["git", "show", f"{baseline}:scripts/known-hashes.sh"], cwd=REPO, capture_output=True)
            if generator.returncode == 0:
                (self.repo / "scripts/known-hashes.sh").write_bytes(generator.stdout)
            else:
                (self.repo / "scripts/known-hashes.sh").unlink()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.data = self.root / "data"
        self.data.mkdir()
        self.settings = self.root / "settings.json"
        self.bindings = self.root / "bindings.lua"
        self.stub = self.root / "stubs"
        self.stub.mkdir()
        self.home = self.root / "home"
        self.home.mkdir()
        self.state = self.root / "state/terminal-delight"
        self.env = dict(os.environ, DEST=str(self.bin), DATA=str(self.data),
                        HOME=str(self.home), XDG_DATA_HOME=str(self.home / ".local/share"),
                        XDG_STATE_HOME=str(self.root / "state"), XDG_CACHE_HOME=str(self.root / "cache"),
                        XDG_RUNTIME_DIR=str(self.root / "runtime"),
                        CLAUDE_SETTINGS=str(self.settings), HYPR_BINDINGS=str(self.bindings),
                        XDG_CONFIG_HOME=str(self.root / "config"), XDG_CURRENT_DESKTOP="GNOME",
                        PATH=f"{self.stub}:/usr/bin:/bin", TD_TEST_ROOT=str(self.root))
        self.executable("hyprctl", "#!/bin/sh\nexit 0\n")
        self.executable("terminal-delight", "#!/bin/sh\nexit 97\n")
        for tool in ("update-desktop-database", "gtk-update-icon-cache"):
            self.executable(tool, '#!/bin/sh\nprintf "%s %s\\n" "$0" "$*" >> "$TD_TEST_ROOT/cache-calls"\n')
        self.executable("rsvg-convert", '''#!/usr/bin/python3
import pathlib, sys
args = sys.argv[1:]
pathlib.Path(args[args.index('-o') + 1]).write_bytes(b'fixture png')
''')
        self.executable("gsettings", '''#!/usr/bin/python3
import json, os, pathlib, sys
root = pathlib.Path(os.environ['TD_TEST_ROOT'])
path = root / 'gsettings.json'
d = json.loads(path.read_text())
op, schema, key, *values = sys.argv[1:]
k = schema + '/' + key
if op == 'get': print(d[k])
elif op == 'set':
    if os.environ.get('TD_TEST_REQUIRE_INTENT') and key == 'terminal':
        intent = pathlib.Path(os.environ['XDG_CONFIG_HOME']) / 'terminal-delight/hotkey-terminal-key.intent'
        if not intent.is_file(): sys.exit(91)
    if os.environ.get('TD_TEST_FAIL_SET') == key: sys.exit(1)
    value = values[0]
    d[k] = value if value.startswith(('[', '@', "'")) else repr(value)
elif op == 'reset': d[k] = "''"
else: sys.exit(98)
if op != 'get': path.write_text(json.dumps(d))
''')

    def executable(self, name, text):
        p = self.stub / name
        p.write_text(text)
        p.chmod(0o755)

    def run_script(self, name, *args):
        # Fail before a subprocess if a future edit accidentally drops an
        # override or redirects a fixture onto the operator's installation.
        for key in ("HOME", "DEST", "DATA", "CLAUDE_SETTINGS", "HYPR_BINDINGS", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME", "XDG_RUNTIME_DIR", "TD_TEST_ROOT"):
            self.assertTrue(Path(self.env[key]).resolve().is_relative_to(self.root), key)
        self.assertEqual(self.env["PATH"].split(":")[0], str(self.stub))
        return subprocess.run(["bash", str(self.repo / "scripts" / name), *args],
                              cwd=self.root, env=self.env, capture_output=True, text=True, timeout=10)

    def assert_no_loose_backups(self):
        self.assertEqual(list(self.root.rglob("*.td-backup.*")), [])
        self.assertEqual(list(self.root.rglob("*.td-snapshot.*")), [])
        self.assertEqual(list(self.root.rglob("*.td-edit.*")), [])

    def receipt(self, component):
        return self.state / "receipts" / f"{component}.receipt"

    def hook(self, helper):
        target = self.bin / helper
        shutil.copyfile(self.repo / "scripts" / helper, target)
        return {"type": "command", "command": str(target)}

    def test_mixed_nested_hooks_preserve_siblings_and_metadata(self):
        for script, helper in [("install-agent-hooks.sh", "td-agent-hooks"), ("install-recovery-hook.sh", "td-agent-ledger")]:
            with self.subTest(script=script):
                ours = self.hook(helper)
                other = {"type": "command", "command": "unrelated-tool", "timeout": 9}
                group = {"matcher": "*", "timeout": 42, "extra": True, "hooks": [ours, other]}
                events = ["Stop", "UserPromptSubmit", "PreToolUse", "PostToolUse", "Notification", "SessionStart", "SessionEnd"]
                before = {"theme": "untouched", "hooks": {e: [group] for e in events}}
                self.settings.write_text(json.dumps(before))
                p = self.run_script(script, "--uninstall")
                self.assertEqual(p.returncode, 0, p.stderr)
                group["hooks"] = [other]
                self.assertEqual(json.loads(self.settings.read_text()), before)
                self.assertFalse((self.bin / helper).exists())

    def test_invalid_settings_keep_helper_and_report_failure(self):
        for script, helper in [("install-agent-hooks.sh", "td-agent-hooks"), ("install-recovery-hook.sh", "td-agent-ledger")]:
            for content in ('{"td-agent-hooks": BROKEN', '[]', '{}\n{}', '{"hooks":null}', '{"hooks":{"Stop":{}}}'):
                with self.subTest(script=script, content=content):
                    self.hook(helper)
                    self.settings.write_text(content)
                    p = self.run_script(script, "--uninstall")
                    self.assertNotEqual(p.returncode, 0)
                    self.assertEqual(self.settings.read_text(), content)
                    self.assertTrue((self.bin / helper).exists())
                    self.assertNotIn("unhooked and removed", p.stdout)
                    self.assert_no_loose_backups()

    def test_unrecognized_reference_blocks_helper_removal(self):
        ours = self.hook("td-agent-hooks")
        for command in ["echo td-agent-hooks", ours["command"] + " --custom", "/elsewhere/td-agent-hooks"]:
            content = json.dumps({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": command}]}]}})
            self.settings.write_text(content)
            self.assertNotEqual(self.run_script("install-agent-hooks.sh", "--uninstall").returncode, 0)
            self.assertEqual(self.settings.read_text(), content)
            self.assertTrue((self.bin / "td-agent-hooks").exists())
            self.assert_no_loose_backups()

    def test_changed_helper_and_symlink_settings_retained(self):
        helper = self.bin / "td-agent-hooks"
        helper.write_text("user replacement")
        self.settings.write_text("{}")
        self.assertNotEqual(self.run_script("install-agent-hooks.sh", "--uninstall").returncode, 0)
        target = self.root / "real.json"
        self.settings.rename(target)
        self.settings.symlink_to(target)
        self.assertNotEqual(self.run_script("install-agent-hooks.sh", "--uninstall").returncode, 0)
        self.assertEqual(helper.read_text(), "user replacement")
        self.assertEqual(target.read_text(), "{}")

    def test_missing_hooks_remains_byte_identical_and_repeated_cleanup_works(self):
        self.hook("td-agent-hooks")
        self.settings.write_text('{ "theme": "keep" }\n')
        for _ in range(2):
            self.assertEqual(self.run_script("install-agent-hooks.sh", "--uninstall").returncode, 0)
        self.assertEqual(self.settings.read_text(), '{ "theme": "keep" }\n')

    def install_keys(self):
        self.bindings.write_text("-- before\n")
        p = self.run_script("install-keys-hotkey.sh")
        self.assertEqual(p.returncode, 0, p.stderr)
        return self.bindings.read_text()

    def test_keys_exact_block_removal_keeps_tail(self):
        self.install_keys()
        with self.bindings.open("a") as f: f.write("-- user after\n")
        self.assertEqual(self.run_script("install-keys-hotkey.sh", "--uninstall").returncode, 0)
        self.assertEqual(self.bindings.read_text(), "-- before\n\n-- user after\n")
        self.assertFalse((self.bin / "td-keybindings").exists())

    def test_keys_keep_unterminated_user_tail_bytes(self):
        self.install_keys()
        with self.bindings.open("a") as f: f.write("-- user tail without newline")
        self.assertEqual(self.run_script("install-keys-hotkey.sh", "--uninstall").returncode, 0)
        self.assertEqual(self.bindings.read_bytes(), b"-- before\n\n-- user tail without newline")

    def test_keys_damaged_duplicate_reversed_edited_blocks_keep_everything(self):
        content = self.install_keys()
        shut = "-- <<< td-keybindings hotkey <<<"
        start = "-- >>> td-keybindings hotkey >>>"
        cases = [content.replace(shut, "-- missing"), content + content,
                 shut + "\n" + content.replace(shut, ""), content.replace("SUPER + ALT + T", "SUPER + ALT + U"),
                 content + "o.bind('X', 'custom', 'td-keybindings')\n"]
        for value in cases:
            with self.subTest(value=value):
                self.bindings.write_text(value + "-- user tail\n")
                before = self.bindings.read_bytes()
                p = self.run_script("install-keys-hotkey.sh", "--uninstall")
                self.assertNotEqual(p.returncode, 0)
                self.assertEqual(self.bindings.read_bytes(), before)
                self.assertTrue((self.bin / "td-keybindings").exists())
                self.assertTrue((self.data / "keys.html").exists())
                self.assert_no_loose_backups()

    MEDIA = "org.gnome.settings-daemon.plugins.media-keys"
    KEYPATH = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/terminal-delight/"

    def gnome(self):
        self.gfile = self.root / "gsettings.json"
        self.gfile.write_text(json.dumps({self.MEDIA + "/terminal": "['<Primary><Alt>t']",
                                        self.MEDIA + "/custom-keybindings": "['/unrelated/']"}))
        p = self.run_script("install-hotkey.sh")
        self.assertEqual(p.returncode, 0, p.stderr)

    def test_gnome_keeps_later_terminal_choice(self):
        self.gnome()
        d = json.loads(self.gfile.read_text())
        d[self.MEDIA + "/terminal"] = "['<Super>Return']"
        self.gfile.write_text(json.dumps(d))
        self.assertEqual(self.run_script("install-hotkey.sh", "--uninstall").returncode, 0)
        d = json.loads(self.gfile.read_text())
        self.assertEqual(d[self.MEDIA + "/terminal"], "['<Super>Return']")
        self.assertEqual(d[self.MEDIA + "/custom-keybindings"], "['/unrelated/']")
        self.assertTrue((self.root / "config/terminal-delight/hotkey-terminal-key.bak").exists())

    def test_gnome_changed_shortcut_or_missing_receipt_blocks(self):
        self.gnome()
        d = json.loads(self.gfile.read_text())
        d[self.MEDIA + ".custom-keybinding:" + self.KEYPATH + "/command"] = "'other-terminal'"
        self.gfile.write_text(json.dumps(d))
        before = self.gfile.read_bytes()
        self.assertNotEqual(self.run_script("install-hotkey.sh", "--uninstall").returncode, 0)
        self.assertEqual(self.gfile.read_bytes(), before)

    def test_gnome_failed_write_keeps_evidence(self):
        self.gnome()
        self.env["TD_TEST_FAIL_SET"] = "custom-keybindings"
        before = self.gfile.read_bytes()
        self.assertNotEqual(self.run_script("install-hotkey.sh", "--uninstall").returncode, 0)
        self.assertEqual(self.gfile.read_bytes(), before)
        self.assertTrue((self.root / "config/terminal-delight/hotkey-terminal-key.owned").exists())

    def test_gnome_restores_only_recorded_binding(self):
        self.gnome()
        self.assertEqual(self.run_script("install-hotkey.sh", "--uninstall").returncode, 0)
        d = json.loads(self.gfile.read_text())
        self.assertEqual(d[self.MEDIA + "/terminal"], "['<Primary><Alt>t']")
        self.assertEqual(d[self.MEDIA + "/custom-keybindings"], "['/unrelated/']")

    def test_gnome_missing_receipt_preserves_all_settings(self):
        self.gnome()
        receipt = self.root / "config/terminal-delight/hotkey-terminal-key.owned"
        if receipt.exists(): receipt.unlink()
        before = self.gfile.read_bytes()
        self.assertNotEqual(self.run_script("install-hotkey.sh", "--uninstall").returncode, 0)
        self.assertEqual(self.gfile.read_bytes(), before)

    def test_gnome_missing_backup_never_guesses_default(self):
        self.gnome()
        (self.root / "config/terminal-delight/hotkey-terminal-key.bak").unlink()
        self.assertEqual(self.run_script("install-hotkey.sh", "--uninstall").returncode, 0)
        self.assertEqual(json.loads(self.gfile.read_text())[self.MEDIA + "/terminal"], "@as []")

    def test_committed_edit_keeps_one_private_state_backup(self):
        ours = self.hook("td-agent-hooks")
        content = json.dumps({"env": {"SECRET": "fixture only"}, "hooks": {"Stop": [{"hooks": [ours]}]}})
        self.settings.write_text(content)
        self.settings.chmod(0o644)
        p = self.run_script("install-agent-hooks.sh", "--uninstall")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assert_no_loose_backups()
        backups = list((self.state / "backups").glob("*"))
        self.assertEqual(len(backups), 1)
        self.assertEqual(backups[0].read_text(), content)
        self.assertEqual(backups[0].stat().st_mode & 0o777, 0o600)
        self.assertIn(str(backups[0]), p.stdout)
        self.assertEqual(self.settings.stat().st_mode & 0o777, 0o644)

    def test_install_backups_are_private_and_failed_install_leaves_no_snapshot(self):
        for script in ("install-agent-hooks.sh", "install-recovery-hook.sh"):
            with self.subTest(script=script):
                shutil.rmtree(self.state, ignore_errors=True)
                self.settings.write_text('{"env":{"SECRET":"fixture"}}\n')
                before = self.settings.read_bytes()
                p = self.run_script(script)
                self.assertEqual(p.returncode, 0, p.stderr)
                self.assert_no_loose_backups()
                backups = list((self.state / "backups").glob("*"))
                self.assertEqual(len(backups), 1)
                self.assertEqual(backups[0].read_bytes(), before)
                self.assertEqual(backups[0].stat().st_mode & 0o777, 0o600)
                self.settings.write_text("broken settings")
                self.assertNotEqual(self.run_script(script).returncode, 0)
                self.assert_no_loose_backups()
                self.assertEqual(list((self.state / "backups").glob("*")), backups)

    def test_component_receipts_record_files_and_integrations_then_disappear(self):
        cases = [("agent-hooks", "install-agent-hooks.sh", "hook"),
                 ("recovery-hook", "install-recovery-hook.sh", "hook"),
                 ("keys-hotkey", "install-keys-hotkey.sh", "block"),
                 ("plugins", "install-plugins.sh", None),
                 ("desktop", "install-desktop.sh", None)]
        for component, script, integration in cases:
            with self.subTest(component=component):
                p = self.run_script(script)
                self.assertEqual(p.returncode, 0, p.stderr)
                receipt = self.receipt(component)
                self.assertTrue(receipt.is_file())
                self.assertEqual(receipt.stat().st_mode & 0o777, 0o600)
                rows = [line.split("\t") for line in receipt.read_text().splitlines() if not line.startswith("#")]
                files = [row for row in rows if row[0] == "file"]
                self.assertTrue(files)
                for _, digest, path in files:
                    self.assertTrue(Path(path).is_relative_to(self.root))
                    self.assertEqual(hashlib.sha256(Path(path).read_bytes()).hexdigest(), digest)
                if integration:
                    self.assertTrue(any(row[0] == integration for row in rows))
                self.assertEqual(list(receipt.parent.glob(f".{component}.*")), [])
                p = self.run_script(script, "--uninstall")
                self.assertEqual(p.returncode, 0, p.stderr)
                self.assertFalse(receipt.exists())
                self.assertTrue(all(not Path(row[2]).exists() for row in files))

    def test_receipt_hash_is_bound_to_the_installed_path(self):
        target = self.bin / "td-agent-hooks"
        target.write_text("previous custom packaged version")
        digest = hashlib.sha256(target.read_bytes()).hexdigest()
        receipt = self.receipt("agent-hooks")
        receipt.parent.mkdir(parents=True)
        receipt.write_text(f"file\t{digest}\t{self.bin / 'elsewhere'}\n")
        self.assertNotEqual(self.run_script("install-agent-hooks.sh", "--uninstall").returncode, 0)
        self.assertTrue(target.exists())
        receipt.write_text(f"file\t{digest}\t{target}\n")
        p = self.run_script("install-agent-hooks.sh", "--uninstall")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertFalse(target.exists())
        self.assertFalse(receipt.exists())

    def test_old_committed_helper_is_owned_by_history(self):
        path = "scripts/td-agent-hooks"
        current = (REPO / path).read_bytes()
        commits = subprocess.check_output(["git", "log", "--all", "--format=%H", "--", path], cwd=REPO).decode().splitlines()
        old = None
        for commit in reversed(commits):
            result = subprocess.run(["git", "show", f"{commit}:{path}"], cwd=REPO, capture_output=True)
            if result.returncode == 0 and result.stdout != current:
                old = result.stdout
                break
        self.assertIsNotNone(old, "history fixture must differ from today's helper")
        target = self.bin / "td-agent-hooks"
        target.write_bytes(old)
        p = self.run_script("install-agent-hooks.sh", "--uninstall")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertFalse(target.exists())

    def test_refused_legacy_keys_install_writes_nothing(self):
        for block in ("-- >>> td-send hotkey >>>\nuser tail\n", "-- <<< td-send hotkey <<<\n"):
            with self.subTest(block=block):
                self.bindings.write_text(block)
                before = {p: p.read_bytes() for p in self.root.rglob("*") if p.is_file()}
                p = self.run_script("install-keys-hotkey.sh")
                self.assertNotEqual(p.returncode, 0)
                after = {p: p.read_bytes() for p in self.root.rglob("*") if p.is_file()}
                self.assertEqual(before.keys(), after.keys())
                self.assertTrue(all(after[path] == value for path, value in before.items()))
                self.assertIn("-- <<< td-send hotkey <<<", p.stderr)
                self.assertIn(str(self.bindings), p.stderr)

    def test_desktop_uninstall_retains_changed_icon_and_refreshes_caches(self):
        self.assertEqual(self.run_script("install-desktop.sh").returncode, 0)
        data = Path(self.env["XDG_DATA_HOME"])
        png = data / "icons/hicolor/48x48/apps/terminal-delight.png"
        png.write_text("user icon")
        other = data / "applications/other.desktop"
        other.write_text("unrelated")
        (self.root / "cache-calls").write_text("")
        p = self.run_script("install-desktop.sh", "--uninstall")
        self.assertNotEqual(p.returncode, 0)
        self.assertEqual(png.read_text(), "user icon")
        self.assertEqual(other.read_text(), "unrelated")
        self.assertFalse((data / "applications/terminal-delight.desktop").exists())
        self.assertTrue(self.receipt("desktop").exists())
        calls = (self.root / "cache-calls").read_text()
        self.assertIn(str(data / "applications"), calls)
        self.assertIn(str(data / "icons/hicolor"), calls)

    def test_plugin_uninstall_is_not_a_destination(self):
        plugin = self.bin / "leanctx-mcp"
        shutil.copyfile(self.repo / "plugins/leanctx-mcp/leanctx-mcp", plugin)
        other = self.bin / "other-tool"
        other.write_text("keep")
        p = self.run_script("install-plugins.sh", "--uninstall")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertFalse(plugin.exists())
        self.assertEqual(other.read_text(), "keep")
        self.assertFalse((self.root / "--uninstall").exists())

    def test_unknown_installer_flags_are_rejected_without_writes(self):
        for script in ("install-desktop.sh", "install-plugins.sh"):
            with self.subTest(script=script):
                before = sorted(str(p) for p in self.root.rglob("*"))
                p = self.run_script(script, "--unknown")
                self.assertEqual(p.returncode, 2, p.stderr)
                self.assertEqual(sorted(str(p) for p in self.root.rglob("*")), before)

    def test_gnome_intent_precedes_first_write_and_survives_failure(self):
        self.gfile = self.root / "gsettings.json"
        self.gfile.write_text(json.dumps({self.MEDIA + "/terminal": "['<Primary><Alt>t']",
                                        self.MEDIA + "/custom-keybindings": "['/unrelated/']"}))
        self.env["TD_TEST_REQUIRE_INTENT"] = "1"
        self.env["TD_TEST_FAIL_SET"] = "custom-keybindings"
        p = self.run_script("install-hotkey.sh")
        self.assertNotEqual(p.returncode, 0)
        intent = self.root / "config/terminal-delight/hotkey-terminal-key.intent"
        self.assertTrue(intent.is_file())
        self.assertIn(str(self.stub / "terminal-delight"), intent.read_text())
        self.assertEqual(intent.stat().st_mode & 0o777, 0o600)
        self.assertEqual(json.loads(self.gfile.read_text())[self.MEDIA + "/terminal"], "@as []")
        self.assertFalse(intent.with_suffix(".owned").exists())
        before = self.gfile.read_bytes()
        p = self.run_script("install-hotkey.sh", "--uninstall")
        self.assertNotEqual(p.returncode, 0)
        self.assertEqual(self.gfile.read_bytes(), before)
        self.assertIn(str(intent), p.stderr)

    def test_gnome_missing_receipt_prints_exact_review_commands(self):
        self.gnome()
        (self.root / "config/terminal-delight/hotkey-terminal-key.owned").unlink()
        p = self.run_script("install-hotkey.sh", "--uninstall")
        self.assertNotEqual(p.returncode, 0)
        schema = self.MEDIA + ".custom-keybinding:" + self.KEYPATH
        for key in ("name", "command", "binding"):
            self.assertIn(f"gsettings get {schema} {key}", p.stderr)
            self.assertIn(f"gsettings reset {schema} {key}", p.stderr)
        self.assertIn(f"gsettings set {self.MEDIA} custom-keybindings", p.stderr)
        commands = "\n".join(line for line in p.stderr.splitlines() if line.startswith(("gsettings ", "td_keys=", "td_path=", "cat -- ")))
        # The documented manual commands are executable, preserving the other
        # shortcut and restoring only the recorded built-in binding.
        manual = subprocess.run(["bash", "-e", "-c", commands], env=self.env, cwd=self.root, capture_output=True, text=True)
        self.assertEqual(manual.returncode, 0, manual.stderr)
        d = json.loads(self.gfile.read_text())
        self.assertEqual(d[self.MEDIA + "/custom-keybindings"], "['/unrelated/']")
        self.assertEqual(d[self.MEDIA + "/terminal"], "['<Primary><Alt>t']")

    def test_known_hash_check_detects_changed_table_and_uncommitted_source(self):
        # Read-only access to the real history, with generated files confined
        # to the fixture. No git write command is invoked by the generator.
        gitdir = subprocess.check_output(["git", "rev-parse", "--absolute-git-dir"], cwd=REPO).decode().strip()
        (self.repo / ".git").write_text(f"gitdir: {gitdir}\n")
        p = self.run_script("known-hashes.sh", "--check")
        self.assertEqual(p.returncode, 0, p.stderr)
        table = self.repo / "app/src/uninstall/known_hashes.txt"
        expected = table.read_bytes()
        table.write_bytes(b"missing historical hashes\n")
        self.assertNotEqual(self.run_script("known-hashes.sh", "--check").returncode, 0)
        p = self.run_script("known-hashes.sh")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(table.read_bytes(), expected)
        (self.repo / "scripts/td-agent-hooks").write_text("uncommitted replacement")
        self.assertNotEqual(self.run_script("known-hashes.sh", "--check").returncode, 0)

    def test_failed_replace_discards_snapshot_and_uncommitted_backup(self):
        ours = self.hook("td-agent-hooks")
        self.settings.write_text(json.dumps({"hooks": {"Stop": [{"hooks": [ours]}]}}))
        before = self.settings.read_bytes()
        self.executable("mv", "#!/bin/sh\nexit 1\n")
        p = self.run_script("install-agent-hooks.sh", "--uninstall")
        self.assertNotEqual(p.returncode, 0)
        self.assertEqual(self.settings.read_bytes(), before)
        self.assertTrue((self.bin / "td-agent-hooks").exists())
        self.assert_no_loose_backups()
        self.assertEqual(list((self.state / "backups").glob("*")), [])

    def test_desktop_remove_and_cache_failures_keep_receipt(self):
        self.assertEqual(self.run_script("install-desktop.sh").returncode, 0)
        self.executable("rm", "#!/bin/sh\nexit 1\n")
        self.assertNotEqual(self.run_script("install-desktop.sh", "--uninstall").returncode, 0)
        self.assertTrue(self.receipt("desktop").exists())
        (self.stub / "rm").unlink()
        self.executable("update-desktop-database", "#!/bin/sh\nexit 1\n")
        self.assertNotEqual(self.run_script("install-desktop.sh", "--uninstall").returncode, 0)
        self.assertTrue(self.receipt("desktop").exists())


if __name__ == "__main__":
    unittest.main()
