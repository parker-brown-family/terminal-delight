#!/usr/bin/env bash
# Qualify the real binary's removal path on a FIXTURE home (never the account
# home): plan, detached worker, decoy TD process matched by executable path,
# someone else's hook kept, receipt written, worker copy gone. The operator's
# own TD processes are listed before and after and must be unchanged.
set -euo pipefail
BIN=${BIN:-$HOME/.cache/td-uninstall-cargo/debug/terminal-delight}
REPO=/home/parker/Work/td-uninstall-lab/terminal-delight
F=$HOME/.cache/td-uninstall-admin/qual-home
OUT=$HOME/.cache/td-uninstall-admin/qual-evidence
rm -rf "$F" "$OUT"; mkdir -p "$F" "$OUT"
lib=$F/.local/lib/terminal-delight
mkdir -p "$lib" "$F/.local/bin" "$F/.config/terminal-delight/sessions" "$F/.local/state/terminal-delight" "$F/run/terminal-delight" "$F/tmp" "$F/.claude"
cp /usr/bin/sleep "$lib/td-qual0001-current"
cp /usr/bin/sleep "$lib/td-qual0002-old"
ln -s "$lib/td-qual0001-current" "$F/.local/bin/terminal-delight"
cp "$REPO/scripts/td-agent-hooks" "$F/.local/bin/td-agent-hooks"
echo 'theme = "x"' > "$F/.config/terminal-delight/theme.toml"
echo > "$F/run/terminal-delight-glyph-qual.svg"
printf '%s\tterminal-delight-glyph-qual.svg\n' "$(sha256sum "$F/run/terminal-delight-glyph-qual.svg" | cut -d' ' -f1)" > "$F/run/terminal-delight-assets.manifest"
echo "another tool's picture" > "$F/run/terminal-delight-scene-storyboard.webp"
echo 'not ours' > "$F/run/other.txt"
printf '{\n  "hooks": {\n    "Stop": [\n      {\n        "hooks": [\n          {\n            "type": "command",\n            "command": "%s/.local/bin/td-agent-hooks"\n          },\n          {\n            "type": "command",\n            "command": "keep-me"\n          }\n        ]\n      }\n    ]\n  }\n}\n' "$F" > "$F/.claude/settings.json"

real_before=$(ps -eo pid=,args= | grep -c '[/]home/parker/.local/bin/terminal-delight' || true)
"$lib/td-qual0001-current" 600 & decoy=$!
run() { env -i HOME="$F" XDG_RUNTIME_DIR="$F/run" TMPDIR="$F/tmp" PATH=/usr/bin:/bin "$BIN" "$@"; }

run admin guard | tee "$OUT/guard.txt"
run uninstall preview > "$OUT/preview.md"
mapfile -t paths < <(run uninstall plan)
echo "plan: ${paths[*]}" | tee "$OUT/plan-paths.txt"
cp "${paths[0]}" "$OUT/plan.md"
run uninstall run --plan "${paths[1]}" | tee "$OUT/run.txt"
receipt=$(find "$F/.local/state/terminal-delight-removal" -name receipt.md 2>/dev/null | head -1 || true)
for _ in $(seq 1 120); do
  receipt=$(find "$F/.local/state/terminal-delight-removal" -name receipt.md 2>/dev/null | head -1 || true)
  [[ -n $receipt ]] && break
  sleep 0.5
done
sleep 1
cp "$receipt" "$OUT/receipt.md"
removal=$(dirname "$receipt")
{
  echo "decoy alive after run: $(kill -0 "$decoy" 2>/dev/null && echo YES || echo no)"
  echo "operator TD relays before/after: $real_before/$(ps -eo pid=,args= | grep -c '[/]home/parker/.local/bin/terminal-delight' || true)"
  for p in .local/bin/terminal-delight .local/bin/td-agent-hooks "$lib/td-qual0001-current" "$lib/td-qual0002-old" run/terminal-delight run/terminal-delight-glyph-qual.svg; do
    q=$p; [[ $q = /* ]] || q=$F/$p
    echo "$( [[ -e $q || -L $q ]] && echo KEPT || echo gone ): $p"
  done
  for p in run/other.txt .config/terminal-delight/theme.toml run/terminal-delight-scene-storyboard.webp; do echo "$( [[ -e $F/$p ]] && echo kept || echo GONE ): $p"; done
  echo "someone else's hook kept: $(grep -c keep-me "$F/.claude/settings.json")"
  echo "td-agent-hooks hook left: $(grep -c td-agent-hooks "$F/.claude/settings.json" || true)"
  echo "files beside settings: $(ls -A "$F/.claude" | tr '\n' ' ')"
  echo "worker copy left: $( [[ -e $removal/worker ]] && echo YES || echo no)"
  echo "removal dir: $(ls "$removal" | tr '\n' ' ')"
} | tee "$OUT/checks.txt"
kill "$decoy" 2>/dev/null || true
