# Uninstall Trust Boundaries

This is the entry point for changes to the removal engine. Read it before
`app/src/uninstall/`. It records what the current worker enforces, what its
tests establish, and which guarantee still needs a design decision.

Parker requested this follow-through on October 30 after reviewing the weekend
tie-off. He also instructed: “if you encounter a non-obvious decision - put a
pin in it and move on.” That instruction governs the open race below.

## Authority and Assets

The person authorizes a particular plan on a particular machine, for one home
and one retention choice. The worker runs with that account's permissions.
It must preserve other programs, project files, native agent histories,
unrecognized files, newer settings choices and records whose required export
failed. The Legion's protection marker forbids removal on the live machine.

An installed build, receipt, shared configuration file, executable name and
environment variable are inputs with different authority. None is a universal
proof that a path or process belongs to TD. A receipt is local provenance,
not a signature against a hostile process running as the same user.

## Who Can Change What

| Actor or condition | Writable or uncertain inputs | Required response |
|---|---|---|
| A person who edits settings after review | Shared settings, managed blocks, launcher, retention request | Re-plan and compare; preserve newer choices. |
| An unrelated tool or another account | TD-like filenames, shared runtime directories, writable receipts | Require recorded hashes and receipt ownership/mode; preserve unproven items. |
| An inherited launch environment | HOME, XDG directories, PATH, client-specific configuration roots | Resolve explicitly; confine fixture roots; pin each removal CLI to the configuration that was inspected. |
| A process that exits, changes executable or cannot be read | PID, executable, argv, process name and kernel state | Positively confirmed exited states are omitted. Otherwise use the running executable's bytes; unknown blocks removal. A name alone grants no signalling authority. |
| Storage and parser failures | Read results, sizes, counts, JSON, copy verification, journal writes | Keep unknown distinct from absent or zero; fail the operation and block its dependants. |
| Another process running as this account during removal | Directory ancestry, final entries, files after their checks | **Current limitation:** checks and pathname operations are separate. Atomic containment is not established. |

## Enforced Boundaries

The source and tests below are relative to the repository root. Test names are
the stable evidence; the turn's run logs live in
`reports/2026-10-30-uninstall-trust-evidence/`.

| Boundary | Enforcement | Executable evidence |
|---|---|---|
| A protected machine does not run a removal | `apply::refusal`, `guard::protection` | `e2e::a_protected_machine_refuses_and_changes_nothing` |
| The staged worker receives the exact reviewed bytes | `uninstall::spawn_worker` checks the digest before staging | `tests::a_changed_review_digest_cannot_stage_a_worker` changes only JSON whitespace and asserts no output directory was created |
| A fixture cannot reach another home through its configured roots | `plan` validates resolved roots; `apply::allowed` checks effects | `e2e::a_fixture_whose_directories_reach_outside_its_home_is_refused`; linked-socket and launcher-escape fixtures |
| A running program needs evidence beyond its name | `apply::is_td_exe`, `/proc/<pid>/exe` classification | Running-AppImage and unreadable-executable fixtures |
| File ownership needs matching bytes | `known` validates receipt ownership/mode and hashes; worker rechecks builds | `known::tests::a_receipt_others_can_write_is_not_believed`; `e2e::the_worker_removes_a_build_only_while_its_bytes_still_say_build` |
| A shared-file edit preserves everything outside the reviewed change | `settings`, `jsonorder`, `blocks`, `apply::edit` | Nested-hook, order-preservation, stale-plan and managed-block tests |
| A new MCP registration identifies the installed launcher independently of PATH | Setup commands save an absolute launcher; `mcp::is_td` resolves that path | `e2e::absolute_launcher_registrations_ignore_shadowing_path_for_every_client`; account-home shell fixture checks the plan and removal |
| Export and bookkeeping failure cannot report complete removal | `export` rereads copied bytes; `apply` blocks dependants and records failures | Export-failure, unknown-size and journal/receipt tests in the removal suite |

The protection, reviewed-byte and fixture-root guards were each disabled alone
in an isolated source copy. All three corresponding tests failed. The
reviewed-byte test has a second blocker so mutation testing cannot start a
worker. The source copy stubs host socket and systemd effects with panics;
those stubs were not reached. This focused test run does not qualify the GUI.

A real-binary rehearsal also exposed an exited child counted as live and
unreadable before its parent reaped it. A deterministic `waitid(WNOWAIT)`
fixture reproduced that mistake. The collector now excludes positively read
`Z` and `X` kernel states; failed state reads remain unclassified and retained.
`tests::a_confirmed_exited_child_is_not_a_live_unreadable_td` fails against the
old collector and when its new exclusion is disabled. This fourth mutation
was caught as well.

## Pinned Directory Policy

The [directory-handle race issue](https://github.com/parker-brown-family/terminal-delight/issues/1222)
remains open. `O_NOFOLLOW` protects a final component; it does not make a check
followed by a full-path edit or unlink atomic. Repeating the pathname check
does not close that window.

Before changing the worker, choose whether a legitimate symlinked XDG root is
accepted and anchored at its resolved destination, or refused. The design
must also specify what happens if an opened directory is moved outside its
original root, a final entry changes after its bytes were checked, or an
agent CLI resolves the configuration path again. A directory descriptor alone
does not answer all of these questions.

Acceptance for that work includes deterministic substitutions at each
check/action boundary, a moved-open-directory case, a final-entry replacement,
and the client-CLI edit boundary. Tests must show that unrelated sentinel
files survive. If the intended guarantee excludes hostile same-account
writers, say that explicitly and still test accidental concurrent changes.
Do not remove the limitation from status or review pages until those checks
pass against the chosen policy.

## Existing Registrations

The new setup commands save an absolute path for Claude, Codex and Gemini.
They do not migrate existing client configurations. The previously accepted
legacy behavior remains: a bare command without its own PATH is classified
using the remover's PATH and the plan says so. Wrappers, unresolved commands
and project `.mcp.json` files remain reported for manual handling.

## Verification Scope

Use the focused module tests for file and planning rules, the cleanup script
suite for installer behavior, and `uninstall-e2e.sh` for a detached real-binary
removal. Account-home mode belongs in a disposable account or the sandbox
wrapper. Never qualify it by removing TD from the Legion. Real-GNOME and the
other-Lenovo tests remain separate work with their existing issues.
