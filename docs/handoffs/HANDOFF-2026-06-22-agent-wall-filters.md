# Agent Wall Filters Handoff

Date: 2026-06-22
Project: terminal-delight
Branch: main
Status: uncommitted local work, not pushed

## Status

The Agent Wall / graveyard visual work and final filter pass are implemented in the dirty working tree. HEAD is `da35da5` on `main`; local `main` is behind `origin/main` by 1 commit. Nothing was committed, pushed, stashed, reset, or reverted during tie-off.

Known dirty state at tie-off:

- `app/src/main.rs`: contains this session's Agent Wall card/filter work, including the final program-filter pass.
- `app/src/lang.rs`, `app/src/pane.rs`, `app/src/plugins.rs`, `app/src/theme.rs`, `docs/2026-06-21-leanctx-savings-plugin-gap-analysis.md`, `plugins/`: pre-existing/concurrent dirty work in the shared tree. Do not assume these are all part of the Agent Wall filter change.

## Done

Agent Wall and graveyard surfaces were restyled as raised phosphor cards and tiled as uniform rectangles. Agent Wall theme mode now inherits the outer visual treatment with curved-glass dashboard behavior rather than only retinting rows.

The final filter pass added a context-dependent program/mode filter:

- `mcp_program_filter: Option<String>` stores the active live program filter.
- `agent_program_glow(...)` centralizes CLAUDE, CODEX, SHELL, and fallback colours.
- The filter strip now generates program chips from live pane modes, so `SHELL` or any other running mode appears only when present.
- Zero-count state chips are hidden; when the current context contains only Shell/non-agent panes, the state-chip row is omitted.
- Group, program, and state predicates combine for chip counts, card visibility, and group header counts.

Survival marker:

```bash
rg -n "agent_program_glow|mcp_program_filter|mcp-program-chips|show_state_chips|visible_program_total" app/src/main.rs
```

## Verification

Run from the Rust crate directory:

```bash
cd /home/pbrown/BROWN-FAMILY-SPORTS/Software/terminal-delight/app
cargo check
cargo build
cargo test -- --test-threads=1
```

Results during this tie-off:

- `cargo check`: pass
- `cargo build`: pass
- `cargo test -- --test-threads=1`: 184 passed
- `git diff --check`: clean

A separate demo process was launched earlier with:

```bash
TD_DEMO=1 TD_DEMO_STATE="$HOME/.config/terminal-delight/state.toml" app/target/debug/terminal-delight
```

It was PID `1593061`; by tie-off it was no longer running.

## Not Done

No commit or PR was created. The local branch is behind `origin/main` by one commit, so reconcile before committing if that upstream change matters.

The cdx/context-delight export step could not package the current Codex transcript. `cdx list` worked, but `cdx export /home/pbrown/.codex/sessions/2026/06/21/rollout-2026-06-21T22-52-57-019eede3-9cb9-7083-b026-acf205fec33a.jsonl` returned `not a recognized transcript`. No `.cdx` or `ctxpkg` artifact was written.

## Watch Out

This was a shared dirty worktree. Do not use broad cleanup commands. If you prepare a commit, inspect the diff carefully and stage only the intended hunks, especially in `app/src/main.rs`, which also contains concurrent plugin/savings/dashboard work.

The Rust crate is under `app/`; running `cargo check` from the repo root fails because there is no root `Cargo.toml`.

The Agent Wall filter logic is intentionally dimensional. If another state or program type is added, counts should be derived from the same predicates used by card visibility, not from a separate ad hoc list.

## Recorded

APES tasks are all closed for this session's Agent Wall work:

- `restyle-agent-wall-and-graveyard-rows-as-raised-phosphor-status-cards-mqotxu5t`
- `tile-agent-wall-cards-and-stabilize-dashboard-overlay-positioning-mqouoncf`
- `add-curved-glass-agent-wall-theme-mode-and-combinable-state-filters-mqovu7jq`
- `make-agent-wall-filters-context-generated-and-hide-zero-count-chips-mqowfh1a`

lean-ctx records written:

- `terminal-delight-ui/agent-wall-filter-model`
- `terminal-delight-testing/terminal-delight-rust-crate-dir`
- session decision breadcrumb describing the uncommitted Agent Wall filter work and verification
