# Status: Terminal Delight can be Omarchy's default terminal

**Difficulty: 6/10** — a familiar shape (flags, the quick window, a pane running a program) landing in the launch path twenty live panes depend on → one gate (`plan.md`)
**Turned out to be:** 5/10. The quick window, `vt::pty::Options.shell` and the reap-closes-the-window path were all already there; the work was a pure parser in front of `dispatch`, one field through `PaneRestore` and `SpawnSpec`, and the reversible switch. The one surprise was in the rig, not the code: `grim -T` no longer answers while the screens are asleep (see below).

- Plan (`plan.md`): **approved 2026-10-15** — Parker stamped CONCUR on "Approve the default-terminal plan now?" and wrote *"GET IT!"* beside "It can be Omarchy's default terminal, and TD can't yet" on the Splinterm intake brief (`reports/2026-10-15-splinterm-intake.html`).
- **Amendment, approved the same way:** the recommendation he concurred with was "approve it, with their way back out: your previous default, restored exactly". So this branch also carries `terminal-delight default-terminal [status|on|off]` (`app/src/defaultterm.rs`), which the plan as written left out ("does not flip your system default"). It is built and tested against scratch XDG directories only; **it has not been run against Parker's own `~/.config`** — flipping his default is still his call.
- Pre-flight B (plugin setup, one password): **not done** — slice 4 skipped, as the plan says it would be.
- Pre-flight C (slice 4 window placement): default hidden workspace.
- Pre-flight D (merge policy): default **leave open** — the PR waits for Parker.

## Slices
- [x] Slice 1 — tracer bullet: `--app-id/--title/-e` open a quick window running the program, closed on exit. `parse_contract`, `Launch::Contract`, `contract_seed`; `PaneRestore.program` → `SpawnSpec.program` → `vt::pty::Options.shell`. Proven live by `scripts/contract-check.sh` phase 2, through xdg-terminal-exec itself.
- [x] Slice 2 — desktop-entry keys (`packaging/terminal-delight.desktop`); xdg-terminal-exec under scratch `XDG_CONFIG_HOME`/`XDG_DATA_HOME` picks the entry and builds the exact command the parser reads (phase 1, `--print-cmd`). `~/.config` and `~/.local/share/applications` never written.
- [x] Slice 3 — `LOCAL MODEL` mark for panes launched under `org.omarchy.local-ai` (`launch_mark`, `TerminalView.launch_mark`, header chip). Proven by the gpui test `a_launched_pane_runs_its_program_and_types_nothing` and live by phase 3. **No photograph yet:** both outputs were DPMS-off and `grim -T` hung, for the installed build too.
- [ ] Slice 4 — the real plugin's CPU recipe opens Claude Code into TD — **skipped: needs pre-flight B.**
- [x] Slice 5 — one PR closing #178. The handoff, sticky and clip command are in the session's outcome brief; the 0xSero outreach learning brief is not written yet.

## Notes for a fresh session
- Worktree: `.worktrees/default-terminal` (moved from `~/Work/td-default-terminal` on 2026-10-15), branch `feat/omarchy-default-terminal`, rebased onto `origin/main` 7d5abb2.
- APES ticket: `terminal-delight-speaks-the-xdg-terminal-exec-contract-so-omarchy-and-local-ai-a-muzzpkgh`.
- Origin: the 2026-10-08 assessment of 0xSero's $50 recipe bounty. The plugin launches agents at `bin/omarchy-local-ai:1279` (`fef7eb8`) via `omarchy-launch-tui` → `xdg-terminal-exec --app-id=… -e …`. Evidence comment on #178.
- Omarchy's own switch, read 2026-10-15: `omarchy-default-terminal <name>` overwrites `~/.config/xdg-terminals.list` with one id; with no list (this box), xdg-terminal-exec 0.14.3 falls back to `~/.local/share/applications/foot.desktop`. `default-terminal off` therefore removes the list rather than writing an empty one.
- No automated screen capture of Parker's screen. The chip photograph needs the screens awake: `scripts/contract-check.sh --shot <dir>`.
