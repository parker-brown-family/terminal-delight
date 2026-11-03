# Execution slices and verification

1. Reshape the existing policy patch for all unscripted global mutations. Fix pointer/probe off-limits selection. Tests fail on old behavior and pass for allowed targets.
2. Launch an offline command with no TD rights. Verify allowed files/build; deny host canaries, services, inherited credentials, host processes, terminal control output and network. Fail setup without running the command.
3. Add the per-run own-note broker and external status/revoke. Reject forged targets, policy changes, delegation, oversized/slow requests, repeats and post-revoke effects. Verify useful notes and grant reset.
4. Run both terminal-core suites, format, strict production lints, release build, isolated native exercise, browser report QA and CI. Carry actual pass/fail/unsupported/unmeasured results into one page and the existing PR. Integrate only into dev/0.5.0 after checks pass.

No launch GUI, online-provider credential service, TUI compatibility or constrained native-agent recipient is claimed by this first profile. Original mockups remain interaction research, not implemented screenshots.
