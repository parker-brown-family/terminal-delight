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
- [x] Batches 3b (224) and 4 (document viewer, 157), from two fresh Pro chats run in parallel
      with the glossary and the 510 earlier translations attached as memory; parts 596/596
- [x] Shorten pass done by hand (taste delegated): 20 shortened, 39 still >25% over a soft
      budget; three doubled prefixes fixed (✔ on SUBMIT/NEXT, `default=`, the D tile)
- [x] Wiring: main.rs and every other module, merged; main.rs adopts the modules' display
      accessors; the notes control-socket report stays English (2498 tests pass)
- [x] The four stale help-menu fields re-translated; a test refuses English in a non-English
      flat field outside a 94-entry cognate allowlist, all four tables
- [x] Merged origin/main (63 commits, 21 conflicts; main's new strings translated), full
      gates on both cores, German and Japanese photographed in a hidden window
- [x] One PR: https://github.com/parker-brown-family/terminal-delight/pull/1033
- [x] OUTER is "outer" in every language (Parker, 2026-10-19: window is a reserved word)

Post-hoc difficulty: 6/10 held. What made it longer than the score was the browser
pipeline (two extension disconnects, a Pro reply stalled for five hours) and main moving
63 commits underneath, not the translation design.

## How the matrix moves

Batches are JSON files attached to a ChatGPT Pro conversation. Terminology stays consistent
inside one conversation; a fresh one gets `td-glossary-and-memory.json` (the agreed glossary
plus every earlier translation with its English) attached instead. A Pro reply can stall with
its final message stuck "in progress" for hours; its tool outputs usually hold the blocks, and
the conversation API (`/backend-api/conversation/<id>`) reads them when the page cannot. Replies are fenced json blocks headed `LANG: <code>`; a script in the page parses
them and a real click on an injected button copies them to the system clipboard, from where
`wl-paste` lands them in the scratchpad. `gen_rust.py` regenerates `app/src/lang/<table>.rs`
and refuses a translation that loses a placeholder, a keycap or a plural form.
