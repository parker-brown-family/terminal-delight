# Evidence: how rio-vt 0.5.28 embeds, against what alacritty_terminal gave TD

Produced 2026-09-25 by a read-only research agent from the published crate
sources (`~/.cargo/registry/src/*/rio-vt-0.5.28`, `teletypewriter-0.5.28`,
`corcovado-0.5.28`, `rio-graphics-0.5.28`) and upstream `raphamorim/rio` at
37624389e8dd (librio, rio-backend, frontends/rioterm). Nothing was compiled for
this map; the architecture gate treats it as the reading to verify, and every
claim that shapes a decision is re-checked in code before it is relied on.
Paths below are relative to the rio-vt `src/` unless named.

## Construction
- `Crosswords::new(dims, CursorShape, listener, WindowId, route_id,
  scrollback_limit)` (crosswords/mod.rs:485). No config struct; knobs are
  `set_grapheme_clustering`, pub fields (`blinking_cursor`, `cursor_shape`,
  `colors`), `grid.update_history(n)`, `graphics.total_limit` (320 MiB default).
- `Dimensions` adds `square_width`/`square_height` (grid/mod.rs:929-974) that
  default to 0.0. **A cell size of 0 silently drops every picture**
  (`place_kitty_overlay` returns early after already answering OK).
  `CrosswordsSize::new(cols, lines)` sets them to 0; `new_with_dimensions` takes
  integer pixels. TD implements `Dimensions` itself with real cell pixels.
- Initial modes include **GRAPHEME_CLUSTER (DEC 2027) on by default**, which
  changes widths from wcwidth; `set_grapheme_clustering(false)` restores
  alacritty-compatible widths.
- `semantic_escape_chars` is fixed and private: ``,│`|:"' ()[]{}<>\t\0``.

## Feeding bytes
- `performer::handler::Processor::default()`, `advance(&mut term, bytes)`. It
  keeps state across chunks (partial UTF-8, sync buffer, XTGETTCAP, APC); one per
  terminal for its whole life.
- Synchronized updates (2026, DCS `=1s`/`=2s`): 150 ms timeout, 2 MiB cap.
  **Bytes after a begin marker in the same `advance` call are parsed at once**,
  unlike vte 0.15 which stops at the marker; buffering starts at the next call.
  `Crosswords` itself ignores the mode (DECRQM 2026 always answers reset).
  Only the Processor knows: `sync_timeout().sync_timeout() -> Option<Instant>`,
  `sync_bytes_count()`, `stop_sync(&mut term)`. **Without rio's own loop an
  expired block stays buffered until the next byte arrives** unless the embedder
  watches the deadline.

## Rio's loop (feature `pty`, on by default)
- `performer::Machine::new(Arc<FairMutex<Crosswords>>, pty, listener,
  WindowId, route_id)`, `channel()` (corcovado), `spawn()`; `Msg::{Input,
  Shutdown, Resize}`. It takes `lease()` for a whole read cycle, like alacritty.
- It is generic over the public `teletypewriter::EventedPty`, so a socket could
  be driven by it after porting from `polling` to `corcovado` (a mio 0.6 fork);
  a closed source makes it spin without a child-event mechanism.
- It never writes replies: `RioEvent::PtyWrite` must be forwarded by the
  embedder. It sends `TerminalDamaged` only while `damage_event_in_flight` is
  false, and **the embedder must clear that flag** after each read of damage.
- `FairMutex` is alacritty's code copied verbatim.
- PTY spawning: `teletypewriter::create_pty_with_spawn(shell, args, cwd, env,
  cols, rows, w_px, h_px)`; no `setup_env` equivalent.

## Reading the grid
- Same addressing as alacritty: `Line(0)` is the top of the live screen,
  history is negative to `-history_size`. `Pos{row, col}` instead of
  `Point{line, column}`. **Out-of-range indices are only `debug_assert!`ed**;
  release builds return the wrong ring slot, so every access must clamp.
- `Square` is a packed `u64`: `c()` (base char only), `wide()` →
  `Narrow|Wide|Spacer|LeadingSpacer`, `CellFlags` (`WRAPLINE`, `HYPERLINK`,
  `GRAPHEME`). Style lives in a side table: `grid.style_of(&sq)` → `Style{fg,
  bg, underline_color: Option<AnsiColor>, flags: StyleFlags}`; **background-only
  cells return garbage from `style_id()`**, use the checked accessors or
  `style_of`. Marks and clusters via `grid.cell_text(pos)`.
- `AnsiColor{Named, Spec(ColorRgb), Indexed(u8)}`; `NamedColor` 0-15, then
  Foreground=256, Background=257, Cursor=258, the dims, LightForeground,
  DimForeground — the same slot layout as vte's.
- `cursor()` → `CursorState{pos, content: CursorShape}`; `content` is `Hidden`
  when DECTCEM hides it **or when the view is scrolled**.
- `display_offset()`, `scroll_display(Scroll)` (emits `MouseCursorDirty`),
  `history_size()`, `lines_evicted()`, `damage()`/`reset_damage()`,
  `peek_damage_event()`.
- `mode()` → `crosswords::Mode`: alacritty's bits plus `MOUSE_REPORT_X10`,
  `GRAPHEME_CLUSTER`, three sixel bits, no sync bit.
- Colours: `colors()` holds OSC overrides only (`None` = embedder palette).

## Selection, search, vi mode
- Ported from alacritty: `selection::{Selection, SelectionType{Simple, Block,
  Semantic, Lines}, SelectionRange}`, `new(ty, Pos, Side)`, `update`, `rotate`,
  `to_range(&term)`; `term.selection` is a pub field; `selection_to_string()`
  and `bounds_to_string(start, end)` include zero-width marks.
  `RegexSearch`, semantic searches, vi mode all present.

## Events (`RioEvent`, only `send_event` is ever called)
`PtyWrite(route, String)`, `TextAreaSizeRequest(route, Fn(WindowSize))` (**total
pixels**, not cell pixels), `ColorRequest(route, idx, Fn(ColorRgb))`,
`ClipboardStore(type, String)`, `ClipboardLoad(route, type, Fn)`, `Title(route,
String)` (empty = reset), `Bell(route)`, `CursorBlinkingChange`,
`MouseCursorDirty`, `ColorChange`, `RenderRoute`, `UpdateGraphics{route_id,
queues}`, `CloseTerminal(route)`, `CurrentDirectoryChanged`, `ProgressReport`,
`DesktopNotification`, `GlyphProtocol*`, and from `Machine` only
`TerminalDamaged`, `ChildExited`, `Render`. There is no `Wakeup`.
**The listener runs on the parsing thread with the terminal lock held**; it
must only post messages.

## Pictures
- Feature `graphics` (off by default, undocumented) is required for PNG
  (`f=100`) and iTerm2; without it PNG fails silently under `q=2`.
- `term.graphics`: `kitty_images` (id → `StoredImage{data: GraphicData{width,
  height, color_type, pixels, ..}}`), `kitty_placements` ((image, placement) →
  `KittyPlacement{dest_row: i64 absolute, dest_col, columns, rows, source rect,
  cell offsets, z_index, ..}`), `kitty_virtual_placements`.
- Absolute row = `lines_evicted() + history_size() + screen row` at placement;
  viewport row = `dest_row − (lines_evicted + history_size − display_offset)`.
  `kitty_overlay_geometry` does it if `OverlayViewport.history_size` is set to
  `lines_evicted + history_size`.
- Placeholders (U+10EEEE) are ordinary cells the renderer resolves itself.
- `delete_graphics` covers the `a=d` actions; `graphics.kitty_graphics_dirty` is
  never cleared by rio-vt.

## Resize, serialisation, threads
- `resize<S: Dimensions>(size)` reflows the main grid, not the alt; never pass 0
  columns or lines (undefined behaviour in release). No PTY resize, no event.
- `format(FormatOptions::vt())` re-emits the live screen only and drops marks,
  hyperlinks, scrollback and modes — TD keeps its own encoder.
- `Crosswords<U>` is Send when `U: Send` (derived, not compiled).

## Traps worth a test each
graphics feature; zero cell size; README `EventListener` example wrong; `pty`
on by default; parking_lot `nightly`/HLE features forced (harmless on stable
Linux); `damage_event_in_flight` and `kitty_graphics_dirty` left to the
embedder; listener under the lock; sync tearing and stuck blocks; mode 2027 on;
background-only cell accessors; out-of-range lines; resize to 0; `WindowSize` in
total pixels; `CSI 16 t` prints an `f32`, so a fractional cell size gives an
invalid reply; colour round-trips truncate; XTVERSION answers "Rio 0.5.28".
