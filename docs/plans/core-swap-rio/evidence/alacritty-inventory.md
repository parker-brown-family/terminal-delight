# Evidence: every dependency Terminal Delight has on alacritty_terminal

Produced 2026-09-25 by a read-only research agent over `app/src` at `origin/main`
62613cf (branch `feat/core-swap-rio`, clean). Reproduced here as the input to
the architecture gate; line numbers are as of that commit. Summary counts: about
200 distinct items (46 types/traits/functions, 86 methods/fields, 67 variants and
flag constants), about 415 production call-site lines (pane.rs about 227,
gridwire.rs 97, host.rs 48, term.rs 29, socketpty.rs 10) and about 305 test lines.

## The facts the architecture has to answer

1. **History is addressed at negative `Line` indices.** `Grid[Line]` is absolute:
   `Line(0)` is the top of the live screen whatever the display offset, history
   is negative. The FOCUS reader (`budget_range`, `document_with`,
   `grid_rows_in`), `last_human_message`, `search_grid`/`grep_grid` (whose
   `GridHit.line` main.rs feeds back into `scroll_to_line` later) and
   `scroll_to_line`/`scroll_to_human` all build on it. The viewport iterator
   works in viewport lines instead, converted with `line + display_offset` in six
   places. Pinned by `scrolled_off_rows_stay_readable_at_negative_line_indices`.
2. **The attach fence is alacritty's lease.** `lease()` then `lock_unfair()` at
   host.rs 879-882, 949-950, 1324-1325 and gridwire.rs 584-585 relies on the
   reader holding the lease across a whole read-and-parse cycle
   (`event_loop.rs:117`), and on lease-then-fair-lock deadlocking.
   `CountingReader` and `TeeReader` assume `reader()` is the only read path.
   Inferred and untested: DEC 2026 synchronized updates buffered inside vte's
   `Processor` (up to 150 ms / 2 MiB), and an escape split across a read, both
   leave bytes counted and teed but not yet in the grid when a snapshot is taken.
3. **The content generation is per event, not per parse.** alacritty sends
   `Wakeup` only if not all bytes were sync-buffered, so the echo bench's "once
   per parse cycle" holds only outside synchronized output. TD's own mutations
   (scroll, selection, `clear_history`, resize) never bump it.
4. **`SocketPty` depends on event-loop internals:** the `pub(crate)` poll keys 0
   and 1, a second descriptor for hang-up because the loop skips interrupt
   events, and `Exited(None)` producing `Event::Exit` rather than `ChildExit`.
5. **`grid_hash` is built from alacritty's bit layouts** — `Flags::bits()`,
   `TermMode::bits()`, `NamedColor as u32`. Host and window must link the same
   crate version, and `PROTO_VERSION = 1` does not say so. A mixed pair would
   mismatch forever; `repair_step` gives up after one retake.
6. **`asked_colour`** relies on `NamedColor::{Foreground,Background,Cursor} as
   usize` matching the `ColorRequest` index convention and named 0..15 being the
   ANSI palette.
7. **Snapshot correctness depends on alacritty/vte details:** `swap_alt` is
   destructive (the primary under an alt screen cannot be read), `CSI 3 J` must
   clear history, the colon SGR underline forms and 58/59 are required, and the
   replica resizes its own `Term` before the host does.
8. **TD already owns more than it seems.** All key and mouse encoding (no DECCKM,
   no keypad mode, no kitty keyboard, no click reporting), the colour palette
   (program OSC 4/10/11 overrides ignored), link detection (no OSC 8), search (no
   `RegexSearch`), the `CSI 16 t` answer (host only). The pane never draws
   combining marks, italic, strikeout, hidden, underline styles or colours, or
   the cursor shape — though gridwire replicates and hashes all of them. OSC 52
   `ClipboardStore` and `ResetTitle` are unhandled. The row-to-string loop is
   duplicated about ten times.
9. **Process side effects.** `setup_env` mutates the environment at main.rs
   39408, after verb dispatch, so `serve` never calls it. `tty::new(.., 0)` stamps
   `ALACRITTY_WINDOW_ID=0` and `WINDOWID=0` into every child. `Pty::on_resize`
   calls `die!` (`process::exit(1)`) if `TIOCSWINSZ` fails, which would take down
   a whole session host. Dropping a `Pty` sends SIGHUP and waits.
10. **The terminal-input security audit is keyed to spelling.**
    `the_manifest_counts_every_write_to_a_terminal` counts the literal
    `notifier.notify(` in pane.rs and pane/doc.rs and expects 11.
11. **Implicit behaviour leaned on:** selection rotating and clearing on scroll,
    the semantic word boundaries in `Config::default()`, the 10,000-line
    scrollback cap, `CSI 18 t` answered through `PtyWrite`, vte ignoring DEC
    1047/1048.

## Capability buckets (condensed)

- **PTY and I/O loop:** `spawn_pty`, `spawn_in`, `wire_event_loop`,
  `Session::resize` (term.rs); `new_restored`, event pump, debounced resize
  (pane.rs); spawn, write, resize, close (host.rs 681-984); `setup_env` (main.rs).
- **Drawing:** `styled_lines` (pane.rs 7295-7454) through
  `renderable_content().display_iter`; `live_rows`, `recent_lines`,
  `last_human_message`, `grid_rows_in`, `document_with`, `grid_snapshot`,
  `screen_signature`, `top_is_human`.
- **Selection and copy:** `Selection::new/update/to_range`,
  `SelectionType::{Simple,Semantic,Lines}`, `Side`, `viewport_to_point`,
  `semantic_search_left/right`, `selection_to_string`, the TD-built visual copy.
- **Scrolling:** `scroll_display(Scroll::{Delta,PageUp,PageDown,Top,Bottom})`,
  `display_offset`, `history_size`, `clear_history`.
- **Modes read:** ALT_SCREEN, BRACKETED_PASTE, MOUSE_MODE, SGR_MOUSE,
  ALTERNATE_SCROLL, APP_CURSOR, FOCUS_IN_OUT, SHOW_CURSOR (pane); plus APP_KEYPAD,
  MOUSE_REPORT_CLICK, MOUSE_DRAG, MOUSE_MOTION, UTF8_MOUSE, LINE_FEED_NEW_LINE,
  INSERT, ORIGIN, LINE_WRAP (snapshot and hash).
- **Events handled:** Wakeup, PtyWrite, TextAreaSizeRequest, ColorRequest,
  Title, Bell, Exit/ChildExit. Dropped: ClipboardStore, ResetTitle,
  ClipboardLoad, CursorBlinkingChange, MouseCursorDirty.
- **Replica:** `encode_snapshot`, `grid_hash`, `ReplicaGuard::check`,
  `SocketPty`, `CountingReader`, `attach_in`, `TeeReader`/`TeePty`, `attach`,
  `grid_check`, `heal_after_the_alternate_screen`, `watch_once`;
  `make_pane_attached`, `watch_for_divergence`, `repair_step` (main.rs).
- **Wrapper surface main.rs uses:** `title`, `exited`, `search_grid`,
  `grep_grid`, `scroll_to_line`, `recent_lines`, `last_human_message`,
  `mirror_snapshot`, `scroll_by_wheel`, `content_generation`,
  `check_divergence`, `stream_consumed`, `agent_status`, `send_notes`.
