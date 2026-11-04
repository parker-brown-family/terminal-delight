#!/usr/bin/env bash
# install.sh against a fake release: a shell script standing in for the AppImage,
# served from a directory, under a scratch HOME. Nothing leaves the scratch tree.
set -euo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/td-install-test.XXXXXX")
trap 'rm -rf "${scratch:?}"' EXIT
asset=terminal-delight-x86_64.AppImage
passed=0

make_release() { # <dir> <--version exit status>
  mkdir -p "$1"
  cat >"$1/$asset" <<FAKE
#!/bin/sh
case \$1 in
  --version) echo "terminal-delight 9.9.9"; exit $2 ;;
  --appimage-extract)
    root=squashfs-root/usr/share
    mkdir -p "\$root/applications" "\$root/icons/hicolor/scalable/apps" "\$root/icons/hicolor/256x256/apps"
    echo '[Desktop Entry]' >"\$root/applications/terminal-delight.desktop"
    echo svg >"\$root/icons/hicolor/scalable/apps/terminal-delight.svg"
    echo png >"\$root/icons/hicolor/256x256/apps/terminal-delight.png"
    echo "squashfs-root/\$2" ;;
esac
FAKE
  (cd "$1" && sha256sum "$asset" >"$asset.sha256")
}

good=$scratch/release/good
make_release "$good" 0
make_release "$scratch/release/broken" 1
make_release "$scratch/release/nosum" 0
rm "${scratch:?}/release/nosum/${asset:?}.sha256"
make_release "$scratch/release/wrongsum" 0
printf '%064d  %s\n' 0 "$asset" >"$scratch/release/wrongsum/$asset.sha256"
make_release "$scratch/release/badsum" 0
echo 'not found' >"$scratch/release/badsum/$asset.sha256"

stubs=$scratch/no-stubs
mkdir -p "$stubs"
# Only what install.sh uses, so a terminal-delight on this machine's own PATH
# cannot answer for the scratch home.
tools=$scratch/tools
mkdir -p "$tools"
for tool in cat chmod curl cut dirname grep id install mkdir mktemp mv rm sh sha256sum uname; do
  ln -s "$(command -v "$tool")" "$tools/$tool"
done

# `run <home> <release>`: leaves the output in $out and the status in $status.
run() {
  local home=$1 release=$2
  mkdir -p "$home"
  status=0
  out=$(env -i HOME="$home" PATH="$stubs:$tools" TD_RELEASE_URL="file://$release" sh "$repo/install.sh" 2>&1) || status=$?
}

fail() {
  echo "FAIL: $1" >&2
  echo "$out" >&2
  exit 1
}
ok() {
  passed=$((passed + 1))
  echo "ok - $1"
}
tree() { (cd "$1" && find . -type f -o -type l | LC_ALL=C sort | xargs -r sha256sum); }
nothing_installed() {
  [[ ! -e $1/.local/bin/terminal-delight ]] || fail "$2: the program is there"
  [[ ! -e $1/.local/share/applications/terminal-delight.desktop ]] || fail "$2: the entry is there"
  [[ -z $(ls -A "$1/.cache" 2>/dev/null) ]] || fail "$2: staging left behind"
}

home=$scratch/fresh
run "$home" "$good"
[[ $status == 0 ]] || fail "fresh install failed"
[[ -x $home/.local/bin/terminal-delight ]] || fail "program not executable"
cmp -s "$good/$asset" "$home/.local/bin/terminal-delight" || fail "program differs from the release"
[[ $out == *"Installed terminal-delight 9.9.9"* ]] || fail "no version reported"
[[ -z $(ls -A "$home/.cache") ]] || fail "staging left behind"
receipt=$home/.local/state/terminal-delight/receipts/desktop.receipt
for f in applications/terminal-delight.desktop icons/hicolor/scalable/apps/terminal-delight.svg icons/hicolor/256x256/apps/terminal-delight.png; do
  path=$home/.local/share/$f
  [[ -f $path ]] || fail "$f missing"
  want=$(sha256sum "$path" | cut -d ' ' -f 1)
  grep -qxF "$(printf 'file\t%s\t%s' "$want" "$path")" "$receipt" || fail "$f not in the receipt"
done
[[ $(stat -c %a "$(dirname "$receipt")") == 700 ]] || fail "receipt directory is open"
ok "a fresh install places the program, the entry, the icons and their receipt"

before=$(tree "$home")
run "$home" "$good"
[[ $status == 0 && $out == *"already installed"* ]] || fail "second run did not say so"
[[ $(tree "$home") == "$before" ]] || fail "second run changed something"
ok "a second run changes nothing"

home=$scratch/theirs
mkdir -p "$home/.local/bin"
echo 'their build' >"$home/.local/bin/terminal-delight"
run "$home" "$good"
[[ $status == 0 && $out == *"already installed"* ]] || fail "existing program not reported"
[[ $(cat "$home/.local/bin/terminal-delight") == "their build" ]] || fail "existing program replaced"
[[ ! -e $home/.local/share/applications ]] || fail "entry written beside an existing program"
ok "a program already there is left alone"

home=$scratch/link
mkdir -p "$home/.local/bin"
ln -s /nowhere "$home/.local/bin/terminal-delight"
run "$home" "$good"
[[ $status == 0 && $(readlink "$home/.local/bin/terminal-delight") == /nowhere ]] || fail "dangling launcher replaced"
ok "a dangling launcher is left alone"

home=$scratch/onpath
mkdir -p "$scratch/elsewhere"
printf '#!/bin/sh\n' >"$scratch/elsewhere/terminal-delight"
chmod +x "$scratch/elsewhere/terminal-delight"
keep=$stubs
stubs=$scratch/elsewhere
run "$home" "$good"
stubs=$keep
[[ $status == 0 && $out == *"$scratch/elsewhere/terminal-delight"* ]] || fail "copy on PATH not reported"
nothing_installed "$home" "a copy elsewhere on PATH"
ok "a copy elsewhere on PATH is left alone"

for case in nosum wrongsum badsum broken; do
  home=$scratch/$case
  run "$home" "$scratch/release/$case"
  [[ $status != 0 ]] || fail "$case: exit 0"
  [[ $out == *"Nothing was installed."* ]] || fail "$case: no refusal"
  nothing_installed "$home" "$case"
  ok "$case installs nothing"
done

home=$scratch/gone
run "$home" "$scratch/release/absent"
[[ $status != 0 ]] || fail "absent release: exit 0"
nothing_installed "$home" "absent release"
ok "an unreachable release installs nothing"

home=$scratch/root
stubs=$scratch/root-stubs
mkdir -p "$stubs"
printf '#!/bin/sh\necho 0\n' >"$stubs/id"
chmod +x "$stubs/id"
run "$home" "$good"
stubs=$keep
[[ $status != 0 && $out == *"without sudo"* ]] || fail "root not refused"
nothing_installed "$home" "root"
ok "root is refused"

home=$scratch/edited
entry=$home/.local/share/applications/terminal-delight.desktop
mkdir -p "$(dirname "$entry")"
echo 'their entry' >"$entry"
receipt=$home/.local/state/terminal-delight/receipts/desktop.receipt
mkdir -p "$(dirname "$receipt")"
printf '# Terminal Delight install receipt: desktop\nfile\tabc\t/earlier/file\n' >"$receipt"
run "$home" "$good"
[[ $status == 0 ]] || fail "install beside an existing entry failed"
[[ $(cat "$entry") == "their entry" ]] || fail "existing entry replaced"
! grep -qF "$entry" "$receipt" || fail "their entry claimed in the receipt"
grep -qF '/earlier/file' "$receipt" || fail "earlier receipt lost"
grep -qF 'terminal-delight.svg' "$receipt" || fail "new icon not recorded"
ok "an existing entry stays theirs, and an earlier receipt survives"

home=$scratch/piped
mkdir -p "$home"
status=0
out=$(cat "$repo/install.sh" | env -i HOME="$home" PATH="$stubs:$tools" TD_RELEASE_URL="file://$good" sh 2>&1) || status=$?
[[ $status == 0 && -x $home/.local/bin/terminal-delight ]] || fail "piped into sh: not installed"
ok "piped into sh, it installs"

size=$(wc -c <"$repo/install.sh")
for percent in 10 30 50 70 90 99; do
  home=$scratch/cut-$percent
  mkdir -p "$home"
  status=0
  out=$(head -c $((size * percent / 100)) "$repo/install.sh" |
    env -i HOME="$home" PATH="$stubs:$tools" TD_RELEASE_URL="file://$good" sh 2>&1) || status=$?
  nothing_installed "$home" "cut at $percent%"
  [[ -z $(ls -A "$home") ]] || fail "cut at $percent%: wrote into the home"
done
ok "a download cut off partway runs nothing"

home=$scratch/beside
mkdir -p "$home/.local/bin"
echo 'theirs' >"$home/.local/bin/terminal-delight.part"
run "$home" "$good"
[[ $status == 0 ]] || fail "install beside a .part file failed"
[[ $(cat "$home/.local/bin/terminal-delight.part") == theirs ]] || fail "a file beside the program was overwritten"
ok "a file beside the program is left alone"

# A run that dies at each copy, then a run that finishes the job. `install`
# fails when handed the file named in the stub, as a full disk would.
real_install=$(command -v install)
copy=0
for victim in terminal-delight.desktop terminal-delight.svg terminal-delight.png "$asset"; do
  copy=$((copy + 1))
  # Numbered, because a home named after the victim would fail every copy.
  home=$scratch/died-$copy
  stubs=$scratch/stubs-$copy
  mkdir -p "$stubs"
  cat >"$stubs/install" <<STUB
#!/bin/sh
case "\$*" in *$victim*) exit 1 ;; esac
exec $real_install "\$@"
STUB
  chmod +x "$stubs/install"
  run "$home" "$good"
  stubs=$keep
  [[ $status != 0 ]] || fail "$victim: the failing copy went unnoticed"
  [[ ! -e $home/.local/bin/terminal-delight ]] || fail "$victim: the program is there after a failed run"
  placed=$(find "$home/.local/share" -name 'terminal-delight.*' 2>/dev/null | wc -l)
  [[ $placed == $((copy > 3 ? 3 : copy - 1)) ]] || fail "$victim: died after $placed copies, not at this one"
  [[ -z $(ls -A "$home/.cache") ]] || fail "$victim: staging left behind"
  [[ -z $(find "$home" -name '.td-install.*') ]] || fail "$victim: a partial copy left behind"
  run "$home" "$good"
  [[ $status == 0 && -x $home/.local/bin/terminal-delight ]] || fail "$victim: the second run did not finish"
  receipt=$home/.local/state/terminal-delight/receipts/desktop.receipt
  for f in applications/terminal-delight.desktop icons/hicolor/scalable/apps/terminal-delight.svg icons/hicolor/256x256/apps/terminal-delight.png; do
    [[ -f $home/.local/share/$f ]] || fail "$victim: $f missing after the second run"
    [[ $(grep -cF "$home/.local/share/$f" "$receipt") == 1 ]] || fail "$victim: $f not recorded exactly once"
  done
  ok "dying at $victim, then running again, finishes with everything claimed once"
done

echo "$passed passed"
