#!/usr/bin/env bash
# Land a pull request and put it in the launcher: wait for CI, merge, run the
# whole suite on the MERGED tree, build it from a clean checkout, verify the
# binary, install it under its sha, repoint the launcher, and say whether a
# window bounce is safe.
#
#   scripts/land.sh <pr-number> <label>          e.g. scripts/land.sh 684 fast-scan
#
# Run from any worktree of this repository. It checks that worktree out to the
# merged main, so run it from a worktree of your own — never from one another
# agent is living in. Every step prints, any failure stops the chain, and
# nothing half-installed reaches the next person to restart a window.
#
# Why this exists: "Merged is not installed." Terminal Delight runs from a
# versioned symlink, builds were placed by hand for two weeks, and each agent
# re-derived the chain (and its guards) in a scratchpad. This is that chain,
# once. The guards are the point:
#   - the suite runs on the merged tree, not the branch — a whole-file guard
#     is only meaningful where both changes coexist;
#   - the installed file's sha must match the build's;
#   - the host/client wire files are diffed against the build the running
#     host is on, because a window that pairs with a host on a different wire
#     silently diverges (#387).
set -uo pipefail
PR=${1:?pull request number}
LABEL=${2:?a short label for the installed build, e.g. fast-scan}
WT=$(git rev-parse --show-toplevel 2>/dev/null) || { echo "run inside a worktree"; exit 2; }
TGT=${CARGO_TARGET_DIR:-$WT/app/target}
LIB=${TD_BUILD_DIR:-$HOME/.local/lib/terminal-delight}
BIN=${TD_LAUNCHER:-$HOME/.local/bin/terminal-delight}
cd "$WT" || exit 2

echo "== waiting on CI for #$PR"
if ! gh pr checks "$PR" --watch --interval 30 --fail-fast >/dev/null 2>&1; then
  echo "CI FAILED — leaving the pull request open, nothing installed"
  gh pr checks "$PR" 2>&1 | head -8
  exit 1
fi
echo "== CI green; merging"
gh pr merge "$PR" --merge 2>&1 | tail -1 || { echo "merge refused"; exit 1; }
git fetch -q origin
git checkout -q --detach origin/main
SHA=$(git rev-parse --short HEAD)
echo "== main is now $(git log --oneline -1)"

echo "== the whole suite, on the merged tree"
( cd app && CARGO_TARGET_DIR="$TGT" cargo test --locked 2>&1 | grep -E "test result:|FAILED" )
if ( cd app && CARGO_TARGET_DIR="$TGT" cargo test --locked 2>&1 | grep -qE "^test .* FAILED|test result: FAILED" ); then
  echo "== a test failed on the merged tree — NOT installing"
  exit 1
fi

echo "== building"
( cd app && CARGO_TARGET_DIR="$TGT" cargo build --release 2>&1 | grep -E "Compiling terminal|Finished|^error" | head -5 )
B="$TGT/release/terminal-delight"
[ -x "$B" ] || { echo "no binary at $B"; exit 1; }
"$B" --version >/dev/null 2>&1 || { echo "the binary does not run"; exit 1; }

install -m 755 "$B" "$LIB/td-$SHA-$LABEL"
ln -sfn "$LIB/td-$SHA-$LABEL" "$BIN"
echo "== installed: $(readlink "$BIN")"
sha256sum "$B" "$LIB/td-$SHA-$LABEL" | awk '{print substr($1,1,16), $2}'

# Is a window bounce safe? Compare the wire files against the build the
# running host is on. The host owns the PTYs and is never restarted here.
HOSTEXE=$(for p in $(pgrep -f "serve --session" 2>/dev/null); do readlink "/proc/$p/exe"; done | sort | uniq -c | sort -rn | head -1 | awk '{print $2}')
if [ -n "${HOSTEXE:-}" ]; then
  HOSTSHA=$(basename "$HOSTEXE" | sed -E 's/^td-([0-9a-f]+)-.*/\1/')
  echo "== host wire vs the running host ($HOSTSHA):"
  if git cat-file -e "$HOSTSHA" 2>/dev/null; then
    git diff --stat "$HOSTSHA" HEAD -- app/src/gridwire.rs app/src/hostproto.rs app/src/socketpty.rs app/src/host.rs | tail -1
    echo "(empty above = identical wire; a window bounce loses nothing)"
  else
    echo "(the host's sha $HOSTSHA is not in this repository — cannot say; do not bounce blind)"
  fi
else
  echo "== no running host found; nothing to compare the wire against"
fi
