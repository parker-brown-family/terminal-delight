# Uninstall Review, Round 6

**1 finding: 0 critical, 1 high, 0 medium.** Reviewed `/home/parker/Work/td-uninstall-lab/terminal-delight`. CONFIRMED means traced through source; no scenarios executed. No builds, tests, TD execution, signals or code edits. This report is the only file written.

The dedicated AppImage deletion and mount parsing are gone. The receipt reader now checks the opened descriptor's type, owner and permissions, then reads through that same descriptor; that addresses the previous separate-lookup finding. One generic removal path still defeats the AppImage policy.

## High

### H1. A regular-file launcher can still delete the AppImage and misidentify a compiled wrapper as TD

Locations: `app/src/uninstall/plan.rs:400–409`, `plan.rs:1017–1033`, `plan.rs:1041–1053`, `plan.rs:1269–1277`; `app/src/uninstall/apply.rs:532–550`; `app/src/uninstall/mcp.rs:103–133`.

Scenario: on an unprotected account, the person copies the TD AppImage itself to `~/.local/bin/terminal-delight` and runs it there. It is a regular ELF file, not a symlink. The AppImage report says “the removal never deletes it,” but `builds_and_launcher` independently accepts that same path on `is_elf` alone and schedules `RemoveFile { sha256: None }`. The worker checks that it remains a regular file and deletes it. Removing the dedicated AppImage operation therefore does not establish the promised manual-only policy.

The same predicate also accepts a user-written compiled launcher at that path. `is_elf` proves only that a regular file starts with the four ELF magic bytes. `td_executables` promotes it into the list of TD executables, and MCP matching then treats a registration resolving to that wrapper as an exact TD registration. The wrapper is also scheduled for deletion without content verification. This conflicts with the approved contract that wrappers are reported and never edited (`docs/plans/uninstall/01-admin-build.md`, line 8).

**CONFIRMED (traced).** Both cases require only an existing regular file at the launcher path. Neither depends on a concurrent replacement or defeating the new AppImage payload check. The symlink protections do not run for these files.

Suggested fix: require verified TD ownership before admitting a regular launcher to executable targets or deletion, retain unresolved wrappers, and apply the manual-only AppImage policy to every generic removal path as well. Preserve and recheck the verified content hash for any regular launcher that is eligible for removal.

Regression cases to add: an AppImage copied directly to the launcher path remains after removal; a compiled user wrapper remains and its MCP registration is reported rather than deregistered; a positively identified TD launcher still receives the intended cleanup.
