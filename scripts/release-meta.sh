#!/usr/bin/env bash
# Decide what a version tag publishes, and refuse a tag that is not ready.
#
#   scripts/release-meta.sh <tag> [repo-root]
#
# Prints `key=value` lines for $GITHUB_OUTPUT and writes the release body to
# <repo-root>/dist/release-body.md. Exits non-zero, saying why, when the tag
# must not publish. CI's publish job runs it after every check has passed for
# the tagged commit; scripts/test-release-meta.sh runs it against fixtures, so
# the policy can be proven without pushing a tag anywhere.
#
# The policy (RELEASING.md is the long form):
#
#   v0.4.0-rc.1  a CANDIDATE. Published as a prerelease, never "latest", so the
#                install page's releases/latest URL keeps serving stable.
#   v0.4.0       STABLE. Created as a DRAFT, never "latest" on creation. A person
#                checks the exact asset (digest in the body) and promotes it with
#                `gh release edit v0.4.0 --draft=false --latest`.
#   anything     refused. A tag that is not vMAJOR.MINOR.PATCH or
#   else         vMAJOR.MINOR.PATCH-rc.N publishes nothing.
#
# Both kinds require:
#   - app/Cargo.toml's version to equal the tag without its `v`, so the binary
#     reports the version its download page names;
#   - docs/release-notes/v<MAJOR.MINOR.PATCH>.md, the short human page that
#     becomes the release body (one file per version, shared by its candidates);
#   - CHANGELOG.md to carry the release: `## [X.Y.Z]` for stable; for a
#     candidate, `## [X.Y.Z]` or `## [Unreleased]`.
#
# A stable tag also requires a candidate of the same version before it, and may
# differ from the newest one only in what names the release: the version lines,
# the changelog, the release notes and the website. Everything a candidate was
# checked for is then what stable ships; a fix found in a candidate goes out as
# the next candidate, not straight to stable. The tagged commit is HEAD of
# <repo-root>, which is what CI checks out.
set -euo pipefail

tag="${1:?usage: release-meta.sh <tag> [repo-root]}"
root="${2:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"

refuse() {
    echo "release-meta: refusing $tag: $*" >&2
    exit 1
}

if [[ "$tag" =~ ^v([0-9]+\.[0-9]+\.[0-9]+)(-rc\.([0-9]+))?$ ]]; then
    base="${BASH_REMATCH[1]}"
    rc="${BASH_REMATCH[3]}"
else
    refuse "not vMAJOR.MINOR.PATCH or vMAJOR.MINOR.PATCH-rc.N"
fi
version="${tag#v}"

cargo_version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/app/Cargo.toml" | head -n1)"
[ "$cargo_version" = "$version" ] \
    || refuse "app/Cargo.toml says version \"$cargo_version\", the tag says \"$version\""

notes="$root/docs/release-notes/v$base.md"
[ -s "$notes" ] || refuse "no release notes at docs/release-notes/v$base.md"

changelog="$root/CHANGELOG.md"
[ -f "$changelog" ] || refuse "no CHANGELOG.md"
base_re="${base//./\\.}"
if grep -Eq "^## \[$base_re\]" "$changelog"; then
    :
elif [ -n "$rc" ] && grep -Eq '^## \[Unreleased\]' "$changelog"; then
    :
elif [ -n "$rc" ]; then
    refuse "CHANGELOG.md has neither ## [$base] nor ## [Unreleased]"
else
    refuse "CHANGELOG.md has no ## [$base] section; a stable release names its own"
fi

if [ -z "$rc" ]; then
    git -C "$root" rev-parse --git-dir >/dev/null 2>&1 \
        || refuse "$root is not a git checkout, so its candidate cannot be found"
    last_rc="$(git -C "$root" tag --list "v$base-rc.*" --sort=-v:refname | head -n1)"
    [ -n "$last_rc" ] || refuse "no v$base-rc.N candidate was tagged before it"
    beyond="$(git -C "$root" diff --name-only "$last_rc" HEAD \
        | grep -Ev '^(app/Cargo\.(toml|lock)|CHANGELOG\.md|docs/release-notes/|docsite/)' || true)"
    [ -z "$beyond" ] || refuse "differs from $last_rc beyond version and notes: $(echo "$beyond" | tr '\n' ' ')"
    echo "candidate=$last_rc"
fi

mkdir -p "$root/dist"
body="$root/dist/release-body.md"
{
    if [ -n "$rc" ]; then
        printf '> **Release candidate %s of %s.** A preview for people who want to try it\n' "$rc" "$base"
        printf '> early. It is not the stable download; the install page keeps serving the\n'
        printf '> latest stable release. Report problems in the issue tracker.\n\n'
    fi
    cat "$notes"
    printf '\n\nFull notes: [CHANGELOG.md at %s](https://github.com/%s/blob/%s/CHANGELOG.md)\n' \
        "$tag" "${GITHUB_REPOSITORY:-parker-brown-family/terminal-delight}" "$tag"
} >"$body"

if [ -n "$rc" ]; then
    echo "kind=candidate"
    echo "prerelease=true"
    echo "draft=false"
else
    echo "kind=stable"
    echo "prerelease=false"
    echo "draft=true"
fi
# Never latest on creation: a candidate is never latest, and a stable release
# becomes latest only when a person promotes the checked draft.
echo "make_latest=false"
echo "version=$version"
echo "body_path=$body"
