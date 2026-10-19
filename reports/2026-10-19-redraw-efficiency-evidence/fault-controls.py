"""Disable each optimization at its production site; require both behavioral failures."""
from pathlib import Path
import hashlib
import json
import os
import subprocess

root = Path(__file__).resolve().parents[2]
out = Path(__file__).resolve().parent
pane = root / "app/src/pane.rs"
term = root / "app/src/term.rs"
originals = {p: p.read_bytes() for p in (pane, term)}
try:
    s = pane.read_text()
    needle = ".cached(gpui::StyleRefinement::default().w_full().h(px(height)))"
    assert s.count(needle) == 1
    pane.write_text(s.replace(needle, ""))
    s = term.read_text()
    needle = "if matches!(event, TermEvent::Wakeup) && !self.screen_wakeups.load(Ordering::Relaxed)"
    assert s.count(needle) == 1
    term.write_text(s.replace(needle, "if false && matches!(event, TermEvent::Wakeup) && !self.screen_wakeups.load(Ordering::Relaxed)"))
    with (out / "fault-controls.log").open("wb") as log:
        result = subprocess.run(
            ["cargo", "test", "--manifest-path", "app/Cargo.toml", "--release", "--locked",
             "--bin", "terminal-delight", "pane::", "--", "--nocapture"],
            cwd=root, env={**os.environ, "CARGO_TARGET_DIR": "/home/parker/Work/terminal-delight/app/target"},
            stdout=log, stderr=subprocess.STDOUT, timeout=900,
        )
    text = (out / "fault-controls.log").read_text()
    required = ["quiet grid rebuilt on parent frames", "hidden output dispatched screen events"]
    proof = {"exit": result.returncode, "expected_failure_messages": {s: s in text for s in required},
             "method": "both production optimizations disabled together; independent tests must each fail at their efficiency assertion"}
    assert result.returncode != 0 and all(proof["expected_failure_messages"].values()), proof
finally:
    for p, data in originals.items():
        p.write_bytes(data)
    assert all(p.read_bytes() == data for p, data in originals.items())
proof["restored_sha256"] = {str(p.relative_to(root)): hashlib.sha256(data).hexdigest() for p, data in originals.items()}
(out / "fault-controls.json").write_text(json.dumps(proof, indent=2) + "\n")
print(json.dumps(proof))
