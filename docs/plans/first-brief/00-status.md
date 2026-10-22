# Status: First Brief — a newcomer's first time through the brief loop

**Difficulty: 4/10.** The loop exists end to end; this teaches it to an agent that was never told and to a person who has never seen a brief. Nothing here is hard to unwind. That buys **one combined plan page and one approval.** Honest note made while planning: the agent half grew once it turned out the hook installer is not in the AppImage, so the binary has to install the hook itself — it may be a 5.
**Turned out to be:** _filled in at the end_

- Plan (product, architecture, slices in one): **APPROVED 2026-10-21**. Parker: *"Great - go end to end with this including the video demo - I am afk - done for the day! <3"*, sent with his one note on the page. No decision was changed, so all five recommendations stand. `01-plan.md`; page `reports/2026-10-21-first-brief.html`, committed as he left it, with his note in its islands.
- **The name is Brief and Back Again**, not the recommended "Brief and Back". His note on that card: *"Brief and back again -- an agent's tale... LIke There and Back again a Hobbits tale"*. The video is titled *Brief and Back Again* with *an agent's tale* under it.

## Found after approval, before building
- **Codex launches are already briefed.** PR #940 (merged 2026-10-15) gives every LAUNCH AGENT harness the shared DECISION BRIEFS packet, Codex and a configured local runner included. The docs page on `main` still says "Codex takes no briefing", so slice 2 corrects it, and its Codex half shrinks to a *typed* `codex`.
- **The public download predates the whole loop.** The latest release is v0.3.0 from 2026-09-18; the reader and the kit landed after it (issue #941). The v0.4.0 release candidates (rc.3 published 2026-10-21) carry it. That track is someone else's and is left alone; the video says which version it shows.
- **The notes bar's words come from the language pack now** (`notes_ui.rs` on `main` moved ~480 lines since the plan was read), so slice 1's line is a language-pack string, not a literal.

## Slices
- [ ] Slice 1: the first-time line in the notes bar (`app/src/docview/notes_ui.rs`, `draw_bar`) — the person's half, and the tracer
- [ ] Slice 2: `terminal-delight briefing` prints the launcher's DECISION BRIEFS paragraph; the binary installs a SessionStart hook that runs it, after one yes on the bench — Claude Code first, Codex as its own pull request
- [ ] Slice 3: one row in `parker-brown-family/agent-skills`' README pointing Terminal Delight users at the Workbench edition
- [ ] Slice 4: the learn-how-to video — Parker records; narration from Writers Block via a learning brief

## Notes for a fresh session
- Parker, 2026-10-21, prompted by an Omarchy user's public post that "Flip an agent's terminal into a bench" had their "UX mental model melting": *"how do we handoff the process for a person to use our html decision brief better with terminal … non-power users."*
- Then: *"It points to agents needing to know how to operate AND humans knowing how to provide feedback on the SYSTEM."* — the plan's two halves.
- Then: *"we are going to make a demo video of this --- A LEARN HOW TO (WHAT DO WE CALL THIS PROCESS?)"* — decision 1 names it; recommendation **Brief and Back**.
- The loop needs no MCP server: the card is a surface file and ↪ is a paste the person presses. `document_notes` and `open_document` stay extras.
- Prior art this builds on, all shipped: `docs/plans/brief-beside-agent/` (↪, `document_notes`, `open_document`), the Workbench edition kit (`app/skills/decision-brief/`, `app/src/briefkit.rs`), the launcher briefing (`app/src/launcher.rs`).
