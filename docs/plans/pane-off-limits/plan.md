# Plan: a pane can be off-limits to agents

**Difficulty 5/10.** One flag on a pane. The risk is that it has to hold at every place an agent can read a pane, and a hole in it is silent: an agent reading what a person marked private looks exactly like an agent working normally. One combined plan, one approval.

## Where this came from

The Splinterm intake brief (2026-10-15) showed their launch video's second half: an agent connected through MCP gets the staging pane, the production pane is marked **OFF-LIMITS — the agent cannot read or type here**, and access is cut on screen. Terminal Delight had two switches for the whole window (MCP on, writes on) and no way to say *not this one*. Parker beside that item: *"GREAT - yes!"*. The first half — labelling what agents read as untrusted — shipped separately; this is the second.

## What it does

- **The terminal's right-click menu gains *Off-limits to agents*.** Pressed, the pane wears an `OFF-LIMITS` mark before its mode label, and the menu row reads *Let agents see this pane*. That gesture is the only way in or out. (First drafted on the header's ⋯ menu, which only appears on a pane narrow enough to have tucked controls away.)
- **Every agent-facing read skips an off-limits pane**, because they all ask one question — `mcp::should_expose(cfg, is_agent, off_limits)` — and the new argument makes the compiler name every caller: `list_panes`, `grep`, `pane_events`, the pushed `agent_appeared`/`tool_call` events, `get_pane_config`/`set_pane_config`, and `leave_note`/`declare_deliverable`/`present_surface`/`open_document` aimed at it from another pane.
- **The agent running inside the pane keeps its own-pane verbs.** Presenting its own work on its own bench, declaring its own deliverable and posting its own sticky note still land: off-limits hides a pane from other agents, it does not silence the one inside it.
- **It survives a restart.** A new field on the saved leaf, written only when set; every file written before reads back as *not off-limits*, which is how those panes behaved.
- **No control-socket verb and no MCP verb can set or clear it.** The control socket answers any process running as you, agents included, and a lock an agent can lift is decoration. For the same reason the socket's bench verbs (`ctl bench say|choose|submit|type`), which reach an agent's prompt from outside, never pick an off-limits pane.

## Least confident decisions

1. **The agent inside keeps its own-pane verbs.** The alternative — an off-limits pane is dark to every agent, its own included — would stop a Claude pane presenting its own turn. If the point of marking a pane is *no agent at all here*, flip `own_pane_may_act` to `false`.
2. **`engineering_state` still names the checkout and tab an off-limits pane is in.** That reading also drives your own attention rail, and what it carries is git's and the tab strip's metadata, not anything the pane printed. Hiding it from agents alone needs the rail's reading split in two; not done here.
3. **A pane cannot be marked from outside the window.** No `ctl` verb, on purpose (above). The cost: a script cannot set up a pane as off-limits for you.

## Slices

1. **The flag and the gate.** `TerminalView.off_limits`, `PaneRestore.off_limits`, the saved-leaf field; `should_expose` takes it; the own-pane exception in `apply_mcp_updates`. Tests: every read skips the pane; the agent inside keeps its verbs; an old file reads back as not off-limits; a saved one round-trips.
2. **The gesture and the mark.** The right-click menu row, the header mark. A gpui test toggles the pane and reads it back, and a source scan holds the toggle to that one caller.
3. **Land.** One PR; the docs (`docs/mcp.md`, the docsite MCP page, `docs/security/terminal-input.md`'s MCP paragraph).
