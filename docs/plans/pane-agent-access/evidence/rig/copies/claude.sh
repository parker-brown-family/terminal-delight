#!/bin/bash
# The agent side of the copy rig's stand-in, run by the compiled `claude` beside
# the transcript it holds. Posts a response card, prints a status line, and —
# for the asking pane — makes cheap asks and copy asks through MCP from inside
# its own seated pane, then reads each copy back with ask_answer.
sd=${XDG_STATE_HOME:-$HOME/.local/state}/terminal-delight/surfaces/$TD_SESSION/$TD_PANE_ID; mkdir -p "$sd"
python3 -c 'import json,os; b=os.environ.get("BRIEF"); print(json.dumps({"td":"0.4","kind":"response","title":"latest","model":{"layman":os.environ.get("LAYMAN","Working on it."),**({"brief":b} if b else {})}}))' > "$sd/rig-$RIG_SESSION.json"
clear
rows=$(tput lines 2>/dev/null || echo 24)
printf '\033[%d;1H' $((rows-4))
if [ "${STATE:-working}" = working ]; then
  printf '\n> %s\n\n\342\234\273 Thinking\342\200\246 (14s \302\267 \342\206\223 2.1k tokens \302\267 esc to interrupt)\n' "${TASK:-the task}"
else
  printf '\n\342\235\257 \n  ? for shortcuts\n'
fi
rpc() { "$TD" ctl --workspace special:warroomrig mcp from "$TD_SESSION" "$TD_PANE_ID" rpc "$1" 2>&1 | cut -f2-; }
call() { python3 -c 'import json,sys; print(json.dumps({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":sys.argv[1],"arguments":json.loads(sys.argv[2])}}))' "$1" "$2"; }
said() { python3 -c 'import json,sys; r=json.load(sys.stdin)["result"]; t=[c["text"] for c in r["content"] if c.get("type")=="text"]; body=" | ".join(x for x in (t[:-1] if len(t)>1 else t)).replace(chr(10)," | "); print(("ERROR " if r.get("isError") else "OK ") + body[:600])' 2>&1; }
field() { python3 -c 'import json,sys; print(json.load(sys.stdin)["result"].get("structuredContent",{}).get(sys.argv[1],""))' "$1" 2>/dev/null; }
if [ -n "${ASK_COPY:-}" ] && [ -n "${TD:-}" ]; then
  out=${ASK_OUT:-$HOME/.cache/war-room-rig/copies.out}; : > "$out"
  sleep 8
  panes=$(rpc "$(call list_panes '{}')")
  IFS=',' read -ra want <<< "$ASK_COPY"
  for title in "${want[@]}"; do
    pid=$(printf '%s' "$panes" | python3 -c 'import json,sys; t=sys.argv[1]; d=json.load(sys.stdin)["result"]["structuredContent"]["panes"]; print(next((p["pid"] for p in d if p["title"]==t), 0))' "$title" 2>/dev/null)
    ans=$(rpc "$(call ask_agent "{\"pid\": $pid, \"question\": \"What exactly is left before you can tag?\", \"copy\": true}")")
    printf '%s  ask_agent copy -> %s: %s\n' "$(date +%T)" "$title" "$(printf '%s' "$ans" | said)" >> "$out"
    id=$(printf '%s' "$ans" | field ask_id)
    [ -z "$id" ] && continue
    for i in $(seq 1 60); do
      sleep 2
      back=$(rpc "$(call ask_answer "{\"ask_id\": $id}")")
      stage=$(printf '%s' "$back" | field stage)
      case "$stage" in running|awaiting_press) continue;; esac
      break
    done
    printf '%s  ask_answer %s (%s): %s\n' "$(date +%T)" "$id" "$title" "$(printf '%s' "$back" | said)" >> "$out"
  done
  printf '%s  ALL-ASKS-FINISHED\n' "$(date +%T)" >> "$out"
fi
while :; do sleep 3600; done
