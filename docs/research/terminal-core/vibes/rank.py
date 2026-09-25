"""Rank the four measured cores under Parker's vibe weights, from stored answers and measurements.

    python3 docs/research/terminal-core/vibes/rank.py → vibes/rank-2026-09-25.json

Only the four cores the bake-off and the twenty-pane runs measured can be scored on
performance, so only they are ranked. Agent fluency carries weight 4 and has no
measurement yet: it is left out of the arithmetic and reported as the open hole, never
scored as zero.
"""
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
W = json.loads((HERE / "weights-2026-09-25.json").read_text())["translation"]["weights"]
M = json.loads((ROOT / "jev/matrix.json").read_text())["rows"]
P = json.loads((ROOT / "bake/results/perf/summary.json").read_text())

CORES = {"alacritty-own-loop": "alacritty", "rio-vt": "rio", "libghostty-vt": "ghostty", "wezterm-term": "wezterm"}

# Published and used unpatched, read from the dossiers and the bake-off; the reason sits beside each.
PUBLISHED = {
    "alacritty-own-loop": (1.0, "crates.io, used as-is; TD would write the Kitty handling around it, not inside it"),
    "rio-vt": (1.0, "crates.io, used as-is with its graphics feature"),
    "libghostty-vt": (0.75, "crates.io bindings 0.2.1 used as-is; the build clones Ghostty's source at a pinned commit and needs Zig"),
    "wezterm-term": (0.25, "never published: a git dependency on the wezterm tree, plus a patch to add placeholder pictures"),
}

ms = {c: P[k]["text_ms_median"] for c, k in CORES.items()}
kb = {c: P[k]["kb_per_pane_full"] for c, k in CORES.items()}
best_ms, best_kb = min(ms.values()), min(kb.values())


def dims(c):
    d = M[c]["dims"]
    return {
        "performance": 0.5 * (best_ms / ms[c]) + 0.5 * (best_kb / kb[c]),
        "agent_fluency": None,
        "published_unpatched": PUBLISHED[c][0],
        "pictures": d["pictures"]["score"],
        "maintainers": d["bus_factor"]["score"],
        "other_picture_protocols": d["other_protocols"]["score"],
        "build_toolchain": d["build"]["score"],
        "persistence_of_pictures": None,
        # not revisited by Parker; earlier weights stand in the second ranking only
        "fit": d["fit"]["score"], "td_work": d["td_work"]["score"], "trajectory": d["trajectory"]["score"],
    }


VIBES = {k: v["weight"] for k, v in W.items()}
EARLIER = {"fit": 3.0, "td_work": 2.5, "trajectory": 2.0}


def score(c, weights):
    num = den = 0.0
    for k, w in weights.items():
        s = dims(c)[k]
        if s is None or w == 0:
            continue
        num += w * s
        den += w
    return round(num / den, 3), round(den / sum(weights.values()), 3)


out = {"measured": {c: {"text_ms": ms[c], "kb_per_pane_full": kb[c]} for c in CORES},
       "dims": {c: dims(c) for c in CORES},
       "published_why": {c: PUBLISHED[c][1] for c in CORES},
       "vibes_only": sorted(((c, *score(c, VIBES)) for c in CORES), key=lambda t: -t[1]),
       "vibes_plus_earlier": sorted(((c, *score(c, {**VIBES, **EARLIER})) for c in CORES), key=lambda t: -t[1])}
(HERE / "rank-2026-09-25.json").write_text(json.dumps(out, indent=1))
for name in ("vibes_only", "vibes_plus_earlier"):
    print(name, " > ".join(f"{c} {s:.2f}" for c, s, cov in out[name]), f"(weight covered: {out[name][0][2]:.0%})")
for c in CORES:
    print(f"  {c:20s}", {k: (round(v, 2) if v is not None else None) for k, v in dims(c).items()})
