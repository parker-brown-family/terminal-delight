# Status: Linux uninstall

**Difficulty: 5/10 for this assessment** — one reviewable proposal; the cleanup and preview milestone is now approved. A complete destructive lifecycle engine will need its own architecture and program-design review after the product choices settle.
**Turned out to be: 5/10 for assessment** — the supplied design already described the architecture; the useful new work was checking the live branch and reproducing cleanup failures.

- Combined assessment: reviewed; four product concurs recorded.
- Product choices: approved as recorded below.
- First milestone source: implemented in `/home/parker/Work/td-uninstall-preview`; fixture-qualified and ready for source review; not integrated or deployed.
- Removal engine, exporter and workflow-analysis skill: not implemented.
- Intended release: unassigned. Proposed integration branch: dev/0.5.0, subject to release selection.

## THE LEGION MUST KEEP TERMINAL DELIGHT

Parker, 2026-10-30: **DO NOT KILL TD ON THIS BOX.** Preserve the installed build, launcher, helpers, settings, records and every live TD process on the Legion. This work may edit source and run isolated fixtures here. No live uninstall, stop, reload, upgrade, deployment or replacement is authorized on this machine. The other Lenovo is a candidate for live tests only after its identity is verified.

## Approved review, 2026-10-30

Parker concurred with keeping records by default, permitting removal before optional AI review, qualifying Omarchy before wider platform claims, and starting with cleanup repairs plus a read-only preview. These approve the first implementation milestone. The full destructive lifecycle still requires its ownership/shutdown design review.

Parker additionally requested an optional brief or skill-like handoff for an agent to manage the process. The handoff carries the target machine, selected retention scope, evidence, supported commands and action authority. In this first milestone it grants inspection only; it must not invent deletion commands. Later removal execution will use the reviewed deterministic worker. The user can finish uninstall without choosing an agent or using hosted AI.

## Evidence

The original chat is https://chatgpt.com/c/6ae4b721-1774-83e8-98f5-95754a5f7299. Its attached Page was downloaded through the signed-in browser and retained unchanged as `source-chat-design.md`. It inspected development commit `034b8bb6d108638f8b263e967f4e79710b7276c7`.

This assessment checks live development commit `28054220a8d5a913cc7a9154b676442afae85636`, read from GitHub on October 30, 2026. The audited uninstall scripts, removal page, default-terminal undo, briefing undo and host implementation have no diff from the chat's snapshot. The local `/home/parker/Work/td-dev-0.5.0` working tree was older; its checkout was not updated. Source checks used pinned git objects.

The reproducible fixture runner and results live under `reports/2026-10-30-uninstall-evidence/`. No live uninstall, settings edit, service stop or process shutdown was performed. The existing modification to `app/src/usage.rs` and other work were left in place.

## Next work

1. Review the implemented cleanup and read-only preview source, then qualify the packaged build on an isolated target.
2. Cleanup repairs, fixture regressions and guide ordering are implemented locally; integration remains pending.
3. The read-only candidate inventory and optional managing-agent handoff are implemented in the binary; downloaded-release qualification remains pending.
4. Design and qualify independent shutdown/removal before adding destructive execution.
5. Add verified preservation exports, then the optional workflow review kit.

Mac remains a medium-term target. Linux owns the current qualification matrix; no Mac removal behavior is claimed.

## Review delivery

The glass gate passed at 38/255 mean luminance. Narrative result: journey, opening 0.90. Browser checks covered 968px and 389px widths, both evidence dialogs, the preservation-choice control, complete annotation coverage, reload persistence and anchored export; the QA note was removed. Native TD in-place comment save/reopen was not exercised because native UI automation is unavailable. Full product tests were not run: product code is unchanged. See `reports/2026-10-30-uninstall-evidence/verification.json`.

Durable follow-ups: [preserve unrelated hooks](https://github.com/parker-brown-family/terminal-delight/issues/1209), [settings cleanup failures](https://github.com/parker-brown-family/terminal-delight/issues/1210), [damaged Hyprland markers](https://github.com/parker-brown-family/terminal-delight/issues/1211), [later GNOME choices](https://github.com/parker-brown-family/terminal-delight/issues/1212), and [complete removal](https://github.com/parker-brown-family/terminal-delight/issues/1213). All are unassigned to a release.

## Implementation, October 30

The cleanup and preview code is in branch `fix/uninstall-preview`, isolated from the operator checkout. Exact TD nested commands are removed while unrelated entries and group metadata survive. Invalid settings, unknown helper references, changed helpers and ambiguous Hyprland blocks stop cleanup. GNOME uses an ownership receipt and compares current values; old installations without a receipt remain unresolved.

The headless `uninstall preview` command accepts text, JSON or `--handoff` Markdown output. Retention intent defaults to keep; export/delete intent does not grant execution. The inventory is deliberately a candidate-path survey. All items are retained; ownership, machine identity, active writers, package provenance and export verification remain unverified. It makes no vendor calls or host connections.

Validation so far: fourteen cleanup test methods pass; the same tests expose twenty-four failed assertions in the baseline. Four tests of the production preview module pass in an isolated Cargo harness, and its Clippy check passes. ShellCheck, Rust formatting, whitespace checks and the documentation build pass. The full application and test targets compile (`cargo check --locked --tests`). A temporary-filesystem quota failure was resolved by moving only this task's build cache to disk. The shared renderer lacked two required patches; a private checkout with the twelve prescribed patches fixed the build without changing shared dependencies. Full application test execution and downloaded-AppImage/live-desktop qualification have not run.

The review page retains all five notes and four concurs, adds optional agent management and a large pulsing Legion protection notice, and passes the glass gate (43/255). Wide and 389px browser checks show no horizontal overflow. Narrative evaluation: journey; cold opening 0.75.

No install, live uninstall, TD process stop, desktop reload, deployment or replacement has run. No other Lenovo has been accessed. Release membership remains unassigned.

The worktree now lives at `/home/parker/Work/td-uninstall-lab/terminal-delight`, beside its private renderer. `/home/parker/Work/td-uninstall-preview` is a stable symlink to it. No source commit, push, PR, merge or release was made in this turn. Implementation evidence is under `reports/2026-10-30-uninstall-implementation-evidence/` in that worktree.

At delivery, the TD endpoint reported window 2269143 / `td-4cc995cf-0.5-floor-map-pool-drop`, whereas the earlier endpoint was window 1552933 / `td-7150fb1b-0.5-floor-map-builder`. The cause is unverified. This task issued no TD stop, install, replacement or reload. Do not claim that the live instance remained globally unchanged during concurrent work.

## CONFIG › ADMIN, built end to end (October 30, afternoon)

Parker concurred on all six decisions of the second-look review and asked for CONFIG › ADMIN (Export, Uninstall) built end to end while away. `01-admin-build.md` records the shape and contracts in place of gates 2–4. Difficulty 8/10.

- **Built:** the ⚙ ADMIN panel (`admin_view.rs`), export with a verified manifest, old-build listing and pruning, the removal planner and worker (`app/src/uninstall/`), `terminal-delight admin export|builds|guard|protect` and `uninstall preview|plan|run`, installer receipts and a table of every shipped hash (Codex, `scripts/`), and the install page's removal steps.
- **The Legion:** `~/.config/terminal-delight/protected` names its machine id. The worker, the panel and the handoff refuse removal on it; a read-only plan against this machine confirms the refusal. No removal, prune, export, stop or swap was run here.
- **Proof:** the engine's whole-removal fixtures; a real-binary qualification on a fixture home (a decoy TD process from a fixture build stopped, the operator's own relays untouched, the person's settings and another program's hook kept, nothing left beside the settings file, the worker deleting its own copy); photographs of the panel in both states from hidden windows on fixture homes.
- **Adversarial review:** the Codex sibling reviewed in rounds. Round 1: 1 critical, 14 high, 5 medium, all fixed. Round 2: 1 critical, 9 high, 6 medium, fixed except two stated limits (loose runtime files are matched by TD's exact names, not hashed; a same-account process racing the worker can redirect it).
- **Not done:** live qualification on another machine (the other Lenovo's identity is unverified); GNOME removal stays manual; installing this build on the Legion is Parker's call.
