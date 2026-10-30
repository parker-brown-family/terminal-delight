#!/usr/bin/env bash
# Shared conservative edits for the legacy integration installers. Callers use
# set -euo pipefail. This is not a complete application uninstaller.

td_cleanup_temps() {
  rm -f -- "${TD_EDIT_STAGE:-}" "${TD_EDIT_SNAPSHOT:-}" \
    "${TD_BACKUP_PENDING:-}" "${TD_RECEIPT_STAGE:-}"
}
trap td_cleanup_temps EXIT

td_state_dir() {
  printf '%s/terminal-delight' "${XDG_STATE_HOME:-$HOME/.local/state}"
}

td_receipt_init() {
  TD_COMPONENT=$1
  TD_RECEIPT="$(td_state_dir)/receipts/$TD_COMPONENT.receipt"
  [[ ! -L $TD_RECEIPT && ( ! -e $TD_RECEIPT || -f $TD_RECEIPT ) ]] || {
    echo "Refusing non-regular receipt: $TD_RECEIPT" >&2; return 1;
  }
}

td_receipt_begin() {
  local dir=${TD_RECEIPT%/*}
  [[ ! -L $dir ]] || { echo "Refusing symlink receipt directory: $dir" >&2; return 1; }
  (umask 077; mkdir -p -- "$dir")
  TD_RECEIPT_STAGE=$(mktemp "$dir/.${TD_COMPONENT}.XXXXXXXX")
  printf '# Terminal Delight install receipt: %s\n' "$TD_COMPONENT" > "$TD_RECEIPT_STAGE"
}

td_receipt_record() {
  local field
  for field in "$@"; do
    [[ $field != *$'\t'* && $field != *$'\n'* && $field != *$'\r'* ]] || {
      echo "Receipt fields cannot contain tabs or newlines" >&2; return 1;
    }
  done
  printf '%s\t%s\t%s\n' "$1" "$2" "$3" >> "$TD_RECEIPT_STAGE"
}

td_receipt_file() {
  [[ $1 == /* && -f $1 && ! -L $1 ]] || {
    echo "Receipt requires an absolute regular file: $1" >&2; return 1;
  }
  local digest
  digest=$(sha256sum < "$1")
  td_receipt_record file "${digest%% *}" "$1"
}

td_receipt_commit() { mv -T -- "$TD_RECEIPT_STAGE" "$TD_RECEIPT"; }
td_receipt_remove() { rm -f -- "$TD_RECEIPT"; }

td_edit_begin() {
  td_cleanup_temps
  TD_EDIT_PATH=$1
  [[ ! -L $TD_EDIT_PATH && -f $TD_EDIT_PATH ]] || {
    echo "Refusing non-regular or symlink settings: $TD_EDIT_PATH" >&2; return 1;
  }
  TD_EDIT_SNAPSHOT=$(mktemp "${TD_EDIT_PATH}.td-snapshot.XXXXXXXX")
  cat -- "$TD_EDIT_PATH" > "$TD_EDIT_SNAPSHOT"
  TD_EDIT_STAGE=$(mktemp "${TD_EDIT_PATH}.td-edit.XXXXXXXX")
}

td_edit_commit() {
  cmp -s -- "$TD_EDIT_PATH" "$TD_EDIT_SNAPSHOT" && [[ ! -L $TD_EDIT_PATH ]] || {
    echo "Settings changed during cleanup; retained $TD_EDIT_PATH" >&2; return 1;
  }
  if cmp -s -- "$TD_EDIT_STAGE" "$TD_EDIT_SNAPSHOT"; then
    rm -f -- "$TD_EDIT_STAGE" "$TD_EDIT_SNAPSHOT"
  else
    local dir
    dir="$(td_state_dir)/backups"
    [[ ! -L $dir ]] || { echo "Refusing symlink backup directory: $dir" >&2; return 1; }
    (umask 077; mkdir -p -- "$dir")
    TD_BACKUP_PENDING=$(mktemp "$dir/${TD_EDIT_PATH##*/}.$(date -u +%Y%m%dT%H%M%S).XXXXXXXX")
    cat -- "$TD_EDIT_SNAPSHOT" > "$TD_BACKUP_PENDING"
    chmod --reference="$TD_EDIT_PATH" "$TD_EDIT_STAGE"
    mv -T -- "$TD_EDIT_STAGE" "$TD_EDIT_PATH"
    local committed_backup=$TD_BACKUP_PENDING
    TD_BACKUP_PENDING=
    echo "Backup: $committed_backup"
    rm -f -- "$TD_EDIT_SNAPSHOT"
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
  local installed=$1 source=${2:-} digest known repo_path
  [[ -e $installed || -L $installed ]] || return 0
  if [[ ! -L $installed && -f $installed ]]; then
    digest=$(sha256sum < "$installed")
    digest=${digest%% *}
    if [[ -n ${TD_RECEIPT:-} && -f $TD_RECEIPT ]] &&
        awk -F '\t' -v hash="$digest" -v path="$installed" \
          '$1 == "file" && $2 == hash && $3 == path && NF == 3 { found=1 } END { exit !found }' "$TD_RECEIPT"; then
      rm -- "$installed"; return
    fi
    if [[ -n $source ]] && cmp -s -- "$installed" "$source"; then
      rm -- "$installed"; return
    fi
    known="${REPO}/app/src/uninstall/known_hashes.txt"
    repo_path=${source#"$REPO/"}
    if [[ -n $source && -f $known ]] && grep -qxF -- "$digest  $repo_path" "$known"; then
      rm -- "$installed"; return
    fi
  fi
  echo "Retained changed or unverified helper: $installed" >&2
  return 1
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
