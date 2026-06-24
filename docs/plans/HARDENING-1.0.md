# Terminal Delight — 1.0 Hardening Plan

**Thesis:** The *feature* North Star is already met (tabs, full tiling, true CRT
shaders, per-card warp, themes, i18n×9, FOCUS reader, find, the agent-wall + MCP
control surface). What 1.0 still owes is **proof and durability**, not new
features. This plan turns "how far from 1.0?" from a guess into a burndown you can
watch.

Tracker: **GitHub milestone [`1.0`](https://github.com/parker-brown-family/terminal-delight/milestone/1)** (12 issues).

---

## Scope boundary (write it down so it stops drifting)

**The agent-wall is a read-only HUD, not an orchestrator.** It tails MCP / panes,
parses status lines, and shows state/timers/tokens/cost for agents running
*elsewhere*. Orchestration lives in APES / chorus / harnesses. TD's defensible,
unique position is *the terminal you watch your fleet through* — do not rebuild a
workflow engine inside a renderer. Dashboard feature-polish (#80/#82/#83/#84) is
moat work, tracked **outside** the 1.0 milestone so it cannot inflate the 1.0 bar.

---

## The 1.0 bar (from docs/PLAN.md §1) → status

| North-Star criterion | 1.0 bar | Status |
|---|---|---|
| Snappy like native | key→photon within +2 ms of Alacritty, rigorously instrumented | ⚠ crude only (p50≈121µs, one box, no A/B) → **#137** |
| 20 terminals, one window | 20 PTYs, focused ≥110 FPS, RAM < Tilix | ❌ never measured → **#138** |
| The look is free | FX ≤1 ms GPU frame time, 0 added latency | ✅ CRT-lite + shaders landed |
| Polished like a web app | blind A/B reads top-tier | ✅ distinct identity; full bar = polish pass |
| Modify on the fly | hot-reload from data files | ✅ |
| Open & shareable | one-command install; portable themes | ⚠ AppImage ✅ / Flatpak ❌ **#140**; one box only **#139** |
| **Terminal correctness** (implicit) | emulation matrix proven | ❌ zero parser tests → **#136** |

---

## Burndown — the 12 milestone issues, classified by AFK-safety

Each item is tagged for an **unattended overnight agent**:
`[GREEN]` land tonight (code + test, no display) ·
`[AMBER]` candidate branch + writeup tonight, **human visual-verify before merge** ·
`[RED]` needs Parker at a real display / specific hardware.

| # | Item | Class | Notes |
|---|---|---|---|
| **#136** | R4 terminal-correctness test matrix | **GREEN** | 197 tests today are all pure-logic/headless; drive `Term` via the notifier seam, inspect grid flags / `term.mode()`. 40–60 new tests. Highest value, lowest risk. Hook: `app/src/term.rs`; pattern ref `pane.rs:1942-1991`. |
| **#127** | alt-screen guard for inverted anchor-top | **GREEN** | 2 surgical edits (`pane.rs:3989` render, `pane.rs:1656` FOCUS mirror): `&& !term.mode().contains(TermMode::ALT_SCREEN)`. Pure unit test on the gate. Box-drawing fix is *visual* → flag for verify, but logic lands safely. **This is the one true regression in the pile.** |
| **#76** | float treatment on 3 popups | **GREEN** | lang-picker `main.rs:4213`, group-config `main.rs:12033`, agent-finished `pane.rs:4625` use bare `.shadow(vec![…])`; swap to the `float_shadows()` + `border_2` pattern. Pure UI. |
| **#75** | i18n the dashboard + 🪦 recover strings | **GREEN** | ~6–8 hardcoded literals (`main.rs:11135` "DEAD AGENTS", `11081` "RESURRECT", `11150`…) → add fields to `lang.rs` `Strings` (all 9 langs), route via `current().strings()`. Compile-checked completeness. |
| **#87** | FOCUS reader cannot drag-select | **AMBER** | Selection plumbing exists (`focus_cell_at`/`focus_sel_drag`/`copy_focus_selection`); likely the scrim `.occlude()` (`main.rs:12454`) swallows mouse-move. Agent instruments + adds `TD_FOCUSDEBUG`, proposes fix; highlight-during-drag needs eyes. |
| **#90** | startup self-heal: offer richer backup | **AMBER** | Backups already rotate (`main.rs:1041`); boot path does not offer restore. Add size-delta check + candidate modal behind a flag; modal UX needs verify. |
| **#137** | rigorous latency rig vs Alacritty | **AMBER→RED** | Author the same-box A/B harness AFK; the *measurement* needs a real PTY echo + reliable key injection on `:1` (with Parker). |
| **#138** | 20-pane stress proof | **AMBER→RED** | AFK: generate 20-pane demo state, sample RSS, screenshot, compare vs Tilix RSS. RED: ≥110 FPS frame-time needs a small emit-patch or perf sampling on a real display. |
| **#140** | Flatpak packaging | **AMBER** | Author manifest + build script + dry-run AFK; human verifies the bundle actually launches/renders (GPU runtime perms). |
| **#88** | tall-pane warp hit-test drift | **RED** | Needs `TD_HITDEBUG=1` interactive repro on `:1` (tall high-curve pane + clickable link). Suspects: stale `self.warp_k` vs live `theme::warp_coeffs`, or `grid_pad` corner-overscan approx. Agent can write the debug instrumentation only. |
| **#23** | acknowledge bar flashes | **RED** | Intermittent; no repro steps. Needs manual observation + capture. |
| **#139** | multi-box / Wayland / non-NVIDIA | **RED** | Validation, not code. Deliverable = the run checklist + capture script; the runs need hardware. |

---

## Sequencing

1. **Tonight (GREEN, unattended):** #136 → #127 → #76 → #75. Each in its own
   worktree, gated on local CI, opened as a PR for morning review.
2. **Tonight (AMBER, candidate-only):** #87, #90, #140 manifest, #137 harness
   script, #138 RSS harness — produce branches + writeups, **do not** open
   auto-merge PRs; tag `needs-visual-verify`.
3. **With Parker (RED + AMBER verify):** #88, #23, #139, the latency/FPS
   measurements, and the visual sign-off on every AMBER branch.

## Definition of 1.0-ready

- #136 green (correctness matrix passing in CI) **and**
- #127 merged (no alt-screen regression ships) **and**
- #137/#138 produce committed *numbers* (even modest) **and**
- #139 boots clean on ≥1 Wayland + ≥1 non-NVIDIA box **and**
- #76/#75/#87/#90/#23/#88 closed, #140 (Flatpak) shipped.

macOS (#124) is **explicitly 1.1** — it does not gate 1.0.

When #136/#127 land + #137/#138 have any real numbers, cut **`v0.3.0-rc`** and let
it bake while the RED items clear.
