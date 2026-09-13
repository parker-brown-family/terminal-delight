# Handoff — chrome skin layer (2026-09-12)

## Status

**Landed and live, not merged.** Branch `chrome/skin-tokens` at `ffe15b4`, pushed,
working tree clean, **PR #405** open (15 commits). Installed as
`td-ffe15b4-skin-tokens` and the live session is running it — session 1, 22 panes,
cut over four times today.

## What's done

A third themeable axis — **shape** — beside the palette (colour) and effects
(texture) that were already data.

| | verified by |
|---|---|
| `app/src/skin.rs` — 27 ink recipes, 14 metrics, 6 strategies, 16-verb vocabulary | 802 tests (21 new), clippy `-D warnings` + fmt clean |
| `app/skins/{default,deco,console}.toml` — hot-reloaded like themes | `every_builtin_skin_parses_with_no_unknown_tokens`; `no_two_builtin_skins_resolve_to_the_same_shape` |
| `app/themes/deco.toml` — brass/ivory/jade palette, names its skin | `embedded_themes_parse_with_distinct_ids_and_icons` |
| SKIN row in the OUTER design tray | painted live under each skin via `TD_TRAY_DEMO=1`, zero panics |
| `ctl skin <name>\|theme\|status` — restyles a running window | driven against a real window: default → deco → console → theme, plus a refused misspelling |
| `skin --list` / `skin --skin X --theme Y` (JSON) | `the_probe_prints_every_shape_strategy` |
| Six surfaces converted (bar, strip, left bar, rows, slot, pane header/frame, workspace body) | `the_always_visible_chrome_takes_its_corners_from_the_skin` |
| Phosphor ring, left-bar counts removed, selection off the accent | guards mutation-tested; four live cutovers |

## How to run / verify

```
cd /home/parker/Work/td-skin/app
```
```
CARGO_TARGET_DIR=/home/parker/Work/.td-skin-target cargo test --locked
```
```
CARGO_TARGET_DIR=/home/parker/Work/.td-skin-target cargo clippy --all-targets -- -D warnings
```

Look at a skin without touching a live window — this is the paint check, and it is
the only verification available for gpui chrome without a screenshot:

```
TD_SCRATCH=1 TD_TRAY_DEMO=1 TD_SKIN=/home/parker/Work/td-skin/app/skins/deco.toml terminal-delight
```

Find that window's pid **by environment, never by cmdline or `comm`**:

```
for p in $(pgrep -f 'bin/terminal-delight$'); do tr '\0' '\n' < /proc/$p/environ | grep -q '^TD_TRAY_DEMO=1' && echo $p; done
```

Read a skin back:

```
terminal-delight skin --list
```
```
terminal-delight skin --skin deco --theme hacker
```

**Cutover recipe** (scripts in this session's scratchpad; guards are in
`preflight.sh`, the bounce in `cutover.sh`): gridwire identical between host and
new client → client holds 0 ptys → separate cgroups → census of pane shells by
parent shows 0 window-owned → back up `~/.config/terminal-delight/sessions/1.toml`
→ record `list_panes` → **SIGTERM** the client (never KILL) → relaunch via
`hyprctl dispatch 'hl.dsp.exec_cmd("env -u CLAUDE_* … TD_SESSION=1 terminal-delight")'`
→ diff the agent resume lines. The MCP relay drops for already-running agents
(#391); reach the window over `ctl` after.

## Not done / next

- **#408 — the overlays and the theme tray are still round on a square skin.** 55
  bare `rounded_*()` calls. The last thing between this and finished.
- **#409 — this session wrote no transcript** (`CLAUDE_CODE_CHILD_SESSION=1`), so
  `cdx` harvest and `cdx-audit` were impossible at tie-off.
- **#410 — `skin --list` panics on a closed pipe** (SIGPIPE).
- **#406 — `SKIN` and `ANCHOR` tray headings are untranslated literals.**
- **#407 — chamfers, parked loudly** at Parker's request.
- **Waiting on Parker, not on code:** whether the bloom (`glow` / `glow_a`) reads
  right after a session of use, and which skin is the house look. Both are TOML,
  hot-reloaded in 300ms.

## Watch out

- **Build from `~/Work`, never from `~/BROWN-FAMILY-SPORTS/Software/terminal-delight`**
  (that is the `td-glyph` fork). `~/Work/terminal-delight` is *also* stale — on
  `shorts-pipeline`, 135 behind. `~/Work/td-cutover` is the tree on `main`.
- **gpui chrome cannot be screenshotted from an agent shell.** Verify by painting
  (above) and by what the process writes back.
- **Three verification harnesses lied in this session.** `pgrep -x` cannot match
  `terminal-delight` (comm truncates at 15 chars); `pgrep -f` on an env var stops
  matching once `env` execs; and a mutation harness inverted its verdict because
  cargo prints `error: test failed` *after* a failing test. Check the instrument
  before believing a negative.
- **Corners/borders/rules do not take the UI scale. Lengths do.** There is a test.
- The `default` skin is no longer a pure transcription of the old chrome — its
  `emphasis` is the ring, deliberately. The golden test names that as the one
  intended departure.

## Where it's recorded

- APES episode: `apes/projects/terminal-delight/episodes/2026-09-12-chrome-skin-layer.md`
- Gates: `docs/plans/chrome-skin/` (post-hoc difficulty **6**, opened at 8)
- Brief (annotatable, carries Parker's notes): `reports/2026-09-12-chrome-skin-layer.html`
- lean-ctx: session `decision` + one `finding`
- file-memory: `the-chrome-has-a-skin-axis`, `convert-the-shape-half-first`,
  `a-data-layer-needs-a-read-back-verb`,
  `small-things-cannot-be-judged-at-a-comfortable-size`,
  `a-guard-that-names-call-sites-guards-only-those`
- PR #405 · issues #406 #407 #408 #409 #410
