# Releasing Terminal Delight

Terminal Delight ships as one file, `terminal-delight-x86_64.AppImage`, attached
to a GitHub release. This page is how a release gets from `main` to the people
downloading it, and when.

## Two channels

| Channel | Tag | What it is | Who gets it |
|---|---|---|---|
| **Stable** | `v0.4.0` | The release the install page serves | Everyone following the install instructions (`releases/latest`) |
| **Candidate** | `v0.4.0-rc.1` | A preview of the next stable, marked *pre-release* on GitHub | People who pick it from the releases page on purpose |

`main` is where finished work is integrated. It is kept buildable and green, and
it is not a channel: nobody outside the project is pointed at it. A release is a
chosen, tested snapshot of it.

## Schedule

A **minor release every four weeks**, on Mondays:

1. **Week 0, Monday — candidate.** `vX.Y.0-rc.1` is tagged from `main`.
2. **Week 0 → 1 — hold.** The candidate is used for at least seven days and run
   through the checklist below. A fix for something the candidate broke lands on
   `main` (or on `release/X.Y` if `main` has moved on with unrelated work) and goes
   out as `rc.2`, which restarts the hold for whatever it touched.
3. **Week 1, Monday — stable**, if the newest candidate held and its checklist
   passed. Otherwise stable waits for one that does; the date moves, and the bar
   does not.

| Release | Candidate | Stable, at the earliest |
|---|---|---|
| 0.4.0 | 2026-10-20 | 2026-10-27 |
| 0.5.0 | 2026-11-13 | 2026-11-20 |
| 0.6.0 | 2026-12-14 | 2026-12-21 |

0.5.0 targets Friday, November 20 at Parker’s request on 2026-10-20. Its
candidate target moves to Friday, November 13 to preserve the seven-day hold.
This is an exception to the usual Monday schedule; the hold and checklist still
determine whether stable ships. Development and local experiments take place on
`dev/0.5.0`, focused on [agent-to-agent communication hardening](docs/plans/release-0.5.0/00-status.md).

0.4.0 is the first release cut this way. It goes out as soon as this process
lands, which is why its dates fall on a Tuesday.

**Patch releases** (`X.Y.1`, …) go out whenever a stable release has a regression
or a security problem worth not waiting four weeks for. They follow the same
path, candidate first, with a hold measured in hours rather than days when the
fix is small. They are cut from `release/X.Y`, so only the fix ships.

Security fixes follow [SECURITY.md](SECURITY.md): an embargoed problem is not
described in the changelog, the release notes or a commit message until its
disclosure date, even when the fix ships before it.

## What CI enforces

The `publish` job in [`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs
only on a version tag, and only after every check passed for that same commit:
Rust (both terminal cores), the browser prototype, supply chain, the release
policy and the AppImage build. It then applies
[`scripts/release-meta.sh`](scripts/release-meta.sh):

- The tag must be `vMAJOR.MINOR.PATCH` or `vMAJOR.MINOR.PATCH-rc.N`. Any other tag
  publishes nothing.
- The tagged commit must be on `main` or a `release/*` branch.
- `app/Cargo.toml`'s version must equal the tag, so the app reports the version
  its download page names.
- `docs/release-notes/vX.Y.Z.md` must exist. It becomes the release page.
- `CHANGELOG.md` must carry the release: a `## [X.Y.Z]` section for stable, or
  that or `## [Unreleased]` for a candidate.
- A **candidate** is published as a pre-release and never as *latest*.
- A **stable** tag needs a candidate of the same version before it, and may differ
  from the newest candidate only in `app/Cargo.toml`, `app/Cargo.lock`,
  `CHANGELOG.md`, `docs/release-notes/` and `docsite/`. Stable then ships the code
  the candidate was checked with. It is created as a **draft**, not *latest*,
  until a person promotes it.

Every release carries the AppImage's SHA-256 and the commit it was built from.
`bash scripts/test-release-meta.sh` proves the policy against fixtures. CI runs it
on every pull request, so the policy can change only by changing that test too.

## Cutting a candidate

From an up-to-date `main` with green CI:

1. **Changelog.** Every user-visible change since the last stable is under
   `## [Unreleased]` in [CHANGELOG.md](CHANGELOG.md). Add an *Upgrading from
   X.Y* note for anything an existing user has to do or will notice: a moved
   config file, a changed default, a removed key.
2. **Release notes.** Write `docs/release-notes/vX.Y.0.md`: a short page of
   highlights, install and upgrade steps, and thanks to outside contributors by
   name. The full list stays in the changelog. GitHub appends the merged pull
   requests on its own.
3. **Version.** Set `version = "X.Y.0-rc.1"` in `app/Cargo.toml`, then run
   `cargo check` in `app/` so `Cargo.lock` follows.
4. Land those as one pull request. Once it merges:

```bash
git fetch origin && git tag -a vX.Y.0-rc.1 origin/main -m "Terminal Delight X.Y.0-rc.1" && git push origin vX.Y.0-rc.1
```

5. Watch the run. If any check fails, nothing is published. Delete the tag, fix
   the problem on `main`, and tag again:

```bash
git push origin :refs/tags/vX.Y.0-rc.1 && git tag -d vX.Y.0-rc.1
```

## The candidate checklist

Run against the **downloaded candidate asset**, after checking its digest
(`sha256sum --check terminal-delight-x86_64.AppImage.sha256`), in a home
directory that has never run Terminal Delight:

- [ ] It starts on a clean home, X11 and Wayland, and the first window is usable.
- [ ] A shell takes input, scrolls and copies; a resize re-wraps.
- [ ] Quit and reopen: tabs, panes and their working directories come back.
- [ ] A Markdown file and an HTML brief open in the document square; a note on
      the brief is saved and read back.
- [ ] An agent in a pane reaches the MCP server and the workbench.
- [ ] **Upgrade:** over a home last run by the previous stable, it starts and
      keeps that home's layout and settings.
- [ ] `terminal-delight --version` prints the candidate's version.

Record the result in the candidate's release notes on GitHub, with the digest it
was run against. A failed item is either fixed in the next candidate or written
up as a known issue in the release notes. It is never left unmentioned.

## Promoting to stable

1. Rename `## [Unreleased]` to `## [X.Y.0] — YYYY-MM-DD` and open a new empty
   `## [Unreleased]` above it. Set `version = "X.Y.0"`. Update the version facts on
   the install page (`docsite/pages/install.html`). These are the only files
   allowed to differ from the candidate. Land them as one pull request.
2. Tag the merged commit:

```bash
git fetch origin && git tag -a vX.Y.0 origin/main -m "Terminal Delight X.Y.0" && git push origin vX.Y.0
```

3. CI creates a **draft**. Download its asset, check the digest, and confirm that
   it starts and reports `X.Y.0`.
4. Publish it as latest. This is the moment the install page changes:

```bash
gh release edit vX.Y.0 --draft=false --latest
```

## Versions

Terminal Delight is `0.x`: a minor release may change behaviour that someone
relied on, and its changelog says so under *Upgrading*. Patch releases only fix.
From `1.0` the project follows [Semantic Versioning](https://semver.org/).
