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
//!     bridge for a fresh snapshot (5 s budget) and, on timeout, returns a
//!     JSON-RPC error rather than dropping the request (no client hang).
//!   • **bridge** (gpui main thread) — sleeps on its request channel. The
//!     notifier requests snapshots only while its client accepts events.
//!   • **notifier thread** — tails exposed agent transcripts off the main
//!     thread, diffs them through [`mcp::Watcher`], and emits
//!     `notifications/message` so an orchestrator reacts without polling.
//!   • **writer thread** — the *single* owner of stdout; both the reader
//!     (responses) and the notifier (notifications) funnel lines through it, so
//!     the two streams never interleave-corrupt.
//!
//! Opt-in via `TD_MCP`; process-wide singleton. It never writes to a PTY.
//!
//! **stdio is not the only way in.** The shared main-thread *bridge* lives in
//! ([`start_bridge`]) — the one main-thread round-trip every transport shares —
//! and [`respond`] turns a JSON-RPC line into a response line against it. stdio
//! requires the MCP client to be our PARENT (it pipes our stdin/stdout), which
//! a desktop-launched GUI terminal never is; the ctl socket ([`crate::ctl`])
//! carries the same protocol to a terminal that is ALREADY running, which is
//! how an agent inside a pane reaches the window hosting it.

use futures::{channel::mpsc as ui_channel, StreamExt};
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use gpui::Context;

use crate::{mcp, mcp_tail, paneident, session, Workspace};

/// What the reader thread asks the bridge (gpui main thread) to do. Both
/// variants carry a one-shot channel the bridge answers on, so the reader can
/// block with a budget and never hang. Reads and writes share the one bridge so
/// every touch of pane state happens on the main thread — the reader never
/// mutates anything itself.
enum UiReq {
    /// Produce a fresh pane snapshot (every `tools/call` needs one to gate on
    /// policy and read identities/grades).
    Snapshot(mcp::SnapshotFor, mpsc::Sender<mcp::Snapshot>),
    /// Apply a parsed `set_pane_config` batch and report the per-target outcome.
    Apply(Vec<mcp::ConfigUpdate>, mpsc::Sender<Vec<mcp::ApplyOutcome>>),
    /// `grep`: carry the verified caller to the grid read, so queued searches
    /// cannot keep using a relationship that was revoked after the snapshot.
    Search(
        String,
        usize,
        Option<u64>,
        mpsc::Sender<Vec<mcp::PaneMatches>>,
    ),
    /// `open_document`: open a document in the caller's pane or tab. The
    /// reply is the router's own line, which for a split arrives only once the
    /// workspace has handled the pane's request — after this tick — so the
    /// sender travels with the request rather than being answered here.
    Open(mcp::OpenRequest, mpsc::Sender<String>),
    /// `ask_agent`: an agent's question for another, decided and answered by
    /// the window, which holds the Threshing Floor's chart and the seat facts.
    Ask(mcp::AskCall, mpsc::Sender<mcp::AskReply>),
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
/// How many recent events to tail per pane when diffing the push feed.
const PUSH_TAIL: usize = 50;

/// Every transport wakes the same main-thread receiver when it has work.
static BRIDGE: OnceLock<ui_channel::UnboundedSender<UiReq>> = OnceLock::new();

fn bridge_send(req: UiReq) -> bool {
    BRIDGE
        .get()
        .is_some_and(|tx| tx.unbounded_send(req).is_ok())
}

/// Start the stdio MCP server once per process. Call from `Workspace::build`.
/// No-op on a second call. Brings the bridge up with it, wired to the push feed.
pub fn start(config: &mcp::McpConfig, cx: &mut Context<Workspace>) {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    start_bridge(cx);
    let (push_tx, push_rx) = mpsc::channel::<bool>();
    let exposed = Arc::new(AtomicBool::new(config.enabled && config.events));
    let policy = exposed.clone();
    let wake = push_tx.clone();
    cx.observe_self(move |ws, _| {
        let enabled = ws.mcp.enabled && ws.mcp.events;
        if policy.swap(enabled, Ordering::Relaxed) != enabled {
            let _ = wake.send(true);
        }
    })
    .detach();
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
            .spawn(move || {
                notify_loop(
                    push_snapshots(push_rx, &active, &level, &exposed),
                    out_tx,
                    level.clone(),
                    active.clone(),
                    home,
                )
            });
    }

    // Reader: blocks on stdin, routes each request.
    {
        let out_tx = out_tx.clone();
        let level = Arc::clone(&level);
        let active = Arc::clone(&active);
        let _ = thread::Builder::new()
            .name("td-mcp".into())
            .spawn(move || serve_stdio(out_tx, level, active, push_tx));
    }
}

/// A socket-only window registers no timer. Receiving a request schedules the UI.
pub fn start_bridge(cx: &mut Context<Workspace>) {
    let (req_tx, mut req_rx) = ui_channel::unbounded();
    if BRIDGE.set(req_tx).is_err() {
        return;
    }
    cx.spawn(async move |this, cx| {
        while let Some(req) = req_rx.next().await {
            match req {
                UiReq::Snapshot(need, reply) => {
                    match this.update(cx, |ws, cx| {
                        mcp::timed("snapshot", || scoped_snapshot(ws, cx, need))
                    }) {
                        Ok(s) => {
                            let _ = reply.send(s);
                        }
                        Err(_) => {
                            let _ = reply.send(mcp::Snapshot::empty());
                            return;
                        }
                    }
                }
                UiReq::Apply(updates, reply) => {
                    match this.update(cx, |ws, cx| ws.apply_mcp_config(&updates, cx)) {
                        Ok(out) => {
                            let _ = reply.send(out);
                        }
                        Err(_) => {
                            let _ =
                                reply.send(refuse_all(&updates, "terminal-delight UI not ready"));
                            return;
                        }
                    }
                }
                UiReq::Search(needle, cap, caller, reply) => {
                    match this.update(cx, |ws, cx| ws.mcp_search(&needle, cap, caller, cx)) {
                        Ok(hits) => {
                            let _ = reply.send(hits);
                        }
                        Err(_) => {
                            let _ = reply.send(Vec::new());
                            return;
                        }
                    }
                }
                UiReq::Open(request, reply) => {
                    let answer = reply.clone();
                    if this
                        .update(cx, |ws, cx| ws.mcp_open(request, reply, cx))
                        .is_err()
                    {
                        let _ = answer.send("err terminal-delight UI not ready".into());
                        return;
                    }
                }
                UiReq::Ask(request, reply) => {
                    match this.update(cx, |ws, cx| ws.ask_bridge(&request, cx)) {
                        Ok(answer) => {
                            let _ = reply.send(answer);
                        }
                        Err(_) => {
                            let _ = reply.send(mcp::AskReply::Refused(
                                crate::threshing_floor::Refusal::Gone,
                            ));
                            return;
                        }
                    }
                }
            }
        }
    })
    .detach();
}

/// Build a fresh snapshot from the live workspace (runs on the main thread).
#[cfg(test)]
pub(crate) fn snapshot_of(ws: &mut Workspace, cx: &mut Context<Workspace>) -> mcp::Snapshot {
    scoped_snapshot(ws, cx, mcp::SnapshotFor::Other)
}

fn scoped_snapshot(
    ws: &mut Workspace,
    cx: &mut Context<Workspace>,
    need: mcp::SnapshotFor,
) -> mcp::Snapshot {
    use mcp::SnapshotFor::*;
    let coord = matches!(need, Coordination | Engineering);
    if coord {
        ws.sync_coordination_scope(cx);
    }
    let runtime = !matches!(
        need,
        Panes { compact: true, .. } | Identity | Engineering | Coordination | Documents
    );
    let filter_facts = matches!(
        need,
        Panes {
            filter_facts: true,
            ..
        }
    );
    mcp::Snapshot {
        config: ws.mcp.clone(),
        panes: ws.mcp_panes(cx, runtime, filter_facts),
        outer_grade: ws.mcp_outer_grade(cx),
        instance: Some(here()),
        // Filled in by the transport that has a caller to name; the bridge
        // serves every transport and knows nothing about who asked.
        caller: None,
        peer_in_pane: false,
        engineering: if matches!(need, Engineering) && ws.coordination_full_scope() {
            ws.eng_reports()
        } else {
            vec![]
        },
        coordination: coord.then(|| ws.coord_authorized.clone()),
        engineering_allowed: coord && ws.coordination_full_scope(),
        documents: if matches!(need, Documents) {
            ws.mcp_documents(cx)
        } else {
            vec![]
        },
        chart: ws.chart.clone(),
        orgs: ws.floor_orgs.clone(),
        moved: ws.mcp_moves(cx),
        far: if matches!(need, Panes { far: true, .. }) {
            ws.far_seen(cx)
        } else {
            vec![]
        },
    }
}

/// Who this window is, for the stamp on every tool result.
///
/// Built here rather than in [`mcp`] so that module stays pure protocol: it
/// needs the process-wide session binding and this executable's own path, and
/// neither belongs in a file that is otherwise data and JSON.
///
/// The build is the executable's file name because that is what actually
/// differs between two windows on one box — they are installed as
/// `td-<sha>-<label>` and cut over by moving a symlink, so `CARGO_PKG_VERSION`
/// reads the same in a window three commits behind and in one built from HEAD.
fn here() -> mcp::Instance {
    mcp::Instance {
        session: crate::instance::key().to_string(),
        window: std::process::id(),
        build: std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())),
    }
}

/// Answer ONE JSON-RPC line against the bridge. `None` for a notification or an
/// unparseable line (both get no reply, per JSON-RPC).
///
/// This is the whole protocol surface a transport needs: stdio calls it per
/// stdin line, the ctl socket calls it per `mcp rpc` connection. Static methods
/// (`initialize`, `tools/list`, `ping`) short-circuit with no main-thread
/// round-trip, so a handshake answers instantly even while the UI is busy.
/// Blocks up to [`SNAPSHOT_BUDGET`], so callers must not run it on a thread that
/// owns an accept loop.
pub(crate) fn respond(line: &str) -> Option<String> {
    respond_as(line, None, None)
}

/// [`respond`], with the caller named.
///
/// `caller` is what a relay worked out about ITSELF from its own process tree —
/// which session it is in and which pane. It reaches the snapshot rather than
/// the transport because it is per-request data the protocol layer needs: a
/// pane-scoped verb can then default to the pane that called it instead of
/// making the agent guess its own pid out of a listing.
pub(crate) fn respond_as(
    line: &str,
    caller: Option<crate::ctl::Caller>,
    peer_pid: Option<u32>,
) -> Option<String> {
    let Some(need) = mcp::snapshot_for(line) else {
        return mcp::handle_line(line, &mcp::Snapshot::empty(), |_, _| vec![]);
    };
    let (reply_tx, reply_rx) = mpsc::channel();
    if !bridge_send(UiReq::Snapshot(need, reply_tx)) {
        return mcp::error_response(line, -32000, "terminal-delight UI not ready");
    }
    match reply_rx.recv_timeout(SNAPSHOT_BUDGET) {
        Ok(mut snap) => {
            let ancestry = crate::ctl::peer_ancestry(peer_pid).collect::<Vec<_>>();
            snap.peer_in_pane = snap
                .panes
                .iter()
                .any(|p| p.pid > 1 && ancestry.contains(&p.pid));
            snap.caller = caller.map(|c| {
                let verified = crate::ctl::verify_caller(c, &ancestry, |pane| {
                    snap.panes
                        .iter()
                        .find(|p| p.pane_id == Some(pane))
                        .map(|p| p.pid)
                });
                mcp::Caller {
                    session: verified.session,
                    pane: verified.pane,
                }
            });
            let home = session::home_dir();
            if mcp::names_agents(line) {
                let bound = paneident::resolve(&fleet_facts(&snap.panes), &home);
                stamp_agents(&mut snap.panes, &bound);
            }
            // The write capability: a set_pane_config batch is applied on the
            // gpui main thread via the same bridge, bounded by the same budget.
            // Reads (list_panes/pane_events/get_pane_config) never invoke this.
            let apply = |updates: &[mcp::ConfigUpdate]| -> Vec<mcp::ApplyOutcome> {
                let (tx, rx) = mpsc::channel();
                if !bridge_send(UiReq::Apply(updates.to_vec(), tx)) {
                    return refuse_all(updates, "terminal-delight UI gone");
                }
                match rx.recv_timeout(SNAPSHOT_BUDGET) {
                    Ok(out) => out,
                    Err(_) => refuse_all(updates, "terminal-delight UI not ready"),
                }
            };
            // The read capability for `grep`: search exposed panes' grids on the
            // gpui main thread via the same bridge + budget. Reads other than
            // grep never invoke this closure.
            let search = |needle: &str, cap: usize| -> Vec<mcp::PaneMatches> {
                let (tx, rx) = mpsc::channel();
                let caller = snap.caller.as_ref().and_then(|c| c.pane);
                if !bridge_send(UiReq::Search(needle.to_string(), cap, caller, tx)) {
                    return Vec::new();
                }
                rx.recv_timeout(SNAPSHOT_BUDGET).unwrap_or_default()
            };
            // The layout capability for `open_document`, the same round-trip
            // and budget. The window answers with the router's own line, sent
            // when the split, the focus or the square has happened.
            let open = |request: &mcp::OpenRequest| -> mcp::OpenOutcome {
                let (tx, rx) = mpsc::channel();
                if !bridge_send(UiReq::Open(request.clone(), tx)) {
                    return Err("terminal-delight UI gone".to_string());
                }
                match rx.recv_timeout(SNAPSHOT_BUDGET) {
                    Ok(reply) => mcp::open_outcome(&reply),
                    Err(_) => Err("terminal-delight UI not ready".to_string()),
                }
            };
            let ask = |call: &mcp::AskCall| -> mcp::AskReply {
                let gone = || mcp::AskReply::Refused(crate::threshing_floor::Refusal::Gone);
                // A copy is priced here, on this connection's thread: which
                // conversation the pane holds is a fact about the whole
                // window, and reading its size is file work the window's
                // thread must not wait on.
                let mut call = call.clone();
                if let mcp::AskCall::Question(req) = &mut call {
                    if req.copy {
                        let bound = paneident::resolve(&fleet_facts(&snap.panes), &home);
                        req.price = Some(crate::askcopy::price(&bound, req.target));
                    }
                }
                let (tx, rx) = mpsc::channel();
                if !bridge_send(UiReq::Ask(call, tx)) {
                    return gone();
                }
                rx.recv_timeout(SNAPSHOT_BUDGET).unwrap_or_else(|_| gone())
            };
            mcp::handle_line_asking(
                line,
                &snap,
                |p, n| tail_for(p, &snap.panes, n, &home),
                apply,
                search,
                open,
                ask,
            )
        }
        Err(RecvTimeoutError::Disconnected) | Err(RecvTimeoutError::Timeout) => {
            mcp::error_response(line, -32000, "terminal-delight UI not ready")
        }
    }
}

/// The reader loop: one request line in, one response line out (via the writer).
fn serve_stdio(
    out_tx: mpsc::Sender<String>,
    level: Arc<AtomicU8>,
    active: Arc<AtomicBool>,
    push: mpsc::Sender<bool>,
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
            let _ = push.send(true);
        }
        if mcp::is_initialize(req) && !active.swap(true, Ordering::Relaxed) {
            let _ = push.send(true);
        }

        if let Some(resp) = respond(req) {
            if out_tx.send(resp).is_err() {
                break; // writer gone
            }
        }
    }
    let _ = push.send(false);
}

/// Resolve a pane's recent tool events by tailing its own transcript. Shared by
/// the request path (`pane_events`) and the push feed. Pure IO, off the main
/// thread; only ever sees panes the policy already marked exposed.
///
/// Takes the whole window, not just the pane being asked about: which
/// conversation a pane holds is a fact about the set — two panes may not hold
/// one — and asking pane by pane is what returned another agent's tool calls
/// under the pid you asked for (#564, and the duplicate session ids in
/// `list_panes` that made it look like a display bug).
///
/// A pane whose conversation cannot be evidenced returns no events, which is
/// the honest answer and a different one from "this agent has run no tools".
fn tail_for(
    p: &mcp::PaneInfo,
    fleet: &[mcp::PaneInfo],
    limit: usize,
    home: &std::path::Path,
) -> Vec<mcp::ToolEvent> {
    match paneident::certain(&fleet_facts(fleet), home).get(&p.pid) {
        Some(path) => mcp_tail::tail_tool_events(path, limit),
        None => vec![],
    }
}

/// Give each agent pane the conversation it is proven to hold: its lasting
/// name, which a verb about another agent can be given instead of a pid. Only
/// a certain bond names an agent. A guess (the last conversation spoken into in
/// a shared directory) is where an agent probably is, and a prompt aimed by one
/// would land on the wrong agent. A pane already named, by its sleep record,
/// keeps that name.
fn stamp_agents(panes: &mut [mcp::PaneInfo], bound: &HashMap<u32, paneident::Bound>) {
    for p in panes.iter_mut().filter(|p| p.agent.is_none()) {
        p.agent = bound
            .get(&p.pid)
            .filter(|b| b.is_certain())
            .map(|b| b.session_id.clone());
    }
}

/// Every agent pane in a snapshot, as the resolver wants them.
fn fleet_facts(panes: &[mcp::PaneInfo]) -> Vec<paneident::PaneFacts> {
    panes
        .iter()
        .filter(|p| p.is_agent)
        .map(|p| paneident::PaneFacts {
            shell_pid: p.pid,
            mode: p.mode.clone(),
            cwd: p.cwd.clone(),
            resume: p.session.clone(),
        })
        .collect()
}

fn push_snapshots<'a>(
    demand: mpsc::Receiver<bool>,
    active: &'a AtomicBool,
    level: &'a AtomicU8,
    exposed: &'a AtomicBool,
) -> impl Iterator<Item = mcp::Snapshot> + 'a {
    std::iter::from_fn(move || loop {
        if !active.load(Ordering::Relaxed)
            || !exposed.load(Ordering::Relaxed)
            || level.load(Ordering::Relaxed) > mcp::log_severity("info")
        {
            if demand.recv() != Ok(true) {
                return None;
            }
            continue;
        }
        match demand.recv_timeout(Duration::from_secs(1)) {
            Ok(true) => continue,
            Ok(false) | Err(RecvTimeoutError::Disconnected) => return None,
            Err(RecvTimeoutError::Timeout) => {}
        }
        let (tx, rx) = mpsc::channel();
        if !bridge_send(UiReq::Snapshot(mcp::SnapshotFor::Push, tx)) {
            return None;
        }
        if let Ok(snap) = rx.recv_timeout(SNAPSHOT_BUDGET) {
            return Some(snap);
        }
    })
}

/// The push feed: each periodic snapshot is tailed and diffed into
/// `notifications/message`. Exits when the bridge drops `snap_rx` (window gone)
/// or the writer disappears.
fn notify_loop(
    snapshots: impl IntoIterator<Item = mcp::Snapshot>,
    out_tx: mpsc::Sender<String>,
    level: Arc<AtomicU8>,
    active: Arc<AtomicBool>,
    home: std::path::PathBuf,
) {
    let mut watcher = mcp::Watcher::default();
    for snap in snapshots {
        // Don't push before the client handshakes, if the client raised the log
        // level past info, or if exposure/events are off.
        if !active.load(Ordering::Relaxed)
            || level.load(Ordering::Relaxed) > mcp::log_severity("info")
            || !(snap.config.enabled && snap.config.events)
        {
            continue;
        }
        // One resolution for the whole snapshot, then a tail per pane: the
        // binding is a property of the set, and recomputing it per pane would
        // also re-walk every transcript in every project directory.
        let bound = paneident::certain(&fleet_facts(&snap.panes), &home);
        let mut tailed: HashMap<u32, Vec<mcp::ToolEvent>> = HashMap::new();
        // Pushed events reach every connection at once, so each pane carries
        // the reach an agent nobody can name gets: a seated pane is not pushed.
        let pushable: Vec<mcp::PaneInfo> = snap
            .panes
            .iter()
            .map(|p| mcp::PaneInfo {
                reach: Some(crate::threshing_floor::anonymous_reach(
                    &snap.config,
                    &p.party(),
                )),
                ..p.clone()
            })
            .collect();
        for p in pushable
            .iter()
            .filter(|p| p.is_agent && p.reach >= Some(crate::threshing_floor::Reach::Read))
        {
            let events = match bound.get(&p.pid) {
                Some(path) => mcp_tail::tail_tool_events(path, PUSH_TAIL),
                None => vec![],
            };
            tailed.insert(p.pid, events);
        }
        for n in watcher.diff(&pushable, &tailed) {
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
                team: false,
            },
            panes: vec![mcp::PaneInfo {
                project: None,
                state: None,
                tab: 0,
                title: "work".into(),
                mode: "CLAUDE".into(),
                is_agent: true,
                pid,
                pane_id: Some(1),
                cwd: Some(cwd.into()),
                session: Some("claude --resume x".into()),
                tool: None,
                note: None,
                reach: None,
                access: crate::threshing_floor::AgentAccess::Open,
                seat: None,
                name: None,
                agent: None,
                dormancy: None,
                grade: mcp::GradeReport::default(),
            }],
            outer_grade: mcp::GradeReport::default(),
            instance: Some(mcp::Instance {
                session: "1".into(),
                window: 99,
                build: Some("td-test".into()),
            }),
            caller: None,
            peer_in_pane: false,
            engineering: vec![],
            coordination: None,
            engineering_allowed: false,
            documents: vec![],
            chart: crate::threshing_floor::Chart::default(),
            orgs: crate::threshing_floor::Orgs::default(),
            moved: vec![],
            far: vec![],
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

    /// One pushed line, or a failure that says which one never came.
    ///
    /// A bare `recv()` here is a HANG rather than a test failure: a change that
    /// stops the feed leaves the whole suite sitting on a channel with 1327
    /// green tests above it and no result line, which is exactly how this one
    /// wedged a run while a resolver change was being written. Waiting has to
    /// have an end, and the end has to name what it was waiting for.
    fn next(rx: &mpsc::Receiver<String>, waiting_for: &str) -> String {
        rx.recv_timeout(Duration::from_secs(10))
            .unwrap_or_else(|_| panic!("no push arrived; expected {waiting_for}"))
    }

    /// A pid no process can have.
    ///
    /// Linux caps pids at `/proc/sys/kernel/pid_max`, which is at most 2^22 on
    /// this architecture, so nothing above it ever has a `/proc` entry. The test
    /// pane below needs that: a pane whose agent CANNOT be read is the one case
    /// where [`crate::paneident`] falls back to the pane's own resume line, and
    /// a pid that merely happens to be free on the machine running the suite is
    /// not the same thing. This test used 4242 and passed here for a year and
    /// failed in CI the first time it ran there, because a busy runner had a
    /// process sitting on it.
    const NO_SUCH_PID: u32 = u32::MAX - 1;

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
        snap_tx.send(agent_snapshot(NO_SUCH_PID, cwd)).unwrap();
        let first: serde_json::Value =
            serde_json::from_str(&next(&out_rx, "agent_appeared")).unwrap();
        assert_eq!(first["method"], "notifications/message");
        assert_eq!(first["params"]["data"]["event"], "agent_appeared");
        assert_eq!(first["params"]["data"]["pid"], NO_SUCH_PID);

        // A new tool call lands, then the next poll arrives → it is pushed.
        append_tool_use(&transcript, "Edit", "second");
        snap_tx.send(agent_snapshot(NO_SUCH_PID, cwd)).unwrap();
        let second: serde_json::Value = serde_json::from_str(&next(&out_rx, "tool_call")).unwrap();
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

    /// Only a proven binding names an agent. A guessed one (the last
    /// conversation spoken into, in a directory several agents share) would
    /// aim a prompt at whichever agent spoke last, so it names nothing; and a
    /// pane its sleep record already named keeps that name.
    #[test]
    fn only_a_certain_binding_gives_an_agent_its_name() {
        use crate::paneident::Bound;
        use crate::vitals::Bond;
        let pane = |pid: u32, agent: Option<&str>| mcp::PaneInfo {
            pid,
            agent: agent.map(String::from),
            ..agent_snapshot(pid, "/w").panes[0].clone()
        };
        let mut panes = vec![pane(1, None), pane(2, None), pane(3, Some("asleep-conv"))];
        let bound = |id: &str, bond| Bound {
            session_id: id.into(),
            path: format!("/t/{id}.jsonl").into(),
            bond,
        };
        let map: HashMap<u32, Bound> = [
            (1, bound("declared-conv", Bond::Declared)),
            (2, bound("guessed-conv", Bond::Guess)),
            (3, bound("other-conv", Bond::Declared)),
        ]
        .into_iter()
        .collect();
        stamp_agents(&mut panes, &map);
        let names: Vec<Option<&str>> = panes.iter().map(|p| p.agent.as_deref()).collect();
        assert_eq!(names, [Some("declared-conv"), None, Some("asleep-conv")]);
    }
}
