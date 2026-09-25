"""Turn the dossiers into Jev items: one per quoted fact (claim-check), one per core (core-fit).

    python3 docs/research/terminal-core/scripts/claims.py claims
        → jev/items/claims.json          every sourced fact that carries a quote
        → jev/items/claims-unquoted.json every sourced fact that does not (never sent to Jev)
    python3 docs/research/terminal-core/scripts/claims.py cores <claim-check run dir>
        → jev/items/cores.json           one item per core, each fact with its claim-check verdict

Facts the agents labelled inferred (td_migration, biggest_risk, kitty_tags) carry no
source by design and are not claims about a source, so they are not checked here.
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
JEV = ROOT / "jev"
QUOTE = re.compile(r'^\s*quote:\s*"(?P<q>.*?)"\s*(?:—|--|-|—)\s*(?P<rest>.*)$', re.S)
QUOTE_LOOSE = re.compile(r'^\s*quote:\s*"(?P<q>.*)"', re.S)

# The rubric's hints, shortened into what Jev reads as `claim.field`.
FIELD_MEANS = {
    "what_it_is": "what the core or option is and who maintains it",
    "license": "the licence it is published under",
    "language": "the language it is implemented in",
    "integration_path": "how Terminal Delight would consume it (crate, C bindings, fork, or not a library)",
    "build_requirements": "the toolchains and system libraries it needs beyond stable cargo",
    "latest_release": "its newest release, with version and date",
    "first_release": "the date of its first published release",
    "releases_last_12mo": "how many releases it had in the 12 months before 2026-09-25",
    "api_stability": "what the project says about the stability of its API",
    "maintainers": "who carries the work: contributors with a large share of recent commits, and the lead",
    "adoption": "its downloads, dependents and the products that embed it",
    "kitty_graphics": "which parts of the Kitty graphics protocol it implements",
    "sixel": "whether it decodes Sixel images",
    "iterm2_images": "whether it supports iTerm2 inline images (OSC 1337)",
    "picture_anchoring": "how a placed picture is tied to the grid so it scrolls, and its storage quota",
    "vt_conformance": "published evidence of how conformant its terminal emulation is",
    "vt_features": "which modern terminal features it supports (keyboard protocol, hyperlinks, synchronized output, clipboard, graphemes, reflow, alternate screen, bracketed paste, SGR mouse)",
    "performance_evidence": "published or measured parsing speed and memory use",
    "grid_api": "how an embedding program reads the screen: the types and methods for rows and cells, and what a cell carries",
    "damage_tracking": "whether it reports which rows changed since the last read",
    "state_snapshot": "whether the whole terminal state can be serialised or replayed for a client that attaches later",
    "threading_and_loop": "who owns the read and parse loop, whether it is thread-safe, and whether it includes a PTY layer",
    "reply_channel": "how its answers to programs' queries reach the embedding program",
    "primary_sources": "the sources the research read",
}
# What the bake-off observed (docs/research/terminal-core/bake/), in the words core-fit v2 sends.
MEASURED = {
    "alacritty-own-loop": "Fed six recorded image programs through its public Processor::advance, alacritty_terminal 0.26 dropped every picture and kept no image state; its Term was driven from the harness's own loop and its replies arrived as PtyWrite events.",
    "rio-vt": "Fed six recorded image programs through Processor::advance, rio-vt 0.5.28 stored their pictures and the harness read each placement (image id, row, column, columns, rows) from the public map graphics.kitty_placements, and virtual placements from graphics.kitty_virtual_placements; replies arrived as PtyWrite events.",
    "libghostty-vt": "Fed six recorded image programs through vt_write, libghostty-vt 0.2.1 stored their pictures and the harness read each placement's image, viewport row and column, and size in cells through PlacementIterator; virtual placements are flagged; PNG needed a decoder the harness supplied; replies arrived through on_pty_write.",
    "ghostty-shadow": "Its picture half is libghostty-vt, which in the bake-off stored every picture and exposed placements through PlacementIterator; the text half, alacritty_terminal, dropped them all.",
    # added 2026-09-25 when bake/wezbake ran; the frame already names this field for every core the bake-off ran
    "wezterm-term": "Fed six recorded image programs through Terminal::advance_bytes with enable_kitty_graphics turned on, wezterm-term (git b09b56c) stored the pictures of the four direct-placement programs as per-cell image slices, from which the harness rebuilt each placement (image id, row, column, columns, rows) by scanning cells; for the two placeholder programs it ignored U=1, drew the picture at the cursor and left the placeholder characters as text. Replies, including Kitty OK answers and DA1, arrived through the writer passed to Terminal::new. The harness read scrollback through lines_in_phys_range.",
}
SKIP = {"primary_sources", "td_migration", "biggest_risk", "kitty_tags", "licence_gate", "kitty_coverage", "evidence_grade"}


def load():
    return json.loads((JEV / "dossiers.json").read_text())


def split_note(note: str):
    m = QUOTE.match(note or "") or QUOTE_LOOSE.match(note or "")
    if not m:
        return None, note or ""
    q = m.group("q").replace('\\"', '"').strip()
    rest = m.groupdict().get("rest") or ""
    return q, rest.strip()


def claims():
    data = load()
    items, unquoted = [], []
    for d in data["dossiers"]:
        for key, f in d["fields"].items():
            if key in SKIP or key not in FIELD_MEANS:
                continue
            src = ((f.get("provenance") or {}).get("source") or "").strip()
            if not src:
                continue
            quote, remark = split_note(f.get("note", ""))
            rec = {
                "id": f"{d['id']}/{key}", "subject_id": d["id"], "subject": d["label"],
                "field": key, "field_means": FIELD_MEANS[key], "value": f.get("value"),
                "quote": quote, "source": src, "remark": remark,
                "measured_by_command": src.startswith("measured"),
                "labelled_inferred": "[inferred]" in (f.get("note") or ""),
                "confidence": (f.get("provenance") or {}).get("confidence"),
            }
            (items if quote else unquoted).append(rec)
    (JEV / "items").mkdir(exist_ok=True)
    (JEV / "items" / "claims.json").write_text(json.dumps(items, indent=1, ensure_ascii=False))
    (JEV / "items" / "claims-unquoted.json").write_text(json.dumps(unquoted, indent=1, ensure_ascii=False))
    print(f"{len(items)} quoted claims, {len(unquoted)} sourced without a quote")


def cores(run_dir: str):
    data = load()
    verdicts = {}
    for line in (Path(run_dir) / "records.jsonl").read_text().splitlines():
        r = json.loads(line)
        verdicts[r["item"]] = (r.get("axes") or {}).get("verdict")
    items = []
    for d in data["dossiers"]:
        ev = {}
        for key in ("grid_api", "damage_tracking", "state_snapshot", "threading_and_loop",
                    "reply_channel", "kitty_graphics", "picture_anchoring", "integration_path"):
            f = d["fields"].get(key)
            if not f or f.get("value") in (None, ""):
                continue
            quote, _ = split_note(f.get("note", ""))
            ev[key] = {"recorded": str(f["value"]), "quoted": quote,
                       "check": verdicts.get(f"{d['id']}/{key}")}
        mat = {}
        for key in ("api_stability", "latest_release", "releases_last_12mo", "maintainers"):
            f = d["fields"].get(key)
            if not f or f.get("value") in (None, ""):
                continue
            mat[key] = {"recorded": str(f["value"]), "check": verdicts.get(f"{d['id']}/{key}")}
        tags = (d["fields"].get("kitty_tags") or {}).get("value")
        rec = {"id": d["id"], "label": d["label"], "group": d.get("group"),
               "kitty_tags": str(tags).strip().lower() if tags else "unknown", "evidence": ev, "maturity": mat}
        if d["id"] in MEASURED:
            rec["measured"] = MEASURED[d["id"]]
        items.append(rec)
    (JEV / "items" / "cores.json").write_text(json.dumps(items, indent=1, ensure_ascii=False))
    print(f"{len(items)} cores")


if __name__ == "__main__":
    {"claims": lambda: claims(), "cores": lambda: cores(sys.argv[2])}[sys.argv[1]]()
