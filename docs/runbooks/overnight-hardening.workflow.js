export const meta = {
  name: 'overnight-hardening',
  description: 'Unattended 1.0 hardening: GREEN tasks -> ready PRs, AMBER -> draft PRs (needs-visual-verify). Sequential, worktree-isolated, CI-gated, never touches main.',
  phases: [
    { title: 'GREEN — land (ready PRs)' },
    { title: 'AMBER — candidate (draft PRs)' },
  ],
}

// ---- config from args (safe defaults) -------------------------------------
// args = { landing: 'pr' | 'main', scope: 'green' | 'green+amber' }
const landing = (args && args.landing) || 'pr'        // 'main' would push direct (NOT recommended)
const scope   = (args && args.scope)   || 'green'     // default: GREEN only
const TARGET  = '/home/pbrown/.cache/td-overnight-target'  // shared CARGO_TARGET_DIR (gpui compiles once)

const RESULT_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['issue', 'branch', 'kind', 'ci_green', 'pr_url', 'draft', 'summary', 'blocked_reason'],
  properties: {
    issue: { type: 'integer' },
    branch: { type: 'string' },
    kind: { type: 'string' },
    ci_green: { type: 'boolean' },
    pr_url: { type: ['string', 'null'] },
    draft: { type: 'boolean' },
    tests_added: { type: ['integer', 'null'] },
    summary: { type: 'string' },
    blocked_reason: { type: ['string', 'null'] },
  },
}

// ---- shared guardrail preamble injected into every task -------------------
const GUARDRAILS = `
You are an UNATTENDED overnight agent on terminal-delight (repo root: /home/pbrown/BROWN-FAMILY-SPORTS/Software/terminal-delight, remote parker-brown-family/terminal-delight). NO human is watching. Follow this recipe EXACTLY.

WORKTREE (correct depth is critical — the gpui path-dep '../../zed-upstream' must resolve to the shared sibling fork):
  cd /home/pbrown/BROWN-FAMILY-SPORTS/Software/terminal-delight
  git worktree add -b <BRANCH> ../td-wt-<ISSUE>      # lands as Software/td-wt-<ISSUE>, a sibling of the repo
  cd ../td-wt-<ISSUE>
  bash scripts/prepare-gpui.sh                        # idempotent; patches the SHARED fork (no-op if already patched)

BUILD/TEST — always export the shared target dir so gpui is compiled once and reused:
  export CARGO_TARGET_DIR=${TARGET}
  (run all cargo commands from ./app)

LOCAL CI GATE (MUST be green before any push; run cargo fmt FIRST — scripted edits bypass rustfmt-on-save):
  cd app
  cargo fmt
  cargo fmt --check && cargo clippy --release --all-targets -- -D warnings && cargo test --release
  If this is NOT fully green after a bounded number of fix attempts (max 3): DO NOT push, DO NOT open a PR. Leave the branch local, set ci_green=false and blocked_reason, then STOP (still remove the worktree with --force is fine to keep main clean, but keep the branch).

COMMIT (author = Parker's git identity; the repo's default):
  - NEVER add 'Co-Authored-By: Claude', a 'Generated with' footer, or ANY AI-attribution to the commit message OR the PR body. This is a hard rule.
  - Stage ONLY the files you changed (git add <paths>), never 'git add -A' across the worktree.

PUSH + PR:
  git push -u origin <BRANCH>
  <see per-task PR instruction>   # gh auto-selects the parker-brown-family account
  Base = main. Reference the issue with 'Closes #<ISSUE>' (GREEN) or 'Refs #<ISSUE>' (AMBER/draft).

CLEANUP:
  cd /home/pbrown/BROWN-FAMILY-SPORTS/Software/terminal-delight
  git worktree remove ../td-wt-<ISSUE> --force

HARD RULES: never push to main; never force-push; never auto-merge; never edit the main checkout. If anything is ambiguous, prefer to STOP with a clear blocked_reason over guessing.
Return the structured result object describing exactly what you did (be honest: ci_green reflects the real gate result; pr_url null if you did not open one).
`

// ---- task table -----------------------------------------------------------
const GREEN = [
  {
    issue: 136, branch: 'test/r4-correctness-matrix', tests: true,
    title: 'test(term): R4 headless terminal-correctness matrix (#136)',
    spec: `Add a HEADLESS, DETERMINISTIC terminal-correctness test module. Prefer driving the parser SYNCHRONOUSLY (no PTY, no shell, no sleeps): construct an alacritty_terminal Term + a vte ansi Processor and feed bytes with processor.advance(&mut term, bytes); then assert on term state. Crib Term construction (Config, size) from app/src/term.rs spawn_in; grid/cell access pattern is app/src/pane.rs:1942-1991 (grid[Line(y)][Column(x)].flags, term.mode(TermMode::...)). DO NOT drive via a real PTY+shell with sleeps — that is flaky in CI.
Cover (assert, or assert-and-document with a comment if alacritty_terminal does not expose it): (a) wide-char width — WIDE_CHAR / WIDE_CHAR_SPACER placement for CJK; (b) emoji/ZWJ width; (c) alt-screen: ESC [ ?1049h sets TermMode::ALT_SCREEN, ?1049l clears it; (d) a mouse-mode CSI sets the corresponding TermMode; (e) bracketed-paste toggle; (f) scrollback: history_size()/display_offset() after overflow; (g) ANSI 16-colour cell fg; (h) OSC8 hyperlink registered (or documented gap); (i) OSC52 (or documented gap). Aim 30-60 tests; they must run under 'cargo test --release' with no display. Put them in app/src/term.rs as a #[cfg(test)] mod or a new module wired into the crate. PR: gh pr create --base main --head <BRANCH> --title "<TITLE>" --body "Closes #136. Headless parser-driven correctness matrix; first parser-level coverage in the suite. <N> tests, all synchronous (no PTY)."`,
  },
  {
    issue: 127, branch: 'fix/anchor-top-altscreen-guard', tests: true,
    title: 'fix(pane): guard inverted anchor-top against alt-screen TUIs (#127)',
    spec: `Inverted anchor-top read corrupts full-screen TUIs (vim/htop) because it reverses row order while the alternate screen assumes top-to-bottom. GUARD the inversion: when the pane is in alt-screen, do NOT invert.
Edit 1 — app/src/pane.rs near line 3989: the decision 'let inverted = anchor_top() && !th.crawl;' must become '... && !alt_screen_active' where alt_screen_active = term is in TermMode::ALT_SCREEN.
Edit 2 — app/src/pane.rs near line 1656 (FOCUS mirror_snapshot): the 'if anchor_top() && !th.crawl {' decision needs the same guard.
CRITICAL: the Term is a FairMutex — do NOT take a second lock if one is already held in that scope (double-lock = DEADLOCK). Inspect the surrounding code; reuse an existing 'term.lock()' guard if present, otherwise lock once: self.session.term.lock().mode().contains(TermMode::ALT_SCREEN). TermMode is already imported (pane.rs:16).
Refactor the boolean into a tiny pure helper (e.g. fn should_invert(anchor_top: bool, crawl: bool, alt_screen: bool) -> bool) and add a unit test covering the truth table. This is logic-provable headless; the BOX-DRAWING fix is visual, so the PR body must list the visual-verify steps. PR: gh pr create --base main --head <BRANCH> --title "<TITLE>" --body "Closes #127. Guards the inverted read so alt-screen TUIs render upright. Unit test on the gate. VISUAL VERIFY before merge: TD_ANCHOR_TOP=1, open vim/htop -> box-drawing must NOT flip; FOCUS reader mirrors correctly."`,
  },
  {
    issue: 76, branch: 'polish/float-popups', tests: false,
    title: 'polish(ui): float treatment on the last 3 popups (#76)',
    spec: `Three popups still use a bare '.shadow(vec![BoxShadow{...}])' instead of the established float treatment: lang-picker (app/src/main.rs ~4213), group-config (app/src/main.rs ~12033), agent-finished card (app/src/pane.rs ~4625). Find the established pattern used by other floats (search for 'float_shadows' and the '.border_2().border_color(...)' + layered shadow treatment, e.g. main.rs ~2931 / ~3837) and apply it consistently to these three so they read as floating panels, matching the rest. Pure visual; no test, but it must compile + pass clippy. PR: gh pr create --base main --head <BRANCH> --title "<TITLE>" --body "Closes #76. Consistent float treatment (border + layered shadow) on lang-picker, group-config, and the agent-finished card."`,
  },
  {
    issue: 75, branch: 'i18n/dashboard-recover-strings', tests: false,
    title: 'i18n: localize agent-dashboard + recover-tool strings (#75)',
    spec: `Hardcoded English literals in the agent-dashboard + recover tool need to go through the i18n catalog. Known offenders: app/src/main.rs ~11135 "DEAD AGENTS", ~11081 "RESURRECT", ~11150 "recoverable" (grep the dashboard/recover.rs region for more bare string literals shown to the user). Add the needed fields to the Strings struct in app/src/lang.rs and provide translations for ALL 9 languages (the catalog is compile-checked: every language must define every field, so the compiler enforces completeness). Route call sites via current().strings().<field>. Keep brand words English. PR: gh pr create --base main --head <BRANCH> --title "<TITLE>" --body "Closes #75. Dashboard + recover strings now flow through lang.rs across all 9 languages (compile-checked)."`,
  },
]

const AMBER = [
  {
    issue: 87, branch: 'investigate/focus-drag-select', tests: false,
    title: 'investigate(focus): enable drag-select to copy in the 👓 reader (#87)',
    spec: `The FOCUS reader cannot highlight/drag-select text. Selection plumbing exists (focus_cell_at / focus_sel_drag / copy_focus_selection); the likely culprit is the scrim '.occlude()' (app/src/main.rs ~12454) swallowing mouse-move, or the reading-area on_mouse_down at ~12306 not wiring a move handler. Add a TD_FOCUSDEBUG trace of the pointer path, propose a candidate fix (route move events to the reading area / adjust occlude), and write a short findings note. This is a DRAFT PR — highlight-during-drag must be confirmed by eye. PR: gh pr create --draft --base main --head <BRANCH> --title "<TITLE>" --body "Refs #87. CANDIDATE — needs-visual-verify. <findings>. VERIFY: open FOCUS, drag across text, Ctrl+C, confirm the selection highlights and copies."`,
  },
  {
    issue: 90, branch: 'investigate/startup-self-heal', tests: false,
    title: 'investigate(session): offer richer backup when state loads small (#90)',
    spec: `On startup, if state.toml loads much smaller than the newest rotated backup (backups rotate at app/src/main.rs ~1041), offer to restore the richer backup. The boot path (load_state ~1003 / Workspace::build ~1083) does not offer this today. Implement a size-delta check + a candidate restore prompt BEHIND a flag (e.g. TD_SELFHEAL=1) so it cannot surprise anyone, plus a findings note on the chosen threshold. DRAFT PR — the modal UX needs visual sign-off. PR: gh pr create --draft --base main --head <BRANCH> --title "<TITLE>" --body "Refs #90. CANDIDATE behind TD_SELFHEAL — needs-visual-verify. VERIFY: shrink state.toml vs a fat backup, relaunch, confirm the restore offer + that declining is safe."`,
  },
  {
    issue: 137, branch: 'bench/latency-ab-harness', tests: false, scriptsOnly: true,
    title: 'bench: same-box latency A/B harness vs Alacritty (#137)',
    spec: `Author (do NOT run a measurement) scripts/bench/latency-ab.sh: types an identical input stream into TD (TD_LATENCY=1, parsing 'td_latency_us=' from stderr — emit point app/src/pane.rs:2120-2121) and into Alacritty on the SAME box, then prints p50/p99 for each + the delta. Document that it needs a real PTY echo + reliable key injection on :1 (xdotool keyboard works on :1) and is therefore a human-present measurement; the script is the deliverable. No Rust change. DRAFT PR. PR: gh pr create --draft --base main --head <BRANCH> --title "<TITLE>" --body "Refs #137. Harness only (no numbers yet) — run with Parker present on :1. needs-visual-verify."`,
  },
  {
    issue: 138, branch: 'bench/stress-20pane-harness', tests: false, scriptsOnly: true,
    title: 'bench: 20-pane RSS stress harness vs Tilix (#138)',
    spec: `Author (do NOT run) scripts/bench/stress-20pane.sh: generate a 20-pane demo state.toml (nested SavedNode Split tree — see app/src/main.rs:364-382; balanced ~4x5), boot TD with TD_DEMO_STATE=<path> TD_DEMO=1 on a caller-supplied DISPLAY, sample /proc/<pid> RSS every 3s for 30s, screenshot the wall (import -window root), and do the same for a 20-pane Tilix; print both RSS curves and assert TD < Tilix. Note in-script that focused >=110 FPS is a SEPARATE follow-up (needs a frame-time emit patch or perf sampling on a real display). No Rust change. DRAFT PR. PR: gh pr create --draft --base main --head <BRANCH> --title "<TITLE>" --body "Refs #138. RSS harness only; FPS proof is a follow-up. needs-visual-verify."`,
  },
  {
    issue: 140, branch: 'pkg/flatpak-manifest', tests: false, scriptsOnly: true,
    title: 'pkg: Flatpak manifest alongside the AppImage (#140)',
    spec: `Author a Flatpak manifest (org.* yml) + a build script for the MIT binary, mirroring what scripts/build-appimage.sh produces. It is a GPU app: include the right runtime + GL/Vulkan finish-args permissions. Dry-run 'flatpak-builder' ONLY if it is installed; otherwise skip the build and say so. Do NOT claim the bundle launches — that is the human verify step. No Rust change. DRAFT PR. PR: gh pr create --draft --base main --head <BRANCH> --title "<TITLE>" --body "Refs #140. Manifest + build script; bundle-launch unverified. needs-visual-verify."`,
  },
]

// ---- runner ---------------------------------------------------------------
function buildPrompt(t, kind) {
  let p = GUARDRAILS.replaceAll('<BRANCH>', t.branch).replaceAll('<ISSUE>', String(t.issue)).replaceAll('<TITLE>', t.title)
  if (landing === 'main') {
    p += `\nLANDING OVERRIDE: Parker chose direct-to-main. After the LOCAL CI GATE is fully green, instead of opening a PR you may 'git push origin <BRANCH>:main' — but ONLY if green, NEVER force, and NEVER for AMBER (AMBER always stays a draft PR). When in doubt, open a PR instead.\n`.replaceAll('<BRANCH>', t.branch)
  }
  p += `\n=== TASK #${t.issue} (${kind}) ===\n${t.spec}\n`
  return p
}

const results = []

phase('GREEN — land (ready PRs)')
for (const t of GREEN) {
  log(`GREEN #${t.issue} ${t.branch} — starting`)
  const r = await agent(buildPrompt(t, 'GREEN'), {
    label: `green:#${t.issue}`,
    phase: 'GREEN — land (ready PRs)',
    schema: RESULT_SCHEMA,
  })
  if (r) results.push(r)
  log(`GREEN #${t.issue} — ${r ? (r.ci_green ? 'CI green' : 'CI RED (no PR)') : 'agent returned null'}${r && r.pr_url ? ' -> ' + r.pr_url : ''}`)
}

if (scope === 'green+amber') {
  phase('AMBER — candidate (draft PRs)')
  for (const t of AMBER) {
    log(`AMBER #${t.issue} ${t.branch} — starting`)
    const r = await agent(buildPrompt(t, 'AMBER'), {
      label: `amber:#${t.issue}`,
      phase: 'AMBER — candidate (draft PRs)',
      schema: RESULT_SCHEMA,
    })
    if (r) results.push(r)
    log(`AMBER #${t.issue} — ${r ? (r.pr_url ? 'draft ' + r.pr_url : 'no PR: ' + (r.blocked_reason || '?')) : 'agent returned null'}`)
  }
}

const green = results.filter(r => r.kind !== 'AMBER' && r.ci_green && r.pr_url)
const blocked = results.filter(r => !r.ci_green)
log(`DONE — ${green.length} ready PR(s), ${results.filter(r => r.draft).length} draft(s), ${blocked.length} blocked.`)
return { landing, scope, results }
