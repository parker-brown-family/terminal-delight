#!/usr/bin/env bash
# Use cases for the left bar and the strip beneath it, end to end, against a
# REAL window of the installed (or given) build.
#
#   ./scripts/td-left-bar-usecases.sh                    # the installed binary
#   BIN=app/target/release/terminal-delight ./scripts/…  # a build you just made
#
# Why this exists as a script rather than as a cargo test: the tree's rules are
# unit-tested in `app/src/tree.rs` and need no window, but "does a group made
# through the control socket reach the file a relaunch reads" spans the window,
# the queue, the save path and the session file — and that can only be answered
# by running one. It needs a Wayland display, so it is not a CI gate.
#
# TWO RULES, both learned the hard way on 2026-09-10:
#
#   1. EVERY ctl call carries `--pid`. Without it, `ctl` is a broadcast to every
#      running window — the first version of this pass adopted three panes into
#      the operator's live session.
#   2. The window runs against a throwaway XDG_CONFIG_HOME with TD_NO_SESSIOND=1,
#      so it claims no real session and never speaks to a live host.
set -u

BIN="${BIN:-$HOME/.local/bin/terminal-delight}"
WORK="${TMPDIR:-/tmp}/td-usecases-$$"
CFG="$WORK/config"
STATE="$CFG/terminal-delight/sessions"
KEY="$STATE/1.toml"
PASS=0
FAIL=0

ok()   { PASS=$((PASS + 1)); printf '  \033[32mPASS\033[0m  %s\n' "$*"; }
bad()  { FAIL=$((FAIL + 1)); printf '  \033[31mFAIL\033[0m  %s\n' "$*"; }
say()  { printf '\n\033[1m%s\033[0m\n' "$*"; }
ctl()  { XDG_CONFIG_HOME="$CFG" "$BIN" ctl "$@" --pid "$APP" 2>&1; }
holds() { grep -q "$1" "$KEY" 2>/dev/null; }
count() { grep -c "$1" "$KEY" 2>/dev/null || echo 0; }

cleanup() {
    [ -n "${APP:-}" ] && kill "$APP" 2>/dev/null
    [ -n "${APP2:-}" ] && kill "$APP2" 2>/dev/null
    sleep 1
    rm -rf "$WORK"
}
trap cleanup EXIT

[ -x "$BIN" ] || { echo "no binary at $BIN"; exit 1; }
mkdir -p "$STATE"

say "A window of $(basename "$(readlink -f "$BIN")") opens"
XDG_CONFIG_HOME="$CFG" TD_NO_SESSIOND=1 nohup "$BIN" >"$WORK/window.log" 2>&1 &
APP=$!
sleep 9
if kill -0 "$APP" 2>/dev/null; then ok "it is up (pid $APP)"; else bad "it died on launch"; cat "$WORK/window.log"; exit 1; fi
if grep -q "not installed" "$WORK/window.log"; then
    bad "it fell back to another font: $(grep 'not installed' "$WORK/window.log")"
else
    ok "it found the desktop's font — no fallback warning"
fi
[ -f "$KEY" ] && ok "it claimed a session and wrote its file" || bad "no session file at $KEY"

say "A task takes a second pane"
before=$(count '\[tabs.node')
ctl adopt --cwd "$PWD" >/dev/null
sleep 3
after=$(count '\[tabs.node')
[ "$after" -gt "$before" ] && ok "the task grew from $before panes to $after" || bad "no pane appeared (still $after)"

say "A task is named"
ctl tabs '[{"op":"name","tab":0,"name":"host debts"}]' >/dev/null
sleep 3
holds 'name = "host debts"' && ok "the name reached the session file" || bad "the name did not persist"

say "A task joins an initiative"
ctl tabs '[{"op":"group","tab":0,"group":"client-server","color":"#3a8f4d"}]' >/dev/null
sleep 3
holds '^\[\[groups\]\]'            && ok "an initiative exists"               || bad "no [[groups]] entry"
holds 'name = "client-server"'     && ok "it carries the name it was given"   || bad "the initiative has no name"
holds '^group = '                  && ok "the task points at it"              || bad "the task is not a member"
holds 'color = "#3a8f4d"'          && ok "it kept the colour it was seeded with" || bad "the colour was dropped"

say "An initiative folds — the left bar's own gesture, through the socket"
ctl tabs '[{"op":"collapse","group":"client-server","collapsed":true}]' >/dev/null
sleep 3
holds 'collapsed = true' && ok "the fold is recorded, so it survives a relaunch" || bad "the fold did not persist"

say "The left bar's state rides in the same file"
for field in left_bar left_bar_w scope; do
    holds "^$field = " && ok "$field is written" || bad "$field is missing"
done
holds '^projects = \[\]' && ok "projects is present and empty on a fresh session" \
    || { holds '^\[\[projects\]\]' && ok "projects are written as entries" || bad "no projects key at all"; }

say "A task leaves its initiative"
ctl tabs '[{"op":"ungroup","tab":0}]' >/dev/null
sleep 3
holds '^group = ' && bad "the task is still in a group" || ok "it is back at the top level"

say "Everything comes back on a relaunch"
tabs_before=$(count '^\[\[tabs\]\]')
kill "$APP" 2>/dev/null
sleep 3
XDG_CONFIG_HOME="$CFG" TD_NO_SESSIOND=1 nohup "$BIN" >"$WORK/window2.log" 2>&1 &
APP2=$!
sleep 9
kill -0 "$APP2" 2>/dev/null && ok "it reopened (pid $APP2)" || bad "the relaunch died"
sleep 2
tabs_after=$(count '^\[\[tabs\]\]')
[ "$tabs_after" -eq "$tabs_before" ] && ok "the same $tabs_after tabs are there" \
    || bad "tab count changed across a relaunch: $tabs_before -> $tabs_after"
holds 'name = "host debts"' && ok "the task kept its name" || bad "the name was lost"

printf '\n\033[1mRESULT: %s passed, %s failed\033[0m\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
