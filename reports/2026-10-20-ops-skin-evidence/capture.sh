#!/usr/bin/env bash
# Native capture of the OPS skin, one scene per run, in a hidden window that
# runs its own session, config, cache and state. Adapted from the Chalk rig
# (reports/2026-10-20-chalk-marker-evidence/capture.sh).
#
#   capture.sh <td-binary> <out-dir> <scene> <surface>
#
# Scenes:
#   tactical   the Tactical design with no palette: its own cyan colours. OPS is
#              NOT picked — it must arrive because the design asks for it.
#   gruvbox    Tactical over the stock Omarchy gruvbox palette.
#   tokyo      Tactical over the stock Omarchy tokyo-night palette.
#   current    Parker's saved theme and grade, with OPS picked explicitly.
#   warped     Tactical with the CRT on and the barrel warp at 1.5.
#   intrusive  the Intrusive design, CRT on, OPS picked explicitly.
#   chalkwet   the Intrusive design under the Chalk skin — the mix it promises.
#   before     the Tactical design with the Default skin picked: the baseline.
# Surfaces: workspace, tray, usage, gauges, rail, paint.
set -euo pipefail
TD=$1
OUT=$2
SCENE=$3
SURFACE=$4
SESSION="opsskin-${SCENE}-${SURFACE}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# A frozen copy of this rig runs outside the repo; OPS_ROOT names the checkout.
ROOT=${OPS_ROOT:-$(git -C "$HERE" rev-parse --show-toplevel)}
mkdir -p "$OUT/config/terminal-delight/sessions"
python3 - "$OUT/config/terminal-delight/sessions/$SESSION.toml" "$SCENE" "$HERE/emitter.py" <<'PY'
import sys, tomllib, pathlib, os, json
out = pathlib.Path(sys.argv[1]); scene = sys.argv[2]
real = tomllib.loads(pathlib.Path(os.environ.get('OPS_SOURCE_SESSION', '/home/parker/.config/terminal-delight/sessions/1.toml')).read_text())
flat = dict(crt=False, scale=1.15, brightness=.5, contrast=.5, colour=.5, text=.5, background=.5, gamma=.5, warp=0., phosphor=1.)
tactical = dict(id='tactical-overdrive', color='default', follow_omarchy=False)
if scene == 'current':
    theme = {k: v for k, v in real['theme'].items() if not isinstance(v, dict)}
    grade = real['theme']['grade'].copy()
elif scene in ('intrusive', 'chalkwet'):
    theme = dict(id='intrusive', color='default', follow_omarchy=False)
    grade = dict(flat, crt=True, warp=0.8, phosphor=1.4)
else:
    theme = dict(tactical)
    if scene == 'gruvbox': theme['palette'] = 'gruvbox'
    if scene == 'tokyo': theme['palette'] = 'tokyo-night'
    grade = dict(flat)
    if scene == 'warped': grade.update(crt=True, warp=1.5)
val = json.dumps
head = 'active = 2\nlang = "en"\nleft_bar = true\nleft_bar_w = 220.0\nanchor_top = false\nwarp = ' + str(grade.get('warp', 0.)) + '\nwin = [0.0,0.0,1576.0,950.0]\n\n[theme]\n'
for k, v in theme.items(): head += k + ' = ' + val(v) + '\n'
head += '\n[theme.grade]\n'
for k, v in grade.items(): head += k + ' = ' + val(v) + '\n'
for i, name in enumerate(['GLOBAL', 'JEV', 'TERMINAL DELIGHT']):
    head += f'\n[[projects]]\nid = {i+1}\nname = "{name}"\ncollapsed = false\ncolor = "#86917B"\n'
for i, (name, project) in enumerate([('home', 1), ('tool call router', 2), ('ops skin', 3), ('surface coverage', 3), ('gauges and spine', 3), ('repeat this review', 3)]):
    head += f'\n[[tabs]]\nproject = {project}\nname = "{name}"\n'
    if i == 2:
        head += '[tabs.node.Split]\ndir = "Row"\nratio = 0.55\n'
        for side in ['a', 'b']: head += f'[tabs.node.Split.{side}.Leaf]\ncwd = "/tmp"\nresume = "python3 {sys.argv[3]}"\n'
    else:
        head += f'[tabs.node.Leaf]\ncwd = "/tmp"\nresume = "python3 {sys.argv[3]}"\n'
out.write_text(head)
PY
cp "$OUT/config/terminal-delight/sessions/$SESSION.toml" "$OUT/seed.toml"
cat > "$OUT/launch.sh" <<EOF
export TD_SESSION=$SESSION XDG_CONFIG_HOME=$OUT/config XDG_CACHE_HOME=$OUT/cache XDG_STATE_HOME=$OUT/state
export SHELL=/bin/bash FLOWER_BUSY=$OUT/busy TMPDIR=$OUT/tmp
unset TD_DEMO_STATE TD_THEME TD_PALETTE TD_SKIN TD_WALL_DEMO TD_BENCH_DEMO TD_TRAY_DEMO TD_OSD_DEMO TD_SPINE_DEMO TD_USAGE_DEMO TD_USAGE_LIVE TD_SAVINGS_DEMO
EOF
mkdir -p "$OUT/tmp"
case "$SURFACE" in
 tray) echo 'export TD_TRAY_DEMO=1' >> "$OUT/launch.sh";;
 usage) echo 'export TD_USAGE_DEMO=1' >> "$OUT/launch.sh";;
 gauges) echo 'export TD_OSD_DEMO=1' >> "$OUT/launch.sh";;
 rail) echo 'export TD_SPINE_DEMO=1' >> "$OUT/launch.sh";;
esac
printf 'exec %s > %s/window.log 2>&1\n' "$TD" "$OUT" >> "$OUT/launch.sh"
. "$ROOT/scripts/lib/hidden-window.sh"
hidden_launch() {
  local session=$1 ws=$2 log=$3 command=$4 said landed
  case " $HIDDEN_SESSIONS " in *" $session "*) ;; *) HIDDEN_SESSIONS="$HIDDEN_SESSIONS $session" ;; esac
  said=$(hyprctl dispatch "hl.dsp.exec_cmd(\"$command\", { monitor = \"eDP-1\", workspace = \"$ws silent\", render_unfocused = true, no_initial_focus = true })" 2>&1)
  case "$said" in ok|"") ;; *) echo "hyprctl refused the launch: $said"; exit 1 ;; esac
  WIN=""
  for _ in $(seq 1 60); do
    WIN=$(hyprctl clients -j 2>/dev/null | jq -r --arg t "terminal-delight — $session" \
      '.[] | select(.title==$t) | .pid' | head -1)
    [ -n "$WIN" ] && break
    sleep 0.5
  done
  [ -n "$WIN" ] || { echo "no window appeared — see $log"; exit 1; }
  HIDDEN_WINDOWS="$HIDDEN_WINDOWS $WIN"
  landed=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .workspace.name' | head -1)
  if [ "$landed" != "$ws" ]; then
    echo "the window landed on '$landed', not $ws — killed, nothing ran"
    kill "$WIN" 2>/dev/null
    exit 4
  fi
  for _ in $(seq 1 40); do [ "$(ctl ping)" = "pong" ] && break; sleep 0.5; done
}
geometry() {
 python3 - "$WIN" <<'PYGEOM'
import subprocess, json, sys
p = int(sys.argv[1])
def clients(): return json.loads(subprocess.check_output(['hyprctl', 'clients', '-j'], text=True))
c = next(c for c in clients() if c['pid'] == p)
mon = next(m for m in json.loads(subprocess.check_output(['hyprctl', 'monitors', '-j'], text=True)) if m['name'] == 'eDP-1')
addr = c['address']
def run(expr):
    r = subprocess.run(['hyprctl', 'dispatch', expr], capture_output=True, text=True); assert r.returncode == 0, r.stdout
if not c['floating']: run(f'hl.dsp.window.float({{ window = "address:{addr}" }})')
run(f'hl.dsp.window.resize({{ x = 1576, y = 950, window = "address:{addr}" }})')
run(f'hl.dsp.window.move({{ x = {mon["x"]+12}, y = {mon["y"]+38}, window = "address:{addr}" }})')
c = next(c for c in clients() if c['pid'] == p)
assert c['workspace']['name'].startswith('special:opsskin-'), c['workspace']
print('geometry', c['size'], 'workspace', c['workspace']['name'])
PYGEOM
}
HIDDEN_SESSION_DIR="$OUT/config/terminal-delight/sessions"
trap hidden_cleanup EXIT
hidden_launch "$SESSION" "special:$SESSION" "$OUT/window.log" "sh $OUT/launch.sh"
geometry
STABLE=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[]|select(.pid==$p)|.stableId')
echo "window=$WIN stable=$STABLE session=$SESSION"
python3 - "$WIN" "$OUT" "$SCENE" "$SURFACE" <<'PY'
import socket, json, pathlib, sys, time
pid = int(sys.argv[1]); out = pathlib.Path(sys.argv[2]); scene = sys.argv[3]; surface = sys.argv[4]
def ctl(line):
    with socket.socket(socket.AF_UNIX) as s:
        s.settimeout(12); s.connect(f'/run/user/1000/terminal-delight/ctl-{pid}.sock'); s.sendall((line + '\n').encode()); return s.makefile().readline().strip()
for x in ['mcp on', 'mcp writes on', 'mcp expose all']: print(x, ctl(x))
# The tactical scenes pick no skin: OPS must come from the design's request.
if scene in ('current', 'intrusive'): print('skin ops', ctl('skin ops'))
if scene == 'chalkwet': print('skin chalk', ctl('skin chalk'))
# BEFORE: the same Tactical window in the skin every design wore until now.
if scene == 'before': print('skin default', ctl('skin default'))
if surface == 'paint': print(ctl('paint on'))
def rpc(name, args):
    return json.loads(ctl('mcp rpc ' + json.dumps({'jsonrpc': '2.0', 'id': 1, 'method': 'tools/call', 'params': {'name': name, 'arguments': args}})))
time.sleep(2)
r = rpc('list_panes', {}); (out / 'panes.json').write_text(json.dumps(r, indent=2))
panes = r['result']['structuredContent']['panes']; pane_pid = next(p['pid'] for p in panes if p['tab'] == 2)
print(rpc('leave_note', {'pid': pane_pid, 'title': 'ORDERS', 'text': 'Give Tactical its own skin. Nine marks.', 'pin': False}))
(out / 'skin.txt').write_text(ctl('skin status') + '\n')
PY
sleep 3
timeout 15 grim -T "$STABLE" "$OUT/native.png"
echo "$WIN" > "$OUT/window.pid"
# The usage demo opens its panel over the left bar's allowance slot. Close the
# panel and the wall under it with Escape, then photograph the slot itself.
if [ "$SURFACE" = usage ]; then
 for _ in 1 2 3; do
  hyprctl dispatch "hl.dsp.send_shortcut({ mods = '', key = 'Escape', window = 'pid:$WIN' })" >/dev/null
  sleep 0.8
 done
 sleep 1
 timeout 15 grim -T "$STABLE" "$OUT/slot.png"
fi
cp "$OUT/config/terminal-delight/sessions/$SESSION.toml" "$OUT/restored-state.toml" 2>/dev/null || true
if [ "${OPS_HOLD:-0}" = 1 ]; then
 for _ in $(seq 1 900); do [ -f "$OUT/finish" ] && break; sleep 1; done
fi
