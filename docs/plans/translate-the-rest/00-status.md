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
- [x] Inventory: 229 git coordination · 309 main.rs · 690 every other module (~1,230)
- [x] Language pack: `Plural` (CLDR forms), one-pass `fill`, sub-tables `gc` / `ui` / `parts`
- [x] Git coordination overlay, HUD chip and footer wired; findings carry `meaning::Line` /
      `attention::Why` keys so MCP keeps its English
- [x] Batch 1 (git coordination, 216) translated by ChatGPT Pro, validated, committed
- [ ] Batch 2 (main.rs chrome, 297) + glossary + fixes — sent
- [ ] Batches 3a / 3b (modules, 223 + 224) and 4 (document viewer, 228)
- [ ] Wiring: main.rs (worktree `i18n-wire-main`), other modules (`i18n-wire-parts`)
- [ ] The four stale help-menu fields re-translated; a test refuses English in a non-English
      flat field outside a cognate allowlist
- [ ] Merge the three branches, regenerate all tables, full gates, one PR

## How the matrix moves

Batches are JSON files attached to one ChatGPT Pro conversation (terminology stays consistent
inside it). Replies are fenced json blocks headed `LANG: <code>`; a script in the page parses
them and a real click on an injected button copies them to the system clipboard, from where
`wl-paste` lands them in the scratchpad. `gen_rust.py` regenerates `app/src/lang/<table>.rs`
and refuses a translation that loses a placeholder, a keycap or a plural form.
