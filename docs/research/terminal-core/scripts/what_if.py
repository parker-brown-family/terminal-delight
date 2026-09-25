"""Re-rank the stored matrix under alternative weights, without asking Jev anything.

    python3 docs/research/terminal-core/scripts/what_if.py   → jev/what-if.json

Two questions the write-up needs answered from the data:
  without_jev      fit and API trajectory (Jev's two dimensions) weighted zero: does the top change?
  measured_weighted the bake-off and parse-speed columns weighted as first drafted (1.0 and 0.5),
                    now that wezterm-term has been measured too
"""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
M = json.loads((ROOT / "jev" / "matrix.json").read_text())
ROWS, W = M["rows"], M["weights"]
OPTS = [o for o in M["ranked"] if ROWS[o]["score"] is not None]


def rank(weights):
    out = []
    for o in OPTS:
        num = den = 0.0
        for k, w in weights.items():
            s = ROWS[o]["dims"][k]["score"]
            if s is None or w == 0:
                continue
            num += w * s
            den += w
        out.append((o, round(num / den, 3) if den else None))
    return sorted(out, key=lambda t: t[1] or -1, reverse=True)


cases = {
    "as_published": W,
    "without_jev": {**W, "fit": 0.0, "trajectory": 0.0},
    "measured_weighted": {**W, "pictures_measured": 1.0, "performance": 0.5},
}
res = {k: rank(w) for k, w in cases.items()}
(ROOT / "jev" / "what-if.json").write_text(json.dumps({"weights": cases, "ranked": res}, indent=1))
for k, r in res.items():
    print(f"{k:18s}", " > ".join(f"{o}({s})" for o, s in r[:5]))
