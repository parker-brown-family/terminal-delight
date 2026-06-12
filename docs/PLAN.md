# terminal-delight — Build Plan v2 (post-critique)

> A **tabful, tiling Linux terminal manager** — 2, 5, or 20 real shells in one window —
> that **runs as snappy as a native terminal**, **looks as polished as a top-tier web app**,
> can be **modified on the fly** (themes, shaders, features at will), and is **open source
> and shareable** with other Linux users.

v1 was an architecture argument hardened against critique. v2 incorporates that critique's
verdict: *right soul, wrong first target.* The build now aims at the **smallest real version
that already feels like this product** — not a generic terminal prototype, and not the
20-pane monster. Architecture decisions stay; sequencing, gating, and risk posture change.

---

## 1. North Star → measurable acceptance criteria (unchanged, now staged)

These remain the 1.0 bars. **MVP 0.1 is NOT judged against all of them** — each criterion
lists the milestone where it starts being enforced.

| Pillar | Acceptance criterion | Enforced from |
|---|---|---|
| **Snappy like native** | Focused-pane keystroke→photon within **+2 ms of Alacritty**, same box, instrumented | 0.1 (crude instrumentation) → 1.0 (rigorous) |
| **20 terminals, one window** | 20 live PTYs, one streaming; focused ≥110 FPS, latency unchanged; RAM < Tilix | 1.0 (5 panes at 0.2) |
| **The look is free** | Visual effects add ≤1 ms GPU frame time, 0 added latency frames | 0.1 for CRT-lite; 0.4 for true shaders |
| **Polished like a web app** | Blind A/B reads top-tier; bar = Zed-grade | 0.1: "distinct visual identity"; 1.0: full bar |
| **Modify on the fly** | Theme/config changes hot-reload from data files, no recompile | **0.1 (day one — moved up per critique)** |
| **Open & shareable** | One-command install; themes are portable data files | 0.2: packaging smoke test; 1.0: full |

---

## 2. Architecture (decisions affirmed, seams sharpened)

```
terminal-delight (single Rust process, MIT)
├─ GPUI ─ GPU-rendered chrome + tiling + panes (one render model)
├─ pane ─ alacritty_terminal (VT/ANSI, grid, scrollback) ─ PTY ─ bash
├─ theme ─ hot-reloaded data files (palette/font/effect dials) — DAY ONE
└─ visuals ─ CRT-lite first (primitives); true post-process shader = research branch
```

- **Substrate: GPUI** — *great candidate, not settled foundation* (pre-1.0, breaking changes
  expected). Treated as a bet with explicit gates, not proof. Zed proves GPUI can host a
  polished terminal-like surface; it does **not** prove our product (20 independent panes,
  custom post-processing, live shader reload, standalone packaging). We stop overclaiming.
- **GPUI consumption: DECIDED (R1 resolved) — git main, pinned rev; NOT crates.io.**
  crates.io `gpui` 0.2.2 (Oct 2025, stale) uses the **blade** Linux renderer with a long
  NVIDIA+X11 breakage record — our exact box. Zed merged **PR #46758 (Feb 2026): Linux
  renderer reimplemented on wgpu**, explicitly to fix that — but it's git-only (new
  `gpui_platform` crate, not yet published). So:
  `gpui = { git = "https://github.com/zed-industries/zed", rev = "<pin>" }` +
  `gpui_platform` w/ `x11`,`wayland`,`font-kit` features. Optimus laptop caveat: keep
  `ZED_DEVICE_ID` / forced-X11 workarounds handy; "wgpu fixes our box" is unproven until
  G0a runs — which is exactly what G0a is for.
- **Emulation: `alacritty_terminal`** (Apache-2.0 ✅) — Term/Grid/event_loop/tty modules;
  the hard core we must not hand-roll.
- **PTY seam: DECIDED (R2 resolved) — Option A: `alacritty_terminal::tty` + `EventLoop`.**
  The crate ships the full I/O runtime: `tty::new()` spawns the shell; `EventLoop` owns the
  reader thread + VTE parser pump + writer, auto-routes `Event::PtyWrite` query responses back
  to the PTY (which manual `portable-pty` glue would have to re-derive, with subtle correctness
  bugs); control via `Msg::Input`/`Msg::Resize`/`Msg::Shutdown`. Same pairing as Alacritty
  itself, Zed's terminal, and MIT-licensed `iced_term`. ~50 lines of glue vs several hundred.
  `portable-pty` only re-enters if we ever need exotic PTY backends (ssh/serial).
- **License strategy: CONFIRMED (R3 resolved), repo stays MIT.**
  `gpui` 0.2.2 on crates.io = Apache-2.0 ✅ (no GPL anywhere in its published dep tree) ·
  `alacritty_terminal` 0.26 = Apache-2.0 *only* (not dual-MIT; fine one-way into MIT) ·
  **Zed's `terminal`/`terminal_view` crates = GPL-3.0-or-later → STUDY ONLY, never copy.**
  Clean-room rule: architectural facts may be learned from Zed (e.g. "wrap `Term` in
  `Arc<FairMutex>`, forward `EventListener` events over an async channel into the UI, send
  `Msg::Resize` on bounds change"); function bodies/identifiers/structure-level transcription
  may not. Write the seam from docs.rs API docs (Apache), not with Zed source open.
  Obligations: ship `THIRD-PARTY-LICENSES` (cargo-about) in binaries; `cargo deny check
  licenses` in CI with MIT/Apache/BSD-class allowlist.

### Visuals: two tracks (de-risked per critique)
- **Track 1 — CRT-lite (MVP):** palette + glow-like primitives + scanline overlays + strong
  typography, built from GPUI's existing paint primitives. Disable-able. No custom shader hook
  required. This already delivers a distinct visual identity.
- **Track 2 — true post-process shaders (0.4, research branch):** R1 verdict is in:
  **FORK-NEEDED** — gpui exposes no public post-process/custom-shader API (discussion #45996 has
  no committed hook). But the patch shape is known and **low-moderate** (~1–2 days of wgpu work):
  in `gpui_platform`'s wgpu renderer, retarget the scene pass to an offscreen texture, then a
  fullscreen-triangle WGSL CRT pass (scanlines/mask/curvature, effect-dial uniforms) into the
  swapchain. Carried as a small patch on the pinned rev (Apache-2.0 → MIT-compatible); rebase
  tax is the ongoing cost. Also: engage upstream on #45996 — a public hook may yet appear.
  This stays out of MVP; CRT-lite carries the identity until 0.4.

### Fallback ladder (rewritten — was too optimistic)
1. **GPUI + CRT-lite** (no arbitrary shaders; strong built-in effects) ← already the MVP plan
2. **GPUI + minimal rendering fork/patch** — only if a small patch unlocks the shader hook
3. **Raw wgpu** — *last resort, only if GPUI blocks core terminal rendering itself.*
   Acknowledged as a different product cost (layout, text, input, a11y, windowing all inherited).
4. **Ghostty shader/config path** — a parallel *visual research branch*, not a product fallback.

---

## 3. Milestones (replaces v1 phases)

### MVP 0.1 — "two-pane real terminal" ← THE TARGET
One native GPUI app · one tab · **max two split panes** · real shell per pane ·
focus switching, resize, scrollback, copy/paste · **theme-file hot reload** (palette/font/
effect dials) · CRT-lite, disable-able · basic layout restore.

**Success =** 30 minutes of real use without rage-quitting; htop in one pane while vim edits
in the other; resize never corrupts the grid; typing feels close to Alacritty; theme changes
reload live; the app already has a distinct visual identity.
**Test matrix:** bash, vim, htop, tmux, `git log`, one noisy background process.

### Sub-gates to 0.1 (G0 split per critique — facts fast, no shiny-blocks-boring)
| Gate | Proves | Kill/fallback trigger |
|---|---|---|
| **G0a** | GPUI window + keyboard input + fixed-grid text rendering | GPUI can't do basic grid text on this box → ladder rung 3 |
| **G0b** | Real shell through `alacritty_terminal` (PTY seam decision lands here) | Neither PTY option integrates cleanly → re-architect seam |
| **G0c** | Resize, scrollback, copy/paste basics | Grid corruption unfixable → emulation-integration rework |
| **G0d** | CRT-lite rendering path (primitives, not shaders) | Identity unachievable w/ primitives → raise R1 priority |
| **G0e** | Crude latency/perf instrumentation (input→present timestamps) | >2× Alacritty and unfixable → substrate re-evaluation |

### Then
- **0.2** — tabs · up to 5 panes · better session restore · **packaging smoke test
  (AppImage or Flatpak)** — moved early per critique.
- **0.3** — detach pane → own OS window.
- **0.4** — custom shader support lands **iff** R1 proved it (else CRT-lite deepens).
- **1.0** — 20 panes + full latency rig + packaging + theme gallery + public docs.

---

## 4. Research backlog (parallel, never blocking the build)

| # | Question | Feeds | Status |
|---|---|---|---|
| **R1** | GPUI shader/post-process access: public API? fork cost? no-go? | 0.4 / ladder | ✅ **RESOLVED: fork-needed, low-moderate patch** (§2); consume gpui via git main/wgpu, not crates.io/blade |
| **R2** | PTY architecture: `alacritty_terminal::tty`+EventLoop vs `portable-pty` | G0b | ✅ **RESOLVED: Option A** — EventLoop owns reader/parser/writer + PtyWrite echo-back (§2) |
| **R3** | License audit: MIT-clean dependency set; GPL boundary doc | repo setup | ✅ **RESOLVED: MIT viable** — gpui Apache-2.0, no GPL in dep tree; Zed terminal crates study-only (§2) |
| **R4** | Terminal correctness matrix: Unicode width, Nerd Fonts, emoji, alt-screen, mouse modes, bracketed paste, OSC52, hyperlinks | 0.1→1.0 tests | open |
| **R5** | Linux matrix: Wayland/X11 · NVIDIA/AMD/Intel · fractional scaling · clipboard + primary selection | G0a, 0.2 | open (R1 supplied the NVIDIA/X11 + build-deps half) |
| **R6** | Competitive: Ghostty/WezTerm/Kitty/Alacritty/Tilix/tmux — *exactly why does someone switch?* | positioning, 1.0 | open |
| **R7** | Packaging: AppImage vs Flatpak for a GPU Rust app | 0.2 smoke test | open |

---

## 5. Adversarial hardening — v1 register + v2 amendments

v1's nine critiques (C1–C9) and mitigations remain in force where unamended:
C2 damage-tracked/throttled background panes · C3 curvature-as-dial (off in dense tilings) ·
C4 data-vs-code iteration split · C5 budgeted polish phase w/ blind A/B · C6 every milestone
ships a working artifact · C7 per-pane panic isolation + session restore · C9 themes as the
wide, no-Rust contribution path.

**v2 amendments from the second-round critique:**
- **C10 (overclaiming Zed):** Zed = plausibility, not proof. GPUI is a gated bet. *(§2)*
- **C11 (GPL contamination):** explicit MIT strategy + clean-room rule for Zed's GPL terminal
  crates. *(§2)*
- **C12 (PTY under-decided):** named research item R2 with a decision gate (G0b). *(§2, §4)*
- **C13 (Gate 0 too large):** split into G0a–G0e; boring terminal core can't be hostage to the
  shiny problem. *(§3)*
- **C14 (missing MVP):** MVP 0.1 "two-pane real terminal" defined with a usability bar. *(§3)*
- **C15 (hot reload too late):** moved into 0.1 day one so the internal shape is grown around
  it. *(§1, §3)*
- **C16 (shader derail):** CRT-lite ships the identity; true shaders are a research branch that
  must earn its way in. *(§2)*
- **C17 (fallback ladder optimistic):** rewritten; raw wgpu demoted to last resort with its true
  cost stated. *(§2)*

---

## 6. Status — MVP 0.1 SHIPPED (2026-06-12, one session)

| Gate | Result | Evidence |
|---|---|---|
| G0a window+grid+input | ✅ | screenshots; wgpu renderer on X11/NVIDIA |
| G0b real shell | ✅ | bash/ls/top/alt-screen; PtyWrite manual-bounce verified from crate source |
| G0c resize/scrollback/clipboard | ✅ | tput agrees with grid; wheel history; selection round trip |
| G0d CRT-lite + hot-reload themes | ✅ | 3-theme live swap, session preserved; JetBrains Mono installed |
| G0e latency | ✅ | key→echo→parsed p50=121µs p99=169µs; seq 1 100000 in 0.089s |
| **MVP 0.1** | ✅ | 2 panes, vim+top side-by-side (file round trip), tmux+git log, split/focus/close/restore |

Bugs found & fixed by the automated gauntlet: window-vs-pane grid sizing (150→75 cols),
mouse-origin offset in right pane, stray key leak on split (spawn guard), DejaVu Sans
silent font fallback. Known quirk: xdotool synthetic typing races `windowactivate`
(test-harness artifact, not app input path).

Next: **0.2** (tabs, 5 panes, drag splitters, packaging smoke test) per §3.
- Browser demo (`index.html`, `src/`) remains the design reference.
