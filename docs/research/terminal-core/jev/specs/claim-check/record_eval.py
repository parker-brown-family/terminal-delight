"""Record the v3 run and its hand-labelled evaluation in the spec, and move status to shadow.

No version bump: nothing about the questions changes. What changes is what the spec knows
about itself — measured precision per answer on a stratified sample, and the known failure.
"""
import json
from pathlib import Path

HERE = Path(__file__).parent
spec = json.loads((HERE / "claim-check.jds.json").read_text())
score = json.loads((HERE.parent.parent / "labels" / "claim-check-v3.score.json").read_text())
t = score["confusion_label_by_jev"]
opts = ["states", "states_part", "implies", "silent", "contradicts"]
precision = {j: {"hits": t[j][j], "n": sum(t[l][j] for l in opts)} for j in opts}
strata = {"states": 37, "states_part": 103, "implies": 38, "silent": 120, "contradicts": 17}
est = sum(strata[j] * precision[j]["hits"] / precision[j]["n"] for j in opts) / sum(strata.values())

spec["status"] = "shadow"
spec["evaluation"]["labels"] = {
    "rubric": "the spec's own v3 criteria, applied to the headline and quote only",
    "n": score["n"], "owner": "the assembling agent (not Parker)", "file": "labels/claim-check-v3.labels.json",
    "design": "stratified: eight items per Jev answer, seed 11, labelled before reading Jev's answers",
    "exact_agreement": f"{score['exact']}/{score['n']}",
    "precision_by_jev_answer": precision,
    "population_estimate": round(est, 3),
    "population_estimate_method": "each answer's sample precision weighted by how often Jev gave that answer in the v3 run (states 37, states_part 103, implies 38, silent 120, contradicts 17); eight items per stratum, so the interval is wide",
}
spec["evaluation"]["runs"].append({"name": "v3", "version": 3, "items": 315, "surprises": [
    "share choosing implies: expected [0.15, 0.5], observed 0.121",
    "share choosing silent: expected [0.03, 0.2], observed 0.381"]})
spec["meaning"]["check.support"]["known_failures"].append(
    "measured on v3: 'contradicts' was wrong in 8 of 8 sampled cases, each a headline stated as an absence or a qualification "
    "('Replies are events, not writes' against 'Write some text to the PTY'); jev-1.13's literal reading of negations. "
    "It routes to a person, which is where it belongs. The assembling agent read all 17 by hand: none was a contradiction in "
    "its source. Two (Contour's and alacritty's performance figures) were reconciled by the elaboration that the headline "
    "check leaves out: decomposing the value removed the context that explained the figure.")
spec["meaning"]["check.support"]["calibration"] = "measured: n=40 stratified labels by the assembling agent, 2026-09-25; 'states' 8/8, 'contradicts' 0/8"
spec["changelog"].append({"version": 3, "date": "2026-09-25", "change": "status draft → shadow; labels and measured precision recorded; no question changed",
                          "evidence": "40 stratified hand labels: exact agreement 24/40; 'states' precision 8/8, 'contradicts' 0/8"})
(HERE / "claim-check.jds.json").write_text(json.dumps(spec, indent=2, ensure_ascii=False))
print("status", spec["status"], "population estimate", round(est, 3), precision)
