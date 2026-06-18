//! Read-only transcript tailer for the MCP event feed.
//!
//! An agent pane already points at its own conversation transcript on disk (the
//! claude/codex JSONL that `session` is lifted from). This module reads the tail
//! of that file and pulls out the *structured* tool calls — the reliable event
//! source the design picked over scraping the rendered screen.
//!
//! Everything here is std + serde_json, no gpui and no PTY, so it is pure I/O
//! and unit-tested against fixture transcripts. It only ever *reads*.

use crate::mcp::ToolEvent;
use crate::session;
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Cap each read to the final chunk of a transcript — a long conversation can
/// be many MB, but the recent tool calls are always at the end.
const TAIL_BYTES: u64 = 256 * 1024;

/// Resolve the transcript file for a pane from its mode label + cwd. `None` for
/// non-agent panes (a plain shell has no transcript) or an unreadable cwd.
pub fn transcript_for(mode: &str, cwd: Option<&str>, home: &Path) -> Option<PathBuf> {
    let cwd = cwd?;
    match mode {
        "CLAUDE" => session::claude_transcript(cwd, home),
        "CODEX" => session::codex_transcript(cwd, home),
        _ => None,
    }
}

/// The last `limit` structured tool-call events from a transcript JSONL, oldest
/// first. Tolerant: an unparseable or irrelevant line is skipped, never fatal;
/// a missing file yields an empty vec.
pub fn tail_tool_events(path: &Path, limit: usize) -> Vec<ToolEvent> {
    let body = read_tail(path).unwrap_or_default();
    let mut events: Vec<ToolEvent> = body.lines().flat_map(parse_line).collect();
    let n = events.len();
    if n > limit {
        events.drain(0..n - limit);
    }
    events
}

/// Read only the final `TAIL_BYTES` of the file, dropping the leading partial
/// line if we seeked into the middle of one.
fn read_tail(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    let (start, partial) = if len > TAIL_BYTES {
        (len - TAIL_BYTES, true)
    } else {
        (0, false)
    };
    f.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    let mut s = String::from_utf8_lossy(&buf).into_owned();
    if partial {
        if let Some(nl) = s.find('\n') {
            s.drain(0..=nl);
        }
    }
    Ok(s)
}

/// Parse one JSONL line into zero or more tool events. Handles both the Claude
/// Code shape (assistant turn carrying `tool_use` content blocks) and the Codex
/// rollout shape (a `function_call` response item, nested or flat).
fn parse_line(line: &str) -> Vec<ToolEvent> {
    let line = line.trim();
    if line.is_empty() {
        return vec![];
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return vec![];
    };
    let ts = v
        .get("timestamp")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // Claude Code: assistant turn, one event per tool_use content block.
    if v.get("type").and_then(Value::as_str) == Some("assistant") {
        if let Some(content) = v.pointer("/message/content").and_then(Value::as_array) {
            let evs: Vec<ToolEvent> = content
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
                .map(|b| ToolEvent {
                    ts: ts.clone(),
                    tool: b
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_string(),
                    summary: summarize(b.get("input").unwrap_or(&Value::Null)),
                })
                .collect();
            if !evs.is_empty() {
                return evs;
            }
        }
    }

    // Codex: a function_call, either under `payload` or at the top level.
    let call = v
        .pointer("/payload")
        .filter(|p| p.get("type").and_then(Value::as_str) == Some("function_call"))
        .or_else(|| (v.get("type").and_then(Value::as_str) == Some("function_call")).then_some(&v));
    if let Some(call) = call {
        let tool = call
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_string();
        let summary = match call.get("arguments") {
            // Codex stores arguments as a JSON *string* — parse it for a gist.
            Some(Value::String(s)) => summarize(&serde_json::from_str(s).unwrap_or(Value::Null)),
            Some(other) => summarize(other),
            None => String::new(),
        };
        return vec![ToolEvent { ts, tool, summary }];
    }
    vec![]
}

/// A short, single-line gist of a tool's input — prefer the human-meaningful
/// key (the command run, the file touched, the pattern searched), else the
/// compact JSON. Always whitespace-collapsed and length-bounded.
fn summarize(input: &Value) -> String {
    match input {
        Value::Object(m) => {
            for k in [
                "command",
                "file_path",
                "path",
                "pattern",
                "query",
                "url",
                "description",
            ] {
                if let Some(s) = m.get(k).and_then(Value::as_str) {
                    return clip(s);
                }
            }
            // Codex `shell` passes command as an argv array.
            if let Some(arr) = m.get("command").and_then(Value::as_array) {
                let joined = arr
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ");
                return clip(&joined);
            }
            clip(&serde_json::to_string(input).unwrap_or_default())
        }
        Value::String(s) => clip(s),
        Value::Null => String::new(),
        other => clip(&other.to_string()),
    }
}

/// Collapse whitespace to single spaces and cap at 120 chars (so one event is
/// one readable line, never a multi-line scrollback dump).
fn clip(s: &str) -> String {
    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > 120 {
        one.chars().take(117).collect::<String>() + "…"
    } else {
        one
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_tmp(name: &str, body: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("td-tail-{}-{name}", std::process::id()));
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn claude_assistant_line_yields_one_event_per_tool_use() {
        let line = r#"{"type":"assistant","timestamp":"2026-06-18T10:00:00Z","message":{"role":"assistant","content":[{"type":"text","text":"hi"},{"type":"tool_use","name":"Bash","input":{"command":"ls -la /tmp"}},{"type":"tool_use","name":"Edit","input":{"file_path":"src/main.rs"}}]}}"#;
        let p = write_tmp("claude.jsonl", &format!("{line}\n"));
        let evs = tail_tool_events(&p, 10);
        std::fs::remove_file(&p).ok();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].tool, "Bash");
        assert_eq!(evs[0].summary, "ls -la /tmp");
        assert_eq!(evs[0].ts, "2026-06-18T10:00:00Z");
        assert_eq!(evs[1].tool, "Edit");
        assert_eq!(evs[1].summary, "src/main.rs");
    }

    #[test]
    fn user_and_text_only_lines_are_ignored() {
        let body = concat!(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"ok"}]}}"#,
            "\n",
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"thinking"}]}}"#,
            "\n",
        );
        let p = write_tmp("noise.jsonl", body);
        let evs = tail_tool_events(&p, 10);
        std::fs::remove_file(&p).ok();
        assert!(evs.is_empty(), "no tool_use blocks ⇒ no events");
    }

    #[test]
    fn codex_function_call_payload_is_parsed() {
        let line = r#"{"timestamp":"2026-06-18T11:00:00Z","type":"response_item","payload":{"type":"function_call","name":"shell","arguments":"{\"command\":[\"git\",\"status\"]}"}}"#;
        let p = write_tmp("codex.jsonl", &format!("{line}\n"));
        let evs = tail_tool_events(&p, 10);
        std::fs::remove_file(&p).ok();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].tool, "shell");
        assert_eq!(evs[0].summary, "git status");
    }

    #[test]
    fn unparseable_lines_are_skipped_not_fatal() {
        let body = concat!(
            "not json\n",
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"a.txt"}}]}}"#,
            "\n",
            "{ broken\n",
        );
        let p = write_tmp("mixed.jsonl", body);
        let evs = tail_tool_events(&p, 10);
        std::fs::remove_file(&p).ok();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].tool, "Read");
    }

    #[test]
    fn limit_keeps_only_the_most_recent() {
        let mut body = String::new();
        for i in 0..5 {
            body.push_str(&format!(
                r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","name":"Bash","input":{{"command":"echo {i}"}}}}]}}}}"#
            ));
            body.push('\n');
        }
        let p = write_tmp("many.jsonl", &body);
        let evs = tail_tool_events(&p, 2);
        std::fs::remove_file(&p).ok();
        assert_eq!(evs.len(), 2, "truncated to the last 2");
        assert_eq!(evs[0].summary, "echo 3");
        assert_eq!(evs[1].summary, "echo 4");
    }

    #[test]
    fn long_command_is_clipped_to_one_line() {
        let long = "x ".repeat(200);
        let line = format!(
            r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","name":"Bash","input":{{"command":"{long}"}}}}]}}}}"#
        );
        let p = write_tmp("long.jsonl", &format!("{line}\n"));
        let evs = tail_tool_events(&p, 10);
        std::fs::remove_file(&p).ok();
        assert_eq!(evs.len(), 1);
        assert!(evs[0].summary.chars().count() <= 120);
        assert!(!evs[0].summary.contains('\n'));
    }

    #[test]
    fn missing_file_is_empty_not_an_error() {
        let evs = tail_tool_events(Path::new("/no/such/transcript.jsonl"), 10);
        assert!(evs.is_empty());
    }

    #[test]
    fn transcript_for_only_resolves_agents_with_a_cwd() {
        let home = Path::new("/nonexistent");
        assert!(transcript_for("SHELL", Some("/tmp"), home).is_none());
        assert!(transcript_for("CLAUDE", None, home).is_none());
    }
}
