#!/usr/bin/env bash
set -euo pipefail
TD=$1
OUT=$2
PALETTE=$3
SURFACE=$4
SESSION="chalkproof-${PALETTE}-${SURFACE}"
ROOT=$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)
mkdir -p "$OUT/config/terminal-delight/sessions"
python3 - "$OUT/config/terminal-delight/sessions/$SESSION.toml" "$PALETTE" "$(dirname "${BASH_SOURCE[0]}")/emitter.py" <<'PY'
import sys,tomllib,pathlib
out=pathlib.Path(sys.argv[1]);mode=sys.argv[2]
real=tomllib.loads(pathlib.Path('/home/parker/.config/terminal-delight/sessions/1.toml').read_text())
grade=real['theme']['grade'].copy() if mode in ['praus','warped'] else dict(crt=False,scale=1.15,brightness=.5,contrast=.5,colour=.5,text=.5,background=.5,gamma=.5,warp=0.,phosphor=0.)
grade['crt']= mode=='warped';grade['warp']=1.5 if mode=='warped' else 0.
head='active = 2\nlang = "en"\nleft_bar = true\nleft_bar_w = 220.0\nanchor_top = false\nwarp = '+str(grade['warp'])+'\nwin = [0.0,0.0,1576.0,950.0]\n\n[theme]\nid = "deco"\ncolor = "default"\n'
head+=('follow_omarchy = true\n' if mode in ['praus','warped'] else 'palette = "flexoki-light"\nfollow_omarchy = false\n')
head+='\n[theme.grade]\n'
for k,v in grade.items():head+=k+' = '+(str(v).lower() if isinstance(v,bool) else str(v))+'\n'
for i,name in enumerate(['GLOBAL','JEV','TERMINAL DELIGHT']):head+=f'\n[[projects]]\nid = {i+1}\nname = "{name}"\ncollapsed = false\ncolor = "#86917B"\n'
for i,(name,project) in enumerate([('home',1),('tool call router',2),('Flower chalk',3),('surface coverage',3),('gauges and spine',3),('repeat this review',3)]):
 head+=f'\n[[tabs]]\nproject = {project}\nname = "{name}"\n'
 if i==2:
  head+='[tabs.node.Split]\ndir = "Row"\nratio = 0.55\n'
  for side in ['a','b']:head+=f'[tabs.node.Split.{side}.Leaf]\ncwd = "/tmp"\nresume = "python3 {sys.argv[3]}"\n'
 else:head+=f'[tabs.node.Leaf]\ncwd = "/tmp"\nresume = "python3 {sys.argv[3]}"\n'
out.write_text(head)
PY
cp "$OUT/config/terminal-delight/sessions/$SESSION.toml" "$OUT/seed.toml"
cat > "$OUT/launch.sh" <<EOF
export TD_SESSION=$SESSION XDG_CONFIG_HOME=$OUT/config XDG_CACHE_HOME=$OUT/cache XDG_STATE_HOME=$OUT/state
export SHELL=/bin/bash FLOWER_BUSY=$OUT/busy
unset TD_DEMO_STATE TD_THEME TD_PALETTE TD_SKIN TD_WALL_DEMO TD_BENCH_DEMO TD_TRAY_DEMO TD_OSD_DEMO TD_SPINE_DEMO
EOF
case "$SURFACE" in
 tray) echo 'export TD_TRAY_DEMO=1' >> "$OUT/launch.sh";;
 gauges) echo 'export TD_OSD_DEMO=1' >> "$OUT/launch.sh";;
 rail) echo 'export TD_SPINE_DEMO=1' >> "$OUT/launch.sh";;
esac
printf 'exec %s > %s/window.log 2>&1\n' "$TD" "$OUT" >> "$OUT/launch.sh"
. "$ROOT/scripts/lib/hidden-window.sh"
HIDDEN_SESSION_DIR="$OUT/config/terminal-delight/sessions"
trap hidden_cleanup EXIT
hidden_launch "$SESSION" "special:$SESSION" "$OUT/window.log" "sh $OUT/launch.sh"
STABLE=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[]|select(.pid==$p)|.stableId')
echo "window=$WIN stable=$STABLE session=$SESSION"
# Longer client budget for the already tracked slow-snapshot socket bug. Never sweep a socket.
python3 - "$WIN" "$OUT" "$SURFACE" <<'PY'
import socket,json,pathlib,sys,time
pid=int(sys.argv[1]);out=pathlib.Path(sys.argv[2])
def ctl(line):
 with socket.socket(socket.AF_UNIX) as s:
  s.settimeout(12);s.connect(f'/run/user/1000/terminal-delight/ctl-{pid}.sock');s.sendall((line+'\n').encode());return s.makefile().readline().strip()
for x in ['mcp on','mcp writes on','mcp expose all','skin chalk']:print(x,ctl(x))
if sys.argv[3]=='paint':print(ctl('paint on'))
def rpc(name,args):
 return json.loads(ctl('mcp rpc '+json.dumps({'jsonrpc':'2.0','id':1,'method':'tools/call','params':{'name':name,'arguments':args}})))
time.sleep(2)
r=rpc('list_panes',{});(out/'panes.json').write_text(json.dumps(r,indent=2))
panes=r['result']['structuredContent']['panes'];pane_pid=next(p['pid'] for p in panes if p['tab']==2)
print(rpc('leave_note',{'pid':pane_pid,'title':'PAPER & PETALS','text':'Second pass: one surface, every theme.','pin':False}))
(out/'skin.txt').write_text(ctl('skin status')+'\n')
PY
sleep 2
timeout 15 grim -T "$STABLE" "$OUT/native.png"
echo "$WIN" > "$OUT/window.pid"
if [ "$SURFACE" = workspace ]; then
 ctl skin default > "$OUT/default-switch.txt"
 sleep 1
 timeout 15 grim -T "$STABLE" "$OUT/default.png"
 ctl skin chalk > "$OUT/chalk-switch.txt"
 sleep 1
 timeout 15 grim -T "$STABLE" "$OUT/restored.png"
 hidden_close || true
 hidden_launch "$SESSION" "special:$SESSION" "$OUT/restart.log" "sh $OUT/launch.sh"
 STABLE=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[]|select(.pid==$p)|.stableId')
 sleep 2
 ctl skin status > "$OUT/restart-skin.txt"
 timeout 15 grim -T "$STABLE" "$OUT/restart.png"
 echo "$WIN" > "$OUT/window.pid"
fi
cp "$OUT/config/terminal-delight/sessions/$SESSION.toml" "$OUT/restored-state.toml"
# Hold only this test window for follow-on probes; the finish file releases it.
if [ "${CHALK_CAPTURE_ONLY:-0}" != 1 ]; then
 for _ in $(seq 1 900); do [ -f "$OUT/finish" ] && break; sleep 1; done
fi
