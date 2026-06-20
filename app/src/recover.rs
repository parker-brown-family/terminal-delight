//! The tombstone manifest: agents that once had a session on disk but are no
//! longer in any live pane — and the command to resurrect each.
//!
//! Read-only discovery: it scans the agents' own transcript stores
//! (`~/.claude/projects/<slug>/<id>.jsonl`, `~/.codex/sessions/**/rollout-*.jsonl`)
//! and subtracts the session ids of the currently-open panes. The only side
//! effect of the whole feature is spawning a fresh pane that *resumes* a dead
//! conversation — the same restore path the app already uses; it never writes to
//! a live PTY. std-only, like [`crate::session`], so the scan stays testable.

use crate::session;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AgentKind {
    Claude,
    Codex,
}

impl AgentKind {
    pub fn label(self) -> &'static str {
        match self {
            AgentKind::Claude => "CLAUDE",
            AgentKind::Codex => "CODEX",
        }
    }
}

/// One recoverable (dead) agent: a transcript with no matching live pane.
#[derive(Clone, Debug)]
pub struct DeadAgent {
    pub kind: AgentKind,
    pub session_id: String,
    pub cwd: Option<String>,
    /// the command to type into a fresh shell to resume it.
    pub resume_cmd: String,
    /// first user task / summary line from the transcript, if recoverable.
    pub summary: Option<String>,
    /// pre-formatted "how long ago" for the manifest.
    pub age: String,
    pub bytes: u64,
}

/// Scan the agents' on-disk session stores for transcripts whose session id is
/// NOT in `live` (the resume ids of the currently-open panes). Newest first,
/// capped at `limit`. `home` is the user's home dir.
///
/// Two passes on purpose: a cheap metadata pass over *every* transcript (there
/// can be thousands), then transcript-head reads for only the `limit` newest —
/// so a big history never makes opening the manifest slow.
pub fn scan_dead(live: &HashSet<String>, home: &Path, limit: usize) -> Vec<DeadAgent> {
    // (kind, session_id, path, mtime, bytes) — metadata only, no content reads.
    let mut cand: Vec<(AgentKind, String, PathBuf, SystemTime, u64)> = Vec::new();

    // Claude: ~/.claude/projects/<slug>/<id>.jsonl  (the file stem IS the id)
    if let Ok(dirs) = std::fs::read_dir(home.join(".claude/projects")) {
        for d in dirs.flatten() {
            let dir = d.path();
            if !dir.is_dir() {
                continue;
            }
            let Ok(files) = std::fs::read_dir(&dir) else {
                continue;
            };
            for f in files.flatten() {
                let fp = f.path();
                if fp.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                let Some(id) = fp.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
                    continue;
                };
                if !session::safe_resume_id(&id) || live.contains(&id) {
                    continue;
                }
                let (mtime, bytes) = meta(&fp);
                cand.push((AgentKind::Claude, id, fp, mtime, bytes));
            }
        }
    }

    // Codex: ~/.codex/sessions/**/rollout-<ts>-<uuid>.jsonl
    let mut rollouts: Vec<PathBuf> = Vec::new();
    session::collect_jsonl(&home.join(".codex/sessions"), &mut rollouts, 4);
    for fp in rollouts {
        let Some(id) = session::rollout_uuid(&fp) else {
            continue;
        };
        if !session::safe_resume_id(&id) || live.contains(&id) {
            continue;
        }
        let (mtime, bytes) = meta(&fp);
        cand.push((AgentKind::Codex, id, fp, mtime, bytes));
    }

    cand.sort_by_key(|c| std::cmp::Reverse(c.3)); // newest first
    cand.truncate(limit);

    let now = SystemTime::now();
    cand.into_iter()
        .map(|(kind, id, path, mtime, bytes)| {
            let head = read_head(&path, 16 * 1024);
            let cwd = json_str(&head, "cwd");
            let summary = json_str(&head, "summary")
                .or_else(|| json_str(&head, "content"))
                .or_else(|| json_str(&head, "text"))
                .map(|s| clip(&s, 96));
            let resume_cmd = match kind {
                AgentKind::Claude => format!("claude --resume {id}"),
                AgentKind::Codex => format!("codex resume {id}"),
            };
            DeadAgent {
                kind,
                session_id: id,
                cwd,
                resume_cmd,
                summary,
                age: fmt_age(now, mtime),
                bytes,
            }
        })
        .collect()
}

fn meta(p: &Path) -> (SystemTime, u64) {
    std::fs::metadata(p)
        .map(|m| (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len()))
        .unwrap_or((SystemTime::UNIX_EPOCH, 0))
}

fn read_head(p: &Path, max: usize) -> String {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(p) else {
        return String::new();
    };
    let mut buf = vec![0u8; max];
    let n = f.read(&mut buf).unwrap_or(0);
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

/// First plain-string value of `"key":"..."` in `s` (minimal escape handling).
/// Returns None when the key is absent or its value isn't a JSON string (e.g.
/// `"content":[…]`), so an object/array value never gets mis-parsed.
fn json_str(s: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\":\"");
    let start = s.find(&pat)? + pat.len();
    let mut out = String::new();
    let mut it = s[start..].chars();
    while let Some(c) = it.next() {
        match c {
            '\\' => {
                if let Some(n) = it.next() {
                    out.push(match n {
                        'n' | 't' | 'r' => ' ',
                        '"' => '"',
                        '\\' => '\\',
                        '/' => '/',
                        o => o,
                    });
                }
            }
            '"' => return Some(out),
            _ => out.push(c),
        }
    }
    Some(out)
}

/// Collapse whitespace and clip to `n` chars with an ellipsis.
fn clip(s: &str, n: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > n {
        format!("{}\u{2026}", s.chars().take(n - 1).collect::<String>())
    } else {
        s
    }
}

fn fmt_age(now: SystemTime, then: SystemTime) -> String {
    let secs = now.duration_since(then).map(|d| d.as_secs()).unwrap_or(0);
    if secs < 90 {
        format!("{secs}s ago")
    } else if secs < 5400 {
        format!("{}m ago", secs / 60)
    } else if secs < 172_800 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_str_pulls_plain_values_only() {
        let line =
            r#"{"cwd":"/home/x/proj","type":"user","content":"refactor the parser","arr":[1,2]}"#;
        assert_eq!(json_str(line, "cwd").as_deref(), Some("/home/x/proj"));
        assert_eq!(
            json_str(line, "content").as_deref(),
            Some("refactor the parser")
        );
        // a non-string value is not mis-parsed as a string
        assert_eq!(json_str(line, "arr"), None);
        assert_eq!(json_str(line, "missing"), None);
    }

    #[test]
    fn json_str_unescapes_minimally() {
        let line = r#"{"cwd":"/a/b","content":"line one\nline two \"quoted\""}"#;
        assert_eq!(
            json_str(line, "content").as_deref(),
            Some("line one line two \"quoted\"")
        );
    }

    #[test]
    fn clip_collapses_and_truncates() {
        assert_eq!(clip("  a   b\tc ", 10), "a b c");
        assert_eq!(clip("abcdefghij", 5), "abcd\u{2026}");
    }

    #[test]
    fn fmt_age_buckets() {
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        let ago = |s: u64| fmt_age(now, now - std::time::Duration::from_secs(s));
        assert_eq!(ago(30), "30s ago");
        assert_eq!(ago(600), "10m ago");
        assert_eq!(ago(7200), "2h ago");
        assert_eq!(ago(200_000), "2d ago");
    }

    #[test]
    fn scan_finds_dead_skips_live() {
        let tmp = std::env::temp_dir().join(format!("td-recover-{}", std::process::id()));
        let proj = tmp.join(".claude/projects").join("-home-x-proj");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(
            proj.join("aaaadead-1111-2222-3333-444455556666.jsonl"),
            r#"{"cwd":"/home/x/proj","content":"the dead task"}"#,
        )
        .unwrap();
        std::fs::write(
            proj.join("bbbblive-1111-2222-3333-444455556666.jsonl"),
            r#"{"cwd":"/home/x/proj","content":"the live task"}"#,
        )
        .unwrap();

        let mut live = HashSet::new();
        live.insert("bbbblive-1111-2222-3333-444455556666".to_string());

        let dead = scan_dead(&live, &tmp, 50);
        assert_eq!(dead.len(), 1, "only the dead session is listed");
        let a = &dead[0];
        assert_eq!(a.kind, AgentKind::Claude);
        assert_eq!(a.session_id, "aaaadead-1111-2222-3333-444455556666");
        assert_eq!(a.cwd.as_deref(), Some("/home/x/proj"));
        assert_eq!(a.summary.as_deref(), Some("the dead task"));
        assert_eq!(
            a.resume_cmd,
            "claude --resume aaaadead-1111-2222-3333-444455556666"
        );
        std::fs::remove_dir_all(&tmp).ok();
    }
}
