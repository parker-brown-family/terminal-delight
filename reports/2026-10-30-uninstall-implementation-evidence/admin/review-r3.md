# Uninstall Review, Round 3

**9 findings: 0 critical, 7 high, 2 medium.** Reviewed `/home/parker/Work/td-uninstall-lab/terminal-delight`. CONFIRMED means traced through source; the scenarios were not executed. No builds, tests, TD execution, process signals or code changes. This report is the only file written.

The socket leaf and peer checks address the previous fixture shutdown case. The journal/icon operand checks, failed confinement status, prune recheck, staged-worker checks, receipt serialization change, duplicate-key rejection and MCP before copy address their specific round-2 cases. Several broader invariants still fail below.

I accept excluding a deliberately hostile same-account ancestor-swap attack from this release's threat model. That leaves a documented race limitation; it does not establish isolation against concurrent directory replacement. I disagree with accepting loose-asset deletion without ownership proof: the counterexample below requires no attacker or concurrent change.

## High

### H1. Runtime aliases bypass record retention

`app/src/uninstall/plan.rs:894`; `app/src/uninstall/plan.rs:635`; `app/src/uninstall/roots.rs:96`

Scenario: `XDG_RUNTIME_DIR` equals `XDG_CONFIG_HOME`, with state, data, cache and builds elsewhere. The five-root overlap check passes because runtime is absent from its matrix. Both `runtime()` and `config` then name the same `terminal-delight` directory. A Keep plan schedules its unconditional runtime `RemoveTree`, deletes the settings and sessions, and reports records kept. Setting runtime to the state base produces the same failure for state records. This works with static paths and with every fixture root inside HOME.

**CONFIRMED (traced).** Retention checks do not cover the full set of deletion targets; the runtime operation has no exception for retained records.

Suggested fix: compare every resolved deletion target, including runtime and temp fallbacks, against every tree retained by the selected policy before accepting the plan.

### H2. An output alias lets removal erase its own evidence

`app/src/uninstall/apply.rs:101`; `app/src/uninstall/plan.rs:861`; `app/src/uninstall/roots.rs:103`

Scenario: inside HOME, `tmp-alias` is an existing symlink to `tmp-real`; set `TMPDIR=HOME/tmp-alias` and choose the fresh output `HOME/tmp-real/terminal-delight-<uid>/removal`. The planned fallback deletion uses the alias spelling. `planned_removal` compares both the original and resolved output against the unresolved deletion path, so both comparisons miss. The additional `is_inside_td` check omits this temp fallback. The worker stages successfully, then its fallback `RemoveTree` erases the output, journal, log and original shared-file copies. Its final receipt cannot be saved there.

**CONFIRMED (traced).** Both preflight checks pass with a stable symlink; no ancestor swap is needed.

Suggested fix: resolve both operands when checking output containment against every planned deletion target.

### H3. An unreadable versioned process still permits removal

`app/src/uninstall.rs:520`; `app/src/uninstall/apply.rs:719`; `app/src/uninstall/apply.rs:775`

Scenario: on an unprotected account-home run, TD is running from a versioned `td-<build>` executable, but `/proc/<pid>/exe` is unreadable. The UID fallback now preserves the process as intended, with an empty executable path and its versioned name in `comm` or `argv[0]`. It cannot match `ours`, and the final unknown-process check recognizes only the exact basename `terminal-delight`. It therefore returns "none running" and permits export and record deletion while that process remains alive. Socket shutdown also declines a peer whose executable cannot be read, so it does not repair this path. The non-account-home branch similarly drops every empty executable before checking name hints.

**CONFIRMED (traced).** The new `td-*` matching requires a nonempty executable path; it does not participate in the uncertainty check.

Suggested fix: preserve unresolved TD candidates, including versioned names, as blockers in both worker modes, using the same identity policy as the builds scanner.

### H4. Loose assets still lack ownership proof

`app/src/uninstall/plan.rs:616`; `app/src/uninstall/apply.rs:536`; `app/src/uninstall/apply.rs:838`

Scenario: with no `XDG_RUNTIME_DIR`, another tool has written an account-owned `TMPDIR/terminal-delight-scene-storyboard.webp`. Its arbitrary content passes the scene-name predicate and is unlinked. With a runtime directory, the same problem remains for matching files in that shared base. Fixed names are also accepted regardless of content. Demo cleanup accepts any nonempty sequence of digits and hyphens, including `td-demo---.toml`, and always scans temp.

**CONFIRMED (traced).** Narrowing the directory removed one case, but name plus UID still authorizes deletion. TD writing embedded bytes does not establish that an existing file contains those bytes; `art.rs:35` even reuses an existing file after checking only its size. This contradicts the approved rule that unresolved TD-like files are retained.

Suggested fix: inventory and verify embedded asset contents against known hashes; retain demo files without a recorded ownership proof.

### H5. AppImage checks do not bind the image to its payload

`app/src/uninstall/plan.rs:875`; `app/src/uninstall/plan.rs:386`; `app/src/uninstall/plan.rs:1224`

Scenario: `APPIMAGE` names a valid unrelated `OtherEditor.AppImage`, while `APPDIR` names a TD AppDir containing its ELF payload. Both independent checks pass. The unrelated image becomes a TD executable target and gets a `RemoveFile` operation without a hash. Neither check establishes that the payload came from that image or that the original running TD came from either path. An ELF file named `usr/bin/terminal-delight` is likewise accepted without establishing its identity.

**CONFIRMED (traced).** AppImage magic proves the container format; the separate ELF test does not prove the relationship asserted by `proven_appimage`.

Suggested fix: authenticate the original running payload and its backing image before staging, carry that relationship into the plan, and retain an image when that relationship cannot be established.

### H6. MCP removal can delete a replacement registration

`app/src/uninstall/plan.rs:400`; `app/src/uninstall/apply.rs:408`; `app/src/uninstall/mcp.rs:48`

Scenario: the worker's fresh-plan check sees an exact TD MCP entry. While earlier cleanup steps run, the user or a configuration synchronizer replaces that entry with an unrelated server under the same name. `McpRemove` copies the current config and calls the client's remove-by-name command without checking the entry again. The replacement is removed. The operation carries no expected command, arguments or content fingerprint against which to validate it.

**CONFIRMED (traced).** The before copy makes restoration possible but does not enforce the exact-match-only removal contract. This is a normal shared-config update between operations; it does not require a hostile directory-swap attack.

Suggested fix: carry the reviewed registration identity and revalidate it immediately before the owner CLI call, using conditional removal or the client's locking mechanism where available and reporting any remaining race limitation.

### H7. A fixture HOME can contain the real account home

`app/src/uninstall/plan.rs:913`; `app/src/uninstall/guard.rs:85`; `app/src/uninstall/apply.rs:365`; `app/src/defaultterm.rs:258`

Scenario: a fixture declares `HOME=/home`, uses separate writable scratch config/data/cache/runtime/temp bases under `/home/<account>`, and inherits `XDG_STATE_HOME=/home/<account>/.local/state`. All roots resolve inside the declared HOME, and the five named roots are distinct, so `blocked` permits the plan. Because HOME differs from the passwd home, marker discovery omits the real account's default protection marker. If the real state contains a default-terminal journal, the first cleanup operation reads and removes that real journal when the scratch terminal list differs. The later process blocker cannot undo this earlier mutation.

**CONFIRMED (traced).** Canonical containment in an arbitrary HOME is insufficient when that HOME contains the protected account. The fixture's bin/builds paths can be absent; their existing prefix still resolves inside `/home`.

Suggested fix: reject a non-account HOME that contains the canonical passwd home, and ensure fixture targets cannot overlap the real protected installation before any mutation.

## Medium

### M1. Any execute bit is not executable access

`app/src/uninstall/mcp.rs:122`

Scenario: PATH first contains an account-owned TD launcher with mode `0641` (owner execute denied, other execute allowed), followed by an executable custom wrapper of the same name. The account cannot execute the first file, so command lookup proceeds to the wrapper. The new `mode & 0o111 != 0` check nevertheless selects the first file and classifies the wrapper's registration as TD, authorizing its removal.

**CONFIRMED (traced).** The predicate tests whether anyone has an execute bit, rather than whether the launching account can execute the candidate.

Suggested fix: use executable-access checks for the launching credentials and environment when resolving PATH.

### M2. Partial record deletion is reported as retained records

`app/src/uninstall/plan.rs:748`; `app/src/uninstall/apply.rs:274`

Scenario: Delete removes the config records successfully, then removal of the separate state records fails on permissions. The two operations have the same earlier prerequisites, so this partial result is possible. Because not every record operation is Done, `records_outcome` says "still there: their removal did not finish". Export has the same overstatement after a successful export and partial deletion. The overall receipt is partial, but its records sentence incorrectly implies all original records remain.

**CONFIRMED (traced).** A single failed record operation collapses the successful and failed roots into one retained outcome.

Suggested fix: report partial record removal explicitly and name which roots were removed, retained or only partly removed.
