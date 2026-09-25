"""Score core-fit runs against ground truth on the three cores this session measured.

    python3 score_truth.py runs/core-fit-v1 runs/core-fit   → labels/core-fit.score.json
"""
import json
import sys
from pathlib import Path

HERE = Path(__file__).parent
truth = json.loads((HERE / "labels" / "core-fit.truth.json").read_text())["labels"]
PASS = {"provides", "provides_with_work"}
out = {}
for run in sys.argv[1:]:
    recs = {json.loads(l)["item"]: json.loads(l)["axes"] for l in (HERE / run / "records.jsonl").read_text().splitlines()}
    right = wrong = unknown = 0
    rows = []
    for core, reqs in truth.items():
        for r, (lab, why) in reqs.items():
            if lab in ("unsure", "note"):
                continue
            a = recs[core].get(r)
            if a is None or a == "evidence_silent":
                unknown += 1
                verdict = "unknown"
            else:
                got = "pass" if a in PASS else "fail"
                verdict = "right" if got == lab else "wrong"
                right += verdict == "right"
                wrong += verdict == "wrong"
            rows.append({"core": core, "req": r, "truth": lab, "jev": a, "verdict": verdict})
    n = right + wrong + unknown
    out[run] = {"scored": n, "right": right, "wrong": wrong, "unknown": unknown, "rows": rows}
    print(f"{run}: right {right}, wrong {wrong}, unknown {unknown} of {n}")
    for x in rows:
        if x["verdict"] != "right":
            print(f"    {x['verdict']:7s} {x['core']}.{x['req']}: truth {x['truth']}, Jev {x['jev']}")
(HERE / "labels" / "core-fit.score.json").write_text(json.dumps(out, indent=1))
