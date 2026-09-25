"""Move claim-check from version 1 to version 2, on the evidence of the v1 run.

Written 2026-09-25 after `jds surprise` on runs/claim-check (v1, 315 facts) and before
any v2 call. What v1 showed, and what changes:

  1. 274 of 315 facts (87%) came back 'states_part'; 11 'states'. Reading the items: the
     values are composite by the rubric's own design ("MIT — an MIT product may link it"
     against a quote of "MIT"). Jev answered the question it was asked, correctly. The
     question was wrong for the material: one excerpt cannot state a composite. Version 2
     checks the value's headline (builders.headline) and labels the rest as elaboration.
  2. 112 verdicts were null: numbers_match returned None for 'no numbers to check', and
     jds reads a None axis as unknown, voiding the rule. It now returns 'none'.
  3. The traps are rewritten so each pair's difference lives in the headline, since the
     headline is now all Jev sees.
Expectations below are v2's predictions, written before its first call.
"""
import json
from pathlib import Path

HERE = Path(__file__).parent
spec = json.loads((HERE / "claim-check.jds.json").read_text())

spec["version"] = 2
spec["frames"]["claim_packet"]["fields"]["claim.value"] = {
    "provenance": "asserted", "from": "reported by the research agent; its headline only",
    "transform": "the value up to its first dash, semicolon, colon or sentence end (builders.headline); the elaboration after it is not sent"}
for a in spec["axes"]:
    if a["id"] == "numbers_match":
        a["means"] = "every multi-digit number in the headline appears in the quote or the command: match, mismatch, or none (no number to check)"
    if a["id"] == "verdict":
        a["compute"] = ("'human' if axis.numbers_match == 'mismatch' else ('human' if axis.support == 'contradicts' or "
                        "conf(check.support) < 0.355 or axis.other_subject >= 0.755 else ('supported' if axis.support == 'states' "
                        "else ('overclaims' if axis.support == 'states_part' else 'unsupported')))")
spec["axes"].insert(-1, {"id": "elaborated", "means": "the value carries more than its headline", "source": "code",
                         "compute": "builders.py:elaborated",
                         "consumer_uses_it_for": "the page marks the elaboration as the agent's, unchecked, so a supported headline never lends its badge to the sentences after it"})
spec["output"]["record"]["numbers_match"] = "match | mismatch | none"
spec["output"]["record"]["elaborated"] = "true | false"
spec["output"]["combination"] = ("version 2: the check covers each value's headline. A number the quote does not contain, a contradiction, "
                                 "a contested reading, or a quote about another project goes to a person; otherwise the Choice decides "
                                 "between supported, overclaims and unsupported. The elaboration is shown as the agent's, unchecked.")
m = spec["meaning"]["check.support"]
m["expect"] = {"option_shares": {"states": [0.6, 0.95], "states_part": [0.0, 0.15], "contradicts": [0.0, 0.08], "silent": [0.03, 0.3]},
               "confidence_median": [0.6, 1.0],
               "why": "a headline is one assertion, and the agents were told to quote the passage the value rests on; a few quotes carry a secondary part of the value (a command's output for a count, a sibling's release), which reads as silent"}
for inv in spec["invariants"]:
    if inv["id"] == "stated-but-numbers-missing":
        inv["when"] = "check.support == 'states' and axis.numbers_match == 'mismatch'"
spec["evaluation"]["runs"] = [{"name": "v1", "version": 1, "items": 315, "surprises": [
    "share choosing states: expected [0.55, 0.95], observed 0.035",
    "share choosing states_part: expected [0.02, 0.25], observed 0.87"], "verdict_null": 112}]
spec["changelog"].append({"version": 2, "date": "2026-09-25",
    "change": "check the headline of each value instead of the whole value; numbers_match returns 'none' instead of None; traps rewritten around the headline",
    "evidence": "run v1 over 315 facts: 87% states_part because values are composite (e.g. 'MIT — an MIT product may link it' against the quote 'MIT'), and 112 null verdicts from a None axis. The world differed from the map's expectation, and the question was the wrong unit; Jev's answers were right."})
(HERE / "claim-check.jds.json").write_text(json.dumps(spec, indent=2, ensure_ascii=False))
print("claim-check is version", spec["version"])
