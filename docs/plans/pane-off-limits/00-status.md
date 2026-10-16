# Status: a pane can be off-limits to agents

**Difficulty: 5/10** — one flag, but it must hold at every place an agent reads a pane, and a hole is silent → one combined plan (`plan.md`)
**Turned out to be:** 5/10. The gate was already one function, so the flag reached every MCP read through the compiler; the work that was not in the plan was the control socket — its bench verbs reach a running agent's prompt from outside, so they had to refuse an off-limits pane too, and the security manifest had been saying the socket could only reach new panes.

- Plan (`plan.md`): direction approved 2026-10-15 — Parker wrote *"GREAT - yes!"* beside "An agent gets a pane, not your keys" on the Splinterm intake brief. The details are mine and the PR waits for his read.
- **Changed from the plan:** the toggle is on the terminal's right-click menu, not the header's ⋯ menu — the ⋯ only appears on a pane narrow enough to have tucked controls away, and the gesture has to be there on every pane.

## Slices
- [x] Slice 1 — the flag and the gate: `should_expose(cfg, is_agent, off_limits)` at all six sites; `may_write` with the caller's-own-pane exception (`ConfigPatch::own_pane`, `#[serde(skip)]`); the saved-leaf field (`skip_serializing_if` false, `#[serde(default)]` on read); the four `ctl bench` verbs refuse it.
- [x] Slice 2 — the gesture and the mark: right-click → *Off-limits to agents* / *Let agents see this pane*; `OFF-LIMITS` in the header before the mode label; `OffLimitsChanged` saves the layout.
- [x] Slice 3 — land: docs (`docs/mcp.md`, the docsite MCP page, `docs/security/terminal-input.md` — corrected about the socket's bench verbs). No photograph: the screens were asleep and `grim -T` hangs.

## Known limits, in the PR
- `ctl bench click|hover` drives the window's pointer; off-limits cannot stop a pointer.
- `engineering_state` still names the checkout and tab an off-limits pane is in (metadata, also what drives the person's own rail).
- The caller's pane is known by an id in its environment; a forged id reaches another pane's note, deliverable and bench — writes only, never a read.
