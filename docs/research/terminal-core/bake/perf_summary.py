"""Summarise the twenty-pane runs: python3 perf_summary.py → results/perf/summary.json"""
import json
import statistics
from pathlib import Path

here = Path(__file__).parent / "results" / "perf"
out = {}
for core in ("alacritty", "rio", "ghostty", "wezterm"):
    runs = [json.loads((here / f"{core}-{r}.json").read_text()) for r in (1, 2, 3)]
    def med(key):
        vals = [r[key] for r in runs if r.get(key) is not None]
        return statistics.median(vals) if vals else None
    out[core] = {
        "core": runs[0]["core"],
        "text_ms_median": med("text_ms_median"),
        "text_ms_by_round": [r["text_ms_median"] for r in runs],
        "text_mb_per_s": med("text_mb_per_s"),
        "kb_per_pane_empty": med("kb_per_pane_empty"),
        "kb_per_pane_full": med("kb_per_pane_full"),
        "kb_per_pane_full_by_round": [r["kb_per_pane_full"] for r in runs],
        "rows_kept": [r["rows_kept"] for r in runs],
        "high_water_mb": med_hw if (med_hw := statistics.median([r["rss_kb"]["high_water"] / 1024 for r in runs if r["rss_kb"]["high_water"]])) else None,
    }
(here / "summary.json").write_text(json.dumps(out, indent=1))
print(f"{'core':12s} {'text ms':>9s} {'MB/s':>7s} {'MB/pane empty':>14s} {'MB/pane full':>13s} {'20 panes MB':>12s}  rows kept")
for k, v in out.items():
    e, f = v["kb_per_pane_empty"], v["kb_per_pane_full"]
    print(f"{k:12s} {v['text_ms_median']:9.1f} {v['text_mb_per_s']:7.1f} {e / 1024 if e is not None else float('nan'):14.2f} "
          f"{f / 1024 if f is not None else float('nan'):13.1f} {f * 20 / 1024 if f is not None else float('nan'):12.0f}  {v['rows_kept']}  rounds {v['text_ms_by_round']}")
