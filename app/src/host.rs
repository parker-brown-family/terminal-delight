//! The process that owns the terminals.
//!
//! A session host holds the pseudoterminals, the shells and agents running
//! inside them, and the authoritative emulator state for each — everything
//! that today dies when a window closes. Windows become clients: they attach,
//! they draw, and they are allowed to die without taking the work with them.
//!
//! It carries no user interface and never links one. That is not tidiness: it
//! is what lets the thing that must not die be a small program that does one
//! job, while the thing that dies often stays free to change.
//!
//! ## The handover, which is the only hard part
//!
//! A client attaching has missed everything printed so far, so it is sent a
//! snapshot of the grid and then the live byte stream. Between those two there
//! is a seam, and a byte that falls in it is either lost or drawn twice.
//!
//! The fence closes it. Alacritty's reader holds a *lease* on the terminal for
//! its whole cycle — taken before it reads, released after everything it read
//! has been parsed — so a lease taken here cannot overlap one. Holding it, the
//! snapshot is taken and the client's stream is installed together, and every
//! byte falls on exactly one side: read before, and therefore already in the
//! snapshot; or read after, and therefore sent live.
//!
//! One detail the plan for this got wrong, and the compiler would not have
//! caught: the lease and the data lock are two different mutexes, and
//! `FairMutex::lock` takes *both*. Holding a lease and then calling `lock`
//! deadlocks against yourself. The unfair lock is the one to pair with a
//! lease, which is exactly what alacritty's own reader does.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Sender, SyncSender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event as TermEvent, EventListener, OnResize, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Msg, Notifier};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::tty::{self, ChildEvent, EventedPty, EventedReadWrite, Pty};
use polling::{Event, PollMode, Poller};

use crate::gridwire;
use crate::hostproto::{
    host_socket_path, parse_stream_greeting, ClientKind, ClosedPane, GridCheck, Outcome, PaneGeom,
    PaneId, PaneInfo, Persisted, Push, Reply, Request, WireMode, ENV_PANE_ID, ENV_SESSION,
    LAYOUT_SCHEMA, PROTO_VERSION,
};
use crate::session::PaneRuntime;
use crate::term::GridSize;

/// How many chunks may queue for a client before it is considered gone.
///
/// A client that cannot keep up must not become the terminal's problem: the
/// PTY reader shares this thread with every other pane, so back-pressure here
/// would be a slow window stalling fast ones. Past this depth the stream is
/// dropped, the client sees its socket close, and it re-attaches — which costs
/// a snapshot and is always correct, because a snapshot is the truth.
const SINK_DEPTH: usize = 512;

/// Somewhere a pane's output goes: one attached client's byte stream.
///
/// Owns a thread rather than writing inline, for the same reason as the depth
/// limit — a socket write that blocks would block the emulator.
struct Sink {
    chunks: SyncSender<Vec<u8>>,
    /// Bytes handed to this client, the opening snapshot included.
    ///
    /// The client counts what it reads off the socket; this counts what was
    /// written to it, and a divergence check is meaningless unless the two are
    /// counting the same bytes. Incremented only after the queue accepts a
    /// chunk, so a chunk dropped for a client that fell behind is never
    /// claimed as sent.
    enqueued: AtomicU64,
    /// Which attachment this is.
    ///
    /// A superseded client is still reading its socket when its successor
    /// takes over, and it will notice the takeover a moment later. Without a
    /// serial it cannot tell "my stream ended" from "the pane has no stream",
    /// so on the way out it tidies away whatever it finds — which by then is
    /// the new window's. That bug is invisible to every test that attaches
    /// once, and presents as a terminal going dead seconds after a reattach.
    serial: u64,
}

impl Sink {
    fn new(stream: UnixStream, serial: u64) -> Self {
        let (chunks, queue) = sync_channel::<Vec<u8>>(SINK_DEPTH);
        std::thread::spawn(move || {
            let mut stream = stream;
            for chunk in queue {
                if stream.write_all(&chunk).is_err() {
                    break;
                }
            }
            // Dropping the stream closes it, which is how the client learns
            // that it is no longer attached.
            let _ = stream.shutdown(std::net::Shutdown::Both);
        });
        Self {
            chunks,
            enqueued: AtomicU64::new(0),
            serial,
        }
    }

    /// Queue bytes. `false` means this client is gone or too far behind, and
    /// the caller should drop it.
    fn send(&self, bytes: &[u8]) -> bool {
        if self.chunks.try_send(bytes.to_vec()).is_err() {
            return false;
        }
        self.enqueued
            .fetch_add(bytes.len() as u64, Ordering::SeqCst);
        true
    }

    /// How far down this client's stream we are.
    fn enqueued(&self) -> u64 {
        self.enqueued.load(Ordering::SeqCst)
    }
}

/// A pane's output, on its way to the emulator, copied to whoever is watching.
///
/// The copy happens *inside* the read, on the reader thread, while the lease
/// is held — which is what makes the attach fence work. Tee somewhere else and
/// the ordering guarantee evaporates.
struct TeeReader {
    master: File,
    sink: Arc<Mutex<Option<Sink>>>,
    /// Set whenever this pane produces anything, and cleared by whoever asks.
    ///
    /// A flag rather than a timestamp, and deliberately: this runs on the
    /// reader thread for every chunk of every pane, and asking the clock there
    /// would put a syscall in the path a keystroke's echo takes. Whether
    /// anything was said since the last look is all a twelve-hour idle needs to
    /// know.
    spoke: Arc<AtomicBool>,
}

impl Read for TeeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.master.read(buf)?;
        if read > 0 {
            self.spoke.store(true, Ordering::Relaxed);
            let mut held = self.sink.lock().expect("sink lock");
            if let Some(sink) = held.as_ref() {
                if !sink.send(&buf[..read]) {
                    // Gone or hopelessly behind: drop it here rather than
                    // letting the queue grow. The client re-attaches.
                    *held = None;
                }
            }
        }
        Ok(read)
    }
}

/// A real pseudoterminal whose output is also copied to an attached client.
///
/// Registration, resizing and child-exit stay with the real one — the host has
/// an actual kernel object and an actual child, and neither is simulated here.
/// Only reading is wrapped.
struct TeePty {
    inner: Pty,
    reader: TeeReader,
}

impl TeePty {
    fn new(inner: Pty, sink: Arc<Mutex<Option<Sink>>>, spoke: Arc<AtomicBool>) -> io::Result<Self> {
        // A second descriptor onto the same open file: readiness is reported
        // on the one the poller holds, reads happen on this one, and because
        // they share a description the two always agree.
        let master = inner.file().try_clone()?;
        Ok(Self {
            inner,
            reader: TeeReader {
                master,
                sink,
                spoke,
            },
        })
    }
}

impl EventedReadWrite for TeePty {
    type Reader = TeeReader;
    type Writer = <Pty as EventedReadWrite>::Writer;

    unsafe fn register(
        &mut self,
        poll: &Arc<Poller>,
        interest: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        unsafe { self.inner.register(poll, interest, mode) }
    }

    fn reregister(
        &mut self,
        poll: &Arc<Poller>,
        interest: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        self.inner.reregister(poll, interest, mode)
    }

    fn deregister(&mut self, poll: &Arc<Poller>) -> io::Result<()> {
        self.inner.deregister(poll)
    }

    fn reader(&mut self) -> &mut Self::Reader {
        &mut self.reader
    }

    fn writer(&mut self) -> &mut Self::Writer {
        self.inner.writer()
    }
}

impl EventedPty for TeePty {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        self.inner.next_child_event()
    }
}

impl OnResize for TeePty {
    fn on_resize(&mut self, window_size: WindowSize) {
        self.inner.on_resize(window_size);
    }
}

/// Ships terminal events off the reader thread. The host answers some of them
/// (a program asking what the terminal is gets an answer) and records others.
#[derive(Clone)]
struct HostProxy(Sender<TermEvent>);

impl EventListener for HostProxy {
    fn send_event(&self, event: TermEvent) {
        let _ = self.0.send(event);
    }
}

/// One live terminal.
struct HostPane {
    term: Arc<FairMutex<Term<HostProxy>>>,
    /// Where keystrokes go.
    input: Notifier,
    sink: Arc<Mutex<Option<Sink>>>,
    shell_pid: u32,
    /// Our own handle on the pseudoterminal master, never read from and never
    /// written to. It exists for the one question only the process holding a
    /// master may ask — which process group is in the foreground — which is
    /// why the watcher and the checkpoint had to move here with the terminals.
    master: File,
    geom: Mutex<PaneGeom>,
    ended: Arc<AtomicBool>,
    /// What the last checkpoint read: where the pane is, and what would put
    /// its agent back. Seeded with the spawn request, then replaced by
    /// readings.
    runtime: Mutex<PaneRuntime>,
    /// What the last watcher tick saw in the foreground, and `None` until one
    /// has.
    mode: Mutex<Option<WireMode>>,
    /// Whether the last watcher tick found the alternate screen up.
    ///
    /// Kept because the interesting thing is the *transition*. A client that
    /// attached while a full-screen program was running was sent that program's
    /// screen and nothing else — the alternate grid is the only one a snapshot
    /// can read — so its scrollback starts empty behind it, and the moment the
    /// program exits is the moment that has to be repaired.
    on_alt: AtomicBool,
}

/// What a spawn did.
pub struct Spawn {
    pub info: PaneInfo,
    /// Whether a terminal was actually started, or this session was already
    /// running the recipe it was asked for.
    pub started: bool,
}

impl HostPane {
    /// Whether this pane is the one already running `recipe`.
    ///
    /// A pane whose child has gone is not running anything, whatever it was
    /// started to run — its leaf wants a fresh terminal, not a binding to a
    /// corpse.
    fn runs(&self, recipe: &str) -> bool {
        !self.ended.load(Ordering::SeqCst)
            && self.runtime.lock().expect("runtime lock").resume.as_deref() == Some(recipe)
    }

    fn info(&self, pane: PaneId) -> PaneInfo {
        let runtime = self.runtime.lock().expect("runtime lock");
        PaneInfo {
            pane,
            shell_pid: self.shell_pid,
            cwd: runtime.cwd.clone(),
            resume: runtime.resume.clone(),
            mode: self.mode.lock().expect("mode lock").clone(),
            attached: self.sink.lock().expect("sink lock").is_some(),
            ended: self.ended.load(Ordering::SeqCst),
            geom: *self.geom.lock().expect("geom lock"),
        }
    }
}

/// How often a host looks at its own panes.
///
/// Two clocks, each with two speeds. The attached numbers are the ones a window
/// has always run: 800 ms to notice what a pane is running, 30 s to write down
/// where it is. The detached ones are the bill for the whole feature — hosts
/// now outlive the windows watching them, so a fleet of headless hosts polling
/// at window speed would be a battery drain nobody asked for and nobody can
/// see.
#[derive(Clone, Copy, Debug)]
pub struct Cadence {
    pub watch_attached: Duration,
    pub watch_detached: Duration,
    pub checkpoint_attached: Duration,
    pub checkpoint_detached: Duration,
    /// How long a host nobody is using waits before it stops.
    pub idle_exit: Duration,
    /// How often it looks to see whether that has happened.
    pub idle_check: Duration,
}

impl Default for Cadence {
    fn default() -> Self {
        Self {
            watch_attached: Duration::from_millis(800),
            watch_detached: Duration::from_secs(5),
            checkpoint_attached: Duration::from_secs(30),
            checkpoint_detached: Duration::from_secs(5 * 60),
            // Twelve hours, decided at Gate 2 and recorded under "Host
            // lifetime". Generous on purpose: it needs no heuristic about
            // whether a silent agent is thinking, and a heuristic is what would
            // eventually kill something irreplaceable in a way nobody could
            // reproduce.
            idle_exit: Duration::from_secs(12 * 60 * 60),
            idle_check: Duration::from_secs(5 * 60),
        }
    }
}

impl Cadence {
    fn watch(&self, attended: bool) -> Duration {
        if attended {
            self.watch_attached
        } else {
            self.watch_detached
        }
    }

    fn checkpoint(&self, attended: bool) -> Duration {
        if attended {
            self.checkpoint_attached
        } else {
            self.checkpoint_detached
        }
    }
}

/// A control connection's write half, shared by the two things that write to
/// it: the answer to a verb, and a push, which answers nothing.
///
/// One mutex, so two lines can never interleave into one. It is held only for
/// as long as a single small write, and the socket carries a send timeout, so a
/// client that has stopped reading holds up nobody but itself.
struct Conn {
    id: u64,
    write: Mutex<UnixStream>,
    /// Set when a later window said hello and took the session away from this
    /// one. A superseded connection may still ask questions — that is how its
    /// window finds out it was superseded rather than that its terminals died
    /// — but it may no longer change anything.
    superseded: AtomicBool,
    /// What this connection has negotiated, which decides whether it may
    /// change anything at all.
    greeting: Mutex<Greeting>,
}

/// How far a control connection has got with saying who it is.
///
/// The protocol says hello opens a connection, and until this existed the host
/// kept no memory of one line to the next: every request was answered on its
/// own, so a client that had never negotiated a version — or had been told its
/// version could not be spoken — could still close panes and stop the host.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Greeting {
    /// Nothing said yet.
    Silent,
    /// A hello arrived speaking a version this build cannot. The peer may not
    /// touch the session, and may ask for exactly one thing: that the host
    /// checkpoint and stand down, which is the whole of the approved path
    /// through a protocol break.
    Skewed,
    /// A hello this build could answer.
    Spoke,
}

impl Conn {
    fn new(id: u64, stream: UnixStream) -> Self {
        Self {
            id,
            write: Mutex::new(stream),
            superseded: AtomicBool::new(false),
            greeting: Mutex::new(Greeting::Silent),
        }
    }

    fn greeting(&self) -> Greeting {
        *self.greeting.lock().expect("greeting")
    }

    /// Record what a hello did.
    ///
    /// A connection that has already negotiated does not un-negotiate by
    /// asking again badly: `Spoke` is a fact about a conversation that
    /// happened, and a later bad number is one more thing this build cannot
    /// speak, not a retraction of one it could.
    fn greeted(&self, outcome: Greeting) {
        let mut greeting = self.greeting.lock().expect("greeting");
        if *greeting != Greeting::Spoke {
            *greeting = outcome;
        }
    }

    /// Write one line. `false` means this connection is finished.
    fn say(&self, line: &str) -> bool {
        let mut out = self.write.lock().expect("conn write");
        out.write_all(line.as_bytes()).is_ok()
    }

    /// Whether a later window has taken the session from this connection.
    fn superseded(&self) -> bool {
        self.superseded.load(Ordering::SeqCst)
    }
}

/// The most live terminals one host will hold.
///
/// Generous, and a sanity cap rather than a policy: the real per-window limit
/// is the client's `MAX_PANES` of four, and this is the number that stops a
/// script talking to the socket in a loop from spawning until the machine
/// falls over. In a house that runs agents in loops that is a live class of
/// accident rather than a hypothetical one. It must stay well above
/// `LEGACY_PANE_CEILING` so that restoring an old eight-pane layout never
/// meets it.
pub const HOST_PANE_SANITY_CAP: usize = 64;

/// How many terminals whose child has exited stay in the table.
///
/// They cannot leave the moment they die. A window that has just seen a stream
/// end asks the host whether the terminal exited or was taken from it, and a
/// pane the host has already forgotten answers "exited" to both — which is the
/// wrong answer to one of them, and costs a running pane its place on screen.
/// So the dead are kept, and kept bounded: oldest first, by ids that only ever
/// go up. Sixteen is enough for any window to have asked its question long
/// before the answer is evicted.
///
/// This is deliberately not the third state that a held-for-a-person pane will
/// need. A terminal whose child exited on its own and one a person closed and
/// may want back are different facts, and collapsing them here would leave the
/// close-undo work with nowhere to put the difference.
const EXITED_KEEP: usize = 16;

/// How long a write to a client may take before that client is written off.
///
/// A push is a hundred-odd bytes into a socket somebody is reading; taking a
/// quarter of a second over it means nobody is. The number matters because this
/// write happens on the watcher's thread, and a clock that can be stopped by
/// one wedged window is not a clock.
const WRITE_WITHIN: Duration = Duration::from_millis(250);

/// The clock the upkeep threads sleep on, and the bell that cuts a sleep short.
///
/// A detached checkpoint sleeps five minutes, and a window arriving must not
/// have to wait out the rest of it to be served — so attaching rings this, both
/// loops wake, and the next tick is at attached speed. It counts rings rather
/// than raising a flag because two loops are listening, and a flag one of them
/// clears is a wake the other never hears.
struct Upkeep {
    rung: Mutex<u64>,
    bell: Condvar,
}

impl Upkeep {
    fn new() -> Self {
        Self {
            rung: Mutex::new(0),
            bell: Condvar::new(),
        }
    }

    fn ring(&self) {
        *self.rung.lock().expect("upkeep") += 1;
        self.bell.notify_all();
    }

    /// Sleep until the bell rings or `period` is up, whichever comes first.
    fn listen(&self, heard: &mut u64, period: Duration) {
        let rung = self.rung.lock().expect("upkeep");
        let (rung, _) = self
            .bell
            .wait_timeout_while(rung, period, |rung| *rung == *heard)
            .expect("upkeep");
        *heard = *rung;
    }
}

/// A client's hold on a pane, and the proof it still has it.
pub struct Attachment {
    /// What was attached to. Nothing reads it yet — the control reply that
    /// carries it back to a window arrives with the window, in the attach
    /// slice — but attaching without reporting what you attached to would be
    /// the sort of API that gets one added later in a hurry.
    #[allow(dead_code)]
    pub info: PaneInfo,
    pub serial: u64,
}

/// Everything one session's host owns.
pub struct Host {
    key: String,
    /// What a pane runs instead of the login shell, if anything.
    ///
    /// Read once, here, rather than at every spawn: it is a property of the
    /// host process, and reading the environment repeatedly from threads that
    /// serve connections is how a program acquires a race it cannot see.
    shell: Option<String>,
    panes: Mutex<HashMap<PaneId, Arc<HostPane>>>,
    next_pane: AtomicU64,
    next_serial: AtomicU64,
    shutdown: AtomicBool,
    upkeep: Arc<Upkeep>,
    /// Whether any pane has produced output since this was last asked.
    spoke: Arc<AtomicBool>,
    /// When this host was first found doing nothing, or `None` if it is not.
    idle_since: Mutex<Option<Instant>>,
    /// Where this session's saved state lives, and what to write into it.
    ///
    /// `None` in a host nobody has told, which is every host in this file's
    /// tests. A checkpoint with nowhere to write writes nothing, rather than
    /// working out a path and writing over somebody's real session.
    state_file: Mutex<Option<std::path::PathBuf>>,
    /// The last layout a client handed over, and the shape it said it was.
    layout: Mutex<Option<(u32, toml::Value)>>,
    /// Held for the whole of a write, so only one happens at a time.
    ///
    /// Two things in this process write the session file — a client's `save`
    /// on its own connection, and the checkpoint on the upkeep thread — and
    /// without this they overlap. `session::write_atomic` builds its temporary
    /// file from the destination's name, so two writers share one temp path and
    /// the first rename takes it away from the second, which then fails with a
    /// puzzling "no such file". Worse than the error: the shrink guard reads
    /// what is on disk and then writes, and another write landing between those
    /// two makes the reading it decided on stale.
    ///
    /// Being the session's single writer is a claim about processes; this is
    /// what makes it true inside one.
    writing: Mutex<()>,
    /// The file's contents as this host last left them, so a write it did not
    /// make can be told from one it did.
    ///
    /// The text rather than a timestamp: exact, and it cannot be defeated by
    /// two writes landing inside one tick of whatever resolution the filesystem
    /// keeps. A session file is a few kilobytes.
    last_written: Mutex<Option<String>>,
    /// Control connections that asked to be told when something changes.
    watchers: Mutex<Vec<Arc<Conn>>>,
    /// The control connection of the window that currently holds this session.
    ///
    /// One slot, because a session has one window: a second window saying
    /// hello takes it, tmux-style, and the window it took it from is frozen
    /// out rather than refused (Gate 2, decision 1). A tool never occupies
    /// this slot and never empties it — every launch probes every candidate
    /// host, and if asking could steal, every launch would rob the window
    /// already running.
    gui: Mutex<Option<Arc<Conn>>>,
    next_conn: AtomicU64,
    /// How many times each clock has come round. Nothing in production reads
    /// them; they are how a test tells a host that has backed off from one that
    /// has stopped.
    watches: AtomicU64,
    checkpoints: AtomicU64,
}

impl Drop for Host {
    /// Cut short whatever the upkeep threads are sleeping through, so a dropped
    /// host's threads notice within a wake rather than within five minutes.
    fn drop(&mut self) {
        self.upkeep.ring();
    }
}

impl Host {
    pub fn new(key: impl Into<String>) -> Arc<Self> {
        let shell = std::env::var_os("TD_HOST_SHELL")
            .filter(|v| !v.is_empty())
            .map(|v| v.to_string_lossy().into_owned());
        Self::with_shell(key, shell)
    }

    pub fn with_shell(key: impl Into<String>, shell: Option<String>) -> Arc<Self> {
        Arc::new(Self {
            key: key.into(),
            shell,
            panes: Mutex::new(HashMap::new()),
            next_pane: AtomicU64::new(1),
            next_serial: AtomicU64::new(1),
            shutdown: AtomicBool::new(false),
            upkeep: Arc::new(Upkeep::new()),
            spoke: Arc::new(AtomicBool::new(false)),
            idle_since: Mutex::new(None),
            state_file: Mutex::new(None),
            writing: Mutex::new(()),
            layout: Mutex::new(None),
            last_written: Mutex::new(None),
            watchers: Mutex::new(Vec::new()),
            gui: Mutex::new(None),
            next_conn: AtomicU64::new(1),
            watches: AtomicU64::new(0),
            checkpoints: AtomicU64::new(0),
        })
    }

    /// Start a terminal: a real pseudoterminal, a real child, and an emulator
    /// whose grid is the truth every client will be shown.
    pub fn spawn_pane(
        &self,
        cwd: Option<String>,
        resume: Option<String>,
        geom: PaneGeom,
    ) -> io::Result<Spawn> {
        // The pane table is held from the check to the insert, and that is the
        // whole of the fix rather than an implementation detail. A client
        // deciding what to start compares a saved layout against a list it took
        // a moment earlier, so an agent can begin in a pane the list never
        // showed and the client starts a second copy of it. Two agents on one
        // conversation, both billing, both writing the same transcript. Only
        // the process that holds the table and does the spawning can check
        // without a gap.
        let mut panes = self.panes.lock().expect("panes");
        if let Some(recipe) = resume.as_deref() {
            if let Some((id, running)) = panes.iter().find(|(_, pane)| pane.runs(recipe)) {
                return Ok(Spawn {
                    info: running.info(*id),
                    started: false,
                });
            }
        }
        // Every terminal this host has ever held arrived through here, so this
        // is the one place where both of its bounds can be kept. The dead are
        // pruned first: they are what a long-running host accumulates, and a
        // corpse must not be able to hold the cap against a live pane.
        forget_the_oldest_dead(&mut panes);
        let live = panes
            .values()
            .filter(|pane| !pane.ended.load(Ordering::SeqCst))
            .count();
        if live >= HOST_PANE_SANITY_CAP {
            return Err(io::Error::other(format!(
                "this session already holds its limit of {HOST_PANE_SANITY_CAP} terminals"
            )));
        }
        let pane = PaneId(self.next_pane.fetch_add(1, Ordering::SeqCst));
        let size = GridSize {
            cols: geom.cols as usize,
            rows: geom.rows as usize,
        };
        let window_size = WindowSize {
            num_lines: geom.rows,
            num_cols: geom.cols,
            cell_width: geom.cell_width,
            cell_height: geom.cell_height,
        };

        let mut options = tty::Options {
            working_directory: cwd
                .as_ref()
                .map(std::path::PathBuf::from)
                .filter(|d| d.is_dir()),
            ..Default::default()
        };
        // Anything running in this pane can now find out where it is without
        // walking /proc and guessing — which is how session identity used to
        // be inferred, and how it used to be inferred wrongly.
        options
            .env
            .insert(ENV_SESSION.to_string(), self.key.clone());
        options
            .env
            .insert(ENV_PANE_ID.to_string(), pane.0.to_string());
        if let Some(program) = &self.shell {
            options.shell = Some(tty::Shell::new(program.clone(), vec![]));
        }

        let pty = tty::new(&options, window_size, 0)?;
        let shell_pid = pty.child().id();
        // Taken before the pseudoterminal is handed to the event loop, and the
        // reason the foreground watcher can live here at all.
        let master = pty.file().try_clone()?;
        let sink: Arc<Mutex<Option<Sink>>> = Arc::new(Mutex::new(None));
        let tee = TeePty::new(pty, sink.clone(), self.spoke.clone())?;

        let (events, incoming) = std::sync::mpsc::channel();
        let proxy = HostProxy(events);
        let term = Arc::new(FairMutex::new(Term::new(
            Config::default(),
            &size,
            proxy.clone(),
        )));
        let event_loop = EventLoop::new(term.clone(), proxy, tee, false, false)?;
        let input = Notifier(event_loop.channel());
        let answers = Notifier(event_loop.channel());
        event_loop.spawn();

        let ended = Arc::new(AtomicBool::new(false));
        let exit_flag = ended.clone();
        let exit_sink = sink.clone();
        std::thread::spawn(move || {
            for event in incoming {
                match event {
                    // A program asking the terminal a question — its size, or
                    // simply what it is — gets its answer here, exactly once.
                    // Answered anywhere else and an interactive program hangs
                    // waiting; answered twice and it reads the second reply as
                    // input the user typed.
                    TermEvent::PtyWrite(text) => {
                        let _ = answers.0.send(Msg::Input(text.into_bytes().into()));
                    }
                    TermEvent::TextAreaSizeRequest(format) => {
                        let reply = format(WindowSize {
                            num_lines: geom.rows,
                            num_cols: geom.cols,
                            cell_width: geom.cell_width,
                            cell_height: geom.cell_height,
                        });
                        let _ = answers.0.send(Msg::Input(reply.into_bytes().into()));
                    }
                    // The program in this terminal is gone, so the stream
                    // watching it is over. Nothing else tells a window: the
                    // socket stays open on its own, and a window drawing a
                    // terminal whose shell exited has no way to find out.
                    //
                    // **The flag first, then the hangup, and the order is
                    // load-bearing.** A window that sees its stream close asks
                    // whether the pane is still running, because a stream also
                    // closes when somebody takes the pane away, and `ended` is
                    // the difference between the two. The close is *caused by*
                    // dropping the sink, which happens after the store, so any
                    // observation of the close happens after it — the window
                    // cannot see the hangup and then be told the pane is fine.
                    TermEvent::ChildExit(_) => {
                        exit_flag.store(true, Ordering::SeqCst);
                        *exit_sink.lock().expect("sink lock") = None;
                    }
                    _ => {}
                }
            }
        });

        let host_pane = Arc::new(HostPane {
            term,
            input,
            sink,
            shell_pid,
            master,
            geom: Mutex::new(geom),
            ended,
            // The directory it was asked for is a claim, and the first
            // checkpoint replaces it with a reading. Nothing is claimed about
            // the agent inside it until something has looked.
            // Both seeded with what this pane was asked to be, and both
            // replaced by readings once there is something to read. The recipe
            // has to be here before the agent has started, or a second spawn
            // arriving in that gap would find nothing and start it again.
            runtime: Mutex::new(PaneRuntime {
                cwd,
                resume: resume.clone(),
            }),
            mode: Mutex::new(None),
            on_alt: AtomicBool::new(false),
        });
        // Typed here rather than by the window, because the host is what
        // decided not to type it a second time. It goes into the same input
        // queue a person's keystrokes do and waits for the shell's first read.
        // Whether a recipe is safe to resume is settled where it is recorded —
        // `session::safe_resume_id` — and a client that wanted to type this
        // itself could always open a byte stream and do so.
        if let Some(recipe) = resume {
            let _ = host_pane
                .input
                .0
                .send(Msg::Input(format!("{recipe}\n").into_bytes().into()));
        }
        let info = host_pane.info(pane);
        panes.insert(pane, host_pane);
        drop(panes);
        // New work for both clocks. A host nobody is watching sleeps five
        // minutes between checkpoints, and a pane that has just appeared must
        // not spend them unread — a restore starting four of them would leave a
        // whole layout unknown for as long.
        self.upkeep.ring();
        Ok(Spawn {
            info,
            started: true,
        })
    }

    pub fn list_panes(&self) -> Vec<PaneInfo> {
        let panes = self.panes.lock().expect("panes");
        let mut out: Vec<PaneInfo> = panes.iter().map(|(id, p)| p.info(*id)).collect();
        out.sort_by_key(|i| i.pane);
        out
    }

    /// Whether any client currently holds a pane's stream.
    pub fn attended(&self) -> bool {
        self.panes
            .lock()
            .expect("panes")
            .values()
            .any(|p| p.sink.lock().expect("sink lock").is_some())
    }

    /// Type into a pane.
    pub fn write_to(&self, pane: PaneId, bytes: Vec<u8>) -> bool {
        let panes = self.panes.lock().expect("panes");
        match panes.get(&pane) {
            Some(p) => p.input.0.send(Msg::Input(bytes.into())).is_ok(),
            None => false,
        }
    }

    pub fn resize(&self, pane: PaneId, geom: PaneGeom) -> Outcome<()> {
        let panes = self.panes.lock().expect("panes");
        let Some(p) = panes.get(&pane) else {
            return Outcome::Err(format!("no pane {pane}"));
        };
        *p.geom.lock().expect("geom lock") = geom;
        let window_size = WindowSize {
            num_lines: geom.rows,
            num_cols: geom.cols,
            cell_width: geom.cell_width,
            cell_height: geom.cell_height,
        };
        let _ = p.input.0.send(Msg::Resize(window_size));
        p.term.lock().resize(GridSize {
            cols: geom.cols as usize,
            rows: geom.rows as usize,
        });
        Outcome::Ok(())
    }

    /// Attach a client's byte stream to a pane: snapshot, then live output,
    /// with nothing lost or doubled between them.
    ///
    /// Supersedes whoever held it. A window relaunching after a crash must not
    /// be refused by the ghost of the window it is replacing, so the newest
    /// attach wins and the previous stream is closed.
    pub fn attach(&self, pane: PaneId, stream: UnixStream) -> Outcome<Attachment> {
        let panes = self.panes.lock().expect("panes");
        let Some(p) = panes.get(&pane) else {
            return Outcome::Err(format!("no pane {pane}"));
        };

        // The fence. `lease` blocks a new read cycle from starting and waits
        // out any in flight — and because the reader parses everything it read
        // before releasing, the grid below is complete as of that moment.
        let _lease = p.term.lease();
        // Unfair on purpose: the fair `lock` would try to take the lease we
        // are already holding, against ourselves, forever.
        let term = p.term.lock_unfair();

        let snapshot = gridwire::encode_snapshot(&*term);
        let serial = self.next_serial.fetch_add(1, Ordering::SeqCst);
        let sink = Sink::new(stream, serial);
        // The snapshot is queued before the stream is installed, so it cannot
        // arrive behind the live bytes that follow it.
        if !sink.send(&snapshot) {
            return Outcome::Err("client went away during the handover".into());
        }
        *p.sink.lock().expect("sink lock") = Some(sink);

        drop(term);
        drop(_lease);
        // The same rule, through the other door: a client can attach to a pane
        // whose child went while nobody was watching, and a stream that never
        // closes would leave it drawing a dead terminal for as long as the
        // window is open. It is shown the final screen — the snapshot is
        // already queued — and then the stream ends, which is the truth.
        //
        // Read after installing, never before. An exit landing between the two
        // would otherwise be missed by both: the exit thread would clear a sink
        // that was not there yet, and this would install one over a pane whose
        // child had already gone.
        if p.ended.load(Ordering::SeqCst) {
            *p.sink.lock().expect("sink lock") = None;
        }
        // Somebody is watching again. Both clocks are sleeping through a
        // detached period, and waiting one out before serving a window that has
        // already arrived is the sort of lag nobody can attribute later.
        self.upkeep.ring();
        Outcome::Ok(Attachment {
            info: p.info(pane),
            serial,
        })
    }

    /// Let go of a pane's stream, but only if it is still the one this client
    /// was given. A client that has been superseded owns nothing to release.
    pub fn detach(&self, pane: PaneId, serial: u64) {
        let panes = self.panes.lock().expect("panes");
        let Some(p) = panes.get(&pane) else { return };
        let mut held = p.sink.lock().expect("sink lock");
        if held.as_ref().is_some_and(|sink| sink.serial == serial) {
            *held = None;
        }
    }

    /// What this pane's authoritative grid hashes to, and how far down the
    /// attached client's stream that reading was taken.
    ///
    /// Under the same fence as the handover, and for the same reason. Alacritty's
    /// reader holds a lease across read-and-parse, so a lease taken here cannot
    /// overlap one: the hash and the byte count therefore describe the same
    /// moment. Taken outside it they would describe two, and the gap between
    /// them is precisely the quantity the guard measures — a guard fed a hash
    /// and an offset from different instants would report divergence for
    /// nothing, which is worse than no guard, because a loud repair costs a
    /// full snapshot every time it fires.
    ///
    /// The unfair lock again: the fair one takes the lease we are already
    /// holding, against ourselves, forever.
    pub fn grid_check(&self, pane: PaneId) -> Outcome<GridCheck> {
        let panes = self.panes.lock().expect("panes");
        let Some(p) = panes.get(&pane) else {
            return Outcome::Err(format!("no pane {pane}"));
        };
        let _lease = p.term.lease();
        let term = p.term.lock_unfair();
        let held = p.sink.lock().expect("sink lock");
        let Some(sink) = held.as_ref() else {
            // Nobody is reading this pane, so there is no stream and no offset
            // into one. Answering zero would hand a client a number it could
            // compare against and be confidently wrong about.
            return Outcome::Err(format!("pane {pane} has no attached client"));
        };
        Outcome::Ok(GridCheck {
            pane,
            stream_offset: sink.enqueued(),
            hash: gridwire::grid_hash(&*term),
        })
    }

    /// Close a pane: hang up its process tree, the way a terminal window
    /// closing always has.
    ///
    /// This is the only thing that kills. A client disconnecting, crashing or
    /// being replaced does not come through here.
    pub fn close_pane(&self, pane: PaneId) -> Outcome<ClosedPane> {
        let mut panes = self.panes.lock().expect("panes");
        let Some(p) = panes.remove(&pane) else {
            return Outcome::Err(format!("no pane {pane}"));
        };
        let shell_pid = p.shell_pid;
        // Signal the group, not the process: the shell is a session leader and
        // what a person means by closing a pane is everything running in it.
        let signalled = unsafe { libc::kill(-(shell_pid as i32), libc::SIGHUP) } == 0;
        let _ = p.input.0.send(Msg::Shutdown);
        Outcome::Ok(ClosedPane {
            shell_pid,
            signalled,
        })
    }

    pub fn pane_count(&self) -> usize {
        self.panes.lock().expect("panes").len()
    }

    /// Every pane, out from under the table's lock.
    ///
    /// The upkeep reads `/proc` and takes each terminal's own lock, and doing
    /// either while holding the pane table would stall every verb behind a
    /// process that happens to be stopped.
    fn pane_list(&self) -> Vec<(PaneId, Arc<HostPane>)> {
        self.panes
            .lock()
            .expect("panes")
            .iter()
            .map(|(id, pane)| (*id, pane.clone()))
            .collect()
    }

    /// Take over writing this session's saved state, seeding from whatever is
    /// already on disk.
    ///
    /// The seed is what makes a host useful before any window has spoken to it:
    /// a checkpoint can merge fresh working directories into the layout that
    /// was there when it started, rather than having nothing to merge into and
    /// writing nothing for as long as nobody saves.
    pub fn persist_to(&self, path: std::path::PathBuf) {
        let found = std::fs::read_to_string(&path).ok();
        let seed = found
            .as_deref()
            .and_then(|body| body.parse::<toml::Value>().ok());
        *self.last_written.lock().expect("last written") = found;
        if let Some(body) = seed {
            // Assumed to be this build's shape, because it is what this build
            // and its predecessors wrote. A newer client will say otherwise on
            // its first save and the assumption is replaced.
            *self.layout.lock().expect("layout") = Some((LAYOUT_SCHEMA, body));
        }
        *self.state_file.lock().expect("state file") = Some(path);
    }

    /// Take a client's layout and write it.
    pub fn save(&self, schema: u32, body: &str, allow_shrink: bool) -> Outcome<Persisted> {
        let parsed = match body.parse::<toml::Value>() {
            Ok(parsed) => parsed,
            Err(err) => return Outcome::Err(format!("the layout is not readable TOML: {err}")),
        };
        *self.layout.lock().expect("layout") = Some((schema, parsed));
        self.persist_now(allow_shrink)
    }

    /// Write the session's state: the client's layout, with what only this
    /// process can know filled in.
    ///
    /// The host is the single writer. It has the pseudoterminals, so it is the
    /// only thing that can say where a pane is or what would resume the agent
    /// in it; and being the only writer is what stops two processes with
    /// different ideas of the tree taking turns overwriting each other.
    pub fn persist_now(&self, allow_shrink: bool) -> Outcome<Persisted> {
        // One writer at a time, for the whole read-decide-write. See `writing`.
        let _writing = self.writing.lock().expect("writing");
        let Some(path) = self.state_file.lock().expect("state file").clone() else {
            return Outcome::Err("this host was never told where to write".into());
        };
        let Some((schema, layout)) = self.layout.lock().expect("layout").clone() else {
            return Outcome::Err("no layout has been handed over yet".into());
        };

        let merged = schema == LAYOUT_SCHEMA;
        let body = if merged {
            merge_capture_into_layout(&layout, &self.runtimes())
        } else {
            layout
        };
        let (leaves, tabs) = count_layout(&body);

        // The guard reads what is actually on disk, and counts it the same way
        // it counts what is offered. A client's own idea of how many panes it
        // has is not evidence — that number is written by whoever is saving,
        // and a save that has lost track of the tree has lost track of the
        // count with it.
        let (had_leaves, had_tabs) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|disk| disk.parse::<toml::Value>().ok())
            .map(|disk| count_layout(&disk))
            .unwrap_or((0, 0));
        if crate::is_catastrophic_shrink(had_leaves, had_tabs, leaves, tabs, allow_shrink) {
            return Outcome::Ok(Persisted::RefusedShrink {
                had_leaves,
                had_tabs,
                offered_leaves: leaves,
                offered_tabs: tabs,
            });
        }

        let mut body = body;
        // The count the file carries is the one the host walked. Session
        // ranking reads it without parsing the tree, so a stale or invented
        // number there decides which session a cold launch reopens.
        if let Some(table) = body.as_table_mut() {
            table.insert("panes".into(), toml::Value::Integer(leaves as i64));
        }
        let Ok(text) = toml::to_string(&body) else {
            return Outcome::Err("the layout could not be written back as TOML".into());
        };
        crate::rotate_state_backup(&path);
        match crate::session::write_atomic(&path, &text) {
            Ok(()) => {
                *self.last_written.lock().expect("last written") = Some(text);
                Outcome::Ok(Persisted::Written {
                    leaves,
                    tabs,
                    merged,
                })
            }
            Err(err) => Outcome::Err(format!("could not write {}: {err}", path.display())),
        }
    }

    /// Take a write this host did not make as the new starting point.
    ///
    /// Holding the pen does not mean nothing else may ever write. The session
    /// file is a plain document in a directory a person can open, and the
    /// documented way to recover a bad save is to copy a backup over it — which
    /// a host that never looks again silently undoes on its next checkpoint,
    /// because the copy it seeded at boot outlives everything.
    ///
    /// So when the file has changed underneath, the later write is the one that
    /// stands, and the host carries on from there. A client's own `save` still
    /// wins over this: the window is showing the live tree, and that is a
    /// better account of the session than anything on disk.
    ///
    /// A file that will not parse is left for the next tick and this host keeps
    /// what it has — half a write is not a layout.
    fn adopt_a_foreign_write(&self) {
        let Some(path) = self.state_file.lock().expect("state file").clone() else {
            return;
        };
        let found = std::fs::read_to_string(&path).ok();
        if *self.last_written.lock().expect("last written") == found {
            return;
        }
        let Some(body) = found
            .as_deref()
            .and_then(|text| text.parse::<toml::Value>().ok())
        else {
            return;
        };
        eprintln!(
            "terminal-delight serve: {} changed underneath this host — carrying on \
             from what is there rather than writing over it",
            path.display()
        );
        *self.layout.lock().expect("layout") = Some((LAYOUT_SCHEMA, body));
        *self.last_written.lock().expect("last written") = found;
    }

    /// What the last checkpoint read from each pane.
    fn runtimes(&self) -> std::collections::BTreeMap<u64, PaneRuntime> {
        self.pane_list()
            .into_iter()
            .map(|(id, pane)| (id.0, pane.runtime.lock().expect("runtime lock").clone()))
            .collect()
    }

    /// Start telling this connection about changes.
    fn watch(&self, conn: &Arc<Conn>) {
        let mut watchers = self.watchers.lock().expect("watchers");
        if !watchers.iter().any(|w| w.id == conn.id) {
            watchers.push(conn.clone());
        }
    }

    /// Called when a connection ends, however it ends.
    ///
    /// A subscription that outlived its socket would be a write to a closed
    /// descriptor on every tick, for the life of the host; and a window slot
    /// still holding a departed window's connection would hold its descriptor
    /// open with it, until some later window happened to arrive.
    fn hang_up(&self, id: u64) {
        self.watchers
            .lock()
            .expect("watchers")
            .retain(|w| w.id != id);
        let mut gui = self.gui.lock().expect("gui");
        if gui.as_ref().is_some_and(|held| held.id == id) {
            *gui = None;
        }
    }

    /// A window says hello: it takes the session, and whoever held it loses it.
    ///
    /// The steal is the approved answer to two windows wanting one session
    /// (Gate 2, decision 1): the newest wins, because it is the one a person
    /// is looking at, and refusing it would leave a relaunch staring at a
    /// terminal the ghost of its predecessor still owns. What the loser loses
    /// is both halves of holding a session — the pane streams it is drawing
    /// from, and the right to change anything, the session file included.
    ///
    /// Its control connection is left open on purpose. The loser has to find
    /// out *which* of the two things happened to its streams — a terminal that
    /// exited is over, a terminal taken away is still running — and asking is
    /// how it tells them apart. A connection dropped here would leave it with
    /// no way to ask and one obvious wrong answer to reach for.
    fn take_the_session(&self, conn: &Arc<Conn>) {
        let mut gui = self.gui.lock().expect("gui");
        let loser = match gui.as_ref() {
            Some(held) if held.id == conn.id => None,
            Some(_) => gui.take(),
            None => None,
        };
        *gui = Some(conn.clone());
        // A window that lost the session and says hello again has it back:
        // taking it is what the verb means, whoever last held it.
        conn.superseded.store(false, Ordering::SeqCst);
        drop(gui);
        let Some(loser) = loser else { return };
        loser.superseded.store(true, Ordering::SeqCst);
        // Everything attached belonged to the window that just lost the
        // session: a window says hello before it attaches anything, so at this
        // instant the newcomer holds nothing. Sinks carry no owner — a byte
        // stream is a separate connection that names a pane and nothing else —
        // and inventing one here would be a fence with a hole in it while the
        // stream connection stays unauthenticated by design.
        for (_, pane) in self.pane_list() {
            *pane.sink.lock().expect("sink lock") = None;
        }
    }

    /// Tell every watching connection about something that happened.
    ///
    /// The handles come out from under the lock before a byte is written, so a
    /// window that has stopped reading delays only itself, and a write that
    /// fails costs that connection its subscription rather than costing the
    /// host its clock.
    fn broadcast(&self, push: &Push) {
        let watching: Vec<Arc<Conn>> = self.watchers.lock().expect("watchers").clone();
        if watching.is_empty() {
            return;
        }
        let Ok(mut line) = serde_json::to_string(push) else {
            return;
        };
        line.push('\n');
        let gone: Vec<u64> = watching
            .iter()
            .filter(|conn| !conn.say(&line))
            .map(|conn| conn.id)
            .collect();
        if !gone.is_empty() {
            self.watchers
                .lock()
                .expect("watchers")
                .retain(|w| !gone.contains(&w.id));
        }
    }

    /// Start the two clocks: the foreground watcher, and the checkpoint.
    ///
    /// Both hold a weak reference. A host that is dropped — which in tests is
    /// every host, at the end of every test — must not be kept alive by its own
    /// upkeep, and a pair of threads per host that never exit is the kind of
    /// leak that shows up as a suite getting slower and never as a failure.
    pub fn start_upkeep(self: &Arc<Self>, cadence: Cadence) {
        upkeep_loop(self, cadence, Cadence::watch, Host::watch_once);
        upkeep_loop(self, cadence, Cadence::checkpoint, Host::checkpoint_once);
        lifetime_loop(self, cadence);
    }

    /// Whether anything is using this session right now.
    ///
    /// Read generously, and on purpose. A window holding a pane's byte stream
    /// is the plain case; a connection that asked to be told about changes is
    /// also a client, and a host that stopped underneath one would be ending a
    /// session somebody has open. The cost of being too generous is a process
    /// that lingers; the cost of being too eager is somebody's work.
    fn being_used(&self) -> bool {
        self.attended() || !self.watchers.lock().expect("watchers").is_empty()
    }

    /// One look at whether this host is still wanted, and the end of it if not.
    ///
    /// Two rules from the decision, and neither is a heuristic. **Never while a
    /// client is attached**, whatever the panes are printing — an attached
    /// window is proof the session is wanted. And **checkpoint before going**,
    /// so what is left on disk is fresh rather than hours stale.
    pub fn idle_once(&self, budget: Duration) {
        // Cleared whether or not it is needed: this is the only reader, and a
        // flag left set would make the next look think somebody had spoken.
        let spoke = self.spoke.swap(false, Ordering::Relaxed);
        let mut since = self.idle_since.lock().expect("idle");
        if self.being_used() || spoke {
            *since = None;
            return;
        }
        let waiting = *since.get_or_insert_with(Instant::now);
        if waiting.elapsed() < budget {
            return;
        }
        drop(since);

        eprintln!(
            "terminal-delight serve: session '{}' has had nobody attached and \
             nothing to say for {} hours — checkpointing and stopping",
            self.key,
            budget.as_secs() / 3600
        );
        let _ = self.persist_now(false);
        self.stop();
    }

    /// Bring this host to a stop, by the one door it has.
    ///
    /// The accept loop is asleep waiting for a connection, so it is given one.
    /// Without that a host would keep running until somebody happened to knock,
    /// which is a stop that depends on a stranger arriving — and going out this
    /// way means the socket is removed and the session released in the order
    /// that has already been got right, rather than in a second copy of it.
    fn stop(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.upkeep.ring();
        let _ = UnixStream::connect(host_socket_path(&self.key));
    }

    /// One pass of the foreground watcher: ask each pane's pseudoterminal what
    /// is running in it, and keep the answer.
    pub fn watch_once(&self) {
        self.watches.fetch_add(1, Ordering::SeqCst);
        for (id, pane) in self.pane_list() {
            let detected = classify_foreground(&pane.master, pane.shell_pid);
            // Read every tick, not only when there is a demotion to weigh: the
            // sticky rule wants to know whether the alternate screen is up, and
            // the heal below wants to know whether it has just come down.
            let on_alt = pane.term.lock().mode().contains(TermMode::ALT_SCREEN);
            let was_on_alt = pane.on_alt.swap(on_alt, Ordering::SeqCst);

            let next = {
                let mut held = pane.mode.lock().expect("mode lock");
                let next = next_mode(held.as_ref(), detected, on_alt);
                let changed = *held != next;
                *held = next.clone();
                changed.then_some(next).flatten()
            };
            // On the change, never on the clock. A window that wanted the
            // current state of everything asked `list-panes` for it.
            if let Some(mode) = next {
                // The moment what a pane is running changes is the moment its
                // directory and its resume recipe may have changed with it, so
                // read them now rather than at the next checkpoint.
                //
                // This is not tidiness. A window planning an attach reads
                // `list-panes` before anything it does rings this host's clock,
                // and it binds a leaf whose pane is gone to a live pane running
                // exactly its resume line. On the checkpoint's clock alone that
                // line is up to five minutes late on a host nobody is watching,
                // so a pane whose agent had just started would look like no
                // agent at all — and the leaf would type `claude --resume <id>`
                // into a second terminal: two agents on one conversation.
                record_reading(&pane);
                self.broadcast(&Push::Mode { pane: id, mode });
            }

            if was_on_alt && !on_alt {
                self.heal_after_the_alternate_screen(&pane);
            }
        }
    }

    /// Send an attached client the history a full-screen program was hiding.
    ///
    /// A snapshot can only read the grid that is active, so a client that
    /// attached to a pane running `vim` was sent the alternate screen and
    /// nothing behind it. Its scrollback is empty and its primary grid is
    /// blank, and no amount of live output will fill them in, because that
    /// history was written before it arrived. The moment the program exits is
    /// the moment to hand it over.
    ///
    /// The bytes are exactly what an attach sends, under exactly the same
    /// fence, which is the argument that this is correct: a snapshot already
    /// lands the terminal in a known state and clears the screen and the
    /// scrollback before painting, so the replica ends up where a fresh attach
    /// would have put it. The real `?1049l` reached it earlier through the byte
    /// stream — that is how the watcher noticed at all — so it is on its
    /// primary screen by now and the paint lands on the right grid.
    fn heal_after_the_alternate_screen(&self, pane: &HostPane) {
        let _lease = pane.term.lease();
        let term = pane.term.lock_unfair();
        let mut held = pane.sink.lock().expect("sink lock");
        let Some(sink) = held.as_ref() else {
            return;
        };
        if !sink.send(&gridwire::encode_snapshot(&*term)) {
            *held = None;
        }
    }

    /// One checkpoint: read from each pane the two things only the process
    /// holding its pseudoterminal can read — where it actually is, and what
    /// would put its agent back in the conversation it was having.
    ///
    /// It records rather than writes. The session file also carries window
    /// bounds, tab names and a theme, none of which a host has ever seen, so
    /// the window stays the one that writes it; what moved here is the half
    /// that stopped being answerable from a window at all. The verb that hands
    /// the layout over for the host to merge and write arrives with the
    /// attaching client.
    pub fn checkpoint_once(&self) {
        self.checkpoints.fetch_add(1, Ordering::SeqCst);
        for (_, pane) in self.pane_list() {
            record_reading(&pane);
        }
        // Before writing, look. Something else may have written this file since
        // the last time, and a checkpoint that does not check is what makes a
        // host's copy permanent.
        self.adopt_a_foreign_write();
        // And then the point of having read them. A host with nowhere to write
        // or nothing to write says so and is ignored here: a checkpoint is a
        // clock, not a request, and there is nobody to tell.
        let _ = self.persist_now(false);
    }

    /// How many times each clock has come round.
    #[cfg(test)]
    fn upkeep_counts(&self) -> (u64, u64) {
        (
            self.watches.load(Ordering::SeqCst),
            self.checkpoints.load(Ordering::SeqCst),
        )
    }

    /// Read a pane's grid as text. Used by tests and by anything that wants to
    /// know what a terminal is showing without drawing it.
    #[cfg(test)]
    fn row_text(&self, pane: PaneId, row: i32) -> Option<String> {
        use alacritty_terminal::grid::Dimensions;
        use alacritty_terminal::index::{Column, Line};
        let panes = self.panes.lock().expect("panes");
        let p = panes.get(&pane)?;
        let term = p.term.lock();
        let grid = term.grid();
        Some(
            (0..grid.columns())
                .map(|c| grid[Line(row)][Column(c)].c)
                .collect::<String>()
                .trim_end()
                .to_string(),
        )
    }
}

/// The clock that ends a host nobody is using.
///
/// Its own loop rather than a third job on the checkpoint's, because it is the
/// only one whose answer is "stop", and because the period it looks on has
/// nothing to do with whether anybody is watching — a host with nobody
/// attached is exactly the one this is counting.
fn lifetime_loop(host: &Arc<Host>, cadence: Cadence) {
    let ghost = Arc::downgrade(host);
    let upkeep = host.upkeep.clone();
    std::thread::spawn(move || {
        let mut heard = 0;
        loop {
            let Some(host) = ghost.upgrade() else { return };
            if host.shutdown.load(Ordering::SeqCst) {
                return;
            }
            host.idle_once(cadence.idle_exit);
            drop(host);
            upkeep.listen(&mut heard, cadence.idle_check);
        }
    });
}

/// One upkeep clock: do the work, then sleep for as long as being watched or
/// not says to.
///
/// The host is upgraded from a weak reference for the work and let go of before
/// the sleep, so the sleep never keeps it alive.
fn upkeep_loop(
    host: &Arc<Host>,
    cadence: Cadence,
    period: fn(&Cadence, bool) -> Duration,
    tick: fn(&Host),
) {
    let ghost = Arc::downgrade(host);
    let upkeep = host.upkeep.clone();
    std::thread::spawn(move || {
        let mut heard = 0;
        loop {
            let Some(host) = ghost.upgrade() else { return };
            if host.shutdown.load(Ordering::SeqCst) {
                return;
            }
            tick(&host);
            let sleep_for = period(&cadence, host.attended());
            drop(host);
            upkeep.listen(&mut heard, sleep_for);
        }
    });
}

/// Read from a pane the two things only the process holding its pseudoterminal
/// can read, and keep whichever of them answered.
///
/// A reading that failed is not a pane in no directory running nothing: only an
/// answer replaces an answer.
fn record_reading(pane: &HostPane) {
    let fresh = crate::session::capture(Some(&pane.master), pane.shell_pid);
    let mut held = pane.runtime.lock().expect("runtime lock");
    if fresh.cwd.is_some() {
        held.cwd = fresh.cwd;
    }
    if fresh.resume.is_some() {
        held.resume = fresh.resume;
    }
}

/// Fill a saved layout's leaves in with what the panes are actually doing.
///
/// Over `toml::Value`, never through the layout's own Rust types, and that is
/// the whole design rather than an implementation detail. A window one version
/// newer than this host writes fields this host has never heard of; parsing the
/// body into a type here would drop every one of them on the way back out, and
/// the loss would surface much later as somebody's settings quietly reverting.
/// So the host walks for the two things it alone can know, edits those, and
/// leaves the document otherwise exactly as it arrived.
///
/// A leaf without a `pane_id` is a pane this host is not running — a legacy
/// file, or a window that owns its own terminals — and is not touched. A leaf
/// whose pane the host has no reading for keeps what it had: a checkpoint that
/// could not read a directory must not erase the last one that could.
fn merge_capture_into_layout(
    layout: &toml::Value,
    live: &std::collections::BTreeMap<u64, PaneRuntime>,
) -> toml::Value {
    let mut merged = layout.clone();
    let Some(tabs) = merged.get_mut("tabs").and_then(|tabs| tabs.as_array_mut()) else {
        return merged;
    };
    for tab in tabs.iter_mut() {
        if let Some(node) = tab.get_mut("node") {
            fill_leaves(node, live);
        }
    }
    merged
}

/// The recursive half: a node is a `Leaf` table or a `Split` holding two more.
fn fill_leaves(node: &mut toml::Value, live: &std::collections::BTreeMap<u64, PaneRuntime>) {
    if let Some(leaf) = node.get_mut("Leaf").and_then(|leaf| leaf.as_table_mut()) {
        let Some(runtime) = leaf
            .get("pane_id")
            .and_then(toml::Value::as_integer)
            .and_then(|id| u64::try_from(id).ok())
            .and_then(|id| live.get(&id))
        else {
            return;
        };
        if let Some(cwd) = &runtime.cwd {
            leaf.insert("cwd".into(), toml::Value::String(cwd.clone()));
        }
        if let Some(resume) = &runtime.resume {
            leaf.insert("resume".into(), toml::Value::String(resume.clone()));
        }
        return;
    }
    if let Some(split) = node.get_mut("Split") {
        for side in ["a", "b"] {
            if let Some(child) = split.get_mut(side) {
                fill_leaves(child, live);
            }
        }
    }
}

/// Count a saved layout's panes and tabs by walking it.
///
/// The same walk that does the merging, so the number that feeds the shrink
/// guard is the host's own reading of the tree rather than a field somebody
/// wrote into it. An envelope whose top-level `panes` says six while its tree
/// holds one must not be able to talk its way past the guard.
fn count_layout(layout: &toml::Value) -> (usize, usize) {
    let Some(tabs) = layout.get("tabs").and_then(toml::Value::as_array) else {
        return (0, 0);
    };
    let leaves = tabs
        .iter()
        .filter_map(|tab| tab.get("node"))
        .map(count_leaves)
        .sum();
    (leaves, tabs.len())
}

fn count_leaves(node: &toml::Value) -> usize {
    if node.get("Leaf").is_some() {
        return 1;
    }
    match node.get("Split") {
        Some(split) => ["a", "b"]
            .iter()
            .filter_map(|side| split.get(side))
            .map(count_leaves)
            .sum(),
        None => 0,
    }
}

/// What is in the foreground of this pane, by the kernel's own account.
///
/// Relocated from the window (pane.rs's `foreground_mode`), because the
/// question is `tcgetpgrp` on a pseudoterminal master and the host is what
/// holds one now.
///
/// `None` means the kernel would not say — a pane whose child has gone, most
/// often — and it is deliberately not `Shell`. The window could collapse those
/// two because it was looking at its own terminal and could see for itself; a
/// value that travels to another process cannot, because nothing on the far end
/// can tell a reading from a stand-in.
fn classify_foreground(master: &File, shell_pid: u32) -> Option<WireMode> {
    use std::os::fd::AsRawFd;
    let pgid = unsafe { libc::tcgetpgrp(master.as_raw_fd()) };
    if pgid <= 0 {
        return None;
    }
    if pgid as u32 == shell_pid {
        return Some(WireMode::Shell);
    }
    let comm = std::fs::read_to_string(format!("/proc/{pgid}/comm")).unwrap_or_default();
    let cmdline = std::fs::read_to_string(format!("/proc/{pgid}/cmdline"))
        .unwrap_or_default()
        .replace('\0', " ");
    Some(classify(&comm, &cmdline))
}

/// The naming rule, kept pure so it can be tested without a terminal. The same
/// vocabulary the window has always used (pane.rs's `PaneMode::classify`).
fn classify(comm: &str, cmdline: &str) -> WireMode {
    let comm = comm.trim();
    if comm == "claude" || cmdline.contains("/claude") {
        WireMode::Claude
    } else if comm == "codex" || cmdline.contains("/codex") {
        WireMode::Codex
    } else if matches!(comm, "ssh" | "mosh-client" | "et" | "telnet") {
        WireMode::Remote
    } else if matches!(comm, "bash" | "zsh" | "fish" | "sh" | "dash" | "nu") {
        WireMode::Shell
    } else {
        WireMode::Other(comm.to_string())
    }
}

/// What a pane's mode becomes, given what it was and what was just seen.
///
/// Two rules live here, and both are about refusing to write down a guess.
///
/// An agent spends much of its time with something else in the foreground —
/// bash, node, rg — which reads as an ordinary shell for as long as that child
/// runs, and a pane that renames itself twice a second is worse than one that is
/// a beat behind. So while the alternate screen is up, which is where an agent's
/// own interface lives, an agent stays an agent. When it exits and the plain
/// shell comes back on the normal screen, the demotion is real and it happens.
///
/// And a reading the kernel would not give never overwrites one it did.
fn next_mode(
    current: Option<&WireMode>,
    detected: Option<WireMode>,
    on_alt: bool,
) -> Option<WireMode> {
    let Some(detected) = detected else {
        return current.cloned();
    };
    let was_agent = matches!(current, Some(WireMode::Claude | WireMode::Codex));
    let still_agent = matches!(detected, WireMode::Claude | WireMode::Codex);
    if was_agent && !still_agent && on_alt {
        return current.cloned();
    }
    Some(detected)
}

/// Whether the peer on this socket is the same user we are.
///
/// The whole authorisation model, and deliberately the whole of it: the socket
/// lives in a private directory, and this confirms at the moment of connection
/// what the directory implies. Nothing further up the stack repeats the claim,
/// so nothing further up can be wrong about it.
/// Keep the pane table's dead down to [`EXITED_KEEP`], oldest first.
///
/// Pane ids are minted from a counter that only goes up and are never reused,
/// so sorting them is sorting by age — no clock, and nothing to be wrong about
/// when two panes die inside one tick of one.
fn forget_the_oldest_dead(panes: &mut HashMap<PaneId, Arc<HostPane>>) {
    let mut dead: Vec<PaneId> = panes
        .iter()
        .filter(|(_, pane)| pane.ended.load(Ordering::SeqCst))
        .map(|(id, _)| *id)
        .collect();
    if dead.len() <= EXITED_KEEP {
        return;
    }
    dead.sort_unstable();
    let surplus = dead.len() - EXITED_KEEP;
    for id in dead.into_iter().take(surplus) {
        panes.remove(&id);
    }
}

fn peer_is_us(stream: &UnixStream) -> bool {
    use std::os::fd::AsRawFd;
    let mut cred = libc::ucred {
        pid: 0,
        uid: u32::MAX,
        gid: u32::MAX,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let ok = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            std::ptr::from_mut(&mut cred).cast::<libc::c_void>(),
            &mut len,
        )
    } == 0;
    ok && cred.uid == unsafe { libc::getuid() }
}

/// Serve one connection: either a control conversation or a pane's byte
/// stream, decided by its first line.
fn serve_connection(host: &Arc<Host>, stream: UnixStream) {
    if !peer_is_us(&stream) {
        return;
    }
    let Ok(reader_half) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(reader_half);
    let mut first = String::new();
    if reader.read_line(&mut first).is_err() || first.is_empty() {
        return;
    }

    // A byte stream announces itself and then stops talking in messages.
    if let Some(pane) = parse_stream_greeting(&first) {
        if let Outcome::Ok(held) = host.attach(pane, stream) {
            keep_typing(host, pane, held.serial, reader);
        }
        return;
    }

    // A client that has stopped reading must not be able to hold the watcher's
    // thread, which writes to this same socket.
    let _ = stream.set_write_timeout(Some(WRITE_WITHIN));
    let conn = Arc::new(Conn::new(
        host.next_conn.fetch_add(1, Ordering::SeqCst),
        stream,
    ));
    control_loop(host, &conn, first, reader);
    // However this connection ended, it is no longer anybody to push to, and
    // no longer the window holding this session.
    host.hang_up(conn.id);
}

/// One control conversation: a verb in, a line out, until somebody hangs up.
fn control_loop(
    host: &Arc<Host>,
    conn: &Arc<Conn>,
    first: String,
    mut reader: BufReader<UnixStream>,
) {
    let mut line = first;
    loop {
        let reply = handle_control_line(host, &line, Some(conn));
        let shutting = matches!(reply, Reply::ShuttingDown);
        let mut out = serde_json::to_string(&reply).unwrap_or_else(|e| {
            format!(r#"{{"reply":"error","msg":"could not encode a reply: {e}"}}"#)
        });
        out.push('\n');
        if !conn.say(&out) {
            return;
        }
        if shutting {
            host.stop();
            return;
        }
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
    }
}

/// Everything after a byte stream's greeting is keystrokes.
fn keep_typing(host: &Arc<Host>, pane: PaneId, serial: u64, mut reader: BufReader<UnixStream>) {
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                if !host.write_to(pane, buf[..read].to_vec()) {
                    break;
                }
            }
        }
    }
    // This client is gone. That kills nothing — closing a pane is a verb, and
    // this is not it — and it releases nothing that is no longer ours, because
    // by now the stream may belong to the window that replaced us.
    host.detach(pane, serial);
}

/// What a window that has lost the session is told when it tries to change it.
///
/// Shaped per verb rather than as a bare error, and that is not politeness. A
/// client reads the replies on its connection in order and steps over the ones
/// it was not waiting for; a bare error is the one shape it cannot step over,
/// so a refusal in that shape would be read as the answer to whatever it asked
/// next. Questions are not refused at all — a window that has just lost its
/// streams has to be able to ask whether its terminals died or were taken, and
/// asking is the only way it can tell those apart.
fn refuse_the_superseded(request: &Request) -> Option<Reply> {
    const LOST: &str = "another window holds this session now";
    Some(match request {
        Request::SpawnPane { .. } => Reply::Spawned {
            outcome: Outcome::Err(LOST.into()),
            started: false,
        },
        Request::AttachPane { pane, .. } => Reply::Attached {
            pane: *pane,
            outcome: Outcome::Err(LOST.into()),
        },
        Request::Resize { pane, .. } => Reply::Resized {
            pane: *pane,
            outcome: Outcome::Err(LOST.into()),
        },
        Request::ClosePane { pane } => Reply::Closed {
            pane: *pane,
            outcome: Outcome::Err(LOST.into()),
        },
        Request::Save { .. } => Reply::Saved {
            outcome: Outcome::Err(LOST.into()),
        },
        Request::Shutdown => Reply::Error {
            msg: format!("{LOST}, and ending it is theirs to ask for"),
        },
        // Saying hello again takes the session back, which is what the verb
        // means. The rest are questions.
        Request::Hello { .. } | Request::ListPanes | Request::GridCheck { .. } | Request::Watch => {
            return None
        }
    })
}

/// What a connection that has not negotiated is told when it tries to change
/// the session.
///
/// Four verbs, and they are the four the status file names: spawn, close, save
/// and shutdown. Not everything, on purpose — a question costs nothing and
/// every launch asks several, and a gate wide enough to cover resize would
/// refuse a client for asking a pane to be the size it already is. What these
/// four have in common is that a client which cannot state a version it speaks
/// should not be able to end a terminal, end a host, or write the file that
/// says what the session was.
///
/// The one exception is the reason this gate can exist at all. A peer refused
/// for version skew must still be able to say "checkpoint and stand down", or
/// the approved path through a protocol break — the old host writes what it
/// has and goes, the new build recovers from the file — has no way to be
/// asked for.
fn refuse_the_ungreeted(request: &Request, greeting: Greeting) -> Option<Reply> {
    if greeting == Greeting::Spoke {
        return None;
    }
    const COLD: &str = "say hello first: this connection has not negotiated a protocol version";
    Some(match request {
        Request::SpawnPane { .. } => Reply::Spawned {
            outcome: Outcome::Err(COLD.into()),
            started: false,
        },
        Request::ClosePane { pane } => Reply::Closed {
            pane: *pane,
            outcome: Outcome::Err(COLD.into()),
        },
        Request::Save { .. } => Reply::Saved {
            outcome: Outcome::Err(COLD.into()),
        },
        Request::Shutdown if greeting == Greeting::Silent => Reply::Error { msg: COLD.into() },
        _ => return None,
    })
}

fn handle_control_line(host: &Arc<Host>, line: &str, conn: Option<&Arc<Conn>>) -> Reply {
    let request: Request = match serde_json::from_str(line.trim()) {
        Ok(request) => request,
        Err(err) => {
            return Reply::Error {
                msg: format!("unreadable request: {err}"),
            }
        }
    };
    // A line that arrived on no connection at all is this file's own tests
    // driving the verb table; there is no peer to have negotiated with, and
    // nothing for a gate to protect. Everything a client can reach arrives on
    // a connection.
    let greeting = conn.map_or(Greeting::Spoke, |conn| conn.greeting());
    if let Some(refusal) = refuse_the_ungreeted(&request, greeting) {
        return refusal;
    }
    if conn.is_some_and(|conn| conn.superseded()) {
        if let Some(refusal) = refuse_the_superseded(&request) {
            return refusal;
        }
    }
    match request {
        Request::Hello { proto, kind } => match crate::hostproto::version_check(proto) {
            Ok(()) => {
                if let Some(conn) = conn {
                    conn.greeted(Greeting::Spoke);
                }
                // Only a window takes the session, and it takes it by saying
                // what it is. A tool asks its questions and leaves the window
                // that is using this session exactly as it found it.
                if let (ClientKind::Window, Some(conn)) = (kind, conn) {
                    host.take_the_session(conn);
                }
                Reply::Hello {
                    proto: PROTO_VERSION,
                    session: host.key.clone(),
                    panes: host.pane_count(),
                    attended: host.attended(),
                }
            }
            Err(msg) => {
                // Refused, and remembered as refused. This is the connection
                // the version-break path runs on: it may not touch the
                // session, and it may ask the host to checkpoint and go.
                if let Some(conn) = conn {
                    conn.greeted(Greeting::Skewed);
                }
                Reply::Error { msg }
            }
        },
        Request::ListPanes => Reply::Panes {
            panes: host.list_panes(),
        },
        Request::SpawnPane { cwd, resume, geom } => match host.spawn_pane(cwd, resume, geom) {
            Ok(spawn) => Reply::Spawned {
                outcome: Outcome::Ok(spawn.info),
                started: spawn.started,
            },
            Err(err) => Reply::Spawned {
                outcome: Outcome::Err(format!("could not start a terminal: {err}")),
                started: false,
            },
        },
        Request::AttachPane { pane, geom } => {
            // Intent and size. The handover itself happens when the byte
            // stream connects, because that is when there is something to hand
            // the pane over to.
            let resized = host.resize(pane, geom);
            Reply::Attached {
                pane,
                outcome: match resized {
                    Outcome::Ok(()) => {
                        let panes = host.panes.lock().expect("panes");
                        match panes.get(&pane) {
                            Some(p) => Outcome::Ok(p.info(pane)),
                            None => Outcome::Err(format!("no pane {pane}")),
                        }
                    }
                    Outcome::Err(msg) => Outcome::Err(msg),
                },
            }
        }
        Request::Resize { pane, geom } => Reply::Resized {
            pane,
            outcome: host.resize(pane, geom),
        },
        Request::ClosePane { pane } => Reply::Closed {
            pane,
            outcome: host.close_pane(pane),
        },
        Request::GridCheck { pane } => Reply::GridChecked {
            pane,
            outcome: host.grid_check(pane),
        },
        Request::Save {
            schema,
            body,
            allow_shrink,
        } => Reply::Saved {
            outcome: host.save(schema, &body, allow_shrink),
        },
        Request::Watch => match conn {
            Some(conn) => {
                host.watch(conn);
                Reply::Watching
            }
            // Unreachable from a socket — every control line arrives on one.
            // The shape exists so the verb table can be exercised without a
            // connection, and answering rather than pretending is the rule
            // everywhere else here.
            None => Reply::Error {
                msg: "watching is a property of a connection, and this line arrived on none".into(),
            },
        },
        Request::Shutdown => {
            // Checkpoint on the way out, and the order is the whole point. A
            // host is asked to stand down by a client that cannot speak its
            // protocol, and what happens next is that the newer build reads
            // this file and starts from it — so the file has to be what the
            // session was a moment ago, not what it was at the last tick of a
            // clock that may have been five minutes back. This degrades a
            // protocol break to exactly what a window used to do, once, and
            // deliberately: the terminals go with the host, and the layout
            // survives them.
            host.checkpoint_once();
            Reply::ShuttingDown
        }
    }
}

/// `terminal-delight serve --session <key>` — run a session host until it is
/// told to stop.
pub fn run_cli(args: &[String]) -> i32 {
    let mut key = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--session" => key = rest.next().cloned(),
            other => {
                eprintln!("terminal-delight serve: unexpected argument `{other}`");
                return 2;
            }
        }
    }
    let Some(key) = key else {
        eprintln!("usage: terminal-delight serve --session <key>");
        return 2;
    };

    let path = host_socket_path(&key);
    if let Some(dir) = path.parent() {
        if let Err(err) = std::fs::create_dir_all(dir) {
            eprintln!(
                "terminal-delight serve: cannot create {}: {err}",
                dir.display()
            );
            return 1;
        }
        // The directory is the authorisation model; make it say so.
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }

    let (listener, claim) = match take_session(
        &crate::instance::config_dir(),
        &path,
        &key,
        SOCKET_APPEARS_WITHIN,
    ) {
        Ok(taken) => taken,
        Err((why, code)) => {
            eprintln!("terminal-delight serve: {why}");
            return code;
        }
    };

    let host = Host::new(&key);
    // The host is this session's writer from here on. Told where before the
    // first client can ask for anything, so a save arriving in the first
    // moments has somewhere to go.
    host.persist_to(state_file_for(&crate::instance::config_dir(), &key));
    // The clocks start with the host, not with a window: a session nobody is
    // looking at still has to know where its panes are.
    host.start_upkeep(Cadence::default());
    for stream in listener.incoming() {
        if host.shutdown.load(Ordering::SeqCst) {
            break;
        }
        let Ok(stream) = stream else { continue };
        let serving = host.clone();
        std::thread::spawn(move || serve_connection(&serving, stream));
    }
    // Order, and it is the whole of the reason this is written out rather than
    // left to the end of the scope: the socket goes while the session is still
    // held. Let go first and a new host could take the session and bind this
    // path between the two statements, and the file removed would be its front
    // door rather than ours.
    let _ = std::fs::remove_file(&path);
    drop(claim);
    0
}

/// How long a host that lost the race waits for the winner's socket to appear.
///
/// The winner takes the session and then binds, so the two are microseconds
/// apart and this is generous. It exists because losing gracefully means
/// telling the caller that a host exists, and a probe run a moment too early
/// would report one that does not — which is the two-windows-at-once case, the
/// commonest way to get here at all.
const SOCKET_APPEARS_WITHIN: Duration = Duration::from_secs(2);

/// Become this session's host, or explain why not.
///
/// `Ok` is the listener together with the lock file whose open descriptor *is*
/// the claim — hold it for the life of the process. `Err` is what to print and
/// what to exit with.
///
/// The order is the fix. A socket file is not proof of a live host: one is left
/// behind by a host that was killed, and one belongs to a host that is running,
/// and nothing about the file tells the two apart. Removing it to find out is
/// how a second host took a first host's front door and left it running with
/// terminals nobody could reach any more (#335). So the session is claimed
/// first, with a lock the kernel releases when its holder dies, and only the
/// holder of that claim ever touches the path.
fn take_session(
    config: &Path,
    socket: &Path,
    key: &str,
    budget: Duration,
) -> Result<(UnixListener, File), (String, i32)> {
    let claim = match claim_host(config, key) {
        Ok(claim) => claim,
        Err(holder) => {
            // Somebody else is this session's host. Whatever is at that path is
            // their front door, and this process will not touch it.
            let by = match holder {
                Some(pid) => format!(" by process {pid}"),
                None => String::new(),
            };
            return Err(if socket_answers_within(socket, budget) {
                // The caller wanted a host for this session and there is one.
                (format!("session '{key}' is already served{by}"), 0)
            } else {
                (
                    format!(
                        "session '{key}' is held{by}, but nothing is answering on {}.                          Refusing to start a second host over it — that would strand                          whatever the first one is holding.",
                        socket.display()
                    ),
                    1,
                )
            });
        }
    };
    // The claim is held, so no live host can be listening here: the only
    // process allowed to bind this path is the one holding it, and that is now
    // us. Anything at the path is therefore a corpse — a host that was killed
    // without getting to tidy up — and this is the first moment at which
    // clearing it is safe rather than a guess.
    let _ = std::fs::remove_file(socket);
    match UnixListener::bind(socket) {
        Ok(listener) => Ok((listener, claim)),
        Err(err) => Err((format!("cannot listen on {}: {err}", socket.display()), 1)),
    }
}

/// Where a session's saved layout lives.
///
/// The layout under `sessions/` belongs to instance.rs, which puts every one of
/// a session's files there; this spells it out rather than calling in, because
/// the accessor there resolves the key from the environment and a host knows
/// its own key without asking.
fn state_file_for(config: &Path, key: &str) -> std::path::PathBuf {
    config.join("sessions").join(format!("{key}.toml"))
}

/// Take this session's host lock, or say who holds it.
///
/// `flock`, for the reason `instance::claim_in` gives for the window's own
/// lock: the kernel releases it when the last descriptor on the open file
/// description closes, so a crash, an OOM kill or a compositor restart can
/// never strand a session. A pid file can go stale and a socket file can go
/// stale; this cannot.
///
/// Deliberately **not** `instance::claim_in`'s lock, though it sits beside it.
/// That one answers *who may write this session's saved state*, and today that
/// is the window — a hosted window claims the session and then starts a host
/// for it, so a host taking the window's lock would refuse to start for the
/// very window that asked for it. This one answers *who is serving this
/// session's terminals*. The two become one question when the host becomes the
/// file's writer, and not before.
fn claim_host(config: &Path, key: &str) -> Result<File, Option<u32>> {
    use std::io::Write as _;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

    // The layout belongs to instance.rs, which keeps every session's files
    // under `sessions/`; the host's lock lives with them so that one directory
    // answers everything about a session.
    let sessions = config.join("sessions");
    if std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&sessions)
        .is_err()
        && !sessions.is_dir()
    {
        // Nowhere to arbitrate. Refusing is the fail-closed answer: an
        // unarbitrated host is indistinguishable from an arbitrated one, and
        // two of those is the bug.
        return Err(None);
    }
    let lock = sessions.join(format!("{key}.host.lock"));
    let Ok(mut file) = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(&lock)
    else {
        return Err(None);
    };
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        // Held. Read who by — the holder writes its pid in, so a person looking
        // at a session that will not start is told which process to look at
        // rather than left to guess.
        return Err(std::fs::read_to_string(&lock)
            .ok()
            .and_then(|body| body.trim().parse().ok()));
    }
    let _ = file.set_len(0);
    let _ = writeln!(file, "{}", std::process::id());
    Ok(file)
}

/// Whether something is listening on `socket` within `budget`.
///
/// A connection that is accepted is proof of a live listener, which a file at
/// the path is not.
fn socket_answers_within(socket: &Path, budget: Duration) -> bool {
    let deadline = std::time::Instant::now() + budget;
    loop {
        if UnixStream::connect(socket).is_ok() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// A host with real pseudoterminals and real children, driven the way a client
/// will drive it — but with both ends in one process, so the tests are fast
/// and nothing has to be cleaned up off-disk.
#[cfg(test)]
mod owning {
    use std::io::Read;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::hostproto::ClientKind;

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

    /// `cat` is the ideal child for these tests: it starts instantly, reads no
    /// startup files, prints no prompt, and echoes exactly what it is given.
    ///
    /// Passed in rather than set in the environment. Tests share one process,
    /// and `set_var` mutates a table every other thread may be reading — which
    /// is not a theory: doing it here made unrelated session tests fail about
    /// half the time, and they were the ones that looked broken.
    fn host_with_cat_pane() -> (Arc<Host>, PaneId) {
        let host = Host::with_shell("test", Some("/bin/cat".into()));
        let info = spawn_guarded(&host);
        (host, info.pane)
    }

    /// Say one verb over a connection of this test's own and take the answer.
    ///
    /// Over a real socket pair rather than with no connection at all, because
    /// that is how every control line reaches the host, and a verb whose answer
    /// depends on which connection asked would otherwise be tested against a
    /// shape a client cannot produce.
    /// Says hello first, as a tool, because every client does and because a
    /// connection that has not negotiated is now refused the verbs that change
    /// anything. A tool rather than a window so that reaching for this helper
    /// never quietly takes the session from a window a test set up.
    fn answer(host: &Arc<Host>, line: &str) -> Reply {
        let (_client, conn) = connection(host);
        handle_control_line(host, &hello_of(ClientKind::Tool), Some(&conn));
        handle_control_line(host, line, Some(&conn))
    }

    /// Say one verb over a connection that has said nothing at all.
    fn answer_cold(host: &Arc<Host>, line: &str) -> Reply {
        let (_client, conn) = connection(host);
        handle_control_line(host, line, Some(&conn))
    }

    /// A control connection that outlives one line, for the verbs whose answer
    /// depends on who has been saying what.
    ///
    /// The client half comes back with it and has to be kept: dropping it
    /// closes the socket under the host's own end, and a connection nobody is
    /// listening to is not the one a client would have opened.
    fn connection(host: &Arc<Host>) -> (UnixStream, Arc<Conn>) {
        let (client, server) = UnixStream::pair().expect("pair");
        let conn = Arc::new(Conn::new(
            host.next_conn.fetch_add(1, Ordering::SeqCst),
            server,
        ));
        (client, conn)
    }

    /// Say one verb on a connection that is having a conversation.
    fn say(host: &Arc<Host>, conn: &Arc<Conn>, line: &str) -> Reply {
        handle_control_line(host, line, Some(conn))
    }

    /// The greeting a client of that kind opens with.
    fn hello_of(kind: ClientKind) -> String {
        serde_json::to_string(&Request::Hello {
            proto: PROTO_VERSION,
            kind,
        })
        .expect("a hello")
    }

    /// Start a pane while holding the fork guard.
    ///
    /// Starting a terminal forks, and a fork briefly hands the child every
    /// descriptor this process holds — including locks another test is in the
    /// middle of asserting about. See `testsync`.
    fn spawn_guarded(host: &Arc<Host>) -> PaneInfo {
        let _guard = crate::testsync::forks_and_locks();
        host.spawn_pane(None, None, PaneGeom::default())
            .expect("start a pane")
            .info
    }

    #[test]
    fn a_pane_is_a_real_child_that_echoes_what_it_is_told() {
        let (host, pane) = host_with_cat_pane();
        assert!(host.write_to(pane, b"marco\n".to_vec()));

        assert!(
            within(Duration::from_secs(5), || {
                host.row_text(pane, 0).as_deref() == Some("marco")
            }),
            "the pane never showed what was typed into it: {:?}",
            host.row_text(pane, 0)
        );
        let info = &host.list_panes()[0];
        assert!(info.shell_pid > 0, "a pane has a real process");
        assert!(!info.attached, "nobody is watching yet");
    }

    #[test]
    fn attaching_delivers_the_history_it_missed_and_then_the_live_bytes() {
        // The whole point of the handover: a client that was not there for the
        // first line still sees it, and sees the second line after it, once.
        let (host, pane) = host_with_cat_pane();
        host.write_to(pane, b"before you arrived\n".to_vec());
        assert!(within(Duration::from_secs(5), || {
            host.row_text(pane, 0).as_deref() == Some("before you arrived")
        }));

        let (client, server) = UnixStream::pair().expect("socket pair");
        assert!(host.attach(pane, server).is_ok());

        host.write_to(pane, b"after you arrived\n".to_vec());

        let mut client = client;
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut seen = Vec::new();
        let mut buf = [0u8; 8192];
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            match client.read(&mut buf) {
                Ok(0) => break,
                Ok(read) => {
                    seen.extend_from_slice(&buf[..read]);
                    let text = String::from_utf8_lossy(&seen);
                    if text.contains("before you arrived") && text.contains("after you arrived") {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let text = String::from_utf8_lossy(&seen);
        let (snapshot, live) = split_snapshot(&text);
        assert!(
            snapshot.contains("before you arrived"),
            "the snapshot did not carry what the client missed: {snapshot:?}"
        );
        assert!(
            live.contains("after you arrived"),
            "live output did not follow the snapshot: {live:?}"
        );
        // The fence's promise, stated exactly: a byte is in the snapshot or in
        // the live stream, never in both. Counting occurrences cannot say this
        // — a pseudoterminal echoes what is typed into it, so every line
        // legitimately appears twice — which is how an earlier version of this
        // test managed to be flaky and right-looking at the same time.
        assert!(
            !live.contains("before you arrived"),
            "output already in the snapshot was sent live as well: {live:?}"
        );
    }

    /// Split what a client received into the snapshot and everything after it.
    ///
    /// A snapshot ends by restoring the modes, and line wrap is the last one
    /// written, so its final bytes are that mode change. Ordinary terminal
    /// output does not contain it.
    fn split_snapshot(text: &str) -> (&str, &str) {
        const TAIL: &str = "\x1b[?7";
        match text.rfind(TAIL) {
            // the mode letter follows the code
            Some(at) => text.split_at((at + TAIL.len() + 1).min(text.len())),
            None => ("", text),
        }
    }

    #[test]
    fn a_second_attach_supersedes_the_first() {
        // A window relaunching must not be refused by the ghost of the window
        // it replaces, so the newest attach wins and the old stream closes.
        let (host, pane) = host_with_cat_pane();
        let (first_client, first_server) = UnixStream::pair().expect("pair");
        assert!(host.attach(pane, first_server).is_ok());
        assert!(host.list_panes()[0].attached);

        let (_second_client, second_server) = UnixStream::pair().expect("pair");
        assert!(host.attach(pane, second_server).is_ok());

        let mut first_client = first_client;
        first_client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut buf = [0u8; 4096];
        let mut closed = false;
        for _ in 0..64 {
            match first_client.read(&mut buf) {
                Ok(0) => {
                    closed = true;
                    break;
                }
                Ok(_) => continue, // its own snapshot, still draining
                Err(_) => {
                    closed = true;
                    break;
                }
            }
        }
        assert!(
            closed,
            "the superseded client was left holding a live stream"
        );
        assert!(host.list_panes()[0].attached, "the new client holds it");
    }

    #[test]
    fn a_superseded_client_leaving_does_not_take_its_successors_stream() {
        // Found by driving the real binary, and invisible to any test that
        // attaches once. The superseded window is still reading when it is
        // replaced; it notices a moment later and tidies up on the way out.
        // Without an identity on the attachment it tidies away whatever it
        // finds — which is by then the new window's stream — and the terminal
        // that just came back goes dead a second after it arrived.
        let (host, pane) = host_with_cat_pane();

        let (_first_client, first_server) = UnixStream::pair().expect("pair");
        let first = match host.attach(pane, first_server) {
            Outcome::Ok(held) => held,
            Outcome::Err(err) => panic!("{err}"),
        };

        let (_second_client, second_server) = UnixStream::pair().expect("pair");
        let second = match host.attach(pane, second_server) {
            Outcome::Ok(held) => held,
            Outcome::Err(err) => panic!("{err}"),
        };
        assert_ne!(first.serial, second.serial);

        // The superseded client's reader now exits and releases what it holds.
        host.detach(pane, first.serial);

        assert!(
            host.list_panes()[0].attached,
            "the departing client released its successor's stream"
        );

        // And when the current one leaves, it does release.
        host.detach(pane, second.serial);
        assert!(!host.list_panes()[0].attached);
    }

    #[test]
    fn a_client_going_away_kills_nothing() {
        // The product promise, at its smallest: closing a window is not
        // closing a terminal.
        let (host, pane) = host_with_cat_pane();
        let (client, server) = UnixStream::pair().expect("pair");
        assert!(host.attach(pane, server).is_ok());
        drop(client);

        host.write_to(pane, b"still here\n".to_vec());
        assert!(
            within(Duration::from_secs(5), || {
                host.row_text(pane, 0).as_deref() == Some("still here")
            }),
            "the pane died with its client"
        );
        assert_eq!(host.pane_count(), 1);
        assert!(!host.list_panes()[0].ended, "the child is still running");
    }

    #[test]
    fn closing_a_pane_hangs_up_its_process_tree() {
        // The other half of the same promise: an explicit close is intent, and
        // intent kills.
        let (host, pane) = host_with_cat_pane();
        let pid = host.list_panes()[0].shell_pid;

        let outcome = host.close_pane(pane);
        assert!(outcome.is_ok(), "{outcome:?}");
        assert_eq!(host.pane_count(), 0, "the pane is gone from the table");

        assert!(
            within(Duration::from_secs(5), || {
                // ESRCH: no such process. Signal 0 asks without sending.
                (unsafe { libc::kill(pid as i32, 0) }) != 0
            }),
            "the child outlived the close"
        );
    }

    #[test]
    fn the_wire_answers_every_verb_and_refuses_nonsense() {
        let (host, pane) = host_with_cat_pane();

        let hello = answer(
            &host,
            &serde_json::to_string(&Request::Hello {
                proto: PROTO_VERSION,
                kind: ClientKind::Window,
            })
            .unwrap(),
        );
        assert!(matches!(hello, Reply::Hello { panes: 1, .. }), "{hello:?}");

        let listed = answer(&host, r#"{"verb":"list-panes"}"#);
        assert!(matches!(listed, Reply::Panes { ref panes } if panes.len() == 1));

        let resized = answer(
            &host,
            &serde_json::to_string(&Request::Resize {
                pane,
                geom: PaneGeom {
                    cols: 100,
                    rows: 30,
                    cell_width: 8,
                    cell_height: 16,
                },
            })
            .unwrap(),
        );
        assert!(matches!(
            resized,
            Reply::Resized {
                outcome: Outcome::Ok(()),
                ..
            }
        ));
        assert_eq!(host.list_panes()[0].geom.cols, 100);

        // A version we cannot speak is refused by name, not ignored.
        let mismatched = answer(&host, r#"{"verb":"hello","proto":99,"kind":"window"}"#);
        match mismatched {
            Reply::Error { msg } => assert!(msg.contains("99"), "{msg}"),
            other => panic!("a bad version was accepted: {other:?}"),
        }

        // Garbage gets an answer too. Silence is what started all of this.
        assert!(matches!(
            answer(&host, "{not json at all"),
            Reply::Error { .. }
        ));
        assert!(matches!(
            answer(&host, r#"{"verb":"teleport"}"#),
            Reply::Error { .. }
        ));

        // A verb naming a pane that does not exist says so.
        let closed = answer(&host, r#"{"verb":"close-pane","pane":9999}"#);
        assert!(
            matches!(
                closed,
                Reply::Closed {
                    outcome: Outcome::Err(_),
                    ..
                }
            ),
            "{closed:?}"
        );
    }

    #[test]
    fn a_session_stops_starting_terminals_at_the_host_cap() {
        // Gate 3 put a generous ceiling on the host and nothing implemented
        // it. The window's own cap of four protects the window and nothing
        // else: anything that can open the socket can ask for terminals in a
        // loop, and in a house that runs agents in loops that is a way to lose
        // a machine rather than a hypothetical.
        let host = Host::with_shell("test", Some("/bin/cat".into()));
        let _guard = crate::testsync::forks_and_locks();
        for n in 0..HOST_PANE_SANITY_CAP {
            assert!(
                host.spawn_pane(None, None, PaneGeom::default()).is_ok(),
                "the host refused terminal {n}, below its own cap"
            );
        }
        let Err(refused) = host.spawn_pane(None, None, PaneGeom::default()) else {
            panic!("the cap let one more terminal through");
        };
        assert!(
            refused.to_string().contains("64"),
            "refused without saying what the limit is: {refused}"
        );
        assert_eq!(host.pane_count(), HOST_PANE_SANITY_CAP);
    }

    #[test]
    fn terminals_whose_children_have_gone_do_not_pile_up() {
        // A host lives for hours and holds every pane whose child exited, so
        // an ordinary session's churn grows the table for as long as the host
        // runs. They cannot go the instant they die — a window that has lost a
        // stream asks whether the pane ended or was taken, and a forgotten
        // pane answers "ended" to both — so they are kept, and bounded.
        let host = Host::with_shell("test", Some("/bin/true".into()));
        let _guard = crate::testsync::forks_and_locks();
        let wanted = EXITED_KEEP + 6;
        for _ in 0..wanted {
            host.spawn_pane(None, None, PaneGeom::default())
                .expect("start a terminal");
        }
        // Not a count: the sweep runs on every spawn, so the dead are already
        // being bounded while this loop is still spawning, and waiting for
        // twenty-two corpses to be visible at once would be waiting for the
        // thing under test to fail.
        assert!(
            within(Duration::from_secs(10), || {
                let held = host.list_panes();
                !held.is_empty() && held.iter().all(|p| p.ended)
            }),
            "the children never exited: {:?}",
            host.list_panes()
        );

        // The next spawn is where the table is swept — the one place every
        // pane this host will ever hold passes through.
        host.spawn_pane(None, None, PaneGeom::default())
            .expect("start a terminal");
        let held = host.list_panes();
        let dead = held.iter().filter(|p| p.ended).count();
        assert!(
            dead <= EXITED_KEEP,
            "the host is holding {dead} dead terminals: {held:?}"
        );

        // And what it kept is the recent ones: a window asking about the pane
        // that died a moment ago must still get an answer, while the one that
        // died twenty terminals back is nobody's question any more.
        assert!(
            held.iter().any(|p| p.pane.0 == wanted as u64),
            "the pane that died last is the one it forgot: {held:?}"
        );
        assert!(
            !held.iter().any(|p| p.pane.0 == 1),
            "the first terminal of the session is still in the table: {held:?}"
        );
    }

    #[test]
    fn a_connection_that_never_said_hello_cannot_change_anything() {
        // The protocol has always said hello opens a connection. The host kept
        // no memory of one line to the next, so it did not: anything that could
        // reach the socket could close a terminal or stop the host without ever
        // stating a version it speaks, and one of our own integration tests
        // relied on that to do exactly those two things.
        let scratch = scratch("cold");
        let (host, pane) = host_with_cat_pane();
        host.persist_to(state_file_in(&scratch));

        match answer_cold(&host, &format!(r#"{{"verb":"close-pane","pane":{pane}}}"#)) {
            Reply::Closed {
                outcome: Outcome::Err(msg),
                ..
            } => assert!(msg.contains("hello"), "{msg}"),
            other => panic!("a cold connection closed a terminal: {other:?}"),
        }
        assert!(
            matches!(
                answer_cold(
                    &host,
                    &save_line("active = 0\n\n[[tabs]]\n\n[tabs.node.Leaf]\ncwd = \"/tmp\"\n")
                ),
                Reply::Saved {
                    outcome: Outcome::Err(_)
                }
            ),
            "a cold connection wrote the session file"
        );
        assert!(
            matches!(
                answer_cold(
                    &host,
                    &serde_json::to_string(&Request::SpawnPane {
                        cwd: None,
                        resume: None,
                        geom: PaneGeom::default(),
                    })
                    .unwrap()
                ),
                Reply::Spawned {
                    outcome: Outcome::Err(_),
                    ..
                }
            ),
            "a cold connection started a terminal"
        );
        assert!(
            matches!(
                answer_cold(&host, r#"{"verb":"shutdown"}"#),
                Reply::Error { .. }
            ),
            "a cold connection stopped the host"
        );

        // The terminal it tried to close is still running, and questions are
        // still free: a gate on everything would refuse a probe for asking.
        match answer_cold(&host, r#"{"verb":"list-panes"}"#) {
            Reply::Panes { panes } => assert_eq!(panes.len(), 1, "{panes:?}"),
            other => panic!("a question was refused: {other:?}"),
        }
    }

    #[test]
    fn a_host_asked_to_stand_down_writes_what_it_had_before_it_goes() {
        // What makes a protocol break survivable. The old host is asked to go
        // by a build that cannot speak to it, and everything the session was
        // has to be in the file by the time it does — the newer build has no
        // other way to learn it. A checkpoint clock that last ran five minutes
        // ago is not that.
        let scratch = scratch("standdown");
        let path = state_file_in(&scratch);
        let host = Host::with_shell("test", None);
        host.persist_to(path.clone());
        assert!(matches!(
            answer(
                &host,
                &save_line("active = 0\n\n[[tabs]]\nname = \"WHAT WAS RUNNING\"\n\n[tabs.node.Leaf]\ncwd = \"/tmp\"\n")
            ),
            Reply::Saved { outcome: Outcome::Ok(_) }
        ));
        // Taken away, so that what is there afterwards can only have been
        // written on the way out. A file changed underneath is a different
        // case with a rule of its own — the host adopts it — and this test
        // would be answering that question instead of this one.
        std::fs::remove_file(&path).expect("take the file away");
        let (_, checkpoints) = host.upkeep_counts();

        assert!(matches!(
            answer(&host, r#"{"verb":"shutdown"}"#),
            Reply::ShuttingDown
        ));

        assert_eq!(
            host.upkeep_counts().1,
            checkpoints + 1,
            "standing down did not checkpoint"
        );
        let left = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("a host stood down without writing anything: {e}"));
        assert!(
            left.contains("WHAT WAS RUNNING"),
            "a host stood down without writing what it was holding: {left}"
        );
    }

    #[test]
    fn a_peer_whose_version_we_cannot_speak_may_still_ask_us_to_stand_down() {
        // The carve-out the gate is built around, and the reason it is a
        // carve-out rather than an oversight: the approved path through a
        // protocol break is that the old host checkpoints and goes, and the
        // only peer who can ask for that is the one that was just refused.
        let (host, _pane) = host_with_cat_pane();
        let (_client, conn) = connection(&host);

        match say(
            &host,
            &conn,
            r#"{"verb":"hello","proto":99,"kind":"window"}"#,
        ) {
            Reply::Error { msg } => assert!(msg.contains("99"), "{msg}"),
            other => panic!("a version we cannot speak was accepted: {other:?}"),
        }
        // It may not touch the session it cannot describe.
        assert!(matches!(
            say(&host, &conn, r#"{"verb":"close-pane","pane":1}"#),
            Reply::Closed {
                outcome: Outcome::Err(_),
                ..
            }
        ));
        // And it may ask for the one thing that makes recovery possible.
        assert!(
            matches!(
                say(&host, &conn, r#"{"verb":"shutdown"}"#),
                Reply::ShuttingDown
            ),
            "the refused peer could not ask the host to stand down"
        );
    }

    #[test]
    fn a_second_window_takes_the_session_and_the_first_keeps_only_its_questions() {
        // The steal Gate 2 approved and nobody built. Two windows want one
        // session — a relaunch beside a window that is already up, most often
        // — and the newest wins, because it is the one a person is looking at.
        let scratch = scratch("steal");
        let (host, pane) = host_with_cat_pane();
        host.persist_to(state_file_in(&scratch));
        let layout = "active = 0\n\n[[tabs]]\n\n[tabs.node.Leaf]\ncwd = \"/tmp\"\n";

        let (_first_client, first) = connection(&host);
        assert!(matches!(
            say(&host, &first, &hello_of(ClientKind::Window)),
            Reply::Hello { .. }
        ));
        let (mut watching, server) = UnixStream::pair().expect("pair");
        assert!(host.attach(pane, server).is_ok());
        assert!(host.list_panes()[0].attached);
        assert!(
            matches!(
                say(&host, &first, &save_line(layout)),
                Reply::Saved {
                    outcome: Outcome::Ok(_)
                }
            ),
            "the window holding the session could not write it"
        );

        // The second window arrives, and says what it is.
        let (_second_client, second) = connection(&host);
        assert!(matches!(
            say(&host, &second, &hello_of(ClientKind::Window)),
            Reply::Hello { .. }
        ));

        // The loser's stream is over. It finds that out the way a real window
        // does: the socket it was reading from ends.
        assert!(
            !host.list_panes()[0].attached,
            "the superseded window was left holding the pane's stream"
        );
        watching
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("a timeout");
        let mut buf = [0u8; 4096];
        let mut closed = false;
        for _ in 0..64 {
            match watching.read(&mut buf) {
                Ok(0) | Err(_) => {
                    closed = true;
                    break;
                }
                Ok(_) => continue, // its own snapshot, still draining
            }
        }
        assert!(closed, "the superseded window's stream is still live");

        // And it may no longer change anything — the session file least of
        // all, which is the invariant the whole split rests on.
        match say(&host, &first, &save_line(layout)) {
            Reply::Saved {
                outcome: Outcome::Err(msg),
            } => assert!(msg.contains("another window"), "{msg}"),
            other => panic!("a superseded window still wrote the session: {other:?}"),
        }
        match say(&host, &first, r#"{"verb":"close-pane","pane":1}"#) {
            Reply::Closed {
                outcome: Outcome::Err(_),
                ..
            } => {}
            other => panic!("a superseded window still closed a terminal: {other:?}"),
        }

        // What it keeps is the ability to ask, and it needs it: a stream that
        // ended because the pane was taken and one that ended because the
        // program exited arrive identically, and this is the only way to tell
        // them apart.
        match say(&host, &first, r#"{"verb":"list-panes"}"#) {
            Reply::Panes { panes } => {
                assert_eq!(panes.len(), 1, "{panes:?}");
                assert!(!panes[0].ended, "the terminal is still running: {panes:?}");
            }
            other => panic!("a superseded window cannot ask what happened: {other:?}"),
        }

        // The winner has the session whole.
        assert!(matches!(
            say(&host, &second, &save_line(layout)),
            Reply::Saved {
                outcome: Outcome::Ok(_)
            }
        ));
    }

    #[test]
    fn a_tools_hello_takes_nothing_from_the_window_using_the_session() {
        // The other half of the same rule, and the reason kind is on the wire
        // at all: resolving a session probes every candidate host, so if
        // asking could steal, every launch would rob the window already
        // running.
        let scratch = scratch("toolhello");
        let (host, pane) = host_with_cat_pane();
        host.persist_to(state_file_in(&scratch));
        let layout = "active = 0\n\n[[tabs]]\n\n[tabs.node.Leaf]\ncwd = \"/tmp\"\n";

        let (_window_client, window) = connection(&host);
        say(&host, &window, &hello_of(ClientKind::Window));
        let (_client, server) = UnixStream::pair().expect("pair");
        assert!(host.attach(pane, server).is_ok());

        let (_tool_client, tool) = connection(&host);
        say(&host, &tool, &hello_of(ClientKind::Tool));
        say(&host, &tool, r#"{"verb":"list-panes"}"#);

        assert!(
            host.list_panes()[0].attached,
            "a tool saying hello took the window's stream"
        );
        assert!(
            matches!(
                say(&host, &window, &save_line(layout)),
                Reply::Saved {
                    outcome: Outcome::Ok(_)
                }
            ),
            "a tool saying hello cost the window the session"
        );
    }

    /// A save of `layout`, as a client would send it.
    fn save_line(layout: &str) -> String {
        serde_json::to_string(&Request::Save {
            schema: LAYOUT_SCHEMA,
            body: layout.to_string(),
            allow_shrink: false,
        })
        .expect("a save")
    }

    #[test]
    fn a_tool_asking_questions_never_costs_a_window_its_pane() {
        // Attaching is a property of opening a byte stream, not of saying
        // hello — a tool's questions cost a window nothing, whatever it asks.
        // Only a window's own hello takes a session, and it takes it from
        // another window (see the steal above). It matters because resolving a
        // session probes every candidate host, and if asking could steal, then
        // every launch would detach the window already running.
        let (host, pane) = host_with_cat_pane();
        let (_client, server) = UnixStream::pair().expect("pair");
        assert!(host.attach(pane, server).is_ok());
        assert!(host.list_panes()[0].attached);

        for line in [
            serde_json::to_string(&Request::Hello {
                proto: PROTO_VERSION,
                kind: ClientKind::Tool,
            })
            .unwrap(),
            r#"{"verb":"list-panes"}"#.to_string(),
        ] {
            answer(&host, &line);
        }
        assert!(
            host.list_panes()[0].attached,
            "a tool's questions detached the window"
        );
    }

    #[test]
    fn a_pane_knows_which_session_and_pane_it_is() {
        // The stale comment made true: identity is handed down rather than
        // inferred by walking /proc, which is where the cross-wiring came from.
        let host = Host::with_shell("session-under-test", Some("/bin/cat".into()));
        let info = spawn_guarded(&host);

        let environ = std::fs::read(format!("/proc/{}/environ", info.shell_pid))
            .expect("read the child's environment");
        let environ = String::from_utf8_lossy(&environ);
        assert!(
            environ.contains("TD_SESSION=session-under-test"),
            "the child was not told its session"
        );
        assert!(
            environ.contains(&format!("TD_PANE_ID={}", info.pane)),
            "the child was not told its pane"
        );
    }

    #[test]
    fn the_watcher_names_what_is_running_in_a_real_pane() {
        // The relocated question, asked where it can now be answered: only the
        // process holding the pseudoterminal can call tcgetpgrp on it.
        let (host, _pane) = host_with_cat_pane();
        assert_eq!(
            host.list_panes()[0].mode,
            None,
            "a pane nobody has looked at claims to be nothing"
        );

        // A child that has been forked but has not yet taken the terminal has
        // no foreground group, so the first tick may legitimately learn
        // nothing. Polling is the honest way to wait for that.
        assert!(
            within(Duration::from_secs(5), || {
                host.watch_once();
                host.list_panes()[0].mode.is_some()
            }),
            "the watcher never classified a live pane"
        );
        assert_eq!(
            host.list_panes()[0].mode,
            Some(WireMode::Shell),
            "this pane's own program is what is in front of it"
        );
    }

    #[test]
    fn the_naming_rule_is_the_one_the_window_has_always_used() {
        assert_eq!(classify("claude", "claude"), WireMode::Claude);
        assert_eq!(
            classify("node", "/home/me/.local/bin/claude --resume 4a1c"),
            WireMode::Claude,
            "an agent run through a launcher is still the agent"
        );
        assert_eq!(classify("codex", "codex"), WireMode::Codex);
        assert_eq!(classify("ssh", "ssh box"), WireMode::Remote);
        assert_eq!(classify("bash", "-bash"), WireMode::Shell);
        assert_eq!(
            classify("vim", "vim src/host.rs"),
            WireMode::Other("vim".into()),
            "anything else is reported by name rather than lumped in"
        );
    }

    #[test]
    fn an_agent_keeps_its_name_through_the_children_it_runs() {
        // An agent shells out constantly; each child is the foreground group
        // for as long as it runs. Renaming the pane every time flickers the
        // header, so the agent's own screen holds its identity.
        let claude = WireMode::Claude;
        assert_eq!(
            next_mode(Some(&claude), Some(WireMode::Shell), true),
            Some(WireMode::Claude),
            "a child process renamed the pane out from under the agent"
        );
        // And when the agent has actually gone, the demotion is real.
        assert_eq!(
            next_mode(Some(&claude), Some(WireMode::Shell), false),
            Some(WireMode::Shell),
            "the pane stayed an agent after the agent exited"
        );
        // A promotion is never held back by the rule.
        assert_eq!(
            next_mode(Some(&WireMode::Shell), Some(WireMode::Codex), true),
            Some(WireMode::Codex)
        );
    }

    #[test]
    fn a_mode_the_kernel_would_not_give_never_overwrites_one_it_did() {
        // The whole reason classify_foreground answers with an Option. A window
        // could collapse "cannot say" into "shell" because it was looking at
        // its own terminal; this value travels to a process that cannot tell
        // the two apart.
        assert_eq!(
            next_mode(Some(&WireMode::Claude), None, false),
            Some(WireMode::Claude),
            "a failed reading erased a real one"
        );
        assert_eq!(next_mode(None, None, false), None);

        // And the failure is real: a file that is not a terminal has no
        // foreground process group.
        let not_a_terminal = File::open("/dev/null").expect("/dev/null");
        assert_eq!(classify_foreground(&not_a_terminal, 1), None);
    }

    #[test]
    fn the_checkpoint_reads_where_a_pane_actually_is() {
        // The other half of the relocation. `cwd` on the wire is a reading
        // taken through the pseudoterminal, not the directory the pane was
        // asked to start in — the two stop being the same thing the moment
        // somebody types `cd`.
        let host = Host::with_shell("test", Some("/bin/cat".into()));
        let info = spawn_guarded(&host);
        assert_eq!(
            info.cwd, None,
            "nothing was asked for, so nothing is claimed"
        );

        assert!(
            within(Duration::from_secs(5), || {
                host.checkpoint_once();
                host.list_panes()[0].cwd.is_some()
            }),
            "the checkpoint never read the pane's directory"
        );
        let read = host.list_panes()[0].cwd.clone().expect("a directory");
        assert_eq!(
            std::fs::canonicalize(&read).expect("the reported directory exists"),
            std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap(),
            "the checkpoint reported a directory the pane is not in"
        );
    }

    /// Start a pane holding a named conversation, under the fork guard.
    fn spawn_agent(host: &Arc<Host>, recipe: &str) -> Spawn {
        let _guard = crate::testsync::forks_and_locks();
        host.spawn_pane(None, Some(recipe.into()), PaneGeom::default())
            .expect("start a pane")
    }

    #[test]
    fn a_session_will_not_run_the_same_agent_twice() {
        // #339. A window decides what to start by comparing a saved layout
        // against a list of panes taken a moment earlier, so an agent can begin
        // in a pane that list never showed and the window starts a second copy.
        // Two agents on one conversation, both billing, both writing the same
        // transcript. The check has to happen where the spawning does.
        let host = Host::with_shell("test", Some("/bin/cat".into()));
        let first = spawn_agent(&host, "claude --resume 48be90b8");
        assert!(first.started, "the first one is a real terminal");

        let second = spawn_agent(&host, "claude --resume 48be90b8");
        assert!(
            !second.started,
            "a second terminal was started for a conversation already running in one"
        );
        assert_eq!(
            second.info.pane, first.info.pane,
            "the answer must name the pane already running it"
        );
        assert_eq!(host.pane_count(), 1, "two terminals exist for one agent");

        // A different conversation is a different terminal.
        let other = spawn_agent(&host, "claude --resume 0000ffff");
        assert!(other.started);
        assert_eq!(host.pane_count(), 2);
    }

    #[test]
    fn a_pane_with_no_recipe_is_deduplicated_against_nothing() {
        // An ordinary terminal has no identity to be the same as. Two of them
        // are two of them, which is what asking for a second one means.
        let host = Host::with_shell("test", Some("/bin/cat".into()));
        spawn_guarded(&host);
        spawn_guarded(&host);
        assert_eq!(host.pane_count(), 2);
    }

    #[test]
    fn a_conversation_whose_terminal_has_ended_is_started_again() {
        // A pane whose child has gone is not running anything, whatever it was
        // started to run. Binding a leaf to a corpse would show somebody a dead
        // screen where their agent should be.
        let host = Host::with_shell("test", Some("/bin/cat".into()));
        let first = spawn_agent(&host, "claude --resume 48be90b8");
        let pid = first.info.shell_pid;
        assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGTERM) }, 0);
        assert!(
            within(Duration::from_secs(5), || host.list_panes()[0].ended),
            "the pane never registered that its child had gone"
        );

        let again = spawn_agent(&host, "claude --resume 48be90b8");
        assert!(
            again.started,
            "a conversation whose terminal had ended was refused a new one"
        );
        assert_ne!(again.info.pane, first.info.pane);
    }

    #[test]
    fn the_host_types_the_recipe_it_was_handed() {
        // It types it because it is the thing that decided not to type it a
        // second time. Split between the two and the refusal means nothing —
        // the window would go on typing into a pane it did not start.
        let host = Host::with_shell("test", Some("/bin/cat".into()));
        let spawn = spawn_agent(&host, "marco");
        assert!(
            within(Duration::from_secs(5), || {
                host.row_text(spawn.info.pane, 0).as_deref() == Some("marco")
            }),
            "the host never typed what it was handed: {:?}",
            host.row_text(spawn.info.pane, 0)
        );
    }

    #[test]
    fn a_pane_whose_child_has_gone_keeps_the_last_directory_it_was_in() {
        // Only an answer replaces an answer. The reading fails the moment the
        // child does — `/proc` goes with it — and a reading that could not be
        // taken is not a pane in no directory: it is a pane nobody could ask.
        //
        // This is the pane a saved layout most needs to be right about. An
        // ended pane's leaf is what a restore reads to decide where to start
        // its replacement, and erasing the directory here would start it in the
        // wrong one.
        let (host, pane) = host_with_cat_pane();
        assert!(
            within(Duration::from_secs(5), || {
                host.checkpoint_once();
                host.list_panes()[0].cwd.is_some()
            }),
            "the pane was never read while it was alive"
        );
        let while_it_lived = host.list_panes()[0].cwd.clone().expect("a directory");

        let pid = host.list_panes()[0].shell_pid;
        assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGTERM) }, 0);
        assert!(
            within(Duration::from_secs(5), || host.list_panes()[0].ended),
            "the pane never registered that its child had gone"
        );
        assert!(
            within(Duration::from_secs(5), || {
                std::fs::read_link(format!("/proc/{pid}/cwd")).is_err()
            }),
            "the child's /proc entry is still readable, so this proves nothing"
        );

        host.checkpoint_once();
        assert_eq!(
            host.list_panes()[0].cwd.as_deref(),
            Some(while_it_lived.as_str()),
            "a reading that could not be taken erased one that had been"
        );
        let _ = pane;
    }

    #[test]
    fn a_host_nobody_is_watching_stops_polling_like_one_that_is() {
        // Hosts now outlive the windows watching them, so an idle one keeping
        // window cadence is a cost that scales with how well the feature works.
        let host = Host::with_shell("test", Some("/bin/cat".into()));
        let info = spawn_guarded(&host);
        host.start_upkeep(Cadence {
            watch_attached: Duration::from_millis(20),
            watch_detached: Duration::from_secs(30),
            checkpoint_attached: Duration::from_millis(20),
            checkpoint_detached: Duration::from_secs(30),
            ..never_idles()
        });

        // Detached: one pass each, and then a long sleep. Twenty milliseconds
        // of cadence over this long would be dozens.
        std::thread::sleep(Duration::from_millis(400));
        let (watches, checkpoints) = host.upkeep_counts();
        assert!(
            watches <= 2 && checkpoints <= 2,
            "a host nobody is watching kept polling: {watches} watches, {checkpoints} checkpoints"
        );

        // A window arrives, and must not wait out the rest of a detached sleep
        // to be served.
        let (_client, server) = UnixStream::pair().expect("pair");
        assert!(host.attach(info.pane, server).is_ok());
        assert!(
            within(Duration::from_secs(5), || {
                let (now_watches, now_checkpoints) = host.upkeep_counts();
                now_watches >= watches + 3 && now_checkpoints >= checkpoints + 3
            }),
            "attaching did not restore the attached cadence"
        );
    }

    #[test]
    fn a_grid_check_states_a_hash_and_the_offset_it_was_taken_at() {
        // The two ends have to be counting the same bytes, or the guard cannot
        // tell a client that is behind from one that is wrong.
        let (host, pane) = host_with_cat_pane();
        let (client, server) = UnixStream::pair().expect("pair");
        assert!(host.attach(pane, server).is_ok());
        host.write_to(pane, b"something to hash\n".to_vec());
        assert!(within(Duration::from_secs(5), || {
            host.row_text(pane, 0).as_deref() == Some("something to hash")
        }));

        let check = match host.grid_check(pane) {
            Outcome::Ok(check) => check,
            Outcome::Err(err) => panic!("{err}"),
        };
        assert_eq!(check.pane, pane);
        assert!(
            check.stream_offset > 0,
            "a snapshot alone is not zero bytes"
        );

        // What the host says it wrote is what the client can read: no more, and
        // no fewer.
        let mut client = client;
        client
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let mut read = 0u64;
        let mut buf = [0u8; 8192];
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && read < check.stream_offset {
            match client.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => read += n as u64,
                Err(_) => {}
            }
        }
        assert_eq!(
            read, check.stream_offset,
            "the host's count of what it wrote is not what the client received"
        );
    }

    #[test]
    fn the_hash_is_the_grid_and_moves_only_when_the_grid_does() {
        let (host, pane) = host_with_cat_pane();
        let (_client, server) = UnixStream::pair().expect("pair");
        assert!(host.attach(pane, server).is_ok());
        host.write_to(pane, b"first\n".to_vec());
        assert!(within(Duration::from_secs(5), || {
            host.row_text(pane, 0).as_deref() == Some("first")
        }));

        let hash_of = |host: &Arc<Host>| match host.grid_check(pane) {
            Outcome::Ok(check) => check.hash,
            Outcome::Err(err) => panic!("{err}"),
        };
        let settled = hash_of(&host);
        assert_eq!(settled, hash_of(&host), "a still terminal changed its hash");

        host.write_to(pane, b"second\n".to_vec());
        assert!(
            within(Duration::from_secs(5), || hash_of(&host) != settled),
            "the grid changed and the hash did not"
        );
    }

    #[test]
    fn a_pane_nobody_is_reading_has_no_offset_to_state() {
        // Unknown is not zero, on the wire most of all: a client handed 0 here
        // would compare its own offset against a number that means nothing.
        let (host, pane) = host_with_cat_pane();
        assert!(
            matches!(host.grid_check(pane), Outcome::Err(_)),
            "an unwatched pane claimed a stream offset"
        );
        assert!(matches!(host.grid_check(PaneId(9999)), Outcome::Err(_)));

        // Over the wire the same, with an answer rather than a silence.
        let checked = answer(&host, r#"{"verb":"grid-check","pane":9999}"#);
        assert!(
            matches!(
                checked,
                Reply::GridChecked {
                    outcome: Outcome::Err(_),
                    ..
                }
            ),
            "{checked:?}"
        );
    }

    #[test]
    fn a_pane_that_has_just_appeared_is_read_without_waiting_for_the_clock() {
        // Every period here is thirty seconds, so the only thing that can move
        // the count within the test is the spawn itself waking the clocks.
        let host = Host::with_shell("test", Some("/bin/cat".into()));
        host.start_upkeep(Cadence {
            watch_attached: Duration::from_secs(30),
            watch_detached: Duration::from_secs(30),
            checkpoint_attached: Duration::from_secs(30),
            checkpoint_detached: Duration::from_secs(30),
            ..never_idles()
        });
        std::thread::sleep(Duration::from_millis(100));
        let (_, before) = host.upkeep_counts();

        spawn_guarded(&host);

        assert!(
            within(Duration::from_secs(5), || host.upkeep_counts().1 > before),
            "a pane that had just appeared waited for the clock instead of waking it"
        );
        assert!(
            within(Duration::from_secs(5), || host.list_panes()[0]
                .cwd
                .is_some()),
            "and nothing had read where it was"
        );
    }

    /// A config directory and a socket path of this test's own.
    ///
    /// Passed in rather than set in the environment, for the reason the shell
    /// is: tests share one process, and `set_var` mutates a table every other
    /// thread may be reading.
    /// The paths a host test needs, inside a scratch directory that takes
    /// itself away.
    ///
    /// The guard is `testsync::Scratch`, shared with every other test helper in
    /// the suite — this file had its own copy of the same idea, and a rule with
    /// two implementations is a rule that drifts. The tag keeps the `claim-`
    /// prefix so a stray directory can still be attributed to these tests at a
    /// glance.
    ///
    /// **Bind the scratch to a name before using these.** `scratch_of(&Scratch::new("x"))`
    /// drops the guard at the end of that statement and takes the directory
    /// with it, which fails loudly if the test touches it afterwards and
    /// silently if it does not.
    fn scratch(tag: &str) -> crate::testsync::Scratch {
        crate::testsync::Scratch::new(&format!("claim-{tag}"))
    }

    /// Where this test's session files live.
    fn config_in(scratch: &crate::testsync::Scratch) -> std::path::PathBuf {
        scratch.join("config")
    }

    /// A socket path of its own, with the runtime directory made — a host binds
    /// into it, and `bind` will not create the parent.
    fn socket_in(scratch: &crate::testsync::Scratch) -> std::path::PathBuf {
        let run = scratch.join("run");
        std::fs::create_dir_all(&run).expect("a private runtime dir");
        run.join("session.sock")
    }

    /// The saved state file, with its directory already made — a test standing
    /// in for a person restoring a backup writes it before anything else has.
    fn state_file_in(scratch: &crate::testsync::Scratch) -> std::path::PathBuf {
        let sessions = config_in(scratch).join("sessions");
        std::fs::create_dir_all(&sessions).expect("a sessions directory");
        sessions.join("under-test.toml")
    }

    fn inode_of(path: &std::path::Path) -> u64 {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(path)
            .unwrap_or_else(|e| panic!("no {}: {e}", path.display()))
            .ino()
    }

    /// Short, because these tests all arrange for the answer to be immediate.
    const SOON: Duration = Duration::from_millis(200);

    #[test]
    fn a_second_host_never_takes_a_live_session_socket() {
        // #335. `serve` used to unlink whatever socket file it found and bind
        // its own, on the reasoning that a file left by a dead host is not a
        // running host. True, and it cannot tell that case from a live one — so
        // a second host took the first's front door and left it running, with
        // its terminals, and no name by which anything could reach them again.
        let _guard = crate::testsync::forks_and_locks();
        let scratch = scratch("clash");
        let (config, socket) = (config_in(&scratch), socket_in(&scratch));

        let (first, _claim) = take_session(&config, &socket, "clash", SOON)
            .expect("the first host takes the session");
        let front_door = inode_of(&socket);
        assert!(
            UnixStream::connect(&socket).is_ok(),
            "the first host is not listening, so this proves nothing"
        );

        let (why, code) = take_session(&config, &socket, "clash", SOON)
            .expect_err("a second host took a session that was already served");
        assert_eq!(
            code, 0,
            "a caller that wanted a host for this session has one: {why}"
        );

        assert_eq!(
            inode_of(&socket),
            front_door,
            "the second host replaced the first host's socket"
        );
        assert!(
            UnixStream::connect(&socket).is_ok(),
            "the first host was left running and unreachable"
        );
        drop(first);
    }

    #[test]
    fn a_socket_left_by_a_host_that_died_is_cleared_rather_than_honoured() {
        // The case the unconditional remove was written for, which has to keep
        // working: a host killed outright leaves its socket behind, and the
        // session must still be startable. A fix that refused here would trade
        // one unreachable session for a permanently unstartable one.
        let _guard = crate::testsync::forks_and_locks();
        let scratch = scratch("corpse");
        let (config, socket) = (config_in(&scratch), socket_in(&scratch));
        {
            let (listener, claim) =
                take_session(&config, &socket, "corpse", SOON).expect("the first host");
            drop(listener);
            drop(claim);
        }
        assert!(
            socket.exists(),
            "the corpse socket is what this test is about"
        );
        assert!(
            UnixStream::connect(&socket).is_err(),
            "nothing should be listening on a corpse"
        );

        let (_listener, _claim) = take_session(&config, &socket, "corpse", SOON)
            .expect("a socket left by a dead host blocked a fresh one");
    }

    #[test]
    fn a_session_held_by_something_that_never_answers_is_left_alone() {
        // The deliberate cost of refusing rather than taking over: a host that
        // holds its session and stops answering makes that session unstartable
        // until somebody looks. Loudly, and naming the process — because the
        // alternative, taking the session anyway, is killing terminals to fix a
        // terminal that might still be fine.
        let _guard = crate::testsync::forks_and_locks();
        let scratch = scratch("silent");
        let (config, socket) = (config_in(&scratch), socket_in(&scratch));
        let held = claim_host(&config, "silent").expect("hold the session");
        std::fs::write(&socket, b"whatever was here before").expect("something at the path");

        let (why, code) = take_session(&config, &socket, "silent", SOON)
            .expect_err("a second host started over a held session");
        assert_eq!(code, 1, "{why}");
        // The phrase, not the number: this test's own temp path carries the
        // process id too, so a looser assertion passed a version of the code
        // that named nothing at all.
        assert!(
            why.contains(&format!("process {}", std::process::id())),
            "a session that will not start must name what is holding it: {why}"
        );
        assert!(
            socket.exists(),
            "the second host removed a file that was not its to remove"
        );
        drop(held);
    }

    /// Read one line from a client, or say what arrived instead.
    fn one_line(client: &mut UnixStream, within: Duration) -> Option<String> {
        client.set_read_timeout(Some(within)).unwrap();
        let mut buf = [0u8; 4096];
        match client.read(&mut buf) {
            Ok(0) | Err(_) => None,
            Ok(read) => Some(String::from_utf8_lossy(&buf[..read]).into_owned()),
        }
    }

    /// A control connection of this test's own, registered to be told things.
    fn watching(host: &Arc<Host>) -> UnixStream {
        let (client, server) = UnixStream::pair().expect("pair");
        let conn = Arc::new(Conn::new(
            host.next_conn.fetch_add(1, Ordering::SeqCst),
            server,
        ));
        host.watch(&conn);
        client
    }

    #[test]
    fn a_connection_that_asked_is_told_when_a_pane_changes_what_it_runs() {
        // The relocation's other half. Until this existed the window ran its
        // own 800ms walk of /proc for every pane it was drawing, against panes
        // it no longer owns, duplicating the host's answer to the same
        // question.
        let (host, pane) = host_with_cat_pane();
        let mut window = watching(&host);

        // Prime it as an agent, so the tick has a real change to report: this
        // pane's program is `cat`, which reads as the shell it stands in for.
        *host.panes.lock().expect("panes")[&pane]
            .mode
            .lock()
            .expect("mode") = Some(WireMode::Claude);

        assert!(
            within(Duration::from_secs(5), || {
                host.watch_once();
                host.list_panes()[0].mode == Some(WireMode::Shell)
            }),
            "the watcher never saw the pane for what it is"
        );

        let line = one_line(&mut window, Duration::from_secs(5))
            .expect("a window that asked to be told was told nothing");
        let push: Push = serde_json::from_str(line.trim())
            .unwrap_or_else(|e| panic!("unreadable push {line:?}: {e}"));
        assert_eq!(
            push,
            Push::Mode {
                pane,
                mode: WireMode::Shell
            }
        );

        // And on the tick after, when nothing has changed, nothing is said. A
        // push per tick would be the polling it replaces, moved one process
        // over.
        host.watch_once();
        assert_eq!(
            one_line(&mut window, Duration::from_millis(200)),
            None,
            "the watcher pushed a change that did not happen"
        );
    }

    #[test]
    fn a_pane_that_changed_what_it_runs_is_read_without_waiting_for_the_checkpoint() {
        // What a pane is running changing is what makes its directory and its
        // resume recipe worth reading again — and something asks for them long
        // before the checkpoint comes round.
        //
        // A window planning an attach reads `list-panes` before anything it
        // does rings this host's clock, and binds a leaf whose pane is gone to
        // a live pane running exactly its resume line. On the checkpoint alone
        // that line is up to five minutes stale on a host nobody is watching,
        // so a pane whose agent had just started would read as no agent, and
        // the leaf would start `claude --resume <id>` in a second terminal.
        let (host, pane) = host_with_cat_pane();
        assert_eq!(host.list_panes()[0].cwd, None, "nothing has been read yet");

        // Prime it as an agent so the next tick is a change rather than a
        // repeat: this pane's own program is `cat`, which reads as the shell it
        // stands in for.
        *host.panes.lock().expect("panes")[&pane]
            .mode
            .lock()
            .expect("mode") = Some(WireMode::Claude);

        assert!(
            within(Duration::from_secs(5), || {
                host.watch_once();
                host.list_panes()[0].mode == Some(WireMode::Shell)
            }),
            "the watcher never saw the pane change"
        );

        let read = host.list_panes()[0]
            .cwd
            .clone()
            .expect("a pane that changed what it runs was never read");
        assert_eq!(
            std::fs::canonicalize(&read).expect("the reported directory exists"),
            std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap(),
            "the watcher recorded a directory the pane is not in"
        );
    }

    #[test]
    fn a_connection_that_never_asked_is_never_pushed_to() {
        // What makes this additive rather than a break: a client written before
        // pushes existed reads one line per verb it sent, and a line it did not
        // ask for is an error to it.
        let (host, pane) = host_with_cat_pane();
        let (mut quiet, server) = UnixStream::pair().expect("pair");
        let older = Arc::new(Conn::new(
            host.next_conn.fetch_add(1, Ordering::SeqCst),
            server,
        ));
        // Everything such a client does say, said: a hello and a question. It
        // is saying hello that must not sign it up for anything, which is the
        // easy mistake and the one that would break every client at once.
        for line in [
            r#"{"verb":"hello","proto":1,"kind":"window"}"#,
            r#"{"verb":"list-panes"}"#,
        ] {
            handle_control_line(&host, line, Some(&older));
        }
        let mut window = watching(&host);

        host.broadcast(&Push::Mode {
            pane,
            mode: WireMode::Codex,
        });

        assert!(
            one_line(&mut window, Duration::from_secs(5)).is_some(),
            "the connection that asked was told nothing"
        );
        assert_eq!(
            one_line(&mut quiet, Duration::from_millis(200)),
            None,
            "a connection that never asked to watch was pushed to anyway"
        );
    }

    #[test]
    fn a_connection_that_has_gone_stops_being_pushed_to() {
        // A subscription that outlived its socket would be a failed write on
        // every tick for the life of the host.
        let (host, pane) = host_with_cat_pane();
        let window = watching(&host);
        assert_eq!(host.watchers.lock().expect("watchers").len(), 1);

        drop(window);
        // The first push after it went is what discovers it.
        for _ in 0..4 {
            host.broadcast(&Push::Mode {
                pane,
                mode: WireMode::Shell,
            });
        }
        assert!(
            host.watchers.lock().expect("watchers").is_empty(),
            "a dead connection is still on the list"
        );
    }

    #[test]
    fn leaving_a_full_screen_program_hands_over_the_history_it_was_hiding() {
        // A snapshot reads the grid that is active, so a client attaching to a
        // pane running vim is sent the alternate screen and nothing behind it.
        // Its scrollback starts empty and live output will never fill it, since
        // that history was written before it arrived.
        let (host, pane) = host_with_cat_pane();
        host.write_to(
            pane,
            b"history written before anyone was watching\n".to_vec(),
        );
        assert!(within(Duration::from_secs(5), || {
            host.row_text(pane, 0).as_deref() == Some("history written before anyone was watching")
        }));

        // Into the alternate screen, the way a full-screen program goes. The
        // newline matters: the line discipline holds a line until one arrives,
        // so without it the escape sits in the kernel's buffer and `cat` never
        // writes it back for the emulator to read.
        host.write_to(pane, b"\x1b[?1049h\n".to_vec());
        assert!(
            within(Duration::from_secs(5), || on_alt(&host, pane)),
            "the pane never entered the alternate screen"
        );

        let (mut client, server) = UnixStream::pair().expect("pair");
        assert!(host.attach(pane, server).is_ok());
        let arrived = drain(&mut client, Duration::from_millis(400));
        assert!(
            !arrived.contains("history written before anyone was watching"),
            "this test proves nothing unless the client really did miss it: {arrived:?}"
        );

        // The watcher has to have seen it up, to notice it come down.
        host.watch_once();

        host.write_to(pane, b"\x1b[?1049l\n".to_vec());
        assert!(
            within(Duration::from_secs(5), || !on_alt(&host, pane)),
            "the pane never left the alternate screen"
        );
        host.watch_once();

        let healed = drain(&mut client, Duration::from_secs(2));
        assert!(
            healed.contains("history written before anyone was watching"),
            "the client was left with an empty scrollback behind the screen it \
             had been watching: {healed:?}"
        );
    }

    fn on_alt(host: &Arc<Host>, pane: PaneId) -> bool {
        host.panes.lock().expect("panes")[&pane]
            .term
            .lock()
            .mode()
            .contains(TermMode::ALT_SCREEN)
    }

    /// Everything a client can be given inside `patience`.
    fn drain(client: &mut UnixStream, patience: Duration) -> String {
        client.set_read_timeout(Some(patience)).unwrap();
        let mut seen = Vec::new();
        let mut buf = [0u8; 8192];
        let deadline = Instant::now() + patience;
        while Instant::now() < deadline {
            match client.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(read) => seen.extend_from_slice(&buf[..read]),
            }
        }
        String::from_utf8_lossy(&seen).into_owned()
    }

    /// Wait for a client's stream to end, and say what it saw first.
    fn read_to_hangup(client: &mut UnixStream, patience: Duration) -> Option<String> {
        // Short timeout, long deadline. A quiet stretch is what waiting looks
        // like — the program has to be signalled, notice, and exit — so a read
        // that times out means keep waiting, and only an end or a real error
        // stops this.
        client
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let mut seen = Vec::new();
        let mut buf = [0u8; 8192];
        let deadline = Instant::now() + patience;
        while Instant::now() < deadline {
            match client.read(&mut buf) {
                // The stream ended, which is the thing being waited for.
                Ok(0) => return Some(String::from_utf8_lossy(&seen).into_owned()),
                Ok(read) => seen.extend_from_slice(&buf[..read]),
                // Waiting, not failing. A read times out because nothing has
                // happened yet, and it is interrupted because a signal arrived
                // — this test sends one, and the child's death sends another —
                // and neither says anything about the stream. A reader that
                // treated `Interrupted` as an ending would report a hangup that
                // never happened, which under the client's rule means asking
                // the host and being told the pane is fine: a live pane read as
                // a steal.
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => break,
            }
        }
        None
    }

    #[test]
    fn a_pane_whose_child_exits_hangs_up_the_stream_watching_it() {
        // #338. The host recorded the exit and kept it: the socket stayed open,
        // `attached` stayed true, and a window went on drawing a terminal whose
        // shell had gone, with no way to find out. Before panes moved out of the
        // window this arrived locally, as the pseudoterminal's own hangup.
        let (host, pane) = host_with_cat_pane();
        let (mut window, server) = UnixStream::pair().expect("pair");
        assert!(host.attach(pane, server).is_ok());
        host.write_to(pane, b"the last thing it said\n".to_vec());
        assert!(within(Duration::from_secs(5), || {
            host.row_text(pane, 0).as_deref() == Some("the last thing it said")
        }));

        // The program ends the ordinary way, of its own accord.
        let pid = host.list_panes()[0].shell_pid;
        assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGTERM) }, 0);

        let final_screen = read_to_hangup(&mut window, Duration::from_secs(10))
            .expect("the stream to a terminal whose program has gone never ended");
        assert!(
            final_screen.contains("the last thing it said"),
            "the window lost the pane's last screen on the way out: {final_screen:?}"
        );

        // And the order the client depends on: by the time it can see the
        // hangup, the host already says the pane ended. A window reads a closed
        // stream as a steal unless the host says otherwise, so a hangup that
        // arrived first would make a dead pane look like a stolen one.
        assert!(
            host.list_panes()[0].ended,
            "the stream closed before the host would admit the pane had ended"
        );
    }

    #[test]
    fn attaching_to_a_pane_whose_child_has_gone_shows_it_and_then_ends() {
        // The same rule through the other door. A pane can end while nobody is
        // watching, and a client arriving afterwards would otherwise hold a
        // stream that never closes — the same dead terminal, drawn just as
        // convincingly, reached by a different route.
        let (host, pane) = host_with_cat_pane();
        host.write_to(pane, b"what it was doing when it stopped\n".to_vec());
        assert!(within(Duration::from_secs(5), || {
            host.row_text(pane, 0).as_deref() == Some("what it was doing when it stopped")
        }));
        let pid = host.list_panes()[0].shell_pid;
        assert_eq!(unsafe { libc::kill(pid as i32, libc::SIGTERM) }, 0);
        assert!(
            within(Duration::from_secs(5), || host.list_panes()[0].ended),
            "the pane never registered that its child had gone"
        );

        let (mut latecomer, server) = UnixStream::pair().expect("pair");
        assert!(host.attach(pane, server).is_ok());

        let final_screen = read_to_hangup(&mut latecomer, Duration::from_secs(10))
            .expect("a client attaching to a pane whose child has gone was left holding it");
        assert!(
            final_screen.contains("what it was doing when it stopped"),
            "the latecomer was hung up on before it was shown anything: {final_screen:?}"
        );
    }

    /// A layout with everything awkward in it: a split, a leaf this host runs,
    /// a leaf it does not, and three fields no version of this build has ever
    /// heard of.
    const A_REAL_ENOUGH_LAYOUT: &str = r#"
active = 0
panes = 99
a_field_from_a_later_build = "kept"

[[tabs]]
name = "one"
another_later_field = true

[tabs.node.Leaf]
pane_id = 1
cwd = "/where-it-was-an-hour-ago"
a_leaf_field_from_a_later_build = "kept"

[[tabs]]

[tabs.node.Split]
dir = "H"
ratio = 0.5

[tabs.node.Split.a.Leaf]
pane_id = 2

[tabs.node.Split.b.Leaf]
cwd = "/somebody-elses-terminal"
"#;

    fn live_panes() -> std::collections::BTreeMap<u64, PaneRuntime> {
        std::collections::BTreeMap::from([
            (
                1,
                PaneRuntime {
                    cwd: Some("/where-it-is-now".into()),
                    resume: Some("claude --resume 4a1c".into()),
                },
            ),
            (
                2,
                PaneRuntime {
                    cwd: Some("/the-other-one".into()),
                    resume: None,
                },
            ),
        ])
    }

    /// A state file of this test's own, with `n` tabs of one pane each.
    fn a_saved_session(path: &std::path::Path, tabs: usize) {
        let mut body = String::from("active = 0\n");
        for tab in 0..tabs {
            body.push_str(&format!(
                "\n[[tabs]]\nname = \"tab{tab}\"\n\n[tabs.node.Leaf]\ncwd = \"/somewhere\"\n"
            ));
        }
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("dir");
        std::fs::write(path, body).expect("write a session");
    }

    #[test]
    fn a_save_keeps_every_field_this_build_has_never_heard_of() {
        // The opaque envelope, which is the whole reason the host walks TOML
        // rather than parsing the layout into a type of its own. A window one
        // version newer writes fields this host does not know; parsing would
        // drop every one of them on the way back out, and it would surface much
        // later as somebody's settings quietly reverting.
        let layout: toml::Value = A_REAL_ENOUGH_LAYOUT.parse().expect("a layout");
        let merged = merge_capture_into_layout(&layout, &live_panes());

        assert_eq!(merged["a_field_from_a_later_build"].as_str(), Some("kept"));
        assert_eq!(
            merged["tabs"][0]["another_later_field"].as_bool(),
            Some(true)
        );
        let leaf = &merged["tabs"][0]["node"]["Leaf"];
        assert_eq!(
            leaf["a_leaf_field_from_a_later_build"].as_str(),
            Some("kept")
        );

        // And the two things the host alone can know are filled in.
        assert_eq!(leaf["cwd"].as_str(), Some("/where-it-is-now"));
        assert_eq!(leaf["resume"].as_str(), Some("claude --resume 4a1c"));
        assert_eq!(
            merged["tabs"][1]["node"]["Split"]["a"]["Leaf"]["cwd"].as_str(),
            Some("/the-other-one"),
            "a leaf inside a split was not reached"
        );

        // A leaf with no pane id is a terminal this host is not running — a
        // legacy file, or a window that owns its own — and is left alone.
        assert_eq!(
            merged["tabs"][1]["node"]["Split"]["b"]["Leaf"]["cwd"].as_str(),
            Some("/somebody-elses-terminal"),
            "the host edited a pane it does not run"
        );
    }

    #[test]
    fn the_tree_is_counted_rather_than_believed() {
        // `panes = 99` sits at the top of that layout, and the tree holds
        // three. The number that feeds the shrink guard has to be the walk.
        let layout: toml::Value = A_REAL_ENOUGH_LAYOUT.parse().expect("a layout");
        assert_eq!(count_layout(&layout), (3, 2));
    }

    #[test]
    fn a_save_that_would_lose_most_of_a_session_is_refused_and_the_file_stands() {
        let scratch = scratch("shrink");
        let path = state_file_in(&scratch);
        a_saved_session(&path, 6);
        let before = std::fs::read_to_string(&path).expect("read back");

        let host = Host::with_shell("test", None);
        host.persist_to(path.clone());

        // One pane offered over six on disk, and the envelope claims otherwise.
        let outcome = host.save(
            LAYOUT_SCHEMA,
            "panes = 6\n\n[[tabs]]\n\n[tabs.node.Leaf]\ncwd = \"/x\"\n",
            false,
        );
        match outcome {
            Outcome::Ok(Persisted::RefusedShrink {
                had_leaves,
                offered_leaves,
                ..
            }) => {
                assert_eq!((had_leaves, offered_leaves), (6, 1));
            }
            other => panic!("a session that lost five of six panes was written: {other:?}"),
        }
        assert_eq!(
            std::fs::read_to_string(&path).expect("read back"),
            before,
            "the file changed under a refused save"
        );

        // And a shrink somebody asked for goes through.
        let outcome = host.save(
            LAYOUT_SCHEMA,
            "panes = 6\n\n[[tabs]]\n\n[tabs.node.Leaf]\ncwd = \"/x\"\n",
            true,
        );
        assert!(
            matches!(outcome, Outcome::Ok(Persisted::Written { leaves: 1, .. })),
            "a deliberate close was refused: {outcome:?}"
        );
    }

    #[test]
    fn the_count_the_file_carries_is_the_one_the_host_walked() {
        // `instance::scan_sessions` reads that integer without parsing the
        // tree, so it decides which session a cold launch reopens. A client
        // claim written straight through would rank sessions by a number
        // nobody checked.
        let scratch = scratch("recount");
        let path = state_file_in(&scratch);
        let host = Host::with_shell("test", None);
        host.persist_to(path.clone());

        assert!(host
            .save(LAYOUT_SCHEMA, A_REAL_ENOUGH_LAYOUT, false)
            .is_ok());
        let written: toml::Value = std::fs::read_to_string(&path)
            .expect("read back")
            .parse()
            .expect("valid TOML");
        assert_eq!(
            written["panes"].as_integer(),
            Some(3),
            "the file kept the claim of 99 the client sent"
        );
    }

    #[test]
    fn a_layout_in_a_shape_this_build_does_not_know_is_written_through_untouched() {
        // Refusing would lose the whole save; writing it through loses only the
        // freshness of two fields, and the reply says which happened.
        let scratch = scratch("schema");
        let path = state_file_in(&scratch);
        let host = Host::with_shell("test", None);
        host.persist_to(path.clone());

        let outcome = host.save(LAYOUT_SCHEMA + 98, A_REAL_ENOUGH_LAYOUT, false);
        assert!(
            matches!(
                outcome,
                Outcome::Ok(Persisted::Written { merged: false, .. })
            ),
            "a shape this build cannot walk was silently treated as one it can: {outcome:?}"
        );
        let written = std::fs::read_to_string(&path).expect("read back");
        assert!(
            written.contains("/where-it-was-an-hour-ago"),
            "an unreadable shape was edited anyway: {written}"
        );
    }

    #[test]
    fn a_host_nobody_told_where_to_write_writes_nowhere() {
        // Every other test in this file makes a host and never mentions a file.
        // If a checkpoint worked a path out for itself, they would all be
        // writing over somebody's real session.
        let host = Host::with_shell("test", None);
        assert!(matches!(host.persist_now(false), Outcome::Err(_)));
        host.checkpoint_once();
    }

    #[test]
    fn the_checkpoint_writes_with_nobody_watching() {
        // The point of the host holding the pen: a session with no window still
        // records where its panes are, so a crash loses recency and never the
        // layout.
        let scratch = scratch("checkpoint");
        let path = state_file_in(&scratch);
        let host = Host::with_shell("test", Some("/bin/cat".into()));
        host.persist_to(path.clone());
        let info = spawn_guarded(&host);

        // A layout naming that pane, saved with a directory that is already
        // wrong.
        let body = format!(
            "active = 0\n\n[[tabs]]\n\n[tabs.node.Leaf]\npane_id = {}\ncwd = \"/nowhere-in-particular\"\n",
            info.pane
        );
        assert!(host.save(LAYOUT_SCHEMA, &body, false).is_ok());

        assert!(
            within(Duration::from_secs(5), || {
                host.checkpoint_once();
                std::fs::read_to_string(&path)
                    .map(|written| !written.contains("/nowhere-in-particular"))
                    .unwrap_or(false)
            }),
            "the checkpoint never replaced the saved directory with a reading: {:?}",
            std::fs::read_to_string(&path)
        );
        let written = std::fs::read_to_string(&path).expect("read back");
        let here = std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
        assert!(
            written.contains(here.to_str().expect("a path")),
            "the checkpoint wrote a directory the pane is not in: {written}"
        );
    }

    #[test]
    fn a_write_this_host_did_not_make_is_carried_on_from_rather_than_undone() {
        // The session file is a plain document in a directory a person can open,
        // and the documented recovery for a bad save is to copy a backup over
        // it. A host that seeded a copy at boot and never looked again undoes
        // that on its next checkpoint — silently, within thirty seconds, so the
        // person concludes the backup was no good.
        let scratch = scratch("foreign");
        let path = state_file_in(&scratch);
        let host = Host::with_shell("test", None);
        host.persist_to(path.clone());
        assert!(host
            .save(
                LAYOUT_SCHEMA,
                "active = 0\n\n[[tabs]]\nname = \"WHAT THE HOST HAD\"\n\n[tabs.node.Leaf]\ncwd = \"/tmp\"\n",
                false,
            )
            .is_ok());

        // Somebody restores a backup over it.
        std::fs::write(
            &path,
            "active = 0\n\n[[tabs]]\nname = \"WHAT WAS RESTORED\"\n\n[tabs.node.Leaf]\ncwd = \"/etc\"\n\n[[tabs]]\nname = \"AND ITS SECOND TAB\"\n\n[tabs.node.Leaf]\ncwd = \"/var\"\n",
        )
        .expect("restore a backup");

        host.checkpoint_once();

        let written = std::fs::read_to_string(&path).expect("read back");
        assert!(
            written.contains("WHAT WAS RESTORED"),
            "the host wrote its own copy over what somebody had just restored: {written}"
        );
        assert!(
            written.contains("AND ITS SECOND TAB"),
            "the restored session lost the tab the host's copy never had: {written}"
        );
        assert!(
            !written.contains("WHAT THE HOST HAD"),
            "the host's boot copy survived a later write: {written}"
        );
    }

    #[test]
    fn two_things_writing_at_once_still_leave_one_good_file() {
        // Two things in this process write the session file: a client's `save`
        // on its own connection, and the checkpoint on the upkeep thread. They
        // overlapped, and `write_atomic` builds its temporary file from the
        // destination's name — so both used one temp path, the first rename
        // took it away from the second, and the second failed with a puzzling
        // "no such file". Found by a soak, at two runs in forty.
        let scratch = scratch("onewriter");
        let path = state_file_in(&scratch);
        let host = Host::with_shell("test", None);
        host.persist_to(path.clone());
        assert!(host
            .save(
                LAYOUT_SCHEMA,
                "active = 0\n\n[[tabs]]\n\n[tabs.node.Leaf]\ncwd = \"/tmp\"\n",
                false,
            )
            .is_ok());

        // The barrier is the test. Eight threads started in a loop do not
        // overlap by starting — the first is several writes in before the last
        // exists — and the fault this guards appeared twice in forty soak
        // runs, so a green run that happened to serialise would prove nothing
        // and look exactly like a run that proved something. Every writer now
        // waits at the door before each write, so they arrive together, every
        // time, on purpose.
        //
        // Together at the door is the strongest thing available here, and it
        // is the right one: what is being tested is a mutex, so two threads
        // genuinely inside the interval at once is the state the mutex exists
        // to prevent. Line them all up in front of it and the first to take it
        // is holding it while seven others are asking.
        const WRITERS: usize = 8;
        let gate = Arc::new(std::sync::Barrier::new(WRITERS));
        let mut writers = Vec::new();
        for n in 0..WRITERS {
            let host = host.clone();
            let gate = gate.clone();
            let body = format!(
                "active = 0\n\n[[tabs]]\nname = \"writer {n}\"\n\n[tabs.node.Leaf]\ncwd = \"/tmp\"\n"
            );
            writers.push(std::thread::spawn(move || {
                let mut outcomes = Vec::new();
                for _ in 0..8 {
                    gate.wait();
                    outcomes.push(host.save(LAYOUT_SCHEMA, &body, true));
                    gate.wait();
                    outcomes.push(host.persist_now(true));
                }
                outcomes
            }));
        }
        let outcomes: Vec<Outcome<Persisted>> = writers
            .into_iter()
            .flat_map(|writer| writer.join().expect("a writer thread"))
            .collect();

        for outcome in &outcomes {
            assert!(
                !matches!(outcome, Outcome::Err(_)),
                "a write failed while another was in flight: {outcome:?}"
            );
        }
        // And what is on disk is one of the things somebody wrote, whole.
        let written = std::fs::read_to_string(&path).expect("read back");
        assert!(
            written.parse::<toml::Value>().is_ok(),
            "the file left behind is not readable TOML: {written}"
        );
    }

    #[test]
    fn a_host_knows_its_own_writing_when_it_sees_it() {
        // The field the rule above rests on. Without it the host cannot tell a
        // write it made from one it did not, so every checkpoint reads the file
        // back as though a stranger had touched it — which today only means a
        // misleading line in the log, and is the wrong ground for anything that
        // ever acts on the difference more strongly than this does.
        let scratch = scratch("ownwriting");
        let path = state_file_in(&scratch);
        let host = Host::with_shell("test", None);
        host.persist_to(path.clone());
        assert!(host
            .save(
                LAYOUT_SCHEMA,
                "active = 0\n\n[[tabs]]\n\n[tabs.node.Leaf]\ncwd = \"/tmp\"\n",
                false,
            )
            .is_ok());

        assert_eq!(
            host.last_written.lock().expect("last written").as_deref(),
            std::fs::read_to_string(&path).ok().as_deref(),
            "the host does not recognise the file it just wrote"
        );
    }

    #[test]
    fn a_client_saving_still_wins_over_what_is_on_disk() {
        // The other side of the same rule. A window is showing the live tree,
        // which is a better account of the session than any file — so `save` is
        // not a merge with whatever happens to be on disk, it replaces it.
        let scratch = scratch("livewins");
        let path = state_file_in(&scratch);
        let host = Host::with_shell("test", None);
        host.persist_to(path.clone());
        std::fs::write(
            &path,
            "active = 0\n\n[[tabs]]\nname = \"STALE\"\n\n[tabs.node.Leaf]\ncwd = \"/etc\"\n",
        )
        .expect("something else writes");

        assert!(host
            .save(
                LAYOUT_SCHEMA,
                "active = 0\n\n[[tabs]]\nname = \"WHAT THE WINDOW IS SHOWING\"\n\n[tabs.node.Leaf]\ncwd = \"/tmp\"\n",
                false,
            )
            .is_ok());
        let written = std::fs::read_to_string(&path).expect("read back");
        assert!(
            written.contains("WHAT THE WINDOW IS SHOWING") && !written.contains("STALE"),
            "a live window's own layout lost to a file: {written}"
        );
    }

    #[test]
    fn half_a_write_is_not_a_layout() {
        // A file caught mid-write parses as nothing, and adopting nothing would
        // throw away a session to a race with somebody's editor.
        let scratch = scratch("halfwritten");
        let path = state_file_in(&scratch);
        let host = Host::with_shell("test", None);
        host.persist_to(path.clone());
        assert!(host
            .save(
                LAYOUT_SCHEMA,
                "active = 0\n\n[[tabs]]\nname = \"THE REAL ONE\"\n\n[tabs.node.Leaf]\ncwd = \"/tmp\"\n",
                false,
            )
            .is_ok());

        std::fs::write(&path, "active = 0\n\n[[tabs]\nname = \"trunc").expect("a partial write");
        host.adopt_a_foreign_write();

        let (_, layout) = host
            .layout
            .lock()
            .expect("layout")
            .clone()
            .expect("a layout");
        assert!(
            toml::to_string(&layout)
                .expect("serialise")
                .contains("THE REAL ONE"),
            "the host took an unparseable file as its session"
        );
    }

    /// A cadence whose idle clock will not fire inside a test.
    ///
    /// The real budget is twelve hours and is never shortened for convenience —
    /// tests that care about the idle clock name their own, and every other
    /// test says plainly that it is not the subject.
    fn never_idles() -> Cadence {
        Cadence {
            idle_exit: Duration::from_secs(60 * 60),
            idle_check: Duration::from_secs(60 * 60),
            ..Cadence::default()
        }
    }

    #[test]
    fn the_idle_budget_that_ships_is_the_one_that_was_approved() {
        // The branch that actually runs is the default one, and a policy that
        // exists only in a test's own cadence is a policy nothing enforces.
        assert_eq!(
            Cadence::default().idle_exit,
            Duration::from_secs(12 * 60 * 60),
            "the shipped idle budget is not the twelve hours that was decided"
        );
        assert!(
            Cadence::default().idle_check < Cadence::default().idle_exit,
            "a host that looks less often than it waits can never notice"
        );
    }

    #[test]
    fn a_host_nobody_has_used_for_long_enough_checkpoints_and_stops() {
        // Nothing ended a host, and that was never a choice anybody made: an
        // empty one sat at fifteen megabytes until a reboot. Hosted by default,
        // that is one per session, forever.
        let scratch = scratch("idle");
        let path = state_file_in(&scratch);
        let host = Host::with_shell("test", None);
        host.persist_to(path.clone());
        assert!(host
            .save(
                LAYOUT_SCHEMA,
                "active = 0\n\n[[tabs]]\n\n[tabs.node.Leaf]\ncwd = \"/tmp\"\n",
                false,
            )
            .is_ok());
        // Taken away rather than replaced with something shorter: a shorter
        // file is a *foreign write*, which this host rightly carries on from,
        // and the test would then be measuring that rule instead of this one.
        std::fs::remove_file(&path).expect("take the file away");

        host.start_upkeep(Cadence {
            idle_exit: Duration::from_millis(50),
            idle_check: Duration::from_millis(20),
            ..never_idles()
        });

        assert!(
            within(Duration::from_secs(5), || host
                .shutdown
                .load(Ordering::SeqCst)),
            "a host nobody has touched went on running"
        );
    }

    #[test]
    fn the_idle_stop_writes_before_it_goes() {
        // What is left on disk should be fresh rather than hours stale, and
        // this is the only thing that can have written it: no clock is started,
        // the budget is nothing, and `idle_once` is called by hand. Started
        // through `start_upkeep` instead, the checkpoint's own first tick
        // writes the file and the assertion proves nothing — which is how the
        // first version of this passed with the checkpoint removed.
        let scratch = scratch("idlewrite");
        let path = state_file_in(&scratch);
        let host = Host::with_shell("test", None);
        host.persist_to(path.clone());
        assert!(host
            .save(
                LAYOUT_SCHEMA,
                "active = 0\n\n[[tabs]]\n\n[tabs.node.Leaf]\ncwd = \"/tmp\"\n",
                false,
            )
            .is_ok());
        std::fs::remove_file(&path).expect("take the file away");

        host.idle_once(Duration::ZERO);

        assert!(
            host.shutdown.load(Ordering::SeqCst),
            "a host past its budget with nobody using it did not stop"
        );
        assert!(
            std::fs::read_to_string(&path)
                .expect("the host stopped without writing anything")
                .contains("tabs"),
            "the host stopped without checkpointing first"
        );
    }

    #[test]
    fn a_host_with_a_window_attached_never_stops() {
        // The hard rule: whatever the panes are printing, an attached window is
        // proof the session is wanted.
        let (host, pane) = host_with_cat_pane();
        let (_client, server) = UnixStream::pair().expect("pair");
        assert!(host.attach(pane, server).is_ok());

        host.start_upkeep(Cadence {
            idle_exit: Duration::from_millis(20),
            idle_check: Duration::from_millis(10),
            ..never_idles()
        });
        std::thread::sleep(Duration::from_millis(400));

        assert!(
            !host.shutdown.load(Ordering::SeqCst),
            "a host stopped underneath the window watching it"
        );
    }

    #[test]
    fn a_host_whose_panes_are_still_talking_never_stops() {
        // The other half of idle: nobody is attached, but the work is going on
        // without them, which is the whole product.
        let (host, pane) = host_with_cat_pane();
        assert!(!host.attended(), "nobody is watching this one");

        host.start_upkeep(Cadence {
            idle_exit: Duration::from_millis(50),
            idle_check: Duration::from_millis(20),
            ..never_idles()
        });
        for _ in 0..20 {
            host.write_to(pane, b"still working\n".to_vec());
            std::thread::sleep(Duration::from_millis(20));
            if host.shutdown.load(Ordering::SeqCst) {
                break;
            }
        }

        assert!(
            !host.shutdown.load(Ordering::SeqCst),
            "a host stopped while its terminals were still producing output"
        );
    }

    /// Where does a keystroke wait, inside the host, under load?
    ///
    /// The echo bench says an attached keystroke costs 2-3 ms more at p99 with
    /// eight panes flooding, while the median holds — a terminal that hiccups
    /// rather than a slow one — and it has already ruled out the window drawing
    /// eight copies of somebody else's output. Two of the remaining hypotheses
    /// are host-side, and this settles the first of them: a keystroke takes the
    /// global pane table (`write_to` does), which every other reader takes too.
    ///
    /// Measured here rather than reasoned about, because the honest answer to
    /// "which of three things is it" is a number.
    ///
    /// ```text
    /// cd app && TD_PROBE_FLOOD=8 cargo test --release --bin terminal-delight \
    ///     where_a_keystroke_waits -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "latency probe — run by hand, see the doc comment"]
    fn where_a_keystroke_waits_inside_the_host() {
        let flood: usize = std::env::var("TD_PROBE_FLOOD")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(8);
        let samples: usize = std::env::var("TD_PROBE_SAMPLES")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(2000);

        // One quiet pane to type into, and `flood` panes saturating the host's
        // reader thread beside it — the bench's stress case, host side only.
        let host = Host::with_shell("probe", Some("/bin/cat".into()));
        let quiet = spawn_guarded(&host).pane;
        let flooding = Host::with_shell("probe", Some("/bin/sh".into()));
        let mut flood_panes = Vec::new();
        for _ in 0..flood {
            flood_panes.push(spawn_guarded(&flooding).pane);
        }
        // Saturating by default, because that is the stress case the gate
        // names. `TD_PROBE_REALISTIC` swaps it for a busy-but-ordinary load — a
        // couple of thousand lines a second per pane, roughly a talkative build
        // — because a saturating writer and a real workload are not the same
        // question, and the gate is written about the second one.
        let realistic = std::env::var_os("TD_PROBE_REALISTIC").is_some();
        let program: &[u8] = if realistic {
            b"while :; do seq 1 200; sleep 0.1; done\n"
        } else {
            b"yes flooding-the-terminal-with-output\n"
        };
        for pane in &flood_panes {
            flooding.write_to(*pane, program.to_vec());
        }
        // Let the flood get going before anything is timed.
        std::thread::sleep(Duration::from_millis(750));

        let mut waited = Vec::with_capacity(samples);
        let mut sent = Vec::with_capacity(samples);
        // The control, and the point of the whole probe: an interval that does
        // nothing at all, taken in the same loop under the same load. Whatever
        // shows up here is this thread waiting for a core rather than waiting
        // for anything in this program, and it has to be subtracted by eye from
        // everything else before any of it means work.
        let mut nothing = Vec::with_capacity(samples);
        for _ in 0..samples {
            let idle_start = Instant::now();
            let idle_end = Instant::now();
            nothing.push(idle_end.duration_since(idle_start).as_micros() as u64);

            let before = Instant::now();
            let panes = host.panes.lock().expect("panes");
            let held = Instant::now();
            let pane = panes.get(&quiet).expect("the quiet pane");
            let _ = pane.input.0.send(Msg::Input(b"x".to_vec().into()));
            drop(panes);
            waited.push(held.duration_since(before).as_micros() as u64);
            sent.push(held.elapsed().as_micros() as u64);
            std::thread::sleep(Duration::from_micros(200));
        }

        waited.sort_unstable();
        sent.sort_unstable();
        nothing.sort_unstable();
        let at = |v: &[u64], q: f64| v[((v.len() as f64 * q) as usize).min(v.len() - 1)];
        println!(
            "{{\"probe\":\"keystroke\",\"flood\":{flood},\"realistic\":{realistic},\
             \"samples\":{samples},\
             \"lock_p50_us\":{},\"lock_p99_us\":{},\"lock_max_us\":{},\
             \"send_p50_us\":{},\"send_p99_us\":{},\"send_max_us\":{},\
             \"nothing_p50_us\":{},\"nothing_p99_us\":{},\"nothing_max_us\":{}}}",
            at(&waited, 0.50),
            at(&waited, 0.99),
            waited[waited.len() - 1],
            at(&sent, 0.50),
            at(&sent, 0.99),
            sent[sent.len() - 1],
            at(&nothing, 0.50),
            at(&nothing, 0.99),
            nothing[nothing.len() - 1],
        );

        for pane in flood_panes {
            let _ = flooding.close_pane(pane);
        }
    }

    #[test]
    fn the_upkeep_does_not_keep_a_dropped_host_alive() {
        // Every test here makes a host and drops it. Two threads holding a
        // strong reference would leave a pair behind per host for the life of
        // the suite, which shows up as a suite that gets slower and never as a
        // failure.
        let host = Host::with_shell("test", None);
        host.start_upkeep(Cadence::default());
        let ghost = Arc::downgrade(&host);
        drop(host);
        assert!(
            within(Duration::from_secs(5), || ghost.upgrade().is_none()),
            "the upkeep threads outlived the host they serve"
        );
    }
}
