#!/usr/bin/env python3
"""Build reports/2026-09-29-alt-r-reader.html, the Gate 1 brief for the Alt+R reader.

    python3 reports/2026-09-29-alt-r-reader.build.py

Inlines the decision-brief skill's assets (the copy Terminal Delight ships), keeps the
wallpaper this brief was first built on, and draws every mockup from real strings at the
sizes measured on 2026-09-29. Edit the prose here, never in the .html: a rebuild rewrites
it. Notes and concurs already saved into the .html are carried across a rebuild.
"""
import html
import pathlib
import re
import subprocess
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
NAME = "2026-09-29-alt-r-reader.html"
OUT = HERE / NAME
SKILL = pathlib.Path.home() / ".local/share/terminal-delight/skills/decision-brief"


def esc(s):
    return html.escape(s, quote=True)


# ---------------------------------------------------------------------------
# Measured on 2026-09-29 (see the method modal). Every number the prose quotes is
# derived here and asserted, so a changed input fails the build instead of the page.
# ---------------------------------------------------------------------------
WIN_W, WIN_H = 1576, 950               # hyprctl clients, the TD window
FONT, ROW, GRADE = 14.0, 20.0, 0.7365  # theme.toml [font]; sessions/1.toml text_size
CW = FONT * GRADE * 0.6                # JetBrains Mono advance 0.6 em -> 6.19 px
CH = ROW * GRADE                       # 14.73 px
GW, GH = WIN_W * 0.8, WIN_H * 0.8      # the reader's glass: 80% of the window
PAD, HDR = 21.0, 30.0
PAD_X = max(GW * 0.07, PAD)
AW, AH = GW - 2 * PAD_X, GH - HDR - 2 * PAD   # 1,084 x 688 for text


def fit(cols):
    return min(max(AW / (cols * CW), 0.7), 6.0)


def shows(cols, ms):
    return int(AW / (CW * ms)), AH / (CH * ms)


F48 = fit(48)
assert round(F48, 2) == 3.65 and int(shows(48, F48)[1]) == 12
assert round(0.35 * F48, 2) == 1.28
assert round(fit(97), 2) == 1.81 and round(fit(201), 2) == 0.87
assert int(AW / CW) == 175 and int(AH / CH) == 46

# ---------------------------------------------------------------------------
# The conversation both glasses show: Parker's prompt, verbatim, and a reply in
# the shape an agent writes one. The top glass draws it the way Claude Code puts
# it on a 48-column screen; the bottom glass draws it from the transcript.
# ---------------------------------------------------------------------------
PROMPT = (
    "The Alt plus R hotkey to open up the reader. One, now that we have the workbench, the reader "
    "could do something else other than a mirror with the session transcript and presenting it in a "
    "way that the reader is helpful for. Also with the reader, opening the reader for video, PDF, HTML, "
    "images, and markdown files should work, and it doesn't. I'm pretty sure that the reader also "
    "doesn't work for Workbench. That is fine because if we open the reader for a pane, whether it's "
    "Workbench or Terminal, it should show just the terminal. We don't want to open up a reader for the "
    "Workbench. The Workbench is already a processed thing. So basically the goal is have HTML, PNG, "
    "images in general, PDF, video, all of the things that we are alt-clicking to open into their own "
    "pane. Opening the reader for that pane should work. And we will update the reader for the terminal "
    "pane so that it doesn't use the terminal mirror. The sizing on the reader is absolutely killing me. "
    "If I open a reader for a tiny, tiny pane, it's exactly because it's a tiny pane and I can't read it "
    "and I want to make it bigger. I want the reader to have resolution on The terminal pane's width and "
    "height that extends far beyond the tiny, tiny pane. We will workshop out an HTML doc that "
    "demonstrates you know what my intention is here for all of these use cases. Use our decision brief "
    "as a starting point for developing this HTML."
)
TOOLS = [
    ("Read", "app/src/doc.rs", "523 lines"),
    ("Bash", 'git grep -n "FZ_MIN" origin/main -- app/src/main.rs', "const FZ_MIN: f32 = 0.35;"),
    ("Bash", "stty -F /proc/3431657/fd/0 size", "51 48"),
]
H1 = "The reader is only as wide as the pane"
P1 = (
    "It draws the pane's own columns and scales them to fill the glass. A 48-column pane opens as 48 "
    "columns at 3.65 times the size, about twelve rows at a time, and the slider's smallest setting still "
    "draws every letter 1.28 times larger than the pane does. A full-width pane goes the other way: 201 "
    "columns at 0.87 times, smaller than the pane itself."
)
TABLE = [
    ("Pane", "Today", "Intended"),
    ("201 columns", "0.87x  201 x 53", "1.0x  175 x 46"),
    ("97 columns", "1.81x   97 x 25", "1.0x  175 x 46"),
    ("48 columns", "3.65x   48 x 12", "1.0x  175 x 46"),
]
H2 = "An agent's reply can't be widened from the screen"
P2 = (
    "Claude Code breaks its own lines and draws every row two columns in, and the reader's healer refuses "
    "a row that starts with a space. Measured on the real function: five rows at 48 columns stay five rows, "
    "47 columns wide, in a 150-column reader. Plain shell output, broken by the terminal instead, heals into "
    "one line."
)
PROBE = "claude reply: 5 grid rows at 48 cols -> 5 logical lines -> 5 reader rows at 150 cols, widest row 47"
H3 = "Document panes open the shell behind them"
P3 = (
    "Alt+R on a pane showing Markdown, HTML, a PDF, a picture or a video mirrors the terminal that pane was "
    "made with, which is hidden behind the document. A floating square already hands its own view to a "
    "split without opening the file again, and the reader can borrow the view the same way."
)
IN_FLIGHT = "Writing reports/2026-09-29-alt-r-reader.html  ·  4m 12s"


def word_wrap(text, width):
    rows, cur = [], ""
    for w in text.split():
        if cur and len(cur) + 1 + len(w) > width:
            rows.append(cur)
            cur = ""
        cur = f"{cur} {w}" if cur else w
    if cur:
        rows.append(cur)
    return rows


def tool_row(name, target, result, cols):
    head = f"{name:<6}{target}"
    row = [("⚙ ", "acc"), (f"{name:<6}", "b"), (target, "txt"), ("  →  ", "dim"), (result, "dim")]
    over = len("⚙ " + head + "  →  " + result) - cols
    if over > 0:
        row[-1] = (result[: max(0, len(result) - over - 1)] + "…", "dim")
    return row


def transcript_rows(cols):
    """The conversation as the reader would draw it from the transcript."""
    rows = [[("YOU", "lbl")]]
    rows += [[(r, "you")] for r in word_wrap(PROMPT, cols)]
    rows += [[], [("CLAUDE", "lbl")]]
    rows += [tool_row(n, t, r, cols) for n, t, r in TOOLS]
    rows += [[], [(H1, "h")], []]
    rows += [[(r, "txt")] for r in word_wrap(P1, cols)]
    rows.append([])
    widths = [max(len(r[i]) for r in TABLE) + 4 for i in range(3)]
    for k, r in enumerate(TABLE):
        cells = "".join(c.ljust(widths[i]) for i, c in enumerate(r)).replace("x", "×")
        rows.append([(cells.upper() if k == 0 else cells, "th" if k == 0 else "txt")])
        if k == 0:
            rows.append([("".join(("─" * (widths[i] - 4)).ljust(widths[i]) for i in range(3)), "dim")])
    rows += [[], [(H2, "h")], []]
    rows += [[(r, "txt")] for r in word_wrap(P2, cols)]
    rows += [[], tool_row("Bash", "cargo run -q --release", PROBE, cols)]
    rows += [[], [(H3, "h")], []]
    rows += [[(r, "txt")] for r in word_wrap(P3, cols)]
    rows += [[], [("◐ ", "acc"), (IN_FLIGHT, "dim")]]
    return rows


def claude_screen_tail(cols, n):
    """The bottom `n` rows of a Claude Code pane `cols` wide, in its own shape."""
    rows = [[("⏺ ", "acc"), ("Bash", "b"), ("(cargo run -q --release)", "txt")]]
    for i, r in enumerate(word_wrap(PROBE, cols - 5)):
        rows.append([("  ⎿  " if i == 0 else "     ", "dim"), (r, "dim")])
    rows += [[], [("⏺ ", "acc"), (H3, "b")], []]
    rows += [[("  " + r, "txt")] for r in word_wrap(P3, cols - 2)]
    rows += [[], [("✻ ", "acc"), ("Writing… (4m 12s · esc to interrupt)", "dim")]]
    rows += [[], [("─" * cols, "dim")], [("> ", "acc"), ("█", "b")], [("─" * cols, "dim")],
             [("  ? for shortcuts", "dim")]]
    return rows[-n:]


def text_rows(rows, x, y0, line_h, size, cls="mt"):
    out = []
    for i, row in enumerate(rows):
        spans = "".join(f'<tspan class="{c}">{esc(t)}</tspan>' for t, c in row if t)
        if spans:
            out.append(
                f'<text x="{x:.1f}" y="{y0 + i * line_h:.1f}" class="{cls}" font-size="{size:.2f}" '
                f'xml:space="preserve">{spans}</text>'
            )
    return "\n".join(out)


# ---------------------------------------------------------------------------
# Figure 02: one glass, at the true size of the reader's glass in Parker's window.
# ---------------------------------------------------------------------------
X0, Y0, Y1 = PAD_X, HDR + PAD, GH - PAD


def glass(uid, title, chip, chip_cls, body, aria, knob, knob_label, view=None, extra=""):
    vb = view or f"0 0 {GW:.0f} {GH:.0f}"
    tw = len(title) * 13 * 0.6
    cw = len(chip) * 10 * 0.66 + 18 if chip else 0
    cx = 18 + tw + 16
    chip_svg = (
        f'<rect class="g-chip {chip_cls}" x="{cx:.1f}" y="6" width="{cw:.1f}" height="18" rx="9"/>'
        f'<text x="{cx + 9:.1f}" y="18.8" class="mt g-chipt {chip_cls}" font-size="10">{esc(chip)}</text>'
        if chip else ""
    )
    track0, track1 = GW - 240, GW - 40
    kx = track0 + knob * (track1 - track0)
    return f"""<svg viewBox="{vb}" role="img" aria-label="{esc(aria)}">
<defs><clipPath id="clip-{uid}"><rect x="0" y="{Y0:.1f}" width="{GW:.0f}" height="{Y1 - Y0:.1f}"/></clipPath></defs>
<rect class="g-panel" x="1" y="1" width="{GW - 2:.0f}" height="{GH - 2:.0f}" rx="16"/>
<path class="g-hdr" d="M1 17 a16 16 0 0 1 16 -16 H{GW - 17:.0f} a16 16 0 0 1 16 16 V{HDR:.0f} H1 Z"/>
<text x="18" y="20.5" class="mt g-title" font-size="13">{esc(title)}</text>
{chip_svg}
<line class="g-track" x1="{track0:.0f}" y1="15" x2="{track1:.0f}" y2="15"/>
<circle class="g-knob" cx="{kx:.1f}" cy="15" r="6"/>
<text x="{track0 - 12:.0f}" y="19" class="mt g-kl" font-size="11" text-anchor="end">{esc(knob_label)}</text>
<g clip-path="url(#clip-{uid})">
{body}
</g>
{extra}
</svg>"""


def today_glass():
    ms = F48
    lh, size = CH * ms, FONT * GRADE * ms
    visible = AH / lh
    n = int(visible) + 1
    top = Y1 - n * lh
    rows = claude_screen_tail(48, n)
    body = text_rows(rows, X0, top + lh * 0.75, lh, size)
    return glass(
        "today", "✳ Alt-R reader improvements", None, "", body,
        "Today's reader on a 48-column Claude pane: the pane's own rows at 3.65 times the size, twelve and a bit rows visible, the input box at the bottom",
        (1.0 - 0.35) / (1.6 - 0.35), "3.65×",
    )


STRIP_ROWS = [[("─" * 48, "dim")], [("> ", "acc"), ("█", "b")], [("─" * 48, "dim")]]


def intended_body(cols=175):
    rows = transcript_rows(cols)
    room = int(AH / CH) - len(STRIP_ROWS) - 1
    assert len(rows) <= room, (len(rows), room)
    body = text_rows(rows, X0, Y0 + CH * 0.78, CH, FONT * GRADE)
    sy = Y1 - len(STRIP_ROWS) * CH - 4
    strip = (
        f'<rect class="g-strip" x="{X0 - 12:.1f}" y="{sy:.1f}" width="{AW + 24:.1f}" height="{len(STRIP_ROWS) * CH + 8:.1f}" rx="4"/>'
        f'<rect class="g-stripedge" x="{X0 - 12:.1f}" y="{sy:.1f}" width="3" height="{len(STRIP_ROWS) * CH + 8:.1f}"/>'
        + text_rows(STRIP_ROWS, X0, sy + 4 + CH * 0.78, CH, FONT * GRADE)
        + f'<text x="{X0 + AW:.1f}" y="{sy + 4 + CH * 1.78:.1f}" class="mt g-stripl" font-size="10.3" text-anchor="end">the pane’s own input rows, live</text>'
    )
    return body + "\n" + strip, rows


def intended_glass():
    body, _ = intended_body()
    return glass(
        "intended", "✳ Alt-R reader improvements", "TRANSCRIPT · live", "mine", body,
        "The intended reader on the same pane: the conversation from its transcript at the pane's own letter size, 175 columns by 46 rows, with the pane's input rows in a strip at the foot",
        (1.0 - 0.35) / (1.6 - 0.35), "1.0×",
    )


# ---------------------------------------------------------------------------
# Figure 04: the same glass, cropped to its left side at full size, with pins.
# ---------------------------------------------------------------------------
def anatomy():
    body, rows = intended_body()

    def row_y(i):
        return Y0 + CH * i + CH * 0.45

    first_tool = next(i for i, r in enumerate(rows) if r and r[0][0] == "⚙ ")
    first_h = next(i for i, r in enumerate(rows) if r and r[0][1] == "h")
    table_i = next(i for i, r in enumerate(rows) if r and r[0][1] == "th")
    flight = len(rows) - 1
    strip_y = Y1 - len(STRIP_ROWS) * CH + CH * 0.9
    chip_x = 18 + len("✳ Alt-R reader improvements") * 13 * 0.6 + 16
    chip_w = len("TRANSCRIPT · live") * 10 * 0.66 + 18
    pins = [
        (1, chip_x + chip_w + 16, 15, ""),
        (2, 66, row_y(0), ""),
        (3, 66, row_y(first_h), ""),
        (3, 66, row_y(table_i), ""),
        (4, 66, row_y(first_tool), "mine"),
        (5, 66, row_y(flight), ""),
        (6, 66, strip_y, "mine"),
    ]
    marks = "".join(
        f'<circle class="pin-c {c}" cx="{x:.1f}" cy="{y:.1f}" r="10"/>'
        f'<text class="pin-t" x="{x:.1f}" y="{y + 4:.1f}" font-size="11.5" text-anchor="middle">{n}</text>'
        for n, x, y, c in pins
    )
    fade = (
        '<defs><linearGradient id="fade-r" x1="0" x2="1"><stop offset="0" class="fade0"/>'
        '<stop offset="1" class="fade1"/></linearGradient></defs>'
        f'<rect x="780" y="{Y0:.0f}" width="72" height="{Y1 - Y0:.0f}" fill="url(#fade-r)"/>'
    )
    return glass(
        "anatomy", "✳ Alt-R reader improvements", "TRANSCRIPT · live", "mine", body,
        "The intended reader cropped to its left side at full size, with six numbered pins: the source chip, your prompt, the prose and table drawn as Markdown, the tool calls, the turn in flight and the input strip",
        (1.0 - 0.35) / (1.6 - 0.35), "1.0×", view=f"0 0 852 {GH:.0f}", extra=fade + marks,
    )


# ---------------------------------------------------------------------------
# Figure 05: the healer, run on the reader's own code (see the method modal).
# ---------------------------------------------------------------------------
PROBE_PARA = (
    "The reader takes its size from the pane, so a forty-eight column pane opens as forty-eight columns "
    "blown up to fill the glass, and the slider can only make those same letters smaller."
)
PROBE_SHELL = (
    "error[E0308]: mismatched types: expected `Option<f32>`, found `f32` in app/src/main.rs:30358:22 "
    "while building the reader layout"
)


def healer():
    c, lh, fs = 5.6, 13.5, 9.3
    # The probe ran with Claude's ⏺; it is drawn as ● because ⏺ falls back to a wide
    # emoji glyph here, and the healer only ever looks at the leading spaces.
    claude = [("● " if i == 0 else "  ") + r for i, r in enumerate(word_wrap(PROBE_PARA, 46))]
    assert len(claude) == 5 and max(len(r) for r in claude) == 47
    shell = [PROBE_SHELL[i:i + 48] for i in range(0, len(PROBE_SHELL), 48)]
    assert len(shell) == 3 and len(PROBE_SHELL) == 128

    def box(x, y, cols, nrows, cls="box"):
        return f'<rect class="{cls}" x="{x}" y="{y}" width="{cols * c + 16:.1f}" height="{nrows * lh + 14:.1f}" rx="7"/>'

    def rows(x, y, lines, cls="mt"):
        return text_rows([[(l, "txt")] for l in lines], x + 8, y + 7 + lh * 0.75, lh, fs, cls)

    out = []
    # lane A
    ya = 34
    out.append('<text x="20" y="22" class="mt lane" font-size="11">A CLAUDE REPLY · 48-COLUMN PANE</text>')
    out.append(box(20, ya, 48, 5))
    for i in range(1, 5):
        out.append(f'<rect class="margin" x="{28:.1f}" y="{ya + 7 + i * lh:.1f}" width="{2 * c:.1f}" height="{lh:.1f}"/>')
    out.append(rows(20, ya, claude))
    ra = ya + 5 * lh + 14 + 34
    out.append(f'<path class="wire" d="M150 {ya + 5 * lh + 14:.1f} V{ra - 4:.1f}" marker-end="url(#ah)"/>')
    out.append(f'<text x="162" y="{ya + 5 * lh + 32:.1f}" class="mt ink2" font-size="10.5">the reader’s healer · from_grid_rows, then layout at 150 columns</text>')
    out.append(box(20, ra, 150, 5, "box rdr"))
    out.append(rows(20, ra, claude))
    edge = 28 + 47 * c + 4
    out.append(f'<line class="edge" x1="{edge:.1f}" y1="{ra + 3}" x2="{edge:.1f}" y2="{ra + 5 * lh + 11:.1f}"/>')
    out.append(f'<text x="{edge + 10:.1f}" y="{ra + 5 * lh * 0.55:.1f}" class="mt fail" font-size="11">← 47 of 150 columns used: five rows in, five rows out</text>')
    # lane B
    yb = ra + 5 * lh + 14 + 40
    out.append(f'<text x="20" y="{yb - 12}" class="mt lane" font-size="11">B SHELL OUTPUT THE TERMINAL WRAPPED · 48-COLUMN PANE</text>')
    out.append(box(20, yb, 48, 3))
    out.append(rows(20, yb, shell))
    rb = yb + 3 * lh + 14 + 34
    out.append(f'<path class="wire" d="M150 {yb + 3 * lh + 14:.1f} V{rb - 4:.1f}" marker-end="url(#ah)"/>')
    out.append(f'<text x="162" y="{yb + 3 * lh + 32:.1f}" class="mt ink2" font-size="10.5">the same healer</text>')
    out.append(box(20, rb, 150, 1, "box rdr"))
    out.append(rows(20, rb, [PROBE_SHELL]))
    out.append(f'<text x="{28 + 128 * c:.1f}" y="{rb + lh + 14 + 16:.1f}" class="mt ready" font-size="11" text-anchor="end">↑ 128 of 150 columns, one line</text>')
    h = rb + lh + 14 + 26
    return f"""<svg viewBox="0 0 960 {h:.0f}" role="img" aria-label="Two lanes run through the reader's own healer. A Claude reply of five rows at 48 columns, every row after the first indented two columns, comes out as the same five rows using 47 of 150 columns. Shell output wrapped across three rows comes out as one 128-column line.">
<defs><marker id="ah" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0 0 L10 5 L0 10 z" class="ah"/></marker></defs>
{chr(10).join(out)}
</svg>"""


# ---------------------------------------------------------------------------
# Figure 03: the magnification each pane opens at, and the slider's reach.
# ---------------------------------------------------------------------------
def scale_chart():
    x0, x1, y0, y1, top = 96, 910, 296, 34, 6.0

    def y(v):
        return y0 - (v / top) * (y0 - y1)

    parts = []
    for v in range(0, 7):
        parts.append(f'<line class="grid" x1="{x0}" y1="{y(v):.1f}" x2="{x1}" y2="{y(v):.1f}"/>')
        parts.append(f'<text x="{x0 - 10}" y="{y(v) + 4:.1f}" class="mt ink2" font-size="11" text-anchor="end">{v}×</text>')
    parts.append(f'<line class="one" x1="{x0}" y1="{y(1):.1f}" x2="{x1}" y2="{y(1):.1f}"/>')
    for gx, cols in ((270, 201), (500, 97), (730, 48)):
        f = fit(cols)
        lo, hi = max(0.35 * f, 0.3), min(1.6 * f, 12.0)
        c, r = shows(cols, f)
        parts.append(f'<rect class="reach" x="{gx - 13}" y="{y(hi):.1f}" width="26" height="{y(lo) - y(hi):.1f}" rx="5"/>')
        parts.append(f'<circle class="opens" cx="{gx}" cy="{y(f):.1f}" r="8"/>')
        parts.append(f'<text x="{gx + 20}" y="{y(f) + 5:.1f}" class="mt ink" font-size="16" font-weight="700">{f:.2f}×</text>')
        parts.append(f'<text x="{gx + 20}" y="{y(lo) + 4:.1f}" class="mt ink2" font-size="10">slider’s smallest {lo:.2f}×</text>')
        parts.append(f'<text x="{gx}" y="{y0 + 24}" class="mt ink" font-size="12.5" text-anchor="middle">{cols}-column pane</text>')
        parts.append(f'<text x="{gx}" y="{y0 + 41}" class="mt ink2" font-size="11" text-anchor="middle">opens {cols} × {int(r)}</text>')
    parts.append(f'<text x="{x0}" y="16" class="mt ink2" font-size="10.5">LETTERS IN THE READER, AGAINST THE PANE</text>')
    parts.append('<circle class="opens" cx="404" cy="12" r="5"/><text x="414" y="16" class="mt ink2" font-size="10.5">opens today</text>')
    parts.append('<rect class="reach" x="500" y="5" width="11" height="14" rx="3"/><text x="518" y="16" class="mt ink2" font-size="10.5">slider’s reach</text>')
    parts.append('<line class="one" x1="620" y1="12" x2="646" y2="12"/><text x="652" y="16" class="mt mine" font-size="10.5">intended: every pane 1.0×, 175 × 46</text>')
    reach = "; ".join(f"{c} columns opens at {fit(c):.2f} and the slider reaches {0.35 * fit(c):.2f} to {1.6 * fit(c):.2f}" for c in (201, 97, 48))
    return f"""<svg viewBox="0 0 960 350" role="img" aria-label="Today's reader, by pane: {reach}. The intended reader opens every pane at 1.0.">
{chr(10).join(parts)}
</svg>"""


# ---------------------------------------------------------------------------
# Figure 06: three sources, and the chip that names each.
# ---------------------------------------------------------------------------
def sources():
    def mini(x, title, chip, cls, lines, note=None):
        w, h = 296, 214
        s = [f'<rect class="g-panel {"unkb" if cls == "unk" else ""}" x="{x}" y="10" width="{w}" height="{h}" rx="12"/>',
             f'<path class="g-hdr" d="M{x} 22 a12 12 0 0 1 12 -12 H{x + w - 12} a12 12 0 0 1 12 12 V34 H{x} Z"/>',
             f'<text x="{x + 12}" y="27" class="mt g-title" font-size="10.5">{esc(title)}</text>',
             f'<rect class="g-chip {cls}" x="{x + 12}" y="42" width="{len(chip) * 9.5 * 0.66 + 16:.1f}" height="17" rx="8.5"/>',
             f'<text x="{x + 20}" y="54.3" class="mt g-chipt {cls}" font-size="9.5">{esc(chip)}</text>']
        yy = 76
        for kind, a in lines:
            if kind == "t":
                s.append(f'<text x="{x + 12}" y="{yy}" class="mt" font-size="8.6" xml:space="preserve"><tspan class="{a[1]}">{esc(a[0])}</tspan></text>')
            else:
                s.append(f'<rect class="bar" x="{x + 12}" y="{yy - 6}" width="{a * (w - 24):.0f}" height="4.5" rx="2"/>')
            yy += 12.5
        if note:
            s.append(f'<text x="{x + 12}" y="{h - 8}" class="mt unk" font-size="8.6">{esc(note)}</text>')
        return "\n".join(s)

    # Line lengths as a fraction of a 175-column reader: a transcript fills it, a
    # screen stays at its pane's 46, and the shell's log lines are their own length.
    t = [("t", ("YOU", "lbl"))] + [("b", v) for v in (1, 1, 1, .47)] + [("t", ("CLAUDE", "lbl"))] + \
        [("b", v) for v in (.2, .3, .19)] + [("b", v) for v in (.22, 1, .9)]
    s = [("b", v) for v in (.27, .26, .26, .27, .25, .24, .27, .26, .1)]
    b = [("t", ("$ git log --oneline -3 origin/main", "txt"))] + \
        [("b", len(x) / 175) for x in (
            "5ba60ed Merge pull request #883 from parker-brown-family/feat/border-phosphor-gauge",
            "c496fb0 The reading rail's lit blocks follow the phosphor gauge too",
            "e713847 The GAUGES tray has a phosphor gauge, and every lit border's glow goes through it")] + \
        [("t", ("$ █", "txt"))]
    return f"""<svg viewBox="0 0 960 232" role="img" aria-label="Three readers side by side, each naming its source on a chip: TRANSCRIPT for an agent whose conversation is bound, SCREEN for an agent whose transcript is not certain, drawn narrow and labelled, and SCROLLBACK for a plain shell, re-flowed wide.">
{mini(8, "✳ Alt-R reader improvements", "TRANSCRIPT · live", "mine", t)}
{mini(332, "✳ Claude Code", "SCREEN · transcript not bound", "unk", s, "no certain transcript, so it reads the screen")}
{mini(656, "parker@legion:~/Work/terminal-delight", "SCROLLBACK", "mine", b)}
</svg>"""


# ---------------------------------------------------------------------------
# Figure 08: the order, as four plates.
# ---------------------------------------------------------------------------
SLICES = [
    ("1 · TRACER", "Its own size", ["every pane opens at 175 × 46", "shells read right from here", "checks the workbench case"]),
    ("2", "Documents", ["the pane lends its own view:", "Markdown, HTML, PDF,", "picture, video"]),
    ("3", "Agents", ["the conversation from its", "transcript, the SCREEN", "fallback, the input strip"]),
    ("4", "The rest", ["Codex transcripts", "the floating square,", "if decision 4 holds"]),
]


def order():
    out = []
    for i, (k, t, lines) in enumerate(SLICES):
        x = 14 + i * 236
        cls = "ready" if i == 0 else "box"
        out.append(f'<polygon class="{cls}" points="{x + 18},14 {x + 226},14 {x + 208},170 {x},170"/>')
        out.append(f'<text x="{x + 26}" y="40" class="mt {"ready" if i == 0 else "ink2"}" font-size="10.5" font-weight="700">{k}</text>')
        out.append(f'<text x="{x + 24}" y="66" class="mt ink" font-size="15" font-weight="700">{esc(t)}</text>')
        for j, l in enumerate(lines):
            out.append(f'<text x="{x + 20 - j * 2}" y="{92 + j * 18}" class="mt ink2" font-size="11">{esc(l)}</text>')
    return f"""<svg viewBox="0 0 960 184" role="img" aria-label="Four slices in order: the reader's own size first as the tracer, then documents, then agents, then Codex and the floating square.">
{chr(10).join(out)}
</svg>"""


# ---------------------------------------------------------------------------
# The architecture modal: one frame, three steps.
# ---------------------------------------------------------------------------
def arch(step):
    s = []
    blue = lambda n: "struct" if step == n else "box"  # noqa: E731
    s.append('<rect class="bound" x="16" y="14" width="430" height="312" rx="14"/>')
    s.append('<text x="30" y="34" class="mt ink2" font-size="10.5" font-weight="700">▣ A PANE</text>')
    for x, w, t in ((32, 118, "Terminal"), (158, 132, "Workbench"), (298, 132, "Document")):
        s.append(f'<rect class="box" x="{x}" y="44" width="{w}" height="24" rx="12"/>')
        s.append(f'<text x="{x + w / 2}" y="60" class="mt ink" font-size="11" text-anchor="middle">{t} face</text>')

    def node(x, y, w, a, b, cls="box"):
        s.append(f'<rect class="{cls}" x="{x}" y="{y}" width="{w}" height="46" rx="8"/>')
        s.append(f'<text x="{x + 12}" y="{y + 19}" class="mt ink" font-size="11.5" font-weight="700">{esc(a)}</text>')
        s.append(f'<text x="{x + 12}" y="{y + 36}" class="mt ink2" font-size="10.5">{esc(b)}</text>')

    node(32, 84, 398, "floating square", "its own document view · seat: over the terminal")
    node(32, 158, 398, "document view", "seat: the Document face")
    node(32, 232, 398, "the terminal’s grid", "scrollback included, on every face")
    s.append('<path class="wire" d="M231 130 V156" marker-end="url(#ah2)"/>')
    s.append('<text x="241" y="148" class="mt ready" font-size="10">promote to a split · today</text>')
    s.append('<rect class="bound" x="560" y="14" width="384" height="312" rx="14"/>')
    s.append('<text x="574" y="34" class="mt ink2" font-size="10.5" font-weight="700">▣ THE READER · ALT+R</text>')
    if step < 3:
        node(576, 232, 352, "the grid’s rows, at the pane’s width", "blown up to fill the glass")
    else:
        node(576, 232, 352, "text laid out at the reader’s own width", "from the transcript, or from the grid", "struct")
    s.append('<path class="wire" d="M430 255 H574" marker-end="url(#ah2)"/>')
    if step >= 2:
        node(576, 158, 352, "a lent document view", "seat: the reader · back on Esc", blue(2))
        s.append('<path class="wire struct" d="M430 181 H574" marker-end="url(#ah2)"/>')
        s.append('<text x="452" y="198" class="mt struct" font-size="10">lent on Alt+R</text>')
        s.append('<path class="wire mine" stroke-dasharray="4 4" d="M430 107 H502 V181"/>')
        s.append('<text x="440" y="99" class="mt mine" font-size="10">if decision 4</text>')
    if step >= 3:
        s.append('<rect class="unk" x="576" y="346" width="352" height="46" rx="8"/>')
        s.append('<text x="588" y="365" class="mt ink" font-size="11.5" font-weight="700">the agent’s transcript</text>')
        s.append('<text x="588" y="382" class="mt ink2" font-size="10.5">written by Claude Code, on disk</text>')
        s.append('<path class="wire struct" d="M752 346 V280" marker-end="url(#ah2)"/>')
        s.append('<polygon class="struct" points="752,296 764,308 752,320 740,308"/>')
        s.append('<text x="772" y="305" class="mt struct" font-size="10">bound for certain?</text>')
        s.append('<text x="772" y="319" class="mt ink2" font-size="10">the bench’s own answer</text>')
    h = 400 if step >= 3 else 336
    return f"""<svg viewBox="0 0 960 {h}" role="img" aria-label="Architecture step {step}: a pane with three faces, a floating square and a document view, and the reader{' reading only the grid' if step == 1 else ''}{', now also holding a lent document view' if step >= 2 else ''}{', and a transcript source bound for certain' if step >= 3 else ''}.">
<defs><marker id="ah2" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0 0 L10 5 L0 10 z" class="ah"/></marker></defs>
{chr(10).join(s)}
</svg>"""


# ---------------------------------------------------------------------------
# Figure 07: five documents and the lending, drawn in HTML.
# ---------------------------------------------------------------------------
def doc_cell(kind, name, pane, glass_body, claim):
    return f"""<div class="dcell">
  <div class="dpair">
    <div class="dside"><div class="dpane {kind}">{pane}</div><span class="dcap">the pane</span></div>
    <div class="dside"><div class="dglass"><div class="dhdr"><b>{esc(name)}</b><i>{kind.upper()}</i></div><div class="dbody {kind}">{glass_body}</div></div><span class="dcap">the reader</span></div>
  </div>
  <p class="dclaim">{claim}</p>
</div>"""


def lines(n, cls=""):
    return f'<div class="ln {cls}" style="--n:{n}"></div>'


DOCS = "\n".join([
    doc_cell("markdown", "00-status.md",
             '<div class="mh"></div>' + lines(19, "short"),
             '<div class="mh t">Video plays in the floating square, and the bench opens what its links hide</div>' + lines(9, "long"),
             "<b>Markdown</b> re-flows at the reader’s width: about 175 characters a line where the pane gave it 45. Notes work as they do on the pane."),
    doc_cell("html", NAME,
             '<div class="mh"></div><div class="fig"></div>' + lines(4) + '<div class="fig"></div>' + lines(4),
             '<div class="mh t">Alt+R</div><div class="figs"><div class="fig"></div><div class="fig"></div></div>' + lines(3, "long"),
             "<b>HTML</b> is laid out again at 1,084 pixels where the pane gave it about 300, so figures meant to sit side by side do."),
    doc_cell("pdf", "a PDF",
             '<div class="page"><div class="mh"></div>' + lines(10) + '</div>',
             '<div class="page wide"><div class="mh"></div>' + lines(7, "long") + '</div>',
             "<b>PDF</b> pages come out at the reader’s width, and poppler draws them again at the new scale, so nothing is a stretched bitmap."),
    doc_cell("picture", "a 1,920 × 1,080 screenshot",
             '<div class="pic"></div>',
             '<div class="pic"></div>',
             "<b>A picture</b> fills the glass: a 1,920 × 1,080 screenshot at 56% of its pixels, where the pane showed it at 15%."),
    doc_cell("video", "a clip, playing",
             '<div class="vid"><span class="vbar"></span></div>',
             '<div class="vid"><span class="vbar"><i>❚❚ 1:23</i><em></em><i>3:10</i></span></div>',
             "<b>Video</b> keeps playing: the player itself moves into the reader, at the same second, with its sound."),
])

LEND_SVG = """<svg viewBox="0 0 340 170" role="img" aria-label="One document view moves from the pane's Document face into the reader on Alt+R and back on Esc; the pane shows that it is being read.">
<defs><marker id="ah3" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0 0 L10 5 L0 10 z" class="ah"/></marker></defs>
<rect class="box" x="8" y="22" width="86" height="126" rx="8"/>
<text x="51" y="16" class="mt ink2" font-size="9.5" text-anchor="middle">THE PANE</text>
<text x="51" y="82" class="mt ink2" font-size="9.5" text-anchor="middle">being read</text>
<text x="51" y="96" class="mt ink2" font-size="9.5" text-anchor="middle">↗</text>
<rect class="struct" x="146" y="22" width="186" height="126" rx="10"/>
<text x="239" y="16" class="mt ink2" font-size="9.5" text-anchor="middle">THE READER</text>
<text x="239" y="80" class="mt ink" font-size="11" font-weight="700" text-anchor="middle">the same view</text>
<text x="239" y="96" class="mt ink2" font-size="9.5" text-anchor="middle">nothing reopened</text>
<path class="wire struct" d="M96 60 H144" marker-end="url(#ah3)"/>
<text x="120" y="54" class="mt struct" font-size="9" text-anchor="middle">Alt+R</text>
<path class="wire" d="M144 112 H98" marker-end="url(#ah3)"/>
<text x="120" y="126" class="mt ink2" font-size="9" text-anchor="middle">Esc</text>
</svg>"""


# ---------------------------------------------------------------------------
# The page
# ---------------------------------------------------------------------------
OWN_CSS = r"""
/* ---- the brief's own: mockup ink, the case table, the document grid ---- */
.mt { font-family: var(--mono); fill: var(--fg); }
.mt .txt { fill: var(--fg); }
.mt .you { fill: var(--fgb); }
.mt .b, .mt .h { fill: var(--fgb); font-weight: 700; }
.mt .dim { fill: var(--dim); }
.mt .acc { fill: var(--acc); }
.mt .lbl { fill: var(--acc); font-weight: 700; letter-spacing: .1em; }
.mt .th { fill: var(--dim); font-weight: 700; }
svg .lane { fill: var(--dim); font-weight: 700; letter-spacing: .12em; }
.g-panel { fill: color-mix(in srgb, var(--bg0) 92%, #000); stroke: var(--line2); stroke-width: 1.5; }
.g-panel.unkb { stroke: var(--dim); stroke-dasharray: 6 5; }
.g-hdr { fill: color-mix(in srgb, var(--bg2) 94%, #000); }
.g-title { fill: var(--fgb); font-weight: 700; }
.g-chip { stroke-width: 1.2; }
.g-chip.fail { fill: color-mix(in srgb, var(--serious) 18%, var(--bg0)); stroke: var(--serious); }
.g-chip.mine { fill: color-mix(in srgb, var(--warning) 16%, var(--bg0)); stroke: var(--warning); }
.g-chip.unk { fill: var(--bg0); stroke: var(--dim); stroke-dasharray: 3 3; }
.g-chipt { font-weight: 700; letter-spacing: .06em; }
.g-chipt.fail { fill: var(--serious); } .g-chipt.mine { fill: var(--warning); } .g-chipt.unk { fill: var(--dim); }
.g-track { stroke: var(--line2); stroke-width: 3; stroke-linecap: round; }
.g-knob { fill: var(--acc); }
.g-kl { fill: var(--dim); }
.g-strip { fill: color-mix(in srgb, var(--warning) 7%, transparent); }
.g-stripedge { fill: var(--warning); }
.g-stripl { fill: var(--warning); font-style: italic; }
.fade0 { stop-color: var(--bg0); stop-opacity: 0; } .fade1 { stop-color: var(--bg0); stop-opacity: 1; }
.pin-c { fill: var(--s1); stroke: var(--bg0); stroke-width: 2; }
.pin-c.mine { fill: var(--s4); }
.pin-t { fill: #fff; font-family: var(--mono); font-weight: 700; }
svg .margin { fill: color-mix(in srgb, var(--serious) 40%, transparent); }
svg .rdr { fill: color-mix(in srgb, var(--bg0) 92%, #000); }
svg .edge { stroke: var(--serious); stroke-width: 1.5; stroke-dasharray: 3 3; }
svg .ah { fill: var(--dim); }
svg .grid { stroke: var(--line); }
svg .one { stroke: var(--warning); stroke-width: 2; stroke-dasharray: 7 5; }
svg .reach { fill: color-mix(in srgb, var(--dim) 22%, transparent); stroke: var(--line2); }
svg .opens { fill: var(--serious); stroke: var(--bg0); stroke-width: 2; }
svg .bar { fill: color-mix(in srgb, var(--dim) 45%, transparent); }
svg .bound { fill: none; stroke: var(--line2); stroke-width: 1.5; }
svg .wire.struct { stroke: var(--s1); } svg .wire.mine { stroke: var(--warning); }

.glab { display: flex; align-items: center; gap: 10px; margin: 16px 0 8px; font: 700 11px/1.3 var(--mono);
  letter-spacing: .12em; text-transform: uppercase; }
.glab:first-of-type { margin-top: 0; }
.glab.fail { color: var(--serious); } .glab.mine { color: var(--warning); }

table.cases { font-size: 13.5px; }
table.cases td:first-child { color: var(--fgb); }
.dot { display: inline-block; width: 9px; height: 9px; border-radius: 50%; margin-right: 8px; vertical-align: 1px;
  background: var(--dim); flex: none; }
.dot.fail { background: var(--serious); } .dot.ready { background: var(--s3); }
.dot.mine { background: var(--warning); } .dot.crit { background: var(--critical); box-shadow: 0 0 8px var(--critical); }
.dot.unk { background: transparent; border: 1.5px dashed var(--dim); }
.keyrow { display: flex; flex-wrap: wrap; gap: 6px 18px; }

.legend { list-style: none; margin: 16px 0 0; padding: 0; display: grid;
  grid-template-columns: repeat(auto-fit, minmax(270px, 1fr)); gap: 8px 24px; font-size: 14px; }
.legend li { display: flex; gap: 10px; align-items: flex-start; }
.pin { display: inline-flex; align-items: center; justify-content: center; width: 18px; height: 18px;
  border-radius: 50%; background: var(--s1); color: #fff; font: 700 10.5px/1 var(--mono); flex: none; margin-top: 2px; }
.pin.mine { background: var(--s4); }

.docgrid { display: grid; grid-template-columns: repeat(auto-fit, minmax(340px, 1fr)); gap: 22px 26px; }
.dpair { display: flex; gap: 12px; align-items: flex-start; flex-wrap: wrap; }
.dside { display: flex; flex-direction: column; gap: 5px; }
.dcap { font: 700 9.5px/1.2 var(--mono); letter-spacing: .12em; text-transform: uppercase; color: var(--dim); }
.dpane { width: 62px; height: 158px; border-radius: 6px; padding: 7px 6px; overflow: hidden;
  background: color-mix(in srgb, var(--bg0) 92%, #000); border: 1px solid var(--line2); }
.dglass { width: 265px; height: 160px; border-radius: 10px; overflow: hidden;
  background: color-mix(in srgb, var(--bg0) 92%, #000); border: 1px solid var(--line2);
  box-shadow: 0 10px 26px -12px #000; }
.dhdr { display: flex; justify-content: space-between; gap: 8px; align-items: center; padding: 4px 9px;
  background: color-mix(in srgb, var(--bg2) 94%, #000); font: 9px/1.3 var(--mono); color: var(--fgb); }
.dhdr b { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.dhdr i { font-style: normal; color: var(--warning); letter-spacing: .1em; }
.dbody { padding: 8px 12px; height: calc(100% - 21px); overflow: hidden; }
.mh { height: 6px; width: 70%; border-radius: 3px; background: var(--fgb); opacity: .8; margin: 2px 0 7px; }
.mh.t { height: auto; width: auto; background: none; opacity: 1; font: 700 8.5px/1.3 var(--mono); color: var(--fgb); }
.ln { height: calc(var(--n) * 6px); margin-bottom: 6px;
  background: repeating-linear-gradient(to bottom, color-mix(in srgb, var(--dim) 55%, transparent) 0 2px, transparent 2px 6px); }
.ln.long { height: calc(var(--n) * 7px);
  background: repeating-linear-gradient(to bottom, color-mix(in srgb, var(--dim) 55%, transparent) 0 2px, transparent 2px 7px); }
.fig { height: 34px; border-radius: 4px; margin: 4px 0 7px; border: 1px solid var(--line2);
  background: color-mix(in srgb, var(--s1) 14%, transparent); }
.figs { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
.figs .fig { height: 60px; }
.page { background: #e9e6dc; border-radius: 2px; padding: 6px 5px; height: 88px; margin-top: 30px; }
.page .mh { background: #3a3830; }
.page .ln { background: repeating-linear-gradient(to bottom, #8d897c 0 1.5px, transparent 1.5px 5px); }
.page.wide { margin-top: 0; height: 140px; padding: 12px 14px; }
.page.wide .ln { background: repeating-linear-gradient(to bottom, #6f6b60 0 2px, transparent 2px 7px); }
.pic, .vid { position: relative; border-radius: 3px; aspect-ratio: 16 / 9; margin-top: 50px;
  background: linear-gradient(135deg, color-mix(in srgb, var(--s1) 45%, #000), color-mix(in srgb, var(--acc) 35%, #000) 70%, #000); }
.dbody .pic, .dbody .vid { margin-top: 0; width: 216px; }
.vid { background: radial-gradient(circle at 60% 40%, color-mix(in srgb, var(--s3) 45%, #000), #050505 70%); }
.vbar { position: absolute; left: 0; right: 0; bottom: 0; height: 12px; display: flex; gap: 6px; align-items: center;
  padding: 0 6px; background: #000000b0; font: 8px/1 var(--mono); color: #fff; }
.vbar i { font-style: normal; } .vbar em { flex: 1; height: 2px; background: linear-gradient(90deg, var(--acc) 40%, #ffffff55 40%); }
.dclaim { margin: 10px 0 0; font-size: 13.5px; }
.lend svg { max-width: 340px; }
.dlgs { display: flex; flex-wrap: wrap; gap: 10px; margin: 8px 0 26px; }
"""

LEDE = (
    "You asked for a reader that works on every pane, stops mirroring the terminal, and gives a tiny pane room "
    "rather than bigger letters. Each case is drawn below; mark what I got wrong."
)

REACH_ROWS = "\n".join(
    f"    <tr><td>{c} columns</td><td>{fit(c):.2f}×</td><td>{c} × {int(shows(c, fit(c))[1])}</td>"
    f"<td>{0.35 * fit(c):.2f}–{1.6 * fit(c):.2f}×</td></tr>" for c in (201, 97, 48))

BODY = f"""
<header class="top"><div class="wrap">
  <p class="eyebrow">Gate 1 of 4 &middot; product &middot; Terminal Delight &middot; 2026-09-29</p>
  <h1>Alt+R</h1>
  <p class="lede">{LEDE}</p>
  <div class="stamps">
    <span class="stamp warn">difficulty 7/10 &middot; four gates</span>
    <span class="stamp good">approved · 2026-09-29</span>
    <span class="stamp">nothing built</span>
  </div>
</div></header>

<div class="wrap">

  <div class="glass">
    <p>Alt+R should read what the pane shows, from its source, at the reader’s own size: a document as that document, an agent’s pane as its conversation, a shell as its scrollback. <b>The pane’s size never decides the reader’s.</b></p>
  </div>

  <ol class="esc-list">
    <li><span class="esc-n">1</span><b>The reader is only as wide as the pane.</b><span>Your 48-column panes open as their own 48 columns at 3.65 times the size, twelve rows at a time. It needs its own size: 175 × 46 in your window, whatever the pane. <span class="tag measured">measured</span></span></li>
    <li><span class="esc-n">2</span><b>An agent’s replies can’t be widened from the screen.</b><span>Claude Code breaks and indents its own lines, and the reader can’t rejoin them. The transcript has them whole, and the app already finds a pane’s own transcript whenever it can be sure. <span class="tag measured">measured</span></span></li>
    <li><span class="esc-n">3</span><b>On a document pane, Alt+R shows the shell hidden behind the document.</b><span>All five kinds do it. The pane can lend its own document to the reader, the way a floating square already hands itself to a split. <span class="tag inferred">inferred from the code</span></span></li>
  </ol>

  <h2><span class="n">01</span>Every pane</h2>
  <figure>
    <span class="lbl">01 &middot; What Alt+R opens, today and intended</span>
    <div class="scroller">
    <table class="wide cases">
      <thead><tr><th>The pane shows</th><th>Today</th><th>Intended</th></tr></thead>
      <tbody>
        <tr><td>Terminal · Claude or Codex, transcript bound</td><td><span class="dot fail"></span>the grid at the pane’s width, blown up</td><td><span class="dot mine"></span>the conversation from its transcript, at the reader’s size</td></tr>
        <tr><td>Terminal · an agent whose transcript isn’t certain</td><td><span class="dot fail"></span>the grid, blown up</td><td><span class="dot unk"></span>the screen, re-flowed where it heals, labelled “transcript not bound”</td></tr>
        <tr><td>Terminal · a plain shell</td><td><span class="dot fail"></span>the grid, blown up</td><td><span class="dot mine"></span>the scrollback, re-flowed at the reader’s size</td></tr>
        <tr><td>Workbench face</td><td><span class="dot ready"></span>the terminal’s grid, on purpose; a test pins it</td><td><span class="dot ready"></span>whatever the terminal face reads; never the bench</td></tr>
        <tr><td>Document · Markdown</td><td><span class="dot fail"></span>the shell hidden behind the document</td><td><span class="dot mine"></span>the Markdown, re-flowed at the reader’s width, notes included</td></tr>
        <tr><td>Document · HTML</td><td><span class="dot fail"></span>the hidden shell</td><td><span class="dot mine"></span>the page, laid out again at the reader’s width</td></tr>
        <tr><td>Document · PDF</td><td><span class="dot fail"></span>the hidden shell</td><td><span class="dot mine"></span>the pages at the reader’s width, drawn sharp</td></tr>
        <tr><td>Document · picture</td><td><span class="dot fail"></span>the hidden shell</td><td><span class="dot mine"></span>the picture, as large as the glass allows</td></tr>
        <tr><td>Document · video</td><td><span class="dot fail"></span>the hidden shell</td><td><span class="dot mine"></span>the same player, still playing, at the same second</td></tr>
        <tr><td>A floating square over a terminal</td><td><span class="dot fail"></span>the terminal under the square</td><td><span class="dot mine"></span>the square’s document (decision 4)</td></tr>
      </tbody>
    </table>
    </div>
    <div class="gutter keyrow"><span><span class="dot fail"></span>wrong today</span><span><span class="dot ready"></span>right today</span><span><span class="dot mine"></span>my reading of what you asked</span><span><span class="dot unk"></span>unknown, and says so</span></div>
    <figcaption>Ten cases. <b>Only the workbench row reads the right thing today, and it stays as it is.</b> The five document rows are the ones you said don’t work: Alt+R reads the shell each document pane was made with. The floating square was the one case your message didn’t cover, and decision 4 settled it.</figcaption>
  </figure>

  <h2><span class="n">02</span>Size</h2>
  <figure>
    <span class="lbl">02 &middot; This conversation in a 48-column pane</span>
    <div class="glab fail">Today · 48 columns at 3.65× · 12 rows</div>
    {today_glass()}
    <div class="glab mine">Intended · 175 × 46 at the pane’s own letter size</div>
    {intended_glass()}
    <div class="gutter">both glasses drawn to the scale of your 1,576 × 950 window; the lower glass’s letters are the pane’s own size, shrunk with the drawing (figure 04 shows them full size)</div>
    <figcaption>The reader uses 80% of the window, and so does this drawing. Top: today’s reader on this conversation, in a 48-column pane. Bottom: the same conversation from its transcript. <b>The same glass and the same letters as the pane, with 3.6 times the columns and 3.6 times the rows.</b></figcaption>
  </figure>

  <figure>
    <span class="lbl">03 &middot; Today the pane decides the size</span>
    <div class="scroller">{scale_chart()}</div>
    <figcaption>Measured against the panes open in your window this morning. The reader divides its glass by the pane’s width, so <b>a narrow pane opens huge and a full-width pane opens smaller than it already was.</b> The slider only scales that first guess, and on a 48-column pane its smallest setting is 1.28×, so you can never read it at its own letter size. <span class="tag measured">measured</span></figcaption>
  </figure>

  <h2><span class="n">03</span>An agent’s pane</h2>
  <figure>
    <span class="lbl">04 &middot; The conversation, as the reader draws it</span>
    <div class="scroller">{anatomy()}</div>
    <ol class="legend">
      <li><span class="pin">1</span><span>The source, on the reader’s face: TRANSCRIPT here; SCREEN or SCROLLBACK elsewhere (figure 06).</span></li>
      <li><span class="pin">2</span><span>Your prompt, whole, as you typed it.</span></li>
      <li><span class="pin">3</span><span>The agent’s prose drawn as Markdown: the heading is a heading and the table is a table.</span></li>
      <li><span class="pin mine">4</span><span>One line per tool call; a click opens what it printed.</span></li>
      <li><span class="pin">5</span><span>What the turn is doing now, and for how long.</span></li>
      <li><span class="pin mine">6</span><span>The pane’s own input rows, live, so what you type still shows and a permission question still reaches you.</span></li>
    </ol>
    <div class="gutter">amber pins = parts I added that your message didn’t ask for</div>
    <figcaption>Full size, cropped to the left two-thirds. The transcript holds each reply whole before the terminal breaks it, so <b>read from there, a reply fills whatever width the reader has</b>, and your prompt comes back exactly as you typed it.</figcaption>
  </figure>

  <figure>
    <span class="lbl">05 &middot; Why the screen can’t be widened</span>
    <div class="scroller">{healer()}</div>
    <div class="gutter">an indented row normally starts a code block, so refusing one is right everywhere except a Claude pane, where every row is indented</div>
    <figcaption>The reader’s own healer, run on today’s main. <b>A Claude reply comes out exactly as narrow as it went in</b>, because every row after the first starts two columns in. The shell’s wrapped line comes out whole. More room fixes the shell and does nothing for an agent, which is why an agent’s reader reads the transcript. <span class="tag measured">measured</span></figcaption>
  </figure>

  <div class="cards">
    <div class="card rec">
      <span class="badge">recommended</span>
      <h3>A · Read the transcript</h3>
      <p>Prompts and replies whole, Markdown drawn as Markdown, a line per tool call, live.</p>
      <p class="why">The only one that widens a reply. What the agent shows but never writes down reaches you through the input strip only.</p>
    </div>
    <div class="card">
      <h3>B · Resize the real terminal</h3>
      <p>The terminal grows to the reader’s size while it’s open, as tmux zooms a pane.</p>
      <p class="why">Dialogs and all, nothing new to draw; but two redraws per read, older output stays narrow, and the phone’s view resizes too. <span class="tag w">hunch</span></p>
    </div>
    <div class="card">
      <h3>C · Keep the screen, fix the size</h3>
      <p>Today’s reader at its own size.</p>
      <p class="why">Enough for a shell. An agent’s replies stay a 47-column ribbon in a 175-column reader.</p>
    </div>
  </div>

  <figure>
    <span class="lbl">06 &middot; Three sources, and the reader names each</span>
    <div class="scroller">{sources()}</div>
    <figcaption><b>The reader never guesses whose conversation it is showing.</b> A pane gets its transcript only when the agent says which one it is, or when nothing else could own it; any other pane gets its screen, labelled. Before that rule, on 18 September, eight benches in one repository showed one agent’s conversation as their own.</figcaption>
  </figure>

  <h2><span class="n">04</span>Documents</h2>
  <figure>
    <span class="lbl">07 &middot; Five documents in the reader</span>
    <div class="docgrid">
{DOCS}
      <div class="dcell lend">
        {LEND_SVG}
        <p class="dclaim"><b>One view, lent.</b> The pane says it is being read, and gets its view back on Esc at its own zoom. <span class="tag w">my assumption</span></p>
      </div>
    </div>
    <div class="gutter">drawn at a fifth of the real size; the pane is one of your 48-column panes</div>
    <figcaption><b>Nothing is opened twice.</b> The document the pane holds moves into the reader and back, the way a floating square already becomes a split. Each kind then does what it already does when its box grows, and for a document that is all the extra room takes.</figcaption>
  </figure>

  <h2><span class="n">05</span>Decisions</h2>
  <div class="callout good"><p>You concurred with all five on 29 September, so nothing on this page is waiting on you. The next decisions come with the architecture gate.</p></div>
  <div class="grill">
    <div class="ask">
      <h3>1 · Where does an agent’s pane read from?</h3>
      <p>Only the transcript widens a reply (figure 05). The screen stays as the labelled fallback when the transcript isn’t certain.</p>
      <p class="rec"><b>Recommendation:</b> the transcript. Claude first, Codex in a later slice.</p>
    </div>
    <div class="ask">
      <h3>2 · Where does a plain shell read from?</h3>
      <p>Shell output heals (figure 05). A program that formats to the width itself, like <code>ls</code>, stays as it printed.</p>
      <p class="rec"><b>Recommendation:</b> its scrollback, re-flowed at the reader’s width.</p>
    </div>
    <div class="ask">
      <h3>3 · What size does the reader open at?</h3>
      <p>That is 175 × 46 in your window, whichever pane; the slider and Ctrl+wheel magnify from there.</p>
      <p class="rec"><b>Recommendation:</b> the pane’s own letter size, so opening the reader only ever adds room.</p>
    </div>
    <div class="ask">
      <h3>4 · With a document floating over a terminal, what does Alt+R read?</h3>
      <p>Your message didn’t cover it. The square is what you’re looking at; Esc closes it when you want the terminal.</p>
      <p class="rec"><b>Recommendation:</b> the document in the square.</p>
    </div>
    <div class="ask">
      <h3>5 · Can you keep typing to the agent while you read?</h3>
      <p>Today you can. A transcript has no input box, so the reader’s foot shows the pane’s live input rows (pin 6, figure 04).</p>
      <p class="rec"><b>Recommendation:</b> yes, with the input strip.</p>
    </div>
  </div>

  <h2><span class="n">06</span>Order</h2>
  <figure>
    <span class="lbl">08 &middot; The order I’d build it in</span>
    <div class="scroller">{order()}</div>
    <figcaption>A first cut, for the slice gate to change. <b>The first slice alone ends the magnification</b>, so it goes first, and on the way it checks the workbench case in your window.</figcaption>
  </figure>

  <div class="dlgs">
    <button class="linkbtn" data-dlg="d-method">How the numbers were measured</button>
    <button class="linkbtn" data-dlg="d-arch">Where it lands in the code</button>
    <button class="linkbtn" data-dlg="d-limits">What this page does not settle</button>
  </div>

</div>



<dialog id="d-method"><div class="dlg-head"><div><h3>How the numbers were measured</h3><p class="repo">stty · the compositor · theme.toml · sessions/1.toml · the reader overlay in main.rs · doc.rs</p></div><button class="x" data-close>&times;</button></div><div class="dlg-body">
  <h4>Your window and panes</h4>
  <p>The window is 1,576 × 950, from the compositor’s client list. Each pane’s size is read from its own terminal with <code>stty -F /proc/&lt;pid&gt;/fd/0 size</code>, this morning: 48 × 51 and 48 × 52 for the two narrow panes in one tab, 93 to 104 columns for the halves, 187 and 201 for full-width panes.</p>
  <h4>Letter size</h4>
  <p>theme.toml sets JetBrains Mono at 14 px with 20 px rows, and the panes carry a text grade of 0.7365 in the session file, so a cell is about 6.19 × 14.73 px, taking JetBrains Mono’s advance as 0.6 em. The cross-check: 201 columns × 6.19 = 1,244 px of the 1,576-pixel window, and 52 rows × 14.73 = 766 px of 950. <span class="tag inferred">computed, not read from the app</span></p>
  <h4>The reader’s rule</h4>
  <p>From the reader overlay in main.rs: the glass is 80% of the window, 1,261 × 760; the side padding is 7% of the glass, 88 px; a 30 px header and 21 px of padding leave 1,084 × 688 for text. Today the magnification is 1,084 ÷ (columns × 6.19), held between 0.7 and 6, and the slider multiplies it by 0.35 to 1.6. At the pane’s own letter size, 1,084 ÷ 6.19 gives 175 columns and 688 ÷ 14.73 gives 46 rows.</p>
  <table><thead><tr><th>Pane</th><th>Opens at</th><th>Shows</th><th>Slider’s reach</th></tr></thead><tbody>
{REACH_ROWS}
    <tr><td>any, intended</td><td>1.0×</td><td>175 × 46</td><td>the slider from there</td></tr>
  </tbody></table>
  <h4>The healer probe</h4>
  <p>doc.rs from main at 5ba60ed, its <code>Document::from_grid_rows</code> and <code>layout</code> unchanged, with gpui’s text-run type replaced by its length, compiled on its own and run on rows built in Claude Code’s shape: the ⏺ bullet in column 0 and every other row two columns in, the shape the copy-chip fix of 28 September describes. The output, verbatim:</p>
  <pre><code>claude reply: 5 grid rows at 48 cols -&gt; 5 logical lines -&gt; 5 reader rows at 150 cols, widest row 47
claude reply, bullet row gone: 4 grid rows at 48 cols -&gt; 1 logical lines -&gt; 1 reader rows at 150 cols, widest row 137
shell soft-wrap: 3 grid rows at 48 cols -&gt; 1 logical lines -&gt; 1 reader rows at 150 cols, widest row 128</code></pre>
  <p>The middle line shows the one case where the healer joins a Claude reply: with the bullet row gone, every row shares the margin and the margin is stripped. In the reader’s real document the bullets and the input box sit in column 0, so that case never comes up.</p>
  <h4>Rebuilding this page</h4>
  <p><code>python3 reports/2026-09-29-alt-r-reader.build.py</code> redraws every figure from the numbers above, and keeps any notes and concurs already saved into the page.</p>
</div></dialog>

<dialog id="d-arch"><div class="dlg-head"><div><h3>Where it lands in the code</h3><p class="repo">a first look for the architecture gate, not for approval here</p></div><button class="x" data-close>&times;</button></div><div class="dlg-body">
  <h4>Step 1 · Today</h4>
  <figure><span class="lbl">A1 &middot; Today</span>{arch(1)}<figcaption>A pane has three faces. A document view sits in one of two seats, and a floating square becomes a split by handing its view over. <b>The reader reads one thing on every face: the grid.</b></figcaption></figure>
  <h4>Step 2 · A third seat</h4>
  <figure><span class="lbl">A2 &middot; A third seat</span>{arch(2)}<figcaption>Alt+R lends the Document face’s view to the reader and takes it back on Esc, and the square’s too if decision 4 holds. <b>Touches the document view’s seat and the reader overlay, nothing else.</b></figcaption></figure>
  <h4>Step 3 · A second source</h4>
  <figure><span class="lbl">A3 &middot; A second source</span>{arch(3)}<figcaption>The pane-to-conversation binding that the bench and the notifications already use decides. <b>A certain binding reads the transcript; anything else reads the grid as today</b>, and either way the text is laid out at the reader’s own width. Touches doc.rs (a second source beside the grid’s), the reader overlay and the reader’s key layer; it reads the binding and never changes it.</figcaption></figure>
</div></dialog>

<dialog id="d-limits"><div class="dlg-head"><div><h3>What this page does not settle</h3><p class="repo">method, sampling, and what nobody has looked at yet</p></div><button class="x" data-close>&times;</button></div><div class="dlg-body">
  <ul>
    <li>Nothing here was built, and no reader was photographed. The document rows’ “today” is read from the code: Alt+R is a window chord on every face, the reader draws the pane’s grid, and a document pane keeps its shell underneath. <span class="tag inferred">inferred</span></li>
    <li>The workbench row: the code reads the terminal from the workbench face on purpose, pinned by <code>the_focus_reader_mirrors_the_grid_on_both_faces</code>. You suspected it doesn’t work. If Alt+R on a pane showing its bench opens anything but that pane’s terminal, the row is wrong, and the first slice fixes it.</li>
    <li>The cell size is computed from the font and the grade, not read from the app.</li>
    <li>The healer probe used rows built in Claude’s shape, not rows captured from a pane.</li>
    <li>I did not count how many of your agent panes have a certain transcript today. If most don’t, the labelled screen becomes the common case, and decision 1 buys less than it looks.</li>
    <li>A permission question, the spinner and the slash menu exist only on the screen. The input strip in decision 5 is what carries them, and it isn’t designed yet.</li>
    <li>Codex writes its transcript in another format; this page assumes it can be read the same way.</li>
    <li>Card B’s costs are a hunch about how agents redraw when their terminal is resized; nothing was measured.</li>
  </ul>
</div></dialog>
"""

MODAL_JS = r"""
(function () {
  document.addEventListener('click', function (e) {
    var b = e.target.closest('[data-dlg]');
    if (!b) return;
    var d = document.getElementById(b.dataset.dlg);
    if (d && d.showModal) d.showModal();
  });
  Array.prototype.forEach.call(document.querySelectorAll('dialog:not(#d-note):not(#d-export)'), function (d) {
    Array.prototype.forEach.call(d.querySelectorAll('[data-close]'), function (x) {
      x.addEventListener('click', function () { d.close(); });
    });
    d.addEventListener('click', function (e) { if (e.target === d) d.close(); });
  });
})();
"""


def island(page, ident):
    """The saved island, verbatim: its attributes (TD adds data-rev) and its JSON."""
    m = re.search(r'<script type="application/json" id="%s"([^>]*)>(.*?)</script>' % ident, page, re.S)
    return (m.group(1), m.group(2)) if m else (' data-format="1"', "{}")


def wall_block():
    with tempfile.TemporaryDirectory() as d:
        out = pathlib.Path(d) / "wall.html"
        subprocess.run(
            ["python3", str(SKILL / "scripts/brief-wall"), "--theme", "matte-black", "--image", "/usr/share/omarchy/themes/matte-black/backgrounds/omarchy.png",
             "--out", str(out)],
            check=True, capture_output=True,
        )
        return out.read_text()


def main():
    old = OUT.read_text() if OUT.exists() else ""
    notes, concurs = island(old, "report-notes"), island(old, "report-concurs")
    markup = (SKILL / "reference/notes-markup.html").read_text()
    markup = re.sub(r"<!--.*?-->", "", markup, flags=re.S)
    markup = markup.replace("'<YYYY-MM-DD>-<topic-slug>.html'", f"'{NAME}'")
    for ident, (attrs, body) in (("report-notes", notes), ("report-concurs", concurs)):
        blank = f'<script type="application/json" id="{ident}" data-format="1">{{}}</script>'
        assert markup.count(blank) == 1, ident
        markup = markup.replace(blank, f'<script type="application/json" id="{ident}"{attrs}>{body}</script>')
    notes_js = (SKILL / "assets/notes.js").read_text()
    for part in (notes_js, MODAL_JS):
        assert "</script" not in part.lower(), "a closing script tag would end the inline script early"
    page = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Alt+R</title>
<style>
{(SKILL / "assets/base.css").read_text()}
</style>
{wall_block()}
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
{MODAL_JS}
</script>
</body>
</html>
"""
    OUT.write_text(page)
    print(f"wrote {OUT} ({len(page) // 1024} KB) · notes carried: {notes[1].strip() != '{}'} · concurs carried: {concurs[1].strip() != '{}'}")


if __name__ == "__main__":
    main()
