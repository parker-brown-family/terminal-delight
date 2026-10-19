# shellcheck shell=bash
# A real terminal-delight window nobody can see, for the scripts that drive
# one through its control socket: scripts/doc-float-soak.sh,
# doc-split-check.sh, doc-notes-check.sh and doc-page-soak.sh. Sourced, never
# run. The caller sets $TD, the binary, before it launches anything.
#
# THE WINDOW IS HIDDEN. Hyprland launches it onto a special workspace with
# `render_unfocused` and `no_initial_focus`, so nothing appears on screen, no
# tile is resized and focus never moves. If it lands anywhere but that
# workspace it is killed at once and the run exits 4, without retrying.
#
#   hidden_launch <session> <workspace> <log> <command>
#       Launch <command> — a shell command line Hyprland runs, which must set
#       TD_SESSION=<session>, exec $TD and write its output to <log> — on
#       <workspace>, a `special:` name. Waits for the window titled for that
#       session, checks where it landed, and waits until its control socket
#       answers. Sets WIN to its pid. Exits 1 if no window appears or Hyprland
#       refuses the launch, 4 if it is not hidden. <command> may not contain
#       a double quote: it travels inside a Lua string.
#   hidden_close
#       Close the window in WIN, wait for it to exit, and forget it. Its
#       session host keeps running, as it would for a person closing it.
#   ctl <args…>
#       `terminal-delight ctl` against WIN, the first line of the answer with
#       the answering window's pid taken off.
#   hidden_cleanup
#       Everything the run made goes with it: hidden_stop, then hidden_sweep.
#       Scripts with more of their own to tidy call the two halves around it.
#   hidden_stop
#       Kill every window still open and every session's `serve --session`
#       host, by pid, and wait for them to exit.
#   hidden_sweep
#       Remove each session's files — sessions/<session>.* and the backups
#       directory every layout save rotates into, sessions/backups/<session>/
#       — and the browser profile each window left in $XDG_RUNTIME_DIR. Those
#       are written as the processes exit, which is why they are swept only
#       once hidden_stop has seen both go.

WIN=""
HIDDEN_WINDOWS=""
HIDDEN_SESSIONS=""
HIDDEN_SESSION_DIR="$HOME/.config/terminal-delight/sessions"

# The client prefixes each reply with the answering window's pid and a tab.
ctl() { "$TD" ctl --pid "$WIN" "$@" 2>&1 | head -1 | sed 's/^[0-9]*\t//'; }

hidden_launch() {
  local session=$1 ws=$2 log=$3 command=$4 said landed
  # Known before the launch, so a host that comes up for a window that then
  # fails its checks is still found and stopped.
  case " $HIDDEN_SESSIONS " in *" $session "*) ;; *) HIDDEN_SESSIONS="$HIDDEN_SESSIONS $session" ;; esac
  said=$(hyprctl dispatch "hl.dsp.exec_cmd(\"$command\", { workspace = \"$ws silent\", render_unfocused = true, no_initial_focus = true })" 2>&1)
  case "$said" in ok|"") ;; *) echo "hyprctl refused the launch: $said"; exit 1 ;; esac
  WIN=""
  for _ in $(seq 1 60); do
    WIN=$(hyprctl clients -j 2>/dev/null | jq -r --arg t "terminal-delight — $session" \
      '.[] | select(.title==$t) | .pid' | head -1)
    [ -n "$WIN" ] && break
    sleep 0.5
  done
  [ -n "$WIN" ] || { echo "no window appeared — see $log"; exit 1; }
  # Tracked before the check, so cleanup waits for it to exit either way.
  HIDDEN_WINDOWS="$HIDDEN_WINDOWS $WIN"
  landed=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .workspace.name' | head -1)
  if [ "$landed" != "$ws" ]; then
    echo "the window landed on '$landed', not $ws — killed, nothing ran"
    kill "$WIN" 2>/dev/null
    exit 4
  fi
  for _ in $(seq 1 40); do [ "$(ctl ping)" = "pong" ] && break; sleep 0.5; done
}

hidden_close() {
  [ -n "$WIN" ] || return 0
  kill "$WIN" 2>/dev/null
  for _ in $(seq 1 50); do kill -0 "$WIN" 2>/dev/null || break; sleep 0.1; done
  # Forgotten, so cleanup never signals a pid the system may have handed on.
  HIDDEN_WINDOWS=$(printf '%s\n' $HIDDEN_WINDOWS | grep -vx "$WIN" | tr '\n' ' ')
  WIN=""
}

hidden_stop() {
  local pids p s alive
  pids="$HIDDEN_WINDOWS"
  # Anchored at the end of the name, so this run's docsoak12 never takes
  # another run's docsoak123 with it.
  for s in $HIDDEN_SESSIONS; do pids="$pids $(pgrep -f "serve --session $s( |\$)" 2>/dev/null)"; done
  for p in $pids; do kill "$p" 2>/dev/null; done
  for _ in $(seq 1 50); do
    alive=0
    for p in $pids; do kill -0 "$p" 2>/dev/null && alive=1; done
    [ "$alive" -eq 0 ] && break
    sleep 0.1
  done
}

hidden_sweep() {
  local s w
  for s in $HIDDEN_SESSIONS; do
    rm -f "$HIDDEN_SESSION_DIR/$s".*
    rm -rf "$HIDDEN_SESSION_DIR/backups/$s"
  done
  # A window killed outright leaves its browser's profile for the next TD to
  # sweep; these windows will not be back, so sweep their own now.
  for w in $HIDDEN_WINDOWS; do rm -rf "${XDG_RUNTIME_DIR:-/nonexistent}/terminal-delight/chromium-$w-"*; done
}

hidden_cleanup() {
  hidden_stop
  hidden_sweep
}
