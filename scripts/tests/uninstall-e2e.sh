#!/usr/bin/env bash
# Uninstall, end to end, with the real binary. Every check is an assertion: the
# script exits 1 on the first one that fails and prints what it saw.
#
#   scripts/tests/uninstall-e2e.sh <terminal-delight binary>
#   TD_E2E_DISPOSABLE_HOME=1 scripts/tests/uninstall-e2e.sh --account-home <binary>
#
# Fixture mode (the default) builds a TD install inside a scratch HOME that is
# not the account's. The planner treats that as a test home: every root must
# resolve inside it, and no agent CLI, systemd unit or session host outside it
# is touched. It is safe on any machine, the Legion included, and CI runs it on
# every pull request.
#
# Account-home mode installs TD into the REAL $HOME with the repository's own
# installers (desktop entry, agent hooks, default terminal, an MCP registration
# behind a stub `claude`) and removes it for real, through the detached worker,
# with records exported first. It refuses unless TD_E2E_DISPOSABLE_HOME=1 and
# the home holds no Terminal Delight already; the binary also refuses on a
# protected machine. Run it on a CI runner, or locally only through
# scripts/tests/uninstall-e2e-sandbox.sh, which mounts a scratch folder over
# $HOME in a PID namespace of its own.
set -euo pipefail

account=0
if [[ ${1:-} == --account-home ]]; then
  account=1
  shift
fi
BIN=$(realpath "${1:?usage: uninstall-e2e.sh [--account-home] <terminal-delight binary>}")
REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
SLEEP=$(command -v sleep)
fails=0
ok() { echo "  ok   $*"; }
bad() { echo "  FAIL $*"; fails=$((fails + 1)); }
check() { local what=$1; shift; if "$@"; then ok "$what"; else bad "$what"; fi; }
gone() { [[ ! -e $1 && ! -L $1 ]]; }
kept() { [[ -e $1 || -L $1 ]]; }
alive() { kill -0 "$1" 2>/dev/null; }

if ((account)); then
  [[ ${TD_E2E_DISPOSABLE_HOME:-} == 1 ]] || { echo "refusing: account-home mode removes TD from this account; set TD_E2E_DISPOSABLE_HOME=1 only on a disposable machine" >&2; exit 2; }
  passwd_home=$(getent passwd "$(id -un)" | cut -d: -f6)
  [[ $HOME == "$passwd_home" ]] || { echo "refusing: HOME ($HOME) is not this account's home ($passwd_home)" >&2; exit 2; }
  for p in "$HOME/.local/lib/terminal-delight" "$HOME/.local/bin/terminal-delight" "$HOME/.config/terminal-delight"; do
    if [[ -e $p || -L $p ]]; then echo "refusing: $p exists, so this home already has a Terminal Delight" >&2; exit 2; fi
  done
  H=$HOME
else
  H=$(mktemp -d "${TMPDIR:-/tmp}/td-uninstall-e2e.XXXXXX")
  trap 'chmod -R u+rwX "$H" 2>/dev/null; rm -rf "$H"' EXIT
fi
RUN=$H/.td-e2e-run
mkdir -p "$RUN" "$H/.local/bin" "$H/.config/terminal-delight/sessions" "$H/.local/state/terminal-delight" "$H/.claude" "$H/tmp"
chmod 700 "$RUN"
lib=$H/.local/lib/terminal-delight
mkdir -p "$lib"

# The install: the binary under test as the current build, an old build, the
# launcher, records, and a decoy TD process running from the old build.
install -m755 "$BIN" "$lib/td-e2e0001-current"
install -m755 "$SLEEP" "$lib/td-e2e0002-old"
ln -s "$lib/td-e2e0001-current" "$H/.local/bin/terminal-delight"
printf 'theme = "default"\n' > "$H/.config/terminal-delight/theme.toml"
printf 'active = 0\n' > "$H/.config/terminal-delight/sessions/1.toml"
printf 'kept history\n' > "$H/.local/state/terminal-delight/history.txt"
# A look-alike of another tool's, and a stranger's file, in TD's runtime dir.
printf "another tool's picture\n" > "$RUN/terminal-delight-scene-storyboard.webp"
printf 'not ours\n' > "$RUN/other.txt"
# Someone else's program with a td- name, running from outside this home. (On
# a test home, a stranger running from inside it would rightly block the
# removal as a TD nobody can rule out.)
elsewhere=$(mktemp -d "${TMPDIR:-/tmp}/td-e2e-elsewhere.XXXXXX")
install -m755 "$SLEEP" "$elsewhere/td-outsider"
"$lib/td-e2e0002-old" 600 & decoy=$!
"$elsewhere/td-outsider" 600 & outsider=$!
cleanup() {
  kill "$decoy" "$outsider" 2>/dev/null || true
  rm -rf "$elsewhere"
  if [[ $account != 1 ]]; then chmod -R u+rwX "$H" 2>/dev/null; rm -rf "$H"; fi
}
trap cleanup EXIT

# Claude settings with someone else's hook beside the one TD's installer adds.
printf '{\n  "hooks": {\n    "Stop": [\n      {\n        "hooks": [\n          {\n            "type": "command",\n            "command": "keep-me"\n          }\n        ]\n      }\n    ]\n  }\n}\n' > "$H/.claude/settings.json"

if ((account)); then
  export PATH="$H/.local/bin:$PATH"
  stubs=$H/.td-e2e-stubs
  mkdir -p "$stubs"
  # A stand-in `claude` that does what `claude mcp remove` does to its file,
  # and logs how it was called, environment pin included.
  cat > "$stubs/claude" <<'STUB'
#!/usr/bin/env bash
printf '%s CLAUDE_CONFIG_DIR=%s\n' "$*" "${CLAUDE_CONFIG_DIR-<unset>}" >> "$HOME/.td-e2e-stubs/claude.log"
if [[ $1 == mcp && $2 == remove ]]; then
  name=${!#}
  python3 - "$HOME/.claude.json" "$name" <<'PY'
import json, sys
p, name = sys.argv[1], sys.argv[2]
d = json.load(open(p))
d.get("mcpServers", {}).pop(name, None)
json.dump(d, open(p, "w"), indent=2)
PY
fi
STUB
  chmod +x "$stubs/claude"
  export PATH="$stubs:$PATH"
  printf '{\n  "mcpServers": {\n    "terminal-delight": {"command": "terminal-delight", "args": ["mcp"]},\n    "other": {"command": "/usr/bin/true", "args": []}\n  }\n}\n' > "$H/.claude.json"
  # The repository's own installers, as a person would run them.
  bash "$REPO/scripts/install-desktop.sh" >/dev/null
  bash "$REPO/scripts/install-agent-hooks.sh" >/dev/null
  mkdir -p "$H/.config"
  printf 'foot.desktop\n' > "$H/.config/xdg-terminals.list"
  terminal-delight default-terminal on >/dev/null
  run() { env XDG_RUNTIME_DIR="$RUN" TMPDIR="$H/tmp" "$H/.local/bin/terminal-delight" "$@"; }
else
  install -m755 "$REPO/scripts/td-agent-hooks" "$H/.local/bin/td-agent-hooks"
  # Rewrite the settings as the installer would, so TD's hook is in them.
  python3 - "$H/.claude/settings.json" "$H/.local/bin/td-agent-hooks" <<'PY'
import json, sys
p, cmd = sys.argv[1], sys.argv[2]
d = json.load(open(p))
d["hooks"]["Stop"][0]["hooks"].insert(0, {"type": "command", "command": cmd})
open(p, "w").write(json.dumps(d, indent=2) + "\n")
PY
  echo > "$RUN/terminal-delight-glyph-e2e.svg"
  printf '%s\tterminal-delight-glyph-e2e.svg\n' "$(sha256sum "$RUN/terminal-delight-glyph-e2e.svg" | cut -d' ' -f1)" > "$RUN/terminal-delight-assets.manifest"
  run() { env -i HOME="$H" XDG_RUNTIME_DIR="$RUN" TMPDIR="$H/tmp" PATH=/usr/bin:/bin "$BIN" "$@"; }
fi

echo "== guard, export and old builds, read-only first"
guard=$(run admin guard)
check "the machine is not protected: $guard" grep -qv "Protected" <<<"$guard"
builds=$(run admin builds --json)
check "two builds listed" test "$(jq '.builds | length' <<<"$builds")" = 2
check "nothing blocks the old-builds count" test "$(jq -r '.blocked' <<<"$builds")" = null

echo "== a marker naming this machine makes the removal refuse"
run admin protect --reason "uninstall e2e" >/dev/null
mapfile -t refused < <(run uninstall plan)
if run uninstall run --plan "${refused[1]}" >/dev/null 2>&1; then bad "a protected machine refused"; else ok "a protected machine refused"; fi
check "nothing was removed while protected" kept "$lib/td-e2e0001-current"
rm -f "$H/.config/terminal-delight/protected"

echo "== plan, review and run"
retention=keep
((account)) && retention="export"
mapfile -t paths < <(run uninstall plan --retention "$retention")
check "the plan was written for review (${paths[1]##*/})" test -s "${paths[1]}"
run uninstall run --plan "${paths[1]}"
receipt=
for _ in $(seq 1 240); do
  receipt=$(find "$H/.local/state/terminal-delight-removal" -name receipt.json 2>/dev/null | head -1 || true)
  [[ -n $receipt ]] && break
  sleep 0.5
done
check "the worker wrote its receipt" test -n "$receipt"
[[ -n $receipt ]] || { echo "no receipt; worker log:"; cat "$H"/.local/state/terminal-delight-removal/*/worker.log 2>/dev/null; exit 1; }
sleep 1
removal=$(dirname "$receipt")
cat "$removal/receipt.md"

echo "== what the receipt and the disk say"
check "no step failed or was blocked" test "$(jq '[.outcomes[] | select(.status == "failed" or .status == "blocked")] | length' "$receipt")" = 0
check "the decoy TD process was stopped" bash -c "! kill -0 $decoy 2>/dev/null"
check "someone else's program still runs" alive "$outsider"
for p in "$H/.local/bin/terminal-delight" "$H/.local/bin/td-agent-hooks" "$lib/td-e2e0001-current" "$lib/td-e2e0002-old"; do
  check "removed: ${p#"$H"/}" gone "$p"
done
check "the stranger's file in the runtime dir stays" kept "$RUN/other.txt"
check "another tool's look-alike stays" kept "$RUN/terminal-delight-scene-storyboard.webp"
check "someone else's hook stays" grep -q keep-me "$H/.claude/settings.json"
check "TD's hook is gone" bash -c "! grep -q td-agent-hooks '$H/.claude/settings.json'"
check "nothing left beside the settings file" test "$(ls -A "$H/.claude")" = settings.json
check "the worker removed its own copy" gone "$removal/worker"
if ((account)); then
  check "records were exported first" bash -c "ls -d '$H'/terminal-delight-export-*/ >/dev/null"
  check "the export has its manifest" bash -c "ls '$H'/terminal-delight-export-*/manifest.json >/dev/null"
  check "records removed after the export" gone "$H/.config/terminal-delight"
  check "the desktop entry is gone" gone "$H/.local/share/applications/terminal-delight.desktop"
  check "the default terminal is put back" test "$(cat "$H/.config/xdg-terminals.list")" = foot.desktop
  check "claude was asked to remove TD's server, pinned to its default config" grep -q "^mcp remove -s user terminal-delight CLAUDE_CONFIG_DIR=<unset>" "$H/.td-e2e-stubs/claude.log"
  check "TD's MCP server is gone" test "$(jq -r '.mcpServers["terminal-delight"]' "$H/.claude.json")" = null
  check "the other MCP server stays" test "$(jq -r '.mcpServers.other.command' "$H/.claude.json")" = /usr/bin/true
else
  check "records kept" kept "$H/.config/terminal-delight/theme.toml"
  check "TD's recorded glyph is gone" gone "$RUN/terminal-delight-glyph-e2e.svg"
fi

if ((fails)); then
  echo "uninstall e2e: $fails check(s) failed"
  exit 1
fi
echo "uninstall e2e: every check passed ($([[ $account == 1 ]] && echo account home || echo fixture home))"
