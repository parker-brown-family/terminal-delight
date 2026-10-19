#!/usr/bin/env bash
# Terminal Delight answers xdg-terminal-exec the way foot does — proved through
# xdg-terminal-exec itself, against a hidden window, touching nothing of yours.
#
#   scripts/contract-check.sh [--bin PATH] [--shot DIR]
#
# Everything runs under scratch XDG directories: a config whose
# xdg-terminals.list names terminal-delight.desktop, and a data directory
# holding that entry with Exec pointing at the binary under test. Your own
# ~/.config and ~/.local/share/applications are never read for the default and
# never written. Windows go to a special workspace with render_unfocused and
# no_initial_focus, so nothing appears on screen.
#
# Phases:
#   1. print     — xdg-terminal-exec --print-id and --print-cmd, no window: the
#                  entry is chosen, and the command it builds is the one the
#                  contract parser reads (app-id, title, working directory, -e).
#   2. tracer    — the slice-1 tracer bullet, launched THROUGH xdg-terminal-exec:
#                  a window whose Hyprland class is the asked-for app-id, whose
#                  program writes a marker in the asked-for directory, and which
#                  closes itself when the program ends. No session file and no
#                  session host appear while it runs.
#   3. mark      — the same launch as Omarchy's Local AI plugin makes it
#                  (app-id org.omarchy.local-ai): the window is photographed
#                  with grim -T so the LOCAL MODEL chip can be seen, if --shot.
#   4. refusal   — a fifth key (--hold) is refused with exit 2 and no window.
#
# Exit 0: every phase held. 1: one did not. 2: bad arguments. 4: a window was
# not hidden; the run stopped there.
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
TD="$ROOT/app/target/debug/terminal-delight"
SHOT=""
while [ $# -gt 0 ]; do
  case "$1" in
    --bin) TD=$2; shift 2 ;;
    --shot) SHOT=$2; shift 2 ;;
    -h|--help) sed -n '2,28p' "$0"; exit 0 ;;
    *) echo "unknown argument $1"; exit 2 ;;
  esac
done
[ -x "$TD" ] || { echo "no binary at $TD — cargo build first"; exit 2; }
TD=$(readlink -f "$TD")
command -v xdg-terminal-exec >/dev/null || { echo "xdg-terminal-exec is not installed"; exit 2; }
OUT=$(mktemp -d "${TMPDIR:-/tmp}/td-contract.XXXXXX")
WS="special:tdcontract$$"
WINS=""
cleanup() {
  for w in $WINS; do kill "$w" 2>/dev/null; done
  # A failed run keeps its logs: the window's own stderr is the evidence.
  if [ "${fail:-0}" = 0 ]; then rm -rf "$OUT"; else echo "logs kept in $OUT"; fi
}
trap cleanup EXIT
fail=0

# The scratch desktop: the entry, with Exec at the binary under test, and a
# list that names it first.
mkdir -p "$OUT/config" "$OUT/data/applications" "$OUT/work"
sed "s|^Exec=.*|Exec=$TD|" "$ROOT/packaging/terminal-delight.desktop" \
  > "$OUT/data/applications/terminal-delight.desktop"
printf 'terminal-delight.desktop\n' > "$OUT/config/xdg-terminals.list"
XENV="XDG_CONFIG_HOME=$OUT/config XDG_DATA_HOME=$OUT/data"

# Session files in both places a window could write one: yours, and the
# scratch config the launched window inherits. A quick window writes neither.
sessions() {
  local f
  for f in "$HOME/.config/terminal-delight/sessions"/* "$OUT/config/terminal-delight/sessions"/*; do
    [ -e "$f" ] && printf '%s ' "$f"
  done
}
hosts() { pgrep -fc 'terminal-delight.* serve --session' || true; }

# ── 1. print ─────────────────────────────────────────────────────────────────
echo "== 1. print: what xdg-terminal-exec would run"
id=$(env $XENV xdg-terminal-exec --print-id 2>&1)
cmd=$(env $XENV xdg-terminal-exec --print-cmd=' ' --app-id=org.td.tracer --title=Tracer \
  --dir="$OUT/work" -- sh -c 'echo hi' 2>&1)
want="$TD --app-id=org.td.tracer --title=Tracer --working-directory=$OUT/work -e sh -c echo hi"
if [ "${id%%:*}" = "terminal-delight.desktop" ] && [ "$cmd" = "$want" ]; then
  echo "PASS 1: chose $id; builds: $cmd"
else
  echo "FAIL 1: chose '$id'; built '$cmd'"; echo "   wanted '$want'"; fail=1
fi

# launch <name> <xdg-terminal-exec arguments…> — through xdg-terminal-exec,
# onto the hidden workspace. Sets WIN to the window's pid, found by class.
launch() {
  local name=$1 class=$2; shift 2
  local script="$OUT/launch-$name.sh"
  {
    echo "export XDG_CONFIG_HOME=$OUT/config XDG_DATA_HOME=$OUT/data"
    printf 'exec xdg-terminal-exec'
    printf ' %q' "$@"
    echo " > $OUT/$name.log 2>&1"
  } > "$script"
  local said
  said=$(hyprctl dispatch "hl.dsp.exec_cmd(\"sh $script\", { workspace = \"$WS silent\", render_unfocused = true, no_initial_focus = true })" 2>&1)
  case "$said" in ok|"") ;; *) echo "hyprctl refused the launch: $said"; exit 1 ;; esac
  WIN=""
  for _ in $(seq 1 60); do
    WIN=$(hyprctl clients -j | jq -r --arg c "$class" '.[] | select(.class==$c) | .pid' | head -1)
    [ -n "$WIN" ] && break
    sleep 0.5
  done
  [ -n "$WIN" ] || { echo "no window with class $class appeared — see $OUT/$name.log"; cat "$OUT/$name.log"; return 1; }
  WINS="$WINS $WIN"
  local landed
  landed=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .workspace.name' | head -1)
  if [ "$landed" != "$WS" ]; then
    echo "the window landed on '$landed', not $WS — killed"; kill "$WIN"; exit 4
  fi
}

# ── 2. tracer ────────────────────────────────────────────────────────────────
echo "== 2. tracer: a program run through xdg-terminal-exec, in a window that closes with it"
before_sessions=$(sessions); before_hosts=$(hosts)
if launch tracer "org.td.tracer$$" --app-id="org.td.tracer$$" --title="Tracer $$" --dir="$OUT/work" \
  -- sh -c 'pwd > marker; sleep 3'; then
  title=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .title' | head -1)
  for _ in $(seq 1 40); do [ -s "$OUT/work/marker" ] && break; sleep 0.25; done
  wrote=$(cat "$OUT/work/marker" 2>/dev/null)
  mid_sessions=$(sessions); mid_hosts=$(hosts)
  gone=0
  for _ in $(seq 1 60); do kill -0 "$WIN" 2>/dev/null || { gone=1; break; }; sleep 0.25; done
  if [ "$title" = "Tracer $$" ] && [ "$wrote" = "$OUT/work" ] && [ "$gone" = 1 ] \
     && [ "$mid_sessions" = "$before_sessions" ] && [ "$mid_hosts" = "$before_hosts" ]; then
    echo "PASS 2: class org.td.tracer$$, title '$title', program ran in $wrote, window gone when it ended; no session file or host appeared"
  else
    echo "FAIL 2: title '$title', marker '$wrote' (want $OUT/work), gone=$gone"
    echo "   sessions before '$before_sessions' during '$mid_sessions'; hosts $before_hosts → $mid_hosts"
    fail=1
  fi
else
  fail=1
fi

# ── 3. mark ──────────────────────────────────────────────────────────────────
echo "== 3. mark: an agent opened the way Omarchy's Local AI opens one"
# The program says when it starts and when it ends, so the check waits on the
# program rather than on a clock.
cat > "$OUT/agent.sh" <<'EOF'
echo started > agent-started
echo "a local model would answer here"
sleep 8
echo ended > agent-ended
EOF
if launch mark "org.omarchy.local-ai" --app-id=org.omarchy.local-ai --title="Local AI" --dir="$OUT/work" \
  -- sh "$OUT/agent.sh"; then
  for _ in $(seq 1 40); do [ -e "$OUT/work/agent-started" ] && break; sleep 0.25; done
  sleep 1.5
  # Alive while its program runs — read before any photograph is attempted,
  # because a capture that hangs would otherwise outlast the program and make
  # a window that closed on time look like one that died early.
  alive_mid=$(kill -0 "$WIN" 2>/dev/null && echo yes || echo no)
  shot=""
  if [ -n "$SHOT" ]; then
    mkdir -p "$SHOT"
    sid=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .stableId' | head -1)
    if timeout 4 grim -T "$sid" "$SHOT/local-model-chip.png" 2>/dev/null; then
      shot=" and photographed it ($SHOT/local-model-chip.png)"
    else
      asleep=$(hyprctl monitors -j | jq -r '[.[] | select(.dpmsStatus == false)] | length')
      shot=" (no photograph: grim -T did not answer in 4 s; $asleep output(s) asleep)"
      pkill -x grim 2>/dev/null
    fi
  fi
  for _ in $(seq 1 60); do [ -e "$OUT/work/agent-ended" ] && break; sleep 0.25; done
  gone=0
  for _ in $(seq 1 40); do kill -0 "$WIN" 2>/dev/null || { gone=1; break; }; sleep 0.25; done
  if [ -e "$OUT/work/agent-started" ] && [ "$alive_mid" = yes ] && [ -e "$OUT/work/agent-ended" ] && [ "$gone" = 1 ]; then
    echo "PASS 3: a window with class org.omarchy.local-ai ran its program to the end$shot, then closed"
  else
    echo "FAIL 3: started=$([ -e "$OUT/work/agent-started" ] && echo yes || echo no) alive mid-run=$alive_mid" \
      "ended=$([ -e "$OUT/work/agent-ended" ] && echo yes || echo no) closed=$gone$shot"
    fail=1
  fi
else
  fail=1
fi

# ── 4. refusal ───────────────────────────────────────────────────────────────
echo "== 4. refusal: a fifth key opens nothing"
"$TD" --app-id=org.td.x --hold -e true > "$OUT/refusal.out" 2>&1
code=$?
if [ "$code" = 2 ] && grep -q '`--hold`' "$OUT/refusal.out"; then
  echo "PASS 4: exit 2, and it says why: $(head -1 "$OUT/refusal.out")"
else
  echo "FAIL 4: exit $code: $(head -3 "$OUT/refusal.out")"; fail=1
fi

[ "$fail" = 0 ] && echo "ALL PASS" || echo "SOMETHING FAILED"
exit "$fail"
