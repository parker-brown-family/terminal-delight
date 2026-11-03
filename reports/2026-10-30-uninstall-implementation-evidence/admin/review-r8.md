# Uninstall Review, Round 8

**3 findings: 0 critical, 2 high, 1 medium.** Reviewed `/home/parker/Work/td-uninstall-lab/terminal-delight`. CONFIRMED means traced through source; no scenarios executed. No builds, tests, TD execution, signals or code edits. This report is the only file written.

The export destination check now covers runtime fallbacks and legacy records. Launcher removal resolves its target before judging it. Install receipts are opened without following links or blocking on FIFOs, then checked and read through the same descriptor. Those fixes address their reported cases. The MCP and AppImage changes leave two identity gaps below; the reporting pass also found an unknown-size case presented as zero.

## High

### H1. Skipping relative PATH entries still authorizes removing a wrapper registration

Locations: `app/src/uninstall/mcp.rs:121–126`, `mcp.rs:371–382`; `app/src/uninstall/apply.rs:462–476`.

Scenario: a Gemini registration sets `command: "terminal-delight"`, `args: ["mcp"]`, `cwd: "/home/u/custom-mcp"`, and `env.PATH: ".:/home/u/.local/bin"`. The executable `custom-mcp/terminal-delight` is the person's wrapper; the later absolute PATH directory contains the real TD launcher. Gemini supports both the server environment and its working directory. [Gemini MCP configuration](https://geminicli.com/docs/tools/mcp-server/)

The client finds the wrapper in the first PATH entry. `is_td` discards that entry because it is relative, continues to the real TD launcher, and calls the registration TD's. The worker's command/args/PATH revalidation succeeds because the registration has not changed, then deregisters the wrapper. The same failure occurs with an empty PATH component, which denotes the working directory. Environment-variable expansion in a server PATH also remains unmodelled.

**CONFIRMED (traced).** Ignoring an unresolved search location does not establish that the client would skip it. This is a static registration, with no race.

Suggested fix: preserve the client's search order and effective launch context, including expansion and working directory. When an earlier component cannot be resolved reliably, retain/report the registration instead of selecting a later TD executable. Add a relative-before-absolute regression case.

### H2. AppImages excluded from the build list can still be signalled as TD

Locations: `app/src/uninstall/plan.rs:1036–1059`; `app/src/uninstall/apply.rs:793–806`, `apply.rs:897–907`.

Scenario: an unrelated AppImage named `td-other.AppImage` is stored in TD's builds directory and is running. The new inventory excludes it and reports that nothing proves it is TD's. Nevertheless, `is_td_exe` accepts any process executable whose parent is the builds directory and whose name starts with `td-`, even when it is absent from the reviewed executable list. `stop` consequently sends TERM and, if needed, KILL to the unrelated AppImage runtime.

There is a second route into the same wrong identity: when the launcher symlink points to that AppImage, `td_executables` adds it back through the symlink branch. That branch still tests only resolved location and ELF format, without the new AppImage exclusion. This also makes an MCP registration resolving to that image eligible for deregistration, although launcher removal itself now retains the link.

**CONFIRMED (traced).** Excluding the image from `StopProcesses.exes` is insufficient because execution independently admits it again. The new test checks the vector and build listing, but does not exercise this process predicate or the launcher-link route.

Suggested fix: apply one ownership policy to build discovery, launcher resolution, MCP identity and process shutdown. A known AppImage must not regain TD identity through the directory/name fallback; an unprovable running executable should be reported or block removal, never become a signal target. Test a running AppImage with an empty explicit executable list and a launcher pointing to it.

## Medium

### M1. Unreadable record trees are presented as zero bytes

Locations: `app/src/uninstall/export.rs:226–240`; `app/src/admin_view.rs:99–105`, `admin_view.rs:357–360`; `app/src/uninstall/plan.rs:1103–1126`.

Scenario: config is empty and the state tree contains records but cannot be listed, for example because its directory permissions deny this account read access. `plan_size` discards the walk error and returns `(0, 0)`. ADMIN unconditionally wraps the bytes in `Some`, then displays `0 B`. The plan's separate `dir_size` similarly skips failed directory reads and returns `Some(0)` for a wholly unreadable tree, or a seemingly complete total after a partial read failure.

**CONFIRMED (traced).** A real export correctly refuses the unreadable source, but the size shown before the person's retention/export decision has already converted “not measured” into zero. Describing this as an estimate does not distinguish an empty tree from an unreadable one.

Suggested fix: return an explicit unavailable or partial measurement with the read error, preserve that state through ADMIN and plan serialization, and render “unavailable” or a labelled lower bound. Add unreadable-root and partially unreadable-subtree cases.
