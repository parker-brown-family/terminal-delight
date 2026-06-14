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
- [ ] For code touching the terminal seam: wrote it from `alacritty_terminal` API docs, not Zed's GPL source (clean-room rule, `docs/PLAN.md` §2)
