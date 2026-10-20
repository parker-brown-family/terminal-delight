"""Tests for record.py (real program output, and the leak gate in front of it)
and unknowns.py (what a run could not check). No Rust build is needed.

    python3 -m unittest discover -s scripts/lab
"""

import json
import os
import re
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import record  # noqa: E402
import unknowns  # noqa: E402

NEEDLES = [("the recorder's home directory", b"/home/someone"), ("the recorder's user name", b"someone"),
           ("this machine's host name", b"box-17"), ("this machine's id", b"0123abcd" * 4)]


class LeakGate(unittest.TestCase):
    def rejects(self, data: bytes) -> list[str]:
        return record.leak_check(data, NEEDLES)

    def test_innocent_terminal_output_passes(self):
        innocent = [
            b"\x1b[1;32mlab\x1b[0m:\x1b[34m~\x1b[0m$ echo hi\r\nhi\r\n",
            b"\x1b[33m4f2a9c1\x1b[0m Merge the editor sample (HEAD -> main, tag: v0.1)\r\n",
            b"Lab <lab@example.invalid>  2026-01-04 00:00:00 +0000\r\n",
            b"vim 9.1.0, python 3.12.3, at 12:34:56, listening on 127.0.0.1 and ::1\r\n",
            "\u2588\u2588\u2591 50% fetching lab.tar \u4f60\u597d".encode(),
            b"\x1b[38;2;10;200;30mtrue colour\x1b[0m \x1b[?1049h\x1b[22;0;0t",
            b"a line the lab pages through this text, with plenty of ordinary words in it\r\n",
            b"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\r\n",
        ]
        for data in innocent:
            self.assertEqual(self.rejects(data), [], data)

    def test_every_planted_leak_is_rejected(self):
        planted = {
            b"cd /home/someone/projects": "home directory",
            b"someone@box:~$": "user name",
            b"welcome to box-17": "host name",
            b"id " + b"0123abcd" * 4: "machine's id",
            b"Author: Parker <person@realmail.com>": "email address",
            b"connected to 192.168.1.20 port 22": "IPv4 address",
            b"inet6 fe80::1c2b:3aff:fe4d:5e6f": "IPv6 address",
            b"2001:0db8:85a3:0000:0000:8a2e:0370:7334": "IPv6 address",
            b"export GH=ghp_" + b"A1b2" * 9: "token-shaped",
            b"github_pat_" + b"11ABCDEFG0" * 3: "token-shaped",
            b"key sk-" + b"proj-abcdEFGH1234" * 2: "token-shaped",
            b"AKIA" + b"ABCDEFGHIJKLMNOP": "token-shaped",
            b"xoxb-1234567890-abcdefghij": "token-shaped",
            b"-----BEGIN OPENSSH PRIVATE KEY-----": "token-shaped",
            b"commit " + b"0123456789abcdef" * 2 + b"01234567": "hex run",
            b"Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9xyzQ": "base64-shaped",
        }
        for data, why in planted.items():
            reasons = self.rejects(b"\x1b[0m" + data + b"\r\n")
            self.assertTrue(any(why in r for r in reasons), (data, reasons))

    def test_a_secret_split_by_colour_codes_is_still_found(self):
        data = b"\x1b[33mperson\x1b[0m@\x1b[1mrealmail\x1b[0m.com and /home/\x1b[31msomeone\x1b[0m/x"
        reasons = self.rejects(data)
        self.assertTrue(any("email" in r for r in reasons), reasons)

    def test_a_reason_never_repeats_the_secret(self):
        data = b"/home/someone ghp_" + b"A1b2" * 9 + b" person@realmail.com 10.1.2.3"
        for reason in self.rejects(data):
            for secret in (b"someone", b"A1b2A1b2", b"realmail", b"10.1.2.3"):
                self.assertNotIn(secret.decode(), reason)

    def test_secret_looking_environment_values_become_needles(self):
        saved = dict(os.environ)
        os.environ["TD_LAB_TEST_API_TOKEN"] = "a-value-that-is-secret"
        os.environ["TD_LAB_TEST_COLOUR"] = "a-value-that-is-not"
        try:
            labels = {label: value for label, value in record.machine_needles()}
        finally:
            os.environ.clear()
            os.environ.update(saved)
        self.assertIn(b"a-value-that-is-secret", labels.values())
        self.assertNotIn(b"a-value-that-is-not", labels.values())
        self.assertTrue(any("TD_LAB_TEST_API_TOKEN" in label for label in labels))

    def test_the_real_machine_is_a_needle(self):
        values = [value for _, value in record.machine_needles()]
        self.assertIn(os.path.expanduser("~").encode(), values)


@unittest.skipUnless(shutil.which("sh") and hasattr(os, "openpty"), "needs a pty and sh")
class Recording(unittest.TestCase):
    def test_a_program_sees_only_the_sterile_environment(self):
        os.environ["TD_LAB_TEST_CANARY"] = "inherited"
        try:
            with tempfile.TemporaryDirectory() as scratch:
                home = Path(scratch, "home")
                home.mkdir()
                program = record.Program("env", ["sh", "-c", "env; stty size"], ["sh"])
                got = record.record(program, (33, 7), Path(scratch), record.sterile_env(home)).data()
        finally:
            os.environ.pop("TD_LAB_TEST_CANARY")
        names = {line.split("=", 1)[0] for line in got.decode().splitlines() if "=" in line}
        self.assertNotIn("TD_LAB_TEST_CANARY", names)
        allowed = set(record.sterile_env(Path("/x"))) | {"COLUMNS", "LINES", "PWD", "SHLVL", "_", "OLDPWD"}
        self.assertTrue(names <= allowed, names - allowed)
        self.assertIn(b"7 33", got)

    def test_reads_are_kept_and_keys_are_sent(self):
        with tempfile.TemporaryDirectory() as scratch:
            home = Path(scratch, "home")
            home.mkdir()
            program = record.Program("echo", ["sh", "-c", "printf 'ready\\n'; read x; printf 'got %s\\n' \"$x\""],
                                     ["sh"], ["typed\r"])
            rec = record.record(program, (20, 4), Path(scratch), record.sterile_env(home))
        self.assertIn(b"got typed", rec.data())
        case = record.as_case(rec)
        self.assertEqual(case["name"], "rec-1/echo-20x4")
        self.assertIn("checkpoint", case["ops"])
        self.assertEqual("".join(op["write"] for op in case["ops"] if isinstance(op, dict)).encode(), rec.data())

    def test_a_resize_is_recorded_where_it_happened(self):
        rec = record.Recording("x", (30, 5), [b"a", ("resize", 20, 5), None, b"b"])
        self.assertEqual(record.as_case(rec)["ops"],
                         [{"write": "a"}, {"resize": [20, 5]}, "checkpoint", {"write": "b"}])

    def test_a_recording_past_the_limit_is_cut_and_says_so(self):
        rec = record.Recording("x", (30, 5), [b"a" * 1000] * 2000)
        case = record.as_case(rec)
        self.assertLessEqual(len(case["ops"]), record.MAX_RECORDED_OPS)
        self.assertTrue(any(n.startswith("cut at") for n in rec.notes))

    def test_a_missing_program_is_reported_not_skipped(self):
        saved = record.programs
        record.programs = lambda: [record.Program("ghost", ["no-such-program-td-lab"], ["no-such-program-td-lab"])]
        try:
            cases, report = record.record_all()
        finally:
            record.programs = saved
        self.assertEqual(cases, [])
        self.assertEqual(len(report), len(record.SIZES))
        self.assertTrue(all("missing on this machine" in r["reason"] for r in report))

    def test_a_leaking_recording_is_dropped_with_its_reason(self):
        saved = record.programs
        record.programs = lambda: [record.Program("leaky", ["sh", "-c", "echo mail me at person@realmail.com"], ["sh"])]
        try:
            cases, report = record.record_all()
        finally:
            record.programs = saved
        self.assertEqual(cases, [])
        self.assertTrue(all("leak gate" in r["reason"] and "realmail" not in r["reason"] for r in report))

    @unittest.skipUnless(shutil.which("python3", path="/usr/bin:/bin"), "needs python3")
    def test_a_real_recording_passes_the_gate(self):
        cases, report = record.record_all(["progress"])
        self.assertTrue(all(r["recorded"] for r in report), report)
        self.assertEqual(len(cases), len(record.SIZES))


class Tokens(unittest.TestCase):
    def test_sgr_parameters_are_split_into_families(self):
        self.assertEqual(unknowns.tokens(b"\x1b[1;38;5;202;48;2;1;2;3;4:3m"),
                         ["SGR 1", "SGR 38;5", "SGR 48;2", "SGR 4:3"])
        self.assertEqual(unknowns.tokens(b"\x1b[m"), ["SGR 0"])

    def test_each_mode_in_one_sequence_counts(self):
        self.assertEqual(unknowns.tokens(b"\x1b[?1049;25h\x1b[4l"),
                         ["CSI ?1049 h/l", "CSI ?25 h/l", "CSI 4 h/l"])

    def test_strings_escapes_and_controls(self):
        data = (b"\x1b]0;title\x07\x1b]8;;http://x\x1b\\\x1bP$qm\x1b\\\x1b_Ga=T\x1b\\"
                b"\x1b(B\x1b7\x1b[ q\x1b[?$p\x07\t")
        self.assertEqual(unknowns.tokens(data),
                         ["OSC 0", "OSC 8", "DCS $q", "APC G", "ESC ( B", "ESC 7", "CSI SP q",
                          "CSI ?$p", "BEL", "HT"])

    def test_text_is_sorted_by_kind(self):
        self.assertEqual(unknowns.tokens("a\u4f60e\u0301\u200d\U0001f642\u00e9".encode() + b"\xff"),
                         ["text: invalid UTF-8", "text: ASCII", "text: wide", "text: ASCII",
                          "text: combining mark", "text: zero-width joiner", "text: emoji", "text: other non-ASCII"])

    def test_an_unfinished_sequence_is_named(self):
        self.assertEqual(unknowns.tokens(b"a\x1b[12"), ["text: ASCII", "unterminated CSI"])


class Coverage(unittest.TestCase):
    FAMILIES = [
        {"id": "bold", "name": "bold", "match": "SGR 1", "observed": True},
        {"id": "title", "name": "title", "match": "OSC [012]", "observed": False, "why": "an event"},
        {"id": "shape", "name": "shape", "match": "CSI SP q", "observed": "partly", "why": "no blink"},
        {"id": "blink", "name": "blink", "match": "SGR 5", "observed": False, "why": "no flag"},
    ]

    def families(self):
        path = Path(tempfile.mkdtemp(), "c.json")
        path.write_text(json.dumps({"families": self.FAMILIES}))
        return unknowns.load_catalogue(path)

    def test_four_states_are_kept_apart(self):
        cover = unknowns.coverage({"generated": [b"\x1b[1m\x1b[ q"], "recorded": [b"\x1b]2;x\x07\x1b[9m"]},
                                  self.families())
        states = {r["id"]: r["state"] for r in cover["families"]}
        self.assertEqual(states, {"bold": "checked", "title": "unseen", "shape": "partly", "blink": "never sent"})
        self.assertEqual(cover["states"], {"checked": 1, "partly": 1, "unseen": 1, "never sent": 1})
        self.assertEqual(cover["uncatalogued"], {"SGR 9": {"recorded": 1}})
        title = next(r for r in cover["families"] if r["id"] == "title")
        self.assertEqual(title["sent"], {"generated": 0, "recorded": 1})

    def test_a_family_not_fully_observed_must_say_why(self):
        self.FAMILIES = [{"id": "x", "name": "x", "match": "BEL", "observed": False}]
        with self.assertRaises(ValueError):
            self.families()

    def test_the_summary_names_what_could_not_be_checked(self):
        cover = unknowns.coverage({"generated": [b"\x1b]2;x\x07"]}, self.families())
        text = "\n".join(unknowns.summary_lines(cover))
        self.assertIn("## What this run could not check", text)
        self.assertIn("| title | 1 | an event |", text)
        self.assertIn("Never sent: bold; shape; blink.", text)

    def test_the_real_catalogue_loads(self):
        self.assertGreater(len(unknowns.load_catalogue()), 50)

    def test_every_mode_the_catalogue_says_is_observed_is_one_td_reads(self):
        """observe() records the TermMode bits TD reads (0 to 17), so a family
        claiming it is seen through a mode must name one of those."""
        types = (Path(__file__).resolve().parents[2] / "app/src/vt/types.rs").read_text()
        bits = {name: int(bit) for name, bit in re.findall(r"const (\w+)\s*= 1 << (\d+);", types)}
        bits["SHOW_CURSOR"] = 0
        for family in unknowns.load_catalogue():
            how = family.get("how") or ""
            for name in re.findall(r"\b([A-Z][A-Z0-9_]{3,})\b", how):
                if family["observed"] is True:
                    self.assertIn(name, bits, family["id"])
                    self.assertLess(bits[name], 18, family["id"])


if __name__ == "__main__":
    unittest.main()
