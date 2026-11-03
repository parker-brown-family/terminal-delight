#!/usr/bin/env bash
# Run the account-home uninstall e2e on this machine without touching it.
#
#   scripts/tests/uninstall-e2e-sandbox.sh <terminal-delight binary>
#
# Inside a bubblewrap sandbox a scratch folder is mounted over $HOME, so the
# removal sees a real account home (passwd agrees) that holds only the install
# the test makes. /run/user/<uid>, /tmp and the network are fresh, and the run
# has a PID namespace of its own: nothing inside can see or signal a process
# outside it, the live Terminal Delight and its session hosts included. The
# machine's own protection marker lives in the real home, which is not mounted,
# so the removal runs for real inside and the marker is never consulted.
set -euo pipefail
BIN=$(realpath "${1:?usage: uninstall-e2e-sandbox.sh <terminal-delight binary>}")
REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
command -v bwrap >/dev/null || { echo "needs bubblewrap (bwrap)" >&2; exit 2; }
scratch=$(mktemp -d "${TMPDIR:-/tmp}/td-e2e-sandbox.XXXXXX")
trap 'chmod -R u+rwX "$scratch" 2>/dev/null; rm -rf "$scratch"' EXIT
mkdir -p "$scratch/home" "$scratch/run"
install -m755 "$BIN" "$scratch/terminal-delight"
uid=$(id -u)
bwrap --die-with-parent --new-session \
  --unshare-pid --unshare-net --unshare-ipc \
  --ro-bind / / \
  --dev /dev --proc /proc --tmpfs /tmp \
  --bind "$scratch/home" "$HOME" \
  --bind "$scratch/run" "/run/user/$uid" \
  --tmpfs /mnt \
  --ro-bind "$REPO" /mnt/repo \
  --ro-bind "$scratch/terminal-delight" /mnt/terminal-delight \
  --chdir "$HOME" \
  env -i HOME="$HOME" USER="$(id -un)" LOGNAME="$(id -un)" PATH=/usr/bin:/bin \
    LANG=C.UTF-8 TMPDIR=/tmp XDG_RUNTIME_DIR="/run/user/$uid" \
    TD_E2E_DISPOSABLE_HOME=1 \
    bash /mnt/repo/scripts/tests/uninstall-e2e.sh --account-home /mnt/terminal-delight
