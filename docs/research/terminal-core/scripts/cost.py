"""What every Jev run in this decision actually cost, read from its receipts.

    python3 docs/research/terminal-core/scripts/cost.py   → jev/cost.json

Each receipt carries the usage OpenRouter returned for that request, so these are
measured totals, not the spec's predictions. Probe runs write no receipts and are
counted separately by probes.py.
"""
import json
import statistics
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
out = {}
for run in sorted((ROOT / "jev" / "runs").iterdir()):
    rec = run / "receipts.jsonl"
    if not rec.exists():
        continue
    rows = [json.loads(l) for l in rec.read_text().splitlines() if l.strip()]
    usage = [r.get("usage") or {} for r in rows]
    lat = [r["latency_ms"] for r in rows if r.get("latency_ms") is not None]
    out[run.name] = {
        "requests": len(rows),
        "items": len({r["item"] for r in rows}),
        "questions": sum(len(r.get("answers") or {}) for r in rows),
        "input_tokens": sum(u.get("input_tokens", 0) for u in usage),
        "output_tokens": sum(u.get("output_tokens", 0) for u in usage),
        "cost_usd": round(sum(u.get("cost", 0) for u in usage), 6),
        "p50_ms": round(statistics.median(lat), 1) if lat else None,
        "model": sorted({r.get("model") for r in rows if r.get("model")}),
        "spec_version": sorted({r.get("version") for r in rows if r.get("version") is not None}),
    }
total = {k: sum(v[k] for v in out.values()) for k in ("requests", "questions", "input_tokens", "output_tokens")}
total["cost_usd"] = round(sum(v["cost_usd"] for v in out.values()), 6)
(ROOT / "jev" / "cost.json").write_text(json.dumps({"runs": out, "total": total}, indent=1))
for k, v in out.items():
    print(f"{k:28s} {v['requests']:4d} req {v['questions']:5d} q {v['input_tokens'] + v['output_tokens']:8d} tok ${v['cost_usd']:.5f} p50 {v['p50_ms']} ms")
print("total", total)
