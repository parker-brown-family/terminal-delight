"""Frame builder and code axes for the core-fit spec.

An item is one candidate core, built by docs/research/terminal-core/scripts/claims.py
from the research-delight dossier after claim-check has run:

    {"id": "rio-vt", "label": "rio-vt — Rio's embeddable core",
     "evidence": {"grid_api": {"recorded": "...", "quoted": "...", "check": "supported"}, ...}}

Only the fields that describe how an embedder uses the core go into the state.
A field the agents could not fill is omitted, never sent empty: an empty string
reads as a claim that there is nothing (using-jev.md, "State"). A fact claim-check
could not confirm travels with its verdict, named as such, so Jev can read it as
the weaker thing it is.
"""

from __future__ import annotations

EVIDENCE_FIELDS = (
    "grid_api", "damage_tracking", "state_snapshot", "threading_and_loop",
    "reply_channel", "kitty_graphics", "picture_anchoring", "integration_path",
)

# The requirements and their weights: TD's must-haves weigh 3, the important 2,
# the nice-to-have 1. These weights are the assembling agent's, not Parker's.
WEIGHTS = {
    "cell_contents": 3, "zero_width": 3, "underline_colour": 2, "wide_chars": 3,
    "scrollback_read": 3, "damage": 2, "snapshot": 2, "feed_loop": 3, "replies_out": 3,
    "threading": 2, "placements_readable": 3, "placeholders": 2, "media_policy": 1,
}
VALUE = {"provides": 1.0, "provides_with_work": 0.5, "does_not": 0.0}


def core_evidence(item: dict, ctx: dict) -> dict | None:
    ev = {}
    for f in EVIDENCE_FIELDS:
        e = (item.get("evidence") or {}).get(f)
        if not e or not str(e.get("recorded") or "").strip():
            continue
        entry = {"recorded_by_research_agent": e["recorded"]}
        if e.get("quoted"):
            entry["quoted_from_source"] = e["quoted"]
        if e.get("check") and e["check"] != "supported":
            entry["claim_check_verdict"] = e["check"]
        ev[f] = entry
    if not ev:
        return None
    return {"candidate": item["label"], "evidence": ev}


MATURITY_FIELDS = ("api_stability", "latest_release", "releases_last_12mo", "maintainers")


def maturity_evidence(item: dict, ctx: dict) -> dict | None:
    m = {}
    for f in MATURITY_FIELDS:
        e = (item.get("maturity") or {}).get(f)
        if not e or not str(e.get("recorded") or "").strip():
            continue
        entry = {"recorded_by_research_agent": e["recorded"]}
        if e.get("check") and e["check"] != "supported":
            entry["claim_check_verdict"] = e["check"]
        m[f] = entry
    if not m:
        return None
    return {"candidate": item["label"], "maturity": m}


def evidence_fields_present(item: dict, ctx: dict) -> int:
    ev = item.get("evidence") or {}
    return sum(1 for f in EVIDENCE_FIELDS if ev.get(f) and str(ev[f].get("recorded") or "").strip())
