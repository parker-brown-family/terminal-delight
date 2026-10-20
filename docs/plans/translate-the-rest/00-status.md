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
- [x] Batch 2 (main.rs chrome, 305 incl. additions) + one glossary; 231 help-menu corrections
      applied, 16 refused (the translated brand and the localized FOCUS name are decisions)
- [x] Batch 3a (modules, 218 + the four lost help-menu headers) collected from the tool outputs
      of a reply whose final message stalled; parts table 214/595 translated
- [ ] Batches 3b (224) and 4 (document viewer, 157), sent to two fresh Pro chats in parallel
      with the glossary and the 510 earlier translations attached as memory
- [ ] Shorten pass: rebuilt from `width.py` over the Rust tables once 3b and 4 land
      (55 fields more than 25% over budget after 3a, 26 of them Japanese)
- [x] Wiring: main.rs and every other module, merged; main.rs adopts the modules' display
      accessors; the notes control-socket report stays English (2498 tests pass)
- [x] The four stale help-menu fields re-translated; a test refuses English in a non-English
      flat field outside a 57-entry cognate allowlist (parts joins it when its batches land)
- [ ] Merge the three branches, regenerate all tables, full gates, one PR

## How the matrix moves

Batches are JSON files attached to a ChatGPT Pro conversation. Terminology stays consistent
inside one conversation; a fresh one gets `td-glossary-and-memory.json` (the agreed glossary
plus every earlier translation with its English) attached instead. A Pro reply can stall with
its final message stuck "in progress" for hours; its tool outputs usually hold the blocks, and
the conversation API (`/backend-api/conversation/<id>`) reads them when the page cannot. Replies are fenced json blocks headed `LANG: <code>`; a script in the page parses
them and a real click on an injected button copies them to the system clipboard, from where
`wl-paste` lands them in the scratchpad. `gen_rust.py` regenerates `app/src/lang/<table>.rs`
and refuses a translation that loses a placeholder, a keycap or a plural form.
