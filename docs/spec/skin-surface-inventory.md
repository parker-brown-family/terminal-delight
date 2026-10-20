# Skin surface inventory

Every native surface a skin has to reach, who draws it, and whether a skin can reach it today. Start every skin pass from this page, not from screenshots. Chalk's first pass passed a neutral fixture and still missed the cabinet, the queue, the gauges and the terminal stock. Its record is in `docs/plans/chalk-flower/02-iteration.md`.

**Read at:** origin/main `d9912a6c`, 2026-10-19. Line numbers drift. Before trusting a row, search for the function name, not the line. When a pass changes a route, update the row in the same pull request.

## What a skin file can say

Built-in skins live in `app/skins/` and are registered in `BUILTIN_SKINS` (`app/src/skin.rs:66`). The picker order is chalk, default, deco, console. A user's own skin is read from `$TD_SKIN` or `~/.config/terminal-delight/skin.toml` and is offered as `custom` (skin.rs:1842). `paper` is an old alias for `chalk` (skin.rs:2102).

Top-level keys (`SkinFile`, skin.rs:1618): `name`, `icon`, `[font] family`, `[ink]`, `[metric]`, `[shape]`. An unknown key is reported with a "did you mean" hint rather than dropped (skin.rs:1660).

**Ink** is `"#hex"` or `{ from, l, s, a, max_l, toward, t }`, built from these roles: `bg surface text accent complement human faint cursor white black ansi0..ansi15` (skin.rs:98). There are 30 keys (skin.rs:303):

- grounds: `panel panel_raised panel_sunken panel_glass btn_face`
- lines: `rule rule_strong edge edge_strong focus`
- text: `ink ink_dim ink_off ink_faint ink_ghost ink_on_mark ink_lit`
- rest: `edge_rest face_rest`
- accent by job: `mark mark_soft mark_dim mark_wash`
- selection: `select row_active hover`
- state: `live ok warn danger`

**Metrics**, with their defaults (skin.rs:422): `radius 4, radius_lg 10, hairline 1, border 1, rule_gap 2, gap 4, pad_x 6, pad_y 4, chip_px 5, chip_py 1, row_h 15, label_size 9, bracket 5, rail 2, glow 5.25, glow_a 0.11`.

**Shapes** (skin.rs:457–593, parsed at 1760):

| key | values |
|---|---|
| `material` | plain, paper |
| `corner` | round, square. There is no chamfer yet; issue 407 asks for one |
| `boundary` | hairline, double, inset, none |
| `emphasis` | fill, underline, bracket, rail, glow |
| `divider` | line, double, none |
| `caps` | off, upper, tracked |
| `shine` | gradient, flat |

**Fonts.** The UI font is the skin's `[font] family` when it sets one, otherwise the scope design's grid font (skin.rs:649, 688). So outside Chalk, every name in the chrome is set in the terminal's font. It is applied on the window root (main.rs:33209) and the pane root (pane.rs:11580). The grid font is `grid_face` (pane.rs:9623), taken from the design's `[font]`, or News Cycle italic in crawl. Caveat and News Cycle are bundled with `include_bytes!` (main.rs:43597–43614). `[font]` takes one family, so a skin cannot yet set names and numbers in different faces.

**Paper is special-cased.** These helpers branch on paper: `tab` 952, `panel` 1044, `bar` 1106, `icon_btn` 1179, `chip` 1200, `slider`/`slider_half` 1262/1286, `ring` 1355, `active_row` 1419, `bezel` 1474, `btn` 1538. These do not: `rule_h`, `rule_v`, `row`, `field`, `label`, `ground`. The paper primitives are in `app/src/paper.rs`.

## Who owns what

| layer | owns | where |
|---|---|---|
| **Palette** (THEME column) | every colour | `paint::stamp`, paint.rs:278. It sets the palette or dynamic and leaves texture, effects, grade and warp alone |
| **Design** (STYLE → DESIGN) | `[effects]` (CRT texture, including `bezel`), the grid font, size and cell height, the preferred skin (`skin =`), and base colours only where no palette, seed or dynamic overrides them | theme.rs:83, resolved in `theme::resolve` theme.rs:2376 |
| **Skin** (STYLE → SKIN) | ink recipes derived from the resolved palette, metrics, shapes, material, and the UI font | skin.rs. Shape is window-global; inks bake per pane in `skin::for_theme` (1988) |
| **Grade** (the scope) | warp, glare, tracking, crawl, CRT on or off, phosphor, scale, text size | theme.rs:2516 |

A fresh window's outer scope has `palette = TERMINAL_DELIGHT` (theme.rs:1276), so picking a design there changes effects, grid font and skin preference, not colour. Skin resolution order (skin.rs:1873): explicit pick, then the user file, then the outer design's `skin =`, then default. An explicit pick is saved in the state file.

The built-in designs are Hacker, Tactical, Quiet and Deco, plus `custom`. Field was folded into Tactical in pull request 1029, and a saved `field-command` reads back as `tactical-overdrive`.

## The surfaces

**Route:**
- **skin**: drawn through the helpers and inks.
- **mixed**: the radius goes through the skin, but fills and borders are raw theme colours. Usually the "menu recipe": `rounded(sk.rad_raw(N)).border_2().border_color(th.accent.alpha(0.85)).bg(paper ? sk.ink.panel : darken(th.surface,…)).map(|d| sk.finish(d))`. Paper reaches these through patches; deco and console do not change their borders or fills.
- **bypass**: raw colours and literal radii, so no skin reaches them.

| surface | drawn by (file:line, function) | route | font |
|---|---|---|---|
| Root ground behind the cabinet | main.rs:33196–33209 | mixed | sets the UI font |
| Window cabinet / outer frame | main.rs:33263–33315 in `Workspace::render`. Literal `rounded(px(14.))`, gradient from `bezel`, accent border | mixed (literal corner) | UI |
| Top band | main.rs:27902–28023, `sk.sheet()` | skin | UI |
| Top-band tabs and strip chips | `tab_button` main.rs:25086 (`sk.tab` 25157), `strip_chip` 25410, `render_strip_head` 25323, `eng_badge` 25472 | skin | UI |
| Agent-state counter, top right | `render_agent_counter` main.rs:22832 | mixed | UI + glyph font (22895) |
| Menu-bar SIZE button and track | main.rs:27545–27756 | mixed | UI |
| Bottom band | main.rs:28026–28077, `sk.sheet()`. Text is raw `th.text` (28048). Glyph row 27757, `bezel_btn` 20974 | skin, text colour raw | UI |
| Left bar tree | `render_left_bar` main.rs:24877. Rows: `bar_row` 21319, `branch_row` 21512, `task_row` 21823 (`sk.active_row` 21879). `bar_cursor_ring` 21474 is a raw `th.cursor` wash. Rename boxes `darken(th.bg,0.8)` (21554, 21857) | mixed | UI |
| Left-bar bay and slot rails | `render_bay` 22178, `render_bar_slot` 22481, `under_the_tree` 3516 | skin-leaning, unconfirmed | UI |
| Left-bar tray menu | `render_bar_menu` main.rs:11945, `bar_menu_row` 11900, `bar_menu_rule` 11931 | skin | UI |
| Pane frame and focus ring | `render_node` main.rs:26600–26720. `rounded_md`, raw shadows, unfocused dim. Paper patch 26683–26707 | bypass (paper patched) | — |
| Pane header | pane.rs:10978–10992 | skin | UI (10975) |
| TERM / BENCH / DOC slider | pane.rs:10760–10790 | skin | UI |
| Handover chip | pane/handed.rs:387 | skin (counted, not read) | UI |
| Pane right-click and ⋯ menus | pane.rs:10050–10110, 10610–10650. Raw `th.surface`/`accent`, `rounded(px(8.))` | bypass | grid font (10057, 10614) |
| Terminal tube stock | pane.rs:11680–11687 (paper only, inside the warp tube) | paper only | — |
| Terminal grid | pane/grid_paint.rs:40 | design | grid font |
| CRT glass | `crt::glass` crt.rs:195, attached pane.rs:11749 | design `[effects]` | — |
| Raised CRT bezel | `crt::bezel` crt.rs:147–190. Raw `rounded_lg`, hsla seams. Hidden under paper (pane.rs:11759) | bypass | — |
| Attention spine pill | `render_spine_pill` main.rs:23893 | mixed | UI |
| Attention queue | `rail_panel` main.rs:24071. `rail_ink` 23844 hard-codes kind colours (23849–23852). `render_rail` 24826, `render_rail_docked` 24050 | mixed | UI |
| Window palette tray (THEME / STYLE / RULES) | main.rs:28078–28584. `theme-panel` 28525. `theme_icon_btn` 26038, `color_mode_btn` 26106, `note_paper_btn` 26172, `palette_rule` 26022. SKIN row 28130–28190 | mixed | UI |
| GAUGES / DISPLAY tray | main.rs:28585–28877 (`osd-panel` 28637) | mixed | UI |
| Switches and sliders | `slider_switch` main.rs:1437, `slider_row` 20371, `track_slider` 20660, `lightness_bar` 20897, `tab_lightness_bar` 25751, `card_scale_slider` 17723, `focus_zoom_slider` 17828. `focus_inherit_toggle` 17937 has no skin calls | mixed; `focus_inherit_toggle` bypass | UI |
| Paint cards (Super+Alt+P) | `paint::card` paint.rs:417, `paint::tile` 560, shelf chip 691 | mixed | UI (paint.rs:435) |
| Sticky note | `sticky::paint` sticky.rs:869, `note_el` pane.rs:11452. Colours from notepaper.rs:367 | own system, by design | Caveat, fixed (sticky.rs:42) |
| Subscriptions / usage panel | `render_savings_overlay` main.rs:10166, `savings_shell` 10604, `render_usage_body` 9489, `pace_band` 26315 | mixed | UI (10646) |
| FOCUS reader (Alt+R) | main.rs:32526–33190. `reader_panel` 17234 raw | bypass, deliberate (keeps the measured monospace) | grid font from the snapshot |
| Document square | pane/doc.rs:304 `float_el`, `square_el` ~330–445 | bypass | inherited, unconfirmed |
| Document face and notes bar | pane/doc.rs:1100, docview/notes_ui.rs | bypass | grid font (notes_ui 1725) |
| Bench cards | benchdraw.rs: `title_card` 2448, `rail_row` 326, `shelf_tab` 533, `dial_menu` 2696, `spine_frame` 194. Corners skinned (test `every_corner_on_the_bench_goes_through_the_skin`); colours from `benchdraw::ink` 125. No paper | mixed | grid font (14 sites) |
| Agent wall and agent cards | main.rs:28898–30677; card 29911–29935, `agent_card_shadows` 26389 | cards bypass; panel mixed | UI |
| Dead-agent recover manifest | main.rs:30679–31336 | mixed | UI |
| Confirm-close and delete-branch dialogs | `danger_panel` main.rs:11796 | mixed | UI |
| Help modal | main.rs:31942–31968 | mixed | UI (31958) |
| SIZE and … popovers | main.rs:31411–31449, 31510–31567 | mixed | UI |
| Tab config pane, group menu | main.rs:32071–32331, 32368–32482 | mixed | UI |
| Tooltips | `over_the_glass_tip` main.rs:25843, `SlotTooltip` 25864, `ThemeTooltip` 25894 | mixed | unconfirmed |
| Marquee toast | `marquee_banner` main.rs:1505 | bypass | UI |
| Find (Ctrl+F) | `render_find` main.rs:15149 | bypass | UI |
| Lookup (Alt+L) | `render_lookup` main.rs:15595 | mixed | UI |
| Plugins, notifications, logo picker | main.rs:15904, 16071, 16807 | bypass | UI |
| Language picker | main.rs:16241 | bypass | UI |
| Agent launcher | `render_agent_launcher` main.rs:16439; shell raw `th.surface` | mixed | UI |
| Git coordination overlay | `render_eng_table` main.rs:25525, coordination_view.rs:720 | mixed | UI |
| CSD resize edges | csd.rs | n/a | — |

## Not yet confirmed

The font on tooltips. The document square body font. Where the vitals bars are drawn. The inside of `render_bay`, `render_bar_slot` and `under_the_tree` (judged from usage counts only). Whether `hacker.toml`'s `curvature` key is read (it is not in `FileEffects`).
