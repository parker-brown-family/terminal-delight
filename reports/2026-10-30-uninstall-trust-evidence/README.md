# Uninstall Trust Evidence

The production changes and regressions are in `app/src/uninstall.rs` and
`app/src/uninstall/e2e.rs`. Full application tests in CI compile those tests
in the actual binary. The focused local harness compiles the uninstall,
default-terminal and test-scratch modules unchanged, with host socket and
systemd functions replaced by panic-only stubs that these tests do not call.
It is a focused verification, not a replacement for application qualification.

- `module-suite.log`: 101 passed, one existing ignored read-only test of the
  real build directory. No GUI is linked in this run.
- `mutations.json` and `mutation-*.log`: four separate guard removals, their
  exact substitutions and the corresponding failed tests. Each source copy
  was restored immediately after its run.
- `exited-child-repro.rs` and its log: a diagnostic test appended only to the
  isolated copy of the old collector. `waitid(WNOWAIT)` confirms exit without
  reaping, proving the child is a zombie before collecting processes. The old
  collector returns it with empty executable and unknown kind. The permanent
  regression is `a_confirmed_exited_child_is_not_a_live_unreadable_td`.
- `real-clients.json`: installed Claude Code 2.1.296, Codex 0.162.1 and Gemini
  0.63.0 accept the absolute launcher and remove it while preserving another
  server. Bubblewrap gives each command a private account home, read-only
  client binaries, private processes and no network. No agent runtime or
  shared Codex service is started.
- `account-e2e.log` and `fixture-e2e.log`: successful rehearsals with the
  installed weekend binary copied into disposable installations. The account
  run checks that the absolute registration has no PATH caveat before it is
  removed. These logs validate the setup change; the new collector's code is
  qualified separately by source tests and CI.

Reproduce the permanent tests in a normal prepared application checkout:

```bash
cargo test --locked --bin terminal-delight uninstall::
```

Run the removal modes with a built binary using
`scripts/tests/uninstall-e2e.sh` and `scripts/tests/uninstall-e2e-sandbox.sh`.
Never invoke account-home mode directly on the Legion. The first account
rehearsal hit the exited-process false positive, which motivated the
deterministic reproduction. Later baseline and final rehearsals passed;
those passes alone do not disprove the race.
