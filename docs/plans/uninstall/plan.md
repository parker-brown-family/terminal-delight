# Plan: Linux uninstall

**Difficulty 5/10 for this assessment.** The four product choices and first cleanup/preview milestone were approved on October 30. This document authorizes no live uninstall on the Legion.

## THE LEGION MUST KEEP TERMINAL DELIGHT

Parker, 2026-10-30: **DO NOT KILL TD ON THIS BOX.** Preserve the installed build, launcher, helpers, settings, records and every live TD process on the Legion. This work may edit source and run isolated fixtures here. No live uninstall, stop, reload, upgrade, deployment or replacement is authorized on this machine. The other Lenovo is a candidate for live tests only after its identity is verified.

## Approved review, 2026-10-30

Parker concurred with keeping records by default, permitting removal before optional AI review, qualifying Omarchy before wider platform claims, and starting with cleanup repairs plus a read-only preview. These approve the first implementation milestone. The full destructive lifecycle still requires its ownership/shutdown design review.

Parker additionally requested an optional brief or skill-like handoff for an agent to manage the process. The handoff carries the target machine, selected retention scope, evidence, supported commands and action authority. In this first milestone it grants inspection only; it must not invent deletion commands. Later removal execution will use the reviewed deterministic worker. The user can finish uninstall without choosing an agent or using hosted AI.

## Purpose

Parker wants serious consideration of uninstall for Terminal Delight users, focused on Omarchy and Linux, with Mac in the medium term. The supplied chat adds an optional agent review that explains the work TD supported and preserves material useful to a later workflow improvement pipeline.

Offer three choices through one removal engine: remove the application and keep records (default); save useful work and remove application and selected data; or remove application and selected data without an export. Full removal describes the selected owned footprint, with an explicit receipt of retained and unresolved items. A partial result must not say complete.

## Shape

An installation registry records actual paths, component ownership and changes to shared settings. Existing installations use conservative recognizers with explicit unresolved items. A filename alone is insufficient ownership evidence. Data roots can have several consumers: retain shared data and hooks if another TD installation needs them.

The read-only planner inventories without launching TD, executing plugins or collecting vendor usage. A maintenance worker staged outside removal roots captures disappearing live facts, resolves active jobs, blocks new mutations, flushes and stops verified writers, finalizes any requested export, restores shared integrations, removes selected owned files and verifies the result. The worker and journal survive removal of the binary. Preserve requested data on any export failure.

Reuse the default-terminal compare-before-restore behavior and briefing hook's nested editing pattern, after extending their outcomes for transaction reporting. Current default-terminal undo deletes its journal when it sees a later edit: preserve that evidence in the transaction before invoking it. Restore neither whole shared settings backups nor guessed desktop defaults.

Interfaces to design after product review:

```text
discover(scope) -> Inventory { owned, shared, external, unresolved }
plan(inventory, choices) -> RemovalPlan { exact_operations, blockers }
apply(plan, journal) -> RemovalReceipt { removed, retained, blocked, verification }
export(plan, capture) -> ExportResult { integrity, coverage, missing_dependencies }
review(approved_inputs) -> ReviewResult { findings, evidence, analysis_coverage }
```

These are proposed boundaries, not implemented commands. Unknown measurement values remain optional. Snapshot verification and analysis completion remain separate fields.

## Linux first

The first supported route is an AppImage or user deployment on Omarchy: desktop entry/icons, xdg-terminal-exec preference, Hyprland managed blocks, exact TD hooks/helpers, versioned executables, TD configuration/state/data/cache/runtime roots. Inspect legacy HOME paths alongside XDG overrides. Treat the keys helper, TD-send helper, embedded skills and generated temporary assets as separate components. Preserve project trees, native agent histories, other products, shared dependencies and arbitrary external assets.

The installed Omarchy terminal launcher delegates to xdg-terminal-exec; it does not hardcode TD. Keep that launcher and restore only TD's owned preference. Its package remover enumerates pacman packages: it cannot be assumed to find an arbitrary AppImage. Detect installation provenance; delegate package-owned files through that package manager, using an explicit removal set. Qualify GNOME separately against its existing hotkey installer. Other desktops get common file cleanup plus explicit unsupported integration results until adapters are qualified.

Platform adapters own discovery, shared-setting undo and runtime handling. The plan, export and receipt formats stay portable. The future Mac adapter requires fresh investigation of the actual Mac distribution and runtime; no launch-agent, keychain or bundle-removal behavior is presumed today.

## Preservation and review

Before shutdown, preserve selected process/session IDs, working directories, native resume instructions and sleeping-agent evidence. Arbitrary running shells, builds and editors cannot be promised lossless migration. Snapshot stable disk records after writers stop; verify copied bytes, coverage and referenced assets. Exports must live outside every deletion root, including symlink resolution.

Ship a portable package with immutable originals, an evidence index and checksums, resume manifest, review kit, separate generated analysis, human review events and removal receipt. It must be readable after TD is gone. Review all eligible records within the selected scope in resumable batches, or label the chosen narrower coverage. Unknown, unreadable and unsupported files remain explicit.

The optional reviewer documents supported uses, useful configurations, unfinished work and testable improvement candidates. Every finding cites preserved evidence. An opened artifact is not endorsement; a saved configuration is not usage frequency. New findings begin unreviewed. Human disposition, evidence strength and action authorization stay separate. Export now/review later is a full product path. Hosted review and downstream pipeline import each require their own selected destination and input scope. No outbound submission belongs to default uninstall.

## Least confident decisions

1. Keep records by default, with preservation and full deletion as equally discoverable choices.
2. Expose the same planner from the app's menu and a shipped headless maintenance entry point; complete removal from outside its victim process tree.
3. Correct existing cleanup first, then read-only inventory; postpone destructive engine implementation until its lifecycle and ownership contracts are approved.
4. Keep exports useful for backup and migration independently of uninstall. The first release need not ship the AI review or any pipeline adapter.

## Proposed slices

1. Existing cleanup repairs: mixed hooks, malformed settings, missing/duplicate markers, later desktop choices, and guide ordering. Fixture regressions must fail on the current code.
2. Tracer: a shipped, read-only inventory and preview on isolated installations. Prove it performs no feature startup or vendor calls.
3. Application removal: one supported Omarchy install, keep records, independent worker, compare-before-restore, receipt, interruption recovery.
4. Data removal and preservation: pre-stop evidence plus verified final snapshot; preserve on disk-full/hash/permission failures; full selected cleanup.
5. Broader Linux qualification: custom and legacy paths, duplicate installs, GNOME, package provenance, stale hosts, externally cached hooks.
6. Optional review kit and a local improvement-record contract. Fresh reviewer runs after TD removal; original evidence remains unchanged.

## Acceptance

Qualify fresh downloaded AppImage with no checkout/network; interrupted/repeated removal; unknown and hung hosts; frozen panes; active jobs; shared roots; changed settings; invalid JSON; damaged markers; dangerous paths and symlink escapes; unwritable export and missing assets. Prove unrelated user data/settings survive byte-for-byte where unchanged. Every planned item gets an outcome. Verify replacement-terminal resolution and report whether a harmless launch was actually tested.

The present fixture suite reproduces five failure cases in current scripts. It is audit evidence, not implementation qualification. Details and proposed user screens are in the review page.
