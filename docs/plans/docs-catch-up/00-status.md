# Docs catch-up — status

**Difficulty: 6/10**, scored before starting. The docs can be wrong for weeks
before anyone notices, and the reader who notices is a stranger deciding
whether to install. Undoing a bad page is cheap, though, so it bought one plan
page and no gates. Parker was away and asked for it to be done without him
(2026-11-01: *"I am AFK so you must unblock and judge for yourself"*), so the
approval below is the agent's, recorded as calls for him to stamp or overturn.

## What was asked

Update the docs website, the README and everywhere else the product is
described or documented, thoroughly. Leave the kiosk pages alone (the root
`index.html`, `info.html`, `tv.html`, `agents.html`, `global.html`,
`start-crawl.html`, `omarchy.html`).

## What was found

- The live docs site is deployed from `main`. Most of its pages were last traced
  to the code on 25 September (`d7cf383`), before most of 0.4.0 landed.
- Pages written since on `dev/0.5.0` (Restricted commands, the Threshing Floor in
  the MCP and agent-wall pages, scripted-only MCP policy) never reached `main`.
- The README, `docs/features/`, `ENV-FLAGS.md`, `BELL_SOUNDS.md` and
  `docs/mcp.md` contradicted the code in dozens of places (issue 803), and the
  surface protocol specs in many more (issue 804).
- 0.5.0 adds features no page described: the Threshing Floor, resting agents,
  Clean and UI CONFIG, TERMINIMALIST, the artifact rail and archive, Restricted
  commands, uninstall from CONFIG › ADMIN.

## The calls

Recorded in `docs-catch-up.calls.json` (the `briefing` skill's ledger).

1. **One docs pull request into `main`, with 0.5.0 work marked on the page.**
   The ledger's policy said to ask first (medium uncertainty, medium taste,
   standard posture). Parker's instruction for this session overrides it; the
   choice is the most reversible one available, and it is the first call for him
   to stamp. 0.4.0 is the baseline every page describes; 0.5.0-only behaviour is
   marked with `.in-dev`, `.dev-tag`, or a nav.json `"release"` that tags the
   page and opens it on a line saying it is development work.
2. **Deploy the rebuilt site after the pull request merges.** A push does not
   deploy. Undo is a redeploy from the previous commit.
3. **`docs/features/` becomes a map to the docs site.** Each file keeps its name
   and points at the traced pages; the two that carried reasoning found nowhere
   else (the usage collectors, the withdrawn keepalive) keep that reasoning.

## How it was done

- The build learned the development markers, a `TD_DOCS_OUT` directory for
  parallel builds, an `either` group so the key sheet builds against either
  release's app source, and a "soon" state for a chapter with no written page.
- Ten agents traced the pages in parallel against both trees (`main` for 0.4.0,
  `dev/0.5.0` for 0.5.0), each owning separate files, under one written brief.
  Nine new pages: Threshing Floor, Resting agents, Artifacts, Documents, Git
  signal, UI CONFIG, Omarchy, Uninstall, Reporting a bug.
- The coordinator rewrote the README, BELL_SOUNDS, the feature map, GLOSSARY's
  stale and missing entries, and the CONTRIBUTING rules that keep a page tied
  to the change that alters it.

## Not done here

- In-app help strings that issue 803 lists (`lang.rs`) are app code and need a
  build, tests and nine languages; they stay on issue 803.
- `CHANGELOG.md` and the release notes are release work and were left alone.
- `dev/0.5.0` receives these pages through its next merge from `main`; the
  `docs/features/` files that branch edited will conflict there, and the pointer
  versions here are the ones to keep.

## Post-hoc

To be filled in when the pull request merges and the site is deployed.
