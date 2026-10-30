# Threshing Floor cards — status

**Difficulty: 6/10** — one combined plan page and one approval. New UI on a live surface plus a new rule for who touched an artifact; wrong, it would show a person's mark on something only an agent opened, and nobody would notice for a while.

## Where this came from

Parker, 2026-10-27, after the review of the Builder Crews brief:

- *"WHO IS ON THE CREW + model etc. is REALLY good info! lower power models are commonly used for grunt work with higher pwoer for planning and coordinating and then lower again higher up the chain for conversing and reporting to human"*
- *"yes artifacts ON the agent cards"*
- *"NO ARTIFACT YET - FLAG YES! in fact COLOUR CODE AS WELL - this essentially means no work done --- the CARD will colour code against the them (not alert, but accent)"*
- *"we tag EACH card with a human bust glyph (the same way that sticky notes are stamped with a robot glyph) for the ARTIFACTS which have comments / concur / approved etc. … a human SIMPLY INTERACTING with an artifact will flag this … DOES NOT REQUIRE human APPROVAL - only interaction (have they seen it)!"*
- Not now: marking work stale, starting agents, next-task assignment (*"the persons agent setup … through the TUI surface"*), refusing agents, voting.

## Gates

- Plan (`plan.html`): **APPROVED 2026-10-27 with amendments** — concurs on decisions 1, 2 and 4; decision 3 (accent on an empty card) stands as Parker asked for it in the message that started this. His notes:
  - The model line is `{HARNESS} · {MODEL} {VERSION} {EFFORT}`: *"CODEX - ASTRA 6.1 XHIGH  CLAUDE - OPUS 5.5 MAX CLAUDE - HAIKU 5.1 MEDIUM"*.
  - *"age and token count are A+ !"*
  - *"Current message is already being tracked on WORKBENCH - so the code for picking this up already exists!"*
  - **No glyph.** *"it would be unclear to a new user if the human bust means... NEEDS human attention --- or HAS ALREADY HAD human attention"* → a word in a soft pill, in two colours: *"NO FEEDBACK -> HUMAN ANNOTATED"*.
  - **Only a saved annotation counts.** *"Needs a save in file … a READ or OPEN of the file is not enough to trigger the human provenance because there was no INTERACTION."* and *"the 'opened' is fine as a subtle point, but the PERTINENT information is human provenance (interaction)."*
- Build: on `feat/threshing-floor-cards`, stacked on `refactor/threshing-floor`.

Builds on `refactor/threshing-floor` (the War Room → Threshing Floor rename).

## Built (2026-10-27, `feat/threshing-floor-cards`)

- The card reads: place › tab, header, the latest workbench message (brief, or the reply's first line), `HARNESS · MODEL VERSION EFFORT` (or `model unknown`), `● state · role` with any access badge, age and tokens, and the newest artifact with `+N` and a soft pill: NO FEEDBACK (neutral) or HUMAN ANNOTATED (the theme's `human` colour). No pill where the artifact cannot take notes (a picture, a PDF, a web page, a page without a notes island).
- No artifact: "no artifact yet" in the accent, and the card's border in the accent.
- Feedback is decided in `app/src/feedback.rs` from saved notes and concurs only; opening is never feedback.
- Not verified: the card's layout in a real window. The test renderer's fonts are far smaller than the real ones (all seven lines end 91 points into the card at either height), so a bounds check could not fail and was removed. Look at it on install.

**Post-hoc difficulty: 6/10 held.** One approval, which carried four substantive amendments, so the plan page earned its place.
