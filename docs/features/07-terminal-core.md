# Terminal core — a real, fast terminal first

Before any of the agent magic, Terminal Delight is a genuine terminal: real PTYs,
full VT emulation, tabs, tiling, session restore, find, selection — at
Alacritty-comparable latency. The agent layer is earned on top of a solid terminal.

## Why it matters

An agent HUD is worthless if the terminal underneath is slow or wrong. TD runs vim,
htop, tmux, and git unmodified, resizes correctly, and keeps key→photon latency in
the same class as Alacritty.

## Features

| Feature | What it does | Evidence | Binding / flag |
|---|---|---|---|
| **Real PTY + VT** | Bash/vim/htop/tmux/git unmodified; full ANSI (16 themed + 256 + truecolor, bold/underline/inverse/dim) | `alacritty_terminal::tty` + EventLoop; `term.rs` | — |
| **Headless correctness matrix** | Parser-level tests: wide-char/emoji width, alt-screen, mouse modes, bracketed paste, scrollback, OSC8/52 | `term.rs` `mod correctness` (PR #141) | `cargo test` |
| **Live resize → SIGWINCH** | Grid resize syncs PTY immediately; tput confirms | `Session::resize` | — |
| **Tiling-tree splits** | Hierarchical splits; each split divides only the focused pane; up to 8 panes; drag panes between splits | `Tree<L>`, `split_leaf` | `ctrl+alt+r` / `ctrl+alt+d` |
| **Tabs + groups** | Multi-tab; per-tab name/pin/🔔; drag-to-reorder; **coloured tab groups**; rich rename | tab model | `ctrl+shift+t`, `ctrl+pgup/dn` |
| **Sub-tab drag-to-split** | Drag a tab onto a pane divider → split there | drop handler | drag |
| **Find / search** | Fuzzy search over scrollback; ↵ jumps + scrolls + selects; highlights all matches | fuzzy_match + search_grid | `ctrl+f` / `ctrl+shift+f` |
| **Selection** | Mouse char/word/line + shift-extend + auto-scroll on drag; **keyboard** shift-arrow (char) / shift-ctrl-arrow (word) | `Selection`, `kbd_sel` | mouse / shift-arrows |
| **Copy / paste** | `ctrl+shift+c/v`, bracketed paste, right-click menu; **X11 PRIMARY** (middle-click) | gpui clipboard + `select_to_copy` | — |
| **Session restore** | Reboot panes with cwd + agent resume command after crash/close; atomic owner-only `state.toml`; 30 s checkpoint | `session.rs` | automatic |
| **Window pop-out** | Drag a tab out → detached scratch window with its own session | renamed-binary scratch detect + flock | drag-out |
| **Frameless window (CSD)** | No OS titlebar; app draws its own frame, shadow, resize edges, rounded corners (conditional on tiling) | `csd.rs` | — |
| **Pane focus, no jiggle** | Focus is paint-only (border/shadow), never layout — zero reflow; +15% phosphor halo | constant geometry | `alt+←/→` |
| **Agent-turn navigation** | Jump between *your* turns in a Claude/Codex pane | `human_input_rows` | `alt+↑/↓` |
| **Close with confirm** | `ctrl+w` always confirms; last pane closes app | dialog | `ctrl+w` |
| **Agent-finished bell** | Sound + dismissable card when an agent finishes; per-pane; trim/loop/volume | `bell.rs` | click to ack |
| **Desktop hotkey** | GNOME `ctrl+alt+t` launches TD (reversible installer) | `scripts/install-hotkey.sh` | install script |
| **Latency probe** | `TD_LATENCY=1` → key→echo→parse p50 121 µs / p99 169 µs; `seq 1 100000` in 0.089 s | instrumentation | `TD_LATENCY` |
| **Ctrl+L / TERM** | Child shells get a correct `TERM` via `tty::setup_env()` so clear works | `term.rs` | `ctrl+l` |

## Added since the first catalog pass (2026-09)

| Feature | What it does | Evidence | Binding / flag |
|---|---|---|---|
| **Alt-held copy affordance** | Hold Alt and the logical line under the pointer is framed; click and it lands on the clipboard **with the wrap seams already healed** — the reconstructed logical line, not the painted rows | `pane.rs::CopyHint` + the affordance resolver | hold `alt`, click |
| **Deliberately strict: commands only** | v1 offers a chip only on lines that are a thing you *run*. Offering one everywhere is never wrong, but it makes the pane restless, and the chip's whole value is that its presence means "this is runnable". JSON and prose are out of scope | `pane.rs` affordance predicate + command allowlist | — |
| **Right mouse button** | The pane had never registered the right button, so every branch testing for one was dead code; fixing it lit up the note pin and the paths depending on it | #270 | right-click |
| **Desktop notifications, omarchy-shaped** | An agent ringing while you are looking elsewhere posts `tab → pane` plus a recap of its last words through `notify-send`; clicking it ZIPS you to that window and pane | `app/src/notify.rs` | automatic |
| **Sticky notes** | `alt+s` to post, `alt+backspace` to peel, right-click to pin — [own page](10-sticky-notes.md) | `sticky.rs` | `alt+s` |
| **Σ usage panel** | Per-subscription spend — [own page](11-usage-vitals-keepalive.md) | `usage.rs` | `ctrl+shift+u` |
| **Scriptable tabs** | Tabs renamed and organised from outside the running app — [ctl](12-ctl-scripting.md) | `ctl.rs`; #304, #305 | `terminal-delight ctl tab` |

## Architecture note

GPU substrate is **gpui** (consumed from a pinned Zed checkout with the wgpu Linux
renderer + the CRT patches); the PTY seam is `alacritty_terminal` (Apache-2.0,
clean-room — Zed's own terminal crates are study-only, never copied). See
[packaging](09-packaging.md) for the licence boundary.

## Status

**Shipped.** Rigorous latency rig + 20-pane stress + multi-GPU/Wayland validation
are the remaining 1.0 hardening items (milestone `1.0`, issues #137–#139).
