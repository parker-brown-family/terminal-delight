"""Score claim-check v3 against the hand labels: exact agreement, a confusion table, and
agreement after collapsing to what the page shows (source-backed, unsupported, contradicted).

    python3 score_labels.py   → labels/claim-check-v3.score.json
"""
import json
from collections import Counter
from pathlib import Path

HERE = Path(__file__).parent
sample = json.loads((HERE / "labels" / "sample-claim-check-v3.json").read_text())
labels = json.loads((HERE / "labels" / "claim-check-v3.labels.json").read_text())["labels"]
OPTS = ["states", "states_part", "implies", "silent", "contradicts"]
COLLAPSE = {"states": "backed", "implies": "backed", "states_part": "partly", "silent": "unsupported", "contradicts": "contradicted"}

pairs = [(labels[str(s["n"])][0], s["jev"], s) for s in sample]
exact = sum(1 for l, j, _ in pairs if l == j)
coll = sum(1 for l, j, _ in pairs if COLLAPSE[l] == COLLAPSE[j])
conf = Counter((l, j) for l, j, _ in pairs)
table = {l: {j: conf.get((l, j), 0) for j in OPTS} for l in OPTS}
disagree = [{"item": s["item"], "label": l, "jev": j, "why": labels[str(s["n"])][1]} for l, j, s in pairs if l != j]
# Where Jev said 'silent', how often did the labeller agree the quote was unrelated?
jev_silent = [(l, j) for l, j, _ in pairs if j == "silent"]
out = {"n": len(pairs), "exact": exact, "collapsed": coll,
       "exact_share": exact / len(pairs), "collapsed_share": coll / len(pairs),
       "jev_silent_labelled_silent": sum(1 for l, j in jev_silent if l == "silent"), "jev_silent_n": len(jev_silent),
       "confusion_label_by_jev": table, "disagreements": disagree}
(HERE / "labels" / "claim-check-v3.score.json").write_text(json.dumps(out, indent=1))
print(f"exact {exact}/{len(pairs)}  collapsed {coll}/{len(pairs)}")
print("rows = label, cols = Jev:", OPTS)
for l in OPTS:
    print(f"  {l:12s}", [table[l][j] for j in OPTS])
print(f"Jev 'silent' confirmed by the labeller: {out['jev_silent_labelled_silent']}/{out['jev_silent_n']}")
for d in disagree:
    print(f"  {d['item']}: label {d['label']}, Jev {d['jev']}")
