#!/usr/bin/env bash
# Shared conservative edits for the legacy integration installers. Callers use
# set -euo pipefail. This is not a complete application uninstaller.

td_edit_begin() {
  TD_EDIT_PATH=$1
  [[ ! -L $TD_EDIT_PATH && -f $TD_EDIT_PATH ]] || {
    echo "Refusing non-regular or symlink settings: $TD_EDIT_PATH" >&2; return 1;
  }
  TD_EDIT_SNAPSHOT=$(mktemp "${TD_EDIT_PATH}.td-backup.XXXXXXXX")
  cp -p -- "$TD_EDIT_PATH" "$TD_EDIT_SNAPSHOT"
  TD_EDIT_STAGE=$(mktemp "${TD_EDIT_PATH}.td-edit.XXXXXXXX")
  trap 'rm -f -- "${TD_EDIT_STAGE:-}"' EXIT
}

td_edit_commit() {
  cmp -s -- "$TD_EDIT_PATH" "$TD_EDIT_SNAPSHOT" && [[ ! -L $TD_EDIT_PATH ]] || {
    echo "Settings changed during cleanup; retained $TD_EDIT_PATH" >&2; return 1;
  }
  if cmp -s -- "$TD_EDIT_STAGE" "$TD_EDIT_SNAPSHOT"; then
    rm -f -- "$TD_EDIT_STAGE" "$TD_EDIT_SNAPSHOT"
  else
    chmod --reference="$TD_EDIT_PATH" "$TD_EDIT_STAGE"
    mv -T -- "$TD_EDIT_STAGE" "$TD_EDIT_PATH"
    echo "Backup: $TD_EDIT_SNAPSHOT"
  fi
}

td_unhook() {
  local settings=$1 cmd=$2
  if [[ ! -e $settings && ! -L $settings ]]; then return 0; fi
  td_edit_begin "$settings"
  jq -es 'length == 1 and (.[0] | type == "object")' "$TD_EDIT_SNAPSHOT" >/dev/null || {
    echo "Retained settings and helper: expected one JSON settings object in $settings" >&2
    return 1
  }
  # Match the exact command written by these installers. Unknown wrappers,
  # old paths and lookalikes are retained and block removal of the helper.
  jq -e --arg cmd "$cmd" --arg name "${cmd##*/}" '
    def owned: .type == "command" and .command == $cmd;
    if type != "object" then error("settings must be an object") else . end |
    if has("hooks") then
      if (.hooks | type) != "object" then error("hooks must be an object") else . end |
      .hooks |= with_entries(.value |= (
        if type != "array" then error("hook event must be an array") else . end |
        map(
          if type != "object" or (.hooks | type) != "array"
          then error("hook group must contain an array") else . end |
          if any(.hooks[]; type != "object") then error("invalid nested hook") else . end |
          if any(.hooks[]; ((.command? // "") | type) != "string")
          then error("invalid hook command") else . end |
          if any(.hooks[]; ((.command? // "") | contains($name)) and (owned | not))
          then error("unrecognized reference to helper; review manually") else . end |
          if any(.hooks[]; owned) then
            .hooks |= map(select(owned | not)) | select(.hooks | length > 0)
          else . end
        )
      ))
    else . end
  ' "$TD_EDIT_SNAPSHOT" > "$TD_EDIT_STAGE"
  if jq -en --slurpfile before "$TD_EDIT_SNAPSHOT" --slurpfile after "$TD_EDIT_STAGE" '$before == $after' >/dev/null; then
    cp -p -- "$TD_EDIT_SNAPSHOT" "$TD_EDIT_STAGE"
  fi
  td_edit_commit
}

td_remove_helper() {
  local installed=$1 source=$2
  [[ -e $installed || -L $installed ]] || return 0
  if [[ -L $installed || ! -f $installed ]] || ! cmp -s -- "$installed" "$source"; then
    echo "Retained changed or unverified helper: $installed" >&2
    return 1
  fi
  rm -- "$installed"
}

td_strip_key_block() {
  local bindings=$1 dest=$2
  [[ -e $bindings || -L $bindings ]] || return 0
  td_edit_begin "$bindings"
  # Require one ordered pair and the exact generated contents. A missing close,
  # duplicate pair or user edit cannot turn into a range-to-end deletion.
  awk -v dest="$dest" '
    BEGIN { open="-- >>> td-keybindings hotkey >>>"; shut="-- <<< td-keybindings hotkey <<<" }
    $0 == open { starts++; inside=1; next }
    $0 == shut { ends++; if (!inside) bad=1; inside=0; next }
    inside {
      block[++n]=$0
      next
    }
    { print }
    END {
      if (!starts && !ends) exit 0
      if (starts != 1 || ends != 1 || inside || bad || n != 7 ||
        block[1] != "-- SUPER+ALT+T shows every Terminal Delight key in Omarchy\047s searchable key" ||
        block[2] != "-- menu, the way SUPER+K shows Omarchy\047s own and SUPER+CTRL+K shows Herdr\047s." ||
        block[3] != "-- The list is the docs site\047s Keys page, copied beside the script by" ||
        block[4] != "-- scripts/install-keys-hotkey.sh in the terminal-delight repo. The seat was" ||
        block[5] != "-- td-send\047s, whose hotkey is retired; T reads for Terminal Delight, and the K" ||
        block[6] != "-- family on SUPER, SUPER+ALT and SUPER+CTRL is all Omarchy\047s already." ||
        block[7] != "o.bind(\"SUPER + ALT + T\", \"Terminal Delight keybindings\", \"" dest "/td-keybindings\")") exit 1
    }
  ' "$TD_EDIT_SNAPSHOT" > "$TD_EDIT_STAGE" || {
    echo "Retained bindings and helpers: damaged, duplicate or edited TD block in $bindings" >&2
    return 1
  }
  # awk reconstructs lines; preserve a user's missing final newline too.
  if [[ -s $TD_EDIT_STAGE ]] && [[ $(tail -c 1 "$TD_EDIT_SNAPSHOT" | od -An -tu1 | tr -d '[:space:]') != 10 ]]; then
    truncate -s -1 "$TD_EDIT_STAGE"
  fi
  # A reference outside our block can still depend on the helper.
  if grep -qF 'td-keybindings' "$TD_EDIT_STAGE"; then
    echo "Retained bindings and helpers: other td-keybindings reference needs review" >&2; return 1
  fi
  td_edit_commit
}
