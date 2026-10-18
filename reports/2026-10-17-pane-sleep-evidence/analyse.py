"""Keep every matched pair; never exclude an observation based on its CPU result."""
import json
import statistics
import sys
from pathlib import Path

out = Path(sys.argv[1]).resolve() if len(sys.argv) == 2 else Path(__file__).resolve().parent
rows = [json.loads(line) for line in (out / "samples.jsonl").read_text().splitlines()]
assert len(rows) == 10
summary = {"method": "five simultaneous 10-second pairs per condition; thirty hidden synthetic agents; one visible quiet pane; CRT off; both windows1000x700 on the same monitor; identical production host binary; quiet precedes working",
           "conditions": {}, "whole_machine_speedup": None, "input_latency_change": None}
for condition in ("idle", "working"):
    pairs = sorted([r for r in rows if r["condition"] == condition], key=lambda r: r["pair"])
    assert [p["pair"] for p in pairs] == [1, 2, 3, 4, 5]
    baseline = [p["cpu"]["baseline_window"] for p in pairs]
    candidate = [p["cpu"]["candidate_window"] for p in pairs]
    a, b = statistics.median(baseline), statistics.median(candidate)
    summary["conditions"][condition] = {
        "baseline_window_cpu_median": a,
        "candidate_window_cpu_median": b,
        "saved_core_percentage_points": a - b,
        "median_paired_saved_core_percentage_points": statistics.median(x - y for x, y in zip(baseline, candidate)),
        "paired_saved_core_point_range": [min(x - y for x, y in zip(baseline, candidate)), max(x - y for x, y in zip(baseline, candidate))],
        "relative_reduction_percent": 100 * (a - b) / a if a > 0 else None,
        "baseline_host_cpu_median": statistics.median(p["cpu"]["baseline_host"] for p in pairs),
        "candidate_host_cpu_median": statistics.median(p["cpu"]["candidate_host"] for p in pairs),
        "pairs_with_lower_window_cpu": sum(y < x for x, y in zip(baseline, candidate)),
        "pairs": [{"pair": p["pair"], "baseline": x, "candidate": y,
                   "relative_reduction_percent": 100 * (x - y) / x if x > 0 else None}
                  for p, x, y in zip(pairs, baseline, candidate)],
    }
(out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary, indent=2))
