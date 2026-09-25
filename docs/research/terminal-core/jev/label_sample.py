"""Draw the stratified evaluation sample for claim-check v3: eight items per answer, seed 11.

    python3 label_sample.py <run dir>   → prints the items; labels go in labels/claim-check-v3.jsonl

The labeller reads the headline and the quote and picks the option the spec's own
criteria describe, without looking at Jev's answer (it is printed last, after a gap,
and the labeller records a label before scrolling to it).
"""
import json
import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent / "specs" / "claim-check"))
from builders import headline  # noqa: E402

run = Path(sys.argv[1])
items = {c["id"]: c for c in json.loads((Path(__file__).parent / "items" / "claims.json").read_text())}
recs = [json.loads(l) for l in (run / "records.jsonl").read_text().splitlines()]
rng = random.Random(11)
sample = []
for ans in ("states", "states_part", "implies", "silent", "contradicts"):
    pool = sorted([r for r in recs if r["axes"].get("support") == ans], key=lambda r: r["item"])
    rng.shuffle(pool)
    sample += [(r["item"], ans) for r in pool[:8]]
rng.shuffle(sample)
out = []
for i, (iid, ans) in enumerate(sample):
    it = items[iid]
    out.append({"n": i, "item": iid, "field": it["field_means"], "headline": headline(it["value"]), "quote": it["quote"][:400], "jev": ans})
(Path(__file__).parent / "labels").mkdir(exist_ok=True)
(Path(__file__).parent / "labels" / "sample-claim-check-v3.json").write_text(json.dumps(out, indent=1, ensure_ascii=False))
for o in out:
    print(f"[{o['n']}] {o['item']}\n    field: {o['field']}\n    headline: {o['headline']!r}\n    quote: {o['quote'][:280]!r}\n")
