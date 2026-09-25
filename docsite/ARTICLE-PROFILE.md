# Article profile — Terminal Delight docs

Read by the `article` skill. It carries the craft; this file carries what is
specific to this site. Drafted 2026-09-24 from Parker's direction in the session
that started the site; the intake answers marked *(default)* were chosen by the
agent and are his to overturn.

**Site:** https://docs.terminal-delight.brownfamilysports.com (Cloudflare Pages,
built by `docsite/build.mjs`). Until that is live the same pages are served from
the kiosk domain at `/docsite/`.
**Renders from:** hand-written HTML in `docsite/`, on the shared vanilla shell in
`assets/td-shell.{css,js}`. No framework, no markdown pipeline yet.

## Audience

- **Publishes:** how Terminal Delight works, one feature per page.
- **Read by:** people running coding agents in Terminal Delight, people deciding
  whether to, and agents that get a page pasted into them (hence the copy
  header).
- **Reader's relationship to the author:** user of a tool. *(default)*

## Registers

- **Axis:** treatment. Keys `brief` / `story` / `technical`, labels **Brief**,
  **Story**, **Technical** — the same axis and labels as parkerbrown-dev, which
  is where Parker pointed.
- **Default register:** Brief.
- **Brief** is answer-first: what the feature is, what you do with it, the one
  key to remember. 120–200 words, then one line of invitation.
- **Story** is the analogy (Parker: *"STORY is the analogy"*). One metaphor, held
  to the end; scene, question, turn, resolution; ends with the reader holding the
  real terms. 500–900 words. Optional — a forced analogy is worse than none.
- **Technical** is the reference: tables of keys, strings exactly as the UI
  prints them, file paths, protocol fields, limits, and a closing paragraph on
  what would prove the page wrong. No cap. Every number resolves to a source.

## Voice

- **Person:** second person, present tense — *you flip the pane*. *(default)*
  Docs describe a tool the reader is holding; they are not a build log.
- **Real strings only.** A label, key or path is quoted as the code defines it,
  and checked against `main` at the commit named in the page's meta row.
- **Byline:** none on the page. The copy header names the source.
- **Never:** marketing adjectives, a feature described by what it is not, or a
  sentence about the implementation where the reader needed the behaviour.

## Page furniture

- Meta row under the title: `Updated <date> · true of main at <sha>`. A docs page
  read cold must say which build it describes.
- Figures are hand-built inline SVG on the shell's tokens, so they repaint in
  glass and paper. Captions make a claim.
- CTA: `Download` in the top bar is a utility and stays. No conversion ask
  anywhere else.

## Liability gate

- **Required:** no.

## Categories

The spine groups (Get started, Agents, The bench, Workspace, The look, MCP and
scripting, Desktop, Reference) are navigation, not categories. No chip renders.
