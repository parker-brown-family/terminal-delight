"""What a Terminal Lab run could and could not check.

A green run says the two cores agree on what the lab looked at. This module
says what that was. It cuts every case's bytes into control-sequence tokens
(`tokens`), sorts them into the families in `catalogue.json`, and reports each
family in exactly one of these states, never folding one into another:

    checked      sent, and its effect is part of what lab.rs's `observe()`
                 records (text, style runs, cursor, named modes, history)
    partly       sent, and only part of its effect is recorded
    unseen       sent, but its effect is not recorded at all: the cores may
                 disagree about it and the run cannot tell
    never sent   in the catalogue, absent from every case in this run

plus `uncatalogued`: tokens the bytes held that no family names, which is a
gap in the catalogue rather than in the cores. Standard library only.
"""

from __future__ import annotations

import json
import re
import unicodedata
from collections import Counter
from pathlib import Path

CATALOGUE = Path(__file__).resolve().parent / "catalogue.json"
STATES = ("checked", "partly", "unseen", "never sent")

C0_NAMES = {0x00: "NUL", 0x07: "BEL", 0x08: "BS", 0x09: "HT", 0x0A: "LF", 0x0B: "VT", 0x0C: "FF",
            0x0D: "CR", 0x0E: "SO", 0x0F: "SI", 0x7F: "DEL"}


def _sgr(params: str) -> list[str]:
    """SGR parameters as families: `SGR 1`, `SGR 38;5`, `SGR 38;2`, `SGR 4:3`."""
    if params == "":
        return ["SGR 0"]
    out, parts, i = [], params.split(";"), 0
    while i < len(parts):
        p = parts[i]
        if ":" in p:
            head, rest = p.split(":", 1)
            sub = rest.split(":", 1)[0]
            out.append(f"SGR {head or '0'}:{sub}" if head in ("38", "48", "58", "4") else f"SGR {head}:")
            i += 1
            continue
        p = p or "0"
        if p in ("38", "48", "58") and i + 1 < len(parts) and parts[i + 1] in ("2", "5"):
            out.append(f"SGR {p};{parts[i + 1]}")
            i += 2 + (3 if parts[i + 1] == "2" else 1)
            continue
        out.append(f"SGR {int(p) if p.isdigit() else p}")
        i += 1
    return out


def _csi(body: bytes) -> list[str]:
    text = body.decode("latin-1")
    final = text[-1]
    head = text[:-1]
    marker = head[0] if head and head[0] in "<=>?" else ""
    rest = head[len(marker):]
    params = "".join(c for c in rest if "0" <= c <= "?")
    inter = "".join(c for c in rest if " " <= c <= "/").replace(" ", "SP ")
    if final == "m" and not marker and not inter:
        return _sgr(params)
    if final in "hl" and not inter and marker in ("", "?"):
        modes = [m for m in params.split(";") if m] or ["0"]
        return [f"CSI {marker}{int(m) if m.isdigit() else m} h/l" for m in modes]
    return [f"CSI {marker}{inter}{final}".replace("  ", " ").strip()]


def _string_body(data: bytes, start: int) -> tuple[bytes, int, bool]:
    """A string sequence's body from `start` to BEL or ST; the index after
    its terminator; whether it was terminated at all."""
    i = start
    while i < len(data):
        if data[i] == 0x07:
            return data[start:i], i + 1, True
        if data[i] == 0x1B and i + 1 < len(data) and data[i + 1] == 0x5C:
            return data[start:i], i + 2, True
        if data[i] == 0x1B:
            return data[start:i], i, False
        i += 1
    return data[start:], i, False


def _text(run: bytes) -> list[str]:
    out = []
    try:
        text = run.decode()
    except UnicodeDecodeError:
        out.append("text: invalid UTF-8")
        text = run.decode(errors="ignore")
    for ch in text:
        code = ord(ch)
        if code < 0x80:
            out.append("text: ASCII")
        elif code == 0x200D:
            out.append("text: zero-width joiner")
        elif unicodedata.combining(ch) or unicodedata.category(ch) in ("Mn", "Me"):
            out.append("text: combining mark")
        elif code >= 0x1F000:
            out.append("text: emoji")
        elif unicodedata.east_asian_width(ch) in ("W", "F"):
            out.append("text: wide")
        else:
            out.append("text: other non-ASCII")
    return out


def tokens(data: bytes) -> list[str]:
    """Every control-sequence family the bytes hold, once per occurrence, and
    every printed character's text family."""
    out: list[str] = []
    i, text_start = 0, 0

    def flush(end: int) -> None:
        if end > text_start:
            out.extend(_text(data[text_start:end]))

    while i < len(data):
        b = data[i]
        if b != 0x1B and (b < 0x20 or b == 0x7F):
            flush(i)
            out.append(C0_NAMES.get(b, f"C0 0x{b:02x}"))
            i += 1
            text_start = i
            continue
        if b != 0x1B:
            i += 1
            continue
        flush(i)
        if i + 1 >= len(data):
            out.append("unterminated ESC")
            i += 1
        else:
            kind = data[i + 1]
            if kind == ord("["):
                j = i + 2
                while j < len(data) and not 0x40 <= data[j] <= 0x7E:
                    if data[j] == 0x1B or data[j] < 0x20:
                        break
                    j += 1
                if j < len(data) and 0x40 <= data[j] <= 0x7E:
                    out.extend(_csi(data[i + 2:j + 1]))
                    i = j + 1
                else:
                    out.append("unterminated CSI")
                    i = j
            elif kind == ord("]"):
                body, i, done = _string_body(data, i + 2)
                number = body.split(b";", 1)[0].decode("latin-1")
                out.append(f"OSC {number}" if number.isdigit() else "OSC ?")
                if not done:
                    out.append("unterminated OSC")
            elif kind == ord("P"):
                body, i, done = _string_body(data, i + 2)
                m = re.match(rb"[0-9;]*([ -/]*)([@-~])", body)
                out.append(f"DCS {(m.group(1) + m.group(2)).decode('latin-1')}" if m else "DCS ?")
                if not done:
                    out.append("unterminated DCS")
            elif kind == ord("_"):
                body, i, done = _string_body(data, i + 2)
                out.append("APC G" if body[:1] == b"G" else "APC")
                if not done:
                    out.append("unterminated APC")
            elif kind in (ord("^"), ord("X")):
                body, i, done = _string_body(data, i + 2)
                out.append("PM" if kind == ord("^") else "SOS")
            else:
                j = i + 1
                while j < len(data) and 0x20 <= data[j] <= 0x2F:
                    j += 1
                if j < len(data) and 0x30 <= data[j] <= 0x7E:
                    seq = data[i + 1:j + 1].decode("latin-1")
                    out.append("ESC " + " ".join(seq) if len(seq) > 1 else f"ESC {seq}")
                    i = j + 1
                else:
                    out.append("unterminated ESC")
                    i = j
        text_start = i
    flush(len(data))
    return out


def load_catalogue(path: Path = CATALOGUE) -> list[dict]:
    families = json.loads(path.read_text())["families"]
    seen = set()
    for family in families:
        if family["id"] in seen:
            raise ValueError(f"catalogue: {family['id']} twice")
        seen.add(family["id"])
        if family.get("observed") not in (True, False, "partly"):
            raise ValueError(f"catalogue: {family['id']} must say whether observe() sees it")
        if family["observed"] is not True and not family.get("why"):
            raise ValueError(f"catalogue: {family['id']} is not fully observed, so it needs a why")
        family["_match"] = re.compile(family["match"])
    return families


def coverage(sources: dict[str, list[bytes]], families: list[dict] | None = None) -> dict:
    """The map for one run. `sources` names each kind of case (generated,
    named, recorded) and holds each case's bytes."""
    families = load_catalogue() if families is None else families
    counts: dict[str, Counter] = {name: Counter() for name in sources}
    for name, cases in sources.items():
        for data in cases:
            counts[name].update(tokens(data))
    seen_tokens = Counter()
    for c in counts.values():
        seen_tokens.update(c)
    claimed: set[str] = set()
    rows = []
    for family in families:
        matched = [t for t in seen_tokens if family["_match"].fullmatch(t)]
        claimed.update(matched)
        sent = {name: sum(counts[name][t] for t in matched) for name in sources}
        total = sum(sent.values())
        if total == 0:
            state = "never sent"
        else:
            state = {True: "checked", "partly": "partly", False: "unseen"}[family["observed"]]
        rows.append({"id": family["id"], "name": family["name"], "state": state, "sent": sent,
                     "observed": family["observed"], "why": family.get("why"), "how": family.get("how")})
    uncatalogued = {t: {name: counts[name][t] for name in sources if counts[name][t]}
                    for t in sorted(seen_tokens) if t not in claimed}
    totals = {state: sum(1 for r in rows if r["state"] == state) for state in STATES}
    return {"families": rows, "states": totals, "uncatalogued": uncatalogued,
            "sources": {name: len(cases) for name, cases in sources.items()}}


def summary_lines(cover: dict) -> list[str]:
    """The run summary's section on what it could not check."""
    t = cover["states"]
    src = ", ".join(f"{n} {name}" for name, n in cover["sources"].items())
    lines = ["## What this run could not check", "",
             f"Families of terminal behaviour in `scripts/lab/catalogue.json`, against this run's cases ({src}):",
             "", f"- **checked** (sent, and the effect is recorded): {t['checked']}",
             f"- **partly** (sent, only part of the effect is recorded): {t['partly']}",
             f"- **unseen** (sent, but the effect is not recorded, so a disagreement would pass): {t['unseen']}",
             f"- **never sent** (no case in this run exercised it): {t['never sent']}",
             f"- **uncatalogued** (in the bytes, named by no family): {len(cover['uncatalogued'])}", ""]

    def table(state: str, title: str) -> None:
        rows = [r for r in cover["families"] if r["state"] == state]
        if not rows:
            return
        lines.extend([title, "", "| family | sent (" + " / ".join(cover["sources"]) + ") | why |",
                      "|---|---|---|"])
        for r in rows:
            sent = " / ".join(str(r["sent"][s]) for s in cover["sources"])
            lines.append(f"| {r['name']} | {sent} | {r.get('why') or ''} |")
        lines.append("")

    table("unseen", "Sent, but invisible to the comparison:")
    table("partly", "Sent, and only partly visible:")
    never = [r["name"] for r in cover["families"] if r["state"] == "never sent"]
    if never:
        lines.extend(["Never sent: " + "; ".join(never) + ".", ""])
    if cover["uncatalogued"]:
        shown = ", ".join(f"`{t}`" for t in list(cover["uncatalogued"])[:40])
        more = len(cover["uncatalogued"]) - 40
        lines.extend(["Uncatalogued: " + shown + (f", and {more} more" if more > 0 else "") + ".", ""])
    return lines
