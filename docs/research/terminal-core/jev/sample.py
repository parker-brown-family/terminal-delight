"""Print a seeded random sample of claim-check items with a given support answer:
the headline Jev saw, the quote, and the answer, for reading by hand.

    python3 sample.py <run dir> <support answer> <n> [seed]
"""
import json
import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent / "specs" / "claim-check"))
from builders import headline  # noqa: E402

run, want, n = sys.argv[1], sys.argv[2], int(sys.argv[3])
seed = int(sys.argv[4]) if len(sys.argv) > 4 else 1
items = {c["id"]: c for c in json.loads((Path(__file__).parent / "items" / "claims.json").read_text())}
recs = [json.loads(l) for l in (Path(run) / "records.jsonl").read_text().splitlines()]
pick = [r for r in recs if r["axes"].get("support") == want]
random.Random(seed).shuffle(pick)
for r in pick[:n]:
    it = items[r["item"]]
    print(f"### {r['item']}  → {r['axes'].get('verdict')}")
    print(f"  headline: {headline(it['value'])!r}")
    print(f"  quote:    {it['quote'][:300]!r}")
    print()
