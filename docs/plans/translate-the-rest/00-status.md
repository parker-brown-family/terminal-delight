# Translate the rest of the chrome — status

**Difficulty: 6/10.** Mechanical, but wide: a wrong placeholder, a broken plural or a German
label too wide for its tray column fails silently until someone switches language. Parker's
request is the plan (inventory everything, have ChatGPT Pro translate the *ideas*, wire it
into `lang.rs`), so no separate gate was taken.

## Decisions

- **The language pack stays the one mechanism.** Every new string is a `Strings` field in
  `app/src/lang.rs`, so the compiler refuses a language that is missing one.
- **Coordination findings stay English as data.** `Finding.headline` / `significance` /
  `limitations` are served to agents over MCP and copied by "Copy assessment context".
  The overlay re-renders each finding from its kind, condition and parameters in the
  reader's language; the stored English is untouched.
- **Ideas, not words, for the git surface.** The translator was told to render what the
  sentence means to someone deciding whether two agents will collide, not to gloss it.
- **Four strings had been relabelled in English and overwritten in every language**
  (`t_theme` DESIGN, `d_display` / `d_crt_hdr` GAUGES & CRT, `k_display_tray`). They go
  back through translation, and a test now refuses a non-English field equal to English
  outside a short cognate allowlist.

## Progress

- [x] Worktree `i18n-rest` from `origin/main`
- [ ] Inventory (git coordination, main.rs ×2, every other module)
- [ ] Translation matrix via ChatGPT Pro
- [ ] Wire strings + render-time finding sentences
- [ ] Gates: cargo test, clippy -D warnings, rustfmt
