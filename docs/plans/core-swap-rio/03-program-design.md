# Gate 3 — Program design: the `vt` module, type by type

**State:** self-approved 2026-09-25 under Parker's instruction. This turns the
architecture into the shapes the code will have, so that the slices in Gate 4
are typing rather than deciding. Every signature below was checked against
the published source of rio-vt 0.5.28 and alacritty_terminal 0.26.0 in the
cargo registry.

## One rule that shapes the rest: speak the vocabulary TD already reads

TD's code reads the terminal in alacritty's words: `Line`, `Column`, `Point`,
`Side`, `Flags::WIDE_CHAR_SPACER`, `TermMode::ALT_SCREEN`,
`Color::Named(NamedColor::Foreground)`. rio-vt forked alacritty and kept most of
them under other names (`Pos{row, col}`, `StyleFlags`, `AnsiColor`, `Mode`).

`vt` defines **its own types with the names and numbering TD already uses**.
That makes the port reviewable: at most call sites the only change is the
import, and a reviewer can read the diff as "same words, TD's crate". The
numbering is frozen by test, not by coincidence, because it is also the host
protocol's wire format.

What changes shape, deliberately:

| alacritty today | `vt` | why |
|---|---|---|
| `term.grid()[Line(l)][Column(c)]` returns `&Cell` | `term.row(Line(l))[Column(c)]` returns an owned `Row` | rio-vt packs a cell into a `u64` with its style in a side table, so there is no `Cell` in memory to lend. A row is materialised on request, padded to the width, and clamped |
| `term.grid().display_offset()` and friends | `term.display_offset()` | one object, no `grid()` |
| `*term.mode()` | `term.mode()` | by value; both cores keep modes in a word |
| `term.selection = Some(Selection::new(ty, p, side))` | `term.start_selection(ty, p, side)` | the selection has to live inside the core, because both cores move it when output scrolls the screen; TD states intent and the core keeps the state |
| `term.selection.as_mut().map(|s| s.update(p, side))` | `term.update_selection(p, side)` | same |
| `term.selection.as_ref().and_then(|s| s.to_range(&term))` | `term.selection_range()` | same |
| `Notify` trait and alacritty's `Notifier` | `vt::Notifier` with an inherent `notify` | the input audit counts the literal `notifier.notify(`; the spelling survives |
| `FairMutex` with `lease` and `lock_unfair` | `parking_lot::Mutex` | the pump holds the lock per chunk and releases it fairly; the fence is the lock |

## Files

```
app/src/vt/
  mod.rs        Term (the facade inside the lock), Shared, re-exports, the
                boundary guard test
  types.rs      Line Column Point Side Scroll SelectionType SelectionRange
                Flags TermMode NamedColor Rgb Color Cell Row Indexed Hyperlink
                CursorShape Cursor RenderableContent WindowSize TermSize
                ClipboardType Event Listener Picture
  pump.rs       Msg Notifier Tap Source Pump — TD's read loop
  pty.rs        spawn a shell on a pseudoterminal; resize; child watch; setup_env
  socket.rs     Source over a UnixStream (the replica), replacing socketpty.rs
  rio.rs        the default core (rio-vt)
  alacritty.rs  the fallback core, compiled only with `--features core-alacritty`
```

## `Term`, the thing behind the lock

```rust
pub type Shared = Arc<parking_lot::Mutex<Term>>;

pub struct Term { core: Core, size: TermSize }   // Core = rio::Core | alacritty::Core

impl Term {
    pub fn new(size: TermSize, listener: Arc<dyn Listener>) -> Term;
    // feeding
    pub fn advance(&mut self, bytes: &[u8]);
    pub fn sync_bytes_count(&self) -> usize;
    pub fn sync_deadline(&self) -> Option<Instant>;
    pub fn flush_sync(&mut self);
    // geometry (alacritty's addressing: Line(0) is the top of the live screen,
    // history is negative down to -history_size)
    pub fn resize(&mut self, size: TermSize);          // floors at 2 x 1
    pub fn columns(&self) -> usize;   screen_lines, history_size, total_lines,
    pub fn topmost_line(&self) -> Line;  bottommost_line, last_column, display_offset
    // content (every access clamped; a row is always `columns()` wide)
    pub fn row(&self, line: Line) -> Row;
    pub fn cell(&self, point: Point) -> Cell;
    pub fn display_iter(&self) -> impl Iterator<Item = Indexed> + '_;
    pub fn renderable_content(&self) -> RenderableContent<'_>;
    pub fn cursor(&self) -> Cursor;                   // grid cursor, absolute
    pub fn mode(&self) -> TermMode;
    pub fn title(&self) -> Option<String>;
    // TD's own mutations
    pub fn scroll_display(&mut self, scroll: Scroll);
    pub fn clear_history(&mut self);
    pub fn start_selection(&mut self, ty: SelectionType, p: Point, side: Side);
    pub fn update_selection(&mut self, p: Point, side: Side);
    pub fn clear_selection(&mut self);
    pub fn has_selection(&self) -> bool;
    pub fn selection_range(&self) -> Option<SelectionRange>;
    pub fn selection_to_string(&self) -> Option<String>;
    pub fn semantic_search_left(&self, p: Point) -> Point;   // and _right
    // pictures
    pub fn pictures(&self) -> Vec<Picture>;           // those on the viewport now
    pub fn forget_pictures(&mut self);
}
```

`Term` owns everything the two cores should agree on — clamping, the resize
floor, row padding, what a blank cell's character is (`' '`, never `'\0'`), the
`CSI 16 t` answer — so each adapter only translates. The adapters implement a
private `Backend` trait with the narrow half of this list; the facade is the
only public face.

## Events

```rust
pub trait Listener: Send + Sync { fn send_event(&self, event: Event); }

pub enum Event {
    Wakeup,                                   // sent by the pump, never a core
    PtyWrite(String),
    TextAreaSizeRequest(Arc<dyn Fn(WindowSize) -> String + Send + Sync>),
    ColorRequest(usize, Arc<dyn Fn(Rgb) -> String + Send + Sync>),
    Title(String), ResetTitle, Bell,
    ClipboardStore(ClipboardType, String),
    ClipboardLoad(ClipboardType, Arc<dyn Fn(&str) -> String + Send + Sync>),
    CursorBlinkingChange, MouseCursorDirty,
    ChildExit(ExitStatus),                    // sent by the pump
    Exit,                                     // sent by the pump
}
```

The listener is called with the terminal lock held, by both cores. The two
listeners TD has (`term::EventProxy` for a window, the host's channel sender)
only bump a counter and send on a channel. A comment on the trait says so.

`TextAreaSizeRequest` keeps alacritty's shape — the formatter is handed
`WindowSize {num_lines, num_cols, cell_width, cell_height}` — so the pane and
the host answer `CSI 14 t` exactly as they do now. rio-vt's formatter wants
the text area in total pixels, and the rio adapter wraps it.

`CSI 16 t` (cell size) becomes something the core answers itself, from the
cell size `Term` was last given, as a `PtyWrite`. rio-vt already does this.
The alacritty adapter does it with TD's existing `ptyscan::CellSizeQuery`,
before the parser, which is where the host does it today. The host's own
scanner in its tee then goes, and a window-owned pane, which never answered
this question, now does.

Everything else rio-vt emits (render requests, damage, graphics queues, desktop
notifications, progress, glyph protocol) is dropped in the adapter, never
forwarded, so it cannot move the content generation.

## The pump

```rust
pub enum Msg { Input(Cow<'static, [u8]>), Resize(WindowSize), Shutdown }

#[derive(Clone)]
pub struct Notifier { tx: mpsc::Sender<Msg>, poller: Arc<Poller> }
impl Notifier {
    pub fn notify<B: Into<Cow<'static, [u8]>>>(&self, bytes: B);   // empty = no-op
    pub fn resize(&self, size: WindowSize);
    pub fn shutdown(&self);
}

pub trait Tap: Send { fn tap(&mut self, bytes: &[u8]); }   // runs under the lock

pub trait Source: Send {
    fn fd(&self) -> BorrowedFd<'_>;
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    fn write(&mut self, buf: &[u8]) -> io::Result<usize>;
    fn resize(&mut self, size: WindowSize) -> io::Result<()>;
    fn child_fd(&self) -> Option<BorrowedFd<'_>>;       // a pidfd, or none
    fn reap(&mut self) -> Option<ExitStatus>;
}

pub fn spawn(term: Shared, listener: Arc<dyn Listener>, source: Box<dyn Source>,
             tap: Option<Box<dyn Tap>>) -> io::Result<Notifier>;
```

One thread per terminal, named `td-pump`, one `polling::Poller`, level-triggered
like alacritty's. Each turn:

1. Wait on the source (readable, and writable while input is queued), the
   child's pidfd, and the notifier's wake, for no longer than the core's
   synchronized-update deadline.
2. Nothing happened and nothing is queued: the deadline passed. Lock, flush the
   update, send `Wakeup`.
3. Drain the channel: queue input; resize the source (an error is logged and
   survives — never `process::exit`); `Shutdown` ends the loop.
4. Readable: read up to 64 KiB **outside** the lock, then lock, tap, `advance`,
   release with `unlock_fair`. Repeat until the source would block or 1 MiB has
   been read this turn. Send `Wakeup` unless every byte went into a
   synchronized-update buffer — alacritty's rule, kept.
5. End of file, `EIO` or any other read error: drain nothing more, send
   `Exit`, stop.
6. The pidfd is readable: drain the source until it would block, reap the
   child, send `ChildExit(status)` then `Exit`, stop.
7. Writable: write queued input until it would block.

On the way out the thread drops the source, which for a pseudoterminal sends
`SIGHUP` to the child and waits for it, as alacritty's `Pty` did.

## The pseudoterminal

`vt::pty::spawn(options, size) -> io::Result<PtySource>` copies alacritty's
Unix `tty::new` step for step, because children have relied on its effects for
years: `openpty` with the window size (`rustix-openpty`), `IUTF8` on, the shell
from the options or `$SHELL` or the password entry, `USER`/`HOME`/
`ALACRITTY_WINDOW_ID=0`/`WINDOWID=0` in the environment (a child checking for
those still finds them), the extra environment, `XDG_ACTIVATION_TOKEN` and
`DESKTOP_STARTUP_ID` removed, then in the child: `setsid`, `chdir`,
`TIOCSCTTY`, close both descriptors, reset six signals. The master is made
non-blocking.

Two differences, both intended. Child exit is watched with a pidfd
(`pidfd_open`, Linux 5.3) instead of a process-wide `SIGCHLD` handler, so
there is no signal-hook registration to leak, and resize returns its error.

`vt::pty::setup_env()` is alacritty's `setup_env` (TERM `alacritty` when that
terminfo exists, otherwise `xterm-256color`; `COLORTERM=truecolor`), so `TERM`
does not move in this change.

## Pictures

```rust
pub struct Picture {
    pub id: u64,                        // stable while the picture lives
    pub pixels: Arc<PictureData>,       // RGBA, width, height; shared, not copied
    pub line: i32,                      // viewport row of the top-left cell (may be negative)
    pub column: usize,
    pub columns: usize, pub rows: usize,// cells it covers
    pub source: Option<(u32, u32, u32, u32)>, // crop, in image pixels
    pub z: i32,
}
```

The rio adapter reads `graphics.kitty_placements` and `kitty_images` and
converts each placement's absolute row with
`dest_row − (lines_evicted + history_size − display_offset)`, keeping those that
touch the viewport. The alacritty adapter returns none. `forget_pictures` clears
rio-vt's images and placements (`a=d,d=A`), so a picture a program sent is gone
from the core as well as from the screen once its pane has been hidden.

The pane draws them in `styled_lines`' canvas pass: one `gpui::RenderImage` per
picture id, converted from RGBA to the BGRA gpui wants, cached in the pane by id
and dropped with `cx.drop_image` when the picture leaves or the pane is hidden.

## The port, file by file

| File | What it becomes |
|---|---|
| `term.rs` | `Session { term: vt::Shared, notifier: vt::Notifier, events, master, shell_pid, generation }`; `spawn_pty` returns a `vt::pty::PtySource`; `wire_event_loop` becomes `wire_pump(source, size, cell, tap, …)`; `attach_in` builds a `vt::socket::SocketSource` and a counting tap. `EventProxy` implements `vt::Listener` unchanged in behaviour |
| `socketpty.rs` | deleted: its three workarounds (private poll keys, a second descriptor for hang-up, `Exited(None)`) belonged to alacritty's loop. Its tests move to `vt/socket.rs` |
| `host.rs` | `TeePty`/`TeeReader` become a `HostTap` (spoke flag, sink); `HostProxy` implements `vt::Listener`; the three `lease()`+`lock_unfair()` fences become `lock()`, the attach one flushing a pending synchronized update before encoding |
| `gridwire.rs` | reads `vt::Term`; `grid_hash` hashes the frozen `bits()` and discriminants it hashed before, so the hash of an identical screen is identical; `ReplicaGuard::check` takes the one lock |
| `pane.rs` | imports move to `vt`; `grid()[..]` becomes `row(..)`; selection calls become the four verbs; pictures drawn; `forget_pictures` on hide |
| `main.rs` | `setup_env` from `vt::pty`; `WindowSize` from `vt` |
| tests | the correctness matrix, the snapshot round trip and the attached tests run unchanged in meaning against `vt::Term`, on both cores |

## Testing on both cores

`cargo test` runs the default core, rio-vt. `cargo test --features
core-alacritty` runs the same suite on alacritty. The second is the oracle for
the first: a test that passes on alacritty and fails on rio-vt is a difference
between the cores, and each one is either fixed in the rio adapter or written
down as a deliberate change, test and all.

Three new tests guard the design itself:

- **the boundary** — no file outside `app/src/vt/` names `rio_vt` or
  `alacritty_terminal` (a source scan, cut before its own test);
- **the frozen numbering** — every `Flags`, `TermMode` and `NamedColor` value
  pinned to its alacritty 0.26 number;
- **the fence** — bytes written while an attach is in progress appear exactly
  once across snapshot plus live stream, including bytes inside an open
  synchronized update.
