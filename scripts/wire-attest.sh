#!/usr/bin/env bash
# Prove that a new build reads an old build's session host correctly, so the
# updater can offer it as "Restart window" instead of staging it for a cold
# start (issue 1267).
#
#   scripts/wire-attest.sh <old-binary> <new-binary> <work-dir>
#
# The updater's own check is a hash of the wire's source (app/build.rs), and
# any change to the terminal core moves it, even one measured to read old
# hosts the same. This is that measurement:
#
#   1. Both builds must speak the same protocol (`wire_proto` in --build-info).
#      A replay tests how a core parses bytes, never a changed control verb or
#      snapshot format, so a protocol change is never attested.
#   2. A throwaway host of the OLD build is captured (probe-capture.py).
#   3. The NEW build replays the capture (`terminal-delight wire-replay`) and
#      must reach the hash the old host reported.
#   4. Controls: the same capture with one letter changed, and with the
#      reported hash changed, must each be called a mismatch. A replay that
#      passes everything has tested nothing.
#
# Exit 0 prints one line of JSON, the attestation, for build.json:
#   {"compatible_with": ["<old wire>"], "against": {...}}
# Exit 3 means there is nothing to attest (same wire, a protocol change, an
# old build without the fields), with the reason on stderr. Anything else is
# the harness failing, which is never read as compatible.
#
# Runs hermetically: its own HOME and XDG_RUNTIME_DIR under <work-dir>, so it
# never meets a person's real session hosts.
set -euo pipefail

old=$(readlink -f "$1")
new=$(readlink -f "$2")
work=$(mkdir -p "$3" && cd "$3" && pwd)
here=$(cd "$(dirname "$0")" && pwd)

skip() { echo "wire-attest: nothing to attest: $*" >&2; exit 3; }
die() { echo "wire-attest: $*" >&2; exit 1; }

mkdir -p "$work/home" "$work/run"
chmod 700 "$work/run"
hermetic() {
    env -i PATH="$PATH" HOME="$work/home" XDG_RUNTIME_DIR="$work/run" LANG=C.UTF-8 "$@"
}

info() { hermetic "$1" --build-info; }
old_info=$(info "$old") || die "the old build did not answer --build-info"
new_info=$(info "$new") || die "the new build did not answer --build-info"
field() { jq -r --arg k "$2" '.[$k] // empty' <<<"$1"; }

old_wire=$(field "$old_info" wire)
new_wire=$(field "$new_info" wire)
[ -n "$old_wire" ] && [ -n "$new_wire" ] || skip "a build does not report its wire"
[ "$old_wire" != "$new_wire" ] || skip "the wires are the same ($new_wire); the updater already keeps terminals"
[ "$(field "$old_info" core)" = "$(field "$new_info" core)" ] || skip "the terminal cores differ"
old_proto=$(field "$old_info" wire_proto)
new_proto=$(field "$new_info" wire_proto)
[ -n "$old_proto" ] || skip "the old build predates wire_proto, so its protocol cannot be compared"
[ "$old_proto" = "$new_proto" ] || skip "the protocol changed ($old_proto -> $new_proto)"

key="wire-attest-$$"
cap="$work/capture"
log=$(hermetic python3 "$here/probe-capture.py" "$old" "$key" "$cap" 2>&1) || die "capture failed: $log"
grep -qx aligned <<<"$log" || die "the capture is not aligned: $log"

replay() { hermetic "$new" wire-replay "$1"; }
verdict=$(replay "$cap") || die "the new build reads the old host differently: $verdict"

# Control 1: one letter of printed text changed. "scroll line 42" is printed
# after the stream opened (probe-content.sh), so it is raw bytes in the
# capture, and the hash walks the scrollback it ends up in.
python3 - "$cap" "$work/mutated" <<'PY'
import json, shutil, sys
src, dst = sys.argv[1], sys.argv[2]
data = open(src + ".bin", "rb").read()
needle = b"scroll line 42"
at = data.find(needle)
if at < 0:
    sys.exit("control: the capture does not contain %r" % needle)
data = data[:at] + b"scroll line 4X" + data[at + len(needle):]
open(dst + ".bin", "wb").write(data)
shutil.copy(src + ".json", dst + ".json")
PY
set +e
replay "$work/mutated" >/dev/null
mutated=$?
# Control 2: the host's reported hash changed by one bit. In Python, because
# the hash is a u64 and jq's arithmetic would round it through a double.
python3 - "$cap" "$work/rehashed" <<'PY'
import json, shutil, sys
src, dst = sys.argv[1], sys.argv[2]
meta = json.load(open(src + ".json"))
meta["hash"] ^= 1
json.dump(meta, open(dst + ".json", "w"))
shutil.copy(src + ".bin", dst + ".bin")
PY
replay "$work/rehashed" >/dev/null
rehashed=$?
set -e
[ "$mutated" -eq 1 ] || die "control failed: a capture with a changed letter replayed as exit $mutated, not a mismatch"
[ "$rehashed" -eq 1 ] || die "control failed: a changed host hash replayed as exit $rehashed, not a mismatch"

jq -cn \
    --arg old_wire "$old_wire" \
    --arg new_wire "$new_wire" \
    --arg old_sha "$(field "$old_info" sha)" \
    --argjson verdict "$verdict" \
    '{compatible_with: [$old_wire],
      against: {wire: $old_wire, sha: $old_sha, replay: $verdict,
                controls: "a changed letter and a changed host hash were both refused"},
      wire: $new_wire}'
