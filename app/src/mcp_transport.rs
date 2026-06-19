//! The live stdio JSON-RPC transport for the read-only MCP server.
//!
//! The protocol ([`crate::mcp`]) is pure; the pane snapshot lives on gpui's
//! main thread; a JSON-RPC server must block on stdin; and the push feed must
//! write to stdout from a *different* thread than the request loop. So the
//! transport is four cooperating pieces around one channel each:
//!
//!   • **reader thread** — blocks on stdin. Static methods (`initialize`,
//!     `tools/list`, `ping`, `logging/setLevel`) are answered instantly with no
//!     snapshot, so the handshake never waits on the GUI. `tools/call` asks the
//!     ticker for a fresh snapshot (5 s budget) and, on timeout, returns a
//!     JSON-RPC error rather than dropping the request (no client hang).
//!   • **ticker** (gpui main thread, leak-safe like the jiggle/checkpoint
//!     clocks) — serves per-request snapshots where `&App` is available, and
//!     every ~1 s also pushes a snapshot to the notifier.
//!   • **notifier thread** — tails exposed agent transcripts off the main
//!     thread, diffs them through [`mcp::Watcher`], and emits
//!     `notifications/message` so an orchestrator reacts without polling.
//!   • **writer thread** — the *single* owner of stdout; both the reader
//!     (responses) and the notifier (notifications) funnel lines through it, so
//!     the two streams never interleave-corrupt.
//!
//! Opt-in via `TD_MCP`; process-wide singleton. It never writes to a PTY.

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use gpui::Context;

use crate::{mcp, mcp_tail, session, Workspace};

/// What the reader thread asks the ticker (gpui main thread) to do. Both
/// variants carry a one-shot channel the ticker answers on, so the reader can
/// block with a budget and never hang. Reads and writes share the one ticker so
/// every touch of pane state happens on the main thread — the reader never
/// mutates anything itself.
enum UiReq {
    /// Produce a fresh pane snapshot (every `tools/call` needs one to gate on
    /// policy and read identities/grades).
    Snapshot(mpsc::Sender<mcp::Snapshot>),
    /// Apply a parsed `set_pane_config` batch and report the per-target outcome.
    Apply(Vec<mcp::ConfigUpdate>, mpsc::Sender<Vec<mcp::ApplyOutcome>>),
    /// `grep`: search every exposed pane's scrollback for an exact substring
    /// (needle, lines-per-pane cap) and report the per-pane matches.
    Search(String, usize, mpsc::Sender<Vec<mcp::PaneMatches>>),
}

/// Build a uniform refusal for a whole batch (used when the UI is gone/wedged).
fn refuse_all(updates: &[mcp::ConfigUpdate], why: &str) -> Vec<mcp::ApplyOutcome> {
    updates
        .iter()
        .map(|(t, _)| (t.clone(), Err(why.to_string())))
        .collect()
}

/// How long a `tools/call` waits for the UI to produce a snapshot before the
/// client gets a "not ready" error instead of hanging.
const SNAPSHOT_BUDGET: Duration = Duration::from_secs(5);
/// Ticks (~40 ms each) between push-snapshots handed to the notifier (~1 s).
const PUSH_EVERY_TICKS: u32 = 25;
/// How many recent events to tail per pane when diffing the push feed.
const PUSH_TAIL: usize = 50;

/// Start the stdio MCP server once per process. Call from `Workspace::build`.
/// No-op on a second call.
pub fn start(cx: &mut Context<Workspace>) {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    let (req_tx, req_rx) = mpsc::channel::<UiReq>(); // reader → ticker (per request)
    let (snap_tx, snap_rx) = mpsc::channel::<mcp::Snapshot>(); // ticker → notifier (periodic)
    let (out_tx, out_rx) = mpsc::channel::<String>(); // reader + notifier → writer

    // Shared, lock-free control bits: the desired log severity (gates the push
    // feed) and whether the client has handshaken (no notifications before).
    let level = Arc::new(AtomicU8::new(mcp::log_severity("info")));
    let active = Arc::new(AtomicBool::new(false));

    // Writer: the single owner of stdout.
    let _ = thread::Builder::new()
        .name("td-mcp-out".into())
        .spawn(move || {
            let mut out = std::io::stdout();
            while let Ok(line) = out_rx.recv() {
                if writeln!(out, "{line}").is_err() || out.flush().is_err() {
                    break;
                }
            }
        });

    // Notifier: tails transcripts off-thread and emits the push feed.
    {
        let out_tx = out_tx.clone();
        let level = Arc::clone(&level);
        let active = Arc::clone(&active);
        let home = session::home_dir();
        let _ = thread::Builder::new()
            .name("td-mcp-notify".into())
            .spawn(move || notify_loop(snap_rx, out_tx, level, active, home));
    }

    // Reader: blocks on stdin, routes each request.
    {
        let out_tx = out_tx.clone();
        let level = Arc::clone(&level);
        let active = Arc::clone(&active);
        let _ = thread::Builder::new()
            .name("td-mcp".into())
            .spawn(move || serve_stdio(req_tx, out_tx, level, active));
    }

    // Ticker: services snapshot requests on the main thread + feeds the notifier.
    cx.spawn(async move |this, cx| {
        let mut since_push: u32 = 0;
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(40))
                .await;

            // Drain any pending per-request snapshot asks.
            let mut alive = true;
            loop {
                match req_rx.try_recv() {
                    Ok(UiReq::Snapshot(reply)) => match this.update(cx, snapshot_of) {
                        Ok(s) => {
                            let _ = reply.send(s);
                        }
                        Err(_) => {
                            let _ = reply.send(mcp::Snapshot::empty());
                            alive = false;
                        }
                    },
                    Ok(UiReq::Apply(updates, reply)) => {
                        match this.update(cx, |ws, cx| ws.apply_mcp_config(&updates, cx)) {
                            Ok(out) => {
                                let _ = reply.send(out);
                            }
                            Err(_) => {
                                let _ = reply
                                    .send(refuse_all(&updates, "terminal-delight UI not ready"));
                                alive = false;
                            }
                        }
                    }
                    Ok(UiReq::Search(needle, cap, reply)) => {
                        match this.update(cx, |ws, cx| ws.mcp_search(&needle, cap, cx)) {
                            Ok(hits) => {
                                let _ = reply.send(hits);
                            }
                            Err(_) => {
                                let _ = reply.send(Vec::new());
                                alive = false;
                            }
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return, // reader gone
                }
            }

            // Periodically feed the notifier — only while the feed is live, so
            // we don't wake it for nothing.
            since_push += 1;
            if since_push >= PUSH_EVERY_TICKS {
                since_push = 0;
                if let Ok(snap) = this.update(cx, snapshot_of) {
                    if snap.config.enabled && snap.config.events {
                        let _ = snap_tx.send(snap);
                    }
                } else {
                    alive = false;
                }
            }

            if !alive || this.update(cx, |_, _| ()).is_err() {
                break;
            }
        }
    })
    .detach();
}

/// Build a fresh snapshot from the live workspace (runs on the main thread).
fn snapshot_of(ws: &mut Workspace, cx: &mut Context<Workspace>) -> mcp::Snapshot {
    mcp::Snapshot {
        config: ws.mcp.clone(),
        panes: ws.mcp_snapshot(cx),
        outer_grade: ws.mcp_outer_grade(cx),
    }
}

/// The reader loop: one request line in, one response line out (via the writer).
fn serve_stdio(
    req_tx: mpsc::Sender<UiReq>,
    out_tx: mpsc::Sender<String>,
    level: Arc<AtomicU8>,
    active: Arc<AtomicBool>,
) {
    let stdin = std::io::stdin();
    let mut line = String::new();
    loop {
        line.clear();
        match stdin.read_line(&mut line) {
            Ok(0) | Err(_) => break, // EOF or broken pipe
            Ok(_) => {}
        }
        let req = line.trim();
        if req.is_empty() {
            continue;
        }

        // Transport-stateful, snapshot-independent bookkeeping.
        if let Some(sev) = mcp::log_level_from(req) {
            level.store(sev, Ordering::Relaxed);
        }
        if mcp::is_initialize(req) {
            active.store(true, Ordering::Relaxed);
        }

        let response = if mcp::requires_snapshot(req) {
            // tools/call — fetch a live snapshot, bounded so we never hang.
            let (reply_tx, reply_rx) = mpsc::channel();
            if req_tx.send(UiReq::Snapshot(reply_tx)).is_err() {
                break; // ticker gone
            }
            match reply_rx.recv_timeout(SNAPSHOT_BUDGET) {
                Ok(snap) => {
                    let home = session::home_dir();
                    // The write capability: a set_pane_config batch is applied on
                    // the gpui main thread via the same ticker, bounded by the
                    // same budget. Reads (list_panes/pane_events/get_pane_config)
                    // never invoke this closure.
                    let apply = |updates: &[mcp::ConfigUpdate]| -> Vec<mcp::ApplyOutcome> {
                        let (tx, rx) = mpsc::channel();
                        if req_tx.send(UiReq::Apply(updates.to_vec(), tx)).is_err() {
                            return refuse_all(updates, "terminal-delight UI gone");
                        }
                        match rx.recv_timeout(SNAPSHOT_BUDGET) {
                            Ok(out) => out,
                            Err(_) => refuse_all(updates, "terminal-delight UI not ready"),
                        }
                    };
                    // The read capability for `grep`: search exposed panes' grids
                    // on the gpui main thread via the same ticker + budget. Reads
                    // other than grep never invoke this closure.
                    let search = |needle: &str, cap: usize| -> Vec<mcp::PaneMatches> {
                        let (tx, rx) = mpsc::channel();
                        if req_tx
                            .send(UiReq::Search(needle.to_string(), cap, tx))
                            .is_err()
                        {
                            return Vec::new();
                        }
                        rx.recv_timeout(SNAPSHOT_BUDGET).unwrap_or_default()
                    };
                    mcp::handle_line_with(req, &snap, |p, n| tail_for(p, n, &home), apply, search)
                }
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    mcp::error_response(req, -32000, "terminal-delight UI not ready")
                }
            }
        } else {
            // Static method: answer instantly, no main-thread round-trip.
            mcp::handle_line(req, &mcp::Snapshot::empty(), |_, _| vec![])
        };

        if let Some(resp) = response {
            if out_tx.send(resp).is_err() {
                break; // writer gone
            }
        }
    }
}

/// Resolve a pane's recent tool events by tailing its own transcript. Shared by
/// the request path (`pane_events`) and the push feed. Pure IO, off the main
/// thread; only ever sees panes the policy already marked exposed.
fn tail_for(p: &mcp::PaneInfo, limit: usize, home: &std::path::Path) -> Vec<mcp::ToolEvent> {
    match mcp_tail::transcript_for(&p.mode, p.cwd.as_deref(), p.session.as_deref(), home) {
        Some(path) => mcp_tail::tail_tool_events(&path, limit),
        None => vec![],
    }
}

/// The push feed: each periodic snapshot is tailed and diffed into
/// `notifications/message`. Exits when the ticker drops `snap_rx` (window gone)
/// or the writer disappears.
fn notify_loop(
    snap_rx: mpsc::Receiver<mcp::Snapshot>,
    out_tx: mpsc::Sender<String>,
    level: Arc<AtomicU8>,
    active: Arc<AtomicBool>,
    home: std::path::PathBuf,
) {
    let mut watcher = mcp::Watcher::default();
    while let Ok(snap) = snap_rx.recv() {
        // Don't push before the client handshakes, if the client raised the log
        // level past info, or if exposure/events are off.
        if !active.load(Ordering::Relaxed)
            || level.load(Ordering::Relaxed) > mcp::log_severity("info")
            || !(snap.config.enabled && snap.config.events)
        {
            continue;
        }
        let mut tailed: HashMap<u32, Vec<mcp::ToolEvent>> = HashMap::new();
        for p in snap.panes.iter().filter(|p| p.exposed && p.is_agent) {
            tailed.insert(p.pid, tail_for(p, PUSH_TAIL, &home));
        }
        for n in watcher.diff(&snap.panes, &tailed) {
            if out_tx.send(mcp::encode_notification(&n)).is_err() {
                return; // writer gone
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Claude Code's per-project transcript-dir slug (mirrors session.rs).
    fn slug(cwd: &str) -> String {
        cwd.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect()
    }

    fn agent_snapshot(pid: u32, cwd: &str) -> mcp::Snapshot {
        mcp::Snapshot {
            config: mcp::McpConfig {
                enabled: true,
                expose: mcp::Expose::AgentsOnly,
                events: true,
                writable: false,
            },
            panes: vec![mcp::PaneInfo {
                tab: 0,
                title: "work".into(),
                mode: "CLAUDE".into(),
                is_agent: true,
                pid,
                cwd: Some(cwd.into()),
                session: Some("claude --resume x".into()),
                exposed: true,
                grade: mcp::GradeReport::default(),
            }],
            outer_grade: mcp::GradeReport::default(),
        }
    }

    fn append_tool_use(path: &std::path::Path, tool: &str, summary: &str) {
        let line = format!(
            r#"{{"type":"assistant","timestamp":"ts-{summary}","message":{{"content":[{{"type":"tool_use","name":"{tool}","input":{{"command":"{summary}"}}}}]}}}}"#
        );
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        writeln!(f, "{line}").unwrap();
    }

    /// End-to-end push feed over a REAL transcript file (no gpui): first sight
    /// announces the agent without replaying history, and a tool call written
    /// afterwards is pushed as a `notifications/message`. The barrier (reading
    /// the first notification) guarantees the bookmark is set before we append,
    /// so this is deterministic — no sleeps.
    #[test]
    fn push_feed_announces_then_streams_new_tool_calls() {
        let home = std::env::temp_dir().join(format!("td-push-{}", std::process::id()));
        let cwd = "/w/push-x";
        let proj = home.join(".claude/projects").join(slug(cwd));
        std::fs::create_dir_all(&proj).unwrap();
        // Named for the pane's session id ("claude --resume x") so the tailer
        // resolves it by the fd-accurate id, not newest-by-mtime.
        let transcript = proj.join("x.jsonl");
        append_tool_use(&transcript, "Bash", "first"); // pre-existing history

        let (snap_tx, snap_rx) = mpsc::channel::<mcp::Snapshot>();
        let (out_tx, out_rx) = mpsc::channel::<String>();
        let level = Arc::new(AtomicU8::new(mcp::log_severity("info")));
        let active = Arc::new(AtomicBool::new(true)); // pretend the client handshaked
        let h = home.clone();
        let handle = thread::spawn(move || notify_loop(snap_rx, out_tx, level, active, h));

        // 1st poll: first sight ⇒ agent_appeared, history NOT replayed.
        snap_tx.send(agent_snapshot(4242, cwd)).unwrap();
        let first: serde_json::Value = serde_json::from_str(&out_rx.recv().unwrap()).unwrap();
        assert_eq!(first["method"], "notifications/message");
        assert_eq!(first["params"]["data"]["event"], "agent_appeared");
        assert_eq!(first["params"]["data"]["pid"], 4242);

        // A new tool call lands, then the next poll arrives → it is pushed.
        append_tool_use(&transcript, "Edit", "second");
        snap_tx.send(agent_snapshot(4242, cwd)).unwrap();
        let second: serde_json::Value = serde_json::from_str(&out_rx.recv().unwrap()).unwrap();
        assert_eq!(second["params"]["data"]["event"], "tool_call");
        assert_eq!(second["params"]["data"]["tool"], "Edit");
        assert_eq!(second["params"]["data"]["summary"], "second");

        drop(snap_tx); // ends the loop cleanly
        handle.join().unwrap();
        std::fs::remove_dir_all(&home).ok();
    }

    /// The feed stays silent until the client has handshaked (active=false).
    #[test]
    fn push_feed_is_silent_before_handshake() {
        let home = std::env::temp_dir().join(format!("td-push-silent-{}", std::process::id()));
        let cwd = "/w/silent";
        let proj = home.join(".claude/projects").join(slug(cwd));
        std::fs::create_dir_all(&proj).unwrap();
        append_tool_use(&proj.join("s.jsonl"), "Bash", "x");

        let (snap_tx, snap_rx) = mpsc::channel::<mcp::Snapshot>();
        let (out_tx, out_rx) = mpsc::channel::<String>();
        let level = Arc::new(AtomicU8::new(mcp::log_severity("info")));
        let active = Arc::new(AtomicBool::new(false)); // NOT handshaked
        let h = home.clone();
        let handle = thread::spawn(move || notify_loop(snap_rx, out_tx, level, active, h));

        snap_tx.send(agent_snapshot(7, cwd)).unwrap();
        drop(snap_tx);
        handle.join().unwrap();
        assert!(
            out_rx.recv().is_err(),
            "nothing should be pushed before initialize"
        );
        std::fs::remove_dir_all(&home).ok();
    }
}
