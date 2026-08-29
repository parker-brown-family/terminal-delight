# PINLOG — the zed_rev pin, managed

`app/Cargo.toml` pins gpui to one Zed commit (`zed_rev`), prepared as a sibling
`zed-upstream/` checkout by `scripts/prepare-gpui.sh`, carrying the five-patch
stack in this directory. This file is the pin's ledger: every bump gets an
entry, and the cadence/trigger policy lives here so the pin always has an
owner. Background: the 2026-08-29 foundation interrogation
(`docs/2026-08-29-foundation-interrogation-zed-gpui-quickshell.md`, §2/§4) and
issue #179.

## Why a bump is not scary

A bump is one rebase in a throwaway worktree, gated before it ships:

1. `git -C zed-upstream fetch && git checkout <candidate-rev>` (a sibling
   checkout — the working build's checkout is untouched).
2. Re-apply the five patches; rebase whichever ones conflict (the CRT pass is
   the likely one — it lives in `gpui_wgpu`, where upstream moves).
3. `cargo test` + `scripts/release-smoke.sh` (fmt, clippy, tests, deny,
   AppImage). Red anywhere → stay on the old pin, file what broke, no harm
   done. Green → update `zed_rev`, commit, PR.

We never ship a red bump, and the old pin keeps working the whole time. The
risk isn't bumping — it's NOT bumping until a forced bump (security fix,
driver workaround) has to cross a year of drift at the worst moment.

## Cadence and triggers

- **Scheduled:** quarterly, or sooner when the drift watcher
  (`.github/workflows/zed-pin-watch.yml`, weekly) reports more than
  16 weeks / raises its issue.
- **Off-cycle triggers:** a gpui/wgpu security advisory · a renderer or driver
  fix TD needs · an `alacritty_terminal` release TD wants that needs newer
  gpui · Zed publishing `gpui_platform` + a post-process seam to crates.io
  (that one triggers a re-evaluation of the pin strategy itself — see the
  interrogation's §4 option C).

## Ledger

| Date | zed_rev | Zed state at pin | Patches | Notes |
|---|---|---|---|---|
| 2026-06-12 | `abbe85a3321bf6cb7f5b241e623d9c2e16c29187` | post-wgpu Linux renderer (PR #46758), pre-1.0 → Zed 1.0 shipped 2026-04-29 | 5 (td-crt-pass, focus-blur, sever-gpl-crates, text-crawl, warp-tube-cap-32) | The founding pin (G0a day). Known drift banked upstream since: wgpu fork → upstream `wgpu 29.0.4`, taffy `=0.10.1` → `=0.13.0`, accesskit landed, WebGL backend landed 2026-08-04. First bump crosses these — budget accordingly. |
