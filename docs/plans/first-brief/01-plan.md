# Plan: First Brief

_One combined plan (difficulty 4/10): product, architecture and slices. The page for Parker is `reports/2026-10-21-first-brief.html`._

## Product

A newcomer installs Terminal Delight, types `claude` as `first-launch.html` tells them to, and asks for a review. Two things stop them completing the brief loop that already runs for Parker:

1. **The agent was never told.** Only an agent started from ⌁ LAUNCH AGENT gets the DECISION BRIEFS paragraph (through `--append-system-prompt`). A typed `claude` gets nothing, and Codex gets nothing from any door (`docsite/pages/briefs.html` says both).
2. **The person is never told.** A brief open in the square, beside an agent, with no notes, shows `0 notes · 0 concurs ⎘ copy map ↪ send to agent 💾 save into file`, every button disabled, and no word on what to do first.

The fix teaches each side once, where it stands: the agent from the binary, the person from the notes bar. Then the learn-how-to video is filmed, so its first step is "type claude and ask", not "find the panel".

Out of scope: the browser path for people without Terminal Delight (the house edition's "save into file" loop), rewriting the house edition of the skill, and any change to the notes format.

## Architecture

**Slice 1, the line (`notes_ui.rs`).** `draw_bar` gains one row, above the buttons, while the brief takes notes (`read_only()` is `None`), has zero notes, zero concurs, zero pictures and nothing pending. Draft wording: `click anything to note it · hold Alt to see what takes one · ✓ stamps a decision · ↪ puts your notes in the agent's prompt`. The ✓ clause is dropped where `concurs()` is `None` (a Markdown document, or a brief whose notes.js predates concurs); the ↪ clause is dropped where `send_button` returns `None` (nobody beside it). Pure presentation; no write, no manifest row.

**Slice 2, the briefing.** The DECISION BRIEFS paragraph moves behind one function the launcher already calls; `terminal-delight briefing` prints that function's output and exits 0, and prints nothing when `TD_SESSION` is unset or when the launcher set a marker saying this pane was already briefed. The binary installs a `SessionStart` hook that runs it, merging into `~/.claude/settings.json` the way `scripts/install-agent-hooks.sh` does (append, never clobber, timestamped backup), because the AppImage carries no source tree (`install.html`). It asks first: the first time the window notices an agent it did not launch, that pane's bench carries a one-time card with a yes. The source-tree script calls the same subcommand rather than keeping its own copy of the paragraph. Codex follows as its own pull request through `~/.codex/hooks.json`, once it is confirmed Codex shows a hook's output to the model.

**Slice 3, the pointer.** One row in `agent-skills/README.md` under decision-brief: Terminal Delight users get the Workbench edition, which the binary already writes to `~/.local/share/terminal-delight/skills/decision-brief/`.

**Slice 4, the video.** *Brief and Back Again*, subtitled *an agent's tale* (Parker's name, from his note on the page). Seven cards, two to five words each (house rule for video cards): Ask for a brief · Open the card · Hold Alt · Note a figure · Stamp a decision · Send it back · The agent answers. Approved as "go end to end … including the video demo", so it is filmed on the film kit's hidden-window rig (`~/Work/td-film-kit`, private, never in this repository) with the mock agent that made the `/briefs` film, and carries no narration: narration is public writing, written in Writers Block from a learning brief.

## Tests (each fails before its slice)

- The line: drawn on a writable brief with nothing on it; gone after one note, one concur, or one pasted picture; never on a read-only page; no ✓ clause where concurs are `None`; no ↪ clause with nobody beside.
- The briefing: `terminal-delight briefing` prints bytes identical to what the launcher appends; prints nothing without `TD_SESSION`; prints nothing with the launcher's marker; the hook install is idempotent, keeps existing hooks and a backup, and uninstalls cleanly; nothing is written to settings before the yes.
- The pointer: the README row links a path that exists in the public repo.

## Least confident

1. **Double briefing.** A LAUNCH AGENT pane gets the paragraph from `--append-system-prompt`; the hook must stay silent there. A marker env var set by the launcher is the plan; a pane restored by `claude --resume` needs checking too.
2. **Codex.** That a SessionStart hook's output reaches Codex's model is inferred from the APES boot working there. Check before building the Codex half.
3. **The wording of the line.** It is the first thing a newcomer reads in the square. The page's figure 04 takes notes on it.
4. **Asking before writing settings.** A one-time card on the bench is the plan. The alternative, installing silently on first launch, edits a file the person never agreed to.
