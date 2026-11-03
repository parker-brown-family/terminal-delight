# Uninstall Review, Round 14

**1 finding: 0 critical, 1 high, 0 medium.** Reviewed `/home/parker/Work/td-uninstall-lab/terminal-delight` at `9a482808`, covering the round-13 changes and the surrounding ownership, deletion, process, guard and ADMIN paths. CONFIRMED means traced through source; no scenarios executed. No builds, tests, TD execution, signals or code edits. This report is the only file written.

The round-13 cases are addressed: default-terminal restoration creates its temporary file exclusively without following links and writes through that descriptor; directory, launcher and process blockers now feed removal eligibility, pruning and the displayed reason; an incomplete directory listing no longer produces a successful empty prune. The accepted same-account race limitation and bare-command PATH decision are not findings here.

## High

### H1. Failed AppImage Classification Still Authorizes File Deletion

Locations: `app/src/uninstall/plan.rs:953–957`, `plan.rs:1056–1064`, `plan.rs:1092–1097`, `plan.rs:1279–1300`; `app/src/uninstall/apply.rs:58–77`, `apply.rs:184–185`, `apply.rs:579–598`. Compare `app/src/uninstall/builds.rs:358–369` and the readable-image regressions at `app/src/uninstall/e2e.rs:1073–1118`.

Scenario: a dormant legacy type 1 AppImage named `td-release.AppImage` is stored in the builds directory. Its initial ELF header remains readable, but reading its ISO descriptor at offset 32769 returns an I/O error. A damaged data block or a filesystem serving that range with an error can produce this state while metadata and unlink still work. The same error exists before preview and persists through the worker's fresh-plan check. The image is not running, so process inspection supplies no blocker.

`ExeKind::of_reader` correctly returns an error when the later read fails. `plan::is_appimage` collapses that error to `false` through `is_ok_and`. In `builds_and_launcher`, the AppImage branch therefore fails, while the following `is_elf` check succeeds by reading only the first four bytes. The planner schedules `RemoveFile { sha256: None }` and calls the image a build.

The repeated classification error produces the same operations in the fresh plan, so plan equality passes. After ordinary shutdown and default-terminal checks succeed, `RemoveFile` checks only that the path is still a regular file. With no hash to check and no executable classification at execution, it unlinks the image even when records are set to Keep. No before-copy is made for this deletion.

The executable discovery and launcher-target predicates also use `is_elf && !is_appimage`, so the loss of the error is shared across those planning decisions. Old-build pruning already handles the same classifier error as `UNREADABLE` and retains the file. The current image regressions provide readable bytes for all three formats; they do not exercise failed classification.

**CONFIRMED (traced).** This is an existing read failure throughout the operation, with no concurrent replacement. The shared classifier preserves the uncertainty, but the planner's boolean wrapper turns it into permission to delete an AppImage that the removal policy promises to retain.

Suggested fix: preserve the classifier's result through planning and authorize build removal only on `Ok(ExeKind::Build)`. An error must retain/report the file as unknown. Use that result consistently for build discovery, launcher targets and deletion planning. Add an injected reader that supplies the ELF header but fails at the ISO descriptor, and assert that its error reaches the planner without creating a file or launcher removal step. Keep the readable type 1/type 2 cases and the successful ordinary-build case alongside it.
