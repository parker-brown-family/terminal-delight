#!/usr/bin/env bash
# End-to-end proof of the updater against a REAL published AppImage, in a
# throwaway HOME, with the release listing served from a fixture over file://.
#
# What it proves, each step asserted:
#   1. check     — the stable channel finds the release and calls it newer
#   2. staged    — a build whose wire cannot be confirmed is staged, not installed
#   3. install   — --now downloads, verifies the checksum, extracts, smoke-checks,
#                  backs up settings, replaces the AppImage and keeps the old one
#   4. rollback  — puts the old image back and keeps the new one as "previous"
#   5. checksum  — a wrong published digest fails, deletes the download, and
#                  leaves the installed image byte-identical
set -euo pipefail
B=${B:?set B to the terminal-delight binary under test}
OUT=${OUT:-${TMPDIR:-/tmp}/td-update-e2e-appimage}
rm -rf "$OUT"; mkdir -p "$OUT"
H="$OUT/home"; FX="$OUT/fixture/repos/parker-brown-family/terminal-delight"
mkdir -p "$H/Apps" "$FX/compare" "$H/.config/terminal-delight"
echo 'theme = "kept"' > "$H/.config/terminal-delight/theme.toml"
printf 'OLD-IMAGE\n' > "$H/Apps/terminal-delight-x86_64.AppImage"
chmod 755 "$H/Apps/terminal-delight-x86_64.AppImage"
OLD_SUM=$(sha256sum "$H/Apps/terminal-delight-x86_64.AppImage" | cut -d' ' -f1)

REL=https://github.com/parker-brown-family/terminal-delight/releases/download/v0.4.0-rc.3
SHA=$("$B" --build-info | jq -r .sha)
# rc.3 dressed as a stable release, so the stable channel offers it.
jq -n --arg rel "$REL" '[{tag_name:"v0.4.0-rc.3", draft:false, prerelease:false,
  published_at:"2026-10-21T16:00:57Z", html_url:"https://github.com/parker-brown-family/terminal-delight/releases/tag/v0.4.0-rc.3",
  body:"fixture: rc.3 presented as stable",
  assets:[{name:"terminal-delight-x86_64.AppImage", size:38287864, browser_download_url:($rel+"/terminal-delight-x86_64.AppImage")},
          {name:"terminal-delight-x86_64.AppImage.sha256", size:99, browser_download_url:($rel+"/terminal-delight-x86_64.AppImage.sha256")}]}]' \
  > "$FX/releases"
echo '{"status":"ahead","ahead_by":3,"commits":[]}' > "$FX/compare/$SHA...v0.4.0-rc.3"

run() { env -i HOME="$H" PATH="$PATH" XDG_RUNTIME_DIR="$OUT/run" TD_UPDATE_API="file://$OUT/fixture" \
          APPIMAGE="$H/Apps/terminal-delight-x86_64.AppImage" APPDIR="$(dirname "$B")" "$B" update "$@"; }
# APPDIR is what an AppImage's runtime sets to its mount; TD treats APPIMAGE as
# its own install only when the running program lives inside APPDIR, so the
# binary under test stands in for one running from the image.
mkdir -p "$OUT/run"; chmod 700 "$OUT/run"

echo "== 1. check"; run check --json | tee "$OUT/1-check.json" | jq -c '{rel: .offer.relation, v: .offer.version}'
[ "$(jq -r .offer.relation "$OUT/1-check.json")" = newer ]

echo "== 2. install without --now (rc.3 predates --build-info, so its wire is unknown)"
run install --json | tee "$OUT/2-staged.json" | jq -c .
[ "$(jq -r .staged "$OUT/2-staged.json")" = true ]
[ "$(sha256sum "$H/Apps/terminal-delight-x86_64.AppImage" | cut -d' ' -f1)" = "$OLD_SUM" ]
grep -q '\[pending\]' "$H/.local/state/terminal-delight/updates.toml"

echo "== 3. install --now"
run install --now --json | tee "$OUT/3-install.json" | jq -c '{describe, entry, backup}'
NEW_SUM=$(sha256sum "$H/Apps/terminal-delight-x86_64.AppImage" | cut -d' ' -f1)
[ "$NEW_SUM" = "$(curl -sL "$REL/terminal-delight-x86_64.AppImage.sha256" | cut -d' ' -f1)" ]
[ "$(sha256sum "$H/Apps/terminal-delight-x86_64.AppImage.previous" | cut -d' ' -f1)" = "$OLD_SUM" ]
BK=$(jq -r .backup "$OUT/3-install.json"); [ "$(cat "$BK/config/theme.toml")" = 'theme = "kept"' ]
"$H/Apps/terminal-delight-x86_64.AppImage" --appimage-version >/dev/null 2>&1 || true
APPIMAGE_EXTRACT_AND_RUN=1 "$H/Apps/terminal-delight-x86_64.AppImage" --version | tee "$OUT/3-version.txt"

echo "== 4. rollback"
run rollback --json | tee "$OUT/4-rollback.json" | jq -c '{describe, backup}'
[ "$(sha256sum "$H/Apps/terminal-delight-x86_64.AppImage" | cut -d' ' -f1)" = "$OLD_SUM" ]
[ "$(sha256sum "$H/Apps/terminal-delight-x86_64.AppImage.previous" | cut -d' ' -f1)" = "$NEW_SUM" ]

echo "== 5. a wrong published checksum"
printf '%064d  terminal-delight-x86_64.AppImage\n' 0 > "$OUT/fixture/bad.sha256"
jq --arg bad "file://$OUT/fixture/bad.sha256" '.[0].assets[1].browser_download_url=$bad' "$FX/releases" > "$FX/releases.new"
mv "$FX/releases.new" "$FX/releases"
set +e; run install --now 2> "$OUT/5-error.txt"; code=$?; set -e
cat "$OUT/5-error.txt"
[ "$code" -ne 0 ]; grep -q "did not match its published checksum" "$OUT/5-error.txt"
[ "$(sha256sum "$H/Apps/terminal-delight-x86_64.AppImage" | cut -d' ' -f1)" = "$OLD_SUM" ]
[ -z "$(find "$H/.cache/terminal-delight/updates" -name '*.AppImage' 2>/dev/null)" ]

echo "== backups"; run backups
echo "ALL PASSED"
