# Uninstall Review, Round 15

**0 findings: 0 critical, 0 high, 0 medium.** Reviewed `/home/parker/Work/td-uninstall-lab/terminal-delight` at `c0ceeb50`, covering the round-14 changes and their planning, launcher, worker, dependency and receipt paths. Verification below means source tracing; no scenarios executed. No builds, tests, TD execution, signals or code edits. This report is the only file written.

Nothing real survives this pass under the work order's bar.

## Verification

- **Failed classification no longer authorizes deletion.** `plan::exe_kind_of` preserves failure as `None`. Build discovery, `is_build_file` and `builds_and_launcher` require a positive `Build` result. The earlier case with a readable ELF header and a failed ISO-descriptor read cannot produce a build-removal operation. Locations: `app/src/uninstall/plan.rs:961–977`, `plan.rs:1076–1083`, `plan.rs:1111–1115`, `plan.rs:1297–1325`.
- **The worker independently checks the build before removal.** `RemoveBuild` requires a regular file, opens it with `O_NOFOLLOW | O_NONBLOCK`, and unlinks only after successful classification as `Build`. Other formats and read errors produce a failed step and retain the file. Locations: `app/src/uninstall/apply.rs:600–629`.
- **The new operation retains the existing dependencies and reporting.** Build removal still waits for shutdown and the default-terminal check. A failed build removal blocks the dependent directory removal and chosen record deletion, appears in the journal and receipt, and prevents a complete result. The operation has a human-readable summary. Locations: `app/src/uninstall/plan.rs:574–582`, `plan.rs:734–737`, `plan.rs:800–807`, `plan.rs:1312–1316`, `plan.rs:1336–1342`; `app/src/uninstall/apply.rs:275–320`, `apply.rs:1082`.
- **Regression source covers the reported conditions.** The added cases exercise a classifier error at the ISO descriptor, an unreadable file during planning, and a planned build whose bytes become an AppImage before the worker acts. The existing image regression now checks both file-removal variants. These tests were inspected, not run. Locations: `app/src/uninstall/e2e.rs:1095–1099`, `e2e.rs:1329–1385`.

The accepted same-account race limitation and bare-command PATH decision remain outside the findings. This result does not establish live qualification on Omarchy or another Linux installation; no live qualification was performed here.
