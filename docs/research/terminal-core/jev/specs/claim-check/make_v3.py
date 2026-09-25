"""Move claim-check from version 2 to version 3, on the evidence of the v2 run.

Written 2026-09-25 after reading a seeded sample (seed 7) of ten v2 'silent' items by hand
and before any v3 call. What v2 showed:

  - 137 of 315 (44%) came back 'silent'. In the sample, eight of ten quotes were code,
    configuration or command output from which the agent inferred its value — a parser
    state that discards a sequence, offered for "no Kitty graphics"; a Cargo dependency
    line, offered for "cargo only". 'Silent' was the literally right answer to "does the
    quote state it", and the map expected too many 'states' again: the world differs.
  - One headline was cut wrongly: 'kitty-keyboard: yes' became 'kitty-keyboard'.

What changes: an option 'implies' separates a quote that is the agent's premise from a
quote that is unrelated, so the page can say which facts a source states and which an
agent concluded from its evidence. The verdict gains 'inferred'. The headline no longer
breaks at a colon that follows a single token. New traps pin implies against silent and
contradicts. Expectations are v3's predictions, written before its first call.
"""
import json
from pathlib import Path

HERE = Path(__file__).parent
spec = json.loads((HERE / "claim-check.jds.json").read_text())
spec["version"] = 3

q = spec["calls"][0]["questions"]["support"]
crit = q["criteria"]
new = {}
for k, v in crit.items():
    new[k] = v
    if k == "states_part":
        new["implies"] = {
            "means": "The quote is code, configuration, a signature or command output from which `claim.value` follows directly: a reader who knows the format would conclude it, though the quote does not say it in words.",
            "not_for": "A quote that only touches the topic without settling it, or one that needs facts from outside the quote."
        }
q["criteria"] = new

m = spec["meaning"]["check.support"]
m["options"]["implies"] = {"means": "the fact is the agent's reading of its evidence: checkable, and not the source's own words", "route": "flag:inferred"}
m["expect"] = {"option_shares": {"states": [0.1, 0.45], "implies": [0.15, 0.5], "states_part": [0.08, 0.35], "contradicts": [0.0, 0.08], "silent": [0.03, 0.2]},
               "confidence_median": [0.4, 1.0],
               "why": "the v2 sample showed most quotes are premises (code, manifests, command output); implies should absorb most of v2's silent; five options spread confidence thinner"}
m["confusable"].append({"pair": ["implies", "silent"], "margin": 0.2,
                        "means": "code that bears on the fact only through knowledge outside the quote", "route": "flag:unsupported"})
for a in spec["axes"]:
    if a["id"] == "verdict":
        a["compute"] = ("'human' if axis.numbers_match == 'mismatch' else ('human' if axis.support == 'contradicts' or "
                        "conf(check.support) < 0.305 or axis.other_subject >= 0.755 else ('supported' if axis.support == 'states' "
                        "else ('inferred' if axis.support == 'implies' else ('overclaims' if axis.support == 'states_part' else 'unsupported'))))")
m["confidence_bands"] = [
    {"id": "contested", "range": [0.0, 0.305], "means": "two readings are live across five options", "route": "human"},
    {"id": "clear", "range": [0.305, 1.0], "means": "one reading dominates", "route": "per-option"},
]
spec["output"]["record"]["support"] = "states | states_part | implies | contradicts | silent | null"
spec["output"]["record"]["verdict"] = "supported | inferred | overclaims | unsupported | human | null"
spec["output"]["combination"] = ("version 3: as version 2, plus 'inferred' for a headline that follows directly from quoted code, "
                                 "configuration or command output; the contested edge moves to 0.305 for five options.")
spec["evaluation"]["runs"].append({"name": "v2", "version": 2, "items": 315, "surprises": [
    "share choosing states: expected [0.6, 0.95], observed 0.143",
    "share choosing states_part: expected [0.0, 0.15], observed 0.368",
    "share choosing silent: expected [0.03, 0.3], observed 0.435"],
    "hand_read": "seed 7, ten 'silent' items: eight quotes were premises the agent reasoned from, one headline cut wrongly, one genuinely unrelated"})
spec["changelog"].append({"version": 3, "date": "2026-09-25",
    "change": "add option 'implies' and verdict 'inferred'; contested edge 0.305; headline keeps 'label: answer'; traps for implies",
    "evidence": "run v2: 44% silent; a seeded hand-read of ten found eight premises-not-statements. The question conflated 'the source says it' with 'the agent concluded it from the source'."})
(HERE / "claim-check.jds.json").write_text(json.dumps(spec, indent=2, ensure_ascii=False))
print("claim-check is version", spec["version"])
