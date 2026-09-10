//! The window's side of the conversation with a session host.
//!
//! Everything here runs in the GUI process and knows nothing about gpui: a
//! connection, some verbs, and the two ways a window ends up talking to a host
//! — finding one already running, or starting one. That split matters more than
//! it looks. A window that can only start a host cannot survive its own death;
//! a window that can only find one cannot be the first to open.
//!
//! **Requests are synchronous and replies are matched by shape.** The host
//! answers in order on the control connection, so a call writes a line and
//! reads until it sees the reply it asked for, stepping over anything left
//! behind by a verb nobody waited on. That is deliberately dull: a reader
//! thread and a router buy nothing while the host has no unsolicited messages
//! to send, and would have to be unpicked when it does.
//!
//! **Every function that touches the filesystem has an injectable twin.** The
//! ambient one reads the runtime directory; the `_at` one is told a path. That
//! is the same discipline `instance.rs` follows, and for the same reason: a
//! test that had to set `XDG_RUNTIME_DIR` would be setting it for every other
//! test in the process, which is how a suite acquires failures nobody can
//! reproduce.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::hostproto::{
    host_socket_path, stream_greeting, ClientKind, ClosedPane, Outcome, PaneGeom, PaneId, PaneInfo,
    Persisted, Push, Reply, Request, PROTO_VERSION,
};

/// How long any single control exchange may take before the window gives up on
/// it. Long enough for a host under load, short enough that a wedged one costs
/// a visible stall rather than a hung window.
const EXCHANGE_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a probe waits on a socket that exists. Short: this runs once per
/// candidate session at launch, and the answer comes off a local socket in
/// microseconds when it comes at all.
pub const PROBE_BUDGET: Duration = Duration::from_millis(250);

/// How long to wait for a host we just started to answer its first hello.
pub const SPAWN_BUDGET: Duration = Duration::from_secs(10);

/// Whether asking for a terminal produced one.
///
/// Not a bare bool at the call site: the two cases call for different things,
/// and a `false` read at a distance looks like failure, which this is the
/// opposite of. `Already` means the session was doing what you asked for
/// before you asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Started {
    Freshly,
    Already,
}

/// What a probe found.
///
/// Four states, and the distinctions are the point. A socket that exists but
/// will not answer is not the same thing as no host at all: the first is a
/// machine in a state somebody should look at, the second is an ordinary cold
/// start, and collapsing them starts a second host beside a sick one. Nor is
/// either the same as a host that answers and refuses, which is healthy, is
/// holding somebody's terminals, and is the only one of the three a launch may
/// act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostProbe {
    Live {
        proto: u32,
        session: String,
        panes: usize,
        /// Whether a window currently holds any of this host's panes. A host
        /// with panes and nobody watching them is exactly what a relaunch
        /// should adopt.
        attended: bool,
    },
    NoSocket,
    Unresponsive,
    /// The socket answered and refused us: it is a host from a build whose
    /// protocol this one does not speak.
    ///
    /// Its own number is deliberately not carried. The refusal names both
    /// versions in prose for a person reading a log, but nothing in the reply
    /// is a field, and inventing one here would put a number in the record
    /// that nobody measured. What this state says is exactly what was
    /// observed: a healthy host, and no common language.
    ///
    /// Kept apart from `Unresponsive` because they call for opposite things. A
    /// host that will not answer is a machine somebody should look at, and is
    /// left alone. A host that answers and refuses is a decision: the approved
    /// one is that it checkpoints and stands down so the newer build can start
    /// from the file it leaves.
    Skewed,
}

impl HostProbe {
    pub fn is_live(&self) -> bool {
        matches!(self, HostProbe::Live { .. })
    }

    /// A host whose panes no window is holding — the kill-and-relaunch case,
    /// and the only one a launch adopts without being told to.
    pub fn is_free(&self) -> bool {
        matches!(
            self,
            HostProbe::Live {
                attended: false,
                ..
            }
        )
    }

    /// Nothing is there — the only state in which it is safe to start a host.
    ///
    /// Not the negation of [`is_live`]: a host that will not answer is neither
    /// live nor absent, and starting a second one over its socket would leave
    /// its terminals running where nothing can reach them.
    pub fn is_absent(&self) -> bool {
        matches!(self, HostProbe::NoSocket)
    }

    /// A live host this build cannot talk to.
    pub fn is_skewed(&self) -> bool {
        matches!(self, HostProbe::Skewed)
    }
}

/// Ask a session's host who it is, without becoming its window.
///
/// Announces itself as a tool, which the host records and acts on in no way —
/// what keeps this harmless is that it opens no pane stream, and a stream is
/// the only thing that takes a pane from whoever is drawing it. Worth saying
/// because every launch probes every candidate host.
pub fn probe_host(key: &str) -> HostProbe {
    probe_at(&host_socket_path(key), PROBE_BUDGET)
}

pub fn probe_at(path: &Path, budget: Duration) -> HostProbe {
    let Ok(stream) = UnixStream::connect(path) else {
        // Either there is no socket, or there is a file nothing is listening
        // on — a host that died without tidying up. Neither is a live host,
        // and both are ordinary.
        return HostProbe::NoSocket;
    };
    let _ = stream.set_read_timeout(Some(budget));
    let _ = stream.set_write_timeout(Some(budget));
    let Ok(mut conn) = Conn::over(stream, budget) else {
        return HostProbe::Unresponsive;
    };
    match conn.hello(ClientKind::Tool) {
        Ok(Reply::Hello {
            proto,
            session,
            panes,
            attended,
        }) => HostProbe::Live {
            proto,
            session,
            panes,
            attended,
        },
        // It answered, and what it said was that it cannot speak to us. That
        // is a healthy host on the wrong side of a protocol change, which is a
        // different fact from a host that will not answer at all — and the
        // only one of the two anybody may act on.
        Err(err) if err.to_string().contains(crate::hostproto::VERSION_REFUSAL) => {
            HostProbe::Skewed
        }
        // It answered something, or nothing, but not a greeting. A host in that
        // state is a fact worth carrying, not an absence.
        _ => HostProbe::Unresponsive,
    }
}

/// Every session with a socket on this machine. Candidates to probe, not facts.
pub fn sockets_present() -> Vec<String> {
    match host_socket_path("x").parent() {
        Some(dir) => sockets_present_in(dir),
        None => vec![],
    }
}

pub fn sockets_present_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut out: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name();
            let name = name.to_str()?;
            name.strip_prefix("session-")?
                .strip_suffix(".sock")
                .map(str::to_string)
        })
        .collect();
    out.sort();
    out
}

/// How long a host we cannot speak to is given to write its file and go.
///
/// Generous, because what it is doing in that time is a checkpoint: reading
/// every pane's working directory out of `/proc` and writing the session file.
/// The alternative to waiting is starting a second host over a socket the
/// first still holds, which is the failure this whole path exists to avoid.
pub const STAND_DOWN_BUDGET: Duration = Duration::from_secs(5);

/// Ask a host this build cannot speak to to checkpoint and stand down, and
/// wait until it has gone.
///
/// The approved path through a protocol break, and the only one: the old host
/// writes what it holds, exits, and the new build starts from the file it
/// left. That degrades a version bump to exactly what a window used to do —
/// the terminals end with the process — once, deliberately, instead of the
/// alternative that shipped, which was a second window rising beside a live
/// host with both of them writing one session file.
///
/// The hello is expected to be refused. Being refused for a version is what
/// earns this connection the right to ask for this one thing and nothing else.
pub fn stand_down_at(path: &Path, budget: Duration) -> std::io::Result<()> {
    let stream = UnixStream::connect(path)?;
    let mut conn = Conn::over(stream, budget)?;
    // Refused, and the refusal is read here rather than left in the stream —
    // an unread error line would be taken for the answer to the next thing
    // asked, which is the only thing this connection has to ask.
    let _ = conn.hello(ClientKind::Window);
    conn.send(&Request::Shutdown)?;
    conn.expect(|reply| match reply {
        Reply::ShuttingDown => Ok(()),
        other => Err(other),
    })?;

    // Gone means gone. The next thing the caller does is bind that socket, and
    // a host that is still holding it would refuse — correctly, and for a
    // reason that would look nothing like this one by the time it surfaced.
    let deadline = Instant::now() + budget;
    while Instant::now() < deadline {
        if probe_at(path, PROBE_BUDGET).is_absent() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        format!(
            "the host at {} was asked to stand down and is still there",
            path.display()
        ),
    ))
}

/// One control connection, and the rule that a reply is read by shape.
struct Conn {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Conn {
    fn over(stream: UnixStream, timeout: Duration) -> std::io::Result<Self> {
        let _ = stream.set_read_timeout(Some(timeout));
        let _ = stream.set_write_timeout(Some(timeout));
        Ok(Self {
            reader: BufReader::new(stream.try_clone()?),
            writer: stream,
        })
    }

    fn send(&mut self, request: &Request) -> std::io::Result<()> {
        let mut line = serde_json::to_string(request)
            .map_err(|e| std::io::Error::other(format!("could not encode a request: {e}")))?;
        line.push('\n');
        self.writer.write_all(line.as_bytes())
    }

    /// Read replies until one satisfies `want`.
    ///
    /// Skipping is not laxity: a verb whose answer nobody waited for (a resize
    /// during a window drag) leaves its reply in the stream, and a client that
    /// insisted the next line be its own would read one answer behind for the
    /// rest of the session.
    fn expect<T>(&mut self, want: impl Fn(Reply) -> Result<T, Reply>) -> std::io::Result<T> {
        // Bounded so a host answering an endless stream of something else
        // cannot hold a window here forever.
        for _ in 0..64 {
            let mut line = String::new();
            match self.reader.read_line(&mut line) {
                Ok(0) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "the session host closed the connection",
                    ))
                }
                Ok(_) => {}
                Err(e) => return Err(e),
            }
            let reply: Reply = serde_json::from_str(line.trim())
                .map_err(|e| std::io::Error::other(format!("unreadable reply: {e} ({line})")))?;
            if let Reply::Error { msg } = &reply {
                return Err(std::io::Error::other(format!(
                    "session host refused: {msg}"
                )));
            }
            match want(reply) {
                Ok(value) => return Ok(value),
                Err(_stale) => continue,
            }
        }
        Err(std::io::Error::other(
            "the session host never sent the reply that was asked for",
        ))
    }

    fn hello(&mut self, kind: ClientKind) -> std::io::Result<Reply> {
        self.send(&Request::Hello {
            proto: PROTO_VERSION,
            kind,
        })?;
        self.expect(|reply| match reply {
            hello @ Reply::Hello { .. } => Ok(hello),
            other => Err(other),
        })
    }
}

/// A window's live connection to the host that owns its panes.
///
/// Shared: a pane's resize announcements come off the terminal event loop's own
/// thread, so the connection is behind a mutex and every caller is one of many.
pub struct HostLink {
    conn: Mutex<Conn>,
    session: String,
    /// Where this host listens — remembered rather than recomputed, so a link
    /// opened at a given path keeps talking to that host even if the ambient
    /// runtime directory changes underneath it.
    socket: PathBuf,
    /// Set the moment any exchange fails.
    ///
    /// A window whose host has gone must stop writing the session file. It no
    /// longer knows what the session contains — its panes are replicas of
    /// terminals that no longer exist — and the layout it would write is a
    /// picture of a corpse. Keeping yesterday's file is better: it at least
    /// describes work that once ran.
    lost: AtomicBool,
}

impl HostLink {
    /// Connect as the window. A hello of kind `Window` is what makes this
    /// connection the one that may take panes.
    pub fn attach(key: &str) -> std::io::Result<Arc<Self>> {
        Self::attach_at(&host_socket_path(key))
    }

    pub fn attach_at(path: &Path) -> std::io::Result<Arc<Self>> {
        let stream = UnixStream::connect(path)?;
        let mut conn = Conn::over(stream, EXCHANGE_TIMEOUT)?;
        let session = match conn.hello(ClientKind::Window)? {
            Reply::Hello { proto, session, .. } => {
                // The one compatibility fact. A build number never gates:
                // adjacent builds must keep talking or every update becomes a
                // forced restart of everything running.
                if let Err(msg) = crate::hostproto::version_check(proto) {
                    return Err(std::io::Error::other(msg));
                }
                session
            }
            other => return Err(std::io::Error::other(format!("odd greeting: {other:?}"))),
        };
        Ok(Arc::new(Self {
            conn: Mutex::new(conn),
            session,
            socket: path.to_path_buf(),
            lost: AtomicBool::new(false),
        }))
    }

    pub fn session(&self) -> &str {
        &self.session
    }

    /// Where this host listens, so a second connection can be opened to it.
    pub fn socket(&self) -> &Path {
        &self.socket
    }

    /// Whether this window has lost the host it was talking to.
    pub fn lost(&self) -> bool {
        self.lost.load(Ordering::SeqCst)
    }

    fn exchange<T>(
        &self,
        request: Request,
        want: impl Fn(Reply) -> Result<T, Reply>,
    ) -> std::io::Result<T> {
        let mut conn = self
            .conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let result = conn.send(&request).and_then(|()| conn.expect(want));
        if result.is_err() {
            self.lost.store(true, Ordering::SeqCst);
        }
        result
    }

    pub fn list_panes(&self) -> std::io::Result<Vec<PaneInfo>> {
        self.exchange(Request::ListPanes, |reply| match reply {
            Reply::Panes { panes } => Ok(panes),
            other => Err(other),
        })
    }

    /// Start a terminal, and have the host type the recipe that puts an agent
    /// back in its conversation.
    ///
    /// The recipe travels WITH the request rather than being typed afterwards
    /// by this window, and the two are one change: the host refuses to type a
    /// recipe this session is already running, and a window that went on typing
    /// it anyway would hand-deliver the second agent the refusal exists to
    /// prevent.
    ///
    /// `Started::Already` means the session was doing what was asked for before
    /// it was asked, and the pane in the reply is the one doing it.
    pub fn spawn_pane(
        &self,
        cwd: Option<String>,
        resume: Option<String>,
        geom: PaneGeom,
    ) -> std::io::Result<(PaneInfo, Started)> {
        self.exchange(
            Request::SpawnPane { cwd, resume, geom },
            |reply| match reply {
                Reply::Spawned { outcome, started } => Ok((outcome, started)),
                other => Err(other),
            },
        )
        .and_then(|(outcome, started)| {
            let info = unwrap_outcome(outcome)?;
            Ok((
                info,
                if started {
                    Started::Freshly
                } else {
                    Started::Already
                },
            ))
        })
    }

    /// Declare intent to take a pane, and tell the host the size it will be
    /// shown at. The handover itself happens when the byte stream connects.
    pub fn attach_pane(&self, pane: PaneId, geom: PaneGeom) -> std::io::Result<PaneInfo> {
        self.exchange(Request::AttachPane { pane, geom }, |reply| match reply {
            Reply::Attached { outcome, .. } => Ok(outcome),
            other => Err(other),
        })
        .and_then(unwrap_outcome)
    }

    #[allow(dead_code)]
    pub fn close_pane(&self, pane: PaneId) -> std::io::Result<ClosedPane> {
        self.exchange(Request::ClosePane { pane }, |reply| match reply {
            Reply::Closed { outcome, .. } => Ok(outcome),
            other => Err(other),
        })
        .and_then(unwrap_outcome)
    }

    /// Tell the host a pane's new size, without waiting to be told it worked.
    ///
    /// Dragging a window edge produces one of these a frame. Waiting for each
    /// would put a socket round trip inside the resize path — on the terminal's
    /// own event-loop thread, where the resize originates — and the answer
    /// would be discarded anyway. The reply is left for the next exchange to
    /// step over.
    pub fn announce_resize(&self, pane: PaneId, geom: PaneGeom) {
        let mut conn = self
            .conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if conn.send(&Request::Resize { pane, geom }).is_err() {
            self.lost.store(true, Ordering::SeqCst);
        }
    }

    /// A second connection carrying one pane's bytes: keystrokes up, terminal
    /// output down, and nothing else. Handed straight to the emulator.
    pub fn open_pane_stream(&self, pane: PaneId) -> std::io::Result<UnixStream> {
        let mut stream = UnixStream::connect(&self.socket)?;
        stream.write_all(stream_greeting(pane).as_bytes())?;
        Ok(stream)
    }

    /// Hand the host this window's layout to write.
    ///
    /// The window stops writing the session file when it is attached, and this
    /// is where its saves go instead. Two processes with different ideas of the
    /// tree taking turns overwriting one file is not a race that can be tuned
    /// away — it is one where the loser's work disappears — so there is one
    /// writer, and it is the one holding the terminals, because only it can say
    /// where a pane is or what would resume the agent inside it.
    ///
    /// A refusal comes back as an outcome rather than an error: the verb
    /// worked, and its answer was no.
    pub fn save(
        &self,
        schema: u32,
        body: String,
        allow_shrink: bool,
    ) -> std::io::Result<Persisted> {
        self.exchange(
            Request::Save {
                schema,
                body,
                allow_shrink,
            },
            |reply| match reply {
                Reply::Saved { outcome } => Ok(outcome),
                other => Err(other),
            },
        )
        .and_then(unwrap_outcome)
    }

    /// Ask the host what its own copy of a pane looks like right now.
    ///
    /// Refused for a pane nobody is attached to, which is right: the offset is
    /// per-attachment and a pane with no stream has no place in it to point at.
    pub fn grid_check(&self, pane: PaneId) -> std::io::Result<crate::hostproto::GridCheck> {
        self.exchange(Request::GridCheck { pane }, |reply| match reply {
            Reply::GridChecked { outcome, .. } => Ok(outcome),
            other => Err(other),
        })
        .and_then(unwrap_outcome)
    }

    /// Ask the host to exit. Used by a client that cannot speak its protocol
    /// version, and by anything tidying up after a test.
    #[allow(dead_code)]
    pub fn shutdown(&self) -> std::io::Result<()> {
        self.exchange(Request::Shutdown, |reply| match reply {
            Reply::ShuttingDown => Ok(()),
            other => Err(other),
        })
    }
}

fn unwrap_outcome<T>(outcome: Outcome<T>) -> std::io::Result<T> {
    match outcome {
        Outcome::Ok(value) => Ok(value),
        Outcome::Err(msg) => Err(std::io::Error::other(msg)),
    }
}

/// Listen to a host for as long as the window lives.
///
/// **Its own connection, deliberately.** [`HostLink`] is synchronous — every
/// call writes a line and reads until the answer it asked for arrives — and a
/// host talking unasked on that connection would land its news in the middle of
/// somebody's reply. Rather than teach the request path to route, the window
/// opens a second connection whose entire job is to listen, which also means a
/// host that never speaks costs a blocked thread and nothing else.
///
/// Announced as a tool: this connection attaches no panes and must never take
/// one from the window that is using them.
pub fn watch(socket: &Path) -> std::io::Result<futures::channel::mpsc::UnboundedReceiver<Push>> {
    let stream = UnixStream::connect(socket)?;
    // No read timeout: waiting is the point. A watcher that gave up after five
    // seconds of quiet would report a healthy host as gone every time nothing
    // happened, which on a terminal is most of the time.
    let mut conn = Conn::over(stream, EXCHANGE_TIMEOUT)?;
    match conn.hello(ClientKind::Tool)? {
        Reply::Hello { proto, .. } => {
            if let Err(msg) = crate::hostproto::version_check(proto) {
                return Err(std::io::Error::other(msg));
            }
        }
        other => return Err(std::io::Error::other(format!("odd greeting: {other:?}"))),
    }
    conn.send(&Request::Watch)?;
    conn.expect(|reply| match reply {
        Reply::Watching => Ok(()),
        other => Err(other),
    })?;
    let _ = conn.reader.get_ref().set_read_timeout(None);

    let (tx, rx) = futures::channel::mpsc::unbounded();
    std::thread::spawn(move || {
        let mut line = String::new();
        loop {
            line.clear();
            match conn.reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            // Read as a document first and dispatched on which key it carries.
            // A reply and a push are different kinds of sentence and neither
            // parses as the other, so trying one type and then the next would
            // make an ordinary message look like a broken one.
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
                continue;
            };
            if value.get("push").is_none() {
                continue;
            }
            let Ok(push) = serde_json::from_value::<Push>(value) else {
                continue;
            };
            if tx.unbounded_send(push).is_err() {
                break; // the window has gone
            }
        }
    });
    Ok(rx)
}

/// Start a session host for `key`, and wait for it to be ready to talk.
///
/// Two windows launching at the same moment both find no host and both try to
/// start one — and the second would unlink the first's socket and bind its own,
/// leaving a live host with live terminals that nothing can reach. So the
/// probe, the spawn and the wait happen inside a file lock, and the loser comes
/// out of it finding the winner's host already running.
///
/// The lock is the same primitive that arbitrates session ownership, for the
/// same reason: the kernel drops it however the process dies, so a crash here
/// cannot leave a session unstartable.
pub fn spawn_host(key: &str, budget: Duration) -> std::io::Result<Arc<HostLink>> {
    let _spawn_lock = crate::instance::lock_host_spawn(key);
    // Inside the lock, ask again: while we waited, the window we were racing
    // may have started the very host we were about to duplicate.
    let found = probe_host(key);
    if found.is_live() {
        return HostLink::attach(key);
    }
    // A host from a build that cannot speak to this one. It is asked to
    // checkpoint and stand down before anything else happens, because the
    // alternative is what the reviews found: this process opens a window that
    // owns its own terminals while that host keeps its own, and two processes
    // write one session file. Waited for rather than fired and forgotten — the
    // next thing done here is bind the socket it is still holding.
    if found.is_skewed() {
        eprintln!(
            "terminal-delight: the host for session '{key}' speaks a protocol this build does \
             not — asking it to checkpoint and stand down"
        );
        stand_down_at(&host_socket_path(key), STAND_DOWN_BUDGET)?;
    }

    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("serve")
        .arg("--session")
        .arg(key)
        .stdin(std::process::Stdio::null())
        // A host outlives the window that started it, so its output cannot go
        // to that window's terminal. It goes next to the socket, where anyone
        // looking for the session will already be looking.
        .stdout(host_log(key))
        .stderr(host_log(key));
    // Its own session leader. Without this the host stays in the launching
    // window's process group and dies with the terminal that started it —
    // precisely the loss this feature exists to end, and one that no `kill -9`
    // test would ever catch, because -9 sends no hangup to anybody.
    use std::os::unix::process::CommandExt;
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = cmd.spawn()?;

    let deadline = Instant::now() + budget;
    while Instant::now() < deadline {
        if probe_host(key).is_live() {
            return HostLink::attach(key);
        }
        if let Ok(Some(status)) = child.try_wait() {
            // A host that exits is not necessarily a host that failed. `serve`
            // refuses to start a second host for a session somebody else is
            // already serving, and says so by exiting 0 — which is an answer
            // ("one exists, go and connect to it"), not an error. So the socket
            // is asked before the exit code is believed.
            if probe_host(key).is_live() {
                return HostLink::attach(key);
            }
            return Err(std::io::Error::other(format!(
                "the session host exited before it was ready ({status}) — see {}",
                host_log_path(key).display()
            )));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        format!(
            "the session host did not answer within {budget:?} — see {}",
            host_log_path(key).display()
        ),
    ))
}

fn host_log_path(key: &str) -> PathBuf {
    host_socket_path(key).with_extension("log")
}

fn host_log(key: &str) -> std::process::Stdio {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(host_log_path(key))
        .map(std::process::Stdio::from)
        .unwrap_or_else(|_| std::process::Stdio::null())
}

#[cfg(test)]
mod talking {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("td-hostctl-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn nothing_there_is_not_a_host_and_is_not_an_error() {
        let dir = tmp("absent");
        assert_eq!(
            probe_at(&dir.join("session-nobody.sock"), PROBE_BUDGET),
            HostProbe::NoSocket
        );
        assert!(sockets_present_in(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_socket_nobody_listens_on_is_a_candidate_but_not_a_host() {
        // The exact residue a host killed with -9 leaves behind. It must read
        // as "no host" — a relaunch that treated it as live would attach to
        // nothing and show the user an empty window instead of their work.
        let dir = tmp("stale");
        let path = dir.join("session-stale.sock");
        std::fs::write(&path, b"").expect("write a stale socket file");
        assert_eq!(sockets_present_in(&dir), vec!["stale".to_string()]);
        assert_eq!(probe_at(&path, PROBE_BUDGET), HostProbe::NoSocket);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_listener_that_never_answers_is_unresponsive_not_absent() {
        // Unknown is not zero. A host wedged mid-answer is a machine somebody
        // should look at; reporting it as "no host" would start a second one
        // beside it and split the session's panes across two processes.
        let dir = tmp("wedged");
        let path = dir.join("session-wedged.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
        // Accept, then say nothing at all.
        let held = std::thread::spawn(move || listener.accept().map(|(s, _)| s));
        let verdict = probe_at(&path, Duration::from_millis(200));
        assert_eq!(verdict, HostProbe::Unresponsive, "{verdict:?}");
        drop(held.join());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_host_that_refuses_our_version_is_skewed_rather_than_unresponsive() {
        // The two look identical from a distance — neither answers a greeting
        // — and they call for opposite things. A host that will not answer is
        // left strictly alone; a host that answers and refuses is one this
        // build may ask to stand down, and must, or the session behind it is
        // unreachable for as long as it runs.
        let dir = tmp("skew");
        let path = dir.join("session-skew.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
        let served = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut line = String::new();
            let read = reader.read_line(&mut line);
            let mut writer = stream;
            let wrote = writer.write_all(
                b"{\"reply\":\"error\",\"msg\":\"protocol mismatch: this build speaks 2, \
                  the other side speaks 1\"}\n",
            );
            (line, read, wrote)
        });
        let verdict = probe_at(&path, EXCHANGE_TIMEOUT);
        let served = served.join();
        assert_eq!(verdict, HostProbe::Skewed, "the fake host said: {served:?}");
        assert!(verdict.is_skewed());
        assert!(!verdict.is_live() && !verdict.is_absent() && !verdict.is_free());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn standing_a_host_down_asks_it_to_go_and_waits_until_it_has() {
        // What a launch does when it meets a host from another build. Both
        // halves matter: it has to ask, and it has to still be there when the
        // asking is over — the next thing the caller does is bind that socket.
        let dir = tmp("standdown");
        let path = dir.join("session-standdown.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
        let socket = path.clone();
        let served = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut writer = stream;
            let mut said = Vec::new();
            for reply in [
                &b"{\"reply\":\"error\",\"msg\":\"protocol mismatch: this build speaks 2, the other side speaks 1\"}\n"[..],
                &b"{\"reply\":\"shutting-down\"}\n"[..],
            ] {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                said.push(line);
                let _ = writer.write_all(reply);
            }
            // A host that has stood down is gone, socket and all.
            drop(writer);
            drop(listener);
            let _ = std::fs::remove_file(&socket);
            said
        });

        stand_down_at(&path, Duration::from_secs(5)).expect("stand down");
        let said = served.join().expect("the conversation");
        assert!(
            said[0].contains(r#""verb":"hello""#),
            "it never introduced itself: {said:?}"
        );
        assert!(
            said[1].contains(r#""verb":"shutdown""#),
            "it never asked the host to go: {said:?}"
        );
        assert!(
            !path.exists(),
            "the caller was told the host had gone while its socket was still there"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_probe_never_claims_a_pane() {
        // Every launch probes every candidate. If a probe attached, opening a
        // second window would take the panes off the first one — so the kind on
        // the wire is the whole of that guarantee, and it is asserted here
        // rather than trusted.
        let dir = tmp("kind");
        let path = dir.join("session-kind.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
        let heard = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut line = String::new();
            reader.read_line(&mut line).expect("read the hello");
            let mut writer = stream;
            let _ = writer.write_all(
                b"{\"reply\":\"hello\",\"proto\":1,\"session\":\"kind\",\"panes\":2,\"attended\":false}\n",
            );
            line
        });
        let verdict = probe_at(&path, EXCHANGE_TIMEOUT);
        assert_eq!(
            verdict,
            HostProbe::Live {
                proto: 1,
                session: "kind".into(),
                panes: 2,
                attended: false,
            }
        );
        assert!(verdict.is_free(), "a host nobody is watching is adoptable");
        let hello = heard.join().expect("the greeting");
        assert!(
            hello.contains(r#""kind":"tool""#),
            "a probe must announce itself as a tool: {hello}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_reply_left_behind_by_a_resize_does_not_desynchronise_the_next_call() {
        // Resizes are fired and not waited on, so their acknowledgements arrive
        // whenever. A client that read the next line as its own answer would be
        // one reply behind for the rest of the session — and would read a
        // pane's geometry as its pane list.
        let dir = tmp("skew");
        let path = dir.join("session-skew.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut writer = stream;
            let mut line = String::new();
            reader.read_line(&mut line).expect("hello");
            let _ = writer.write_all(
                b"{\"reply\":\"hello\",\"proto\":1,\"session\":\"skew\",\"panes\":0,\"attended\":false}\n",
            );
            line.clear();
            reader.read_line(&mut line).expect("list");
            // A stale resize acknowledgement first, then the real answer.
            let _ = writer.write_all(
                b"{\"reply\":\"resized\",\"pane\":9,\"outcome\":{\"ok\":null}}\n\
                  {\"reply\":\"panes\",\"panes\":[]}\n",
            );
        });
        let link = HostLink::attach_at(&path).expect("attach");
        assert_eq!(link.list_panes().expect("a pane list"), vec![]);
        assert!(!link.lost(), "a stale reply is not a lost host");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_host_that_hangs_up_is_recorded_as_lost() {
        // The window must know, because a window that keeps saving after its
        // host has gone writes a layout of dead panes over a good one.
        let dir = tmp("lost");
        let path = dir.join("session-lost.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut writer = stream;
            let mut line = String::new();
            reader.read_line(&mut line).expect("hello");
            let _ = writer.write_all(
                b"{\"reply\":\"hello\",\"proto\":1,\"session\":\"lost\",\"panes\":0,\"attended\":false}\n",
            );
            // and then die, the way a killed host does
        });
        let link = HostLink::attach_at(&path).expect("attach");
        assert!(!link.lost(), "not lost until something actually fails");
        assert!(link.list_panes().is_err(), "the host is gone");
        assert!(link.lost(), "and the window knows it");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_watching_connection_hears_changes_and_is_not_confused_by_replies() {
        // The listening half, against a host that says all the things a host
        // says: a greeting, an acknowledgement, and then news — with an
        // ordinary reply in the middle of the news, because a connection that
        // asked one question still gets its answer on the same wire, and a
        // watcher that mistook a reply for a change would either drop it as
        // broken or, worse, treat it as one.
        let dir = tmp("watching");
        let path = dir.join("session-watching.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
        let heard = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut writer = stream;
            let mut hello = String::new();
            reader.read_line(&mut hello).expect("the greeting");
            let _ = writer.write_all(
                b"{\"reply\":\"hello\",\"proto\":1,\"session\":\"watching\",\"panes\":1,\"attended\":false}\n",
            );
            let mut asked = String::new();
            reader.read_line(&mut asked).expect("the watch verb");
            let _ = writer.write_all(b"{\"reply\":\"watching\"}\n");
            let _ = writer.write_all(
                b"{\"push\":\"mode\",\"pane\":1,\"mode\":\"claude\"}\n\
                  {\"reply\":\"panes\",\"panes\":[]}\n\
                  {\"push\":\"mode\",\"pane\":2,\"mode\":{\"other\":\"vim\"}}\n",
            );
            std::thread::sleep(Duration::from_millis(400));
            (hello, asked)
        });

        let mut news = watch(&path).expect("start watching");
        let first =
            futures::executor::block_on(futures::StreamExt::next(&mut news)).expect("a change");
        assert_eq!(
            first,
            Push::Mode {
                pane: PaneId(1),
                mode: crate::hostproto::WireMode::Claude
            }
        );
        let second = futures::executor::block_on(futures::StreamExt::next(&mut news))
            .expect("the change after the reply");
        assert_eq!(
            second,
            Push::Mode {
                pane: PaneId(2),
                mode: crate::hostproto::WireMode::Other("vim".into())
            },
            "a reply in the middle must be stepped over, not mistaken for news"
        );

        let (hello, asked) = heard.join().expect("the host thread");
        assert!(
            hello.contains(r#""kind":"tool""#),
            "a listening connection attaches nothing and must say so: {hello}"
        );
        assert!(asked.contains(r#""verb":"watch""#), "{asked}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_watcher_outlives_a_long_silence() {
        // Silence is the normal state of a terminal nobody is typing into, and
        // it lasts longer than any exchange timeout. A watcher that gave up on
        // quiet would leave the window showing whatever the modes were when it
        // attached, with nothing to say it had stopped listening — so what is
        // asserted here is not that nothing arrives during the silence, but
        // that something still arrives AFTER it.
        let dir = tmp("quiet");
        let path = dir.join("session-quiet.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut writer = stream;
            let mut line = String::new();
            reader.read_line(&mut line).expect("hello");
            let _ = writer.write_all(
                b"{\"reply\":\"hello\",\"proto\":1,\"session\":\"quiet\",\"panes\":0,\"attended\":false}\n",
            );
            line.clear();
            reader.read_line(&mut line).expect("watch");
            let _ = writer.write_all(b"{\"reply\":\"watching\"}\n");
            // Longer than EXCHANGE_TIMEOUT, which is the point.
            std::thread::sleep(EXCHANGE_TIMEOUT + Duration::from_secs(2));
            let _ = writer.write_all(b"{\"push\":\"mode\",\"pane\":4,\"mode\":\"shell\"}\n");
            std::thread::sleep(Duration::from_millis(200));
        });

        let mut news = watch(&path).expect("start watching");
        let after_the_quiet = futures::executor::block_on(futures::StreamExt::next(&mut news));
        assert_eq!(
            after_the_quiet,
            Some(Push::Mode {
                pane: PaneId(4),
                mode: crate::hostproto::WireMode::Shell
            }),
            "a watcher that survived the silence should still be listening; \
             None here means it hung up on a healthy host"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_signal_arriving_mid_answer_is_not_the_host_hanging_up() {
        // A read interrupted by a signal fails with EINTR, and to anything that
        // treats an error as an ending it is indistinguishable from the far end
        // closing. That confusion is expensive here in both directions: a
        // window that read a stray signal as a lost host would stop writing its
        // session file while the host sat there perfectly healthy, and — since
        // a window now asks the host to explain an ending — a spurious hangup
        // on a pane stream would have it conclude the pane was stolen and
        // freeze a terminal that is running fine.
        //
        // std retries on `Interrupted` inside `read_until`, which is what makes
        // this safe. That is a documented behaviour of somebody else's code,
        // which is exactly the kind of thing worth a test rather than a
        // comment: it is load-bearing here, and nothing in this file would
        // notice if it changed.
        // Counted, because a test that never actually got interrupted would
        // pass for the wrong reason and keep passing if the retry disappeared.
        static ARRIVED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        extern "C" fn nothing(_: libc::c_int) {
            ARRIVED.fetch_add(1, Ordering::Relaxed);
        }

        let dir = tmp("signals");
        let path = dir.join("session-signals.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut writer = stream;
            let mut line = String::new();
            reader.read_line(&mut line).expect("hello");
            let _ = writer.write_all(
                b"{\"reply\":\"hello\",\"proto\":1,\"session\":\"signals\",\"panes\":0,\"attended\":false}\n",
            );
            line.clear();
            reader.read_line(&mut line).expect("list");
            // Answer late, so the reader is genuinely blocked while the signals
            // land rather than racing them.
            std::thread::sleep(Duration::from_millis(400));
            let _ = writer.write_all(b"{\"reply\":\"panes\",\"panes\":[]}\n");
        });

        let link = HostLink::attach_at(&path).expect("attach");

        // A handler with no SA_RESTART, so the signal interrupts the read
        // instead of the kernel quietly restarting it — the hostile case.
        let previous = unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            let mut old: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = nothing as *const () as usize;
            action.sa_flags = 0;
            libc::sigaction(libc::SIGUSR1, &action, &mut old);
            old
        };
        // Signal THIS thread only: every other test in the process is running
        // beside this one and none of them asked to be interrupted.
        let waiting = unsafe { libc::syscall(libc::SYS_gettid) } as libc::pid_t;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let storm = std::thread::spawn(move || {
            for _ in 0..40 {
                if flag.load(Ordering::SeqCst) {
                    break;
                }
                unsafe {
                    libc::syscall(libc::SYS_tgkill, libc::getpid(), waiting, libc::SIGUSR1);
                }
                std::thread::sleep(Duration::from_millis(15));
            }
        });

        let answer = link.list_panes();
        stop.store(true, Ordering::SeqCst);
        storm.join().expect("the signaller");
        unsafe { libc::sigaction(libc::SIGUSR1, &previous, std::ptr::null_mut()) };

        assert!(
            ARRIVED.load(Ordering::Relaxed) > 0,
            "no signal ever reached the waiting thread, so this proved nothing"
        );
        assert!(
            answer.is_ok(),
            "a signal was read as the host going away: {:?}",
            answer.err()
        );
        assert!(!link.lost(), "and the window wrote itself off over it");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_version_it_cannot_speak_is_refused_rather_than_half_understood() {
        let dir = tmp("version");
        let path = dir.join("session-version.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).expect("bind");
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut writer = stream;
            let mut line = String::new();
            reader.read_line(&mut line).expect("hello");
            let _ = writer.write_all(
                b"{\"reply\":\"hello\",\"proto\":99,\"session\":\"version\",\"panes\":0,\"attended\":false}\n",
            );
            std::thread::sleep(Duration::from_millis(200));
        });
        let said = match HostLink::attach_at(&path) {
            Ok(_) => panic!("a version this build cannot speak must be refused"),
            Err(refused) => refused.to_string(),
        };
        assert!(said.contains("99"), "name the other side's version: {said}");
        assert!(
            said.contains(&PROTO_VERSION.to_string()),
            "and our own: {said}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
