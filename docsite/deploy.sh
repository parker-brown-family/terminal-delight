#!/usr/bin/env bash
# Deploy the docs to https://docs.terminal-delight.brownfamilysports.com.
#
# The docs are a static site served by Caddy from /var/www/td-docs on the
# shared BFS-Services box piper-prod (165.245.234.21), the same box and the
# same pattern as parker.brownfamilysports.com. There is no push-triggered
# deploy: this script is the deploy.
#
#   docsite/deploy.sh            build, rsync, verify
#   docsite/deploy.sh --dry-run  show what would change, transfer nothing
#   docsite/deploy.sh --caddy    also install docsite/td-docs.caddy, validate
#                                the whole Caddy config, and reload — only
#                                needed when that file changes
#
# Requires the deploy key at ~/.piper-deploy/piper_deploy (root@piper-prod).
# The host is addressed by IP because `piper-prod` does not resolve off the
# tailnet; override with PIPER_HOST.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

KEY="${PIPER_KEY:-$HOME/.piper-deploy/piper_deploy}"
HOST="${PIPER_HOST:-root@165.245.234.21}"
DEST="/var/www/td-docs/"
URL="https://docs.terminal-delight.brownfamilysports.com"
SSH=(ssh -i "$KEY" -o IdentitiesOnly=yes -o BatchMode=yes -o ConnectTimeout=15)
DRY=""; CADDY=""
for a in "$@"; do
  case "$a" in
    --dry-run) DRY="n" ;;
    --caddy) CADDY=1 ;;
    *) echo "unknown flag: $a"; exit 2 ;;
  esac
done

[ -f "$KEY" ] || { echo "✗ deploy key missing: $KEY"; exit 1; }

echo "▸ building docsite/dist"
node docsite/build.mjs

# /var/www/td-docs holds nothing but this build, so --delete is safe here.
# (parkerbrown-dev's web root is shared with other publishers and cannot use
# a bare --delete; this one is not.)
echo "▸ rsync → $HOST:$DEST"
"${SSH[@]}" "$HOST" "mkdir -p $DEST"
rsync -az${DRY} --delete --itemize-changes -e "${SSH[*]}" docsite/dist/ "$HOST:$DEST"
[ -n "$DRY" ] && { echo "dry run: nothing transferred"; exit 0; }

if [ -n "$CADDY" ]; then
  echo "▸ installing td-docs.caddy, validating, reloading"
  # Staged as .new (the import glob is *.caddy, so it is not live yet); the
  # previous version is kept as .prev and put back if validation fails.
  rsync -a -e "${SSH[*]}" docsite/td-docs.caddy "$HOST:/etc/caddy/conf.d/td-docs.caddy.new"
  "${SSH[@]}" "$HOST" 'set -e; cd /etc/caddy/conf.d
    [ -f td-docs.caddy ] && cp td-docs.caddy td-docs.caddy.prev
    mv td-docs.caddy.new td-docs.caddy
    if caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile >/tmp/td-docs-validate.log 2>&1; then
      systemctl reload caddy && echo "  caddy validated and reloaded"
    else
      echo "✗ caddy validate failed — not reloading, restoring the previous file"; tail -5 /tmp/td-docs-validate.log
      if [ -f td-docs.caddy.prev ]; then mv td-docs.caddy.prev td-docs.caddy; else rm -f td-docs.caddy; fi
      exit 1
    fi'
fi

echo "▸ verifying $URL"
fail=0
for p in / /install /workbench /briefs /assets/clips/decision-briefs.mp4 /assets/briefs/telemetry-cache.html /assets/docs.css /assets/docs.js /assets/kiosk-theme.js /assets/td-shell.js /assets/td-glass.js /assets/td-docs.css /assets/omarchy/bg/last-voyage.webp /assets/fonts/fonts.css /assets/fonts/inter-latin.woff2 /favicon.svg; do
  code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 20 "$URL$p" || true)
  echo "  $code $p"
  [ "$code" = "200" ] || fail=1
done
[ "$fail" = 0 ] && echo "✓ live at $URL" || { echo "✗ something did not answer 200"; exit 1; }
