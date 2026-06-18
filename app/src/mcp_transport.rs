//! The live stdio JSON-RPC transport for the read-only MCP server.
//!
//! Wiring problem: the protocol ([`crate::mcp`]) is pure and the pane snapshot
//! lives in gpui's main thread, but a JSON-RPC server has to block on stdin.
//! So the transport is two halves bridged by a channel:
//!
//!   • a dedicated **reader thread** blocks on stdin, and for each request line
//!     asks the main thread for a fresh [`mcp::Snapshot`], runs the pure
//!     dispatch, and writes the one response line to stdout (the single writer);
//!   • a **gpui ticker** (same leak-safe shape as the jiggle/checkpoint clocks)
//!     drains those snapshot requests on the main thread, where `&App` is
//!     available, and replies. When the window is dropped it answers any
//!     in-flight request with a locked-down snapshot and then stops, so the
//!     reader never hangs and no orphan task survives the window.
//!
//! Transport is opt-in: `main` only starts it when `TD_MCP` is set (an
//! orchestrator launching terminal-delight with stdio piped). It never writes
//! to a PTY — exposure is still gated by the operator policy in the snapshot.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::time::Duration;

use gpui::Context;

use crate::{mcp, mcp_tail, session, Workspace};

/// A one-shot reply channel the main thread sends a fresh snapshot back on.
type Reply = mpsc::Sender<mcp::Snapshot>;

/// Start the stdio MCP server once per process. Call from `Workspace::new`
/// (only the primary, non-scratch window). No-op on a second call.
pub fn start(cx: &mut Context<Workspace>) {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    let (req_tx, req_rx) = mpsc::channel::<Reply>();

    // Reader half: blocks on stdin off the main thread.
    let _ = std::thread::Builder::new()
        .name("td-mcp".into())
        .spawn(move || serve_stdio(req_tx));

    // Ticker half: services snapshot requests on the gpui main thread.
    cx.spawn(async move |this, cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(40))
                .await;
            let mut alive = true;
            loop {
                match req_rx.try_recv() {
                    Ok(reply) => {
                        let snap = this.update(cx, |ws: &mut Workspace, cx| mcp::Snapshot {
                            config: ws.mcp.clone(),
                            panes: ws.mcp_snapshot(cx),
                        });
                        match snap {
                            Ok(s) => {
                                let _ = reply.send(s);
                            }
                            Err(_) => {
                                // Window gone mid-drain: unblock the reader with
                                // a locked snapshot, then stop the ticker.
                                let _ = reply.send(locked());
                                alive = false;
                            }
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    // Reader thread ended (stdin closed) — nothing left to serve.
                    Err(TryRecvError::Disconnected) => return,
                }
            }
            // Stop (and let the reader exit on its next send) once the window
            // is dropped — mirrors the jiggle clock's orphan-task guard.
            if !alive || this.update(cx, |_, _| ()).is_err() {
                break;
            }
        }
    })
    .detach();
}

/// A snapshot that exposes nothing — handed to the reader when the window has
/// gone so an in-flight request still gets a (safe, empty) answer.
fn locked() -> mcp::Snapshot {
    mcp::Snapshot {
        config: mcp::McpConfig::default(),
        panes: vec![],
    }
}

/// The reader loop: one request line in, one response line out. Exits on EOF,
/// a write error, or the main thread going away.
fn serve_stdio(req_tx: mpsc::Sender<Reply>) {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    let mut line = String::new();
    loop {
        line.clear();
        match stdin.read_line(&mut line) {
            Ok(0) | Err(_) => break, // EOF or a broken pipe
            Ok(_) => {}
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Ask the main thread for a fresh snapshot via a per-request channel.
        let (reply_tx, reply_rx) = mpsc::channel();
        if req_tx.send(reply_tx).is_err() {
            break; // ticker gone
        }
        let snap = match reply_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(s) => s,
            // Shutting down or wedged — don't hang the reader forever.
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => continue,
        };

        // Transcript IO happens here, off the main thread; the closure only
        // sees panes the policy already marked exposed.
        let tail = |p: &mcp::PaneInfo, limit: usize| -> Vec<mcp::ToolEvent> {
            let home = session::home_dir();
            match mcp_tail::transcript_for(&p.mode, p.cwd.as_deref(), &home) {
                Some(path) => mcp_tail::tail_tool_events(&path, limit),
                None => vec![],
            }
        };

        if let Some(resp) = mcp::handle_line(trimmed, &snap, tail) {
            if writeln!(out, "{resp}").is_err() || out.flush().is_err() {
                break;
            }
        }
    }
}
