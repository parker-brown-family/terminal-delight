# Status: Terminal Delight can be Omarchy's default terminal

**Difficulty: 6/10** — a familiar shape (flags, the quick window, a pane running a program) landing in the launch path twenty live panes depend on → one gate (`plan.md`)
**Turned out to be:** <filled in at the end>

- Plan (`plan.md`): in progress — awaiting Parker's approval
- Pre-flight B (plugin setup, one password): pending
- Pre-flight C (slice 4 window placement): default hidden workspace
- Pre-flight D (merge policy): default leave open

## Slices
- [ ] Slice 1 — tracer bullet: `--app-id/--title/-e` open a quick window running the program, closed on exit
- [ ] Slice 2 — desktop-entry keys; the plugin's exact `omarchy-launch-tui` line lands in TD via scratch XDG env
- [ ] Slice 3 — `LOCAL MODEL` chip for panes launched under `org.omarchy.local-ai`
- [ ] Slice 4 — the real plugin's CPU recipe opens Claude Code into TD (needs pre-flight B)
- [ ] Slice 5 — one PR closing #178, handoff, sticky, clip command for Parker, learning brief for outreach

## Notes for a fresh session
- Worktree: `~/Work/td-default-terminal`, branch `feat/omarchy-default-terminal`, cut from `origin/main` 8659826; `app/target` reflinked warm from the main checkout.
- APES ticket: `terminal-delight-speaks-the-xdg-terminal-exec-contract-so-omarchy-and-local-ai-a-muzzpkgh`.
- Origin: the 2026-10-08 assessment of 0xSero's $50 recipe bounty. The plugin launches agents at `bin/omarchy-local-ai:1279` (`fef7eb8`) via `omarchy-launch-tui` → `xdg-terminal-exec --app-id=… -e …`. Evidence comment on #178.
- Today, a contract launch hits `flag_reply` and is refused with exit 2. That is safe: no window and no session claimed.
- Quick windows already get a per-pid ctl socket (`ctl.rs::socket_path`), never save (`Workspace::save`), and hide the left bar.
- `vt::pty::Options.shell = Some((program, args))` is the lever the demo window uses to run a non-shell program in a pane.
- `~/.config/xdg-terminals.list` does not exist on this box. Omarchy's foot entry with the four `X-TerminalArg*` keys is at `/usr/share/omarchy/applications/foot.desktop`.
- The plugin's CPU recipe is `lfm25-26b-qad-q4-cpu` (1.6 GB, llama.cpp). Whether the 16 GB RTX 3080 Laptop matches their `rtx-3080-10gb` entry is unknown until slice 4.
- No automated screen capture. Standing instruction since 2026-09-08: hand Parker the clip command.
