#!/usr/bin/env bash
# How much an idle Terminal Delight holds, beside Foot, and where the window's
# own share goes.
#
#   scripts/idle-memory.sh [--bin PATH] [--panes N] [--settle SECS] [--no-foot]
#                          [--empty-gpui]
#
# Splinterm publishes its idle footprint beside Foot's; Terminal Delight had no
# number at all until 2026-10-15, when this was first run by hand (258 MiB RSS
# for one pane, 182 MiB more on the GPU, Foot 40 MiB on the same machine). This
# is that measurement, repeatable: one hidden window on a session of its own,
# idle for SETTLE seconds with one pane, then with N, then Foot launched the
# same way as the ruler. Then the window's proportional set size, grouped by
# what each mapping is, so "the window is heavy" becomes "these parts are".
#
# --empty-gpui adds the second ruler #953 asks for: one gpui window holding one
# line of text (app/examples/empty_window.rs, built with `cargo build --release
# --example empty_window`), launched hidden the same way on a workspace of its
# own so it is the size the Terminal Delight window was. What it holds, every
# gpui application here pays; what Terminal Delight holds beyond it is its own.
# The breakdown by part is docs/plans/idle-memory-account.md.
#
# THE WINDOWS ARE HIDDEN (scripts/lib/hidden-window.sh): a special workspace,
# render_unfocused, no_initial_focus, its own TD_SESSION. Nothing appears on
# screen, nothing joins a running session, and everything is torn down on exit.
#
# Unknown is not zero. A number this cannot read — no nvidia-smi, no Foot, a
# process that went away — prints as `unknown`, never as 0.
#
# Exit 0 with the readings on stdout; the last line is one `key=value` record
# for a program. Exit 1 if no window came up, 4 if one was not hidden.
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
TD="$ROOT/app/target/release/terminal-delight"
[ -x "$TD" ] || TD="$HOME/.local/bin/terminal-delight"
PANES=4
SETTLE=8
FOOT=1
EMPTY=0
while [ $# -gt 0 ]; do
  case "$1" in
    --bin) TD=$2; shift 2 ;;
    --panes) PANES=$2; shift 2 ;;
    --settle) SETTLE=$2; shift 2 ;;
    --no-foot) FOOT=0; shift ;;
    --empty-gpui) EMPTY=1; shift ;;
    -h|--help) sed -n '2,31p' "$0"; exit 0 ;;
    *) echo "unknown argument $1"; exit 2 ;;
  esac
done
[ -x "$TD" ] || { echo "no binary at $TD"; exit 2; }
# Absolute, because Hyprland starts the hidden window from its own directory,
# not this one: a relative --bin passed the check above and then launched
# nothing, which read as "no window appeared" with no hint why.
TD=$(readlink -f "$TD")
OUT=$(mktemp -d "${TMPDIR:-/tmp}/td-idle-memory.XXXXXX")

. "$ROOT/scripts/lib/hidden-window.sh"
FOOT_PID=""
cleanup() { [ -n "$FOOT_PID" ] && kill "$FOOT_PID" 2>/dev/null; hidden_cleanup; rm -rf "$OUT"; }
trap cleanup EXIT

# Readings in MiB, or "unknown".
rss() { awk '/^VmRSS:/ {print int($2/1024); f=1} END {if (!f) print "unknown"}' "/proc/$1/status" 2>/dev/null || echo unknown; }
pss() { awk '/^Pss:/ {print int($2/1024); f=1} END {if (!f) print "unknown"}' "/proc/$1/smaps_rollup" 2>/dev/null || echo unknown; }
gpu() {
  command -v nvidia-smi >/dev/null || { echo unknown; return; }
  nvidia-smi --query-compute-apps=pid,used_memory --format=csv,noheader,nounits 2>/dev/null \
    | awk -F', ' -v p="$1" '$1==p {print $2; f=1} END {if (!f) print "unknown"}'
}
# Every pid under a root, the root included.
tree() {
  local out=$1 grew=1 s pid ppid
  while [ $grew -eq 1 ]; do
    grew=0
    for s in /proc/[0-9]*/stat; do
      read -r pid _ _ ppid _ 2>/dev/null < "$s" || continue
      case " $out " in *" $pid "*) continue ;; esac
      case " $out " in *" $ppid "*) out="$out $pid"; grew=1 ;; esac
    done
  done
  echo "$out"
}
# Sum a reading over pids; any unknown makes the sum unknown.
sum() {
  local f=$1 t=0 v; shift
  for p in "$@"; do v=$("$f" "$p"); [ "$v" = unknown ] && { echo unknown; return; }; t=$((t + v)); done
  echo $t
}

SESSION="idlemem$$"
HOST() { pgrep -f "serve --session $SESSION( |\$)" | head -1; }
reading() { # reading <label> -> prints a line, sets R_RSS R_PSS R_GPU
  local host shells
  host=$(HOST)
  shells=$(tree "$host" | tr ' ' '\n' | grep -vx "$host" | tr '\n' ' ')
  R_RSS=$(sum rss "$WIN" "$host" $shells)
  R_PSS=$(sum pss "$WIN" "$host" $shells)
  R_GPU=$(gpu "$WIN")
  printf '%-22s rss %6s MiB   pss %6s MiB   gpu %6s MiB   (window %s / host %s / %s shell%s)\n' \
    "$1" "$R_RSS" "$R_PSS" "$R_GPU" "$(rss "$WIN")" "$(rss "$host")" "$(echo $shells | wc -w)" \
    "$([ "$(echo $shells | wc -w)" = 1 ] || echo s)"
}

echo "terminal-delight: $(readlink -f "$TD")"
cat > "$OUT/launch.sh" <<EOF
export TD_SESSION=$SESSION
exec $TD > $OUT/td.log 2>&1
EOF
hidden_launch "$SESSION" "special:idlemem$$" "$OUT/td.log" "sh $OUT/launch.sh"
sleep "$SETTLE"
reading "1 pane"; ONE_RSS=$R_RSS; ONE_PSS=$R_PSS; GPU=$R_GPU
for _ in $(seq 2 "$PANES"); do ctl adopt --cwd /tmp >/dev/null; sleep 0.5; done
sleep "$SETTLE"
reading "$PANES panes"; N_RSS=$R_RSS; N_PSS=$R_PSS

# Where the window's own proportional share goes, by what each mapping is.
echo
echo "the window's own pss, by part:"
awk '
  /^[0-9a-f]+-[0-9a-f]+ / { name = (NF >= 6) ? $6 : ""; for (i = 7; i <= NF; i++) name = name " " $i
    if (name == "")                        part = "anonymous (heap-like, allocator arenas)"
    else if (name == "[heap]")             part = "[heap]"
    else if (name ~ /^\[stack/)            part = "stacks"
    else if (name ~ /^\/dev\/(nvidia|dri)/) part = "GPU driver mappings"
    else if (name ~ /^\/memfd:/)           part = "memfd (shared buffers)"
    else if (name ~ /\.so(\.|$)/)          part = "shared libraries"
    else if (name ~ /fonts?\//)            part = "fonts"
    else if (name ~ /terminal-delight|td-[0-9a-f]+/) part = "the binary itself"
    else                                   part = "other files"
    next }
  /^Pss:/ { kb[part] += $2 }
  END { for (p in kb) printf "  %8.1f MiB  %s\n", kb[p] / 1024, p }
' "/proc/$WIN/smaps" 2>/dev/null | sort -rn || echo "  unknown: /proc/$WIN/smaps unreadable"
if [ -r "/proc/$WIN/maps" ]; then
  FONT_FILES=$(awk '$6 ~ /\.(ttf|otf|ttc|otc)$/ {print $6}' "/proc/$WIN/maps" | sort -u | wc -l)
else
  FONT_FILES=unknown
fi
echo "  font files mapped: $FONT_FILES"

EMPTY_RSS=unknown; EMPTY_PSS=unknown; EMPTY_GPU=unknown
if [ "$EMPTY" = 1 ]; then
  RULER="$ROOT/app/target/release/examples/empty_window"
  if [ -x "$RULER" ]; then
    printf 'export TD_SESSION=%s\nexec %s > %s 2>&1\n' "${SESSION}r" "$RULER" "$OUT/ruler.log" > "$OUT/ruler.sh"
    # hidden_launch pings the window through $TD; the ruler answers a ping and
    # exits rather than open a window of its own outside the hidden workspace.
    TD_WINDOW=$WIN; TD_BIN=$TD; TD=$RULER
    hidden_launch "${SESSION}r" "special:idlemem$$r" "$OUT/ruler.log" "sh $OUT/ruler.sh"
    TD=$TD_BIN
    sleep "$SETTLE"
    EMPTY_RSS=$(rss "$WIN"); EMPTY_PSS=$(pss "$WIN"); EMPTY_GPU=$(gpu "$WIN")
    WIN=$TD_WINDOW
  else
    echo
    echo "no ruler at $RULER: build it with (cd app && cargo build --release --example empty_window)"
  fi
fi

FOOT_RSS=unknown; FOOT_PSS=unknown
if [ "$FOOT" = 1 ] && command -v foot >/dev/null; then
  hyprctl dispatch "hl.dsp.exec_cmd(\"foot --app-id=idlemem-foot$$\", { workspace = \"special:idlemem$$ silent\", render_unfocused = true, no_initial_focus = true })" >/dev/null 2>&1
  for _ in $(seq 1 30); do
    FOOT_PID=$(hyprctl clients -j | jq -r --arg c "idlemem-foot$$" '.[] | select(.class==$c) | .pid' | head -1)
    [ -n "$FOOT_PID" ] && break
    sleep 0.5
  done
  if [ -n "$FOOT_PID" ]; then
    landed=$(hyprctl clients -j | jq -r --argjson p "$FOOT_PID" '.[] | select(.pid==$p) | .workspace.name' | head -1)
    if [ "$landed" != "special:idlemem$$" ]; then
      echo "foot landed on $landed, not the hidden workspace — killed"
      kill "$FOOT_PID"; FOOT_PID=""; exit 4
    fi
    sleep "$SETTLE"
    FT=$(tree "$FOOT_PID")
    FOOT_RSS=$(sum rss $FT); FOOT_PSS=$(sum pss $FT)
  fi
fi
echo
printf '%-22s rss %6s MiB   pss %6s MiB   (the ruler: Splinterm publishes against Foot too)\n' "foot, 1 window" "$FOOT_RSS" "$FOOT_PSS"
if [ "$EMPTY" = 1 ]; then
  printf '%-22s rss %6s MiB   pss %6s MiB   gpu %6s MiB   (what any gpui window here pays)\n' \
    "empty gpui, 1 line" "$EMPTY_RSS" "$EMPTY_PSS" "$EMPTY_GPU"
fi
echo
echo "idle-memory: td_1_rss=$ONE_RSS td_1_pss=$ONE_PSS td_${PANES}_rss=$N_RSS td_${PANES}_pss=$N_PSS td_gpu=$GPU td_font_files=$FONT_FILES foot_rss=$FOOT_RSS foot_pss=$FOOT_PSS empty_gpui_rss=$EMPTY_RSS empty_gpui_pss=$EMPTY_PSS empty_gpui_gpu=$EMPTY_GPU"
