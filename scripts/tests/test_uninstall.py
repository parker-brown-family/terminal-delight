#!/usr/bin/env python3
"""Legacy cleanup regressions. Every mutable path is confined to a temp fixture.

No HOME override, desktop connection, TD launch or process signal is used.
TD_TEST_BASELINE=<git commit> exercises the same oracles on historical scripts.
"""
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
        if baseline := os.environ.get("TD_TEST_BASELINE"):
            for name in ("install-agent-hooks.sh", "install-recovery-hook.sh", "install-keys-hotkey.sh", "install-hotkey.sh"):
                (self.repo / "scripts" / name).write_bytes(subprocess.check_output(
                    ["git", "show", f"{baseline}:scripts/{name}"], cwd=REPO))
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.data = self.root / "data"
        self.data.mkdir()
        self.settings = self.root / "settings.json"
        self.bindings = self.root / "bindings.lua"
        self.stub = self.root / "stubs"
        self.stub.mkdir()
        self.env = dict(os.environ, DEST=str(self.bin), DATA=str(self.data),
                        CLAUDE_SETTINGS=str(self.settings), HYPR_BINDINGS=str(self.bindings),
                        XDG_CONFIG_HOME=str(self.root / "config"), XDG_CURRENT_DESKTOP="GNOME",
                        PATH=f"{self.stub}:/usr/bin:/bin", TD_TEST_ROOT=str(self.root))
        self.executable("hyprctl", "#!/bin/sh\nexit 0\n")
        self.executable("terminal-delight", "#!/bin/sh\nexit 97\n")
        self.executable("gsettings", '''#!/usr/bin/python3
import json, os, pathlib, sys
root = pathlib.Path(os.environ['TD_TEST_ROOT'])
path = root / 'gsettings.json'
d = json.loads(path.read_text())
op, schema, key, *values = sys.argv[1:]
k = schema + '/' + key
if op == 'get': print(d[k])
elif op == 'set':
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
        for key in ("DEST", "DATA", "CLAUDE_SETTINGS", "HYPR_BINDINGS", "XDG_CONFIG_HOME", "TD_TEST_ROOT"):
            self.assertTrue(Path(self.env[key]).resolve().is_relative_to(self.root), key)
        self.assertEqual(self.env["HOME"], os.environ["HOME"])
        self.assertEqual(self.env["PATH"].split(":")[0], str(self.stub))
        return subprocess.run(["bash", str(self.repo / "scripts" / name), *args],
                              env=self.env, capture_output=True, text=True, timeout=10)

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

    def test_unrecognized_reference_blocks_helper_removal(self):
        ours = self.hook("td-agent-hooks")
        for command in ["echo td-agent-hooks", ours["command"] + " --custom", "/elsewhere/td-agent-hooks"]:
            content = json.dumps({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": command}]}]}})
            self.settings.write_text(content)
            self.assertNotEqual(self.run_script("install-agent-hooks.sh", "--uninstall").returncode, 0)
            self.assertEqual(self.settings.read_text(), content)
            self.assertTrue((self.bin / "td-agent-hooks").exists())

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


if __name__ == "__main__":
    unittest.main()
