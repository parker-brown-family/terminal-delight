# Status: Terminal Lab

**Difficulty: 5/10** — reducing background work can delay badges and attention updates; the implementation needs one combined plan.
**Turned out to be:** pending.

## Approved direction

Parker concurred with both recommendations in reports/2026-10-16-terminal-lab.html:

- `ask-start-with-background-pane` — performance first; reproduce the reported cost, then set the improvement plan.
- `ask-use-a-weekly-bounded` — four weekly shards of at most 15 minutes including setup, failure artifacts capped at 10 MB per shard with seven-day retention; start manually after replay and minimization work.

These decisions remain approved. They are not being reopened.

## Current work

- [x] Read current project truth and inspect completed performance work.
- [x] Revalidate installed build with the existing isolated hidden-window method.
- [x] Write the combined implementation plan, identifying new work separately from shipped optimizations.
- [x] Combined implementation gate — accepted 2026-10-17: Parker said “ok” and directed a shipped-build performance comparison first. Do not ask again for the already accepted priority, budget or implementation scope.

## Findings

The original brief omitted completed performance changes. Main already includes hidden-pane refresh throttling, screen-copy memoization, visible-output frame limiting, stepped tab badges and bounded Codex rollout-head reads. Their individual outcomes and limitations must be carried into the next plan. The local checkout is the older usage-pace-burndown branch; no application code here will be used as current-main evidence without comparison.

No application or workflow changes in this planning turn. No performance claim for 50 panes exists.

## Current-build diagnostic

Installed td-0c10d9b-human-prompt-arrows: 32 total panes, 30 printing hidden behind an idle tab, CRT off, same panes paused/busy twice. Window CPU 3.70/6.20% and 3.40/6.10% of one logical CPU; separate host 0.30/2.60% and 0.20/2.70%. Ten-second intervals after six-second settling. Raw counters, layout and exact diagnostic scripts are under reports/2026-10-16-terminal-lab-evidence/. This meets the CPU target in this rig but leaves badge latency and visible response unmeasured; no old-build speedup is claimed.

Four-figure implementation page: 278 main prose words / 211 caption words. Browser checks passed at 1100/720/420px, including notes, concur, exported saved annotations and modal close. Narrative gate: journey. Isolated window/host and run-specific session files were removed.

The concrete plan is now accepted, with the shipped-build comparison requested first. Future implementation continues with the reusable measurement-runner slice in a clean worktree based on current origin/main; these diagnostic evidence scripts are not yet that finished runner.

## Shipped-build comparison, 17 October

reports/2026-10-17-shipped-performance.html answers the follow-up. Four matched pairs show 49.666% lower median window CPU under 30 hidden status streams (14.9949% to 7.5475% of one core); window plus unchanged host falls about 42.3%. Five pairs were attempted, with pair 4 excluded because old/new windows landed at different sizes/monitors; all excluded data retained. Raw intervals, binary hashes, geometry, layouts, analysis and cleanup proof are under reports/2026-10-17-shipped-performance-evidence/.

Live window PID 240255 runs td-5b495a1-direct-human-turns, verified to contain all five relevant shipped changes. Live host PID 3678 is the older td-2d5b879-video-clips-and-codex-files; it intentionally remained running. Neither live process was restarted. The isolated benchmark host and all test windows were removed.

## Correctness lane and GitHub pilot, 19 October

Parker said go on the recommendation to take slices 3 → 4 → 5 first, since the correctness lane is what runs on GitHub's free minutes and the performance lane needs a real compositor. Built on branch lab/correctness-tracer, one PR.

- [x] Slice 3, correctness tracer — `app/src/vt/lab.rs` (cfg(test)) replays cases through TD's `Term` boundary and records explicit observations; `scripts/lab/run.py` compares, classifies, minimizes and replays. Two planted faults (erase-line ignored; ESC lost at a read boundary) are detected, shrunk (500 → 11 bytes; 774 → 22) and replayed on every run.
- [x] Slice 4, bounded corpus — corpus `gen-1`, splitmix64 seeds shared by Rust and Python, limits checked before parsing, chunk invariance at two seeded cut plans per core. Cross-core differences are classified by counterfactual (`scripts/lab/known.json`: remove the cause, the cores must agree), reduced to the causes they need, and never dropped.
- [x] Slice 5, GitHub pilot — `.github/workflows/td-lab.yml`, workflow_dispatch only; build job id `rust` restores ci.yml's cache; three shards; reproducers capped at 10 MB, 7 days. ci.yml cancels superseded pull-request runs only.
- [x] Weekly schedule — switched on 19 October after Parker concurred on the decision page; a scheduled run takes seed base week × 100,000, so each week is 6,000 new cases.

**Measured manual run, 20 October 00:21 UTC** (run 39347146038, pushed from the branch through a temporary trigger since removed): success in 7 min 6 s wall. Build job 5:51 including a full cache hit (`v0-rust-rust-Linux-x64-…`, 2.1 GB, `full match: true`); shards 0:55, 1:08, 1:11. 6,011 cases, 36,066 replays, 3,424 findings (3,409 cross-core, 15 alacritty cut-dependent), 0 unexplained; both controls caught. A cold build is 21.5 min locally, so the cache is what keeps the budget.

**Findings** (evidence in known.json): rio-vt does not home the cursor when origin mode changes; narrowing reflow leaves extra blank history rows in rio-vt (undecided which is right); clear-screen with an attribute set adds a history row in rio-vt; rio-vt drops a zero-width mark with no base, and the tab marker when a tab lands on the last column; the cores differ on half-erased wide characters; alacritty's vte 0.15.0 drops the character after a split UTF-8 sequence (alacritty/vte#159, fixed on main, unreleased; pinned by a test). rio-vt never depended on where reads ended: 0 of 1,722 cases locally, 0 of 6,011 on GitHub.

Least confident: the wide-character entry explains by deleting every wide character, which can hide a new wide-character bug; split it per operation next. Observations exclude events, synchronized updates and pictures.

Decision page: ~/Work/reports/2026-10-19-terminal-lab-correctness.html (private, outside this public repository).

## Round two, 19 October (evening)

Parker's notes on the decision page: concur on the weekly schedule, on splitting the wide-character cause, and on holding upstream reports (with a spam concern: a person's message first, never an automated report). Then he approved four more: replay real programs ("be careful we don't leak sensitive stuff"), settle narrowing, a known-unknowns map, and fix the rio-vt defects; release candidate 0.4.0 is cut 2026-10-20.

- [x] Real programs — `scripts/lab/record.py` records vim, nvim, less, git log/diff, ls, bash readline, a progress bar, tput, man and whiptail on a pty at 80×24 and 40×12, with mid-session resizes, in a blank environment with fixture files only, behind a fail-closed leak gate; recordings are made at run time, used on shard 1, never committed. 20 recordings here, 19 on GitHub (nvim missing; less 80×24 rejected by the gate for a base64-shaped run). Zero disagreements on any of them.
- [x] Known-unknowns map — `scripts/lab/catalogue.json` (94 families) and `unknowns.py`; every summary ends with "What this run could not check". GitHub run: 37 checked, 1 partly, 7 sent but unseen, 49 never sent, 3 uncatalogued (sent by a real program).
- [x] Wide-character cause split — neutralize rules take an optional `when` pattern; six narrow wide-character entries plus invalid-utf8-cut-by-an-escape. 9,061 fresh cases (3,031 local at seed 400000, 6,030 on GitHub at seed 500000) leave 0 unexplained.
- [x] Narrowing settled — tmux as a third opinion agrees with alacritty; rio-vt agrees once the text style is removed. rio-vt counts styled blank cells as content: the same root cause as clear-screen-with-an-attribute.
- [ ] rio-vt fixes — separate pull request, not merged before the release candidate.

**Measured GitHub run, 20 October 05:21 UTC** (run 39374472892, seed base 500000): success in 7 min 13 s; build 5:17, shards 1:00, 1:30, 1:53 (shard 1 records). 6,030 cases, 3,482 findings, 0 unexplained; both controls caught.

Least confident: wide-half-keeps-its-style removes every style code from a case that holds a wide character, which makes it the broadest entry left (622 findings on GitHub); narrowing-reflow still drops every resize until the styled-blank fix lands. The lab records the screen only, so query answers (device attributes, mode reports) are unseen by design.

Decision page: ~/Work/reports/2026-10-19-terminal-lab-round-two.html (private).
