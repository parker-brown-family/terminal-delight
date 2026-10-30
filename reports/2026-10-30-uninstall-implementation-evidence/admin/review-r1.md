# Uninstall Review, Round 1

Reviewed the uncommitted Rust engine and ADMIN panel in `/home/parker/Work/td-uninstall-lab/terminal-delight` against `docs/plans/uninstall/01-admin-build.md`.

**20 findings: 1 critical, 14 high, 5 medium.** CONFIRMED means traced through source, not executed. This was a read-only review: no builds, tests, TD execution, process signals or installation changes. This report is the only file written. Locations refer to the source read during this review; concurrent formatting may move them.

## Critical

### C1. A Fixture Launcher Can Authorize Killing the Legion's TD

`app/src/uninstall/plan.rs:786`; `app/src/uninstall/apply.rs:496`; `app/src/uninstall/guard.rs:85`; `app/src/uninstall.rs:319`

Scenario: a fixture HOME has `.local/bin/terminal-delight` symlinked to the real installed TD executable. `td_executables` accepts that target without ownership or confinement checks. The fixture bypasses the account-home marker lookup, and `StopProcesses` has no account-home or fixture boundary check. It canonicalizes the target, finds the real account's processes and signals them. The same mechanism accepts a launcher pointing to `/usr/bin/bash` and can signal unrelated shells. Matching an executable path only helps after that path has been authenticated.

**CONFIRMED.** The plan supplies the unchecked target, `stop` selects global account processes, and `RealEffects::signal` calls `kill`.

Suggested fix: require independently proven TD executable identity and enforce canonical fixture confinement before discovering or signaling processes; a fixture must never gain authority over a live executable.

## High

### H1. Ancestor Symlinks Escape the Fixture File Boundary

`app/src/uninstall/apply.rs:205`; `app/src/uninstall/apply.rs:356`; `app/src/uninstall/apply.rs:483`; `app/src/uninstall/builds.rs:77`

Scenario: fixture HOME contains `alias -> /home/parker/.local/state`, and `XDG_STATE_HOME` is `HOME/alias`. The planned state directory is lexically under HOME, while its final `terminal-delight` component is a real directory. Both `allowed` and `tree_ok` pass, and the leaf-only symlink check passes. Record deletion consequently reaches the real state directory. The same ancestor escape applies to file edits, host sockets and loose assets. Builds pruning also follows a symlinked builds directory and unlinks entries through it.

**CONFIRMED.** Prefix checks do not constrain the resolved ancestors of the paths passed to filesystem and socket operations.

Suggested fix: enforce resolved containment with descriptor-relative, no-follow operations through every ancestor, including the builds and host paths.

### H2. Another Review Can Replace Keep Records with Delete Records

`app/src/admin_view.rs:202`; `app/src/admin_view.rs:236`; `app/src/uninstall.rs:166`

Scenario: window A reviews Keep records. Window B reviews Delete records, overwriting the same `cache/admin/uninstall-plan.json`. A still stores `(Keep, that path)` and still displays Keep. Its two removal clicks reread B's Delete plan and pass it to the worker. The worker generates a fresh Delete plan from the submitted retention, so inventory validation accepts it. A's reviewed choice never binds the bytes executed.

**CONFIRMED.** ADMIN stores a retention and mutable pathname, then checks only its stored retention against the current widget selection.

Suggested fix: keep an immutable reviewed plan and digest per review, validate its retention at execution, and stage those exact reviewed bytes.

### H3. A Refused Plan Can Already Have Overwritten Files

`app/src/uninstall.rs:64`; `app/src/uninstall.rs:217`; `app/src/uninstall.rs:260`; `app/src/uninstall/apply.rs:121`

Scenario: submit a stale or wrong-machine plan with `--out` naming an existing directory containing `worker.log` or `worker/terminal-delight`. Before checking machine, home or inventory equality, `spawn_worker` creates directories and overwrites those paths. Its copies and log creation also follow destination symlinks. The child subsequently refuses the plan. An edited absolute or traversal-containing `stamp` can select such an output directory without `--out`, because `removal_dir` joins the stamp without validation. Direct `--now` also creates `before/` and writes a receipt after deciding to refuse.

**CONFIRMED.** Full validation happens inside `apply`, after staging, and even `apply` performs writes before returning its refusal.

Suggested fix: validate the complete plan, stamp and output boundary before any write, then create a fresh private output directory and exclusive files without following links.

### H4. Export and Remove Deletes Legacy Records That Were Never Exported

`app/src/uninstall/export.rs:248`; `app/src/uninstall/plan.rs:685`

Scenario: the current XDG config/state roots differ from the defaults, and an earlier installation left sessions under `~/.config/terminal-delight` or records under `~/.local/state/terminal-delight`. Export copies only the current roots. Removal then recursively deletes both legacy roots as well. The verified export contains no copy of those older records.

**CONFIRMED.** The deletion source set is larger than the export source set.

Suggested fix: export and verify every record tree scheduled for deletion, or retain legacy trees and report them separately.

### H5. The Verified Export Predates the Final Session Save

`app/src/uninstall/plan.rs:227`; `app/src/uninstall/plan.rs:446`; `app/src/host.rs:1859`; `app/src/uninstall/export.rs:258`

Scenario: a live host has layout changes since its last checkpoint. Export copies and verifies the older session file while TD is still running. Later `CloseHosts` sends Shutdown, whose handler calls `checkpoint_once` and writes the current session. Record removal deletes that new file, but the verified export still contains the older state. Writes during a multi-file export can also produce a mixture of session states.

**CONFIRMED.** Verification checks the copied bytes; no final source check or export follows host shutdown.

Suggested fix: retain the initial safety export if desired, then quiesce all writers and verify a final export covering their last saved state before deleting records.

### H6. Shared-File Temporary Writes Can Clobber a Link Target or Expose Secrets

`app/src/uninstall/apply.rs:456`

Scenario: a private settings file is mode 0600 in a traversable directory, with umask 022. Its replacement is created using `File::create`, written and synced before permissions are changed. During that interval the replacement can be 0644 and readable by other users. If the predictable `.settings.json.td-uninstall-<pid>` already exists as a symlink, `File::create` instead truncates and writes its target; the final rename can replace the original settings with that symlink.

**CONFIRMED.** Temporary creation is neither exclusive nor no-follow, and private permissions are applied after writing.

Suggested fix: create an unpredictable exclusive no-follow temporary file with mode 0600 before writing, then apply the intended mode and rename safely.

### H7. Failed Backups and Journals Do Not Stop Destructive Work

`app/src/uninstall/apply.rs:130`; `app/src/uninstall/apply.rs:216`; `app/src/uninstall/apply.rs:452`; `app/src/uninstall/apply.rs:604`

Scenario: `out/before` is unwritable, or the output filesystem fills while copying shared settings. `keep_copy` discards the failure and the edit still replaces the original. Failure to create or append the journal also does not stop subsequent removal. Final receipt writes discard errors, so `complete` can remain true and the CLI can exit successfully without a saved receipt. The promised recovery evidence is absent after destructive work.

**CONFIRMED.** These I/O results are discarded rather than propagated into operation status or the run result.

Suggested fix: require durable backups and journal entries before each mutation, and make receipt persistence failure an explicit failed result.

### H8. Receipt-Recorded Hooks Can Survive After Their Helpers Are Deleted

`app/src/uninstall/plan.rs:271`; `app/src/uninstall/plan.rs:284`; `app/src/uninstall/plan.rs:480`

Scenario: the installer recorded a hook in `/home/user/custom-claude/settings.json`, but the uninstall environment no longer sets `CLAUDE_SETTINGS` or `CLAUDE_CONFIG_DIR`. The planner scans only its environment candidates and `~/.claude/settings.json`; receipt hook paths are used only after a candidate has already been selected. It therefore never inspects the custom file, yet removes the receipt-owned helper. An existing candidate whose read fails also gets silently skipped without retaining its helper.

**CONFIRMED.** Receipt paths do not contribute to candidate enumeration, and read failures bypass `blocked_helpers`.

Suggested fix: enumerate all receipt hook paths and treat unreadable existing settings as unresolved dependencies that retain their helpers.

### H9. The Default Terminal Can Still Name TD When Its Binary Is Removed

`app/src/uninstall/plan.rs:243`; `app/src/defaultterm.rs:258`; `app/src/uninstall/apply.rs:237`

Scenario: after TD sets the default, a person appends a comment to `xdg-terminals.list`, leaving `terminal-delight.desktop` first. `defaultterm::off` sees changed bytes, leaves the list, removes its journal and returns `Ok`. The worker treats this as successful restoration and proceeds to remove TD's desktop entry and executable. If TD is listed but the journal is already missing, the plan merely reports a manual action and still schedules removal.

**CONFIRMED.** Neither route verifies that the effective default has a usable replacement before authorizing application removal.

Suggested fix: return a typed restoration result and block removal of the required binary/desktop entry until the remaining default-terminal reference is resolved.

### H10. MCP Ownership Falls Back to a Basename

`app/src/uninstall/mcp.rs:104`; `app/src/uninstall/plan.rs:354`

Scenario: another integration uses `/home/user/wrappers/terminal-delight` with first argument `mcp`. Its executable is a custom wrapper, unrelated to the proven TD executables. `is_td` returns true solely because its basename is `terminal-delight`, so the planner schedules deletion of that registration through the owner's CLI. This contradicts the approved rule that wrappers remain for manual review.

**CONFIRMED.** The basename shortcut returns before canonical executable comparison.

Suggested fix: resolve the command to a proven TD executable; retain and report unresolved bare commands, wrappers and other paths.

### H11. Pruning Treats an Unreadable Process Executable as Unused

`app/src/uninstall/builds.rs:85`; `app/src/uninstall/builds.rs:165`; `app/src/uninstall/builds.rs:334`

Scenario: an old TD build is running, is outside the newest three, and is not the launcher target. Reading its account-owned `/proc/<pid>/exe` returns a permission error. `scan_procs` increments `unreadable_procs`, but both the running list and the pre-unlink callback omit that process. `prune` never gates on the uncertainty and deletes the running build. Listing a warning does not protect it.

**CONFIRMED.** The scan preserves uncertainty in one field, then the destructive decision ignores that field.

Suggested fix: carry a complete-or-unknown process scan through both checks and refuse candidate deletion when live use cannot be ruled out.

### H12. PID Reuse Can Redirect the Escalation Signal

`app/src/uninstall/apply.rs:503`; `app/src/uninstall/apply.rs:535`; `app/src/uninstall/apply.rs:549`

Scenario: a matched TD process exits after TERM and its PID is reused by an unrelated process of the same account before a later poll. `alive` checks only whether the original number occurs in the new process list. It no longer compares executable identity or start time. After the grace period, KILL is delivered to that unrelated process. A process that execs another program under the same PID has the same problem.

**CONFIRMED.** Initial executable matching is reduced to numeric PIDs for all subsequent checks and signals.

Suggested fix: retain pidfds or equivalent stable process identities, and revalidate executable identity when the policy requires it before escalation.

### H13. A Known Running TD Outside the Build Directory Does Not Block Removal

`app/src/uninstall/apply.rs:510`; `app/src/uninstall/apply.rs:529`; `app/src/uninstall/plan.rs:675`

Scenario: a development build runs from `target/debug/terminal-delight` using the same settings and records as the installed build. `stop` explicitly discovers it in `elsewhere`, leaves it running, and returns `Ok` with a note. The worker then removes shared runtime/cache and, under Delete or Export, the settings and records this process still uses. New matching processes started after the initial target snapshot are also absent from the completion check.

**CONFIRMED.** The code observes a live TD process but does not make that observation a blocker.

Suggested fix: require a final complete process/writer check before deleting shared roots, and retain those roots while any relevant process remains or cannot be classified.

### H14. Names and Image Headers Authorize Deleting Unowned Assets

`app/src/uninstall/plan.rs:927`; `app/src/uninstall/apply.rs:378`; `app/src/uninstall/apply.rs:570`

Scenario: the user replaced the PNG icon with their own image while retaining TD's original desktop entry. Finding that owned desktop entry authorizes deletion of every matching-size PNG based only on the PNG signature, with `sha256: None`. Separately, an account-owned file such as `/tmp/terminal-delight-logo-draft.png` is deleted by the loose-asset name predicate even if TD never created it. Files newly appearing after review also match that directory-wide removal operation.

**CONFIRMED.** Neither path requires the receipt or shipped-hash ownership proof promised by the plan.

Suggested fix: inventory and authenticate each asset with receipts or known content hashes, and recheck that proof before unlinking it.

## Medium

### M1. Export Failure Does Not Always Block Stopping and Removing TD

`app/src/uninstall/plan.rs:237`; `app/src/uninstall/plan.rs:448`; `app/src/uninstall/plan.rs:460`

Scenario: an installation has no shared integration steps or host directories, and its requested export fails because the destination already exists. The stop step depends only on integrations and hosts, so its prerequisite list is empty. It still runs, and file/build removal follows it. Record deletion is blocked, but the application can already be removed despite the promised export-first gate.

**CONFIRMED.** The export dependency reaches later work only indirectly through optional integration steps.

Suggested fix: make successful export an explicit prerequisite of every mutating or stopping branch when Export is selected.

### M2. Removing a Hook Rewrites Unrelated JSON Bytes

`app/src/uninstall/jsonorder.rs:141`

Scenario: settings contain an unrelated string written as `"\u0061"`, CRLF line endings or spacing that differs from the serializer's inferred indentation. Removing one TD hook reparses and serializes the entire document, changing the string spelling, line endings and unrelated whitespace. Preserving key order does not meet the contract that only TD's removed entries change bytes.

**CONFIRMED.** `print_like` regenerates all JSON tokens and whitespace through serde rather than preserving source spans.

Suggested fix: remove the selected entries using source spans and minimal comma adjustments, preserving every other byte.

### M3. A Retained Key-Sheet Helper Loses Its Page

`app/src/uninstall/plan.rs:432`; `app/src/uninstall/plan.rs:528`

Scenario: a customized keybinding block cannot be safely removed. The planner retains `td-keybindings` because the binding still invokes it, but independently schedules the receipt-owned `keys.html` for deletion. The retained shortcut/helper then opens a missing page. The TD data-directory cleanup provides another route to remove that page.

**CONFIRMED.** `blocked_helpers` protects the executable but is not applied to its supporting page or data removal.

Suggested fix: retain the helper's required assets whenever a surviving integration retains the helper.

### M4. Receipts Can Claim Success for Failed Cleanup and Unexported Records

`app/src/uninstall/apply.rs:378`; `app/src/uninstall/apply.rs:402`; `app/src/uninstall/apply.rs:186`; `app/src/uninstall/apply.rs:631`

Scenario: a loose asset cannot be unlinked or a cache-refresh command fails. Both operation branches still return `Ok`, allowing the receipt to say the removal completed. On another path, an export failure correctly blocks record deletion, but the receipt still states `Records: exported, then removed` because it renders the requested retention instead of operation results. Delete failures similarly produce `Records: removed`.

**CONFIRMED.** Cleanup errors are converted into successful outcomes, and the records summary does not consult outcomes at all.

Suggested fix: propagate cleanup failures and derive every completion/records claim from verified operation results, preserving unknown and blocked states.

### M5. Export Includes the New Machine-Bound Hotkey Intent

`app/src/uninstall/export.rs:133`; `scripts/install-hotkey.sh:22`

Scenario: the hotkey installer has written `hotkey-terminal-key.intent`, including an interrupted installation that never produced `.owned`. Export excludes only the `.bak` and `.owned` filenames, so it includes this machine-bound intent despite the agreed `hotkey-terminal-key.*` exclusion. A transferred export carries undo state belonging to the source machine.

**CONFIRMED.** The current installer creates `.intent`; the export exclusion list omits it.

Suggested fix: exclude the complete root-level `hotkey-terminal-key.*` undo-record family and record the exclusion reason in the manifest.
