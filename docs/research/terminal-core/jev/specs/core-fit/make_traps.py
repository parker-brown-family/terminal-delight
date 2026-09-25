"""core-fit trap anchors: matched pairs, written 2026-09-25 before any call.

Parts of an input: the candidate, each evidence field. Guide words kept:
  field:no        the field that would answer is absent        -> evidence_silent (unknown is not zero)
  field:reverse   the evidence says the capability is absent   -> does_not
  field:other-than the evidence answers a neighbouring question -> evidence_silent
  claim vs shown  the candidate's self-description asserts it, the interface does not show it
The evidence below is invented for the traps; it describes no real project.
"""
import json
from pathlib import Path

OUT = Path(__file__).parent / "anchors" / "traps"
OUT.mkdir(parents=True, exist_ok=True)
W = "2026-09-25, before any call"


def cand(i, **fields):
    return {"id": f"trap/{i}", "label": f"Core {i.upper()} (a fixture)", "kitty_tags": "unknown",
            "evidence": {k: {"recorded": v} for k, v in fields.items()}}


GRID_OK = "Each cell is a `Cell { c: char, fg: Color, bg: Color, flags: Flags, extra: Option<Vec<char>> }`; Flags has BOLD, ITALIC, UNDERLINE, INVERSE, WIDE_CHAR and WIDE_CHAR_SPACER; `extra` holds combining marks. `grid.row(i)` returns a row by index, including rows in history."
GRID_BAD = "Cells expose only their character through `cell.ch()`; colours and attributes are resolved inside the bundled renderer and are not part of the public API."
anchors = [
    ("cells-base", cand("a", grid_api=GRID_OK), {"cell_contents": ["provides"], "wide_chars": ["provides", "provides_with_work"], "zero_width": ["provides", "provides_with_work"]}),
    ("cells-flip", cand("b", grid_api=GRID_BAD), {"cell_contents": ["does_not"]}),
    ("silent-base", cand("c", grid_api=GRID_OK, kitty_graphics="Implements Kitty transmit and direct placement."), {"cell_contents": ["provides"]}),
    ("silent-flip", cand("d", kitty_graphics="Implements Kitty transmit and direct placement."), {"cell_contents": ["evidence_silent"], "scrollback_read": ["evidence_silent"]}),
    ("thread-base", cand("e", threading_and_loop="`Term` is Send; embedders wrap it in a `Mutex` shared between the reader thread and the renderer."), {"threading": ["provides"]}),
    ("thread-flip", cand("f", threading_and_loop="Handle types are !Send + !Sync by design. Callers must drive all operations from a single thread."), {"threading": ["does_not"]}),
    ("loop-base", cand("g", threading_and_loop="`Processor::advance(&mut term, bytes)` parses bytes the embedder supplies; the PTY layer is an optional feature."), {"feed_loop": ["provides"]}),
    ("loop-flip", cand("h", threading_and_loop="The library opens the pseudoterminal, spawns its own reader thread and parses internally; there is no function for handing it bytes."), {"feed_loop": ["does_not"]}),
    ("place-base", cand("i", kitty_graphics="Placements are stored in a public map keyed by (image_id, placement_id) with dest_row, dest_col, columns and rows; decoded pixels are kept per image.", kitty_tags_note="n/a"), {"placements_readable": ["provides", "provides_with_work"]}),
    ("place-flip", cand("j", kitty_graphics="APC sequences are consumed by the parser and discarded; no image state is kept."), {"placements_readable": ["does_not"]}),
    ("snap-base", cand("k", state_snapshot="A Formatter re-emits the screen as VT sequences, with options to include modes, the scrolling region, tab stops and the cursor."), {"snapshot": ["provides"]}),
    ("snap-flip", cand("l", state_snapshot="There is no serialisation; the state exists only in memory while the terminal runs."), {"snapshot": ["does_not"]}),
    ("claim-vs-shown", cand("m", integration_path="Described by its authors as a drop-in replacement for alacritty_terminal with full image support."), {"placements_readable": ["evidence_silent", "provides_with_work"], "feed_loop": ["evidence_silent", "provides_with_work", "provides"]}),
]
def mat(i, **fields):
    return {"id": f"trap/{i}", "label": f"Core {i.upper()} (a fixture)", "kitty_tags": "unknown", "evidence": {},
            "maturity": {k: {"recorded": v} for k, v in fields.items()}}


maturity_anchors = [
    ("traj-unstable", mat("n", api_stability="This library is currently in development and the API is not yet stable. Breaking changes are expected.", releases_last_12mo="4"),
     {"api_trajectory": ["declared_unstable"]}),
    ("traj-dormant", mat("o", api_stability="Pre-1.0 (0.3.3) with no stability statement.", releases_last_12mo="0 — nothing released since 2023-11-22", maintainers="0 contributors in the last 12 months; no commits since 2024-07-24"),
     {"api_trajectory": ["dormant"]}),
    ("traj-active", mat("p", api_stability="Pre-1.0 (0.26); breaking changes are listed in bold in each minor release's changelog.", releases_last_12mo="3", maintainers="2 contributors with over 10% of commits"),
     {"api_trajectory": ["active_pre_1_0"]}),
    ("traj-silent", mat("q", latest_release="0.5.28 on 2026-09-17"),
     {"api_trajectory": ["not_established", "active_pre_1_0"]}),
]
for name, item, expect in maturity_anchors:
    (OUT / f"{name}.json").write_text(json.dumps({"call": "maturity", "item": item, "expect": expect, "written": W}, indent=2))

for name, item, expect in anchors:
    for f in list(item["evidence"]):
        if f.endswith("_note"):
            del item["evidence"][f]
    (OUT / f"{name}.json").write_text(json.dumps({"call": "assess", "item": item, "expect": expect, "written": W}, indent=2))
print("wrote", len(anchors), "core-fit traps")
