# As built: where the code differs from the design

_Written 2026-09-25, when the ten slices and the follow-ups had merged. The Gate 3 documents (`03-program-design*.md`) describe the design as approved and are left as they were. Read this first: it lists every place the code went another way, and why. **Their `file:line` references were measured at de7776b and are stale.** Find symbols by name._

## Names and homes
- `device_cell` lives in `ptyscan.rs`, not `docopen.rs`, because the size-answers slice was built in parallel with slice 1.
- `chip_label` lives in `pane.rs`, because `docopen.rs` imports nothing and the labels come from `lang.rs`.
- `ImageDoc::release` became `give_back`, so the seam's `DocumentView::release` (a pointer let go) doesn't share its name. Textures are given back by the view's release hook, registered in `DocumentView::new`, not by whoever closes it.
- `doc_wheel` is called from `on_wheel`, not `scroll_by_wheel`, because the FOCUS modal routes its own wheel through the latter.
- `FollowLink` carries a separate `fragment`, so a `#` in a file name is not ambiguous.
- The helper the control verbs use to pick a pane was called `bench_apply` and ran the document verbs too; it is now `act_on_picked_pane`.
- The design says "ten languages"; `lang.rs` has nine tables.

## Behaviour the design did not specify, or specified differently
- **Control verbs.** `ctl doc here | beside | close | notes | note | concur | save`, so every gesture can be driven and measured without a pointer (the soak and check scripts under `scripts/`).
- **Resize.** The floating square resizes from its left, right and bottom edges (7 px grip) and all four corners (14 px). The top border between the corners is still the move strip, and buttons keep every pixel of their face.
- **Unsaved notes.** Every deliberate close keeps a square with unsaved notes once, with a sentence in its bar: Escape, ✕, `ctl doc close`, a replacing Alt+click or link, and a duplicate split. Only closing a pane or tab drops it without asking. `request_close_float` is the only caller of `close_float`, and a test holds that.
- **Rows where gpui draws them.** The pane's `cell_h`, its grid padding and the FOCUS reader's rows are snapped to whole device pixels (`laid_out_length`, `grid_pad_drawn`), because gpui rounds every authored length before layout. A test holds the copy to gpui's `util.rs`.
- **The Document face** refuses paste as well as typing (a step past ruling 4). A dropped file is not typed into the hidden shell.
- **`beside()` canonicalises paths**, so `..` and symlinks find a pane already showing the file. Only mouse gestures are debounced, so scripted calls are never dropped.
- **`set_seat` forgets the old size**, or zoom is computed against the square after a promotion. The replica-repair carry takes `cx`, so moved views are re-subscribed to their links.
- **`Bench::toggle_face` takes `has_document`**, so `ctl bench toggle` cannot walk onto a document by alt+k's order.

## The HTML engine
- The client uses 19 DevTools method names, not 16.
- The page-level network block lets WebSockets through, and its newer form blocks nothing, so the browser is also started with dead-proxy flags. The network test holds a local listener at zero connections.
- At scale 1.25 the page's last tile comes back one device row short; tile edges are snapped to whole device pixels.
- A freshly spawned browser gets 30 s (`START`) for its first answer, where the design had 10 s: GitHub's runner took longer than 10 s on a cold start. A hang is reported as a hang, not as its last stderr line.
- CI runs the real-Chromium tests after lifting the runner's AppArmor restriction on user namespaces (`.github/workflows/ci.yml`). They skip, and say why, only where the sandbox cannot start.
- A restored HTML document pane on a machine without Chromium says why it cannot be drawn and opens no browser window.

## Notes
- The format is pinned by agent-skills (PR #29, 0eb9e20): `data-format="1"`, `data-rev` set the way `setAttribute` does it, `</` → `<\/` and `<!` → `<!` in the islands, and the READER NOTES mirror replaced rather than appended. Unknown formats are refused. `scripts/sync-brief-fixtures` and the weekly drift watch keep TD's copy honest.
- Concur support is decided only by a script that makes concur zones, not by zones present in the markup. A sticker study that drew its own zones never showed its stamp, and the read-back caught it.
- The layout hash leaves out the notes islands' opening tags as well as their text, because format 1 changes the tag.
- If the file's layout changed since it was read, a save refuses and redraws instead of retrying.
- Measured: a note (and a stamp where supported) saved into copies of 94 briefs moved no anchor in any of them.

## The size answers
- `cell_px` starts at the host's cell for attached panes, or every pane would re-announce its size on the first frame. No `scale` field was added for the size answers. The pane does now keep a `scale`, but for the row-height and padding snapping.

## Still open
See "Open follow-ups" in `00-status.md`.
