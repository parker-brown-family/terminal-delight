#!/usr/bin/env bash
# A/B: does BUN_OPTIONS=--smol shrink an idle Claude Code? Two disposable --fork-session copies of the same
# 20 MB conversation, side by side. Each pid is taken from its OWN tmux pane and verified before any signal.
set -u
S=$(dirname "$0"); LOG=$S/probe5.log; : > $LOG
log(){ printf '%s %s\n' "$(date +%T.%3N)" "$*" | tee -a $LOG; }
SID=80915041-fe1f-413a-9f48-95217cb9ec6b
start(){ # $1 = tmux socket, $2 = extra env
  tmux -L $1 -f /dev/null new-session -d -s p -x 200 -y 50 "cd /home/parker && exec env -u TD_PANE_ID -u TD_SESSION -u TD_TAG $2 /home/parker/.local/bin/claude --resume $SID --fork-session"; }
pidof_pane(){ local p=$(tmux -L $1 list-panes -t p -F '#{pane_pid}' 2>/dev/null)
  [ -n "$p" ] && [ "$(cat /proc/$p/comm 2>/dev/null)" = claude ] && grep -q fork-session /proc/$p/cmdline && echo $p; }
ready(){ for i in $(seq 1 1500); do tmux -L $1 capture-pane -p -t p 2>/dev/null | grep -q 'shift+tab' && return 0; sleep 0.04; done; return 1; }
start smolA ""; start smolB "BUN_OPTIONS=--smol"
ready smolA; ready smolB
A=$(pidof_pane smolA); B=$(pidof_pane smolB)
[ -n "$A" ] && [ -n "$B" ] || { log "ABORT A=$A B=$B"; tmux -L smolA kill-server; tmux -L smolB kill-server; exit 1; }
log "A(default)=$A B(--smol)=$B B_env_has_smol=$(tr '\0' '\n' </proc/$B/environ | grep -c '^BUN_OPTIONS=--smol')"
m(){ awk '/^Rss:/{r=$2} /^Pss:/{p=$2} /^Anonymous:/{a=$2} END{printf "rss=%dM pss=%dM anon=%dM", r/1024, p/1024, a/1024}' /proc/$1/smaps_rollup; }
cpu(){ awk '{print $14+$15}' /proc/$1/stat; }
sleep 60; log "t60  A: $(m $A)   B: $(m $B)"
a0=$(cpu $A); b0=$(cpu $B); sleep 60
log "t120 A: $(m $A) cpu60=$(( $(cpu $A)-a0 ))t   B: $(m $B) cpu60=$(( $(cpu $B)-b0 ))t"
sleep 60; log "t180 A: $(m $A)   B: $(m $B)"
for x in $A $B; do [ "$(cat /proc/$x/comm 2>/dev/null)" = claude ] && grep -q fork-session /proc/$x/cmdline && kill -TERM $x; done
sleep 2; tmux -L smolA kill-server 2>/dev/null; tmux -L smolB kill-server 2>/dev/null; log done
