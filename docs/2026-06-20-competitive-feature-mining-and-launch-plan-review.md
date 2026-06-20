# Terminal Delight — Competitive Feature Mining + Launch-Plan Review

**Date:** 2026-06-20 · **Author:** Parker (research run) · **Scope:** the three questions from the Codex GTM-plan thread.

> **What this answers**
> 1. **Feature ideas to adopt** — mined from 6 competitor codebases (kitty, wezterm, alacritty, ghostty, tilix, cool-retro-term) cloned to `~/Projects/td-competitor-research`, plus a Warp/market scan. Ranked by value × effort-in-TD's-stack × identity-fit.
> 2. **Plan executability by agent** — which Codex-plan tasks an agent can do, and whether **Claude** or **Codex** is the better fit for each.
> 3. **Is it a good plan? A complete plan?** — graded against the *actual* repo state, with a deep-dive on bug intake (the part you flagged).

---

## TL;DR

- **The Codex GTM plan is strong strategy on a weak factual base.** Its positioning ("own the *intersection*, not any one feature"), launch sequencing (proof → soft → r/unixporn → r/linux → r/commandline → r/rust → HN, video before HN), and five-judges rubric are genuinely good. But the "Launch Readiness" and "Bug Intake" sections were written **blind to the repo**: ~40% of their concrete tasks are **already shipped** (v0.2.0 is released; issue + theme-submission templates exist; the bug template already collects distro/GPU/driver/X11-Wayland/scaling/commit; the **language pack shipped today**). It reads like a smart outsider who never opened the codebase — which is exactly Codex's failure mode and exactly where Claude's grounded workflow wins.
- **The single best engineering insight from the mining:** TD compiles **`alacritty_terminal 0.26`** but uses it only for the PTY/parser seam. Its **`RegexSearch`, per-cell `Hyperlink` (OSC-8), and `vi_mode` engines are already in the binary and unused.** So **clickable hyperlinks, real regex search, and keyboard-only navigation are UI-only work** — not new subsystems. That collapses three "big" features into a week of wiring.
- **The moat is the agent wall, not the CRT.** Market scan: Warp's answer (Oz) is cloud + proprietary; claude-squad is tmux logistics with no visual layer; the entire $2B agent-observability market is cloud SaaS that never touches the local PTY. **Nobody is doing a native-GPU, local-first, read-only wall for agents already running.** TD's live MCP server is architecturally prescient. Lead with that.
- **Highest-leverage next moves:** ship the **"nearly-free" Tier-0 cluster** (OSC-8 links, regex search, desktop notifications, error-line marks) this week; build the **agent-wall moat** (OSC-133 shell integration, output triggers, an agent-status HUD, worktree/branch in the pane header); ship a **`--doctor`** that turns bug intake from prose into one paste; and **cut 0.3.0** (v0.2.0 is 5 days and ~10 shipped features stale).

---

## How this was produced (credibility)

- Shallow-cloned and code-read six competitors (one Sonnet agent each, ~60–70k tokens of analysis apiece): `kitty` (38M), `wezterm` (243M), `alacritty` (51M), `ghostty` (133M), `tilix` (7M), `cool-retro-term` (78M) under `~/Projects/td-competitor-research/`.
- A seventh agent scanned the closed-source + market layer (Warp Oz, claude-squad, Ghostty momentum, MCP adoption, the agent-observability market).
- Grounded every recommendation against TD's real state: `app/Cargo.toml`, `app/src/*`, `.github/`, `CHANGELOG.md`, `info.html`, README roadmap, git tags + GitHub Releases. Where the plan claimed work, I checked whether it already exists.

---

# Part 1 — Feature ideas to adopt

### Where TD already stands vs the field

TD is **not** an early terminal. It already has what most of these projects are known for: GPU rendering, real PTYs, tiling + tabs + tab-groups + drag-to-split + tear-off, hot-reload TOML themes, a colour wheel + theme packs + per-pane inheritance, a full CRT shader (curve/phosphor/glare/scanline/tracking), OSD grade sliders, syntax schemes, a FOCUS reader with GPU blur, fuzzy find (per-pane + all-panes), a **read-only MCP server** (list_panes / pane_events / grep) with agent-finished bell + session capture, frameless CSD, a global hotkey, an AppImage, and a language pack (En/Es/De/中文, shipped today). The gaps are a **specific, coherent cluster** — and most of them are cheap because of the crate TD already links.

### The "already in the binary" unlock (verified)

`alacritty_terminal = "0.26"` (app/Cargo.toml:19) ships three things TD doesn't yet surface:

| Capability | Where it lives in the crate | TD status | Implication |
|---|---|---|---|
| `RegexSearch` (lazy DFA, fwd/back, wrapping, case-fold) | `term/search.rs` | **unused** — TD's find is a substring scan (`main.rs:1776` "exact, case-insensitive substring") | Real regex search + regex MCP grep is a **UI swap**, not an engine |
| `Hyperlink` per-cell (OSC-8) | `term/cell.rs` | **unused** — data is parsed into cells, never rendered/clicked | Click-to-open links ≈ **~50 lines** (hover-detect + `xdg-open`) |
| `vi_mode` motion engine (`ViMotion`, `ViModeCursor`) | `vi_mode.rs` | **unused** | Keyboard-only nav/selection is **UI mode-dispatch only** |

### Ranked adoption roadmap

Effort is in TD's gpui + wgpu + `alacritty_terminal` + Rust stack: **S** ≈ days, **M** ≈ 1–2 wks, **L** ≈ 3–6 wks, **XL** ≈ quarter+. "Fit" = alignment with TD's CRT + agent-wall identity. Sources cite the competitor that proves the pattern.

#### Tier 0 — Nearly free, ship this week (S effort, the crate already carries it)

| # | Feature | What it does | Value | Effort | Fit | Seen in |
|---|---|---|---|---|---|---|
| 1 | **OSC-8 clickable hyperlinks** | Ctrl-click a URL/`file://`/PR link an agent printed → `xdg-open`; hover-underline | **High** — agents emit links/paths constantly | **S** | Strong | all 5 (kitty, alacritty, wezterm, tilix, ghostty) |
| 2 | **Regex search** | Replace substring find with `RegexSearch`; also fixes MCP `grep` ("no regex yet") | **High** — scan agent logs for patterns | **S** | Strong | alacritty, tilix, wezterm |
| 3 | **Desktop notifications** | OSC 9/777 + on-finish → `notify-send`/libnotify; know when *any* pane finishes unfocused | **High** for walls | **S** | Strong | kitty, wezterm, tilix, ghostty |
| 4 | **Error-line marks** | Persistent highlight of `ERROR\|WARN\|panic` lines in scrollback (render-time, same loop as syntax) | **Med-High** | **S** | Strong — glowing red on phosphor green | kitty (marks) |
| 5 | **Open scrollback in editor** | `write_scrollback_file` → `$EDITOR`/pager on a pane's output | **Med** | **S** | OK | kitty, ghostty |

#### Tier 1 — The agent-wall moat (M–L; this is where TD *wins*, not just keeps up)

| # | Feature | What it does | Value | Effort | Fit | Seen in |
|---|---|---|---|---|---|---|
| 6 | **OSC-133 shell integration + jump-to-prompt** | Semantic prompt/output zones; jump between commands; select a command's whole output; **last-command exit status** | **High** — replaces the fragile `agent_is_thinking` bottom-scan with ground truth; "command done + exit code" lands straight in MCP | **M** | Strong | kitty, wezterm, alacritty, ghostty |
| 7 | **Output triggers** | User regex on new output → action (notify / run cmd / highlight / bell). "`rate limit` → ping me", "`FAILED` → flash the pane" | **High** | **M** | Strong — extends MCP + bell | tilix, kitty |
| 8 | **Quick-select label-jump** | Label every visible URL/path/hash/UUID; 1–2 keys to copy/open (wezterm ships 14 ready regexes) | **High** — best UX ROI on dense output | **M** | Strong | kitty, wezterm, alacritty, ghostty |
| 9 | **Broadcast / synchronize input** | Type into all panes at once (seed identical prompts, `Ctrl-C` all) | **High** for walls | **M** | Strong | kitty, tilix (ghostty abstains — see note) |
| 10 | **MCP write API + `td` CLI** | Promote the `TD_MCP_WRITE` stub: send-text / split / activate / set-config so an orchestrator can lay out the wall + spawn a pane per new agent | **High** | **L** | Strong (opt-in) — crosses "read-only", frame carefully | kitty `@`, wezterm cli, alacritty msg, ghostty CLI |

#### Tier 1b — Net-new white-space *only TD can own* (from the market scan)

| # | Feature | What it does | Value | Effort | Fit |
|---|---|---|---|---|---|
| 11 | **Agent-wall HUD** | Toggleable GPU overlay: per-pane grid of state (thinking/idle/finished/error) · uptime · last tool call. TD already detects `agent_is_thinking` | **High** | **M** | Strong — native-GPU, no browser tool matches it |
| 12 | **Per-pane cost/token meter** | Parse MCP `pane_events` for token/cost lines → footer/grade overlay | **High** | **M** | Strong — Langfuse does this in cloud; nothing local |
| 13 | **Worktree/branch in pane header** | Parse Claude Code's `--worktree` branch → pin as pane subtitle | **High** | **S-M** | Strong — only terminal that shows which agent is on which branch |
| 14 | **Cross-agent search highlight** | Extend `Ctrl+Shift+F` to highlight the same term across *all* panes simultaneously | **Med-High** | **M** | Strong |
| 15 | **Agent-aware record + replay** | Timestamped pane capture; replay at speed with tool-call/finish markers as chapter jumps | **High** (demos + debugging) | **L** | Strong |

#### Tier 2 — Delight & identity (CRT richness, theming, UX polish)

| # | Feature | What it does | Value | Effort | Fit | Seen in |
|---|---|---|---|---|---|---|
| 16 | **CRT richness pack** | Add to the wgsl post-process: **phosphor burn-in/persistence** (M, "the single most convincing CRT trick"), **horizontal-sync jitter** (S, high impact), **RGB/chroma shift** (S), **rasterization modes** scanline/pixel/aperture-grille (M), **static noise** (S, cap low), **flicker** (S) | **High** (identity) | **S–M each** | Strong | cool-retro-term |
| 17 | **Named CRT profiles** | One-click "Amber / C64 / Apple ][ / IBM 3278 / Plasma" presets over the grade sliders (CRT ships 14) | **High** | **S** | Strong — pure delight + marketing, ties to theme packs | cool-retro-term |
| 18 | **User custom shaders** | `custom-shader = effect.glsl`, Shadertoy-compatible, runs *after* the CRT pass — users layer effects on TD's phosphor base coat (naga already in wgpu) | **Med-High** | **M** | Strong | ghostty |
| 19 | **iTerm2 / base16 theme import** | Import the thousands of existing colour schemes → instant theme library (TD's roadmap already wants a "theme gallery") | **High** | **S-M** | Strong | ghostty |
| 20 | **Command palette** | Fuzzy action launcher (reuse the find modal + an action registry) | **Med** | **S-M** | OK | kitty, wezterm, ghostty |
| 21 | **Custom hyperlink rules** | User regex → command (`PR #123`, `JIRA-7`, `file:line`) with capture-group substitution | **Med** | **S** | Strong | tilix, wezterm |
| 22 | **Named / exportable layouts** | Save & reload "4-agent wall" templates (extends `state.toml` into named, shareable presets) | **Med-High** | **L** | Strong | tilix, wezterm |
| 23 | **Quake / dropdown mode** | Global-hotkey slide-down terminal (TD already has `Ctrl+Alt+T` + frameless CSD to build on) | **Med** | **M** | OK | tilix, ghostty |
| 24 | **Modal key tables / leader keys** | Keybind layers so users build vi/copy/resize modes in config without a baked-in mode | **Med** | **S-M** | OK | wezterm, ghostty |
| 25 | **`--doctor` + `+show-config`/`+list-actions`** | Diagnostic dump + config/action introspection (also a bug-intake win — see Part 3) | **Med** | **S-M** | Strong | ghostty |

#### Tier 3 — Big bets / deliberate skips

| Feature | Verdict | Why |
|---|---|---|
| **Graphics protocol (Kitty/sixel)** | **Defer, but flag as a future moat** | Inline images (agent charts, AI assets) differentiate vs alacritty (which refuses images) — but it's **XL** (new render stage, texture atlas, shm IPC). Revisit once the wall moat lands. |
| **Multiplexer / SSH domains / persistent sessions** | **Skip** | ~30k LOC; conflicts with local-first single-binary. The useful 20% is the CLI write API (#10). |
| **Lua scripting** | **Skip** | TOML + Rust hooks is the right, auditable call for an agent tool. |
| **Ligatures / HarfBuzz shaping** | **Low** | gpui's shaper sits upstream of TD's cell grid; agents don't emit ligatures. **XL**. |
| **Unicode picker, bookmarks, profile auto-switch** | **Minor** | Niche. One exception worth an S-effort look: an **advanced-paste guard** (warn on multi-line / `sudo` in agent-generated paste) is a real safety win. |

### Anti-patterns — what *not* to copy (cross-project consensus)

- **kitty's "kittens" subprocess model** — works because each kitten hijacks the whole terminal via IPC. Wrong for a multi-pane wall; build hints/palette/picker as **native gpui overlays**, not subprocesses.
- **Full multiplexer / SSH-domain stack** (wezterm, kitty ssh) — massive surface, conflicts with the single-binary local-first model.
- **GTK `/proc` polling** (tilix `monitor.d`) — TD already watches live PTY output for agent state; don't regress to polling.
- **A settings DB** (tilix dconf) — TD's file-based TOML is correct (shareable, hot-reload).
- **CRT readability traps** (cool-retro-term): cap static-noise < 0.15 and h-sync < 0.15; scope **burn-in to FOCUS or an opt-in per-pane toggle** — one recursive framebuffer per pane will blow VRAM across a wall.
- **Note on the broadcast debate:** ghostty deliberately omits synchronized-input and built-in vi-mode (offers key-table primitives instead). For a *human* terminal that's defensible; for an **agent wall**, broadcast (#9) earns its keep (seed N agents, kill N agents). Adopt it, but as an explicit, clearly-indicated mode.

---

# Part 2 — Can an agent execute the plan? Claude vs Codex per task

### The division that actually matters

- **Codex** is best on **self-contained, well-specified code** — one feature, clear spec, few cross-cutting unknowns. Its failure mode is **confident output ungrounded in reality** — which *this very plan demonstrates* (it proposed building things already shipped because it never read the repo).
- **Claude** (this harness) is best on **repo-grounded judgment, multi-tool orchestration, prose/positioning, docs, and review** — anything that must "read the codebase (and the world) first, then act with taste." It also owns TD's project skills: `td-demo` (headless staged capture), `td-redeploy`, `verify`, `code-review`.
- **Human-only** = anything **outward-facing or relationship/reputation-bound**: publishing a release, posting to Reddit/HN, DMing trusted testers, recording your own voice/screen. Agents *draft and stage*; a human *ships*.
- **The meta-recommendation:** let **Codex** generate contained code; let **Claude** ground, plan, write, and **review whatever Codex produces** before it lands. (Had Claude grounded this plan first, the redundant tasks would never have been written.)

### Phase-by-phase assignment

| Codex-plan task | Agent-doable? | Best fit | Why |
|---|---|---|---|
| **P0** Finish language pack | **Already shipped** (En/Es/De/中文, today) | — | If *extending* to more languages: either agent; Claude edge on translation-review nuance |
| **P0** Create release v0.2 / v1.0-preview | **Redundant — v0.2.0 already released.** Real task = **cut 0.3.0** | Claude drafts; **human/CI publishes** | Claude writes CHANGELOG + release notes from git log; tagging/publishing is human-gated |
| **P0** Add checksums to artifacts | **Yes** | **Codex or Claude** | Contained CI-YAML edit (sha256sums step + attach). Deterministic. |
| **P0** Known Issues doc | **Yes** | **Claude** | Needs repo-grounded judgment (Wayland/scaling caveats, glibc floor) + prose |
| **P0** "Known Good Systems" matrix | **Partial — template only** | **Claude drafts; humans fill** | You cannot truthfully claim "works on GPU X / driver Y" without a real machine. Agent builds the table; verified rows are human data |
| **P0** Disable blank issues | **Yes** (one-line flip) | **Either** | `config.yml: blank_issues_enabled: false` |
| **P0** Type-specific issue templates | **Yes** | **Claude** | YAML authoring + wording/UX judgment (see Part 3 bug-intake) |
| **`terminal-delight --doctor`** | **Yes** | **Codex *or* Claude** | Contained Rust feature. Codex fine for the mechanics; **Claude has the loaded TD context** (config paths, the wgpu adapter seam, grade hash) so it can one-shot it — slight Claude edge |
| **P1** Record 5 videos | **No (record) / Yes (everything around it)** | **Human records; Claude scripts + stages + captions** | Claude writes storyboards/voiceover + can produce staged b-roll headlessly via the **`td-demo`** skill (leak-safe, no real prompts/paths). Human does the final screen+voice |
| **P2** Update README w/ image+video | **Yes** | **Claude** | Markdown + prose + asset wiring |
| **P2** GitHub release publish | **Draft yes / publish no** | **Claude drafts; human publishes** | Outward-facing, irreversible |
| **P2/P3** Post to social / Reddit / HN | **Draft yes / post no** | **Claude drafts; human posts** | Accounts, timing, community norms, reputation risk — human. Claude writes titles, body, and a pre-baked FAQ/comment-response kit |
| **P2** Recruit 5–10 testers | **No** | **Human** | Relationships |
| **P4** Weekly release notes | **Yes** | **Claude or Codex** | Draft from `git log` |
| **P4** "Agent-wall workflow" examples | **Yes** | **Claude** | Docs + judgment about real workflows |
| **P4** Short clip per feature | **Human + Claude** | **Claude stages via `td-demo`; human polishes** | Same as P1 |
| **P4** Technical article ("PTYs through GPU CRT glass") | **Yes (draft)** | **Claude** | Strong technical writing + it already knows the architecture (gpui fork, CRT pass, GPL-sever). Human edits/owns the byline |

**Net:** roughly **half the plan is agent-executable today**, almost all of it Claude-side (it's grounding/prose/docs/CI-judgment, not isolated codegen). The few pure-code items (`--doctor`, checksums) are the only natural Codex hand-offs — and even those benefit from Claude's loaded repo context. The recording/publishing/posting/recruiting tasks are irreducibly human; agents make them 5× faster by drafting and staging.

---

# Part 3 — Is it a good plan? Is it a complete plan?

### Verdict: **strong strategy, weak diligence — a B+ skeleton that needs a repo-aware pass.**

The *thinking* is good. The *facts* are stale because nobody read the code. Ship the strategy; rebuild the task lists against reality.

### What's genuinely good (keep verbatim)

- **The positioning.** "You are novel at the *intersection*, not any single feature… claim the combined product, don't claim 'first CRT terminal'." Correct and well-argued. The market scan backs it: the **local-first GPU agent-wall is unclaimed**.
- **The GTM sequencing.** proof assets → soft launch → r/unixporn (visuals) → r/linux (OSS + honesty) → r/commandline (PTY correctness) → r/rust (architecture) → **HN only after install + intake are clean** → **YouTube before HN**. This is textbook-correct ordering and the per-subreddit framing is right.
- **The five-judges rubric** (Linux user / terminal nerd / designer / OSS maintainer / AI-era dev) is a useful acceptance test — keep it as the launch checklist.
- **The bug-intake instinct** ("structured or strangers will eat you alive") and the **`--doctor`** idea are right (details in the deep-dive below).

### What's already done in-repo (the blind-to-repo problem)

| Plan proposes… | Reality | Evidence |
|---|---|---|
| "Create release v0.2 / v1.0-preview" | **v0.1.0 *and* v0.2.0 already released** (v0.2.0 = "Latest", 2026-06-15), AppImage auto-attached, notes auto-generated | `git tag`; `gh release list`; `ci.yml` softprops/action-gh-release |
| "Improve issue templates" / "Add theme-submission template" | **`bug_report.yml`, `theme_submission.yml`, `config.yml` already exist** | `.github/ISSUE_TEMPLATE/` |
| "Require distro / GPU+driver / X11-Wayland / scaling / commit" | **The bug template already collects all of these** | `bug_report.yml` Environment + Commit fields |
| "Finish language pack" | **Shipped *today*** — `lang.rs` Strings catalog En/Es/De/中文 + 🌐 picker + CJK fallback (PR #49/#50, origin/main) | `git ls-tree origin/main app/src/lang.rs` |
| "Issues / license / contribution path clean" (judge concern) | **`CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`, `CLA.md`, `THIRD-PARTY-LICENSES.md` all present** | repo root |
| Page rebuild: "add hero H1 + subhead + Download button + sections; demote GAMBA" | **info.html already has** a hero (H1 + subhead + Download-AppImage + Star CTAs + staged demo), a sectioned narrative (overview → features → **dedicated MCP "mission control" section** → gallery → gamba → crawl → run → vision → license → build), and **GAMBA is already mid-page** | `info.html` |

**One bug-intake item the plan gets exactly right:** `config.yml` currently has `blank_issues_enabled: true` — the plan's "disable blank issues" is **valid and actionable.** And `--doctor`, checksums, type-specific templates, and Known-Issues/Known-Good docs are all genuinely **net-new and worth doing.**

### What's missing (completeness gaps — including TD's own best assets)

The plan under-uses things TD *already has* that are pure credibility:

1. **The latency numbers.** README: **key→echo→parsed p50 121µs / p99 169µs; `seq 1 100000` in 0.089s.** This is the **single most persuasive stat for the terminal-nerd and r/commandline judge** and the plan never mentions it. Headline it in "Technical receipts" and the r/commandline post.
2. **The MIT / GPL-sever story.** TD severed three GPL-3.0 crates out of the Zed graph so the binary is cleanly MIT-redistributable, enforced by `cargo deny` with **no GPL exceptions**. That's a *specific, impressive* OSS-maintainer-judge asset — far stronger than the plan's generic "license is clean."
3. **The AppImage glibc floor** (≥ 2.35 / Ubuntu 22.04+) — *the* compatibility fact for "Known Good / Known Issues." Omitted.
4. **Wayland + fractional-scaling rough edges** (TD's own roadmap flags them) — a launch-day bug magnet that **must** be in Known Issues pre-emptively. Omitted.
5. **Flatpak** (on TD's roadmap) — broadens reach beyond the AppImage for the r/linux crowd. Omitted.
6. **Demo-content safety as a *production constraint* for the 5 videos.** TD enforces staged demo content (throwaway home, fake prompt — never real paths/usernames) and there's history-scrub debt from a prior leak. The plan's video phase ignores this; a single real-path leak in a launch trailer is the worst-case. Bake "staged-only, via `td-demo`" into the video runbook.
7. **Lead with the agent wall, not the CRT.** The plan treats agent-wall as one feature among many and lets "CRT terminal" carry the headline. The market scan is unambiguous: **the ownable, uncontested position is "the native-GPU wall for watching local agents."** That should be the H1 story; CRT is the *aesthetic*, the wall is the *category*.
8. **No definition of launch success.** No target metric (stars? installs? bug-report quality? HN rank? first-week crash rate?). Add explicit success criteria so the launch is measurable.
9. **Net-new differentiators the plan can't see** (because it didn't do market research): agent-status HUD, per-pane cost/token meter, worktree-in-header, agent-aware record/replay (Part 1, Tier-1b). Record/replay in particular **is also the engine for the proof videos** — build it and the trailer films itself.

### Bug intake — deep dive (the part you flagged)

Current state: **one generic `bug_report.yml`** (good environment fields), **blank issues ON**, a `theme_submission.yml`. The plan wants 5 type-specific templates + `--doctor` + structured requirements. Here's the **complete, repo-aware version** — do these in order:

1. **Flip `blank_issues_enabled: false`** and add `config.yml` `contact_links` → GitHub **Discussions** for Q&A and **SECURITY.md** for vulns. *(S, agent — Claude.)* Stops the firehose of unstructured issues immediately.
2. **Ship `terminal-delight --doctor`** — the highest-leverage intake move. One paste must contain: distro + kernel + **glibc version**, **wgpu adapter name + backend (Vulkan/GL) + driver**, X11/Wayland + compositor, display scaling, locale, TD **version + commit**, active theme/grade (hashed), and the **startup GPU/font diagnostic** TD already emits. Then every template just says *"paste `--doctor` output."* *(M, code — Codex or Claude; Claude has the wgpu-adapter + config-path context loaded.)*
3. **Split templates by failure mode — but 4, not 5** (theme-submission already exists): **Visual/rendering + CRT** (requires screenshot/video + grade settings), **Terminal correctness** (requires the exact program/command + expected-vs-actual — e.g. a vim/tmux/htop misrender), **Crash / freeze / GPU-init** (requires stderr + `RUST_BACKTRACE=1` + the GPU diag), **Agent / MCP** (requires which agent + MCP client + `TD_MCP` mode). Each embeds `--doctor`. *(S, agent — Claude.)*
4. **Add `docs/KNOWN_ISSUES.md` + a "Known Good Systems" table**, linked from every template header so duplicates self-filter (lead with the Wayland/fractional-scaling + glibc caveats). *(S–M; Claude drafts, humans verify the "good systems" rows.)*
5. **Triage scaffolding:** labels per type + a saved reply ("please paste `--doctor`"). *(S, human-config; Claude can draft the label set + a labeler action.)*

That sequence turns "reporting bugs" from a prose negotiation into a one-paste, self-deduplicating, type-routed pipeline — which is exactly the "structured or they'll eat you alive" outcome the plan wanted, grounded in what the repo actually has.

### The corrected, repo-aware plan (tightened)

- **P0 (now):** flip blank-issues → false; build `--doctor`; split into 4 issue templates; write KNOWN_ISSUES (+ glibc/Wayland caveats); add **checksums** to the release; **cut 0.3.0** (v0.2.0 is ~10 features stale — schemes, MCP+grep, theme packs, optimize pass, language pack, CJK, find). *Skip "create v0.2" and "finish language pack" — done.*
- **P0.5 (page):** the page doesn't need a rebuild — it needs a **motion hero (gif/short clip of the real app)**, a **"Watch the demo" CTA**, the **latency stat surfaced**, the **"What's in 0.1.0" heading fixed to 0.3**, and the **agent-wall section promoted toward the top**.
- **P1 (proof):** build **agent-aware record/replay** first — it both ships a Tier-1b differentiator *and* becomes the camera for the trailer + tour + per-feature clips. Script via Claude, stage via `td-demo` (staged content only), record/voice by human.
- **P2–P3:** run the Codex sequencing as written (it's good), with Claude drafting every post + a comment-response kit; human publishes/posts.
- **P4:** weekly notes (agent-drafted), theme gallery (already roadmapped) seeded by **iTerm2 import**, the technical article (Claude draft → human byline) leading with the **GPL-sever + latency** receipts.

---

## Recommended next actions (ordered)

1. **This week, nearly free:** Tier-0 cluster — OSC-8 links, regex search (+ regex MCP grep), desktop notifications, error-line marks. All ride the `alacritty_terminal 0.26` engines TD already links.
2. **Bug intake:** `--doctor` + blank-issues-off + 4 templates + KNOWN_ISSUES. (Unblocks a safe public launch.)
3. **Cut 0.3.0** with checksums; fix the page's stale "0.1.0" heading + add a motion hero and the latency stat.
4. **Start the moat:** OSC-133 shell integration (kills the `agent_is_thinking` heuristic), output triggers, agent-status HUD, worktree-in-header.
5. **Then proof + launch:** record/replay → trailer/tour → the Codex launch sequence (Claude drafts, human ships).
6. **Delight backlog:** named CRT profiles + the burn-in/h-sync/RGB-shift richness pack + iTerm2 theme import.

---

## Appendix — competitor source pointers

- **kitty** `~/Projects/td-competitor-research/kitty` — `shell-integration/` (OSC-133), `kittens/hints/` (label-jump), `docs/marks.rst`, `kitty/rc/` (remote control), `docs/desktop-notifications.rst`, `kittens/broadcast/`.
- **wezterm** `…/wezterm` — `wezterm-gui/src/overlay/quickselect.rs` (14 regexes), `docs/hyperlinks.md`, `docs/shell-integration.md`, `wezterm-gui/src/commands.rs` (palette), `docs/cli/cli/` (write API).
- **alacritty** `…/alacritty` — `alacritty_terminal/src/term/search.rs` (`RegexSearch`), `…/term/cell.rs` (`Hyperlink`), `…/vi_mode.rs`, `alacritty/src/display/hint.rs`, `…/polling/ipc.rs` (`alacritty msg`).
- **ghostty** `…/ghostty` — `src/renderer/shadertoy.zig` (custom shaders), `src/terminal/osc/parsers/semantic_prompt.zig` (OSC-133), `src/input/Link.zig` + `src/terminal/hyperlink.zig`, `src/input/Binding.zig` (key tables, `write_scrollback_file`), `src/terminal/kitty/` (graphics).
- **tilix** `…/tilix` — `source/gx/tilix/terminal/terminal.d` (triggers L1589, sync-input L267, hyperlinks L2578), `…/terminal/search.d`, `…/session.d` (JSON layout save/restore).
- **cool-retro-term** `…/cool-retro-term` — `app/shaders/burn_in.frag`, `…/terminal_dynamic.{frag,vert}` (noise/jitter/flicker/h-sync/raster), `…/terminal_static.frag` (RGB-shift/bloom), `app/qml/ApplicationSettings.qml` (14 named profiles).
- **market scan** — Warp Oz (cloud/proprietary), claude-squad (tmux, no visual), Ghostty momentum (GPU-for-humans), MCP ubiquity, $2B cloud-only agent-observability → **local GPU agent-wall is white-space.**
