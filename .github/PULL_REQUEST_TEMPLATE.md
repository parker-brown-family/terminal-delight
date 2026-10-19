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
- [ ] No prebuilt binary is attached (source-only — see `THIRD-PARTY-LICENSES.md`)
- [ ] Screenshot included for anything user-visible
- [ ] For code touching the terminal seam (`app/src/vt/`): wrote it from the `rio-vt` / `alacritty_terminal` sources and API docs, not Zed's GPL source (clean-room rule, `docs/PLAN.md` §2), and ran `cargo test --features core-alacritty` as well as `cargo test`
