#!/usr/bin/env bash
# Isolated paired windows; never changes the user's session or visible workspace.
set -euo pipefail
ROOT=$(git rev-parse --show-toplevel)
SOURCE="$ROOT/reports/2026-10-17-td-styling-evidence"
OUT=${TD_REDRAW_OUT:?output directory}
mkdir -p "$OUT"
cp "$SOURCE/layout.py" "$SOURCE/output.py" "$SOURCE/long.md" "$OUT/"
cp "$ROOT/reports/2026-10-19-redraw-efficiency-evidence/sample.py" "$OUT/sample-pair.py"
test ! -e "$OUT/samples.jsonl"
if [ "${TD_REDRAW_HZ:-20}" = 100 ]; then
  sed -i 's/time.sleep(.05)/time.sleep(.01)/' "$OUT/output.py"
fi
if [ "${TD_REDRAW_ALL_BUSY:-0}" = 1 ]; then
  sed -i "s/'quiet'/'busy'/g" "$OUT/layout.py"
fi
BASE=${1:?baseline binary}
CAND=${2:?candidate binary}
export XDG_RUNTIME_DIR=/run/user/1000
export WAYLAND_DISPLAY=wayland-1
export HYPRLAND_INSTANCE_SIGNATURE=efb50993780079460b0cbed1363e2166a2de1d9f_1791921895_961721468
. "$ROOT/reports/2026-10-19-redraw-efficiency-evidence/headless-window.sh"
trap 'hidden_cleanup' EXIT
RUN=tdstyle-$(date +%s)-$$
WINDOWS=(); HOSTS=()
for label in ${TD_REDRAW_ORDER:-baseline candidate}; do
  if [ "$label" = baseline ]; then TD=$BASE; else TD=$CAND; fi
  session=$RUN-$label
  python3 "$OUT/layout.py" "$session" "$HIDDEN_SESSION_DIR/$session.toml" "$OUT/output.py"
  printf 'export TD_SESSION=%s\ncd /tmp\nexec %s > %s/%s-window.log 2>&1\n' "$session" "$TD" "$OUT" "$label" > /tmp/$session-launch.sh
  hidden_launch "$session" "special:$RUN" "$OUT/$label-window.log" "bash /tmp/$session-launch.sh"
  if [ "$label" = baseline ]; then SLOT=0; else SLOT=1; fi
  WINDOWS[SLOT]="$WIN"
  address=$(hyprctl clients -j | jq -r --argjson pid "$WIN" '.[]|select(.pid==$pid)|.address')
  hyprctl dispatch "hl.dsp.window.float({ action = \"on\", window = \"address:$address\" })"
  hyprctl dispatch "hl.dsp.window.resize({ x = 1000, y = 700, window = \"address:$address\" })"
  host=$(pgrep -f "serve --session $session( |\$)" | head -1)
  [ -n "$host" ] || { echo 'missing host'; exit 1; }
  HOSTS[SLOT]="$host"
  ctl mcp on
  ctl mcp expose all
  ctl mcp rpc '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_panes","arguments":{}}}' > "$OUT/$label-panes.json"
  cp "$HIDDEN_SESSION_DIR/$session.toml" "$OUT/$label-layout.toml"
  sha256sum "$TD" > "$OUT/$label-binary.txt"
  rm /tmp/$session-launch.sh
  sleep 3
  ctl mcp rpc '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"grep","arguments":{"query":"workload-ready","scrollback":1000}}}' > "$OUT/$label-readiness.json"
done
python3 - "$OUT/baseline-panes.json" "$OUT/candidate-panes.json" <<'PY'
import sys,json
for p in sys.argv[1:]:
 s=open(p).read();s=s[s.index('{'):];a=json.loads(s)['result']['structuredContent']['panes']
 assert len(a)==4,(p,len(a))
 assert sum(x.get('title','').startswith('busy') for x in a)==int(__import__('os').environ.get('TD_REDRAW_BUSY_COUNT','1')),(p,a)
 text=open(p.replace('-panes.json','-readiness.json')).read()
 ready=json.loads(text[text.index('{'):])['result']['structuredContent']['panes']
 assert len(ready)==4,(p,'output readiness missing',ready)
 print(p,'validated four panes, configured busy count, all output-ready')
PY
# The host owns the streams and is measured separately. The two windows run
# concurrently, on separate hidden workspaces of equal size. Alternate face
# order by round to reduce systematic drift; no profiler runs during samples.
for round in $(seq 0 $((${TD_STYLE_ROUNDS:-5} - 1))); do
  faces=${TD_REDRAW_FACES:-terminal}
  for face in $faces; do
    for idx in 0 1; do
      WIN=${WINDOWS[$idx]};TD=$BASE
      # A clean terminal has no square; that refusal is expected here.
      reply=$(ctl doc close || true)
      case "$reply" in ok*|*'no pane has a floating document open'*) ;; *) echo "close setup failed: $reply"; exit 1;; esac
      if [ "$face" = bench ]; then ctl bench on; else ctl bench off; fi
      if [ "$face" = document ]; then
        reply=$(ctl doc here "$OUT/long.md")
        case "$reply" in ok*) ;; *) echo "document setup failed: $reply"; exit 1;; esac
      fi
    done
    sleep 3
    python3 "$OUT/sample-pair.py" "${WINDOWS[0]}" "${HOSTS[0]}" "${WINDOWS[1]}" "${HOSTS[1]}" "$face-$round" "$OUT/samples.jsonl"
  done
done
hyprctl clients -j | jq --argjson a "${WINDOWS[0]}" --argjson b "${WINDOWS[1]}" '.[] | select(.pid == $a or .pid == $b) | {pid,at,size,workspace}' > "$OUT/final-window-geometry.json"
printf 'baseline and candidate pairs complete; cleaning isolated sessions\n'
