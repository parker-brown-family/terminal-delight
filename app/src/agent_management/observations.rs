//! Positive, correlated evidence only. No text matching or time-window attribution.
use crate::{identity, state::*};
use serde_json::{json, Value};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

/// Raw identifiers with provenance. Family/display aliases are never substituted.
pub fn models(actor: &Actor) -> Result<ModelFacts> {
    let path = actor
        .transcript
        .value()
        .ok_or("Transcript binding unavailable")?;
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let offset = size.saturating_sub(512 * 1024);
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut body = String::new();
    file.take(512 * 1024)
        .read_to_string(&mut body)
        .map_err(|e| e.to_string())?;
    let mut facts = ModelFacts {
        requested: Fact::Unknown {
            reason: "No requested model in the bounded transcript observation".into(),
        },
        reported: Fact::Unknown {
            reason: "No model response identifier in the bounded transcript observation".into(),
        },
        effort: Fact::Unknown {
            reason: "Effort has not been reported in the bounded transcript observation".into(),
        },
        ..ModelFacts::default()
    };
    for (n, line) in body.lines().enumerate() {
        if (n == 0 && offset > 0) || line.len() > 128 * 1024 {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if actor.run.harness == "claude"
            && v["type"] == "assistant"
            && v["sessionId"] == actor.run.conversation
        {
            if let Some(model) = v.pointer("/message/model").and_then(Value::as_str) {
                facts.reported = Fact::observed(
                    model.into(),
                    format!(
                        "{path}; assistant {}",
                        v["uuid"].as_str().unwrap_or("UUID unavailable")
                    ),
                );
            }
        }
        if actor.run.harness == "codex" && v["type"] == "turn_context" {
            if let Some(model) = v.pointer("/payload/model").and_then(Value::as_str) {
                facts.requested = Fact::observed(
                    model.into(),
                    format!(
                        "{path}; turn_context at {}",
                        v["timestamp"].as_str().unwrap_or("source time unavailable")
                    ),
                );
            }
            if let Some(effort) = v.pointer("/payload/effort").and_then(Value::as_str) {
                facts.effort = Fact::observed(effort.into(), format!("{path}; turn_context"));
            }
        }
    }
    Ok(facts)
}

pub fn claude_consumed(actor: &Actor, id: &str) -> Result<Option<Value>> {
    identity::check_run(&actor.run)?;
    let path = actor
        .transcript
        .value()
        .ok_or("Transcript binding unavailable")?;
    let mut file = File::open(Path::new(path)).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let offset = size.saturating_sub(2 * 1024 * 1024);
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut body = String::new();
    file.take(2 * 1024 * 1024)
        .read_to_string(&mut body)
        .map_err(|e| e.to_string())?;
    for (n, line) in body.lines().enumerate() {
        if (n == 0 && offset > 0) || line.len() > 128 * 1024 {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v["type"] == "user" && v["uuid"] == id && v["sessionId"] == actor.run.conversation {
            return Ok(Some(
                json!({"source":path,"native_message_id":id,"session_id":actor.run.conversation,"source_time":v.get("timestamp"),"observed_at_ms":now_ms(),"scope":"exact user-message UUID in recipient transcript"}),
            ));
        }
    }
    Ok(None)
}
