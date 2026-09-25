"""Majority answer per cell across repeated core-fit runs on identical input.

    python3 docs/research/terminal-core/scripts/consensus.py jev/runs/core-fit-r1 jev/runs/core-fit-r2 jev/runs/core-fit-r3
        → jev/runs/core-fit/records.jsonl   the consensus the matrix reads
        → jev/runs/core-fit/agreement.json  how often the runs agreed, cell by cell

Why: on 2026-09-25 a second run on unchanged input changed 13 of 182 answers (7%), while the
two leading cores sat 0.003 apart in the matrix. One run cannot carry a ranking that close.
A cell takes the answer at least two of three runs gave; with no majority it is unknown,
which the matrix already excludes rather than scoring as zero.
"""
import json
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MUST = ["cell_contents", "zero_width", "wide_chars", "scrollback_read", "feed_loop", "replies_out", "placements_readable"]
SKIP = {"evidence_fields_present", "must_have_gaps"}

runs = [{json.loads(l)["item"]: json.loads(l) for l in (ROOT / r / "records.jsonl").read_text().splitlines() if l.strip()} for r in sys.argv[1:]]
out, agree, split = [], {}, []
total = unanimous = 0
for item in runs[0]:
    axes = {}
    for k in runs[0][item]["axes"]:
        if k in SKIP:
            continue
        votes = [r[item]["axes"].get(k) for r in runs]
        top, n = Counter(votes).most_common(1)[0]
        axes[k] = top if n * 2 > len(votes) else None
        total += 1
        unanimous += n == len(votes)
        if n != len(votes):
            split.append({"item": item, "q": k, "votes": votes, "taken": axes[k]})
    axes["evidence_fields_present"] = runs[0][item]["axes"].get("evidence_fields_present")
    known = [axes.get(m) for m in MUST]
    axes["must_have_gaps"] = sum(1 for a in known if a == "does_not") if all(a is not None for a in known) else None
    out.append({"item": item, "axes": axes, "consensus_of": len(runs)})
dest = ROOT / "jev" / "runs" / "core-fit"
dest.mkdir(parents=True, exist_ok=True)
(dest / "records.jsonl").write_text("".join(json.dumps(r) + "\n" for r in out))
(dest / "agreement.json").write_text(json.dumps({"runs": sys.argv[1:], "cells": total, "unanimous": unanimous, "split": split}, indent=1))
print(f"{unanimous} of {total} cells unanimous across {len(runs)} runs; {len(split)} split, "
      f"{sum(1 for s in split if s['taken'] is None)} with no majority")
