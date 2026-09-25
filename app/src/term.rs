//! A pane's terminal: a core behind TD's boundary (`crate::vt`), TD's own read
//! loop over a pseudoterminal or a session host's socket, and the channel the
//! pane reads the terminal's events from.
//!
//! NOTE: `Event::PtyWrite` (a reply to a program's question — DA, DSR) arrives
//! through the proxy and is NOT written back to the program by the loop — the
//! pane must bounce it through the notifier, or interactive programs hang
//! waiting for an answer.

use std::fs::File;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::vt::{self, Event as TermEvent, Listener, Notifier, TermSize, WindowSize};
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};

/// A terminal's grid, in cells.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GridSize {
    pub cols: usize,
    pub rows: usize,
}

impl GridSize {
    /// The size a core is told: this grid, and one cell's pixels.
    pub fn with_cell(self, cell_width: u16, cell_height: u16) -> TermSize {
        TermSize::new(self.cols, self.rows, cell_width, cell_height)
    }
}

/// Forwards terminal events from the read loop onto an async channel,
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
/// where is the cursor (DSR), how big is the text area (`CSI 14 t`) — each of
/// which must be answered exactly once.
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

impl Listener for EventProxy {
    fn send_event(&self, event: TermEvent) {
        // The counter moves for every event, forwarded or swallowed: it is the
        // invalidation token for cached views, and an event we decline to
        // answer still means the emulation has moved on. A replica whose
        // generation stopped advancing would freeze every cache built on it.
        self.generation.fetch_add(1, Ordering::Relaxed);
        // `CSI 14 t` travels as its own event rather than as a `PtyWrite`, so
        // it has to be named: the host answers it from the geometry it holds,
        // and a pane answering it too would type a second reply at the program.
        //
        // A colour question (`ColorRequest`) is the opposite case and is
        // passed on deliberately. The host has no theme and answers nothing;
        // the pane answers from the colours it draws, so swallowing it here
        // would leave it unanswered, which was issue 719.
        if !self.answers_here
            && matches!(
                event,
                TermEvent::PtyWrite(_) | TermEvent::TextAreaSizeRequest(_)
            )
        {
            return;
        }
        let _ = self.events.unbounded_send(event);
    }
}

pub struct Session {
    pub term: vt::Shared,
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
    /// [`crate::vt::socket::SocketSource`]), because a terminal that more than
    /// one client may be looking at is told its size rather than inferring it.
    ///
    /// The cell's pixels go to the core as well as the kernel: a core places a
    /// program's pictures in cells, and answers `CSI 16 t`, from them.
    pub fn resize(&self, size: GridSize, cell_width: u16, cell_height: u16) {
        let size = size.with_cell(cell_width, cell_height);
        self.notifier.resize(size.window_size());
        self.term.lock().resize(size);
    }
}

/// Everything a pseudoterminal needs to start a shell, plus the environment the
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
    // Read before the pseudoterminal is handed to the loop, which takes
    // ownership of it and asks only what every source can answer.
    let master = pty.file().try_clone().ok();
    let shell_pid = pty.child().id();
    wire_pump(
        Box::new(pty),
        size.with_cell(cell_width, cell_height),
        master,
        Some(shell_pid),
        Answers::Here,
        None,
    )
}

/// The pseudoterminal half of a spawn: a real kernel object with a real child.
pub fn spawn_pty(spec: &SpawnSpec) -> io::Result<vt::pty::PtySource> {
    let mut options = vt::pty::Options {
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
            options.shell = Some((
                exe.to_string_lossy().into_owned(),
                vec!["--td-emit-demo".to_string()],
            ));
        }
    }
    let size = spec.size.with_cell(spec.cell_width, spec.cell_height);
    vt::pty::spawn(&options, size.window_size())
}

/// The emulator half: a core, TD's read loop over whatever the terminal
/// actually is, and the channel the pane reads events from.
///
/// Any [`vt::pump::Source`] will do — a pseudoterminal this window started, or
/// a session host's socket — which is what makes a replica an ordinary
/// [`Session`]. `master` and `shell_pid` are passed in rather than asked of the
/// source, because on a replica neither exists to ask. `tap` sees each chunk of
/// output under the terminal's lock before it is parsed.
pub fn wire_pump(
    source: Box<dyn vt::pump::Source>,
    size: TermSize,
    master: Option<File>,
    shell_pid: Option<u32>,
    answers: Answers,
    tap: Option<Box<dyn vt::pump::Tap>>,
) -> io::Result<Session> {
    let (tx, rx) = unbounded();
    let generation = Arc::new(AtomicU64::new(0));
    let proxy: Arc<dyn Listener> = Arc::new(EventProxy::new(tx, generation.clone(), answers));
    let term = vt::shared(vt::Term::new(size, proxy.clone()));
    // The loop owns its thread and lives as long as the program it reads.
    let (notifier, _thread) = vt::pump::spawn(term.clone(), proxy, source, tap)?;

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
    /// read loop's own thread, hence `Send`.
    pub announce_resize: Box<dyn FnMut(WindowSize) + Send + 'static>,
}

/// THE SEAM: a terminal this process does not own, driven by TD's read loop
/// over a socket, into an ordinary [`Session`].
///
/// The same type comes back as [`spawn_in`] returns, which is the whole point —
/// every one of the pane's reads (styled lines, selection, scrollback, the
/// FOCUS document mirror) runs against the replica unchanged, and selection and
/// scroll position stay client state by construction, because they were never
/// anywhere else.
///
/// `size` is the host's, cell included: the snapshot on the stream was encoded
/// from a grid of exactly that shape. `shell_pid` is what the host reported: an
/// attribute of a process this window did not start, or `None` if the host
/// named none.
pub fn attach_in(
    size: TermSize,
    streams: AttachStreams,
    shell_pid: Option<u32>,
) -> io::Result<(Session, crate::gridwire::ReplicaGuard)> {
    let AttachStreams {
        bytes,
        announce_resize,
    } = streams;
    let source = vt::socket::SocketSource::with_resize(bytes, announce_resize)?;
    // Shared with the loop's tap: the count of bytes that have actually reached
    // this replica's parser, which is the clock the divergence guard reads.
    let consumed = Arc::new(AtomicU64::new(0));
    let tap = vt::socket::CountingTap(consumed.clone());
    let session = wire_pump(
        Box::new(source),
        size,
        None,
        shell_pid,
        Answers::Elsewhere,
        Some(Box::new(tap)),
    )?;
    let guard = crate::gridwire::ReplicaGuard::new(
        session.term.clone(),
        session.generation.clone(),
        consumed,
    );
    Ok((session, guard))
}

/// A terminal this process owns, running `program` rather than the user's
/// shell: a real pseudoterminal, a real child, and everything above the child
/// — the read loop, the parser, the core — the code a pane runs.
///
/// For tests, which need to choose what the terminal prints. [`spawn_in`]
/// deliberately runs whatever shell the person has, and a test that depended
/// on their prompt or their rc files would be testing their machine.
#[cfg(test)]
pub(crate) fn spawn_program(
    size: GridSize,
    cell_px: (u16, u16),
    program: &str,
    args: &[&str],
) -> io::Result<Session> {
    let options = vt::pty::Options {
        shell: Some((
            program.to_string(),
            args.iter().map(|a| a.to_string()).collect(),
        )),
        ..Default::default()
    };
    let size = size.with_cell(cell_px.0, cell_px.1);
    let pty = vt::pty::spawn(&options, size.window_size())?;
    let master = pty.file().try_clone().ok();
    let shell_pid = pty.child().id();
    wire_pump(
        Box::new(pty),
        size,
        master,
        Some(shell_pid),
        Answers::Here,
        None,
    )
}

/// How long a keystroke takes to come back, with and without the seam.
///
/// This is the instrument behind the flip gate: the split becomes the default
/// only if typing into an attached window feels like typing into today's, and
/// "feels like" is not a thing anybody can settle by discussion. So the same
/// measurement runs against both — a terminal this process owns, and one a
/// session host owns two socket hops away — and the difference between them is
/// the cost of the seam.
///
/// **The signal is the content generation**, which TD's read loop bumps once
/// per parse (a `Wakeup`, from `vt::pump`) after the bytes it read are in the
/// grid. So a sample is exactly: write a byte the way the GUI writes one, and
/// wait until the terminal has drawn what came back. Not a proxy for that, and
/// not a sleep.
///
/// **The instrument is identical in both modes** — same program in the pane
/// (`cat`, so the echo comes from the line discipline and nothing else is
/// running), same number of flooding panes beside it, same loop. That is the
/// whole design: any overhead the harness itself carries is carried twice and
/// cancels in the difference.
///
/// Driven by `scripts/td-echo-bench.sh`, which runs it four times over
/// mode × flood and applies the gate. Ignored by default because it takes
/// seconds, spawns processes, and answers a question the suite is not asking.
#[cfg(test)]
mod echo_bench {
    use std::io::Write;
    use std::time::{Duration, Instant};

    use super::*;

    /// Samples taken per run, after the warm-up. A thousand is enough for a p99
    /// to mean something and quick enough that four runs are a coffee.
    const SAMPLES: usize = 1000;
    /// Discarded: the first keystrokes into a fresh terminal pay for pages the
    /// process has not touched yet, and a flip gate is about the steady state.
    const WARM_UP: usize = 100;
    /// A sample that takes this long has not measured latency, it has measured
    /// something being broken.
    const PATIENCE: Duration = Duration::from_secs(5);

    fn env_or(name: &str, fallback: &str) -> String {
        std::env::var(name).unwrap_or_else(|_| fallback.to_string())
    }

    /// What a flooding pane runs, and there are two honest answers.
    ///
    /// `yes` at full rate is a stress case: it writes as fast as a
    /// pseudoterminal will take it, which on a machine with no spare core is a
    /// measurement of the scheduler more than of the seam. A busy build is
    /// nothing like it — a couple of thousand lines a second, in bursts, with
    /// gaps. Both are worth knowing and they answer different questions, so the
    /// bench says which one it ran rather than letting a number stand for both.
    fn flood_program(kind: &str) -> (&'static str, Vec<String>) {
        match kind {
            "saturating" => ("yes", vec!["flooding-the-terminal-with-output".to_string()]),
            // ~2000 lines a second per pane, in bursts: more than a talkative
            // build, and leaves the machine cores to schedule with.
            _ => (
                "sh",
                vec![
                    "-c".to_string(),
                    "while :; do seq 1 200; sleep 0.1; done".to_string(),
                ],
            ),
        }
    }

    /// One keystroke, there and back.
    fn sample(session: &Session) -> Duration {
        let before = session.content_generation();
        let started = Instant::now();
        session.notifier.notify(b"x".to_vec());
        loop {
            if session.content_generation() != before {
                return started.elapsed();
            }
            if started.elapsed() > PATIENCE {
                panic!("a keystroke never came back — the terminal is not echoing");
            }
            std::hint::spin_loop();
        }
    }

    fn percentile(sorted: &[Duration], p: f64) -> u128 {
        let at = ((sorted.len() as f64 - 1.0) * p).round() as usize;
        sorted[at].as_micros()
    }

    /// A terminal this process owns, running `prog`.
    ///
    /// Built here rather than through `spawn_in` because the bench needs to
    /// choose the program, and `spawn_in` deliberately runs the user's shell.
    /// Everything below the program — the event loop, the parser, the Term — is
    /// the same code the pane uses.
    fn local_pane(prog: &str, args: &[&str]) -> Session {
        let size = GridSize {
            cols: 100,
            rows: 28,
        };
        spawn_program(size, (8, 20), prog, args).expect("a pseudoterminal")
    }

    /// The same shape, two socket hops away: a real session host in its own
    /// runtime directory, with the panes attached exactly as a window attaches
    /// them.
    fn hosted_panes(
        binary: &str,
        flood: usize,
        watch_flood: bool,
        flood_kind: &str,
    ) -> (std::process::Child, Vec<Session>) {
        let run = std::env::temp_dir().join(format!("td-echo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&run);
        std::fs::create_dir_all(&run).expect("a place to run");

        // One host, two programs: the pane being measured runs `cat` so its echo
        // comes from the line discipline alone, and every other pane floods. The
        // host stamps TD_PANE_ID into each pane, which is what lets one shell
        // script be both.
        let shell = run.join("bench-shell.sh");
        let flooding = match flood_kind {
            "saturating" => "exec yes flooding-the-terminal-with-output".to_string(),
            _ => "while :; do seq 1 200; sleep 0.1; done".to_string(),
        };
        std::fs::write(
            &shell,
            format!("#!/bin/sh\nif [ \"$TD_PANE_ID\" = \"1\" ]; then exec cat; fi\n{flooding}\n"),
        )
        .expect("write the pane program");
        let mut permissions = std::fs::metadata(&shell).expect("stat").permissions();
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o755);
        std::fs::set_permissions(&shell, permissions).expect("make it runnable");

        let session = "echo-bench";
        let host = std::process::Command::new(binary)
            .args(["serve", "--session", session])
            // Child-only environment: the bench never touches its own, so it
            // cannot disturb anything else running in this process.
            .env("XDG_RUNTIME_DIR", &run)
            .env("XDG_CONFIG_HOME", run.join("config"))
            .env("TD_HOST_SHELL", &shell)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("start a session host");

        let socket = run
            .join("terminal-delight")
            .join(format!("session-{session}.sock"));
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline && !socket.exists() {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(socket.exists(), "the host never bound {}", socket.display());

        let link = crate::hostctl::HostLink::attach_at(&socket).expect("attach to the host");
        let geom = crate::hostproto::PaneGeom {
            cols: 100,
            rows: 28,
            cell_width: 8,
            cell_height: 20,
        };
        let mut sessions = vec![];
        for index in 0..=flood {
            // No recipe: the bench measures a terminal echoing, and an agent in
            // the pane would be measuring the agent.
            let (info, _) = link.spawn_pane(None, None, geom).expect("start a pane");
            // A flooding pane the client does not attach still floods the HOST:
            // its pseudoterminal is read and parsed there either way. Leaving it
            // unattached is how the bench tells the two costs apart — what the
            // host pays to keep the truth, and what the window pays to draw a
            // copy of it.
            if index > 0 && !watch_flood {
                continue;
            }
            link.attach_pane(info.pane, geom)
                .expect("declare the attach");
            let stream = link.open_pane_stream(info.pane).expect("the pane's bytes");
            let announce = link.clone();
            let pane = info.pane;
            let (session, _guard) = attach_in(
                GridSize {
                    cols: 100,
                    rows: 28,
                }
                .with_cell(8, 20),
                AttachStreams {
                    bytes: stream,
                    announce_resize: Box::new(move |size: WindowSize| {
                        announce.announce_resize(
                            pane,
                            crate::hostproto::PaneGeom {
                                cols: size.num_cols,
                                rows: size.num_lines,
                                cell_width: size.cell_width,
                                cell_height: size.cell_height,
                            },
                        );
                    }),
                },
                Some(info.shell_pid),
            )
            .expect("attach the pane");
            // The guard is dropped: this bench measures latency, and divergence
            // is somebody else's test.
            sessions.push(session);
        }
        (host, sessions)
    }

    #[test]
    #[ignore = "latency bench — run via scripts/td-echo-bench.sh"]
    fn echo_latency_bench() {
        let mode = env_or("TD_ECHO_MODE", "local");
        // Default to the load a person actually produces. The saturating case
        // is still one env var away, and still worth running — but a gate
        // reported against it would be answering a question nobody asked.
        let flood_kind = env_or("TD_ECHO_FLOOD_KIND", "realistic");
        let flood: usize = env_or("TD_ECHO_FLOOD", "0").parse().expect("TD_ECHO_FLOOD");
        let samples: usize = env_or("TD_ECHO_SAMPLES", &SAMPLES.to_string())
            .parse()
            .expect("TD_ECHO_SAMPLES");

        let _guard = crate::testsync::forks_and_locks();
        let mut host = None;
        let measured: Session;
        let mut _flooding: Vec<Session> = vec![];

        match mode.as_str() {
            "local" => {
                measured = local_pane("cat", &[]);
                let (program, args) = flood_program(&flood_kind);
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                for _ in 0..flood {
                    _flooding.push(local_pane(program, &args));
                }
            }
            "attached" => {
                // Never a number that was not measured: a bench that quietly
                // fell back to the local path would report the seam as free.
                let binary = env_or("TD_BIN", "");
                assert!(
                    !binary.is_empty() && std::path::Path::new(&binary).exists(),
                    "attached mode needs TD_BIN pointing at a built terminal-delight; \
                     refusing to report a number this run did not measure"
                );
                let watch_flood = env_or("TD_ECHO_WATCH_FLOOD", "1") != "0";
                let (child, mut sessions) = hosted_panes(&binary, flood, watch_flood, &flood_kind);
                host = Some(child);
                measured = sessions.remove(0);
                _flooding = sessions;
            }
            other => panic!("TD_ECHO_MODE must be local or attached, not {other:?}"),
        }

        // Wait for the pane to be a terminal before timing anything: a shell
        // that has not finished starting is not a latency measurement.
        let ready = Instant::now();
        while measured.content_generation() == 0 {
            sample(&measured);
            assert!(
                ready.elapsed() < Duration::from_secs(20),
                "the measured pane never echoed anything"
            );
        }
        for _ in 0..WARM_UP {
            sample(&measured);
        }

        let mut taken: Vec<Duration> = Vec::with_capacity(samples);
        for _ in 0..samples {
            taken.push(sample(&measured));
        }
        taken.sort();

        let watching = env_or("TD_ECHO_WATCH_FLOOD", "1") != "0";
        // The tail is reported with its shape, not just its edge. A p99 alone
        // cannot tell "every keystroke is slightly slow" from "one keystroke in
        // a hundred stalls", and those are different experiences to sit in front
        // of — the first is a slow terminal, the second is a terminal that
        // hiccups. `over_1ms` is the count a person would actually notice.
        let over_1ms = taken.iter().filter(|d| d.as_micros() > 1000).count();
        let line = format!(
            r#"{{"mode":"{mode}","flood":{flood},"load":"{flood_kind}","watched":{watching},"samples":{samples},"p50_us":{},"p99_us":{},"p999_us":{},"max_us":{},"over_1ms":{over_1ms}}}"#,
            percentile(&taken, 0.50),
            percentile(&taken, 0.99),
            percentile(&taken, 0.999),
            taken.last().expect("samples").as_micros(),
        );
        println!("{line}");
        let _ = std::io::stdout().flush();

        drop(measured);
        drop(std::mem::take(&mut _flooding));
        if let Some(mut host) = host.take() {
            let _ = host.kill();
            let _ = host.wait();
            let _ = std::fs::remove_dir_all(
                std::env::temp_dir().join(format!("td-echo-{}", std::process::id())),
            );
        }
    }
}

/// The client seam, driven for real.
///
/// A socket pair stands in for the host: one end is handed to [`attach_in`] and
/// becomes an ordinary [`Session`], the other is written to and read from the
/// way a session host would. Nothing here is a mock — it is TD's own read loop
/// and the compiled-in core, which is the whole argument for cutting the seam at
/// a file descriptor.
#[cfg(test)]
mod attached {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use super::*;
    use crate::gridwire::{grid_hash, GridCheck, GuardVerdict, Unsettled};
    use crate::vt::{Column, Line, Point, Term};

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
            GridSize { cols, rows }.with_cell(8, 16),
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
                term.lock().cell(Point::new(Line(1), Column(4))).c == 'd'
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
            within(Duration::from_secs(5), || pair.session.content_generation()
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
            matches!(rx.try_recv(), Ok(TermEvent::PtyWrite(_))),
            "a terminal we own must still forward its own answers"
        );
    }

    /// A colour question reaches the pane from a replica, because the pane is
    /// what answers it: the host has no theme (issue 719). First the filter
    /// on its own, then end to end over a real replica's byte stream, where
    /// the question arrives exactly once, with a reply format that ends the
    /// way the question did.
    #[test]
    fn a_replica_passes_a_colour_question_to_its_pane() {
        use crate::vt::Rgb;
        let reply: Arc<dyn Fn(Rgb) -> String + Sync + Send> =
            Arc::new(|c| format!("{:02x}{:02x}{:02x}", c.r, c.g, c.b));
        let (tx, mut rx) = unbounded();
        let replica = EventProxy::new(tx, Arc::new(AtomicU64::new(0)), Answers::Elsewhere);
        replica.send_event(TermEvent::ColorRequest(257, reply));
        assert!(
            matches!(rx.try_recv(), Ok(TermEvent::ColorRequest(257, _))),
            "a replica swallowed a colour question nobody else answers"
        );

        let mut pair = attached(20, 5, Some(1));
        let mut events = pair.session.events.take().expect("events");
        pair.host
            .write_all(b"\x1b]11;?\x07done")
            .expect("host writes");
        let term = pair.session.term.clone();
        assert!(
            within(Duration::from_secs(5), || {
                term.lock().cell(Point::new(Line(0), Column(3))).c == 'e'
            }),
            "the host's bytes never reached the replica"
        );
        let mut asked = 0;
        while let Ok(event) = events.try_recv() {
            if let TermEvent::ColorRequest(index, format) = event {
                assert_eq!(index, 257, "OSC 11 asks for the default background");
                let bg = Rgb {
                    r: 0x10,
                    g: 0x14,
                    b: 0x1c,
                };
                assert_eq!(format(bg), "\x1b]11;rgb:1010/1414/1c1c\x07");
                asked += 1;
            }
        }
        assert_eq!(asked, 1, "one question, one event");
    }

    #[test]
    fn a_replica_swallows_a_size_question_the_host_answers() {
        // `CSI 14 t` asks for the text area in pixels. The host answers it
        // from the geometry it holds; a replica forwarding it too would hand
        // the pane a question that, once the pane learned to answer it, would
        // be answered twice — and the second reply reaches the program as
        // typed input. alacritty sends this one as its own event rather than
        // as a `PtyWrite`, so the filter has to name it.
        let reply: Arc<dyn Fn(WindowSize) -> String + Sync + Send> =
            Arc::new(|size| format!("{}x{}", size.num_cols, size.num_lines));

        let (tx, mut rx) = unbounded();
        let replica = EventProxy::new(tx, Arc::new(AtomicU64::new(0)), Answers::Elsewhere);
        replica.send_event(TermEvent::TextAreaSizeRequest(reply.clone()));
        assert!(
            rx.try_recv().is_err(),
            "a replica forwarded a size question the host has already answered"
        );

        let (tx, mut rx) = unbounded();
        let local = EventProxy::new(tx, Arc::new(AtomicU64::new(0)), Answers::Here);
        local.send_event(TermEvent::TextAreaSizeRequest(reply));
        assert!(
            matches!(rx.try_recv(), Ok(TermEvent::TextAreaSizeRequest(_))),
            "a terminal we own must still hand the question to its pane"
        );

        // And end to end: the question arriving on a real replica's byte
        // stream is parsed, and nothing about it reaches the pane. The marker
        // follows it in the same write, so once the marker is on the grid the
        // parser has already been past the question.
        let mut pair = attached(20, 5, Some(1));
        let mut events = pair.session.events.take().expect("events");
        pair.host.write_all(b"\x1b[14tdone").expect("host writes");
        let term = pair.session.term.clone();
        assert!(
            within(Duration::from_secs(5), || {
                term.lock().cell(Point::new(Line(0), Column(3))).c == 'e'
            }),
            "the host's bytes never reached the replica"
        );
        while let Ok(event) = events.try_recv() {
            assert!(
                !matches!(event, TermEvent::TextAreaSizeRequest(_)),
                "a replica passed a size question on to its pane"
            );
        }
    }

    #[test]
    fn keystrokes_go_up_the_socket_and_a_resize_does_not() {
        // A size is a fact the owner is told, never one it infers from the byte
        // stream: more than one client may be looking, and a resize mixed into
        // the output is a resize somebody has to guess the end of.
        let mut pair = attached(20, 5, Some(1));
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
    fn authoritative(cols: usize, rows: usize, bytes: &[u8]) -> Term {
        let (tx, _rx) = unbounded();
        let proxy = EventProxy::new(tx, Arc::new(AtomicU64::new(0)), Answers::Here);
        let size = GridSize { cols, rows }.with_cell(8, 16);
        let mut term = Term::new(size, Arc::new(proxy));
        term.advance(bytes);
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
        use crate::vt::Scroll;
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
/// These tests drive the core the way the real read loop does (`vt::pump`
/// calls `term.advance(bytes)` under the lock), but SYNCHRONOUSLY and with NO
/// PTY, NO child shell, and NO sleeps: we build a bare [`crate::vt::Term`],
/// feed raw control bytes, then assert on the resulting grid / mode / emitted
/// events. That makes them fast and CI-stable (a real PTY+shell+sleep loop is
/// the flaky pattern we deliberately avoid).
///
/// They run against whichever core is compiled in, and the suite is run on
/// both: a test that passes on `--features core-alacritty` and fails on the
/// default is a difference between the cores, and each one is either fixed in
/// the rio adapter or recorded in `docs/plans/core-swap-rio/evidence/`.
///
/// Cell access mirrors the production read path in `pane.rs`
/// (`row(Line(y))[Column(x)].{c,flags,fg}`, `term.mode().contains(...)`).
#[cfg(test)]
mod correctness {
    use std::sync::{Arc, Mutex};

    use crate::vt::{
        ClipboardType, Color, Column, Event as TermEvent, Flags, Line, Listener, NamedColor, Point,
        Scroll, Term, TermMode,
    };

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

    impl Listener for Recorder {
        fn send_event(&self, event: TermEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    /// A fresh core (its parser inside) + event recorder at the given size.
    fn harness(cols: usize, rows: usize) -> (Term, Recorder) {
        let size = GridSize { cols, rows }.with_cell(8, 16);
        let rec = Recorder::default();
        let term = Term::new(size, Arc::new(rec.clone()));
        (term, rec)
    }

    /// 80x24 default — the common case.
    fn term80() -> (Term, Recorder) {
        harness(80, 24)
    }

    /// Feed bytes into the core exactly as the real read loop does.
    fn feed(term: &mut Term, bytes: &[u8]) {
        term.advance(bytes);
    }

    /// The `char` at a live-screen cell (row/col, 0-based from the top).
    fn ch(term: &Term, row: i32, col: usize) -> char {
        term.cell(Point::new(Line(row), Column(col))).c
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
        let (mut term, _) = harness(20, 5);
        // 12 numbered lines through a 5-row screen: 7 must scroll into history
        for i in 0..12 {
            feed(&mut term, format!("line{i}\r\n").as_bytes());
        }

        let top = term.topmost_line().0;
        assert!(top < 0, "history must extend above line 0, got {top}");
        // 12 written lines plus the blank row the last newline opens = 13 rows
        // through a 5-row screen, so 8 are displaced into history.
        assert_eq!(top, -8, "history depth");
        assert_eq!(term.history_size(), 8);

        let row_text = |l: i32| -> String {
            (0..20)
                .map(|c| term.cell(Point::new(Line(l), Column(c))).c)
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
    fn flags(term: &Term, row: i32, col: usize) -> Flags {
        term.cell(Point::new(Line(row), Column(col))).flags
    }

    /// Read the live top row as a string, skipping wide-char spacers (the same
    /// thing `pane.rs::live_rows` does for the HUD).
    fn row_text(term: &Term, row: i32) -> String {
        let cells = term.row(Line(row));
        let cols = term.columns();
        let mut s = String::new();
        for c in 0..cols {
            let cell = &cells[Column(c)];
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
        assert_eq!(term.columns(), 80);
        assert_eq!(term.screen_lines(), 24);
    }

    #[test]
    fn plain_ascii_lands_on_the_grid() {
        let (mut term, _) = term80();
        feed(&mut term, b"hello");
        assert_eq!(row_text(&term, 0), "hello");
        assert_eq!(ch(&term, 0, 0), 'h');
        assert_eq!(ch(&term, 0, 4), 'o');
    }

    #[test]
    fn newline_and_carriage_return_move_the_cursor() {
        let (mut term, _) = term80();
        feed(&mut term, b"ab\r\ncd");
        assert_eq!(row_text(&term, 0), "ab");
        assert_eq!(row_text(&term, 1), "cd");
    }

    #[test]
    fn fresh_term_starts_with_no_history() {
        let (term, ..) = term80();
        // total_lines == screen_lines until the screen overflows into history.
        assert_eq!(term.total_lines(), term.screen_lines());
        assert_eq!(term.display_offset(), 0);
    }

    // ----------------------------------------------------------------------
    // (a) Wide-char width — CJK occupies WIDE_CHAR + a WIDE_CHAR_SPACER.
    // ----------------------------------------------------------------------

    #[test]
    fn cjk_sets_wide_char_flag() {
        let (mut term, _) = term80();
        feed(&mut term, "你".as_bytes());
        assert!(flags(&term, 0, 0).contains(Flags::WIDE_CHAR));
        assert_eq!(ch(&term, 0, 0), '你');
    }

    #[test]
    fn cjk_occupies_a_spacer_cell() {
        let (mut term, _) = term80();
        feed(&mut term, "你".as_bytes());
        // The cell immediately right of a wide char is a spacer (no glyph).
        assert!(flags(&term, 0, 1).contains(Flags::WIDE_CHAR_SPACER));
    }

    #[test]
    fn cjk_advances_cursor_by_two_columns() {
        let (mut term, _) = term80();
        feed(&mut term, "你A".as_bytes());
        // 你 at col0(+spacer col1), 'A' lands at col2.
        assert_eq!(ch(&term, 0, 2), 'A');
        assert!(!flags(&term, 0, 2).contains(Flags::WIDE_CHAR));
    }

    #[test]
    fn multiple_cjk_pack_two_columns_each() {
        let (mut term, _) = term80();
        feed(&mut term, "日本語".as_bytes());
        assert_eq!(ch(&term, 0, 0), '日');
        assert!(flags(&term, 0, 1).contains(Flags::WIDE_CHAR_SPACER));
        assert_eq!(ch(&term, 0, 2), '本');
        assert!(flags(&term, 0, 3).contains(Flags::WIDE_CHAR_SPACER));
        assert_eq!(ch(&term, 0, 4), '語');
        assert_eq!(row_text(&term, 0), "日本語");
    }

    #[test]
    fn ascii_is_not_wide() {
        let (mut term, _) = term80();
        feed(&mut term, b"A");
        assert!(!flags(&term, 0, 0).contains(Flags::WIDE_CHAR));
        assert!(!flags(&term, 0, 1).contains(Flags::WIDE_CHAR_SPACER));
    }

    #[test]
    fn wide_char_at_last_column_wraps() {
        // A 2-wide glyph cannot start in the final column of an odd-width grid;
        // alacritty marks the trailing cell LEADING_WIDE_CHAR_SPACER and wraps
        // the glyph to the next line. Use a 3-col grid so col2 is the last.
        let (mut term, _) = harness(3, 4);
        feed(&mut term, b"AB"); // fill col0,col1; cursor at col2 (last)
        feed(&mut term, "你".as_bytes());
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
        let (mut term, _) = term80();
        feed(&mut term, "😀".as_bytes());
        assert!(flags(&term, 0, 0).contains(Flags::WIDE_CHAR));
        assert!(flags(&term, 0, 1).contains(Flags::WIDE_CHAR_SPACER));
        assert_eq!(ch(&term, 0, 0), '😀');
    }

    #[test]
    fn text_after_emoji_lands_two_columns_over() {
        let (mut term, _) = term80();
        feed(&mut term, "😀X".as_bytes());
        assert_eq!(ch(&term, 0, 2), 'X');
    }

    #[test]
    fn zwj_family_emoji_collapses_into_the_base_cell() {
        // 👨‍👩‍👧 is base + ZWJ + emoji + ZWJ + emoji. vte appends the
        // zero-width joiners/combining members onto the base cell as `extra`
        // rather than consuming extra columns: the base cell stays WIDE_CHAR
        // and the visible advance is still 2 columns, with the trailing text
        // landing right after the spacer.
        let (mut term, _) = term80();
        feed(&mut term, "👨‍👩‍👧!".as_bytes());
        assert!(flags(&term, 0, 0).contains(Flags::WIDE_CHAR));
        assert!(flags(&term, 0, 1).contains(Flags::WIDE_CHAR_SPACER));
        // Trailing '!' is the first non-spacer after the cluster.
        let txt = row_text(&term, 0);
        assert!(txt.ends_with('!'), "row was {txt:?}");
    }

    #[test]
    fn combining_accent_does_not_consume_a_column() {
        // 'e' + U+0301 (combining acute) is one grid cell, not two.
        let (mut term, _) = term80();
        feed(&mut term, "e\u{0301}X".as_bytes());
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
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[?1049h");
        assert!(term.mode().contains(TermMode::ALT_SCREEN));
    }

    #[test]
    fn decrst_1049_leaves_alt_screen() {
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[?1049h");
        assert!(term.mode().contains(TermMode::ALT_SCREEN));
        feed(&mut term, b"\x1b[?1049l");
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
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[?1047h");
        assert!(!term.mode().contains(TermMode::ALT_SCREEN));
    }

    #[test]
    fn alt_screen_content_is_isolated_from_primary() {
        let (mut term, _) = term80();
        feed(&mut term, b"primary");
        feed(&mut term, b"\x1b[?1049h");
        // Alt screen starts blank (no primary content bleeds through).
        assert_eq!(row_text(&term, 0), "");
        // 1049 SAVES the cursor, so it is still at the column after "primary".
        // Real full-screen apps home the cursor before drawing; do the same so
        // the written text is deterministic regardless of the saved column.
        feed(&mut term, b"\x1b[H");
        feed(&mut term, b"alt");
        assert_eq!(row_text(&term, 0), "alt");
        feed(&mut term, b"\x1b[?1049l");
        // Primary content restored on exit.
        assert_eq!(row_text(&term, 0), "primary");
    }

    // ----------------------------------------------------------------------
    // (d) Mouse modes — each DECSET sets the matching TermMode flag.
    // ----------------------------------------------------------------------

    #[test]
    fn mouse_click_mode_1000() {
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[?1000h");
        assert!(term.mode().contains(TermMode::MOUSE_REPORT_CLICK));
        feed(&mut term, b"\x1b[?1000l");
        assert!(!term.mode().contains(TermMode::MOUSE_REPORT_CLICK));
    }

    #[test]
    fn mouse_drag_mode_1002() {
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[?1002h");
        assert!(term.mode().contains(TermMode::MOUSE_DRAG));
    }

    #[test]
    fn mouse_motion_mode_1003() {
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[?1003h");
        assert!(term.mode().contains(TermMode::MOUSE_MOTION));
    }

    #[test]
    fn sgr_mouse_mode_1006() {
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[?1006h");
        assert!(term.mode().contains(TermMode::SGR_MOUSE));
        feed(&mut term, b"\x1b[?1006l");
        assert!(!term.mode().contains(TermMode::SGR_MOUSE));
    }

    #[test]
    fn utf8_mouse_mode_1005() {
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[?1005h");
        assert!(term.mode().contains(TermMode::UTF8_MOUSE));
    }

    #[test]
    fn focus_event_mode_1004() {
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[?1004h");
        assert!(term.mode().contains(TermMode::FOCUS_IN_OUT));
        feed(&mut term, b"\x1b[?1004l");
        assert!(!term.mode().contains(TermMode::FOCUS_IN_OUT));
    }

    #[test]
    fn alternate_scroll_mode_1007() {
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[?1007h");
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
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[?2004h");
        assert!(term.mode().contains(TermMode::BRACKETED_PASTE));
        feed(&mut term, b"\x1b[?2004l");
        assert!(!term.mode().contains(TermMode::BRACKETED_PASTE));
    }

    // ----------------------------------------------------------------------
    // Other DEC private modes we read elsewhere in the app.
    // ----------------------------------------------------------------------

    #[test]
    fn app_cursor_keys_mode_1() {
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[?1h");
        assert!(term.mode().contains(TermMode::APP_CURSOR));
        feed(&mut term, b"\x1b[?1l");
        assert!(!term.mode().contains(TermMode::APP_CURSOR));
    }

    #[test]
    fn show_cursor_mode_25() {
        let (mut term, _) = term80();
        // SHOW_CURSOR is set by default; ?25l hides it.
        assert!(term.mode().contains(TermMode::SHOW_CURSOR));
        feed(&mut term, b"\x1b[?25l");
        assert!(!term.mode().contains(TermMode::SHOW_CURSOR));
        feed(&mut term, b"\x1b[?25h");
        assert!(term.mode().contains(TermMode::SHOW_CURSOR));
    }

    #[test]
    fn line_wrap_mode_7() {
        let (mut term, _) = term80();
        // LINE_WRAP (autowrap, DECAWM) is on by default.
        assert!(term.mode().contains(TermMode::LINE_WRAP));
        feed(&mut term, b"\x1b[?7l");
        assert!(!term.mode().contains(TermMode::LINE_WRAP));
    }

    // ----------------------------------------------------------------------
    // (f) Scrollback — overflow grows history; scroll_display moves the offset.
    // ----------------------------------------------------------------------

    #[test]
    fn overflow_grows_history() {
        let (mut term, _) = harness(20, 4); // 4 visible rows
        let before = term.total_lines();
        assert_eq!(before, 4);
        // Print 10 lines: 6 must spill into scrollback history.
        for i in 0..10 {
            feed(&mut term, format!("L{i}\r\n").as_bytes());
        }
        let after = term.total_lines();
        assert!(after > before, "history did not grow: {before} -> {after}");
        // history = total - screen.
        assert_eq!(after - term.screen_lines(), after - 4);
        assert!(
            after - 4 >= 6,
            "expected >=6 history rows, got {}",
            after - 4
        );
    }

    #[test]
    fn display_offset_zero_at_bottom() {
        let (mut term, _) = harness(20, 4);
        for i in 0..10 {
            feed(&mut term, format!("L{i}\r\n").as_bytes());
        }
        // Not scrolled back yet — viewport pinned to the live bottom.
        assert_eq!(term.display_offset(), 0);
    }

    #[test]
    fn scroll_display_moves_offset_into_history() {
        let (mut term, _) = harness(20, 4);
        for i in 0..10 {
            feed(&mut term, format!("L{i}\r\n").as_bytes());
        }
        term.scroll_display(Scroll::Delta(3));
        assert_eq!(term.display_offset(), 3);
        term.scroll_display(Scroll::Top);
        assert!(term.display_offset() > 0);
        term.scroll_display(Scroll::Bottom);
        assert_eq!(term.display_offset(), 0);
    }

    #[test]
    fn scrolled_back_content_is_a_prior_line() {
        let (mut term, _) = harness(20, 6);
        for i in 0..20 {
            feed(&mut term, format!("line{i}\r\n").as_bytes());
        }
        term.scroll_display(Scroll::Top);
        // The very top of history is the oldest retained line.
        let top = row_text(&term, 0);
        assert!(top.starts_with("line"), "top of scrollback was {top:?}");
    }

    // ----------------------------------------------------------------------
    // (g) ANSI 16-colour cell fg.
    // ----------------------------------------------------------------------

    fn fg(term: &Term, row: i32, col: usize) -> Color {
        term.cell(Point::new(Line(row), Column(col))).fg
    }

    #[test]
    fn sgr_31_sets_named_red_fg() {
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[31mR");
        assert_eq!(fg(&term, 0, 0), Color::Named(NamedColor::Red));
    }

    #[test]
    fn sgr_32_sets_named_green_fg() {
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[32mG");
        assert_eq!(fg(&term, 0, 0), Color::Named(NamedColor::Green));
    }

    #[test]
    fn sgr_34_sets_named_blue_fg() {
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[34mB");
        assert_eq!(fg(&term, 0, 0), Color::Named(NamedColor::Blue));
    }

    #[test]
    fn sgr_91_sets_bright_red_fg() {
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[91mr");
        assert_eq!(fg(&term, 0, 0), Color::Named(NamedColor::BrightRed));
    }

    #[test]
    fn sgr_0_resets_fg_to_default() {
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[31mR\x1b[0mD");
        assert_eq!(fg(&term, 0, 0), Color::Named(NamedColor::Red));
        assert_eq!(fg(&term, 0, 1), Color::Named(NamedColor::Foreground));
    }

    #[test]
    fn sgr_256_indexed_fg() {
        let (mut term, _) = term80();
        // 38;5;208 = indexed orange.
        feed(&mut term, b"\x1b[38;5;208mX");
        assert_eq!(fg(&term, 0, 0), Color::Indexed(208));
    }

    /// A blank takes the pen's background and nothing else, however it was
    /// made. xterm's rule, and alacritty's: the green, the inverse and the
    /// underline reach the characters written with the pen, never the blanks
    /// an erase, a scroll, an inserted line or a cleared screen leaves. On
    /// rio-vt the last three used to carry the whole pen, so a line that
    /// scrolled in under inverse was drawn inverted from edge to edge — and an
    /// erase read its red back as a palette index rather than as red.
    #[test]
    fn a_blank_takes_the_pens_background_and_nothing_else() {
        const PEN: &str = "\x1b[4;7;32;41m";
        let cases = [
            (
                "an erase",
                6,
                2,
                format!("abcdef\x1b[1;3H{PEN}\x1b[K"),
                (0, 4),
            ),
            ("a scroll", 6, 2, format!("{PEN}a\r\nb\r\nc"), (1, 3)),
            (
                "an inserted line",
                6,
                3,
                format!("x\r\ny\x1b[1;1H{PEN}\x1b[1L"),
                (0, 2),
            ),
            (
                "a cleared screen",
                6,
                2,
                format!("abcdef{PEN}\x1b[2J"),
                (1, 2),
            ),
        ];
        for (how, cols, rows, bytes, (row, col)) in cases {
            let (mut term, _) = harness(cols, rows);
            feed(&mut term, bytes.as_bytes());
            let blank = term.cell(Point::new(Line(row), Column(col)));
            assert_eq!(blank.c, ' ', "{how}");
            assert_eq!(
                blank.bg,
                Color::Named(NamedColor::Red),
                "{how}: the background"
            );
            assert_eq!(
                blank.fg,
                Color::Named(NamedColor::Foreground),
                "{how}: no pen colour"
            );
            assert!(
                blank.flags.is_empty(),
                "{how}: no pen flags, {:?}",
                blank.flags
            );
        }
    }

    #[test]
    fn sgr_truecolor_fg() {
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[38;2;10;20;30mX");
        match fg(&term, 0, 0) {
            Color::Spec(rgb) => {
                assert_eq!((rgb.r, rgb.g, rgb.b), (10, 20, 30));
            }
            other => panic!("expected truecolor spec, got {other:?}"),
        }
    }

    #[test]
    fn sgr_bold_sets_bold_flag() {
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[1mX");
        assert!(flags(&term, 0, 0).contains(Flags::BOLD));
    }

    #[test]
    fn sgr_underline_sets_underline_flag() {
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[4mX");
        assert!(flags(&term, 0, 0).contains(Flags::UNDERLINE));
    }

    #[test]
    fn sgr_inverse_sets_inverse_flag() {
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b[7mX");
        assert!(flags(&term, 0, 0).contains(Flags::INVERSE));
    }

    // ----------------------------------------------------------------------
    // (h) OSC 8 hyperlinks — uri (and id) attach to the painted cells.
    // ----------------------------------------------------------------------

    #[test]
    fn osc8_hyperlink_attaches_uri_to_cells() {
        let (mut term, _) = term80();
        // OSC 8 ; ; URI ST  text  OSC 8 ; ; ST   (BEL-terminated form)
        feed(
            &mut term,
            b"\x1b]8;;https://example.com\x07link\x1b]8;;\x07",
        );
        let hl = term
            .hyperlink(Point::new(Line(0), Column(0)))
            .expect("cell 0 should carry a hyperlink");
        assert_eq!(hl.uri(), "https://example.com");
        // 'l' of "link" is the painted glyph.
        assert_eq!(ch(&term, 0, 0), 'l');
    }

    #[test]
    fn osc8_hyperlink_with_id_param() {
        let (mut term, _) = term80();
        feed(
            &mut term,
            b"\x1b]8;id=42;https://rust-lang.org\x07rust\x1b]8;;\x07",
        );
        let hl = term
            .hyperlink(Point::new(Line(0), Column(0)))
            .expect("hyperlink present");
        assert_eq!(hl.uri(), "https://rust-lang.org");
        // An explicit id is preserved as the program wrote it.
        assert_eq!(hl.id(), "42");
    }

    #[test]
    fn osc8_close_stops_attaching_links() {
        let (mut term, _) = term80();
        feed(&mut term, b"\x1b]8;;https://a.test\x07A\x1b]8;;\x07B");
        assert!(term.hyperlink(Point::new(Line(0), Column(0))).is_some());
        // 'B' is printed after the link was closed — no hyperlink.
        assert!(term.hyperlink(Point::new(Line(0), Column(1))).is_none());
    }

    #[test]
    fn cells_without_osc8_have_no_hyperlink() {
        let (mut term, _) = term80();
        feed(&mut term, b"plain");
        assert!(term.hyperlink(Point::new(Line(0), Column(0))).is_none());
    }

    // ----------------------------------------------------------------------
    // (i) OSC 52 — clipboard store. Default Config osc52 == OnlyCopy, so a
    //     copy ('c') sequence emits Event::ClipboardStore with the decoded text.
    // ----------------------------------------------------------------------

    #[test]
    fn osc52_copy_emits_clipboard_store() {
        let (mut term, rec) = term80();
        // base64("hi") == "aGk=".
        feed(&mut term, b"\x1b]52;c;aGk=\x07");
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
        let (mut term, rec) = term80();
        // base64("terminal-delight") == "dGVybWluYWwtZGVsaWdodA==".
        feed(&mut term, b"\x1b]52;c;dGVybWluYWwtZGVsaWdodA==\x07");
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
        let (mut term, rec) = term80();
        feed(&mut term, b"\x1b]52;c;not-valid-base64!!!\x07");
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
        let (mut term, rec) = term80();
        feed(&mut term, b"\x1b]0;my-title\x07");
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
        let (mut term, _) = term80();
        // CUP row3 col5 (1-based) then write — lands at grid (2,4).
        feed(&mut term, b"\x1b[3;5fZ");
        assert_eq!(ch(&term, 2, 4), 'Z');
    }

    #[test]
    fn ed_clears_the_screen() {
        let (mut term, _) = term80();
        feed(&mut term, b"dirty\r\nmore");
        feed(&mut term, b"\x1b[2J");
        assert_eq!(row_text(&term, 0), "");
        assert_eq!(row_text(&term, 1), "");
    }

    #[test]
    fn el_clears_to_end_of_line() {
        let (mut term, _) = term80();
        feed(&mut term, b"keepXXXX");
        // Move cursor back to col4 and erase to EOL.
        feed(&mut term, b"\x1b[1;5H\x1b[K");
        assert_eq!(row_text(&term, 0), "keep");
    }

    #[test]
    fn backspace_moves_left_without_erasing() {
        let (mut term, _) = term80();
        feed(&mut term, b"ab\x08c");
        // 'c' overwrites 'b'.
        assert_eq!(row_text(&term, 0), "ac");
    }

    #[test]
    fn tab_advances_to_next_stop() {
        let (mut term, _) = term80();
        feed(&mut term, b"a\tb");
        assert_eq!(ch(&term, 0, 0), 'a');
        // Default tab stops every 8 columns → 'b' at col8.
        assert_eq!(ch(&term, 0, 8), 'b');
    }

    #[test]
    fn autowrap_pushes_overflow_to_next_row() {
        let (mut term, _) = harness(4, 4);
        feed(&mut term, b"abcdef");
        assert_eq!(row_text(&term, 0), "abcd");
        assert_eq!(row_text(&term, 1), "ef");
    }
}
