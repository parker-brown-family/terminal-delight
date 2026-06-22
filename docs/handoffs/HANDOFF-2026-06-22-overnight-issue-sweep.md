# Handoff — overnight issue sweep (2026-06-22)

Autonomous session while Parker slept. Goal: graveyard-card polish (award-winning),
then kick off work across the open GitHub issues, boyscout (tests/clippy/principles)
along the way. Each fix is its own squashed PR so anything can be reverted cleanly.

## Landed (merged to `main` + redeployed)

Binary rebuilt + hot-swapped after the last merge → `~/.local/bin/terminal-delight-main.bin`
(07:06). The auto-relaunch watcher (armed earlier) will launch THIS binary when the
old window closes, restoring the 82-pane layout. Plugins installed to `~/.local/bin`.

| PR | Issue | What |
|----|-------|------|
| #95 | — | Savings overlay shrunk from near-fullscreen to a small ~420px centred card |
| #96 | — (Parker's explicit ask) | **Award-winning graveyard cards**: dropped the full-height left pill, tucked the .cdx/RESURRECT actions into a compact centred stack, inline kind-chip + title + meta — now the same family as the live agent-wall card. Added leak-safe `TD_GRAVEYARD_DEMO`. |
| #97 | #68 | clippy `-D warnings --all-targets` sweep → 0 errors (was 7: gamba×2 const-assert, theme×1 + main×1 items-after-test, mcp×2 needless-borrow, main×1 default-field) |
| #98 | #92 | `scripts/install-plugins.sh` (idempotent) + td-redeploy skill step → installed binary now resolves bundled plugins (`leanctx-mcp`) |
| #99 | #86 | Mother bar `overflow_hidden` on the left group → truncates instead of painting over the right controls at narrow widths (verified 940px + 720px) |
| (prior) #85 | — | The `</> LeanCTX` savings plugin itself (plugin #2) |

**Closed:** #77 (plugin UI verified on real GPU — savings overlay does the full
discover→spawn→tools/call→render roundtrip; graveyard .cdx renders).

## Analyzed but deliberately NOT changed (don't "fix" a non-problem)

- **#88 (warp hit-test drift, tall panes/max warp)** — the screen→cell map in
  `pane.rs::viewport_cell`/`warp_screen_to_content` is provably self-consistent with
  the shader (it round-trips: a displayed cell maps back to itself). The residual
  drift is second-order. Top candidates documented on the issue: (1) stale
  `self.warp_k` vs the live `theme::warp_coeffs` at click time; (2) `grid_pad`'s
  corner-overscan approximation (evaluated at `r²=0.25`, but the bottom corner bows
  to `r²→0.5`). **Needs `TD_HITDEBUG=1` interactive repro** on the real display to
  pin which — changing working hit-test math blind risks a regression.
- **#81 (dashboard hit-test offset)** — almost certainly already fixed: `mcp_menu`
  is in `warp::set_suppressed()`, so the dashboard renders flat (no warp) and clicks
  aren't bent. Commented; can close if it no longer repros.
- **#90 (startup self-heal from richer backup)** — infra already exists
  (`backups/`, `state.toml.last-good`, the write-side `is_catastrophic_shrink`
  guard). The startup side is risky: auto-restoring a richer backup would wrongly
  resurrect a session the user *deliberately* downsized (crash-collapse vs
  intentional-close can't be told apart at load). Should be an **offer/prompt**, not
  an auto-restore. Deferred to avoid a session-state landmine.

## Not started (need repro / design / scale)

- **#23** ack-bar flash, **#87** FOCUS select-to-copy — need interactive repro.
- **#78** global.html CRT perf — needs a real WebGL2 browser.
- **#82** richer card data, **#80** per-card CONTEXT button, **#83** 3-mode theme
  inheritance, **#84** Continue button — features needing design decisions.
- **#75** i18n the new agent-dashboard + recover strings — real, but the savings +
  graveyard strings I added are English literals, so #75's scope grew slightly.
  Deserves the `apes/localization/` i18n-sweep method + quality 9-lang translations,
  not a rushed overnight pass (bad translations are worse than English).
- **#76** float treatment — lang-picker + group-config are already in
  `set_suppressed`/`any_popup_open`; only the per-pane agent-finished card is outside
  that system (different mechanism). Mostly addressed.

## Notes
- All captures used leak-safe fictional data (`TD_GRAVEYARD_DEMO`, `TD_SAVINGS_DEMO`)
  on a throwaway Xvfb `:2`, never your real session. `docs/media/graveyard-cards-redesign.png`.
- The real-data screenshots Parker pasted live only in `~/.claude/image-cache/`; none
  were uploaded to any issue (the "don't make a git issue for the screenshots one").
