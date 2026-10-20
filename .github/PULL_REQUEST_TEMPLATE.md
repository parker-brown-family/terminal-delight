<!-- Thanks for contributing! Keep it focused — one change per PR. -->

## What & why

<!-- What does this change, and what problem does it solve? -->

## Type

- [ ] Theme (new/updated `.toml`)
- [ ] Bug fix
- [ ] Feature
- [ ] Docs / infra

## Checklist

- [ ] `cargo fmt -- --check`, `cargo clippy --locked -- -D warnings`, `cargo test --locked` all pass (in `app/`)
- [ ] No prebuilt binary is attached (releases are built by CI only — see `RELEASING.md`)
- [ ] User-visible? A line is added under `## [Unreleased]` in `CHANGELOG.md`
- [ ] Screenshot included for anything user-visible
- [ ] New on-screen text (a skin, a design, a button, a label)? It is in the language pack in all nine languages, or a follow-up says it isn't yet — `scripts/translation-suggest.py` lists what to add, and `docs/TRANSLATING.md` says how
- [ ] For code touching the terminal seam (`app/src/vt/`): wrote it from the `rio-vt` / `alacritty_terminal` sources and API docs, not Zed's GPL source (clean-room rule, `docs/PLAN.md` §2), and ran `cargo test --features core-alacritty` as well as `cargo test`
