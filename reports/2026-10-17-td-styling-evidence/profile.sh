#!/usr/bin/env bash
set -euo pipefail
ROOT=$(git rev-parse --show-toplevel)
OUT="$ROOT/reports/2026-10-17-td-styling-evidence"
TD=${1:?normal release candidate}
export XDG_RUNTIME_DIR=/run/user/1000
export WAYLAND_DISPLAY=wayland-1
export HYPRLAND_INSTANCE_SIGNATURE=efb50993780079460b0cbed1363e2166a2de1d9f_1791921895_961721468
. "$ROOT/scripts/lib/hidden-window.sh"
trap 'hidden_cleanup' EXIT
RUN=tdstyle-prof-$(date +%s)-$$
python3 "$OUT/layout.py" "$RUN" "$HIDDEN_SESSION_DIR/$RUN.toml" "$OUT/output.py"
printf 'export TD_SESSION=%s\ncd /tmp\nexec %s > %s/profile-window.log 2>&1\n' "$RUN" "$TD" "$OUT" > /tmp/$RUN-launch.sh
hidden_launch "$RUN" "special:$RUN" "$OUT/profile-window.log" "bash /tmp/$RUN-launch.sh"
rm /tmp/$RUN-launch.sh
sleep 5
perf record -q --no-inherit -e cpu-clock:u -F 99 --call-graph dwarf,32760 -t "$WIN" -o /tmp/td-styling-candidate.perf -- sleep 12
perf report -i /tmp/td-styling-candidate.perf --stdio --no-children --sort dso,symbol --call-graph none --percent-limit 1 --field-separator '|' > "$OUT/candidate-profile.txt"
