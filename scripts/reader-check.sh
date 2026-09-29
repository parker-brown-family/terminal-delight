#!/usr/bin/env bash
# Photograph the FOCUS reader on a narrow pane, in a window nobody can see.
#
#   scripts/reader-check.sh <terminal-delight binary> <out dir> [label] [keys] [document]
#
# `keys` is a space-separated list of chords sent to the hidden window before the
# photograph: an xkb keysym, with modifiers before a plus — "Page_Up Page_Up" pages
# the reader back, "ALT+k" flips the pane behind it to its workbench face.
#
# `document` puts a file on the narrow pane's Document face first, as a pane
# opened with Ctrl+Alt+click on it would be, so the reader opens on a Markdown
# file, a page, a PDF, a picture or a video instead of a shell. SETTLE (seconds,
# default 4) is how long the window gets before the keys: a page or a PDF needs
# longer to be drawn.
#
# AGENT=<transcript.jsonl> puts an agent on the narrow pane instead, so the
# reader reads a conversation: a copy of the transcript is laid where Claude Code
# keeps it, under a scratch HOME that is the window's own (never yours), and the
# pane runs a stand-in named `claude` whose command line says `--resume <id>` —
# which is all Terminal Delight asks of an agent before it binds a transcript —
# and which draws the bottom of Claude Code's screen: a reply's last lines, the
# status line, the input box and its hint. It reads and writes nothing.
# app/tests/fixtures/reader/conversation.jsonl is the conversation the reader's
# first brief drew. LATER=<records.jsonl> is appended to the transcript after the
# photograph, as an agent writes a turn, and a second photograph,
# <label>-later.png, is taken LATER_WAIT seconds (default 2) after that.
# UNBOUND=1 runs the same agent with no transcript laid down, so there is
# nothing to bind it to and the reader must say so and read the screen.
#
# Stages the layout the reader is opened for: a 1,576 x 950 window (Parker's) with
# a tab of three panes whose first is a quarter of the window wide, each running a
# plain sh that prints real output — git log lines longer than the pane, ls laid out
# for the pane's own width, a page of Markdown — with the reader opened on the first
# pane at the first render (TD_FOCUS_DEMO). Run it once with the installed build
# and once with a new one, and the two PNGs are the before and after.
#
# THE WINDOW IS HIDDEN: it is launched onto a special workspace with its own
# TD_SESSION, photographed there by its stable id, and killed, with its session
# files swept. Nothing moves on any monitor, and no live session is touched.
set -u
TD=${1:?usage: reader-check.sh <td binary> <out dir> [label]}
OUT=${2:?usage: reader-check.sh <td binary> <out dir> [label]}
LABEL=${3:-reader}
KEYS=${4:-}
DOC=${5:-}
SETTLE=${SETTLE:-4}
AGENT=${AGENT:-}
LATER=${LATER:-}
LATER_WAIT=${LATER_WAIT:-2}
here=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$here/.." && pwd)
# shellcheck source=lib/hidden-window.sh
. "$here/lib/hidden-window.sh"
for tool in hyprctl jq grim; do
  command -v "$tool" >/dev/null || { echo "reader-check needs $tool"; exit 2; }
done
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)
SESSION="readercheck$$"
WS="special:tdreader$$"
trap hidden_cleanup EXIT

# What every pane prints, then keeps quiet with cat so the pane stays alive.
{
  echo "cd $REPO"
  echo "echo '\$ git log --oneline -5 5ba60ed'"
  echo "git --no-pager log --oneline -5 5ba60ed"
  echo "echo '\$ ls app/src/docview'"
  echo "ls -C app/src/docview"
  echo "echo '\$ head -9 docs/plans/alt-r-reader/03-plan.md'"
  echo "head -9 docs/plans/alt-r-reader/03-plan.md"
  echo "exec cat"
} > "$OUT/pane.sh"

FIRST_CWD=/tmp
FIRST_RUN="sh $OUT/pane.sh"
WINDOW_HOME=$HOME
if [ -n "$AGENT" ]; then
  [ -f "$AGENT" ] || { echo "no transcript at $AGENT"; exit 2; }
  # The id the stand-in resumes is the one the records carry, and the copy is
  # named for it: Claude Code's file stem IS the session id.
  ID=$(jq -r 'select(.sessionId) | .sessionId' "$AGENT" | head -1)
  [ -n "$ID" ] || ID=5f0c2a9e-7d1b-4c3e-9a28-1e6b0d4f7c31
  WINDOW_HOME="$OUT/home"
  FIRST_CWD="$OUT/work"
  mkdir -p "$FIRST_CWD" "$OUT/bin"
  # Claude Code's directory for a cwd: every character that is not a letter or a
  # digit becomes a dash.
  TALK="$WINDOW_HOME/.claude/projects/$(printf %s "$FIRST_CWD" | sed 's/[^A-Za-z0-9]/-/g')/$ID.jsonl"
  mkdir -p "$(dirname "$TALK")"
  [ "${UNBOUND:-}" = 1 ] || cp "$AGENT" "$TALK"
  cat > "$OUT/bin/claude" <<'STAND_IN'
#!/bin/bash
# reader-check's stand-in for Claude Code. Its name and its --resume line are
# what bind the pane to a transcript; the rest is the bottom of Claude Code's
# screen, redrawn when the pane is resized — a pane is born wide and narrowed.
draw() {
  cols=$(stty size 2>/dev/null | cut -d' ' -f2)
  case "$cols" in '' | 0 | *[!0-9]*) cols=48 ;; esac
  rule=$(printf '%*s' "$cols" '' | sed 's/ /─/g')
  printf '\033[?25l\033[H\033[2J'
  printf '%s\n' "A floating square already hands its own view to a split without opening the file again, and the reader can borrow the view the same way." |
    fold -s -w $((cols - 2)) | sed 's/^/  /'
  printf '\n✻ Writing… (4m 12s · esc to interrupt)\n\n%s\n> \n%s\n  ⏵⏵ auto mode on (shift+tab to cycle)' "$rule" "$rule"
}
# Claude Code names its pane after the conversation's topic.
printf '\033]0;✳ Alt+R reader\007'
trap draw WINCH
draw
while :; do sleep 1; done
STAND_IN
  chmod +x "$OUT/bin/claude"
  echo "exec $OUT/bin/claude --resume $ID" > "$OUT/agent.sh"
  FIRST_RUN="sh $OUT/agent.sh"
fi

{
  printf '%s\n' \
    'active = 0' 'lang = "en"' 'left_bar = false' '' \
    '[[tabs]]' 'name = "reader"' '' \
    '[tabs.node.Split]' 'dir = "Row"' 'ratio = 0.25' '' \
    '[tabs.node.Split.a.Leaf]' "cwd = \"$FIRST_CWD\"" "resume = \"$FIRST_RUN\"" ''
  # The narrow pane shows the document on its Document face, restored as a
  # saved layout restores one.
  [ -n "$DOC" ] && printf '%s\n' '[tabs.node.Split.a.Leaf.document]' "path = \"$DOC\"" ''
  printf '%s\n' \
    '[tabs.node.Split.b.Split]' 'dir = "Row"' 'ratio = 0.34' '' \
    '[tabs.node.Split.b.Split.a.Leaf]' 'cwd = "/tmp"' "resume = \"sh $OUT/pane.sh\"" '' \
    '[tabs.node.Split.b.Split.b.Leaf]' 'cwd = "/tmp"' "resume = \"sh $OUT/pane.sh\""
} > "$OUT/state.toml"

LOG="$OUT/$LABEL-window.log"
{
  echo "export TD_SESSION=$SESSION XDG_CACHE_HOME=$OUT/cache XDG_STATE_HOME=$OUT/state"
  echo "export SHELL=/bin/sh TD_DEMO_STATE=$OUT/state.toml TD_FOCUS_DEMO=1"
  # With an agent, the window's HOME is the scratch one, so the transcripts it
  # can find are the one laid there; its config is still yours, read-only.
  echo "export HOME=$WINDOW_HOME XDG_CONFIG_HOME=${XDG_CONFIG_HOME:-$HOME/.config}"
  echo "exec $TD > $LOG 2>&1"
} > "$OUT/launch.sh"

# Not hidden_launch: that finds its window by the title a session gives it, and a
# window restored from a demo state may wear the default title. The special
# workspace this run made is its own, so the window is found there instead.
said=$(hyprctl dispatch "hl.dsp.exec_cmd(\"sh $OUT/launch.sh\", { workspace = \"$WS silent\", render_unfocused = true, no_initial_focus = true, float = true, size = \"1576 950\", no_anim = true })" 2>&1)
case "$said" in ok|"") ;; *) echo "hyprctl refused the launch: $said"; exit 1 ;; esac
WIN=""
for _ in $(seq 1 60); do
  WIN=$(hyprctl clients -j 2>/dev/null | jq -r --arg w "$WS" '.[] | select(.workspace.name==$w) | .pid' | head -1)
  [ -n "$WIN" ] && break
  sleep 0.5
done
[ -n "$WIN" ] || { echo "no window appeared on $WS — see $LOG"; exit 1; }
HIDDEN_WINDOWS="$HIDDEN_WINDOWS $WIN"
HIDDEN_SESSIONS="$HIDDEN_SESSIONS $SESSION"
STABLE=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .stableId' | head -1)
SIZE=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | "\(.size[0])x\(.size[1])"' | head -1)
# The panes print within a second; the reader's ease-in settles in a quarter.
sleep "$SETTLE"
# A window that started a real agent is a process on somebody's desk, not a
# fixture: the run stops, and the cleanup takes the window and its host with it.
if grep -q "launching — claude" "$LOG"; then
  echo "the window launched a real claude — stopped; see $LOG"
  exit 5
fi
for chord in $KEYS; do
  mods=""; key=$chord
  case "$chord" in *+*) mods=${chord%+*}; key=${chord##*+} ;; esac
  hyprctl dispatch "hl.dsp.send_shortcut({ mods = '$mods', key = '$key', window = 'pid:$WIN' })" >/dev/null
  sleep 0.6
done
if timeout 10 grim -T "$STABLE" "$OUT/$LABEL.png"; then
  echo "photographed $OUT/$LABEL.png (window $SIZE)"
else
  echo "grim could not photograph window $WIN"
  exit 1
fi
grep -q "auto-opening FOCUS" "$LOG" && echo "the reader opened on the first pane"
if [ -n "$AGENT" ]; then
  if grep -q "the reader reads the conversation" "$LOG"; then
    echo "the reader bound the stand-in's transcript"
  else
    echo "the reader bound no transcript"
  fi
  if [ -n "$LATER" ] && [ "${UNBOUND:-}" != 1 ]; then
    cat "$LATER" >> "$TALK"
    sleep "$LATER_WAIT"
    if timeout 10 grim -T "$STABLE" "$OUT/$LABEL-later.png"; then
      echo "photographed $OUT/$LABEL-later.png, $LATER_WAIT s after the turn was written"
    else
      echo "grim could not photograph the later turn"
      exit 1
    fi
  fi
fi
