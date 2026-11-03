# Uninstall Review, Round 7

**5 findings: 0 critical, 3 high, 2 medium.** Reviewed `/home/parker/Work/td-uninstall-lab/terminal-delight` across planning, execution, roots and protection, ownership, shared settings, MCP, export, build pruning and ADMIN callers. CONFIRMED means traced through source; no scenarios executed. No builds, tests, TD execution, signals or code edits. This report is the only file written.

The regular-file launcher fix addresses the three round-6 cases: a matching TD copy receives a content hash, while the AppImage and compiled wrapper at that path remain. The earlier single-descriptor loose-asset receipt fix also remains in place. The broader pass found the following gaps.

## High

### H1. MCP ownership ignores the server's own PATH override

Locations: `app/src/uninstall/mcp.rs:218`, `mcp.rs:319–338`, `mcp.rs:118–132`, `mcp.rs:192–197`; `app/src/uninstall/apply.rs:462–475`.

Scenario: Gemini has this user-scope registration, and `/home/u/custom-mcp/bin/terminal-delight` is the person's wrapper:

```json
{"mcpServers":{"terminal-delight":{"command":"terminal-delight","args":["mcp"],"env":{"PATH":"/home/u/custom-mcp/bin"}}}}
```

The uninstall process's PATH finds the real TD launcher. Discovery passes that PATH to `is_td` and never reads the entry's `env`, so it classifies the wrapper registration as TD. The worker's final check compares only command and arguments, then calls `gemini mcp remove -s user terminal-delight`. The person's wrapper registration is removed even though the approved contract says wrappers remain. Gemini explicitly supports server environment overrides and a server working directory. [Gemini MCP configuration](https://geminicli.com/docs/tools/mcp-server/)

**CONFIRMED (traced against the documented client contract).** This requires no configuration change during removal. The same missing launch context also matters when PATH contains relative entries interpreted from a different working directory.

Suggested fix: resolve against the server's effective environment and working directory, carry that identity into revalidation, and report the registration when its actual executable cannot be established.

### H2. Export accepts destinations that uninstall subsequently deletes

Locations: `app/src/uninstall/export.rs:536–550`; `app/src/uninstall/roots.rs:103–124`; `app/src/uninstall/plan.rs:657–662`, `plan.rs:777–789`, `plan.rs:883–887`.

Scenario: the account has an existing `TMPDIR/terminal-delight-<uid>` fallback directory. An explicit ADMIN export destination inside it passes `check_dest`, because `Roots::td_dirs` omits the fallback. The export reports a verified copy. A subsequent uninstall unconditionally schedules removal of the entire fallback directory, including that export. If the person chooses Delete after making the export, both the originals and the verified backup disappear.

There is a second static case: after moving the current XDG config/state elsewhere, export accepts a destination inside the old `~/.config/terminal-delight` or `~/.local/state/terminal-delight`. Those roots are absent from `td_dirs`, but Delete explicitly removes them.

**CONFIRMED (traced).** The removal-directory containment repair does not protect independently created exports. These paths require no symlink race or change of environment between export and removal.

Suggested fix: make export reject every directory any removal mode can delete, including runtime fallbacks and legacy records, using resolved containment for both destination and partial directory.

### H3. The builds path still deletes AppImages without establishing their identity

Locations: `app/src/uninstall/plan.rs:400–409`, `plan.rs:1033–1037`, `plan.rs:1233–1246`; `app/src/uninstall/apply.rs:532–550`.

Scenario: the person keeps a TD AppImage as `~/.local/lib/terminal-delight/td-release.AppImage` and runs it there. The AppImage report promises that removal never deletes it. Nevertheless, the builds scan accepts its `td-` name and ELF header, adds the outer image to TD executable targets, and schedules its deletion with no hash. The new launcher-only exception never examines this path. An unrelated AppImage stored with a `td-` name in that directory is accepted by the same predicate.

**CONFIRMED (traced).** The dedicated AppImage operation is gone, but the promised manual-only policy is still bypassed through generic build handling. The round-6 regression covers only the launcher location.

Suggested fix: exclude AppImage containers consistently from build identity, uninstall removal and old-build pruning while preserving recognition of verified TD payload executables; add the build-directory variant to the regression cases.

## Medium

### M1. The symlink launcher check accepts a target outside the builds directory

Locations: `app/src/uninstall/plan.rs:1266–1283`; compare `plan.rs:1027–1031`; `app/src/uninstall/apply.rs:553–566`.

Scenario: the launcher points to the absolute path `/home/u/.local/lib/terminal-delight/../custom-wrapper`. Its resolved target lies outside TD's builds directory. Executable discovery correctly declines it, but launcher removal checks only the lexical `starts_with` prefix, accepts the target and unlinks the person's launcher. A target inside the builds directory that is itself a symlink to an external wrapper has the same discrepancy.

**CONFIRMED (traced).** The worker only checks that the link still contains the reviewed target; it never checks where that target resolves. This contradicts the stated rule that an unexpected launcher is reported and retained.

Suggested fix: use the same resolved, verified target predicate for launcher discovery and removal, retaining links whose ownership or resolved destination is unresolved.

### M2. A FIFO install receipt still blocks preview and the UI

Locations: `app/src/uninstall/known.rs:65–84`; `app/src/uninstall/plan.rs:228`; `app/src/admin_view.rs:203–207`, `admin_view.rs:221–225`.

Scenario: `<state>/receipts/broken.receipt` is a FIFO with no writer, or a symlink to one. `known::receipts` selects it by extension and calls blocking `read_to_string` without checking the opened file type. Both preview and planning wait indefinitely. ADMIN review and agent handoff call planning synchronously on the interface thread, so clicking either freezes the TD interface before any removal starts.

**CONFIRMED (traced).** The fixed `apply::read_receipt` is a different reader for loose assets; install receipts still use the unrestricted reader.

Suggested fix: open install receipts with nonblocking, no-follow flags, validate the opened descriptor as a trusted regular file, and report unreadable or invalid receipts without treating them as usable ownership evidence.
