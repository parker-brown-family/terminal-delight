# Uninstall and ADMIN — hindsight, invariants, wishlist

Written at tie-off on 2026-10-30, after fifteen adversarial review rounds by the
Codex sibling (reports in `reports/2026-10-30-uninstall-implementation-evidence/admin/`).
Read [the threat model](03-threat-model.md) before changing anything under
`app/src/uninstall/`; this file preserves the history that led to it.

## What I would do differently

1. **Write the threat model before the first line of the engine.** The fifteen
   rounds found the same six classes over and over, each in a new place:
   a fixture run reaching the live machine; identity decided by a name or a
   path instead of bytes; unknown shown as zero, absent or a count; an
   environment variable moving a client's configuration; another account
   able to write into one of this account's directories; a classifier error
   collapsed into a boolean. Written down on day one as "who can write where,
   and what proves ownership", most rounds would not have happened. The
   invariants below are that list, late.
2. **Test the whole removal on a real account home from the start.** The first
   real removal ran at the very end, in a sandbox. It found nothing new, but
   it would have made rounds one to fourteen cheaper to verify.
3. **Batch pushes during a review loop.** Every push cancelled the branch's
   running CI, so for an afternoon no head had a verdict.
4. **Break the guards on purpose.** The mutation pass below came last; it
   belongs next to each fix.

## Invariants, and the test that pins each

| Invariant | Test |
|---|---|
| A protected machine refuses, in the worker, the panel and the handoff | `e2e` protected cases; `uninstall-e2e.sh` "a protected machine refused" |
| A fixture home never reaches outside itself | `e2e::a_fixture_launcher_cannot_make_a_live_build_a_target`, the overlap and linked-socket cases |
| What runs is the bytes reviewed | `e2e` stale-plan and edited-plan cases; `run_verb` hash check |
| A running program is TD's only on its own bytes (`/proc/<pid>/exe`) | `e2e::a_running_appimage_in_the_builds_directory_is_never_signalled`, `a_program_in_the_builds_directory_that_cannot_be_read_blocks_instead_of_being_signalled` |
| Every AppImage format is kept; a read error proves nothing | `e2e::an_appimage_in_the_builds_directory_is_never_a_build`, `a_classifier_that_cannot_finish_reading_proves_nothing` |
| A build is removed only while its bytes still say build | `e2e::the_worker_removes_a_build_only_while_its_bytes_still_say_build` |
| The launcher's target meets the scan's build test | `e2e::a_launcher_pointing_at_a_persons_own_program_in_the_builds_directory_keeps_both` |
| Ownership needs a hash: a receipt this account wrote and only it can write, or the shipped table | `known::tests::a_receipt_others_can_write_is_not_believed`, `ownership_needs_a_hash_match_not_a_name` |
| An MCP entry is TD's only by its own PATH and cwd; the client edits the file that was read | `e2e::a_registration_whose_path_starts_relative_or_with_its_own_cwd_is_reported`, `mcp::tests::gemini_is_read_and_edited_where_gemini_cli_home_points` |
| Shared files are edited only when they print back byte-identical; a temporary is never followed or reused | `settings`, `jsonorder` tests; `defaultterm::tests::restoring_never_writes_through_a_planted_temporary_link` |
| Unknown is never zero, absent or a count | `e2e::an_unreadable_tree_has_no_size_rather_than_zero`, `a_records_folder_that_cannot_be_looked_at_is_unknown_not_absent`; `builds::tests::the_cli_says_unknown_…`, `an_unreadable_builds_directory_is_a_refused_prune_not_an_empty_one`; `admin_view::tests::*` |
| The whole removal works on a real account home | `scripts/tests/uninstall-e2e.sh --account-home` in CI (a user made for the job) and through `uninstall-e2e-sandbox.sh` locally |

## Guards that earn their place

Each guard was broken on purpose and its test run (`~/.cache/td-uninstall-admin/mutate.py`
at tie-off; results in the commit that added this file):

| Guard broken | Test that caught it |
|---|---|
| `is_td_exe` accepts any kind | `a_running_appimage_in_the_builds_directory_is_never_signalled` |
| `known::receipts` believes any regular file | `a_receipt_others_can_write_is_not_believed` |
| `RemoveBuild` removes whatever the bytes say | `the_worker_removes_a_build_only_while_its_bytes_still_say_build` |
| `removable` ignores `Listing.blocked` | `the_cli_says_unknown_instead_of_a_removable_count_while_a_possible_td_is_unreadable` |
| the launcher branch skips the build test | `a_launcher_pointing_at_a_persons_own_program_in_the_builds_directory_keeps_both` |

Five of five caught in the original run. The October 30 follow-through also
disabled the protected-machine refusal, reviewed-bytes hash and fixture-root
confinement guards, one at a time. All three were caught; the mutation diffs,
test names and failure logs are in
`reports/2026-10-30-uninstall-trust-evidence/`.

## Wishlist, each a falsifiable issue

- Act relative to directory handles, closing the same-account race — #1222
- Register TD's MCP server by its launcher path on the setup page — implemented
  for Claude, Codex and Gemini in the October 30 follow-through; tracked by #1223.
- A scheduled contract test through the real agent CLIs instead of a stub — #1224
- The first real-hardware removal, on the other Lenovo — #1225
- GNOME shortcuts validated on a real GNOME — #1212
- `scripts/land.sh` installs the pull request's base branch (fixed in #1219; was #1220)
