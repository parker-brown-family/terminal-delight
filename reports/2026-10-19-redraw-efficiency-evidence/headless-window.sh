# Scoped test adapter: reuse the production fixture helper and add a verified
# headless monitor launch rule. No physical display is awakened.
. "$ROOT/scripts/lib/hidden-window.sh"
_original_launch=$(declare -f hidden_launch)
_original_launch=${_original_launch/hidden_launch ()/hidden_launch_original ()}
eval "$_original_launch"
unset _original_launch
hidden_launch() {
  hyprctl eval "hl.workspace_rule({workspace=\"$2\", monitor=\"HEADLESS-1\"})" >/dev/null
  hidden_launch_original "$@"
  local m
  m=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[]|select(.pid==$p)|.monitor')
  [ "$(hyprctl monitors -j | jq -r --argjson m "$m" '.[]|select(.id==$m)|.name')" = HEADLESS-1 ] || { echo 'wrong monitor'; exit 4; }
}
