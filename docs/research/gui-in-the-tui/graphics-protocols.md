# Terminal graphics protocols

Research date 2026-09-24. Scope: what Terminal Delight (TD) itself must implement so that programs running *inside* a TD pane — `mpv --vo=kitty`, yazi, chafa, timg, presenterm, `kitten icat`, ratatui-image apps — can put real pixels on screen. TD is the terminal here; nothing in this document is about TD probing some outer terminal.

Confidence labels: **measured** (read in code or docs, cited), **inferred** (follows from code that was read, not run), **hunch** (judgement, no direct evidence). "Not found" means searched for and absent.

Source checkouts, with the commit date each citation refers to:

| Tree | Where | Commit / date |
|---|---|---|
| TD | `/home/parker/Work/td-gui-in-the-tui` (paths below are `app/src/…`) | `de7776b`, 2026-09-24 |
| vte | `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/vte-0.15.0` | crates.io 0.15.0 |
| alacritty_terminal | `…/alacritty_terminal-0.26.0` | crates.io 0.26.0 |
| ghostty | `/home/parker/Projects/td-competitor-research/ghostty` | `5d0a82ba`, 2026-06-19 |
| kitty | `…/td-competitor-research/kitty` | `42d3cb21`, 2026-06-19 |
| wezterm | `…/td-competitor-research/wezterm` | `c4b1d89f`, 2026-06-20 |
| alacritty | `…/td-competitor-research/alacritty` | `985394dd`, 2026-06-16 |
| zed | `/home/parker/Work/zed-upstream` (shallow clone) | `abbe85a3`, 2026-06-12 |
| mpv | `/usr/bin/mpv` 0.41.0 + man page; source read on GitHub | `2a4eb806`, 2026-09-23 |
| xterm ctlseqs | https://invisible-island.net/xterm/ctlseqs/ctlseqs.txt | Patch #411, 2026-08-23 |

---

## Summary — the facts that decide feasibility

1. **Nothing a graphics client sends reaches TD's emulator today.** vte 0.15 routes `ESC _` (APC), `ESC ^` (PM) and `ESC X` (SOS) into one state whose bytes go to `anywhere()`, which only watches for CAN, SUB and ESC; every other byte is discarded and no `Perform` callback exists for APC (`vte/src/lib.rs:182,359,377,438-450`, trait at `:761-826`). DCS *is* delivered to `Perform::hook/put/unhook` (`lib.rs:317-337`), but alacritty's own performer only logs and drops it (`vte/src/ansi.rs:1311-1326`). So Kitty graphics (APC) and Sixel (DCS `q`) both vanish. (measured)

2. **So every client's detection fails, quietly.** Clients detect Kitty by sending `a=q` followed by DA1 and treating "DA1 answered, query not" as unsupported (kitty `docs/graphics-protocol.rst:436-448`). TD answers DA1 with `ESC[?6c` (alacritty `src/term/mod.rs:1261`), which does not advertise Sixel (attribute 4). Every client above falls back to text-cell art. (measured)

3. **The size and identity answers clients rely on are partly wrong or missing today.** `CSI 16 t` (cell size in pixels) is never dispatched — vte matches only 14, 18, 22, 23 (`vte/src/ansi.rs:1739-1745`) — and ratatui-image, yazi and chafa all send it (§4). XTVERSION (`CSI > q`) has no arm either (`ansi.rs` has no `('q', [b'>'])`), and yazi and timg identify the terminal by it. `CSI 14 t` is answered with the geometry the pane was *spawned* with in host-owned panes (`host.rs:705-711` uses a `Copy` of `geom` captured by the event thread; `resize` at `host.rs:809-814` updates a different copy) and not at all in window-owned panes (`pane.rs:3909-3972` has no `TextAreaSizeRequest` arm). `TIOCGWINSZ` pixel fields are set to `cols × cell_width` and `rows × cell_height` (alacritty `src/tty/unix.rs:433-434`), and that is the one mpv reads (mpv `osdep/terminal-unix.c:579-589`). (measured code; the stale-geometry reply is inferred, not reproduced)

4. **alacritty's grid already stores Kitty Unicode placeholder cells losslessly, and TD's attach snapshot already carries them.** A placeholder is U+10EEEE with the image id in the foreground colour, the placement id in the underline colour (SGR 58) and row/column/high-byte as combining diacritics (kitty `graphics-protocol.rst:594-653`). alacritty appends zero-width characters to the previous cell without a cap (`src/term/mod.rs:1069-1083`, `src/term/cell.rs:164-167`) and stores underline colour independently of the underline flag (`term/mod.rs:1887`, `cell.rs:180-193`). TD's `gridwire` re-emits combining marks and `58;…` in snapshots and includes both in `grid_hash` (`gridwire.rs:152,191-194,307,459-465`). (measured code; no end-to-end test exists yet)

5. **So the Kitty placeholder path needs no fork.** Transmitting an image (`a=t`), creating a virtual placement (`a=p,U=1`), deleting by id and answering `a=q` do not depend on the cursor. A scanner that sees the raw byte stream can do all four, and TD already has one seat for it: the host's `TeeReader` copies every PTY read before alacritty parses it (`host.rs:129-163`), and the window replica parses the same bytes (`socketpty.rs:1-21`). The only missing pieces are the image store and a renderer that draws U+10EEEE cells. It covers ratatui-image, which only ever uses placeholders, `kitten icat --unicode-placeholder`, and yazi once yazi recognises TD (checklist item 8). (inferred)

6. **Direct placement, Sixel and iTerm2 images need the cursor position at the instant the sequence arrives, and a scanner in the reader cannot know it.** alacritty's `pty_read` may concatenate several reads before one `advance` (`src/event_loop.rs:120-163`), holds the `Term` lock across the loop, and vte buffers up to 2 MiB / 150 ms of a synchronized update before applying any of it (`vte/src/ansi.rs:36-39,298-312,370-387`). TD must either own the read loop and split the stream itself, or patch vte + alacritty_terminal. (measured code; conclusion inferred)

7. **Most clients use direct placement, so placeholders alone cover little of the list.** ratatui-image always sends `a=T,U=1` and prints U+10EEEE. yazi uses placeholders only for terminals it recognises (kitty, Ghostty, Rio); an unrecognised one gets `KgpOld` — a direct `a=p,c=1,r=1` per cell — or Sixel if DA1 contains 4. mpv sends `CUP top;left` then `ESC_Ga=T,f=24,s=W,v=H,C=1,q=2,m=1;…` every frame with no image id and no per-frame delete (`video/out/vo_kitty.c:80,340-395`; `a=d` only at reconfigure and exit, `:225,474`). timg, chafa, presenterm and icat all place directly unless they are inside tmux. Details and citations in §1.10. (measured)

8. **Upstream will not supply this, and the closest sibling forked.** vte's APC pull requests were closed ("I'm very hesitant to accept this sort of 'second rate escape sequence'", https://github.com/alacritty/vte/pull/115); alacritty's own graphics PR has been open since 2021 and "has little chance of ever getting upstreamed" (https://github.com/alacritty/alacritty/pull/4763#issuecomment-3236877718). Rio, a Rust + wgpu terminal descended from alacritty, supports Sixel, iTerm2 and Kitty graphics including placeholders by forking vte into its own parser with `apc_start/apc_put/apc_end` callbacks and anchoring placements to an absolute row counter (§6.4). (measured)

9. **The reference emulators store placements four different ways.** Ghostty anchors each placement to a tracked pin in its page list, so reflow and scrollback move it (MIT). kitty keeps integer `start_row`/`start_column` and shifts them on scroll, with no reflow when columns change (GPL-3.0 — read, never copy). WezTerm writes an image slice into every covered cell's attributes, so images tear on reflow like text (MIT). Rio keeps placements in side tables keyed by an absolute row, `lines_evicted + history + screen_row` (MIT). Details in §5 and §6.4. (measured)

10. **TD's host/window split adds three constraints the single-process emulators above do not face.** Replies must be written exactly once, by the host (`host.rs:697-713`; the replica swallows `PtyWrite`, `term.rs:94`). File, temp-file and shared-memory transmissions are consumed — and deleted — by whichever process reads them first, so the window cannot re-read what the host already unlinked. And the host drops a client whose queue exceeds 512 chunks (`host.rs:57-64`), while one base64 1280×720 RGB frame is about 3.7 MB. (measured constants; the interaction is inferred)

11. **Reusable, licence-compatible code exists for most of the parsing.** WezTerm's `KittyImage::parse_apc` (MIT, standalone, `wezterm-escape-parser/src/apc.rs:1061`), Rio's `rio-vt` Kitty, placeholder and Sixel modules (MIT; its sixel decoder credits the Apache-2.0 alacritty PR), `icy_sixel` 0.7 (MIT OR Apache-2.0, incremental DCS decoder), the `image` crate, and Ghostty's diacritic table (MIT). kitty's code is GPL-3.0 (`LICENSE:1-2`) and cannot be copied into TD.

12. **gpui can draw it.** `Window::paint_image(bounds, corner_radii, Arc<RenderImage>, frame_index, grayscale)` uploads into the sprite atlas (`zed-upstream/crates/gpui/src/window.rs:4084-4127`); it has no source-rectangle parameter, so cropping is done by clipping with a content mask, and video needs `drop_image` (`:4149-4157`) for every retired frame. The CRT warp is a whole-frame post-pass in TD's patched `gpui_wgpu` (`warp.rs:24,150`), so images painted inside the pane bend with the glass for free. (measured API; warp behaviour inferred)

---

## 1. Kitty graphics protocol — implementer's essentials

All citations are to `kitty/docs/graphics-protocol.rst`, the source of https://sw.kovidgoyal.net/kitty/graphics-protocol/ (the live page was fetched 2026-09-24 and matches on detection and placeholders). (measured)

### 1.1 Framing and chunking
- `ESC _ G <control data> ; <payload> ESC \` (`:245-247`). Control data is comma-separated `key=value`; the payload is base64 (`:252-255`).
- Chunks: base64 first, then split into chunks of at most 4096 bytes, each but the last a multiple of 4; `m=1` on all but the last, `m=0` on the last (`:386-396`). Later chunks carry only `m` and optionally `q` (`:398-405`). Nothing displays until the final chunk; **the cursor position used is the one when the final chunk arrives** (`:398-405`).
- All integers are 32-bit (`:1043`). Keys that take a single character: `a`, `d`, `t`, `o`; signed: `z`, `H`, `V`; everything else unsigned (kitty `gen/apc_parsers.py:288-318`).

### 1.2 Transmission media (`t=`)
| `t` | Meaning | Implementer's obligation |
|---|---|---|
| `d` | data in the escape (default) | base64-decode, reassemble chunks |
| `f` | path to a regular file | refuse non-regular files; follow symlinks, fail on loops; may refuse `/proc`, `/sys`, `/dev`; honour `S` size and `O` offset (`:324-377`) |
| `t` | temporary file | as `f`, then delete it — but only if it is in a known temp directory (`/tmp`, `/dev/shm`, `$TMPDIR`) **and** the path contains `tty-graphics-protocol` (`:324-353`) |
| `s` | POSIX shared memory name | `shm_open`, read, then unlink and close (`:324-353`) |

### 1.3 Formats and identifiers
- `f=32` RGBA (default), `f=24` RGB, `f=100` PNG (`:271-275`). `s`/`v` (width/height) are mandatory for raw formats (`:280-289`). `o=z` is RFC 1950 zlib applied before base64 (`:307-318`); PNG plus compression needs `S` (`:299-301`).
- `i` image id (1–4294967295), `I` image *number* — non-unique, the terminal allocates an id and replies with both (`:808-831`; `i` and `I` together is `EINVAL`). `p` placement id; `(i, p)` names a placement and a repeat replaces it (`:478-495`). Retransmitting an id deletes the old image and all its placements (`:498-503`).

### 1.4 Placement
- Actions: `t` transmit (default), `T` transmit and display, `q` query, `p` put, `d` delete, `f` frame, `a` animate, `c` compose (`:1048-1052`).
- The image's top-left is the cursor cell's top-left, offset by `X,Y` pixels (smaller than a cell); `x,y,w,h` select a source rectangle; `c,r` scale to that many cells, the missing one derived from aspect ratio (`:513-530`).
- **z-index** (`:532-540`): negative draws under text; below `INT32_MIN/2` (−1,073,741,824) draws under cells with non-default backgrounds; equal z breaks ties by lower id first.
- **Cursor after placement**: the spec says right by `c` columns and down by `r` rows (`:542-546`); kitty's code moves `x += cols; y += rows − 1` (`kitty/graphics.c:1150-1154`), so the cursor ends on the image's last row. `C=1` means do not move (`:547-549`). Virtual (`U=1`) and relative placements never move the cursor (`:738-742`, `graphics.c:1151`).

### 1.5 Scrolling, clear and alternate screen (`:1153-1167`)
- Images scroll with text; with margins set, only images wholly inside the margins scroll, and are clipped.
- Reset clears visible images; entering the 1049 alternate screen clears the alternate screen's images; `ESC[2J` "should also clear all images"; **other erase commands must not affect graphics**.

### 1.6 Deletion (`a=d`, `:748-790`)
Lowercase deletes placements; uppercase also frees image data if nothing else (e.g. scrollback) references it. Values: `a/A` all visible, `i/I` by id (optionally `p`), `n/N` newest with number `I`, `c/C` at cursor, `f/F` animation frames, `p/P` intersecting cell `x,y`, `q/Q` at `x,y` with z, `r/R` id range `x..=y`, `x/X` column, `y/Y` row, `z/Z` z-index. Coordinates are 1-based. Any delete aborts a partial chunked upload. Virtual placements are deletable only by `i I r R n N` (`:669-673`).

### 1.7 Animation (`:880-1017`)
`a=f` sends a frame (sub-rectangle `x,y,s,v`; base canvas colour `Y` or copy-from frame `c`; edit-in-place `r`; `X=1` replace instead of blend; gap `z` ms, default 40). `a=a` controls playback (`s=1` stop, `2` loading, `3` loop; `v` loop count; `c` current frame). `a=c` composes between frames — **the prose and the table in the spec contradict each other**; the example and kitty's code agree on `r` = source frame, `c` = destination, `X,Y` = source offset, `x,y` = destination offset (`:994-1017`, `:1115-1122`, `graphics.c:1841-1854`). Ghostty's June 2026 checkout still had no animation, and Rio still answers `a=f/a/c` with an error so clients fall back (§5.1, §6.4); it is safe to defer.

### 1.8 Query and detection handshake
- Reply format: `ESC _G i=<id>;OK ESC \` or `…;ECODE:message` (`:421`, `:464-491`; `,p=<pid>` added when a placement id was given). `q=1` suppresses OK, `q=2` suppresses errors too (`:796-799`).
- `a=q` loads the image without storing it (`:428-433`). The recommended detection, verbatim (`:436-448`):
  > send the above *query action* followed by a request for the primary device attributes. If you get back an answer for the device attributes without getting back an answer for the *query action* the terminal emulator does not support the graphics protocol. This means that terminal emulators that support the graphics protocol, **must** reply to *query actions* immediately without processing other input.

  Example: `<ESC>_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA<ESC>\<ESC>[c`
- **A query without `i` gets no reply** in kitty (`graphics.c:2236`) and in Ghostty (`graphics_exec.zig:107-109`). `kitten icat --detect-support` sends three queries — direct, temp file and shared memory — then `ESC[c`, and stops at the DA1 reply (`kittens/icat/detect.go:55-85,90-110`), so TD has to really read (and unlink) the temp file and shm object named in a *query*. (measured)

### 1.9 Unicode placeholders and virtual placements
How it works (`:555-742`):
1. The client transmits the image (often `q=2`) and creates a **virtual placement**: `ESC_Ga=p,U=1,i=<id>,c=<cols>,r=<rows>ESC\` (can be combined with `a=T`) (`:580-589`). Nothing is drawn yet and the cursor does not move.
2. The client then *prints text*: the character U+10EEEE in every cell the image should cover. The **foreground colour** carries the image id — 8-bit ids in 256-colour mode, 24-bit ids in true colour (`:617-618`). The **underline colour** (SGR 58) carries the placement id; 0 means "any virtual placement of this image" (`:630-633`).
3. Combining **diacritics** after each U+10EEEE give its row, then column, then (optionally) the most significant byte of a 32-bit image id — e.g. 33554474 = 42 + (2 << 24) with U+030E = 2 (`:594-628`). The table is `gen/rowcolumn-diacritics.txt`, 297 Unicode 6.0 class-230 marks starting U+0305, U+030D, U+030E, U+0310 (`:14-310`).
4. **Inheritance**, left to right (`:635-653`): a cell with no diacritics and the same colours as its left neighbour takes that neighbour's row, column + 1 and high byte; a cell with only a row diacritic that matches inherits column + 1 and high byte; row and column present with column = left + 1 inherits the high byte.
5. The terminal draws the slice `(row, col)` of the image, fitted into the virtual placement's `c×r` box with aspect ratio kept and centred (kitty `graphics.c:892-1033`; Ghostty `graphics_unicode.zig:156-241`). Background colour shows through transparency; images drawn this way "are not considered placements" — to move or delete them, rewrite the text (`:675-678`).

For an emulator the attraction is that once the cells exist, *everything else is text*. Scrolling, scrollback, insert/delete line, erase, the alternate screen and a multiplexer's own redraw all move the image because they move the characters.

Implementation details worth copying from Ghostty rather than kitty: kitty's diacritic decoder accepts extra codepoints because its generator passes an exclusive range end into an inclusive `case` (`gen/wcwidth.py:423-431` → `kitty/rowcolumn-diacritics.c:8-13`), while Ghostty binary-searches a sorted 297-entry table (`graphics_unicode.zig:555-570`). Ghostty only applies inheritance within one contiguous run on one row (`graphics_unicode.zig:84-87,506-513`). Both draw placeholder images at z = −1 so the cursor shows on top (`kitty/graphics.c:1023`, Ghostty `renderer/image.zig:511`). (measured)

### 1.10 How each client detects and places

Web sources were read at pinned commits: ratatui-image `5cec7871`, yazi `0ea4c5d9`, chafa `39e548cd`, timg `a4014844`, presenterm `5f8add11`, mpv `2a4eb806`, all on GitHub (paths below are relative to each repo). kitty's `kittens/` are from the local checkout.

| Client | How it detects | How it places | Works on placeholders-only (route A)? |
|---|---|---|---|
| ratatui-image 12.0.0-rc.0 (MIT) | one write: Kitty `a=q` `i=31` (plus optional `o=z` and `t=s` probes), `ESC[c`, `ESC[16t`, optional `OSC 11 ?`, sentinel `ESC[5n`; 2 s timeout (`src/picker/cap_parser.rs:133-214`, `picker.rs:57`). `_Gi=31;OK` → Kitty; DA1 `4` → Sixel; Kitty wins. iTerm2 only from env (`picker.rs:382-400`); `[1337n` is commented out (`cap_parser.rs:184`) | **always placeholders**: `a=T,U=1` then U+10EEEE (`src/protocol/kitty.rs:31,375,436`). Blacklists Kitty under WezTerm and Konsole because "Neither implement the placeholder part of kitty correctly" (`picker.rs:129-136`) | **yes** |
| kitten icat | three `a=q` queries — direct, temp file, shm — then `ESC[c` (`kittens/icat/detect.go:52-110`) | direct by default; `--unicode-placeholder` opt-in, implied by `--passthrough` (`kittens/icat/main.py:145-164`) | only with the flag |
| yazi | XTVERSION `CSI > q`, Kitty `a=q` (direct and shm), DA1, `CSI 16 t`; waits 3 s for DA1 (`yazi-emulator/src/probe.rs:90,114-129`); brand from the XTVERSION reply or env (`brand.rs:33-94`) | kitty, Ghostty, Rio → `Kgp` = `a=T,C=1,U=1` + U+10EEEE with 24-bit RGB id (`yazi-adapter/src/drivers/kgp.rs:365-428`). **Unknown brand** → `[Sixel, KgpOld]` if both detected, `[KgpOld]` if only Kitty; `KgpOld` = `a=t` then one direct `a=p,…,c=1,r=1,C=1` per cell (`kgp_old.rs:66-129`; `drivers.rs:21-100`) | no, unless TD is a recognised brand |
| mpv `--vo=kitty` | none; `TIOCGWINSZ` only | direct `a=T,C=1` every frame (summary item 7) | no |
| chafa | fg/bg, `CSI 14 t`, `CSI 16 t`, DA1 (`chafa-term.c:935-944`); Kitty chosen from env only (`xterm-kitty`, `KITTY_PID`, `xterm-ghostty`, `TERM_PROGRAM=ghostty`, `GHOSTTY_BIN_DIR`; `chafa-term-db.c:656-690`) | direct `a=T`; `U=1` only when passthrough is on (`chafa/internal/chafa-kitty-renderer.c:455-475`) | no |
| timg | env, then XTVERSION + `CSI 5n` in 250 ms, then DA1 `;4`; deliberately no Kitty `a=q` (`src/term-query.cc:226-345`) | direct `a=T,f=100` PNG; `U=1` only in tmux (`src/kitty-canvas.cc:192-227`) | no |
| presenterm | Kitty `a=q` with `t=f` (i=42) and direct (i=43), then DA1 (`src/terminal/capabilities.rs:37-128`) | direct; `U=1` only in tmux (`src/terminal/image/protocols/kitty.rs:180-205`) | no |

(measured; the ratatui-image placeholder lines and the yazi driver table were re-read directly)

For TD, placeholders alone reach ratatui-image apps and one icat flag, and yazi's placeholder path depends on being *recognised*: it keys on the XTVERSION reply, which TD cannot send today because vte has no arm for `CSI > q`. Spoofing another terminal's `TERM_PROGRAM` is not a fix — chafa would then pick Kitty from env and place directly. The clean fix is a TD answer to XTVERSION plus a yazi brand entry upstream.

### 1.11 Storage quota
"terminal emulators should have a maximum storage quota for image data … at least a few full screen images. For example the quota in kitty is 320MB per buffer" — evict older images, images without placements first (`:1030-1036`, `:777-779`). kitty: `DEFAULT_STORAGE_LIMIT 320u * (1024u * 1024u)` per screen (`graphics.c:27,80`), no user option. Ghostty: `image-storage-limit`, default 320 MB per screen, 0 disables all image protocols (`src/config/Config.zig:2391-2399`). WezTerm: hard-coded `320 * 1024 * 1024` with a FIXME (`term/src/terminalstate/kitty.rs:47`). (measured)

---

## 2. Sixel — essentials

- **Framing**: `DCS Pa ; Pb ; Ph q <sixel data> ST` — `Pa` pixel aspect ratio, `Pb` background option, `Ph` horizontal grid size (ignored) (ctlseqs.txt lines 3230-3240). `Pb` = 0 or 2 paints 0-bits with the background colour, 1 leaves them unchanged — i.e. transparent (vt100.net VT330/340 manual ch. 14, https://vt100.net/docs/vt3xx-gp/chapter14.html).
- **Data**: each character `?`..`~` is six vertical pixels, value − 0x3F, least significant bit on top; `!Pn c` repeats; `"Pan;Pad;Ph;Pv` raster attributes (aspect + image size in pixels); `#Pc;Pu;Px;Py;Pz` defines colour register `Pc` in HLS (`Pu=1`) or RGB percent 0–100 (`Pu=2`), `#Pc` alone selects it; `$` returns to the start of the current sixel row, `-` moves to the next six-pixel row (same vt100.net chapter). (measured via fetch)
- **Palette**: registers are shared across images unless private mode **1070** ("use private color registers for each graphic") is set (ctlseqs.txt 3224-3225). WezTerm defaults it off and seeds the 16 VT340 colours (`term/src/terminalstate/mod.rs:403-427,536`). `XTSMGRAPHICS` reports register count and maximum geometry: `CSI ? Pi ; Pa ; Pv S`, `Pi` 1 = colour registers, 2 = sixel geometry; `Pa` 1 read, 2 reset, 3 set, 4 read max; reply `CSI ? Pi ; Ps ; Pv S` (ctlseqs.txt 671-700). (measured)
- **Scrolling and cursor**: DECSDM is private mode 80 (ctlseqs.txt 967, 1113-1115). With sixel scrolling on (the usual state), the image starts at the cursor, scrolls the screen as it passes the bottom margin, and on exit the text cursor is at the sixel cursor position; with it off, the image is drawn at the top-left and the cursor does not move (vt100.net chapter 14). **DEC's own manuals disagree on the polarity of mode 80** (ctlseqs.txt 3468-3495), and emulators differ on whether the cursor ends on the image's last row or the row after — WezTerm leaves it on the last row at the starting column (`term/src/terminalstate/image.rs:220-246`, inferred from control flow). Mode **8452** leaves the cursor to the right of the image instead (ctlseqs.txt 3226). mpv avoids the question by never using the last row (man page: "mpv doesn't use the last row with sixel because this seems to result in scrolling").
- **Detection**: DA1 attribute **4** ("Sixel graphics", ctlseqs.txt 3215-3218) and XTSMGRAPHICS. WezTerm answers `ESC[?65;4;6;18;22;52c` (`term/src/terminalstate/mod.rs:1287-1294`); Ghostty's DA1 is `ESC[?62;22;52c`, no 4 (`src/termio/stream_handler.zig:836-844`); TD's is `ESC[?6c`. (measured)
- **Who supports it**: WezTerm (yes); foot (yes — limits `SIXEL_MAX_COLORS 1024u`, `SIXEL_MAX_WIDTH/HEIGHT 10000u` in `sixel.h:5-7`, DA1 `;4` when `tweak.sixel` is on in `csi.c:876-884`, XTSMGRAPHICS in `csi.c:1509-1537`, modes 80/1070/8452 in `csi.c:648,666,675`, at codeberg `dnkl/foot@9f5c70ea`); Contour (yes, README "✅ Sixel inline images", `README.md:38` at `2ec3c63c`); Rio (yes, DA1 `ESC[?62;4;6;22;52c`, `rio-vt/src/crosswords/mod.rs:3941` at `37624389`); Ghostty (no — only the DA1 enum value exists, `src/terminal/device_attributes.zig:53`; "Ghostty will not support sixels", https://github.com/ghostty-org/ghostty/discussions/2496); kitty (no — no sixel in `kitty/*.c|h|py`). (measured)

---

## 3. iTerm2 inline images (OSC 1337)

- Form: `ESC ] 1337 ; File = [key=value;…] : <base64 file contents> BEL` (https://iterm2.com/documentation-images.html). Keys: `name` (base64 filename), `size` (bytes, for progress), `width`, `height`, `preserveAspectRatio` (default 1), `inline` (default 0 = download, not display). Width/height take `N` cells, `Npx`, `N%` or `auto`. The payload is a whole file (PNG, JPEG, GIF…), not raw pixels, so the terminal needs a general image decoder.
- Since iTerm2 3.5 there is a multipart form for multiplexers: `MultipartFile=[args]`, then `FilePart=<base64>` chunks, then `FileEnd`, each part at most 1,048,576 bytes (same page).
- WezTerm also accepts `doNotMoveCursor` (`wezterm-escape-parser/src/osc.rs:1024-1045`) and has no `MultipartFile` support (not found). Ghostty parses 1337 but logs `File`, `FilePart`, `FileEnd`, `MultipartFile` and `ReportCellSize` as unimplemented (`src/terminal/osc/parsers/iterm2.zig:157-195`). kitty routes 1337 only to `SetUserVar` (`kitty/window.py:1263-1268`). Rio handles it (`rio-vt/src/performer/handler.rs:1259-1260`, decoder `rio-vt/src/ansi/iterm2_image_protocol.rs`), and Contour added it in 0.7.0 alongside Kitty graphics (`metainfo.xml:273-274` at `2ec3c63c`). (measured)
- In TD today the whole OSC reaches alacritty, because vte's OSC buffer is an unbounded `Vec` in `std` builds (`vte/src/lib.rs:63-64,544-552`), and alacritty's unhandled-OSC path Debug-formats every byte of the payload into a `String` before calling `debug!` (`vte/src/ansi.rs:1332-1342`) — an O(n) cost per image paid even with logging off. (measured code; cost inferred)
- Placement is at the cursor, so it has the same cursor-at-instant requirement as Sixel.

---

## 4. Size reporting — what each client reads

| Query | Answer format | TD today |
|---|---|---|
| `TIOCGWINSZ` `ws_xpixel`/`ws_ypixel` | kernel struct | `cols × cell_width`, `rows × cell_height` from whatever `WindowSize` TD passes (alacritty `tty/unix.rs:431-435`) — correct if TD passes measured cell metrics |
| `CSI 14 t` | `CSI 4 ; height ; width t` (ctlseqs.txt 1672-1673) | host panes: spawn-time geometry (`host.rs:705-711`); window-owned panes: never answered (`pane.rs:3972`) |
| `CSI 16 t` | `CSI 6 ; height ; width t` (1681-1682) | **never answered** — vte does not dispatch 16 (`vte/src/ansi.rs:1739-1745`) |
| `CSI 18 t` | `CSI 8 ; height ; width t` (1683-1684) | answered from the live grid (alacritty `term/mod.rs:2268-2271`) |
| `CSI ? 2 ; 1 S` XTSMGRAPHICS | `CSI ? 2 ; 0 ; w ; h S` | not dispatched — `('S', [])` only matches with no intermediates (`vte/src/ansi.rs:1736`) |

What the reference terminals answer, for comparison: kitty answers 14, 16 and 18 and fills `ws_xpixel/ws_ypixel` (`kitty/screen.c:2822-2851`, `kitty/child-monitor.c:641`); Ghostty answers all three plus mode 2048 in-band resize reports and sets the pixel fields from the screen minus padding (`src/termio/Termio.zig:512-532`, `src/termio/Exec.zig:1117-1133`); WezTerm answers all three with `16t` computed as pixels ÷ cells (`term/src/terminalstate/mod.rs:2025-2054`) and sets the pixel fields to the text area (`pty/src/unix.rs:29-30,184-185`). (measured)

Per client:
- **mpv** (`vo_kitty`, `vo_sixel`): `TIOCGWINSZ` only, requiring all four fields non-zero (`osdep/terminal-unix.c:579-589`), with command-line overrides (§9a). Default fallbacks are 80×25 cells and 320×240 pixels (man page). (measured)
- **ratatui-image**: `CSI 16 t` in its single probe write (`src/picker/cap_parser.rs:181`); with no reply it has to guess the cell size. (measured; fallback behaviour not traced)
- **yazi**: `CSI 16 t` in its probe (`yazi-emulator/src/probe.rs:114-129`). (measured)
- **chafa**: `CSI 14 t` and `CSI 16 t` (`chafa-term.c:935-944`). (measured)
- **kitten icat**: `TIOCGWINSZ` via its Go tty loop (hunch — the call site was not traced).

So of the three `CSI t` queries, the one TD cannot answer at all (16) is the one three of the clients ask.

TD therefore needs to make `CSI 14 t` read the live geometry, answer `CSI 16 t` (a byte scanner can do it — the query is cursor-independent), and keep `TIOCGWINSZ` pixel fields equal to `cols × cell_w` so all three agree. A disagreement between them is how images come out the wrong size.

---

## 5. How ghostty, kitty and wezterm store and render images

### 5.1 Ghostty (MIT) — Kitty only
- **Parser seat**: APC is a real parser state that emits `apc_start`/`apc_put`/`apc_end` per byte (`src/terminal/parse_table.zig:74-76,116-119,143-146`, `Parser.zig:275,307`); `apc.zig` switches to the Kitty parser on a leading `G` (`:58-70`) and drops anything over 65 MiB (`:196-203`). Base64 is decoded in place with SIMD (`kitty/graphics_command.zig:213-238`).
- **Store**: one `ImageStorage` per screen — `images: AutoHashMap(u32, Image)`, `placements: AutoHashMap(PlacementKey, Placement)`, one in-flight chunked load (`kitty/graphics_storage.zig:22-61`; `Screen.zig:73-76`). Placement location is either a tracked `*PageList.Pin` or `virtual` (`graphics_storage.zig:647-653`); `display()` tracks a new pin at the cursor (`graphics_exec.zig:229-236`).
- **Scroll, scrollback, reflow**: the page list updates every tracked pin on every mutation, so placements scroll into history and reflow cell by cell (`PageList.zig:1279-1296,1349-1360`). Pixel size is recomputed at render time from the current cell size, so `c`/`r`-sized placements rescale with font changes (`graphics_storage.zig:693-694`).
- **Clear**: ED 2 deletes every non-virtual placement and frees unused images (`Terminal.zig:2535-2542,2592-2598`); ED 3 leaves Kitty state alone (`:2639`). The alternate screen has its own storage; entering 1049 runs ED 2 on it (`Terminal.zig:3023-3031,3117-3119`).
- **Quota**: `image-storage-limit`, 320 MB per screen; evict unused first, then oldest transmit time (`graphics_storage.zig:515-613`).
- **Rendering**: sorted by z then id; three layers split at `INT32_MIN/2` and 0 — below cell backgrounds, between backgrounds and text, above text (`renderer/image.zig:358-391`; draw order `renderer/generic.zig:1612-1689`). One texture per image, re-uploaded only when `(id, transmit_time)` changes (`renderer/image.zig:524-590,895-900`).
- **Not implemented at this checkout**: animation (`graphics_exec.zig:71-74` — though upstream `ghostty-org/ghostty@7c40388b`, 2026-09-24, now has `src/terminal/kitty/graphics_animation.zig`, not read here), relative placements (`graphics_storage.zig:626-645`), delete `f/F` (`:428-430`), Sixel, iTerm2 images.
- **Bugs not to copy**: `d=r` tests `id >= first or id <= last`, true for every id (`graphics_storage.zig:416`); eviction removes placements without untracking their pins (`:592-597`). (measured)

### 5.2 kitty (GPL-3.0) — Kitty only
- **Parser seat**: `ESC _` enters APC state (`kitty/vt-parser.c:290`); bytes accumulate until ST, capped at `MAX_ESCAPE_CODE_LENGTH` = 256 KiB per escape (`:18-21,427-441`) — which is why clients chunk at 4096. `dispatch_apc` hands `G` commands to `parse_graphics_code` (`:1417-1421,1480-1481`).
- **Store**: `GraphicsManager` per screen (`main_grman`, `alt_grman`, `kitty/screen.c:144-148`) holding images; `ImageRef` placements with `int32_t start_row/start_column` (`kitty/graphics.h:52-73,91-109,137-157`).
- **Scroll**: `grman_scroll_images` adds the scroll delta and drops refs past `-historybuf->ynum` on the main screen, 0 on the alternate (`graphics.c:1958-2013`, `screen.c:344`). **No reflow when columns change** (`grman_resize`, `graphics.c:2190-2202`); font-size changes recompute cell spans (`grman_rescale`, `:2205-2216`).
- **Clear**: ED 2 → `grman_clear(false)`, ED 3 → `grman_clear(true)` (`screen.c:2598-2604`); ED 2 also frees stored images with no placements (`graphics.c:1934`, inferred).
- **Placeholders**: `IMAGE_PLACEHOLDER_CHAR 0x10EEEE` (`kitty/data-types.h:132`) flags the line; at render time runs of placeholder cells on a row become one `grman_put_cell_image` call (`screen.c:3522-3608`). (measured)

### 5.3 WezTerm (MIT) — Kitty, Sixel and iTerm2
- **Parser seat**: its own `vtparse` crate buffers the whole APC body and calls `apc_dispatch(Vec<u8>)` (`vtparse/src/lib.rs:180-183,648-654`), no size cap; DCS streams through `dcs_hook/dcs_put/dcs_unhook` (`:128-150`). The escape parser moved from `termwiz/src/escape/` to the unpublished `wezterm-escape-parser/` crate (`termwiz/src/lib.rs:68`).
- **Store — images live in cells**: `CellAttributes.fat.image: Vec<Box<ImageCell>>`, each holding texture coordinates for that cell's slice, an `Arc<ImageData>`, z-index and Kitty ids (`wezterm-cell/src/lib.rs:65,91-96`, `wezterm-cell/src/image.rs:81-100`). Consequences: images scroll into scrollback and are freed with their lines (inferred); on reflow slices wrap like text and the picture tears (`term/src/screen.rs:100-175`, inferred); printing over a cell drops Sixel/iTerm slices and Kitty slices without `p=` (`wezterm-surface/src/line/vecstorage.rs:23-34`); ED clears cell images but leaves the Kitty placement map stale (`term/src/terminalstate/mod.rs:2107-2138`).
- **Kitty coverage**: media d/f/t/s — `f` with no path restriction (`apc.rs:179-211`); deletes only `i/I` and `a/A` (`kitty.rs:241-272`); `a=f`/`a=c` yes, `a=a` rejected; **no Unicode placeholders** (no hit for 10EEEE, `U`); gated by `enable_kitty_graphics` (`kitty.rs:176`).
- **Sixel**: decoded straight to `RgbaImage`; cap 100 MB of RGBA (`term/src/terminalstate/image.rs:273-290`); DECSDM honoured (`mod.rs:1719-1730`, `sixel.rs:125-154`); 65536 registers reported via XTSMGRAPHICS (`mod.rs:1339-1379`). (measured)

### 5.4 Where graphics parsing sits relative to the VT parser
All three parse graphics *inside* the VT parser's state machine, in stream order, and act on the grid while the parser holds it: Ghostty via `apc_start/put/end` (§5.1), kitty via its APC buffer and `screen_handle_graphics_command` (`parse-graphics-command.h:395`, `screen.c:1605`), WezTerm via `vtparse::apc_dispatch`. None of them pre-scans the byte stream. That ordering is what gives them the cursor for free — and what TD does not have (§6).

---

## 6. alacritty_terminal: what reaches the Handler, and TD's options

### 6.1 What is delivered
| Sequence | vte low-level `Parser` | vte `ansi::Processor` → `Handler` | Result in TD |
|---|---|---|---|
| APC `ESC _ … ESC \` (Kitty) | state `SosPmApcString`; bytes dropped in `anywhere()` (`lib.rs:182,377,438-450`) | nothing | lost |
| DCS `ESC P … q … ESC \` (Sixel) | `hook/put/unhook` (`lib.rs:280,317-337`) | logged with `debug!`, dropped (`ansi.rs:1311-1326`) | lost |
| OSC 1337 (iTerm2) | buffered, unbounded (`lib.rs:63-64,544-552`) | "unhandled", Debug-formatted (`ansi.rs:1332-1342`) | lost, O(n) cost |
| `CSI 16 t` | dispatched | no arm (`ansi.rs:1739-1745`) | no reply |
| `CSI ? … S` | dispatched | no arm | no reply |

Neither `Perform` nor `Handler` has a method a downstream crate could implement to receive APC. (measured)

Zed, built on the same crate, has not added it either. Its terminal has no image code (no match for sixel, kitty graphics, 1337 or image placement in `crates/terminal` or `crates/terminal_view`), pins its own alacritty fork whose recent commits are about signal masks (`Cargo.toml:506`, `zed-industries/alacritty@fcf32fe`), and closed "Graphics support in the integrated terminal" as not planned with "Zed uses Alacritty and does not modify it in any way, esp. related to graphics" (https://github.com/zed-industries/zed/issues/20860). (measured)

### 6.2 Why a pre-scanner in the reader cannot place images
TD already wraps the PTY reader: `TeeReader::read` copies each chunk to an attached window before returning it to alacritty (`host.rs:129-163`). It is tempting to also pull graphics sequences out there. Three things stop that working for anything cursor-dependent:
1. **Reads are not parses.** `pty_read` reads into a buffer; if `try_lock_unfair` fails it reads *again* and appends (`event_loop.rs:137-146`), then parses everything at once (`:154`). At the moment the reader sees `ESC_G…a=T`, the bytes before it may not be in the grid yet.
2. **The Term is locked by the same thread.** Once acquired, the guard is held across loop iterations (`event_loop.rs:117-118,138-146`); the reader cannot lock it to read the cursor, and `FairMutex` is not re-entrant.
3. **Synchronized updates defer everything.** Inside `CSI ? 2026 h … l`, `Processor::advance` appends to a buffer and applies it only at the end or after 150 ms / 2 MiB (`ansi.rs:36-39,298-312,366-418`). Even a parse-then-act loop sees a stale cursor unless it flushes first.

Injecting synthetic bytes instead (write the placeholder cells as text in the stream) runs into pen and cursor state the scanner does not track: it would have to save and restore the application's SGR and cursor with DECSC/DECRC, which clobbers the application's single saved-cursor slot, and it cannot clamp to the right margin without knowing the column. (inferred)

### 6.3 The three routes
**Route A — byte scanner, placeholders only (no fork).** Scan the stream in the host's `TeeReader` and in the window's `SocketPty` reader; handle only cursor-independent commands: Kitty `a=t`, `a=p,U=1`, `a=d` with `d=i/I/n/N/r/R`, `a=q`, plus `CSI 16 t` and a correct `CSI 14 t`. Leave the bytes in the stream (vte discards APC anyway). Placeholder cells go through alacritty untouched. Covers every client that uses placeholders; covers none that use direct placement. Effort: M. (inferred)

**Route B — TD owns the read loop (no fork).** Replace alacritty's `EventLoop` (486 lines, Apache-2.0, `event_loop.rs`) with a TD loop that scans for graphics sequences, calls `Processor::advance(&mut term, prefix)` for the bytes before each one, then handles the sequence with `&mut Term` in hand: cursor at `term.grid().cursor.point`, cells writable through `grid_mut()` (public, `term/mod.rs:645-653`; `Cell` fields public, `cell.rs:134-140`), cursor moves through the public `Handler for Term` impl (`term/mod.rs:1059`). Before acting, call `Processor::stop_sync(&mut term)` if `sync_bytes_count() > 0` (both public, `ansi.rs:315,362`), or queue the command until the sync ends. Costs: TD's attach fence is written against alacritty's lease discipline (`host.rs:18-29`) and `SocketPty` against its poll tokens (`socketpty.rs:15-21`); both would be re-expressed against TD's own loop. The scanner must agree with vte about string boundaries (CAN/SUB abort, ESC inside OSC) or it will misfire. Effort: L. (inferred)

**Route C — patch vte and alacritty_terminal.** Add an APC collection state and a `Perform` callback to vte, forward APC and DCS through `ansi::Handler`, and implement them on `Term`. Ordering is automatic, synchronized updates replay graphics in order, and placements can be anchored inside the grid. Costs: two forks to rebase against crates.io, against TD's standing choice of an unforked crate (`term.rs:1-2`, `socketpty.rs:6-9`). A published starting point exists for the DCS half: `vte-graphics` 0.15.0 (ayosec, Apache-2.0 OR MIT) adds `dcs_hook/dcs_put/dcs_unhook`, `graphics_attribute` (XTSMGRAPHICS) and `osc_unhandled` to `ansi::Handler` (https://github.com/ayosec/vte-graphics, `src/ansi.rs:731-752`) — but no APC. Effort: L, plus ongoing rebase cost. (inferred)

**Where placements live in routes B and C.** alacritty offers no tracked position that survives scrolling and history eviction — no pin, no monotonic scroll counter, no scroll event. The cheapest anchor in route B is therefore to *write TD-internal placeholder cells* into the grid for a direct placement (U+10EEEE plus TD-private diacritics for image, row and column), so the placement becomes text and inherits scrolling, scrollback, reflow, erase and the snapshot exactly like a client-printed placeholder. What that gives up is §8. Route C can do what Rio did instead and add a counter.

### 6.4 How Rio and the alacritty PR solved it

Both patched the parser; neither pre-scans.

- **Rio** (MIT, `raphamorim/rio@37624389`, 2026-09-17). Its parser file says "Forked from Alacritty's VTE; previously the standalone `copa` crate" (`rio-vt/src/performer/parser/mod.rs:7`). `ESC _` calls `apc_start()`, bytes stream through `apc_put`/`apc_put_slice` and end on BEL, CAN/SUB or ESC (`:466-469,614-638`); `Perform` gains `sos_*`, `pm_*`, `apc_*` and a bulk DCS `put_slice` (`:1452-1560`). The handler buffers APC and runs `process_apc_buffer` (`performer/handler.rs:765,1608-1636`); DCS `q` goes to Sixel (`:1019-1090`). Storage moved *away* from per-cell attributes — "Bit 1 is reserved (was the per-cell graphics flag before sixel/iTerm2 moved to placements)" (`crosswords/square.rs:138-140`). Sixel and iTerm2 images are `AtlasPlacement`s anchored at `abs_row = lines_evicted + history + screen_row`, clip under text and erase, and split when overwritten (`rio-vt/src/ansi/graphics.rs:367-394`); Kitty placements are keyed by `(image_id, placement_id)` with an absolute `dest_row` (`:48-65`); virtual placements store metadata and "the renderer scans visible cells for U+10EEEE" (`crosswords/mod.rs:5788-5797`). Quota `320 * 1024 * 1024` (`graphics.rs:698`), max 4096×4096 (`rio-graphics/src/lib.rs:92`). Rendering goes through a `GraphicOverlay` with a `source_rect` for placeholder slices (`rio-graphics/src/lib.rs:146-162`). No animation (`kitty_graphics_protocol.rs:757-765`). (measured)
- **alacritty PR "Support for graphics in the terminal"** (https://github.com/alacritty/alacritty/pull/4763; open since 2021-02-05; +2882/−100 across 33 files; Sixel only). Swaps vte for `vte-graphics`; adds `alacritty_terminal/src/graphics/`, a `Flags::GRAPHICS` cell flag and `CellExtra.graphics: Option<GraphicsCell>` where every covered cell holds a texture reference and pixel offset, up to 20 per cell (ayosec `alacritty@3d658d2e`: `term/cell.rs:34,133,194`, `graphics/mod.rs:25-28,75-91,555-610`). The ayosec fork ships releases (`v0.17.0-graphics`, 2026-04-06). Maintainer position, chrisduerr: "This PR not being merged is all about image protocols being bad, not about maintainers not having time" (issuecomment-1685121031); "Opting out of performance is not something that aligns with Alacritty's goals" (issuecomment-2136102910); a measured "consistent performance decrease" (issuecomment-774202448). (measured)
- **vte upstream**: latest 0.15.0 (2025-02-02); alacritty_terminal master is `0.26.1-dev` and still pins it with nothing about DCS/APC in its changelog. APC issue open (https://github.com/alacritty/vte/issues/109); PRs #110, #115, #133, #135 closed. (measured)
- **Zed**: nothing (§6.1).

Rio started with the alacritty PR's per-cell model (its Sixel decoder still says it "was retired from a alacritty PR made by ayosec", `rio-vt/src/ansi/sixel.rs:1-3`) and moved to placements with an absolute-row anchor. That anchor needs a counter inside the grid, which is why it arrived together with a fork.

---

## 7. Reusable Rust code

| Code | Licence | What it gives TD | Caveats |
|---|---|---|---|
| `wezterm-escape-parser/src/apc.rs` — `KittyImage::parse_apc(&[u8]) -> Option<Self>` (`:1061`) | MIT (root `LICENSE.md`) | full Kitty key parser and typed commands; no feature gate, `no_std` capable | crate unpublished — vendor the file. Drops `U`, `P`, `Q`, `H`, `V` silently and rejects `a=a` (`:1072`); `t=d` payload stays base64; encoder writes `S` for offset and `a=Q` for transmit-and-display (`:147,1134`) |
| `vtparse` 0.7.0 | MIT, on crates.io | `VTActor::apc_dispatch(Vec<u8>)`, streaming DCS | a whole second VT parser; useful as the *scanner* for route B, not alongside vte |
| `termwiz` 0.23.3 | MIT, on crates.io | older published copy of the parser | pulls terminfo, regex, signal-hook; in-tree 0.24 depends on unpublished crates |
| `icy_sixel` 0.7.0 | MIT OR Apache-2.0 | SIMD sixel decoder with an incremental `SixelDecoder::begin_frame(DcsSettings)` / `feed()` API designed for "after your ANSI parser has recognized the DCS header" (crate README) | new API, verify on real streams |
| `sixel-image` 0.2.1 / `sixel-tokenizer` | MIT | Zellij's decoder + tokenizer | alternative to icy_sixel |
| `image` 0.25.10 | MIT OR Apache-2.0 | PNG for `f=100`; PNG/JPEG/GIF for iTerm2 | enable only the codecs needed |
| `flate2` / `zune-inflate` | MIT OR Apache-2.0 (+Zlib) | `o=z` | cap output size |
| Ghostty `graphics_unicode.zig:571-867` diacritics table | MIT | the 297-entry sorted table | data, transliterate |
| alacritty `event_loop.rs` | Apache-2.0 | starting point for route B's loop | keep the notice |
| Rio `rio-vt/src/ansi/{kitty_graphics_protocol,kitty_virtual,sixel,iterm2_image_protocol,graphics}.rs` | MIT (repo); `sixel.rs` credits the Apache-2.0 alacritty PR (`:1-3`) | a Rust implementation of all three protocols against an alacritty-shaped grid, including placeholder decoding | written against Rio's forked `Perform` and its own grid — port, don't depend |
| Rio's parser / `copa` 0.4.2 | Apache-2.0 OR MIT | vte with APC callbacks | `copa` is now folded into `rio-vt` ("previously the standalone `copa` crate"); a 2023 `copa` 0.12.0 still sorts highest on crates.io |
| `vte-graphics` 0.15.0 | Apache-2.0 OR MIT | vte with DCS and XTSMGRAPHICS on `Handler` | no APC |
| kitty `graphics.c`, `rowcolumn-diacritics.c` | **GPL-3.0** | nothing — read only | cannot be copied into TD |

Licences and versions for crates were read from the crates.io API on 2026-09-24; Rio's from `gh api repos/raphamorim/rio` (`MIT`). (measured)

---

## 8. The Unicode-placeholder angle for TD's architecture

TD's window does not receive grid cells from the host: it receives a snapshot re-encoded as ordinary VT bytes (`gridwire.rs:1-34,206`) followed by the raw live byte stream, which a stock alacritty `Term` in the window parses (`term.rs:280-304`, `socketpty.rs:1-21`). That makes placeholders a particularly good fit.

**What comes for free** (inferred, from the code in fact 4):
- Scrolling, scrollback, insert/delete line, erase, the alternate screen: the cells move, so the image moves.
- Attach and re-attach: the snapshot already re-emits U+10EEEE, the diacritics and SGR 58; `grid_hash` already covers them, so the divergence guard would catch a mismatch.
- Live output: the window sees the same APC bytes the host does, so it can build its own image store from the stream. No side channel for `t=d`.

**What needs a side channel anyway:**
- **The attach snapshot carries cells, not pixels.** An image transmitted before the window attached exists only in the host's store. The snapshot must re-send it (as `ESC_Ga=t,i=…,q=2` APC ahead of the rows, or a TD-private message) for every image id still referenced.
- **`t=f`, `t=t`, `t=s`**: the host reads and unlinks; the window cannot. The host has to forward pixels — for example by rewriting the tee'd command into an equivalent `t=d` before `Sink::send`. `mpv --vo-kitty-use-shm=yes`, ratatui-image's shared-memory path (`src/protocol/kitty.rs:375`) and `kitten icat --transfer-mode=file|memory` all take it.

**What breaks or is lost:**
- **Direct-placement clients, which is most of them** (§1.10): mpv, timg, chafa, presenterm, icat by default, and yazi's `KgpOld` path for any terminal it does not recognise. Route A leaves all of them blank. Route B can convert each placement into TD-internal placeholder cells — at the costs below. yazi's `KgpOld` is the easiest fit, since each of its placements is already one cell (`c=1,r=1`).
- **Text under or over an image.** A placeholder *replaces* the cell's character. Kitty's `z<0` "image under text" and `z>0` "image over text" composites cannot be expressed; a converted direct placement erases what it covers. For mpv, icat and file-manager previews that area is blank anyway (hunch — most previews clear the region first).
- **Reflow.** alacritty rewraps long lines on resize. A placeholder row wider than the new width wraps and interleaves, which kitty documents for its own placeholders ("images … that do not fit on the screen, will get wrapped at the screen edge … interleaved with blank lines", `kittens/icat/main.py:145-153`). Clients redraw on `SIGWINCH`; history does not.
- **Fonts.** TD renders rows as `StyledText` runs (`pane.rs:6339`); U+10EEEE would draw as tofu and U+0305 is COMBINING OVERLINE, which *draws*. `styled_lines` must emit a space for placeholder cells and drop their combining marks. Selection, copy and the FOCUS reader read the same cells, so they need the same filter or a copied image pastes as private-use characters.

**What it costs:**
- Memory: each placeholder cell carries its own `Arc<CellExtra>` with a `Vec<char>` of diacritics and an underline colour (`cell.rs:125-129,164-167`), so no sharing between cells. Roughly 100–150 bytes per cell on top of the 24-byte cell (hunch, from struct layout, not measured) — about 0.5 MB for a 100×40-cell image, more in scrollback.
- Rendering: Ghostty rescans every frame whenever one virtual placement exists (`renderer/image.zig:236-250`). TD should coalesce each row's run of placeholder cells into one clipped draw, as kitty does (`screen.c:3566-3606`).
- Garbage collection: with cells as the only reference, TD cannot cheaply know when an image becomes unreferenced (alacritty does not report overwrites). Use kitty's policy — evict least-recently-used over a byte quota, images not in the viewport first — and draw an "evicted" hatch if a cell references a missing image.

**Video through the split.** mpv at 1280×720, RGB: 2.76 MB raw → about 3.7 MB base64 per frame, about 110 MB/s at 30 fps (arithmetic). The host tees that to the window over a Unix socket and both sides must base64-decode it. `SINK_DEPTH` is 512 *chunks* (`host.rs:64`); if a PTY read returns around 4 KiB (hunch — not measured), the window can fall about 2 MB behind before it is dropped and re-attaches, less than one frame. A window that decodes images on its parse thread will trip that. Either decode off-thread, raise the depth for panes carrying graphics, or have the host forward decoded frames by id and skip superseded ones.

---

## 9. Claims checked

**(a) "mpv has `--vo=kitty` and `--vo=sixel`, including explicit left, top, rows and cols controls."** Confirmed with a correction. `mpv --vo=help` lists `sixel` ("terminal graphics using sixels") and `kitty` ("Kitty terminal graphics protocol"); `mpv --list-options` lists `--vo-kitty-{left,top,rows,cols,width,height,config-clear,alt-screen,use-shm,auto-multiplexer-passthrough}` and `--vo-sixel-{left,top,rows,cols,width,height,pad-x,pad-y,config-clear,alt-screen,buffered,dither,fixedpalette,reqcolors,threshold}` (mpv 0.41.0, run locally). The man page (`/usr/share/man/man1/mpv.1.gz`, roff lines 12479-12650) is the correction: `rows`/`cols` "Specify the terminal size in character cells, otherwise (0) read it from the terminal, or fall back to 80x25" — they describe the *terminal*, not the image. `left`/`top` do position the image ("1 is the first column or row … If 0 (default) then try to automatically determine it"). `width`/`height` are the available size in pixels. (measured)

**(b) "ratatui-image autodetects Kitty/Sixel/iTerm2 and specifically supports Foot via Sixel and Ghostty via Kitty graphics; MIT licensed."** Mostly true, with two corrections (ratatui-image `5cec7871`, 12.0.0-rc.0). Kitty and Sixel are detected by querying the terminal (`src/picker/cap_parser.rs:133-214`); **iTerm2 is chosen from environment variables only** — its query is commented out (`cap_parser.rs:184`, `picker.rs:382-400`). Foot and Ghostty are rows in the README's compatibility table — "Foot | `Sixel` | ✔️ | ✔️ | Wayland." and "Ghostty | `Kitty` | ✔️ | ✔️ | Implements `Kitty` with unicode placeholders." (`README.md:177,180`) — not special-cased code. MIT is correct (`Cargo.toml:12`, `LICENSE`). The detail that matters for TD: its Kitty backend *only* uses Unicode placeholders (`src/protocol/kitty.rs:31,375,436`), so answering its `a=q` with OK before placeholders render would make it draw nothing. (measured)

**(c) "Foot gives us Sixel; Ghostty/Kitty give us the Kitty graphics protocol."** True as stated. foot: Sixel yes, Kitty no — APC bytes are ignored in `state_sos_pm_apc_string_switch` (`vt.c:1025-1035` at `dnkl/foot@9f5c70ea`) and the maintainer wrote "I'm fairly sure I don't want _another_ image protocol in foot" (https://codeberg.org/dnkl/foot/issues/481#issuecomment-358715). Ghostty: Kitty only, no Sixel (§5.1). kitty: Kitty only, no Sixel (§5.2). (measured) The framing does not apply to TD in any case: these are other terminals' capabilities, and a program inside TD gets whatever *TD* implements.

---

## Implementer's checklist for TD

Ordered so each stage ships something a real client can use. S ≈ a day, M ≈ a few days, L ≈ a week or more (hunch estimates). Items 1–8 are route A and need no fork; item 9 is the decision point between routes B and C.

1. **Fix the size answers — S.** Make the host's `TextAreaSizeRequest` reply read `HostPane.geom` (the `Mutex` updated by `resize`) rather than the spawn-time copy (`host.rs:705-711` vs `:809-814`); answer it in window-owned panes too (`pane.rs:3909-3972`). Answer `CSI 16 t` from a byte scanner (vte never dispatches it). Keep `ws_xpixel/ws_ypixel` = cols × cell width. Borrow: nothing needed.
2. **Byte scanner seat — M.** A small state machine (ground / ESC / CSI / OSC / DCS / APC / SOS-PM) in the host's `TeeReader` and the window's `SocketPty` reader that extracts complete APC `G` bodies and a few CSI queries without altering the stream. Borrow: `vtparse` 0.7 (MIT) as the state machine, or hand-roll ~200 lines.
3. **Kitty command parser — S.** Vendor `wezterm-escape-parser/src/apc.rs` (MIT); add `U`, `P`, `Q`, `H`, `V`, `a=a`; fix the encoder bugs if replies are encoded with it.
4. **Image store — M.** Per screen (main and alternate): id → decoded RGBA, image numbers (`I`), chunked-load state, virtual placements, byte quota (start at kitty's 320 MB per screen) with evict-unreferenced-then-oldest. Formats 24/32/100, `o=z`. Media `t=d` first; `t=f/t=t/t=s` with Ghostty's path rules (`graphics_image.zig:125-345`, MIT). Borrow: `image`, `flate2`.
5. **Query replies, exactly once — S.** Host only; enqueue through the same `Notifier` channel DA1 uses so the `a=q` reply precedes the DA1 reply (`host.rs:697-713`). Honour `q=1/2`; no reply without `i`. Test with `kitten icat --detect-support`. **Ship together with item 6, never before it**: the moment `a=q` answers OK, ratatui-image switches to placeholders and draws nothing if the renderer is missing — the failure its README records for Warp.
6. **Placeholder renderer — M.** In `styled_lines` render U+10EEEE as a space and drop its diacritics; in the pane, paint each row's run of placeholder cells with one `paint_image` clipped by a content mask (gpui has no source-rect parameter), using the same cell geometry as `viewport_cell`/`grid_pad`, above cell backgrounds and below the cursor (Ghostty's z = −1 layer; how that maps onto TD's per-row `StyledText` elements was not traced — hunch that it needs the image painted after the row and before the cursor). Decode inheritance per Ghostty (`graphics_unicode.zig:36-99,437-562`, MIT). Drop retired images from the atlas with `drop_image`.
7. **Attach and side-channel plumbing — M.** Snapshot re-sends referenced images; host rewrites `t=f/t=t/t=s` into `t=d` for the tee (or a TD-private frame); add a snapshot test that feeds placeholder cells, snapshots, re-feeds and compares `grid_hash` (the harness already exists in `gridwire.rs` tests).
   *At this point ratatui-image apps and `kitten icat --unicode-placeholder` show real pixels.*
8. **Identify as TD — S.** Answer XTVERSION (`CSI > q` → `DCS > | text ST`, ctlseqs.txt 1532-1534) from the scanner — it is cursor-independent — and offer yazi an upstream brand entry keyed on that reply, so it picks `Kgp` (placeholders) instead of `KgpOld`. Do not spoof `TERM_PROGRAM`. *yazi previews work here, still on route A.*
9. **Decide route B or C — S (a decision, with a spike).** Route B keeps crates.io alacritty and rebuilds the read loop; route C forks vte + alacritty_terminal the way Rio did. The spike that decides it: build the route-B loop for one pane and check whether the attach fence and `SocketPty` survive intact (`host.rs:18-29`, `socketpty.rs:15-21`). If they do not, route C's rebase cost is the cheaper one.
10. **Own the read loop (route B) — L.** Replace alacritty's `EventLoop` with a TD loop that splits the stream at graphics sequences; re-express the attach fence and `SocketPty` against it; flush synchronized updates (`stop_sync`) before acting. Borrow: alacritty `event_loop.rs` (Apache-2.0). Under route C this item becomes "carry the vte/alacritty patches", borrowing Rio's parser changes (MIT).
11. **Direct placements — M.** In the loop, convert `a=T`/`a=p` without `U=1` into TD-internal placeholder cells at the cursor; move the cursor per `C` (kitty's `rows − 1` behaviour); handle position-based deletes (`c`, `p`, `q`, `x`, `y`, `z`). Replace same-cell id-less images promptly — mpv never deletes during playback. Borrow: Rio's `kitty_graphics_protocol.rs` (MIT) for the command semantics. *mpv `--vo=kitty`, timg, chafa `--format=kitty`, presenterm, icat default and yazi `KgpOld` work here.*
12. **Sixel — M.** DCS `q` in the loop; decode with `icy_sixel`'s incremental API (or port Rio's `sixel.rs`); place at the cursor as TD-internal placeholder cells; DECSDM (80), 8452 and 1070; XTSMGRAPHICS replies; then add `4` to DA1 by rewriting alacritty's `?6c` reply in the host proxy. *mpv `--vo=sixel`, yazi's Sixel driver and every DA1-probing client work here.*
13. **iTerm2 OSC 1337 — S/M.** `File=` and `MultipartFile/FilePart/FileEnd`; decode with `image`; same placement path. Removing the OSC before vte sees it also stops alacritty's O(n) Debug-format of the payload.
14. **Video throughput — M.** Measure a real `mpv --vo=kitty` run through the host/window split; move base64 decode off the parse thread; decide on `SINK_DEPTH` or host-side frame dropping from the measurement, not from the arithmetic in §8.
15. **Deferred** — animation (`a=f/a=a/a=c`), relative placements (`P/Q/H/V`), `z≠0` text/image composites. Rio still ships without animation.
