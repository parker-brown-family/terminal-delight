> **Superseded 2026-10-22** as the plan of record by the org chart (`00-status.md`, `01-tiers-and-enforcement.md`). Kept because its read/act split, position lattice and verb table carry forward.

# Plan: read-only panes

**Difficulty 6/10** — see `00-status.md` for why it is not 7.

## What it is for

A pane today is either open to agents or off-limits to them. Parker exercised that on 2026-10-22: he hid a pane, an agent asked to see it again, he pressed Yes, and nothing moved without his press. 0.5.0 adds `agent_send`, which lets one agent hand work to another agent's conversation. With two positions, a pane a managing agent can watch is a pane it can assign work to, and the only way to stop the assignment is to blind the manager. A person needs a third position: **agents may watch this pane, and may not write to it or hand it work.**

## The shape

Three levels on each pane, chosen only by a person's press. The level answers *what other agents may do to this pane*; what one program may do anywhere stays with Restricted runs (`docs/plans/process-authority/`).

```rust
/// A person's choice for one pane. Only the pane's right-click menu and the
/// rail's Yes set it; no MCP or ctl verb can.
pub enum AgentAccess { Open, ReadOnly, OffLimits }

/// What another agent gets of one pane, after the window's switches.
pub enum Reach { Hidden, Read, Act }
impl Reach { pub fn can_read(self) -> bool; pub fn can_act(self) -> bool; }

/// Replaces `should_expose`. The pane can only lower what the window allows:
/// MCP off → Hidden; OffLimits → Hidden; a plain shell under agents-only →
/// Hidden; ReadOnly → Read; Open → Act. Writes off is still checked where it
/// is today (`apply_permission`), so Act means "the pane permits it".
pub fn reach(cfg: &McpConfig, is_agent: bool, access: AgentAccess) -> Reach;

/// Replaces `may_write`. The caller's own pane is treated as Open, so the agent
/// inside keeps its note, deliverable, bench and documents at every level.
pub fn reach_for(cfg: &McpConfig, is_agent: bool, access: AgentAccess, own_pane: bool) -> Reach;
```

- **`PaneInfo.exposed: bool` is deleted**, replaced by `reach: Reach` and `access: AgentAccess`. Every read of `exposed` stops compiling and must say `can_read()` or `can_act()`. Counted by grep, tests excluded: 25 lines in `main.rs`, 11 in `mcp.rs`, 3 in `coordination_view.rs` on `dev/0.5.0` at `51ae902e`, and 4 in `agent_management/worker.rs` on `feat/agent-management` at `017da926`.
- **Saved leaf:** `off_limits: bool` stays and is written `true` for both ReadOnly and OffLimits; a new `agents` key is written `"read-only"` for ReadOnly only. Reading: a known `agents` word wins; an unknown word is OffLimits; no `agents` key falls back to `off_limits` (true → OffLimits, absent → Open). The 0.4 reader (`LeafFields`, `release/0.4` `main.rs` ~914) derives `Deserialize` without `deny_unknown_fields`, so it ignores `agents` and reads `off_limits = true` — an older window sees a read-only pane as off-limits.
- **`access.rs`:** `Change::ReadOnlyPane` (wire `read_only_pane`); `open_pane` keeps meaning fully open; `Now.pane_off_limits: Option<bool>` becomes `pane_access: Option<AgentAccess>`; `pane_mark` becomes `pane_level(change, now) -> (AgentAccess, Outcome)`.
- **Control socket:** `bench say|type|choose|submit|click|hover`, `doc here|beside|close|scroll|note|concur|save` act only on an Open pane; `doc notes` and `bench probe` read a ReadOnly one.
- **Census:** each pane in `list_panes` carries `agents: "open" | "read-only"`; the answer carries `hidden: <n>`, a count of panes this caller cannot see, with no titles or pids.
- **Agent-management:** `agent_send` and the worker's dispatch-time recheck read the actable set; `agent_targets` lists a read-only agent pane under `unavailable` with the reason.

## Least confident decisions

1. **The `hidden` count.** It tells an agent that panes exist which it cannot see — metadata, not content — so that an empty answer is not read as "nothing is running". Drop it if even existence should be private.
2. **The ceiling is strict.** Under "agents only", a plain shell cannot be made readable on its own; the person exposes shells and closes the rest. The alternative (a pane level may raise a shell above the window's expose policy) is a widening and is left out.
3. **`outer` restyles still reach a read-only pane that inherits the outer look.** `set_pane_config` on `outer` is a window-level write, not a write to that pane; a person who minds pins the pane's own appearance.
4. **Coordination stays withheld whenever any off-limits pane has been seen** (`coordination_view.rs` line 118); read-only panes do not trip it.
5. **`engineering_state` still names the checkout and tab of every pane at every level** — the known limit from the off-limits plan, unchanged.

## Slices

1. **Tracer bullet — the type and the split gate.** `AgentAccess`, `Reach`, `reach`, `reach_for`; delete `exposed`; fix all 43 sites; ReadOnly settable only from tests. Proof: a matrix test over every write verb × {Open, ReadOnly, OffLimits} × {other agent, own pane}; for each write gate, a mutation that swaps `can_act` for `can_read` fails a test.
2. **The saved layout, both directions.** Write and read rules above. Proof: a 0.4-shaped fixture parser (copied `LeafFields`) reads a new file as off-limits; an unknown `agents` word reads as off-limits; an old file reads as open.
3. **The gesture and the mark.** Three rows in the right-click menu (*Open to agents*, *Read-only to agents*, *Off-limits to agents*), the current one checked; a `READ-ONLY` badge where `OFF-LIMITS` sits; nine languages; the source scan that holds setting the level to the menu and the rail extends to the new setter.
4. **The control socket and the ask.** The ctl refusals; `read_only_pane` on the rail with its words and `nothing_to_ask`.
5. **The census and the docs.** `agents` and `hidden` in `list_panes`; `docs/mcp.md`, the docsite MCP page, `docs/security/terminal-input.md`.
6. **Agent-management adoption**, on that PR's branch with its owner: the actable set at send and at dispatch; read-only panes in `unavailable`. Proof: `agent_send` to a read-only pane is refused at request and, after a level change, at dispatch.
