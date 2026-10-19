"""Describe identical-binary variation; retain every control observation."""
import hashlib
import json
import shutil
import statistics
from pathlib import Path

out = Path(__file__).resolve().parent
source = Path("/tmp/td-sleep-aa")
rows = [json.loads(line) for line in (source / "samples.jsonl").read_text().splitlines()]
assert len(rows) == 4
builds = json.loads((out / "build-pair.json").read_text())
hashes = (source / "binaries.sha256").read_text().splitlines()
assert len(hashes) == 3 and len({line.split()[0] for line in hashes}) == 1
assert hashes[0].split()[0] == builds["baseline_sha256"]
summary = {"design": "two simultaneous ten-second identical-baseline-binary pairs per condition, before A/B; descriptive range only, not a statistical confidence interval", "conditions": {}}
for condition in ("idle", "working"):
    pairs = sorted([r for r in rows if r["condition"] == condition], key=lambda r: r["pair"])
    assert [p["pair"] for p in pairs] == [1, 2]
    differences = [p["cpu"]["baseline_window"] - p["cpu"]["candidate_window"] for p in pairs]
    summary["conditions"][condition] = {
        "signed_core_point_differences": differences,
        "largest_absolute_core_point_difference": max(map(abs, differences)),
        "baseline_window_cpu_median": statistics.median(p["cpu"]["baseline_window"] for p in pairs),
        "other_identical_window_cpu_median": statistics.median(p["cpu"]["candidate_window"] for p in pairs),
    }
dest = out / "identical-control"
dest.mkdir(exist_ok=False)
for name in ("samples.jsonl", "binaries.sha256", "geometry.json", "baseline-panes.json", "candidate-panes.json", "baseline-layout.toml", "candidate-layout.toml"):
    shutil.copy2(source / name, dest / name)
(out / "control-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary, indent=2))
