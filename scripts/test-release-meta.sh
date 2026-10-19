#!/usr/bin/env bash
# Prove the release policy in scripts/release-meta.sh against fixture trees,
# without pushing a tag anywhere. CI runs this on every pull request.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
meta="$here/release-meta.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
fails=0

git_quiet() { git -C "$1" -c user.name=fixture -c user.email=fixture@example.invalid "${@:2}" >/dev/null; }

# A fixture repo root, committed: Cargo version, an optional notes file, a
# changelog. Calling it again on the same dir rewrites those and commits again.
fixture() {
    local dir="$1" version="$2" heading="$3" notes="${4:-yes}"
    mkdir -p "$dir/app" "$dir/docs/release-notes"
    [ -d "$dir/.git" ] || git_quiet "$dir" init -q
    printf '[package]\nname = "terminal-delight"\nversion = "%s"\n' "$version" >"$dir/app/Cargo.toml"
    printf '# Changelog\n\n%s\n\n- a change\n' "$heading" >"$dir/CHANGELOG.md"
    if [ "$notes" = yes ]; then
        local base="${version%%-*}"
        printf 'Terminal Delight %s highlights.\n' "$base" >"$dir/docs/release-notes/v$base.md"
    fi
    git_quiet "$dir" add -A
    git_quiet "$dir" commit -q --allow-empty -m "fixture $version"
}

# Stable trees: a candidate tagged, then only the version and notes moved on.
stable_fixture() {
    local dir="$1"
    fixture "$dir" "0.4.0-rc.1" "## [Unreleased]"
    git_quiet "$dir" tag v0.4.0-rc.1
    fixture "$dir" "0.4.0" "## [0.4.0] — 2026-11-02"
}

expect_ok() {
    local name="$1" tag="$2" dir="$3"; shift 3
    local out
    if ! out="$("$meta" "$tag" "$dir" 2>&1)"; then
        echo "FAIL $name: refused: $out"; fails=$((fails + 1)); return
    fi
    for want in "$@"; do
        grep -qx "$want" <<<"$out" || { echo "FAIL $name: missing '$want' in: $out"; fails=$((fails + 1)); return; }
    done
    echo "ok   $name"
}

expect_refused() {
    local name="$1" tag="$2" dir="$3" why="$4"
    local out
    if out="$("$meta" "$tag" "$dir" 2>&1)"; then
        echo "FAIL $name: published, wanted a refusal ($why): $out"; fails=$((fails + 1)); return
    fi
    grep -q "$why" <<<"$out" || { echo "FAIL $name: refused for the wrong reason: $out"; fails=$((fails + 1)); return; }
    echo "ok   $name"
}

fixture "$work/rc" "0.4.0-rc.1" "## [Unreleased]"
expect_ok "a candidate is a prerelease and never latest" v0.4.0-rc.1 "$work/rc" \
    kind=candidate prerelease=true draft=false make_latest=false version=0.4.0-rc.1
if grep -q "Release candidate 1 of 0.4.0" "$work/rc/dist/release-body.md" &&
    grep -q "0.4.0 highlights" "$work/rc/dist/release-body.md"; then
    echo "ok   a candidate's body says it is a candidate, then the notes"
else
    echo "FAIL candidate body"; fails=$((fails + 1))
fi

stable_fixture "$work/stable"
expect_ok "a stable release is a draft and not latest until promoted" v0.4.0 "$work/stable" \
    kind=stable prerelease=false draft=true make_latest=false version=0.4.0 candidate=v0.4.0-rc.1
if grep -q "Release candidate" "$work/stable/dist/release-body.md"; then
    echo "FAIL a stable body carries the candidate banner"; fails=$((fails + 1))
else
    echo "ok   a stable body carries no candidate banner"
fi

fixture "$work/unreleased" "0.4.0" "## [Unreleased]"
expect_refused "a stable release needs its own changelog section" v0.4.0 "$work/unreleased" "no ## \[0.4.0\]"

fixture "$work/nocandidate" "0.4.0" "## [0.4.0]"
expect_refused "a stable release needs a candidate first" v0.4.0 "$work/nocandidate" "no v0.4.0-rc.N candidate"

stable_fixture "$work/drifted"
mkdir -p "$work/drifted/app/src" && echo 'fn main() {}' >"$work/drifted/app/src/main.rs"
git_quiet "$work/drifted" add -A && git_quiet "$work/drifted" commit -q -m "a fix after the candidate"
expect_refused "stable ships only what its candidate was checked for" v0.4.0 "$work/drifted" "beyond version and notes: app/src/main.rs"

stable_fixture "$work/latest-rc"
git_quiet "$work/latest-rc" tag v0.4.0-rc.10 "$(git -C "$work/latest-rc" rev-list -n1 HEAD)"
git_quiet "$work/latest-rc" tag v0.4.0-rc.9 v0.4.0-rc.1
expect_ok "the newest candidate is rc.10, not rc.9" v0.4.0 "$work/latest-rc" candidate=v0.4.0-rc.10

fixture "$work/mismatch" "0.3.0" "## [Unreleased]"
expect_refused "the binary must report the tagged version" v0.4.0-rc.1 "$work/mismatch" 'says version "0.3.0"'

fixture "$work/nonotes" "0.4.0-rc.1" "## [Unreleased]" no
expect_refused "a release needs its notes page" v0.4.0-rc.1 "$work/nonotes" "no release notes"

fixture "$work/odd" "0.4.0" "## [0.4.0]"
for tag in v0.4 v0.4.0-beta.1 v0.4.0-rc1 0.4.0 v0.4.0-rc.1-hotfix vfoo; do
    expect_refused "a tag outside the scheme publishes nothing ($tag)" "$tag" "$work/odd" "not vMAJOR"
done

# The real tree must at least parse: its version is a version this script accepts.
real_version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$here/../app/Cargo.toml" | head -n1)"
if [[ "v$real_version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-rc\.[0-9]+)?$ ]]; then
    echo "ok   app/Cargo.toml's version ($real_version) is in the tag scheme"
else
    echo "FAIL app/Cargo.toml's version ($real_version) could never be tagged"; fails=$((fails + 1))
fi

if [ "$fails" -gt 0 ]; then
    echo "$fails release-policy check(s) failed" >&2
    exit 1
fi
echo "release policy: all checks passed"
