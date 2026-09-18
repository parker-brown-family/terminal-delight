# Status: the response feed, and the launcher's two bugs

**Difficulty: 6/10:** one new protocol kind with a renderer, plus a shelf-semantics change
every existing bench test leans on; wrong for a week would mean agents writing a reply
shape nobody reads, and the unwind is a kind removal → one combined plan page, this file.
**Turned out to be:** 6 was right for the shape and high for the ceremony. Parker's brief
was the plan ("well defined but very flexible", "collapsable elements", "the overview will
only show the responses"), the decisions below are the ones it left open, and all of it
landed in one sitting with the suite green.

## The brief, 2026-09-17

Three things in one message with two screenshots:

1. *"effort does not correlate with ACTUAL claude efforts"* — the launcher's
   `quick / standard / hard / ultra` row.
2. *"I don't see terminal delight"* — `ter` typed into the launcher's filter, an empty
   PROJECT area.
3. The Overview should be a feed of agent replies in a well-defined but flexible JSON:
   tl;dr, ELI5, technical brief, layman brief, articles of doubt, other ideas; collapsible;
   no artifacts or decisions on that tab. *"Let's make it award winning!"*

## What was found

- **Effort.** Claude Code 2.1.270 takes `--effort <low|medium|high|xhigh|max>`; the module
  comment said it had no such flag and appended a "think hard" sentence instead. Codex takes
  `model_reasoning_effort` with `low`, `medium`, `high`, `xhigh` (read off the binary).
- **The empty list.** Not the filter and not the scan: the panel's height was
  `250 + 30 per matched row`, the 250 dating from one chip row. With four chip rows, a
  preview and a hint the chrome needs ~390, so three matches gave a 340-pixel panel whose
  only shrinkable child — the list — got zero. Confirmed by arithmetic against the
  screenshot (panel ≈ 340 real pixels). A second trap sat behind it: the scan kept the 60
  newest of 257 directories, so a project with no top-level change in a week fell off.

## Decisions taken without asking (amber — argue with any of them)

- **The registers are keys under `model`**, not a nested `response` object — but the nested
  shape Parker drew is accepted as an alias, so both land.
- **Known keys and their labels:** `tldr` (required), `eli5` → ELI5, `layman` → Plain brief,
  `technical` → Technical brief, `evidence` → What was verified, `asks` → Needs from you,
  `next` → What's next, `doubts` → its own strip. Aliases for each. Unknown keys are kept as
  sections labelled by their key, sorted after the known ones.
- **Default folds:** gist and doubts always open; `asks` open; everything else folded. The
  policy lives in `workbench::section_default_open`, not in the renderer.
- **The overview shows the newest response by itself** when nothing is opened
  (`Bench::showing`). An opened card is kept under new arrivals. This is a shelf property,
  not an arrival selecting itself.
- **Where the other kinds went:** changeset → decisions (a change a person answers),
  unclassified → artifacts (a thing that arrived, drawn with its bytes).
- **The chip word is the flag word.** No prose about effort in the briefing at any level.
  Default `high` for Claude, `medium` for Codex; switching harness clamps to the nearest
  level the other one takes (`max` → `xhigh`).
- **The launcher keeps every scanned directory** (cap raised from 60 to 1000) and caps only
  what is drawn (40 rows); the filter reaches the rest.

## Slices

- [x] Launcher: real effort levels, per harness, clamped across a harness switch; the panel
  height as a named sum with a test that holds rows above chrome for every count to the cap.
- [x] TDSP 0.3: `Response`, `Section`, `Register`, `Body`, `Doubt`; parser with aliases,
  shape-driven bodies, nested-or-flat; catalogue and launch briefing updated.
- [x] Bench: `Hit::ToggleSection`, per-surface toggles forgotten on retire, `showing()`,
  the overview as a filter for responses only.
- [x] Renderer: gist block, one panel per section with a clickable header carrying the
  fold chevron, label and measure; doubts strip; compact and summary bodies.
- [x] Demo fixture carries a response, so a screenshot of the overview is never empty.
- [ ] Looked at on a screen — pending the release build and the photographs.

## Difficulty, after the fact

6 was the right shape. Nothing here needed a gate; what it needed was the two screenshots,
which said more than a plan page would have.
