# Uninstall Review, Round 9

**3 findings: 0 critical, 2 high, 1 medium.** Reviewed `/home/parker/Work/td-uninstall-lab/terminal-delight`. CONFIRMED means traced through source; no scenarios executed. No builds, tests, TD execution, signals or code edits. This report is the only file written.

The supplied round-8 cases are addressed: ambiguous PATH components stop lookup, an explicit server working directory prevents automatic MCP removal, an existing readable AppImage is excluded from shutdown, and a failed directory walk no longer produces a zero size. Three cases remain outside those regressions.

## High

### H1. The uninstall process's PATH does not establish the MCP client's inherited PATH

Locations: `app/src/uninstall/mcp.rs:256`, `mcp.rs:283–293`; the same fallback is used for Codex and Gemini.

Scenario: TD was launched from the desktop with `/home/u/.local/bin:/usr/bin` as PATH. The person starts Gemini from a shell whose startup config prepends `/home/u/custom-mcp/bin`, containing their `terminal-delight` wrapper. The saved MCP entry is `command: "terminal-delight"`, `args: ["mcp"]`, with no explicit `env.PATH` or `cwd`. Both PATH values contain only absolute directories. Gemini's server inherits its host process's environment; the client's environment need not equal the desktop TD process's environment. [Gemini MCP environment handling](https://geminicli.com/docs/tools/mcp-server/)

The actual client resolves the wrapper. Discovery instead substitutes the uninstall process's PATH, finds the real TD launcher and schedules removal of the wrapper registration. The worker's identity comparison sees unchanged command, arguments and absent PATH, so it proceeds. The new relative-component and explicit-cwd checks do not apply.

**CONFIRMED (traced against the documented client environment contract).** This is a static, valid registration and requires no concurrent change. The inherited client environment is unavailable to the planner; using the reviewer's environment supplies an unproved identity.

Suggested fix: treat a bare command without an independently established client PATH as unresolved and report it for manual removal. Continue automatic removal for a verified absolute executable, or a fully specified supported launch environment. Add a case where TD and the MCP client have different absolute-only PATH values.

### H2. A deleted AppImage becomes a signal target again

Locations: `app/src/uninstall/apply.rs:897–909`; `app/src/uninstall/plan.rs:930–937`; `app/src/uninstall.rs:417–420`, `uninstall.rs:436–451`.

Scenario: an unrelated `td-other.AppImage` was launched from TD's builds directory, then its outer file was unlinked while the process remained running. This can be the initial state before the preview; no change during uninstall is required. `/proc/<pid>/exe` still identifies the running executable with a ` (deleted)` suffix, which `exe_of` strips.

The executable is absent from the reviewed list. The directory/name fallback nevertheless accepts its old `td-` path because `is_appimage` returns false when that path cannot be opened, and `!is_appimage` therefore grants TD identity. Shutdown sends TERM and may escalate to KILL. The pidfd check confirms that the same process still has that deleted pathname; it does not establish that the process is TD. An unreadable image header has the same negative-check problem.

**CONFIRMED (traced).** The regression leaves the image file readable on disk. It therefore exercises the recognized-AppImage case, not the unknown executable type that the new boolean predicate treats as a build.

Suggested fix: distinguish a confirmed non-AppImage build from unavailable executable evidence. Inspect the running executable through `/proc/<pid>/exe` or equivalent retained evidence; when ownership cannot be established, retain/report or block rather than signal. Preserve that distinction when handling deleted TD builds, so declining to signal an unknown process does not silently authorize record deletion around it.

## Medium

### M1. A metadata permission error still makes the plan claim records are absent

Locations: `app/src/uninstall/plan.rs:1103–1105`; `app/src/uninstall.rs:661–668`.

Scenario: a records directory exists beneath an ancestor this account cannot traverse, such as an inaccessible XDG state base. `dir_size` returns no measurement. The new Markdown renderer calls `symlink_metadata`, gets a permission error, and prints “not present” because every metadata error follows the same branch as an absent path.

**CONFIRMED (traced).** The fixed case has a stat-able root and an unreadable child. Failure to stat the root itself remains unknown, not evidence that there are no records. ADMIN now preserves unknown correctly, while the review document still loses that distinction.

Suggested fix: render absence only for a confirmed NotFound result; preserve other errors as unknown. Prefer carrying presence and measurement status in the plan so rendering does not re-infer them from a second filesystem lookup. Add an inaccessible-ancestor case.
