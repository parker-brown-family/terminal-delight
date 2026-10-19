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
