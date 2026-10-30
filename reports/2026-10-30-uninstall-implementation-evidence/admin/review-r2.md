# Uninstall Review, Round 2

**16 findings: 1 critical, 9 high, 6 medium.** Reviewed the worktree at `/home/parker/Work/td-uninstall-lab/terminal-delight`. CONFIRMED means traced through source; none of these scenarios was executed. No builds, tests, TD execution, process signals or code changes. This report is the only file written.

The revised policy of removing the application after an export failure is reasonable provided records remain protected. I have not counted that deliberate change as a defect. The original cases for the reviewed-plan digest, private exclusive temporary files, receipt hook-path enumeration, legacy-record retention during export, default-terminal dependency, retained key-sheet page and hotkey-intent exclusion are addressed. The findings below cover remaining paths and interactions.

## Critical

### C1. A Fixture Socket Can Still Shut Down a Live Host

`app/src/uninstall/apply.rs:378`; `app/src/hostctl.rs:297`; `app/src/uninstall/guard.rs:85`

Scenario: a fixture runtime directory contains `session-live.sock`, a symlink to a real Legion session socket. `allowed` validates the directory, but the directory scan accepts entries solely by filename. No check resolves or authenticates the individual socket before `close_at` connects through it and sends Shutdown. The fixture still bypasses the real account's marker lookup. The executable confinement added to `StopProcesses` never participates in this earlier shutdown.

**CONFIRMED.** `CloseHosts` passes the unchecked entry to `UnixStream::connect`, followed by `Request::Shutdown`.

Suggested fix: validate each socket without following links and authenticate its connected peer against the permitted TD executable/home before sending any shutdown request.

## High

### H1. Secondary Operation Paths Bypass Fixture Confinement

`app/src/uninstall/apply.rs:326`; `app/src/uninstall/apply.rs:512`; `app/src/defaultterm.rs:258`

Scenario: fixture HOME/config are isolated, but `XDG_STATE_HOME` points at the real account's state containing `default-terminal.journal`. `DefaultTerminalOff` validates only the fixture list path. It copies the external journal and calls `off`, which can remove that real journal even when the fixture list differs. A second route is `RefreshCaches`: an owned desktop entry under the fixture schedules refresh, while `icons/hicolor` is a symlink outside the fixture. Only `applications` is validated before the external icon-cache command runs.

**CONFIRMED.** Each operation has multiple filesystem operands, but checks only its first one.

Suggested fix: apply the same confinement policy to every read/write operand, including undo journals and both cache directories, before entering the effect.

### H2. Pruning Rejects a Symlinked Builds Directory but Follows Its Ancestors

`app/src/uninstall/builds.rs:81`; `app/src/uninstall/builds.rs:198`; `app/src/uninstall/builds.rs:222`

Scenario: a fixture's `.local/lib` is a symlink to the real account's `.local/lib`. The final `terminal-delight` component is a normal directory, so the new leaf symlink check passes. Listing and pruning then operate on the real builds. Any old build that passes the ordinary unused/newest checks is unlinked outside fixture HOME.

**CONFIRMED.** Canonicalization supplies paths for process matching; it does not enforce the fixture boundary before deletion.

Suggested fix: enforce canonical HOME confinement through every ancestor for the standalone builds tool, using the same protected filesystem operations as the removal engine.

### H3. A Skipped Export Authorizes Deleting Unexported Records

`app/src/uninstall/apply.rs:189`; `app/src/uninstall/apply.rs:290`; `app/src/uninstall/apply.rs:322`; `app/src/uninstall/plan.rs:714`

Scenario: select Export in a fixture and place a symlink at `HOME/terminal-delight-export-<stamp>` pointing outside HOME. The export step's `allowed` check returns `Halt::Skip`, so the exporter never runs. Dependency evaluation treats Skipped as satisfied. If the other prerequisites finish or skip, both `RemoveRecords` steps execute and erase the fixture's unexported records. The receipt then says records were kept because export did not finish.

**CONFIRMED.** A safety refusal is converted into dependency success, bypassing the verified-export requirement.

Suggested fix: require Done for safety prerequisites such as Export and shared-setting cleanup; distinguish harmless absence from an operation refused by confinement.

### H4. Keep Records Can Delete Them Through an Aliased Cache Root

`app/src/uninstall/roots.rs:65`; `app/src/uninstall/plan.rs:610`; `app/src/uninstall/apply.rs:457`

Scenario: `XDG_CACHE_HOME` and `XDG_CONFIG_HOME` name the same directory. Both resolve to the same `terminal-delight` root, which Roots accepts. A Keep plan still schedules `RemoveTree(r.cache)`, deleting settings and sessions, and later reports them kept. An ancestor symlink that makes those two roots resolve to the same directory has the same effect. A state/cache alias similarly deletes records.

**CONFIRMED.** The planner assigns retention by the root's label without checking whether disposable and retained roots overlap physically.

Suggested fix: reject overlapping resolved roots or subtract every retained record tree from all deletion operations before a plan can be reviewed.

### H5. Unknown Process Identity Still Becomes Permission to Remove Files

`app/src/uninstall.rs:515`; `app/src/uninstall/builds.rs:188`; `app/src/uninstall/builds.rs:267`; `app/src/uninstall/builds.rs:392`

Scenario A: an account-owned TD process becomes undumpable and its `/proc/<pid>` metadata reports root ownership. The worker's `own_processes` drops it at the UID check, unlike the builds scanner's `owned_by` fallback. It never reaches the new unknown-process blocker, so record removal can proceed while it remains alive.

Scenario B: pruning initially lists no uncertain processes. Just before unlinking, its process callback returns `(pid, empty_path)` for a newly unreadable TD process. `recheck` passes that list only to `pids_running`; the empty path cannot match a build, and deletion proceeds. Even the initial scanner drops unreadable executables if both name hints fail, although that does not establish that the process is unrelated.

**CONFIRMED.** The worker loses uncertainty during enumeration, and prune checks it only in the initial listing.

Suggested fix: share a scanner that preserves unresolved account-owned processes and require a complete process-identity check at every destructive decision, including the final prune recheck.

### H6. Loose Assets Are Still Deleted Without Ownership Proof

`app/src/uninstall/apply.rs:479`; `app/src/uninstall/apply.rs:751`

Scenario: an account-owned `/tmp/terminal-delight-scene-storyboard.webp` or `/tmp/terminal-delight-prop-customer-logo.png` was created by another tool. The revised predicates accept any alphanumeric/hyphen suffix in those families and delete the file. Even fixed names such as `terminal-delight-robot.png` are deleted without checking their content. These entries are not individually inventoried, so a new matching file appearing after review is also removed.

**CONFIRMED.** The PNG desktop-icon fix uses ownership hashes, but loose assets still use only name plus UID. Narrowing the name patterns did not close round 1's ownership failure.

Suggested fix: inventory individual loose assets, prove their ownership with receipts or known content, and revalidate that proof before unlinking them.

### H7. The Removal Output Can Escape a Fixture or Be Deleted by Its Own Plan

`app/src/uninstall/apply.rs:96`; `app/src/uninstall.rs:320`; `app/src/uninstall.rs:351`; `app/src/uninstall/roots.rs:103`; `app/src/uninstall/plan.rs:603`

Scenario A: a fixture run supplies a fresh `--out` outside HOME. `refusal` checks only whether it is inside a TD root, then `fresh_dir` and staging write outside the fixture. Inheriting a real `XDG_STATE_HOME` can produce the same escape through the default output path.

Scenario B: choose a fresh output directory beneath the fallback runtime tree, `<TMPDIR>/terminal-delight-<uid>/removal`. `td_dirs` omits that fallback tree, so the output passes validation. The planned runtime `RemoveTree` later erases the worker, its journal, original shared-file copies and log. The worker can continue with unlinked descriptors, but its receipt cannot be saved there.

**CONFIRMED.** Output validation neither enforces fixture containment nor compares the output against the complete set of planned removals.

Suggested fix: validate the output against canonical HOME policy and every planned deletion target, then create and retain it through protected directory handles.

### H8. Canonical Path Checks Still Leave an Ancestor-Swap Race

`app/src/uninstall/apply.rs:278`; `app/src/uninstall/apply.rs:457`; `app/src/uninstall/apply.rs:557`

Scenario: a planned directory is `HOME/alias/terminal-delight`, with `alias` initially resolving inside the fixture. After `allowed` returns, another process replaces `alias` with a symlink to an external parent. The later leaf metadata lookup sees a normal external `terminal-delight` directory and `remove_dir_all` deletes it. Shared-file reads, backups and final renames similarly resolve the path again after its containment check.

**CONFIRMED.** The check and mutation use separate pathname resolutions; the successful check holds no directory descriptor or inode identity through the mutation.

Suggested fix: use descriptor-relative operations anchored to validated directories, with no-follow traversal, so ancestor replacement cannot redirect the effect.

### H9. APPIMAGE Alone Can Authorize Removing Another Application

`app/src/uninstall/plan.rs:375`; `app/src/uninstall/plan.rs:876`; `app/src/uninstall/plan.rs:1121`

Scenario: native TD inherits `APPIMAGE=/home/user/OtherEditor.AppImage` from an AppImage parent, or that variable is otherwise set to an unrelated ELF file. The planner accepts it solely because it exists and has ELF magic. It adds the path to the executable targets and schedules its deletion with no hash. Nothing proves that the running TD came from that image. On an unprotected account-home run, this can delete the other application's image and signal a matching process.

**CONFIRMED.** The environment value is treated as provenance without checking it against the running executable or an install record.

Suggested fix: accept an AppImage target only after verifying its relationship to this running TD; otherwise retain and report it.

## Medium

### M1. A Running AppImage Payload Is Absent from the Stop Targets

`scripts/build-appimage.sh:74`; `app/src/uninstall/plan.rs:837`; `app/src/uninstall/apply.rs:655`; `app/src/uninstall/apply.rs:715`

Scenario: a live TD window or MCP relay runs from the AppImage payload at `<mount>/usr/bin/terminal-delight`. The AppRun script execs that file. The plan contains the outer `.AppImage` path and installed build paths, but no authenticated payload executable. `stop` cannot select that process for TERM; its final scan instead classifies it as another TD and blocks removal. Staging the worker does not add the original mounted executable to the plan.

**CONFIRMED.** The packaged execution path and the planned executable paths differ, and the remaining-process check turns that mismatch into a blocker.

Suggested fix: inventory the authenticated running AppImage payload as well as the outer image, preserving that identity across worker staging.

### M2. A File Named plan-reviewed.json Can Spoof a Staged Output

`app/src/uninstall.rs:267`; `app/src/uninstall/apply.rs:148`; `app/src/uninstall/apply.rs:825`

Scenario: invoke `--now --out DIR` where DIR is an existing directory containing any regular `plan-reviewed.json` and no journal. That is sufficient to pass the staged-directory check; the file need not match the submitted plan, and the process need not be a staged worker. A protected-machine or stale-plan refusal then calls `write_receipt`, creating or replacing `receipt.json` and `receipt.md` in DIR while reporting that nothing changed.

**CONFIRMED.** This direct `--now --out` route still writes after refusal; the parent-side preflight does not cover it.

Suggested fix: validate the plan before accepting or writing to any output, and authenticate actual staged state rather than recognizing it by filenames.

### M3. Every Saved JSON Receipt Says It Was Not Saved

`app/src/uninstall/apply.rs:146`; `app/src/uninstall/apply.rs:231`; `app/src/uninstall/apply.rs:825`

Scenario: a successful run reaches receipt writing. `saved` is still false when the receipt is serialized. Only after `write_receipt` returns does the in-memory field become true; the file is never rewritten. Thus a successfully persisted JSON receipt reports `saved: false`, contradicting the CLI result and defeating consumers of that field. If the JSON succeeds but the Markdown write fails, the persisted JSON can also retain the pre-failure `complete: true`.

**CONFIRMED.** Persistence state is updated after the only serialization.

Suggested fix: define a consistent persisted receipt status and publish it atomically, keeping write acknowledgement outside the record if it cannot truthfully describe itself.

### M4. Duplicate JSON Keys Hide a Hook from Dependency Discovery

`app/src/uninstall/jsonorder.rs:23`; `app/src/uninstall/jsonorder.rs:95`; `app/src/uninstall/settings.rs:51`; `app/src/uninstall/plan.rs:547`

Scenario: settings contain two top-level `hooks` members: the first is empty, and the second contains a TD helper command. The custom parser preserves both, so compact input can pass the new byte-for-byte round-trip check. `get_mut` inspects only the first member and reports no TD hooks or dependencies. The planner removes the owned helper while the second hook object remains unchanged; a consumer using the last duplicate member still invokes it.

**CONFIRMED** for parsing, dependency discovery and removal; client execution was not exercised.

Suggested fix: reject duplicate object keys before editing or deriving removal dependencies, retaining the associated helpers when settings are ambiguous.

### M5. MCP PATH Resolution Accepts Non-Executable Files

`app/src/uninstall/mcp.rs:118`; `app/src/uninstall/plan.rs:872`

Scenario: PATH first contains a regular ELF `terminal-delight` without execute permission, and a later directory contains an executable custom wrapper of that name. The operating system skips the first entry when launching the MCP command, but `is_td` selects it because it tests only `is_file`. If that first file is the installed launcher, `td_executables` also accepts it as ELF, so the wrapper's registration is incorrectly classified as TD and removed. The reverse arrangement can leave a real TD registration unresolved.

**CONFIRMED.** The resolver's first-file rule differs from executable lookup.

Suggested fix: resolve commands using executable-access semantics for the relevant environment and working directory, then compare the actual executable identity.

### M6. MCP Configuration Changes Have No Before Copy

`app/src/uninstall/mcp.rs:33`; `app/src/uninstall/plan.rs:377`; `app/src/uninstall/apply.rs:368`

Scenario: removing a Claude, Codex or Gemini registration changes its shared configuration through the owner's CLI. Discovery knows the config file, but the planned operation discards that path and its original registration content. `McpRemove` calls the CLI without `keep_copy`. The promised copy of every shared file before modification is therefore absent for these files, even on a fully successful run; backups now being mandatory for direct settings edits does not cover this branch.

**CONFIRMED.** No backup or restorable registration snapshot precedes this external mutation.

Suggested fix: retain the configuration identity in the operation and save its verified before-state or an equivalent restorable registration record before invoking the owner CLI.
