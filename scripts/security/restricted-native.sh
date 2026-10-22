#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
TD="${TD_AUTHORITY_BIN:-$ROOT/app/target/debug/terminal-delight}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
: "${HYPRLAND_INSTANCE_SIGNATURE:?run inside your Hyprland session}"
. "$ROOT/scripts/lib/hidden-window.sh"
OUT="${TD_AUTHORITY_OUT:-$(mktemp -d "${TMPDIR:-$HOME/.cache}/td-authority-native.XXXXXX")}"
mkdir -p "$OUT"
GRANT_ROOT=$(mktemp -d "$HOME/TDRestrictedNative.XXXXXX")
WORK="$GRANT_ROOT/work"
mkdir -p "$WORK"
trap 'set +e; hidden_cleanup; rm -rf -- "$GRANT_ROOT"' EXIT
SESSION="authnative$$"
WS="special:authnative$$"
if [ -n "${TD_AUTHORITY_OLD_HOST:-}" ]; then
  "$TD_AUTHORITY_OLD_HOST" serve --session "$SESSION" > "$OUT/old-host.log" 2>&1 &
  OLD_HOST_PID=$!
  HIDDEN_SESSIONS="$HIDDEN_SESSIONS $SESSION"
  for _ in $(seq 1 80); do [ -S "${XDG_RUNTIME_DIR}/terminal-delight/session-$SESSION.sock" ] && break; sleep .05; done
  [ -S "${XDG_RUNTIME_DIR}/terminal-delight/session-$SESSION.sock" ]
  readlink "/proc/$OLD_HOST_PID/exe" > "$OUT/old-host-exe.txt"
fi
cat > "$OUT/launch.sh" <<EOF
#!/bin/sh
export TD_SESSION=$SESSION TD_MCP_SCRIPTED=1
unset TD_SEED_CWD TD_SEED_RESUME TD_SCRATCH TD_NO_SESSIOND TD_DEMO_STATE
exec $TD > $OUT/window.log 2>&1
EOF
hidden_launch "$SESSION" "$WS" "$OUT/window.log" "sh $OUT/launch.sh"
ctl whoami > "$OUT/window-identity.txt"
ctl mcp on
ctl mcp writes on
ctl mcp expose all
cat > "$WORK/job.sh" <<'EOF'
#!/bin/sh
set -eu
terminal-delight restricted call '{"op":"status"}' > /work/status.json
terminal-delight restricted call '{"op":"note","text":"Allowed offline build completed"}' > /work/note.json
terminal-delight restricted call '{"op":"note","text":"Forged target","pid":1}' > /work/forged.json || true
terminal-delight restricted call '{"op":"send","recipient":"standard","text":"execute"}' > /work/delegate.json || true
terminal-delight restricted call '{"op":"request_access","reason":"Need external packages"}' > /work/request.json
: > /work/ready
while [ ! -e /work/policy-go ]; do sleep 0.05; done
sleep 1.05
terminal-delight restricted call '{"op":"note","text":"Policy must still apply"}' > /work/policy-refused.json || true
: > /work/policy-done
while [ ! -e /work/go ]; do sleep 0.05; done
terminal-delight restricted call '{"op":"note","text":"After revocation"}' > /work/revoked.json || true
: > /work/done
EOF
cat > "$OUT/run.sh" <<EOF
#!/bin/sh
echo \$\$ > $OUT/runner.pid
exec $TD restricted run --workdir $WORK --td-note -- sh /work/job.sh > $OUT/run.stdout 2> $OUT/run.stderr
EOF
ctl adopt --cwd "$WORK" --run "exec sh $OUT/run.sh" > "$OUT/adopt.txt"
for _ in $(seq 1 160); do [ -e "$WORK/ready" ] && break; sleep .1; done
[ -e "$WORK/ready" ] || { cat "$OUT/run.stderr"; exit 1; }
ID=$(sed -n 's/^Starting Restricted offline run \([a-f0-9]*\):.*/\1/p' "$OUT/run.stderr")
[ ${#ID} -eq 32 ]
"$TD" restricted status "$ID" > "$OUT/operator-status.json"
RPC='{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_panes","arguments":{}}}'
ctl mcp rpc "$RPC" > "$OUT/panes-before-revoke.json"
ctl mcp writes off
: > "$WORK/policy-go"
for _ in $(seq 1 100); do [ -e "$WORK/policy-done" ] && break; sleep .1; done
[ -e "$WORK/policy-done" ]
ctl mcp writes on
"$TD" restricted revoke "$ID" > "$OUT/operator-revoke.json"
: > "$WORK/go"
for _ in $(seq 1 100); do [ -e "$WORK/done" ] && break; sleep .1; done
[ -e "$WORK/done" ]
cp -a "$WORK" "$OUT/work"
python3 - "$OUT" "$WORK" "$ROOT" "$TD" <<'PY'
import json,pathlib,sys
p=pathlib.Path(sys.argv[1]);j=lambda x:json.loads((p/x).read_text())
assert j('work/note.json')['ok']
assert 'MCP writes are disabled' in j('work/policy-refused.json')['error']
assert not j('work/forged.json')['ok']
assert not j('work/delegate.json')['ok']
assert not j('work/request.json')['granted']
assert j('operator-revoke.json')['note']=='revoked'
assert 'revoked' in j('work/revoked.json')['error']
snapshot=j('panes-before-revoke.json')['result']['structuredContent']['panes']
notes=[x for x in snapshot if x.get('note')]
assert len(notes)==1,notes
assert notes[0]['pid']==int((p/'runner.pid').read_text()),notes[0]
assert notes[0]['cwd']==sys.argv[2],notes[0]
note=notes[0]['note'];assert note['title']=='RESTRICTED' and note['text']=='Allowed offline build completed',note
sys.path.insert(0,str(pathlib.Path(sys.argv[3])/'scripts/security'))
from receipt import source,digest
(p/'native-proof.json').write_text(json.dumps({'source':source(sys.argv[3]),'binary_sha256':digest(sys.argv[4]),'own_note':True,'exact_supervisor_pid':int((p/'runner.pid').read_text()),'global_writes_still_apply':True,'forged_target_denied':True,'standard_delegation_denied':True,'proposal_grants_nothing':True,'revoked_note_denied':True,'other_panes_untouched':True,'window':(p/'window-identity.txt').read_text().strip()},indent=2))
print(json.dumps({k:v for k,v in json.loads((p/'native-proof.json').read_text()).items() if k!='source'},indent=2))
PY
ctl bench on > "$OUT/bench-on.txt"
ctl bench probe > "$OUT/allowed-probe.txt"
ctl tabs '[{"op":"name","tab":0,"name":"AUTHORITY TEST"}]' > "$OUT/save.txt"
printf '%s\n' "$SESSION" > "$OUT/session.txt"
printf '%s\n' "$WIN" > "$OUT/window-pid.txt"
printf 'Native evidence: %s\n' "$OUT"
