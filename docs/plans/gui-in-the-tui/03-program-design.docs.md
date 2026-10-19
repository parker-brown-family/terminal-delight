# Program Design: GUI in the TUI — the document half

_Draft for Gate 3, one of two halves. This half is the document view and everything under it: the entity a pane holds, the image, Markdown and HTML backends, the page engine that drives Chromium, the page cache, and TD's notes layer over decision briefs. The other half, `03-program-design.td-wiring.md`, owns everything between a click and a view on screen, and it asked this half for eight calls; they are answered in "The seam" below._

_Written against the build worktree at de7776b, the spike write-up `spikes/snapshot-engine.md`, and the TD-wiring draft as it stood at 13:41. **[m]** = measured: I opened the line or ran the command. **[i]** = inferred from lines I read. **[h]** = hunch. Paths without a prefix are in `app/src/`. gpui paths are in `/home/parker/Work/zed-upstream/crates/`._

## The seam

The wiring half's request is accepted as written, with one change and six additions: four methods, one event and one free function. Nothing inside the view registers a gpui mouse, scroll or hover handler: every press, move, wheel and key arrives through these methods, already un-bent by the pane, in flat coordinates relative to the view's own top-left. That is the constraint `pane.rs:8468-8474` and `pane/bench.rs:317-365` already live under **[m]**, and it rules out `overflow_y_scroll`, `InteractiveText::on_click` and `.on_mouse_down` anywhere under `docview`. The test `nothing_in_the_document_view_listens_for_the_mouse` pins it.

```rust
impl DocumentView {
    // asked for by the wiring half, unchanged
    pub fn new(source: DocumentSource, seat: DocSeat, cx: &mut Context<Self>) -> Self;
    pub fn set_seat(&mut self, seat: DocSeat, cx: &mut Context<Self>);
    pub fn restore_scroll(&mut self, at: DocScroll, cx: &mut Context<Self>);
    pub fn press(&mut self, at: Point<Pixels>, mods: Modifiers, window: &mut Window, cx: &mut Context<Self>) -> bool;
    pub fn wheel(&mut self, delta: ScrollDelta, cx: &mut Context<Self>);
    pub fn key(&mut self, ks: &Keystroke, cx: &mut Context<Self>) -> bool;
    pub fn has_caret(&self) -> bool;
    // changed: None until a layout exists, and always None for an image, which has no scroll to save.
    // The wiring half already stores Option<f32> (its SavedDocument), so the None has somewhere to go.
    pub fn scroll(&self) -> Option<DocScroll>;
    // added
    pub fn set_theme(&mut self, theme: Arc<Theme>, cx: &mut Context<Self>); // the pane's resolved theme, when it changes
    pub fn hover(&mut self, at: Option<Point<Pixels>>, cx: &mut Context<Self>); // None = the pointer left the view
    pub fn drag(&mut self, at: Point<Pixels>, cx: &mut Context<Self>);          // moved with the left button held
    pub fn release(&mut self, cx: &mut Context<Self>);
}
pub struct FollowLink { pub target: String }     // as asked: an absolute path (with #fragment) or a URL
impl EventEmitter<FollowLink> for DocumentView {}
pub struct CannotShow { pub reason: String }     // added: the pane hands the file to the desktop
impl EventEmitter<CannotShow> for DocumentView {}
/// Added, for the router before it places an HTML document. A cached PATH lookup; no process starts.
pub fn html_ready(cx: &mut App) -> Result<(), engine::Unavailable>;
```

- **Why `set_theme`:** a pane can carry its own theme (`PaneTheme`, `theme.rs:1192`), resolved per pane by `TerminalView::resolved_theme` (`pane.rs:2726`, a `Theme` by value, called at the top of `render`, `:7372`) **[m]**. The global `theme::theme(cx)` (`theme.rs:184`) would paint a document in the wrong palette on a re-themed pane. The pane wraps the resolved theme in an `Arc` and passes it only when it differs from the last one.
- **Why `hover`, `drag`, `release`:** note buttons appear on hover, as they do in the browser (`notes.css:6-16`) **[m]**, and an image pans by dragging.
- **Why `CannotShow` and `html_ready`:** Gate 2 says a missing Chromium falls back to the desktop "and says so". `html_ready` lets the router decide before any float appears; `CannotShow` covers a browser that exists but fails to start after the view is placed. Where the sentence is shown (the float's strip, the chip) is the wiring half's call.
- **`key` and Escape:** Escape answers true while a note dialog or one of the brief's own dialogs is open, and closes it; otherwise false, so the wiring half's Escape closes the float as its flow 4 says. `has_caret` is true only while a note dialog is open.
- **Naming, for the wiring half:** `docopen::DocumentSource` collides with the trait `doc::DocumentSource` (`doc.rs:84`), which `pane.rs:9` imports by name **[m]**. `pane.rs` will need `docopen::DocumentSource` spelled with its module path or imported under another name.

## Files

**New**, all window-side. The entity is `app/src/docview.rs` with its children in `app/src/docview/`, the shape `pane.rs` and `pane/bench.rs` already use (`pane.rs:15-17`) **[m]**. Not `doc.rs` or `document.rs`: `doc.rs` is the FOCUS reader's line model (`doc.rs:1-30`), and a reader of `pane.rs`, which uses both, should never have to guess which "doc" a line means.

- `app/src/docview.rs` — `DocumentView`: the seam above, backend dispatch, the file watcher, zones and link spans measured at paint.
- `app/src/docview/image.rs` — the image backend: gpui's decoder, fit, zoom, pan, and giving the texture back.
- `app/src/docview/markdown.rs` — markdown-delight's `render.rs`, lifted (482 lines, MIT, same Zed pin), with the three changes below.
- `app/src/docview/page.rs` — the HTML backend's view side: tiles, scroll, a brief's own dialogs, page-to-view geometry.
- `app/src/docview/engine.rs` — `trait PageEngine`, the data it returns, the per-process engine global, selection by config.
- `app/src/docview/snapshot.rs` — the first engine: headless Chromium, one browser per TD process, one tab per open page.
- `app/src/docview/cdp.rs` — a DevTools client over `--remote-debugging-pipe`. Separate from `snapshot.rs` so its framing and demultiplexing are tested without a browser.
- `app/src/docview/extract.js` — the script the engine runs in the page to collect anchors, links, openers and capability. A `.js` file so it reads as JavaScript, pulled in with `include_str!`.
- `app/src/docview/cache.rs` — `$XDG_CACHE_HOME/terminal-delight/pages/`.
- `app/src/docview/notes.rs` — the brief notes format: byte-precise regions, parsing, `JSON.stringify` output, `buildMap`, the splice writer, the atomic commit. No gpui, so every rule is a plain test.
- `app/src/docview/notes_ui.rs` — the gpui half of notes: buttons, concur zones and stamps, the note dialog, the notes bar.
- `app/src/docview/pref.rs` — `~/.config/terminal-delight/documents.toml`, the `launchpref.rs`/`notifpref.rs` pattern (one small file per preference, every field an `Option`) **[m]**.
- `app/assets/img/concur-stamp.svg` — the stamp's ink as one monochrome drawing, for gpui's `svg()` mask with rotation (`gpui/src/elements/svg.rs:37`, `:44`, `:223`) **[m]**.
- `app/tests/fixtures/decision-brief/notes-format/` — a vendored, byte-identical copy of the skill's fixtures plus `SOURCE`, naming the agent-skills commit it came from. See "Shared fixtures".
- `app/tests/snapshot_engine.rs` — engine against a real Chromium and the fixture briefs.
- `scripts/sync-brief-fixtures` — copies the fixtures from the skill; `--check` exits non-zero on any difference.
- `.github/workflows/brief-fixtures-drift-watch.yml` — weekly compare against agent-skills `main` that opens an issue and never fails the build, the shape of `collector-drift-watch.yml:1-11` **[m]**.

**Changed**
- `app/Cargo.toml` — `comrak = { version = "0.52", default-features = false }` and `base64 = "0.22"`.
- `app/Cargo.lock` — +8 packages, all comrak's: caseless, comrak, entities, finl_unicode, jetscii, phf_codegen, typed-arena, unicode-normalization **[m: resolved alone, diffed against TD's Linux tree]**. `base64` 0.22.1 is already there through alacritty_terminal and usvg, so it adds nothing **[m]**.
- `app/src/main.rs` — `mod docview;` in the list at `:30-87`; `EditBuffer` (`:1711`) gains `pub(crate) fn insert(&mut self, s: &str)` so a note can hold a newline. `apply` (`:1784`) leaves Enter to its caller **[m]**.
- `app/src/benchdraw.rs` — the Markdown arms of `compact` (`:912`) and `full` (`:993`) both call a new `markdown(` that uses `docview::markdown`; the delegation test's list (`:4459-4466`) gains the Markdown row.
- `THIRD-PARTY-LICENSES.md` — comrak (BSD-2-Clause, already allowed at `app/deny.toml:15` **[m]**) and the lifted markdown-delight module.
- In agent-skills (proposed, not TD): `decision-brief/fixtures/notes-format/`, with the spike's `notes-writer.mjs` promoted to be the reference writer there.

**Not touched by this half:** `pane.rs`, `pane/bench.rs`, `workbench.rs`, `keylayer.rs`, `docopen.rs`, `host.rs`. Those are the wiring half's.

---

## Types & signatures

### `docview.rs`

```rust
pub mod cache; pub mod cdp; pub mod engine; pub mod image; pub mod markdown;
pub mod notes; pub mod notes_ui; pub mod page; pub mod pref; pub mod snapshot;

use crate::docopen::{DocKind, DocScroll, DocSeat, DocumentSource};

pub struct DocumentView {
    source: DocumentSource,
    seat: DocSeat,              // Float or Face; changes only the notes bar's inset
    theme: Arc<Theme>,
    zoom: Zoom,
    backend: Backend,
    frame: Rc<RefCell<Frame>>,  // what the last paint measured
    links: markdown::LinkSink,  // cleared at the top of render, filled as paragraphs are built
    hover: Option<Point<Pixels>>,
    seen: Option<FileStamp>,    // None before the first stat
    own_write: Option<FileStamp>,
    _watch: Task<()>,
}

/// Measured at paint by canvas recorders (the benchdraw::zone pattern, benchdraw.rs:3045-3063).
/// Flat window coordinates; `press(at)` adds `origin` before looking anything up.
#[derive(Default)]
struct Frame {
    origin: Option<Point<Pixels>>,
    size: Option<Size<Pixels>>,
    zones: Vec<DocZone>,
}

enum Backend { Image(image::ImageDoc), Markdown(markdown::MarkdownDoc), Page(page::PageDoc), Unshown(Unshown) }

/// A document this view cannot draw: the reason as a sentence, and an "open with desktop" zone.
pub struct Unshown { pub reason: String }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FileStamp { pub mtime: SystemTime, pub len: u64 }
impl FileStamp { pub fn of(path: &Path) -> Option<FileStamp>; }

/// Discrete steps, like a browser's, so a re-render lands on a width the cache has seen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Zoom(u8);
pub const ZOOM_STEPS: [f32; 11] = [0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];

pub struct DocZone { pub rect: Bounds<Pixels>, pub hit: DocHit }
pub enum DocHit {
    Link(usize),                     // index into the page's links
    NoteButton(String),              // nid
    Concur(String),                  // nid
    Opener(String),                  // a brief's own dialog, by id
    Dialog(notes_ui::DialogHit),
    Bar(notes_ui::BarHit),
    CloseBriefDialog,
    OpenWithDesktop,
}

pub enum LinkTarget { Fragment(String), File { path: PathBuf, fragment: Option<String> }, Url(String) }
/// An href resolved against the document's folder: percent-decoded, `file://` stripped, `#` kept.
pub fn resolve_link(doc_dir: &Path, href: &str) -> LinkTarget;

pub const WATCH_EVERY: Duration = Duration::from_millis(500);
```

The watcher is the house pattern: TD watches no file with inotify (`notify` is not in `app/Cargo.lock` **[m]**). The skin and theme hot-reload poll `mtime` every 300 ms from a `cx.spawn` loop (`skin.rs:1834-1860`, `theme.rs:4603`) **[m]**, and the view does the same, only while it is open.

### `docview/image.rs`

gpui already decodes PNG, JPEG, WebP, GIF, BMP, TIFF, ICO, PNM and SVG on a background task (`gpui/src/elements/img.rs:605-748`) **[m]**. The backend keeps that loader and takes ownership of the texture:

```rust
pub struct ImageDoc {
    resource: Resource,                              // Resource::Path, the key gpui caches it under
    loaded: Option<Result<Arc<RenderImage>, String>>, // None = still decoding
    zoom: ImageZoom,
    centre: Point<f32>,                              // image pixels under the view's centre
    pan: Option<PanDrag>,
}
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ImageZoom { Fit, Scale(f32) }               // Scale(1.0) = one image pixel per device pixel
pub struct PanDrag { start: Point<Pixels>, centre_at_start: Point<f32> }
/// gpui gives the app no texture limit (not found). The atlas clamps a texture to the
/// device limit and then fails to allocate (gpui_wgpu/src/wgpu_atlas.rs:185-195), so a
/// larger image would draw nothing. 16,384 is the lower of the two limits the spike saw.
pub const MAX_SIDE: u32 = 16_384;

/// Pure. Fit is contain, and never above 1:1.
pub fn fit_scale(img: Size<DevicePixels>, view: Size<Pixels>, scale_factor: f32) -> f32;
pub fn image_rect(img: Size<DevicePixels>, view: Size<Pixels>, scale_factor: f32, zoom: ImageZoom, centre: Point<f32>) -> Bounds<Pixels>;
pub fn step_zoom(zoom: ImageZoom, fit: f32, up: bool) -> ImageZoom;
pub fn clamp_centre(centre: Point<f32>, img: Size<DevicePixels>, view: Size<Pixels>, zoom: f32, scale_factor: f32) -> Point<f32>;

impl ImageDoc {
    pub fn new(path: &Path) -> Self;
    /// window.use_asset::<ImgResourceLoader> (window.rs:3351; img.rs:37) → img(ImageSource::Render(arc)).
    fn element(&mut self, view: Size<Pixels>, th: &Theme, window: &mut Window, cx: &mut App) -> AnyElement;
    /// cx.remove_asset::<ImgResourceLoader> (app.rs:2349), then cx.drop_image(old, None) (app.rs:2436).
    fn release(&mut self, cx: &mut App);
}
```

Keys: `0` fit, `1` actual size, `+`/`-` zoom about the centre, arrows pan. The wheel pans. Ctrl+wheel is not the view's: the wiring half keeps it for the pane's text dial. Transparent pixels sit on `gpui::checkerboard` (`gpui/src/color.rs:841`) **[m]**.

### `docview/markdown.rs` — the lift

**What is copied**, from markdown-delight at `425041b`, `/home/parker/BROWN-FAMILY-SPORTS/Software/markdown-delight/app/src/`:

| From | Lines | Becomes |
|---|---|---|
| `render.rs` | 1-16, 25-482 (all but the palette constants at 17-23) | this module, with the three changes below |
| `comments.rs` | 96-103 `BlockMeta` | copied as is |
| `comments.rs` | 113-125 `normalize`, `fingerprint` | copied as is |
| `comments.rs` | `now_millis`, `doc_key`, threads | not copied |
| `render.rs` | 353-358 `paragraph_text` | dropped: its only caller is markdown-delight's comment mode |

Both apps pin `zed_rev = abbe85a3…` (TD `app/Cargo.toml:16`) and `render.rs` calls nothing from TD's glyph-transform patch, so the module compiles as copied **[i, from the research write-up's skew table]**. comrak stays at 0.52 so the copy compiles unchanged; 0.55.0 is current (2026-09-06) **[m, crates.io]**, and moving to it is a separate change. Its defaults (`cli`, `syntect`, `bon`) pull clap, syntect and the onig C library, and `render.rs` uses none of them; with `default-features = false` comrak adds 8 packages **[m]**. The extensions `md_options()` sets (`render.rs:46-53`) are runtime `Options`, not cargo features **[i]**.

```rust
pub type Runs = Vec<(Range<usize>, HighlightStyle)>;              // render.rs:28, unchanged
pub struct Inline {                                               // render.rs:30-33
    text: SharedString,
    runs: Runs,
    links: Vec<(Range<usize>, String)>,                           // change 2: the byte range and its url
}
pub enum Block {                                                  // render.rs:35-44
    Heading { level: u8, inline: Inline }, Paragraph(Inline), Code(Vec<SharedString>),
    Quote(Vec<Block>), List(Vec<(SharedString, Vec<Block>)>), Table(Vec<(bool, Vec<Inline>)>),
    Rule, Html(Vec<SharedString>),
    Image { src: String, alt: SharedString },                     // change 3
}
pub struct BlockMeta { pub fp: u64, pub plain: String, pub src: Range<usize> }   // comments.rs:97-103
pub fn fingerprint(text: &str) -> u64;                                           // comments.rs:120-125

pub struct MdDoc { pub blocks: Vec<Block>, pub meta: Vec<BlockMeta>, pub dir: Option<PathBuf> }
/// Was `parse_with_meta(text) -> (Vec<Block>, Vec<BlockMeta>)` (render.rs:60).
pub fn parse(text: &str, dir: Option<&Path>) -> MdDoc;
pub fn block_plain(block: &Block) -> String;                      // render.rs:97, plus the Image arm

// ── change 1: a palette parameter, from TD's theme tokens ──
pub struct MdPalette { pub bg: Hsla, pub surface: Hsla, pub text: Hsla, pub accent: Hsla, pub faint: Hsla, pub complement: Hsla }
impl MdPalette { pub fn from_theme(th: &Theme) -> MdPalette; }  // Theme fields at theme.rs:122-134
pub struct MdStyle { pub palette: MdPalette, pub body: Pixels, pub zoom: f32 }
impl MdStyle {
    pub fn document(th: &Theme, zoom: f32) -> MdStyle;
    pub fn bench(sk: &Skin, th: &Theme) -> MdStyle;               // body = sk.pt(Step::Body), as paragraph() uses (benchdraw.rs:2324)
}

// ── change 2: link targets kept, and routed out ──
/// A paragraph's laid-out text and its link ranges, pushed at build time and read after paint.
pub struct LinkSpan { pub layout: TextLayout, pub links: Vec<(Range<usize>, String)> }
pub type LinkSink = Rc<RefCell<Vec<LinkSpan>>>;
/// TextLayout::index_for_position (gpui/src/elements/text.rs:808), Ok hits only.
pub fn link_at(spans: &[LinkSpan], flat: Point<Pixels>) -> Option<String>;

// ── change 3: images, from the document's folder ──
/// Local images decoded through the same loader `img(path)` uses (ImgResourceLoader, img.rs:37),
/// resolved by the view in render, where it has a Window. Keyed by doc.dir.join(src).
/// A path absent from the map is still decoding; one mapped to Err could not be read.
pub type Images = HashMap<PathBuf, Result<Arc<RenderImage>, String>>;

/// Was `block_element(&Block)` (render.rs:345) and `element(&Block)` (render.rs:366).
/// `links: None` draws link text in the accent with no underline: the bench, where there is nothing to press.
/// `images: None` draws every image as its placeholder: the bench again.
pub fn block(block: &Block, doc: &MdDoc, style: &MdStyle, links: Option<&LinkSink>, images: Option<&Images>) -> AnyElement;
/// Every block, top to bottom, with a canvas under each recording its top for scroll and re-anchoring.
pub fn document(doc: &MdDoc, style: &MdStyle, links: Option<&LinkSink>, images: Option<&Images>,
                tops: Option<&Rc<RefCell<Vec<Pixels>>>>) -> Div;
/// Every local image path the document draws, for the view to resolve before building.
pub fn image_paths(doc: &MdDoc) -> Vec<PathBuf>;
/// The bench's memo: a surface body parsed once rather than every frame. Thread-local, 32 entries.
pub fn parsed(body: &str) -> Rc<MdDoc>;

// ── view-side state ──
pub struct MarkdownDoc {
    doc: Option<Result<Rc<MdDoc>, String>>,    // None = still reading
    top: Pixels,                               // manual scroll; no overflow_y_scroll (see The seam)
    height: Option<Pixels>,
    tops: Rc<RefCell<Vec<Pixels>>>,
    images: Images,                            // every local image drawn, so each texture is given back
}
/// Pure. After a reload, the block whose fingerprint matches the one that was at the top,
/// or the nearest by index when that block is gone.
pub fn reanchor(old: &[BlockMeta], top: usize, new: &[BlockMeta]) -> usize;
```

**The three changes, as behaviour:**

1. **Palette.** The six constants at `render.rs:17-23` go, and every `rgb(CONST)` reads a field. Headings, links, list markers and the quote bar take `accent`; code and quote backgrounds and the table header take `surface`; rules and borders take `faint`; inline code text takes `complement`, so it is not mistaken for a link; body text takes `text`; muted text (quotes, strikethrough, raw HTML) is `text` at 62%; the page ground is `bg`. `human` is left unused on purpose: it is the colour of Parker's own typing, and nothing in a document is his typing.
2. **Links.** `collect_inline` (`render.rs:171-253`) records `(range, url)` on `NodeValue::Link`, which also covers autolinks, instead of keeping only the text (`render.rs:234-246`). Paragraphs are plain `StyledText` (`render.rs:360-364`) with the layout handle (`text.rs:412`) pushed to the sink. `InteractiveText` is not used: its click handler is a gpui hitbox. A press finds the link through `link_at` and `resolve_link`. A `#fragment` scrolls to the heading whose slug matches; anything else is emitted as `FollowLink`.
3. **Images.** A paragraph that holds only images, soft breaks and whitespace becomes one `Block::Image` per image. The image is `doc.dir.join(src)`, percent-decoded, loaded by the same `ImgResourceLoader` that `img(path)` uses. The view resolves it with `window.use_asset` (`gpui/src/window.rs:3351`) and `block()` draws `img(ImageSource::Render(arc))` with `ObjectFit::ScaleDown` (`gpui/src/style.rs:29-40`), capped at the column width. A bare `img(path)` would draw the same pixels, but the view would never hold the `Arc` it needs to give the texture back, and a Markdown file full of screenshots, opened and closed through a day, would keep every one of them in the atlas **[i]**. An image inside running text draws its alt text as a link to the image file. An `http(s)` source is not fetched, since the view makes no network requests; it draws a dashed box with the alt text and the host. `dir: None`, the bench's case, draws the same box for relative sources.

**The bench as a second caller.** `benchdraw.rs:993` draws a Markdown surface as plain lines through `paragraph` (`:2324-2335`), and the compact arm at `:912` shows its first six raw lines, `#` and `**` included **[m]**. Both arms call one new function:

```rust
// benchdraw.rs
enum CardSize { Compact, Full }
/// Full: docview::markdown::document(&parsed(&m.body), &MdStyle::bench(sk, th), None, None, None).
/// Compact: the blocks whose source starts in the first six lines, then "…" if more follow.
fn markdown(m: &Markdown, size: CardSize, sk: &Skin, th: &Theme) -> Div;
```

The delegation test (`benchdraw.rs:4443-4479`) gains `("Kind::Markdown(m)", "markdown(")`, so the two sizes cannot drift apart. `first_lines` (`:4012`) stays for its other callers.

### `docview/engine.rs`

The interface follows the spike's "What the engine interface needs to return" section item for item. Two additions: the rects of the page's own hidden `.note-btn` and `.concur-zone`, so TD draws its buttons exactly where a browser draws them, and `read_back`, the spike's step 8 as a call.

```rust
/// Pixels and geometry for an HTML page. Never writes the file and never owns the notes.
/// Blocking: called from cx.background_executor().spawn, the pattern at main.rs:5525-5528.
pub trait PageEngine: Send + Sync {
    fn name(&self) -> &'static str;
    /// Load and lay out once. Everything returned comes from that one layout pass.
    fn open(&self, req: &PageRequest) -> Result<PageLayout, EngineError>;
    /// Same page instance, new width or zoom, no reload. A new generation.
    fn relayout(&self, page: PageId, geometry: Geometry) -> Result<PageLayout, EngineError>;
    /// A band of the current layout, full layout width, as PNG. Err(Stale) for an old generation.
    fn tile(&self, page: PageId, generation: u64, band: Band) -> Result<Tile, EngineError>;
    /// One of the page's own <dialog>s, opened by id rather than through the page's wiring.
    fn dialog(&self, page: PageId, generation: u64, id: &str) -> Result<DialogRender, EngineError>;
    /// Load the file fresh in a throwaway page and report what the brief's own script shows.
    fn read_back(&self, path: &Path, geometry: Geometry) -> Result<ReadBack, EngineError>;
    fn close(&self, page: PageId);
}
// A live engine later adds input and frames through a second trait. Nothing above changes.

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)] pub struct PageId(pub u64);
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Geometry { pub css_width: u32, pub viewport_css_height: u32, pub scale: f32 } // scale = window scale × zoom
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)] pub struct LayoutHash(pub u64);
pub struct PageRequest { pub path: PathBuf, pub geometry: Geometry, pub expect: LayoutHash }
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)] pub struct Band { pub top_dev: u32, pub height_dev: u32 }
pub const TILE_DEV: u32 = 2048;   // the spike's tile height: 12.1 MiB of BGRA at 1549 wide
pub struct Tile { pub band: Band, pub png: Vec<u8>, pub width_dev: u32, pub height_dev: u32 }

#[derive(Clone, Serialize, Deserialize)]
pub struct PageLayout {
    pub page: Option<PageId>,        // None when read from the cache: no live page behind it
    pub generation: u64,
    pub rendered: LayoutHash,
    pub geometry: Geometry,
    pub height_css: f32,
    /// notes.js's FILE: window.NOTES_FILE, else the location's basename (notes.js:35-36).
    /// Percent-encoded as location gives it. None when no notes.js ran.
    pub notes_file: Option<String>,
    pub capability: NotesCapability,
    pub anchors: Vec<Anchor>,        // document order: querySelectorAll('.notable'), what buildMap walks
    pub links: Vec<Link>,
    pub openers: Vec<Opener>,
    pub dialogs: Vec<String>,        // every <dialog> but notes.js's own #d-note and #d-export
    pub diagnostics: Vec<String>,    // page errors, dead openers, dialogs that overflowed
}
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct RectCss { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }   // page coordinates, or the dialog's box
#[derive(Clone, Serialize, Deserialize)]
pub struct Anchor {
    pub nid: String, pub title: String, pub tag: String,
    pub dialog: Option<String>,
    pub rect: Option<RectCss>,        // None inside a closed dialog, where the browser reports zero size
    pub button: Option<RectCss>,      // the page's own .note-btn, hidden with visibility, so it keeps its box
    pub concur_zone: Option<RectCss>, // Some only where this brief's notes.js made one
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Link { pub href: String, pub resolved: String, pub rect: Option<RectCss>, pub dialog: Option<String>, pub fragment_top_css: Option<f32> }
#[derive(Clone, Serialize, Deserialize)]
pub struct Opener { pub dialog: String, pub rect: Option<RectCss>, pub inside: Option<String> }
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct NotesCapability { pub notes_islands: u32, pub concurs_island: bool, pub tagged: u32, pub concur: ConcurSupport }
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ConcurSupport { Supported, NotSupported, Unknown }
#[derive(Clone, Serialize, Deserialize)]
pub struct DialogRender { pub id: String, pub width_dev: u32, pub height_dev: u32, pub grew_viewport: bool,
                          pub anchors: Vec<Anchor>, pub links: Vec<Link>, pub openers: Vec<Opener>,  // rects relative to the dialog's box
                          #[serde(skip)] pub png: Vec<u8> }                                            // cached as a file beside the JSON
pub struct ReadBack { pub with_notes: Vec<String>, pub concurred: Vec<String>, pub errors: Vec<String> }

pub enum EngineError { Unavailable(Unavailable), Launch(String), Timeout(&'static str), FileChanged, Stale, Closed, Page(String), Protocol(String) }
pub enum Unavailable { Off, UnknownEngine(String), NoBrowser { searched: Vec<PathBuf> } }
impl Unavailable { pub fn sentence(&self) -> String; }

/// One engine per TD process, created on first use from documents.toml, shared by every window.
/// An engine, once made, is kept; an Unavailable answer is asked again after 30 s, so installing
/// Chromium does not need a TD restart.
pub struct Engines { engine: Option<(Result<Arc<dyn PageEngine>, Unavailable>, Instant)> }
impl Global for Engines {}
pub fn engine(cx: &mut App) -> Result<Arc<dyn PageEngine>, Unavailable>;
```

`ConcurSupport` is read in the page: `Supported` when any `.concur-zone` exists or an inline script defines `function concurZones`; `NotSupported` when notes.js tagged anchors and neither holds; `Unknown` when nothing was tagged. The spike found 1 concur-capable brief and 92 without **[m, spike §1]**, so `NotSupported` is the common case.

### How TD drives Chromium

Four candidates, four criteria. Crate counts come from resolving each candidate alone in a scratch project, taking `cargo tree --target x86_64-unknown-linux-gnu -e normal,build`, and counting the packages with no semver-compatible entry in TD's own tree **[m]**.

| | `chromiumoxide` 0.9.1 | `headless_chrome` 1.0.22 | own client, WebSocket (`tungstenite` 0.29) | own client, `--remote-debugging-pipe` |
|---|---|---|---|---|
| Runtime | Needs tokio: a non-optional dependency with `rt-multi-thread`, `process`, `fs` **[m, crates.io]**. TD has no tokio in `app/Cargo.lock` **[m]**, so it would run a second async runtime on its own thread beside gpui's executors | Blocking calls, own threads: fits TD's thread-plus-channel pattern (`ctl.rs:856-872`) **[m/i]** | Blocking; fits | Blocking; one reader thread and two file descriptors; the `ctl.rs` pattern exactly |
| Packages added to TD | **32**: tokio, hyper, reqwest, four tower crates, async-tungstenite, four chromiumoxide crates… | **35**: ureq, rustls, ring, webpki-roots, derive_builder, three darling crates, auto_generate_cdp… (TLS for a local connection) | **5**: tungstenite, sha1, httparse, data-encoding, syn 3 | **0**: `serde_json`, `libc`, `futures`, `base64` are already in the lock |
| Licence | MIT OR Apache-2.0 | MIT | MIT OR Apache-2.0 | TD's MIT |
| Maintenance | last release 2026-02-25 | last release 2026-06-11 | tungstenite 0.30.0, 2026-07-11 | ours: 16 protocol names (list below) |
| Transport | `--remote-debugging-port` (`chromiumoxide-0.9.1/src/browser/config.rs:379-380`) **[m]** | `--remote-debugging-port` (`headless_chrome-1.0.22/src/browser/process.rs:345`) **[m]** | a port | no port |

TD drives Chromium with its own client over `--remote-debugging-pipe`, which adds no package and no async runtime, and opens no port. A debugging port listens on 127.0.0.1, where any local process can connect and drive a browser that reads `file://` **[i]**; with the pipe, only TD holds the descriptors. It is also the transport the spike measured through: Playwright launches Chromium with `--remote-debugging-pipe` (playwright-core `lib/coreBundle.js:42518`) and frames each JSON message with a trailing NUL on fds 3 and 4 (`:38796-38845`, `:8752`) **[m]**. The cost is owning the protocol code. The snapshot engine needs 16 names: `Target.createTarget`, `Target.attachToTarget`, `Target.closeTarget`, `Browser.close`, `Page.enable`, `Page.navigate`, `Page.loadEventFired`, `Page.addScriptToEvaluateOnNewDocument`, `Page.captureScreenshot`, `Page.javascriptDialogOpening`, `Page.handleJavaScriptDialog`, `Runtime.evaluate`, `Emulation.setDeviceMetricsOverride`, `Emulation.setEmulatedMedia`, `Network.enable`, `Network.setBlockedURLs`.

### `docview/cdp.rs`

```rust
/// Chromium reads fd 3 and writes fd 4; one JSON message per NUL byte.
pub struct Cdp {
    out: Mutex<Box<dyn Write + Send>>,
    waiting: Mutex<HashMap<u64, mpsc::Sender<Result<Value, CdpError>>>>,
    expects: Mutex<Vec<(Option<SessionId>, &'static str, mpsc::Sender<Value>)>>,
    next: AtomicU64,
}
#[derive(Clone, PartialEq, Eq, Hash, Debug)] pub struct SessionId(pub String);
pub enum CdpError { Closed, Timeout, Remote { code: i64, message: String }, Io(String), Parse(String) }
pub struct Expect { rx: mpsc::Receiver<Value> }

impl Cdp {
    /// pipe2(O_CLOEXEC) twice; pre_exec dup2s the child's ends onto 3 and 4 and sets
    /// PR_SET_PDEATHSIG(SIGKILL), so a crashed TD takes Chromium with it. Starts "td-cdp-read".
    pub fn spawn(binary: &Path, args: &[OsString]) -> io::Result<(Child, Arc<Cdp>)>;
    /// The same client over any stream pair: the tests use UnixStream::pair.
    pub fn over(read: Box<dyn Read + Send>, write: Box<dyn Write + Send>) -> Arc<Cdp>;
    pub fn call(&self, session: Option<&SessionId>, method: &str, params: Value, within: Duration) -> Result<Value, CdpError>;
    /// Register before the call that causes the event, or the event can arrive first.
    pub fn expect(&self, session: Option<&SessionId>, method: &'static str) -> Expect;
}
impl Expect { pub fn wait(self, within: Duration) -> Result<Value, CdpError>; }
/// Pure: split a byte stream into messages, keeping an unfinished tail.
pub fn frames(pending: &mut Vec<u8>, incoming: &[u8]) -> Vec<Vec<u8>>;
```

The reader thread answers `Page.javascriptDialogOpening` itself with `Page.handleJavaScriptDialog { accept: false }` on that session. The spike measured that an unanswered `confirm()` blocks the page's main thread **[m, spike §7]**.

### `docview/snapshot.rs`

```rust
pub struct SnapshotEngine {
    pref: pref::Html,
    browser: Mutex<Option<Browser>>,
    pages: Mutex<HashMap<PageId, Arc<Mutex<LivePage>>>>,  // one lock per page: a relayout never interleaves with a capture
    next: AtomicU64,
    thread: mpsc::Sender<EngineJob>,                       // "td-page-engine": launches and idle shutdown
}
struct Browser { cdp: Arc<cdp::Cdp>, child: Child, profile: PathBuf, stderr_tail: Arc<Mutex<VecDeque<u8>>> }
struct LivePage { target: String, session: cdp::SessionId, generation: u64, geometry: Geometry, path: PathBuf }
enum EngineJob { Launch(mpsc::Sender<Result<Arc<cdp::Cdp>, EngineError>>), Idle }

pub const EXTRACT_JS: &str = include_str!("extract.js");
pub const EXTRACT_VERSION: u32 = 1;   // part of the cache key
/// Spike §3: hiding these with visibility moved no anchor in 119 files.
pub const HIDE_NOTES_UI: &str = ".notebar, .note-btn, .concur-zone { visibility: hidden !important; }";
pub const IDLE_SHUTDOWN: Duration = Duration::from_secs(300);

impl SnapshotEngine {
    pub fn new(pref: pref::Html) -> Self;
    /// pref.chromium if executable; else PATH for chromium, chromium-browser, google-chrome-stable, google-chrome.
    pub fn locate(pref: &pref::Html, path_var: Option<&OsStr>) -> Result<PathBuf, Unavailable>;
    pub fn launch_args(profile: &Path, pref: &pref::Html) -> Vec<OsString>;
}
impl PageEngine for SnapshotEngine { /* the six methods */ }
```

- **Launch args:** `--headless`, `--remote-debugging-pipe`, `--user-data-dir=$XDG_RUNTIME_DIR/terminal-delight/chromium-<td pid>` (a fresh profile, so the page's `localStorage` is empty and the file's own islands win, which is the precedence problem of spike §2 turned the right way round), `--no-startup-window`, `--no-first-run`, `--no-default-browser-check`, `--disable-extensions`, `--disable-sync`, `--disable-background-networking`, `--disable-component-update`, `--mute-audio`, `--hide-scrollbars`, `--force-color-profile=srgb`. With `gpu = "vulkan"`, the spike's four Vulkan flags (`q7-screencast.mjs:11`) **[m]**. Stale profile directories whose pid is dead are removed at launch.
- **Why a thread launches Chromium:** `PR_SET_PDEATHSIG` fires when the *thread* that forked the child exits, not the process **[i, prctl(2)]**. Launching from a gpui pool thread could kill Chromium whenever the pool retires a thread, so "td-page-engine" launches and lives as long as TD.
- **Per page:** `Target.createTarget` → `attachToTarget { flatten: true }` → `Page.enable`, `Network.enable`, `Network.setBlockedURLs` for `http://*`, `https://*`, `ws://*`, `wss://*` (with `network` unset) → `setDeviceMetricsOverride { width: css_width, height: viewport_css_height, deviceScaleFactor: scale }` → `setEmulatedMedia { prefers-reduced-motion: reduce }` (the spike saw something animate in one brief) → `addScriptToEvaluateOnNewDocument` for an error hook and the `HIDE_NOTES_UI` style.
- **Idle:** with no page open for `IDLE_SHUTDOWN`, `Browser.close`, reap the child, delete the profile. The spike measured 402–570 MiB PSS for the browser with one brief, 265 MiB with the Vulkan flags **[m]**.

### `docview/cache.rs`

```rust
pub fn root() -> PathBuf;   // $XDG_CACHE_HOME (absolute) else ~/.cache, + terminal-delight/pages; usage.rs:356-360's rule
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)] pub struct CacheKey(pub u64);
/// The full path, not the basename: notes_file is derived from the path, and two copies may differ in it.
pub fn key(path: &Path, layout: LayoutHash, geometry: Geometry, engine: &str, extract_version: u32) -> CacheKey;
/// One render's layout and every tile of it. Complete or absent, never partial.
pub struct Cached { pub layout: PageLayout, pub tiles: Vec<(Band, PathBuf)>, pub dialogs: Vec<(String, PathBuf)> }
pub fn lookup(root: &Path, key: CacheKey) -> Option<Cached>;
/// Written into <key>.partial/ and renamed to <key>/ once the last tile has landed.
pub fn store(root: &Path, key: CacheKey, layout: &PageLayout, tiles: &[(Band, &[u8])]) -> io::Result<()>;
pub fn store_dialog(root: &Path, key: CacheKey, render: &DialogRender) -> io::Result<()>;   // JSON plus render.png beside it
pub const CAP_BYTES: u64 = 256 << 20;
/// Oldest last-used first, until the total is under the cap.
pub fn sweep(root: &Path, cap: u64) -> io::Result<u64>;
```

**Complete or absent** is the spike's layout-drift finding applied to disk: a brief lays out 17–29 CSS px taller from the third load in one context **[m, spike §3]**, so a tile captured by a later page instance can disagree with tiles from an earlier one at the seam. A cache entry holds one instance's layout and all of its tiles. A warm open therefore never needs the browser for pixels.

**Invalidation is by content, not by mtime.** The key carries a `LayoutHash` of the file's bytes with the notes regions masked out (see `notes.rs`). A save by TD, which touches only those regions, keeps the key and the render. Any other byte changing makes a new key, and the old entry ages out in `sweep`. PNGs run 0.4–5.2 MB per page **[m, spike §3]**, so 256 MiB holds roughly 50 to 600 pages **[i]**.

### `docview/page.rs`

```rust
pub struct PageDoc {
    engine: Result<Arc<dyn PageEngine>, Unavailable>,
    layout: Option<PageLayout>,      // None until the first render or cache hit
    tiles: TileSet,
    scroll_dev: u32,                 // device px from the page top: whole pixels, so tile edges land on pixel edges
    dialog: Option<OpenDialog>,
    notes: Option<notes_ui::NotesLayer>,  // None: no island, or a build input; the page is shown and takes no notes
    status: PageStatus,
    work: Option<Task<()>>,
    pending_geometry: Option<(Geometry, Instant)>,   // a resize or zoom waiting out its 250 ms debounce
}
pub enum PageStatus { Reading, Rendering, Ready, ChangedOnDisk, Gone, Failed(String) }

/// Every Arc<RenderImage> the view creates lives in exactly one slot here.
pub struct TileSet {
    generation: u64,
    png: BTreeMap<u32, Arc<[u8]>>,           // top_dev → PNG; the whole page, 0.4–5.2 MB
    gpu: BTreeMap<u32, Arc<RenderImage>>,    // at most MAX_RESIDENT
    decoding: BTreeSet<u32>,
}
pub const MAX_RESIDENT: usize = 5;           // visible ±1; 61 MiB at 1549 wide (spike §3)
pub struct Residency { pub decode: Vec<u32>, pub drop: Vec<u32>, pub fetch: Vec<u32> }
/// Pure. Visible tiles and one either side, nearest the viewport first; everything else dropped.
pub fn plan_residency(all: &[u32], resident: &[u32], have_png: &[u32], view_top_dev: u32, view_h_dev: u32) -> Residency;

/// Pure. Page CSS coordinates → flat view coordinates.
pub struct PageToView { pub origin: Point<Pixels>, pub px_per_css: f32, pub scroll: Pixels, pub clip: Bounds<Pixels> }
pub fn place(rect: RectCss, map: &PageToView) -> Option<Bounds<Pixels>>;   // None when clipped out

pub struct OpenDialog { id: String, render: Option<DialogRender>, image: Option<Arc<RenderImage>>, scroll_dev: u32 }
```

**Tiles to `RenderImage`s, and back.**

1. **Decode on the pool:** `gpui::Image::from_bytes(ImageFormat::Png, png).to_image_data(svg_renderer)` (`gpui/src/platform.rs:2141`, `:2182-2237`) decodes the PNG, swaps RGBA to BGRA, and wraps one `Frame` in a `RenderImage` at scale factor 1.0 (`gpui/src/assets.rs:61-69`) **[m]**. TD needs no direct dependency on `image`. The spike measured about 20 ms per 2,048-px tile and 10 ms for a viewport **[m/i, spike §3]**.
2. **Draw:** `img(ImageSource::Render(arc))`, absolutely positioned, sized explicitly to the tile's logical size, `ObjectFit::Fill`. The first paint uploads it (`gpui/src/window.rs:4101`), and each tile taller than 1,024 px gets an atlas texture of its own (`gpui_wgpu/src/wgpu_atlas.rs:185-195`) **[m]**.
3. **Give back:** a `Render` source never frees itself (`img.rs:579`) **[m]**, so the view does. A tile leaving residency, a generation replaced, a file re-rendered, the view released: each calls `cx.drop_image(arc, None)` (`app.rs:2436-2446`), which removes every frame's atlas tile (`window.rs:4149-4160`) and frees the texture when nothing else uses it (`wgpu_atlas.rs:130-155`) **[m]**. A decode that finishes for an evicted slot or a stale generation is dropped without a `drop_image`, because it was never painted and so is not in the atlas (`app.rs:2435`: "a no-op if the image is not in the sprite atlas") **[m]**.

**Note buttons, from page coordinates to device pixels under scroll.** With the view's flat origin `O`, its logical width `V`, the layout's `css_width` `W`, and the window's scale factor `s`:

- `px_per_css = V / W`. This equals the zoom once the page has been re-rendered at `V / zoom`, and it stretches the old tiles during the 250 ms before a resize re-renders.
- `scroll = scroll_dev / s` logical pixels.
- A rect `r` lands at `x = O.x + r.x·px_per_css`, `y = O.y + r.y·px_per_css − scroll`, `w = r.w·px_per_css`, `h = r.h·px_per_css`; gpui multiplies by `s` to reach device pixels.
- Tile `k` lands at `O.y + (k·TILE_DEV − scroll_dev) / s`: integer device rows, so tiles never blur against each other.
- TD's button uses `anchor.button`, the box of the page's own `.note-btn` (26×26 CSS at top 8, right 8, `notes.css:7-8`; inline in table rows, `:26-29`) **[m]**. When that box is absent, TD falls back to the element's top-right corner inset by 8 px. Inside an open dialog, `O` is the dialog image's top-left, and rects are relative to the dialog's box. An anchor with `rect: None` gets no button until its dialog opens.

### `docview/notes.rs` — the format

Everything here works on bytes, and every rule is the spike's reference writer (`spikes/snapshot-engine/notes-writer.mjs`) in Rust, so the shared fixtures can hold both to the same bytes.

```rust
pub struct Island { pub element: Range<usize>, pub inner: Range<usize> }
pub struct Regions {
    pub notes: Option<Island>,          // the FIRST <script … id="report-notes">: what getElementById returns
    pub notes_count: usize,             // one corpus brief has two (spike §2); only the first is ever written
    pub concurs: Option<Island>,        // the first id="report-concurs"
    pub mirror: Option<Range<usize>>,   // the LAST "<!--\nREADER NOTES —" through its "-->", only if it follows the notes island
    pub body_close: Option<usize>,      // the last "</body>", ASCII case-insensitive
    pub notes_js: Option<Range<usize>>, // the inline script holding "function tag()" and "reader notes"
}
pub fn find_regions(bytes: &[u8]) -> Regions;

pub struct Note { pub text: String, pub title: Option<String>, pub ts: Option<String>, pub extra: serde_json::Map<String, Value> }
pub struct NoteMap(pub Vec<(String, Vec<Note>)>);   // the island's own key order
pub struct ConcurMap(pub Vec<(String, String)>);    // nid → "YYYY-MM-DD HH:MM", UTC
pub fn read_notes(bytes: &[u8], r: &Regions) -> Result<NoteMap, Unreadable>;
pub fn read_concurs(bytes: &[u8], r: &Regions) -> Result<ConcurMap, Unreadable>;   // no island reads as an empty map
pub struct Unreadable(pub String);

/// JSON.stringify(map, null, 1), then "</" → "<\/" and "<!" → "<!".
pub fn island_json_notes(n: &NoteMap) -> String;
pub fn island_json_concurs(c: &ConcurMap) -> String;
/// "--!>" → "--! >", then "-->" → "-- >". Parker's own "---" is left alone.
pub fn comment_safe(s: &str) -> String;
/// notes.js buildMap() at skill commit 250188f (notes.js:325-348).
pub fn build_map(file_label: &str, notes: &NoteMap, concurs: &ConcurMap, anchors: &[(&str, &str)]) -> String;
/// "<!--\nREADER NOTES —\n" + comment_safe(map) + "\n-->"
pub fn mirror_comment(map: &str) -> String;
/// new Date().toISOString().slice(0, 16).replace('T', ' ') (notes.js:104, :312): UTC, to the minute.
pub fn utc_minute(t: SystemTime) -> String;
/// A hash of the bytes with the notes island's inner text, the whole concurs island and the
/// last mirror comment left out: notes are data, not layout.
pub fn layout_hash(bytes: &[u8], r: &Regions) -> LayoutHash;
/// notes.js seed() (notes.js:165-169): FNV-1a over UTF-16 code units, scaled to 0..1.
pub fn stamp_seed(s: &str) -> f64;
pub struct StampPose { pub degrees: f32, pub dx: f32, pub dy: f32 }
pub fn stamp_pose(nid: &str) -> StampPose;   // notes.js:171: -12 + 9·seed(id), (seed(id+":x") − ½)·6, (seed(id+":y") − ½)·6

pub enum NoteEdit {
    Add { nid: String, title: String, text: String, ts: String },
    Delete { nid: String, text: String, ts: Option<String> },   // found by its words, not its index
    Concur { nid: String, ts: String },
    Unconcur { nid: String },
}
pub struct Splice { pub what: &'static str, pub range: Range<usize>, pub bytes: Vec<u8> }
pub struct WritePlan { pub out: Vec<u8>, pub splices: Vec<Splice>, pub notes: NoteMap, pub concurs: Option<ConcurMap> }
pub enum Refusal { NoIsland, IslandAfterScript, Unreadable(String), BuildInput, UnterminatedMirror,
                   ConcursUnsupported, AnchorGone(String), Symlink, NotUtf8Text }
pub fn plan_write(bytes: &[u8], edits: &[NoteEdit], anchors: &[(&str, &str)], label: &str,
                  concur: ConcurSupport, path: &Path) -> Result<WritePlan, Refusal>;
/// The island re-parses to the intended map, and every byte outside the splices equals the original.
pub fn verify(before: &[u8], plan: &WritePlan) -> Result<(), String>;

pub struct Backups { pub dir: PathBuf, pub keep: usize }
/// $XDG_STATE_HOME/terminal-delight/brief-backups/<hash of the path>/, the last 8 kept.
/// XDG_STATE_HOME resolves as surfacefeed.rs:60-65 does.
pub fn backups_for(path: &Path) -> Backups;
pub enum WriteError { Refused(Refusal), Changed, Io(String) }
pub struct Written { pub backup: PathBuf, pub stamp: FileStamp }
/// Backup, then a temporary file in the same directory, fsync, mode copied, a re-stat to be sure
/// nobody wrote meanwhile, rename over the original, fsync the directory.
pub fn commit(path: &Path, read: &[u8], plan: &WritePlan, backups: &Backups) -> Result<Written, WriteError>;
```

**Parsing, byte for byte.**

- **The notes island.** Markers are matched as ASCII in the raw bytes; the file is never decoded as a whole. The open tag is the first match of `<script\b[^>]*\bid\s*=\s*["']?report-notes["']?[^>]*>`, case-insensitive. The inner text runs from the end of that tag to the first case-insensitive `</script` after it, and the element ends at the next `>`. The spike's finder uses exactly this rule and picked the same element as `document.getElementById` in all 94 islands in the corpus **[m, spike §2]**; keeping it character for character is the point. It shares one looseness with the reference: `data-id="report-notes"` would also match. No brief has one today, and the post-write read-back would catch it.
- **The concurs island:** the same rule with `report-concurs`.
- **Island text:** the inner bytes decoded as UTF-8, whitespace-trimmed, and an empty string read as `{}`, which is `JSON.parse(island.textContent || '{}')` (`notes.js:76`, `:89`) **[m]**. Script text is not entity-decoded, in the browser or here.
- **The mirror:** the last occurrence of the bytes `<!--\nREADER NOTES \xE2\x80\x94`, through the first `-->` after it. It counts only when it follows the notes island's end: inline notes.js carries the phrase `'\nREADER NOTES —\n'` as a string (`notes.js:404`) but never preceded by `<!--` **[m]**.
- **Order:** `serde_json` in TD's graph already has `preserve_order` (the lock lists `indexmap` as a serde_json dependency, and `zed-upstream/Cargo.toml:730` turns the feature on) **[m]**, so a parsed island keeps the file's key order. A test pins that, because losing the feature would reorder every island TD writes.
- **Unknown fields:** a note's keys other than `text`, `title` and `ts` ride along in `extra`, so a field another writer adds survives TD's save.

**The writer, in order**, the spike's "smallest safe write rule" with TD's names:

1. Read the file fresh at save time. If `layout_hash` differs from the one the anchors were rendered from, re-render first (flow 6) and then retry once.
2. Parse both islands from those bytes and apply the pending edits as deltas. A map held in memory never overwrites the file.
3. Refuse in these cases: no notes island (26 corpus files, spike §6); a notes island that comes after notes.js (0 today, and notes.js could not read it); an unreadable island; a file whose name starts with `_` (11 build inputs); a symlink; an `Add` whose nid is not among the anchors (it would be a note no browser shows); any concur edit when concur support is not `Supported`.
4. Splice at most three regions. The notes island's inner text becomes `island_json_notes`. The concurs island's inner text is replaced, or, only when there are concurs to write and no island exists, a `<script type="application/json" id="report-concurs">…</script>` is inserted right after the notes island's `</script>`, where notes.js's own save puts it (`notes.js:392-399`) **[m]**. The mirror is replaced if one exists, else inserted before the last `</body>`, else appended at the end of the file. On a brief whose notes.js predates concurs, the concurs island is never touched and the map is built with no concurs. That map equals what the brief's own notes.js writes: the three older releases share one `buildMap`, and it matches today's whenever the concur count is zero **[m: the three are byte-identical to each other; the only differences from 250188f are the concur clauses]**.
5. `verify`, then `commit`.
6. Ask the engine to `read_back` the written file in a throwaway page, and compare the nids that show a note and a stamp with the map just written. A mismatch keeps the file and says where the backup is.

**`buildMap`, exactly** (`notes.js:325-348`) **[m]**:

```
NOTES — <FILE>
<count> notes on <els> elements[ · <cc> concur|concurs].
Each [anchor] is an element id in that file — search it to find the passage.

[<nid>] <title>[  ✓ concur]
  · <text, each run of \n replaced by one space>

```

The lines are joined with `\n`. `count` sums every key's notes and `els` counts every key, including keys whose anchor no longer exists, but the blocks walk only anchors in document order. An orphaned note is counted and not listed. An anchor gets a block if it has a note or a concur. Only LF runs are collapsed, so a CR survives as it does in the browser. `<FILE>` is the page's `notes_file`, not TD's path: two corpus briefs set `NOTES_FILE` to another name **[m, spike §1]**.

### `docview/notes_ui.rs`

```rust
pub struct NotesLayer {
    label: String,                      // PageLayout.notes_file
    base: NoteMap,                      // as last read from the file
    base_concurs: ConcurMap,
    pending: Vec<NoteEdit>,             // added in the dialog, not yet in the file
    writable: Result<(), Refusal>,
    concur: ConcurSupport,
    dialog: Option<NoteDialog>,
    saving: Option<Task<()>>,
    said: Option<Said>,
}
pub struct NoteDialog { pub nid: String, pub title: String, pub draft: crate::EditBuffer }   // main.rs:1711
pub enum Said { Saved { file: String }, SavedUnconfirmed { backup: PathBuf, why: String }, Refused(String), Failed(String) }
pub enum DialogHit { Add, Close, Delete(usize) }
pub enum BarHit { CopyMap, Save }

impl NotesLayer {
    /// None when the file has no notes island or is a build input: the page takes no notes.
    pub fn new(bytes: &[u8], regions: &Regions, layout: &PageLayout, path: &Path) -> Option<NotesLayer>;
    pub fn shown(&self) -> (NoteMap, ConcurMap);          // base with pending applied
    pub fn open(&mut self, anchor: &Anchor);
    pub fn key(&mut self, ks: &Keystroke) -> bool;        // Ctrl+Enter adds, Enter is a newline, Esc closes
    pub fn add(&mut self, now: SystemTime);
    pub fn toggle_concur(&mut self, nid: &str, now: SystemTime);
    pub fn copy_map(&self, anchors: &[Anchor], cx: &mut App);
    pub fn save(&mut self, path: PathBuf, rendered: LayoutHash, anchors: Vec<(String, String)>,
                engine: Arc<dyn PageEngine>, geometry: Geometry, cx: &mut Context<DocumentView>);
    pub fn elements(&self, layout: &PageLayout, map: &PageToView, hover: Option<&str>, th: &Theme,
                    zones: &mut Vec<DocZone>) -> Vec<AnyElement>;
}
```

- **The note dialog** is drawn inside the view, as mockup 03 draws it, and bends with the glass like the page under it. It takes the hyperglow: `.border_2()`, `.border_color(th.accent)`, `.shadow(crate::float_shadows(th.accent))` (`main.rs:22187`, `pub(crate)`) **[m]**, on `th.surface`. From the top: the anchor's title, `#nid` in faint, the saved and pending notes (time, text, a delete zone), the draft, then "Add note", "Close" and "ctrl+enter to add". The draft is an `EditBuffer`; `render_edit_buffer` (`main.rs:1881`) draws one line, so the dialog draws its own wrapped lines with the caret, splitting on `\n`.
- **Buttons** follow the browser: shown on hover, always shown with a count once an anchor has a note (`notes.css:13-21`) **[m]**. They are drawn in TD's accent, since they belong to TD's layer over the page.
- **The concur stamp** is `svg().external_path(runtime_asset("td-concur-stamp.svg", STAMP))` (`art.rs:25-35`), rotated by `stamp_pose(nid)`, in the brief's own ink `#35c27a` (`notes.js:172`) **[m]**, so a stamp lands at the angle a browser gives the same decision. The concur zone is a dashed box reading "concur", drawn only where `anchor.concur_zone` exists.
- **The bar** sits bottom-right, fixed in the view: `N notes · M concurs · ⎘ copy map · 💾 save into file`, then `K unsaved` while anything is pending, then the last `Said`, for example "saved into 2026-09-24-gui-in-the-tui.html ✓". A read-only brief shows why instead of the save button.

### `docview/pref.rs`

```rust
/// ~/.config/terminal-delight/documents.toml (crate::instance::config_dir(), instance.rs:139).
#[derive(Deserialize, Default)] pub struct Prefs { #[serde(default)] pub html: Html }
#[derive(Deserialize, Default, Clone)]
pub struct Html {
    pub engine: Option<String>,    // unset = "snapshot"; "off" = always the desktop; any other word is an error naming it
    pub chromium: Option<PathBuf>, // unset = search PATH
    pub gpu: Option<String>,       // unset = headless defaults; "vulkan" = the spike's four flags
    pub network: Option<String>,   // unset = blocked; "allowed" lets a page fetch
}
pub fn path() -> PathBuf;
pub fn load(path: &Path) -> Result<Prefs, String>;
```

### Shared fixtures

`~/.claude/skills/decision-brief` is a symlink into agent-skills (`parker-brown-family/agent-skills`, public) **[m]**. The canonical fixtures live there, beside the file whose format they pin, and change in the same commit as any change to notes.js:

```
decision-brief/fixtures/notes-format/
  README.md                    what each case pins, how to regenerate
  writer.mjs                   the reference writer (the spike's notes-writer.mjs, promoted)
  check.mjs                    for every case: re-derive anchors.json and expected-map.txt from the
                               brief's own notes.js in Chromium; apply edits.json with writer.mjs and
                               compare with expected.html; reopen expected.html in a fresh profile and
                               check the badges and the concur count
  cases/<case>/brief.html      input bytes
  cases/<case>/edits.json      [{ "op": "add" | "delete" | "concur" | "unconcur", nid, text?, title?, ts }]
  cases/<case>/anchors.json    [{ nid, title, concurrable, stamp: { r, dx, dy } | null }], document order
  cases/<case>/expected.html   bytes after the write, or absent when the case must be refused
  cases/<case>/expected-map.txt  #export-out's text from the brief's own exportNotes()
  cases/<case>/expect.json     { "notes_text": the island's textContent as the browser reads it,
                                 "refuse": "NoIsland" | … | null }
```

Thirteen cases: `concur-era-pristine` (both islands `{}`; one note and two concurs), one per older release (`b689671-saved`, `eca5cb8`, `5717474`, each with notes and a mirror), `hostile-text` (`</script>`, `-->`, `--!>`, `<!--<script>`, emoji, CRLF), `orphan-note`, `two-islands`, `stale-mirrors` (three `READER NOTES` comments from two browser saves), `notes-file-override`, `no-body-close`, `no-island` (refused), `island-after-script` (refused), `unreadable-island` (refused). The briefs are synthetic, built from the skill's own markup at each release, so no real brief's content goes into two public repositories. Forks get a case when a fork's shape needs one.

**On TD's side**, `app/tests/fixtures/decision-brief/notes-format/` holds a byte-identical copy plus `SOURCE`, the agent-skills commit. Tests find it through `env!("CARGO_MANIFEST_DIR")`, the way `hostproto.rs:869-874` finds the protocol page **[m]**; TD has no tempfile dependency (`surfacefeed.rs:1114`) and writes through `testsync::Scratch` (`testsync.rs:50-60`) **[m]**. `scripts/sync-brief-fixtures` refreshes the copy, and the weekly drift watch opens an issue when agent-skills `main` differs. So notes.js can change and TD learns within a week, whichever repository moved first.

---

## Call stack

Every flow starts where the wiring half's flows hand over. In every one, input reaches the view only through the seam's methods; the view registers no gpui mouse handler.

### 1. Open a Markdown file

1. Wiring: `open_float` → `cx.new(|cx| DocumentView::new(src, DocSeat::Float, cx))`.
2. `DocumentView::new` → `Backend::Markdown(MarkdownDoc { doc: None, .. })`; `cx.spawn` → `background_executor().spawn`: `fs::read_to_string` → `markdown::parse(&text, path.parent())` → update: `doc = Some(Ok(Rc::new(md)))`, `seen = FileStamp::of(path)`, `notify`. `_watch = cx.spawn(loop every WATCH_EVERY)`.
3. The pane calls `set_theme` with its resolved theme whenever that theme changes.
4. `render`: `frame` and `links` cleared → for each of `markdown::image_paths(&doc)`: `window.use_asset::<ImgResourceLoader>(&Resource::Path(p), cx)` → `images` → root `div().size_full().overflow_hidden().bg(palette.bg)` with a canvas recording `origin` and `size` → `div().absolute().top(-top).w_full().child(markdown::document(&doc, &MdStyle::document(&theme, zoom), Some(&links), Some(&images), Some(&tops)))`. There is no `.id()`, no `overflow_y_scroll`, and no `on_*` handler.
5. Wheel: pane → `wheel(delta)` → `top = clamp(top − delta.y, 0, height − view_h)` → `notify`.
6. Press on a link: pane un-bends → `press(at)` → `markdown::link_at(&links.borrow(), origin + at)` → `resolve_link(dir, href)`: a `Fragment` scrolls to the heading's recorded top; a `File` or `Url` → `cx.emit(FollowLink { target })`.
7. Close: the pane drops the entity → `cx.on_release` → for each `Ok(arc)` in `images`: `cx.drop_image(arc, None)` and `cx.remove_asset::<ImgResourceLoader>(&resource)`.

### 2. Open an HTML brief, cold: no cache, no browser running

1. Wiring: `open_document(Html)` → `docview::html_ready(cx)` → `Engines` has no engine yet → `pref::load` → `SnapshotEngine::locate` finds `/usr/bin/chromium` → `Ok`. Then `show_document` → `DocumentView::new(src, DocSeat::Face)` → `Backend::Page(PageDoc { status: Reading, .. })`.
2. The first paint records `size` → `Geometry { css_width: round(V / zoom), viewport_css_height, scale: window.scale_factor() × zoom }`.
3. Pool: `fs::read` → `notes::find_regions` → `layout_hash` → `cache::key` → `cache::lookup` → miss.
4. Pool: `engine.open(PageRequest { path, geometry, expect })` → `SnapshotEngine`:
   1. `ensure_browser` → `EngineJob::Launch` to "td-page-engine" → `Cdp::spawn(chromium, launch_args)`. The spike measured 155–163 ms for a launch **[m]**.
   2. `Target.createTarget` → `attachToTarget` → session setup (see `snapshot.rs`).
   3. `expect(Page.loadEventFired)` → `Page.navigate(file://…)` → wait (spike: p50 99 ms) → `Runtime.evaluate("document.fonts.ready")` → `Runtime.evaluate(EXTRACT_JS, returnByValue)` → `PageLayout`. The spike measured 22 ms p50 for a first extraction **[m]**.
   4. Re-read and re-hash the file. Different from `expect` → retry once, then `Err(FileChanged)`.
5. Update: `layout = Some(..)`; `NotesLayer::new(bytes, regions, &layout, path)`; `plan_residency` → the viewport's tiles first.
6. Pool, per tile: `engine.tile(page, generation, band)` → `Page.captureScreenshot { format: png, optimizeForSpeed: true, captureBeyondViewport: true, clip: { x: 0, y: top_css, width: css_width, height: band_css, scale: 1 } }` → base64 → PNG → `Image::from_bytes(Png).to_image_data` → update: insert if the generation is current and the slot is still wanted, otherwise drop the `Arc` → `notify`.
7. `render`: tiles as `img(ImageSource::Render(..))` at `place`d positions; then `notes_ui::elements` → buttons, concur zones and stamps within the clip, each recorded as a `DocZone`; then the bar.
8. Pool, continuing: the remaining tiles outward from the viewport → `cache::store` once the last lands.

First pixels arrive after a launch, a load, an extraction, one viewport capture, a decode and an upload: about 0.4–0.7 s by the spike's parts **[i]**. A whole long brief takes about as long as one full-page capture, 1.5–2.2 s **[m, spike §3]**.

### 3. Open an HTML brief, warm: cached

1–3. As flow 2, until `cache::lookup` → `Some(Cached)`.
4. Update: `layout = cached.layout` (with `page: None`); `NotesLayer::new` from the fresh bytes; the viewport's tile PNGs read from disk → decoded on the pool → drawn. No browser starts.
5. The engine is reached later only by a press on a dialog opener (flow 2's steps 4.1–4.3 in a page opened then, and the dialog render cached beside the tiles), by a save's read-back, or by a relayout.

### 4. Add a note and save

1. Pointer: pane → `hover(Some(at))` → the anchor under it → its button is drawn.
2. `press(at)` → `DocHit::NoteButton(nid)` → `NotesLayer::open(anchor)` → dialog up → `has_caret()` is true → the wiring half's key layer sends every key to `key`.
3. Typing → `EditBuffer::apply`; Enter → `EditBuffer::insert("\n")`; Ctrl+Enter or a press on `DialogHit::Add` → `pending.push(Add { nid, title, text, ts: utc_minute(now) })` → the badge counts it; the bar reads "1 unsaved".
4. Esc → the dialog closes (`key` answers true).
5. `press` → `BarHit::Save` → `NotesLayer::save` → pool:
   1. `fs::read` → `find_regions` → `layout_hash == rendered`. If not, flow 6, then save again.
   2. `plan_write(bytes, &pending, &anchors, label, concur, path)` → `verify` → `commit` (backup under `brief-backups/`, temp file, fsync, mode, re-stat, rename).
   3. Update: `base` re-read from `plan.out`, `pending` cleared, `own_write = Written.stamp`, said "saved into … ✓".
   4. Pool: `engine.read_back(path, geometry)` → compare nids → keep the ✓, or `SavedUnconfirmed` with the backup path.
6. The layout hash has not moved, so the cache entry and the tiles on screen stay valid, and nothing re-renders.

### 5. Add a concur

1. `press(at)` → `DocHit::Concur(nid)`. It can only exist where `anchor.concur_zone` is `Some`, which only a concur-era notes.js produces.
2. `toggle_concur(nid, now)` → `pending.push(Concur { nid, ts })`, or `Unconcur` if it is concurred now → the stamp is drawn at `stamp_pose(nid)` → the bar's concur count moves.
3. Save as in flow 4. `plan_write` replaces the `report-concurs` island in place, or inserts one after the notes island; the mirror gains `  ✓ concur` on that decision's line and ` · N concurs` in its header.
4. On a brief whose notes.js predates concurs, step 1 never happens: there are no zones, no stamps, and the concurs island is never written.

### 6. The file changes underneath while it is open

1. The watcher, every 500 ms: `FileStamp::of(path)` differs from `seen` and is not `own_write` → pool: `fs::read` → `find_regions` → `layout_hash`.
2. **HTML, same layout hash** (only the notes regions changed, as when an agent edits the island) → re-read `base` and `base_concurs`; `pending` stays, since it is a list of deltas → `notify`.
3. **HTML, new layout hash** → status `ChangedOnDisk`, old tiles stay up → `engine.close(old page)` → `engine.open` with the new `expect` → the new generation's viewport tiles → swap in one update: every old `Arc` goes through `cx.drop_image(.., None)` → scroll kept as a fraction → `NotesLayer::new` from the new bytes. A pending note whose nid is gone is marked; at save it is refused with `AnchorGone`, and its text stays in the dialog to be copied.
4. **Markdown** → re-parse on the pool → `reanchor(old_meta, top_block, new_meta)` → the same block stays at the top.
5. **Image** → `ImageDoc::release` → the next paint's `use_asset` decodes the new bytes.
6. **The file is gone** → status `Gone`: the last render stays up, and saving is disabled.

### 7. Chromium is missing

1. Wiring: `open_document(Html)` → `docview::html_ready(cx)` → `pref.html.engine` unset (snapshot) → `SnapshotEngine::locate` → no `chromium` setting, and none of the four names on `PATH` → `Err(Unavailable::NoBrowser { searched })`. `Engines` remembers the answer for 30 s, so a second click does not search again.
2. Wiring: the file goes to the desktop through `open_with_system` (`pane.rs:696-702`), and the reason is shown from `Unavailable::sentence()`: "No Chromium found (looked for chromium, chromium-browser, google-chrome-stable, google-chrome on PATH). Opened with the desktop."
3. If the binary exists but fails to start after the view is placed: `engine.open` → `Err(Launch(stderr tail))` → `Backend::Unshown { reason }` with an "open with desktop" zone → `cx.emit(CannotShow { reason })` → the wiring half decides whether to hand the file over at once.

---

## Test plan

Every test below fails today: none of these modules exist, and the benchdraw test fails on the current arms. Pure tests sit in each module's `mod tests`; the ones that need a browser are in `app/tests/snapshot_engine.rs`. The fixture-driven tests loop over every case directory.

**The seam** (`docview.rs`)

| Test | Asserts |
|---|---|
| `nothing_in_the_document_view_listens_for_the_mouse` | a source scan of `docview.rs` and `docview/*.rs`, comments stripped as in `benchdraw.rs:4447-4451`, finds no `on_mouse_`, `on_click`, `on_scroll_wheel`, `overflow_y_scroll`, `overflow_scroll` or `InteractiveText` |
| `a_link_is_resolved_against_the_documents_folder` | `resolve_link("/a/b", "c%20d.md#x")` → `File { /a/b/c d.md, Some("x") }`; `"#y"` → `Fragment`; `"https://…"` → `Url`; `"file:///e"` → `File { /e }` |
| `an_image_has_no_scroll_to_save` | an image view's `scroll()` is `None`; a Markdown view's is `None` before its first layout |
| `the_documents_own_save_does_not_look_like_someone_elses` | a watcher tick whose stamp equals `own_write` changes nothing |

**Image** (`docview/image.rs`)

| Test | Asserts |
|---|---|
| `fit_never_enlarges_a_small_image` | a 64×64 image in a 600×400 view fits at scale 1.0, not 6.25 |
| `zooming_keeps_the_centre_where_it_was` | `step_zoom` then `image_rect` keeps the image point at the view centre fixed |
| `panning_stops_at_the_images_edge` | `clamp_centre` never shows more than half a view of background past an edge |
| `an_image_past_the_texture_limit_says_so_instead_of_drawing_nothing` | a 1000×40000 image becomes `Unshown` naming both sizes |

**Markdown** (`docview/markdown.rs`, `benchdraw.rs`)

| Test | Asserts |
|---|---|
| `a_link_keeps_its_target` | `parse("[a](b.md#x)")` keeps the text `a` and one link `(0..1, "b.md#x")`; an autolink keeps its URL |
| `a_paragraph_that_is_only_an_image_becomes_an_image_block` | `![alt](p.png)` → `Block::Image { src: "p.png", alt: "alt" }`; `see ![alt](p.png) here` stays a paragraph with a link run |
| `a_remote_image_is_never_fetched` | an `https://` source draws the placeholder; no element's source is a URI |
| `the_lifted_renderer_carries_no_colour_of_its_own` | a source scan of `markdown.rs` finds no `rgb(0x` and no `const .*: u32 = 0x` |
| `headings_and_links_take_the_themes_accent` | `MdPalette::from_theme` maps accent, surface, faint, complement and text as specified, for two different themes |
| `a_reloaded_markdown_file_keeps_the_block_you_were_reading` | `reanchor` finds the moved block by fingerprint, and falls back to the nearest index when it is gone |
| `comrak_is_built_without_its_defaults` | `app/Cargo.toml`'s comrak line carries `default-features = false` |
| `a_markdown_surface_is_drawn_by_the_document_renderer_at_both_sizes` | the delegation test's list includes `("Kind::Markdown(m)", "markdown(")` and both arms pass it |
| `a_compact_markdown_card_shows_no_markdown_syntax` | the compact arm's first block of `# Title\n**bold**` has the text `Title`, without `#` or `**` |

**The DevTools client and the engine** (`docview/cdp.rs`, `docview/snapshot.rs`, `docview/pref.rs`)

| Test | Asserts |
|---|---|
| `a_devtools_message_is_one_json_value_ended_by_a_nul` | `frames` splits two messages in one read, joins one split across three reads, and keeps an unfinished tail |
| `a_reply_reaches_the_call_that_asked_even_when_events_arrive_first` | over `UnixStream::pair`, two threads calling at once each get their own `id`'s result, with events interleaved |
| `an_event_awaited_before_its_cause_is_not_missed` | `expect` then a fake `loadEventFired` delivered before `call` returns → `wait` gets it |
| `a_dialog_the_page_opens_is_dismissed_without_blocking` | a fake `Page.javascriptDialogOpening` makes the client write `handleJavaScriptDialog { accept: false }` on that session |
| `the_browser_is_driven_over_a_pipe_and_never_a_port` | `launch_args` has `--remote-debugging-pipe`, no `--remote-debugging-port`, and a `--user-data-dir` under the runtime directory |
| `a_missing_browser_names_every_place_it_looked` | `locate` with an empty PATH and no setting → `NoBrowser` listing the four names |
| `an_unknown_engine_name_is_an_error_not_a_quiet_default` | `engine = "servo"` → `UnknownEngine("servo")`; `"off"` → `Off`; unset → snapshot |
| `a_missing_rect_is_none_not_zero` | an extraction result with a 0×0 rect deserialises to `rect: None` |

**The engine against a real Chromium** (`app/tests/snapshot_engine.rs`)

| Test | Asserts |
|---|---|
| `a_brief_renders_to_anchors_and_tiles_in_one_pass` | each fixture brief: the anchors' nids and titles equal `anchors.json`; a tile PNG is `css_width × scale` wide |
| `anchor_ids_do_not_depend_on_the_width` | the same brief at 968 and 600 CSS px gives the same ordered nids (spike §1) |
| `the_pages_own_notes_ui_is_not_in_the_picture` | a capture of a brief with a notebar equals a capture with `.notebar` removed, over the notebar's rect |
| `a_briefs_confirm_does_not_hang_the_engine` | a page calling `confirm()` on load still returns a layout within the timeout |
| `the_page_cannot_reach_the_network` | a page that fetches `http://127.0.0.1:<port>` of a listener the test owns: the listener sees no connection |

These need a browser. GitHub's `ubuntu-latest` image carries Google Chrome **[i, not checked on TD's runner]**, which `locate` finds as `google-chrome`. A missing browser fails these tests; it does not skip them.

**Cache and tiles** (`docview/cache.rs`, `docview/page.rs`)

| Test | Asserts |
|---|---|
| `a_cache_entry_is_complete_or_absent` | an interrupted `store` leaves no readable entry; `lookup` answers `None` |
| `saving_notes_keeps_the_cached_render` | `layout_hash` is unchanged by a `plan_write`, so `key` is too |
| `any_other_change_misses_the_cache` | one byte changed outside the notes regions changes `layout_hash` |
| `the_cache_forgets_the_oldest_pages_first` | `sweep` past the cap removes by last use, oldest first |
| `scrolling_keeps_at_most_five_tiles_resident` | `plan_residency` over a 13-tile page never holds more than five, and drops the ones left behind |
| `the_viewports_tiles_are_asked_for_first` | `fetch` is ordered by distance from the viewport |
| `a_note_button_sits_where_the_browser_puts_it_after_scroll_and_zoom` | `place` at scale 1.6, zoom 1.25 and a scroll of 3,000 device px equals the arithmetic in `page.rs` |
| `tile_edges_land_on_whole_device_pixels` | every tile's top, times the scale factor, is an integer for scales 1, 1.25, 1.6 and 2 |
| `an_anchor_in_a_closed_dialog_has_no_button` | `rect: None` gives no zone |

**The notes format** (`docview/notes.rs`, all fixture-driven where a case exists)

| Test | Asserts |
|---|---|
| `the_island_is_found_where_the_browser_finds_it` | the inner text `find_regions` returns equals the `textContent` of `getElementById('report-notes')` that `check.mjs` recorded in the case's `expect.json`, including `two-islands`, where both pick the first |
| `adding_notes_gives_the_skills_expected_bytes` | `plan_write(brief.html, edits.json)` equals `expected.html` byte for byte |
| `nothing_outside_the_three_regions_moves` | `verify` passes on every plan, and fails on a plan with one byte altered outside a splice |
| `writing_the_same_notes_back_changes_no_byte` | a zero-edit write of each `expected.html` returns the same bytes |
| `the_island_is_written_the_way_json_stringify_writes_it` | the `b689671-saved` island, written by the browser, is reproduced exactly from its own parse |
| `a_note_cannot_end_the_island_or_the_comment` | the `hostile-text` case round-trips: `</` as `<\/`, `<!` as `<!`, `-->` as `-- >`, `--!>` as `--! >`, `---` untouched |
| `the_map_is_the_one_notes_js_builds` | `build_map` equals `expected-map.txt` for every case |
| `a_note_whose_anchor_is_gone_is_counted_but_not_listed` | the `orphan-note` header counts it; no line names it |
| `a_concur_rides_on_its_decisions_line_even_without_a_note` | a concurred anchor with no notes gets `[nid] title  ✓ concur` and a blank line |
| `the_map_names_the_briefs_notes_file_not_the_path` | `notes-file-override` → the header reads the override |
| `only_the_last_reader_notes_comment_is_replaced` | `stale-mirrors`: the first two comments are byte-identical afterwards |
| `a_mirror_goes_before_the_last_body_close_or_at_the_end` | pristine → before `</body>`; `no-body-close` → at end of file |
| `a_concurs_island_is_inserted_after_the_notes_island_only_when_needed` | no concurs and no island → no insertion; one concur → inserted straight after the notes `</script>` |
| `a_brief_older_than_concurs_never_gets_a_concurs_island` | `plan_write` with `NotSupported` and a `Concur` edit → `Refused(ConcursUnsupported)`; with notes only, no concurs splice |
| `a_file_without_an_island_is_refused_not_invented` | `no-island` → `NoIsland`; `island-after-script` → `IslandAfterScript`; `unreadable-island` → `Unreadable` |
| `a_build_input_is_never_written` | `_x_body.html` → `BuildInput` |
| `a_deleted_note_is_found_by_its_words_not_its_position` | a `Delete` applied after another writer inserted a note ahead of it removes the right one |
| `pending_notes_join_whatever_the_file_holds_now` | a note added to the file after TD read it survives TD's save |
| `a_note_on_a_passage_that_is_gone_is_not_written` | an `Add` whose nid is not among the anchors → `AnchorGone` |
| `the_notes_island_keeps_its_keys_in_the_order_the_file_had_them` | `{"z":…,"a":…}` writes back `z` before `a` |
| `a_field_another_writer_added_survives_a_save` | a note with an `author` key keeps it |
| `a_notes_time_is_written_in_utc_as_notes_js_writes_it` | `utc_minute(UNIX_EPOCH + 1_790_278_680 s)` is `"2026-09-24 19:38"`, the stamp on Parker's first note in the saved research brief, typed at 12:38 Pacific |
| `a_concur_stamp_lands_at_the_angle_the_browser_gives_it` | `stamp_pose` is within 0.05 of each fixture's recorded `stamp` |
| `a_save_is_a_rename_and_leaves_a_backup` | after `commit`: the backup's bytes equal the original, the mode is kept, no temporary file remains |
| `a_file_written_by_someone_else_mid_save_is_not_overwritten` | a change between the read and the rename → `Changed`, and the file keeps the other writer's bytes |
| `a_symlinked_brief_is_refused` | `commit` on a symlink → `Refused(Symlink)` |
| `the_vendored_fixtures_match_the_skill_when_it_is_installed` | with `~/.claude/skills/decision-brief/fixtures/notes-format` present, every file is byte-identical to TD's copy; without it, the test prints that it compared nothing |

**Notes UI** (`docview/notes_ui.rs`, pure parts)

| Test | Asserts |
|---|---|
| `ctrl_enter_adds_and_enter_starts_a_new_line` | `key` with Enter puts `\n` in the draft; Ctrl+Enter pushes an `Add` and empties the draft |
| `escape_closes_the_note_box_before_anything_else` | with the dialog open, `key(escape)` → true and the dialog is gone |
| `concur_is_offered_only_where_the_brief_offers_it` | anchors without `concur_zone` produce no `DocHit::Concur` zone |

## Least confident decisions

1. **Owning the DevTools client.** No package added, no port opened, and the same transport the spike measured. The cost is protocol code TD maintains. If the live engine later needs input dispatch, screencast and a large share of the protocol, `chromiumoxide`'s generated types start to pay, and tokio arrives with them. The trait keeps that swap to one module.
2. **The cache and the save both rest on "notes are data, not layout."** The layout hash masks the notes regions, so a save keeps the render and the cache. The spike measured that hiding the notes UI moves no anchor, but it did not measure whether a changed island moves one. It should not: `has-note` adds an inset shadow, and the stamp sits inside a hidden zone **[i]**. The read-back after each save is the guard. This also departs from the spike's "never from a cache". Cached anchors are only ever paired with cached tiles from the same render, and the write path re-reads the bytes before every save.
3. **The note dialog lives inside the bent tube.** Mockup 03 puts it there, and it keeps the note beside the passage. But every other hyperglow surface in TD is a flat window-level modal, and editing multi-line text on the single-line `EditBuffer` with a hand-drawn wrapped caret is new code. A flat modal over the window would be simpler to type into.
4. **Explicit save, with unsaved notes held in memory.** This matches the brief's own buttons and Parker's "SAVE doc", but a crash loses unsaved notes, which the browser's `localStorage` would have kept. A draft journal in TD's state directory would fix that and create a third place notes can live, which the spike argued against.
5. **The browser's own copy still wins in the browser.** TD writes the file correctly, and a Chromium profile holding `notes:<file>` keeps showing its older map **[m, spike §2]**. TD cannot reach that storage. The fix belongs in notes.js: a revision stamp in the island that beats an older stored copy. It should be filed against the skill before this ships.
6. **The page cannot touch the network.** A brief runs its own script inside TD's engine, and blocking fetches means it cannot send anything anywhere. A brief that loads a web font or an image by URL renders with fallbacks. `network = "allowed"` exists for that.
7. **Vendored fixtures and a weekly watch**, rather than CI fetching the skill. TD's writer tests can pass against last week's format for up to a week after notes.js changes. The alternative, fetching agent-skills in CI, adds a network dependency to every build.
8. **One Chromium per TD process, shut down after five idle minutes.** The spike measured 265–570 MiB for it **[m]**. Reopening a brief after the shutdown pays a launch again, 155–163 ms, which the warm cache usually hides.
9. **A 16,384-px image limit invented on TD's side.** gpui does not expose the device's limit (not found). This GPU takes 32,768 **[m, spike §3]**, so images between the two limits are refused here although they would draw.
10. **`drop_image(.., None)` at release relies on the window being back in `App.windows`.** `app.rs:2432-2446` skips a window that is mid-update unless it is passed in. Entity release happens when effects are flushed, after the window update returns **[i, not traced]**. If that is wrong, up to five tiles (61 MiB) leak per closed brief. The tracer slice should count atlas textures across a hundred open/close cycles.
