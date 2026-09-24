# Document renderers inside a pane

Research date 2026-09-24. Scope: what it takes for Terminal Delight (TD) to render an image, a Markdown file, an HTML brief or a PDF inside a region of its own window, reached by Ctrl+click on a path and shown either in a new split or as a floating square inside the current pane. Write-up only; no repository was modified.

Confidence labels: **measured** (read in code or docs and cited, or run on this machine), **inferred** (follows from code that was read, not run), **hunch** (judgement, no direct evidence). "Not found" means searched for and absent. Effort: **S** is two days or less, **M** two weeks or less, **L** longer.

Parker's HTML briefs are the case that decides most of this. They are single-file pages whose notes system (`~/.claude/skills/decision-brief/assets/notes.js`) needs JavaScript, `localStorage`, `<dialog>.showModal()`, one `confirm()`, and a Blob download for "save into file" (`notes.js:73-79`, `:169`, `:222`, `:252-256`, `:264`). A renderer without JavaScript can show a brief; it cannot annotate one.

Source trees, with the commit each citation refers to:

| Tree | Where | Commit / date |
|---|---|---|
| TD | `/home/parker/Work/td-gui-in-the-tui` (paths below are `app/src/…`) | `de7776b`, 2026-09-24 |
| TD's gpui | `/home/parker/Work/zed-upstream` | `abbe85a3`, 2026-06-12, plus uncommitted TD patches |
| markdown-delight | `/home/parker/BROWN-FAMILY-SPORTS/Software/markdown-delight` | `425041b` "wip: pre-transfer snapshot", 2026-08-18; last real commit `38dc3bb`, 2026-06-20 |
| markdown-delight's gpui | `/home/parker/BROWN-FAMILY-SPORTS/Software/zed-upstream` | `611b458` snapshot on top of `abbe85a3` |
| Servo fork | `/home/parker/BROWN-FAMILY-SPORTS/Software/servo`, branch `t3/native-read` | `c5cbd93` snapshot 2026-08-18; upstream base `495c38d694d`, 2026-06-17 |
| servo-agent | `/home/parker/BROWN-FAMILY-SPORTS/Software/servo-agent` | `e96bf2f`, 2026-09-03 |
| wgpu-graft (web) | https://github.com/merely-made/wgpu-graft | `main`, fetched 2026-09-24 |

The sibling note on terminal graphics protocols is `graphics-protocols.md` in this directory; it establishes that no Kitty or Sixel image reaches TD's emulator today, which matters for the PTY fallbacks below.

---

## Summary — the facts that decide it

1. **Images are already solved by the gpui TD builds against.** `gpui::img()` decodes PNG, JPEG, WebP, GIF, BMP, TIFF, ICO and PNM on a background task, animates GIF and animated WebP, and rasterises SVG files in full colour through resvg (`zed-upstream/crates/gpui/src/elements/img.rs:605-748`, `crates/gpui/src/platform.rs:2057-2078`). TD already calls `gpui::img()` with a path at eight call sites (`pane.rs:8099`, `:8127`; `main.rs:14205`, `:17342`, `:17348`, `:17359`, `:23683`, `:25776`). An image view is S. (measured)

2. **markdown-delight's renderer is one 482-line module, MIT, on the same Zed commit as TD, and it should be lifted rather than shared.** `app/src/render.rs` depends on nothing but comrak, gpui and about 20 lines of `BlockMeta`, `normalize` and `fingerprint` from `comments.rs:96-125`. Both apps pin `zed_rev = abbe85a3321bf6cb7f5b241e623d9c2e16c29187` (TD `app/Cargo.toml:16`, markdown-delight `app/Cargo.toml:11`); TD's checkout differs only by an additive glyph-transform patch in `gpui/src/window.rs` (+40 lines), and the two CRT-pass diffs in `gpui_wgpu` are byte-identical. (measured)

3. **The module needs three changes before it is TD's Markdown surface.** Its colours are hard-coded to the hacker palette (`render.rs:17-23`, used throughout `element()`), it throws away link URLs and image sources (`render.rs:234` matches `NodeValue::Link(_) | NodeValue::Image(_)` and keeps only the text), and it shows raw HTML blocks as source text (`render.rs:415-420`). (measured)

4. **Zed's own Markdown crates are not an option.** `markdown`, `markdown_preview` and `image_viewer` are GPL-3.0-or-later, and walking their `Cargo.toml` files pulls in 53, 94 and 94 Zed workspace crates against gpui's 10 — language, lsp, git, rpc, project, workspace. (measured)

5. **gpui on Linux cannot show a texture it did not upload itself.** `gpui::surface()` exists only on macOS (`gpui/src/elements/surface.rs:13-33`) and the wgpu renderer drops surface batches with the comment "Surfaces are macOS-only for video playback" (`gpui_wgpu/src/wgpu_renderer.rs:1590-1593`). So any live foreign renderer — Servo, Blitz on its own device, WPE — reaches the pane as CPU pixels in a `RenderImage`, unless TD adds a sixth `gpui_wgpu` patch for external textures. (measured)

6. **Per-frame CPU readback of a 1000×1400 region costs about 7 ms of copying before the GPU transfer is counted.** A benchmark of Servo's vertical flip, the RGBA→BGRA swap and gpui's atlas staging copy on this machine measured 6.98–7.11 ms per frame. That is fine for a page that repaints on scroll and hover, and roughly half a core at a continuous 60 fps. (measured, method below; the 60 fps figure is arithmetic)

7. **Parker's brief JavaScript runs in the Servo fork, and two things fail.** Driving a copy of a real brief headless through servo-agent's WebDriver wrapper, notes.js tagged 38 note affordances, `showModal()` opened the note dialog on top, and a note was added. `localStorage` threw `SecurityError: Cannot access localStorage from opaque origin` on `file://`, so notes would not survive a reload, and "save into file" navigated the page to a `blob:null/…` URL because Servo does not implement `<a download>` (`components/script/dom/html/htmlanchorelement.rs:375`, a TODO). (measured)

8. **Servo misrenders at least one layout Parker's briefs use.** The same brief's four stat tiles use `grid-template-columns: repeat(auto-fit, minmax(148px, 1fr))`; headless Chromium lays them out four across, the Servo fork stacks them in one column. The fork's upstream base is 1,537 commits behind Servo `main` (GitHub compare, 2026-09-24), so this may already be fixed upstream. (measured; the upstream state is unknown)

9. **Servo is heavy next to TD.** A clean dev build of `servoshell` took 596.4 s over 1,005 units on this machine (`target/cargo-timings/cargo-timing.html`, 2026-09-16); the debug binary is 2.0 GB and `target/` is 19 GB. Rendering one brief headless peaked at 378 MB resident. TD's release binary is 49.5 MB; Servo's benchmark bot reports a 229.96 MB release build (https://github.com/servo/servo/pull/45421). (measured)

10. **A live Servo view inside a gpui window already exists as a demo.** wgpu-graft's `demo-servo-gpui` reads each frame back to the CPU, swaps to BGRA, wraps it in a `RenderImage` and shows it with `img(ImageSource::Render(…))` (https://raw.githubusercontent.com/merely-made/wgpu-graft/main/demo-servo-gpui/src/main.rs, lines 7-21 and 170-191). It uses a no-op waker and polls Servo on every animation frame, and it never calls `drop_image`. Read it before the TD spike; its polling loop and its missing frame release are the parts to replace. (measured)

11. **No WebKit route embeds in a non-GTK Wayland window.** wry's `build_as_child` is "Only X11 is supported" (https://docs.rs/wry/latest/wry/struct.WebViewBuilder.html), and gpui-kit's WebView "Only supports macOS and Windows currently" (https://raw.githubusercontent.com/longbridge/gpui-kit/main/crates/webview/README.md). WPE WebKit can export frames as dmabufs, but the Rust wrapper found for that path, `waterui-browser-wpe`, was created on 2026-09-05. (measured)

12. **TD's structure favours the floating square, and TD has a written rule against its own viewer.** The split tree's leaf type is `Entity<TerminalView>` (`main.rs:309-320`), so a document in a split needs a new leaf kind through the tree and its saved form (`main.rs:735`); a floating square inside a `TerminalView` does not. And `surface.rs:386-389` says of Markdown: "Open it with the desktop's own handler. Never our own viewer: the person already told their machine where Markdown goes." This feature reverses that decision, and the reversal should be made on purpose. (measured)

---

## Where TD stands today

Ctrl+click on a path already works: `pane.rs:6240-6246` resolves the link under the cursor and calls `open_with_system` (`pane.rs:696-702`), which runs `xdg-open`, through `uwsm-app` on an Omarchy session. Super+Ctrl+click reveals the file in a file manager instead (`pane.rs:6226-6236`). The handlers on this machine today, from `xdg-mime query default`: `markdown-delight.desktop` for Markdown, `chromium.desktop` for HTML, `imv.desktop` for PNG, `org.gnome.Evince.desktop` for PDF. (measured)

Three pieces of TD bear on any in-pane document:

- The CRT treatment is a post-process over rectangles of the finished frame, registered through a global (`gpui_wgpu/src/wgpu_renderer.rs:114-126`, `set_crt_rects`). Anything painted inside a pane's rectangle gets the curvature and scanlines, whatever produced the pixels. (measured)
- Pointer positions under the warp are mapped back by `workbench::unwarp` (`workbench.rs:1782`). A document view that forwards clicks to an engine needs the same mapping. (measured)
- The workbench's Markdown surface kind draws its body as one plain paragraph (`benchdraw.rs:993`). A native Markdown renderer would have a second customer there. (measured)

---

## 1. Per format

### Images

| Option | Fidelity | Interactive | Effort | Runtime cost | License |
|---|---|---|---|---|---|
| `gpui::img(path)` in a pane or floating div, with zoom and pan | Full for rasters; animated GIF/WebP play; SVG in colour | Zoom/pan you write | **S** | Decoded frames in RAM plus one atlas texture | Apache-2.0 (gpui), MIT/Apache (`image`) |
| `imv` in its own window (today) | Full | Yes | none | separate process | MIT |

- Decoding runs in `ImageAssetLoader::load` (`img.rs:605-748`): `image::guess_format`, then per-frame decoders for GIF and animated WebP with RGBA→BGRA conversion, and anything that is not a raster falls through to `svg_renderer.render_single_frame` (`img.rs:743-744`). (measured)
- SVG files are rasterised once at `SMOOTH_SVG_SCALE_FACTOR = 2` (`gpui/src/svg_renderer.rs:81`, `:173-195`), so zooming past about 2× shows softening (inferred). Use `img()`, not `gpui::svg()`: TD's own `art.rs:13-16` records that `svg()` paints a single-colour mask. (measured)
- Zed's `image_viewer` shows the pattern: an absolutely positioned `img()` inside a div sized by zoom and offset by pan, over a `checkerboard()` background (`crates/image_viewer/src/image_viewer.rs:387-414`). The crate is GPL, so the roughly 150 lines are rewritten, not copied. (measured)
- Every image is a separate atlas allocation; the atlas grows textures from 1024² up to the device's `max_texture_dimension_2d` (`gpui_wgpu/src/wgpu_atlas.rs:185-195`). An image larger than that limit needs downscaling before display, and a 4000×3000 photo costs about 48 MB of RAM and the same again on the GPU (inferred arithmetic, 4 bytes a pixel). Whether gpui's path-keyed asset cache notices an edited file was not verified.

### Markdown

| Option | Fidelity | Interactive | Effort | Runtime cost | License |
|---|---|---|---|---|---|
| Lift markdown-delight's `render.rs` into TD | CommonMark + GFM tables, strikethrough, task lists, autolinks; no images, no code highlighting, raw HTML as text | Selection and comments live in its 2,520-line `pane.rs`; not in the lift | **S** | Parse once per load, build elements per frame (`render.rs:1-7`) | MIT, comrak BSD-2 |
| Zed `markdown` / `markdown_preview` | High (syntax highlighting, mermaid) | Yes | L | pulls 53–94 Zed crates | **GPL-3.0-or-later** — excluded |
| markdown-delight in its own window (today) | Its own | Full: edit, read, comment modes | none | single-instance socket, second opens are cheap | MIT |
| Markdown → HTML → a web engine | High | JS irrelevant | M | an engine per document | — |

Detail in section 2.

### HTML, static (read-only)

| Option | Fidelity on a real brief | JS | Effort | Runtime cost | License |
|---|---|---|---|---|---|
| Headless Chromium `--screenshot` → PNG → `img()` | Full; tile grid correct | Runs during the render; the result is pixels | **S** | 0.99 s to a 1000×1400 PNG | BSD-3, separate process |
| Servo `servoshell -z -o` → PNG → `img()` | Tile grid collapses to one column | Runs during the render | **S** | 3.6 s from launch to PNG (debug build), 378 MB peak RSS | MPL-2.0, separate process |
| Blitz in-process → CPU buffer → `RenderImage` | Unknown on briefs | None in the released crates | **M** | not measured | MIT/Apache (`stylo_taffy` MPL-2.0) |
| HTML → Markdown (servo-agent `read_page`) → native Markdown | Loses every figure | n/a | S | fast | Apache-2.0 |

- Chromium: `chromium --headless=new --disable-gpu --hide-scrollbars --window-size=1000,1400 --screenshot=… file://…` on `2026-09-09-client-server-review.html` finished in 992 ms and matched the page as authored. The capture is the viewport only; a full page needs a tall window or the DevTools protocol (inferred). (measured)
- Servo: the same brief through `target/debug/servoshell --headless --window-size=1000x1400 -o brief.png` wrote the PNG 3.6 s after launch; peak `VmHWM` was 378,012 kB. The process did not exit after writing and was killed after four minutes — seen once, not investigated. Headless servoshell renders through `SoftwareRenderingContext` (`ports/servoshell/desktop/headless_window.rs:50`). (measured)
- Blitz is "currently in a beta state", at 0.3.0-beta.2 (2026-08-24); `VelloImageRenderer::render_to_vec` gives CPU pixels, and `vello::Renderer::render_to_texture` could paint into a texture on a shared wgpu device (https://raw.githubusercontent.com/DioxusLabs/blitz/main/README.md, https://docs.rs/anyrender_vello/latest/anyrender_vello/struct.VelloImageRenderer.html, https://docs.rs/vello/latest/vello/struct.Renderer.html). Blitz asks for crates.io `wgpu = "29"`, while Zed uses a git fork of wgpu 29.0.3 (`zed-upstream/Cargo.toml:829`), so sharing a device would need a `[patch]` redirect as well as the missing gpui external-texture primitive (inferred). A Boa-based JavaScript crate, `blitz-vibey-script`, exists in the repo but is not published. Whether Blitz draws inline SVG was not verified.
- HTML→Markdown is wrong for briefs: the pictures are the medium and would be dropped. (inferred)

### HTML, interactive

| Option | JS and notes | Effort | Runtime cost | License |
|---|---|---|---|---|
| Servo `WebView` in the pane, CPU readback per frame | JS runs; dialogs work; persistence and save need host fixes (below) | spike S–M, product M–L | ~7 ms CPU per changed frame plus transfer; ~0.4 GB resident in one headless measurement; +~230 MB binary; +~10 min clean build | MPL-2.0 |
| Servo with zero-copy frames (GL↔Vulkan interop + a `gpui_wgpu` patch) | same | **L** | sub-millisecond per frame (inferred) | MPL-2.0 |
| Headless Chromium over the DevTools protocol, screencast frames into `img()` | Full; `file://` storage likely works | M (hunch) | not measured; one encode and decode per frame | BSD-3, separate process |
| WPE WebKit exporting dmabufs | Full (JavaScriptCore) | **L** (hunch) | not measured | LGPL engine; wrappers MIT/Apache or MPL |
| WebKitGTK through wry | — | not possible on Wayland without GTK | — | — |
| gpui-kit (gpui-component) WebView | — | macOS and Windows only | — | Apache-2.0 |
| Carbonyl in a PTY | Chromium, JS runs | S | Chromium process | BSD-3 |
| awrit in a PTY | Chromium, JS runs | — | — | BSD-3, archived |

- Servo's details, the measurements and the spike are in section 3.
- The Chromium-over-DevTools route was not in the brief and was not investigated. It keeps Chromium out of TD's build at the price of an encoded frame stream. `confirm()` blocking a DevTools-driven tab is a known trap on this machine (memory note `a-native-confirm-freezes-a-cdp-driven-tab`), which an embedder has to answer through the protocol's dialog events (inferred).
- WPE: "Since 2.54, WPEPlatform is the default integration layer", with `WPEBufferDMABuf` output and a headless display (https://wpewebkit.org/about/architecture.html). `waterui-browser-wpe` 0.5.1 composites a WPE dmabuf stream into wgpu "without CPU readback" but was created 2026-09-05 and has 93 downloads (https://docs.rs/waterui-browser-wpe/latest/waterui_browser_wpe/). (measured)
- wry on Linux Wayland needs `build_gtk` into a GTK container (https://docs.rs/wry/latest/wry/trait.WebViewBuilderExtUnix.html); TD has no GTK window to give it. (measured)
- Carbonyl draws with half-block characters and has had no commit since 2023-02-26 (https://api.github.com/repos/fathyb/carbonyl); a brief at terminal-cell resolution would be unreadable (hunch). awrit needs the Kitty graphics protocol, which TD does not receive today (`graphics-protocols.md`), and was archived on 2026-04-25 (https://github.com/chase/awrit). (measured)
- Verso, the Servo-based browser, was archived on 2025-10-08 because it could not keep pace with Servo (https://github.com/versotile-org/verso). (measured)

### PDF

| Option | Fidelity | Interactive | Effort | Runtime cost | License |
|---|---|---|---|---|---|
| `pdftoppm` subprocess → PNG per page → `img()` | Poppler's | none (pixels) | **S** | 102 ms for one text page at 130 dpi (1105×1430) | GPL, separate process |
| hayro → pixmap → `RenderImage`, or lift gpui-pdf | Good, experimental | gpui-pdf claims search, highlights, forms | **M** | CPU raster (`vello_cpu`) | Apache-2.0 OR MIT; gpui-pdf MIT |
| pdfium-render → `DynamicImage` → `RenderImage` | High (Chromium's engine) | what you build | M | needs `libpdfium.so` | MIT OR Apache-2.0 |
| mupdf bindings | High | — | — | — | **AGPL-3.0** — excluded |
| pdf.js inside a web engine | High | Yes | rides on the HTML engine | an engine | Apache-2.0 |

- `pdftoppm` (poppler 26.08.0, installed) rendered page 1 of `/usr/share/doc/speexdsp/manual.pdf` at 130 dpi in 102 ms including process start. (measured)
- hayro 0.7.1 calls itself "an experimental, work-in-progress PDF interpreter and renderer"; `render(…) -> Pixmap` has "no dependencies on the GPU" (https://github.com/LaurenzV/hayro). (measured)
- gpui-pdf 0.6.1 (MIT, 2026-09-22) is a hayro-based PDF viewer for gpui, but it depends on `gpui-pre ^0.3`, not TD's pinned gpui (crates.io dependency API) — a lift, not a dependency. (measured)
- pdfium-render does not bundle Pdfium; it binds to a system or downloaded library (https://raw.githubusercontent.com/ajrcarey/pdfium-render/master/README.md). `pdfium` is not installed on this machine (`pacman -Q pdfium` found nothing). (measured)

---

## 2. Can markdown-delight's renderer be TD's Markdown surface?

Yes, by lifting one module.

**How it is built.** markdown-delight is a single binary crate of 10,673 lines with no library target (`app/Cargo.toml`, `app/src/*.rs`). The renderer is `render.rs`, which parses CommonMark+GFM with comrak once into an owned `Block` tree (`render.rs:35-44`, `:60-93`) and builds gpui elements from that tree every frame (`render.rs:341-482`); the split exists so the CRT tracking band can animate without re-parsing (`render.rs:1-7`). `parse_with_meta` also emits a fingerprint and source byte range per block, which the comment system anchors to (`comments.rs:98-125`). The editor, comments, CRT effects, IPC and fuzzy finder are separate modules that `pane.rs` (2,520 lines) and `main.rs` (4,259 lines) tie together. `docs/PLAN.md:102-107` records the founding decision: comrak AST to native gpui elements, "No webview, ever". (measured)

**gpui skew between the two copies.** Measured with `git log` and `git diff` in both checkouts:

| | TD | markdown-delight |
|---|---|---|
| Declared pin | `abbe85a3321b…` (`app/Cargo.toml:16`) | `abbe85a3321b…` (`app/Cargo.toml:11`) |
| Checkout HEAD | `abbe85a3`, committed 2026-06-12 16:48 UTC | `611b458` "wip: pre-transfer snapshot", 2026-08-18, parent `abbe85a3` |
| `gpui_wgpu` CRT pass | uncommitted; `wgpu_renderer.rs` diff identical to markdown-delight's, `crt_pass.wgsl` identical | committed in the snapshot |
| `gpui/src/window.rs` | +40 lines: `with_text_transformation` and a `text_transform` field (glyph-transform patch) | unpatched |
| `sum_tree` | `zlog`/`ztracing` severed (GPL removal) | same |
| Distance from Zed `main` | 1,680 commits (GitHub compare, 2026-09-24) | same base |

The skew is one additive public method. `render.rs` does not call it, so the module compiles against TD's gpui as is (inferred). The risk lies ahead: TD's `docs/patches/PINLOG.md` plans a quarterly bump that crosses wgpu, taffy and accesskit changes, and markdown-delight has had no real commit since 2026-06-20.

**What the module lacks for TD.** Four things, all visible in the code:

- A palette parameter. Block colours are constants (`render.rs:17-23`) set with `.text_color(rgb(…))` directly on each element, so an ancestor cannot override them; markdown-delight's own `pane.rs` merges the active theme only into paragraph runs (`pane.rs:1136-1158`, `:1271-1281`). Headings, code, quotes and tables therefore stay hacker-green under the `paper` and `amber` themes (inferred from the code, not seen on screen). The tokens map one to one: markdown-delight's `Theme` has `bg`, `surface`, `text`, `accent`, `faint` (`theme.rs:103-110`) and TD's has the same five plus `complement` and `human` (`theme.rs:112-134`).
- Link targets and image sources. The `Block` model keeps only text for `Link` and `Image` nodes (`render.rs:234-246`), so links cannot be clicked and images are never drawn. Adding a URL to the inline runs and an `Image` block that calls `img(doc_dir.join(src))` is small.
- Code highlighting and footnotes: `md_options()` turns on tables, strikethrough, task lists and autolinks only (`render.rs:46-53`); code blocks are plain lines.
- Raw HTML: shown as muted source text (`render.rs:415-420`).

**The three paths.**

1. **Shared crate.** Extract `render.rs` into a crate both apps depend on. Both would have to resolve gpui to the same package, and today they point at two different checkouts (`/home/parker/Work/zed-upstream` and `/home/parker/BROWN-FAMILY-SPORTS/Software/zed-upstream`); Cargo treats path dependencies at different paths as different packages, so a shared crate needs a shared gpui source or a `[patch]` in each consumer (inferred). That is coordination across a dormant repository for 500 lines.
2. **Lift the code.** Copy `render.rs` plus `BlockMeta`, `normalize` and `fingerprint` into a TD module, add the palette parameter, link URLs and image blocks. Same author, same license, same gpui. **S.** Take comrak with `default-features = false`: its defaults are `["cli", "syntect", "bon"]` (https://raw.githubusercontent.com/kivikakk/comrak/v0.52.0/Cargo.toml), which is why markdown-delight's lockfile carries `clap`, `syntect` and the `onig` C library that `render.rs` never uses (measured in `app/Cargo.lock`). The extension switches `render.rs` sets are runtime `Options`, not cargo features (inferred).
3. **Launch markdown-delight as its own window.** This is what Ctrl+click does now. Its single-instance socket forwards a second launch to the running primary before touching the GPU (`ipc.rs:8-9`, `main.rs:4148-4156`), so it is quick after the first open. It is not inside TD, and it cannot become a floating square in a pane.

The cheapest real path is the lift. Comment mode is the natural second step, since it gives Markdown the same annotate-and-export loop the briefs have, but it lives in `comments.rs` (714 lines), `comment_ui.rs` (147) and the selection code in `pane.rs`, and is M on its own.

---

## 3. Servo in a pane

### The embedding API, as read in the fork

- `RenderingContext` (`components/shared/paint/rendering_context.rs:35-88`) is an OpenGL/GLES context: `read_to_image`, `size`, `resize`, `present`, `make_current`, `gleam_gl_api`, `glow_gl_api`, plus defaulted `create_texture`/`destroy_texture` for surfman surfaces and `refresh_driver`. The public implementations:
  - `SoftwareRenderingContext` (`:289-397`): surfman's software adapter, a swap chain, "will generally have bad performance".
  - `WindowRenderingContext` (`:407-588`): binds an EGL surface to a raw window handle (`:449-462`). `offscreen_context()` (`:470-475`) returns an `OffscreenRenderingContext` (`:732-904`), an FBO whose `render_to_parent_callback` blits into the parent window's framebuffer (`:759-815`).
  - The surfman glue that backs all three, `SurfmanRenderingContext`, is private (`:98`).
- Pixels out: `read_to_image` is `glReadPixels` followed by a CPU vertical flip that clones the whole buffer (`:685-729`). `WebView::take_screenshot` (`components/servo/webview.rs:784-793`) waits for load, fonts, images and pending frames first.
- Input in: `WebView::notify_input_event` (`webview.rs:608-631`) takes `InputEvent::{EditingAction, Ime, Keyboard, MouseButton, MouseLeftViewport, MouseMove, Touch, Wheel}` (`components/shared/embedder/input_events.rs:53-64`); `notify_scroll_event` (`webview.rs:595`) scrolls directly.
- The loop: `ServoBuilder::event_loop_waker` (`components/servo/servo.rs:1449`) is how Servo asks the host to call `Servo::spin_event_loop` (`servo.rs:1053`); `WebViewDelegate::notify_new_frame_ready` (`webview_delegate.rs:948`) says a frame is ready for `WebView::paint` (`webview.rs:744`).
- Host hooks the brief needs: `show_embedder_control` (`webview_delegate.rs:1051`) delivers `alert`/`confirm`/`prompt` as `SimpleDialogRequest` (`components/shared/embedder/embedder_controls.rs:193-199`) for the host to draw; `request_navigation` and `load_web_resource` let it intercept loads; `evaluate_javascript` (`webview.rs:755`) runs script; `UserContentManager::add_script` and `add_stylesheet` (`components/servo/user_content_manager.rs:34`, `:52`) inject code and CSS; `ServoBuilder::protocol_registry` (`servo.rs:1454`) registers custom URL schemes; `WebView::notify_theme_change` (`webview.rs:495`) sets light or dark.
- The smallest embedder in the tree is `components/servo/examples/winit_minimal.rs` (193 lines): a `WindowRenderingContext` on a winit window, `ServoBuilder`, `WebViewBuilder::new(&servo, ctx)`, `paint()` then `present()` on redraw, wheel events forwarded.
- Desktop servoshell uses the window context plus an offscreen context for the web area and blits it under its egui chrome (`ports/servoshell/desktop/headed_window.rs:171-187`, `gui.rs:621`).

### What it would take inside TD

`WindowRenderingContext` is the wrong tool: it binds an EGL surface to the window handle, and gpui's wgpu renderer already owns that `wl_surface` (inferred). That leaves two ways to get frames:

- **CPU readback.** Render into `SoftwareRenderingContext`, or into a hardware offscreen GL context TD writes itself (the trait is public; wgpu-graft ships one, `ServoWgpuRenderingContext`), call `read_to_image` when `notify_new_frame_ready` fires, swap to BGRA, build a `RenderImage`, paint it with `img(ImageSource::Render(…))`. Each new `RenderImage` is a new atlas allocation, and gpui frees atlas tiles only through `App::drop_image` (`gpui/src/app.rs:2436`), which its image caches call (`elements/image_cache.rs:246`, `:292`, `:303`) and a `Render` source never does. The embedder must drop the previous frame itself or the atlas grows by a frame per paint (inferred; the wgpu-graft demo has no `drop_image` call).
- **Zero-copy.** Two new pieces. A Linux external-texture primitive in `gpui_wgpu`, most naturally a global registry keyed by rectangle the way the CRT pass is (`wgpu_renderer.rs:114-126`); `gpui_wgpu` already re-exports `wgpu` and a `WgpuContext` with public `device` and `queue` (`gpui_wgpu/src/gpui_wgpu.rs:7-15`, `wgpu_context.rs:9-13`), but the app has no handle to the live renderer's context. And a GL-to-Vulkan hand-off: Slint's Servo integration allocates a Vulkan image with exportable memory, imports it into GL through `GL_EXT_memory_object_fd`, blits Servo's framebuffer into it and wraps it for wgpu with `create_texture_from_hal` (https://slint.dev/blog/using-servo-with-slint). The GPU here is an RTX 3080 Mobile on NVIDIA 610.57.04 (`lspci`, `/sys/module/nvidia/version`); whether it exposes the needed external-memory extensions was not checked, because `vulkaninfo` and `eglinfo` are blocked by the shell guard.

Input needs a gpui-to-`keyboard_types` key map (servoshell's `ports/servoshell/desktop/keyutils.rs` does the winit version), device-pixel conversion, and `workbench::unwarp` when the pane is warped. Cursor changes come back through `notify_cursor_changed`.

### Readback cost for a ~1000×1400 region

One frame is 1000 × 1400 × 4 = 5.6 MB. The CPU work was measured with a small Rust program built at `opt-level = 2` (scratch benchmark, 200 iterations, two runs) that repeats what the code does per frame:

| Step | Source | ms per frame |
|---|---|---|
| Servo's flip: clone the buffer, copy rows back reversed | `rendering_context.rs:713-722` | 3.29–3.39 |
| RGBA→BGRA swap | as in `img.rs:664-667` and the wgpu-graft demo | 0.78–0.80 |
| Atlas staging copy (`to_vec` / swizzle into a new `Vec`) | `wgpu_atlas.rs:250-258` | 2.92 |
| **CPU total** | | **6.98–7.11** |

Not measured: `glReadPixels` from the GPU and `queue.write_texture` back to it. At PCIe 4.0 rates each 5.6 MB transfer is on the order of a millisecond, plus a pipeline stall on the read (inferred), putting a changed frame at roughly 8–11 ms. If TD reads back only when Servo reports a new frame, an idle brief costs nothing; polling every animation frame, as the demo does, pays the full cost at display rate. For a document that repaints on scroll, hover and typing, readback is acceptable. The zero-copy path replaces all of it with one GPU-side blit of 5.6 MB, well under a millisecond at GPU memory bandwidth (inferred).

### Size, build time, memory

- Build: clean dev build of `servoshell`, 596.4 s, 1,005 units, 8 jobs on 16 CPUs, rustc 1.95.0 (`target/cargo-timings/cargo-timing.html`). Slowest units: `servo-script` 237.9 s, `mozangle` build script 149.5 s, `tikv-jemalloc-sys` build script 136.2 s, `servo-layout` 132.6 s, `servo-script-bindings` 110.9 s. TD builds dependencies at `opt-level = 2` even in dev (`app/Cargo.toml:40-41`), so Servo inside TD's dev build would be slower than servoshell's (inferred). TD's clean build time was not measured. (measured)
- Dependencies: Servo's lockfile has 1,097 packages, TD's 725. Both pin Rust 1.95.0 (`rust-toolchain.toml` in both trees). `ash 0.38` is shared. Servo's `wgpu-core` 29.0.3 comes from crates.io and only with the optional `webgpu` feature (`components/servo/Cargo.toml:68`); Zed's is a git fork (`zed-upstream/Cargo.toml:829`). (measured)
- Binary: debug `servoshell` is 2,005,831,680 bytes; `target/` is 19 GB; Servo's bot reports 229.96 MB for a release build; TD's release binary is 49,488,928 bytes. (measured)
- Memory: 378 MB peak resident rendering one brief headless in the debug build. Per-view cost inside TD, and the cost of a second view, were not measured. (measured for the one case)

### The fork against upstream

The fork's branch `t3/native-read` sits on upstream commit `495c38d694d` (2026-06-17), with four commits of Parker's on top ending in `c5cbd93` (2026-08-18). Upstream `main` is `7256c05b108` (2026-09-24 14:54 UTC), 1,537 commits ahead. The fork's workspace version is 0.3.0 (`Cargo.toml:15`); crates.io has `servo` 0.5.0 (2026-08-17) and a long-term-support line at 0.1.x (https://crates.io/api/v1/crates/servo, https://book.servo.org/embedding/lts-release.html). The fork's own `CLAUDE.md` asks that its diff against upstream stay minimal. (measured)

### Is it feasible?

Yes, with readback, and the notes system mostly works. What the probe showed (`servo_notes_probe.py`, a copy of `2026-09-09-client-server-review.html`, headless, via servo-agent's `ServoBrowser`):

| Check | Result |
|---|---|
| Page load to settled | 1.7 s |
| Note affordances tagged by notes.js | 38 |
| `HTMLDialogElement.prototype.showModal` | present |
| Note dialog after clicking an affordance | open, and on top at its centre |
| Add a note | note count 1 |
| `location.origin` | `"null"` |
| `localStorage` on `file://` | `SecurityError: Cannot access localStorage from opaque origin.` |
| "save into file" | page navigated to `blob:null/25ea4baf-…`; no file written |

The probe ran against servo-agent's working tree, which carries uncommitted edits to `browser.py` dated 2026-09-23 that are not from this session. The clicks were dispatched from JavaScript because servo-agent's `click_selector` failed with "Failed to decode request as JSON" from Servo's WebDriver, so trusted-click behaviour is still untested. (measured)

The two failures are host-fixable, and fixing them could make annotation better than it is in a browser (inferred):

- Save: TD owns the file, so it can intercept the `blob:` navigation in `request_navigation`, or read the notes with `evaluate_javascript`, and write the brief in place instead of producing a download that someone has to move back.
- Persistence: serve briefs from a TD scheme registered with `protocol_registry` so the page gets a real origin. Whether Servo gives a custom scheme's pages a storage-capable origin was not verified.

### The minimum spike

Behind a cargo feature so default builds are untouched:

1. A `ServoBuilder` whose waker sends on a channel that a foreground task spawned from gpui awaits; each wake calls `spin_event_loop`.
2. `WebViewBuilder::new(&servo, Rc<SoftwareRenderingContext>)` sized to the floating square in device pixels, loading a brief by `file://`.
3. On `notify_new_frame_ready`: `paint`, `read_to_image`, swap, `RenderImage`, `img(ImageSource::Render(…))`, then `cx.drop_image` on the previous frame.
4. Mouse, wheel and key events mapped and forwarded; `unwarp` applied when the pane is warped.
5. Measure time to first frame, idle CPU, CPU while scrolling, resident memory, and whether atlas memory stays flat across 1,000 scroll frames.

It passes if a note can be added with a real click, idle CPU is near zero, and memory is stable. The second step swaps the software context for a hardware offscreen GL context and measures again; zero-copy comes only if those numbers are bad.

### The hardest unknown

Whether a current Servo lays out and runs Parker's briefs well enough to be worth embedding. The event loop and the frame transport are engineering with a working demo beside them; page fidelity is outside TD's control, and the one brief tested already shows a collapsed grid. It is also the cheapest unknown to settle, before any TD code: rebuild Servo at upstream `main`, then screenshot the 118 HTML reports under `~/Work/reports` and `~/Work/terminal-delight/reports` (91 of them carry notes.js) in both Servo and headless Chromium at the same size, and compare. If a large share of the briefs break, the in-pane engine should be headless Chromium over the DevTools protocol, or nothing (hunch).

---

## 4. Theming

Native rendering takes TD's look completely. A Markdown view built from TD's `Theme` uses the same five tokens the terminal does, and the CRT pass warps it because it warps everything in the pane's rectangle. Images take the CRT treatment too; their colours are their own, which is correct.

HTML is different. The CRT pass still applies — Servo pixels in a pane get the tube like anything else — but the colours come from the page. For briefs, the page's own colours are part of what it says:

- The brief palette carries meaning. `base.css:15-40` defines neutral tokens (`--surface-0` to `--surface-3`, `--line`, `--text-primary` and friends) alongside categorical `--s1`…`--s8` and status `--good`, `--warning`, `--serious`, `--critical`, and the brief doctrine binds red, orange, green, blue, amber and grey to decisions, failures, readiness, structure, arguable items and unknowns. Recolouring those to a terminal theme would change what the pictures say.
- The neutral tokens can follow TD. Because briefs route their chrome colours through `:root` custom properties, a user stylesheet injected with `UserContentManager::add_stylesheet` could set the neutral tokens from TD's `bg`, `surface`, `text` and `faint` and leave the semantic ones alone (inferred; not tried). Briefs declare `color-scheme: dark` and the tested brief never uses `prefers-color-scheme`, so `notify_theme_change` alone would do nothing.
- The static snapshot route cannot inject CSS through a command-line screenshot (inferred), so there the brief keeps its own palette under TD's tube.

The implication is two visual regimes in one window: native documents that look like TD, and HTML that looks like itself on TD's glass. The better default is to let a brief be a page on the glass, with an opt-in that maps only the neutral tokens. Dense 12–13 px brief text under a strong barrel warp may also be hard to read (hunch), so the document view may want its own, flatter warp setting.

---

## 5. The ladder

**First rung: images and Markdown, native, in a floating square.** Both render with code TD already runs — `gpui::img()` and a lifted 482-line comrak renderer on the same gpui pin — both take TD's palette and CRT, and both are S. The floating square sits inside a `TerminalView`, so the split tree keeps its single leaf type; a split can follow once there is a document view worth tiling. This rung also forces the decisions every later rung needs: what a document view is in TD, how Ctrl+click chooses between the view and `xdg-open`, and the explicit reversal of `surface.rs:386-389`.

**Second rung: HTML briefs and PDFs as rendered pages, out of process.** Headless Chromium turns a brief into a correct 1000×1400 image in about a second, and `pdftoppm` turns a PDF page into one in about a tenth of a second; the first rung's image view shows them, with an "open live" action that hands the file to Chromium or Evince. Each is S, adds nothing to TD's build, and makes briefs readable in the pane at full fidelity. It does not make them annotatable, and should say so on the view. hayro, or a lift of gpui-pdf, is the upgrade if PDFs need text search.

**Third rung: a live Servo pane for annotating briefs, behind two gates.** Among engines that run notes.js in-process, it is the one already shown inside a gpui window (the wgpu-graft demo) and already built on this machine, and with TD writing the file itself it could beat the browser's download-and-move loop. It costs a Servo dependency of about 230 MB of binary and ten-plus minutes of clean build, so it starts with the fidelity comparison on upstream Servo and then the readback spike, and ships only if both pass. Zero-copy frame transport stays a later, separate patch, taken only if the spike's CPU numbers demand it. If the fidelity comparison fails, the alternative to price is headless Chromium over the DevTools protocol, which keeps full fidelity outside TD's process.
