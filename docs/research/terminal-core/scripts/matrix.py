"""The decision matrix: every candidate across every dimension, ranked by code.

    python3 docs/research/terminal-core/scripts/matrix.py <claim-check run> <core-fit run>
        → jev/matrix.json

Jev never ranks. It supplied two kinds of answer upstream: whether each fact's quote
states it (claim-check), and, per core, how its interface meets each of TD's needs and
how settled that interface is (core-fit). Everything below is code over stored answers
and gathered facts, so a change of weights re-ranks without asking Jev again.

Two rankings are produced on purpose:
  - the BASELINE: a plain rule anyone could write before asking a model anything —
    licence first, then Kitty coverage, then a cargo-only build, then activity;
  - the MATRIX: weighted dimensions, including Jev's judgments, with unknowns excluded
    and reported as coverage rather than scored as zero.
If the two agree on the top, the model added explanation, not the decision; the page says so.

Weights are the assembling agent's defaults (drawn amber on the page), and the sensitivity
pass samples 10,000 random weightings to show how much the order depends on them.
"""
from __future__ import annotations

import json
import random
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
JEV = ROOT / "jev"

OPTIONS = ["alacritty-own-loop", "alacritty-patched-vte", "alacritty-graphics-fork", "rio-vt", "wezterm-term",
           "vt100-rust", "td-own-core", "libghostty-vt", "libvterm", "contour-vtbackend", "kitty-core", "ghostty-shadow"]
REFERENCES = ["zellij-grid", "zmx"]

# How much TD writes to reach pictures on each road: the assembling agent's estimate, from
# each dossier's td_migration inference and TD's seven alacritty-importing files. S..XXL.
TD_WORK = {
    "alacritty-own-loop": ("L", "TD's own read loop at three call sites, plus the whole Kitty implementation (store, placements, placeholders, deletes, queries)"),
    "alacritty-patched-vte": ("L", "an APC patch to vte (one small commit exists in Warp's fork and a lapsed upstream PR) plus the Kitty implementation in TD"),
    "alacritty-graphics-fork": ("XL", "a git dependency on a one-person fork that brings Sixel only; Kitty still has to be written, and its per-cell pictures leave the Term for a renderer TD's host does not have"),
    "rio-vt": ("L", "rewrite the seven files that read alacritty's grid onto rio-vt's Crosswords; pictures, placeholders and replies come with it"),
    "wezterm-term": ("XL", "the seven files rewritten onto an unpublished git dependency whose releases stopped in 2024 though its commits resumed in 2026, and a fork to add placeholders, which the bake-off showed it prints as text"),
    "vt100-rust": ("XL", "a dormant core with no pictures and a visible-rows-only snapshot; TD would still write every picture feature"),
    "td-own-core": ("XXL", "a whole emulator: grid, scrollback, reflow, modes and pictures; Zellij's grid alone is 6,298 lines"),
    "libghostty-vt": ("L", "the seven files rewritten across a C boundary, a Zig toolchain in TD's build, single-thread handles, and a PNG decoder TD supplies"),
    "libvterm": ("XL", "C bindings last published in 2016, a dormant upstream and no pictures"),
    "contour-vtbackend": ("XXL", "a C++23 bridge TD writes itself, to a core with no C interface or install rule"),
    "kitty-core": ("n/a", "GPL-3.0: not usable in an MIT product"),
    "ghostty-shadow": ("L", "libghostty-vt beside alacritty, plus reconciliation whenever the two cursors disagree after a placement"),
}
WORK_SCORE = {"S": 1.0, "M": 0.75, "L": 0.5, "XL": 0.25, "XXL": 0.1}

# Counts the agents wrote as prose that the first-integer rule would misread; hand-read from
# each dossier's own value and labelled as such on the page.
MAINTAINERS_OVERRIDE = {
    "alacritty-patched-vte": (1, "vte upstream is one maintainer with three commits in a year"),
    "alacritty-graphics-fork": (1, "ayosec alone carries the graphics layer"),
    "ghostty-shadow": (1, "each of its three upstreams is carried by one or two people"),
}

BAKE = {  # measured in the bake-off: recordings whose pictures the core placed, of the six reproducible
    "alacritty-own-loop": (0, 6, "alacritty_terminal 0.26 as it runs in TD today: every picture dropped"),
    "rio-vt": (6, 6, "rio-vt 0.5.28"),
    "libghostty-vt": (6, 6, "libghostty-vt 0.2.1 with an embedder PNG decoder"),
    "ghostty-shadow": (6, 6, "its picture half is libghostty-vt; the cursor divergence is not measured by the bake-off"),
    "wezterm-term": (4, 6, "wezterm-term git b09b56c29c: every direct placement right; both placeholder programs wrong, because it ignores U=1, draws at the cursor, and the placeholder characters land as text"),
}
# bake/results/parse-times.json: median of three runs over the same 13 MB four-frame mpv recording
PARSE_MS = {"alacritty-own-loop": 39.8, "rio-vt": 40.7, "libghostty-vt": 19.3, "wezterm-term": 109.3}

TRAJECTORY_SCORE = {"stable_contract": 1.0, "active_pre_1_0": 0.6, "owned_by_td": 0.7, "declared_unstable": 0.3, "dormant": 0.1}
FIT_WEIGHTS = {"cell_contents": 3, "zero_width": 3, "underline_colour": 2, "wide_chars": 3, "scrollback_read": 3,
               "damage": 2, "snapshot": 2, "feed_loop": 3, "replies_out": 3, "threading": 2,
               "placements_readable": 3, "placeholders": 2, "media_policy": 1}
FIT_VALUE = {"provides": 1.0, "provides_with_work": 0.5, "does_not": 0.0}

# The build each road asks of TD's toolchain, hand-read from every dossier's value. A keyword
# rule scored this until 2026-09-25 and misread three of twelve: "no C compiler, cmake or system
# library" (wezterm-term) matched on "cmake", and rio-vt's single C++11 file through the cc crate
# scored as a CMake build.
BUILD = {
    "alacritty-own-loop": (1.0, "cargo only"),
    "alacritty-patched-vte": (0.9, "cargo, plus a git dependency"),
    "alacritty-graphics-fork": (0.9, "cargo, plus a git dependency"),
    "rio-vt": (0.7, "cargo, plus a C++11 compiler through the cc crate (simdutf); built on this box by the bake-off"),
    "wezterm-term": (0.9, "cargo, plus the wezterm git workspace"),
    "vt100-rust": (1.0, "cargo only"),
    "td-own-core": (1.0, "cargo only"),
    "libghostty-vt": (0.4, "a Zig 0.15 toolchain, and a git clone of ghostty at build time"),
    "libvterm": (0.7, "a C99 compiler through the cc crate"),
    "contour-vtbackend": (0.15, "a C++23 compiler and CMake 3.25, with network at configure time"),
    "ghostty-shadow": (0.4, "a Zig 0.15 toolchain, and a git clone of ghostty at build time"),
}

# Weighted dimensions only. pictures_measured and performance are shown but not weighted: they
# were measured for four and three of the twelve options, and with unknowns excluded rather than
# scored as zero, a weighted dimension measured for a third of the field lets the unmeasured
# two-thirds skip it. On 2026-09-25 that let wezterm-term, never run, top 37% of random weightings.
DEFAULT_WEIGHTS = {"pictures": 3.0, "fit": 3.0, "td_work": 2.5, "trajectory": 2.0, "bus_factor": 1.5,
                   "build": 1.5, "vt_features": 1.0, "other_protocols": 0.5}


def first_int(s):
    m = re.search(r"\d[\d,]*", str(s or ""))
    return int(m.group(0).replace(",", "")) if m else None


def yes_no(s):
    t = str(s or "").strip().lower()
    if t.startswith("yes"):
        return 1.0
    if t.startswith("partial"):
        return 0.5
    if t.startswith("no"):
        return 0.0
    return None


def build_score(s):
    t = str(s or "").lower()
    if not t or "not applicable" in t:
        return None, "not applicable"
    if "c++" in t or "cmake" in t:
        return 0.15, "C++ toolchain and CMake"
    if "zig" in t:
        return 0.4, "a Zig toolchain"
    if "c99" in t or "cc crate" in t or "c compiler" in t:
        return 0.7, "a C compiler"
    if "cargo only" in t:
        return (0.9, "cargo, plus a git dependency") if "git" in t else (1.0, "cargo only")
    return None, "unclassified"


def vt_features(s):
    yes = no = unk = 0
    for line in re.split(r"[;\n]", str(s or "")):
        l = line.lower()
        if not l.strip():
            continue
        if re.search(r":\s*yes|\byes\b", l):
            yes += 1
        elif re.search(r":\s*no\b|\bno\b", l):
            no += 1
        elif "unknown" in l:
            unk += 1
    return (yes / (yes + no) if yes + no else None), yes, no, unk


def load_records(run_dir):
    recs = {}
    for line in (Path(run_dir) / "records.jsonl").read_text().splitlines():
        r = json.loads(line)
        recs[r["item"]] = r
    return recs


def main(claim_run, fit_run):
    dossiers = {d["id"]: d for d in json.loads((JEV / "dossiers.json").read_text())["dossiers"]}
    claims = load_records(claim_run)
    fits = load_records(fit_run)
    rows = {}
    for oid in OPTIONS + REFERENCES:
        d = dossiers[oid]
        f = d["fields"]
        v = lambda k: (f.get(k) or {}).get("value")
        dims, facts = {}, {}
        # licence (a gate, not a weight)
        gate = v("licence_gate")
        facts["licence"] = {"value": v("license"), "gate": gate}
        # pictures from the core today
        cov = v("kitty_coverage")
        n = first_int(cov) if cov and "/" in str(cov) else None
        dims["pictures"] = {"score": (n / 14) if n is not None else None, "shown": cov, "source": "code over the agents' kitty_tags"}
        b = BAKE.get(oid)
        dims["pictures_measured"] = {"score": (b[0] / b[1]) if b else None, "shown": f"{b[0]}/{b[1]}" if b else "not run",
                                     "source": "measured: the bake-off" if b else "not run", "note": b[2] if b else None}
        so, it = yes_no(v("sixel")), yes_no(v("iterm2_images"))
        known = [x for x in (so, it) if x is not None]
        dims["other_protocols"] = {"score": sum(known) / len(known) if known else None,
                                   "shown": f"Sixel {v('sixel') and str(v('sixel')).split(' ')[0]}, iTerm2 {v('iterm2_images') and str(v('iterm2_images')).split(' ')[0]}",
                                   "source": "code over the agents' values"}
        # fit (Jev: core-fit.assess)
        fr = fits.get(oid, {})
        ax = fr.get("axes") or {}
        num = den = 0.0
        silent = []
        answers = {}
        for rid, w in FIT_WEIGHTS.items():
            a = ax.get(rid)
            answers[rid] = a
            if a in FIT_VALUE:
                num += w * FIT_VALUE[a]
                den += w
            else:
                silent.append(rid)
        dims["fit"] = {"score": (num / den) if den else None, "coverage": den / sum(FIT_WEIGHTS.values()),
                       "shown": f"{num:.1f}/{den:.0f}" if den else "not established", "answers": answers,
                       "must_have_gaps": ax.get("must_have_gaps"), "source": "Jev (core-fit.assess), combined by code"}
        traj = ax.get("api_trajectory")
        dims["trajectory"] = {"score": TRAJECTORY_SCORE.get(traj), "shown": traj or "not established",
                              "source": "Jev (core-fit.maturity)"}
        # bus factor
        if oid in MAINTAINERS_OVERRIDE:
            m, why = MAINTAINERS_OVERRIDE[oid]
            msrc = f"hand-read: {why}"
        else:
            m, msrc = first_int(v("maintainers")), "code: the first count in the agents' value"
        dims["bus_factor"] = {"score": None if m is None else (1.0 if m >= 3 else 0.6 if m == 2 else 0.3 if m == 1 else 0.0),
                              "shown": "unknown" if m is None else f"{m}", "source": msrc}
        bs, bshown = BUILD.get(oid, (None, "not applicable"))
        dims["build"] = {"score": bs, "shown": bshown, "source": "hand-read from the agents' value"}
        vs, yes, no, unk = vt_features(v("vt_features"))
        dims["vt_features"] = {"score": vs, "shown": f"{yes} yes, {no} no, {unk} unknown", "source": "code over the agents' value"}
        w = TD_WORK.get(oid)
        dims["td_work"] = {"score": WORK_SCORE.get(w[0]) if w else None, "shown": w[0] if w else "reference", "note": w[1] if w else None,
                           "source": "estimate: the assembling agent"}
        ms = PARSE_MS.get(oid)
        dims["performance"] = {"score": (min(PARSE_MS.values()) / ms) if ms else None, "shown": f"{ms} ms" if ms else "not measured",
                               "source": "measured: one parse of a 13 MB mpv recording" if ms else "not measured"}
        # evidence quality (not weighted: it is about the research, not the core)
        verdicts = [r["axes"].get("verdict") for k, r in claims.items() if k.startswith(oid + "/")]
        counts = {x: verdicts.count(x) for x in ("supported", "inferred", "overclaims", "unsupported", "human", None)}
        facts["evidence"] = {"claims": len(verdicts), **{str(k): v for k, v in counts.items()}}
        facts["risk"] = v("biggest_risk")
        facts["migration"] = v("td_migration")
        rows[oid] = {"id": oid, "label": d["label"], "group": d.get("group"), "reference": oid in REFERENCES,
                     "dims": dims, "facts": facts}

    def score(row, weights):
        if row["facts"]["licence"]["gate"] == "blocked":
            return None, 0.0
        num = den = 0.0
        for k, w in weights.items():
            s = row["dims"][k]["score"]
            if s is None:
                continue
            num += w * s
            den += w
        return (num / den if den else None), den / sum(weights.values())

    ranked = []
    for oid in OPTIONS:
        s, c = score(rows[oid], DEFAULT_WEIGHTS)
        rows[oid]["score"], rows[oid]["coverage"] = s, c
        ranked.append(oid)
    ranked.sort(key=lambda o: (rows[o]["score"] is not None, rows[o]["score"] or 0), reverse=True)

    # the baseline: a rule written without any model
    def baseline_key(o):
        r = rows[o]
        lic = 0 if r["facts"]["licence"]["gate"] == "blocked" else 1
        pics = r["dims"]["pictures"]["score"] or 0
        build = r["dims"]["build"]["score"] or 0
        active = first_int((dossiers[o]["fields"].get("releases_last_12mo") or {}).get("value")) or 0
        return (lic, round(pics, 3), build, active)
    baseline = sorted(OPTIONS, key=baseline_key, reverse=True)

    # sensitivity: random weightings (Dirichlet, alpha = 1) over the same dimensions
    rng = random.Random(20260925)
    top1 = {o: 0 for o in OPTIONS}
    top3 = {o: 0 for o in OPTIONS}
    N = 10_000
    keys = list(DEFAULT_WEIGHTS)
    for _ in range(N):
        g = [rng.gammavariate(1.0, 1.0) for _ in keys]
        wts = dict(zip(keys, g))
        order = sorted(OPTIONS, key=lambda o: (score(rows[o], wts)[0] or -1), reverse=True)
        top1[order[0]] += 1
        for o in order[:3]:
            top3[o] += 1
    # with pictures weighted zero: does the order hinge on them?
    no_pics = {**DEFAULT_WEIGHTS, "pictures": 0.0}
    order_no_pics = sorted(OPTIONS, key=lambda o: (score(rows[o], no_pics)[0] or -1), reverse=True)

    out = {"weights": DEFAULT_WEIGHTS, "rows": rows, "ranked": ranked, "baseline": baseline,
           "sensitivity": {"samples": N, "top1": {o: top1[o] / N for o in OPTIONS}, "top3": {o: top3[o] / N for o in OPTIONS}},
           "order_without_pictures": order_no_pics}
    (JEV / "matrix.json").write_text(json.dumps(out, indent=1, ensure_ascii=False))
    print("matrix:   ", " > ".join(f"{o}({rows[o]['score']:.2f})" if rows[o]["score"] is not None else f"{o}(blocked)" for o in ranked))
    print("baseline: ", " > ".join(baseline))
    print("top1 share:", {o: round(top1[o] / N, 3) for o in OPTIONS if top1[o]})
    print("no pictures:", " > ".join(order_no_pics[:5]))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
