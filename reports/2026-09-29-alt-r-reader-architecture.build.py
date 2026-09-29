#!/usr/bin/env python3
"""Build reports/2026-09-29-alt-r-reader-architecture.html, the Gate 2 brief for the Alt+R reader.

    python3 reports/2026-09-29-alt-r-reader-architecture.build.py

Shares its mockup ink, helpers and assembly with the Gate 1 build script beside it
(2026-09-29-alt-r-reader.build.py), so the two briefs read as one piece. Keeps the
wallpaper this brief was first built on, and carries notes and concurs already saved
into the .html across a rebuild, attributes included.
"""
import html
import importlib.util
import pathlib
import re
import subprocess
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
NAME = "2026-09-29-alt-r-reader-architecture.html"
OUT = HERE / NAME
_spec = importlib.util.spec_from_file_location("gate1", HERE / "2026-09-29-alt-r-reader.build.py")
g1 = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(g1)
SKILL = g1.SKILL
esc = g1.esc


# ---------------------------------------------------------------------------
# Figure 01 · the five attempts and the sixth
# ---------------------------------------------------------------------------
ATTEMPTS = [
    ("15 Jun", "7a43d17", "mirror, fit to the glass",
     "the pane’s rows, scaled to the tighter axis",
     "a tall pane fits by its height: a narrow column mid-glass", "column"),
    ("20 Jun", "ef74db9", "wrap to fit",
     "re-wrap every row at the glass width",
     "rows already broken at 48 columns can’t get longer: a ribbon", "ribbon"),
    ("31 Aug", "9fabb41", "fill the width",
     "scale the pane’s columns to the glass",
     "no ribbon, but 3.65× letters on a 48-column pane: today", "big"),
    ("31 Aug", "128d6cf", "the document model",
     "rejoin the rows the terminal broke, by guessing",
     "shell lines heal; a Claude reply never does, every row is indented", "healed"),
    ("31 Aug", "121e475", "the whole conversation",
     "all the scrollback, mirrored; the transcript source set aside",
     "all of history, still at the pane’s width", "scroll"),
    ("29 Sep", "Gate 1", "attempt six",
     "a source per kind of pane, at the reader’s own size",
     "a lent document · a transcript · a shell’s lines joined on its own flag", "six"),
]


def mini(kind, x, y, w=150, h=86):
    out = [f'<rect class="g-panel" x="{x}" y="{y}" width="{w}" height="{h}" rx="7"/>']
    ix, iy, iw = x + 12, y + 10, w - 24

    def bar(bx, by, bw, bh=3, cls="bar"):
        out.append(f'<rect class="{cls}" x="{bx:.1f}" y="{by:.1f}" width="{bw:.1f}" height="{bh}" rx="1.5"/>')

    if kind in ("column", "ribbon"):
        cw = iw * 0.27
        cx = ix + (iw - cw) / 2
        for i in range(11):
            bar(cx, iy + i * 6, cw * (0.55 + 0.45 * ((i * 7) % 5) / 4))
    elif kind == "big":
        for i in range(4):
            bar(ix, iy + i * 17, iw * (0.62 + 0.38 * ((i * 3) % 4) / 3), 9)
    elif kind in ("healed", "scroll"):
        for i in range(12):
            if i % 4 == 0:
                bar(ix, iy + i * 5.4, iw * 0.95, 2.4, "bar ok")
            else:
                bar(ix, iy + i * 5.4, iw * 0.27 * (0.6 + 0.4 * ((i * 5) % 3) / 2), 2.4)
        if kind == "scroll":
            out.append(f'<rect class="thumb" x="{x + w - 7}" y="{y + 44}" width="3" height="30" rx="1.5"/>')
    elif kind == "six":
        for i in range(10):
            bar(ix, iy + i * 5.4, iw * (0.98 if i % 3 else 0.6), 2.4, "bar ok")
        out.append(f'<rect class="strip" x="{ix - 4}" y="{y + h - 16}" width="{iw + 8}" height="10" rx="2"/>')
    return "\n".join(out)


def fig_attempts():
    rows = []
    for i, (date, ref, name, tried, hit, kind) in enumerate(ATTEMPTS):
        y = 12 + i * 104
        six = kind == "six"
        tone = "ready" if six else "ink"
        rows.append(f'<text x="14" y="{y + 20}" class="mt {tone}" font-size="13" font-weight="700">{i + 1}</text>')
        rows.append(f'<text x="14" y="{y + 38}" class="mt ink2" font-size="10">{esc(date)}</text>')
        rows.append(mini(kind, 60, y + 2))
        rows.append(f'<text x="232" y="{y + 20}" class="mt {tone}" font-size="13" font-weight="700">{esc(name)}</text>')
        rows.append(f'<text x="{232 + len(name) * 7.9 + 12:.0f}" y="{y + 20}" class="mt ink2" font-size="10">{esc(ref)}</text>')
        rows.append(f'<text x="232" y="{y + 44}" class="mt ink2" font-size="11">{"approved" if six else "tried"}</text>')
        rows.append(f'<text x="310" y="{y + 44}" class="mt ink" font-size="11.5">{esc(tried)}</text>')
        rows.append(f'<text x="232" y="{y + 66}" class="mt {"ready" if six else "fail"}" font-size="11">{"reads" if six else "hit"}</text>')
        rows.append(f'<text x="310" y="{y + 66}" class="mt {tone}" font-size="11.5">{esc(hit)}</text>')
        if not six:
            rows.append(f'<line class="grid" x1="14" y1="{y + 96}" x2="946" y2="{y + 96}"/>')
    h = 12 + len(ATTEMPTS) * 104
    return (f'<svg viewBox="0 0 960 {h}" role="img" aria-label="Six attempts at the reader in order. Five read the pane’s grid '
            f'at the pane’s width and hit a narrow column, a ribbon, magnified letters, or replies that never heal. The sixth reads '
            f'a source per kind of pane at the reader’s own size.">\n' + "\n".join(rows) + "\n</svg>")


# ---------------------------------------------------------------------------
# Figures 02–04 · where it lands, one frame built up in three steps
# ---------------------------------------------------------------------------
def arch(step):
    s = []

    def node(x, y, w, a, b, cls="box"):
        s.append(f'<rect class="{cls}" x="{x}" y="{y}" width="{w}" height="46" rx="8"/>')
        s.append(f'<text x="{x + 12}" y="{y + 19}" class="mt ink" font-size="11.5" font-weight="700">{esc(a)}</text>')
        s.append(f'<text x="{x + 12}" y="{y + 36}" class="mt ink2" font-size="10.5">{esc(b)}</text>')

    s.append('<rect class="bound" x="16" y="14" width="430" height="262" rx="14"/>')
    s.append('<text x="30" y="34" class="mt ink2" font-size="10.5" font-weight="700">▣ A PANE</text>')
    for x, w, t in ((32, 118, "Terminal"), (158, 132, "Workbench"), (298, 132, "Document")):
        s.append(f'<rect class="box" x="{x}" y="44" width="{w}" height="24" rx="12"/>')
        s.append(f'<text x="{x + w / 2}" y="60" class="mt ink" font-size="11" text-anchor="middle">{t} face</text>')
    node(32, 84, 398, "floating square", "its own document view, over the terminal face")
    if step >= 2:
        node(32, 150, 398, "document view", "stays the pane’s; while lent, the face says ‘being read’", "struct" if step == 2 else "box")
    else:
        node(32, 150, 398, "document view", "on the Document face")
    node(32, 216, 398, "the terminal’s grid", "every row carries its soft-wrap flag ↩")
    s.append('<rect class="bound" x="560" y="14" width="384" height="330" rx="14"/>')
    s.append('<text x="574" y="34" class="mt ink2" font-size="10.5" font-weight="700">▣ THE READER · ALT+R</text>')
    if step < 3:
        node(576, 216, 352, "the grid’s rows, ↩ dropped", "blown up to the pane’s width", "fail" if step == 1 else "box")
    else:
        node(576, 216, 352, "lines at the reader’s own width", "shell: joined on ↩ · agent: its transcript", "struct")
    s.append('<path class="wire" d="M430 239 H574" marker-end="url(#ah4)"/>')
    if step >= 2:
        node(576, 150, 352, "the lent document view", "drawn here, still owned by the pane", "struct" if step == 2 else "box")
        s.append(f'<path class="wire{" struct" if step == 2 else ""}" d="M430 173 H574" marker-end="url(#ah4)"/>')
        s.append(f'<text x="452" y="192" class="mt {"struct" if step == 2 else "ink2"}" font-size="10">lent on Alt+R</text>')
        s.append(f'<path class="wire{" struct" if step == 2 else ""}" d="M430 107 H502 V173"/>')
    if step >= 3:
        node(576, 84, 352, "which source: one pure table", "face · square up · transcript certain", "struct")
        s.append('<rect class="mine" x="576" y="282" width="300" height="46" rx="8"/>')
        s.append('<text x="588" y="301" class="mt ink" font-size="11.5" font-weight="700">the live footer</text>')
        s.append('<text x="588" y="318" class="mt ink2" font-size="10.5">input rows, a permission question</text>')
        s.append('<rect class="unk" x="576" y="370" width="352" height="46" rx="8"/>')
        s.append('<text x="588" y="389" class="mt ink" font-size="11.5" font-weight="700">the agent’s transcript, on disk</text>')
        s.append('<text x="588" y="406" class="mt ink2" font-size="10.5">read from the last byte it reached, each second</text>')
        s.append('<path class="wire struct" d="M905 370 V264" marker-end="url(#ah4)"/>')
        s.append('<polygon class="struct" points="905,347 915,357 905,367 895,357"/>')
        s.append('<text x="890" y="361" class="mt struct" font-size="10" text-anchor="end">certain? the probe already knows</text>')
    h = 428 if step >= 3 else 356
    return (f'<svg viewBox="0 0 960 {h}" role="img" aria-label="Where the reader lands, step {step} of 3: a pane with three faces, '
            f'a floating square, a document view and a grid, beside the reader'
            + (' reading the grid with its flags dropped' if step == 1 else '')
            + (', which also draws a lent document view' if step >= 2 else '')
            + (', lays text out at its own width from a joined grid or a certain transcript, and chooses its source from one table' if step >= 3 else '')
            + '.">\n<defs><marker id="ah4" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">'
            '<path d="M0 0 L10 5 L0 10 z" class="ah"/></marker></defs>\n' + "\n".join(s) + "\n</svg>")


# ---------------------------------------------------------------------------
# Figure 05 · a shell's lines joined on the terminal's own flag
# ---------------------------------------------------------------------------
LOG = [
    "5ba60ed Merge pull request #883 from parker-brown-family/feat/border-phosphor-gauge",
    "c496fb0 The reading rail's lit blocks follow the phosphor gauge too",
    "e713847 The GAUGES tray has a phosphor gauge, and every lit border's glow goes through it",
]
LS = [  # COLUMNS=48 ls -C app/src/docview, the first three of its seven rows
    "backend.rs   markdown_view.rs  poppler.rs",
    "cache.rs     md_notes.rs       pref.rs",
    "cdp.rs       mpv.rs            progress.rs",
]


def fig_shell():
    cols, lh = 48, 15.0
    pc = 5.6
    rows = [("$ git log --oneline -3 origin/main", False)]
    for line in LOG:
        parts = [line[i:i + cols] for i in range(0, len(line), cols)]
        rows += [(p, k < len(parts) - 1) for k, p in enumerate(parts)]
    rows.append(("$ ls app/src/docview", False))
    rows += [(r, False) for r in LS] + [("⋮", False)]
    out = []
    px, py = 20, 40
    pw = cols * pc + 44
    out.append('<text x="20" y="24" class="mt lane" font-size="11">THE PANE’S GRID · 48 COLUMNS · ↩ = THE ROW’S SOFT-WRAP FLAG</text>')
    out.append(f'<rect class="box" x="{px}" y="{py}" width="{pw:.0f}" height="{len(rows) * lh + 14:.0f}" rx="7"/>')
    for i, (t, soft) in enumerate(rows):
        y = py + 7 + lh * 0.78 + i * lh
        out.append(f'<text x="{px + 8}" y="{y:.1f}" class="mt" font-size="{pc / 0.6:.2f}" xml:space="preserve"><tspan class="txt">{esc(t)}</tspan></text>')
        if soft:
            out.append(f'<text x="{px + 8 + cols * pc + 10:.1f}" y="{y:.1f}" class="mt ready" font-size="11">↩</text>')
    nx = px + pw + 30
    out.append(f'<text x="{nx:.0f}" y="{py + 30}" class="mt struct" font-size="11.5" font-weight="700">join on ↩</text>')
    out.append(f'<text x="{nx:.0f}" y="{py + 48}" class="mt ink2" font-size="10.5">the flag the terminal set when it wrapped the row;</text>')
    out.append(f'<text x="{nx:.0f}" y="{py + 64}" class="mt ink2" font-size="10.5">the same one both cores’ own re-wrap joins on</text>')
    out.append(f'<text x="{nx:.0f}" y="{py + 100}" class="mt ink2" font-size="11.5" font-weight="700">today</text>')
    out.append(f'<text x="{nx:.0f}" y="{py + 118}" class="mt ink2" font-size="10.5">the flag is dropped, the width guessed, and</text>')
    out.append(f'<text x="{nx:.0f}" y="{py + 134}" class="mt ink2" font-size="10.5">the terminal stays locked for the whole read</text>')
    pb = py + len(rows) * lh + 14
    ry = pb + 44
    out.append(f'<path class="wire struct" d="M{px + pw / 2:.0f} {pb + 4:.0f} V{ry - 6:.0f}" marker-end="url(#ah5)"/>')
    lines = ["$ git log --oneline -3 origin/main"] + LOG + ["$ ls app/src/docview"] + LS + ["⋮"]
    rc = (926 - 16) / 175
    out.append(f'<text x="{px + pw / 2 + 12:.0f}" y="{pb + 28:.0f}" class="mt lane" font-size="11">THE READER · 175 COLUMNS</text>')
    out.append(f'<rect class="box rdr" x="20" y="{ry:.0f}" width="926" height="{len(lines) * lh + 14:.0f}" rx="7"/>')
    for i, t in enumerate(lines):
        y = ry + 7 + lh * 0.78 + i * lh
        cls = "dim" if t in LS or t == "⋮" else "txt"
        out.append(f'<text x="28" y="{y:.1f}" class="mt" font-size="{rc / 0.6:.2f}" xml:space="preserve"><tspan class="{cls}">{esc(t)}</tspan></text>')
    rb = ry + len(lines) * lh + 14
    out.append(f'<text x="28" y="{rb + 18:.0f}" class="mt ink2" font-size="10">ls broke its own lines for 48 columns, so they stay as printed</text>')
    by = rb + 34
    out.append(f'<rect class="mine" x="20" y="{by:.0f}" width="926" height="46" rx="8"/>')
    out.append(f'<text x="34" y="{by + 19:.0f}" class="mt mine" font-size="11" font-weight="700">kept, not rebuilt</text>')
    out.append(f'<text x="34" y="{by + 36:.0f}" class="mt ink" font-size="10.5">joined history is cached until a resize, a clear or rows falling off the top; each update re-reads the seam and the screen</text>')
    ay = by + 66
    out.append(f'<text x="20" y="{ay:.0f}" class="mt unk" font-size="10.5">vim, htop and anything else on the alternate screen: drawn as its screen, with no history, and the reader says so</text>')
    return (f'<svg viewBox="0 0 960 {ay + 14:.0f}" role="img" aria-label="A 48-column shell grid where each row of a wrapped '
            f'git log line carries a soft-wrap flag; joined on that flag they come back as three whole lines in a 175-column reader, '
            f'while ls output that ls broke itself stays as printed. Joined history is cached and only the seam and the screen are re-read.">\n'
            f'<defs><marker id="ah5" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">'
            f'<path d="M0 0 L10 5 L0 10 z" class="ah"/></marker></defs>\n' + "\n".join(out) + "\n</svg>")


# ---------------------------------------------------------------------------
# Figures 06–07 · an agent's conversation, from its transcript
# ---------------------------------------------------------------------------
STAGES = [
    ("1", "which transcript", ["the pane’s certain path,", "refreshed every 2 s today;", "none → SCREEN, labelled"], "struct"),
    ("2", "read what’s new", ["from the last byte read,", "polled once a second;", "one line can be 1.1 MB"], "mine"),
    ("3", "fold it", ["join a reply’s blocks;", "a tool call and result", "become one line"], "mine"),
    ("4", "lay it out", ["Markdown to styled lines,", "tables as aligned text,", "at the reader’s width"], "mine"),
    ("5", "the live footer", ["from the screen: input,", "a permission question,", "a reply being written"], "mine"),
]


def fig_pipeline():
    out = []
    w, gap, x0, y0, h = 178, 10, 14, 34, 132
    for i, (n, title, lines, cls) in enumerate(STAGES):
        x = x0 + i * (w + gap)
        out.append(f'<rect class="{cls}" x="{x}" y="{y0}" width="{w}" height="{h}" rx="10"/>')
        out.append(f'<text x="{x + 14}" y="{y0 + 24}" class="mt {cls}" font-size="11" font-weight="700">{n}</text>')
        out.append(f'<text x="{x + 30}" y="{y0 + 24}" class="mt ink" font-size="12.5" font-weight="700">{esc(title)}</text>')
        for j, l in enumerate(lines):
            out.append(f'<text x="{x + 14}" y="{y0 + 54 + j * 19}" class="mt ink2" font-size="9.6">{esc(l)}</text>')
        if i < len(STAGES) - 1:
            ax = x + w + 2
            out.append(f'<path class="wire" d="M{ax} {y0 + h / 2} H{ax + gap - 4}" marker-end="url(#ahp)"/>')
    out.append('<text x="14" y="20" class="mt lane" font-size="11">AN AGENT’S PANE · FROM ITS TRANSCRIPT TO THE GLASS</text>')
    out.append(f'<text x="14" y="{y0 + h + 28}" class="mt ink2" font-size="10.5">blue = exists today, reused · amber = new</text>')
    return (f'<svg viewBox="0 0 960 {y0 + h + 40}" role="img" aria-label="Five stages: the pane’s certain transcript path, already '
            f'refreshed every two seconds; reading only new bytes each second; folding records into prompts, replies and one line per '
            f'tool call; laying Markdown out as styled lines at the reader’s width; and a live footer of the pane’s own rows.">\n'
            f'<defs><marker id="ahp" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">'
            f'<path d="M0 0 L10 5 L0 10 z" class="ah"/></marker></defs>\n' + "\n".join(out) + "\n</svg>")


CENSUS = [
    ("Your typed prompts", "2 of 162 user records", "mine", "shown whole"),
    ("Reply text", "27 text blocks across 122 replies", "mine", "drawn as Markdown; a reply split over up to 6 records is joined"),
    ("Tool calls", "159, each with its result", "mine", "one line each; a click opens the result"),
    ("Tool results", "84% of the file’s bytes, up to 547 KB each", "mine", "behind their tool line; spilled ones by link"),
    ("Thinking", "121 blocks, 91 of them empty", "unk", "hidden"),
    ("Attachments, modes, file history, queue", "most of the other records", "unk", "skipped"),
    ("Compaction", "35 across the whole machine", "mine", "a divider"),
    ("Subagents", "their own files, beside the session", "mine", "a line naming the agent; its file on click"),
    ("Images in results", "21 in this session", "mine", "a placeholder"),
]


def census_rows():
    return "\n".join(f'<tr><td>{esc(a)}</td><td>{esc(b)}</td><td><span class="dot {d}"></span>{esc(c)}</td></tr>'
                     for a, b, d, c in CENSUS)


# ---------------------------------------------------------------------------
# Figure 08 · lending a document, in order
# ---------------------------------------------------------------------------
def fig_lend():
    lanes = [("you", 50), ("the pane", 250), ("the reader", 450), ("the document view", 650)]
    out = []
    for name, x in lanes:
        out.append(f'<text x="{x}" y="22" class="mt ink" font-size="11.5" font-weight="700" text-anchor="middle">{esc(name)}</text>')
        out.append(f'<line class="grid" x1="{x}" y1="32" x2="{x}" y2="372"/>')

    def msg(y, a, b, label, cls="wire", note=None):
        xa, xb = dict(lanes)[a], dict(lanes)[b]
        out.append(f'<path class="{cls}" d="M{xa} {y} H{xb}" marker-end="url(#ah8)"/>')
        lx = (xa + xb) / 2
        out.append(f'<text x="{lx}" y="{y - 7}" class="mt ink" font-size="10.5" text-anchor="middle">{esc(label)}</text>')
        if note:
            out.append(f'<text x="{lx}" y="{y + 15}" class="mt ink2" font-size="9.5" text-anchor="middle">{esc(note)}</text>')

    def act(y, lane, label, cls="ink2"):
        x = dict(lanes)[lane]
        for k, part in enumerate(label.split("\n")):
            out.append(f'<text x="{x + 10}" y="{y + k * 14}" class="mt {cls}" font-size="10">{esc(part)}</text>')

    msg(56, "you", "the pane", "Alt+R")
    act(80, "the pane", "marks the view lent;\nits face says ‘being read’")
    msg(128, "the reader", "the pane", "the lent view?", "wire struct", "asked every frame; no handle kept")
    msg(176, "the reader", "the document view", "drawn in the glass", "wire struct")
    act(200, "the document view", "a new box: measure again", "struct")
    act(218, "the document view", "HTML renders again · PDF redraws tiles")
    act(234, "the document view", "video keeps playing · Markdown keeps its place")
    msg(276, "you", "the document view", "Esc", "wire", "an open note box closes first")
    msg(318, "you", "the pane", "Esc again", "wire")
    act(342, "the pane", "takes it back; the view\nmeasures the pane’s box")
    return (f'<svg viewBox="0 0 960 380" role="img" aria-label="Lending a document in order: Alt+R marks the view lent and the '
            f'pane’s face says being read; the reader asks for the lent view every frame and draws it; the view measures its new box '
            f'and each kind lays itself out again; Esc closes an open note box first, then the reader, and the pane takes the view back.">\n'
            f'<defs><marker id="ah8" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">'
            f'<path d="M0 0 L10 5 L0 10 z" class="ah"/></marker></defs>\n' + "\n".join(out) + "\n</svg>")


KINDS = [
    ("HTML", "laid out again at the reader’s width by headless Chromium",
     "a fresh render: soft for about a quarter second, and again on the way back"),
    ("PDF", "pages at the reader’s width, tiles drawn again at the new scale",
     "tiles arrive three at a time, roughly a tenth of a second each"),
    ("Video", "the same player keeps playing, drawn at the new size",
     "decoding at up to the file’s own pixels costs CPU nobody has measured"),
    ("Picture", "fitted to the glass", "never past its own pixels: a small picture stays small until you zoom"),
    ("Markdown", "re-flowed at the reader’s width",
     "loses its place today; it has to re-anchor to the block at the top"),
]
KEYS = [
    ("Esc", "closes the reader", "closes an open note box or dialog first, then the reader"),
    ("Page Up / Down, Ctrl+Home / End", "pages the text", "pages the document by its own height"),
    ("Ctrl+wheel", "letter size", "the document’s zoom"),
    ("0 · 1 · + · − · arrows", "reach the agent", "fit, actual size, zoom, pan"),
    ("Any other key", "reaches the agent", "reaches the agent from a square; swallowed on a Document face"),
]


def table(head, rows, first_bold=True):
    body = "\n".join("<tr>" + "".join(f"<td>{esc(c)}</td>" for c in r) + "</tr>" for r in rows)
    return (f'<table class="wide cases"><thead><tr>' + "".join(f"<th>{esc(h)}</th>" for h in head)
            + f"</tr></thead><tbody>\n{body}\n</tbody></table>")


# ---------------------------------------------------------------------------
# The page
# ---------------------------------------------------------------------------
OWN_CSS = g1.OWN_CSS + r"""
svg .bar.ok { fill: color-mix(in srgb, var(--s3) 70%, transparent); }
svg .thumb { fill: var(--dim); }
svg .strip { fill: color-mix(in srgb, var(--warning) 45%, transparent); }
pre.shape { font-size: 12.5px; }
"""

SHAPE = """// which source: pure, beside the rule the reader follows today (pane.rs)
pub enum ReadFrom { Grid, Document(DocSeat), Transcript }
pub fn read_from(face: Face, square_up: bool, transcript: Option<Bond>) -> ReadFrom;

// a shell: rows keep their soft-wrap flag; joined history is cached (doc.rs, vt/)
pub struct GridLine { pub text: String, pub runs: Vec<TextRun>, pub wrapped: bool }
pub fn join_wrapped(rows: &[GridLine]) -> Document;          // exact, no width test
fn evicted(&self) -> Option<u64>;                            // rio counts rows gone; alacritty says None

// an agent: a conversation model and a tail that reads only new bytes (new module)
pub struct Tail { path: PathBuf, offset: u64, partial: Vec<u8> }
impl Tail { pub fn poll(&mut self, conv: &mut Conversation) -> io::Result<Polled>; }
pub enum Polled { Unchanged, Grew, Reset }
pub struct Conversation { pub entries: Vec<Entry>, pub rev: u64 }
pub enum EntryKind { Prompt(String), Reply { message_id: String, markdown: String },
    Tool { id: String, name: String, gist: String, state: ToolState, output: Option<OutputRef> },
    Command(String), Boundary(Boundary), Error(String) }
impl DocumentSource for TranscriptSource<'_> { fn document(&self, b: RowBudget) -> Document; }
impl ToolProbe { pub fn path(&self) -> Option<&Path>; }       // the certain path, no I/O

// a document: lent, never moved (docview.rs, pane)
impl DocumentView { pub fn lend(&mut self, reading: bool, cx: &mut Context<Self>);
                    pub fn page(&mut self, dir: i8, cx: &mut Context<Self>) -> bool; }
trait Backend { fn reseated(&mut self) {} }                  // Markdown re-anchors to its top block
impl TerminalView { pub(crate) fn lent_view(&self) -> Option<Entity<DocumentView>>; }"""

BODY = f"""
<header class="top"><div class="wrap">
  <p class="eyebrow">Gate 2 of 4 &middot; architecture &middot; Terminal Delight &middot; 2026-09-29</p>
  <h1>Attempt six</h1>
  <p class="lede">Gate 1 settled what the reader shows. This page is where each case lands in the code, what it reuses, and five calls I need from you before program design.</p>
  <div class="stamps">
    <span class="stamp good">Gate 1 approved</span>
    <span class="stamp good">approved · 2026-09-29</span>
    <span class="stamp">nothing built</span>
  </div>
</div></header>

<div class="wrap">

  <div class="glass">
    <p>The reader stops starting from the screen. It asks the pane what it shows and borrows that: the document itself, the conversation from its transcript, or a shell’s lines joined where the terminal broke them. <b>The reader’s size comes from the window alone.</b></p>
  </div>

  <ol class="esc-list">
    <li><span class="esc-n">1</span><b>All five attempts started from the grid at the pane’s width.</b><span>Each tried to undo that afterwards. The transcript source was designed on 31 August and set aside for the mirror the same day; your decision 1 brought it back. <span class="tag measured">from the history</span></span></li>
    <li><span class="esc-n">2</span><b>A shell’s fix was already in the grid.</b><span>Every row carries the terminal’s soft-wrap flag, and the reader drops it before guessing. Joining on it is exact on both cores with no copy of the grid, so rio-vt’s re-wrap isn’t needed. <span class="tag measured">read in the code</span></span></li>
    <li><span class="esc-n">3</span><b>Two costs you would see.</b><span>A reply reaches the transcript only when each block of it is finished, so a long answer shows in the live footer first. HTML renders once more when it is lent, soft for about a quarter second. <span class="tag inferred">inferred</span></span></li>
  </ol>

  <h2><span class="n">01</span>History</h2>
  <figure>
    <span class="lbl">01 &middot; Five attempts, one starting point</span>
    <div class="scroller">{fig_attempts()}</div>
    <figcaption>From the commits of June and August. Every attempt began from the pane’s grid at the pane’s width, so more room never helped: the breaks were already in the rows. <b>Attempt six starts from a different source for each kind of pane</b>, with the reader sized from the window and documents borrowed rather than copied.</figcaption>
  </figure>

  <h2><span class="n">02</span>Where it lands</h2>
  <figure>
    <span class="lbl">02 &middot; Today</span>
    <div class="scroller">{arch(1)}</div>
    <figcaption>One source on every face. <b>The reader reads the grid, drops each row’s soft-wrap flag, and scales what’s left to the pane’s width.</b> On a Document face that grid is the shell hidden behind the document.</figcaption>
  </figure>
  <figure>
    <span class="lbl">03 &middot; Documents are lent</span>
    <div class="scroller">{arch(2)}</div>
    <figcaption>The pane keeps its document view; the reader asks for it every frame and draws it in the glass. <b>Nothing changes owner</b>, so notes, links, the ↪ target and the saved layout stay exactly where they are, and the face says “being read” meanwhile.</figcaption>
  </figure>
  <figure>
    <span class="lbl">04 &middot; Text at the reader’s width</span>
    <div class="scroller">{arch(3)}</div>
    <figcaption>A shell’s lines join on the terminal’s own flag; an agent’s come from its transcript when the binding is certain. <b>One pure table decides which</b>, in the place today’s rule lives, and a test that pins today’s rule changes with it. The footer carries what exists only on screen.</figcaption>
  </figure>

  <h2><span class="n">03</span>A shell</h2>
  <figure>
    <span class="lbl">05 &middot; Joined where the terminal broke them</span>
    <div class="scroller">{fig_shell()}</div>
    <figcaption>Real output from this session, drawn at 48 and 175 columns. <b>Joined on the flag the terminal set, a wrapped line comes back whole</b>; what a program broke itself stays as printed, since nothing can know better. Keeping the joined history also stops the reader rebuilding ten thousand lines on every frame while a shell streams, with the terminal locked the whole time. <span class="tag inferred">rebuild rate read in the code, not timed</span></figcaption>
  </figure>

  <h2><span class="n">04</span>An agent</h2>
  <figure>
    <span class="lbl">06 &middot; From transcript to glass</span>
    <div class="scroller">{fig_pipeline()}</div>
    <figcaption><b>Only the first stage exists today</b>: the app already works out each pane’s certain transcript every two seconds. None of the five places that read transcripts keeps the prose in order, and each re-reads a 256 KB tail, which one line can fill on its own. The reader needs its own conversation and a tail that reads only new bytes.</figcaption>
  </figure>
  <figure>
    <span class="lbl">07 &middot; What the reader keeps from a transcript</span>
    <div class="scroller">
    <table class="wide cases"><thead><tr><th>In the transcript</th><th>How much, in this session’s own</th><th>In the reader</th></tr></thead><tbody>
{census_rows()}
    </tbody></table>
    </div>
    <div class="gutter keyrow"><span><span class="dot mine"></span>shown, my proposal</span><span><span class="dot unk"></span>left out, my proposal</span></div>
    <figcaption>Counted in this session’s own transcript: 15.4 MB, 1,060 lines. <b>Tool output is 84% of the bytes, so it waits behind a click.</b> Every row takes a note if you want it drawn differently.</figcaption>
  </figure>

  <h2><span class="n">05</span>A document</h2>
  <figure>
    <span class="lbl">08 &middot; Lending, in order</span>
    <div class="scroller">{fig_lend()}</div>
    <figcaption>The pane never lets go of its view, and the reader never holds one, so a pane that closes or follows a link mid-read can’t leave a video playing nowhere. <b>Esc goes to an open note box before it closes the reader.</b></figcaption>
  </figure>
  <figure>
    <span class="lbl">09 &middot; What each kind does in a bigger box</span>
    <div class="scroller">{table(("Kind", "In the reader", "Cost or catch"), KINDS)}</div>
    <figcaption>Every kind already lays itself out again when its box grows; the reader only hands it a bigger one. <b>Two need work: Markdown loses its place, and a picture never grows past its own pixels.</b> <span class="tag inferred">timings from the code’s comments</span></figcaption>
  </figure>
  <figure>
    <span class="lbl">10 &middot; Keys while reading</span>
    <div class="scroller">{table(("Key", "Reading text", "Reading a document"), KEYS)}</div>
    <figcaption>Today the reader takes Esc and the paging keys and lets everything else through, so a document never sees Page Down and an open note box never sees Esc. <b>With a document lent, those go to the document first.</b></figcaption>
  </figure>

  <h2><span class="n">06</span>Decisions</h2>
  <div class="callout good"><p>You concurred with all five on 29 September, so nothing here is waiting on you. Program design and the slices follow as one plan.</p></div>
  <div class="grill">
    <div class="ask">
      <h3>1 · Join a shell’s lines on the terminal’s flag, with no copy of the grid?</h3>
      <p>Exact on both cores and cheaper than re-wrapping a 16–58 MB copy; the joined history is cached and the terminal is no longer locked for the whole read.</p>
      <p class="rec"><b>Recommendation:</b> yes. rio-vt’s re-wrap stays unused, and its count of rows gone off the top keeps the cache cheap.</p>
    </div>
    <div class="ask">
      <h3>2 · Draw a conversation in the reader’s own text layout?</h3>
      <p>It keeps fast scrolling through huge transcripts, selection and copy. Headings are bold rather than larger, and tables are aligned text. The Markdown document view draws real tables but builds every block every frame and has no selection.</p>
      <p class="rec"><b>Recommendation:</b> the reader’s own layout, as figure 04 of Gate 1 drew it.</p>
    </div>
    <div class="ask">
      <h3>3 · Show a reply when each block of it is finished?</h3>
      <p>Claude Code writes a block to the transcript only when it’s done. Until then the words exist only on screen, so the live footer carries them.</p>
      <p class="rec"><b>Recommendation:</b> yes, with the footer showing the pane’s rows from the input box up.</p>
    </div>
    <div class="ask">
      <h3>4 · Keep what figure 07 keeps?</h3>
      <p>Prompts and replies shown, one line per tool call with its output behind a click, thinking hidden, the rest skipped.</p>
      <p class="rec"><b>Recommendation:</b> as drawn; note any row you want otherwise.</p>
    </div>
    <div class="ask">
      <h3>5 · Lend documents, and let a small picture stay at its own pixels?</h3>
      <p>Lending keeps notes, links and the saved layout untouched. Fitting a small picture to the glass would enlarge it into blur; the zoom keys still go larger.</p>
      <p class="rec"><b>Recommendation:</b> lend, and keep a picture at no more than its own pixels until you zoom.</p>
    </div>
  </div>

  <div class="dlgs">
    <button class="linkbtn" data-dlg="d-method">How this was researched</button>
    <button class="linkbtn" data-dlg="d-shape">The shape, for program design</button>
    <button class="linkbtn" data-dlg="d-limits">What this page does not settle</button>
  </div>

</div>

<dialog id="d-method"><div class="dlg-head"><div><h3>How this was researched</h3><p class="repo">three read-only passes over main at 5ba60ed, each checked where it mattered</p></div><button class="x" data-close>&times;</button></div><div class="dlg-body">
  <h4>The history</h4>
  <p>From <code>git log</code> on main and the handoff of 31 August, which records the call that set the transcript source aside and the ticket it blocked. That ticket is back in the queue.</p>
  <h4>A shell</h4>
  <p>The grid research read TD’s terminal boundary and both cores’ sources (rio-vt 0.5.28, alacritty_terminal 0.26.0). Checked by hand: the reader’s row reader (<code>grid_rows_in</code>, pane.rs line 5107) locks the terminal for the whole range and keeps only each character and its colour, and <code>Grid</code> is <code>Clone</code> with a reflowing <code>resize</code> on both cores, where only rio-vt records a row map.</p>
  <h4>An agent</h4>
  <p>The transcript research counted this session’s own transcript and all 815 on the machine, and read the five places TD parses them. Checked by hand: comrak 0.52 is already a dependency; the per-pane transcript probe is <code>Workspace.tool_probe</code> (main.rs line 4160) with its path in <code>ToolProbe</code> (toolprop.rs line 299).</p>
  <h4>A document</h4>
  <p>The document research read the view, its five backends, the pane’s square and face, and the reader overlay. Checked by hand: the reader’s key handler pages or closes and nothing else (pane.rs line 6176), and a picture’s fit is capped at its own pixels (<code>fit_scale</code>, image.rs line 94). It also corrected Gate 1’s sketch: a new seat would clash with the one enum the pane already uses to say which of its two documents something came from, so the view is lent instead.</p>
  <h4>Rebuilding this page</h4>
  <p><code>python3 reports/2026-09-29-alt-r-reader-architecture.build.py</code>, which keeps any notes and concurs already saved into the page.</p>
</div></dialog>

<dialog id="d-shape"><div class="dlg-head"><div><h3>The shape, for program design</h3><p class="repo">types and signatures only; Gate 3 names every file and test</p></div><button class="x" data-close>&times;</button></div><div class="dlg-body">
  <pre class="shape"><code>{esc(SHAPE)}</code></pre>
</div></dialog>

<dialog id="d-limits"><div class="dlg-head"><div><h3>What this page does not settle</h3><p class="repo">what was read rather than run, and what nobody has looked at yet</p></div><button class="x" data-close>&times;</button></div><div class="dlg-body">
  <ul>
    <li>Nothing was built or run. Every behaviour on this page is read from the code, and the timings come from the code’s own comments. <span class="tag inferred">inferred</span></li>
    <li>That a reply appears only when each block finishes comes from the records on disk; it has not been timed against a live turn.</li>
    <li>The rebuild-every-frame cost of today’s reader follows from how the content generation and the cache key work; nobody has measured it.</li>
    <li>The app counts a binding made by launch time as certain, and it picks the closest of agents launched close together. After a <code>/clear</code>, before the new file exists, a pane can be bound to its own previous conversation. It can’t be bound to another pane’s.</li>
    <li>No <code>/clear</code> rotation has been observed in the session ledger yet, so following one is designed, not seen.</li>
    <li>Codex writes a different format and needs its own reader of it: a medium job, left to a later slice.</li>
    <li>Video decoding at the reader’s size has an unmeasured CPU cost.</li>
    <li>A pane attached while vim was up has no history until vim exits; the reader should say so rather than show an empty page.</li>
  </ul>
</div></dialog>
"""


def main():
    old = OUT.read_text() if OUT.exists() else ""
    notes, concurs = g1.island(old, "report-notes"), g1.island(old, "report-concurs")
    markup = (SKILL / "reference/notes-markup.html").read_text()
    markup = re.sub(r"<!--.*?-->", "", markup, flags=re.S)
    markup = markup.replace("'<YYYY-MM-DD>-<topic-slug>.html'", f"'{NAME}'")
    for ident, (attrs, body) in (("report-notes", notes), ("report-concurs", concurs)):
        blank = f'<script type="application/json" id="{ident}" data-format="1">{{}}</script>'
        assert markup.count(blank) == 1, ident
        markup = markup.replace(blank, f'<script type="application/json" id="{ident}"{attrs}>{body}</script>')
    notes_js = (SKILL / "assets/notes.js").read_text()
    for part in (notes_js, g1.MODAL_JS):
        assert "</script" not in part.lower()
    with tempfile.TemporaryDirectory() as d:
        wall = pathlib.Path(d) / "wall.html"
        subprocess.run(["python3", str(SKILL / "scripts/brief-wall"), "--theme", "terminal-delight", "--image",
                        str(pathlib.Path.home() / ".config/omarchy/themes/terminal-delight/backgrounds/12-tokyo-night-omakub.webp"),
                        "--out", str(wall)], check=True, capture_output=True)
        wall_block = wall.read_text()
    page = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Attempt six</title>
<style>
{(SKILL / "assets/base.css").read_text()}
</style>
{wall_block}
<style>
{OWN_CSS}
</style>
<style>
{(SKILL / "assets/notes.css").read_text()}
</style>
</head>
<body>
{BODY}
{markup}
<script>
{notes_js}
</script>
<script>
{g1.MODAL_JS}
</script>
</body>
</html>
"""
    OUT.write_text(page)
    print(f"wrote {OUT} ({len(page) // 1024} KB) · notes carried: {notes[1].strip() != '{}'} · concurs carried: {concurs[1].strip() != '{}'}")


if __name__ == "__main__":
    main()
