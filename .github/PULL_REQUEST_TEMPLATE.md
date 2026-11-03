<!-- Thanks for contributing! Keep it focused — one change per PR. -->

## What & why

<!-- What does this change, and what problem does it solve? -->

## Release

<!-- Read the current release section in RELEASING.md. Set the GitHub milestone as well as this text. -->
- Intended release: <!-- milestone name, or explicitly unassigned -->
- Merge target: <!-- actual base branch -->
- Release fit: <!-- why this belongs, or which scope decision is still needed -->

## Type

- [ ] Theme (new/updated `.toml`)
- [ ] Bug fix
- [ ] Feature
- [ ] Docs / infra

## Checklist

- [ ] Release intent is recorded with a milestone or explicitly unassigned, and the PR base matches the intended integration branch

- [ ] `cargo fmt -- --check`, `cargo clippy --locked -- -D warnings`, `cargo test --locked` all pass (in `app/`)
- [ ] No prebuilt binary is attached (releases are built by CI only — see `RELEASING.md`)
- [ ] User-visible? A line is added under `## [Unreleased]` in `CHANGELOG.md`
- [ ] Screenshot included for anything user-visible
- [ ] New on-screen text (a skin, a design, a button, a label)? It is in the language pack in all nine languages, or a follow-up says it isn't yet — `scripts/translation-suggest.py` lists what to add, and `docs/TRANSLATING.md` says how
- [ ] For code touching the terminal seam (`app/src/vt/`): wrote it from the `rio-vt` / `alacritty_terminal` sources and API docs, not Zed's GPL source (clean-room rule, `docs/PLAN.md` §2), and ran `cargo test --features core-alacritty` as well as `cargo test`
