//! The host as a real process, spoken to over a real socket.
//!
//! The unit tests drive the host's own types directly, which proves the logic
//! and skips the layer a client actually meets: a binary that binds a socket,
//! decides what each connection is from its first line, and answers. This runs
//! the shipped binary and talks to it the way a window will.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Session {
    child: Child,
    socket: PathBuf,
    #[allow(dead_code)]
    runtime: PathBuf,
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.runtime);
    }
}

/// Start a real session host in its own runtime directory, running `cat` in
/// every pane so the child is instant, silent and echoes exactly what it gets.
fn start_host(name: &str) -> Session {
    let runtime = std::env::temp_dir().join(format!("td-host-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&runtime);
    std::fs::create_dir_all(&runtime).expect("runtime dir");

    let child = Command::new(env!("CARGO_BIN_EXE_terminal-delight"))
        .args(["serve", "--session", name])
        .env("XDG_RUNTIME_DIR", &runtime)
        // A host now claims its session with a lock kept beside the session
        // files, so a test that did not say where those live would arbitrate
        // against — and litter — the real one.
        .env("XDG_CONFIG_HOME", runtime.join("config"))
        .env("TD_HOST_SHELL", "/bin/cat")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the host");

    let socket = runtime
        .join("terminal-delight")
        .join(format!("session-{name}.sock"));
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && !socket.exists() {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(socket.exists(), "the host never bound {}", socket.display());
    Session {
        child,
        socket,
        runtime,
    }
}

/// A control connection: one line of JSON out, one line of JSON back.
struct Control {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Control {
    fn open(session: &Session) -> Self {
        let writer = UnixStream::connect(&session.socket).expect("connect");
        writer
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let reader = BufReader::new(writer.try_clone().unwrap());
        Self { reader, writer }
    }

    fn ask(&mut self, line: &str) -> serde_json::Value {
        self.writer.write_all(line.as_bytes()).expect("write");
        self.writer.write_all(b"\n").expect("newline");
        let mut back = String::new();
        self.reader.read_line(&mut back).expect("read a reply");
        serde_json::from_str(&back).unwrap_or_else(|e| panic!("unreadable reply {back:?}: {e}"))
    }
}

#[test]
fn a_window_can_start_a_terminal_take_it_over_and_type_into_it() {
    // The whole slice, end to end, as a client experiences it.
    let session = start_host("takeover");
    let mut control = Control::open(&session);

    let hello = control.ask(r#"{"verb":"hello","proto":1,"kind":"window"}"#);
    assert_eq!(hello["reply"], "hello", "{hello}");
    assert_eq!(hello["session"], "takeover");
    assert_eq!(hello["panes"], 0);
    assert_eq!(hello["attended"], false, "nobody is watching a fresh host");

    let spawned = control.ask(
        r#"{"verb":"spawn-pane","geom":{"cols":40,"rows":8,"cell_width":8,"cell_height":16}}"#,
    );
    let pane = spawned["outcome"]["ok"]["pane"]
        .as_u64()
        .unwrap_or_else(|| panic!("no pane in {spawned}"));
    let shell_pid = spawned["outcome"]["ok"]["shell_pid"].as_u64().expect("pid");
    assert!(shell_pid > 1, "a pane is a real process: {spawned}");

    // Type into it before anyone is attached — the work starts without a
    // window, which is the point of the whole feature.
    let mut early = UnixStream::connect(&session.socket).expect("stream connect");
    early
        .write_all(format!("stream {pane}\n").as_bytes())
        .expect("greet");
    early
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    early.write_all(b"printed before\n").expect("type");

    // Wait until that line has actually come back, rather than until the
    // attachment exists — the two are seconds apart and only the first means
    // the terminal has the content.
    //
    // It is also the exact moment the fence's promise becomes testable: bytes
    // are copied to a client and parsed into the grid inside one reader cycle,
    // and an attach cannot interleave with a cycle. So once any client has
    // seen a byte, every later snapshot must contain it.
    let first_saw = read_until(&mut early, Duration::from_secs(10), |text| {
        text.contains("printed before")
    });
    assert!(
        first_saw.contains("printed before"),
        "the first window never saw its own echo: {first_saw:?}"
    );

    // A second window arrives. It missed everything, and must not stay behind.
    let mut window = UnixStream::connect(&session.socket).expect("stream connect");
    window
        .write_all(format!("stream {pane}\n").as_bytes())
        .expect("greet");
    window
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();

    let seen = read_until(&mut window, Duration::from_secs(10), |text| {
        text.contains("printed before")
    });
    let (snapshot, live) = split_snapshot(&seen);
    assert!(
        snapshot.contains("printed before"),
        "the snapshot did not carry what the second window missed: {snapshot:?}"
    );
    assert!(
        !live.contains("printed before"),
        "output already in the snapshot was sent live as well: {live:?}"
    );

    // And it is live: what it types comes back to it.
    window.write_all(b"typed after\n").expect("type");
    let echoed = read_until(&mut window, Duration::from_secs(10), |text| {
        text.contains("typed after")
    });
    assert!(
        echoed.contains("typed after"),
        "the attached window is not live: {echoed:?}"
    );

    // Closing is a verb, and it kills. Asked cold, on a fresh connection that
    // never introduced itself — which is exactly what a script does, and what
    // the wire is meant to allow.
    let mut control = Control::open(&session);
    let closed = control.ask(&format!(r#"{{"verb":"close-pane","pane":{pane}}}"#));
    assert_eq!(closed["outcome"]["ok"]["signalled"], true, "{closed}");
    assert!(
        wait_for(Duration::from_secs(10), || {
            !PathBuf::from(format!("/proc/{shell_pid}")).exists()
        }),
        "the child outlived the close"
    );

    // And the host stops when told to, rather than when someone next knocks —
    // which is why shutdown knocks on its own door on the way out.
    //
    // Asked of the child directly rather than of /proc: a process that has
    // exited but not yet been waited for is a zombie, and its /proc entry is
    // still there. "Still listed" and "still running" are not the same
    // question, and only one of them is this one.
    let mut control = Control::open(&session);
    let bye = control.ask(r#"{"verb":"shutdown"}"#);
    assert_eq!(bye["reply"], "shutting-down", "{bye}");
    let mut session = session;
    assert!(
        wait_for(Duration::from_secs(10), || {
            matches!(session.child.try_wait(), Ok(Some(_)))
        }),
        "the host ignored shutdown and is still running"
    );
}

#[test]
fn a_terminal_outlives_the_window_that_was_watching_it() {
    // The product promise, over a real socket: the window goes, the work does
    // not, and the next window that attaches is shown what it missed.
    let session = start_host("outlives");
    let mut control = Control::open(&session);
    control.ask(r#"{"verb":"hello","proto":1,"kind":"window"}"#);
    let spawned = control.ask(
        r#"{"verb":"spawn-pane","geom":{"cols":40,"rows":8,"cell_width":8,"cell_height":16}}"#,
    );
    let pane = spawned["outcome"]["ok"]["pane"].as_u64().expect("pane");
    let shell_pid = spawned["outcome"]["ok"]["shell_pid"].as_u64().expect("pid");

    {
        let mut window = UnixStream::connect(&session.socket).expect("connect");
        window
            .write_all(format!("stream {pane}\n").as_bytes())
            .expect("greet");
        window
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        window.write_all(b"work in progress\n").expect("type");
        read_until(&mut window, Duration::from_secs(10), |t| {
            t.contains("work in progress")
        });
        // The window dies here, abruptly, exactly as a crash would.
    }

    assert!(
        PathBuf::from(format!("/proc/{shell_pid}")).exists(),
        "the terminal died with its window — this is the bug the feature exists for"
    );

    // A new window attaches and is shown the work it never saw.
    let mut reopened = UnixStream::connect(&session.socket).expect("connect");
    reopened
        .write_all(format!("stream {pane}\n").as_bytes())
        .expect("greet");
    reopened
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let restored = read_until(&mut reopened, Duration::from_secs(10), |t| {
        t.contains("work in progress")
    });
    assert!(
        restored.contains("work in progress"),
        "the relaunched window was not shown what survived: {restored:?}"
    );
}

#[test]
fn asking_the_host_questions_never_takes_a_pane_away_from_a_window() {
    // Resolving a session probes every candidate host. If asking could steal,
    // every launch would detach the window already running.
    let session = start_host("probing");
    let mut control = Control::open(&session);
    control.ask(r#"{"verb":"hello","proto":1,"kind":"window"}"#);
    let spawned = control.ask(
        r#"{"verb":"spawn-pane","geom":{"cols":40,"rows":8,"cell_width":8,"cell_height":16}}"#,
    );
    let pane = spawned["outcome"]["ok"]["pane"].as_u64().expect("pane");

    let mut window = UnixStream::connect(&session.socket).expect("connect");
    window
        .write_all(format!("stream {pane}\n").as_bytes())
        .expect("greet");
    window
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    window.write_all(b"mine\n").expect("type");
    read_until(&mut window, Duration::from_secs(10), |t| t.contains("mine"));

    for _ in 0..5 {
        let mut probe = Control::open(&session);
        probe.ask(r#"{"verb":"hello","proto":1,"kind":"tool"}"#);
        probe.ask(r#"{"verb":"list-panes"}"#);
    }

    window.write_all(b"still mine\n").expect("type again");
    let after = read_until(&mut window, Duration::from_secs(10), |t| {
        t.contains("still mine")
    });
    assert!(
        after.contains("still mine"),
        "a probe cost the window its pane: {after:?}"
    );
}

#[test]
fn a_version_it_cannot_speak_is_refused_by_name() {
    let session = start_host("versions");
    let mut control = Control::open(&session);
    let refused = control.ask(r#"{"verb":"hello","proto":99,"kind":"window"}"#);
    assert_eq!(refused["reply"], "error", "{refused}");
    let msg = refused["msg"].as_str().unwrap_or_default();
    assert!(msg.contains("99") && msg.contains('1'), "{msg}");

    // And nonsense gets an answer rather than silence, which is the failure
    // this whole feature started from.
    let mut control = Control::open(&session);
    assert_eq!(control.ask("{not json}")["reply"], "error");
}

#[test]
fn a_connection_that_never_negotiated_is_served_like_any_other() {
    // Over a real socket, because the statelessness is a property of the real
    // socket rather than of a type: the host answers each line on its own
    // merits, and what decides whether a peer may speak at all is the uid
    // check taken before the first byte is parsed. Every connection here is
    // same-uid, which is the whole authorisation story, and the condition Gate
    // 1 actually set — authority is a checkable property of the connection,
    // never a claim inside a payload.
    //
    // Written the other way round on 2026-09-10 and reversed the same day: a
    // hello gate would leave a refused peer unable to ask an old host to stand
    // down, which is the only approved way through a protocol bump.
    let session = start_host("cold");
    let mut control = Control::open(&session);
    control.ask(r#"{"verb":"hello","proto":1,"kind":"window"}"#);
    let spawned = control.ask(
        r#"{"verb":"spawn-pane","geom":{"cols":40,"rows":8,"cell_width":8,"cell_height":16}}"#,
    );
    let pane = spawned["outcome"]["ok"]["pane"].as_u64().expect("pane");
    let shell_pid = spawned["outcome"]["ok"]["shell_pid"].as_u64().expect("pid");

    // A connection that has said nothing about itself asks for a terminal to
    // be hung up, and gets what it asked for.
    let mut cold = Control::open(&session);
    let closed = cold.ask(&format!(r#"{{"verb":"close-pane","pane":{pane}}}"#));
    assert_eq!(
        closed["outcome"]["ok"]["signalled"], true,
        "a connection that skipped the greeting was refused: {closed}"
    );
    assert!(
        wait_for(Duration::from_secs(10), || {
            !PathBuf::from(format!("/proc/{shell_pid}")).exists()
        }),
        "the child outlived a close asked for without a greeting"
    );

    // And so does the verb the version-break repair is made of.
    let mut cold = Control::open(&session);
    let bye = cold.ask(r#"{"verb":"shutdown"}"#);
    assert_eq!(bye["reply"], "shutting-down", "{bye}");
    let mut session = session;
    assert!(
        wait_for(Duration::from_secs(10), || {
            matches!(session.child.try_wait(), Ok(Some(_)))
        }),
        "a host asked to stand down by an un-negotiated peer stayed up"
    );
}

#[test]
fn a_second_host_for_a_live_session_refuses_instead_of_taking_its_socket() {
    // #335, over two real processes. `serve` used to unlink whatever socket
    // file it found and bind its own; a socket file left by a dead host and one
    // belonging to a live host look identical, so the second host took the
    // first's front door and the first was left running — holding its
    // terminals, with no name by which anything could reach them again.
    let session = start_host("clash");
    let mut control = Control::open(&session);
    control.ask(r#"{"verb":"hello","proto":1,"kind":"window"}"#);
    let spawned = control.ask(
        r#"{"verb":"spawn-pane","geom":{"cols":40,"rows":8,"cell_width":8,"cell_height":16}}"#,
    );
    let pane = spawned["outcome"]["ok"]["pane"].as_u64().expect("pane");
    let shell_pid = spawned["outcome"]["ok"]["shell_pid"].as_u64().expect("pid");
    let front_door = inode_of(&session.socket);

    // A second `serve`, exactly as a second window launching at the same moment
    // would start one.
    let mut second = Command::new(env!("CARGO_BIN_EXE_terminal-delight"))
        .args(["serve", "--session", "clash"])
        .env("XDG_RUNTIME_DIR", &session.runtime)
        .env("XDG_CONFIG_HOME", session.runtime.join("config"))
        .env("TD_HOST_SHELL", "/bin/cat")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn a second host");

    // It must decline and go, rather than settle in. Asserted before anything
    // else, because a second host that keeps running is the bug itself and
    // every assertion after this one would be measuring its aftermath.
    let left = wait_for(Duration::from_secs(10), || {
        matches!(second.try_wait(), Ok(Some(_)))
    });
    if !left {
        let _ = second.kill();
        let _ = second.wait();
        panic!("a second host for a live session started and stayed running");
    }
    let status = second.wait().expect("the second host's exit");
    assert_eq!(
        status.code(),
        Some(0),
        "a caller that wanted a host for this session has one, so this is success"
    );

    assert_eq!(
        inode_of(&session.socket),
        front_door,
        "the second host replaced the first host's socket"
    );

    // The first host is still there, still owns its pane, and still answers to
    // the name it was reachable by.
    let mut control = Control::open(&session);
    let listed = control.ask(r#"{"verb":"list-panes"}"#);
    assert_eq!(
        listed["panes"][0]["pane"], pane,
        "the session socket no longer reaches the host holding the work: {listed}"
    );
    assert!(
        PathBuf::from(format!("/proc/{shell_pid}")).exists(),
        "the pane the first host was holding is gone"
    );
}

fn inode_of(path: &PathBuf) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path)
        .unwrap_or_else(|e| panic!("no {}: {e}", path.display()))
        .ino()
}

#[test]
fn the_host_writes_the_session_file_and_fills_in_where_the_panes_are() {
    // Slice 4's half of #337: the host is the single writer of the session
    // file. It holds the pseudoterminals, so it is the only process that can
    // say where a pane actually is — a window saving a layout can only write
    // down what it was told at spawn.
    let session = start_host("writer");
    let sessions = session
        .runtime
        .join("config")
        .join("terminal-delight")
        .join("sessions");
    let state = sessions.join("writer.toml");
    assert!(!state.exists(), "nothing has been saved yet");

    let mut control = Control::open(&session);
    control.ask(r#"{"verb":"hello","proto":1,"kind":"window"}"#);
    let spawned = control.ask(
        r#"{"verb":"spawn-pane","cwd":"/usr/share","geom":{"cols":40,"rows":8,"cell_width":8,"cell_height":16}}"#,
    );
    let pane = spawned["outcome"]["ok"]["pane"].as_u64().expect("pane");

    // A layout naming that pane, carrying a directory that is already wrong —
    // which is what an attached window writes, since it has no pseudoterminal
    // to read one from.
    let body = format!(
        "active = 0\n\n[[tabs]]\nname = \"the tab\"\n\n[tabs.node.Leaf]\npane_id = {pane}\ncwd = \"/nowhere-in-particular\"\n"
    );
    let save = serde_json::json!({
        "verb": "save",
        "schema": 1,
        "body": body,
        "allow_shrink": false,
    });
    let saved = control.ask(&save.to_string());
    assert_eq!(
        saved["outcome"]["ok"]["written"]["leaves"], 1,
        "the host did not write the layout it was handed: {saved}"
    );

    let written = std::fs::read_to_string(&state)
        .unwrap_or_else(|e| panic!("the host wrote no session file at {}: {e}", state.display()));
    assert!(
        written.contains("/usr/share"),
        "the host wrote the client's stale directory instead of reading the pane's: {written}"
    );
    assert!(
        !written.contains("/nowhere-in-particular"),
        "the stale directory survived the merge: {written}"
    );
    // And the parts the host does not understand came through untouched.
    assert!(
        written.contains("the tab"),
        "the host dropped a field it does not read: {written}"
    );
}

#[test]
fn a_session_asked_twice_for_one_agent_runs_it_once() {
    // #339, over a real socket. Two windows naming one session both plan
    // against a layout that names no panes, and both ask for the same
    // conversation — which without this is two agents on one transcript, both
    // billing. The check has to happen where the spawning does.
    let session = start_host("oneagent");
    let mut control = Control::open(&session);
    control.ask(r#"{"verb":"hello","proto":1,"kind":"window"}"#);

    let recipe = "claude --resume 48be90b8";
    let spawn = format!(
        r#"{{"verb":"spawn-pane","cwd":"/tmp","resume":"{recipe}","geom":{{"cols":40,"rows":8,"cell_width":8,"cell_height":16}}}}"#
    );

    let first = control.ask(&spawn);
    assert_eq!(
        first["started"], true,
        "the first ask starts a terminal: {first}"
    );
    let pane = first["outcome"]["ok"]["pane"].as_u64().expect("pane");

    // A second window, arriving before the first has saved anything.
    let mut other = Control::open(&session);
    other.ask(r#"{"verb":"hello","proto":1,"kind":"window"}"#);
    let second = other.ask(&spawn);
    assert_eq!(
        second["started"], false,
        "a second terminal was started for a conversation already running in one: {second}"
    );
    assert_eq!(
        second["outcome"]["ok"]["pane"].as_u64(),
        Some(pane),
        "the refusal must name the pane already running it: {second}"
    );

    let listed = control.ask(r#"{"verb":"list-panes"}"#);
    assert_eq!(
        listed["panes"].as_array().map(|panes| panes.len()),
        Some(1),
        "the session is running the same agent twice: {listed}"
    );
}

/// Split what a client received into the snapshot and everything after it.
///
/// A snapshot ends by restoring the modes, and line wrap is the last one
/// written, so its final bytes are that mode change. Ordinary terminal output
/// does not contain it.
fn split_snapshot(text: &str) -> (&str, &str) {
    const TAIL: &str = "\x1b[?7";
    match text.rfind(TAIL) {
        Some(at) => text.split_at((at + TAIL.len() + 1).min(text.len())),
        None => ("", text),
    }
}

fn wait_for(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    done()
}

/// Read from a stream until the accumulated text satisfies `done`, or time is
/// up. Returns everything read either way, so a failure can show what arrived.
fn read_until(
    stream: &mut UnixStream,
    limit: Duration,
    mut done: impl FnMut(&str) -> bool,
) -> String {
    let deadline = Instant::now() + limit;
    let mut seen = Vec::new();
    let mut buf = [0u8; 8192];
    while Instant::now() < deadline {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(read) => {
                seen.extend_from_slice(&buf[..read]);
                if done(&String::from_utf8_lossy(&seen)) {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&seen).into_owned()
}
