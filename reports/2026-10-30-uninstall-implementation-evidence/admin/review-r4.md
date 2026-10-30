# Uninstall Review, Round 4

**6 findings: 0 critical, 3 high, 3 medium.** Reviewed `/home/parker/Work/td-uninstall-lab/terminal-delight`. CONFIRMED means traced through source; no scenarios were executed. No builds, tests, TD execution, process signals or code edits. This report is the only file written.

The resolved-output comparison, current-root runtime overlap check, versioned-process uncertainty check, ancestor-HOME refusal and executable-access check address their round-3 examples. Loose files without matching receipt contents now remain. The manifest consumer rejects a symlink leaf, and a receipt name containing path separators cannot match an enumerated filename to authorize deletion outside the directory. The remaining cases follow.

## High

### H1. Runtime cleanup can delete retained legacy records

`app/src/uninstall/plan.rs:919`; `app/src/uninstall/plan.rs:645`; `app/src/uninstall/plan.rs:765`

Scenario: on an unprotected installation, set `XDG_CONFIG_HOME=HOME/custom-config` and `XDG_RUNTIME_DIR=HOME/.config`, with the other current roots separate. Leave an older settings/session tree at `HOME/.config/terminal-delight`. The expanded overlap matrix passes: it contains current config and runtime, but does not contain the legacy config tree as a retained root. Runtime cleanup then removes that older tree. In Export mode, the export holds only the current roots, while the plan explicitly reports that the older copy is kept because it was not exported. Keep mode also deletes it through the same runtime operation.

**CONFIRMED (traced).** The runtime and legacy paths are identical; `tree_ok` permits that tree. The deletion requires no symlink or concurrent change.

Suggested fix: derive the protected set from all records retained by the chosen policy, including legacy roots, and compare every resolved deletion target against that set.

### H2. Another user's manifest can authorize deleting this user's files

`app/src/uninstall/apply.rs:583`; `app/src/uninstall/apply.rs:603`; `app/src/art.rs:73`

Scenario: with runtime unset and a shared `/tmp`, another local user creates a world-readable `terminal-delight-assets.manifest` first. It contains the hash and name of the uninstalling user's unrelated, readable `terminal-delight-scene-storyboard.webp`. The consumer requires the manifest to be a regular file but never checks its owner or write permissions. It checks ownership only on the target file, then accepts the other user's supplied hash as TD's receipt and unlinks the target. The writer's creation mode does not authenticate a pre-existing manifest, and a failed append does not remove the planted file.

**CONFIRMED (traced).** A different account can supply the ownership evidence. This is outside the accepted limitation concerning a hostile process already running as the uninstalling account.

Suggested fix: store receipts in a private per-user location, authenticate the opened receipt's owner/type/permissions before trusting it, and refuse foreign or externally writable manifests.

### H3. The AppImage still is not bound to the verified payload

`app/src/uninstall/plan.rs:885`; `app/src/uninstall/plan.rs:903`; `app/src/uninstall/plan.rs:1264`

Scenario: native TD runs build A; `APPDIR` names a genuine extracted TD AppDir containing the same build A, while `APPIMAGE` names an unrelated valid `OtherEditor.AppImage`. The payload hash now correctly matches this running TD, but the unrelated image still passes its independent magic check. `proven_appimage` returns the pair, and removal schedules that other image without a hash. Staging does not change the result because the worker has build A's bytes too.

**CONFIRMED (traced).** The added check establishes the payload-to-worker relationship. No check establishes the image-to-payload relationship required by the round-3 finding.

Suggested fix: bind the image to the original payload through a verified backing-image relationship or trusted image hash before staging; otherwise preserve and report the image.

## Medium

### M1. A FIFO receipt can block TD's UI

`app/src/art.rs:68`; `app/src/docview/notes_ui.rs:1944`; `app/src/docview/notes_ui.rs:2554`

Scenario: the receipt path in the selected asset directory is an account-owned FIFO with no reader. On the first asset use, `record_asset` opens it for append without `O_NONBLOCK` or a regular-file check. Opening that FIFO for writing blocks indefinitely. The call runs synchronously from asset creation, including the notes renderer's `draw_marks` → `stamp_path` → `runtime_asset` path, so a bad receipt can freeze the UI even though recording ownership is meant to be best effort. `O_NOFOLLOW` rejects symlinks but permits a FIFO.

**CONFIRMED (traced).** The new write path has no timeout, nonblocking open or file-type validation before the potentially blocking open.

Suggested fix: open without blocking, validate the opened descriptor as an owned regular file before writing, and abandon receipt recording safely for any other type.

### M2. An unreadable MCP config becomes “already gone”

`app/src/uninstall/mcp.rs:148`; `app/src/uninstall/apply.rs:452`; `app/src/uninstall/plan.rs:515`

Scenario: the fresh plan sees a TD registration. Before its operation runs, the config becomes unreadable or temporarily malformed during another program's update. `current_entry` converts read and parse failures into `None`, the same value as an absent entry. The worker returns Done with “already gone,” skips the CLI and before copy, and permits dependent process/file removal. The registration can remain in the config while its TD executable is removed, with a successful cleanup outcome recorded for it.

**CONFIRMED (traced).** Every `.ok()?` in the new reader discards the difference between an inspection failure and a verified absence. The existing `read_json` helper already preserves that distinction during discovery.

Suggested fix: return `Result<Option<EntryIdentity>, Error>`, mark read/parse failures as Failed, and use “already gone” only for a successfully established absence.

### M3. A partly deleted records root is still called intact

`app/src/uninstall/apply.rs:573`; `app/src/uninstall/apply.rs:290`

Scenario: recursive deletion removes some entries from a records root, then fails on an inaccessible child. `remove_dir_all` returns an error after those earlier removals. The revised summary maps every non-Done root into `kept`, then says that root is “still in place.” Root-by-root reporting fixes the previous case where one whole root succeeded and another failed, but cannot infer intact records from a failed recursive operation. The overall partial status does not tell the reader which retained-looking root has already lost contents.

**CONFIRMED (traced).** The summary uses a boolean derived solely from Done; Failed and Blocked are both rendered as retained, although Failed can follow partial mutation.

Suggested fix: distinguish untouched/blocked roots from failed deletions and describe a failed recursive removal as potentially partial, naming the affected root.
