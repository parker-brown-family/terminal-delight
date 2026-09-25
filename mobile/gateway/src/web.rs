//! The door: HTTP and WebSocket, for one kind of client — a paired phone.
//!
//! Three checks run before any route, in this order, and each refuses rather
//! than guesses:
//!
//! 1. **No `Origin`.** The phone's HTTP client never sends one; a browser
//!    always does on a cross-site request. A loopback port is reachable from
//!    every page open on this machine, so a request that carries an origin is a
//!    web page trying the door. (The APES daemon's rule, for the same reason.)
//! 2. **`Host` is an address we bound.** Refuses DNS rebinding, where a page
//!    points a name it controls at 127.0.0.1.
//! 3. **The bearer token**, compared in constant time. Everything except
//!    `/v0/ping` needs it; `ping` says only that a gateway lives here and what
//!    the machine is called, which is what the phone needs to find it.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::broadcast;

use crate::{host, mailbox, paths, tree};

pub struct Gate {
    pub token: String,
    pub machine: String,
    pub tailnet_name: Option<String>,
    pub allowed_hosts: HashSet<String>,
    pub events: broadcast::Sender<String>,
    pub watchers: AtomicUsize,
}

pub fn router(gate: Arc<Gate>) -> Router {
    Router::new()
        .route("/v0/ping", get(ping))
        .route("/v0/hello", get(hello))
        .route("/v0/events", get(events))
        .route("/v0/sessions/{key}/wall", get(wall))
        .route("/v0/sessions/{key}/panes/{pane}/bench", get(bench))
        .route("/v0/sessions/{key}/spawn", post(spawn))
        .route("/v0/sessions/{key}/panes/{pane}/term", get(term))
        .layer(middleware::from_fn_with_state(gate.clone(), guard))
        .with_state(gate)
}

fn refuse(status: StatusCode, why: &str) -> Response {
    (status, Json(json!({"error": why}))).into_response()
}

async fn guard(State(gate): State<Arc<Gate>>, req: Request, next: Next) -> Response {
    if req.headers().contains_key(header::ORIGIN) {
        return refuse(StatusCode::FORBIDDEN, "requests from web pages are refused");
    }
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !gate.allowed_hosts.contains(&host) {
        return refuse(StatusCode::MISDIRECTED_REQUEST, "not an address this gateway serves");
    }
    if req.uri().path() != "/v0/ping" {
        let offered = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .unwrap_or_default();
        if !same(offered.as_bytes(), gate.token.as_bytes()) {
            return refuse(StatusCode::UNAUTHORIZED, "this phone is not paired with this machine");
        }
    }
    next.run(req).await
}

/// Constant-time equality, so the time a refusal takes says nothing about how
/// much of the token was right.
fn same(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

async fn ping(State(gate): State<Arc<Gate>>) -> Json<Value> {
    Json(json!({"gateway": "terminal-delight", "v": 0, "machine": gate.machine}))
}

async fn hello(State(gate): State<Arc<Gate>>) -> Json<Value> {
    let sessions = host::live_sessions().await;
    Json(json!({
        "gateway": "terminal-delight",
        "v": 0,
        "machine": gate.machine,
        "tailnet": gate.tailnet_name,
        "gateway_pid": std::process::id(),
        "server_ms": now_ms(),
        "sessions": sessions,
    }))
}

/// The session, drawn the way the desk's left bar draws it, with each pane's
/// pulse and its newest response card.
async fn wall(Path(key): Path<String>) -> Response {
    let panes = match host::list_panes(&key).await {
        Ok(p) => p,
        Err(e) => return refuse(StatusCode::BAD_GATEWAY, &e),
    };
    let key2 = key.clone();
    // Reading every mailbox is file work; keep it off the async threads.
    let panes = tokio::task::spawn_blocking(move || {
        panes
            .into_iter()
            .map(|mut p| {
                let id = p["pane"].as_u64().unwrap_or_default();
                let ended = p["ended"].as_bool().unwrap_or_default();
                let resume = p["resume"].as_str().map(str::to_string);
                let surfaces = mailbox::surfaces(&key2, id);
                p["pulse"] = mailbox::pulse(&key2, id, resume.as_deref(), ended);
                p["latest"] = mailbox::latest_response(&surfaces).unwrap_or(Value::Null);
                p["surfaces"] = json!(surfaces.len());
                p
            })
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    Json(json!({
        "session": key,
        "server_ms": now_ms(),
        "tree": tree::read(&key),
        "panes": panes,
    }))
    .into_response()
}

async fn bench(Path((key, pane)): Path<(String, u64)>) -> Response {
    let info = host::list_panes(&key)
        .await
        .ok()
        .and_then(|ps| ps.into_iter().find(|p| p["pane"].as_u64() == Some(pane)));
    let body = tokio::task::spawn_blocking(move || {
        let resume = info
            .as_ref()
            .and_then(|p| p["resume"].as_str())
            .map(str::to_string);
        let ended = info
            .as_ref()
            .and_then(|p| p["ended"].as_bool())
            .unwrap_or(true);
        json!({
            "session": key,
            "pane": pane,
            "info": info,
            "pulse": mailbox::pulse(&key, pane, resume.as_deref(), ended),
            "surfaces": mailbox::surfaces(&key, pane),
            "timeline": mailbox::timeline(&key, pane, 240),
            "server_ms": now_ms(),
        })
    })
    .await
    .unwrap_or_default();
    Json(body).into_response()
}

#[derive(Deserialize)]
struct SpawnReq {
    cwd: Option<String>,
    /// A command line for the host to type, as `resume` on the wire.
    run: Option<String>,
    /// `claude`: start Claude Code wearing this session's workbench briefing,
    /// the way the desk's launcher does, so what it makes lands on a bench.
    agent: Option<String>,
    model: Option<String>,
    cols: u16,
    rows: u16,
    #[serde(default = "cell_w")]
    cell_width: u16,
    #[serde(default = "cell_h")]
    cell_height: u16,
}
fn cell_w() -> u16 {
    8
}
fn cell_h() -> u16 {
    16
}

async fn spawn(Path(key): Path<String>, Json(req): Json<SpawnReq>) -> Response {
    let run = match (req.agent.as_deref(), req.run) {
        (Some("claude"), _) => {
            let mut line = "claude".to_string();
            if let Some(m) = req.model.as_deref().filter(|m| is_word(m)) {
                line.push_str(&format!(" --model {m}"));
            }
            let briefing = paths::mailbox(&key, 0)
                .parent()
                .map(|d| d.join("briefing.txt"));
            if let Some(b) = briefing.filter(|b| b.exists()) {
                line.push_str(&format!(
                    " --append-system-prompt \"$(cat '{}')\"",
                    b.display()
                ));
            }
            Some(line)
        }
        (Some("codex"), _) => Some("codex".to_string()),
        (Some(other), _) => {
            return refuse(StatusCode::BAD_REQUEST, &format!("no agent called {other}"))
        }
        (None, run) => run.filter(|r| !r.trim().is_empty()),
    };
    let request = json!({
        "verb": "spawn-pane",
        "cwd": req.cwd,
        "resume": run,
        "geom": host::geom(req.cols, req.rows, req.cell_width, req.cell_height),
    });
    match host::ask(&key, request).await {
        Ok(reply) => Json(reply).into_response(),
        Err(e) => refuse(StatusCode::BAD_GATEWAY, &e),
    }
}

fn is_word(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-._[]".contains(c))
}

#[derive(Deserialize)]
struct TermQuery {
    cols: u16,
    rows: u16,
    #[serde(default = "cell_w")]
    cell_width: u16,
    #[serde(default = "cell_h")]
    cell_height: u16,
    /// Take the pane from whichever window holds it. Without this, a pane a
    /// window is showing is refused: taking it freezes the desk's copy.
    #[serde(default)]
    take: bool,
}

async fn term(
    ws: WebSocketUpgrade,
    Path((key, pane)): Path<(String, u64)>,
    Query(q): Query<TermQuery>,
) -> Response {
    let panes = match host::list_panes(&key).await {
        Ok(p) => p,
        Err(e) => return refuse(StatusCode::BAD_GATEWAY, &e),
    };
    let Some(info) = panes.iter().find(|p| p["pane"].as_u64() == Some(pane)) else {
        return refuse(StatusCode::NOT_FOUND, &format!("session {key} has no pane {pane}"));
    };
    if info["ended"].as_bool() == Some(true) {
        return refuse(StatusCode::GONE, "the process in this pane has exited");
    }
    if info["attached"].as_bool() == Some(true) && !q.take {
        return refuse(
            StatusCode::CONFLICT,
            "a window is showing this pane; taking it freezes the desk's copy",
        );
    }
    ws.on_upgrade(move |socket| bridge(socket, key, pane, q))
}

/// One pane's byte stream, carried over one WebSocket: binary frames are the
/// terminal's bytes both ways, text frames are facts about it.
async fn bridge(socket: WebSocket, key: String, pane: u64, q: TermQuery) {
    let (mut tx, mut rx) = socket.split();
    let geom = host::geom(q.cols, q.rows, q.cell_width, q.cell_height);

    let mut control = match host::Control::open(&paths::session_socket(&key)).await {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(Message::Text(json!({"closed":"error","why":e}).to_string().into())).await;
            return;
        }
    };
    let attached = control
        .ask(&json!({"verb":"attach-pane","pane":pane,"geom":geom}))
        .await;
    if let Some(err) = attached
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|r| match r.pointer("/outcome/err") {
            Some(e) => Err(e.as_str().unwrap_or("refused").to_string()),
            None => Ok(()),
        })
        .err()
    {
        let _ = tx.send(Message::Text(json!({"closed":"error","why":err}).to_string().into())).await;
        return;
    }
    let stream = match host::open_stream(&key, pane).await {
        Ok(s) => s,
        Err(e) => {
            let _ = tx.send(Message::Text(json!({"closed":"error","why":e}).to_string().into())).await;
            return;
        }
    };
    let (mut from_pane, mut to_pane) = stream.into_split();
    let _ = tx
        .send(Message::Text(json!({"attached": pane}).to_string().into()))
        .await;

    let mut buf = vec![0u8; 32 * 1024];
    let mut keepalive = tokio::time::interval(Duration::from_secs(20));
    keepalive.tick().await;
    let closed_by_pane = loop {
        tokio::select! {
            n = from_pane.read(&mut buf) => match n {
                Ok(0) | Err(_) => break true,
                Ok(n) => {
                    if tx.send(Message::Binary(buf[..n].to_vec().into())).await.is_err() {
                        break false;
                    }
                }
            },
            msg = rx.next() => match msg {
                Some(Ok(Message::Binary(bytes))) => {
                    if to_pane.write_all(&bytes).await.is_err() {
                        break true;
                    }
                }
                Some(Ok(Message::Text(text))) => {
                    if let Ok(v) = serde_json::from_str::<Value>(&text) {
                        if let (Some(c), Some(r)) = (
                            v.pointer("/resize/cols").and_then(Value::as_u64),
                            v.pointer("/resize/rows").and_then(Value::as_u64),
                        ) {
                            let g = host::geom(c as u16, r as u16, q.cell_width, q.cell_height);
                            let _ = control.ask(&json!({"verb":"resize","pane":pane,"geom":g})).await;
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break false,
                Some(Ok(_)) => {}
            },
            _ = keepalive.tick() => {
                if tx.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break false;
                }
            }
        }
    };
    if closed_by_pane {
        // A closed stream is one of two things and the protocol says ask
        // rather than guess: a pane still listed and not ended was taken.
        let why = match host::list_panes(&key).await {
            Ok(ps) => match ps.iter().find(|p| p["pane"].as_u64() == Some(pane)) {
                Some(p) if p["ended"].as_bool() != Some(true) => "taken",
                _ => "ended",
            },
            Err(_) => "host-gone",
        };
        let _ = tx
            .send(Message::Text(json!({"closed": why}).to_string().into()))
            .await;
        let _ = tx.close().await;
    }
    // Dropping the stream detaches. The terminal keeps running: a client
    // leaving kills nothing, which is the whole point of the host.
}

/// A live feed of "look again" hints. Each is small on purpose — which pane's
/// mailbox moved, which session's pane table changed — and the phone fetches
/// what it is showing. A hint missed is recovered by the next fetch, so none
/// is retried.
async fn events(ws: WebSocketUpgrade, State(gate): State<Arc<Gate>>) -> Response {
    ws.on_upgrade(move |socket| async move {
        let mut feed = gate.events.subscribe();
        gate.watchers.fetch_add(1, Ordering::SeqCst);
        let (mut tx, mut rx) = socket.split();
        let _ = tx
            .send(Message::Text(json!({"type":"hello","server_ms":now_ms()}).to_string().into()))
            .await;
        let mut keepalive = tokio::time::interval(Duration::from_secs(20));
        keepalive.tick().await;
        loop {
            tokio::select! {
                ev = feed.recv() => match ev {
                    Ok(line) => {
                        if tx.send(Message::Text(line.into())).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let _ = tx.send(Message::Text(json!({"type":"resync"}).to_string().into())).await;
                    }
                    Err(_) => break,
                },
                msg = rx.next() => match msg {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                },
                _ = keepalive.tick() => {
                    if tx.send(Message::Ping(Vec::new().into())).await.is_err() { break; }
                }
            }
        }
        gate.watchers.fetch_sub(1, Ordering::SeqCst);
    })
}

/// Turns file and pane-table changes into hints — and only while a phone is
/// listening, so a gateway with nobody connected costs nothing but a sleep.
pub async fn poll(gate: Arc<Gate>) {
    let mut sessions: Vec<String> = vec![];
    let mut sessions_at = 0u64;
    let mut panes_seen: HashMap<String, String> = HashMap::new();
    let mut mail_seen: HashMap<(String, u64), (u64, usize)> = HashMap::new();
    let mut tree_seen: HashMap<String, u64> = HashMap::new();
    let mut tick = tokio::time::interval(Duration::from_millis(900));
    loop {
        tick.tick().await;
        if gate.watchers.load(Ordering::SeqCst) == 0 {
            // Nobody to tell. Forget what was seen, so a phone arriving later
            // is told nothing stale.
            panes_seen.clear();
            mail_seen.clear();
            tree_seen.clear();
            sessions_at = 0;
            continue;
        }
        if now_ms().saturating_sub(sessions_at) > 5000 {
            let live: Vec<String> = host::live_sessions().await.into_iter().map(|s| s.key).collect();
            if live != sessions && sessions_at != 0 {
                let _ = gate.events.send(json!({"type":"sessions"}).to_string());
            }
            sessions = live;
            sessions_at = now_ms();
        }
        for key in &sessions {
            let Ok(panes) = host::list_panes(key).await else { continue };
            let shape: String = panes
                .iter()
                .map(|p| format!("{}:{}:{}:{} ", p["pane"], p["mode"], p["attached"], p["ended"]))
                .collect();
            if panes_seen.get(key).is_some_and(|s| *s != shape) {
                let _ = gate.events.send(json!({"type":"panes","session":key}).to_string());
            }
            panes_seen.insert(key.clone(), shape);

            let ids: Vec<u64> = panes.iter().filter_map(|p| p["pane"].as_u64()).collect();
            let key2 = key.clone();
            let prints = tokio::task::spawn_blocking(move || {
                let tree = std::fs::metadata(paths::session_file(&key2))
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64);
                let mail: Vec<(u64, Option<(u64, usize)>)> = ids
                    .into_iter()
                    .map(|id| (id, mailbox::fingerprint(&key2, id)))
                    .collect();
                (tree, mail)
            })
            .await;
            let Ok((tree, mail)) = prints else { continue };
            if let Some(t) = tree {
                if tree_seen.insert(key.clone(), t).is_some_and(|old| old != t) {
                    let _ = gate.events.send(json!({"type":"tree","session":key}).to_string());
                }
            }
            for (id, print) in mail {
                let Some(print) = print else { continue };
                if mail_seen
                    .insert((key.clone(), id), print)
                    .is_some_and(|old| old != print)
                {
                    let _ = gate
                        .events
                        .send(json!({"type":"mailbox","session":key,"pane":id}).to_string());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_is_equality() {
        assert!(same(b"abc", b"abc"));
        assert!(!same(b"abc", b"abd"));
        assert!(!same(b"abc", b"ab"));
        assert!(!same(b"", b"a"));
    }

    #[test]
    fn a_model_name_is_a_word_not_a_command() {
        assert!(is_word("opus"));
        assert!(is_word("claude-sonnet-5"));
        assert!(!is_word("opus; rm -rf ~"));
        assert!(!is_word("$(id)"));
    }
}
