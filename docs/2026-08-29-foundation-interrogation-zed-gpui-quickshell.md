# Foundation interrogation — the Zed/gpui bet, the recovery story, and the Quickshell/quattro surface

**Date:** 2026-08-29 · **Scope:** re-examine the two assumptions behind terminal-delight's
architecture now that the world has moved: (1) the founding choice to build on **Zed's gpui**
(git-pinned, patched) + **`alacritty_terminal`**, and (2) whether **Omarchy quattro's
Quickshell shell** opens integration opportunities worth building — or doesn't.

This is a decision document. Every recommendation carries the evidence that produced it and
the trigger that would invalidate it.

---

## 0. Verdicts

| # | Question | Verdict |
|---|---|---|
| 1 | Was "build on Zed" right? | **Yes — and it still is.** The bet was placed with gates, paid off on every gate, and no 2026 alternative wins on TD's actual requirements. |
| 2 | Is the frozen pin right? | **No.** `zed_rev abbe85a` (2026-06-12) has never moved. Keep the *strategy*, replace the frozen pin with a **managed bump cadence** with explicit triggers. |
| 3 | Is reincarnation the right recovery model? | **Yes, as the primary — confirmed against the field.** The 2026 standard is two layers (live-PTY daemon for crash/logout + serialize-and-re-run for reboot); TD ships layer 2 only, and that's the right layer for agent panes, whose durable state is the transcript. Harden it with field-proven techniques (§5); stage the daemon behind a real trigger, not ambition. |
| 4 | Does Quickshell unlock optimizations? | **Integration yes, embedding no.** The flagship isn't Quickshell at all — it's quattro's **xdg-terminal-exec default-terminal slot** and **`omarchy agent`** contract, which TD can occupy with one S/M-sized feature. Bonus finding: the plugin surface is bigger than we recorded — six kinds, multi-file; the "one-file limit" was a broken error path (basecamp/omarchy#7418), not policy. |

---

## 1. What we actually chose (the receipts)

The June decision, from `docs/PLAN.md` §2 (R1/R2/R3, all resolved by gate):

- **Substrate: gpui, consumed from git at a pinned rev — deliberately not crates.io.**
  crates.io `gpui 0.2.2` (Oct 2025) shipped the **blade** Linux renderer with a known
  NVIDIA+X11 breakage record — our exact box. Zed's PR #46758 (Feb 2026) reimplemented the
  Linux renderer on **wgpu**, git-only, via the new `gpui_platform` crate. So:
  `zed_rev = abbe85a3321bf6cb7f5b241e623d9c2e16c29187` as a sibling `zed-upstream/` checkout,
  plus a local patch stack.
- **Emulation: `alacritty_terminal 0.26`** — Option A (the crate's own `tty` + `EventLoop`
  owns reader thread, VTE pump, writer), same pairing as Alacritty itself, Zed's terminal,
  and `iced_term`. Written clean-room against the docs.rs API because Zed's own
  `terminal`/`terminal_view` crates are GPL-3.0 — study-only, never copy.
- **License: MIT source AND MIT binaries.** `0002-sever-gpl-crates.patch` cuts the
  `gpui → sum_tree → ztracing/zlog` GPL edge; `cargo deny` enforces zero GPL with no
  exceptions; the AppImage ships a `cargo about` notice bundle.

The bet was explicitly hedged — PLAN.md calls gpui "*great candidate, not settled
foundation*" — and de-risked through five kill-gates (G0a–G0e), all passed on 2026-06-12,
MVP same day.

**What the bet bought, measured:** the per-pane CRT barrel-warp pass (the product's visual
identity — impossible on crates.io gpui, the entire reason for the fork), key→echo→parsed
latency **p50 121µs / p99 169µs**, `seq 1 100000` in 0.089s, tiling tree well past 5 panes,
hot themes in ~300ms, MIT-clean AppImage. The North-Star criteria the substrate was chosen
against are being met by it.

## 2. What the bet costs today (measured)

**The patch stack is small and stable.** Five patches, 1,080 lines total — and the README
still says two (fixed in this PR):

| Patch | Lines | What it is |
|---|---|---|
| `0001-td-crt-pass` | 458 | The per-pane CRT barrel warp — the reason the fork exists |
| `0002-focus-blur` | 193 | Focus/blur events for pane dimming |
| `0002-sever-gpl-crates` | 108 | MIT-clean binaries (note: duplicate `0002` number — cosmetic wart) |
| `0003-text-crawl` | 194 | Star-Wars crawl mode's perspective hook |
| `0004-warp-tube-cap-32` | 127 | Warp instance cap |

**The pin is frozen, not managed.** `abbe85a` is dated 2026-06-12 — the day of G0a — and has
never been bumped. That was the right posture while shipping 0.1→0.4 in a sprint; eleven
weeks later it's drift with no owner: every week of Zed development widens the gap the
*eventual* forced bump (security fix, Vulkan driver workaround, font-kit fix) must cross at
the worst possible time. Zed ships weekly (1.17.2 as of 2026-08-26 — roughly eleven
releases past our pin); measured drift at the dependency level already includes a taffy
major-pin move and the renderer's wgpu-fork → upstream-29.0.4 migration (§3). The right
time to learn the bump ritual is while the delta is eleven weeks, not fifty.

**Coupling is asymmetric — and that's good design to preserve.**

- `alacritty_terminal` is contained: one clean-room seam (`app/src/term.rs`, 861 lines
  including its synchronous VT-correctness suite), seven import sites. Swappable in
  principle; no reason to swap.
- gpui is everywhere: ~35k lines of app code written in gpui idiom (`Div`, `Hsla`,
  entities, `canvas`). Replacing gpui = rewriting the UI. This is the real lock-in, and it's
  the acceptable kind: it's the render model the product is made of.

**Packaging friction lands on the pin.** #162 (deps script is Ubuntu-only), #165 (AUR
packaging) both get harder because a PKGBUILD must reproduce the sibling-clone + patch
ritual against a fixed rev. Not a blocker — `prepare-gpui.sh` already is that ritual — but
the AUR work should treat the pin as an input, which is another argument for the pin having
an owner and a changelog.

## 3. What changed upstream since June

**Zed / gpui** (checked 2026-08-29, primary sources — Appendix B):

- gpui *is* on crates.io — 0.2.2, frozen since 2025-10-22, still shipping the blade-era
  Linux renderer; `gpui_platform` was never published, so even the in-repo README's
  crates.io instructions are unsatisfiable today. Everyone real rides git — gpui-component
  (13.6k stars, the ecosystem anchor) tracks zed main *unpinned*. TD's pin-a-rev + sibling
  checkout + patch script is the conservative variant of the standard practice, not a hack.
- The wgpu Linux renderer TD bet on (PR #46758, merged 2026-02-13, community-authored) is
  now **actively maintained by Zed staff** — commits to `gpui_wgpu` through 2026-08-25.
  The blade-era freeze class is gone; the current pain area is adapter selection
  (hybrid-GPU "Invalid surface", #52517). Since our pin, main moved off Zed's private wgpu
  fork onto upstream `wgpu 29.0.4` — a real, bounded rebase surface for `td-crt-pass`.
- Measured churn across our eleven-week gap: taffy `=0.10.1` → `=0.13.0`, accesskit
  landed, a WebGL backend landed 2026-08-04. Zed the company is healthy — 1.0 shipped
  2026-04-29, weekly releases, $32M Series B — but hold one cultural signal: in Feb 2026
  Zed explicitly deprioritized *community* gpui work for the year. gpui remains what
  PLAN.md called it: a great candidate that is nobody's product.
- Two escape hatches exist now that didn't in June. **`gpui-unofficial`**: an automated
  crates.io republish tracking every Zed release, wgpu renderer included — useless to us
  directly (a registry dep can't carry our patches) but a perfect canary for "could we
  drop the sibling clone." **`gpui-ce`**: an active community fork, semi-blessed by Zed
  devs for exactly the feature class Zed rejects — custom shaders — i.e. the natural
  upstream for `td-crt-pass` if we ever want the patch stack to become someone else's diff.

**Emulation (the contained seam):** `alacritty_terminal` 0.26.0 (2026-04-06) is the
**current latest release** — TD is not behind at all. Churn is tiny (0.25→0.26: one
options rename, one Windows arg, one event-payload change), cadence steady at ~2/year,
1.3M downloads, the broadest-consumed VT crate in Rust. Two young challengers appeared:
**`rio-vt`** (2026-07-27, MIT, VT + PTY + grid *plus sixel/kitty/iTerm2 image protocols*,
explicitly positioned as the modern alacritty_terminal alternative) and **`libghostty-vt`**
(Ghostty's extracted Zig core; Rust bindings at 0.2.1, API explicitly unstable). Neither
justifies a swap; both are watch items — image protocols are the incumbent's one gap.
WezTerm, the other 2024-era candidate, is confirmed stalled (no stable release since
2024-02, maintainer absent, `wezterm-term` still unpublished).

**The category moved.** "Terminal that manages AI-agent sessions" got crowded in 2026:
**Quil** (Go reboot-proof mux whose Claude recovery captures session ids push-style via
per-pane `SessionStart` hooks and replays 500-line "ghost buffers"), **Warp** (agent
management panel, per-agent worktrees, client partially open-sourced under MIT+AGPL),
**cmux** (Swift on libghostty — proof libghostty embeds in production), **claude-squad**,
**amux**, kitty 0.43's native sessions — and, three days before this report, **VS Code's
Agent Host**: agent sessions as authoritative server-side state behind an open protocol.
None of them combines GPU-native rendering, per-pane forensic resume, and a distro-level
home (Omarchy). That is TD's lane; §5 and §6 are about defending it.

## 4. Re-running the ladder — alternatives scored against TD's real requirements

The requirements that actually discriminate (from PLAN's North Star + what shipped):
**(a)** per-pane post-process shader access, **(b)** 20 real PTYs in one window at native
latency, **(c)** MIT-clean distributable, **(d)** hot-reload theming, **(e)** Linux-first
Wayland+X11, **(f)** agent-pane semantics (resume capture, message pips, bell).

| Option | Wins | Breaks | When it becomes right |
|---|---|---|---|
| **A. Keep frozen pin** (status quo) | Zero effort now | Drift compounds; forced bump lands unplanned | Never — it's the no-decision decision |
| **B. Keep foundation, manage the pin** ← recommended | Keeps everything that works; makes cost visible + scheduled | A bump ritual — bounded but real, and the first one is the expensive one (it crosses the wgpu-fork → upstream-29.0.4 move and a taffy major, §3) | Now |
| **C. crates.io gpui** | No sibling clone; normal Cargo | Published 0.2.2 froze in Oct 2025 *with the blade renderer* — dead on (a) and on our NVIDIA history; `gpui-unofficial` republishes current Zed with wgpu, but a registry dep can't carry our patches | If Zed publishes `gpui_platform` AND a post-process seam; watch `gpui-unofficial` as the canary |
| **C′. gpui-ce (community fork)** | The one venue that *wants* our patch class (custom shaders) — could turn the 5-patch stack into upstream code | The fork lags mainline, where the renderer is the actively-staffed part; two upstreams to track instead of one | Worth one exploratory PR conversation for `td-crt-pass`; a migration only if it lands and the fork keeps pace |
| **D. iced + iced_term** | Published crates, MIT ecosystem | Rewrite of ~35k lines for zero user-visible gain; unproven at (a)/(b) | Only if gpui consumption became untenable |
| **E. raw wgpu + cosmic-text** | Total control of (a) | Re-derive layout/input/text/windowing — the "different product cost" PLAN already rejected | Last resort, as PLAN's ladder says |
| **F. Swap the VT seam (rio-vt / libghostty-vt)** | rio-vt adds the image protocols (sixel/kitty/iTerm2) the incumbent lacks, MIT, on crates.io; libghostty-vt is the fastest parser in class | Replaces the *contained, current-latest* dependency — zero product win today; rio-vt is a month old, libghostty-vt's API is explicitly unstable | If `alacritty_terminal` stalls, or when image support becomes a committed TD feature |
| **G. QML/Quickshell frontend** | Rides Omarchy's toolkit | Loses the one-render-model + per-pane shader identity; QML terminal widgets are legacy-grade | No |

The decisive fact: **every alternative attacks the cheap dependency or rewrites the
expensive one.** The contained seam (`term.rs`) doesn't need rescuing, and the pervasive one
(gpui) is delivering the product's two differentiators — the shader identity and the
latency. Interrogation result: the assumption holds; the *process* around it (frozen pin,
stale README, unowned bump) is what needed fixing.

**Concrete mechanics for Option B** (files an issue, see §7): a quarterly-or-triggered
`zed_rev` bump: rebase the 5 patches, run the existing correctness suite +
`release-smoke.sh`, record the delta in a `docs/patches/PINLOG.md`. Triggers that force an
off-cycle bump: a gpui security advisory, a wgpu/driver fix TD needs, an
`alacritty_terminal` major TD wants, or upstream publishing `gpui_platform` to crates.io
(→ re-evaluate Option C).

## 5. Interrogating "primarily for agent recovery after logout"

**What recovery actually is today** (and it's more forensic than the tagline suggests):
`session.rs` (843 lines, the best-tested module in the app) captures each pane's foreground
process, cwd, and **derives the agent resume command** — binding a pane to its own
transcript by process birth-time windows and open-fd scans, with explicit forgery
resistance (`a_typed_prompt_cannot_forge_the_history_fields`,
`resume_id_must_be_shell_safe`). `recover.rs` scans `~/.claude/projects` for dead agents
the live set doesn't cover. State lands atomically in `state.toml` (0600). The
`feat/per-workspace-sessions` branch is extending this to per-Hyprland-workspace restorable
windows.

**The honest gap:** TD is a Wayland client. On logout it dies and every child gets SIGHUP —
recovery is **reincarnation** (`claude --resume <id>` re-run), not survival. The 2026-08-29
logout incident proved both halves: TD auto-resumed the panes whose leaves carried resume
commands, and the sessions that needed *continuity* got rebuilt on tmux + `hyprctl` by hand.
Reincarnation is lossy in exactly one way: an agent mid-turn loses the in-flight turn (the
transcript survives; a session that never wrote a transcript is unrecoverable).

**Options considered:**

1. **Reincarnation++ (recommended):** keep the model; steal the field's best techniques.
   The survey confirms nobody else does per-pane *forensic* resume (birth-binding, fd
   scans) — but Quil does one thing better: it captures the session id **push-style**,
   via a per-pane `SessionStart` hook injected with `claude --settings`, so id rotation
   on `/clear`, `/resume`, and compaction never goes stale. Adopt that (hooks are the
   sanctioned interface and receive `transcript_path`), keeping our forensic derivation
   as the fallback for agents without hooks. Add **scrollback ghosting** — Quil replays
   ~500 lines from binary ghost buffers instantly while shells re-init; VS Code calls the
   same idea `persistentSessionScrollback`. And one sharp edge from the official docs:
   **`claude --resume` does not restore a CLI-flagged permission mode** — it falls back
   to settings defaults. Harmless where settings carry the mode (this box), wrong for
   flag-configured agents: TD should re-assert recorded flags when it re-runs a resume
   command.
2. **TD server split** (the wezterm-mux / iTerm2 / VS Code ptyHost pattern — and, days
   ago, VS Code's Agent Host applied the same shape to agent sessions): PTYs live in a
   display-independent user daemon; the gpui window is a reattaching client. On this box
   the substrate is already free — Arch ships `KillUserProcesses=no` and lingering is
   enabled for this user, so a `systemd --user` daemon survives logout and starts at
   boot; zmx even demonstrates snapshot-on-reattach with a VT core held server-side. It
   is still a rewrite of TD's process model plus a wire protocol for grid state — the
   "massive surface" the June competitive review declined as "conflicts with the
   single-binary local-first model." Stage it behind a trigger: build the daemon when
   mid-turn losses or logout frequency measurably hurt.
3. **Mux substrate** (spawn panes inside tmux/zellij sessions): survival without a rewrite,
   but double-drawn UI (their status chrome under TD's), scrollback ownership conflicts,
   and TD's per-pane identity (themes, warp, agent pips) degrades into "a pretty tmux
   client." Fine as a *user choice* per-pane (it works today — that's how the rescue went);
   wrong as the architecture.

**Verdict:** the product claim should be said precisely — TD is the terminal that
**re-materializes your agent wall** (layout + cwd + resume per pane), not a process
freezer. For Claude/codex agents, whose durable state *is* the transcript, reincarnation
recovers everything that matters. The one improvement worth specifying now is scrollback
capture at death/exit, so the wall comes back with its context visible, not just its
processes relaunched. And if the daemon layer ever earns its trigger, it slots *under*
this model — live PTYs answer crash/logout, reincarnation stays the reboot answer — the
two layers compose, so nothing built now is throwaway.

## 6. The Quickshell/quattro surface — imagine, then check

**What quattro actually is** (verified on this box, omarchy 4.0.1 / quickshell 0.3.1, plus
upstream docs): Omarchy 4.0 "Quattro" (2026-08-14) rewrote the entire shell as **one warm
Quickshell process** — `quickshell -p /usr/share/omarchy/shell` — replacing Waybar, Walker,
Mako, SwayOSD, hyprlock, and polkit-gnome in a single QML application with a
`PluginRegistry` scanning `~/.config/omarchy/plugins/`. Plugins are manifest-driven git
directories with **six kinds open to third parties** (`bar-widget`, `panel`, `overlay`,
`menu`, headless `service`, full `bar` replacement), typed settings schemas, hot reload on
save, and distribution via `omarchy plugin add <git-url>` — backed by a community
marketplace (omarchyplugins.com) listing **zero plugins as of today**.

A correction to our own record: the "one QML entry point or the shell dies" behavior we hit
(omarchy-lab#8) is the *error path*, not the design. Multi-file plugins are documented and
first-party plugins ship them; upstream **basecamp/omarchy#7418** (open, 2026-08-18) is the
broken `Loader.Error` fallback that turns any plugin load failure into a blank bar with the
real error swallowed. Cheap falsification next time we touch the widget: add a second file
to td-palette and `rescanPlugins` (deliberately not run today — no reason to poke the live
shell mid-session). We've already shipped on this surface: the
`brownfamilysports.td-palette` 🎨 widget driving TD's paint mode over the ctl socket. (One
distribution note: the validator forbids symlinks inside a plugin dir, and our widget is
dev-linked — publishing means a real clone.)

Quickshell 0.3.1's toolbox is deep where it matters for us: `Socket`/`SocketServer` (QML
speaks unix sockets natively, both directions), `IpcHandler` with signal listeners
(`qs ipc listen/wait`), `ScreencopyView` (live capture of screens *or individual toplevels*
via hyprland-toplevel-export — explicitly view-only, no input forwarding),
`ToplevelManager` (activate/close other apps' windows), `GlobalShortcut` (plugin keybinds
without touching hyprland.conf), a full `org.freedesktop.Notifications` server with actions
and inline reply, `WlSessionLock`, and a `Greetd` service (DankMaterialShell's dank-greeter
proves shells can extend to the login screen).

Two quattro facts matter more than Quickshell itself:

1. **The default-terminal slot is open plumbing.** `omarchy-default-terminal` writes
   `~/.config/xdg-terminals.list` with a desktop id; every terminal launch
   (`omarchy-launch-terminal`, `omarchy-launch-tui`) goes through **`xdg-terminal-exec`**
   with `--app-id=… -e <cmd>`. Omarchy ships enhanced desktop entries carrying the contract
   keys — for foot: `X-TerminalArgExec=-e`, `X-TerminalArgAppId=--app-id=`,
   `X-TerminalArgTitle=--title=`, `X-TerminalArgDir=--working-directory=`.
2. **quattro is agent-native.** `omarchy agent` launches the default agent
   (claude/codex/opencode/…, each with its own don't-ask spelling) in the default terminal
   under a fixed `org.omarchy.agent` app-id; a first-party **Agents bar panel** shows usage,
   limits, and pace per provider; `omarchy agent crash` feeds a coredump into the default
   agent with a diagnose skill.

### Opportunities, ranked

| # | Opportunity | Size | Why it wins |
|---|---|---|---|
| 1 | **TD speaks xdg-terminal-exec** — accept `-e <cmd>`, `--app-id=`, `--title=`, `--working-directory=`; add the `X-TerminalArg*` keys to `terminal-delight.desktop` | **S/M** — gpui already exposes `set_app_id()` (verified in the pinned checkout); `spawn_in(cwd)` already exists; `-e` = run command instead of shell in the first pane | TD becomes selectable as **quattro's default terminal** (one line in `xdg-terminals.list`; the `omarchy default terminal` setter's case list is a 4-line upstream PR) — and **`omarchy agent` starts landing agents in TD panes**, where the bell, agent pips, and resume capture live. This is the deepest integration available and none of it touches Quickshell. |
| 2 | **Recovery wall in the shell** — a `bar-widget` (+`panel` kind) showing `recover.rs`'s dead-agent count; click → panel of dead sessions → "resurrect" via TD ctl / `omarchy-launch-tui`; TD events as real notifications (the shell owns the freedesktop notification server — actions can carry a "resume" button) | S–M: widget + a `td ctl recover list --json` verb | Surfaces TD's differentiator at the shell level: you *see* the recoverable wall at login before opening a terminal. Complements the first-party Agents panel (usage ≠ liveness). No Quickshell shell we surveyed integrates terminal session-restore — unclaimed territory. |
| 3 | **Agent liveness HUD** — TD publishes per-pane agent state (running/waiting/finished) over the ctl socket; the plugin consumes it natively (QML `Socket` speaks unix sockets directly — no helper daemon) | S (socket verb; the 🎨 widget already proves the pattern) | The bar becomes the agent wall's HUD, and the sidecar shape is the ecosystem's own best practice (DankMaterialShell keeps heavy logic in a Go daemon; TD *is* our daemon). Note: `state.toml [mcp] enabled = false` is the current default — the ctl socket must be on for any of this; decide the default deliberately. |
| 4 | **Agent exposé** — live thumbnails of TD windows via `ScreencopyView` (per-toplevel capture) in a summoned `overlay`: pick a pane/window visually, activate via `ToplevelManager` | M | An agent-wall overview "for free" from compositor capture — no TD rendering work at all. View-only by protocol, which is exactly what an exposé needs. |
| 5 | **Publish `td-palette` to the marketplace** — real clone (validator forbids symlinked internals), `omarchy plugin add` installable | S | omarchyplugins.com lists zero community plugins today. First-mover slot for the TD×Omarchy offering, at packaging-only cost. |
| 6 | **Theme pipeline continuation** — already in flight (omarchy-lab #2/#3/#4, monitor pass v2, td-monitor) | ongoing | Already proven; not new scope from this interrogation. |

### The "maybe not" list — checked and closed

- **Embedding TD inside a Quickshell window:** no, confirmed at the protocol level. Wayland
  has no XEmbed equivalent; `xdg-foreign` only exports a handle for parenting/stacking,
  never for compositing another client's buffers into your scene. The ceiling is
  capture-only mirroring (`ScreencopyView`, no input forwarding) — which opportunity #4
  turns into a feature. The one true loophole — running a *nested Wayland compositor*
  inside a window (QtWayland Compositor, or smithay inside TD) — is real technology and
  wrong for us twice over: in the shell it puts every client inside one crashable process;
  in TD it's a different product (GUI-app panes) that the June competitive review's scope
  logic already declines. Noted as horizon, not plan.
- **Porting TD's chrome (tabs/HUD/trays) to Quickshell:** no. One render model is the
  architecture (PLAN §2) — chrome and terminal share the scene so the CRT pass warps them
  together per pane. The ecosystem agrees from the other side: serious Quickshell shells
  push heavy logic *out* of the QML process into sidecar daemons. TD already is the
  sidecar; moving chrome in would be swimming against both currents.
- **Quickshell as a terminal:** no. `qmltermwidget` is alive (Qt6 port, commits through
  2026-05-31, packaged in Arch extra) but it's GPL-2, CPU-painted, and would run inside the
  shared shell process where a VT crash takes down bar, lock, polkit, and notifications
  together. Nothing there approaches `alacritty_terminal` + gpui, and nothing needs to.

## 7. Actions from this interrogation

**In this PR:** this report; README truth-up (five patches, not two).

**Issues to file (falsifiable, per house doctrine):**
1. *TD implements the xdg-terminal-exec contract* (§6.1) — done when
   `xdg-terminal-exec --app-id=x --dir=y -e cmd` opens a TD window running `cmd` in `y`
   with app-id `x`, and `omarchy agent` with `xdg-terminals.list: terminal-delight.desktop`
   lands in a TD pane.
2. *Manage the zed_rev pin* (§4.B) — cadence + triggers + PINLOG; done when the first
   scheduled bump lands green through `release-smoke.sh`.
3. *Reincarnation hardening* (§5.1) — hook-based session-id capture (per-pane
   `claude --settings` SessionStart hook, forensic derivation as fallback), bounded
   scrollback ghosting, and re-asserting recorded CLI flags on resume (the
   `--resume`-drops-permission-mode edge). Done when a pane killed after a `/clear`
   rotation resurrects to the *current* session with its predecessor's tail visible.

**Deliberately not done:** no pin bump in this session (three feature worktrees in flight —
a bump belongs in a quiet window under its own issue); no `-e` implementation (feature work
under its own ticket, not a docs branch).

---

## Appendix A — evidence log

- Founding decision + gates: `docs/PLAN.md` §2/§3/§6 (R1/R2/R3 resolutions, G0a–G0e).
- Pin: `app/Cargo.toml` `zed_rev abbe85a` = zed commit of 2026-06-12 16:48Z; introduced
  in `60f9849` (2026-06-13) and never changed since (`git log -S zed_rev`).
- Patch stack: `wc -l docs/patches/*.patch` → 458/193/108/194/127 = 1,080.
- Coupling: `grep -c 'alacritty_terminal::'` → 7 sites, all in `term.rs`;
  gpui idiom throughout ~35k lines of `app/src`.
- Latency: README status table (probe p50 121µs / p99 169µs; `seq 1e5` 0.089s).
- Recovery architecture: `app/src/session.rs` (843L), `app/src/recover.rs` (269L), tests
  named in-file; logout incident + tmux rescue: 2026-08-29 session log.
- quattro: `pacman -Q quickshell` → 0.3.1-1; `/usr/share/omarchy/shell/` (shell.qml,
  services/PluginRegistry.qml, plugins/agents/{manifest.json,Panel.qml});
  `/usr/bin/omarchy-{agent,agent-crash,agent-usage-claude,default-terminal,launch-terminal,launch-tui}`;
  foot/alacritty desktop entries with `X-TerminalArg*` keys under `/usr/share/omarchy/`.
- gpui app-id: `zed-upstream/crates/gpui/src/window.rs` `set_app_id` (+ WindowOptions path).

## Appendix B — external findings

### Quickshell / Omarchy quattro (researched 2026-08-29)

- Quickshell 0.3.1 type index: https://quickshell.org/docs/v0.3.1/types/ — PanelWindow,
  WlSessionLock, ScreencopyView (screen + per-toplevel via hyprland-toplevel-export,
  view-only), Toplevel/ToplevelManager, Socket/SocketServer, FileView (watch + atomic
  JSON), IpcHandler (+ `qs ipc listen/wait` since 0.3), GlobalShortcut, NotificationServer
  (freedesktop daemon w/ actions + inline reply), Greetd, Pam, Polkit agent.
- 0.3 release + roadmap (LibQuickshell, shader hot-reload, QML precompile):
  https://outfoxxed.me/blog/quickshell-0-3 · 0.3.1 bugfix release 2026-08-21.
- Omarchy 4.0.0 "Quattro" 2026-08-14, 4.0.1 2026-08-25:
  https://github.com/basecamp/omarchy/releases — shell rewrite replacing
  Waybar/Walker/Mako/SwayOSD/hyprlock/polkit-gnome with one Quickshell process.
- Plugin manual (six kinds, manifest, `omarchy plugin add`):
  https://github.com/basecamp/omarchy/blob/quattro/manual/32-shell-plugins.md and on-box
  `/usr/share/omarchy/shell/README.md` (IPC contract: ping/summon/call/rescanPlugins/…).
- The load-failure bug behind our omarchy-lab#8 experience:
  https://github.com/basecamp/omarchy/issues/7418 (broken `Loader.Error` fallback).
- Wayland embedding impossibility: https://wayland.app/protocols/xdg-foreign-unstable-v2
  (export-for-parenting only); nested-compositor loophole:
  https://doc.qt.io/qt-6/qtwaylandcompositor-index.html.
- QML terminal state: https://github.com/Swordfish90/qmltermwidget (Qt6, alive 2026-05,
  GPL-2, CPU-painted).
- Patterns worth stealing: DankMaterialShell sidecar daemon + dank-greeter
  (https://github.com/AvengeMedia/DankMaterialShell,
  https://github.com/AvengeMedia/dank-greeter); caelestia compiled C++ QML plugin + CLI
  over `qs ipc` (https://github.com/caelestia-dots/shell); noctalia "keybind = ipc verb"
  (https://github.com/noctalia-dev/noctalia). None integrate terminals or session restore.
- Marketplace: https://omarchyplugins.com — zero community plugins as of 2026-08-29.

### Zed / gpui / alacritty_terminal (researched 2026-08-29)

- blade→wgpu Linux renderer PR (merged 2026-02-13):
  https://github.com/zed-industries/zed/pull/46758 · `gpui_wgpu` commits by Zed staff
  through 2026-08-25; main now on upstream `wgpu 29.0.4`.
- gpui on crates.io, frozen 0.2.2 / blade-era: https://crates.io/crates/gpui ·
  `gpui_platform` unpublished (404).
- Auto-republish canary: https://github.com/iamnbutler/gpui-unofficial (tracks Zed
  releases, 1.17.2 current) · community fork accepting shader-class features:
  https://github.com/gpui-ce/gpui-ce · ecosystem anchor:
  https://github.com/longbridge/gpui-component (13.6k stars, pushed today) · catalog:
  https://github.com/zed-industries/awesome-gpui.
- Feb 2026 community-pause signal: https://news.ycombinator.com/item?id=47003569 · Zed
  1.0 2026-04-29: https://www.phoronix.com/news/Zed-1.0-Released · $32M Series B:
  BusinessWire 2025-08-20.
- `alacritty_terminal` 0.26.0 (2026-04-06, current latest):
  https://crates.io/crates/alacritty_terminal · GPL crates TD severs (`ztracing`,
  `zlog`, both GPL-3.0-or-later at our rev) confirmed in-tree.
- Direct gpui-terminal comps: `seance`, `termy`, `zortax/gpui-terminal`.

### Terminal ecosystem + session persistence (researched 2026-08-29)

- Embeddable cores: rio-vt + librio announcement (2026-07-27):
  https://rioterm.com/blog/2026/07/27/rio-vt-and-librio · sugarloaf renderer:
  https://crates.io/crates/sugarloaf · libghostty: https://libghostty.tip.ghostty.org/ +
  Rust bindings https://lib.rs/crates/libghostty-vt · WezTerm stall:
  https://github.com/wezterm/wezterm/issues/7825 (no stable since 2024-02).
- Persistence substrates: zellij 0.45 (2026-08-20; resurrection serializes layout+command
  every 1s, `post_command_discovery_hook` = programmable resume rewrite):
  https://zellij.dev/documentation/session-resurrection.html · shpool 0.11.4:
  https://github.com/shell-pool/shpool · zmx (libghostty-vt server-side snapshots):
  https://zmx.sh/ · tmux 3.6b.
- systemd ground truth: logind defaults + linger semantics
  (https://man7.org/linux/man-pages/man5/logind.conf.5.html); Arch compiles
  `KillUserProcesses=no`; this box additionally has `Linger=yes` — both verified locally.
- Agent-terminal field: Quil (hook-captured resume + ghost buffers):
  https://quil.cc/blog/resume-claude-code-session-after-reboot/ · Warp agent management:
  https://docs.warp.dev/agents/using-agents/managing-agents · cmux (libghostty, macOS):
  https://github.com/manaflow-ai/cmux · VS Code Agent Host (2026-08-26):
  https://code.visualstudio.com/blogs/2026/08/26/agent-host-architecture · VS Code
  terminal persistence: https://code.visualstudio.com/docs/terminal/advanced.
- Claude Code substrate: sessions/resume/hook docs
  (https://code.claude.com/docs/en/sessions) — `--resume` works cross-directory
  (v2.1.223+), embedders blessed via `CLAUDE_CONFIG_DIR`; CLI-flagged permission modes
  are not restored on resume.
