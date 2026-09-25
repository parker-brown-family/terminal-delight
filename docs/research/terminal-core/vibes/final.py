"""The final round, rio-vt against libghostty-vt, under Parker's sealed weights.

    python3 docs/research/terminal-core/vibes/final.py → vibes/final-2026-09-25.json

Agent fluency, the weight rank.py had to leave out, is now measured by the six-agent test
(fluency/results.json): the share of the four checks passed, averaged with token
efficiency (the fewest mean tokens divided by this core's). Every other dimension is
rank.py's. The flip line is how large the performance weight would have to become,
everything else unchanged, for libghostty-vt to tie.
"""
import importlib.util
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("rank", HERE / "rank.py")
rank = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rank)
F = json.loads((HERE.parent / "fluency/results.json").read_text())["agents"]

pair = {"rio-vt": "rio", "libghostty-vt": "ghostty"}
tok = {c: sum(a["tokens"] for k, a in F.items() if k.startswith(p)) / 3 for c, p in pair.items()}
passed = {c: sum(sum(a["checks"].values()) for k, a in F.items() if k.startswith(p)) / 12 for c, p in pair.items()}
best = min(tok.values())
fluency = {c: 0.5 * passed[c] + 0.5 * best / tok[c] for c in pair}

W = dict(rank.VIBES)


def dims(c):
    d = rank.dims(c)
    d["agent_fluency"] = fluency[c]
    return d


def score(c, w):
    num = den = 0.0
    for k, wt in w.items():
        s = dims(c)[k]
        if s is None or wt == 0:
            continue
        num += wt * s
        den += wt
    return num / den


scores = {c: round(score(c, W), 3) for c in pair}
# performance weight at which the two tie, others fixed
lo, hi = W["performance"], 1000.0
for _ in range(80):
    mid = (lo + hi) / 2
    w = {**W, "performance": mid}
    if score("libghostty-vt", w) < score("rio-vt", w):
        lo = mid
    else:
        hi = mid
out = {"fluency": {c: {"checks_passed": passed[c], "mean_tokens": round(tok[c]), "score": round(fluency[c], 3)} for c in pair},
       "dims": {c: {k: (round(v, 3) if v is not None else None) for k, v in dims(c).items()} for c in pair},
       "scores_under_sealed_weights": scores,
       "performance_weight_to_flip": round(hi, 1),
       "note": "weights from vibes/weights-2026-09-25.json; persistence of pictures weighs 0; the earlier fit, rewrite and API weights are not included"}
(HERE / "final-2026-09-25.json").write_text(json.dumps(out, indent=1))
print(json.dumps(out, indent=1))
