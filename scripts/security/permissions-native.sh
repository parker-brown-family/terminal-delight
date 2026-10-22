#!/usr/bin/env bash
# Actual ctl routing and caller identity in disposable hidden native windows.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
TD="${TD_AUTHORITY_BIN:?select the exact candidate binary}"
OUT="${TD_AUTHORITY_OUT:?select a new evidence directory}"
mkdir -p "$OUT"
: "${HYPRLAND_INSTANCE_SIGNATURE:?run inside your Hyprland session}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
. "$ROOT/scripts/lib/hidden-window.sh"
trap 'set +e; hidden_cleanup' EXIT
SESSION="permissions$$"
python3 "$ROOT/scripts/security/permissions-native.py" prepare "$ROOT" "$OUT" "$TD" "$SESSION"
hidden_launch "$SESSION" "special:$SESSION" "$OUT/window.log" "sh $OUT/launch.sh"
printf '%s\n' "$WIN" > "$OUT/window"
for _ in $(seq 1 160); do [ -e "$OUT/caller-receipt.json" ] && break; sleep .1; done
[ -e "$OUT/caller-receipt.json" ] || { cat "$OUT/caller.log"; exit 1; }
python3 "$ROOT/scripts/security/permissions-native.py" check "$ROOT" "$OUT" "$TD" "$SESSION"
hidden_close || true
SESSION="ordinary$$"
cat > "$OUT/ordinary.sh" <<EOF
#!/bin/sh
unset TD_MCP_SCRIPTED TD_WORKBENCH_DEMO TD_SEED_CWD TD_SEED_RESUME TD_SCRATCH TD_NO_SESSIOND TD_DEMO_STATE
export TD_SESSION=$SESSION
exec "$TD" > "$OUT/ordinary.log" 2>&1
EOF
hidden_launch "$SESSION" "special:$SESSION" "$OUT/ordinary.log" "sh $OUT/ordinary.sh"
python3 "$ROOT/scripts/security/permissions-native.py" ordinary "$ROOT" "$OUT" "$TD" "$SESSION" "$WIN"
printf 'Native permission evidence: %s\n' "$OUT"
