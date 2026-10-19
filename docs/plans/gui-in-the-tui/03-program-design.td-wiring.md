# Program Design: GUI in the TUI — the TD wiring half

_Draft for Gate 3, one of two halves. This half covers everything between a click on a path and a document view on screen: the router, the classifier, the floating square, the `Document` face, the split, the saved layout, the chip, the menu, the keys, and the size answers. The other half (being drafted separately) covers the document view itself: its image, Markdown and HTML backends, the page engine and the notes layer. Here that is one opaque gpui entity, `DocumentView`, and the calls this half needs from it are listed under "The seam" below._

_Written against the build worktree at de7776b. **[m]** = measured: I opened the line in this worktree or ran the command. **[i]** = inferred from lines I read. **[h]** = hunch. Every `file:line` is in `app/src/` unless another path is given._

## The seam: what this half asks of the other

The wiring needs eight things from `DocumentView`. None of them exists today **[m]**: there is no document view, and the name `doc` is taken, because `doc.rs` is the FOCUS reader's line model and `pane.rs:1955` holds an `Arc<Document>` from it **[m]**. So the other half's module needs another name; `crate::document` is used below as a placeholder.

```rust
// Owned by the document-view half. The names are requests.
pub struct DocumentView { /* opaque */ }
impl Render for DocumentView {}          // fills whatever box the pane gives it

impl DocumentView {
    pub fn new(source: DocumentSource, seat: DocSeat, cx: &mut Context<Self>) -> Self;
    pub fn set_seat(&mut self, seat: DocSeat, cx: &mut Context<Self>);    // float <-> face
    pub fn scroll(&self) -> DocScroll;                                      // for the layout file
    pub fn restore_scroll(&mut self, at: DocScroll, cx: &mut Context<Self>);
    /// A press the pane has already un-bent. `at` is flat and relative to the
    /// view's own top-left. Answers whether the view took it.
    pub fn press(&mut self, at: Point<Pixels>, mods: Modifiers,
                 window: &mut Window, cx: &mut Context<Self>) -> bool;
    pub fn wheel(&mut self, delta: ScrollDelta, cx: &mut Context<Self>);
    /// Paging, arrows, and typing into a note dialog. Answers whether it took the key.
    pub fn key(&mut self, ks: &Keystroke, cx: &mut Context<Self>) -> bool;
    pub fn has_caret(&self) -> bool;                                        // a note dialog is taking text
}
/// Emitted when a link inside the document is clicked. The pane routes it.
pub struct FollowLink { pub target: String }
impl EventEmitter<FollowLink> for DocumentView {}
```

**One constraint on that half:** the view may not rely on gpui's own mouse or scroll handlers. gpui hit-tests the flat element tree while the CRT pass bends the pixels, so a handler inside a bent tube fires beside what it draws. That is why the copy chip carries no handler (`pane.rs:8468-8474`) and why the bench routes every press through the warp's inverse (`pane.rs:6141-6150`, `pane/bench.rs:317-332`) **[m]**. `press`, `wheel` and `key` above are how input reaches the view instead.

This half owns `DocumentSource`, `DocKind`, `DocSeat` and `DocScroll` (in `docopen.rs`, below), because the router produces them before any view exists.

---

## Files

**New**
- `app/src/docopen.rs` — the pure decisions: the drawable-document classifier, the click ladder, the chip's action and label, the menu items, the float's geometry and hit-testing. No `use` statements, like `keylayer.rs` (`keylayer.rs:59-64` explains why: `rustc --test` runs a standalone file in about a second) **[m]**.
- `app/src/ptyscan.rs` — the `CSI 16 t` byte scanner the host's tee runs. Standalone for the same reason, and the first piece of the later program-pixel track (`02-architecture.md` "Size answers").

**Changed**
- `app/src/main.rs` — `mod docopen; mod ptyscan;` beside the others (`main.rs:30-80`); `SavedDocument`; a `document` field on `SavedNode::Leaf` (`:735-758`), `LeafState` (`:700-708`), the hand-written deserializer's `LeafFields` (`:771-791`) and both "Leaf" arms (`:816-824`, `:837-848`); `to_saved_with` (`:642-665`) and `Node::to_saved` (`:675-694`); both restores (`build_node_attached` `:4978`, `build_node` `:5056`); the `OpenDoc` subscription in `wire_pane` (`:4211-4361`); `open_doc_beside`, `split_with_document`, `tab_with_leaf` and `beside` next to `split` (`:9203`) and `may_split` (`:136`); `set_all_faces` skips document panes (`:7481-7499`); the help modal's links section (`:27377-27383`); twelve test-side `SavedNode::Leaf { … }` literals gain `document: None` (`:31431 34432 34506 34569 34578 34642 34712 35021 35072 35106 35493 35715`) **[m]**.
- `app/src/pane.rs` — the router (`open_document`) and `document_under`; the float (state, render, hit-testing, drag, promotion); the `Document` face's render, press, wheel and keys; `OpenDoc`; the chip's action and label; the context menu; `show_document` and `restore_document`; `Presentation` carries the document across a replica repair (`:1835-1843`, `:2967-2998`); the serverless `14 t` answer (`handle_term_event`, `:3909-3975`); device pixels to the PTY (`:3509-3517`, `sync_size` `:3981`).
- `app/src/pane/bench.rs` — a file dropped on a `Document` face must not be pasted into the hidden shell (`:2735-2741`) **[m]**.
- `app/src/workbench.rs` — `Face::Document` and the four exhaustive matches over `Face`; `Bench::set_face` stops toggling its way to a face (`:4081-4085`); `next_face` for alt+k.
- `app/src/keylayer.rs` — `Layer::Float` and `Layer::Document`; `Up.float`, `Up.float_caret`, `Up.document`; the ladder grows from 11 to 13 rungs (`:190-204`), and its two property tests grow with it (`:453-541`).
- `app/src/term.rs` — `EventProxy::send_event` must swallow `TextAreaSizeRequest` on a replica, as it already swallows `PtyWrite` (`:94`).
- `app/src/host.rs` — `HostPane.geom` becomes shared with the answering thread (`:262`, `:705-713`, `:742`); `TeeReader` gains the scanner and a sender (`:134-163`); the event channel is created before the tee (`:677-679`).
- `app/src/lang.rs` — five new gesture strings in each of the 10 languages (`k_shift_ctrl_click` has 10 definitions **[m]**); the `rclick` summary names the new menu items (`:391`).

**Not touched:** `hostproto.rs` (`PaneGeom`'s `u16` cell fields simply carry device pixels; `:54-62` already describes them as "pixels per cell for anything that draws" **[m]**), `socketpty.rs`, `gridwire.rs`, `warp.rs`, `Cargo.toml`, alacritty_terminal, vte.

---

## Types & signatures

### `docopen.rs`: the pure half

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DocKind { Markdown, Html, Image }

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DocumentSource { pub path: std::path::PathBuf, pub kind: DocKind }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DocSeat { Float, Face }

/// The top edge of the view as a fraction of the document's height, 0.0..=1.0.
/// Proposed to the other half; a fraction survives a reflow at a new width
/// better than a pixel offset does.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DocScroll { pub top: f32 }

/// Where a document goes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Placement { Float, Split }

/// Who asked. It decides what happens at the four-pane cap: a float asking to
/// become a split stays a float; anything else opens a float instead.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Asker { Click, Menu, Float }

/// By file name alone: .md .markdown .html .htm .png .jpg .jpeg .webp .gif .svg
/// .bmp, case-insensitive. Used on restore, when the file may be missing.
pub fn doc_kind_by_name(path: &std::path::Path) -> Option<DocKind>;
/// By name, then checked against the first bytes: a raster name must carry a
/// PNG/JPEG/GIF/WebP/BMP signature, and a file with no known name is an Image
/// if it carries one. SVG, Markdown and HTML are by name only.
pub fn doc_kind(path: &std::path::Path, head: &[u8]) -> Option<DocKind>;
/// Impure: a regular file (not a directory), its first 16 bytes read.
pub fn drawable_document(path: &std::path::Path) -> Option<DocumentSource>;

/// The modifier state the click ladder reads. Built from gpui::Modifiers at the
/// call site, so this file stays standalone.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Mods { pub alt: bool, pub control: bool, pub shift: bool, pub platform: bool }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClickIntent { OpenHere, OpenBeside, CopyChip, Reveal, OpenWithDesktop, Pass }

/// The left-click ladder of `TerminalView::on_mouse_down`, as one table:
///   ctrl+alt on a document  -> OpenBeside
///   alt on a document       -> OpenHere
///   alt on an armed chip    -> CopyChip        (as today; ctrl+alt off a document lands here too)
///   super+ctrl, revealable  -> Reveal          (unchanged)
///   shift, revealable       -> Reveal          (was OpenWithDesktop)
///   shift or ctrl on a link -> OpenWithDesktop (a web link under shift keeps opening)
///   otherwise               -> Pass            (selection)
pub fn click_intent(m: Mods, on_document: bool, on_link: bool,
                    revealable: bool, chip_armed: bool) -> ClickIntent;

/// What an Alt+click on this line would do.
#[derive(Clone, PartialEq, Debug)]
pub enum AltClick { Copy, OpenHere(DocumentSource) }
/// A document under the pointer wins, on any line and on the alt screen too;
/// otherwise a command line copies, and never on the alt screen (as today).
pub fn alt_click(on_alt_screen: bool, line_is_command: bool,
                 doc: Option<DocumentSource>) -> Option<AltClick>;
/// "◳ alt+click · open here", "⎘ alt+click · copy", "✓ copied".
pub fn chip_label(does: &AltClick, copied: bool) -> &'static str;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LinkItem { OpenHere, OpenBeside, OpenWithDesktop, Reveal, CopyLink }
/// The link half of the right-click menu, in order.
pub fn link_menu(is_document: bool, revealable: bool) -> Vec<LinkItem>;

/// A float's rectangle, flat and relative to the pane's screen (content) origin,
/// in logical pixels, so a pane resize moves it with the screen.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FloatRect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }
/// The first placement: side = min(0.56·w, 0.84·h), right-aligned with an
/// inset, top just below `line_bottom` if it fits, otherwise above the line,
/// clamped inside the screen. 56% is the mockup's number (a proposal).
pub fn float_home(screen_w: f32, screen_h: f32, line_bottom: f32, line_top: f32) -> FloatRect;
pub fn clamp_float(r: FloatRect, screen_w: f32, screen_h: f32) -> FloatRect;

/// The drag, the shape of `PaneDrag` (`main.rs:2857-2873` [m]) held on the pane.
/// Points are flat (un-bent). Engages past 6 px, the literal the pane drag
/// uses at `main.rs:15890` [m].
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FloatDrag { pub start: (f32, f32), pub at: (f32, f32), pub origin: FloatRect, pub engaged: bool }
pub const FLOAT_DRAG_ENGAGE: f32 = 6.0;
/// Moves `at`, engages past the threshold, and answers the rect to draw.
pub fn drag_to(d: &mut FloatDrag, flat: (f32, f32), screen_w: f32, screen_h: f32) -> FloatRect;

/// What part of the float a flat point landed on. The strip's buttons are
/// measured as they paint (the bench's zone pattern, `benchdraw.rs:3045-3063`
/// [m]), so their widths follow the font and no arithmetic guesses them.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum FloatHit { Strip, Split, Desktop, Close, Body }
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FloatZone { pub x: f32, pub y: f32, pub w: f32, pub h: f32, pub hit: FloatHit }
/// Topmost zone containing the point, like `workbench::hit_at` (`workbench.rs:1423` [m]).
pub fn float_hit_at(zones: &[FloatZone], x: f32, y: f32) -> Option<FloatHit>;

/// Why a float is open when a split was asked for. Shown in the float's strip:
/// there is no general toast in TD. [m: the only in-window banner is the
/// agent-done marquee, `main.rs:3725`, gated on a notification pref at `:12050`.]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FloatNote { FourPanes }

/// Device pixels per cell for the PTY, rounded rather than truncated.
/// (6.3, 14.7, 1.6) -> (10, 24); today's `cell_w as u16` gives (6, 14).
pub fn device_cell(cell_w: f32, cell_h: f32, scale: f32) -> (u16, u16);
```

### `pane.rs`: the router, the float, the face

```rust
/// A split was asked for. The workspace decides: focus an existing pane, split,
/// or (at four panes) send the document back as a float. Carries the float's
/// own view when a float is being promoted, so the document is not re-opened.
pub struct OpenDoc {
    pub source: DocumentSource,
    pub carry: Option<Entity<DocumentView>>,
    pub by: Asker,
}
impl gpui::EventEmitter<OpenDoc> for TerminalView {}   // beside DragPaneStart, pane.rs:2354-2357 [m]

/// The floating square. Not saved (decided).
pub(crate) struct FloatingDoc {
    view: Entity<DocumentView>,
    source: DocumentSource,
    rect: FloatRect,
    drag: Option<FloatDrag>,
    note: Option<FloatNote>,
    _links: gpui::Subscription,   // FollowLink -> this pane; dropped with the float
}

/// What the Document face is showing.
pub(crate) struct DocFace {
    view: Entity<DocumentView>,
    source: DocumentSource,
    _links: gpui::Subscription,
}

/// Replica-repair carry (Presentation derives Clone; Subscription does not).
#[derive(Clone)]
pub(crate) struct DocCarry { view: Entity<DocumentView>, source: DocumentSource }

// New fields on TerminalView (pane.rs:1845):
//   float: Option<FloatingDoc>,
//   float_zones: Rc<RefCell<Vec<FloatZone>>>,   // cleared at the top of render
//   doc: Option<DocFace>,
//   scale: f32,                                 // window.scale_factor(), stored by sync_size
//   cell_px: (u16, u16),                        // what the PTY was last told
// Changed field:
//   pending_grid: Option<(term::GridSize, Instant)>   (pane.rs:1937)
//     -> pending_pty: Option<(PtySize, Instant)>
// New fields on Presentation (pane.rs:1835): doc: Option<DocCarry>, float: Option<(DocCarry, FloatRect)>.

#[derive(Clone, Copy, PartialEq, Debug)]
struct PtySize { grid: term::GridSize, cell_px: (u16, u16) }

// CopyHint (pane.rs:229-233) gains one field:
//   does: AltClick,

impl TerminalView {
    // ── the router ──
    pub(crate) fn open_document(&mut self, source: DocumentSource, placement: Placement,
                                by: Asker, window: &mut Window, cx: &mut Context<Self>);
    /// link_under (pane.rs:4293) -> reveal_target (pane.rs:709, decodes file://)
    /// -> docopen::drawable_document. Memoised on the last target string,
    /// because the chip asks on every Alt-held mouse move.
    fn document_under(&self, pos: Point<Pixels>) -> Option<DocumentSource>;

    // ── the float ──
    pub(crate) fn open_float(&mut self, source: DocumentSource, carry: Option<Entity<DocumentView>>,
                             note: Option<FloatNote>, cx: &mut Context<Self>);
    pub(crate) fn note_float(&mut self, note: FloatNote, cx: &mut Context<Self>);
    fn close_float(&mut self, cx: &mut Context<Self>);
    /// Drops the float and its subscription, not the view (it may be moving).
    pub(crate) fn release_float(&mut self, cx: &mut Context<Self>);
    fn promote_float(&mut self, cx: &mut Context<Self>);
    /// Un-bends through workbench::unwarp (workbench.rs:1782 [m]) with self.warp_k,
    /// then float_hit_at. None when no float, or the face is not Terminal.
    fn float_hit(&self, pos: Point<Pixels>) -> Option<(FloatHit, Point<Pixels>)>;
    fn float_press(&mut self, hit: FloatHit, flat: Point<Pixels>, ev: &MouseDownEvent,
                   window: &mut Window, cx: &mut Context<Self>);
    fn float_drag_move(&mut self, ev: &MouseMoveEvent, cx: &mut Context<Self>) -> bool;
    fn float_drag_end(&mut self, cx: &mut Context<Self>) -> bool;
    fn on_float_release_out(&mut self, ev: &MouseUpEvent, w: &mut Window, cx: &mut Context<Self>);
    fn float_el(&self, th: &Theme) -> Option<gpui::AnyElement>;

    // ── the Document face ──
    /// The only way onto the face: sets `doc` and the face together.
    pub(crate) fn show_document(&mut self, source: DocumentSource, carry: Option<Entity<DocumentView>>,
                                scroll: Option<DocScroll>, cx: &mut Context<Self>);
    /// On restore: classifies by name only (the file may be missing) and keeps
    /// the face; the view shows its own "cannot read" state.
    pub(crate) fn restore_document(&mut self, saved_path: &str, scroll: Option<f32>, cx: &mut Context<Self>);
    pub(crate) fn document_path(&self) -> Option<&std::path::Path>;
    pub(crate) fn saved_document(&self, cx: &App) -> Option<(String, Option<f32>)>;
    fn doc_face_press(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool;
    /// Called from scroll_by_wheel (pane.rs:5394) after the size chord, before
    /// the terminal legs: the Document face, or a pointer over the float.
    fn doc_wheel(&mut self, ev: &ScrollWheelEvent, cx: &mut Context<Self>) -> bool;
    fn doc_key(&mut self, ks: &Keystroke, cx: &mut Context<Self>) -> Handled;
    fn follow_doc_link(&mut self, link: &FollowLink, window: &mut Window, cx: &mut Context<Self>);

    // ── size answers (window-owned panes) ──
    fn pty_window_size(&self) -> WindowSize;   // grid + cell_px
}
```

`Presentation::adopt` needs `cx` to re-subscribe, so `adopt_presentation(&mut self, from: Presentation)` (`pane.rs:2988`) becomes `adopt_presentation(&mut self, from: Presentation, cx: &mut Context<Self>)`. Its one caller already has `cx` (`main.rs:6279`) **[m]**.

### `workbench.rs`: the face

```rust
pub enum Face {               // workbench.rs:76-80
    #[default] Terminal,
    Workbench,
    Document,                 // new
}
impl Face {
    pub fn other(self) -> Face;   // Document -> Terminal
    pub fn chip(self) -> &'static str;   // Document -> "DOC"
}
pub fn vignette_on(face: Face, vignette: f32) -> f32;   // Document -> 0.0, the bench's reasoning
/// alt+k. With a document attached it moves between the document and its
/// shell (and back to the document from the bench); without one, as today.
pub fn next_face(now: Face, has_document: bool) -> Face;

impl Bench {
    /// Assigns rather than toggling. Today's body calls `toggle_face` when the
    /// face differs (workbench.rs:4081-4085 [m]), which only works while there
    /// are two faces: set_face(Document) from Terminal would land on Workbench.
    pub fn set_face(&mut self, face: Face);
}
```

**Every site that reads `workbench::Face`**, from `grep -n "Face::\|bench.face()\|vignette_on(\|face_now\|on_bench\|set_face(\|toggle_face("` over `app/src` **[m]**. `paint::Face` (`paint.rs:188-427`, `fav.rs:193-201`) and `toolprop::Face` are different types.

| Site | Today | With `Document` |
|---|---|---|
| `workbench.rs:94-99` `vignette_on` | exhaustive match | learns `Document => 0.0` (compiler forces it) |
| `workbench.rs:124-129` `other` | exhaustive | `Document => Terminal` |
| `workbench.rs:134-139` `chip` | exhaustive | `Document => "DOC"` |
| `workbench.rs:4065-4079` `face`/`toggle_face` | two-way toggle | unchanged; `TerminalView::toggle_face` calls `next_face` instead |
| `workbench.rs:4081-4085` `set_face` | toggles to reach a face | **assigns** (see above) |
| `workbench.rs:4117`, `:4630` | `== Workbench` | correct as is |
| `pane.rs:4585` `note_layout` | no note on `Workbench` | no note unless `Terminal`: the note is the terminal's (Parker's rule quoted at `:4578-4580`) **[i]** |
| `pane.rs:4995` `layers_up` | `bench: face == Workbench` | adds `document`, `float`, `float_caret` |
| `pane.rs:5359` `size_dial_under` | bench or text size | unchanged: ctrl+wheel on a document sizes the pane's text dial |
| `pane.rs:6151` `on_mouse_down` | bench branch | a `Document` branch follows it |
| `pane.rs:6268` `on_mouse_move` hover | bench only | unchanged |
| `pane.rs:6277-6288` Alt chip | any face | only on `Terminal`, and not over the float |
| `pane.rs:6294` bench drag | bench only | unchanged |
| `pane.rs:7303-7315` `set_face`/`toggle_face` | two faces | `toggle_face` uses `next_face(face, self.doc.is_some())`; `set_face(Document)` with no `doc` is refused |
| `pane.rs:7958-7983` header slider | TERM · BENCH | DOC · TERM · BENCH while a document is attached |
| `pane.rs:8027` `on_bench` | bool | becomes a `match face_now` |
| `pane.rs:8195` header label | name or OSC title | the file name on the `Document` face, unless the pane was renamed |
| `pane.rs:8514` `note_el` filter | `!on_bench` | `face_now == Terminal` |
| `pane.rs:8630` crawl | off on bench | only on `Terminal` |
| `pane.rs:8677-8712` grid or bench | two arms | three: grid, bench, `doc_el` |
| `pane.rs:8747` glass vignette | `vignette_on` | through `vignette_on` |
| `pane/bench.rs:472` `bench_wheel` | returns off the bench | correct as is |
| `pane/bench.rs:1475` beacon | `== Workbench` | correct as is |
| `pane/bench.rs:2735-2741` drop | pastes the path into the PTY off the bench | on `Document`, swallowed: the shell is hidden |
| `main.rs:5665` demo | sets `Workbench` | correct as is |
| `main.rs:7481-7499` `set_all_faces` | every leaf | skips leaves on `Document`: `ctl bench off` does not close a document someone is reading |
| `main.rs:7523`, `:7547`, `:7634`, `:7652` | `== Workbench` | correct as is |
| `ctl.rs:476-478` `BenchFace` | two faces | unchanged |
| `surfacefeed.rs:1594` | sets `Workbench` | unchanged |

Two source-scan tests constrain where the face may move, and this design stays inside both: `nothing_in_the_bench_half_flips_the_pane_off_the_bench` (`pane.rs:9899`) forbids `set_face(` in `pane/bench.rs`, and `main.rs:29929-29933` forbids it in `launch_agent` **[m]**. `show_document` lives in `pane.rs`.

### `keylayer.rs`: Esc and the document's keys

```rust
pub enum Layer {
    Help, Face, Window, Paint, CtxMenu, HeaderMenu, Reader, PaneChord, Sticky, Rename,
    /// A floating document. Escape closes it; with a caret in its note dialog it
    /// claims every key, like Sticky. Below Sticky and Rename, so a caret
    /// elsewhere on the pane keeps its Escape.
    Float,
    /// The Document face. Claims every key the window and pane chords leave:
    /// the shell behind it is hidden, and typing into it would be invisible.
    Document,
    Bench, Terminal,
}
pub struct Up {
    // existing: paint, ctx_menu, header_menu, reader, sticky, rename, note, bench
    pub float: bool,         // a float is up on the Terminal face
    pub float_caret: bool,   // DocumentView::has_caret() on that float
    pub document: bool,      // the pane is on its Document face
}
const LADDER: [Rung; 13] = [
    /* … the eleven rungs at keylayer.rs:191-203, unchanged, with two inserted after Rename: */
    (Layer::Float, |k, u| u.float && (k.key == "escape" || u.float_caret)),
    (Layer::Document, |_, u| u.document),
    (Layer::Bench, |_, u| u.bench),
];
```

`TerminalView::on_key` (`pane.rs:4892-4976`) gains two arms: `Layer::Float` → `DocumentView::key` first, then Escape closes the float, anything else declines to the terminal; `Layer::Document` → `doc_key`, which always consumes.

### `main.rs`: the split and the layout file

```rust
/// A document on its way into the state file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SavedDocument {
    path: String,
    /// None = never measured, which is not the top of the page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scroll: Option<f32>,
}

// SavedNode::Leaf (main.rs:736-758), LeafState (:700-708), LeafFields (:771-791) each gain:
//   #[serde(default, skip_serializing_if = "Option::is_none")]
//   document: Option<SavedDocument>,
// An older build ignores the key: LeafFields carries no deny_unknown_fields [m, :771].

/// What a split-beside request becomes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Beside { Focus(usize), Split, Float }
/// Pure: an existing leaf showing `path` wins; then the cap (may_split, :136).
fn beside<L>(leaves: &[L], showing: impl Fn(&L) -> Option<&std::path::Path>,
             path: &std::path::Path) -> Beside;

impl Workspace {
    /// wire_pane's OpenDoc handler.
    fn open_doc_beside(&mut self, from: Entity<TerminalView>, ev: &pane::OpenDoc,
                       window: &mut Window, cx: &mut Context<Self>);
    /// Beside `split` (:9203): a new pane to the RIGHT of `from` (SplitDir::Row,
    /// side b of split_leaf, :359-381), showing the document; focus goes back
    /// to `from`; saves. The shell spawns in `from`'s cwd, like split (:9234).
    fn split_with_document(&mut self, tab: usize, from: &Entity<TerminalView>,
                           source: DocumentSource, carry: Option<Entity<DocumentView>>,
                           window: &mut Window, cx: &mut Context<Self>) -> Entity<TerminalView>;
    /// The tab holding a leaf. Not found today: tab_index_of (:11777-11782) matches a tab's FIRST leaf only [m].
    fn tab_with_leaf(&self, pane: EntityId) -> Option<usize>;
}
```

`wire_pane` gains, beside `DragPaneStart` (`main.rs:4321-4334`):

```rust
cx.subscribe_in(pane, window, |ws, pane, ev: &pane::OpenDoc, window, cx| {
    ws.open_doc_beside(pane.clone(), ev, window, cx);
}).detach();
```

### The chip, the menu, the help

- **Chip** (`pane.rs:8475-8508`): the label comes from `chip_label(&hint.does, copied)` instead of the literal at `:8502-8506`. The frame is unchanged: the logical line's painted rows, or the one row under the pointer on the alt screen.
- **`copy_hint_at`** (`pane.rs:4206-4245`) becomes the Alt-hint resolver: `None` off the `Terminal` face or over the float; otherwise `alt_click(on_alt_screen, is_copyable_command(line), self.document_under(pos))`. The deliverable line in mockup 01 is not a command (its first token is `Deliverable:`), so the chip must arm on it for the document alone **[i]**.
- **Menu** (`pane.rs:7410-7434`): the link rows come from `link_menu(doc.is_some(), reveal_target(&l).is_some())`: `◳ Open here  alt+click`, `⤢ Open beside  ctrl+alt+click`, `↗ Open with desktop  ctrl+click` (today's "Open link ↗"), `⌖ Reveal in folder  shift+click`, `⎘ Copy link`. Copy link copies what `link_under` resolved (the absolute path, or the URL). The menu is anchored and deferred, so its own gpui handlers are safe, as today's are **[i]**.
- **Help** (`main.rs:27377-27383`): five rows replace the two: Ctrl-click opens with the desktop, Shift-click reveals, Super+Ctrl-click reveals, Alt-click opens here, Ctrl+Alt-click opens beside. New `lang.rs` keys: `k_ctrl_click`, `k_shift_click`, `k_alt_click`, `k_ctrl_alt_click`, `open_here`, `open_beside`; `k_shift_ctrl_click` goes.

### The size answers

```rust
// host.rs — the answering thread reads the live size, not a spawn-time copy.
struct HostPane { geom: Arc<Mutex<PaneGeom>>, /* … */ }          // was Mutex<PaneGeom> (:262)
// spawn_pane: `let geom = Arc::new(Mutex::new(geom));` shared with the thread
// at :694-734; the TextAreaSizeRequest arm (:705-713) locks it at answer time.
// HostPane::resize (:809-827) already writes it (:814) [m].

struct TeeReader {                                                // host.rs:134-145
    master: File,
    sink: Arc<Mutex<Option<Sink>>>,
    spoke: Arc<AtomicBool>,
    cell_query: crate::ptyscan::CellSizeQuery,                    // new
    questions: std::sync::mpsc::Sender<TermEvent>,               // new: the HostProxy channel
}
/// The reply formatter handed to the existing answer arm.
fn cell_size_reply(ws: WindowSize) -> String;   // "\x1b[6;{cell_height};{cell_width}t"

// ptyscan.rs
#[derive(Default)]
pub struct CellSizeQuery { /* state: how much of ESC [ 1 6 t has been seen */ }
impl CellSizeQuery {
    /// Feed one chunk; answers how many complete `CSI 16 t` it finished. Keeps
    /// its state across calls, because a read can end mid-sequence.
    pub fn feed(&mut self, bytes: &[u8]) -> usize;
}

// term.rs:94 — a replica swallows the size question as it swallows PtyWrite.
if !self.answers_here && matches!(event, TermEvent::PtyWrite(_) | TermEvent::TextAreaSizeRequest(_)) { return; }

// pane.rs:3909-3975 — handle_term_event, a window-owned pane answers 14 t.
TermEvent::TextAreaSizeRequest(format) =>
    self.session.notifier.notify(format(self.pty_window_size()).into_bytes()),

// pane.rs:3509-3517 — the resize carries device pixels.
view.session.resize(size.grid, size.cell_px.0, size.cell_px.1);   // was view.cell_w as u16, view.cell_h as u16
```

**Where the scale factor is:** `window.scale_factor()` is already read in the screen's prepaint canvas (`pane.rs:8598`) **[m]**. `sync_size(&mut self, th, window)` (`pane.rs:3981`) takes the `Window` and runs every frame from `render` (`:7639`) **[m]**, so it stores `self.scale` and stages `PtySize { grid, cell_px: device_cell(self.cell_w, self.cell_h, self.scale) }`. The resize ticker (`:3509`) has no `Window` **[m]**, which is why the scale is stored rather than read there. Staging compares the whole `PtySize`, so moving the window to a monitor with another scale re-announces the size even when the grid did not change.

Why the `term.rs:94` change is needed: today the replica's proxy forwards `TextAreaSizeRequest` (only `PtyWrite` is filtered) and the pane ignores it (no arm, `pane.rs:3972`) **[m]**. Adding the arm without the filter would answer `14 t` twice in hosted mode, once from the host and once from the window, and the second reply would reach the program as typed input **[i]**.

---

## Call stack

### 1. Alt+click on a Markdown path (terminal face, no float up)

1. gpui → the root div's `on_mouse_down(Left, Self::on_mouse_down)` (`pane.rs:8556`).
2. `TerminalView::on_mouse_down` (`:6122`): commit a rename, focus the pane, `ack_bell` (as today).
3. Not on the bench (`:6151`), not on `Document`, no float → skip those branches.
4. `sticky_click` (`:6177`) → false.
5. `document_under(pos)` → `link_under` (`:4293`: `viewport_cell` un-bends, `stitch_wrapped_line`, `link_at`, `resolve_path` + `exists`) → `reveal_target` (`:709`) → `docopen::drawable_document` → regular file, 16 bytes, `doc_kind` → `Some(DocumentSource { kind: Markdown })`.
6. `click_intent(Mods { alt }, on_document: true, …)` → `OpenHere`.
7. No float on that path → `open_document(src, Placement::Float, Asker::Click, window, cx)` → `open_float(src, None, None, cx)`:
   `cx.new(|cx| DocumentView::new(src, DocSeat::Float, cx))`; `cx.subscribe(&view, …FollowLink…)` kept in `_links`; `rect = float_home(screen_w, screen_h, line_bottom, line_top)` from the chip's painted rows × `cell_h` + `grid_pad_y`; `self.float = Some(…)`; `stop_propagation`; `notify`.
8. Next frame, `render` (`:7370`): `float_zones` cleared; `float_el` = an absolute div at `rect` with `.border_2()`, `.border_color(th.accent)`, `.shadow(crate::float_shadows(th.accent))` (called the same way from `benchdraw.rs:196` **[m]**), a strip whose buttons each carry a zone recorder, and a body holding `view.clone()`. Added as `.children(float_el)` right after `.children(note_el)` (`:8723`), inside the screen: over the grid, the chip and the note, under the glass, and bent by the tube.

### 2. Ctrl+Alt+click on an HTML path, in a tab with three panes

1–5. As in flow 1; `kind: Html`.
6. `click_intent` → `OpenBeside` → `open_document(src, Split, Click)` → `cx.emit(OpenDoc { source, carry: None, by: Click })`; stop.
7. `wire_pane`'s subscriber → `Workspace::open_doc_beside(from, ev, window, cx)`.
8. `debounced()` (`main.rs:7135`): a double-click cannot open two. `tab_with_leaf(from.entity_id())` → tab `t`.
9. `beside(&leaves, |p| p.read(cx).document_path(), &path)` → no leaf shows it; `may_split(3)` → `Beside::Split`.
10. `split_with_document(t, &from, source, None, window, cx)`:
    1. `cwd = from.read(cx).runtime().cwd`.
    2. `make_pane_in_mode(PaneRestore { cwd, ..Default::default() })` (`:4957`) spawns the idle shell, host-side or local. `wire_pane` runs inside it and focuses the new pane (`:4360`, reached from `:4199` and `:4878`) **[m]**.
    3. `new_pane.update(|v, cx| v.show_document(source, None, None, cx))` → `DocumentView::new(src, DocSeat::Face)`; `v.doc = Some(…)`; `v.bench.set_face(Face::Document)`.
    4. `tabs[t].root.split_leaf(&|p| p.entity_id() == from_id, SplitDir::Row, new_pane)` (`:359`) → the new pane is `b`, on the right.
    5. `cx.defer_in(window, move |_, window, cx| window.focus(&from.focus_handle(cx), cx))`: focus goes back to the clicked pane, deferred for the same reason `split` defers (`:9251-9259`).
    6. `self.save(cx)`; `notify`.
11. `render_node` (`:22341`) draws four leaves; the new pane's render takes the `Document` arm.

**With four panes:** step 9 → `may_split(4)` is false → `Beside::Float` → `from.update(|v, cx| v.open_float(source, None, Some(FloatNote::FourPanes), cx))`. The float's strip reads "four panes · opened here". Nothing is split or saved.

**If a pane in the tab already shows that path:** `Beside::Focus(i)` → `window.focus(&leaves[i].focus_handle(cx), cx)`. Nothing is split.

### 3. The float's "click to split"

1. `on_mouse_down` → face is `Terminal` and a float is up → `float_hit(pos)`: `workbench::unwarp(content_rect, warp_k, pos)` → `float_hit_at(&float_zones, fx, fy)` → `FloatHit::Split`.
2. `promote_float(cx)` → `cx.emit(OpenDoc { source, carry: Some(view), by: Asker::Float })`; stop. The float stays up until the workspace answers.
3. `open_doc_beside` → `Beside::Split` → `split_with_document(…, carry)`: `from.update(|v, cx| v.release_float(cx))` first (the float and its `FollowLink` subscription go; the view lives on in `carry`), then `show_document(source, Some(view), None)` → `view.update(|d, cx| d.set_seat(DocSeat::Face, cx))`. Same scroll position and same pixels, with nothing re-rendered.
4. At four panes: `Beside::Float` with `by == Float` → `from.update(|v, cx| v.note_float(FloatNote::FourPanes, cx))`; the float stays where it is.

A second Alt+click on the same path takes flow 1 to step 6, finds `self.float.source.path == src.path`, and calls `promote_float`.

### 4. Esc with a float up

1. `on_key` (`pane.rs:4877`) → `layers_up` (`:4986`) now reports `float: true` → `keylayer::route`: nothing above claims a plain Escape, unless a menu, the reader, a sticky caret or the rename box is up (those keep it, as today) → `Layer::Float`.
2. `DocumentView::key(escape)` → false (no note dialog open) → `close_float(cx)` → `self.float = None` (view and subscription dropped) → `Handled::Consumed` → `stop_propagation`.
3. With no float, Escape reaches the terminal exactly as today.

### 5. Dragging the float

1. `on_mouse_down` → `float_hit` → `FloatHit::Strip` → `float.drag = Some(FloatDrag { start: flat, at: flat, origin: rect, engaged: false })`; stop.
2. `on_mouse_move` (`:6264`): `float_drag_move` runs first. If `ev.pressed_button != Some(Left)`, the drag ends (a release outside the pane was missed). Otherwise it un-bends `ev.position` and calls `drag_to(&mut drag, flat, w, h)`, which engages past 6 px and returns `clamp_float(origin + delta)`; then `rect = that`, `notify`, and return before the grid's own drag code. Deltas are measured between un-bent points, so the square stays under the pointer as the glass shows it **[i]**.
3. `on_mouse_up` (`:6317`): `float_drag_end` runs before `bench_release_at`. gpui delivers the pane's mouse-up and mouse-move only while the pane is hovered (`div.rs:213-216`, `:302` in `zed-upstream/crates/gpui/src/elements/`) **[m]**, so the root also registers `.on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_float_release_out))` (`div.rs:276-290`) **[m]**. The float cannot leave its pane: `clamp_float` keeps it inside the screen.
4. Not saved.

### 6. Restart with a document split saved

1. Save: `Workspace::save` → `Node::to_saved` (`main.rs:675`) → `LeafState { document: view.saved_document(cx).map(…) }` → `to_saved_with` (`:642`) → `SavedNode::Leaf { document: Some(SavedDocument { path, scroll }) }` → TOML, `[…Leaf.document] path = "…" scroll = 0.37`. The host stores it opaquely (`hostproto.rs` `Save`), and its round-trip of unknown fields is tested by `a_save_keeps_every_field_this_build_has_never_heard_of` (host.rs, `mod owning`) **[i: by the test's name]**.
2. Load → `build_node_attached` (`:4978`, hosted) or `build_node` (`:5056`, serverless) builds the leaf exactly as today: the idle shell is bound if the host still runs it, respawned otherwise. Then, new: `if let Some(d) = document { pane.update(cx, |v, cx| v.restore_document(&d.path, d.scroll, cx)) }`.
3. `restore_document` → `doc_kind_by_name` → `show_document(source, None, scroll.map(|top| DocScroll { top }))` → `DocumentView::new(…, DocSeat::Face)` + `restore_scroll`. A missing file keeps the face and the layout entry; the view says it cannot read the file.

### 7. A program sends `CSI 16 t` (hosted mode)

1. The program writes `ESC [ 1 6 t` → the host's PTY master becomes readable → alacritty's `EventLoop` reader calls `TeeReader::read` (`host.rs:148`).
2. `master.read` → tee to the attached window's sink (as today) → `cell_query.feed(&buf[..read])` → 1 → `questions.send(TermEvent::TextAreaSizeRequest(Arc::new(cell_size_reply)))`. The variant takes `Arc<dyn Fn(WindowSize) -> String + Sync + Send>` (`alacritty_terminal-0.26.0/src/event.rs:43`) **[m]**, so no new event type is needed.
3. The parser then advances over the same bytes; vte has no arm for 16 (`vte-0.15.0/src/ansi.rs:1739-1745`) **[m]** and drops it.
4. The host's event thread (`host.rs:694`) → the `TextAreaSizeRequest` arm (`:705`) → `format(WindowSize` from the live `geom`) → `answers.0.send(Msg::Input("\x1b[6;24;10t"))` → the program reads it (cell 10 × 24 device pixels at scale 1.6).
5. The window's replica parses the same bytes, vte drops them again, and nothing is forwarded. A serverless window does not answer `16 t` (see least-confident 9).

`CSI 14 t` takes steps 4–5 only, via alacritty's own `text_area_size_pixels` (`term/mod.rs:2259-2264`) **[m]**, now answered from the live geometry.

---

## Test plan

TD has no gpui test harness (`#[gpui::test]` appears 0 times in `pane.rs`, `main.rs`, `workbench.rs`, `host.rs`) **[m]**. So, like the existing suite, the tests are pure functions, source scans (cut at the test module, comments stripped, the way `the_pane_root_takes_a_file_drop` does at `pane.rs:11705-11710` **[m]**), and real-PTY host tests. "Fails today" is stated for each test; "does not compile" means a symbol it needs does not exist yet.

**`app/src/docopen.rs`, its own `mod tests`** (standalone; `rustc --test` works)
- `a_markdown_html_or_image_name_is_drawable_and_anything_else_is_not`: `doc_kind_by_name` on every listed extension in both cases → `Some`; `.txt`, `.pdf`, no extension → `None`. Fails today: does not compile.
- `a_png_name_on_bytes_that_are_not_an_image_is_not_drawable`: `doc_kind("x.png", b"hello")` → `None`; the PNG signature → `Some(Image)`. Fails today: does not compile.
- `an_image_with_no_extension_is_recognised_by_its_first_bytes`: `doc_kind("shot", JPEG signature)` → `Some(Image)`. Fails today: does not compile.
- `a_directory_named_like_a_document_is_not_drawable`: `drawable_document` on a temp dir named `notes.md` → `None`. Fails today: does not compile.
- `every_click_on_a_path_does_what_the_gesture_table_says`: one row per line of mockup 04 through `click_intent`, including `Shift` on a revealable path → `Reveal`, `Shift` on a web link → `OpenWithDesktop`, `Ctrl+Alt` off a document with a chip → `CopyChip`, `Super+Ctrl` → `Reveal`. Fails today: does not compile, and the Shift row states the behaviour change (today Shift opens, `pane.rs:6240` **[m]**).
- `the_alt_chip_offers_open_here_on_a_line_that_is_not_a_command`: `alt_click(false, false, Some(doc))` → `OpenHere`; `alt_click(false, true, None)` → `Copy`; `alt_click(true, true, None)` → `None`; `alt_click(true, false, Some(doc))` → `OpenHere`. Fails today: does not compile.
- `the_chip_says_open_here_over_a_document_and_copy_over_a_command`: `chip_label` strings. Fails today: does not compile.
- `the_menu_on_a_document_path_offers_open_here_open_beside_and_copy_link` and `the_menu_on_a_web_link_offers_neither_open_here_nor_reveal`. Fails today: does not compile.
- `a_float_opens_below_its_line_and_above_it_near_the_bottom`: `float_home` with the line at the top vs the last row; the rect is always inside the screen. Fails today: does not compile.
- `a_float_dragged_past_the_edge_stays_inside_its_pane` and `a_float_drag_engages_only_past_six_pixels`. Fails today: does not compile.
- `a_press_on_a_bent_float_is_found_where_the_glass_shows_it`: with k1 = 0.2, a pointer whose flat position misses the float but whose un-bent position (through `pane::warp_screen_to_content`) lands on its strip → `Strip`. This one uses the real inverse, so it lives in `pane.rs`'s `mod tests`. Fails today: does not compile.
- `the_pty_is_told_its_cell_in_device_pixels`: `device_cell(6.3, 14.7, 1.6) == (10, 24)`, `device_cell(6.3, 14.7, 1.0) == (6, 15)`. Fails today: does not compile.

**`app/src/ptyscan.rs`, `mod tests`**
- `the_scanner_finds_a_cell_size_question_split_across_two_reads`: `ESC [ 1` then `6 t` → 0, then 1.
- `the_scanner_counts_two_questions_in_one_read`.
- `the_scanner_ignores_other_window_ops_and_lookalikes`: `CSI 14 t`, `CSI 18 t`, `CSI 116 t`, `CSI ? 16 t`, and a lone `16t` in text → 0. (`CSI 16 ; t` is left out on purpose: xterm may read a trailing empty parameter as `16 t`, and I have not checked **[h]**.)
All three fail today: the module does not exist.

**`app/src/keylayer.rs`, `mod tests` (`:364`)**
- `escape_closes_a_floating_document_before_it_reaches_the_terminal`: `Up { float }` + Escape → `Float`; `ch("a")` → `Terminal`.
- `a_caret_elsewhere_on_the_pane_keeps_escape_over_a_float`: `sticky` + `float` → `Sticky`; `rename` + `float` → `Rename`.
- `a_note_dialog_in_the_float_takes_every_key`: `float` + `float_caret` + `ch("a")` → `Float`.
- `the_document_face_claims_every_key_but_the_window_and_pane_chords`: `document` + `ch("a")` → `Document`; `alt("w")` → `Window`; `ctrl_shift("c")` → `PaneChord`.
- `the_topmost_claiming_layer_always_wins` (`:454`) extends to `0u16..2048` over eleven fields, and `of_any_two_layers_the_earlier_declared_one_wins` (`:502`) gains the `float` and `document` setters.
All fail today: the fields and layers do not exist.

**`app/src/workbench.rs`, `mod tests`**
- `setting_the_document_face_lands_on_it_from_either_face`: `set_face(Document)` from `Terminal` and from `Workbench` → `Document`. Fails today: does not compile; with the variant added and `set_face` unchanged, it fails by landing on `Workbench`.
- `alt_k_on_a_document_pane_moves_between_the_document_and_its_shell`: `next_face(Document, true) == Terminal`, `next_face(Terminal, true) == Document`, `next_face(Workbench, true) == Document`, `next_face(Terminal, false) == Workbench`. Fails today: does not compile.
- `the_document_face_has_no_vignette`: `vignette_on(Document, 0.7) == 0.0`. Fails today: does not compile.

**`app/src/pane.rs`, `mod tests` (`:8763`)**
- `a_press_on_a_bent_float_is_found_where_the_glass_shows_it` (above).
- `a_file_url_is_decoded_before_it_is_classified`: `reveal_target("file:///tmp/a%20b.md")` then `doc_kind_by_name` → `Markdown`. Fails today: does not compile (`doc_kind_by_name`).
- `the_pane_click_handler_decides_through_click_intent`: scan `on_mouse_down`: it calls `click_intent(` before any `open_with_system(` or `reveal_with_system(`, and the `shift || control` branch at `:6240` is gone. Fails today: neither holds.
- `a_window_owned_pane_answers_the_text_area_question`: scan `handle_term_event` for a `TermEvent::TextAreaSizeRequest` arm. Fails today: no arm (`:3909-3975` **[m]**).
- `the_resize_carries_device_pixels`: scan the ticker for `cell_px`, and that `view.cell_w as u16` no longer appears. Fails today: `:3514` has it **[m]**.
- `the_float_is_drawn_inside_the_screen_after_the_note`: scan `render` for `.children(float_el)` after `.children(note_el)` and before the screen's closing `)` ahead of `crt::glass`. Fails today: no float.
- `the_pane_releases_a_float_drag_outside_itself`: scan for `.on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_float_release_out))`. Fails today.
- `a_repaired_replica_keeps_its_document`: scan `presentation()` and `adopt_presentation` for `doc` and `float`. Fails today: `Presentation` has neither (`:1835-1843` **[m]**).

**`app/src/main.rs`, `mod tests` (`:28902`)**
- `a_document_leaf_round_trips_its_path_and_scroll_through_the_state_file`: `Tree<u32>::to_saved_with(&|_| LeafState { document: Some(…), ..Default::default() })` → TOML → `SavedNode` → same path and scroll; a `None` scroll stays `None`, not `0.0`. Fails today: does not compile.
- `a_layout_written_before_documents_reads_every_leaf_as_a_terminal`: an old TOML string → `document: None`. Fails today: does not compile (it is a guard, not a driver).
- `a_document_already_showing_in_the_tab_is_focused_not_opened_twice`: `beside(&[a, b, c], …, path_of_b)` → `Focus(1)`. Fails today: does not compile.
- `a_fifth_pane_is_refused_and_the_document_floats_instead`: `beside` over four leaves, none showing it → `Float`; over three → `Split`. Fails today: does not compile.
- `the_document_split_keeps_focus_on_the_pane_that_was_clicked`: scan `split_with_document`: `from.focus_handle` inside `defer_in`, `SplitDir::Row`, `show_document(`, `save(cx)`. Fails today: no such function.
- `ctl_bench_off_leaves_a_document_pane_on_its_document`: scan `set_all_faces` for a `Document` guard. Fails today.
- `split_leaf_dir_places_the_dropped_pane_on_the_chosen_side` (`:35305`) is the model for all the above and stays untouched.

**`app/src/term.rs`, `mod attached` (`:644`)**
- `a_replica_swallows_a_size_question_the_host_answers`: `EventProxy::new(tx, gen, Answers::Elsewhere)`, `send_event(TextAreaSizeRequest(…))` → `rx.try_recv()` is empty; with `Answers::Here` it arrives. Modelled on `:736-781`. Fails today: the replica forwards it (`:94` filters only `PtyWrite` **[m]**).

**`app/src/host.rs`, `mod owning` (`:2116`)**
- `a_size_question_is_answered_with_the_size_the_pane_is_now`: `host_with_cat_pane()` (`:2141`), `host.resize(pane, PaneGeom { cols: 50, rows: 10, cell_width: 10, cell_height: 24 })`, `write_to(pane, b"\x1b[14t\n")`. `cat` prints the line back, the host answers, and the reply is typed into the PTY, where the line discipline echoes it. So `within(5s)` some row contains `[4;240;500t`. Fails today: the answer uses the spawn-time 8×16 default (`PaneGeom::default`, `hostproto.rs:64-72`) **[m for the default; i for the echo form, which I expect as `^[[4;240;500t`]**.
- `a_cell_size_question_is_answered_by_the_host`: the same with `\x1b[16t` → a row contains `[6;24;10t`. Fails today: nothing answers `16 t`.

---

## Least confident decisions

1. **The face lives in two places.** `Face` is a field of `Bench` (`workbench.rs:3668`) while the `DocumentView` is a field of `TerminalView`, so "on the Document face" and "has a document" can disagree. `show_document` sets both and `set_face(Document)` without a document is refused, but that is discipline, not a type. The alternative is to leave `workbench::Face` alone and give `TerminalView` its own `doc: Option<DocFace>` that outranks the bench face in `render`. That touches fewer `Face` sites and makes the extra state honest, at the cost of Gate 2's wording ("`workbench::Face` gains `Document`").
2. **The `Document` face swallows typing.** The alternative, letting unclaimed keys reach the idle shell, types into something nobody can see; this pane has been bitten by exactly that (`pane.rs:6162-6166`, "invisible, and the slot this takes"). The cost: a person who clicks into a document pane and types gets nothing until alt+k.
3. **The document view must do no hit-testing of its own.** If the other half uses `overflow_y_scroll` or gpui click handlers, they will work on a flat pane and miss on a bent one, and nothing will fail a test. This is the largest integration risk between the two halves, and it needs saying in their design too.
4. **The `16 t` reply can overtake other replies.** The scanner fires when the bytes are read, before the parser has answered anything earlier in the same chunk. A program that sends `CSI c` (DA1) and then `CSI 16 t` in one write gets the `16 t` reply first. The usual probe order is the query first and DA1 last as a sentinel, which this gets right **[h]**. If the order matters, the fix is to answer from inside the parser, which means patching vte.
5. **Promotion moves the view entity into the new pane** rather than re-opening the file. It keeps the scroll position and avoids a second render, but it means one `DocumentView` changes owner mid-life, so its link subscription has to be dropped and re-made exactly once.
6. **At four panes the float explains itself in its own strip**, because TD has no general toast (not found). If Parker wants the reason louder, that is a new window-level channel.
7. **An existing document pane gets focus**, as the brief says, even though every other split in this feature keeps focus on the clicked pane. The alternative is to flash it and leave focus alone.
8. **Shift+click on a web link keeps opening it**, since there is nothing to reveal. The alternative is to fall through to selection.
9. **Serverless panes answer `14 t` but not `16 t`.** The scanner sits in the host's tee only, as the brief places it; a window-owned pane would need a reader wrapper of its own. Hosted mode is the default, so this should rarely matter **[h]**.
10. **What the Document face gives up:** no sticky note, no vignette, no crawl. That extends Parker's "workbench = no sticky note" rule to documents; he has not said so.
11. **Carried and not carried.** `Presentation` carries the document and the float across a replica repair. Tear-off (seeded from a `PaneRestore`, `main.rs:36738`) and reopen-closed (`ClosedPane`, `main.rs:2293`) do not, so a torn-off or reopened document pane comes back as its shell.
12. **The scroll is saved as a fraction of the page.** It survives a width change, but it is approximate after a Markdown reflow. The other half may prefer an anchor id.
13. **A missing file on restore keeps its face** and shows a "cannot read" state, rather than quietly turning the leaf back into a shell. That keeps "not there right now" (an unmounted drive) apart from "never was a document".
14. **The copy chip's label changes too**, to "⎘ alt+click · copy", following mockup 01. Gate 1 only decided the open-here label. Keeping "⎘ alt+click" is a one-line revert.
15. **Rounding for device pixels.** `round` gives 10 × 24 for a 6.3 × 14.7 cell at 1.6; `floor` gives 10 × 23. Programs size images from `ws_ypixel / rows`, so rounding up by half a pixel slightly overstates the cell, where `floor` understates it.
