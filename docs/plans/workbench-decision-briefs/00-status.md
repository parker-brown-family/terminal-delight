# Decision briefs travel with Terminal Delight, and the Workbench asks for them

**Difficulty: 5/10.** Every mechanism the loop needs already exists: the bench
opens an HTML artifact in the floating square, the square takes notes on any
element of a brief, and ↪ pastes the notes map into the agent's prompt. What is
new is a paragraph in the briefing a Workbench-launched agent reads, the skill
itself riding inside the binary, the spec saying so, and a public page with a
film of it. The briefing is the risky part: it steers every agent the launcher
starts, so a wrong sentence there is wrong in every pane at once. That keeps it
out of the threes. Nothing here touches the grid, the host or the wire, which
keeps it out of the sevens. One combined plan page, this one.

**Approval:** Parker, 2026-09-25, in the request itself: *"SEND IT --- AFK for
full commit merge and docuemntaion deployment etc..."*. No gate was held for a
second approval; the decisions below are the ones he would otherwise have been
asked, each with the call made.

**Turned out to be:** _filled in at the end_

## What he asked for

- Ship the decision-brief skill to Terminal Delight **as it stands** — the
  Omarchy-wallpaper glass look included.
- Urge it to be **extra brief, with extra visuals**.
- Tie it to the **Workbench**: update the briefing (the "Workbench JSON
  request") so agents in the Workbench answer decision-shaped work with a brief.
- **Only the Workbench.** *"We don't want to get in the way of agents spinning
  up in the terminal the way that people usually do it."*
- A docs-site page with an **mp4** of a brief opening inside the window, notes
  being made, and the notes landing in an agent.
- Commit, merge, install, deploy.

## Decisions made without him

| Question | Call | Why |
|---|---|---|
| Where does the skill live? | `app/skills/decision-brief/`, embedded in the binary, written to `$XDG_DATA_HOME/terminal-delight/skills/decision-brief/` when the launcher briefs an agent | A path the briefing names has to exist on every machine TD runs on, including an AppImage with no checkout beside it. Written on launch, so the kit on disk is always the one this build was tested with. |
| A copy of a managed source? | The assets, references and `brief-wall` are vendored **byte for byte** with a `SOURCE` file, the same pattern as `app/tests/fixtures/decision-brief/notes-format/`. Only `SKILL.md` is TD's own. | The house rule is *no copy beside a managed source*. A vendored copy with a named commit and a sync script is how this repo already carries the notes fixtures, and TD is a product that ships to people without `~/.claude`. |
| Which upstream state? | `agent-skills` at `3cc6baf` **plus its uncommitted working tree** as of 2026-09-25 11:26 (the glass look, `brief-wall`, wet-ink concur) | "As it stands" is the working tree: the glass look he asked for is not committed upstream. `SOURCE` says so and lists every file's sha256, so the drift is checkable. Notes format is unchanged (still format 1); the notes.js change is the stamp's ink. |
| Who is told? | Only agents the Workbench launcher starts, through `launch_briefing` | That is the only text TD hands an agent. A plain `claude` in a pane reads nothing from us, which is what he asked for. |
| Wire change? | None. No TDSP bump. | A brief is an `artifact` with `mime: text/html`; the bench already opens drawable artifacts in the square. The spec's artifact section still said "opened with the desktop's own handler", which stopped being true on 2026-09-25 — fixed. |
| What does "extra brief" mean in numbers? | A five-minute page. Headline ≤ 40 words, read-first ≤ 3, a figure for every argument, no section longer than one screen of the square, depth in modals. | The upstream skill budgets twenty minutes. The square is narrower than a browser tab and sits over the agent's own pane; a brief read there has to be read in the time a person spends deciding whether to type. |

## Slices

- [ ] 1 · The kit: `app/skills/decision-brief/` (vendored + TD `SKILL.md`), `briefkit.rs` embeds and writes it, `scripts/sync-brief-skill` re-vendors it
- [ ] 2 · The briefing: `launch_briefing` names the kit and asks for briefs on decision-shaped work, only when the kit was written; tests read the returned string
- [ ] 3 · The spec: TDSP §5 artifact, the brief loop; implementation map
- [ ] 4 · The docs site: a Decision briefs page, the workbench and surface-protocol pages linked to it
- [ ] 5 · The film: a brief opening from the bench, notes made, ↪ into the agent, as mp4 on that page
- [ ] 6 · Land: PR, CI, merge, install, deploy the docs site
