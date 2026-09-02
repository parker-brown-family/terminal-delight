# Handoff — subscription usage on the agent wall (2026-09-02)

## Status

**Landed, installed and live.** Two PRs merged, `main` at `f633d5e`, clean and
pushed. The binary Parker runs was rebuilt and repointed.

| PR | Squash | What |
|---|---|---|
| #257 | `8948224` | The kiosk opens on Last Voyage, a theme of ours rather than Omarchy's |
| #262 | `393e572` | The agent wall shows what each AI subscription has left, and the binary collects it |

`~/.local/bin/terminal-delight` → `~/.local/lib/terminal-delight/td-393e572-usage`.
Nothing in `.rs` has changed since that sha, so the installed build is current.

## What's done

- **`Σ usage`, the `</>` card's second face.** Per subscription: the plan, a meter
  per limit with the time until each window resets, today, the last seven days,
  tokens by model, and a draining credit gauge for prepaid plans. Shares one
  shell (`main.rs::savings_shell`) with the lean-ctx savings face so the two
  cannot drift apart. *Verified* against real records at 1600×1000 and against
  the fictional set (`TD_USAGE_DEMO`).
- **`app/src/usage.rs`** — the record contract, two-directory discovery
  (newest `updatedAt` wins), reset countdowns, and the collector runner. 16 tests.
- **The collectors ship inside the binary.** Omarchy's three, vendored
  byte-identical under `app/src/vendor/`, compiled in with `include_str!`, run by
  `terminal-delight agent-usage update`. *Verified*: three records written in 1.1s
  by the installed binary, and the panel drew them — including a third Claude
  limit (`Fable Weekly`) that arrives keyed by `title` rather than `label`.
- **🤖 leads the header glyph row**, matching what the narrow collapse already did.
- **The Last Voyage theme** — palette read off the plate, wallpaper at
  `assets/omarchy/bg/last-voyage.webp`, and the footer's licence line corrected so
  the page no longer claims every backdrop is Omarchy's art.
- 456 tests, clippy clean, `cargo fmt` clean, `cargo deny` clean.

## How to run / verify

```bash
cd /home/parker/Work/terminal-delight/app && cargo test
```
```bash
TD_SKIP_APPIMAGE=1 bash /home/parker/Work/terminal-delight/scripts/release-smoke.sh
```
```bash
terminal-delight agent-usage update
```
```bash
TD_USAGE_DEMO=1 TD_SCRATCH=1 /home/parker/Work/terminal-delight/app/target/debug/terminal-delight
```

`TD_USAGE_DEMO` stages fictional subscriptions (capture-safe). `TD_USAGE_LIVE`
opens the same face on this machine's **real** records — development only, never
for a screenshot.

**After any Rust change**, rebuild and repoint or Parker sees nothing:

```bash
cargo build --release --manifest-path /home/parker/Work/terminal-delight/app/Cargo.toml
```
```bash
install -Dm755 /home/parker/Work/terminal-delight/app/target/release/terminal-delight /home/parker/.local/lib/terminal-delight/td-<sha>-<label>
```
```bash
ln -sfn /home/parker/.local/lib/terminal-delight/td-<sha>-<label> /home/parker/.local/bin/terminal-delight
```

Running windows hold the old inode — a restart is required.

## Not done / next

- **`#264` — the `</>` card clips at a tiled window width (~500px).** The live
  one. Two fixes were tried and both failed: a viewport clamp
  (`window.viewport_size()` is not the containing block's width) and blanket
  `min_w(0)` on every row (it collapsed the body to an empty column). **Start by
  determining what box `.absolute().inset_0()` resolves against** for overlays
  attached at the bottom of `render` — `mcp`, `dead`, `plugins`, `help` and the
  rest are attached identically, so this is probably one bug, not one card's.
- **`#263` — verify the collectors on a machine with no Omarchy.** Everything so
  far was checked on a box that has it. The vendored collectors keep upstream's
  `~/.cache/omarchy` and `~/.config/omarchy` paths by design.
- The Fireworks record reports `ready: false` here (no API key), so its chip is
  filtered out of the card. Untested against a funded account.

## Watch out

- **`hyprctl` is a trap on 0.56.2.** `keyword windowrule*` is refused outright and
  the dispatchers moved behind an undiscoverable `hl.dsp.*` Lua namespace, so a
  window cannot be placed by script. Worse, failed probes surface as **user-visible
  error toasts** — one of mine spawned a separate agent to investigate a phantom
  config error. To capture a window: launch it yourself, find it by `pid:` in
  `hyprctl clients`, confirm the rect is fully inside a monitor (a partly
  offscreen window grabs as smeared garbage), then `grim -g "<x>,<y> <w>x<h>"`.
- **The vendored collectors must stay byte-identical.** `cmp` against
  `/usr/share/omarchy/bin/` is the audit and `cp` is the upgrade. Patching one
  turns every future upstream fix into a merge. See `app/src/vendor/README.md`.
- **Sibling worktrees**: `td-reveal` (`build-main`) and `td-themes` — the latter
  had a live process in it at tie-off. `~/Work/terminal-delight` is the flagship
  home; remove a worktree once its branch merges.
- `ctx_read` refuses paths in sibling worktrees of the same repo (root jail), so
  reads there fall back to `ctx_shell sed`.

## Where it's recorded

- Episode: `apes/projects/terminal-delight/episodes/2026-09-02-subscription-usage-on-the-wall.md` (+ `.cdx`)
- Kanban: `show-per-subscription-agent-usage-on-the-wall-collected-by-the-binary-itself-mtkdoy51` (done),
  plus backlog tickets mirroring #264 and #263
- lean-ctx: session decision + two findings (gpui clamping, Hyprland capture)
- file-memory: `merged-is-not-installed`, `dont-probe-hyprland-on-a-live-session`,
  `flagship-work-lives-in-work`
