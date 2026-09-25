"""Pairwise answer changes between core-fit runs on identical input, against the question-order test.

    python3 docs/research/terminal-core/scripts/jitter.py   → jev/jitter.json
"""
import itertools
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SKIP = {"evidence_fields_present", "must_have_gaps"}


def load(r):
    return {json.loads(l)["item"]: json.loads(l)["axes"] for l in (ROOT / "jev/runs" / r / "records.jsonl").read_text().splitlines() if l.strip()}


def diff(a, b):
    n = c = 0
    for item in a:
        for k, v in a[item].items():
            if k in SKIP:
                continue
            n += 1
            c += v != b[item].get(k)
    return c, n


pairs = {}
for x, y in itertools.combinations(["core-fit-r1", "core-fit-r2", "core-fit-r3"], 2):
    pairs[f"{x} vs {y}"] = diff(load(x), load(y))
order = json.loads((ROOT / "jev/order-check.json").read_text())
out = {"identical_input": {k: {"changed": c, "of": n, "share": round(c / n, 3)} for k, (c, n) in pairs.items()},
       "order_reversed": order}
(ROOT / "jev/jitter.json").write_text(json.dumps(out, indent=1))
for k, (c, n) in pairs.items():
    print(f"{k}: {c} of {n} ({100 * c / n:.1f}%)")
print("order check:", {k: order[k] for k in list(order)[:4]})
