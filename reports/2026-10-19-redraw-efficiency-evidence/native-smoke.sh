#!/usr/bin/env bash
# Isolated real-window check against the same host build as Parker's session.
set -euo pipefail
ROOT=$(git rev-parse --show-toplevel)
TD=${1:?installed binary}
OLD=${2:?running host binary}
OUT=${3:?output directory}
mkdir -p "$OUT"
. "$ROOT/reports/2026-10-19-redraw-efficiency-evidence/headless-window.sh"
SESSION="tdsleep-smoke-$$"
WS="special:$SESSION"
trap hidden_cleanup EXIT
cat > "$OUT/emit.py" <<'PY'
import time
n = 0
while True:
    print("compat-ready fresh hidden output " + str(n), flush=True)
    n += 1
    time.sleep(.05)
PY
cat > "$HIDDEN_SESSION_DIR/$SESSION.toml" <<TOML
active = 1
[mcp]
enabled = true
expose = "all"
writable = true
[[tabs]]
[tabs.node.Leaf]
cwd = "/tmp"
pane_id = 1
name = "isolated compatibility check"
resume = "python3 $OUT/emit.py"
[tabs.node.Leaf.appearance]
pins = ["crt"]
[tabs.node.Leaf.appearance.grade]
crt = false
[[tabs]]
[tabs.node.Leaf]
cwd = "/tmp"
pane_id = 2
name = "quiet pane during hidden output"
resume = "exec sleep 600"
[tabs.node.Leaf.appearance]
pins = ["crt"]
[tabs.node.Leaf.appearance.grade]
crt = false
TOML
HIDDEN_SESSIONS="$SESSION"
"$OLD" serve --session "$SESSION" > "$OUT/host.log" 2>&1 &
HOST=$!
for _ in $(seq 1 50); do
  [ -S "$XDG_RUNTIME_DIR/terminal-delight/session-$SESSION.sock" ] && break
  sleep .1
done
readlink "/proc/$HOST/exe" > "$OUT/host-binary.txt"
sha256sum "$TD" "$OLD" > "$OUT/binaries.txt"
launch() {
  local script="$OUT/launch.sh"
  printf 'export TD_SESSION=%s\ncd /tmp\nexec %s > %s/window-%s.log 2>&1\n' "$SESSION" "$TD" "$OUT" "$1" > "$script"
  hidden_launch "$SESSION" "$WS" "$OUT/window-$1.log" "bash $script"
}
panes() {
  ctl mcp rpc '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_panes","arguments":{}}}' | sed 's/^[^{]*//' > "$1"
  [ "$(jq '.result.structuredContent.panes | length' "$1")" = 2 ]
  python3 - "$1" <<'PY'
import json,sys
from pathlib import Path
p = Path(sys.argv[1])
rows = json.loads(p.read_text())["result"]["structuredContent"]["panes"]
identity = {}
for row in rows:
    pid = row["pid"]
    stat = Path("/proc", str(pid), "stat").read_text().rsplit(")", 1)[1].split()
    identity[str(row["pane_id"])] = {"pid": pid, "start_ticks": int(stat[19])}
Path(str(p) + ".starts.json").write_text(json.dumps(identity, sort_keys=True))
PY
}
find_text() {
  ctl mcp rpc "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"grep\",\"arguments\":{\"query\":\"$1\",\"scrollback\":1000}}}" | sed 's/^[^{]*//' > "$OUT/grep-$1.json"
  [ "$(jq '.result.structuredContent.panes | length' "$OUT/grep-$1.json")" = 1 ]
}
launch before
sleep 2
panes "$OUT/before.json"
find_text compat-ready
# Target only this isolated hidden GUI. The real workspace key path returns
# to the output pane; a policy save lets us verify the selected tab on disk.
hyprctl dispatch "hl.dsp.send_shortcut({ mods = 'CTRL', key = 'Page_Up', window = 'pid:$WIN' })"
sleep .2
ctl mcp on
python3 - "$HIDDEN_SESSION_DIR/$SESSION.toml" <<'PY'
import sys,tomllib
state = tomllib.load(open(sys.argv[1], "rb"))
assert state["active"] == 0, state["active"]
PY
stable=$(hyprctl clients -j | jq -r --argjson pid "$WIN" '.[]|select(.pid==$pid)|.stableId')
timeout 10 grim -T "$stable" "$OUT/activated-pane.png"
ctl bench on
reply=$(ctl bench say "compat-input-survives" || true)
case "$reply" in "err no agent pane"*) ;; *) echo "shell input protection failed: $reply"; exit 1;; esac
ctl bench off
find_text compat-ready
printf '# Installed build\n\nA real Markdown document.\n\n' > "$OUT/reading.md"
for _ in $(seq 1 90); do printf 'Paragraph to scroll through.\n\n' >> "$OUT/reading.md"; done
ctl doc here "$OUT/reading.md"
ctl doc scroll 300
ctl doc close
# The checkpoint persists the layout; never close its terminal or session host.
sleep 2
CLOSED=$WIN
# hidden_close's bookkeeping grep returns 1 when its list becomes empty.
# Check the actual process outcome rather than treating that grep as a failure.
hidden_close || true
if kill -0 "$CLOSED" 2>/dev/null; then echo 'old GUI remained alive'; exit 1; fi
kill -0 "$HOST"
launch after
sleep 2
panes "$OUT/after.json"
find_text compat-ready
jq -S '.result.structuredContent.panes | map({pane_id,pid})' "$OUT/before.json" > "$OUT/before-identity.json"
jq -S '.result.structuredContent.panes | map({pane_id,pid})' "$OUT/after.json" > "$OUT/after-identity.json"
cmp "$OUT/before-identity.json" "$OUT/after-identity.json"
cmp "$OUT/before.json.starts.json" "$OUT/after.json.starts.json"
kill -0 "$HOST"
printf 'PASS: new GUI with old host, hidden output, real tab activation screenshot, shell input protection, face switching, document scroll, GUI restart, both pane PIDs preserved.\n'
