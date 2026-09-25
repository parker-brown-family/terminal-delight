"""The falsifier core-fit wrote before its first call: does the order of the thirteen batched
questions change the answers? (Memory from an earlier session: a batched Score anchored on the
order it was handed. These are Choices, and TypeSafe documents batched questions as
independent; this measures it instead of trusting either.)

    python3 docs/research/terminal-core/scripts/order_check.py <forward run> <reversed run>

The reversed run comes from a copy of the spec with the `assess` questions in reverse order
(specs/core-fit-reversed/, written by this script with --make).
"""
import json
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "jev"
if sys.argv[1] == "--make":
    src, dst = ROOT / "specs/core-fit", ROOT / "specs/core-fit-reversed"
    if dst.exists():
        shutil.rmtree(dst)
    shutil.copytree(src, dst)
    p = dst / "core-fit.jds.json"
    spec = json.loads(p.read_text())
    spec["id"] = "core-fit"  # same spec, reordered
    q = spec["calls"][0]["questions"]
    spec["calls"][0]["questions"] = dict(reversed(list(q.items())))
    p.write_text(json.dumps(spec, indent=2, ensure_ascii=False))
    print("wrote", p)
    sys.exit(0)

fwd = {json.loads(l)["item"]: json.loads(l)["axes"] for l in (Path(sys.argv[1]) / "records.jsonl").read_text().splitlines()}
rev = {json.loads(l)["item"]: json.loads(l)["axes"] for l in (Path(sys.argv[2]) / "records.jsonl").read_text().splitlines()}
reqs = ["cell_contents", "zero_width", "underline_colour", "wide_chars", "scrollback_read", "damage", "snapshot",
        "feed_loop", "replies_out", "threading", "placements_readable", "placeholders", "media_policy"]
n = changed = 0
flips = []
for item in fwd:
    for r in reqs:
        n += 1
        if fwd[item].get(r) != rev.get(item, {}).get(r):
            changed += 1
            flips.append(f"{item}.{r}: {fwd[item].get(r)} → {rev.get(item, {}).get(r)}")
out = {"answers": n, "changed": changed, "share": changed / n, "flips": flips,
       "falsifier": "more than 10% changed would mean the batch anchors on order", "falsified": changed / n > 0.10}
(ROOT / "order-check.json").write_text(json.dumps(out, indent=1))
print(f"{changed}/{n} answers changed when the question order was reversed ({100 * changed / n:.1f}%)")
for f in flips:
    print("  ", f)
