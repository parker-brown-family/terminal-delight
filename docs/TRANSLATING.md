# Translating Terminal Delight

Terminal Delight draws every word a person reads in the language they picked: English, Spanish, German, French, Russian, Simplified Chinese, Japanese, Korean or Hindi. This page is what to do when a change adds or changes on-screen text, such as a new skin, a new design, a button, a label, a hint or an error a person sees.

## When this applies

Any string a person reads on screen. Text that a program, an agent or a log reads stays English: MCP answers, the control socket, the CLI, clipboard text written for an agent ("Copy assessment context"), log lines, saved state, ids, and anything compared as a string. If you are unsure, ask whether a person or a program reads it.

Every pull request runs **Translation suggestions** in CI. It finds on-screen English the change adds, and any new skin or design that has no caption in the pack. It marks them on the changed lines and lists them in the run summary. It never fails: add the strings in the same pull request if you can, or file a follow-up if you can't. To run it locally before you push:

```
python3 scripts/translation-suggest.py
```

## Where the words live

| File | What it holds |
|---|---|
| `app/src/lang.rs` | The `Strings` struct, one `const` per language (`EN ES DE FR RU ZH JA KO HI`), the help menu, `Plural`, `fill`, and the tests |
| `app/src/lang/chrome.rs` | Window chrome drawn by `main.rs`: the paint tray, menus, dialogs, panels, the left bar (`strings().ui`) |
| `app/src/lang/parts.rs` | Everything drawn by the other modules: pane, workbench, document viewer, vitals, notes (`strings().parts`) |
| `app/src/lang/coord.rs` | The Git coordination overlay (`strings().gc`) |

The tables were generated once and are edited by hand from here.

## Adding a string

1. Add a field to the struct, with a `///` line saying where it is drawn and anything a translator needs (how much room it has, what it must not imply).
2. Give it a value in all nine `const` tables. The compiler refuses a table that is missing it.
3. Draw it from the pack: `lang::current().strings().ui.<field>` in `main.rs`, `.parts.<field>` in other modules, or `self.lang.strings()` where the view holds its language.
4. Counts use a `Plural` (`one`, `few`, `many`, `other`) and `lang.count(&p, n)`. Russian needs real `one`/`few`/`many` forms; Spanish, German, French and Hindi use `one`/`other`; Chinese, Japanese and Korean have no plural, so the same string goes in every slot.
5. Values go in with `lang::fill(template, &[("name", value)])`, which fills in one pass. Placeholders such as `{n}` and `{path}` must appear in every translation; move them wherever the grammar wants them.
6. A new skin or design also needs its caption arm in the picker's caption match in `main.rs` (beside `"chalk" => t.ui.pal_skin_chalk`), and a help line.

`no_translation_is_left_as_its_english_unless_it_is_a_cognate` fails if a non-English value is identical to its English. When the match is right (a cognate such as CRT or Plugins, a Git word developers keep such as HEAD, a name such as ELI5), add the field to `SAME_AS_ENGLISH` in `lang.rs`, with a reason if it isn't obvious. A name that isn't a word in other languages is still translated: the OPS skin is a command deck, so its caption is the word for command in each language.

## Wording

- **Translate the idea, not the words.** Write what a native-speaking developer expects from a well-localized pro tool. Keep hedges exactly as strong as the English: "unconfirmed", "unknown" and "not established" never become firmer or softer.
- **ALL-CAPS English is a small-caps header.** Use uppercase in Spanish, German, French and Russian; the other scripts just translate.
- **Room is real.** Chips and headers are narrow. Prefer the short natural word, and count a Chinese, Japanese or Korean character as two cells.
- **Never repeat what the code draws.** If the code prints `✔ {word}` or `default={label}`, the translation is only the word.
- **Agents may decide the wording.** Parker has delegated translation taste (2026-10-19); name your least-confident choices in the commit.
- For a large batch, the house method is a JSON batch with context and a width budget per string, translated by a strong model in one conversation per glossary, then checked for placeholders, keycaps and plural forms before it goes in.

## Glossary

Use these words, so a pane is the same pane everywhere.

| English | es | de | fr | ru | zh | ja | ko | hi |
|---|---|---|---|---|---|---|---|---|
| pane | panel | Bereich | volet | панель | 窗格 | ペイン | 패널 | पैनल |
| tab | pestaña | Tab | onglet | вкладка | 标签页 | タブ | 탭 | टैब |
| group | grupo | Gruppe | groupe | группа | 组 | グループ | 그룹 | समूह |
| project | proyecto | Projekt | projet | проект | 项目 | プロジェクト | 프로젝트 | प्रोजेक्ट |
| window | ventana | Fenster | fenêtre | окно | 窗口 | ウィンドウ | 창 | विंडो |
| outer (the window-level layer; never the word for window) | EXTERIOR | AUSSEN | EXTÉRIEUR | ВНЕШНЕЕ | 外层 | 外側 | 바깥 | बाहरी |
| sticky note | nota adhesiva | Haftnotiz | pense-bête | стикер | 便签 | 付箋 | 스티커 메모 | स्टिकी नोट |
| workbench | banco de trabajo | Werkbank | établi | верстак | 工作台 | ワークベンチ | 워크벤치 | वर्कबेंच |
| agent | agente | Agent | agent | агент | 智能体 | エージェント | 에이전트 | एजेंट |
| theme | tema | Theme | thème | тема | 主题 | テーマ | 테마 | थीम |
| design | diseño | Design | design | дизайн | 设计 | デザイン | 디자인 | डिज़ाइन |
| skin | skin | Skin | habillage | скин | 皮肤 | スキン | 스킨 | स्किन |
| scrollback | historial | Verlauf | historique | история вывода | 回滚缓冲区 | スクロールバック | 스크롤백 | स्क्रॉलबैक |
