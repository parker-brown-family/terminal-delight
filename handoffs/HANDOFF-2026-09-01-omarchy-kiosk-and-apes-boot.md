# Handoff — the Omarchy kiosk, and the APES boot that wasn't firing

**Date:** 2026-09-01
**Branch:** `feat/omarchy-kiosk` → PR
[#242](https://github.com/parker-brown-family/terminal-delight/pull/242)
**Status:** kiosk built and verified, PR open, awaiting Rust CI (the PR touches
no Rust; the job runs on every PR)

Two unrelated threads, done together because the first one explains why the
second was invisible.

---

## 1. APES was not booting in this repo, and had not been for two weeks

### What was wrong

`~/bin/agent-orient-boot` resolves the APES project by **longest-path-prefix**
match of cwd against `$APES_ROOT/projects/index.json`. APES had
`terminal-delight` registered at
`/home/parker/BROWN-FAMILY-SPORTS/Software/terminal-delight`.

That is not this repo. It is an abandoned `td-glyph` checkout whose HEAD is
`1b87b04 wip: pre-transfer snapshot` (2026-08-18). The live tree moved to
`~/Work/terminal-delight` and nothing told APES.

So no prefix matched, the resolver returned `(none)`, and **every session in
this repo since 2026-08-18 booted with no APES project truth at all** — with no
warning, because "the registration is stale" and "this is not an APES
workspace" were the same silence.

An earlier agent had spotted it and filed
`repoint-the-apes-terminal-delight-project-at-the-live-repo-mthbrqw0` on
2026-08-31. It sat in backlog. That ticket is now closed.

### What changed

**The registration**, in `$APES_ROOT`:

- `projects/index.json` → `/home/parker/Work/terminal-delight`
- `projects/terminal-delight/manifest.json` → same (it also still carried a
  `/home/pbrown` username from an older machine)

Both backed up as `*.bak-20260901-141925`.

**The resolver**, so this class cannot silently recur:

- **Git-repo-name drift fallback.** When no path prefix matches, ask git for the
  enclosing repo and match its directory name against registered project
  *names*. A hit resolves against the **live** tree and appends a loud
  stale-registration warning naming the repair. A miss still means "not an APES
  workspace", so an unregistered repo does not false-positive.
- **`agent-orient-boot --audit`** — every registration whose path has gone
  missing (always a fault) or whose git remote disagrees with the project name
  (a hint; a monorepo subdir or renamed fork trips it legitimately). Currently:
  72 registered, 1 path gone (`omarchy`), 10 remote-name hints.
- **`agent-orient-boot --repoint`** — run inside a repo, re-registers it where
  it actually lives, backing up the index and manifest first.

**Subagents now boot too.** Claude Code 2.1.251 exposes a `SubagentStart` hook
event that nothing was wired to, so every agent spawned by the Agent tool worked
contextless while its parent believed the fleet was oriented. It is now wired in
`~/.claude/settings.json` to the same resolver and envelope. It always emits (no
`--if-changed`): a subagent has no prior turn to have been told, and the parent's
stamp would otherwise silence it.

**Doctrine** updated in `~/.config/agents/AGENTS.md` — the canonical cross-agent
file, so Codex and Gemini get it through their symlinks. The agent table now
lists `SubagentStart`, and there is a section on the drift fallback and the two
repair verbs.

### Verified

On a synthetic index, not by breaking the real one
(`scratchpad/test-drift.sh`, six checks, all pass):

1. `--project` resolves despite a stale registration
2. `--why` names the fallback and prints the stale path
3. the emitted mandate carries the drift warning
4. an unregistered git repo still resolves to nothing
5. `--repoint` repairs the index and manifest
6. after repair, resolution is by prefix again and the warning is gone

### Left open

`config/registry.yaml` in the APES repo is a **second** path registry, read by
`con`, `pi`, `status` and the launcher — and it still holds **70 paths under
`/home/pbrown`**, a user that does not exist here. So the CLI fallback the boot
mandate literally names (`apes … con <project>`) serves confident truth about
the wrong repo. Filed as a falsifiable issue with an invalidation criterion:

**parker-brown-family/apes#26** — `config/registry.yaml` still resolves 70
project paths under `/home/pbrown`

The `agent-orient-boot` fix does not reach that leg.

---

## 2. The Omarchy kiosk

`omarchy.html` joins the kiosk family (info · agents · tv · global · gamba ·
start-crawl), linked from the home page's nav, both cabinet rows and the footer.

### Why

The desktop half was the least visible thing the project ships — three
repositories, a picker that paints terminal tiles which have never heard of
Terminal Delight, agents that survive being dragged between windows by session
id — and all of it lived in a README table, below the fold. Every other feature
of comparable weight already had a cabinet.

### What it covers

The two-shelf paint overlay and its keyboard · the eleven variant names shared
with the Omarchy theme pack · `SUPER+ALT+T` window adoption (what migrates, what
is refused, that nothing is closed on a refusal) · the Quickshell-rendered
notification with its transcript-derived recap and click-to-jump · the per-agent
tab badge strip and its precedence rule · the MCP control surface · the
three-repo topology with `td-tint` as the seam.

### The move worth keeping

**The page wears the themes it describes.** All 23 Omarchy schemes on a stock
box were read at build time from the same two roots `palette.rs` reads
(`/usr/share/omarchy/themes` and `~/.config/omarchy/themes`, user shadowing
stock by name) and inlined as their **named roles**. Selecting one rewrites the
page's role variables — which is exactly what painting a pane does to its colour
table. So the light schemes invert the whole page, the texture never moves, and
the claim in the copy is demonstrated rather than asserted.

`?t=<theme>` opens it already wearing one; the choice persists in
`localStorage`.

The badge strip uses the **shipped mascot art at the shipped timings** —
`assets/omarchy/{robot-only,blinker-only}.png`, copied from `app/assets/img/` —
with the HEY layer's 700 ms hard square wave and the 1.4 s bounce-eased breath
that never reaches zero. `prefers-reduced-motion` stills all of it.

### Deliberately not done

No screenshots of the paint overlay or the notification. Both are staged mocks
built from the same live variables, because a stale PNG of a themed surface is
the one asset that cannot survive a palette change.

### Regenerating the palette payload

If Omarchy themes are added or removed, re-run the generator and re-inject —
`scratchpad/palettes.sh` emits the JSON, `scratchpad/inject.sh` replaces the
`const PALETTES = [...]` block one object per line so the diff stays legible.
Both are in the session scratchpad; worth promoting into `scripts/` if this
needs doing more than once.

### Verified

Chromium headless at 1440 and 520 wide, dark and light schemes; `node --check`
on the script block; tag balance; every local asset, sibling-page link and
in-page anchor resolves. Same CSP posture as the other kiosks — nothing
third-party can load, payload inline, art same-origin.

---

## Next

- Merge #242 once Rust CI is green; Pages serves `main` root, so the merge **is**
  the deploy. It lands at `/omarchy`.
- Pick up parker-brown-family/apes#26 — run its invalidation checks first.
- `agent-orient-boot --audit` reports `omarchy` as a gone path; it is the only
  hard fault left in the index.
