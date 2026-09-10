#!/usr/bin/env bash
# Does the work survive the window?
#
# This is the metric the client-server split exists to move, and it is a
# measurement rather than an argument: stage a scene of four terminals — three
# thousand lines of scrollback, a vim, an htop, an idle shell — kill the window
# outright, and count what is gone.
#
#   ./scripts/td-survival-test.sh gui-kill --cycles 20
#   ./scripts/td-survival-test.sh floor-control --cycles 20
#   ./scripts/td-survival-test.sh host-kill --cycles 3
#   ./scripts/td-survival-test.sh pane-exit --cycles 3
#   ./scripts/td-survival-test.sh legacy-load
#
# THE FLOOR CONTROL IS NOT OPTIONAL. A harness that reports zero losses against
# a build without session hosts is not measuring anything, and would report zero
# just as happily against a broken one. `floor-control` runs the same scene on
# today's path, where every kill is expected to take everything with it: it must
# report losses == cycles, and if it does not, the instrument is wrong and the
# other legs' numbers are worthless.
#
# `host-kill` is the never-worse gate: kill the host as well, and the recovery
# must be exactly today's — the layout restored from the file, the shells
# started again, the scrollback gone. Losses there are compared against the
# floor, not against zero.
#
# `legacy-load` is about a number that changed under people. A new split stops
# at four panes now, but layouts written before that hold up to eight, and a
# loader that enforced the new cap would open one of them with terminals
# missing. So it hand-writes an eight-pane tab and requires every one of them to
# come up.
#
# `pane-exit` is the opposite question, and the one this harness was missing:
# when a terminal genuinely ENDS, does the window notice? Everything else here
# measures work surviving, so a window that had quietly stopped hearing about
# endings would have passed every leg while leaving dead terminals on screen
# forever. It ends one terminal and requires the window to outlive it, then ends
# the rest and requires the window to close — which is what a terminal emulator
# has always done when its last shell exits.
#
# Isolation, because this runs on the machine Terminal Delight is developed on:
#
#   * a private XDG_CONFIG_HOME, so it cannot see, adopt, write or delete any
#     session of yours;
#   * a private XDG_RUNTIME_DIR, so its host sockets cannot land beside your
#     real ones — with the Wayland socket symlinked in, because a window still
#     has to reach the compositor;
#   * the debug binary from this worktree, never whatever is on your PATH.
#
# What it cannot isolate is the screen. A window opens and is killed once per
# cycle, on your display, and it will take focus while it is up.

set -uo pipefail

LEG="${1:-}"
shift || true
CYCLES=5
KEEP=0
while [ $# -gt 0 ]; do
  case "$1" in
    --cycles) CYCLES="$2"; shift 2 ;;
    --keep) KEEP=1; shift ;;
    *) echo "td-survival-test: unknown argument $1" >&2; exit 2 ;;
  esac
done

case "$LEG" in
  gui-kill|floor-control|host-kill|pane-exit|legacy-load) ;;
  *)
    echo "usage: td-survival-test.sh {gui-kill|floor-control|host-kill|pane-exit|legacy-load} [--cycles N] [--keep]" >&2
    exit 2
    ;;
esac

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(dirname "$HERE")"
BIN="${TD_BIN:-$REPO/app/target/debug/terminal-delight}"
if [ ! -x "$BIN" ]; then
  echo "td-survival-test: no binary at $BIN — run: cd app && cargo build" >&2
  exit 2
fi

# The two programs the metric is really about: a full-screen editor and a
# full-screen monitor, neither of which can be resumed. Which ones exist varies
# by machine — this box has neovim and btop where the plan said vim and htop —
# so they are looked up rather than assumed, and named in the summary so a
# number can never be read as being about a program that was never there.
EDITOR_BIN=""
for candidate in vim nvim vi; do
  if command -v "$candidate" >/dev/null 2>&1; then EDITOR_BIN="$candidate"; break; fi
done
MONITOR_BIN=""
for candidate in htop btop top; do
  if command -v "$candidate" >/dev/null 2>&1; then MONITOR_BIN="$candidate"; break; fi
done
if [ -z "$EDITOR_BIN" ] || [ -z "$MONITOR_BIN" ]; then
  echo "td-survival-test: this box has no full-screen editor and/or monitor to stage \
(looked for vim/nvim/vi and htop/btop/top) — the scene cannot be built" >&2
  exit 2
fi

MARK="td-survival-$$"
RUN="$(mktemp -d "/tmp/${MARK}-XXXXXX")"
SESSION="survival"
export XDG_CONFIG_HOME="$RUN/config"
REAL_RUNTIME="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export XDG_RUNTIME_DIR="$RUN/run"
mkdir -p "$XDG_CONFIG_HOME/terminal-delight/sessions" "$XDG_RUNTIME_DIR/terminal-delight"
chmod 700 "$XDG_RUNTIME_DIR"
# A window still has to reach the compositor, and Wayland finds its socket
# relative to the runtime directory we have just replaced.
if [ -n "${WAYLAND_DISPLAY:-}" ] && [ -e "$REAL_RUNTIME/$WAYLAND_DISPLAY" ]; then
  ln -sf "$REAL_RUNTIME/$WAYLAND_DISPLAY" "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY"
  [ -e "$REAL_RUNTIME/$WAYLAND_DISPLAY.lock" ] &&
    ln -sf "$REAL_RUNTIME/$WAYLAND_DISPLAY.lock" "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY.lock"
fi

SOCKET="$XDG_RUNTIME_DIR/terminal-delight/session-$SESSION.sock"
STATE="$XDG_CONFIG_HOME/terminal-delight/sessions/$SESSION.toml"
SENTINEL="$RUN/sentinel.txt"
GUI_LOG="$RUN/gui.log"
HOST_LOG="$XDG_RUNTIME_DIR/terminal-delight/session-$SESSION.log"

cleanup() {
  [ -n "${GUI_PID:-}" ] && kill -9 "$GUI_PID" 2>/dev/null
  type kill_scene >/dev/null 2>&1 && kill_scene
  # the host, if one is running: ask, then insist
  if [ -S "$SOCKET" ]; then
    printf '{"verb":"hello","proto":1,"kind":"tool"}\n{"verb":"shutdown"}\n' |
      timeout 2 node "$RUN/say.mjs" "$SOCKET" >/dev/null 2>&1
  fi
  pkill -9 -f "serve --session $SESSION" 2>/dev/null
  if [ "$KEEP" = "1" ]; then
    echo "td-survival-test: kept $RUN" >&2
  else
    rm -rf "$RUN"
  fi
}
trap cleanup EXIT INT TERM

# ---------------------------------------------------------------- the client --
# One control conversation per invocation: connect, greet, send the verbs on
# stdin, print the replies. Enough to stage a scene and to ask what survived.
cat >"$RUN/say.mjs" <<'NODE'
import net from "node:net";
const socket = net.connect(process.argv[2]);
const lines = [];
let stdin = "";
process.stdin.on("data", (c) => (stdin += c));
process.stdin.on("end", () => {
  socket.write(stdin.endsWith("\n") ? stdin : stdin + "\n");
});
socket.on("data", (chunk) => {
  lines.push(chunk.toString());
  const text = lines.join("");
  // one reply per request line sent
  const wanted = stdin.trim().split("\n").filter(Boolean).length;
  if (text.trim().split("\n").length >= wanted) {
    process.stdout.write(text);
    socket.end();
    process.exit(0);
  }
});
socket.on("error", (e) => {
  console.error(String(e));
  process.exit(1);
});
setTimeout(() => {
  process.stdout.write(lines.join(""));
  process.exit(lines.length ? 0 : 1);
}, 5000);
NODE

# Attach to a pane, read what the host sends (the snapshot of everything it has
# printed, then live output), and print it. This is how the harness reads a
# terminal's scrollback without a window: the same handover a window gets.
#
# COUNTING WHAT COMES BACK: a line that was TYPED into a pane appears TWICE in
# what this returns, and neither copy is a bug. The line discipline echoes a
# keystroke as it arrives, and then the program in the pane writes it out again
# — `cat` most obviously, a shell as part of its prompt redraw. So "it appears
# twice, therefore it was sent twice" is wrong, and it is wrong in the direction
# of reporting a duplicate that is not there.
#
# The way to count a typing is to move one variable and compare: run the same
# measurement with one ask and with two, and see whether the number changes. It
# did not, which is how the recipe-dedupe was verified end to end. This note is
# here rather than in a test comment because this is the file somebody reaches
# for when they want to count something in a pane — the trap was already
# written down two slices ago, in a place nobody had a reason to look.
cat >"$RUN/peek.mjs" <<'NODE'
import net from "node:net";
const [, , path, pane, millis] = process.argv;
const socket = net.connect(path);
const chunks = [];
socket.on("connect", () => socket.write(`stream ${pane}\n`));
socket.on("data", (c) => chunks.push(c));
socket.on("error", () => process.exit(1));
setTimeout(() => {
  process.stdout.write(Buffer.concat(chunks));
  socket.destroy();
  process.exit(0);
}, Number(millis || 1500));
NODE

say() { printf '%s\n' "$@" | timeout 8 node "$RUN/say.mjs" "$SOCKET" 2>/dev/null; }

hello_line='{"verb":"hello","proto":1,"kind":"tool"}'

# ------------------------------------------------------------------ the scene --
# Four terminals, staged the way a restore stages them: the layout on disk names
# each pane's recipe, and the window types it in. Both legs stage identically, so
# the two numbers are comparable.
printf 'sentinel file for %s\n' "$MARK" >"$SENTINEL"

# A stand-in for an agent, because the metric is about a window dying during
# real work and real work here means an agent pane.
#
# It is a script named `claude` on purpose: Terminal Delight classifies a pane
# by the program in the foreground, so this is what makes the pane an AGENT
# pane — the badge, the bell, the keepalive clock and the vitals sweep all take
# the branch they would take for the real thing. It is emphatically NOT the real
# thing: staging a genuine agent would need credentials and would spend money to
# measure a property that has nothing to do with what the agent is saying.
mkdir -p "$RUN/bin"
cat >"$RUN/bin/claude" <<'AGENT'
#!/usr/bin/env bash
# a stand-in for an agent: an alternate screen, a spinner, and a turn that
# never finishes — the shape of a pane you would hate to lose.
printf '\033[?1049h\033[H'
trap 'printf "\033[?1049l"; exit 0' TERM INT
i=0
while true; do
  i=$((i + 1))
  printf '\033[H\033[2K  thinking… (%s)  esc to interrupt\n' "$i"
  sleep 1
done
AGENT
chmod +x "$RUN/bin/claude"
cat >"$STATE" <<TOML
active = 0
panes = 5
win = [40.0, 60.0, 760.0, 460.0]

[[tabs]]
name = "scrollback"
[tabs.node.Leaf]
cwd = "$RUN"
resume = "seq -f '%06g' 1 3000"

[[tabs]]
name = "editor"
[tabs.node.Leaf]
cwd = "$RUN"
resume = "$EDITOR_BIN $SENTINEL"

[[tabs]]
name = "monitor"
[tabs.node.Leaf]
cwd = "$RUN"
resume = "$MONITOR_BIN"

[[tabs]]
name = "idle"
[tabs.node.Leaf]
cwd = "$RUN"
resume = "echo $MARK-idle-ready"

[[tabs]]
name = "agent"
[tabs.node.Leaf]
cwd = "$RUN"
resume = "$RUN/bin/claude"
[tabs.node.Leaf.note]
title = "SURVIVE"
text = "$MARK note"
seed = 7
pinned = true
TOML
cp "$STATE" "$RUN/staged.toml"

alive() { kill -0 "$1" 2>/dev/null; }

# Which of the scene's programs are running, and specifically OURS.
#
# The editor carries the run's own sentinel path in its arguments, so it names
# itself. The monitor takes no argument that could carry a marker, so it is
# identified by exclusion: every one of them running before this started is
# remembered, and the one that appears afterwards is the one this harness
# staged. Matching it by name alone would have this script report that a
# person's own btop survived a window it was never in.
PRE_MONITORS=" $(pgrep -x "$MONITOR_BIN" 2>/dev/null | tr '\n' ' ') "

vim_pid() { pgrep -f -- "$EDITOR_BIN $SENTINEL" 2>/dev/null | head -1; }

agent_pid() { pgrep -f -- "$RUN/bin/claude" 2>/dev/null | head -1; }

# The note the layout staged, as it stands in the file the window writes back.
note_survives() { grep -q "$MARK note" "$STATE" 2>/dev/null; }

htop_pid() {
  local p
  for p in $(pgrep -x "$MONITOR_BIN" 2>/dev/null); do
    case "$PRE_MONITORS" in
      *" $p "*) ;;
      *) echo "$p"; return 0 ;;
    esac
  done
  return 1
}

scene_is_up() {
  [ -n "$(vim_pid)" ] && [ -n "$(htop_pid)" ] && [ -n "$(agent_pid)" ]
}

# What the harness could see when it decided something was missing.
#
# A scene that fails to come up is the one failure that says nothing about the
# thing being measured, so it has to say everything about itself instead: what
# the host thinks it is running, what each of those terminals is showing, and
# which processes this machine has.
diagnose() {
  local cycle="$1"
  local file="$RUN/diagnosis-cycle-$cycle.txt"
  {
    echo "== list-panes"
    host_panes_json
    echo "== $EDITOR_BIN: $(vim_pid)  $MONITOR_BIN: $(htop_pid)"
    echo "== ps"
    pgrep -a -x "$MONITOR_BIN" 2>/dev/null
    pgrep -a -f -- "$RUN" 2>/dev/null | head -20
    local ids id
    ids="$(host_pane_pids)"
    for id in $ids; do
      echo "== pane ${id%%:*}"
      timeout 6 node "$RUN/peek.mjs" "$SOCKET" "${id%%:*}" 1200 2>/dev/null |
        tr -dc '[:print:]\n' | grep -v '^$' | head -12
    done
    echo "== gui log"
    tail -20 "$GUI_LOG" 2>/dev/null
    echo "== host log"
    tail -20 "$HOST_LOG" 2>/dev/null
  } >"$file" 2>&1
  echo "$file"
}

kill_scene() {
  local p
  p="$(htop_pid)" && [ -n "$p" ] && kill -9 "$p" 2>/dev/null
  p="$(vim_pid)" && [ -n "$p" ] && kill -9 "$p" 2>/dev/null
  pkill -9 -f -- "$SENTINEL" 2>/dev/null
  pkill -9 -f -- "$RUN/bin/claude" 2>/dev/null
}

wait_for() { # wait_for <seconds> <command...>
  local limit="$1"; shift
  local deadline=$(( $(date +%s) + limit ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    if "$@" >/dev/null 2>&1; then return 0; fi
    sleep 0.25
  done
  "$@" >/dev/null 2>&1
}

host_panes_json() { say "$hello_line" '{"verb":"list-panes"}' | tail -1; }

host_pane_pids() {
  host_panes_json | node -e '
    let s = ""; process.stdin.on("data", c => s += c).on("end", () => {
      try {
        const panes = JSON.parse(s).panes || [];
        console.log(panes.filter(p => !p.ended).map(p => `${p.pane}:${p.shell_pid}`).join(" "));
      } catch { console.log(""); }
    });'
}

host_is_attended() {
  say "$hello_line" | head -1 | grep -q '"attended":true'
}

launch_gui() { # launch_gui <hosted|serverless>
  # Since 2026-09-10 hosted is the DEFAULT, so a plain launch hosts and the
  # opt-out env picks serverless. Before the flip this was the other way round —
  # hosted needed TD_SESSIOND=1 and a plain launch was serverless — which would
  # now make the floor-control leg run hosted and quietly stop losing.
  local mode="$1"
  if [ "$mode" = serverless ]; then
    TD_NO_SESSIOND=1 TD_SESSION="$SESSION" "$BIN" >>"$GUI_LOG" 2>&1 &
  else
    TD_SESSION="$SESSION" "$BIN" >>"$GUI_LOG" 2>&1 &
  fi
  GUI_PID=$!
}

kill_gui() {
  [ -n "${GUI_PID:-}" ] || return 0
  kill -9 "$GUI_PID" 2>/dev/null
  wait "$GUI_PID" 2>/dev/null
  GUI_PID=""
}

# ------------------------------------------------------------------- the legs --
losses=0
cycles_run=0
failures=()

note() { echo "td-survival-test: $*" >&2; }
lose() { failures+=("cycle $1: $2"); }

run_gui_kill() {
  # Cycle one stages the scene; every cycle after it is the measurement.
  for cycle in $(seq 1 "$CYCLES"); do
    launch_gui hosted
    if ! wait_for 30 test -S "$SOCKET"; then
      lose "$cycle" "no session host ever appeared"
      losses=$((losses + 1)); cycles_run=$((cycles_run + 1)); kill_gui; continue
    fi
    if ! wait_for 40 scene_is_up; then
      lose "$cycle" "the scene never came up ($EDITOR_BIN/$MONITOR_BIN missing); see $(diagnose "$cycle")"
      losses=$((losses + 1)); cycles_run=$((cycles_run + 1)); kill_gui; continue
    fi
    wait_for 20 host_is_attended
    sleep 2

    local before_panes before_vim before_htop before_agent
    before_panes="$(host_pane_pids)"
    before_vim="$(vim_pid)"
    before_htop="$(htop_pid)"
    before_agent="$(agent_pid)"

    kill_gui
    sleep 1

    local lost=0
    local after_panes
    after_panes="$(host_pane_pids)"
    if [ -z "$before_panes" ] || [ "$before_panes" != "$after_panes" ]; then
      lose "$cycle" "the host's terminals changed across the kill: [$before_panes] -> [$after_panes]"
      lost=1
    fi
    if [ -z "$before_vim" ] || ! alive "$before_vim"; then
      lose "$cycle" "$EDITOR_BIN did not survive the window"
      lost=1
    fi
    if [ -z "$before_htop" ] || ! alive "$before_htop"; then
      lose "$cycle" "$MONITOR_BIN did not survive the window"
      lost=1
    fi
    if [ -z "$before_agent" ] || ! alive "$before_agent"; then
      lose "$cycle" "the agent pane did not survive the window"
      lost=1
    fi
    if ! note_survives; then
      lose "$cycle" "the sticky note is not in the layout the window wrote"
      lost=1
    fi
    # Scrollback: attach to the pane that printed three thousand lines and read
    # what the host hands a client arriving cold. The first line and the last
    # must both be in it.
    local pane_one pane_two
    pane_one="$(echo "$before_panes" | tr ' ' '\n' | sed -n 1p | cut -d: -f1)"
    pane_two="$(echo "$before_panes" | tr ' ' '\n' | sed -n 2p | cut -d: -f1)"
    if [ -n "$pane_one" ]; then
      local seen
      seen="$(timeout 10 node "$RUN/peek.mjs" "$SOCKET" "$pane_one" 1500 2>/dev/null | tr -dc '[:print:]\n')"
      echo "$seen" | grep -q "000001" || { lose "$cycle" "the first line of scrollback was gone"; lost=1; }
      echo "$seen" | grep -q "003000" || { lose "$cycle" "the last line of scrollback was gone"; lost=1; }
    else
      lose "$cycle" "no pane to read scrollback from"
      lost=1
    fi
    # The pane sitting inside a full-screen editor: what a client arriving cold
    # is shown must be the editor's screen, not the shell behind it. This is the
    # case the design was least sure of — an alternate screen has no scrollback
    # of its own, and the grid beneath it cannot be read without disturbing the
    # program on top — so it is asserted rather than assumed.
    if [ -n "$pane_two" ]; then
      local editor_screen
      editor_screen="$(timeout 10 node "$RUN/peek.mjs" "$SOCKET" "$pane_two" 1500 2>/dev/null | tr -dc '[:print:]\n')"
      echo "$editor_screen" | grep -q "sentinel file for" ||
        { lose "$cycle" "the editor's own screen was not in what a fresh client is sent"; lost=1; }
    else
      lose "$cycle" "no editor pane to read"
      lost=1
    fi

    losses=$((losses + lost))
    cycles_run=$((cycles_run + 1))
    note "cycle $cycle: $([ "$lost" = 0 ] && echo "nothing lost" || echo "LOST")"
  done
  # Leave the scene as we found it.
  kill_gui
}

run_floor_control() {
  # The same scene on today's path, where the window owns the terminals. Every
  # kill is expected to take everything with it, and this leg exists to prove
  # the instrument can see that.
  for cycle in $(seq 1 "$CYCLES"); do
    cp "$RUN/staged.toml" "$STATE"
    launch_gui serverless
    if ! wait_for 40 scene_is_up; then
      lose "$cycle" "the scene never came up ($EDITOR_BIN/$MONITOR_BIN missing)"
      cycles_run=$((cycles_run + 1)); kill_gui; continue
    fi
    sleep 2

    local before_vim before_htop before_agent
    before_vim="$(vim_pid)"
    before_htop="$(htop_pid)"
    before_agent="$(agent_pid)"

    kill_gui
    sleep 1

    local lost=0
    if [ -n "$before_vim" ] && alive "$before_vim"; then
      lose "$cycle" "$EDITOR_BIN outlived the window on a path with no session host"
    else
      lost=1
    fi
    if [ -n "$before_htop" ] && alive "$before_htop"; then
      lose "$cycle" "$MONITOR_BIN outlived the window on a path with no session host"
    else
      lost=1
    fi
    if [ -n "$before_agent" ] && alive "$before_agent"; then
      lose "$cycle" "the agent pane outlived the window on a path with no session host"
    else
      lost=1
    fi
    if [ -S "$SOCKET" ]; then
      lose "$cycle" "a session host appeared on the serverless path"
    fi
    losses=$((losses + lost))
    cycles_run=$((cycles_run + 1))
    note "cycle $cycle: $([ "$lost" = 0 ] && echo "nothing lost — WHICH IS THE BUG" || echo "lost, as expected")"
    kill_scene
    sleep 0.5
  done
}

run_host_kill() {
  # The never-worse gate. Kill the host too, and what comes back must be exactly
  # what today's build gives you: the layout from the file, the shells started
  # again, the scrollback gone. Losses here are compared against the floor.
  for cycle in $(seq 1 "$CYCLES"); do
    launch_gui hosted
    if ! wait_for 30 test -S "$SOCKET"; then
      lose "$cycle" "no session host ever appeared"
      losses=$((losses + 1)); cycles_run=$((cycles_run + 1)); kill_gui; continue
    fi
    wait_for 40 scene_is_up
    sleep 2

    local before_panes
    before_panes="$(host_pane_pids)"
    kill_gui
    pkill -9 -f "serve --session $SESSION"
    sleep 1

    local lost=0
    local still
    still="$(host_pane_pids)"
    if [ -n "$still" ]; then
      lose "$cycle" "the host was killed and something still answered: [$still]"
      lost=1
    fi
    # Relaunch: a new host, the layout read from the file, terminals started
    # again — today's recovery, reached deliberately.
    launch_gui hosted
    if ! wait_for 30 test -S "$SOCKET"; then
      lose "$cycle" "no host came back after the old one was killed"
      lost=1
    else
      local after_panes
      wait_for 40 scene_is_up
      after_panes="$(host_pane_pids)"
      if [ -z "$after_panes" ]; then
        lose "$cycle" "the session did not come back at all"
        lost=1
      elif [ "$after_panes" = "$before_panes" ]; then
        lose "$cycle" "the same pids came back after a host kill, which cannot be true"
        lost=1
      fi
    fi
    kill_gui
    pkill -9 -f "serve --session $SESSION" 2>/dev/null
    kill_scene
    losses=$((losses + lost))
    cycles_run=$((cycles_run + 1))
    note "cycle $cycle: $([ "$lost" = 0 ] && echo "recovered to the floor" || echo "WORSE THAN THE FLOOR")"
    sleep 0.5
  done
}

run_pane_exit() {
  # A terminal ending is not a terminal being taken away, and for a while this
  # window could not tell the difference — it had no way to hear about either.
  # So: end one terminal and require the window to survive it, then end them all
  # and require the window to go, which it can only do by having noticed.
  for cycle in $(seq 1 "$CYCLES"); do
    launch_gui hosted
    if ! wait_for 30 test -S "$SOCKET"; then
      lose "$cycle" "no session host ever appeared"
      losses=$((losses + 1)); cycles_run=$((cycles_run + 1)); kill_gui; continue
    fi
    if ! wait_for 40 scene_is_up; then
      lose "$cycle" "the scene never came up; see $(diagnose "$cycle")"
      losses=$((losses + 1)); cycles_run=$((cycles_run + 1)); kill_gui; continue
    fi
    sleep 2

    local lost=0
    local panes idle_pid rest
    panes="$(host_pane_pids)"
    # The idle shell: the fourth pane in the staged layout, and the one whose
    # ending costs nothing to arrange — no editor to quit, no monitor to stop.
    idle_pid="$(echo "$panes" | tr ' ' '\n' | sed -n 4p | cut -d: -f2)"
    if [ -z "$idle_pid" ]; then
      lose "$cycle" "could not find the idle pane's process"
      losses=$((losses + 1)); cycles_run=$((cycles_run + 1)); kill_gui; continue
    fi

    # End ONE terminal by hanging up on its shell — which is what a closing
    # terminal has always done, and what an interactive shell actually honours.
    # (SIGTERM does not end one: a shell with job control ignores it, which this
    # harness learned by trying.) Nothing here touches the window or its stream,
    # so what the window learns, it learns from the host.
    kill -HUP "$idle_pid" 2>/dev/null
    if ! wait_for 15 bash -c "! kill -0 $idle_pid 2>/dev/null"; then
      lose "$cycle" "the idle shell would not end"
      lost=1
    fi
    sleep 3
    if [ -z "${GUI_PID:-}" ] || ! alive "$GUI_PID"; then
      lose "$cycle" "the window closed when a single terminal ended"
      lost=1
      losses=$((losses + lost)); cycles_run=$((cycles_run + 1)); continue
    fi

    # Now end the rest. A window that heard none of this would sit there
    # forever showing five dead terminals; a window that heard it closes, which
    # is what every terminal emulator does when its last shell exits.
    rest="$(echo "$panes" | tr ' ' '\n' | cut -d: -f2)"
    for pid in $rest; do
      kill -HUP "$pid" 2>/dev/null
    done
    kill_scene
    if wait_for 25 bash -c "! kill -0 ${GUI_PID:-0} 2>/dev/null"; then
      GUI_PID=""
    else
      lose "$cycle" "the window was still up after every one of its terminals had ended"
      lost=1
    fi

    kill_gui
    pkill -9 -f "serve --session $SESSION" 2>/dev/null
    losses=$((losses + lost))
    cycles_run=$((cycles_run + 1))
    note "cycle $cycle: $([ "$lost" = 0 ] && echo "the window noticed" || echo "AN ENDING WENT UNNOTICED")"
    sleep 1
  done
}

run_legacy_load() {
  # Eight panes in one tab, nested the way splitting produces them, with no
  # pane ids — a file written before session hosts existed and before the cap
  # came down. Every one of them must come up.
  for cycle in $(seq 1 "$CYCLES"); do
    python3 - "$STATE" "$RUN" <<'PY'
import sys

state, run = sys.argv[1], sys.argv[2]
# a right-leaning spine of eight leaves, which is what splitting one tab
# repeatedly leaves behind
leaves = [f'{{ Leaf = {{ cwd = "{run}", resume = "echo legacy-pane-{n}-ready" }} }}' for n in range(1, 9)]
node = leaves[-1]
for leaf in reversed(leaves[:-1]):
    node = f'{{ Split = {{ dir = "Row", ratio = 0.5, a = {leaf}, b = {node} }} }}'
with open(state, "w") as f:
    f.write("active = 0\npanes = 8\nwin = [40.0, 60.0, 900.0, 560.0]\n\n")
    f.write("[[tabs]]\nname = \"eight\"\n")
    f.write(f"node = {node}\n")
PY
    launch_gui hosted
    if ! wait_for 30 test -S "$SOCKET"; then
      lose "$cycle" "no session host ever appeared"
      losses=$((losses + 1)); cycles_run=$((cycles_run + 1)); kill_gui; continue
    fi
    # Give every pane time to be started and typed into.
    sleep 8
    local held
    held="$(host_pane_pids | tr ' ' '\n' | grep -c ':')"
    if [ "$held" != "8" ]; then
      lose "$cycle" "a legacy eight-pane tab opened with $held terminals; see $(diagnose "$cycle")"
      losses=$((losses + 1))
    fi
    if [ -z "${GUI_PID:-}" ] || ! alive "$GUI_PID"; then
      lose "$cycle" "the window did not survive opening an over-cap layout"
    fi
    note "cycle $cycle: the legacy tab opened with $held terminals"
    kill_gui
    pkill -9 -f "serve --session $SESSION" 2>/dev/null
    cycles_run=$((cycles_run + 1))
    sleep 1
  done
}

case "$LEG" in
  gui-kill) run_gui_kill ;;
  floor-control) run_floor_control ;;
  host-kill) run_host_kill ;;
  pane-exit) run_pane_exit ;;
  legacy-load) run_legacy_load ;;
esac

# --------------------------------------------------------------- the verdict --
expected=0
case "$LEG" in
  gui-kill) expected=0 ;;
  floor-control) expected="$cycles_run" ;;
  host-kill) expected=0 ;;
  pane-exit) expected=0 ;;
  legacy-load) expected=0 ;;
esac

printf '{"leg":"%s","cycles":%s,"losses":%s,"expected_losses":%s,"editor":"%s","monitor":"%s","failures":[' \
  "$LEG" "$cycles_run" "$losses" "$expected" "$EDITOR_BIN" "$MONITOR_BIN"
for i in "${!failures[@]}"; do
  [ "$i" -gt 0 ] && printf ','
  printf '"%s"' "$(echo "${failures[$i]}" | sed 's/"/\\"/g')"
done
printf ']}\n'

if [ "$losses" = "$expected" ] && [ "$cycles_run" = "$CYCLES" ] && [ "${#failures[@]}" = 0 ]; then
  exit 0
fi
exit 1
