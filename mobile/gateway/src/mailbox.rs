//! Reading a pane's mailbox the way the desk's bench reads it.
//!
//! Two sources, both files the window and the agent's hooks already write:
//!
//! - **Surfaces** (`*.json`, TDSP): what the agent MADE. Applied oldest first
//!   by modification time, filename breaking ties, with `present` replacing a
//!   surface by id, `update` merging into it and `retire` taking it away —
//!   the window's sweep, restated.
//! - **The channel** (`inbound.jsonl`, `outbound.jsonl`, TDAC 0.2): what the
//!   person typed, what the agent asked and said back, what the window sent.
//!
//! Nothing here writes. The gateway is a reader of the mailbox; the one thing
//! that may deliver into a pane is the byte stream, and that is `term.rs`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde_json::{json, Map, Value};

use crate::paths;

/// How much of a journal is read from its tail. A busy pane's inbound journal
/// rotates at 2 MiB; the last 384 KiB holds hundreds of turns, and the phone
/// shows tens.
const JOURNAL_TAIL: u64 = 384 * 1024;

fn mtime_ms(path: &Path) -> Option<u64> {
    let m = fs::metadata(path).ok()?.modified().ok()?;
    Some(m.duration_since(UNIX_EPOCH).ok()?.as_millis() as u64)
}

/// The current surfaces of a pane, newest first, each as
/// `{file, mtime_ms, doc}` with `doc` the TDSP document after every op.
pub fn surfaces(session: &str, pane: u64) -> Vec<Value> {
    let dir = paths::mailbox(session, pane);
    let Ok(entries) = fs::read_dir(&dir) else {
        return vec![];
    };
    let mut files: Vec<(u64, String)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            // `bench.json` is the window's liveness marker, not a surface.
            if !name.ends_with(".json") || name == "bench.json" {
                return None;
            }
            Some((mtime_ms(&e.path())?, name))
        })
        .collect();
    files.sort();

    // id → (file, mtime, doc). BTreeMap only so ties come out stable.
    let mut live: BTreeMap<String, (String, u64, Value)> = BTreeMap::new();
    for (mtime, name) in files {
        let Ok(text) = fs::read_to_string(dir.join(&name)) else {
            continue;
        };
        // Half a write is not a surface; the next sweep will see it whole.
        let Ok(Value::Object(doc)) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let id = surface_id(&doc, &name);
        match doc.get("op").and_then(Value::as_str).unwrap_or("present") {
            "retire" => {
                live.remove(&id);
            }
            "update" => {
                let merged = match live.remove(&id) {
                    Some((_, _, Value::Object(mut old))) => {
                        merge(&mut old, &doc);
                        Value::Object(old)
                    }
                    _ => Value::Object(doc),
                };
                live.insert(id, (name, mtime, merged));
            }
            _ => {
                live.insert(id, (name, mtime, Value::Object(doc)));
            }
        }
    }
    let mut out: Vec<Value> = live
        .into_iter()
        .map(|(id, (file, mtime, doc))| json!({"id": id, "file": file, "mtime_ms": mtime, "doc": doc}))
        .collect();
    out.sort_by_key(|s| std::cmp::Reverse(s["mtime_ms"].as_u64().unwrap_or_default()));
    out
}

/// TDSP: an explicit `id` wins; otherwise kind plus title, which is what the
/// window derives. The filename is the last resort, so two untitled drops are
/// still two surfaces.
fn surface_id(doc: &Map<String, Value>, file: &str) -> String {
    if let Some(id) = doc.get("id").and_then(Value::as_str) {
        return id.to_string();
    }
    match (
        doc.get("kind").and_then(Value::as_str),
        doc.get("title").and_then(Value::as_str),
    ) {
        (Some(k), Some(t)) => format!("{k}:{t}"),
        _ => format!("file:{file}"),
    }
}

fn merge(into: &mut Map<String, Value>, from: &Map<String, Value>) {
    for (k, v) in from {
        match (into.get_mut(k), v) {
            (Some(Value::Object(a)), Value::Object(b)) => merge(a, b),
            _ => {
                into.insert(k.clone(), v.clone());
            }
        }
    }
}

/// The tail of a JSON-lines journal, parsed. The first line of a tail read is
/// usually cut in half, and any line that does not parse is skipped — the
/// channel's own rule for a line caught mid-append.
fn journal(path: &Path) -> Vec<Value> {
    let Ok(mut f) = fs::File::open(path) else {
        return vec![];
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or_default();
    if len > JOURNAL_TAIL {
        let _ = f.seek(SeekFrom::Start(len - JOURNAL_TAIL));
    }
    let mut buf = Vec::new();
    let _ = f.read_to_end(&mut buf);
    String::from_utf8_lossy(&buf)
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v.is_object())
        .collect()
}

/// Both journals, merged into one timeline, oldest first, each record tagged
/// with the side it came from.
pub fn timeline(session: &str, pane: u64, limit: usize) -> Vec<Value> {
    let dir = paths::mailbox(session, pane);
    let mut all: Vec<Value> = journal(&dir.join("inbound.jsonl"))
        .into_iter()
        .map(|mut v| {
            v["side"] = json!("in");
            if v["type"] == "prompt" && from_harness(v["text"].as_str().unwrap_or_default()) {
                v["harness"] = json!(true);
            }
            v
        })
        .chain(journal(&dir.join("outbound.jsonl")).into_iter().map(|mut v| {
            v["side"] = json!("out");
            v
        }))
        .collect();
    all.sort_by_key(|v| v.get("at_ms").and_then(Value::as_u64).unwrap_or_default());
    let skip = all.len().saturating_sub(limit);
    all.into_iter().skip(skip).collect()
}

/// What a pane is doing, from its channel records and its transcript.
///
/// `unknown` is a real answer and the default: a pane with no hooked agent has
/// no records, and saying `idle` about it would be a guess dressed as a
/// reading.
pub fn pulse(session: &str, pane: u64, resume: Option<&str>, ended: bool) -> Value {
    let records = journal(&paths::mailbox(session, pane).join("inbound.jsonl"));
    let at = |v: &Value| v.get("at_ms").and_then(Value::as_u64);

    let mut last_prompt: Option<&Value> = None;
    let mut last_said: Option<&Value> = None;
    let mut last_reply: Option<&Value> = None;
    let mut last_needs: Option<&Value> = None;
    let mut asked: HashMap<String, &Value> = HashMap::new();
    let mut settled: HashSet<String> = HashSet::new();
    for r in &records {
        match r.get("type").and_then(Value::as_str) {
            Some("prompt") => {
                // Every prompt starts a turn, but only a person's is shown as
                // what the person asked.
                last_prompt = Some(r);
                if !from_harness(r.get("text").and_then(Value::as_str).unwrap_or_default()) {
                    last_said = Some(r);
                }
            }
            Some("reply") => last_reply = Some(r),
            Some("notify") => {
                let t = r.get("notification_type").and_then(Value::as_str);
                if matches!(t, Some("permission_prompt" | "agent_needs_input")) {
                    last_needs = Some(r);
                }
            }
            Some("question") => {
                if let Some(id) = r.get("tool_use_id").and_then(Value::as_str) {
                    asked.insert(id.to_string(), r);
                }
            }
            Some("answered") => {
                if let Some(id) = r.get("tool_use_id").and_then(Value::as_str) {
                    settled.insert(id.to_string());
                }
            }
            _ => {}
        }
    }
    let prompt_at = last_prompt.and_then(at);
    let reply_at = last_reply.and_then(at);
    let transcript = resume.and_then(transcript_ms);

    // A question still open after the person's latest turn is the loudest
    // thing a pane can be doing.
    let open_question = asked
        .iter()
        .filter(|(id, _)| !settled.contains(*id))
        .filter_map(|(_, q)| Some((at(q)?, *q)))
        .filter(|(t, _)| prompt_at.is_none_or(|p| *t >= p))
        .max_by_key(|(t, _)| *t)
        .map(|(_, q)| q);

    // A permission prompt holds until something moves after it. The hooks do
    // not record the person approving, but the transcript is written the
    // moment the agent carries on, so a transcript newer than the prompt says
    // it was answered.
    let needs = last_needs.filter(|n| {
        let t = at(n).unwrap_or_default();
        prompt_at.is_none_or(|p| t >= p)
            && reply_at.is_none_or(|r| t >= r)
            && transcript.is_none_or(|m| m <= t + 1500)
    });

    let (state, why) = if ended {
        ("ended", "the process in this pane has exited".to_string())
    } else if let Some(q) = open_question {
        let text = q
            .pointer("/questions/0/question")
            .and_then(Value::as_str)
            .unwrap_or("a question");
        ("needs-you", format!("asked: {text}"))
    } else if let Some(n) = needs {
        let text = n
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("waiting for permission");
        ("needs-you", text.to_string())
    } else if prompt_at.is_some() && prompt_at > reply_at {
        ("working", "on the turn you gave it".to_string())
    } else if reply_at.is_some() {
        ("idle", "finished its turn".to_string())
    } else {
        ("unknown", "no channel records from this pane".to_string())
    };

    let activity = [prompt_at, reply_at, transcript]
        .into_iter()
        .flatten()
        .max();
    json!({
        "state": state,
        "why": why,
        "prompt": last_said.map(|p| json!({"text": p.get("text"), "at_ms": at(p)})),
        "reply": last_reply.map(|r| json!({"text": r.get("text"), "at_ms": at(r)})),
        "transcript_ms": transcript,
        "activity_ms": activity,
    })
}

/// Whether a prompt record was typed by the harness rather than a person.
///
/// `UserPromptSubmit` fires for everything that enters the conversation as a
/// user turn, and Claude Code submits some of those itself: a background task
/// reporting back, a slash command's expansion, a shell escape's output. Shown
/// as "you asked", each is a sentence the person never said.
pub fn from_harness(text: &str) -> bool {
    // Measured 2026-09-25 across session 1's 1,050 prompt records: 315 opened
    // `<task-notification>`, 86 `<cross-session-message`, 5 `<agent-message`.
    // The desk's own table (app/src/benchstore.rs, SYSTEM_ENVELOPES) has the
    // first two; the rest are here because the harness sends them too.
    const TAGS: [&str; 11] = [
        "<task-notification>",
        "<cross-session-message",
        "<agent-message ",
        "<teammate-message ",
        "[SYSTEM NOTIFICATION",
        "<system-reminder>",
        "<local-command-",
        "<command-name>",
        "<command-message>",
        "<bash-input>",
        "<bash-stdout>",
    ];
    let t = text.trim_start();
    TAGS.iter().any(|tag| t.starts_with(tag))
}

/// The newest `response` surface, cut down to what a wall card shows.
pub fn latest_response(surfaces: &[Value]) -> Option<Value> {
    let s = surfaces
        .iter()
        .find(|s| s.pointer("/doc/kind").and_then(Value::as_str) == Some("response"))?;
    let m = s.pointer("/doc/model");
    Some(json!({
        "title": s.pointer("/doc/title"),
        "brief": m.and_then(|m| m.get("brief")).or_else(|| m.and_then(|m| m.get("tldr"))),
        "layman": m.and_then(|m| m.get("layman")),
        "escalation": m.and_then(|m| m.get("escalation")),
        "mtime_ms": s.get("mtime_ms"),
    }))
}

/// When Claude Code last wrote this conversation's transcript. The id is the
/// argument after `--resume`; the directory it sits in is named after a
/// working directory we would otherwise have to re-derive, so every project
/// directory is asked. Codex keeps its own store and answers `None` here,
/// which is an honest absence, not a zero.
fn transcript_ms(resume: &str) -> Option<u64> {
    let mut words = resume.split_whitespace();
    let id = loop {
        match words.next()? {
            "--resume" | "-r" => break words.next()?,
            _ => continue,
        }
    };
    if id.len() < 32 || !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return None;
    }
    let file = format!("{id}.jsonl");
    fs::read_dir(paths::claude_projects())
        .ok()?
        .flatten()
        .find_map(|d| mtime_ms(&d.path().join(&file)))
}

/// A cheap fingerprint of a mailbox — the newest modification time and the
/// number of files — so the poller can tell a phone "look again" without
/// parsing anything.
pub fn fingerprint(session: &str, pane: u64) -> Option<(u64, usize)> {
    let entries = fs::read_dir(paths::mailbox(session, pane)).ok()?;
    let mut newest = 0;
    let mut count = 0;
    for e in entries.flatten() {
        let name = e.file_name();
        // The window rewrites its liveness marker every second while the bench
        // is on screen; that is not news about the pane.
        if name == "bench.json" {
            continue;
        }
        count += 1;
        if let Some(t) = mtime_ms(&e.path()) {
            newest = newest.max(t);
        }
    }
    Some((newest, count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resume_line_names_its_conversation() {
        // Not a real transcript, so the answer is absent — but the parse must
        // not mistake a flag for the id.
        assert_eq!(transcript_ms("claude --model opus"), None);
        assert_eq!(transcript_ms("claude --resume not-an-id"), None);
    }

    #[test]
    fn the_harness_speaking_is_not_the_person() {
        assert!(from_harness("<task-notification>\n<task-id>b1</task-id>"));
        assert!(from_harness("  <command-name>/clear</command-name>"));
        assert!(from_harness("<agent-message from=\"a8131\">\n[Subagent hand-back]"));
        assert!(from_harness("<cross-session-message from=\"u1\">hi</cross-session-message>"));
        assert!(!from_harness("<pasted_content id=\"11f4\">a person's paste"));
        assert!(!from_harness("make the <div> wider"));
        assert!(!from_harness("Okay, we are finally going to build it"));
    }

    #[test]
    fn update_merges_and_leaves_the_rest() {
        let mut a = serde_json::from_str::<Value>(r#"{"model":{"layman":"x","brief":"y"}}"#)
            .unwrap()
            .as_object()
            .unwrap()
            .clone();
        let b = serde_json::from_str::<Value>(r#"{"model":{"brief":"z"}}"#)
            .unwrap()
            .as_object()
            .unwrap()
            .clone();
        merge(&mut a, &b);
        assert_eq!(a["model"]["layman"], "x");
        assert_eq!(a["model"]["brief"], "z");
    }
}
