//! Connect to an already-running app server over WebSocket/UDS. Never bootstrap it.
use super::{connect, peer, Outcome};
use crate::{identity, state::*};
use serde_json::{json, Value};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};

pub struct Connection {
    socket: WebSocket<UnixStream>,
    next_id: u64,
    pub runtime: ProcessKey,
    pub initialize: Value,
}

impl Connection {
    pub fn open(path: &Path) -> Result<Self> {
        let stream = connect(path)?;
        let runtime = peer(&stream)?;
        let config = tungstenite::protocol::WebSocketConfig::default()
            .max_message_size(Some(256 * 1024))
            .max_frame_size(Some(256 * 1024));
        let (socket, _) =
            tungstenite::client::client_with_config("ws://localhost/", stream, Some(config))
                .map_err(|e| e.to_string())?;
        let mut c = Self {
            socket,
            next_id: 0,
            runtime,
            initialize: Value::Null,
        };
        c.initialize=c.call("initialize",json!({"clientInfo":{"name":"terminal_delight_management","version":"0.1"},"capabilities":{"experimentalApi":true}}))?;
        let agent = c.initialize["userAgent"]
            .as_str()
            .ok_or("Codex runtime did not identify its version")?;
        if !agent.starts_with("codex-tui/0.163.0 ") {
            return Err(
                "This Codex app server version has not passed TD protocol conformance".into(),
            );
        }
        c.socket
            .send(Message::text(json!({"method":"initialized"}).to_string()))
            .map_err(|e| e.to_string())?;
        Ok(c)
    }

    pub fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        self.socket
            .send(Message::text(
                json!({"id":id,"method":method,"params":params}).to_string(),
            ))
            .map_err(|e| e.to_string())?;
        let end = Instant::now() + Duration::from_secs(5);
        for _ in 0..64 {
            if Instant::now() >= end {
                return Err("Native RPC deadline exceeded".into());
            }
            self.socket
                .get_mut()
                .set_read_timeout(Some(
                    end.saturating_duration_since(Instant::now())
                        .max(Duration::from_millis(1)),
                ))
                .map_err(|e| e.to_string())?;
            let frame = self.socket.read().map_err(|e| e.to_string())?;
            let Message::Text(text) = frame else {
                continue;
            };
            let v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            if v.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = v.get("error") {
                let message = error["message"]
                    .as_str()
                    .unwrap_or("reason unavailable")
                    .chars()
                    .take(512)
                    .collect::<String>();
                return Err(format!(
                    "Native RPC refused ({}): {message}",
                    error.get("code").unwrap_or(&Value::Null)
                ));
            }
            return v
                .get("result")
                .cloned()
                .ok_or("Native RPC has no result".into());
        }
        Err("Native RPC frame budget exceeded".into())
    }

    pub fn loaded(&mut self, thread: &str) -> Result<bool> {
        let mut cursor = Value::Null;
        for _ in 0..16 {
            let v = self.call("thread/loaded/list", json!({"limit":256,"cursor":cursor}))?;
            if v["data"]
                .as_array()
                .ok_or("Malformed loaded-thread list")?
                .iter()
                .any(|id| id.as_str() == Some(thread))
            {
                return Ok(true);
            }
            cursor = v.get("nextCursor").cloned().unwrap_or(Value::Null);
            if cursor.is_null() {
                return Ok(false);
            }
        }
        Err("Loaded-thread coverage exceeded page budget".into())
    }

    pub fn send(
        &mut self,
        actor: &Actor,
        id: &str,
        text: &str,
        dispatch_before_ms: u64,
    ) -> Outcome {
        if let Err(e) = identity::check_run(&actor.run) {
            return Outcome::rejected(e);
        }
        if let Err(e) = self.check_routing(actor) {
            return Outcome {
                kind: "native_unsupported",
                evidence: json!({"reason":e,"runtime":self.runtime}),
            };
        }
        match self.loaded(&actor.run.conversation) {
            Ok(true) => {}
            Ok(false) => {
                return Outcome::rejected(
                    "Selected Codex conversation is not loaded; automatic resume is forbidden",
                )
            }
            Err(e) => return Outcome::rejected(e),
        }
        if now_ms() >= dispatch_before_ms {
            return Outcome {
                kind: "expired_before_dispatch",
                evidence: json!({"reason":"Deadline passed before native queue write"}),
            };
        }
        let queued=self.call("thread/queue/add",json!({"threadId":actor.run.conversation,"clientUserMessageId":id,"input":[{"type":"text","text":text,"text_elements":[]}]}));
        // Even an RPC error could follow a successful write. Until server dedupe is proven, quarantine it.
        let queued = match queued {
            Ok(v) => v,
            Err(e) => return Outcome::unconfirmed(e),
        };
        let Some(queue_id) = queued
            .pointer("/queuedSubmission/id")
            .or_else(|| queued.pointer("/submission/id"))
            .or_else(|| queued.get("id"))
            .and_then(Value::as_str)
        else {
            return Outcome::unconfirmed(
                "Queue accepted but its native identifier was not recognized",
            );
        };
        let started = self.call(
            "thread/queue/start",
            json!({"threadId":actor.run.conversation,"queuedSubmissionId":queue_id}),
        );
        let start = match started {
            Ok(v) => json!({"confirmed":true,"turn_id":v.pointer("/turn/id")}),
            Err(e) => json!({"confirmed":false,"reason":e}),
        };
        Outcome {
            kind: "native_accepted",
            evidence: json!({"protocol":"codex-app-server-0.163.0","client_user_message_id":id,"queued_submission_id":queue_id,"queue_start":start,"runtime":self.runtime,"scope":"native queue ownership; task completion remains unproven"}),
        }
    }

    pub fn routing(&self) -> Result<Value> {
        let bytes = std::fs::read(format!("/proc/{}/environ", self.runtime.pid))
            .map_err(|e| e.to_string())?;
        if bytes.len() > 1024 * 1024 {
            return Err("Runtime environment exceeds observation limit".into());
        }
        let get = |key: &[u8]| -> Option<String> {
            bytes
                .split(|b| *b == 0)
                .find_map(|entry| entry.strip_prefix(key))
                .map(|s| String::from_utf8_lossy(s).into_owned())
        };
        Ok(
            json!({"td_session":get(b"TD_SESSION="),"td_pane_id":get(b"TD_PANE_ID="),"source":format!("/proc/{}/environ; allowlisted routing fields only",self.runtime.pid)}),
        )
    }

    pub fn check_routing(&self, actor: &Actor) -> Result<()> {
        let routing = self.routing()?;
        if routing["td_session"].as_str() != Some(&actor.location.td_session)
            || routing["td_pane_id"].as_str() != Some(actor.location.pane.to_string().as_str())
        {
            return Err("Shared Codex runtime carries another or unknown TD session/pane environment. Native assignment is unsupported until per-thread tool routing is evidenced.".into());
        }
        Ok(())
    }

    pub fn observe(&mut self, actor: &Actor, id: &str) -> Result<Option<Value>> {
        identity::check_run(&actor.run)?;
        if !self.loaded(&actor.run.conversation)? {
            return Err("Recipient conversation is no longer loaded".into());
        }
        let v=self.call("thread/turns/list",json!({"threadId":actor.run.conversation,"limit":1,"itemsView":"full","sortDirection":"desc"}))?;
        for turn in v["data"].as_array().ok_or("Malformed turn page")? {
            let items = turn["items"]
                .as_array()
                .ok_or("Turn item coverage unavailable")?;
            if !items.iter().any(|item| {
                item["type"] == "userMessage" && (item["clientId"] == id || item["id"] == id)
            }) {
                continue;
            }
            let output = items
                .iter()
                .filter(|item| item["type"] == "agentMessage" && item["phase"] == "final_answer")
                .map(|item| json!({"native_item_id":item["id"],"text":item["text"]}))
                .collect::<Vec<_>>();
            return Ok(Some(
                json!({"source":"thread/turns/list; exact native userMessage clientId/id","thread_id":actor.run.conversation,"client_user_message_id":id,"turn_id":turn["id"],"status":turn["status"],"started_at":turn["startedAt"],"completed_at":turn["completedAt"],"output":output,"runtime":self.runtime}),
            ));
        }
        Ok(None)
    }
}
