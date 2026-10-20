"""Tests for run.py's own failure modes, with no Rust build: the harness is
stood in for by small scripts, because what is under test here is what the
lab does when the harness misbehaves.

    python3 -m unittest discover -s scripts/lab
"""

import json
import os
import shlex
import stat
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import run  # noqa: E402

HEADER = json.dumps({"format_version": 2, "core": "stand-in", "chunks": "whole", "mutation": ""})


def observation(text="hi", line=0, column=2):
    return {"size": [10, 2], "cursor": {"line": line, "column": column, "shape": "Block"},
            "modes": ["SHOW_CURSOR"], "history_size": 0, "sync_open": False, "events": [],
            "screen": [{"text": text, "runs": []}, {"text": "", "runs": []}], "history": []}


class StandIn:
    """A fake harness binary: a script that writes the given output file."""

    def __init__(self, body: str):
        self.dir = tempfile.TemporaryDirectory()
        self.path = Path(self.dir.name, "harness")
        self.path.write_text("#!/bin/sh\n" + body + "\n")
        self.path.chmod(self.path.stat().st_mode | stat.S_IEXEC)

    def __enter__(self):
        run._binaries["rio"] = self.path
        return self

    def __exit__(self, *exc):
        run._binaries.pop("rio", None)
        self.dir.cleanup()


class Determinism(unittest.TestCase):
    def test_splitmix_matches_the_rust_harness(self):
        rng = run.SplitMix(0)
        self.assertEqual(rng.next(), 0xE220A8397B1DCDAF)
        self.assertEqual(rng.next(), 0x6E789E6AA1B965F4)

    def test_a_seed_names_one_case(self):
        self.assertEqual(run.generate(41), run.generate(41))
        self.assertNotEqual(run.generate(41)["ops"], run.generate(42)["ops"])

    def test_generated_cases_respect_the_limits(self):
        for seed in range(300):
            run.check_limits(run.generate(seed))

    def test_about_a_quarter_of_generated_cases_set_no_style(self):
        cases = [run.generate(seed) for seed in range(2000)]
        free = [c for c in cases if run.is_style_free(c)]
        self.assertGreater(len(free), 2000 * 0.2)
        self.assertLess(len(free), 2000 * 0.35, "a quarter by choice, plus styled cases that drew no style")
        resized = [c for c in free if any(isinstance(op, dict) and "resize" in op for op in c["ops"])]
        self.assertGreater(len(resized), 200, "style-free cases must resize too")

    def test_a_style_is_one_that_sets_something(self):
        def case(text):
            return {"name": "c", "size": [4, 1], "ops": [{"write": text}]}
        self.assertTrue(run.is_style_free(case("a\x1b[mb\x1b[0m\x1b[0;0m")))
        for styled in ("\x1b[1m", "\x1b[0;31m", "\x1b[38;5;202m", "\x1b[4:3m"):
            self.assertFalse(run.is_style_free(case(styled)), repr(styled))

    def test_gen_2_sends_synchronized_updates_and_prompt_marks(self):
        data = b"".join(run.all_bytes(run.generate(seed)) for seed in range(200))
        for needle in (b"\x1b[?2026h", b"\x1b[?2026l", b"\x1b[?25;2026h", b"\x1bP=1s\x1b\\",
                       b"\x1b]133;A\x07", b"\x1b]133;A;aid=7\x1b\\"):
            self.assertIn(needle, data)

    def test_shards_partition_the_generated_cases(self):
        names = lambda i: {c["name"] for c in run.select(30, 7, (i, 3))}  # noqa: E731
        everything = {c["name"] for c in run.select(30, 7, (1, 1))}
        shards = [names(1), names(2), names(3)]
        self.assertEqual(set().union(*shards), everything)
        self.assertEqual(sum(len(s) for s in shards), len(everything))


class Limits(unittest.TestCase):
    def test_oversized_cases_are_refused(self):
        with self.assertRaises(run.SetupError):
            run.check_limits({"name": "w", "size": [run.MAX_COLUMNS + 1, 2], "ops": []})
        with self.assertRaises(run.SetupError):
            run.check_limits({"name": "r", "size": [2, 2], "ops": [{"resize": [2, run.MAX_ROWS + 1]}]})
        with self.assertRaises(run.SetupError):
            run.check_limits({"name": "b", "size": [2, 2], "ops": [{"write": "x" * (run.MAX_BYTES + 1)}]})

    def test_the_artifact_cap_stops_writing(self):
        finding = run.Finding("cores", {"name": "c", "size": [2, 2], "ops": [{"write": "x" * 2000}]},
                              run.Spec("rio"), run.Spec("alacritty"), "differs in screen", "d")
        with tempfile.TemporaryDirectory() as out:
            saved = run.ARTIFACT_CAP
            try:
                run.ARTIFACT_CAP = 1000
                self.assertFalse(run.write_finding(finding, Path(out), [0]))
                self.assertEqual(list(Path(out).iterdir()), [])
            finally:
                run.ARTIFACT_CAP = saved


class OutputParsing(unittest.TestCase):
    def ok(self, name="a"):
        return json.dumps({"case": name, "observations": [observation()]})

    def test_a_valid_output_reads(self):
        results = run.parse_output("\n".join([HEADER, self.ok("a"), self.ok("b")]), ["a", "b"], "t")
        self.assertEqual(set(results), {"a", "b"})

    def test_nothing_written_is_a_setup_failure(self):
        with self.assertRaises(run.SetupError):
            run.parse_output("", ["a"], "t")

    def test_a_missing_case_is_a_setup_failure_not_agreement(self):
        with self.assertRaisesRegex(run.SetupError, "no result"):
            run.parse_output("\n".join([HEADER, self.ok("a")]), ["a", "b"], "t")

    def test_malformed_lines_are_setup_failures(self):
        for bad in ["{", json.dumps({"case": "a"}), json.dumps({"case": "a", "observations": []}),
                    json.dumps({"case": "a", "observations": ["x"]}), json.dumps({"case": "zz", "observations": [{}]})]:
            with self.assertRaises(run.SetupError, msg=bad):
                run.parse_output("\n".join([HEADER, bad]), ["a"], "t")

    def test_a_wrong_header_is_a_setup_failure(self):
        with self.assertRaises(run.SetupError):
            run.parse_output("\n".join([json.dumps({"format_version": 9, "core": "x"}), self.ok()]), ["a"], "t")

    def test_a_refused_corpus_is_reported(self):
        refusal = json.dumps({"case": None, "failure": {"kind": "setup_failed", "detail": {}}})
        with self.assertRaisesRegex(run.SetupError, "refused"):
            run.parse_output("\n".join([HEADER, refusal]), ["a"], "t")


class Comparing(unittest.TestCase):
    def test_agreement_has_no_signature(self):
        a = {"observations": [observation()]}
        self.assertIsNone(run.signature(a, json.loads(json.dumps(a))))

    def test_a_difference_names_its_field(self):
        a = {"observations": [observation("hi")]}
        b = {"observations": [observation("ho")]}
        self.assertEqual(run.signature(a, b), "differs in screen")
        index, detail = run.first_difference(a["observations"], b["observations"])
        self.assertEqual(index, 0)
        self.assertIn("screen row 0", detail)

    def test_an_open_update_and_what_the_core_said_are_named_too(self):
        held = observation()
        held["sync_open"] = True
        self.assertEqual(run.signature({"observations": [observation()]}, {"observations": [held]}),
                         "differs in sync_open")
        told = observation()
        told["events"] = [["reply", "\x1b[?6c"]]
        self.assertEqual(run.signature({"observations": [observation()]}, {"observations": [told]}),
                         "differs in events")

    def test_a_crash_on_one_side_is_a_difference(self):
        a = {"observations": [observation()]}
        b = {"failure": {"kind": "crash"}}
        self.assertEqual(run.signature(a, b), "a:ok/b:crash")


class Harness(unittest.TestCase):
    case = {"name": "a", "size": [4, 2], "ops": [{"write": "x"}]}

    def test_a_harness_that_writes_nothing_fails_setup(self):
        with StandIn("exit 0"):
            with self.assertRaises(run.SetupError):
                run.execute(run.Spec("rio"), [self.case])

    def test_a_harness_that_exits_nonzero_fails_setup(self):
        with StandIn('echo boom; exit 101'):
            with self.assertRaisesRegex(run.SetupError, "boom"):
                run.execute(run.Spec("rio"), [self.case])

    def test_a_harness_that_hangs_is_stopped(self):
        saved = run.HARNESS_FLOOR_S
        try:
            run.HARNESS_FLOOR_S = 0.5
            with StandIn("sleep 30"):
                with self.assertRaisesRegex(run.SetupError, "ran past"):
                    run.execute(run.Spec("rio"), [self.case], timeout_ms=1)
        finally:
            run.HARNESS_FLOOR_S = saved

    def test_a_working_harness_is_read(self):
        line = json.dumps({"case": "a", "observations": [observation()]}).replace("'", "'\\''")
        header = HEADER.replace("'", "'\\''")
        with StandIn(f"printf '%s\\n%s\\n' '{header}' '{line}' > \"$TD_LAB_OUT\""):
            results = run.execute(run.Spec("rio"), [self.case])
        self.assertIn("observations", results["a"])


class Shrinking(unittest.TestCase):
    def test_ddmin_finds_the_two_items_that_matter(self):
        budget = {"left": 10_000}
        keep = lambda cands: [7 in c and 13 in c for c in cands]  # noqa: E731
        self.assertEqual(sorted(run.ddmin(list(range(64)), keep, budget)), [7, 13])

    def test_ddmin_stops_when_the_budget_is_spent(self):
        budget = {"left": 3}
        result = run.ddmin(list(range(64)), lambda cands: [False] * len(cands), budget)
        self.assertEqual(len(result), 64)

    def test_minimize_shrinks_a_case_to_its_trigger_and_keeps_the_signature(self):
        trigger = "\x1b[K"

        def fake_evaluate(cases, a, b):
            return ["differs in screen" if trigger in "".join(
                op.get("write", "") for op in c["ops"] if isinstance(op, dict)) else None for c in cases]

        saved = run.evaluate
        run.evaluate = fake_evaluate
        try:
            case = {"name": "big", "size": [80, 24], "ops": [
                {"write": "hello world"}, "checkpoint", {"write": "abc" + trigger + "defgh"},
                {"resize": [10, 3]}, {"write": "tail"}]}
            finding = run.Finding("control", case, run.Spec("rio"), run.Spec("rio", "whole", "m"),
                                  "differs in screen", "planted")
            small = run.minimize(finding)
        finally:
            run.evaluate = saved
        self.assertEqual(small["ops"], [{"write": trigger}])
        self.assertEqual(small["size"], [2, 1])

    def test_a_minimized_case_that_stops_failing_is_refused(self):
        calls = {"n": 0}

        def flaky(cases, a, b):
            calls["n"] += 1
            return ["differs in screen" if calls["n"] < 3 else None for _ in cases]

        saved = run.evaluate
        run.evaluate = flaky
        try:
            finding = run.Finding("control", {"name": "f", "size": [4, 2], "ops": [{"write": "ab"}]},
                                  run.Spec("rio"), run.Spec("rio"), "differs in screen", "d")
            with self.assertRaises(run.SetupError):
                run.minimize(finding)
        finally:
            run.evaluate = saved


def fake_cores(*triggers):
    """A stand-in for `evaluate`: two cores disagree exactly when a case's
    writes still hold one of these strings."""
    def evaluate(cases, a, b):
        out = []
        for c in cases:
            text = "".join(op.get("write", "") for op in c["ops"] if isinstance(op, dict))
            resized = any(isinstance(op, dict) and "resize" in op for op in c["ops"])
            hit = any(t in text for t in triggers) or ("RESIZE" in triggers and resized)
            out.append("differs in screen" if hit else None)
        return out
    return evaluate


class Known(unittest.TestCase):
    ORIGIN = {"id": "origin", "reason": "r", "neutralize": {"strip": "\\x1b\\[\\?6[hl]"}}
    REFLOW = {"id": "reflow", "reason": "r", "neutralize": {"drop_resize": True}}

    def setUp(self):
        self.saved = run.evaluate

    def tearDown(self):
        run.evaluate = self.saved

    def finding(self, ops, kind="cores", a=None, b=None):
        return run.Finding(kind, {"name": "c", "size": [4, 2], "ops": ops},
                           a or run.Spec("rio"), b or run.Spec("alacritty"), "differs in screen", "d")

    def write_known(self, entries):
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        path = Path(d.name, "known.json")
        path.write_text(json.dumps({"differences": entries}))
        saved = run.KNOWN
        run.KNOWN = path
        self.addCleanup(setattr, run, "KNOWN", saved)

    def test_entries_that_cannot_be_checked_are_refused(self):
        for bad in [{"id": "x", "neutralize": {"strip": "a"}},
                    {"id": "x", "reason": "r"},
                    {"id": "x", "reason": "r", "neutralize": {"strip": "("}},
                    {"id": "x", "reason": "r", "neutralize": {"bogus": 1}},
                    {"id": "x", "reason": "r", "neutralize": {"when": "a"}},
                    {"id": "x", "reason": "r", "neutralize": {"strip": "a", "when": "("}},
                    {"id": "x", "reason": "r", "kind": "chunk", "neutralize": {"strip": "a"}}]:
            self.write_known([bad])
            with self.assertRaises(run.SetupError, msg=bad):
                run.known_differences()

    def test_the_real_known_file_loads(self):
        self.assertTrue(run.known_differences())

    def test_neutralize_removes_only_the_cause(self):
        case = {"name": "c", "size": [4, 2], "ops": [{"write": "a\x1b[?6hb"}, {"resize": [2, 2]}, "checkpoint"]}
        self.assertEqual(run.neutralize(case, [self.ORIGIN])["ops"], [{"write": "ab"}, {"resize": [2, 2]}, "checkpoint"])
        self.assertEqual(run.neutralize(case, [self.REFLOW])["ops"], [{"write": "a\x1b[?6hb"}, "checkpoint"])

    def test_a_when_pattern_confines_a_cause_to_its_circumstances(self):
        erase_near_wide = {"id": "w", "reason": "r", "neutralize": {"strip": "\\x1b\\[K", "when": "\\xe5"}}
        wide = {"name": "c", "size": [4, 1], "ops": [{"write": "好\x1b[K"}]}
        narrow = {"name": "c", "size": [4, 1], "ops": [{"write": "ab\x1b[K"}]}
        self.assertEqual(run.neutralize(wide, [erase_near_wide])["ops"], [{"write": "好"}])
        self.assertEqual(run.neutralize(narrow, [erase_near_wide])["ops"], narrow["ops"])

    def test_a_cause_explains_a_finding_only_if_removing_it_makes_the_cores_agree(self):
        run.evaluate = fake_cores("\x1b[?6h", "MYSTERY")
        explained = self.finding([{"write": "x\x1b[?6h"}])
        unexplained = self.finding([{"write": "x\x1b[?6hMYSTERY"}])
        untouched = self.finding([{"write": "MYSTERY"}])
        run.classify_all([explained, unexplained, untouched], [self.ORIGIN, self.REFLOW])
        self.assertEqual(explained.classified, "origin")
        self.assertIsNone(unexplained.classified, "a second, unknown cause must keep it unexplained")
        self.assertIsNone(untouched.classified)

    def test_a_combination_is_reduced_to_the_causes_it_needs(self):
        run.evaluate = fake_cores("\x1b[?6h", "RESIZE")
        both = self.finding([{"write": "\x1b[?6h"}, {"resize": [2, 2]}])
        run.classify_all([both], [self.ORIGIN, self.REFLOW])
        self.assertEqual(both.classified, "origin+reflow")
        tab = {"id": "tab", "reason": "r", "neutralize": {"strip": "\\t"}}
        needless = self.finding([{"write": "\t\x1b[?6h"}, {"resize": [2, 2]}])
        run.classify_all([needless], [tab, self.ORIGIN, self.REFLOW])
        self.assertEqual(needless.classified, "origin+reflow", "the tab was present but not needed")

    def test_a_chunk_entry_explains_only_its_own_core(self):
        run.evaluate = fake_cores("é")
        entry = {"id": "vte", "reason": "r", "kind": "chunk", "core": "alacritty",
                 "neutralize": {"strip": "[\\x80-\\xff]"}}
        ala = self.finding([{"write": "é"}], "chunk", run.Spec("alacritty"), run.Spec("alacritty", "seed:11"))
        rio = self.finding([{"write": "é"}], "chunk", run.Spec("rio"), run.Spec("rio", "seed:11"))
        run.classify_all([ala, rio], [entry])
        self.assertEqual(ala.classified, "vte")
        self.assertIsNone(rio.classified, "a known alacritty bug must not excuse rio-vt")

    def test_materialized_cuts_keep_every_byte_in_order(self):
        case = run.generate(77)
        written = run.materialize(case, 11)
        self.assertEqual(run.all_bytes(written), run.all_bytes(case))
        writes = [op for op in written["ops"] if run.write_payload(op)]
        self.assertTrue(all(1 <= len(run.write_payload(op)[1]) <= 16 for op in writes))


class ExitCode(unittest.TestCase):
    """The exit code is the only thing CI reads, so it is tested directly."""

    def run_with(self, finding, known):
        saved = (run.find_differences, run.known_differences, run.select, run.minimize, run.explain)
        run.find_differences = lambda cases, cores: ([finding] if finding else [], {"cases": 1, "runs": 6, "observations": 3})
        run.known_differences = lambda: known
        run.select = lambda *a: [{"name": "c", "size": [2, 1], "ops": []}]
        run.minimize = lambda f: f.case
        run.explain = lambda *a: None
        try:
            with tempfile.TemporaryDirectory() as out:
                args = run.argparse.Namespace(out=out, cases=1, seed_base=0, shard=(1, 1), minimize_budget=60,
                                              record=False)
                with open(os.devnull, "w") as quiet:
                    saved_stdout, sys.stdout = sys.stdout, quiet
                    try:
                        code = run.command_run(args)
                    finally:
                        sys.stdout = saved_stdout
                self.written = {p.name: p.read_text() for p in Path(out).iterdir() if p.is_file()}
                return code
        finally:
            (run.find_differences, run.known_differences, run.select, run.minimize, run.explain) = saved

    def test_an_unexplained_finding_fails_the_run(self):
        finding = run.Finding("cores", {"name": "c", "size": [2, 1], "ops": []},
                              run.Spec("rio"), run.Spec("alacritty"), "differs in screen", "d")
        self.assertEqual(self.run_with(finding, []), 1)

    def test_a_run_with_nothing_unexplained_passes(self):
        self.assertEqual(self.run_with(None, []), 0)

    def test_every_run_says_what_it_could_not_check(self):
        self.run_with(None, [])
        self.assertIn("## What this run could not check", self.written["summary.md"])
        cover = json.loads(self.written["coverage.json"])
        self.assertEqual(set(cover["states"]), set(run.unknowns.STATES))

    def test_switching_recording_off_is_said_not_silent(self):
        self.run_with(None, [])
        self.assertIn("recording was switched off", self.written["summary.md"])


class Recorded(unittest.TestCase):
    def test_recorded_cases_ride_on_shard_one_only(self):
        recorded = [{"name": "rec-1/x-2x1", "size": [2, 1], "ops": [{"write": "a"}]}]
        self.assertIn(recorded[0], run.select(3, 0, (1, 2), recorded))
        self.assertNotIn(recorded[0], run.select(3, 0, (2, 2), recorded))

    def test_cases_are_counted_by_where_they_came_from(self):
        cases = [{"name": "rec-1/x", "ops": [{"write": "a"}]}, {"name": f"{run.CORPUS_VERSION}/4", "ops": []},
                 {"name": "printable-text", "ops": []}]
        self.assertEqual({k: len(v) for k, v in run.sources(cases).items()},
                         {"generated": 1, "named": 1, "recorded": 1})

    def test_a_recording_is_compared_against_its_own_read_boundaries(self):
        # A stand-in core that draws something else when a recording's writes
        # are joined, and the same thing however a plain case is cut.
        code = ("import json, os\n"
                "cases = json.load(open(os.environ['TD_LAB_IN']))['cases']\n"
                "joined = os.environ['TD_LAB_CHUNKS'] == 'joined'\n"
                f"base = {observation()!r}\n"
                "with open(os.environ['TD_LAB_OUT'], 'w') as out:\n"
                f"    out.write({HEADER!r} + '\\n')\n"
                "    for c in cases:\n"
                "        text = 'joined' if joined and c['name'].startswith('rec-') else 'hi'\n"
                "        o = dict(base, screen=[{'text': text, 'runs': []}, {'text': '', 'runs': []}])\n"
                "        out.write(json.dumps({'case': c['name'], 'observations': [o]}) + '\\n')\n")
        body = "exec python3 -c " + shlex.quote(code)
        cases = [{"name": "rec-1/x-10x2", "size": [10, 2], "ops": [{"write": "a"}, {"write": "b"}]},
                 {"name": "plain", "size": [10, 2], "ops": [{"write": "a"}]}]
        with StandIn(body):
            findings, counts = run.find_differences(cases, ["rio"])
        self.assertEqual([(f.kind, f.case["name"], f.b.chunks) for f in findings],
                         [("chunk", "rec-1/x-10x2", "joined")])
        self.assertEqual(counts["runs"], 2 * 3 + 1)


if __name__ == "__main__":
    unittest.main()
