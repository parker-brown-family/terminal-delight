#!/usr/bin/env bash
# The launcher install path, with a LIVE session host holding real shells.
#
# Throwaway HOME with the layout Parker's machine uses:
#   ~/.local/bin/terminal-delight -> ~/.local/lib/terminal-delight/td-<sha>-qa
# A session host is started from that launcher and given three shells. Then the
# dev channel is followed through install, a second install, rollback and a
# channel switch, and after EVERY step the host's pid and start time and each
# shell's pid and start time must be exactly what they were.
#
# The dev build offered here is this worktree's own binary wrapped in a stand-in
# AppImage (a script answering --appimage-extract), so its --build-info carries
# the same wire as the running build: this exercises the "Same wire, restart
# only the window" path end to end, which a pre-0.5 release cannot.
set -euo pipefail
SRC=${SRC:?set SRC to the terminal-delight binary under test}
OUT=${OUT:-${TMPDIR:-/tmp}/td-update-e2e-launcher}
rm -rf "$OUT"; mkdir -p "$OUT"
H="$OUT/home"; RUN="$OUT/run"; FX="$OUT/fixture"; API="$FX/repos/parker-brown-family/terminal-delight"
mkdir -p "$H/.local/bin" "$H/.local/lib/terminal-delight" "$RUN" "$API/compare" "$API/releases/tags" "$H/.config/terminal-delight"
chmod 700 "$RUN"
echo 'theme = "kept"' > "$H/.config/terminal-delight/theme.toml"
SHA=$("$SRC" --build-info | jq -r .sha); S8=${SHA:0:8}
cp "$SRC" "$H/.local/lib/terminal-delight/td-$S8-qa"
ln -s "$H/.local/lib/terminal-delight/td-$S8-qa" "$H/.local/bin/terminal-delight"
L="$H/.local/bin/terminal-delight"

# The dev build: this same source built with TD_BUILD_SHA set to a stand-in
# commit (DEV below), so it is a different build with the same wire.
# The stand-in AppImage extracts it; its --build-info names that commit, and
# build.json must name the same one or the download is refused.
DEV=${DEV:?set DEV to the same source built with TD_BUILD_SHA=<another 40-hex commit>}
DEVSHA=$("$DEV" --build-info | jq -r .sha); D8=${DEVSHA:0:8}
mkdir -p "$FX/payload"
cp "$DEV" "$FX/payload/terminal-delight"
cat > "$FX/terminal-delight-x86_64.AppImage" <<EOF
#!/bin/sh
[ "\$1" = --appimage-extract ] || exit 9
mkdir -p squashfs-root/usr/bin && cp "$FX/payload/terminal-delight" squashfs-root/usr/bin/terminal-delight
EOF
chmod 755 "$FX/terminal-delight-x86_64.AppImage"
(cd "$FX" && sha256sum terminal-delight-x86_64.AppImage > terminal-delight-x86_64.AppImage.sha256)
jq -n --arg sha "$DEVSHA" '{schema:"td-dev-build/1", sha:$sha, date:"2026-11-02T12:00:00Z", branch:"dev/0.5.0", version:"0.4.0-rc.3"}' > "$FX/build.json"
jq -n --arg fx "file://$FX" '{tag_name:"dev-build", prerelease:true, draft:false, html_url:"https://example.invalid/dev-build",
  assets:[{name:"terminal-delight-x86_64.AppImage", size:1, browser_download_url:($fx+"/terminal-delight-x86_64.AppImage")},
          {name:"terminal-delight-x86_64.AppImage.sha256", size:1, browser_download_url:($fx+"/terminal-delight-x86_64.AppImage.sha256")},
          {name:"build.json", size:1, browser_download_url:($fx+"/build.json")}]}' > "$API/releases/tags/dev-build"
mkdir -p "$API/commits/dev"; echo '{"sha":"'"$DEVSHA"'"}' > "$API/commits/dev/0.5.0"
# The running build differs from the dev build by a "diverged" history, so it is offered.
echo '{"status":"diverged","ahead_by":2,"commits":[{"sha":"'"$DEVSHA"'","commit":{"message":"qa: a dev commit"}}]}' > "$API/compare/$SHA...$DEVSHA"

# TD_SESSION=qa: the CLI runs as it would inside one of this session's panes,
# so the host it is attached to is the one it may vouch for.
envs=(env -i HOME="$H" PATH="$PATH" SHELL=/bin/bash XDG_RUNTIME_DIR="$RUN" TD_UPDATE_API="file://$FX" TD_SESSION=qa)
td() { "${envs[@]}" "$L" "$@"; }

# --- a live session host with three shells -----------------------------------
"${envs[@]}" setsid "$L" serve --session qa > "$OUT/host.log" 2>&1 < /dev/null &
for _ in $(seq 50); do [ -S "$RUN/terminal-delight/session-qa.sock" ] && break; sleep 0.1; done
python3 -I - "$RUN/terminal-delight/session-qa.sock" spawn 3 > "$OUT/panes-before.json" <<'PY'
import json, socket, sys
path, what = sys.argv[1], sys.argv[2]
def call(f, req):
    f.write(json.dumps(req) + "\n"); f.flush(); return json.loads(f.readline())
s = socket.socket(socket.AF_UNIX); s.connect(path); f = s.makefile("rw")
hello = call(f, {"verb": "hello", "proto": 1, "kind": "tool"})
geom = {"cols": 80, "rows": 24, "cell_width": 8, "cell_height": 20}
if what == "spawn":
    for _ in range(int(sys.argv[3])):
        call(f, {"verb": "spawn-pane", "geom": geom})
print(json.dumps({"host": hello.get("host"), "panes": call(f, {"verb": "list-panes"})}))
PY
panes() { python3 -I - "$RUN/terminal-delight/session-qa.sock" list <<'PY'
import json, socket, sys
s = socket.socket(socket.AF_UNIX); s.connect(sys.argv[1]); f = s.makefile("rw")
def call(req):
    f.write(json.dumps(req) + "\n"); f.flush(); return json.loads(f.readline())
hello = call({"verb": "hello", "proto": 1, "kind": "tool"})
print(json.dumps({"host": hello.get("host"), "panes": call({"verb": "list-panes"})}))
PY
}
# The host's own pid, from its hello: never found by a process-name pattern,
# which would match another run's host, or this shell.
HOSTPID=$(jq -r .host "$OUT/panes-before.json")
[ "$HOSTPID" -gt 0 ]
starttick() { awk '{print $22}' "/proc/$1/stat"; }
HOSTTICK=$(starttick "$HOSTPID")
SHELLS=$(jq -r '.. | objects | select(has("shell_pid")) | .shell_pid' "$OUT/panes-before.json" | sort -n)
echo "host $HOSTPID (tick $HOSTTICK), shells: $(echo $SHELLS)"
[ "$(echo "$SHELLS" | wc -w)" -eq 3 ]
declare -A TICK; for p in $SHELLS; do TICK[$p]=$(starttick "$p"); done

same() {
  [ "$(panes | jq -r .host)" = "$HOSTPID" ] && [ "$(starttick "$HOSTPID")" = "$HOSTTICK" ] || { echo "HOST CHANGED after $1"; exit 1; }
  for p in $SHELLS; do [ -d "/proc/$p" ] && [ "$(starttick "$p")" = "${TICK[$p]}" ] || { echo "SHELL $p LOST after $1"; exit 1; }; done
  now=$(panes | jq -r '.. | objects | select(has("shell_pid")) | .shell_pid' | sort -n)
  [ "$now" = "$SHELLS" ] || { echo "PANE LIST CHANGED after $1"; exit 1; }
  echo "   kept: host $HOSTPID and shells $(echo $SHELLS) after $1"
}
same "setup"

echo "== channel dev"; td update channel dev
same "channel switch"
echo "== check"; td update check --json | tee "$OUT/check.json" | jq -c '{rel: .offer.relation, sha: .offer.sha[0:8], ahead: .offer.ahead_by}'
echo "== another session's host runs a different build: the install is staged, not done"
cp "$DEV" "$OUT/other-build"
"${envs[@]}" setsid "$OUT/other-build" serve --session qa2 > "$OUT/host2.log" 2>&1 < /dev/null &
OTHER=$!
for _ in $(seq 50); do [ -S "$RUN/terminal-delight/session-qa2.sock" ] && break; sleep 0.1; done
BEFORE=$(readlink "$L")
td update install --json | tee "$OUT/staged-other.json" | jq -c '{staged, unvouched}'
[ "$(jq -r .staged "$OUT/staged-other.json")" = true ]
[ "$(jq -r '.unvouched[0]' "$OUT/staged-other.json")" = qa2 ]
[ "$(readlink "$L")" = "$BEFORE" ]
same "staging for another session"
kill "$OTHER"; for _ in $(seq 50); do [ -d "/proc/$OTHER" ] || break; sleep 0.1; done
echo "== install (same wire: no --now needed)"
td update install --json | tee "$OUT/install.json" | jq -c '{describe, entry}'
NEW=$(readlink "$L"); echo "   launcher -> $NEW"
[ "$NEW" = "$H/.local/lib/terminal-delight/td-$D8-dev" ]
[ -f "$H/.local/lib/terminal-delight/td-$S8-qa" ]
same "install"
"$L" --version
echo "== status"; td update status
echo "== rollback"; td update rollback --json | jq -c '{describe}'
[ "$(readlink "$L")" = "$H/.local/lib/terminal-delight/td-$S8-qa" ]
same "rollback"
echo "== roll forward again (rollback is itself undoable)"; td update rollback --json | jq -c '{describe}'
[ "$(readlink "$L")" = "$H/.local/lib/terminal-delight/td-$D8-dev" ]
same "second rollback"
echo "== switch back to stable (no stable published in this fixture: nothing is offered, nothing changes)"
td update channel stable
td update check || true
[ "$(readlink "$L")" = "$H/.local/lib/terminal-delight/td-$D8-dev" ]
same "switch to stable"
kill "$HOSTPID"; echo "ALL PASSED"
