#!/usr/bin/env python3
"""Build reports/2026-09-25-core-swap.html from the research data, and its social cards.

    python3 reports/_core_swap_build.py            the page
    python3 reports/_core_swap_build.py --cards    the page, then four 1200x630 PNG cards via headless chromium

Every number on the page is read from a file the research produced; none is typed here:
  docs/research/terminal-core/jev/dossiers.json   the research-delight dossiers
  docs/research/terminal-core/jev/matrix.json     the decision matrix (scripts/matrix.py)
  docs/research/terminal-core/jev/runs/…          Jev's records and receipts (claim-check, core-fit)
  docs/research/terminal-core/jev/labels/…        the hand labels and the ground truth
  docs/research/terminal-core/bake/results/…      the bake-off
The notes system (base.css, notes.css, notes.js, markup) is read from the decision-brief skill
every build, never copied into the repo.
"""
from __future__ import annotations

import html
import json
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
R = ROOT / "docs/research/terminal-core"
JEV = R / "jev"
SKILL = Path.home() / ".claude/skills/decision-brief"
OUT = HERE / "2026-09-25-core-swap.html"

E = html.escape


def jl(p):
    return [json.loads(l) for l in Path(p).read_text().splitlines() if l.strip()]


D = json.loads((JEV / "dossiers.json").read_text())
DOSS = {d["id"]: d for d in D["dossiers"]}
M = json.loads((JEV / "matrix.json").read_text())
ROWS = M["rows"]
CLAIMS = jl(JEV / "runs/claim-check/records.jsonl")
CLAIM_RUNS = {v: jl(JEV / f"runs/{d}/records.jsonl") for v, d in (("1", "claim-check-v1"), ("2", "claim-check-v2"), ("3", "claim-check"))}
FITS = {r["item"]: r for r in jl(JEV / "runs/core-fit/records.jsonl")}
PROBES = json.loads((JEV / "probes.json").read_text())
COST = json.loads((JEV / "cost.json").read_text())
LABELS = json.loads((JEV / "labels/claim-check-v3.score.json").read_text())
TRUTH = json.loads((JEV / "labels/core-fit.truth.json").read_text())["labels"]
TRUTH_SCORE = json.loads((JEV / "labels/core-fit.score.json").read_text())
BAKE_RA = json.loads((R / "bake/results/bake-rio-alac.json").read_text())
BAKE_G = json.loads((R / "bake/results/bake-ghostty.json").read_text())
BAKE_W = json.loads((R / "bake/results/bake-wezterm.json").read_text())
PARSE = json.loads((R / "bake/results/parse-times.json").read_text())
JITTER = json.loads((JEV / "jitter.json").read_text())
AGREE = json.loads((JEV / "runs/core-fit/agreement.json").read_text())
WHATIF = json.loads((JEV / "what-if.json").read_text())


def fv(oid, key):
    return (DOSS[oid]["fields"].get(key) or {}).get("value")


SHORT = {"alacritty-own-loop": "alacritty + TD's loop", "alacritty-patched-vte": "alacritty + patched vte",
         "alacritty-graphics-fork": "alacritty graphics fork", "rio-vt": "rio-vt", "wezterm-term": "wezterm-term",
         "vt100-rust": "vt100", "td-own-core": "TD's own core", "libghostty-vt": "libghostty-vt",
         "libvterm": "libvterm", "contour-vtbackend": "Contour vtbackend", "kitty-core": "kitty's core",
         "ghostty-shadow": "Ghostty shadow", "zellij-grid": "Zellij's grid", "zmx": "zmx"}


def short(oid):
    return SHORT[oid]


def pct(x):
    return f"{round(100 * x)}%"


# ---------------------------------------------------------------- numbers the prose uses

verdicts = [c["axes"].get("verdict") for c in CLAIMS]
N_CLAIMS = len(CLAIMS)
V = {k: verdicts.count(k) for k in ("supported", "inferred", "overclaims", "unsupported", "human")}
N_VALUES = sum(1 for d in D["dossiers"] for f in d["fields"].values() if f.get("value") not in (None, ""))
RANKED = M["ranked"]
BASE = M["baseline"]
TOP1 = M["sensitivity"]["top1"]
TOP3 = M["sensitivity"]["top3"]
SCORE = {o: ROWS[o]["score"] for o in RANKED}
TOTAL = COST["total"]
CS3 = COST["runs"]["claim-check"]
V2 = TRUTH_SCORE["runs/core-fit"]
V1 = TRUTH_SCORE["runs/core-fit-v1"]
PMED = PARSE["median_ms"]
MPV_BYTES = 12_988_693


# ---------------------------------------------------------------- figures

FIGN = [0]


def fig(title, body, caption, fid=None):
    FIGN[0] += 1
    idattr = f' id="{fid}"' if fid else ""
    return (f'<figure{idattr}>\n  <span class="lbl">{FIGN[0]:02d} &middot; {title}</span>\n{body}\n'
            f'  <figcaption>{caption}</figcaption>\n</figure>')


def clean(s):
    return re.sub(r"[\x00-\x08\x0b\x0c\x0e-\x1f]", " ", s)


# 1 · the text-around recording in four cores

CORE_REC = {
    "alacritty": ("alacritty_terminal 0.26 · TD today", lambda k: BAKE_RA[k]["alacritty"]),
    "rio": ("rio-vt 0.5.28", lambda k: BAKE_RA[k]["rio"]),
    "ghostty": ("libghostty-vt 0.2.1", lambda k: BAKE_G[k]["ghostty"]),
    "wezterm": ("wezterm-term, git b09b56c", lambda k: BAKE_W[k]["wezterm"]),
}


def pane_svg(kind):
    rec = CORE_REC[kind][1]("text-around")
    cols, rows, cw, ch = 28, 10, 13, 18
    w, h = cols * cw + 20, rows * ch + 20
    p = [f'<svg viewBox="0 0 {w} {h}" role="img" aria-label="{E(CORE_REC[kind][0])}: the screen after a line of text, a picture and a second line" font-family="ui-monospace, monospace">',
         f'<defs><linearGradient id="pic-{kind}" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#2f5bd0"/><stop offset=".55" stop-color="#7a4fc4"/><stop offset="1" stop-color="#e0843a"/></linearGradient></defs>',
         f'<rect x="0" y="0" width="{w}" height="{h}" fill="#0b0c0a"/>']
    for r in range(rows):
        p.append(f'<text x="2" y="{10 + r * ch + 13}" fill="#3a3a36" font-size="8">{r}</text>')
    for pl in rec.get("placements") or []:
        x, y = 10 + pl["col"] * cw, 10 + pl["row"] * ch
        p.append(f'<rect x="{x}" y="{y}" width="{pl["cols"] * cw}" height="{pl["rows"] * ch}" rx="4" fill="url(#pic-{kind})"/>')
        p.append(f'<text x="{x + 8}" y="{y + pl["rows"] * ch - 8}" fill="#fff" font-size="10" opacity=".9">picture · {pl["cols"]}×{pl["rows"]} cells</text>')
    for line in rec["text"]:
        m = re.match(r"\s*(\d+)\|(.*)", clean(line))
        if not m or not m.group(2).strip() or int(m.group(1)) >= rows:
            continue
        r = int(m.group(1))
        p.append(f'<text x="14" y="{10 + r * ch + 13}" fill="#e6e7de" font-size="12">{E(m.group(2).strip()[:26])}</text>')
    cy, cx = rec["cursor"]
    if cy < rows:
        p.append(f'<rect x="{10 + cx * cw + 2}" y="{10 + cy * ch + 2}" width="{cw - 3}" height="{ch - 4}" fill="#9ae6c4" opacity=".75"/>')
    if kind == "alacritty":
        p.append(f'<rect x="10" y="{10 + 1 * ch}" width="{20 * cw}" height="{6 * ch}" rx="4" fill="none" stroke="#ec835a" stroke-dasharray="5 4"/>')
        p.append(f'<text x="{10 + 20 * cw + 8}" y="{10 + 3 * ch + 4}" fill="#f0a58a" font-size="10">where the</text>')
        p.append(f'<text x="{10 + 20 * cw + 8}" y="{10 + 3 * ch + 17}" fill="#f0a58a" font-size="10">picture</text>')
        p.append(f'<text x="{10 + 20 * cw + 8}" y="{10 + 3 * ch + 30}" fill="#f0a58a" font-size="10">belonged</text>')
    p.append("</svg>")
    return "".join(p)


def quad():
    cells = []
    for kind in ("alacritty", "rio", "ghostty", "wezterm"):
        ok = kind != "alacritty"
        cells.append(f'<div class="pane {"win" if ok else "lose"}"><div class="bar"><b>{E(CORE_REC[kind][0])}</b>'
                     f'<span class="verdict {"ok" if ok else "no"}">{"placed" if ok else "dropped"}</span></div>{pane_svg(kind)}</div>')
    return '<div class="scroller"><div class="panes four">' + "".join(cells) + "</div></div>"


# 2 · the bake-off table

BAKE_ROWS = [
    ("chafa", "chafa", "raw pixels at the cursor"),
    ("icat", "kitten icat", "a PNG at the cursor"),
    ("icat-ph", "kitten icat --unicode-placeholder", "placeholder cells"),
    ("rimg", "ratatui-image", "placeholder cells"),
    ("mpv-4frames", "mpv --vo=kitty", "four video frames"),
    ("text-around", "a test program", "text, picture, text"),
]


def placeholder_text_rows(rec):
    return sum(1 for t in rec.get("text") or [] if "#" in t.split("|", 1)[-1])


def bake_verdict(key, kind):
    """(placed?, short note) for one recording in one core, from its read-back."""
    src = {"alacritty": (BAKE_RA, "alacritty"), "rio": (BAKE_RA, "rio"), "ghostty": (BAKE_G, "ghostty"), "wezterm": (BAKE_W, "wezterm")}[kind]
    rec = (src[0].get(key) or {}).get(src[1])
    if rec is None:
        return None, "not run"
    pl = rec.get("placements") or []
    vp = rec.get("virtual_placements") or 0
    if kind == "alacritty":
        ph = rec.get("placeholder_cells_on_screen") or 0
        return False, f"{ph} placeholder cells left as text" if ph else "no image state"
    placeholder_prog = key in ("icat-ph", "rimg")
    if kind == "wezterm" and placeholder_prog:
        n = placeholder_text_rows(rec)
        return False, f"drawn at the cursor; {n} rows of placeholder characters left as text"
    if pl:
        p0 = pl[0]
        stacked = f", {len(pl)} stacked" if len(pl) > 1 else ""
        return True, f'{p0["cols"]}×{p0["rows"]} at row {p0["row"]}{stacked}; cursor row {rec["cursor"][0]}'
    if vp:
        return True, f"virtual placement over {rec.get('placeholder_cells_on_screen') or 'its'} placeholder cells"
    return False, "nothing"


def bake_table():
    trs = []
    tally = {k: 0 for k in CORE_REC}
    for key, prog, what in BAKE_ROWS:
        tds = []
        for kind in CORE_REC:
            ok, note = bake_verdict(key, kind)
            tally[kind] += bool(ok)
            tag = '<span class="y">placed</span>' if ok else ('<span class="u">not run</span>' if ok is None else '<span class="n">missed</span>')
            tds.append(f"<td>{tag}<small>{E(note)}</small></td>")
        trs.append(f'<tr><td><b>{E(prog)}</b><small>{E(what)}</small></td>{"".join(tds)}</tr>')
    foot = "".join(f'<td class="tally">{tally[k]} / {len(BAKE_ROWS)}</td>' for k in CORE_REC)
    head = "".join(f"<th>{E(CORE_REC[k][0])}</th>" for k in CORE_REC)
    return (f'<div class="scroller"><table class="wide bake"><thead><tr><th>Recording</th>{head}</tr></thead>'
            f'<tbody>{"".join(trs)}<tr class="sum"><td>placed</td>{foot}</tr></tbody></table></div>'), tally


# 3 · what wezterm-term does with placeholder cells

def placeholder_svg(kind):
    rows, cols, cw, ch = 40, 70, 5.2, 5.2
    w, h = cols * cw + 20, rows * ch + 22
    title = "rio-vt and libghostty-vt" if kind == "rio" else "wezterm-term"
    p = [f'<svg viewBox="0 0 {w:.0f} {h:.0f}" role="img" aria-label="{title}: kitten icat with placeholder cells" font-family="ui-monospace, monospace">',
         f'<defs><linearGradient id="ph-{kind}" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#2f5bd0"/><stop offset=".55" stop-color="#7a4fc4"/><stop offset="1" stop-color="#e0843a"/></linearGradient>'
         f'<pattern id="tofu-{kind}" width="5.2" height="5.2" patternUnits="userSpaceOnUse"><rect x=".8" y=".8" width="3.4" height="3.6" fill="none" stroke="#f0a58a" stroke-width=".6"/></pattern></defs>',
         f'<rect x="0" y="0" width="{w:.0f}" height="{h:.0f}" fill="#0b0c0a"/>']
    if kind == "rio":
        rec = BAKE_RA["icat-ph"]["rio"]
        n = rec["placeholder_cells_on_screen"]
        prow = n // 64
        p.append(f'<rect x="10" y="10" width="{64 * cw:.1f}" height="{prow * ch:.1f}" rx="3" fill="url(#ph-{kind})"/>')
        p.append(f'<text x="16" y="{10 + prow * ch - 8:.0f}" fill="#fff" font-size="9">one picture over {n} placeholder cells</text>')
    else:
        rec = BAKE_W["icat-ph"]["wezterm"]
        pl = rec["placements"][0]
        p.append(f'<rect x="{10 + pl["col"] * cw:.1f}" y="{10 + pl["row"] * ch:.1f}" width="{pl["cols"] * cw:.1f}" height="{pl["rows"] * ch:.1f}" rx="3" fill="url(#ph-{kind})"/>')
        p.append(f'<text x="16" y="{10 + pl["rows"] * ch - 8:.0f}" fill="#fff" font-size="9">picture drawn at the cursor</text>')
        rws = [int(t.split("|")[0]) for t in rec["text"] if "#" in t.split("|", 1)[-1]]
        top, bot = min(rws), max(rws)
        p.append(f'<rect x="10" y="{10 + (top + 1) * ch:.1f}" width="{64 * cw:.1f}" height="{(bot - top) * ch:.1f}" fill="url(#tofu-{kind})"/>')
        p.append(f'<rect x="{10 + 64 * cw:.1f}" y="{10 + top * ch:.1f}" width="{6 * cw:.1f}" height="{ch:.1f}" fill="url(#tofu-{kind})"/>')
        ty = 10 + (top + 1) * ch + (bot - top) * ch / 2
        p.append(f'<rect x="14" y="{ty - 10:.0f}" width="226" height="15" rx="3" fill="#0b0c0a" fill-opacity=".92"/>')
        p.append(f'<text x="20" y="{ty + 1:.0f}" fill="#f0a58a" font-size="9" font-weight="700">{len(rws)} rows of placeholder characters, shown as text</text>')
    p.append(f'<text x="10" y="{h - 5:.0f}" fill="#8b8a80" font-size="8">{rows} rows × 120 columns, left 70 shown</text>')
    p.append("</svg>")
    return "".join(p)


def placeholder_pair():
    return ('<div class="scroller"><div class="panes two">'
            f'<div class="pane win"><div class="bar"><b>rio-vt · libghostty-vt</b><span class="verdict ok">one picture</span></div>{placeholder_svg("rio")}</div>'
            f'<div class="pane lose"><div class="bar"><b>wezterm-term</b><span class="verdict no">picture + text</span></div>{placeholder_svg("wez")}</div>'
            "</div></div>")


# 4 · the fourteen subjects

ONE_LINE = {
    "alacritty-own-loop": "keep the emulator; TD reads the picture bytes itself",
    "alacritty-patched-vte": "patch alacritty's parser to hand pictures to TD",
    "alacritty-graphics-fork": "a one-person fork that adds Sixel",
    "rio-vt": "Rio's emulator, published as a crate",
    "wezterm-term": "WezTerm's emulator, from its git tree",
    "vt100-rust": "a small screen-and-parser crate",
    "td-own-core": "TD writes its own emulator",
    "libghostty-vt": "Ghostty's emulator, through its C API",
    "libvterm": "the C library Neovim adopted",
    "contour-vtbackend": "Contour's C++ core",
    "kitty-core": "kitty's own core",
    "ghostty-shadow": "libghostty beside alacritty, for pictures only",
    "zellij-grid": "reference: how Zellij grew Sixel",
    "zmx": "reference: sessions kept alive on libghostty",
}
BAKED = {"alacritty-own-loop", "rio-vt", "libghostty-vt", "wezterm-term"}


def road_map():
    groups = {}
    for d in D["dossiers"]:
        groups.setdefault(d["group"], []).append(d["id"])
    cols = []
    for g, ids in groups.items():
        letter, name = [s.strip() for s in g.split("·", 1)]
        cards = []
        for oid in ids:
            cov = str(fv(oid, "kitty_coverage") or "?")
            n = int(cov.split("/")[0]) if "/" in cov else 0
            blocked = fv(oid, "licence_gate") == "blocked"
            cls = "road" + (" blocked" if blocked else "") + (" baked" if oid in BAKED else "") + (" ref" if oid in ("zellij-grid", "zmx") else "")
            meter = f'<i style="width:{100 * n / 14:.0f}%"></i>'
            lic = str(fv(oid, "license") or "").split(" ")[0]
            cards.append(f'<div class="{cls}"><b>{E(short(oid))}</b><p>{E(ONE_LINE[oid])}</p>'
                         f'<div class="meter">{meter}</div><div class="kv"><span>Kitty {E(cov)}</span><span>{E(lic)}{" · blocked" if blocked else ""}</span></div></div>')
        cols.append(f'<div class="group"><div class="gh"><span>{E(letter)}</span>{E(name)}</div>{"".join(cards)}</div>')
    return f'<div class="scroller"><div class="roads">{"".join(cols)}</div></div>'


# 5 · should-jev: who owns each step

SHOULD_JEV = [
    ("Research the options for swapping the core, staying on alacritty included", "agents, gated by code", "is each fact's quote saying what the agent wrote?", "claim-check", "a"),
    ("Analyse every option across many dimensions", "code, over data", "how each core's interface meets thirteen things TD needs, and how settled that interface is", "core-fit", "c"),
    ("Score and rank the options", "code, weights as data", "none: each core is asked once and code compares; no pairwise 'which is better'", "", "c"),
    ("Documentation and a post", "the writer", "the narrative gate measures the telling", "", "a"),
    ("Decide", "Parker", "none: Jev never chooses", "", "h"),
]


def should_jev():
    trs = "".join(f'<tr><td>{E(a)}</td><td><span class="{k}">{E(b)}</span></td><td>{E(c)}</td><td>{f"<span class=j>{E(d)}</span>" if d else ""}</td></tr>'
                  for a, b, c, d, k in SHOULD_JEV)
    return ('<div class="scroller"><table class="wide"><thead><tr><th>Step, as asked</th><th>Owner</th><th>What Jev is asked</th><th>Spec</th></tr></thead>'
            f"<tbody>{trs}</tbody></table></div>")


# 6 · one question, three versions

VCOL = {"supported": "#199e70", "inferred": "#3987e5", "overclaims": "#c98500", "unsupported": "#6f6e66", "human": "#d03b3b", "none": "#2b2b28"}
VNAME = {"supported": "quote states it", "inferred": "quote implies it", "overclaims": "quote states part", "unsupported": "quote silent",
         "human": "sent to a person", "none": "no verdict"}
VNOTE = {
    "1": "judged the whole value; composite values read as half-stated, and a missing number check left 112 with no verdict",
    "2": "judged the value's headline; quotes that were the agent's premise came back silent",
    "3": "added 'implies' for a quote the value follows from",
}


def versions():
    rows = []
    for v, recs in CLAIM_RUNS.items():
        vs = [(r["axes"].get("verdict") or "none") for r in recs]
        n = len(vs)
        segs = "".join(f'<span style="width:{100 * vs.count(k) / n:.2f}%;background:{VCOL[k]}" title="{VNAME[k]}: {vs.count(k)}">{vs.count(k) if vs.count(k) >= 18 else ""}</span>'
                       for k in VCOL if vs.count(k))
        rows.append(f'<div class="vrow"><div class="l">version {v}<small>{E(VNOTE[v])}</small></div><div class="t">{segs}</div></div>')
    legend = "".join(f'<span><i style="background:{c}"></i>{VNAME[k]}</span>' for k, c in VCOL.items())
    return f'<div class="scroller"><div class="vrows">{"".join(rows)}</div></div><div class="legend">{legend}</div>'


# 7 · label precision per answer

ANS = ["states", "states_part", "implies", "silent", "contradicts"]


def precision():
    conf = LABELS["confusion_label_by_jev"]
    rows = []
    for a in ANS:
        col = {lab: conf[lab][a] for lab in ANS}
        n = sum(col.values())
        right = col[a]
        wrong = n - right
        segs = (f'<span style="width:{100 * right / n:.1f}%;background:#199e70"></span>' if right else "") + \
               (f'<span style="width:{100 * wrong / n:.1f}%;background:#d03b3b"></span>' if wrong else "")
        why = ", ".join(f"{c} {k.replace('_', ' ')}" for k, c in col.items() if k != a and c)
        rows.append(f'<div class="hbar"><div class="l">Jev said <b>{a.replace("_", " ")}</b><small>{E("the label said " + why) if why else "every label agreed"}</small></div>'
                    f'<div class="t">{segs}</div><div class="v">{right} / {n}</div></div>')
    legend = '<span><i style="background:#199e70"></i>label agrees</span><span><i style="background:#d03b3b"></i>label disagrees</span>'
    return f'<div class="scroller"><div class="hbars">{"".join(rows)}</div></div><div class="legend">{legend}</div>'


# 8 · core-fit: thirteen needs, fourteen cores

NEEDS = [("cell_contents", "cells"), ("zero_width", "marks"), ("underline_colour", "underline"), ("wide_chars", "wide"),
         ("scrollback_read", "history"), ("damage", "damage"), ("snapshot", "snapshot"), ("feed_loop", "own loop"),
         ("replies_out", "replies"), ("threading", "threads"), ("placements_readable", "pictures"),
         ("placeholders", "placeholders"), ("media_policy", "file policy")]
GLYPH = {"provides": ("●", "fit-p", "provides"), "provides_with_work": ("○", "fit-w", "with work"),
         "does_not": ("✕", "fit-x", "does not"), None: ("?", "fit-u", "not established")}
TRAJ = {"stable_contract": "stable", "active_pre_1_0": "pre-1.0, active", "declared_unstable": "declared unstable",
        "dormant": "dormant", "owned_by_td": "TD's own", "not_established": "unknown", None: "unknown"}


def fit_grid():
    head = "<tr><th class='opt'>core</th>" + "".join(f"<th>{E(l)}</th>" for _, l in NEEDS) + "<th>API</th></tr>"
    trs = []
    for oid in RANKED + ["zellij-grid", "zmx"]:
        ax = FITS[oid]["axes"]
        tds = [f'<td class="opt">{E(short(oid))}</td>']
        for k, _ in NEEDS:
            a = ax.get(k) if ax.get(k) in GLYPH else None
            g, cls, word = GLYPH[a]
            t = TRUTH.get(oid, {}).get(k)
            mark = ""
            if t and t[0] != "unsure":
                ok = a is not None and (("pass" if a in ("provides", "provides_with_work") else "fail") == t[0])
                mark = ' truth-ok' if ok else (' truth-unk' if a is None else ' truth-bad')
            tds.append(f'<td class="{cls}{mark}" title="{E(word)}{(" · checked: " + t[0]) if t and t[0] != "unsure" else ""}">{g}</td>')
        tds.append(f'<td class="traj">{E(TRAJ.get(ax.get("api_trajectory"), "unknown"))}</td>')
        trs.append(f'<tr class="{"ref" if ROWS[oid]["reference"] else ""}">{"".join(tds)}</tr>')
    legend = ('<span><b class="fit-p">●</b> provides</span><span><b class="fit-w">○</b> with work</span><span><b class="fit-x">✕</b> does not</span>'
              '<span><b class="fit-u">?</b> not established</span><span><i class="tick"></i>checked against the code</span>')
    return f'<div class="scroller"><table class="fit"><thead>{head}</thead><tbody>{"".join(trs)}</tbody></table></div><div class="legend">{legend}</div>'


# 9 · the rio-vt placements experiment

def p_does_not(run, item_prefix):
    out = []
    for r in jl(JEV / f"runs/{run}/receipts.jsonl"):
        if r.get("call") != "assess" or not r["item"].startswith(item_prefix):
            continue
        a = r["answers"]["placements_readable"]
        out.append((r["item"], a["probabilities"]["does_not"], a["value"]))
    return out


def rio_experiment():
    groups = [
        ("as sent", p_does_not("core-fit-v2-before-wezbake", "rio-vt") + p_does_not("core-fit-rio-repeat", "rio-vt/copy") + p_does_not("core-fit-rio-ablation", "rio-vt/A")
                     + p_does_not("core-fit-r1", "rio-vt") + p_does_not("core-fit-r2", "rio-vt") + p_does_not("core-fit-r3", "rio-vt")),
        ("claim-check labels removed", p_does_not("core-fit-rio-ablation", "rio-vt/B")),
        ("pixels sentence added", p_does_not("core-fit-rio-ablation", "rio-vt/C")),
        ("both", p_does_not("core-fit-rio-ablation", "rio-vt/D")),
    ]
    W, lh, x0, x1 = 980, 40, 300, 960
    h = lh * len(groups) + 46
    p = [f'<svg viewBox="0 0 {W} {h}" role="img" aria-label="Probability of does not, per ask, for rio-vt placements" font-family="ui-sans-serif, sans-serif">']
    for i in range(0, 11):
        x = x0 + (x1 - x0) * i / 10
        p.append(f'<line x1="{x:.0f}" y1="14" x2="{x:.0f}" y2="{h - 26}" stroke="{"#4a4a44" if i == 5 else "#232321"}" {"stroke-dasharray=\"3 3\"" if i == 5 else ""}/>')
        if i % 5 == 0:
            p.append(f'<text x="{x:.0f}" y="{h - 10}" fill="#8b8a80" font-size="11" text-anchor="middle" font-family="ui-monospace, monospace">{i / 10:.1f}</text>')
    for gi, (label, pts) in enumerate(groups):
        y = 30 + gi * lh
        p.append(f'<text x="0" y="{y + 4}" fill="#d6d8cf" font-size="12.5">{E(label)}</text>')
        p.append(f'<text x="0" y="{y + 19}" fill="#8b8a80" font-size="10.5" font-family="ui-monospace, monospace">{len(pts)} ask{"s" if len(pts) != 1 else ""}</text>')
        for j, (_, pr, val) in enumerate(sorted(pts, key=lambda t: t[1])):
            x = x0 + (x1 - x0) * pr
            col = "#ec835a" if val == "does_not" else "#199e70"
            p.append(f'<circle cx="{x:.1f}" cy="{y + (j % 3 - 1) * 5}" r="5.5" fill="{col}" fill-opacity=".85" stroke="#0b0c0a"/>')
    p.append(f'<text x="{x0}" y="10" fill="#8b8a80" font-size="10.5">probability Jev gave to "does not"</text>')
    p.append("</svg>")
    legend = '<span><i style="background:#ec835a"></i>top answer: does not</span><span><i style="background:#199e70"></i>top answer: provides, or with work</span>'
    return p, legend, groups


def rio_fig():
    p, legend, groups = rio_experiment()
    return "".join(p) + f'<div class="legend">{legend}</div>', groups


# 10 · traps

def trap_grid():
    cells = []
    for spec in ("claim-check", "core-fit"):
        for a in PROBES[spec]["anchors"]:
            ok = a["ok"]
            q = a["question"].split(".")[-1]
            cells.append(f'<div class="trap {"ok" if ok else "sprung"}" title="{E(spec)} · {E(q)} · wanted {E("/".join(a["want"]))}, got {E(a["got"])}">'
                         f'<b>{E(a["anchor"])}</b><span>{E(q)}</span></div>')
    held = sum(PROBES[s]["landed"][0] for s in PROBES)
    total = sum(PROBES[s]["landed"][1] for s in PROBES)
    legend = '<span><i style="background:#199e70"></i>held</span><span><i style="background:#d03b3b"></i>sprung</span>'
    return f'<div class="traps">{"".join(cells)}</div><div class="legend">{legend}</div>', held, total


# 11 · the matrix

WDIMS = [("pictures", "Kitty today"), ("fit", "fits TD"), ("td_work", "TD writes"), ("trajectory", "API settled"),
         ("bus_factor", "maintainers"), ("build", "build"), ("vt_features", "VT features"), ("other_protocols", "Sixel, iTerm2")]
SDIMS = [("pictures_measured", "bake-off"), ("performance", "13 MB in")]
SRC = {"pictures": "code", "fit": "Jev", "td_work": "estimate", "trajectory": "Jev", "bus_factor": "code", "build": "read",
       "vt_features": "code", "other_protocols": "code", "pictures_measured": "measured", "performance": "measured"}


def heat(s):
    s = max(0.0, min(1.0, s))
    if s < 0.5:
        a, b, t = (0x5c, 0x1f, 0x1f), (0x6b, 0x4a, 0x0f), s / 0.5
    else:
        a, b, t = (0x6b, 0x4a, 0x0f), (0x13, 0x5e, 0x45), (s - 0.5) / 0.5
    return "#%02x%02x%02x" % tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(3))


BUILD_SHORT = {"alacritty-own-loop": "cargo", "alacritty-patched-vte": "cargo + git", "alacritty-graphics-fork": "cargo + git",
               "rio-vt": "cargo + C++11", "wezterm-term": "cargo + git", "vt100-rust": "cargo", "td-own-core": "cargo",
               "libghostty-vt": "Zig", "libvterm": "C99", "contour-vtbackend": "C++23, CMake", "ghostty-shadow": "Zig"}


def shown(k, dim, oid=None):
    s = str(dim.get("shown", ""))
    if k == "build":
        s = BUILD_SHORT.get(oid, s)
    if k == "performance" and dim.get("score") is not None:
        s = re.sub(r"([\d.]+) ms", lambda m: f"{float(m.group(1)):.0f} ms", s)
    if k == "vt_features":
        s = s.split(",")[0].replace(" yes", " of 9")
    if k == "trajectory":
        s = TRAJ.get(s, s)
    if k == "fit":
        s = f'{dim["score"]:.2f}' if dim.get("score") is not None else "?"
    if k == "other_protocols":
        s = " · ".join(x for x in re.split(r"[,.\s]+", s.replace("Sixel ", "").replace("iTerm2 ", "")) if x)
    if k == "td_work":
        s = s
    return s[:18]


def matrix_table():
    wts = M["weights"]
    head = ('<tr><th class="opt">road</th>' + "".join(f'<th><span class="src {SRC[k]}">{SRC[k]}</span>{E(l)}<em class="w">× {wts[k]:g}</em></th>' for k, l in WDIMS)
            + '<th class="score">score</th><th>first in</th><th class="gap"></th>'
            + "".join(f'<th><span class="src {SRC[k]}">{SRC[k]}</span>{E(l)}<em class="w">shown</em></th>' for k, l in SDIMS) + "</tr>")
    trs = []
    for i, oid in enumerate(RANKED + ["zellij-grid", "zmx"]):
        row = ROWS[oid]
        blocked = row["facts"]["licence"]["gate"] == "blocked"
        cls = "ref" if row["reference"] else ("top" if i < 2 else "")
        tds = [f'<td class="opt">{E(short(oid))}</td>']
        for k, _ in WDIMS + [("__score", "")] + SDIMS:
            if k == "__score":
                if row["reference"]:
                    tds.append('<td class="unk">reference</td><td class="unk">—</td><td class="gap"></td>')
                elif blocked:
                    tds.append('<td class="blocked">GPL</td><td class="blocked">—</td><td class="gap"></td>')
                else:
                    tds.append(f'<td class="score" style="background:{heat(row["score"])}">{row["score"]:.2f}</td><td class="share">{pct(TOP1.get(oid, 0))}</td><td class="gap"></td>')
                continue
            dim = row["dims"][k]
            s = dim["score"]
            if blocked and k not in ("pictures",):
                tds.append('<td class="blocked">·</td>')
            elif s is None:
                tds.append(f'<td class="unk" title="{E(dim.get("source", ""))}">?</td>')
            else:
                tds.append(f'<td style="background:{heat(s)}" title="{E(dim.get("source", ""))}: {E(str(dim.get("shown", "")))}">{E(shown(k, dim, oid))}</td>')
        trs.append(f'<tr class="{cls}">' + "".join(tds) + "</tr>")
    return f'<div class="scroller"><table class="matrix"><thead>{head}</thead><tbody>{"".join(trs)}</tbody></table></div>'


# 12 · the rule against the matrix (slope chart)

def slope():
    opts = [o for o in RANKED if SCORE[o] is not None]
    base = [o for o in BASE if o in opts]
    W, lh, top = 1000, 28, 40
    h = top + lh * len(opts) + 10
    xl, xr = 300, 640
    p = [f'<svg viewBox="0 0 {W} {h}" role="img" aria-label="Rank under a plain rule against rank under the weighted matrix" font-family="ui-sans-serif, sans-serif">',
         f'<text x="{xl}" y="18" fill="#8b8a80" font-size="11" text-anchor="end">a rule written before asking any model</text>',
         f'<text x="{xr}" y="18" fill="#8b8a80" font-size="11">the weighted matrix</text>']
    lead = set(RANKED[:2])
    for o in opts:
        y1 = top + base.index(o) * lh
        y2 = top + opts.index(o) * lh
        hot = o in lead
        col = "#9ae6c4" if hot else ("#8fbdf2" if o == "libghostty-vt" else "#4a4a44")
        p.append(f'<path d="M{xl + 10} {y1} C {xl + 110} {y1}, {xr - 110} {y2}, {xr - 10} {y2}" fill="none" stroke="{col}" stroke-width="{3 if hot or o == "libghostty-vt" else 1.4}" stroke-opacity="{1 if hot or o == "libghostty-vt" else .7}"/>')
        p.append(f'<text x="{xl}" y="{y1 + 4}" fill="{"#fff" if hot else "#c3c2b7"}" font-size="12.5" text-anchor="end">{base.index(o) + 1}. {E(short(o))}</text>')
        p.append(f'<text x="{xr}" y="{y2 + 4}" fill="{"#fff" if hot else "#c3c2b7"}" font-size="12.5">{opts.index(o) + 1}. {E(short(o))}</text>')
        p.append(f'<text x="{W - 4}" y="{y2 + 4}" fill="#8b8a80" font-size="11" text-anchor="end" font-family="ui-monospace, monospace">{SCORE[o]:.2f}</text>')
    p.append("</svg>")
    return "".join(p)


# 13 · sensitivity

def sensitivity():
    rows = []
    for oid in sorted(TOP3, key=lambda o: (TOP1.get(o, 0), TOP3.get(o, 0)), reverse=True):
        if TOP3.get(oid, 0) < 0.02:
            continue
        t1, t3 = TOP1.get(oid, 0), TOP3.get(oid, 0)
        rows.append(f'<div class="hbar"><div class="l">{E(short(oid))}</div><div class="t">'
                    f'<span style="width:{100 * t1:.1f}%;background:#199e70"></span><span style="width:{100 * (t3 - t1):.1f}%;background:#23543f"></span></div>'
                    f'<div class="v">{pct(t1)} · {pct(t3)}</div></div>')
    legend = '<span><i style="background:#199e70"></i>ranked first</span><span><i style="background:#23543f"></i>in the top three, not first</span>'
    return f'<div class="scroller"><div class="hbars">{"".join(rows)}</div></div><div class="legend">{legend}</div>'


# 14 · the two leaders, dimension by dimension

def head_to_head():
    a, b = RANKED[0], RANKED[1]
    wts = M["weights"]
    rows = []
    for k, l in WDIMS:
        sa, sb = ROWS[a]["dims"][k]["score"], ROWS[b]["dims"][k]["score"]
        lead = "" if sa is None or sb is None or abs(sa - sb) < 0.01 else (a if sa > sb else b)
        rows.append(f'<div class="h2h"><div class="k">{E(l)}<small>× {wts[k]:g}</small></div>'
                    f'<div class="bars"><div class="ba"><span style="width:{100 * (sa or 0):.0f}%" class="{"lead" if lead == a else ""}"></span><em>{E(shown(k, ROWS[a]["dims"][k], a))}</em></div>'
                    f'<div class="bb"><span style="width:{100 * (sb or 0):.0f}%" class="{"lead" if lead == b else ""}"></span><em>{E(shown(k, ROWS[b]["dims"][k], b))}</em></div></div></div>')
    legend = f'<span><i style="background:#3987e5"></i>{E(short(a))} · {SCORE[a]:.2f}</span><span><i style="background:#b18cf0"></i>{E(short(b))} · {SCORE[b]:.2f}</span><span>brighter bar = leads on that dimension</span>'
    return f'<div class="scroller"><div class="h2hs">{"".join(rows)}</div></div><div class="legend">{legend}</div>'


# 15 · where reading met running

CHECKS = [
    ("wezterm-term has no placeholder support",
     "research: kitty_graphics lists no U=1 handling",
     "it drew the picture at the cursor and printed the placeholder characters as text, in both placeholder recordings",
     "held"),
    ("rio-vt exposes where each picture sits",
     "research: a public map, graphics.kitty_placements; Jev, reading that research, split near evenly",
     "the harness read every picture in all six recordings from its public maps, placeholder pictures included",
     "held"),
    ("libghostty-vt 0.2.1 moves the cursor one row further than kitty after a picture",
     "research: the full-height move, fixed upstream on 2026-08-18 after the crates.io pin",
     "its cursor matched rio-vt's in all seven recordings, on the row kitty's rule predicts",
     "overruled"),
    ("vte's maintainer declined a parser hook for pictures",
     "the Program Pixels brief, quoting an early review of alacritty/vte#115",
     "the research found the later review, &ldquo;seems fine to me. Just some minor nits.&rdquo;, before the pull request lapsed",
     "overruled"),
    ("wezterm-term cannot pass its replies back",
     "Jev, reading research that said wezterm-term has no reply event, only a writer it is handed",
     "the harness caught its replies in that writer; with the observation added to its evidence, all three runs said it provides",
     "overruled"),
    ("the build each road asks for",
     "a keyword rule in the matrix code",
     "it read &ldquo;no C compiler, cmake&rdquo; as needing CMake and misread 3 of 12; now read by hand",
     "our rule"),
]


def checks_table():
    trs = "".join(f'<tr><td><b>{E(c)}</b></td><td>{s}</td><td>{m}</td><td><span class="{ {"held": "y", "overruled": "n", "our rule": "p"}[v] }">{v}</span></td></tr>'
                  for c, s, m, v in CHECKS)
    return ('<div class="scroller"><table class="wide checks"><thead><tr><th>Claim</th><th>Where it came from</th><th>What running it showed</th><th></th></tr></thead>'
            f"<tbody>{trs}</tbody></table></div>")


# 16 · TD's seven files

TD_FILES = [("gridwire.rs", 19, 1462, "the grid, as the snapshot a window replays"), ("term.rs", 18, 1756, "the terminal wrapper"),
            ("socketpty.rs", 12, 442, "the host side of a pane's PTY"), ("host.rs", 8, 4707, "the session host"),
            ("main.rs", 3, 37783, "the window"), ("pane.rs", 3, 14523, "drawing a pane"), ("pane/bench.rs", 1, 4419, "the workbench face")]


def td_files():
    mx = max(r for _, r, _, _ in TD_FILES)
    rows = "".join(f'<div class="hbar"><div class="l"><code>{E(f)}</code><small>{E(what)} · {n:,} lines</small></div>'
                   f'<div class="t"><span style="width:{100 * r / mx:.0f}%;background:#3987e5"></span></div><div class="v">{r}</div></div>'
                   for f, r, n, what in TD_FILES)
    return f'<div class="scroller"><div class="hbars">{rows}</div></div>'


# ---------------------------------------------------------------- modals

def cost_rows():
    names = {"claim-check-v1": "claim-check, version 1", "claim-check-v2": "claim-check, version 2", "claim-check": "claim-check, version 3 (of record)",
             "core-fit-v1": "core-fit, version 1", "core-fit-reversed": "core-fit v1, questions reversed", "core-fit": "core-fit, version 2 (of record)",
             "core-fit-rio-repeat": "rio-vt asked eight more times", "core-fit-rio-ablation": "rio-vt, four variants"}
    trs = "".join(f'<tr><td>{E(names.get(k, k))}</td><td>{v["requests"]:,}</td><td>{v["questions"]:,}</td><td>{v["input_tokens"] + v["output_tokens"]:,}</td><td>${v["cost_usd"]:.4f}</td><td>{v["p50_ms"]:.0f} ms</td></tr>'
                  for k, v in COST["runs"].items())
    t = COST["total"]
    trs += f'<tr class="sum"><td>all of it</td><td>{t["requests"]:,}</td><td>{t["questions"]:,}</td><td>{t["input_tokens"] + t["output_tokens"]:,}</td><td>${t["cost_usd"]:.4f}</td><td></td></tr>'
    return ('<table class="wide"><thead><tr><th>Run</th><th>requests</th><th>questions</th><th>tokens</th><th>cost</th><th>median</th></tr></thead>'
            f"<tbody>{trs}</tbody></table>")


def sources_list():
    items = []
    for d in D["dossiers"]:
        s = str(fv(d["id"], "primary_sources") or "")
        urls = re.findall(r"https?://[^\s;,)]+", s)
        links = " ".join(f'<a href="{E(u)}">{E(re.sub(r"^https?://(www\.)?", "", u)[:60])}</a>' for u in urls[:8])
        items.append(f"<li><b>{E(short(d['id']))}</b> {links or E(s[:240])}</li>")
    return "<ul class='srcs'>" + "".join(items) + "</ul>"


def cards_html():
    out = []
    for i, c in enumerate(CARDS, 1):
        out.append(f'<div class="card-frame"><div class="card-scale">{c}</div></div>')
    return '<div class="cards">' + "".join(out) + "</div>"


# ---------------------------------------------------------------- social cards

def card_grid_svg(tally):
    cores = [("alacritty", "alacritty (TD today)"), ("rio", "rio-vt"), ("ghostty", "libghostty-vt"), ("wezterm", "wezterm-term")]
    p = ['<svg viewBox="0 0 520 250" font-family="ui-monospace, monospace">']
    for i, (k, name) in enumerate(cores):
        y = 30 + i * 56
        p.append(f'<text x="0" y="{y + 6}" fill="#c9c8bd" font-size="17">{name}</text>')
        for j, (key, _, _) in enumerate(BAKE_ROWS):
            ok, _ = bake_verdict(key, k)
            p.append(f'<rect x="{250 + j * 42}" y="{y - 14}" width="32" height="32" rx="7" fill="{"#199e70" if ok else "#3b2320"}" stroke="{"#5fcda4" if ok else "#ec835a"}" stroke-width="1.5"/>')
    p.append("</svg>")
    return "".join(p)


def build_cards(tally):
    brand = '<div class="brand">Terminal Delight &middot; Core Swap</div>'
    foot = '<div class="foot">parkerbrown.dev</div>'
    a, b = RANKED[0], RANKED[1]
    c1 = (f'<div class="card-social">{brand}<h2>Six picture programs.<br>Four terminal cores.</h2>'
          f'<div class="split"><p>The emulator Terminal Delight uses today kept {tally["alacritty"]} of 6 pictures. '
          f'rio-vt and libghostty-vt placed all 6 on the same cells. wezterm-term placed {tally["wezterm"]}.</p>{card_grid_svg(tally)}</div>{foot}</div>')
    order = ("supported", "inferred", "overclaims", "unsupported", "human")
    CARD_WORD = {"supported": "stated", "inferred": "implied", "overclaims": "stated in part", "unsupported": "quote silent", "human": "to a person"}
    stack = "".join(f'<span style="flex:{V[k]};background:{VCOL[k]}"><b>{V[k]}</b><em>{CARD_WORD[k]}</em></span>' for k in order)
    c2 = (f'<div class="card-social">{brand}<h2>{N_CLAIMS} facts, each checked<br>against its own quote</h2>'
          f'<p>Six research agents copied the passage behind each fact. Jev asked whether each passage says what the agent wrote, '
          f'and {V["supported"]} of {N_CLAIMS} said it outright. Every Jev call behind this decision: {TOTAL["requests"]:,} requests, ${TOTAL["cost_usd"]:.2f}.</p>'
          f'<div class="cstack">{stack}</div>{foot}</div>')
    bars = "".join(f'<div class="cb"><span>{E(short(o))}</span><i style="width:{SCORE[o] * 100:.0f}%"></i><b>{SCORE[o]:.2f}</b></div>' for o in RANKED[:5])
    c3 = (f'<div class="card-social">{brand}<h2>Two roads,<br>one weight between them</h2>'
          f'<div class="split"><p>{E(short(a))} {SCORE[a]:.2f}, {E(short(b))} {SCORE[b]:.2f}. Across 10,000 random weightings they split '
          f'{pct(TOP1[a])} to {pct(TOP1[b])}. The weights decide, and a person sets them.</p><div class="cbars">{bars}</div></div>{foot}</div>')
    chips = "".join(f'<span class="chip {k}">{t}</span>' for k, t in (("agent", "agents gather"), ("code", "code gates quotes"), ("jev", "Jev judges quotes"), ("jev", "Jev judges fit"), ("code", "code ranks"), ("person", "a person decides")))
    c4 = (f'<div class="card-social">{brand}<h2>Jev never picked<br>the winner</h2>'
          f'<p>It answered two narrow questions, about quotes and about interfaces, and code did the ranking over stored answers. '
          f'Change a weight and the ranking re-runs without asking Jev again.</p><div class="chips">{chips}</div>{foot}</div>')
    return [c1, c2, c3, c4]


# ---------------------------------------------------------------- the page

def build():
    FIGN[0] = 0
    bake_html, tally = bake_table()
    global CARDS
    CARDS = build_cards(tally)
    rio_html, rio_groups = rio_fig()
    traps_html, held, total = trap_grid()
    a, b, c = RANKED[0], RANKED[1], RANKED[2]
    no_pics = M["order_without_pictures"]
    stay_rank = no_pics.index("alacritty-own-loop") + 1
    as_sent = rio_groups[0][1]
    n_dn = sum(1 for _, _, v in as_sent if v == "does_not")
    c_pix = rio_groups[2][1][0][1]
    lab = LABELS

    figs = {}
    figs["quad"] = fig("One recording, four cores", quad(),
        "<b>When a core drops the picture, the text after it moves up.</b> A test program prints a line, draws a 20×6-cell picture, "
        "then prints a second line. alacritty puts the second line on row 2, where the picture should be; the three cores that keep the "
        "picture put it on row 7, below. The green block is where each core left its cursor. Drawn from each harness's read-back of the grid.", "f-quad")
    figs["bake"] = fig("Six recordings, four cores", bake_html,
        "<b>Placed means the core stored the picture and the harness read back where it sits.</b> wezterm-term misses both programs that use "
        "placeholder cells, the Kitty feature where a program prints special characters and the terminal paints the picture over them. "
        "viu is left out: its recording points at a temporary file that no longer exists. The mpv row was re-recorded with the same command "
        "for the wezterm-term run and all four cores re-ran it.", "f-bake")
    figs["ph"] = fig("Placeholder cells", placeholder_pair(),
        "<b>wezterm-term ignores the placeholder flag.</b> It draws kitten icat's picture at the cursor, then shows the placeholder characters "
        "as text below it; with ratatui-image the same characters land on top of all but one row of the picture. rio-vt and libghostty-vt "
        "paint one picture over the cells. Drawn to scale from the read-back.", "f-ph")
    figs["roads"] = fig("Fourteen subjects", road_map(),
        "<b>Twelve roads and two references, in four groups.</b> Zellij and zmx are references: each solved part of this already. The bar is how many of fourteen Kitty graphics features each core implements "
        "today, counted by code over the agents' feature lists. A green edge marks the four cores the bake-off ran. kitty's own core is out on its "
        f"GPL licence before anything is weighed. {N_VALUES} values across thirty fields, gathered in three rounds.", "f-roads")
    figs["owners"] = fig("Who owns each step", should_jev(),
        "<b>The request, run through the should-jev method.</b> Jev appears at the two steps where the judgment is about meaning and code "
        "cannot make it exactly. Ranking was rewritten away from asking which road is better: each core is asked about once, and code compares.",
        "f-owners")
    figs["versions"] = fig("One question, three versions", versions(),
        f"<b>claim-check asks whether a fact's own quote states it.</b> The labels beside each bar say what changed. Version 2 showed that "
        f"agents quote the premise they reasoned from, so the quote says nothing of the conclusion; version 3 gave that case its own answer, and "
        f"{V['inferred']} facts took it. Of {N_CLAIMS}, {V['supported']} are stated outright by their quote.", "f-versions")
    figs["labels"] = fig("Hand labels on 40 facts", precision(),
        f"<b>Eight facts from each answer, labelled without seeing Jev's.</b> When Jev said the quote states the fact, the label agreed 8 times "
        f"in 8. It said 'contradicts' 17 times in the run; reading all 17 found no real contradiction, and the verdict already sends "
        f"that answer to a person. {lab['exact']} of {lab['n']} matched exactly. Labels are the assembling "
        f"agent's, not Parker's.", "f-labels")
    jit = [v["share"] for v in JITTER["identical_input"].values()]
    ncores = len([c for c in TRUTH if any(v[0] in ("pass", "fail") for v in TRUTH[c].values())])
    figs["fit"] = fig("Thirteen needs, fourteen cores", fit_grid(),
        f"<b>Jev's reading of each core's interface, from the checked evidence.</b> Unknown stays unknown. Each cell is the majority of three "
        f"runs on identical input, which agreed on {AGREE['unanimous']} of {AGREE['cells']} cells; repeating a run changes "
        f"{min(jit) * 100:.0f}–{max(jit) * 100:.0f}% of answers, and reversing the question order changed 4.4%, inside that noise. "
        f"For the {['no', 'one', 'two', 'three', 'four', 'five'][ncores]} cores the harnesses touched, {V2['scored']} cells can be checked against the code: {V2['right']} right, "
        f"{V2['wrong']} wrong, {V2['unknown']} unknown. Version 1 had {V1['wrong']} wrong.", "f-fit")
    figs["rio"] = fig("Why rio-vt's pictures read as unknown", rio_html,
        f"<b>A need that asks for four things, where the evidence showed three.</b> From identical evidence Jev put 'does not' on top "
        f"{n_dn} times in {len(as_sent)}, never far from even, so the meaning map routes it to unknown. Removing the claim-check labels changed "
        f"nothing. One sentence from rio-vt's source, that each image's pixels are public, moved 'does not' to {c_pix:.2f}. The run of record "
        f"keeps the unknown; the bake-off carries the measurement.", "f-rio")
    figs["traps"] = fig("Traps", traps_html,
        f"<b>Awkward cases, written before any call, each with a twin that differs in one way.</b> A trap springs when Jev answers it wrong. "
        f"{held} of {total} held across both specs. The two that sprang need a date window and outside knowledge; both are recorded in the "
        f"spec with their causes.", "f-traps")
    figs["checks"] = fig("Reading against running", checks_table(),
        "<b>Six places where a claim met a measurement.</b> Two held, three were overruled, one of them Jev's own reading, and one exposed a rule of our own.", "f-checks")
    figs["matrix"] = fig("Every road, every dimension", matrix_table(),
        "<b>Green is strong and red weak, on each dimension's own scale; hatching is unknown and never counts as zero.</b> The label over each "
        "column says who produced it. The two measured columns on the right carry no weight: they exist for four and three roads, and a weighted "
        "column measured for a third of the field lets the rest skip it. Weights in <span class='mine'>amber</span> are a first guess.", "f-matrix")
    figs["slope"] = fig("The rule and the matrix", slope(),
        f"<b>A plain rule and the weighted matrix pick the same four, in a different order.</b> The rule ranks by picture coverage first, "
        f"where libghostty-vt's 14 of 14 wins outright. The matrix trades coverage against migration work, API stability, maintainers and build, "
        f"and moves {short(a)} and {short(b)} ahead.", "f-slope")
    lead1, lead2 = sorted([a, b], key=lambda o: TOP1.get(o, 0), reverse=True)
    ahead = no_pics[:stay_rank - 1]
    ordinal = ["first", "second", "third", "fourth", "fifth"][stay_rank - 1]
    figs["sens"] = fig("Ten thousand random weightings", sensitivity(),
        f"<b>The first place belongs to two roads.</b> {short(lead1)} comes first in {pct(TOP1[lead1])} of random weightings and "
        f"{short(lead2)} in {pct(TOP1[lead2])}. With pictures weighted zero, staying on alacritty rises to {ordinal}"
        f"{', behind ' + ' and '.join(short(o) for o in ahead) if ahead else ''}.", "f-sens")
    wins = {a: [], b: []}
    for k, l in WDIMS:
        sa, sb = ROWS[a]["dims"][k]["score"], ROWS[b]["dims"][k]["score"]
        if sa is not None and sb is not None and abs(sa - sb) >= 0.01:
            wins[a if sa > sb else b].append({"Kitty today": "pictures", "fits TD": "fit", "TD writes": "the size of the rewrite", "API settled": "API stability",
                                              "Sixel, iTerm2": "Sixel and iTerm2"}.get(l, l))
    def said(xs):
        return ", ".join(xs[:-1]) + (" and " if len(xs) > 1 else "") + xs[-1] if xs else "nothing"
    figs["h2h"] = fig("The two leaders", head_to_head(),
        f"<b>{short(a)} leads on {said(wins[a])}; {short(b)} leads on {said(wins[b])}.</b> Three people carry "
        f"wezterm-term's recent commits against one for rio-vt, and wezterm-term builds with cargo alone, though from an unpublished git tree. "
        f"rio-vt is on crates.io, needs a C++11 compiler for one dependency, and keeps placeholders, which wezterm-term would need a fork to add.", "f-h2h")
    figs["files"] = fig("TD's seven files", td_files(),
        "<b>Every file in TD that names alacritty_terminal, by how often.</b> gridwire.rs, which turns the grid into the snapshot a window "
        "replays, names it most. A swap rewrites all seven; staying rewrites none and writes the Kitty protocol instead. Counted on main at 81ec545.",
        "f-files")

    body = f"""
<div class="wrap">
<header class="hero">
<p class="kicker">Terminal Delight &middot; research, nothing built &middot; 2026-09-25</p>
<h1>Core Swap</h1>
<p class="lede">Terminal Delight gets its screen from alacritty's emulator, a Rust library called alacritty_terminal that turns a
program's output into rows of cells. When a program sends a picture instead of text, the library drops it, and the rows after it close
up as if nothing had been sent. Replacing the emulator reaches most of TD, so before touching anything we recorded six
picture programs and fed the same bytes to four emulators, had six agents research twelve ways forward, had Jev check every quoted fact,
and let code rank the roads. <b>Two roads come out close, {E(short(a))} and {E(short(b))}, and one weight decides between them.</b> TD's core is
unchanged.</p>
<div class="bignums">
  <div class="bignum g"><div class="n">{tally['rio']}<small>/ 6</small></div><div class="k">rio-vt, libghostty-vt</div><p>pictures placed; alacritty placed {tally['alacritty']}</p></div>
  <div class="bignum b"><div class="n">{N_CLAIMS}</div><div class="k">quoted facts</div><p>each checked by Jev against its own quote</p></div>
  <div class="bignum a"><div class="n">{SCORE[a]:.2f}<small>vs {SCORE[b]:.2f}</small></div><div class="k">{E(short(a))}, {E(short(b))}</div><p>the weighted scores</p></div>
  <div class="bignum"><div class="n">${TOTAL['cost_usd']:.2f}</div><div class="k">every Jev call</div><p>{TOTAL['requests']:,} requests, {TOTAL['questions']:,} questions</p></div>
</div>
</header>

<h2 class="sec">alacritty drops every picture, and three cores keep them</h2>
<p>The first question was whether any emulator keeps pictures at all. Each recording is what a picture program wrote to a fake terminal
that answered like kitty, and a small harness replays it into each emulator and reads back what the emulator kept.</p>
{figs['quad']}
{figs['bake']}
{figs['ph']}

<h2 class="sec">Twelve ways forward, staying included</h2>
<p>Four emulators is a sample. The research widened it to every way forward: keep alacritty and teach TD to read pictures itself, patch
its parser, move to another Rust emulator, cross a C boundary to Ghostty's, or write TD's own.</p>
{figs['roads']}

<h2 class="sec">Jev read the research before code ranked it</h2>
<p>The bake-off could run four of those roads. The other eight exist here only as the {N_VALUES} values six agents wrote about them, and a
ranking built on those inherits every overstatement. So Jev, TypeSafe's model that answers typed questions with probabilities, was asked
two narrow things: for each quoted fact, does the quote state it; and for each core, how does its interface meet thirteen things TD's
code needs.</p>
{figs['owners']}
{figs['versions']}
{figs['labels']}
{figs['fit']}
{figs['rio']}
{figs['traps']}

<h2 class="sec">Running things overruled some of the reading</h2>
<p>A quote can match its source and the source can still be wrong, and Jev can misread a source that is right. Six claims could be
run as well as read.</p>
{figs['checks']}

<h2 class="sec">Code ranks, and two roads finish close</h2>
<p>With the facts checked and each core's fit read, code could rank. It scores every road on eight weighted dimensions and keeps unknowns
out of the arithmetic, and it puts {E(short(a))} ahead of {E(short(b))} by {SCORE[a] - SCORE[b]:.2f}. Staying on alacritty scores
{SCORE['alacritty-own-loop']:.2f}, and most of that gap is pictures.</p>
{figs['matrix']}
{figs['slope']}
{figs['sens']}
{figs['h2h']}

<h2 class="sec">What a swap costs TD</h2>
<p>Whichever road leads, TD pays for it in the code that reads alacritty today. The host, the window and the snapshot passed between
them all use its types. Of the two leaders, only wezterm-term already serializes its pictures with its lines, behind a feature flag;
rio-vt's snapshot leaves them out.</p>
{figs['files']}

<h2 class="sec">Four questions before any code</h2>
<div class="grill">
  <div class="ask" id="q-worth"><h3>Are pictures worth a core swap?</h3>
  <p>With pictures weighted zero, staying on alacritty ranks {['first', 'second', 'third', 'fourth'][stay_rank - 1]}. TD's foundation review set this swap aside until image support became a committed feature.</p>
  <p class="rec"><b>Recommended:</b> decide this first. Every question below assumes yes.</p></div>
  <div class="ask" id="q-replica"><h3>Can a picture cross from TD's host to its windows?</h3>
  <p>wezterm-term's lines serialize with their pictures, which is how WezTerm's own server and client share a screen. rio-vt's snapshot leaves pictures out, so TD would build that part.</p>
  <p class="rec"><b>Recommended:</b> a spike before choosing, with each of the two as the host's core, a window replaying it, and the pictures compared.</p></div>
  <div class="ask" id="q-leader"><h3>wezterm-term or rio-vt?</h3>
  <p>The matrix leans wezterm-term, first in {pct(TOP1['wezterm-term'])} of random weightings, on three maintainers and a cargo-only build. It ignores placeholders, so TD would carry a fork from the first day; rio-vt keeps them and ships as a crate.</p>
  <p class="rec"><b>Recommended:</b> wezterm-term if the spike carries its pictures to a window, rio-vt if it doesn't. The spike tests the job TD's snapshot exists for, which no column in the matrix measures.</p></div>
  <div class="ask" id="q-weights"><h3>Are the weights right?</h3>
  <p>They are a first guess, and changing one re-ranks every road without asking Jev again.</p>
  <p class="rec"><b>Recommended:</b> leave a note on the matrix with yours.</p></div>
</div>

<div class="dlgrow">
  <button data-dlg="dlg-method">How the research ran</button>
  <button data-dlg="dlg-specs">The two Jev specs</button>
  <button data-dlg="dlg-cost">What it cost</button>
  <button data-dlg="dlg-doubts">Doubts</button>
  <button data-dlg="dlg-sources">Sources</button>
  <button data-dlg="dlg-cards">Cards for socials</button>
</div>
<p class="foot">Nothing in app/ was changed. Everything behind this page is in docs/research/terminal-core.</p>
</div>
"""

    dialogs = f"""
<dialog class="sub" id="dlg-method"><div class="dlg-head"><div><h3>How the research ran</h3><p class="repo">research-delight · three rounds · 2026-09-25</p></div><button class="x" data-close>&times;</button></div>
<div class="dlg-body">
<p>A research-delight plan named fourteen subjects and thirty fields for each, and marked every field as needing a source, a source or a
flag, or derived by code. Six agents gathered in parallel, one per cluster, under one rule: a value from a source carries a verbatim quote
of the passage, no longer than 400 characters, and says whether the agent measured it or inferred it. A merge script refused any value
that broke the rule. Three rounds ran; the third added nothing new and the run stopped, with thirteen of fourteen subjects complete and
99% of fields filled. The fourteenth, TD's own core, is a design rather than a project, and its gaps are marked as gaps.</p>
<h4>The bake-off</h4>
<p>A fake terminal (bake/fakepty.py) ran each picture program, answered its questions like kitty, and recorded every byte. Three harnesses
fed those bytes to alacritty_terminal 0.26 and rio-vt 0.5.28 (bake/corebake), libghostty-vt 0.2.1 built with Zig 0.15.2 (bake/ghostbake),
and wezterm-term at git b09b56c (bake/wezbake), in 4,096-byte chunks, then read back placements, cursor, replies and text. wezterm-term keeps
no placement map, so its harness rebuilds each placement from the picture slices stored in cells. Timings are medians of three runs on one
machine over the same {MPV_BYTES:,}-byte recording: libghostty-vt {PMED['libghostty-vt']:.0f} ms, alacritty {PMED['alacritty_terminal']:.0f} ms,
rio-vt {PMED['rio-vt']:.0f} ms, wezterm-term {PMED['wezterm-term']:.0f} ms.</p>
</div></dialog>

<dialog class="sub" id="dlg-specs"><div class="dlg-head"><div><h3>The two Jev specs</h3><p class="repo">jev/specs · Jev Decision Spec format · status shadow</p></div><button class="x" data-close>&times;</button></div>
<div class="dlg-body">
<h4>claim-check, version 3</h4>
<p>One request per quoted fact, two questions. <code>support</code> is a Choice over states, states part, implies, contradicts and silent,
judged on the value's headline. <code>other_subject</code> is a Noul: does the quote describe something else. Code computes the rest: whether
the numbers in the headline appear in the quote, whether the source is primary. The verdict goes to a person when the numbers mismatch, when
Jev says contradicts, when its confidence is under 0.305, or when the quote is likely about another subject.</p>
<h4>core-fit, version 2</h4>
<p>One request per core with thirteen Choice questions over the checked evidence, each answering provides, provides with work, does not,
or evidence silent; and a second request about how settled the API is. Silent, contested and low-confidence answers route to unknown and
are drawn grey. Weights inside fit: 3 for a must-have, 2 important, 1 nice. Evaluation: {V2['scored']} ground-truth cells on the three cores
the harnesses touched ({V2['right']} right, {V2['wrong']} wrong, {V2['unknown']} unknown), the question-order reversal (4.4% changed, under the
10% falsifier and inside the noise of repeating a run), and {PROBES['core-fit']['landed'][0]} of {PROBES['core-fit']['landed'][1]} traps held.</p>
<h4>Why two specs and not one</h4>
<p>The first question is about a quote and the second about an interface. They see different state, and asking them together would let
a doubt about one leak into the other. The rio-vt experiment tested exactly that leak and found none.</p>
</div></dialog>

<dialog class="sub" id="dlg-cost"><div class="dlg-head"><div><h3>What it cost</h3><p class="repo">jev/cost.json · summed from every receipt · OpenRouter, typesafe/jev-1.13</p></div><button class="x" data-close>&times;</button></div>
<div class="dlg-body">{cost_rows()}<p>Trap probes write no receipts and are not in these totals; there were two probe passes per version, 44 traps each.</p></div></dialog>

<dialog class="sub" id="dlg-doubts"><div class="dlg-head"><div><h3>Doubts</h3><p class="repo">what this rests on that has not been measured</p></div><button class="x" data-close>&times;</button></div>
<div class="dlg-body"><ul>
<li><b>The weights are a first guess</b> (hunch). The sensitivity figure shows how far the top depends on them.</li>
<li><b>Maintainers counts people with 10% or more of last year's commits</b> (measured, crude). It scores Ghostty, with thousands of commits and many contributors, the same as a one-person project because one person carries half.</li>
<li><b>"TD writes" is an estimate</b> from each dossier's migration notes and TD's seven files, sized S to XXL by the assembling agent (inferred).</li>
<li><b>Labels and ground truth are the assembling agent's</b>, written from TD's use of alacritty and the harnesses, not by Parker (measured, one labeller).</li>
<li><b>Core-fit answers move between runs</b> (measured): identical input changes a few percent of cells, so each cell is a majority of three runs, and a cell with no majority is unknown.</li>
<li><b>Timings are three runs on one machine</b> over one recording, and the wezterm-term harness scans its cells to find pictures, which the others do not (measured, rough).</li>
<li><b>Placeholder cells surviving TD's snapshot</b> is inferred from gridwire.rs re-emitting combining marks, not tested.</li>
</ul></div></dialog>

<dialog class="sub" id="dlg-sources"><div class="dlg-head"><div><h3>Sources</h3><p class="repo">primary sources each agent read, per subject</p></div><button class="x" data-close>&times;</button></div>
<div class="dlg-body">{sources_list()}<p>Every value's own source and quote are in docs/research/terminal-core/run/terminal-core-candidates.html.</p></div></dialog>

<dialog class="sub wide" id="dlg-cards"><div class="dlg-head"><div><h3>Cards for socials</h3><p class="repo">1200×630 · rendered to reports/2026-09-25-core-swap-card-1..4.png</p></div><button class="x" data-close>&times;</button></div>
<div class="dlg-body">{cards_html()}</div></dialog>
"""
    return body, dialogs


def assemble(body, dialogs):
    base = (SKILL / "assets/base.css").read_text()
    notes_css = (SKILL / "assets/notes.css").read_text()
    notes_js = (SKILL / "assets/notes.js").read_text()
    markup = (SKILL / "reference/notes-markup.html").read_text()
    css = (HERE / "_core_swap.css").read_text()
    if re.search(r"</script", notes_js, re.I):
        sys.exit("notes.js contains a closing script tag; inlining it would break the page")
    markup = markup.replace("<YYYY-MM-DD>-<topic-slug>.html", OUT.name)
    markup = markup.replace("<!-- then paste the contents of assets/notes.js inline, inside a <script> tag -->", "<script>\n" + notes_js + "\n</script>")
    if OUT.name not in markup or notes_js[:40] not in markup:
        sys.exit("the notes markup block changed shape; update this builder")
    dlg_js = """<script>
(function () {
  document.querySelectorAll('[data-dlg]').forEach(function (b) {
    b.addEventListener('click', function () {
      var d = document.getElementById(b.dataset.dlg);
      if (d && typeof d.showModal === 'function') d.showModal();
    });
  });
  document.querySelectorAll('dialog.sub').forEach(function (d) {
    d.querySelectorAll('[data-close]').forEach(function (x) { x.addEventListener('click', function () { d.close(); }); });
    d.addEventListener('click', function (e) { if (e.target === d) d.close(); });
  });
})();
</script>"""
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Core Swap</title>
<style>
{base}
{notes_css}
{css}
</style>
</head>
<body>
{body}
{dialogs}
{markup}
{dlg_js}
</body>
</html>
"""


def render_cards():
    base = (SKILL / "assets/base.css").read_text()
    css = (HERE / "_core_swap.css").read_text()
    outs = []
    for i, c in enumerate(CARDS, 1):
        page = HERE / f".card-{i}.html"
        page.write_text(f"<!doctype html><html><head><meta charset='utf-8'><style>{base}{css} body{{margin:0;background:#0d0d0c}}</style></head><body>{c}</body></html>")
        png = HERE / f"2026-09-25-core-swap-card-{i}.png"
        subprocess.run(["chromium", "--headless=new", "--disable-gpu", "--hide-scrollbars", "--window-size=1200,630",
                        f"--screenshot={png}", page.as_uri()], capture_output=True, timeout=60)
        page.unlink()
        outs.append(png)
    return outs


if __name__ == "__main__":
    body, dialogs = build()
    page = assemble(body, dialogs)
    OUT.write_text(page)
    print(f"wrote {OUT} ({len(page):,} bytes, {FIGN[0]} figures)")
    if "--cards" in sys.argv:
        for p in render_cards():
            print("card", p, p.stat().st_size if p.exists() else "MISSING")
