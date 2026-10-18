"""Restore old/broken production behavior and require behavioral assertion failures."""
import hashlib
import json
import os
import sys
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
CASES = [
    ("old-hidden-callbacks", [
        ("app/src/pane/housekeeping.rs",
         "HIDDEN_INTERVAL.saturating_sub(self.last_think_scan.elapsed())",
         "Duration::from_millis(150)"),
        ("app/src/pane.rs", "if session.master.is_some() && session.shell_pid.is_some() {", "if true {"),
    ], ["hidden_housekeeping_sleeps_and_visibility_edges_wake_it",
        "only_locally_owned_ptys_get_a_foreground_watcher"]),
    ("lost-visibility-wake", [
        ("app/src/pane/housekeeping.rs",
         "if self.shown.swap(shown, std::sync::atomic::Ordering::Relaxed) != shown {\n            self.wake_housekeeping();",
         "if self.shown.swap(shown, std::sync::atomic::Ordering::Relaxed) != shown {"),
    ], ["hidden_housekeeping_sleeps_and_visibility_edges_wake_it"]),
    ("lost-resize-wake", [
        ("app/src/pane/housekeeping.rs",
         "self.pending_pty = Some((target, Instant::now()));\n            self.wake_housekeeping();",
         "self.pending_pty = Some((target, Instant::now()));"),
    ], ["hidden_resize_interrupts_sleep_and_keeps_the_last_target"]),
    ("lost-completion-deadline", [
        ("app/src/pane/housekeeping.rs",
         "if let Some(at) = self.completion_deadline() {", "if let Some(at) = None::<Instant> {"),
    ], ["hidden_completion_keeps_its_short_debounce_without_a_busy_loop"]),
    ("completion-ignores-scroll-settle", [
        ("app/src/pane.rs", "if scroll_settled {", "if true {"),
    ], ["hidden_completion_waits_for_scroll_settle"]),
]
test_filter = "housekeeping_tests"
test_prefix = "pane::housekeeping_tests::"
record_name = "mutations.json"
assertion_evidence = None
if sys.argv[1:] == ["--guard"]:
    signature = "fn on_key(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {"
    CASES = [("key-routing-bypass", [
        ("app/src/pane.rs", signature, signature + '\n        if ev.keystroke.key.as_str() == "f1" { return; }'),
    ], ["on_key_decides_nothing_it_can_decide_in_the_table"])]
    test_filter = "on_key_decides_nothing_it_can_decide_in_the_table"
    test_prefix = "pane::tests::"
    record_name = "guard-control.json"
    assertion_evidence = "on_key looks at `key.as_str()` itself"
else:
    assert not sys.argv[1:], sys.argv[1:]
env = dict(os.environ, CARGO_TARGET_DIR="/home/parker/Work/terminal-delight/app/target")
# A negative control is invalid if the unmodified test already fails.
baseline_log = OUT / (record_name.removesuffix(".json") + "-baseline.log")
with baseline_log.open("w") as stream:
    baseline = subprocess.run(
        ["cargo", "test", "--bin", "terminal-delight", "--locked", test_filter, "--", "--nocapture"],
        cwd=ROOT / "app", env=env, stdout=stream, stderr=subprocess.STDOUT, timeout=600)
assert baseline.returncode == 0, "Baseline test failed before fault injection: " + str(baseline_log)
results = []
for name, edits, expected_tests in CASES:
    originals = {ROOT / p: (ROOT / p).read_bytes() for p, _, _ in edits}
    digests = {str(p.relative_to(ROOT)): hashlib.sha256(data).hexdigest()
               for p, data in originals.items()}
    log = OUT / (name + ".mutation.log")
    try:
        for relative, needle, replacement in edits:
            path = ROOT / relative
            text = path.read_text()
            assert text.count(needle) == 1, (name, relative, text.count(needle))
            path.write_text(text.replace(needle, replacement))
        with log.open("w") as stream:
            run = subprocess.run(
                ["cargo", "test", "--bin", "terminal-delight", "--locked",
                 test_filter, "--", "--nocapture"],
                cwd=ROOT / "app", env=env, stdout=stream, stderr=subprocess.STDOUT,
                timeout=600,
            )
        output = log.read_text()
        failed = [test for test in expected_tests
                  if ("test " + test_prefix + test + " ... FAILED") in output]
        rejected = (run.returncode != 0 and failed == expected_tests and "error[E" not in output
                    and (assertion_evidence is None or assertion_evidence in output))
        results.append({"mutation": name, "exit": run.returncode,
                        "expected_assertion_failures": expected_tests, "observed": failed,
                        "rejected_by_behavior": rejected, "required_assertion_text": assertion_evidence,
                        "production_sources_sha256": digests,
                        "log_sha256": hashlib.sha256(log.read_bytes()).hexdigest()})
        print(json.dumps(results[-1]), flush=True)
        if not rejected:
            raise RuntimeError("Mutation survived, compiled incorrectly, or failed for another reason: " + name)
    finally:
        for path, original in originals.items():
            path.write_bytes(original)
            assert path.read_bytes() == original
        (OUT / record_name).write_text(json.dumps(results, indent=2) + "\n")
print("All faults rejected and production sources byte-restored.", flush=True)
