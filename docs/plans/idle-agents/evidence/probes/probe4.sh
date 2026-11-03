#!/usr/bin/env bash
# Is the slow wake real? Controls: keystroke echo with no freeze (x3), then cgroup freeze 10s -> thaw -> key at once
# (sampling CPU and thread state), then freeze -> thaw -> wait 40s -> key, then SIGSTOP/SIGCONT -> key. Bounded waits.
set -u
S=$(dirname "$0"); LOG=$S/probe4.log; : > $LOG
ms(){ date +%s%3N; }
log(){ printf '%s %s\n' "$(date +%T.%3N)" "$*" | tee -a $LOG; }
T="tmux -L idleprobe4"; SID=80915041-fe1f-413a-9f48-95217cb9ec6b
waitprompt(){ for i in $(seq 1 3000); do $T capture-pane -p -t p 2>/dev/null | grep -q 'shift+tab' && return 0; sleep 0.01; done; return 1; }
# the prompt draws U+00A0 after the chevron, so match the typed char at the end of the chevron line
waitecho(){ for i in $(seq 1 4000); do $T capture-pane -p -t p 2>/dev/null | grep '❯' | grep -q "$1[[:space:][:cntrl:]]*\$" && return 0; sleep 0.005; done; return 1; }
waitgone(){ for i in $(seq 1 1000); do kill -0 $1 2>/dev/null || return 0; sleep 0.01; done; return 1; }
cpu(){ awk '{print $14+$15}' /proc/$CL/stat; }
key(){ local w=$(ms); $T send-keys -t p "$1"; waitecho "$1"; local rc=$?; log "$2 echo_ms=$(( $(ms)-w )) rc=$rc"; $T send-keys -t p BSpace; sleep 2; }
$T -f /dev/null new-session -d -s p -x 200 -y 50 "cd /home/parker && exec env -u TD_PANE_ID -u TD_SESSION -u TD_TAG /home/parker/.local/bin/claude --resume $SID --fork-session"
waitprompt; CL=$($T list-panes -t p -F "#{pane_pid}")
[ -n "$CL" ] && [ "$(cat /proc/$CL/comm 2>/dev/null)" = claude ] && grep -q fork-session /proc/$CL/cmdline || { log "ABORT: pane pid $CL is not the probe claude"; $T kill-server; exit 1; }
CG=/sys/fs/cgroup$(cut -d: -f3 /proc/$CL/cgroup); log "claude=$CL threads=$(ls /proc/$CL/task | wc -l)"
sleep 30
key a C0_nofreeze_1; key b C0_nofreeze_2; key c C0_nofreeze_3
# C1: freeze, thaw, key at once, sample CPU every 100ms meanwhile
echo 1 > $CG/cgroup.freeze; sleep 10; c0=$(cpu); w=$(ms); echo 0 > $CG/cgroup.freeze; $T send-keys -t p 'd'
( for i in $(seq 1 400); do printf '%s:%s:%s ' $(( $(ms)-w )) $(( $(cpu)-c0 )) "$(awk '{print $3}' /proc/$CL/stat)"; $T capture-pane -p -t p | grep '❯' | grep -q 'd[[:space:]]*$' && break; sleep 0.1; done ) > $S/probe4_c1_samples.txt
log "C1_thaw_then_key echo_ms=$(( $(ms)-w )) samples(ms:cpu_ticks:state)=$(head -c 900 $S/probe4_c1_samples.txt)"
$T send-keys -t p BSpace; sleep 3
# C2: freeze, thaw, wait 40s, then key
echo 1 > $CG/cgroup.freeze; sleep 10; echo 0 > $CG/cgroup.freeze; sleep 40; key e C2_thaw_wait40_then_key
# C3: SIGSTOP/SIGCONT instead of the cgroup freezer
kill -STOP $CL; sleep 10; kill -CONT $CL; key f C3_sigstop_sigcont_then_key
# C4: freeze again, thaw, key at once (repeat of C1 for variance)
echo 1 > $CG/cgroup.freeze; sleep 10; echo 0 > $CG/cgroup.freeze; key g C4_thaw_then_key_repeat
# C5: freeze, reclaim everything, wait, thaw, key at once
snapm(){ log "$1 cg_mem=$(( $(cat $CG/memory.current)/1048576 ))M cg_swap=$(( $(cat $CG/memory.swap.current)/1048576 ))M majflt=$(awk '{print $12}' /proc/$CL/stat)"; }
echo 1 > $CG/cgroup.freeze; sleep 1; echo 8G > $CG/memory.reclaim 2>/dev/null; sleep 10; snapm C5_frozen_reclaimed
echo 0 > $CG/cgroup.freeze; key h C5_reclaimed_thaw_then_key; snapm C5_after
echo 1 > $CG/cgroup.freeze; sleep 1; echo 8G > $CG/memory.reclaim 2>/dev/null; sleep 10; echo 0 > $CG/cgroup.freeze; key i C6_reclaimed_repeat
FORK=$(ls -l /proc/$CL/fd 2>/dev/null | grep -oE '/home/parker/.claude/projects/[^ ]+\.jsonl' | head -1); log "fork=$FORK"
kill -TERM $CL; waitgone $CL; $T kill-server 2>/dev/null; log done
