#!/bin/sh
# Install Terminal Delight for the person running this: the stable AppImage as
# ~/.local/bin/terminal-delight, plus its launcher entry and icons. From then on
# the app updates itself (CONFIG > UPDATES). An install that is already here is
# left exactly as it is.
#
#   curl -fsSL https://terminal-delight.brownfamilysports.com/install.sh | sh
#
# Omarchy's Install > AI row runs this file. Tests: scripts/test-install.sh.
# TD_RELEASE_URL points it at another directory of release assets.
set -eu

release=${TD_RELEASE_URL:-https://github.com/parker-brown-family/terminal-delight/releases/latest/download}
asset=terminal-delight-x86_64.AppImage
bin=$HOME/.local/bin
target=$bin/terminal-delight
data=${XDG_DATA_HOME:-$HOME/.local/share}
cache=${XDG_CACHE_HOME:-$HOME/.cache}
receipts=${XDG_STATE_HOME:-$HOME/.local/state}/terminal-delight/receipts

die() {
  echo "terminal-delight: $1 Nothing was installed." >&2
  exit 1
}

fetch() {
  curl -fL --retry 3 --proto '=https,file' --proto-redir '=https' "$@"
}

place() {
  [ ! -e "$2" ] && [ ! -L "$2" ] || return 0
  install -Dm644 "$1" "$2"
  printf 'file\t%s\t%s\n' "$(sha256sum "$2" | cut -d ' ' -f 1)" "$2" >>"$stage"
}

# Everything that acts is in main, called on the last line: piped into sh, a
# download cut off partway runs nothing at all.
main() {
  [ "$(id -u)" -ne 0 ] || die "run this as your desktop user, without sudo."
  [ "$(uname -m)" = x86_64 ] || die "the AppImage is built for x86_64 only."
  for tool in curl sha256sum install; do
    command -v "$tool" >/dev/null || die "this needs $tool."
  done

  existing=$(command -v terminal-delight || true)
  if [ -e "$target" ] || [ -L "$target" ]; then
    existing=$target
  fi
  if [ -n "$existing" ]; then
    echo "Terminal Delight is already installed at $existing. Nothing was changed."
    echo "It updates from inside the app: CONFIG > UPDATES."
    exit 0
  fi

  # Staged in the cache rather than /tmp, which can be mounted noexec.
  mkdir -p "$cache"
  work=$(mktemp -d "$cache/td-install.XXXXXX")
  trap 'rm -rf "${work:?}"' EXIT
  trap 'exit 130' INT TERM

  # A release with no checksum is refused, the same rule the app's updater keeps.
  fetch -sS "$release/$asset.sha256" -o "$work/sum" ||
    die "no checksum could be fetched from $release."
  read -r want _ <"$work/sum" || true

  echo "Downloading Terminal Delight..."
  image=$work/$asset
  fetch --progress-bar "$release/$asset" -o "$image" ||
    die "the download from $release failed."
  got=$(sha256sum "$image" | cut -d ' ' -f 1)
  [ "$got" = "$want" ] || die "the download does not match its checksum."

  chmod +x "$image"
  version=$("$image" --version </dev/null 2>&1) ||
    die "the AppImage would not start here: $version."

  (
    cd "$work"
    "$image" --appimage-extract usr/share/applications
    "$image" --appimage-extract usr/share/icons
  ) >/dev/null ||
    die "the launcher entry could not be read out of the AppImage."

  # The receipt is how the uninstaller knows these files are ours. Anything
  # already at a destination stays, and stays out of the receipt.
  receipt=$receipts/desktop.receipt
  [ ! -L "$receipt" ] || die "$receipt is a symlink."
  (
    umask 077
    mkdir -p "$receipts"
  )
  stage=$work/receipt
  if [ -f "$receipt" ]; then
    cat "$receipt" >"$stage"
  else
    printf '# Terminal Delight install receipt: desktop\n' >"$stage"
  fi

  from=$work/squashfs-root/usr/share
  place "$from/applications/terminal-delight.desktop" "$data/applications/terminal-delight.desktop"
  for icon in scalable/apps/terminal-delight.svg 256x256/apps/terminal-delight.png; do
    place "$from/icons/hicolor/$icon" "$data/icons/hicolor/$icon"
  done
  mv -f "$stage" "$receipt"

  # The program goes in last, in one step: until this line an interrupted run
  # leaves nothing that looks installed, so running this again finishes the job.
  mkdir -p "$bin"
  install -m755 "$image" "$target.part"
  mv -f "$target.part" "$target"

  if command -v update-desktop-database >/dev/null; then
    update-desktop-database "$data/applications" 2>/dev/null || :
  fi
  if command -v gtk-update-icon-cache >/dev/null; then
    gtk-update-icon-cache -q -f -t "$data/icons/hicolor" 2>/dev/null || :
  fi

  echo "Installed $version at $target."
  case :$PATH: in
    *:"$bin":*) ;;
    *) echo "$bin is not on your PATH; add it so the terminal-delight command is found." ;;
  esac
}

main
