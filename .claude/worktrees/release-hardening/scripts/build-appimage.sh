#!/usr/bin/env bash
# Build a distributable, MIT-clean terminal-delight AppImage.
#
# Why an AppImage is even possible: docs/patches/0002-sever-gpl-crates.patch
# removes the GPL crates the Zed graph would otherwise link, so the binary is
# permissive-licensed and redistributable (see THIRD-PARTY-LICENSES.md). This
# script bundles the binary + .desktop + icon + a generated THIRD-PARTY-LICENSES
# text (cargo-about) into a single self-contained file.
#
# Graphics libraries (Vulkan loader, Wayland/X11/xkbcommon) are intentionally NOT
# bundled — a GPU app must load the *host's* driver stack, so we rely on the
# host's system libs (present on any desktop Linux) exactly like Alacritty/Zed do.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
arch="x86_64"
app="terminal-delight"
dist="$repo_root/dist"
build="$repo_root/.appimage-build"
appdir="$build/AppDir"

export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"

echo "==> Ensuring patched gpui checkout (0001 + 0002)"
bash "$repo_root/scripts/prepare-gpui.sh"

echo "==> Release build"
( cd "$repo_root/app" && cargo build --release --locked )
bin="$repo_root/app/target/release/$app"
[ -x "$bin" ] || { echo "missing release binary: $bin" >&2; exit 1; }

echo "==> Generating THIRD-PARTY-LICENSES (cargo-about)"
if ! command -v cargo-about >/dev/null; then
    echo "   cargo-about not found; installing..."
    cargo install cargo-about --version "^0.6"
fi
tpl="$build/THIRD-PARTY-LICENSES.txt"
mkdir -p "$build"
( cd "$repo_root/app" && cargo about generate about.hbs ) > "$tpl"
[ -s "$tpl" ] || { echo "license bundle came out empty" >&2; exit 1; }

echo "==> Staging AppDir"
rm -rf "$appdir"
install -Dm755 "$bin"                                   "$appdir/usr/bin/$app"
install -Dm644 "$repo_root/packaging/$app.desktop"      "$appdir/usr/share/applications/$app.desktop"
install -Dm644 "$repo_root/packaging/$app.desktop"      "$appdir/$app.desktop"
install -Dm644 "$repo_root/packaging/$app.svg"          "$appdir/usr/share/icons/hicolor/scalable/apps/$app.svg"
install -Dm644 "$repo_root/packaging/$app.svg"          "$appdir/$app.svg"
install -Dm644 "$tpl"                                   "$appdir/usr/share/licenses/$app/THIRD-PARTY-LICENSES.txt"
install -Dm644 "$repo_root/LICENSE"                     "$appdir/usr/share/licenses/$app/LICENSE"

# Default agent-bell sounds. bell::ensure_seeded() copies these into
# ~/.config/terminal-delight/sounds on first run; it looks in $TD_SOUNDS (set by
# AppRun below) and in assets/sounds next to the binary. We bundle them so the
# distributed AppImage ships working defaults. All bundled clips are PD/CC0 (see
# BELL_SOUNDS.md, also shipped as the sound credits). The bell plays via the host
# `ffplay` (ffmpeg) and degrades silently if it isn't installed — ffmpeg is NOT
# bundled (it would be large and is often GPL, which would taint the binary).
sounds_src="$repo_root/app/assets/sounds"
if [ -d "$sounds_src" ]; then
    for s in "$sounds_src"/*; do
        [ -f "$s" ] && install -Dm644 "$s" "$appdir/usr/share/$app/sounds/$(basename "$s")"
    done
    [ -f "$repo_root/BELL_SOUNDS.md" ] && \
        install -Dm644 "$repo_root/BELL_SOUNDS.md" "$appdir/usr/share/licenses/$app/SOUND-CREDITS.md"
fi

cat > "$appdir/AppRun" <<'APPRUN'
#!/usr/bin/env bash
HERE="$(dirname "$(readlink -f "${0}")")"
# Point the agent-bell seeder at the bundled default sounds (works regardless of
# how/where the AppImage is mounted). The bell needs the host `ffplay` to actually
# play; it degrades silently if ffmpeg isn't installed.
export TD_SOUNDS="${TD_SOUNDS:-$HERE/usr/share/terminal-delight/sounds}"
exec "$HERE/usr/bin/terminal-delight" "$@"
APPRUN
chmod +x "$appdir/AppRun"

echo "==> Fetching appimagetool (cached)"
tool="$build/appimagetool-$arch.AppImage"
if [ ! -x "$tool" ]; then
    url="https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$arch.AppImage"
    curl -fL --retry 3 -o "$tool" "$url"
    chmod +x "$tool"
fi

echo "==> Packing AppImage"
mkdir -p "$dist"
out="$dist/$app-$arch.AppImage"
# --appimage-extract-and-run avoids a hard FUSE dependency in CI/containers.
ARCH="$arch" "$tool" --appimage-extract-and-run "$appdir" "$out"

echo "==> Done: $out"
ls -lh "$out"
