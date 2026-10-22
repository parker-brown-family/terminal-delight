#!/usr/bin/env python3
"""Remove one real application guard at a time, then restore exact bytes.

Run without other builds or edits in this worktree. Native cases require
Hyprland and use only the disposable windows in permissions-native.sh.
"""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
from receipt import source

p = argparse.ArgumentParser()
p.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[2])
p.add_argument("--out", type=Path, required=True)
p.add_argument("--target", type=Path, required=True)
p.add_argument("--native", action="store_true")
a = p.parse_args()
a.repo = a.repo.resolve()
a.out.mkdir(parents=True, exist_ok=False)
baseline = source(a.repo)
env = dict(os.environ, CARGO_TARGET_DIR=str(a.target.resolve()), CARGO_BUILD_JOBS="1")
unit_cases = [
    ("pointer-dispatch", "app/src/pane/bench.rs",
     '        if self.off_limits {\n            return "err an off-limits pane takes no scripted pointer".into();\n        }\n', "",
     "pane::behaviour::synthetic_bench_pointer_refuses_off_limits_but_reaches_an_allowed_pane"),
    ("global-policy", "app/src/ctl.rs",
     "        Ok(Cmd::McpPolicy(p)) if policy_control == PolicyControl::Refused => p.refusal(),\n", "",
     "ctl::tests::an_unscripted_window_refuses_all_global_policy_mutations"),
    ("peer-ancestry", "app/src/ctl.rs", "shell > 1 && descends_from(pid, shell)", "true",
     "ctl::tests::a_claimed_pane_is_downgraded_unless_the_socket_peer_descends_from_its_shell"),
    ("note-injection", "app/src/mcp.rs", "    #[serde(skip)]\n    pub note:", "    pub note:",
     "mcp::tests::config_json_cannot_inject_validated_notes_or_deliverables"),
    ("deliverable-injection", "app/src/mcp.rs", "    #[serde(skip)]\n    pub deliverable:", "    pub deliverable:",
     "mcp::tests::config_json_cannot_inject_validated_notes_or_deliverables"),
    ("hidden-document", "app/src/mcp.rs", " && (!d.off_limits || d.pane == caller.pid)", "",
     "mcp::tests::hidden_document_notes_are_private_but_the_callers_own_float_still_works"),
    ("queued-input", "app/src/pane/bench.rs", "if self.off_limits || self.wb_queued.is_empty()", "if self.wb_queued.is_empty()",
     "pane::behaviour::queued_input_waits_while_the_pane_is_off_limits"),
    ("surface-before-acceptance", "app/src/mcp.rs", "MOVE_PERSISTENCE", "",
     "mcp::tests::a_refused_surface_is_not_persisted_for_the_sweep_to_deliver"),
]
native_cases = [
    ("picker-routing", "app/src/main.rs", "|v| !v.off_limits && v.bench.face() == workbench::Face::Workbench", "|v| v.bench.face() == workbench::Face::Workbench", "bench probe"),
    ("startup-opt-in", "app/src/ctl.rs", "let policy_control = PolicyControl::from_env();", "let policy_control = PolicyControl::Scripted;", "mcp on"),
]
paths = {a.repo / case[1] for case in unit_cases + native_cases}
original = {path: path.read_bytes() for path in paths}
for path, content in original.items():
    (a.out / (path.name + ".original")).write_bytes(content)


def restore():
    for path, content in original.items():
        path.write_bytes(content)


def command(label, args, extra=None):
    with (a.out / (label + ".log")).open("w") as log:
        result = subprocess.run(args, cwd=a.repo / "app", env=dict(env, **(extra or {})), stdout=log, stderr=subprocess.STDOUT, timeout=600)
    return result.returncode, (a.out / (label + ".log")).read_text()


def build_test(label):
    code, log = command(label, ["cargo", "test", "--locked", "--bin", "terminal-delight", "--no-run", "--message-format=json"])
    assert code == 0, log[-5000:]
    artifacts = [json.loads(line) for line in log.splitlines() if line.startswith("{")]
    return next(item["executable"] for item in artifacts if item.get("reason") == "compiler-artifact" and item.get("profile", {}).get("test") and item.get("target", {}).get("name") == "terminal-delight" and item.get("executable"))


def native(label):
    code, log = command(label + "-build", ["cargo", "build", "--locked", "--bin", "terminal-delight"])
    assert code == 0, log[-5000:]
    return command(label, ["bash", str(a.repo / "scripts/security/permissions-native.sh")], {
        "TD_AUTHORITY_BIN": str(a.target.resolve() / "debug/terminal-delight"),
        "TD_AUTHORITY_OUT": str(a.out / (label + "-window")),
    })


def interrupted(*_):
    raise RuntimeError("interrupted; restoring exact source bytes")


signal.signal(signal.SIGTERM, interrupted)
signal.signal(signal.SIGINT, interrupted)
results = []
try:
    binary = build_test("baseline-build")
    for label, _, _, _, test in unit_cases:
        code, log = command("baseline-" + label, [binary, test, "--exact", "--test-threads=1"])
        assert code == 0 and "1 passed" in log, log
    if a.native:
        code, log = native("baseline-native")
        assert code == 0, log
    for label, relative, before, after, test in unit_cases:
        restore()
        path = a.repo / relative
        text = path.read_text()
        if label == "surface-before-acceptance":
            start = text.index("fn present_surface")
            begin = text.index("            if let Some(dir) = pane_dir.as_ref() {", start)
            end = text.index("            tool_ok(", begin)
            block = text[begin:end]
            text = text[:begin] + text[end:]
            insert = text.index("    let patch = ConfigPatch {", start)
            text = text[:insert] + block + "\n" + text[insert:]
            path.write_text(text)
        else:
            assert before in text, (label, "mutation no longer matches")
            path.write_text(text.replace(before, after, 1))
        mutant = source(a.repo)["source_fingerprint"]
        binary = build_test(label + "-build")
        code, log = command(label, [binary, test, "--exact", "--test-threads=1"])
        assert code != 0 and "test result: FAILED." in log, (label, log)
        results.append({"mutation": label, "test": test, "result": "killed", "mutant_source_fingerprint": mutant})
        print(json.dumps(results[-1]), flush=True)
    if a.native:
        for label, relative, before, after, boundary in native_cases:
            restore()
            path = a.repo / relative
            text = path.read_text()
            assert before in text, (label, "mutation no longer matches")
            path.write_text(text.replace(before, after, 1))
            mutant = source(a.repo)["source_fingerprint"]
            code, log = native(label)
            assert code != 0 and "AssertionError" in log and boundary in log, (label, log)
            results.append({"mutation": label, "test": "real scratch native window: " + boundary, "result": "killed", "mutant_source_fingerprint": mutant})
            print(json.dumps(results[-1]), flush=True)
finally:
    restore()
assert source(a.repo)["source_fingerprint"] == baseline["source_fingerprint"]
binary = build_test("restored-build")
for label, _, _, _, test in unit_cases:
    code, log = command("restored-" + label, [binary, test, "--exact", "--test-threads=1"])
    assert code == 0 and "1 passed" in log, log
if a.native:
    code, log = native("restored-native")
    assert code == 0, log
(a.out / "receipt.json").write_text(json.dumps({"source": baseline, "one_mutation_at_a_time": True,
    "restored_exact_bytes": True, "routing_oracle": "native" if a.native else "not_run", "mutations": results}, indent=2) + "\n")
