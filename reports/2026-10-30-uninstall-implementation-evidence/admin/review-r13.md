# Uninstall Review, Round 13

**3 findings: 0 critical, 1 high, 2 medium.** Reviewed `/home/parker/Work/td-uninstall-lab/terminal-delight` at `15ab37d2`, covering the round-12 changes and surrounding planning, ownership, shared-file editing, process shutdown, export, build pruning and ADMIN paths. CONFIRMED means traced through source; no scenarios executed. No builds, tests, TD execution, signals or code edits. This report is the only file written.

The round-12 cases are addressed: launcher targets now need the build predicate or byte-identical program evidence; install receipts are judged for UID and write permissions through the descriptor that is read; uncertain process identity suppresses removable counts in CLI text, JSON and ADMIN. The accepted same-account race limitation and bare-command PATH decision are not findings here.

## High

### H1. Default-Terminal Restoration Can Overwrite an Unrelated File Through Its Temporary Path

Locations: `app/src/uninstall/plan.rs:267–280`, `plan.rs:993–1006`; `app/src/uninstall/apply.rs:448–455`; `app/src/uninstall.rs:495–500`; `app/src/defaultterm.rs:176–190`, `defaultterm.rs:249–277`.

Scenario: an unprotected account-home installation uses `XDG_CONFIG_HOME=/srv/shared-config`, a non-sticky directory writable by a group containing another account. TD has a valid default-terminal journal, and the current terminal list still equals the journal's `wrote` value. The other account preplants `.xdg-terminals.list.tmp-<worker-pid>` as a symlink to a private document owned by the uninstalling account. The predictable PID name can be covered by preplanting a range of names before preview; no replacement between checking and writing is required.

The roots are absolute and non-overlapping, so the account-home plan accepts this configuration. `DefaultTerminalOff` copies the journal and terminal list, then delegates to `defaultterm::off`. That function restores `previous` through `write_atomically`, whose temporary-file creation is `std::fs::write` with neither exclusive creation nor a no-follow flag. It follows the planted symlink and truncates the private document, writing the old terminal list into it. The subsequent rename can also put that symlink at `xdg-terminals.list`.

The removal directory holds before-copies of the list and journal only. It has no copy of the unrelated document whose original bytes were destroyed. The private, exclusive temporary-file fix in `apply::edit` does not cover this delegated writer.

**CONFIRMED (traced).** This crosses account boundaries through a pre-existing symlink in a shared directory. It does not depend on the accepted same-account directory-swap race.

Suggested fix: harden the writer reached by `defaultterm::off` with exclusive, no-follow temporary creation and private permissions, retaining the opened descriptor through the write. Reject unsafe shared-directory arrangements where the final replacement cannot be secured. Add a fixture case with the expected temporary name already linked to a sentinel document; restoration must preserve the sentinel byte for byte and must not replace the terminal list with the link.

## Medium

### M1. Removal Counts Ignore Blockers That Pruning Already Enforces

Locations: `app/src/uninstall/builds.rs:82–108`, `builds.rs:170–183`, `builds.rs:224–232`, `builds.rs:308–313`, `builds.rs:579–589`, `builds.rs:635–647`; `app/src/admin_view.rs:65–80`, `admin_view.rs:119–143`.

Scenario: the person relocates their builds and makes `~/.local/lib/terminal-delight` a symlink to the new directory. It contains four readable, dated TD builds; the newest is the launcher's target, none is running, and process inspection succeeds. This arrangement remains unchanged throughout listing and pruning.

`list` follows the directory symlink and finds all four builds. `removable` checks only `listing.uncertain`, so it returns the oldest build. CLI text and JSON report one removable build and its bytes. ADMIN displays the same promise and enables the confirmation button. `prune` then refuses the entire operation because the builds directory is a symlink. Its existing blocker was available before any removal count was published.

An unreadable launcher has the same omission: listing drops the `read_launcher` error with `.ok().flatten()`, publishes eligibility without knowing the launcher's target, and `recheck` subsequently keeps every candidate with “launcher unreadable.”

**CONFIRMED (traced).** The uncertain-process fix works, but the shared eligibility function still does not describe what pruning can do under other stable, already-detected blockers. No wrong deletion occurs in these cases.

Suggested fix: carry directory and launcher inspection failures into the shared eligibility result, with reasons that CLI and ADMIN can display. Preserve the existing pruning refusals. Add a four-build symlinked-directory case and an unreadable-launcher case; neither should advertise an actionable removal count or enable pruning.

### M2. An Unreadable Builds Directory Becomes a Successful Empty Prune

Locations: `app/src/uninstall/builds.rs:84–100`, `builds.rs:170–183`, `builds.rs:192–256`, `builds.rs:538–589`, `builds.rs:600–632`, `builds.rs:635–647`, `builds.rs:700–716`.

Scenario: the real builds directory contains old builds but has mode `0111`: the account can traverse known executable paths, including its launcher target, but cannot enumerate the directory. The process table is readable, so the CLI's self-process check passes. The directory is an ordinary directory, not a symlink.

`list` correctly records the `read_dir` permission error, but returns an empty build vector. Listing output still derives definite totals from that vector: text reports zero builds and zero removable bytes after the warning; JSON supplies an empty `removable` array and `removable_bytes: 0`. Those values do not establish that no eligible builds exist.

`prune` discards `listing.error` completely. Its other guards pass, its empty loop does nothing, and it returns a default result with no failures. The CLI prints “Nothing to remove” and exits successfully; JSON likewise exposes no inspection failure. A partial directory-iteration error is also discarded, allowing a partial prune to be reported without the missing inventory being named.

**CONFIRMED (traced).** The directory error is captured, then lost at the eligibility and execution boundaries. A missing directory can legitimately produce an empty result; a directory that could not be read cannot establish one.

Suggested fix: make incomplete inventory explicit in eligibility and prune results. Propagate the directory error to `Pruned.failed` or an equivalent blocked result, return failure from the CLI, and suppress complete removable totals while inventory is incomplete. Add an unreadable-directory fixture and, where the directory source can be injected, a partial-enumeration failure; neither should produce an unqualified empty-success result.
