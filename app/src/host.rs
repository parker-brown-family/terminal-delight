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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Sender, SyncSender};
use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event as TermEvent, EventListener, OnResize, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Msg, Notifier};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::tty::{self, ChildEvent, EventedPty, EventedReadWrite, Pty};
use polling::{Event, PollMode, Poller};

use crate::gridwire;
use crate::hostproto::{
    host_socket_path, parse_stream_greeting, ClosedPane, Outcome, PaneGeom, PaneId, PaneInfo, Reply,
    Request, ENV_PANE_ID, ENV_SESSION, PROTO_VERSION,
};
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
        Self { chunks, serial }
    }

    /// Queue bytes. `false` means this client is gone or too far behind, and
    /// the caller should drop it.
    fn send(&self, bytes: &[u8]) -> bool {
        self.chunks.try_send(bytes.to_vec()).is_ok()
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
}

impl Read for TeeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.master.read(buf)?;
        if read > 0 {
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
    fn new(inner: Pty, sink: Arc<Mutex<Option<Sink>>>) -> io::Result<Self> {
        // A second descriptor onto the same open file: readiness is reported
        // on the one the poller holds, reads happen on this one, and because
        // they share a description the two always agree.
        let master = inner.file().try_clone()?;
        Ok(Self {
            inner,
            reader: TeeReader { master, sink },
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

    fn reregister(&mut self, poll: &Arc<Poller>, interest: Event, mode: PollMode) -> io::Result<()> {
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
    geom: Mutex<PaneGeom>,
    ended: Arc<AtomicBool>,
    cwd: Option<String>,
}

impl HostPane {
    fn info(&self, pane: PaneId) -> PaneInfo {
        PaneInfo {
            pane,
            shell_pid: self.shell_pid,
            cwd: self.cwd.clone(),
            attached: self.sink.lock().expect("sink lock").is_some(),
            ended: self.ended.load(Ordering::SeqCst),
            geom: *self.geom.lock().expect("geom lock"),
        }
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
    panes: Mutex<HashMap<PaneId, HostPane>>,
    next_pane: AtomicU64,
    next_serial: AtomicU64,
    shutdown: AtomicBool,
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
        })
    }

    /// Start a terminal: a real pseudoterminal, a real child, and an emulator
    /// whose grid is the truth every client will be shown.
    pub fn spawn_pane(&self, cwd: Option<String>, geom: PaneGeom) -> io::Result<PaneInfo> {
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
            working_directory: cwd.as_ref().map(std::path::PathBuf::from).filter(|d| d.is_dir()),
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
        let sink: Arc<Mutex<Option<Sink>>> = Arc::new(Mutex::new(None));
        let tee = TeePty::new(pty, sink.clone())?;

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
                    TermEvent::ChildExit(_) => exit_flag.store(true, Ordering::SeqCst),
                    _ => {}
                }
            }
        });

        let host_pane = HostPane {
            term,
            input,
            sink,
            shell_pid,
            geom: Mutex::new(geom),
            ended,
            cwd,
        };
        let info = host_pane.info(pane);
        self.panes.lock().expect("panes").insert(pane, host_pane);
        Ok(info)
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

/// Whether the peer on this socket is the same user we are.
///
/// The whole authorisation model, and deliberately the whole of it: the socket
/// lives in a private directory, and this confirms at the moment of connection
/// what the directory implies. Nothing further up the stack repeats the claim,
/// so nothing further up can be wrong about it.
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

    let mut writer = stream;
    let mut line = first;
    loop {
        let reply = handle_control_line(host, &line);
        let shutting = matches!(reply, Reply::ShuttingDown);
        let mut out = serde_json::to_string(&reply).unwrap_or_else(|e| {
            format!(r#"{{"reply":"error","msg":"could not encode a reply: {e}"}}"#)
        });
        out.push('\n');
        if writer.write_all(out.as_bytes()).is_err() {
            return;
        }
        if shutting {
            host.shutdown.store(true, Ordering::SeqCst);
            // The accept loop is asleep waiting for a connection, so give it
            // one. Without this the host would keep running until somebody
            // happened to knock, which is a shutdown that depends on a
            // stranger arriving.
            let _ = UnixStream::connect(host_socket_path(&host.key));
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

fn handle_control_line(host: &Arc<Host>, line: &str) -> Reply {
    let request: Request = match serde_json::from_str(line.trim()) {
        Ok(request) => request,
        Err(err) => {
            return Reply::Error {
                msg: format!("unreadable request: {err}"),
            }
        }
    };
    match request {
        Request::Hello { proto, kind } => match crate::hostproto::version_check(proto) {
            Ok(()) => {
                let _ = kind;
                Reply::Hello {
                    proto: PROTO_VERSION,
                    session: host.key.clone(),
                    panes: host.pane_count(),
                    attended: host.attended(),
                }
            }
            Err(msg) => Reply::Error { msg },
        },
        Request::ListPanes => Reply::Panes {
            panes: host.list_panes(),
        },
        Request::SpawnPane { cwd, geom } => Reply::Spawned {
            outcome: match host.spawn_pane(cwd, geom) {
                Ok(info) => Outcome::Ok(info),
                Err(err) => Outcome::Err(format!("could not start a terminal: {err}")),
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
        Request::Shutdown => Reply::ShuttingDown,
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
            eprintln!("terminal-delight serve: cannot create {}: {err}", dir.display());
            return 1;
        }
        // The directory is the authorisation model; make it say so.
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    // A socket file left by a dead host is not a running host.
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("terminal-delight serve: cannot listen on {}: {err}", path.display());
            return 1;
        }
    };

    let host = Host::new(key);
    for stream in listener.incoming() {
        if host.shutdown.load(Ordering::SeqCst) {
            break;
        }
        let Ok(stream) = stream else { continue };
        let serving = host.clone();
        std::thread::spawn(move || serve_connection(&serving, stream));
    }
    let _ = std::fs::remove_file(&path);
    0
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

    /// Start a pane while holding the fork guard.
    ///
    /// Starting a terminal forks, and a fork briefly hands the child every
    /// descriptor this process holds — including locks another test is in the
    /// middle of asserting about. See `testsync`.
    fn spawn_guarded(host: &Arc<Host>) -> PaneInfo {
        let _guard = crate::testsync::forks_and_locks();
        host.spawn_pane(None, PaneGeom::default())
            .expect("start a pane")
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
        assert!(closed, "the superseded client was left holding a live stream");
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

        let hello = handle_control_line(
            &host,
            &serde_json::to_string(&Request::Hello {
                proto: PROTO_VERSION,
                kind: ClientKind::Window,
            })
            .unwrap(),
        );
        assert!(matches!(hello, Reply::Hello { panes: 1, .. }), "{hello:?}");

        let listed = handle_control_line(&host, r#"{"verb":"list-panes"}"#);
        assert!(matches!(listed, Reply::Panes { ref panes } if panes.len() == 1));

        let resized = handle_control_line(
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
        assert!(matches!(resized, Reply::Resized { outcome: Outcome::Ok(()), .. }));
        assert_eq!(host.list_panes()[0].geom.cols, 100);

        // A version we cannot speak is refused by name, not ignored.
        let mismatched = handle_control_line(&host, r#"{"verb":"hello","proto":99,"kind":"window"}"#);
        match mismatched {
            Reply::Error { msg } => assert!(msg.contains("99"), "{msg}"),
            other => panic!("a bad version was accepted: {other:?}"),
        }

        // Garbage gets an answer too. Silence is what started all of this.
        assert!(matches!(
            handle_control_line(&host, "{not json at all"),
            Reply::Error { .. }
        ));
        assert!(matches!(
            handle_control_line(&host, r#"{"verb":"teleport"}"#),
            Reply::Error { .. }
        ));

        // A verb naming a pane that does not exist says so.
        let closed = handle_control_line(&host, r#"{"verb":"close-pane","pane":9999}"#);
        assert!(
            matches!(closed, Reply::Closed { outcome: Outcome::Err(_), .. }),
            "{closed:?}"
        );
    }

    #[test]
    fn a_tool_asking_questions_never_costs_a_window_its_pane() {
        // Attaching is a property of opening a byte stream, not of saying
        // hello — which is what makes this safe by construction rather than by
        // a check somebody has to remember. It matters because resolving a
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
            handle_control_line(&host, &line);
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
}
