# Handoff — the notifications bell + every tray's render clamp (2026-09-17)

## Status

**Both landed on main.** `eb84c1d` (#468, the bell) and `f7a19d5` (#470, the tray
clamp), pushed, merged, CI green. Nothing uncommitted. Installed and cut over:
`~/.local/bin/terminal-delight` → `td-0e08ac1-trays`.

⚠️ A window opened before 22:22 still runs whatever build it launched with — the
binary changed, the client did not. Restart a window to see either feature.

⚠️ Other agents cut over this same symlink. If the bell or the clamp appears to be
missing, read `ls -l ~/.local/bin/terminal-delight` before reading any code.

## What's done

| Change | Verified by |
|---|---|
| 🔔 glyph in the bottom-left chrome row (4th, between 🪦 and 🧩) + a `…`-menu entry in all 9 languages | photographed in a `TD_DEMO` window built at the commit |
| `notifpref` — `notifications.toml`, two switches, hot-reloaded on the 2s sweep | 4 unit tests: absent file, half-written file, round trip, garbage |
| `Prefs::default()` = `system: true` — the behaviour that already shipped, not a serde all-false | the absent-file test, which is the only branch that runs on a fresh machine |
| System switch gates `agent_done`'s `notify-send` (and the transcript read behind it) | **no test** — see #481 |
| Marquee switch → chasing-bulb banner, 6s, self-expiring, fades | visual only |
| `render_band` + `tray_cap`/`tray_band`/`tray_max_h`/`tray_card_h` | 5 arithmetic tests |
| 8 trays capped to the render + 1% and scrolling: palette, display, scale, `…`, left-bar, group, plugins, notifications | census test, watched to fail at 7 against a broken build; palette tray photographed |

989 tests green (`981 + 4 + 8` at the clamp commit), clippy clean, fmt clean.

## How to run / verify

```
~/.local/bin/terminal-delight
```
```
TD_DEMO=1 TD_TRAY_DEMO=1 /home/parker/Work/td-tray-height/app/target/debug/terminal-delight
```
```
cd /home/parker/Work/td-tray-height/app && cargo test tray && cargo test notifpref
```

Photograph a demo window by its own rect (`hyprctl -j clients` → `grim -g "<x>,<y> <w>x<h>"`);
never probe Hyprland speculatively on a live session.

## Not done / next

- **#480** — the agent wall, graveyard and help panel still size against the
  window, not the render band. Deliberate (they are full-window modals), but
  undecided and now falsifiable.
- **#481** — deleting the `if !self.notif.system { return; }` gate leaves the
  suite green. The fix is a pure `(watching, prefs) -> intent` decision.
- The mochi mark is 🍡 (dango) — Unicode has no mochi and the repo has no logo
  asset. Swap for inline SVG if Parker has a real mark.

## Watch out

- Two worktrees kept for lineage and clean: `~/Work/td-notify-glyph`,
  `~/Work/td-tray-height`. `~/Work/terminal-delight` itself belongs to another
  thread (`spine/pane-mode-reconciliation`, dirty) — do not check out in it.
- `stash@{0}` is another session's (`shorts-pipeline`, 2026-09-15). Not ours.
- `gh` resolves to mise's copy on PATH; `/home/parker/bin/gh` is the wrapper that
  pins the account. Both answered `parker-brown-family` here.

## Where it's recorded

- APES episode: `projects/terminal-delight/episodes/2026-09-17-a-bell-and-the-trays-that-stop-at-the-render.md`
- APES kanban: both tickets closed with deliverables; two follow-up tickets opened, cross-linked to #480/#481
- lean-ctx: `ctx_session` decision breadcrumb
- file-memory: `an-install-is-not-a-merge`, `a-rule-needs-an-accessor-the-weakest-caller-can-reach`
- Session harvest: `handoffs/2026-09-17-bell-and-tray-clamp.cdx`
