//! G0b seam — real shell via alacritty_terminal (Option A from docs/PLAN.md §2).
//! Written clean-room against the crate's public API (docs.rs + registry source).
//!
//! alacritty_terminal's EventLoop owns the PTY reader thread, the VTE parser
//! pump, and the writer. We provide: an EventListener proxy that ships events
//! onto an async channel (consumed by the gpui entity), and keystroke bytes in
//! via the Notifier. NOTE: Event::PtyWrite (query responses like DA/DSR) is
//! emitted through the proxy and NOT auto-routed back to the PTY — the consumer
//! must bounce it via the notifier or interactive apps hang.

use std::fs::File;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use alacritty_terminal::{
    event::{Event as TermEvent, EventListener, WindowSize},
    event_loop::{EventLoop, Msg, Notifier},
    grid::Dimensions,
    sync::FairMutex,
    term::{Config, Term},
    tty,
};
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};

/// Grid dimensions for Term::new (the crate's own TermSize lives in its test module).
#[derive(Clone, Copy, Debug)]
pub struct GridSize {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// Forwards terminal events from the EventLoop thread onto an async channel,
/// and bumps the session's content generation as it does — every event marks a
/// moment the emulation state may have changed, so "the counter moved" is a
/// sound (if slightly over-eager) proxy for "the grid is different now".
/// Consumers cache derived views (the FOCUS reader's document) against it.
#[derive(Clone)]
pub struct EventProxy {
    events: UnboundedSender<TermEvent>,
    generation: Arc<AtomicU64>,
    /// Whether a program's question to the terminal is answered from here.
    answers_here: bool,
}

/// Who answers the questions a program asks the terminal — what are you (DA),
/// where is the cursor (DSR) — each of which must be answered exactly once.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Answers {
    /// This process owns the pseudoterminal, so it answers. Note that it does
    /// not answer *here*: the event is forwarded to the pane, which bounces it
    /// back through the notifier (see the module note).
    Here,
    /// Another process owns the pseudoterminal and has already answered, and
    /// its answer is in the byte stream we are reading. Answering again would
    /// not be a duplicate reply — it would reach the program as characters the
    /// user appears to have typed, mid-screen, in the middle of vim.
    Elsewhere,
}

impl EventProxy {
    fn new(
        events: UnboundedSender<TermEvent>,
        generation: Arc<AtomicU64>,
        answers: Answers,
    ) -> Self {
        Self {
            events,
            generation,
            answers_here: answers == Answers::Here,
        }
    }
}

impl EventListener for EventProxy {
    fn send_event(&self, event: TermEvent) {
        // The counter moves for every event, forwarded or swallowed: it is the
        // invalidation token for cached views, and an event we decline to
        // answer still means the emulation has moved on. A replica whose
        // generation stopped advancing would freeze every cache built on it.
        self.generation.fetch_add(1, Ordering::Relaxed);
        if !self.answers_here && matches!(event, TermEvent::PtyWrite(_)) {
            return;
        }
        let _ = self.events.unbounded_send(event);
    }
}

pub struct Session {
    pub term: Arc<FairMutex<Term<EventProxy>>>,
    pub notifier: Notifier,
    /// Taken once by the UI entity to drive event handling.
    pub events: Option<UnboundedReceiver<TermEvent>>,
    /// Our own handle on the PTY master — used to ask the kernel what the
    /// foreground process is (tcgetpgrp), powering mode detection. `None` on a
    /// replica: the kernel object belongs to the session host, and the only
    /// honest thing a client can say about a file descriptor it does not have
    /// is that it does not have it.
    pub master: Option<File>,
    /// The process at the far end of this terminal.
    ///
    /// `Some`, and our own child, when we forked it. `Some`, and somebody
    /// else's child, when a session host reported it — readable through /proc,
    /// but never ours to signal. `None` when nobody has said, which is a state
    /// that genuinely occurs and is not a zero: a pane whose owner has not
    /// named a process is not a pane running pid 0, and mode detection, cwd
    /// capture and the vitals sweep each have to be able to tell those apart or
    /// they will confidently report facts about init.
    pub shell_pid: Option<u32>,
    /// Monotonic content generation, bumped by [`EventProxy`] on every terminal
    /// event. Equal generations guarantee the grid has not changed since; a
    /// changed generation merely *permits* change (title/bell events bump it
    /// too), which errs on the safe side for cache invalidation.
    pub generation: Arc<AtomicU64>,
}

impl Session {
    /// The current content generation (see [`Session::generation`]).
    pub fn content_generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }
}

impl Session {
    /// Resize both the emulation grid and the PTY (SIGWINCH to the child).
    ///
    /// On a replica the second half is not a syscall but a sentence: the size
    /// is announced to whoever owns the pseudoterminal (see
    /// [`crate::socketpty::SocketPty`]), because a terminal that more than one
    /// client may be looking at is told its size rather than inferring it.
    pub fn resize(&self, size: GridSize, cell_width: u16, cell_height: u16) {
        let window_size = WindowSize {
            num_lines: size.rows as u16,
            num_cols: size.cols as u16,
            cell_width,
            cell_height,
        };
        let _ = self.notifier.0.send(Msg::Resize(window_size));
        self.term.lock().resize(size);
    }
}

/// Everything `tty::new` needs to start a shell, plus the environment the
/// caller wants stamped into it.
pub struct SpawnSpec {
    pub size: GridSize,
    pub cell_width: u16,
    pub cell_height: u16,
    /// A vanished dir falls back to the default start directory rather than
    /// failing the pane.
    pub cwd: Option<std::path::PathBuf>,
    pub env: Vec<(String, String)>,
}

/// Spawn the user's default shell on a PTY, emulation wired, I/O thread running.
#[allow(dead_code)]
pub fn spawn(size: GridSize, cell_width: u16, cell_height: u16) -> io::Result<Session> {
    spawn_in(size, cell_width, cell_height, None)
}

/// `spawn`, but the shell starts in `cwd` (session restore).
pub fn spawn_in(
    size: GridSize,
    cell_width: u16,
    cell_height: u16,
    cwd: Option<std::path::PathBuf>,
) -> io::Result<Session> {
    let spec = SpawnSpec {
        size,
        cell_width,
        cell_height,
        cwd,
        env: vec![],
    };
    let pty = spawn_pty(&spec)?;
    // Read before the pseudoterminal is handed to the event loop, which takes
    // ownership of it: a generic `T` has neither of these accessors.
    let master = pty.file().try_clone().ok();
    let shell_pid = pty.child().id();
    wire_event_loop(pty, size, master, Some(shell_pid), Answers::Here)
}

/// The pseudoterminal half of a spawn: a real kernel object with a real child.
pub fn spawn_pty(spec: &SpawnSpec) -> io::Result<tty::Pty> {
    let window_size = WindowSize {
        num_lines: spec.size.rows as u16,
        num_cols: spec.size.cols as u16,
        cell_width: spec.cell_width,
        cell_height: spec.cell_height,
    };

    let mut options = tty::Options {
        working_directory: spec.cwd.clone().filter(|d| d.is_dir()),
        ..Default::default()
    };
    for (name, value) in &spec.env {
        options.env.insert(name.clone(), value.clone());
    }
    // A demo window runs THIS binary as every pane's program — a frozen screen of
    // lorem-ipsum styled like a real agent session — instead of the user's shell.
    // So a shared demo shows no real shell, cwd, scrollback, or secret, yet flows
    // through the normal PTY→grid→warp/grade render path for full fidelity. Gated
    // on TD_DEMO, which is set only for the spawned demo window (see main).
    if std::env::var_os("TD_DEMO").is_some() {
        if let Ok(exe) = std::env::current_exe() {
            options.shell = Some(tty::Shell::new(
                exe.to_string_lossy().into_owned(),
                vec!["--td-emit-demo".to_string()],
            ));
        }
    }
    tty::new(&options, window_size, 0)
}

/// The emulator half: a `Term`, alacritty's own event loop over whatever the
/// terminal actually is, and the channel the pane reads events from.
///
/// Generic over the pseudoterminal because the client's is a socket
/// ([`crate::socketpty::SocketPty`]) and the bounds are exactly the ones
/// `EventLoop::new` states. `master` and `shell_pid` are passed in rather than
/// asked of `pty`: a generic `T` has no `.file()` or `.child()`, and on a
/// replica neither exists to ask.
pub fn wire_event_loop<T>(
    pty: T,
    size: GridSize,
    master: Option<File>,
    shell_pid: Option<u32>,
    answers: Answers,
) -> io::Result<Session>
where
    T: tty::EventedPty + alacritty_terminal::event::OnResize + Send + 'static,
{
    let (tx, rx) = unbounded();
    let generation = Arc::new(AtomicU64::new(0));
    let proxy = EventProxy::new(tx, generation.clone(), answers);
    let term = Arc::new(FairMutex::new(Term::new(
        Config::default(),
        &size,
        proxy.clone(),
    )));
    let event_loop = EventLoop::new(term.clone(), proxy, pty, false, false)?;
    let notifier = Notifier(event_loop.channel());
    let _io_thread = event_loop.spawn(); // owns its thread; lives as long as the PTY

    Ok(Session {
        term,
        notifier,
        events: Some(rx),
        master,
        shell_pid,
        generation,
    })
}

/// What a window hands [`attach_in`] once the control conversation is done: the
/// pane's byte stream, and somewhere to put a resize.
pub struct AttachStreams {
    /// Connected, and already past its greeting: the next bytes readable are
    /// the snapshot of everything this pane has printed, then its live output.
    pub bytes: std::os::unix::net::UnixStream,
    /// A resize is a control verb, never a byte-stream fact. Called from the
    /// event loop's own thread, hence `Send`.
    pub announce_resize: Box<dyn FnMut(WindowSize) + Send + 'static>,
}

/// THE SEAM: a terminal this process does not own, driven by alacritty's stock
/// event loop over a socket, into an ordinary [`Session`].
///
/// The same type comes back as [`spawn_in`] returns, which is the whole point —
/// every one of the pane's reads (styled lines, selection, scrollback, the
/// FOCUS document mirror) runs against the replica unchanged, and selection and
/// scroll position stay client state by construction, because they were never
/// anywhere else.
///
/// `shell_pid` is what the host reported: an attribute of a process this window
/// did not start, or `None` if the host named none.
pub fn attach_in(
    size: GridSize,
    streams: AttachStreams,
    shell_pid: Option<u32>,
) -> io::Result<(Session, crate::gridwire::ReplicaGuard)> {
    let AttachStreams {
        bytes,
        announce_resize,
    } = streams;
    let pty = crate::socketpty::SocketPty::with_resize(bytes, announce_resize)?;
    // Shared with the reader the event loop is about to own: the count of bytes
    // that have actually reached this replica's parser, which is the clock the
    // divergence guard reads.
    let consumed = pty.consumed();
    let session = wire_event_loop(pty, size, None, shell_pid, Answers::Elsewhere)?;
    let guard = crate::gridwire::ReplicaGuard::new(
        session.term.clone(),
        session.generation.clone(),
        consumed,
    );
    Ok((session, guard))
}

/// The client seam, driven for real.
///
/// A socket pair stands in for the host: one end is handed to [`attach_in`] and
/// becomes an ordinary [`Session`], the other is written to and read from the
/// way a session host would. Nothing here is a mock — it is alacritty's own
/// event loop, parser and `Term`, which is the whole argument for cutting the
/// seam at a file descriptor.
#[cfg(test)]
mod attached {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use alacritty_terminal::event::Event as TermEvent;
    use alacritty_terminal::index::{Column, Line};
    use alacritty_terminal::term::{Config, Term};
    use alacritty_terminal::vte::ansi::Processor;

    use super::*;
    use crate::gridwire::{grid_hash, GridCheck, GuardVerdict, Unsettled};

    fn within(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + limit;
        while Instant::now() < deadline {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        done()
    }

    /// An attached pane and the host end of its stream.
    struct Pair {
        host: UnixStream,
        session: Session,
        guard: crate::gridwire::ReplicaGuard,
        /// Every resize the replica announced, in order — the control verbs a
        /// real host would have received.
        announced: Arc<Mutex<Vec<WindowSize>>>,
    }

    fn attached(cols: usize, rows: usize, shell_pid: Option<u32>) -> Pair {
        let (host, client) = UnixStream::pair().expect("socket pair");
        let announced: Arc<Mutex<Vec<WindowSize>>> = Arc::default();
        let heard = announced.clone();
        let (session, guard) = attach_in(
            GridSize { cols, rows },
            AttachStreams {
                bytes: client,
                announce_resize: Box::new(move |size| {
                    heard.lock().expect("announced").push(size);
                }),
            },
            shell_pid,
        )
        .expect("attach");
        Pair {
            host,
            session,
            guard,
            announced,
        }
    }

    #[test]
    fn an_attached_pane_has_no_descriptor_and_says_so() {
        // The two facts every caller downstream branches on. A replica that
        // reported a master would have `capture` read the wrong process's
        // foreground group through a descriptor it does not have.
        let pair = attached(20, 5, Some(4242));
        assert!(
            pair.session.master.is_none(),
            "the pseudoterminal belongs to the host"
        );
        assert_eq!(pair.session.shell_pid, Some(4242), "the host said which");

        let unknown = attached(20, 5, None);
        assert_eq!(
            unknown.session.shell_pid, None,
            "and when nobody said, nothing is claimed"
        );
    }

    #[test]
    fn the_hosts_output_lands_in_the_replica_grid() {
        let mut pair = attached(20, 5, Some(1));
        pair.host.write_all(b"hello\r\nworld").expect("host writes");
        let term = pair.session.term.clone();
        assert!(
            within(Duration::from_secs(5), || {
                term.lock().grid()[Line(1)][Column(4)].c == 'd'
            }),
            "the host's bytes never reached the replica"
        );
    }

    #[test]
    fn a_replica_never_answers_a_question_the_host_has_already_answered() {
        // A program asking the terminal what it is (DA) gets exactly one reply,
        // and it comes from the process that owns the pseudoterminal. If the
        // replica answered too, the second answer would arrive at the program
        // as characters the user appears to have typed — mid-screen, inside
        // vim. The generation must still move, because it is the invalidation
        // token every cached view is built on.
        let mut pair = attached(20, 5, Some(1));
        let before = pair.session.content_generation();
        pair.host.write_all(b"\x1b[c").expect("device attributes");

        assert!(
            within(Duration::from_secs(5), || pair
                .session
                .content_generation()
                > before),
            "the replica's generation must move even for an event it swallows"
        );
        // Nothing goes back up the socket. Read with a short timeout: the
        // absence of an answer is the assertion.
        pair.host
            .set_read_timeout(Some(Duration::from_millis(300)))
            .expect("timeout");
        let mut buf = [0u8; 64];
        match pair.host.read(&mut buf) {
            Ok(0) => {}
            Ok(n) => panic!(
                "the replica answered a query the host owns: {:?}",
                String::from_utf8_lossy(&buf[..n])
            ),
            Err(e) => assert!(
                matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ),
                "unexpected error waiting for silence: {e}"
            ),
        }
        // A local terminal, by contrast, forwards it for the pane to bounce.
        let (tx, mut rx) = unbounded();
        let generation = Arc::new(AtomicU64::new(0));
        let local = EventProxy::new(tx, generation, Answers::Here);
        local.send_event(TermEvent::PtyWrite("\x1b[?6c".into()));
        assert!(
            matches!(rx.try_next(), Ok(Some(TermEvent::PtyWrite(_)))),
            "a terminal we own must still forward its own answers"
        );
    }

    #[test]
    fn keystrokes_go_up_the_socket_and_a_resize_does_not() {
        // A size is a fact the owner is told, never one it infers from the byte
        // stream: more than one client may be looking, and a resize mixed into
        // the output is a resize somebody has to guess the end of.
        let mut pair = attached(20, 5, Some(1));
        use alacritty_terminal::event::Notify;
        pair.session.notifier.notify(b"ls -la\n".to_vec());
        pair.host
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
        let mut buf = [0u8; 32];
        let read = pair.host.read(&mut buf).expect("the host reads keystrokes");
        assert_eq!(&buf[..read], b"ls -la\n");

        pair.session.resize(GridSize { cols: 30, rows: 9 }, 7, 15);
        assert!(
            within(Duration::from_secs(5), || !pair
                .announced
                .lock()
                .expect("announced")
                .is_empty()),
            "the new size was never announced to the host"
        );
        let announced = pair.announced.lock().expect("announced");
        let last = announced.last().expect("one announcement");
        assert_eq!((last.num_cols, last.num_lines), (30, 9));
        assert_eq!((last.cell_width, last.cell_height), (7, 15));

        // and nothing about the size went up the byte stream
        pair.host
            .set_read_timeout(Some(Duration::from_millis(200)))
            .expect("timeout");
        let mut spill = [0u8; 64];
        match pair.host.read(&mut spill) {
            Ok(0) => {}
            Ok(n) => panic!(
                "a resize leaked into the byte stream: {:?}",
                String::from_utf8_lossy(&spill[..n])
            ),
            Err(_) => {}
        }
    }

    /// An independent terminal fed the same bytes — what a host's own grid
    /// would look like, without a host.
    fn authoritative(cols: usize, rows: usize, bytes: &[u8]) -> Term<EventProxy> {
        let (tx, _rx) = unbounded();
        let proxy = EventProxy::new(tx, Arc::new(AtomicU64::new(0)), Answers::Here);
        let size = GridSize { cols, rows };
        let mut term = Term::new(Config::default(), &size, proxy);
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, bytes);
        term
    }

    #[test]
    fn the_guard_agrees_when_the_two_terminals_have_seen_the_same_bytes() {
        let mut pair = attached(20, 5, Some(1));
        let bytes = b"one\r\ntwo\r\nthree\r\n\x1b[32mgreen\x1b[0m";
        pair.host.write_all(bytes).expect("host writes");
        assert!(
            within(Duration::from_secs(5), || pair.guard.consumed()
                == bytes.len() as u64),
            "the replica never consumed the stream: {} of {}",
            pair.guard.consumed(),
            bytes.len()
        );

        let host_term = authoritative(20, 5, bytes);
        let probe = GridCheck {
            pane: crate::hostproto::PaneId(1),
            stream_offset: bytes.len() as u64,
            hash: grid_hash(&host_term),
        };
        assert!(
            within(Duration::from_secs(5), || pair.guard.check(&probe)
                == GuardVerdict::Match),
            "two terminals fed the same bytes disagreed: {:?}",
            pair.guard.check(&probe)
        );

        // Being at a different point in the stream is not disagreement, in
        // either direction — and the guard says which, because "behind" and
        // "past it" are different facts about a client.
        let behind = GridCheck {
            stream_offset: probe.stream_offset + 64,
            ..probe
        };
        assert!(
            matches!(
                pair.guard.check(&behind),
                GuardVerdict::NotYet(Unsettled::Behind { .. })
            ),
            "a probe from further along the stream is not a mismatch"
        );
        let stale = GridCheck {
            stream_offset: probe.stream_offset - 1,
            ..probe
        };
        assert!(
            matches!(
                pair.guard.check(&stale),
                GuardVerdict::NotYet(Unsettled::Ahead { .. })
            ),
            "a probe the replica has already read past is stale, not wrong"
        );
    }

    #[test]
    fn the_guard_notices_a_terminal_that_has_actually_diverged() {
        // The failure it exists for: the same byte count, a different screen.
        // Without this the whole guard is decoration.
        let mut pair = attached(20, 5, Some(1));
        let bytes = b"the quick brown fox";
        pair.host.write_all(bytes).expect("host writes");
        assert!(within(Duration::from_secs(5), || pair.guard.consumed()
            == bytes.len() as u64));

        let host_term = authoritative(20, 5, b"the quick brown FOX");
        let verdict = pair.guard.check(&GridCheck {
            pane: crate::hostproto::PaneId(1),
            stream_offset: bytes.len() as u64,
            hash: grid_hash(&host_term),
        });
        assert!(
            matches!(verdict, GuardVerdict::Mismatch { .. }),
            "one different word must read as divergence, got {verdict:?}"
        );
    }

    #[test]
    fn scrolling_the_replica_is_not_divergence() {
        // A person reading their scrollback is not a client that has gone
        // wrong. The hash covers the terminal; the view belongs to the viewer.
        use alacritty_terminal::grid::Scroll;
        let mut pair = attached(20, 3, Some(1));
        let bytes = b"a\r\nb\r\nc\r\nd\r\ne\r\nf\r\n";
        pair.host.write_all(bytes).expect("host writes");
        assert!(within(Duration::from_secs(5), || pair.guard.consumed()
            == bytes.len() as u64));
        let probe = GridCheck {
            pane: crate::hostproto::PaneId(1),
            stream_offset: bytes.len() as u64,
            hash: grid_hash(&authoritative(20, 3, bytes)),
        };
        assert!(within(Duration::from_secs(5), || pair.guard.check(&probe)
            == GuardVerdict::Match));

        pair.session.term.lock().scroll_display(Scroll::PageUp);
        assert_eq!(
            pair.guard.check(&probe),
            GuardVerdict::Match,
            "a scrolled-back reader must not read as a diverged client"
        );
    }
}

/// Headless, deterministic terminal-correctness matrix.
///
/// These tests drive the emulator the way the real I/O thread does
/// (`event_loop.rs:154` calls `parser.advance(&mut **terminal, bytes)`), but
/// SYNCHRONOUSLY and with NO PTY, NO child shell, and NO sleeps: we build a
/// bare [`Term`] + a `vte::ansi::Processor`, feed raw control bytes with
/// `processor.advance(&mut term, bytes)`, then assert on the resulting grid /
/// mode / emitted events. That makes them fast and CI-stable (a real
/// PTY+shell+sleep loop is the flaky pattern we deliberately avoid).
///
/// Grid/cell access mirrors the production read path in `pane.rs`
/// (`grid[Line(y)][Column(x)].{c,flags,fg}`, `term.mode().contains(...)`).
#[cfg(test)]
mod correctness {
    use std::sync::{Arc, Mutex};

    use alacritty_terminal::event::{Event as TermEvent, EventListener};
    use alacritty_terminal::grid::{Dimensions, Scroll};
    use alacritty_terminal::index::{Column, Line};
    use alacritty_terminal::term::cell::Flags;
    use alacritty_terminal::term::{ClipboardType, Config, Term, TermMode};
    use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor};

    use super::GridSize;

    /// Records every event the emulator emits, so OSC-driven side effects
    /// (clipboard stores from OSC 52, title changes, etc.) can be asserted on.
    /// Cheap, synchronous, and `Send + Sync` — no channel, no thread.
    #[derive(Clone, Default)]
    struct Recorder(Arc<Mutex<Vec<TermEvent>>>);

    impl Recorder {
        fn events(&self) -> Vec<TermEvent> {
            self.0.lock().unwrap().clone()
        }
    }

    impl EventListener for Recorder {
        fn send_event(&self, event: TermEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    /// A fresh emulator + parser + event recorder at the given size.
    fn harness(cols: usize, rows: usize) -> (Term<Recorder>, Processor, Recorder) {
        let size = GridSize { cols, rows };
        let rec = Recorder::default();
        let term = Term::new(Config::default(), &size, rec.clone());
        (term, Processor::new(), rec)
    }

    /// 80x24 default — the common case.
    fn term80() -> (Term<Recorder>, Processor, Recorder) {
        harness(80, 24)
    }

    /// Feed bytes into the emulator exactly as the real reader thread does.
    fn feed(term: &mut Term<Recorder>, p: &mut Processor, bytes: &[u8]) {
        p.advance(term, bytes);
    }

    /// The `char` at a live-screen cell (row/col, 0-based from the top).
    fn ch(term: &Term<Recorder>, row: i32, col: usize) -> char {
        term.grid()[Line(row)][Column(col)].c
    }

    /// The FOCUS reader's vertical fill rests on one assumption: that rows which
    /// have scrolled off the screen are still reachable through the grid at
    /// NEGATIVE line indices. Everything above the reader — `budget_range`,
    /// `grid_rows_in`, `PaneSource` — is built on that, and nothing else in the
    /// suite touches it. If alacritty ever changed how history is addressed, the
    /// reader would silently fall back to showing one screenful and no test would
    /// notice, which is precisely the bug the document model was built to end.
    #[test]
    fn scrolled_off_rows_stay_readable_at_negative_line_indices() {
        let (mut term, mut p, _) = harness(20, 5);
        // 12 numbered lines through a 5-row screen: 7 must scroll into history
        for i in 0..12 {
            feed(&mut term, &mut p, format!("line{i}\r\n").as_bytes());
        }

        let top = term.grid().topmost_line().0;
        assert!(top < 0, "history must extend above line 0, got {top}");
        // 12 written lines plus the blank row the last newline opens = 13 rows
        // through a 5-row screen, so 8 are displaced into history.
        assert_eq!(top, -8, "history depth");
        assert_eq!(term.grid().history_size(), 8);

        let row_text = |l: i32| -> String {
            (0..20)
                .map(|c| term.grid()[Line(l)][Column(c)].c)
                .collect::<String>()
                .trim_end()
                .to_string()
        };
        // the very first line written is still readable at the top of history
        assert_eq!(row_text(top), "line0", "the first line survives in history");
        assert_eq!(row_text(-1), "line7", "the row just above the screen");
        // line 0 remains the top of the VISIBLE screen, not the top of history —
        // the distinction `budget_range` depends on
        assert_eq!(row_text(0), "line8");
    }

    /// The flags at a live-screen cell.
    fn flags(term: &Term<Recorder>, row: i32, col: usize) -> Flags {
        term.grid()[Line(row)][Column(col)].flags
    }

    /// Read the live top row as a string, skipping wide-char spacers (the same
    /// thing `pane.rs::live_rows` does for the HUD).
    fn row_text(term: &Term<Recorder>, row: i32) -> String {
        let grid = term.grid();
        let cols = grid.columns();
        let mut s = String::new();
        for c in 0..cols {
            let cell = &grid[Line(row)][Column(c)];
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            s.push(if cell.c == '\0' { ' ' } else { cell.c });
        }
        s.trim_end().to_string()
    }

    // ----------------------------------------------------------------------
    // Sanity: the harness itself behaves.
    // ----------------------------------------------------------------------

    #[test]
    fn fresh_term_has_expected_dimensions() {
        let (term, ..) = term80();
        assert_eq!(term.grid().columns(), 80);
        assert_eq!(term.grid().screen_lines(), 24);
    }

    #[test]
    fn plain_ascii_lands_on_the_grid() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"hello");
        assert_eq!(row_text(&term, 0), "hello");
        assert_eq!(ch(&term, 0, 0), 'h');
        assert_eq!(ch(&term, 0, 4), 'o');
    }

    #[test]
    fn newline_and_carriage_return_move_the_cursor() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"ab\r\ncd");
        assert_eq!(row_text(&term, 0), "ab");
        assert_eq!(row_text(&term, 1), "cd");
    }

    #[test]
    fn fresh_term_starts_with_no_history() {
        let (term, ..) = term80();
        // total_lines == screen_lines until the screen overflows into history.
        assert_eq!(term.grid().total_lines(), term.grid().screen_lines());
        assert_eq!(term.grid().display_offset(), 0);
    }

    // ----------------------------------------------------------------------
    // (a) Wide-char width — CJK occupies WIDE_CHAR + a WIDE_CHAR_SPACER.
    // ----------------------------------------------------------------------

    #[test]
    fn cjk_sets_wide_char_flag() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, "你".as_bytes());
        assert!(flags(&term, 0, 0).contains(Flags::WIDE_CHAR));
        assert_eq!(ch(&term, 0, 0), '你');
    }

    #[test]
    fn cjk_occupies_a_spacer_cell() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, "你".as_bytes());
        // The cell immediately right of a wide char is a spacer (no glyph).
        assert!(flags(&term, 0, 1).contains(Flags::WIDE_CHAR_SPACER));
    }

    #[test]
    fn cjk_advances_cursor_by_two_columns() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, "你A".as_bytes());
        // 你 at col0(+spacer col1), 'A' lands at col2.
        assert_eq!(ch(&term, 0, 2), 'A');
        assert!(!flags(&term, 0, 2).contains(Flags::WIDE_CHAR));
    }

    #[test]
    fn multiple_cjk_pack_two_columns_each() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, "日本語".as_bytes());
        assert_eq!(ch(&term, 0, 0), '日');
        assert!(flags(&term, 0, 1).contains(Flags::WIDE_CHAR_SPACER));
        assert_eq!(ch(&term, 0, 2), '本');
        assert!(flags(&term, 0, 3).contains(Flags::WIDE_CHAR_SPACER));
        assert_eq!(ch(&term, 0, 4), '語');
        assert_eq!(row_text(&term, 0), "日本語");
    }

    #[test]
    fn ascii_is_not_wide() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"A");
        assert!(!flags(&term, 0, 0).contains(Flags::WIDE_CHAR));
        assert!(!flags(&term, 0, 1).contains(Flags::WIDE_CHAR_SPACER));
    }

    #[test]
    fn wide_char_at_last_column_wraps() {
        // A 2-wide glyph cannot start in the final column of an odd-width grid;
        // alacritty marks the trailing cell LEADING_WIDE_CHAR_SPACER and wraps
        // the glyph to the next line. Use a 3-col grid so col2 is the last.
        let (mut term, mut p, _) = harness(3, 4);
        feed(&mut term, &mut p, b"AB"); // fill col0,col1; cursor at col2 (last)
        feed(&mut term, &mut p, "你".as_bytes());
        // The wide char could not fit at col2, so it wrapped to row1 col0.
        assert!(
            flags(&term, 2 - 2, 2).contains(Flags::LEADING_WIDE_CHAR_SPACER)
                || ch(&term, 1, 0) == '你'
        );
        assert_eq!(ch(&term, 1, 0), '你');
    }

    // ----------------------------------------------------------------------
    // (b) Emoji / ZWJ width.
    // ----------------------------------------------------------------------

    #[test]
    fn emoji_is_wide() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, "😀".as_bytes());
        assert!(flags(&term, 0, 0).contains(Flags::WIDE_CHAR));
        assert!(flags(&term, 0, 1).contains(Flags::WIDE_CHAR_SPACER));
        assert_eq!(ch(&term, 0, 0), '😀');
    }

    #[test]
    fn text_after_emoji_lands_two_columns_over() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, "😀X".as_bytes());
        assert_eq!(ch(&term, 0, 2), 'X');
    }

    #[test]
    fn zwj_family_emoji_collapses_into_the_base_cell() {
        // 👨‍👩‍👧 is base + ZWJ + emoji + ZWJ + emoji. vte appends the
        // zero-width joiners/combining members onto the base cell as `extra`
        // rather than consuming extra columns: the base cell stays WIDE_CHAR
        // and the visible advance is still 2 columns, with the trailing text
        // landing right after the spacer.
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, "👨‍👩‍👧!".as_bytes());
        assert!(flags(&term, 0, 0).contains(Flags::WIDE_CHAR));
        assert!(flags(&term, 0, 1).contains(Flags::WIDE_CHAR_SPACER));
        // Trailing '!' is the first non-spacer after the cluster.
        let txt = row_text(&term, 0);
        assert!(txt.ends_with('!'), "row was {txt:?}");
    }

    #[test]
    fn combining_accent_does_not_consume_a_column() {
        // 'e' + U+0301 (combining acute) is one grid cell, not two.
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, "e\u{0301}X".as_bytes());
        // 'X' must be at col1: the accent attached to the 'e' cell.
        assert_eq!(ch(&term, 0, 1), 'X');
    }

    // ----------------------------------------------------------------------
    // (c) Alt-screen — DECSET/DECRST 1049.
    // ----------------------------------------------------------------------

    #[test]
    fn alt_screen_off_by_default() {
        let (term, ..) = term80();
        assert!(!term.mode().contains(TermMode::ALT_SCREEN));
    }

    #[test]
    fn decset_1049_enters_alt_screen() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1049h");
        assert!(term.mode().contains(TermMode::ALT_SCREEN));
    }

    #[test]
    fn decrst_1049_leaves_alt_screen() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1049h");
        assert!(term.mode().contains(TermMode::ALT_SCREEN));
        feed(&mut term, &mut p, b"\x1b[?1049l");
        assert!(!term.mode().contains(TermMode::ALT_SCREEN));
    }

    #[test]
    fn legacy_1047_alt_screen_is_unknown_in_this_version() {
        // DOCUMENTED GAP: alacritty_terminal 0.26 / vte 0.15 only map DEC 1049
        // (SwapScreenAndSetRestoreCursor) to alt-screen — the older 1047 (and
        // 1048 cursor save/restore) are unrecognised private modes and are
        // ignored, so 1047 does NOT toggle ALT_SCREEN here. Apps that emit 1049
        // (vim, less, tmux, fzf, $PAGER) are covered by the tests above; this
        // assertion pins the no-op so a future crate bump that adds 1047 support
        // turns this red and prompts a real toggle assertion.
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1047h");
        assert!(!term.mode().contains(TermMode::ALT_SCREEN));
    }

    #[test]
    fn alt_screen_content_is_isolated_from_primary() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"primary");
        feed(&mut term, &mut p, b"\x1b[?1049h");
        // Alt screen starts blank (no primary content bleeds through).
        assert_eq!(row_text(&term, 0), "");
        // 1049 SAVES the cursor, so it is still at the column after "primary".
        // Real full-screen apps home the cursor before drawing; do the same so
        // the written text is deterministic regardless of the saved column.
        feed(&mut term, &mut p, b"\x1b[H");
        feed(&mut term, &mut p, b"alt");
        assert_eq!(row_text(&term, 0), "alt");
        feed(&mut term, &mut p, b"\x1b[?1049l");
        // Primary content restored on exit.
        assert_eq!(row_text(&term, 0), "primary");
    }

    // ----------------------------------------------------------------------
    // (d) Mouse modes — each DECSET sets the matching TermMode flag.
    // ----------------------------------------------------------------------

    #[test]
    fn mouse_click_mode_1000() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1000h");
        assert!(term.mode().contains(TermMode::MOUSE_REPORT_CLICK));
        feed(&mut term, &mut p, b"\x1b[?1000l");
        assert!(!term.mode().contains(TermMode::MOUSE_REPORT_CLICK));
    }

    #[test]
    fn mouse_drag_mode_1002() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1002h");
        assert!(term.mode().contains(TermMode::MOUSE_DRAG));
    }

    #[test]
    fn mouse_motion_mode_1003() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1003h");
        assert!(term.mode().contains(TermMode::MOUSE_MOTION));
    }

    #[test]
    fn sgr_mouse_mode_1006() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1006h");
        assert!(term.mode().contains(TermMode::SGR_MOUSE));
        feed(&mut term, &mut p, b"\x1b[?1006l");
        assert!(!term.mode().contains(TermMode::SGR_MOUSE));
    }

    #[test]
    fn utf8_mouse_mode_1005() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1005h");
        assert!(term.mode().contains(TermMode::UTF8_MOUSE));
    }

    #[test]
    fn focus_event_mode_1004() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1004h");
        assert!(term.mode().contains(TermMode::FOCUS_IN_OUT));
        feed(&mut term, &mut p, b"\x1b[?1004l");
        assert!(!term.mode().contains(TermMode::FOCUS_IN_OUT));
    }

    #[test]
    fn alternate_scroll_mode_1007() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1007h");
        assert!(term.mode().contains(TermMode::ALTERNATE_SCROLL));
    }

    // ----------------------------------------------------------------------
    // (e) Bracketed paste — DECSET 2004.
    // ----------------------------------------------------------------------

    #[test]
    fn bracketed_paste_off_by_default() {
        let (term, ..) = term80();
        assert!(!term.mode().contains(TermMode::BRACKETED_PASTE));
    }

    #[test]
    fn bracketed_paste_toggles() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?2004h");
        assert!(term.mode().contains(TermMode::BRACKETED_PASTE));
        feed(&mut term, &mut p, b"\x1b[?2004l");
        assert!(!term.mode().contains(TermMode::BRACKETED_PASTE));
    }

    // ----------------------------------------------------------------------
    // Other DEC private modes we read elsewhere in the app.
    // ----------------------------------------------------------------------

    #[test]
    fn app_cursor_keys_mode_1() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1h");
        assert!(term.mode().contains(TermMode::APP_CURSOR));
        feed(&mut term, &mut p, b"\x1b[?1l");
        assert!(!term.mode().contains(TermMode::APP_CURSOR));
    }

    #[test]
    fn show_cursor_mode_25() {
        let (mut term, mut p, _) = term80();
        // SHOW_CURSOR is set by default; ?25l hides it.
        assert!(term.mode().contains(TermMode::SHOW_CURSOR));
        feed(&mut term, &mut p, b"\x1b[?25l");
        assert!(!term.mode().contains(TermMode::SHOW_CURSOR));
        feed(&mut term, &mut p, b"\x1b[?25h");
        assert!(term.mode().contains(TermMode::SHOW_CURSOR));
    }

    #[test]
    fn line_wrap_mode_7() {
        let (mut term, mut p, _) = term80();
        // LINE_WRAP (autowrap, DECAWM) is on by default.
        assert!(term.mode().contains(TermMode::LINE_WRAP));
        feed(&mut term, &mut p, b"\x1b[?7l");
        assert!(!term.mode().contains(TermMode::LINE_WRAP));
    }

    // ----------------------------------------------------------------------
    // (f) Scrollback — overflow grows history; scroll_display moves the offset.
    // ----------------------------------------------------------------------

    #[test]
    fn overflow_grows_history() {
        let (mut term, mut p, _) = harness(20, 4); // 4 visible rows
        let before = term.grid().total_lines();
        assert_eq!(before, 4);
        // Print 10 lines: 6 must spill into scrollback history.
        for i in 0..10 {
            feed(&mut term, &mut p, format!("L{i}\r\n").as_bytes());
        }
        let after = term.grid().total_lines();
        assert!(after > before, "history did not grow: {before} -> {after}");
        // history = total - screen.
        assert_eq!(after - term.grid().screen_lines(), after - 4);
        assert!(
            after - 4 >= 6,
            "expected >=6 history rows, got {}",
            after - 4
        );
    }

    #[test]
    fn display_offset_zero_at_bottom() {
        let (mut term, mut p, _) = harness(20, 4);
        for i in 0..10 {
            feed(&mut term, &mut p, format!("L{i}\r\n").as_bytes());
        }
        // Not scrolled back yet — viewport pinned to the live bottom.
        assert_eq!(term.grid().display_offset(), 0);
    }

    #[test]
    fn scroll_display_moves_offset_into_history() {
        let (mut term, mut p, _) = harness(20, 4);
        for i in 0..10 {
            feed(&mut term, &mut p, format!("L{i}\r\n").as_bytes());
        }
        term.scroll_display(Scroll::Delta(3));
        assert_eq!(term.grid().display_offset(), 3);
        term.scroll_display(Scroll::Top);
        assert!(term.grid().display_offset() > 0);
        term.scroll_display(Scroll::Bottom);
        assert_eq!(term.grid().display_offset(), 0);
    }

    #[test]
    fn scrolled_back_content_is_a_prior_line() {
        let (mut term, mut p, _) = harness(20, 6);
        for i in 0..20 {
            feed(&mut term, &mut p, format!("line{i}\r\n").as_bytes());
        }
        term.scroll_display(Scroll::Top);
        // The very top of history is the oldest retained line.
        let top = row_text(&term, 0);
        assert!(top.starts_with("line"), "top of scrollback was {top:?}");
    }

    // ----------------------------------------------------------------------
    // (g) ANSI 16-colour cell fg.
    // ----------------------------------------------------------------------

    fn fg(term: &Term<Recorder>, row: i32, col: usize) -> Color {
        term.grid()[Line(row)][Column(col)].fg
    }

    #[test]
    fn sgr_31_sets_named_red_fg() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[31mR");
        assert_eq!(fg(&term, 0, 0), Color::Named(NamedColor::Red));
    }

    #[test]
    fn sgr_32_sets_named_green_fg() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[32mG");
        assert_eq!(fg(&term, 0, 0), Color::Named(NamedColor::Green));
    }

    #[test]
    fn sgr_34_sets_named_blue_fg() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[34mB");
        assert_eq!(fg(&term, 0, 0), Color::Named(NamedColor::Blue));
    }

    #[test]
    fn sgr_91_sets_bright_red_fg() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[91mr");
        assert_eq!(fg(&term, 0, 0), Color::Named(NamedColor::BrightRed));
    }

    #[test]
    fn sgr_0_resets_fg_to_default() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[31mR\x1b[0mD");
        assert_eq!(fg(&term, 0, 0), Color::Named(NamedColor::Red));
        assert_eq!(fg(&term, 0, 1), Color::Named(NamedColor::Foreground));
    }

    #[test]
    fn sgr_256_indexed_fg() {
        let (mut term, mut p, _) = term80();
        // 38;5;208 = indexed orange.
        feed(&mut term, &mut p, b"\x1b[38;5;208mX");
        assert_eq!(fg(&term, 0, 0), Color::Indexed(208));
    }

    #[test]
    fn sgr_truecolor_fg() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[38;2;10;20;30mX");
        match fg(&term, 0, 0) {
            Color::Spec(rgb) => {
                assert_eq!((rgb.r, rgb.g, rgb.b), (10, 20, 30));
            }
            other => panic!("expected truecolor spec, got {other:?}"),
        }
    }

    #[test]
    fn sgr_bold_sets_bold_flag() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[1mX");
        assert!(flags(&term, 0, 0).contains(Flags::BOLD));
    }

    #[test]
    fn sgr_underline_sets_underline_flag() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[4mX");
        assert!(flags(&term, 0, 0).contains(Flags::UNDERLINE));
    }

    #[test]
    fn sgr_inverse_sets_inverse_flag() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[7mX");
        assert!(flags(&term, 0, 0).contains(Flags::INVERSE));
    }

    // ----------------------------------------------------------------------
    // (h) OSC 8 hyperlinks — uri (and id) attach to the painted cells.
    // ----------------------------------------------------------------------

    #[test]
    fn osc8_hyperlink_attaches_uri_to_cells() {
        let (mut term, mut p, _) = term80();
        // OSC 8 ; ; URI ST  text  OSC 8 ; ; ST   (BEL-terminated form)
        feed(
            &mut term,
            &mut p,
            b"\x1b]8;;https://example.com\x07link\x1b]8;;\x07",
        );
        let cell = &term.grid()[Line(0)][Column(0)];
        let hl = cell.hyperlink().expect("cell 0 should carry a hyperlink");
        assert_eq!(hl.uri(), "https://example.com");
        // 'l' of "link" is the painted glyph.
        assert_eq!(ch(&term, 0, 0), 'l');
    }

    #[test]
    fn osc8_hyperlink_with_id_param() {
        let (mut term, mut p, _) = term80();
        feed(
            &mut term,
            &mut p,
            b"\x1b]8;id=42;https://rust-lang.org\x07rust\x1b]8;;\x07",
        );
        let hl = term.grid()[Line(0)][Column(0)]
            .hyperlink()
            .expect("hyperlink present");
        assert_eq!(hl.uri(), "https://rust-lang.org");
        // alacritty's Hyperlink::id() returns &str; the explicit id is preserved.
        assert_eq!(hl.id(), "42");
    }

    #[test]
    fn osc8_close_stops_attaching_links() {
        let (mut term, mut p, _) = term80();
        feed(
            &mut term,
            &mut p,
            b"\x1b]8;;https://a.test\x07A\x1b]8;;\x07B",
        );
        assert!(term.grid()[Line(0)][Column(0)].hyperlink().is_some());
        // 'B' is printed after the link was closed — no hyperlink.
        assert!(term.grid()[Line(0)][Column(1)].hyperlink().is_none());
    }

    #[test]
    fn cells_without_osc8_have_no_hyperlink() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"plain");
        assert!(term.grid()[Line(0)][Column(0)].hyperlink().is_none());
    }

    // ----------------------------------------------------------------------
    // (i) OSC 52 — clipboard store. Default Config osc52 == OnlyCopy, so a
    //     copy ('c') sequence emits Event::ClipboardStore with the decoded text.
    // ----------------------------------------------------------------------

    #[test]
    fn osc52_copy_emits_clipboard_store() {
        let (mut term, mut p, rec) = term80();
        // base64("hi") == "aGk=".
        feed(&mut term, &mut p, b"\x1b]52;c;aGk=\x07");
        let stored: Vec<_> = rec
            .events()
            .into_iter()
            .filter_map(|e| match e {
                TermEvent::ClipboardStore(ClipboardType::Clipboard, s) => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(stored, vec!["hi".to_string()]);
    }

    #[test]
    fn osc52_decodes_longer_payload() {
        let (mut term, mut p, rec) = term80();
        // base64("terminal-delight") == "dGVybWluYWwtZGVsaWdodA==".
        feed(&mut term, &mut p, b"\x1b]52;c;dGVybWluYWwtZGVsaWdodA==\x07");
        let stored: Vec<_> = rec
            .events()
            .into_iter()
            .filter_map(|e| match e {
                TermEvent::ClipboardStore(_, s) => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(stored, vec!["terminal-delight".to_string()]);
    }

    #[test]
    fn osc52_invalid_base64_emits_nothing() {
        let (mut term, mut p, rec) = term80();
        feed(&mut term, &mut p, b"\x1b]52;c;not-valid-base64!!!\x07");
        let any_store = rec
            .events()
            .into_iter()
            .any(|e| matches!(e, TermEvent::ClipboardStore(..)));
        assert!(!any_store, "invalid base64 must not store to clipboard");
    }

    // ----------------------------------------------------------------------
    // Misc OSC: title / colour — exercise the event surface used by the app.
    // ----------------------------------------------------------------------

    #[test]
    fn osc0_sets_window_title() {
        let (mut term, mut p, rec) = term80();
        feed(&mut term, &mut p, b"\x1b]0;my-title\x07");
        let titled = rec
            .events()
            .into_iter()
            .any(|e| matches!(e, TermEvent::Title(t) if t == "my-title"));
        assert!(titled, "OSC 0 should emit a Title event");
    }

    // ----------------------------------------------------------------------
    // Cursor movement / erase — the geometry the warp hit-test relies on.
    // ----------------------------------------------------------------------

    #[test]
    fn cup_positions_the_cursor() {
        let (mut term, mut p, _) = term80();
        // CUP row3 col5 (1-based) then write — lands at grid (2,4).
        feed(&mut term, &mut p, b"\x1b[3;5fZ");
        assert_eq!(ch(&term, 2, 4), 'Z');
    }

    #[test]
    fn ed_clears_the_screen() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"dirty\r\nmore");
        feed(&mut term, &mut p, b"\x1b[2J");
        assert_eq!(row_text(&term, 0), "");
        assert_eq!(row_text(&term, 1), "");
    }

    #[test]
    fn el_clears_to_end_of_line() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"keepXXXX");
        // Move cursor back to col4 and erase to EOL.
        feed(&mut term, &mut p, b"\x1b[1;5H\x1b[K");
        assert_eq!(row_text(&term, 0), "keep");
    }

    #[test]
    fn backspace_moves_left_without_erasing() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"ab\x08c");
        // 'c' overwrites 'b'.
        assert_eq!(row_text(&term, 0), "ac");
    }

    #[test]
    fn tab_advances_to_next_stop() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"a\tb");
        assert_eq!(ch(&term, 0, 0), 'a');
        // Default tab stops every 8 columns → 'b' at col8.
        assert_eq!(ch(&term, 0, 8), 'b');
    }

    #[test]
    fn autowrap_pushes_overflow_to_next_row() {
        let (mut term, mut p, _) = harness(4, 4);
        feed(&mut term, &mut p, b"abcdef");
        assert_eq!(row_text(&term, 0), "abcd");
        assert_eq!(row_text(&term, 1), "ef");
    }
}
