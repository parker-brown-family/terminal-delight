# Uninstall Review, Round 12

**3 findings: 0 critical, 2 high, 1 medium.** Reviewed `/home/parker/Work/td-uninstall-lab/terminal-delight` at `5eaa7285`, covering planning, ownership, execution, export, build pruning, CLI dispatch and output, and ADMIN callers. CONFIRMED means traced through source; no scenarios executed. No builds, tests, TD execution, signals or code edits. This report is the only file written.

The round-11 ADMIN fix carries the uncertain PIDs into the row, suppresses usage/removal claims in both armed states, and disables pruning. The accepted same-account race limitation and bare-command PATH decision are not findings here.

## High

### H1. A Launcher Symlink Admits a Program the Plan Calls “Not a TD Build”

Locations: `app/src/uninstall/plan.rs:1056–1078`, `plan.rs:1290–1297`, `plan.rs:1320–1334`; `app/src/uninstall/apply.rs:948–960`; `app/src/uninstall.rs:444–461`.

Scenario: the person keeps a compiled, non-AppImage supervisor at `~/.local/lib/terminal-delight/custom-wrapper` and points `~/.local/bin/terminal-delight` to it. The supervisor is running. They open ADMIN in a real TD instance and review uninstall. This configuration is already in place before preview and remains unchanged throughout removal.

The directory scan excludes `custom-wrapper` because its name does not begin with `td-`; the plan explicitly reports it as “not a TD build, so it stays.” The launcher branch then adds the same program to TD's executable list using only its resolved parent directory, ELF header and non-AppImage classification. It never applies the build-name predicate or establishes that this is the running TD's program.

`is_td_exe` accepts the explicit executable-list match, so shutdown sends TERM to the unrelated supervisor and may escalate to KILL. The launcher-removal branch uses the same weaker predicate and deletes the person's symlink. The supervisor file itself stays, as promised, but its process and launcher do not.

**CONFIRMED (traced).** No race, unreadable evidence or AppImage ambiguity is involved. The plan's two ownership decisions disagree for the same file.

Suggested fix: apply one build-identity predicate to directory discovery, launcher-target discovery and launcher removal. A target outside the recognized `td-*` build set needs independent TD identity evidence, such as the existing byte-identical-program check; otherwise retain/report it. Add a running non-`td-*` ELF target under the builds directory and assert that its PID is never signalled and its launcher link remains.

### H2. Another Account Can Supply an Install Receipt That Authorizes Deletion

Locations: `app/src/uninstall/known.rs:65–99`, `known.rs:163–168`; `app/src/uninstall/plan.rs:235`, `plan.rs:637–649`, `plan.rs:895–904`; `app/src/uninstall/apply.rs:579–597`. Compare the stricter loose-asset receipt validation at `apply.rs:966–981`.

Scenario: on an unprotected account-home run, `XDG_STATE_HOME=/srv/shared-state`, and its `terminal-delight/receipts` directory permits another account to create files. That account supplies a regular `00-forged.receipt`, owned by itself, containing a `file` record for `/home/u/Documents/project/keys.html` and that unrelated file's known SHA-256. All of this predates the preview; the other account cannot delete the victim file directly.

`known::receipts` opens without following links and checks that the descriptor is a regular file. It never checks its owner or whether other accounts can write it. `receipt_file("keys.html")` selects the supplied absolute path, and `ownership` accepts its matching hash as proof that TD installed it. The key-sheet step schedules `RemoveFile`; the worker verifies the same hash and deletes the unrelated document. This occurs even with records set to Keep, and file removal does not retain a before-copy.

An existing account-owned receipt writable by another account has the same problem. The state-root checks accept this absolute, non-overlapping location; the account-home worker permits the document path.

**CONFIRMED (traced).** The attack crosses account boundaries and requires no concurrent replacement. Reading and judging one descriptor prevents a pathname race, but a regular-file check does not establish who supplied the ownership claim. The loose-asset receipt reader already checks both UID and write permissions.

Suggested fix: validate install-receipt ownership and write permissions on the opened descriptor before accepting any records, with an explicit policy for any supported privileged installer. Reject/report untrusted receipts. Add foreign-owned and group/world-writable receipt cases and assert that they cannot authorize removal of an otherwise unrecognized file.

## Medium

### M1. The CLI Still Publishes Removable Counts When Process Use Is Unknown

Locations: `app/src/uninstall/builds.rs:167–173`, `builds.rs:186–197`, `builds.rs:569–581`, `builds.rs:621–631`.

Scenario: the round-11 four-build case is inspected through `admin builds` or `admin builds --json`: the oldest build is running, its executable cannot be read, and its PID appears in `listing.uncertain`. ADMIN now handles this correctly.

The CLI still calls `prunable`, which ignores `listing.uncertain`. Text output says one build is removable and prints its byte count, then appends a warning that process use is unknown. JSON likewise includes the oldest build in `removable` and its size in `removable_bytes`. An unchanged `prune` call refuses the entire operation. The warning and uncertainty field do not make those definite removal claims true.

**CONFIRMED (traced).** This is the remaining CLI output path for the uncertainty fixed in ADMIN; deletion remains blocked.

Suggested fix: share the eligibility/blocker result between CLI output, ADMIN and pruning. When process identity is unresolved, report eligibility as unknown or blocked and omit/null the affirmative removable list and byte total. Add text and JSON regressions alongside the new ADMIN regression.
