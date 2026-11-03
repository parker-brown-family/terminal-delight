# Uninstall Review, Round 10

**2 findings: 0 critical, 2 high, 0 medium.** Reviewed `/home/parker/Work/td-uninstall-lab/terminal-delight` at `b75de3b1`. CONFIRMED means traced through source; no scenarios executed. No builds, tests, TD execution, signals or code edits. This report is the only file written.

The supplied round-9 cases are addressed: shutdown reads the running executable through `/proc/<pid>/exe`, unreadable executable evidence blocks removal, and the plan preserves unknown directory presence. The decision to resolve bare MCP commands on the removal process's PATH is accepted for this review. The first finding below deletes a registration absent from the before-copy, even when the inspected command is absolute.

## High

### H1. Gemini can remove an unrelated registration from a file TD never backed up

Locations: `app/src/uninstall/mcp.rs:77–86`, `mcp.rs:372–405`; `app/src/uninstall/apply.rs:473–504`; `app/src/uninstall.rs:380–395`, `uninstall.rs:485–503`.

Scenario: on an unprotected machine, HOME is the account's real home, `/home/u`, and the removal process inherits `GEMINI_CLI_HOME=/home/u/gemini-work`. Two files contain a server named `terminal-delight`:

- `/home/u/.gemini/settings.json` registers the verified absolute TD executable with arguments `["mcp"]`.
- `/home/u/gemini-work/.gemini/settings.json` registers an unrelated custom server, with its own command, arguments and environment.

Discovery always reads `HOME/.gemini/settings.json`. The worker checks that default file's entry and copies that file into the removal directory. It then runs `gemini mcp remove -s user terminal-delight`, inheriting `GEMINI_CLI_HOME` through both worker launches.

Gemini derives its user settings location from `GEMINI_CLI_HOME`, and its removal command deletes the named entry from that user scope. The alternate registration is deleted; the inspected default registration remains. The saved before-copy contains none of the deleted server's configuration. [Gemini home resolution](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/utils/paths.ts), [settings path](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/config/storage.ts), [user settings binding](https://github.com/google-gemini/gemini-cli/blob/main/packages/cli/src/config/settings.ts), [MCP removal](https://github.com/google-gemini/gemini-cli/blob/main/packages/cli/src/commands/mcp/remove.ts).

**CONFIRMED (traced through TD and the client's source).** No PATH disagreement, concurrent edit or symlink substitution is required. The account-home guard passes because HOME itself is unchanged. Removal is not reversible from the before-copy, satisfying the work order's exception.

Suggested fix: bind discovery, final identity verification, backup and the client invocation to the same effective Gemini configuration root. Honor `GEMINI_CLI_HOME`, or retain/report when that binding cannot be established. Add a regression with both files present and different servers under the same name; verify the actual edited file is the one inspected and copied.

### H2. Type 1 AppImages still qualify for deletion and shutdown

Locations: `app/src/uninstall/plan.rs:953–960`, `plan.rs:1273–1292`; `app/src/uninstall/builds.rs:316–345`; `app/src/uninstall/apply.rs:53–58`, `apply.rs:561–579`, `apply.rs:930–942`.

Scenario: a type 1 AppImage named `td-other.AppImage` sits in TD's builds directory. Its ELF header carries `AI\x01` at offset 8. Type 1 is a defined AppImage format; the specification recommends that marker. [AppImage format specification](https://github.com/AppImage/AppImageSpec/blob/master/draft.md#type-1-image-format)

All three classifiers recognize only `AI\x02`. The resulting paths are:

- Uninstall classifies the image as an ELF build and schedules `RemoveFile` with no hash. The worker unlinks it.
- Old builds lists it as a build. If it is outside the newest three, is not the launcher target and is not running, the repeated `judge` check still accepts it and prune deletes it.
- If the image runtime is running from that path, `ExeKind::of_head` returns `Build`. The builds-directory fallback admits it to TERM/KILL, including when the file has already been deleted.

**CONFIRMED (traced).** The current regressions exercise only the type 2 marker. Reading through `/proc/<pid>/exe` fixes stale pathname evidence, but the new classifier still grants build identity to the type 1 header.

Suggested fix: share AppImage classification across planning, pruning and process inspection, and retain recognized type 1 images as well as type 2. Add type 1 cases for each of the three paths above. The type 1 marker is optional in the specification, so also account for legacy ISO-format images without it before claiming that every remaining ELF is a non-AppImage build.
