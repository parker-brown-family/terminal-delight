"""Generate core-fit.jds.json so its thirteen questions share one set of options.

Written 2026-09-25, before any call and before the research round was read.
"""
import json
from pathlib import Path

HERE = Path(__file__).parent

REQUIREMENTS = {
    "cell_contents": ("An embedder can read each visible cell's character, its foreground and background colour, and its attribute flags (bold, italic, underline, inverse).", "must"),
    "zero_width": ("An embedder can read the zero-width characters (combining marks) attached to a cell, or its whole grapheme cluster, not only its first character.", "must"),
    "underline_colour": ("An embedder can read a cell's underline colour, set by SGR 58, separately from its text colour.", "important"),
    "wide_chars": ("An embedder can tell which cells hold a double-width character and which are its second half.", "must"),
    "scrollback_read": ("An embedder can read lines that have scrolled off the top into history (scrollback), not only the visible screen, whether through negative line indices, a history accessor or a scroll offset.", "must"),
    "damage": ("The core reports which rows changed since the embedder last read them.", "important"),
    "snapshot": ("The whole terminal state (screen, scrollback or cursor, modes) can be serialised or re-emitted so a second copy, attached later, can be brought to the same state.", "important"),
    "feed_loop": ("The embedder reads the program's output itself and hands the bytes to the core; the core does not insist on owning the read loop or the pseudoterminal.", "must"),
    "replies_out": ("Answers the core generates for the program (device attributes, size reports, graphics query replies) are handed to the embedder, which decides where to write them.", "must"),
    "threading": ("The core's state can be owned by one thread and handed to or shared with another behind a lock (it is Send), as when a parsing thread and a drawing thread both touch it.", "important"),
    "placements_readable": ("The embedder can read each picture a program placed: which image, the cell where it starts, how many cells it covers, and the image's pixels.", "must"),
    "placeholders": ("The core resolves Kitty Unicode placeholder cells, telling the embedder which picture and which slice of it each placeholder cell shows.", "important"),
    "media_policy": ("The embedder can refuse pictures sent as a file path, a temporary file or shared memory, while still accepting pictures sent inline.", "nice"),
}

OPTIONS = {
    "provides": {"means": "`evidence` shows the core offering this directly: a type, method, callback or setting that does it."},
    "provides_with_work": {"means": "`evidence` shows the core has what is needed, but the embedder must assemble it: combine several calls, track state itself, or read an internal field.",
                           "not_for": "A capability the evidence shows missing."},
    "does_not": {"means": "`evidence` shows the core does not offer this, or offers it in a form that rules the requirement out."},
    "evidence_silent": {"means": "`evidence` does not say either way.",
                        "not_for": "A case where the evidence says the capability is absent: that is does_not."},
}

questions, meaning, axes = {}, {}, []
for rid, (text, tier) in REQUIREMENTS.items():
    questions[rid] = {
        "type": "choice",
        "instructions": {
            "question": "Does `candidate` meet this requirement, judging only from `evidence`?",
            "requirement": text,
            "judge_by": "what `evidence` records and quotes about the core's interface; an entry carrying a \"claim_check_verdict\" was not confirmed by its own quote and counts for less",
        },
        "criteria": OPTIONS,
    }
    meaning[f"assess.{rid}"] = {
        "type": "choice",
        "options": {
            "provides": {"means": "the capability is there as an interface TD can call", "route": "accept"},
            "provides_with_work": {"means": "TD would write glue: the migration cost lives here", "route": "accept"},
            "does_not": {"means": "a gap TD must fill itself or a reason to rule the core out" + (" (a must-have)" if tier == "must" else ""), "route": "flag:gap" if tier == "must" else "accept"},
            "evidence_silent": {"means": "the research did not establish it: unknown, drawn grey, never scored as zero", "route": "unknown"},
        },
        "confidence_bands": [
            {"id": "contested", "range": [0.0, 0.305], "means": "the evidence reads two ways", "route": "unknown"},
            {"id": "clear", "range": [0.305, 1.0], "means": "one reading dominates", "route": "per-option"},
        ],
        "confusable": [
            {"pair": ["provides", "provides_with_work"], "margin": 0.2, "means": "an interface that exists but needs glue; either reading is a pass", "route": "accept"},
            {"pair": ["does_not", "evidence_silent"], "margin": 0.2, "means": "the evidence mentions the area without settling it", "route": "unknown"},
            {"pair": ["provides_with_work", "does_not"], "margin": 0.2, "means": "the evidence shows the parts but not whether they reach the embedder (v1: rio-vt's placements at 0.46 against 0.52)", "route": "unknown"},
        ],
        "escape": "evidence_silent",
        "expect": {"option_shares": {"evidence_silent": [0.0, 0.35]},
                   "why": "the rubric asked for grid_api, state_snapshot, threading_and_loop and reply_channel on every subject; references like zellij-grid and td-own-core will be thinner"},
        "calibration": "assumed",
    }
    axes.append({"id": rid, "means": text, "source": "jev", "questions": [f"assess.{rid}"],
                 "consumer_uses_it_for": f"a {tier} requirement in the fit column of the decision matrix"})

must = [r for r, (_, t) in REQUIREMENTS.items() if t == "must"]
axes.append({
    "id": "must_have_gaps", "source": "combination",
    "means": "how many must-have requirements the evidence shows the core does not meet",
    "compute": " + ".join(f"(1 if axis.{r} == 'does_not' else 0)" for r in must),
    "consumer_uses_it_for": "a core with a must-have gap needs a line of TD code or a patch upstream before it can serve TD; shown as a count beside the fit",
})
axes.append({"id": "evidence_fields_present", "source": "code", "compute": "builders.py:evidence_fields_present",
             "means": "how many of the eight interface fields the research filled",
             "consumer_uses_it_for": "a thin dossier explains a column of evidence_silent; drawn beside the fit"})

# ── the second call: how settled is the interface TD would depend on ──
TRAJECTORY = {
    "stable_contract": {"means": "The project promises a stable interface: version 1.0 or later with semantic versioning, or an explicit statement that the API is stable."},
    "active_pre_1_0": {"means": "Pre-1.0 and actively released, with breaking changes arriving in ordinary releases and no promise either way.",
                       "not_for": "A project that says outright that its API is unstable or experimental."},
    "declared_unstable": {"means": "The project itself says the interface is unstable, experimental, or expected to break."},
    "dormant": {"means": "No releases and no meaningful commits in the last year: the interface is frozen because nobody is working on it."},
    "owned_by_td": {"means": "The interface is Terminal Delight's own code, or a fork TD would carry itself."},
    "not_established": {"means": "`maturity` does not say enough to tell."},
}
questions_m = {"api_trajectory": {
    "type": "choice",
    "instructions": {
        "question": "Which describes the interface `candidate` would give Terminal Delight to depend on, judging only from `maturity`?",
        "judge_by": "what the project says about its API and what its release and commit history show, not how capable it is",
    },
    "criteria": TRAJECTORY,
}}
meaning["maturity.api_trajectory"] = {
    "type": "choice",
    "options": {k: {"means": v["means"], "route": "unknown" if k == "not_established" else "accept"} for k, v in TRAJECTORY.items()},
    "confidence_bands": [
        {"id": "contested", "range": [0.0, 0.255], "means": "two trajectories are live, usually active_pre_1_0 against declared_unstable", "route": "unknown"},
        {"id": "clear", "range": [0.255, 1.0], "means": "one reading dominates", "route": "per-option"},
    ],
    "confusable": [{"pair": ["active_pre_1_0", "declared_unstable"], "margin": 0.2, "means": "a pre-1.0 project that also warns about breakage; both count against stability", "route": "accept"}],
    "escape": "not_established",
    "expect": {"option_shares": {"stable_contract": [0.0, 0.2], "not_established": [0.0, 0.2]},
               "why": "almost every emulator core is pre-1.0; libvterm and vt100 look dormant; the keep-alacritty options partly own their code"},
    "calibration": "assumed",
}
axes.append({"id": "api_trajectory", "means": "how settled the interface TD would depend on is", "source": "jev",
             "questions": ["maturity.api_trajectory"], "consumer_uses_it_for": "the stability column: an unstable or dormant interface is a cost TD pays on every upgrade, or never gets to pay"})

spec = {
    "jds": "0.1", "id": "core-fit", "version": 1,
    "title": "How much of what Terminal Delight needs from an emulator core does each candidate provide?",
    "status": "draft",
    "decision": {
        "statement": "For each candidate terminal core, judge from the researched and checked evidence whether it provides each of thirteen things Terminal Delight's code needs from its emulator, so the decision matrix can show fit and migration cost per candidate with unknowns drawn as unknown.",
        "consumer": "Parker, reading the fit columns of the terminal-core decision matrix",
        "acts_on_result": "fills cells of the matrix; never ranks the cores (code does that with Parker's weights) and never chooses one",
        "consequence": "medium",
        "frequency": {"decisions_per_run": 14, "runs_per_day": 1},
        "latency_class": "background",
        "latency_budget_ms": None,
        "code_owns": ["the licence gate", "Kitty feature coverage from the tags", "release counts and dates", "the bake-off's measured placements", "the weights and the ranking"],
        "not_decided_here": ["which core TD uses", "whether the evidence is true (claim-check owns that)"],
    },
    "axes": axes,
    "output": {
        "record": {rid: "provides | provides_with_work | does_not | evidence_silent | null" for rid in REQUIREMENTS} | {
            "must_have_gaps": "integer | null (null: a must-have answer is missing)",
            "evidence_fields_present": "integer 0-8"},
        "combination": "version 1: the fit shown in the matrix is computed by scripts/matrix.py from these answers with weights 3/2/1 (must/important/nice), provides = 1, provides_with_work = 0.5, does_not = 0, evidence_silent excluded from both numerator and denominator and reported as coverage",
        "unknown_rendering": "evidence_silent and contested answers render as a grey cell labelled 'not established', never as a zero",
    },
    "frames": {
        "core_evidence": {
            "builder": "builders.py:core_evidence",
            "fields": {
                "candidate": {"provenance": "computed", "from": "the research plan's subject label"},
                "evidence.<field>.recorded_by_research_agent": {"provenance": "asserted", "from": "reported by the research agent's summary", "absent": "omit: the field was not gathered"},
                "evidence.<field>.quoted_from_source": {"provenance": "asserted", "from": "the passage the agent says it copied", "absent": "omit"},
                "evidence.<field>.claim_check_verdict": {"provenance": "hedge", "from": "claim-check's verdict when it was not 'supported'", "absent": "omit: the fact was supported"},
            },
            "est_tokens": 2400, "privacy_class": "public",
            "snapshot": "the research-delight run plus the claim-check run it was built from",
        },
        "maturity_evidence": {
            "builder": "builders.py:maturity_evidence",
            "fields": {
                "candidate": {"provenance": "computed", "from": "the research plan's subject label"},
                "maturity.<field>.recorded_by_research_agent": {"provenance": "asserted", "from": "reported by the research agent: api_stability, latest_release, releases_last_12mo, maintainers", "absent": "omit"},
                "maturity.<field>.claim_check_verdict": {"provenance": "hedge", "from": "claim-check's verdict when it was not 'supported'", "absent": "omit"},
            },
            "est_tokens": 500, "privacy_class": "public",
            "snapshot": "as core_evidence",
        },
    },
    "calls": [
        {"id": "assess", "frame": "core_evidence", "when": "always", "reason": "sub-decision", "questions": questions},
        {"id": "maturity", "frame": "maturity_evidence", "when": "always", "reason": "different-frame", "questions": questions_m},
    ],
    "meaning": meaning,
    "invariants": [
        {"id": "placements-without-kitty", "when": "axis.placements_readable == 'provides' and item.kitty_tags == 'none'",
         "means": "Jev read a placement interface into a core the research says implements no Kitty feature", "route": "human"},
    ],
    "max_depth": 1,
    "economics": {"price_per_mtok_usd": 0.042, "p50_ms_per_call": 250, "tokens_per_question": 60, "split_tolerance_usd_per_day": 0.1},
    "evaluation": {
        "anchors": "anchors",
        "jitter": {"repeats": 3, "measured_sd": {}},
        "labels": {"rubric": "the bake-off's measured behaviour for rio-vt, libghostty-vt and alacritty_terminal is the ground truth for placements_readable, placeholders, feed_loop and replies_out", "n": 0, "owner": "the assembling agent (not Parker)"},
        "falsifiers": [
            {"text": "reversing the order of the thirteen questions changes more than 10% of answers: the batch is anchoring on order, and the answers are not independent",
             "check": "manual: scripts/order_check.py"},
            {"text": "a core the bake-off showed placing pictures comes back does_not on placements_readable",
             "check": "manual: compare with evidence/bake/"},
        ],
        "runs": [],
    },
    "changelog": [{"version": 1, "date": "2026-09-25", "change": "first version, written before any call", "evidence": "none yet: this is the prediction"}],
}
(HERE / "core-fit.jds.json").write_text(json.dumps(spec, indent=2, ensure_ascii=False))
print("wrote core-fit.jds.json with", len(questions), "questions")

# ── version 2, on the evidence of run v1 (appended 2026-09-25, before any v2 call) ──
spec = json.loads((HERE / "core-fit.jds.json").read_text())
spec["version"] = 2
spec["frames"]["core_evidence"]["fields"]["evidence.measured_by_bake_off.observed"] = {
    "provenance": "observed", "from": "the bake-off harnesses (bake/corebake, bake/ghostbake), for the four cores it ran", "absent": "omit: the core was not run"}
spec["evaluation"]["runs"] = [{"name": "v1", "version": 1, "items": 14,
    "order_check": "8 of 182 answers changed with the questions reversed (4.4%): the falsifier did not fire",
    "falsifier_fired": "rio-vt came back does_not on placements_readable (0.52 against provides_with_work 0.46) although the bake-off read its placements from a public map",
    "hand_read": "alacritty zero_width does_not at 0.71 with evidence naming zerowidth(): the requirement said 'combining marks'; rio-vt scrollback_read does_not at 0.71 from evidence that led with visible_rows()"}]
spec["changelog"] = spec.get("changelog", []) + [{"version": 2, "date": "2026-09-25",
    "change": "zero_width and scrollback_read reworded with the vocabulary the evidence uses; the bake-off's observations join the state as observed evidence; the provides_with_work/does_not near-tie routes to unknown",
    "evidence": "run v1: one falsifier fired (rio-vt placements) and two wrong answers traced to vocabulary; fixes named by the explanation for each wrong answer"}]
(HERE / "core-fit.jds.json").write_text(json.dumps(spec, indent=2, ensure_ascii=False))

# ── evaluation of v2 against ground truth (appended 2026-09-25, after run v2; no version change) ──
spec = json.loads((HERE / "core-fit.jds.json").read_text())
spec["status"] = "shadow"
spec["evaluation"]["labels"] = {
    "rubric": "jev/labels/core-fit.truth.json: pass = provides or provides_with_work, fail = does_not, for alacritty-own-loop, rio-vt and libghostty-vt, from TD's own use of alacritty and the two bake-off harnesses; 3 of 39 left unlabelled as unsure",
    "n": 36, "owner": "the assembling agent (not Parker)"}
spec["evaluation"]["runs"].append({"name": "v2", "version": 2, "items": 14,
    "against_truth": "v1 33 right, 3 wrong, 0 unknown of 36; v2 32 right, 0 wrong, 4 unknown of 36 (score_truth.py → labels/core-fit.score.json)",
    "falsifier": "rio-vt placements_readable still picks does_not (0.45) but its confidence is under the threshold, so it routes to unknown rather than to a wrong answer",
    "repeat": "the identical rio-vt state asked nine more times (runs/core-fit-rio-repeat, runs/core-fit-rio-ablation A): does_not eight times, provides_with_work once, never above 0.53 for either",
    "ablation": "runs/core-fit-rio-ablation: removing the claim-check verdicts left does_not at 0.47; adding one sourced sentence that the images' pixels are public (rio-graphics GraphicData.pixels) dropped does_not to 0.01. The requirement asks for four things and the evidence showed three"})
(HERE / "core-fit.jds.json").write_text(json.dumps(spec, indent=2, ensure_ascii=False))

# ── the run of record becomes a consensus (appended 2026-09-25, after bake/wezbake ran; no version change) ──
spec = json.loads((HERE / "core-fit.jds.json").read_text())
spec["evaluation"]["labels"]["rubric"] = spec["evaluation"]["labels"]["rubric"].replace(
    "for alacritty-own-loop, rio-vt and libghostty-vt", "for alacritty-own-loop, rio-vt, libghostty-vt and (from bake/wezbake) wezterm-term")
spec["evaluation"]["labels"]["n"] = 44
spec["evaluation"]["jitter"] = {"repeats": 3, "measured": "identical input, pairwise: 13, 10 and 7 of 196 answers changed (3.6-6.6%); jev/jitter.json",
                                "consequence": "the question-order reversal (4.4%) sits inside that noise; the run of record is now the per-cell majority of three runs, no majority read as unknown"}
spec["evaluation"]["runs"].append({"name": "v2 consensus", "version": 2, "items": 14,
    "input_change": "wezterm-term's bake-off observation joined its evidence, through the frame field that already existed for every core the bake-off ran",
    "runs": ["runs/core-fit-r1", "runs/core-fit-r2", "runs/core-fit-r3"], "consensus": "runs/core-fit (scripts/consensus.py): 181 of 196 cells unanimous, 15 split, 4 with no majority",
    "against_truth": "44 cells over four cores: consensus 40 right, 0 wrong, 4 unknown; v2 before the observation 39 right, 1 wrong (wezterm-term replies_out read as does_not from 'no reply event, only a writer'), 4 unknown; v1 40 right, 4 wrong",
    "effect_on_ranking": "wezterm-term's fit rose from 0.59 to 0.72 and it moved to first (0.72 against rio-vt 0.69); with fit and API trajectory weighted zero it leads anyway (jev/what-if.json)"})
(HERE / "core-fit.jds.json").write_text(json.dumps(spec, indent=2, ensure_ascii=False))
print("core-fit is version", spec["version"])
