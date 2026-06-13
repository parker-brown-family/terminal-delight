//! VS-Code-style session capture: per-pane cwd + agent (claude/codex) session
//! identity, so a crash or close-everything reboots straight back into work.
//!
//! Capture answers two questions per pane, straight from the kernel:
//!   1. WHERE — the foreground process's cwd (falls back to the shell's).
//!   2. WHO — if the foreground process is an agent, a shell command that
//!      resumes that exact conversation (`claude --resume <id>`, `codex
//!      resume <id>`), synthesized from the cmdline when the id is visible
//!      there, otherwise recovered from the agent's own session store on disk
//!      (~/.claude/projects/<cwd-slug>/*.jsonl, ~/.codex/sessions/**.jsonl).
//!
//! Restore = spawn the shell in `cwd`, then type `resume` into the PTY.
//! Everything here is std+libc only — no gpui — so it stays testable.

use std::fs::File;
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// What a pane needs at spawn time to pick its work back up.
#[derive(Clone, Default, Debug)]
pub struct PaneRestore {
    pub cwd: Option<String>,
    /// Command typed into the fresh shell (newline appended by the caller's PTY writer).
    pub resume: Option<String>,
}

/// What capture() learned about a live pane. Field-for-field what we persist.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct PaneRuntime {
    pub cwd: Option<String>,
    pub resume: Option<String>,
}

/// Snapshot one live pane from its PTY master + shell pid.
pub fn capture(master: Option<&File>, shell_pid: u32) -> PaneRuntime {
    let fg = master.and_then(fg_pgid).unwrap_or(shell_pid);
    let cwd = proc_cwd(fg).or_else(|| proc_cwd(shell_pid));
    let resume = if fg != shell_pid {
        let comm = proc_read(fg, "comm");
        let cmdline = proc_cmdline(fg);
        agent_resume(&comm, &cmdline, cwd.as_deref(), Path::new(&home()))
    } else {
        None
    };
    PaneRuntime { cwd, resume }
}

/// Crash-safe write: tmp file + rename, so a crash mid-write never truncates
/// the last good state.
pub fn write_atomic(path: &Path, body: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        // 0700 dir: the state it holds (cwd history + agent session ids) is the
        // user's alone, so keep it owner-only on multi-user machines.
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    let tmp = path.with_extension("toml.tmp");
    // 0600 file for the same reason — never world-readable.
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(body.as_bytes())?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)
}

// ---- kernel-side plumbing ----

fn fg_pgid(master: &File) -> Option<u32> {
    use std::os::fd::AsRawFd;
    let pgid = unsafe { libc::tcgetpgrp(master.as_raw_fd()) };
    (pgid > 0).then_some(pgid as u32)
}

fn proc_cwd(pid: u32) -> Option<String> {
    std::fs::read_link(format!("/proc/{pid}/cwd"))
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

fn proc_read(pid: u32, what: &str) -> String {
    std::fs::read_to_string(format!("/proc/{pid}/{what}")).unwrap_or_default()
}

fn proc_cmdline(pid: u32) -> String {
    proc_read(pid, "cmdline").replace('\0', " ")
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_else(|_| ".".into())
}

// ---- agent identity ----

/// Synthesize the resume command for an agent foreground process, or None if
/// the process isn't an agent we know how to resume.
fn agent_resume(comm: &str, cmdline: &str, cwd: Option<&str>, home: &Path) -> Option<String> {
    let c = comm.trim();
    if c == "claude" || cmdline.contains("/claude") || cmdline.starts_with("claude ") {
        // Both sources (a cmdline arg, a transcript filename stem) end up typed
        // into a shell, so reject anything that isn't a plain id before use.
        let id = arg_after(cmdline, &["--resume", "-r"])
            .map(str::to_string)
            .or_else(|| cwd.and_then(|d| claude_session_for(d, home)))
            .filter(|id| safe_resume_id(id));
        Some(match id {
            Some(id) => format!("claude --resume {id}"),
            // --continue picks the most recent conversation for this cwd
            None => "claude --continue".to_string(),
        })
    } else if c == "codex" || cmdline.contains("/codex") || cmdline.starts_with("codex ") {
        let id = arg_after(cmdline, &["resume", "--resume"])
            .filter(|v| looks_like_uuid(v))
            .map(str::to_string)
            .or_else(|| cwd.and_then(|d| codex_session_for(d, home)))
            .filter(|id| safe_resume_id(id));
        Some(match id {
            Some(id) => format!("codex resume {id}"),
            None => "codex resume --last".to_string(),
        })
    } else {
        None
    }
}

/// The value following any of `keys` in a space-joined cmdline.
fn arg_after<'a>(cmdline: &'a str, keys: &[&str]) -> Option<&'a str> {
    let mut words = cmdline.split_whitespace().peekable();
    while let Some(w) = words.next() {
        if keys.contains(&w) {
            return words.peek().copied().filter(|v| !v.starts_with('-'));
        }
    }
    None
}

fn looks_like_uuid(v: &str) -> bool {
    v.len() >= 32 && v.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// A resume id is interpolated into a command line that gets typed straight
/// into a fresh shell, so it must not be able to break out of the command.
/// Session ids are uuids (hex + dashes) or transcript filename stems; allow
/// only those plain characters and reject shell metacharacters / whitespace.
fn safe_resume_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Claude Code's per-project transcript dir slug: every non-alphanumeric
/// character of the absolute cwd becomes '-'.
fn claude_slug(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Most recent Claude Code session id for `cwd`: newest *.jsonl in
/// ~/.claude/projects/<slug>/ — the file stem IS the session uuid.
fn claude_session_for(cwd: &str, home: &Path) -> Option<String> {
    let dir = home.join(".claude/projects").join(claude_slug(cwd));
    newest_jsonl(&dir).and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
}

/// Most recent Codex rollout whose header mentions `cwd`:
/// ~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl — uuid from the name,
/// cwd matched against the first bytes of the file.
fn codex_session_for(cwd: &str, home: &Path) -> Option<String> {
    let root = home.join(".codex/sessions");
    let mut rollouts: Vec<PathBuf> = vec![];
    collect_jsonl(&root, &mut rollouts, 4);
    rollouts.sort_by_key(|p| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH)
    });
    for p in rollouts.iter().rev().take(20) {
        let head = std::fs::read(p)
            .map(|b| String::from_utf8_lossy(&b[..b.len().min(4096)]).into_owned())
            .unwrap_or_default();
        if head.contains(cwd) {
            return rollout_uuid(p);
        }
    }
    None
}

/// `rollout-2026-06-12T10-00-00-<uuid>.jsonl` → uuid (the last 36 chars of the stem).
fn rollout_uuid(p: &Path) -> Option<String> {
    let stem = p.file_stem()?.to_string_lossy();
    let tail: String = stem
        .chars()
        .rev()
        .take(36)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    looks_like_uuid(&tail).then_some(tail)
}

fn newest_jsonl(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .max_by_key(|p| {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH)
        })
}

fn collect_jsonl(dir: &Path, out: &mut Vec<PathBuf>, depth: u8) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() && depth > 0 {
            collect_jsonl(&p, out, depth - 1);
        } else if p.extension().is_some_and(|x| x == "jsonl") {
            out.push(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_matches_claude_code_layout() {
        assert_eq!(
            claude_slug("/home/user/Code/terminal-delight"),
            "-home-user-Code-terminal-delight"
        );
    }

    #[test]
    fn resume_id_must_be_shell_safe() {
        let home = Path::new("/nonexistent");
        // a cmdline arg carrying shell metacharacters must NOT be typed into the
        // shell — fall back to the safe cwd-scoped resume instead.
        assert_eq!(
            agent_resume("claude", "claude --resume a;rm~-rf~/", Some("/tmp"), home).as_deref(),
            Some("claude --continue"),
            "unsafe id rejected, falls back to --continue"
        );
        // a plain uuid still rides through untouched
        let id = "48be90b8-5777-44b6-bb6f-1c6069205c0d";
        assert_eq!(
            agent_resume(
                "claude",
                &format!("claude --resume {id}"),
                Some("/tmp"),
                home
            )
            .as_deref(),
            Some("claude --resume 48be90b8-5777-44b6-bb6f-1c6069205c0d")
        );
        assert!(safe_resume_id(id));
        assert!(safe_resume_id("bbbb-new_2"));
        assert!(!safe_resume_id("a;b"));
        assert!(!safe_resume_id("$(whoami)"));
        assert!(!safe_resume_id(""));
    }

    #[test]
    fn resume_arg_ignores_a_following_flag() {
        // `--resume` with a flag (not an id) after it must not capture the flag.
        let home = Path::new("/nonexistent");
        assert_eq!(
            agent_resume("claude", "claude --resume --verbose", Some("/tmp"), home).as_deref(),
            Some("claude --continue")
        );
    }

    #[test]
    fn resume_id_lifted_from_cmdline() {
        let home = Path::new("/nonexistent");
        assert_eq!(
            agent_resume(
                "claude",
                "claude --resume 48be90b8-5777-44b6-bb6f-1c6069205c0d",
                Some("/tmp"),
                home
            )
            .as_deref(),
            Some("claude --resume 48be90b8-5777-44b6-bb6f-1c6069205c0d")
        );
        assert_eq!(
            agent_resume("claude", "claude -r abc123", Some("/tmp"), home).as_deref(),
            Some("claude --resume abc123")
        );
        // bare `claude`, no transcripts on disk → cwd-scoped continue
        assert_eq!(
            agent_resume("claude", "claude", Some("/tmp"), home).as_deref(),
            Some("claude --continue")
        );
    }

    #[test]
    fn codex_resume_forms() {
        let home = Path::new("/nonexistent");
        let id = "0196f9a1-2222-7333-8444-555566667777";
        assert_eq!(
            agent_resume("codex", &format!("codex resume {id}"), None, home),
            Some(format!("codex resume {id}"))
        );
        assert_eq!(
            agent_resume("codex", "codex", Some("/tmp"), home).as_deref(),
            Some("codex resume --last")
        );
    }

    #[test]
    fn non_agents_get_no_resume() {
        let home = Path::new("/nonexistent");
        assert_eq!(agent_resume("vim", "vim src/main.rs", None, home), None);
        assert_eq!(agent_resume("bash", "bash", None, home), None);
    }

    #[test]
    fn claude_session_recovered_from_disk() {
        let tmp = std::env::temp_dir().join(format!("td-sess-test-{}", std::process::id()));
        let proj = tmp.join(".claude/projects").join(claude_slug("/work/x"));
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("aaaa-old.jsonl"), "{}").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(proj.join("bbbb-new.jsonl"), "{}").unwrap();
        assert_eq!(
            claude_session_for("/work/x", &tmp).as_deref(),
            Some("bbbb-new")
        );
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn atomic_write_replaces_not_truncates() {
        let tmp = std::env::temp_dir().join(format!("td-atomic-{}.toml", std::process::id()));
        write_atomic(&tmp, "first").unwrap();
        write_atomic(&tmp, "second").unwrap();
        assert_eq!(std::fs::read_to_string(&tmp).unwrap(), "second");
        std::fs::remove_file(&tmp).unwrap();
    }

    #[test]
    fn state_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        // it holds cwd history + agent session ids — must never be world-readable
        let tmp = std::env::temp_dir().join(format!("td-perm-{}.toml", std::process::id()));
        write_atomic(&tmp, "secret cwds").unwrap();
        let mode = std::fs::metadata(&tmp).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "state file must be 0600, got {mode:o}");
        std::fs::remove_file(&tmp).unwrap();
    }
}
