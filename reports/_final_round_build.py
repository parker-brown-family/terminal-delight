#!/usr/bin/env python3
"""Build reports/2026-09-25-core-swap-final-round.html: rio-vt against libghostty-vt.

    python3 reports/_final_round_build.py

Every number is read from the research files:
  docs/research/terminal-core/fluency/results.json      six agents, checked against sealed answers
  docs/research/terminal-core/quality/summary.json       build quality, the same checks for both
  docs/research/terminal-core/quality/static-facts.json  counted code facts
  docs/research/terminal-core/bake/results/perf/summary.json   twenty panes
  docs/research/terminal-core/vibes/final-2026-09-25.json      Parker's sealed weights, applied
The notes system is read from the decision-brief skill, as in _core_swap_build.py.
"""
import html
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
R = HERE.parent / "docs/research/terminal-core"
SKILL = Path.home() / ".claude/skills/decision-brief"
OUT = HERE / "2026-09-25-core-swap-final-round.html"
E = html.escape

FL = json.loads((R / "fluency/results.json").read_text())
Q = json.loads((R / "quality/summary.json").read_text())
ST = json.loads((R / "quality/static-facts.json").read_text())
PF = json.loads((R / "bake/results/perf/summary.json").read_text())
FN = json.loads((R / "vibes/final-2026-09-25.json").read_text())
W = json.loads((R / "vibes/weights-2026-09-25.json").read_text())["translation"]["weights"]

N = [0]


def fig(title, body, caption, fid):
    N[0] += 1
    return f'<figure id="{fid}">\n<span class="lbl">{N[0]:02d} &middot; {title}</span>\n{body}\n<figcaption>{caption}</figcaption>\n</figure>'


# 1 · six agents
def agents_table():
    trs = []
    for name, a in FL["agents"].items():
        core = "rio-vt" if name.startswith("rio") else "libghostty-vt"
        cells = "".join(f'<td class="{"ok" if v else "no"}">{"✓" if v else "✕"}</td>' for v in a["checks"].values())
        trs.append(f'<tr class="{ "rio" if core == "rio-vt" else "gh" }"><td class="who">{E(core)} <small>agent {name[-1]}</small></td>{cells}'
                   f'<td>{a["rows_held"]:,}</td><td>{a["tokens"] / 1000:.0f}k</td><td>{a["tool_uses"]}</td><td>{a["minutes"]:.1f}</td><td>{a["cold_build_s"]} s</td></tr>')
    m = FL["means"]
    for core, v in m.items():
        trs.append(f'<tr class="mean"><td class="who">{E(core)} <small>mean</small></td><td colspan="4">{E(v["checks_passed"])} checks</td><td></td>'
                   f'<td>{v["tokens"] / 1000:.0f}k</td><td>{v["tool_uses"]:.0f}</td><td>{v["minutes"]:.1f}</td><td>{v["cold_build_s"]} s</td></tr>')
    head = ("<tr><th>Agent</th><th>picture</th><th>cursor</th><th>replies</th><th>10,000 lines</th><th>rows held</th>"
            "<th>tokens</th><th>tool calls</th><th>minutes</th><th>cold build</th></tr>")
    return f'<div class="scroller"><table class="agents"><thead>{head}</thead><tbody>{"".join(trs)}</tbody></table></div>'


# 2 · what all three tripped on
TAGS = {
    "rio-vt": ["silent", "docs", "docs", "gap", "silent", "gap", "odd"],
    "libghostty-vt": ["blocks", "docs", "docs", "docs", "gap", "silent", "odd"],
}
TAG_WORD = {"blocks": "blocks the task", "silent": "fails silently", "docs": "docs wrong", "gap": "API gap", "odd": "surprising"}


def trips():
    cols = []
    for core, items in FL["found_by_all_three"].items():
        lis = "".join(f'<li><span class="tag {t}">{TAG_WORD[t]}</span>{E(x)}</li>' for x, t in zip(items, TAGS[core]))
        cols.append(f'<div class="col"><h4>{E(core)}</h4><ul>{lis}</ul></div>')
    return f'<div class="twocol">{"".join(cols)}</div>'


# 3 · twenty panes
def perf_bars():
    rows = []
    order = [("ghostty", "libghostty-vt"), ("rio", "rio-vt"), ("alacritty", "alacritty (today)"), ("wezterm", "wezterm-term")]
    mx_ms = max(PF[k]["text_ms_median"] for k, _ in order)
    mx_mb = max(PF[k]["kb_per_pane_full"] for k, _ in order) / 1024
    for k, label in order:
        ms, mb = PF[k]["text_ms_median"], PF[k]["kb_per_pane_full"] / 1024
        dim = "" if k in ("rio", "ghostty") else " dim"
        rows.append(f'<div class="pbar{dim}"><div class="l">{E(label)}</div>'
                    f'<div class="t"><span class="ms" style="width:{100 * ms / mx_ms:.0f}%"></span><em>{ms:.0f} ms</em></div>'
                    f'<div class="t"><span class="mb" style="width:{100 * mb / mx_mb:.0f}%"></span><em>{mb:.1f} MB per pane</em></div></div>')
    legend = '<div class="legend"><span><i class="ms"></i>time to take in 8 MB of text (shorter is faster)</span><span><i class="mb"></i>memory per pane holding 10,000 lines (shorter is lighter)</span></div>'
    return f'<div class="scroller"><div class="pbars">{"".join(rows)}</div></div>{legend}'


# 4 · build quality
QROWS = [("code", "The code"), ("tests", "Tests"), ("lint", "Lint"), ("ci", "CI"), ("fuzzing", "Fuzzing"), ("docs", "Docs"), ("build", "Build"), ("how_it_is_built", "How it is built")]


def quality_table():
    a, b = list(Q)[1], list(Q)[2]
    trs = "".join(f"<tr><th>{E(l)}</th><td>{E(Q[a][k])}</td><td>{E(Q[b][k])}</td></tr>" for k, l in QROWS)
    return f'<div class="scroller"><table class="quality"><thead><tr><th></th><th>{E(a)}</th><th>{E(b)}</th></tr></thead><tbody>{trs}</tbody></table></div>'


# 5 · philosophy
def philosophy():
    rio = ("<h4>Rio and Canario</h4><ul>"
           "<li>Canario, Rio's spin-off on the same engine, calls itself &ldquo;the terminal that thinks like a browser&rdquo;: spaces, splits, a command bar, link hints.</li>"
           "<li>Its tabs badge an agent that needs input and show progress, the job TD's agent wall does.</li>"
           "<li>Rio's recent commits pin each contract in a test, and give a missing cell an explicit &ldquo;none&rdquo; colour instead of a default: unknown is not zero.</li>"
           "<li>No agent files or AI policy in the repository; Copilot review is on; one person carries 92% of the commits.</li></ul>")
    gh = ("<h4>Ghostty and libghostty</h4><ul>"
          "<li>libghostty-vt is presented as Ghostty's core extracted for embedding: zero dependencies, not even libc, SIMD parsing, fuzzed and Valgrind-tested.</li>"
          "<li>The same post calls it an early public alpha with no API stability promised yet.</li>"
          "<li>The project says it is written with plenty of AI assistance, keeps AGENTS.md and CLAUDE.md, and holds outside contributors to a strict AI policy that maintainers are exempt from.</li>"
          "<li>Sixty-one thousand stars against Rio's seven and a half thousand: the popularity pull is real.</li></ul>")
    return f'<div class="twocol"><div class="col">{rio}</div><div class="col">{gh}</div></div>'


# 6 · where the weights land
DIM_LABEL = {"performance": "speed and memory", "agent_fluency": "agent fluency", "published_unpatched": "published, unpatched",
             "pictures": "pictures", "maintainers": "maintainers", "other_picture_protocols": "Sixel and iTerm2", "build_toolchain": "build toolchain"}
DIM_COL = {"performance": "#3987e5", "agent_fluency": "#199e70", "published_unpatched": "#8f6fd8", "pictures": "#e0843a",
           "maintainers": "#8b8a80", "other_picture_protocols": "#c98500", "build_toolchain": "#5b5a54"}


def weights_bars():
    wsum = sum(v["weight"] for k, v in W.items() if v["weight"] > 0)
    rows = []
    for core in ("rio-vt", "libghostty-vt"):
        d = FN["dims"][core]
        segs = ""
        for k in DIM_LABEL:
            s = d.get(k)
            if s is None:
                continue
            share = W[k]["weight"] * s / wsum
            segs += f'<span style="width:{100 * share:.2f}%;background:{DIM_COL[k]}" title="{DIM_LABEL[k]}: {s:.2f} × weight {W[k]["weight"]}"></span>'
        rows.append(f'<div class="wbar"><div class="l">{E(core)}</div><div class="t">{segs}</div><div class="v">{FN["scores_under_sealed_weights"][core]:.2f}</div></div>')
    legend = "".join(f'<span><i style="background:{DIM_COL[k]}"></i>{DIM_LABEL[k]} × {W[k]["weight"]:g}</span>' for k in DIM_LABEL)
    return f'<div class="scroller"><div class="wbars">{"".join(rows)}</div></div><div class="legend">{legend}</div>'


def build():
    N[0] = 0
    m = FL["means"]
    rv, gv = FN["scores_under_sealed_weights"]["rio-vt"], FN["scores_under_sealed_weights"]["libghostty-vt"]
    others = sum(v["weight"] for k, v in W.items() if k != "performance" and v["weight"] > 0)
    f = {
        "agents": fig("Six agents, one task", agents_table(),
                      f"<b>Every rio-vt agent passed all four checks; every libghostty-vt agent missed the history one.</b> libghostty-vt's scrollback "
                      f"setting is documented as lines and counts bytes, so 10,000 keeps 620 rows and no setting asks for lines; one agent bisected a byte "
                      f"budget and landed on 10,295. The libghostty-vt agents also used about {100 * (m['libghostty-vt']['tokens'] / m['rio-vt']['tokens'] - 1):.0f}% "
                      f"more tokens and {100 * (m['libghostty-vt']['tool_uses'] / m['rio-vt']['tool_uses'] - 1):.0f}% more tool calls for the same time. Each result "
                      f"was re-run from the agent's own program against answers sealed before any agent reported.", "f-agents"),
        "trips": fig("What all three agents tripped on", trips(),
                     "<b>Both cores have traps, and they are different in kind.</b> rio-vt's are a stale README and two silent ways to lose a picture; "
                     "libghostty-vt's include one that stops the task and a PNG decoder that ships unusable. Only findings all three agents reported "
                     "independently are listed.", "f-trips"),
        "perf": fig("Twenty panes", perf_bars(),
                    "<b>libghostty-vt is the fastest; rio-vt is close behind on speed and level on memory.</b> Both beat today's alacritty on both. "
                    "Medians of three rounds on this machine, with each core holding 10,000 lines of history.", "f-perf"),
        "quality": fig("Build quality", quality_table(),
                       "<b>Ghostty's core is the more rigorously engineered; the Rust layer TD would call is the weaker part of it.</b> rio-vt is one "
                       "crate whose suite passes here. Ghostty's own tests would not run outside its Nix environment, after two attempts that failed "
                       "for different reasons; that is a note on portability, not a verdict on its code.", "f-quality"),
        "phil": fig("How each is built, and what each is for", philosophy(),
                    "<b>Both projects are built with AI assistance.</b> Rio's commits show it in their shape; Ghostty states it and sets rules "
                    "for outside contributors. Canario's agent badges do the job TD's agent wall does. Drawn from each project's own pages and "
                    "repository, 2026-09-25.", "f-phil"),
        "weights": fig("Your weights, with fluency measured", weights_bars(),
                       f"<b>rio-vt {rv:.2f}, libghostty-vt {gv:.2f}.</b> Each bar is made of your weights times each core's score. For libghostty-vt to "
                       f"tie, speed and memory would have to weigh {FN['performance_weight_to_flip']:.1f}, more than every other weight together "
                       f"({others:g}). Weights are the ones committed before the runs.", "f-weights"),
    }
    body = f"""
<div class="wrap">
<header class="hero">
<p class="kicker">Terminal Delight &middot; core swap, final round &middot; 2026-09-25</p>
<h1>Final Round</h1>
<p class="lede">You narrowed the field to Ghostty's core and Rio's, and asked three things of them: can an agent work with it fluently, how well is it
built, and does its philosophy fit TD. Six fresh agents got the same task, three per core, and every answer was checked against results
written down first. Rio's agents all finished it. Ghostty's all stopped at the same documented-wrong setting. <b>Under the weights you gave
before any of this ran, rio-vt scores {rv:.2f} and libghostty-vt {gv:.2f}.</b> Ghostty's core is faster and more rigorously tested underneath;
the layer TD would touch is where the trouble was.</p>
</header>

<h2 class="sec">Can an agent work with it?</h2>
<p>The first test was the one you named: give fresh agents the same job on each core and see who finishes it.</p>
{f['agents']}
{f['trips']}

<h2 class="sec">Is it fast enough, and well built?</h2>
<p>Finishing a job says little about the code underneath it, so both cores were also run at twenty panes and read the same way.</p>
{f['perf']}
{f['quality']}

<h2 class="sec">Does it think like TD?</h2>
<p>The vibe has evidence too: what each project says it is for, and how its own repository shows it being built.</p>
{f['phil']}

<h2 class="sec">Where your weights land</h2>
<p>With agent fluency measured, every weight you set before the runs now has a number behind it.</p>
{f['weights']}
<p>Nothing in TD has changed. If you pick rio-vt, the next step is the test filed earlier, now for one core: rio-vt as the session host's
core, a window drawing what arrives, pictures compared.</p>
<p class="foot">Everything behind this page is in docs/research/terminal-core: fluency/, quality/, vibes/ and bake/results/perf/.</p>
</div>
"""
    return body


CSS = """
table.agents { border-collapse: separate; border-spacing: 2px; min-width: 860px; width: 100%; font-size: 12.5px; }
table.agents th { font-family: var(--mono); font-size: 9.5px; letter-spacing: .06em; text-transform: uppercase; color: var(--text-muted); font-weight: 500; padding: 4px 6px; }
table.agents td { text-align: center; padding: 7px 6px; background: #181816; border-radius: 5px; font-family: var(--mono); font-size: 11.5px; color: var(--text-secondary); }
table.agents td.who { text-align: left; background: transparent; font-family: var(--sans); font-size: 13px; color: #fff; white-space: nowrap; }
table.agents td.who small { color: var(--text-muted); font-family: var(--mono); font-size: 10px; margin-left: 4px; }
table.agents td.ok { background: rgba(25,158,112,.22); color: #6fe0b0; font-size: 14px; }
table.agents td.no { background: rgba(208,59,59,.22); color: #f3a0a0; font-size: 14px; }
table.agents tr.mean td { background: transparent; border-top: 1px solid var(--line); color: #fff; }
table.agents tr.rio td.who { border-left: 3px solid #3987e5; padding-left: 8px; }
table.agents tr.gh td.who { border-left: 3px solid #b18cf0; padding-left: 8px; }
.twocol { display: grid; grid-template-columns: repeat(2, minmax(0,1fr)); gap: 16px; }
.twocol .col { border: 1px solid var(--line); border-radius: 10px; padding: 12px 14px; background: #151513; }
.twocol h4 { margin: 0 0 8px; font-size: 14px; color: #fff; }
.twocol ul { margin: 0; padding-left: 16px; font-size: 12.5px; color: var(--text-secondary); line-height: 1.5; }
.twocol li { margin-bottom: 7px; }
.tag { display: inline-block; font-family: var(--mono); font-size: 9px; padding: 0 5px; border-radius: 3px; border: 1px solid; margin-right: 6px; vertical-align: 1px; white-space: nowrap; }
.tag.blocks { color: #f3a0a0; border-color: rgba(208,59,59,.7); }
.tag.silent { color: #f0a58a; border-color: rgba(236,131,90,.6); }
.tag.docs { color: #f5c95f; border-color: rgba(201,133,0,.6); }
.tag.gap { color: #8fbdf2; border-color: rgba(57,135,229,.6); }
.tag.odd { color: #a9a89e; border-color: rgba(139,138,128,.5); }
.pbars { display: grid; gap: 10px; min-width: 620px; }
.pbar { display: grid; grid-template-columns: 150px 1fr 1fr; gap: 12px; align-items: center; }
.pbar.dim { opacity: .5; }
.pbar .l { font-size: 13px; color: #fff; }
.pbar .t { position: relative; height: 18px; background: #1d1d1b; border-radius: 4px; overflow: hidden; }
.pbar .t span { display: block; height: 100%; }
.pbar .t span.ms, .legend i.ms { background: #3987e5; }
.pbar .t span.mb, .legend i.mb { background: #199e70; }
.pbar .t em { position: absolute; left: 8px; top: 0; line-height: 18px; font-style: normal; font-family: var(--mono); font-size: 10.5px; color: #fff; }
table.quality { width: 100%; min-width: 820px; border-collapse: collapse; font-size: 12.5px; }
table.quality th, table.quality td { text-align: left; vertical-align: top; padding: 8px 10px; border-bottom: 1px solid #2a2a27; }
table.quality thead th { font-family: var(--mono); font-size: 10px; letter-spacing: .06em; color: var(--text-muted); font-weight: 500; }
table.quality tbody th { color: #fff; font-weight: 600; width: 120px; }
table.quality td { color: var(--text-secondary); line-height: 1.5; }
.wbars { display: grid; gap: 12px; min-width: 620px; }
.wbar { display: grid; grid-template-columns: 150px 1fr 60px; gap: 12px; align-items: center; }
.wbar .l { font-size: 13.5px; color: #fff; }
.wbar .t { display: flex; height: 26px; border-radius: 6px; overflow: hidden; background: #1d1d1b; }
.wbar .t span { display: block; height: 100%; border-right: 1px solid #111110; }
.wbar .v { font-family: var(--mono); font-size: 16px; color: #fff; font-weight: 700; text-align: right; }
@media (max-width: 760px) { .twocol { grid-template-columns: 1fr; } }
"""


def assemble(body):
    base = (SKILL / "assets/base.css").read_text()
    notes_css = (SKILL / "assets/notes.css").read_text()
    notes_js = (SKILL / "assets/notes.js").read_text()
    markup = (SKILL / "reference/notes-markup.html").read_text()
    css = (HERE / "_core_swap.css").read_text() + CSS
    if re.search(r"</script", notes_js, re.I):
        sys.exit("notes.js contains a closing script tag; inlining it would break the page")
    markup = markup.replace("<YYYY-MM-DD>-<topic-slug>.html", OUT.name)
    markup = markup.replace("<!-- then paste the contents of assets/notes.js inline, inside a <script> tag -->", "<script>\n" + notes_js + "\n</script>")
    if OUT.name not in markup or notes_js[:40] not in markup:
        sys.exit("the notes markup block changed shape; update this builder")
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Final Round</title>
<style>
{base}
{notes_css}
{css}
</style>
</head>
<body>
{body}
{markup}
</body>
</html>
"""


if __name__ == "__main__":
    page = assemble(build())
    OUT.write_text(page)
    print(f"wrote {OUT} ({len(page):,} bytes, {N[0]} figures)")
