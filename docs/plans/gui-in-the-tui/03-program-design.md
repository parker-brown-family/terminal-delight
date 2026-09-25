# Program Design: GUI in the TUI — documents open in the pane

_Gate 3. Two halves were designed in parallel against the build worktree at de7776b and are kept whole beside this page, because their signatures and call stacks are the detail this gate exists to fix:_

- _`03-program-design.td-wiring.md` — the click to the view: router, classifier, floating square, `Document` face, split, saved layout, chip, menu, keys, and the size answers._
- _`03-program-design.docs.md` — the view and everything under it: image, Markdown and HTML backends, the page engine and its Chromium client, the page cache, and TD's notes layer._

_**As built:** where the code went another way, and why, is `05-as-built.md`; read it before trusting a `file:line` here._

_This page is the join: where the halves meet, the rulings that reconcile them, the combined file list, the test plan's shape, and the least-confident decisions merged and ranked. Where this page and a half disagree, this page wins._

## Rulings that join the halves

1. **The click's target is `docopen::DocTarget { path, kind }`**, not `DocumentSource`. `doc::DocumentSource` is already a trait (`doc.rs:84`) that `pane.rs:9` imports by name; a second `DocumentSource` in the same file would make every reader guess. Both halves' signatures read `DocTarget` wherever they say `DocumentSource`.
2. **The seam is the documents half's version** (its "The seam" section): the wiring half's eight calls, with `scroll()` returning `Option<DocScroll>` (none before a layout, none for an image), plus `set_theme`, `hover`, `drag`, `release`, the `CannotShow` event and `html_ready`. The router calls `html_ready` before placing an HTML document, so a missing Chromium hands the file to the desktop with a sentence instead of opening an empty square.
3. **Input reaches the view only through the pane.** The pane un-bends every press, move, wheel and key through the warp inverse and calls the view's methods with flat, view-local points; nothing under `docview` registers a gpui mouse, scroll or hover handler, and `nothing_in_the_document_view_listens_for_the_mouse` fails the build if anything does. Both halves designed to this; it is the largest integration risk between them, so it is a test, not a convention.
4. **The `Document` face stays a third `workbench::Face`**, as Gate 2 approved, and the two-places risk is closed by construction: `show_document` is the only way onto the face and sets the document and the face together; `set_face(Document)` without a document is refused; `Bench::set_face` assigns instead of toggling. The alternative — the pane's document outranking the bench face — is least-confident decision 1.
5. **The note dialog is drawn inside the pane**, bent with the glass, as mockup 03 draws it; clicks reach it through the pane like everything else in the view.
6. **One Chromium per TD process**, driven by TD's own client over `--remote-debugging-pipe` (no crates added, no port opened, an empty throwaway profile so the file's own notes win), launched from a dedicated thread, shut down after five idle minutes.

## Files

**New**
- `app/src/docopen.rs` — the pure decisions: `DocTarget`, `DocKind`, the classifier, the click table, the Alt chip, the link menu, the float's geometry, drag and hit zones, device-pixel cells. Standalone, like `keylayer.rs`.
- `app/src/ptyscan.rs` — the `CSI 16 t` scanner the host's tee runs.
- `app/src/docview.rs` — `DocumentView`: the seam, backend dispatch, the file watcher (500 ms `mtime` poll, the house pattern), zones measured at paint.
- `app/src/docview/{image,markdown,page,engine,snapshot,cdp,cache,notes,notes_ui,pref}.rs` and `docview/extract.js` — as listed in the documents half.
- `app/assets/img/concur-stamp.svg`; `app/tests/snapshot_engine.rs`; `app/tests/fixtures/decision-brief/notes-format/` (vendored from the skill, with `SOURCE`); `scripts/sync-brief-fixtures`; `.github/workflows/brief-fixtures-drift-watch.yml`.

**Changed**
- `app/src/pane.rs` — router, float, `Document` face, chip, menu, keys, replica carry, serverless `14 t`, device pixels to the PTY.
- `app/src/main.rs` — `mod docopen; mod ptyscan; mod docview;`, `SavedDocument` on the layout, `OpenDoc` handling and `split_with_document`, `set_all_faces` skips documents, the help modal, `EditBuffer::insert`, test literals.
- `app/src/workbench.rs` — `Face::Document`, `set_face` assigns, `next_face` for alt+k.
- `app/src/keylayer.rs` — `Layer::Float` and `Layer::Document`, the ladder at 13 rungs.
- `app/src/term.rs` — a replica swallows the size question as it swallows `PtyWrite`.
- `app/src/host.rs` — the answering thread reads live geometry (closes #718); the tee runs the `16 t` scanner.
- `app/src/pane/bench.rs` — a file dropped on a document face is not typed into the hidden shell.
- `app/src/benchdraw.rs` — Markdown surfaces drawn by the lifted renderer at both card sizes.
- `app/src/lang.rs` — the new gesture strings in all ten languages.
- `app/Cargo.toml` / `Cargo.lock` — `comrak 0.52` without default features (+8 packages); `base64` is already in the lock.
- `THIRD-PARTY-LICENSES.md` — comrak and the lifted markdown-delight module.

**Outside TD (agent-skills):** `decision-brief/fixtures/notes-format/` with the reference writer, the island revision for #24, the escaping fix for #25, and a declared format version on the island.

**Not touched:** `hostproto.rs`, `socketpty.rs`, `gridwire.rs`, `warp.rs`, alacritty_terminal, vte.

## The shape of the tests

TD has no gpui test harness, so the plan follows the house style: pure functions in each module's `mod tests`, source scans for rules a type cannot hold, real-PTY tests in the host, and a new `app/tests/snapshot_engine.rs` against a real Chromium. The wiring half names 40 new tests and extends two existing ones, and the documents half names 69 (counted from the two drafts' test plans); every new one fails on today's code, most because the symbol does not exist yet, and the ones that pin a behaviour change say so (Shift+click opening today, the stale `14 t` answer, the replica forwarding the size question, the bench's plain-text Markdown). The notes-format tests run over 13 shared fixture cases, so the skill and TD are held to the same bytes.

## Least confident decisions, merged and ranked

1. **The face in two places.** A third `workbench::Face` plus a document on the pane can disagree; ruling 4 makes the disagreement unreachable through the API, but not through the type. The alternative leaves `Face` alone and lets the pane's document outrank it, touching fewer sites.
2. **Notes are data, not layout.** The cache key and the no-re-render-after-save both assume a changed notes block moves no anchor. Inferred, not measured; the read-back after every save is the guard, and slice 8 measures it.
3. **Textures given back at close.** `drop_image` at release relies on gpui's release order; if it is wrong, up to 61 MiB leaks per closed brief. Slice 1 counts atlas textures across a hundred open-and-close cycles before anything else builds on it.
4. **Owning the DevTools client.** No crates and no port, at the price of protocol code TD maintains (16 method names). A live engine later may want a full client; the trait keeps that to one module.
5. **The note dialog inside the bent glass**, with multi-line editing built on TD's single-line `EditBuffer`. A flat window-level modal would be simpler to type into.
6. **Explicit save.** Unsaved notes live in memory, as the brief's own buttons imply; a crash loses them.
7. **The `16 t` reply can overtake an earlier reply in the same read.** Right for the usual probe order; wrong only for a program that asks for device attributes before cell size in one write.
8. **The document face swallows typing** rather than passing keys to the hidden shell.
9. **Browser storage still wins in the browser** until the skill fixes #24.
10. **The page cannot reach the network**; a brief that loads a web font renders with fallbacks unless `documents.toml` allows it.
11. **Vendored fixtures and a weekly drift watch** rather than fetching the skill in CI: TD's writer can lag a format change by up to a week.
12. **One Chromium per process with a five-minute idle shutdown** (265–570 MiB while alive).
13. **Serverless panes answer `14 t` but not `16 t`**; hosted mode is the default.
14. **Promotion moves the view entity** from float to split rather than reopening the file.

The halves carry further, smaller calls (rounding for device pixels, the scroll saved as a fraction, the copy chip's own label, Shift+click on a web link, a missing file keeping its face on restore); they are in each half's own list.
