# TD internals for "GUI in the TUI": where documents and pixels would attach

Research against `origin/main` at `de7776b`, in the worktree `/home/parker/Work/td-gui-in-the-tui`, 2026-09-24. Read-only; nothing was built.
gpui comes from `/home/parker/Work/zed-upstream` (a pinned Zed checkout with TD's patches applied, uncommitted there; `crt_pass.wgsl` is untracked).
Registry sources are under `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`: `alacritty_terminal-0.26.0` and `vte-0.15.0` (versions from `app/Cargo.lock`).

Confidence labels: **measured** means I read the line or ran the command. **Inferred** means it follows from lines I read, with no run to prove it. **Hunch** means an informed guess.

## The facts that decide the design

1. **The pixel primitive already works.** TD draws PNG, animated WebP and SVG through `gpui::img(PathBuf)` in seven places. gpui also accepts decoded frames (`ImageSource::Render(Arc<RenderImage>)`) and encoded bytes (`Image::from_bytes`). Every image becomes an atlas sprite through `Window::paint_image`. Nothing in TD draws an image inside the terminal grid or the workbench body yet. (measured)
2. **The terminal grid is ordinary gpui text.** Each row is a `div` holding a `StyledText` with per-cell `TextRun`s (`pane.rs:8680-8711`), so an image element can sit inside a pane like any other child. (measured)
3. **The CRT warp is a post-pass over registered rectangles.** Anything painted inside a pane's screen rectangle bends with it, images included. The first rectangle that contains a pixel wins (`crt_pass.wgsl:63-78`). There is no per-element exemption today: the open issue "Exempt only an overlay's own rectangle from the CRT warp" asks for one. (measured)
4. **A split leaf can only hold a terminal.** The tiling tree is `Tree<L>`, but the live type is `Node = Tree<Entity<TerminalView>>` (`main.rs:309-320`), and `TerminalView` requires a `term::Session`. (measured)
5. **A pane already has two faces, Terminal and Workbench.** The switch between them is one `if` in the render (`pane.rs:8677`). A third, Document, face is the cheapest seam for a document pane. (measured for the switch; inferred for the seam)
6. **Every click that opens something goes to `xdg-open`.** That covers Ctrl/Shift+click on a path in terminal output, the right-click "Open link" item, a workbench artifact and a rail deliverable. All of them call `open_with_system` (`pane.rs:696-702`). The docs forbid an in-TD viewer in three places, so this feature reverses a written decision. (measured)
7. **Link detection is a token scan over grid text, not OSC 8.** alacritty stores OSC 8 hyperlinks on cells, but TD never reads them. Relative paths such as `docs/x.md` (no `./`) are not detected, and a detected path must exist on disk. (measured)
8. **The workbench has no Markdown renderer.** A `markdown` surface is drawn line by line as plain text (`benchdraw.rs:993`, `:2324-2335`). There is no Markdown or HTML crate in the build. A webview was rejected on the record. (measured)
9. **Kitty graphics are dropped inside vte.** vte 0.15 has no APC callback, and its string state discards the bytes (`vte lib.rs:377, 182, 438-450`). Sixel reaches `hook`/`put`/`unhook`, but those only debug-log (`vte ansi.rs:1311-1326`). The parser is private to alacritty's `EventLoop`, so TD's only interception point is a byte-level `Read` wrapper; two of those already exist. (measured)
10. **Pixel geometry is reported, but in truncated logical pixels.** The kernel holds `ws_xpixel`/`ws_ypixel` = cols×`cell_w as u16`, rows×`cell_h as u16`. My pane reads 189×49 cells, 1134×686 px, and a 6×14 cell, on a monitor at scale 1.6. CSI 14t is answered in hosted mode only, and CSI 16t (cell size) is never answered. (measured)
11. **The host and the window each run a full parser on the same raw bytes.** The window renders its replica, and only the host answers queries (`term.rs:58-70`). Images would have to be decoded window-side, answered host-side, and re-sent on re-attach, which today's attach snapshot (text only) cannot carry. (measured for the wire; inferred for the consequences)
12. **None of the names in the outside summary exist.** TubeTarget, EffectGraph, SurfaceHost, CellTarget, PixelTarget, ChildPTYTarget and Ratatui have zero hits in code or docs. They appear only in an earlier attempt ref from today. "Tube" is real, and it means a warp-registered rectangle. (measured)

---

## 1. Rendering: how a pane draws, and where the CRT pass applies

**The pane render.** `impl Render for TerminalView` is at `pane.rs:7370`. (measured)
- The root is a flex column: `.child(header)` at `pane.rs:8586`, then the "screen" `div` at `:8587-8740`.
- The screen holds an absolute measuring `canvas` (`:8592-8670`), then either the grid or the bench (`:8677-8712`), then the copy chip, the sticky note and the ignition flash.
- After the screen, the pane root adds `crt::glass` (`:8741-8749`), which is scanlines, specular and vignette drawn as gpui elements (`crt.rs:195-206`, absolute `inset_0`). It adds the bezel, the gamba reels, the context menu and the paint overlay (`:8751-8758`) after that.

**Glyphs.** TD does not paint glyphs itself on the grid. (measured)
- Each row is `div().h(px(self.cell_h)).whitespace_nowrap().child(StyledText::new(text).with_runs(runs))` (`pane.rs:8704-8708`).
- The runs come from `styled_lines` (`pane.rs:6339`).
- Custom painting through `canvas` plus `window.paint_*` is used elsewhere: the sticky note (`sticky.rs:878`), benchdraw figures (`benchdraw.rs:2921` and others), and `crt.rs:244`.

**Where the warp applies: per pane, screen area only.** (measured)
- The screen's canvas calls `crate::warp::register_tube([x,y,w,h] in physical px, glare, k1, k2, crawl)` with the pane's own curvature during prepaint (`pane.rs:8639-8650`).
- The header is outside that rectangle, so it stays flat.
- `warp.rs` keeps a static list (`RECTS`, capped by `MAX_TUBES = 32` at `warp.rs:27`). It is cleared by `begin_frame` (`warp.rs:67`) and emptied for the frame by `flatten()` (`warp.rs:58`) whenever an overlay scrim is built (`Workspace::over_the_glass`, `main.rs:21569-21591`).

**The renderer side.** (measured)
- In `zed-upstream/crates/gpui_wgpu/src/wgpu_renderer.rs:1414-1439`, when any tube is bent, the whole scene renders to an offscreen texture.
- `crt_pass` then samples that texture onto the swapchain (`:1617-1688`).
- The shader `crates/gpui_wgpu/src/crt_pass.wgsl` (v3, 174 lines) loops over the rectangles. It returns on the first rectangle that contains the pixel (`:63-65`). A tube with |k|<0.0005 and no crawl passes the pixel straight through (`:76-78`).
- Patch provenance is `docs/patches/0001-td-crt-pass.patch` (per-rectangle barrel), `0002-focus-blur.patch`, `0003-text-crawl.patch`, `0004-warp-tube-cap-32.patch` and `0005-glyph-transform.patch`. The last one gives glyph sprites an affine, and the sticky note uses it.

**Would an image placed inside a pane go through the warp?** Yes, if it is painted inside the screen `div`. (inferred, strongly)
- `paint_image` inserts an ordinary `PolychromeSprite` into the scene (`gpui/src/window.rs:4084-4127`).
- The scene is what the pass samples, and the pass works on pixels, not on elements.
- Two consequences follow:
  - (a) gpui hit-tests the flat layout, so clicks on a bent image land off-target unless they go through the warp's inverse. The grid does this through `viewport_cell` (`pane.rs:4038`) and the bench through `bench_hit_at`.
  - (b) An image cannot be pre-warped the way the sticky note is. `PolychromeSprite` has no transform field (`window.rs:4116-4125`); only paths and (patched) glyphs carry one. So an image either bends with the glass or needs a flat tube registered **before** the pane's tube, because the first match wins. That needs a new insert-at-front API. `register_overlay_tube` (`warp.rs:139`) appends. (measured for the struct and the shader order; inferred for the remedy)
- gpui's `surface()` element, the video path, is not implemented on Linux/wgpu: `PrimitiveBatch::Surfaces(_) => { // Surfaces are macOS-only … Not implemented for Linux/wgpu`, at `wgpu_renderer.rs:1590-1594`. (measured)

## 2. Existing image use: the pixel primitive already exists

**Where TD draws images today.** All seven sites go through `gpui::img(<path>)`. (measured)

| Where | What | Format |
|---|---|---|
| `main.rs:17342`, `:17348`, `:17359` | Tab badges: the robot, plus a blinker overlay animated through `with_animation` | PNG (`art.rs`) |
| `main.rs:23683` | Mother-bar mark | PNG |
| `pane.rs:8099` | Pane header: tool prop (`face.plate`) | PNG |
| `pane.rs:8127` | Pane header: user or directory logo | any file gpui decodes |
| `main.rs:25776` | Agent-wall card art. Animated robot scenes from `toolprop.rs`, each warped by its own overlay tube (`main.rs:25759`, `warp::register_overlay_tube`) | animated WebP |
| `main.rs:14205` | Logo-picker thumbnails | any |
| `main.rs:18864` | Provider marks through `gpui::svg()` | SVG (single-colour mask) |

- Embedded art is written to `$XDG_RUNTIME_DIR` and handed over as a path. The comment says "because gpui's `img()` eats paths" (`art.rs:8`, `:24`). (measured)
- None of these images is inside a pane's screen tube; the header ones sit above it. (measured)

**Pasted images are never drawn.** (measured)
- Agent pane: Ctrl+V goes down the PTY as 0x16, and the composer keeps only a count (`workbench.rs:533`).
- Shell pane: the image is saved by `save_pasted_image(&self, image: &gpui::Image)` (`pane/bench.rs:2843`), and its path is typed.
- `benchstore.rs:103-105`: "**Nothing writes to the pool yet**".
- The two open issues asked about concern paste and drop, not rendering:
  - "A dial change drops an image pasted into an unsent draft" is marked inferred, not observed.
  - "gpui destroys drags carrying non-local URIs" happens in gpui's Wayland client; fixing it needs a second Zed patch.

**The gpui API, read in `/home/parker/Work/zed-upstream/crates/gpui`.** (measured)
- `pub fn img(source: impl Into<ImageSource>) -> Img` is at `elements/img.rs:200`.
- `pub enum ImageSource` is at `elements/img.rs:41`. Its variants are `Resource`, `Render(Arc<RenderImage>)`, `Image(Arc<Image>)` and `Custom(...)`.
- The loader sniffs the format with `image::guess_format`, handles multi-frame GIF/WebP, and falls back to resvg for SVG (`img.rs:619-746`).
- `RenderImage::new(data: impl Into<SmallVec<[Frame; 1]>>)` is at `assets.rs:61`. `Frame` is `image::Frame`, in BGRA order.
- `Image::from_bytes(format, bytes)` is at `platform.rs:2141`. `ImageFormat` covers Png, Jpeg, Webp, Gif, Svg, Bmp, Tiff, Ico and Pnm.
- `Window::paint_image(bounds, corner_radii, data: Arc<RenderImage>, frame_index, grayscale)` is at `window.rs:4084`. It inserts the image into the sprite atlas, keyed by `(image_id, frame_index)`. `Window::drop_image(data)` is at `window.rs:4149`.
- Implication for video (mpv `--vo=kitty`): every new frame is a new `RenderImage` id and a new atlas upload, so the old frame must be `drop_image`d or the atlas grows. (inferred)

**Dependencies.** `app/Cargo.toml` has no `image` crate. `image 0.25` and `resvg` are in the lock file only through gpui. (measured) Raw RGBA from a decoder can become a `RenderImage` after a swizzle to BGRA, which needs `image = "0.25"` in TD's manifest to name `Frame`. Re-encoding to PNG and going through `Image::from_bytes` avoids that dependency. (inferred)

## 3. Panes, splits, tabs

**The data model.** (measured)
- A binary tree per tab: `enum Tree<L> { Leaf(L), Split { id, dir: SplitDir, ratio: f32, a, b } }` (`main.rs:309-318`), with `type Node = Tree<Entity<TerminalView>>` (`main.rs:320`). `SplitDir` is `Row` or `Col` (`main.rs:157-160`).
- `struct Tab { root: Node, name, focused, color, text_color, group, project, level }` (`main.rs:1548`).
- The left-bar project/initiative/task tree in `tree.rs` is a separate thing, the tab organiser, not the split layout.
- Persistence: `SavedNode::Leaf { appearance, cwd, resume, name, logo, note, pane_id }` or `Split { dir, ratio, a, b }` (`main.rs:735-765`). A leaf is a terminal recipe.
- Rendering is `fn render_node` (`main.rs:22341`).

**How a split is made.** (measured)
- Keys:
  - Alt+V gives `split(SplitDir::Row)`, a new pane beside.
  - Alt+H gives `SplitDir::Col`, a new pane below.
  - Ctrl+Alt+D is also `Col`.
  - All three are in `Workspace::on_key` (`main.rs:15196`; bindings at `:15640`, `:15677-15685`). Bezel buttons also call split (`main.rs:23378`, `:23384`).
- `Workspace::split` (`main.rs:9203-9262`):
  1. Checks the cap: `may_split(leaves) = leaves < MAX_PANES`, with `MAX_PANES = 4` (`main.rs:118`, `:136`). Loaded layouts may hold 8 (`LEGACY_PANE_CEILING`, `:127`).
  2. Picks the focused leaf.
  3. Builds a pane through `make_pane_in_mode(PaneRestore { cwd, .. })` (`main.rs:4957`), which is the chokepoint that spawns a PTY.
  4. Calls `root.split_leaf(target, dir, new_pane)` (`main.rs:359-381`).
  5. Focuses the new pane after a defer, then saves.
- Drag-to-split (`PaneDrag`, `DropTarget::Split/Edge/Tab`, `main.rs:2803-2873`) calls `split_leaf_dir` (`main.rs:386`).

**Can a pane hold something other than a PTY terminal?** Not as a leaf. (measured)
- `TerminalView` owns `session: term::Session` (`pane.rs:1847`), which is `term: Arc<FairMutex<Term<EventProxy>>>` plus a notifier (`term.rs:101-127`).
- `main.rs` touches leaves as `Entity<TerminalView>` throughout. Counts over `main.rs`, tests included: 63 `.leaves(&mut` calls, 103 `Node::Leaf`/`Tree::Leaf` mentions and 39 `Entity<TerminalView>` mentions.
- What does exist is a second **face** inside every terminal leaf. `workbench::Face { Terminal, Workbench }` (`workbench.rs:76-80`) is chosen at `pane.rs:8677`, with the terminal running on underneath.

**Programmatic creation.** (measured)
- The ctl socket (`ctl.rs`, one line in, one line out) offers:
  - `ping`, `whoami` and `paint …`
  - `bench on|off|toggle|choose|submit|say|type`
  - `skin …` and `mcp …`
  - `tabs '<json>'`, which only names, groups and colours tabs (`TabAction`, `ctl.rs:154-191`)
  - `adopt`, which opens a **new tab** at a cwd with an optional command (`AdoptReq`, `ctl.rs:48-52`, landing in `Workspace::adopt_pane` → `open_tab` at `main.rs:7309-7329`)
- There is **no split verb and no "open a document" verb**. (measured)
- The host protocol (`hostproto.rs:102-174`) has `SpawnPane { cwd, resume, geom }` but knows nothing of layout. Splits and tabs live only in the window. The host stores the layout as an opaque TOML body (`Save`, `:160-165`). (measured)
- The terminal-delight MCP `list_panes` reports tab, title, mode, pid, pane_id, cwd, session and note. It reports **no geometry**. (measured, live call)

## 4. Floating things inside a pane

**The sticky note (`sticky.rs`).** (measured)
- One `Option<Sticky>` per pane.
- **Position is fixed, not draggable.** `layout(content, tilt)` puts it in the top-right corner, sized from the pane: width `(w*0.28).clamp(128,200)`. It returns `None` below 250×190 (`sticky.rs:407-444`, `MIN_PANE` at `:323`). There is no drag code in the file: a grep for `drag` in `sticky.rs` finds nothing.
- **Drawn pre-warped.** Its paths go through the exact barrel map, and its glyphs through an affine fit (`Warp`, `sticky.rs:177-273`; `Layout::text_matrix`, `:370-403`), so it looks flat on bent glass. It is painted from a `canvas` inside the screen `div` (`pane.rs:8514-8541`, `:8723`).
- **Hit-testing is manual.** `Hit::at(p, &layout)` inverts the tilt and ignores the pre-warp (`sticky.rs:851-870`). It is called from `TerminalView::on_mouse_down` ahead of everything except the bench (`pane.rs:6177-6189`); hover is `sticky_hover` (`pane.rs:6273`).
- **Z-order is child order.** The note comes after the grid and before the glass (`pane.rs:8713-8749`).
- **Persisted** as `SavedNote { title, text, seed, pinned }` on the leaf (`main.rs:714-728`).

**Other overlays.** (measured)
- Window-level modals and menus are built through `Workspace::over_the_glass` (`main.rs:21569`). It is a full-window scrim with `occlude()` that calls `warp::flatten()`, flattening every pane for the frame.
- Hover cards use `over_the_glass_tip` (`main.rs:21900`).
- The FOCUS reader (Alt+R) is a window modal that mirrors a pane's scrollback as a document (`doc.rs:1-35`, `pane.rs:2772`). It registers itself as the only tube (`warp::register_focus_tube`, `warp.rs:100`) and frosts the backdrop.
- `hud.rs` is pure parsing, with no geometry.
- Workbench "cards" replace the conversation body inside the bench (`pane/bench.rs:4270-4310`); they do not float.

**Is sticky dragging a precedent for a draggable document square?** There is no sticky dragging. (measured) Two pieces are reusable:
- **Rendering:** a square drawn inside the screen `div` from a `canvas`, hit-tested manually because it lives in a bent tube. (inferred)
- **Dragging:** the window's hand-rolled drag state, `PaneDrag { id, start, at, engaged, … }` (`main.rs:2857-2873`). It is armed by a `DragPaneStart` event that the pane emits and `wire_pane` subscribes to (`main.rs:4211`, `:4321-4333`), and the gesture engages past a threshold. Split-ratio dragging (`drag_split`, `main.rs:3854`) is the same pattern.

The real obstacle is the warp. An image square cannot be pre-warped (§1), so it either bends and hit-tests through the inverse, or needs a front-inserted flat tube. That exemption is exactly what the open warp-exemption issue asks for. (inferred)

## 5. Link handling today

**Detection.** `TerminalView::link_under(pos)` (`pane.rs:4293-4311`). (measured)
- `viewport_cell` un-warps the pointer.
- The row is mapped through `paint_row_to_grid_row`.
- `stitch_wrapped_line` (`pane.rs:377`) rejoins soft-wrapped or edge-filled rows.
- `link_at(line, col)` (`pane.rs:306-347`) takes the whitespace-delimited token and trims brackets and punctuation. It classifies:
  - `http(s)://`, `file://`, `ftp(s)://`, `mailto:` and `www.` as `Link::Url`
  - tokens starting `/`, `~/`, `./` or `../` as `Link::Path`
- A path is resolved against the pane cwd and **kept only if it exists** (`pane.rs:4306-4309`).
- It is a token scan with no regex.
- **OSC 8 is not consulted.** A grep for `hyperlink` in `app/src` hits only tests in `term.rs:1526-1575`. The 2026-06-20 review's "per-cell `Hyperlink` (OSC-8) … unused" (`docs/2026-06-20-competitive-feature-mining-and-launch-plan-review.md:15`) is still true.

**Gestures, all in `TerminalView::on_mouse_down` (`pane.rs:6122-6262`).** (measured)
- **Ctrl+click or Shift+click** on a link calls `open_with_system(&target)` (`pane.rs:6240-6246`).
- **Super+Ctrl+click** calls `reveal_with_system` (the FileManager1 D-Bus `ShowItems` call, with an `xdg-open` fallback on the parent directory) (`pane.rs:6226-6236`).
- **Alt+click** does **not** open anything. It copies the reconstructed line under the "⎘ alt+click" chip (`copy_hint_at`, `pane.rs:6196-6206`; the chip is drawn at `:8480-8507`). The chip arms only on command-like lines and line-initial URLs (`is_copyable_command`, `pane.rs:595-631`).
- **Right-click** opens a context menu with "Open link ↗" and "Reveal in folder ⌖" when `link_under` finds something (`pane.rs:7374-7434`).
- `open_with_system` runs `uwsm-app -- xdg-open <target>` on a uwsm session and plain `xdg-open` otherwise, detached with `setsid` (`pane.rs:657-702`).

**Every call site of `open_with_system`.** (measured)
- `pane.rs:6242`: Ctrl/Shift+click
- `pane.rs:7417`: context menu
- `pane/bench.rs:2120`: artifact rail click
- `pane/bench.rs:3356`: `Dispatch::Open`
- `main.rs:19430`: rail keyboard deliverable
- `main.rs:20258`: rail deliverable click

**Mouse reporting.** Clicks are never forwarded to a PTY app; only the wheel is reported to apps in mouse mode (`pane.rs:5435-5450`). A grep for `MOUSE_MODE`/`SGR_MOUSE` outside `pane.rs` finds only mode replication in `gridwire.rs:94-99` and tests. So Ctrl+click steals nothing from TUIs today. (measured)

**Where "open inside TD" hooks in.** The Ctrl-click branch at `pane.rs:6240-6246` and the menu item at `:7414-7422` both know the pane and the resolved target. See the seams below. (inferred)

## 6. Workbench and TDSP

- There are ten surface kinds: Artifact, Markdown, Table, Architecture, Changeset, Decision, Question, Response, Comment and Unclassified (`surface.rs:1460-1472`). (measured)
- **Markdown is plain lines.** `full()` sends `Kind::Markdown(m)` to `paragraph(m.body)` (`benchdraw.rs:993`). `paragraph` makes one `div` per `text.lines()` with selectable text (`benchdraw.rs:2324-2335`), so `## Heading` shows literally. (measured)
  - No `pulldown-cmark`, `comrak` or `markdown` crate appears in `Cargo.toml` or `Cargo.lock`. (measured)
  - `doc.rs` is the FOCUS reader's line model built from the terminal grid, not a Markdown parser. (measured)
  - The plan on record: Zed's `markdown` crate is GPL-3.0, so use "`pulldown-cmark` (MIT) … plus TD's own elements" (`reports/2026-09-12-agent-native-output-surface.html:672`, `:966`, `:1048`). (measured)
- **HTML: none.** `benchdraw.rs:16`: "there is no HTML, no webview, no component tree". The surface protocol spec rejects the sandboxed-iframe approach because "gpui has no webview, and a second renderer would be the one window on this desktop that does not match itself" (`docs/spec/td-surface-protocol.md:728`). (measured)
- **Artifact click.** Both paths hand the href to the desktop and draw nothing in TD. (measured)
  - A rail click calls `bench_open`, which calls `open_with_system(href)` then `close_card()` (`pane/bench.rs:2112-2125`).
  - The Open verb yields `Dispatch::Open(href)` (`workbench.rs:4775-4776`), which calls `open_with_system` (`pane/bench.rs:3356`).
- **The no-viewer rule is written in three places.** (measured)
  - `surface.rs:388-389`: "Never our own viewer: the person already told their machine where Markdown goes."
  - `docs/spec/td-surface-protocol.md:315-316`: "The window never hard-wires a viewer."
  - `docs/plans/attention-spine/plan.md:176-181`: "**The desktop opens it, not the terminal.**"
- **Could the Workbench be the document pane?** It is a strong host, but it has no renderer yet. (inferred)
  - It exists on every pane (`bench` field at `pane.rs:2083`).
  - Its card body scrolls (`overflow_y_scroll().track_scroll(&self.wb_card_scroll)`, `pane/bench.rs:4296-4308`).
  - It bends with the barrel warp and does its own hit-testing through the inverse (`pane.rs:6141-6171`, `pane.rs:8618-8624`).
  - It takes file drops (`on_drop::<ExternalPaths>`, `pane.rs:8567`).
  - Its rail takes 208 px and collapses below 420 px (`workbench.rs:53-66`).
  - It is conversation-shaped: a rail of surfaces plus a composer. A document viewer is a different layout, which argues for a third face beside it rather than a surface kind inside it.

## 7. VT input path for graphics

**Where bytes meet the parser.** TD never calls the parser itself. alacritty's stock `EventLoop` owns `parser: ansi::Processor` (`event_loop.rs:404`) and calls `state.parser.advance(&mut **terminal, &buf[..unprocessed])` (`event_loop.rs:154`). (measured)
- **Host:** `host.rs:671-689` runs `tty::new`, then `TeePty`, then `Term::new`, then `EventLoop::new(...).spawn()`. `TeeReader::read` copies every chunk to the attached window's sink (`host.rs:147-162`).
- **Window, the default hosted mode:** `term::attach_in` wraps the socket in `SocketPty` (`term.rs:289-310`). `wire_event_loop` (`term.rs:235-265`) runs the same stock `EventLoop` on a replica `Term`. `CountingReader::read` counts the bytes (`socketpty.rs:98-107`).
- **Window, serverless (`TD_NO_SESSIOND=1`):** `spawn_in` → `wire_event_loop` (`term.rs:174-225`).

**Does TD inspect the byte stream?** Only by copying (`TeeReader`) and counting (`CountingReader`). (measured)
- The TD_LATENCY probe times the `Wakeup` event and reads no bytes (`pane.rs:3911-3916`).
- `screenread.rs` reads the grid as text.
- OSC 0/8/52 are handled inside alacritty's `Term`.
- A grep of `app/src` for sixel, kitty graphics and APC finds nothing.

**APC (Kitty graphics, `ESC _ G … ESC \`).** It is dropped silently inside vte. (measured)
```rust
// vte-0.15.0/src/lib.rs:377
0x5E..=0x5F => self.state = State::SosPmApcString,
// vte-0.15.0/src/lib.rs:182
State::SosPmApcString => self.anywhere(performer, byte),
// vte-0.15.0/src/lib.rs:438-450 — every byte except CAN/SUB/ESC is discarded
fn anywhere<P: Perform>(&mut self, performer: &mut P, byte: u8) {
    match byte {
        0x18 | 0x1A => { performer.execute(byte); self.state = State::Ground },
        0x1B => { self.reset_params(); self.state = State::Escape },
        _ => (),
    }
}
```

**DCS (Sixel, `ESC P … q … ESC \`).** It is delivered to `hook`/`put`/`unhook` on vte's ansi performer, which only logs. `Handler`, the trait alacritty's `Term` implements, has no DCS method. (measured)
```rust
// vte-0.15.0/src/ansi.rs:1311-1326
fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
    debug!("[unhandled hook] params={:?}, ints: {:?}, ignore: {:?}, action: {:?}", ...);
}
fn put(&mut self, byte: u8) { debug!("[unhandled put] byte={:?}", byte); }
fn unhook(&mut self) { debug!("[unhandled unhook]"); }
```
- The primary device attributes reply is `"\x1b[?6c"` (`alacritty_terminal term/mod.rs:1261`), so TD does not advertise sixel (attribute 4). (measured)
- Kitty's `a=q` support query therefore goes unanswered, and a probing program falls back after its own timeout. (inferred for TD; the program-side behaviour is outside knowledge, hunch)
- Because the `Processor` is private to the `EventLoop`, the only interception point that does not fork alacritty is a `Read` wrapper around the byte source, the way `TeeReader` and `CountingReader` already sit. The other routes are forking or patching alacritty_terminal, or swapping the VT layer. (inferred)

**PTY pixel size.** It is set, as truncated logical pixels. (measured)
- alacritty computes `ws_xpixel = ws_col * cell_width`, `ws_ypixel = ws_row * cell_height`, then `TIOCSWINSZ` (`tty/unix.rs:411-436`).
- TD passes `session.resize(grid, view.cell_w as u16, view.cell_h as u16)` (`pane.rs:3513-3514`) from `Session::resize` (`term.rs:143-152`).
- `cell_w` is the advance of 'M' and `cell_h` comes from the theme, scaled by text size (`pane.rs:3981-3991`, `pane.rs:2758-2762`).
- In hosted mode the size travels `SocketPty::on_resize` → `Request::Resize` → `host.resize` → ioctl.
- The live read in §11 shows `xpixel = cols × 6` on a monitor at scale 1.6, so these are logical pixels, with the fractional cell width truncated. (measured for the numbers; inferred for "logical")

**CSI 14t / 16t / 18t.** (measured)
- vte dispatches `14 => handler.text_area_size_pixels()` and `18 => text_area_size_chars()`. There is **no arm for 16** (cell size in pixels) (`vte ansi.rs:1739-1745`).
- alacritty emits `Event::TextAreaSizeRequest` with the formatter `\x1b[4;{h};{w}t` (`term/mod.rs:2259-2264`).
- The host answers it (`host.rs:705-713`).
- The window replica suppresses `PtyWrite` (`term.rs:94`, `Answers::Elsewhere`).
- In serverless mode, `TerminalView`'s event handler has no `TextAreaSizeRequest` arm (`pane.rs:3909-3975`), so 14t goes unanswered there.

## 8. Host and window: what crosses the wire

- The wire carries raw bytes plus control JSON: "One line of JSON per message on the control connection, and raw bytes on a second connection per pane" (`hostproto.rs:3-4`). (measured)
- **Requests:** `Hello`, `ListPanes`, `SpawnPane`, `AttachPane`, `Resize`, `ClosePane`, `GridCheck`, `Save`, `Watch` and `Shutdown` (`hostproto.rs:102-174`).
- **Pushes:** only `Mode` (`:184-188`).
- **Protocol version:** `PROTO_VERSION` 1 (`:26`).
- **Attach snapshot:** at attach, `gridwire::encode_snapshot` (`gridwire.rs:206`, called at `host.rs:849`) re-emits the host's grid as VT text: text, attributes, cursor and modes only (`gridwire.rs:20-34`). `grid_hash` (`gridwire.rs:438`) covers cells only.
- **Where the Terms live:** the authoritative one in the host (`host.rs:252`); the window renders a replica (`term.rs:289`). Two processes, two terminals and two parsers for one stream (`docs/plans/client-server/00-status.md:219-220`). Hosted mode has been the default since 2026-09-10 (`00-status.md:227-230`).

**If images arrived in the PTY stream.** (inferred)
- The window already receives the APC and DCS bytes, because the stream is raw. The window must decode and store them to draw pixels.
- Replies to graphics queries (`a=q`, the OK/error responses) must come from the host, the only side allowed to answer (`term.rs:58-70`).
- An image received before a re-attach is gone unless the host also stores placements, because the snapshot carries text only. That needs one of:
  - (a) `encode_snapshot` re-emitting stored images as Kitty APC, or
  - (b) a new `Push`/`Reply` variant carrying image blobs and placements.
- The divergence guard (`GridCheck`/`grid_hash`) is blind to images.
- The open issue "the attach snapshot cannot see a half-parsed escape sequence" already names APC and DCS parser state as a hazard at attach time.
- Optional: device pixels per cell in `PaneGeom` (`hostproto.rs:69`), so the host can report true sizes.

## 9. Prior internal thinking (quoted)

- **Graphics were deferred.** `docs/2026-06-20-competitive-feature-mining-and-launch-plan-review.md:98`: "**Graphics protocol (Kitty/sixel)** | **Defer, but flag as a future moat** | … it's **XL** (new render stage, texture atlas, shm IPC). Revisit once the wall moat lands." (measured)
- **OSC 8 idea.** Same doc, line 15: alacritty's "per-cell `Hyperlink` (OSC-8) … already in the binary and unused". Line 53 proposed "Ctrl-click a URL/`file://`/PR link … → `xdg-open`". The Ctrl-click shipped, but by token scan; OSC 8 is still unused (§5). (measured)
- **Overlays should be native.** Same doc, line 106: "build hints/palette/picker as **native gpui overlays**, not subprocesses." (measured)
- **Image protocols are the known gap.** `docs/2026-08-29-foundation-interrogation-zed-gpui-quickshell.md:119-122`: rio-vt offers "sixel/kitty/iTerm2 image protocols … image protocols are the incumbent's one gap." Line 151, option F, "Swap the VT seam (rio-vt / libghostty-vt)", becomes right "when image support becomes a committed TD feature." (measured)
- **GUI-app panes declined.** Same doc, lines 289-293: a nested Wayland compositor is "a different product (GUI-app panes) … Noted as horizon, not plan." Lines 294-296: "chrome and terminal share the scene so the CRT pass warps them together per pane." (measured)
- **Documents open on the desktop, and there is no webview.** This is the decision the feature reverses: `surface.rs:388-389`, `td-surface-protocol.md:315-316` and `:728`, `attention-spine/plan.md:176-181` (quoted in §6). (measured)
- **Markdown route chosen, not built.** `reports/2026-09-12-agent-native-output-surface.html:672` and `:1048`: `pulldown-cmark` plus TD's own elements. (measured)
- **Split caps.** `MAX_PANES = 4` for new splits (`main.rs:118`) and 8 for loaded layouts (`main.rs:127`). `docs/features/07-terminal-core.md:20` still says "up to 8 panes", which is stale. (measured by the Q9 subagent)
- **Open issues that touch this work** (all open; measured by the Q9 subagent):
  - "Exempt only an overlay's own rectangle from the CRT warp"
  - Click hit-testing drifts low at maximum warp on tall panes
  - The overlay suppression list can drift from `close_popups`
  - The attach snapshot cannot see a half-parsed escape sequence (APC/DCS)
  - Decoded memory for animated scenes was never measured
- Searches for issues about sixel, kitty graphics, inline images and webviews found none.

## 10. The outside summary's names

- `grep -rIi --exclude-dir=.git <name> .` over the worktree gives **zero hits** for TubeTarget, EffectGraph, SurfaceHost, CellTarget, PixelTarget, ChildPTYTarget and Ratatui/ratatui, in code and docs, case-sensitive and case-insensitive. (measured)
- `git log --all -S` finds all of them except EffectGraph in one file only: `research/2026-09-24-SURFACE-ARCHITECTURE.md` on the attempt ref `refs/attempts/gui-in-the-tui/haiku-pass` (commit `f0095a9`). That file calls "SurfaceHost (compositor) ├── CellTarget (Ratatui terminal cells) …" "Existing TD architecture unchanged". That is false: TD has no ratatui dependency and no such types. EffectGraph appears in no ref. (measured)
- **"Tube" is real**, with 236 hits in `app/src`. It means a warp-registered pane content rectangle (`warp.rs:15-28`, `type Tube`, `register_tube` at `warp.rs:111`), not a render-target abstraction. The real chain is pane screen `canvas` → `warp::register_tube` → `gpui_wgpu::set_crt_rects_tubes` → the `crt_pass` post-pass over the whole offscreen scene. (measured)

## 11. Geometry for mockups

**Defaults from the code.** (measured)
- Font size 14 and cell height 20 unless the theme says otherwise (`theme.rs:2766-2767`). All six bundled themes set 14/20 (`app/themes/*.toml`).
- Family: the desktop's `monospace` if available, else "JetBrains Mono" (`pane.rs:6909-6917`). `fc-match monospace` on this box gives JetBrainsMono Nerd Font.
- The grid scales both numbers by the text-size grade (`pane.rs:2758-2762`). The house default is `HOUSE_TEXT_SIZE = 0.75` (`theme.rs:2370`), giving 10.5 px and 15 px.
- Cell width is the advance of 'M' (`pane.rs:3984-3991`). JetBrains Mono's advance is 0.6 em, so about 6.3 px at 10.5 px. (inferred; font metric from outside knowledge)

**Live, this pane.** (measured)
- `get_pane_config` reports `text_size` 9.75%. On the 0.6..2.0 range (`theme.rs:360`, `:374-380`) that is 0.7365, so the font is ≈10.3 px and the cell ≈6.2 × 14.7 logical px (inferred arithmetic).
- A read-only `TIOCGWINSZ` on the pane's pty (a script in the scratchpad) gave:

| pane (pid, tab) | cols × rows | ws_xpixel × ws_ypixel | reported cell |
|---|---|---|---|
| 3768847, tab 17 (this agent) | 189 × 49 | 1134 × 686 | 6 × 14 |
| 4146385, tab 12 (one of three splits) | 91 × 22 | 546 × 308 | 6 × 14 |
| 1334738, tab 15 | 74 × 49 | 444 × 686 | 6 × 14 |

- The TD window (pid 1743308) is 1576 × 950 logical px on eDP-1, which runs at **scale 1.6** (`hyprctl`).
- So one reported cell of 6 × 14 is about 9.9 × 23.6 device pixels. An image sized from `ws_xpixel` would be about 1.6× too small in device pixels and about 3% narrower than the true logical cell, because the width is truncated. (inferred)
- `list_panes` exposes no geometry, and neither does the ctl socket.

---

## Integration seams

Each seam gives the file and function where the piece would attach, what is already there, and what is missing. All are **inferred** from the code cited above.

**A native image/document surface in a pane.**
- Attach in `TerminalView::render` (`pane.rs:7370`), inside the screen `div` (`pane.rs:8587-8740`). Use a `gpui::img(ImageSource::…)` element, or a `canvas` that calls `window.paint_image(bounds, corners, Arc<RenderImage>, frame, false)`.
- Decoded pixels come from `RenderImage::new(Frame)` (BGRA) or `Image::from_bytes(fmt, bytes)`. Cache the `Arc<RenderImage>` on the view, and `window.drop_image` it when it is replaced.
- Warp: it bends with the tube by default. For flat, add a front-inserting variant of `warp::register_tube` (`warp.rs:111`), since the first match wins at `crt_pass.wgsl:63-78`. That is also what the warp-exemption issue asks for.
- Clicks inside it must go through the pane's inverse map, the way `bench_hit_at` does, from `TerminalView::on_mouse_down` (`pane.rs:6122`).
- Markdown needs a new parser dependency (the recorded choice is `pulldown-cmark`) and new block elements. There is none today; `benchdraw::paragraph` is plain lines.
- HTML has no in-process renderer. Rejected on the record (`td-surface-protocol.md:728`).

**A pane that holds a document instead of a PTY.** There are two paths.
- **(a) Cheapest: a third face on `TerminalView`.**
  - Extend `workbench::Face` (`workbench.rs:76-80`) or add a sibling field.
  - Branch at `pane.rs:8677` (`if on_bench { bench_el } else { grid }`).
  - Persist on the leaf by adding a field to `SavedNode::Leaf` (`main.rs:735-758`) and `LeafState` (`main.rs:700-708`).
  - Create it through `Workspace::split` (`main.rs:9203`) or a sibling "split with document", which calls `make_pane_in_mode` (`main.rs:4957`) and then sets the face.
  - Cost: a shell still spawns behind the document. The pane counts against `MAX_PANES = 4`.
- **(b) A real non-terminal leaf.**
  - Change `type Node = Tree<Entity<TerminalView>>` (`main.rs:320`) to an enum leaf.
  - Touch `render_node` (`main.rs:22341`), `wire_pane` (`main.rs:4211`), save and restore (`SavedNode`), and the leaf-walking code (63 `.leaves(` call sites in `main.rs`).
  - The `Tree<L>` operations are already generic (`main.rs:329-420`).
- **Programmatic creation:** there is no split or document verb. Add one next to `adopt` in `ctl.rs` (parse around `ctl.rs:430`, apply at `ctl.rs:885`, following `queue_adopt`/`drain_pending_adopts` at `main.rs:7333-7348`), or as an MCP tool in `mcp.rs`.

**A floating, draggable document inside a pane.**
- **Rendering:** a child of the screen `div` after `note_el` (`pane.rs:8723`), so it sits above the grid and under the glass.
- **State on `TerminalView`:** rect, source and drag anchor, following the `Option<Sticky>` pattern.
- **Press, move and release:** in `on_mouse_down`/`on_mouse_move`/`on_mouse_up` (`pane.rs:6122`, `:6264`, `:6317`), ahead of the sticky branch at `:6177`. Hit-test like `sticky::Hit::at` (`sticky.rs:851`), but with the warp inverse, because an image cannot be pre-warped.
- **Drag state:** copy the `PaneDrag` shape (`main.rs:2857-2873`: start, at, engaged threshold), held on the pane rather than the workspace.
- **Persistence:** alongside `SavedNote` in `SavedNode::Leaf`.
- **Warp:** either it bends (and clicks go through the inverse), or it needs the front-inserted flat tube above. `over_the_glass` (`main.rs:21569`) is the wrong tool: it flattens every pane.

**Ctrl+click routing.**
- The branch is `TerminalView::on_mouse_down` at `pane.rs:6240-6246`, plus the context-menu item at `pane.rs:7414-7422`.
- Classify the resolved target from `link_under` (`pane.rs:4293`) by extension or MIME (`.md`, `.html`, `.png`/`.jpg`/`.webp`/`.gif`/`.svg`), and emit a new pane event such as `OpenInTd { path, how: Split|Float }` instead of calling `open_with_system`.
- Split: subscribe in `wire_pane` (`main.rs:4211`), exactly like `DragPaneStart` at `main.rs:4321`, and have `Workspace` split. Float: handle it inside the pane.
- Keep `open_with_system` as the fallback, and probably as the Shift+click meaning, since the no-viewer rule is a written decision this reverses.
- Detection gaps to close at the same time:
  - Bare relative paths (`docs/x.md`) are not links (`pane.rs:339-345`).
  - OSC 8 `cell.hyperlink()` is ignored.
- Other open sites to route the same way: `pane/bench.rs:2120`, `:3356`, `main.rs:19430`, `:20258`.

**A Kitty/Sixel decoder in the PTY path.**
- **Window side (for pixels):** wrap the replica's reader beside `CountingReader` (`socketpty.rs:93-108`), created in `term::attach_in` (`term.rs:289-310`) and `wire_event_loop` (`term.rs:235`). It must remove APC `G` and sixel DCS sequences before alacritty sees them, or else re-inject cursor movement. A Kitty placement moves the cursor unless `C=1`, and vte cannot express that. The `CountingReader` byte count must still agree with the host.
- **Host side (for replies and re-attach):** the same wrapper around `TeeReader` (`host.rs:147-162`), so the host answers `a=q` and stores placements. Replies go out through the existing answers channel (`host.rs:697-713`).
- **Wire:** image placements in `gridwire::encode_snapshot` (`gridwire.rs:206`), or a new `Push`/`Reply` variant in `hostproto.rs` (Reply at `:259-322`, Push at `:184-188`). Consider device-pixel cell size in `PaneGeom` (`hostproto.rs:69`).
- **Size queries:** add a CSI 16t reply (none today) and use device-pixel values. Serverless mode also needs a `TextAreaSizeRequest` arm in `pane.rs:3909-3975`.
- **Rendering:** placements anchored to grid cells drawn in the grid `div` (`pane.rs:8680-8711`), with rows mapped through `paint_to_grid` (`pane.rs:1931`). Kitty Unicode placeholders would let the anchor survive the text-only snapshot. (hunch)
- **Alternatives:** patching or forking `alacritty_terminal` (today it is crates.io, not forked), or the recorded "swap the VT seam" option (`docs/2026-08-29-foundation-interrogation-zed-gpui-quickshell.md:151`).
