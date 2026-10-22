# Reproduce the Restricted security checks

Run from the repository root on Linux with bubblewrap 0.12.0+, the patched GPUI checkout required by `app/Cargo.toml`, and the ordinary build dependencies. Real OS tests require an owned 0700 `/run/user/UID` directory. Native tests additionally require the active Hyprland 0.56 session and its `HYPRLAND_INSTANCE_SIGNATURE`.

Use a short cache scratch path: two existing pane geometry tests print fixture paths into a fixed-width terminal. A long TMPDIR makes those paths wrap outside their click coordinates. Do not use `/tmp` when its user quota is full.

```sh
mkdir -p "$HOME/.cache/tdtmp"
export TMPDIR="$HOME/.cache/tdtmp"
```

Keep one Cargo build active at a time. Give this worktree an independent target directory. The qualification command removes application artifacts first, preserving dependency caches, and stops on any failure or source change. Every receipt contains the full input-file digest map, its combined fingerprint, HEAD at recording, commands and log digests. HEAD alone cannot describe uncommitted source; the digest map does.

```sh
python3 scripts/security/qualify.py --repo "$PWD" --out "$HOME/.cache/td-checks/final" --target "$HOME/.cache/td-checks/target"
```

The module mutation harness copies the actual Restricted sources and the real OS test target. Its only stub refuses host-pane grants, which the native run tests separately. Every mutation starts from the original source. A compiler failure is never a killed mutation.

```sh
python3 scripts/security/restricted-mutations.py --repo "$PWD" --out "$HOME/.cache/td-checks/module-mutations"
```

The application mutation runner temporarily edits this worktree. Run it without other builds or edits. It backs up and restores exact bytes, checks the source fingerprint, and runs the named tests again after restoration. `--native` adds actual picker-routing and ordinary-window startup checks; these use independent hidden sessions. The routing result comes from the running application, not a source-text search.

```sh
python3 scripts/security/application-mutations.py --repo "$PWD" --out "$HOME/.cache/td-checks/application-mutations" --target "$HOME/.cache/td-checks/target" --native
```

Run the native exercises against the exact selected binary. The Restricted run creates a dedicated non-hidden home directory as its grant, removes it afterward, and saves copied evidence under OUT. The permission run restores a private hidden pane in its own new session, checks genuine and forged callers, tests actual document/Workbench routing and tab edits, then checks all six policy refusals in a second ordinary window. Cleanup targets only those sessions. Neither script installs a binary or reloads the live window.

```sh
TD_AUTHORITY_BIN="$HOME/.cache/td-checks/target/release/terminal-delight" TD_AUTHORITY_OUT="$HOME/.cache/td-checks/native-permissions" bash scripts/security/permissions-native.sh
TD_AUTHORITY_BIN="$HOME/.cache/td-checks/target/release/terminal-delight" TD_AUTHORITY_OUT="$HOME/.cache/td-checks/native-restricted" bash scripts/security/restricted-native.sh
```

To check older-host compatibility, set `TD_AUTHORITY_OLD_HOST` to the exact earlier TD binary on the Restricted native invocation. The script starts that host only for its own disposable session and records its executable. All OUT directories should be fresh. Receipts from earlier sources remain historical; a later source hash does not relabel an earlier test run.
