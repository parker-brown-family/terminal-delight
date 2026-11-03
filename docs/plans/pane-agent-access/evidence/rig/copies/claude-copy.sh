#!/bin/bash
# The copy side of the rig's stand-in: started by the window exactly as a real
# copy is, it records what it was given (arguments, the NAMES of its
# environment, its folder), then speaks Claude's stream-json: servers on
# request, the init with its tools once the question arrives, an answer and a
# result with what it read.
log=$HOME/.cache/war-room-rig/copies.log
{ printf 'argv:'; printf ' %q' "$@"; printf '\nenv: %s\ncwd: %s\n' "$(env | cut -d= -f1 | sort | tr '\n' ' ')" "$PWD"; } >> "$log"
resume=; prev=
for a in "$@"; do [ "$prev" = --resume ] && resume=$a; prev=$a; done
read -r status
rid=$(printf '%s' "$status" | python3 -c 'import json,sys; print(json.load(sys.stdin)["request_id"])')
printf '{"type":"control_response","response":{"subtype":"success","request_id":"%s","response":{"mcpServers":[]}}}\n' "$rid"
read -r question
printf 'question: %s chars (the words stay in memory)\n---\n' "${#question}" >> "$log"
printf '{"type":"system","subtype":"init","tools":["Read"],"mcp_servers":[]}\n'
sleep 2
slug=$(printf '%s' "$PWD" | sed 's/[^A-Za-z0-9]/-/g')
read_tokens=$(python3 -c 'import json,sys; print(json.loads(open(sys.argv[1]).readline())["message"]["usage"]["cache_read_input_tokens"])' "$HOME/.claude/projects/$slug/$resume.jsonl" 2>/dev/null || echo 0)
answer="(copy of conversation ${resume:0:8} in ${PWD##*/}) Two tests are left on the expiry path; the refresh retry is done and reviewed."
printf '{"type":"assistant","message":{"content":[{"type":"text","text":"%s"}]}}\n' "$answer"
printf '{"type":"result","subtype":"success","is_error":false,"result":"%s","usage":{"input_tokens":3,"cache_read_input_tokens":%s,"cache_creation_input_tokens":0,"output_tokens":40},"total_cost_usd":0.21}\n' "$answer" "$read_tokens"
sleep 60
