# Architecture: GUI in the TUI — documents open in the pane

_Draft for Gate 2. Written against `origin/main` at de7776b. Code references are from the research write-up `td-internals.md` (research branch, b26b67a) and were re-read where marked. Numbers marked **[spike]** come from `spikes/snapshot-engine.md`._

## The shape in one paragraph

Everything happens in the window process; the session host and the wire between them do not change. A **router** turns a click into `open(document, placement, who_asked)`. A **placement** is either a floating square inside the pane you clicked in, or a split beside it that shows a third pane face. Either placement holds one **document view**, which asks a **backend** for pixels: images through gpui's own image element, Markdown through the renderer lifted from markdown-delight, HTML through a **page engine** behind a trait — a headless-browser snapshot first, a live engine later. On top of any HTML engine sits TD's own **notes layer**, which reads and writes the brief's notes in the brief's own format. The engine only ever supplies pixels and geometry; it never owns the notes, so swapping it cannot break them.

## Fit

**Clicks — `pane.rs`, `TerminalView::on_mouse_down` (line 6122, re-read).** Today's order: bench → sticky note → Alt+click on an armed copy chip copies (6196) → right-click menu (6208) → Super+Ctrl reveals (6226) → Shift or Ctrl opens with the desktop (6240) → selection. After:

| Order | Gesture | On a drawable document | Otherwise |
|---|---|---|---|
| 1 | Ctrl+Alt+click | `open(doc, Split, User)` | falls through (Ctrl branch) |
| 2 | Alt+click | `open(doc, Float, User)` | copy the armed chip's line, as today |
| 3 | Super+Ctrl+click | reveal | unchanged |
| 4 | Shift+click | **reveal** (was: open with desktop) | selection |
| 5 | Ctrl+click | open with desktop, unchanged | selection |

"Drawable document" = `link_under(pos)` resolves to an existing file whose extension (then magic bytes) is `.md`/`.markdown`, `.html`/`.htm`, or `.png`/`.jpg`/`.jpeg`/`.webp`/`.gif`/`.svg`/`.bmp`. The copy chip (`copy_hint_at`, 4206; drawn at 8480–8507) reads **"◳ alt+click · open here"** when the pointer's link is drawable, so the gesture says what it will do. The right-click menu (7374–7434) gains Open here, Open beside and Copy link — the last replaces the copy Alt+click gives up on document lines.

**Float placement — inside `TerminalView`.** A new `Option<FloatingDoc>` on the view (rect, drag state, the document view), painted as a child of the screen div after the sticky note (`pane.rs:8723`) and before the glass, so it sits over the grid and under the scanlines. Styled with `float_shadows(th.accent)` and `.border_2()` — the hyperglow every modal and menu uses (`main.rs:22187`). It is inside the pane's tube, so it bends with the glass (decided); its clicks go through the pane's warp inverse the way the workbench's do (`bench_hit_at`). Drag copies the window's `PaneDrag` shape (`main.rs:2857`: start, current, engaged past a threshold), held on the pane. Esc closes it; the title strip's "click to split", or a second Alt+click on the same path, promotes it.

**Split placement — a third face.** `workbench::Face` (`workbench.rs:76-80`) gains `Document`; the render branch at `pane.rs:8677` draws the document view instead of the grid or the bench. A split is made by a new `PaneEvent::OpenDoc { source, placement }` that `wire_pane` (`main.rs:4211`) subscribes to, like `DragPaneStart`, calling `Workspace::split` (`main.rs:9203`) with `SplitDir::Row` (beside) and flipping the new pane's face. The split tree keeps its single leaf type (`main.rs:320`). Costs, accepted: a shell idles behind the document, and the pane counts against the four-pane cap. At the cap, the router opens the float instead and says why. If a pane in the tab already shows the same path, it is focused rather than duplicated. Focus stays on the pane you clicked in (proposed; cmux's rule for opens that do not come from a click is stricter: never take focus).

**Document view — a new window-side module.** One gpui entity per open document: `source`, `kind`, scroll offset, and a backend.
- **Image** — `gpui::img(path)`; zoom and pan are the view's.
- **Markdown** — markdown-delight's `render.rs` (482 lines, MIT, same Zed commit) lifted into TD with three changes: a palette parameter taking TD's theme tokens, link targets kept and routed back through the router, and images drawn with `img(doc_dir.join(src))`. The workbench's `markdown` surface kind (drawn as plain lines today, `benchdraw.rs:993`) becomes its second caller.
- **HTML** — a `PageEngine` trait. First implementation: a headless-browser snapshot. Later: a live engine (Servo in-process, or Chromium streamed over its DevTools protocol) behind the same trait, chosen by a config key, so a disappointing engine is unplugged by configuration, not by rewrite.

**Notes layer — engine-independent.** Reads the brief's baked notes from its `report-notes` JSON island; draws TD's own note affordances at the anchor rectangles the engine reports; opens a TD note dialog (hyperglow); **save into file** rewrites only the island and the trailing `READER NOTES` comment in the file on disk, in place, atomically, keeping a backup; **copy map** produces the same map format `notes.js` does. Anchor ids come from the brief's own `notes.js`, run inside the engine, so they are the ids a browser computes. The brief's own notes UI is hidden inside the engine (injected stylesheet), so there is one notes UI, TD's.

**Size answers — `host.rs` and the PTY size.** Ships with the first build as its own slice: the host answers `CSI 14 t` from live geometry (the stale-size issue, #718), `CSI 16 t` gets an answer, and the PTY's pixel size carries device pixels rather than truncated logical ones. The `16 t` answer needs a small scanner in the host's tee because vte never dispatches it — the first piece of the later program-pixel track.

**Not touched:** the session host's pane model, the host↔window protocol, the attach snapshot, alacritty_terminal and vte.

## Endpoints
None. A `ctl` verb `open <path> --float|--split` for agents (open without taking focus) is a candidate for a later slice, not the first build.

## Data
- **The brief file.** `<script type="application/json" id="report-notes">{ "<nid>": [ { "text", "title", "ts" } ] }</script>`, and — since the decision-brief skill's commit 250188f on 2026-09-24 — `<script type="application/json" id="report-concurs">{ "<nid>": "YYYY-MM-DD HH:MM" }</script>` for the CONCUR stamp every decision now takes, plus a trailing `<!-- READER NOTES — … -->` comment carrying the map, with a "✓ concur" on each concurred decision's line. TD reads and writes exactly these regions and nothing else. Briefs written before today carry no concurs island; TD reads both shapes. **[spike]** on whether that is safe across the report corpus and how browser localStorage interacts with it.
- **The format has another writer.** The notes system (`~/.claude/skills/decision-brief/assets/notes.js`) is edited by other sessions — concurs arrived the same day this plan was written. TD's layer is a second writer of the same format, so the guard is shared fixtures: a handful of briefs with notes and concurs, checked in beside the skill, that TD's tests round-trip byte for byte.
- **Page cache.** Per `(path, mtime, size, CSS width, scale)`: page tiles, anchors (`nid`, title, rect, text), links (href, rect), per-dialog renders. Under `$XDG_CACHE_HOME/terminal-delight/pages/`. Invalidated when the file changes, which also covers an agent rewriting a brief while it is open.
- **Layout.** `SavedNode::Leaf` (`main.rs:735-758`) gains `document: Option<{ path, scroll }>` so a split showing a document survives a restart. A floating square is not saved (proposed).

## Flow
1. **Alt+click a Markdown path.** `on_mouse_down` → `link_under` → classify: Markdown → `open(doc, Float, User)` → `FloatingDoc` created under the pointer's line → document view parses once with comrak, builds elements each frame → painted in the square with the hyperglow → Esc closes and drops it.
2. **Ctrl+Alt+click an HTML brief.** `on_mouse_down` → classify: HTML → emit `OpenDoc { Split }` → `Workspace::split(Row)` → new pane, face `Document` → document view asks the snapshot engine (async, off the UI thread): launch or reuse a headless browser → load the file → hide the page's own notes UI → collect anchors, links, text and dialog renders → capture tiles **[spike: size, timing, tile height]** → tiles become `RenderImage`s and are painted as the view scrolls → notes layer draws affordances from the island and the anchors.
3. **Add a note and save.** Click an affordance (through the warp inverse) → TD note dialog → Add → in memory → Save → rewrite the island and the map comment in place (atomic rename, backup kept) → badge updates; the page does not need re-rendering, because notes are data, not layout.
4. **The file changes underneath.** Watcher → cache invalidated → re-render → notes re-read from the island.
5. **Promote a float.** "Click to split" → close the float → `OpenDoc { Split }` with the same source and scroll.

## External
- **A Chromium binary** (`/usr/bin/chromium` on this machine), run headless and driven over its DevTools protocol for the snapshot engine. Missing binary → the router falls back to opening with the desktop and says so. The Rust client for the protocol is a Gate 3 decision (a crate such as `chromiumoxide` or `headless_chrome`, or a small client over a WebSocket).
- **comrak** for Markdown, `default-features = false` (its defaults pull clap, syntect and the onig C library, which the renderer never uses).
- No environment variables and no network.

## Least confident decisions
1. **The split as a third face** with a shell idling behind it, rather than a real non-terminal leaf (63 leaf-walking call sites in `main.rs`). Cheap now; wrong if documents become a first-class layout citizen.
2. **TD's notes layer replaces the brief's own notes UI in every engine**, including future live ones. One UI in TD, at the cost of hiding working JavaScript in a live engine — and of following a format another writer keeps changing: the CONCUR stamp landed in `notes.js` on the day this was written. Shared round-trip fixtures are the guard; a live engine that let the page's own script write would remove the drift entirely, which is the strongest argument for one later.
3. **Saving in place** instead of downloading a copy. Better than the browser, but a bug here damages Parker's files; the guard list is in the spike's "Risks to the file".
4. **The snapshot engine depends on a system browser.** Fine on this machine; a fresh Omarchy install may not have Chromium.
5. **Alt+click gives up copying on document lines.** The right-click menu gains Copy link; if that proves annoying, the rule flips to "Alt+click on the chip copies, anywhere else on a document path opens".
6. **Focus stays with the pane you clicked in** when a split opens.

## Refined by the snapshot spike (2026-09-24, after approval)

The spike (`spikes/snapshot-engine.md`) confirmed both claims Gate 2 rests on — anchor ids from a headless render equal a browser's (93 of 93 briefs, at every width), and rewriting only the notes regions leaves every other byte identical (+240 bytes, 1.4 ms, on a 135 KB brief) — so the gate stands. It sharpens the design in these places, all measured unless marked:

- **Run each brief's own `notes.js` in the engine; never port the tagging rules.** The corpus carries 10 different copies (3 releases, 7 hand-edited forks); today's version would add anchors to 2 briefs. The engine also reports the brief's notes capability — island present, concurs supported or not, the `notes.js` hash — and TD goes read-only on anything unknown.
- **Tiles, not one image.** Full layout width (1,549 device px at 968 CSS × 1.6), **2,048 device px tall** (12.1 MiB each on the GPU); the visible tiles plus one above and one below resident (≤ 5, ~61 MiB); every tile's PNG kept on the CPU side so bringing one back is a ~20 ms decode and a ~4 ms upload. Capture the viewport first (45–73 ms over the DevTools protocol), then tiles outward. Scrolling moves resident tiles in TD; the engine is called again only when a scroll settles, a note changes, or the pane resizes (70–93 ms to re-lay out, re-anchor and capture). The tallest brief (25,902 device px) fits the GPU's 32,768 limit but would hold 153 MiB as one texture.
- **Anchors, links and text come from the same page instance and layout pass as the tiles**, tagged with a layout generation — a brief laid out 17–29 CSS px taller from its third load in one browser context.
- **Hide the page's own notes chrome** (`.notebar, .note-btn, .concur-zone { visibility: hidden }`) before capture; it moved no anchor in any of the 119 files.
- **Modals render on demand, one image each**, opened by dialog id rather than through the page's wiring (6 briefs ship buttons with no script behind them), with the viewport grown when a dialog body scrolls (30 of 285 do): ~150 ms per modal over the DevTools protocol. TD routes clicks on `[data-dlg]` buttons to these renders.
- **Files with no notes island are read-only images** (26 in the corpus, 11 of them `_name_body.html` build partials TD must never write). "Make this brief annotatable" can come later as an explicit, backed-up conversion.
- **The write rule, in order:** anchors from a render of the current bytes (content hash carried); re-read and re-hash at write time, re-render on mismatch; parse the islands fresh and apply TD's change as a delta; serialise as `JSON.stringify(map, null, 1)` then escape `</` as `<\/` and `<!` as `<!`; write concurs only where the brief supports them; label the map with the brief's `NOTES_FILE`; replace the last `READER NOTES` comment (else insert before the last `</body>`, else append), breaking `-->` inside it; verify in memory that only the three spans changed; keep a short per-file backup ring in TD's state directory, write a temp file beside the original, fsync, rename (btrfs, atomic), preserving the mode and refusing symlinks; re-render and confirm the badges. `spikes/snapshot-engine/notes-writer.mjs` is the reference implementation.
- **Browser storage beats the file.** A browser that ever stored a map for a brief's filename — even the `{}` the ✕ button leaves — never shows the file's notes again; same-basename copies share one entry. That is the skill's to fix (parker-brown-family/agent-skills#24: an island revision `notes.js` prefers when newer); until then TD warns when it changes a brief's notes. A note containing `</script>` also breaks the page's own save (agent-skills#25); TD's writer escapes it.
- **Format drift guard:** a declared format version on the island (a skill-side change) that TD's writer checks, refusing unknown versions, plus golden fixtures in the skill's repository — one brief per `notes.js` release and fork, each with the expected island, concurs and map bytes — run by the skill's tests and TD's writer tests.
- **For the later live engine:** Chromium's screencast delivers 968 × 1400 frames at scale 1.6 — 39 % of the pixels the screen needs — whether an emulation override fixes that is not established; and a live engine must answer `confirm()` itself (agent-skills#18) and route note clicks to TD's layer rather than let the page save. The Chromium process tree costs 265–570 MiB while a brief is open.
