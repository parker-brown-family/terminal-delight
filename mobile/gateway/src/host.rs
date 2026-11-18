//! A client for the session host protocol, version 1.
//!
//! `docs/protocol/session-host-v1.md` is the contract. This speaks it as plain
//! JSON lines rather than through the app's types, so a field added to the host
//! within a version passes straight through to the phone instead of being
//! dropped by a struct that did not know it.
//!
//! Every control request here opens its own connection. A unix socket connect
//! costs microseconds, and one connection per question means a slow answer can
//! never leave a later request reading the wrong reply.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::UnixStream;
use tokio::time::timeout;

use crate::paths;

pub const PROTO: u64 = 1;
const ANSWER_WITHIN: Duration = Duration::from_secs(3);

/// A live session, as its host describes itself.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Session {
    pub key: String,
    /// The host's pid. `None` when an older host sent `0`, which the protocol
    /// says is not a pid.
    pub host: Option<u64>,
    pub panes: u64,
    pub attended: bool,
}

/// One control connection, kept open for as long as its owner needs it — the
/// terminal bridge holds one to send `resize` while the stream runs.
pub struct Control {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
}

impl Control {
    pub async fn open(socket: &Path) -> Result<Self, String> {
        let stream = timeout(ANSWER_WITHIN, UnixStream::connect(socket))
            .await
            .map_err(|_| format!("{} did not accept within 3s", socket.display()))?
            .map_err(|e| format!("{}: {e}", socket.display()))?;
        let (r, w) = stream.into_split();
        Ok(Self {
            reader: BufReader::new(r),
            writer: w,
        })
    }

    /// Send one request and read its one reply.
    pub async fn ask(&mut self, request: &Value) -> Result<Value, String> {
        let mut line = serde_json::to_string(request).map_err(|e| e.to_string())?;
        line.push('\n');
        self.writer
            .write_all(line.as_bytes())
            .await
            .map_err(|e| format!("write: {e}"))?;
        loop {
            let mut answer = String::new();
            let n = timeout(ANSWER_WITHIN, self.reader.read_line(&mut answer))
                .await
                .map_err(|_| "the host did not answer within 3s".to_string())?
                .map_err(|e| format!("read: {e}"))?;
            if n == 0 {
                return Err("the host closed the connection".into());
            }
            let v: Value = serde_json::from_str(&answer)
                .map_err(|e| format!("unreadable reply: {e}"))?;
            // A push answers no request. This connection never sends `watch`,
            // so none should arrive — but a reader that took one for a reply
            // would read one answer behind for the rest of its life.
            if v.get("push").is_some() {
                continue;
            }
            if v.get("reply").and_then(Value::as_str) == Some("error") {
                return Err(v
                    .get("msg")
                    .and_then(Value::as_str)
                    .unwrap_or("the host refused without saying why")
                    .to_string());
            }
            return Ok(v);
        }
    }
}

/// Ask one question on a fresh connection.
pub async fn ask(session: &str, request: Value) -> Result<Value, String> {
    let mut c = Control::open(&paths::session_socket(session)).await?;
    c.ask(&request).await
}

/// Every session whose host answers `hello` in our protocol.
///
/// The runtime directory keeps sockets for hosts long gone (a probe's, a
/// test's), so a socket file is a candidate and only an answer is a session.
pub async fn live_sessions() -> Vec<Session> {
    let Ok(dir) = std::fs::read_dir(paths::runtime_dir()) else {
        return vec![];
    };
    let candidates: Vec<(String, PathBuf)> = dir
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let key = name.strip_prefix("session-")?.strip_suffix(".sock")?;
            Some((key.to_string(), e.path()))
        })
        .collect();
    let probes = candidates.into_iter().map(|(key, path)| async move {
        let mut c = timeout(Duration::from_millis(400), Control::open(&path))
            .await
            .ok()?
            .ok()?;
        let hello = timeout(
            Duration::from_millis(600),
            c.ask(&json!({"verb":"hello","proto":PROTO,"kind":"tool"})),
        )
        .await
        .ok()?
        .ok()?;
        Some(Session {
            key,
            host: hello.get("host").and_then(Value::as_u64).filter(|p| *p != 0),
            panes: hello.get("panes").and_then(Value::as_u64).unwrap_or_default(),
            attended: hello
                .get("attended")
                .and_then(Value::as_bool)
                .unwrap_or_default(),
        })
    });
    let mut live: Vec<Session> = futures_util::future::join_all(probes)
        .await
        .into_iter()
        .flatten()
        .collect();
    // The session a person is most likely looking at first: the one with the
    // most panes, then by name so the order is stable between polls.
    live.sort_by(|a, b| b.panes.cmp(&a.panes).then(a.key.cmp(&b.key)));
    live
}

pub async fn list_panes(session: &str) -> Result<Vec<Value>, String> {
    let reply = ask(session, json!({"verb":"list-panes"})).await?;
    reply
        .get("panes")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| "list-panes answered without a pane list".into())
}

/// Open a pane's byte stream: the greeting, then the terminal's own bytes.
/// The caller must have sent `attach-pane` first, on a control connection.
pub async fn open_stream(session: &str, pane: u64) -> Result<UnixStream, String> {
    let mut s = UnixStream::connect(paths::session_socket(session))
        .await
        .map_err(|e| format!("stream: {e}"))?;
    s.write_all(format!("stream {pane}\n").as_bytes())
        .await
        .map_err(|e| format!("stream greeting: {e}"))?;
    Ok(s)
}

pub fn geom(cols: u16, rows: u16, cell_width: u16, cell_height: u16) -> Value {
    json!({"cols":cols,"rows":rows,"cell_width":cell_width,"cell_height":cell_height})
}
