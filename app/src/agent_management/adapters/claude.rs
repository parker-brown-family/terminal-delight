//! Claude 2.1.296 peer protocol 1. Wire fields only; no vendor code copied.
use super::{connect, peer, Outcome};
use crate::{identity, state::*};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// One callback inbox per TD window process, owned for its entire lifetime.
pub struct Inbox {
    listener: UnixListener,
    path: PathBuf,
}

impl Inbox {
    pub fn bind() -> Result<Self> {
        let dir = PathBuf::from(format!("/run/user/{}/cc-socks", unsafe { libc::geteuid() }));
        Self::bind_directory(&dir)
    }

    #[cfg(test)]
    pub(crate) fn bind_fixture(dir: &Path) -> Result<Self> {
        Self::bind_directory(dir)
    }

    fn bind_directory(dir: &Path) -> Result<Self> {
        use std::os::unix::fs::MetadataExt;
        let m = fs::symlink_metadata(dir).map_err(|e| e.to_string())?;
        if !m.is_dir()
            || m.file_type().is_symlink()
            || m.uid() != unsafe { libc::geteuid() }
            || m.mode() & 0o077 != 0
        {
            return Err("Claude callback directory is not private".into());
        }
        let path = dir.join(format!("{}.sock", std::process::id()));
        // Never remove another live listener or publish a fake Claude registry record.
        let listener =
            UnixListener::bind(&path).map_err(|e| format!("Cannot bind TD callback inbox: {e}"))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        Ok(Self { listener, path })
    }

    pub fn send(
        &self,
        home: &Path,
        actor: &Actor,
        id: &str,
        text: &str,
        dispatch_before_ms: u64,
    ) -> Outcome {
        let prepare = || -> Result<std::os::unix::net::UnixStream> {
            identity::check_run(&actor.run)?;
            let stream = connect(Path::new(&actor.run.endpoint))?;
            if peer(&stream)? != actor.run.process {
                return Err("Claude socket peer differs from selected process".into());
            }
            Ok(stream)
        };
        let mut stream = match prepare() {
            Ok(s) => s,
            Err(e) => return Outcome::rejected(e),
        };
        let hash = format!("{:x}", Sha256::digest(actor.run.endpoint.as_bytes()));
        let key_path = home
            .join(".claude/sessions")
            .join(format!("{}.{hash}.key", actor.run.process.pid));
        // Credentials are only used on the native connection, never returned or journalled.
        let auth = || -> Result<()> {
            let bytes =
                fs::read(key_path).map_err(|_| "Claude inbox authentication key unavailable")?;
            if bytes.len() > 4096 {
                return Err("Invalid native authentication record".into());
            }
            let key: Value = serde_json::from_slice(&bytes)
                .map_err(|_| "Invalid native authentication record")?;
            let token = key["peerToken"]
                .as_str()
                .filter(|s| s.len() == 32 && s.bytes().all(|c| c.is_ascii_hexdigit()))
                .ok_or("Invalid native authentication record")?;
            let frame = serde_json::to_vec(&json!({"type":"auth","token":token}))
                .map_err(|_| "Cannot encode native authentication")?;
            stream
                .write_all(&frame)
                .and_then(|_| stream.write_all(b"\n"))
                .map_err(|_| "Native authentication connection failed")?;
            Ok(())
        };
        if let Err(e) = auth() {
            return Outcome::rejected(e);
        }
        if now_ms() >= dispatch_before_ms {
            return Outcome {
                kind: "expired_before_dispatch",
                evidence: json!({"reason":"Deadline passed before native message write"}),
            };
        }
        let frame = json!({"type":"user","session_id":actor.run.conversation,"uuid":id,"msg_id":id,"priority":"next","from":format!("uds:{}",self.path.display()),"message":{"content":text}});
        let bytes = match serde_json::to_vec(&frame) {
            Ok(b) => b,
            Err(e) => return Outcome::rejected(e.to_string()),
        };
        if stream
            .write_all(&bytes)
            .and_then(|_| stream.write_all(b"\n"))
            .is_err()
        {
            return Outcome::unconfirmed("Native write failed after delivery may have begun");
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut diagnostics = Vec::<Value>::new();
        while Instant::now() < deadline {
            match self.listener.accept() {
                Ok((callback, _)) => {
                    let callback_peer = peer(&callback);
                    if callback_peer.as_ref().ok() != Some(&actor.run.process) {
                        if diagnostics.len() < 64 {
                            diagnostics.push(json!({"kind":"callback_peer_mismatch","observed_peer":callback_peer.ok()}));
                        }
                        continue;
                    }
                    let _ = callback.set_read_timeout(Some(Duration::from_millis(200)));
                    let mut reader = BufReader::new(callback.take(128 * 1024));
                    let mut line = String::new();
                    for _ in 0..64 {
                        if Instant::now() >= deadline {
                            break;
                        }
                        line.clear();
                        if !matches!(reader.read_line(&mut line),Ok(n) if n>0) {
                            break;
                        }
                        let Ok(v) = serde_json::from_str::<Value>(&line) else {
                            continue;
                        };
                        if v["type"] != "control"
                            || v["action"] != "peer_message_status"
                            || v["orig_msg_id"] != id
                        {
                            if diagnostics.len() < 64 {
                                diagnostics.push(json!({"kind":"uncorrelated_native_callback","orig_msg_id":v["orig_msg_id"].as_str().filter(|s|s.len()<=128),"action":v["action"].as_str().filter(|s|s.len()<=64),"status":v["status"].as_str().filter(|s|s.len()<=64)}));
                            }
                            continue;
                        }
                        let status = v["status"].as_str().unwrap_or("unknown");
                        let kind = match status {
                            "delivered" => "native_accepted",
                            "held" => "native_held",
                            "denied" | "expired" | "refused" | "dropped" => "native_refused",
                            _ => "delivery_unconfirmed",
                        };
                        return Outcome {
                            kind,
                            evidence: json!({"protocol":"claude-peer-1","native_message_id":id,"status":status,"detail":v["status_detail"].as_str().map(|s|s.chars().take(512).collect::<String>()),"diagnostics":diagnostics,"scope":"native peer inbox; consumption and work success remain unproven"}),
                        };
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(_) => break,
            }
        }
        Outcome {
            kind: "delivery_unconfirmed",
            evidence: json!({"reason":"No correlated native receipt within five seconds; no automatic resend","diagnostics":diagnostics}),
        }
    }
}

impl Drop for Inbox {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
