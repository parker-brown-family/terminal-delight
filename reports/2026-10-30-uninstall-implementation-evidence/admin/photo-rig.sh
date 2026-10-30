#!/usr/bin/env bash
# Photograph the ADMIN panel in a hidden window on a fixture HOME: once on a
# machine with no protection marker, once with a marker naming this machine.
# Hidden = a special workspace, no focus change, nothing on screen.
set -u
export PATH=/home/parker/bin:$PATH
export HYPRLAND_INSTANCE_SIGNATURE=$(ls -t /run/user/1000/hypr/ | head -1)
export WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-wayland-1} XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/1000}
TD=${TD:-$HOME/.cache/td-uninstall-cargo/debug/terminal-delight}
OUT=$HOME/.cache/td-uninstall-admin/photos
REPO=/home/parker/Work/td-uninstall-lab/terminal-delight
mkdir -p "$OUT"
source "$REPO/scripts/lib/hidden-window.sh"
trap 'hidden_cleanup' EXIT

shoot() {
  local tag=$1 protect=$2
  local F=$HOME/.cache/td-uninstall-admin/photo-home-$tag
  rm -rf "$F"; mkdir -p "$F/.config/terminal-delight/sessions" "$F/.local/lib/terminal-delight" "$F/.local/bin" "$F/.local/state" "$F/.cache"
  for n in td-0a1b2c3d-0.5-floor td-1b2c3d4e-0.5-admin td-2c3d4e5f-0.5-export td-3d4e5f6a-rc3 td-4e5f6a7b-rc2; do
    cp /usr/bin/sleep "$F/.local/lib/terminal-delight/$n"
  done
  ln -s "$F/.local/lib/terminal-delight/td-1b2c3d4e-0.5-admin" "$F/.local/bin/terminal-delight"
  echo 'theme = "default"' > "$F/.config/terminal-delight/theme.toml"
  [ "$protect" = yes ] && cp "$HOME/.config/terminal-delight/protected" "$F/.config/terminal-delight/protected"
  HIDDEN_SESSION_DIR="$F/.config/terminal-delight/sessions"
  local SESSION="adminrig$tag$$" WS="special:adminrig$tag$$"
  printf 'active = 0\n\n[[tabs]]\nname = "admin"\n\n[tabs.node.Leaf]\ncwd = "%s"\n' "$F" > "$HIDDEN_SESSION_DIR/$SESSION.toml"
  {
    echo "export HOME=$F TD_SESSION=$SESSION TD_SKIN_SCENE=admin XDG_CONFIG_HOME=$F/.config XDG_STATE_HOME=$F/.local/state XDG_CACHE_HOME=$F/.cache XDG_DATA_HOME=$F/.local/share"
    echo "exec $TD > $OUT/window-$tag.log 2>&1"
  } > "$OUT/launch-$tag.sh"
  hidden_launch "$SESSION" "$WS" "$OUT/window-$tag.log" "sh $OUT/launch-$tag.sh"
  echo "window $WIN ($tag)"
  sleep 4
  local STABLE
  STABLE=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .stableId' | head -1)
  timeout 15 grim -T "$STABLE" "$OUT/admin-$tag.png" && echo "photographed $OUT/admin-$tag.png"
  hidden_close
}

shoot open no
shoot protected yes
