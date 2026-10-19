#!/usr/bin/env python3
"""Deliberately break each optimization; require its real-pane test to reject it."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
cases = [
    ("stale-styling", "app/src/pane.rs", "if memo.key == key {", "if true {",
     "styled_viewport_reuses_only_identical_cells_and_view_state"),
    ("uncached-documents", "app/src/pane/doc.rs",
     ".cached(gpui::StyleRefinement::default().size_full())", "",
     "quiet_documents_reuse_paint_and_changes_invalidate_it"),
    ("covered-grid", "app/src/pane.rs",
     "if self.bench.face() == crate::workbench::Face::Workbench || self.has_doc_face() {",
     "if false {", "covered_faces_skip_grid_styling_and_terminal_return_catches_up"),
]
env = dict(os.environ, CARGO_TARGET_DIR="/home/parker/Work/terminal-delight/app/target")
results = []
for name, relative, needle, replacement, test in cases:
    path = ROOT / relative
    original = path.read_bytes()
    digest = hashlib.sha256(original).hexdigest()
    text = original.decode()
    expected = 2 if name == "uncached-documents" else 1
    assert text.count(needle) == expected, (name, text.count(needle))
    log = OUT / (name + ".mutation.log")
    try:
        path.write_text(text.replace(needle, replacement))
        with log.open("w") as stream:
            run = subprocess.run(
                ["cargo", "test", "--release", "--locked", test, "--", "--nocapture"],
                cwd=ROOT / "app", env=env, stdout=stream, stderr=subprocess.STDOUT,
                timeout=900)
        output = log.read_text()
        rejected = (run.returncode != 0 and
                    ("test pane::styling::tests::" + test + " ... FAILED") in output and
                    "error[E" not in output)
        results.append(dict(mutation=name, test=test, exit=run.returncode,
                            rejected_by_behavior=rejected,
                            source_sha256=digest,
                            log_sha256=hashlib.sha256(log.read_bytes()).hexdigest()))
        print(json.dumps(results[-1]), flush=True)
        if not rejected:
            raise RuntimeError("Mutation survived or compilation failed: " + name)
    finally:
        path.write_bytes(original)
        assert hashlib.sha256(path.read_bytes()).hexdigest() == digest
        (OUT / "mutations.json").write_text(json.dumps(results, indent=2) + "\n")
print("All deliberate faults rejected; original production sources restored.", flush=True)
