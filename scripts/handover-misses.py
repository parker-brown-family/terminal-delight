#!/usr/bin/env python3
"""How many printed handovers reached the artifact chip, and how many of those
a window restart would keep (#909).

An agent hands a file over by printing a `Deliverable:` line with a link, by
calling `declare_deliverable`, or both. The verb always reaches Terminal
Delight. A printed line has two roads in: the once-a-second screen scan
(`TerminalView::scan_handovers`) and the derive sweep, which re-reads the
agent's transcript and keeps its newest `Deliverable:` line (`derive.rs`).

Three sides, all read off disk, nothing sent anywhere:

- the agents' own transcripts (`~/.claude/projects/*/*.jsonl`): every
  `Deliverable:` line with a link on it or the next line, and every
  `declare_deliverable` call;
- each conversation's history page (`$XDG_STATE_HOME/terminal-delight/
  handovers/<root>/handed-over.md`): what the chip lists, and the word it
  gives each row (`said`, `presented`, `declared`);
- each conversation's record (`.../conversations/<root>.ws`): the `handover-`
  cards TD filed, which is what the list is rebuilt from after a restart.

Every handover printed after `--since` and not also declared lands in one of:

    on the chip, filed    listed on the page and in the record: survives a restart
    on the chip only      listed on the page, nowhere in the record: a restart forgets it
    missed                on neither
    unknown               the conversation has no page and no record: a pane the
                          window never bound, whose list lived only in memory
    printed before        the same file was handed over before `--since`, so the
                          chip already held it; a re-print is not a new handover

`--since` should be when the window running the build under test started
(`ps -o lstart= -p <window pid>`). The declared handovers are counted too, as
the control: all of them should be on the page and in the record.

    scripts/handover-misses.py --since "2026-10-07 06:28" [--bindings bindings.json]

`--bindings` is `terminal-delight bindings` saved to a file: it maps a session
to its conversation's root. Without it a session is taken to be its own root,
which is true of every conversation that has not been resumed or compacted.
Claude transcripts only; Codex rollouts are not read.
"""
import argparse
import glob
import json
import os
import re
import sys
from datetime import datetime
from urllib.parse import unquote

LINK = re.compile(r"(file://\S+|https?://\S+)")
PAGE_ROW = re.compile(r"^- \*\*\[(?P<label>.*)\]\((?P<href>[^)\s]+)\)\*\*$")
WORDS = ("declared", "presented", "said")


def key_of(href):
    """The identity `handover::key_of` gives a link: a file's path."""
    h = href.strip().rstrip(").,`>\"")
    if h.startswith("/"):
        return h
    if h.lower().startswith("file:"):
        rest = h[5:]
        if rest.startswith("//"):
            rest = rest[2:]
            if rest.startswith("localhost"):
                rest = rest[len("localhost"):]
        return unquote(rest.split("#")[0])
    return h


def transcript_handovers(path, since_ms):
    """(printed, declared, before): the first two {key: (ms, label)} after
    `since_ms`; `before` the keys handed over either way before it."""
    printed, declared, before = {}, {}, set()
    for raw in open(path, errors="replace"):
        try:
            ev = json.loads(raw)
        except ValueError:
            continue
        if ev.get("type") != "assistant":
            continue
        try:
            ms = int(datetime.fromisoformat(ev["timestamp"].replace("Z", "+00:00")).timestamp() * 1000)
        except (KeyError, ValueError):
            continue
        for part in (ev.get("message") or {}).get("content") or []:
            found = []
            if part.get("type") == "text":
                lines = part.get("text", "").splitlines()
                for i, line in enumerate(lines):
                    s = line.strip()
                    if not s.startswith("Deliverable:"):
                        continue
                    rest = s[len("Deliverable:"):].strip()
                    m = LINK.search(rest) or (LINK.search(lines[i + 1].strip()) if i + 1 < len(lines) else None)
                    if m:
                        found.append((printed, key_of(m.group(1)), rest.split(" — ")[0]))
            elif part.get("type") == "tool_use" and part.get("name", "").endswith("declare_deliverable"):
                inp = part.get("input") or {}
                if inp.get("href"):
                    found.append((declared, key_of(inp["href"]), inp.get("label") or ""))
            for into, key, label in found:
                if ms < since_ms:
                    before.add(key)
                else:
                    into.setdefault(key, (ms, label))
    return printed, declared, before


def page_rows(state, root):
    """{key: word} for the rows on the conversation's history page, or None
    when it has no page."""
    path = os.path.join(state, "handovers", root, "handed-over.md")
    if not os.path.exists(path):
        return None
    rows, key = {}, None
    for line in open(path, errors="replace"):
        m = PAGE_ROW.match(line.rstrip("\n"))
        if m:
            key = key_of(m.group("href"))
            rows[key] = "?"
            continue
        if key and line.strip():
            rows[key] = next((w for w in WORDS if f"· {w}" in line), "?")
            key = None
    return rows


def filed(state, root):
    """The keys TD filed as `handover-` cards into the conversation's record,
    or None when it has no record."""
    path = os.path.join(state, "conversations", f"{root}.ws")
    if not os.path.exists(path):
        return None
    out = set()
    for raw in open(path, errors="replace"):
        try:
            rec = json.loads(raw)
        except ValueError:
            continue
        if rec.get("t") != "said" or not str(rec.get("id", "")).startswith("handover-"):
            continue
        href = ((rec.get("surface") or {}).get("model") or {}).get("href")
        if href:
            out.add(key_of(href))
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--since", required=True, help='local time, e.g. "2026-10-07 06:28"')
    ap.add_argument("--bindings", help="terminal-delight bindings, saved as JSON")
    ap.add_argument("--projects", default=os.path.expanduser("~/.claude/projects"))
    ap.add_argument("--state", default=os.path.join(
        os.environ.get("XDG_STATE_HOME", os.path.expanduser("~/.local/state")), "terminal-delight"))
    args = ap.parse_args()
    since_ms = int(datetime.strptime(args.since, "%Y-%m-%d %H:%M").timestamp() * 1000)

    roots = {}
    if args.bindings:
        for pane in json.load(open(args.bindings)).get("panes", []):
            if pane.get("session") and pane.get("root"):
                roots[pane["session"]] = pane["root"]

    rows = []  # (category, ms, session, label, key, page word)
    control = {"declared": 0, "on the page": 0, "in the record": 0}
    for path in sorted(glob.glob(os.path.join(args.projects, "*", "*.jsonl"))):
        if os.path.getmtime(path) * 1000 < since_ms:
            continue
        sid = os.path.basename(path)[:-len(".jsonl")]
        printed, declared, before = transcript_handovers(path, since_ms)
        if not printed and not declared:
            continue
        root = roots.get(sid, sid)
        page, record = page_rows(args.state, root), filed(args.state, root)
        for key in declared:
            control["declared"] += 1
            control["on the page"] += bool(page and key in page)
            control["in the record"] += bool(record and key in record)
        for key, (ms, label) in printed.items():
            if key in declared:
                continue
            word = (page or {}).get(key, "")
            if key in before:
                cat = "printed before"
            elif page is None and record is None:
                cat = "unknown"
            elif page and key in page:
                cat = "on the chip, filed" if record and key in record else "on the chip only"
            else:
                cat = "missed"
            rows.append((cat, ms, sid, label, key, word))

    order = ["on the chip, filed", "on the chip only", "missed", "unknown", "printed before"]
    counts = {c: sum(1 for r in rows if r[0] == c) for c in order}
    print(f"since {args.since}: {len(rows)} handovers printed and not declared")
    for c in order:
        print(f"  {c:20} {counts[c]}")
    measured = counts["on the chip, filed"] + counts["on the chip only"] + counts["missed"]
    if measured:
        print(f"reached the chip: {measured - counts['missed']} of {measured}; "
              f"would survive a restart: {counts['on the chip, filed']} of {measured}")
    print(f"control, declared with the verb: {control['declared']}, on the page {control['on the page']}, "
          f"in the record {control['in the record']}")
    for cat, ms, sid, label, key, word in sorted(rows, key=lambda r: (order.index(r[0]), r[1])):
        if cat == "on the chip, filed":
            continue
        when = datetime.fromtimestamp(ms / 1000).strftime("%m-%d %H:%M")
        print(f"  {cat:18} {when} {sid[:8]} {word or '-':9} {label[:38]!r} {key}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
