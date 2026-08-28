//! VS-Code-style session capture: per-pane cwd + agent (claude/codex) session
//! identity, so a crash or close-everything reboots straight back into work.
//!
//! Capture answers two questions per pane, straight from the kernel:
//!   1. WHERE — the foreground process's cwd (falls back to the shell's).
//!   2. WHO — if the foreground process is an agent, a shell command that
//!      resumes that exact conversation (`claude --resume <id>`, `codex
//!      resume <id>`), synthesized from the cmdline when the id is visible
//!      there, otherwise recovered from the agent's own session store on disk
//!      (~/.claude/history.jsonl, ~/.claude/projects/<cwd-slug>/*.jsonl,
//!      ~/.codex/sessions/**.jsonl). Anything recovered from disk must be newer
//!      than the agent process, or it is some earlier conversation in the same
//!      directory and we fall back to `--continue` rather than reopen it.
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
        agent_resume(
            &comm,
            &cmdline,
            cwd.as_deref(),
            Path::new(&home()),
            proc_started_at(fg),
        )
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
///
/// `started_at` is when the agent process itself started (unix seconds), and it
/// is what keeps a directory's *previous* conversation from being mistaken for
/// this one — see [`claude_session_for`]. `None` means we could not tell, and
/// every source is trusted as it was before.
fn agent_resume(
    comm: &str,
    cmdline: &str,
    cwd: Option<&str>,
    home: &Path,
    started_at: Option<u64>,
) -> Option<String> {
    let c = comm.trim();
    if c == "claude" || cmdline.contains("/claude") || cmdline.starts_with("claude ") {
        // Both sources (a cmdline arg, a session id recovered from disk) end up
        // typed into a shell, so reject anything that isn't a plain id first.
        let id = arg_after(cmdline, &["--resume", "-r"])
            .map(str::to_string)
            .or_else(|| cwd.and_then(|d| claude_session_for(d, home, started_at)))
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

/// The Claude Code session running in `cwd`, if one can be named with
/// confidence — an explicit prompt-history mapping first, the transcript
/// directory second.
///
/// Both refuse to answer with anything older than the agent process itself
/// (`not_before`, unix seconds). An id written *before* this agent started
/// belongs to some earlier conversation in that directory — a previous pane's,
/// or yesterday's — and typing it into the restored shell would silently reopen
/// the wrong history. Declining leaves `claude --continue`, which asks Claude
/// Code the same question at restore time, when it can actually answer it.
fn claude_session_for(cwd: &str, home: &Path, not_before: Option<u64>) -> Option<String> {
    history_session_for(cwd, home, not_before)
        .or_else(|| transcript_session_for(cwd, home, not_before))
}

/// `~/.claude/history.jsonl` records every prompt as
/// `{"display":…,"timestamp":<ms>,"project":"<cwd>","sessionId":"<uuid>"}`.
/// The newest line for this directory *names* the conversation the agent is in,
/// where the transcript scan below can only infer it from mtimes.
fn history_session_for(cwd: &str, home: &Path, not_before: Option<u64>) -> Option<String> {
    let body = std::fs::read_to_string(home.join(".claude/history.jsonl")).ok()?;
    for line in body.lines().rev() {
        if json_tail_string(line, "project").as_deref() != Some(cwd) {
            continue;
        }
        // Lines are appended in time order, so the first match walking backwards
        // is the newest for this directory; if that one predates the agent,
        // nothing further back can help either.
        if let Some(floor) = not_before {
            match json_tail_number(line, "timestamp") {
                Some(ms) if ms / 1000 >= floor => {}
                _ => return None,
            }
        }
        return json_tail_string(line, "sessionId").filter(|id| looks_like_uuid(id));
    }
    None
}

/// Newest `*.jsonl` in `~/.claude/projects/<slug>/` — the file stem IS the
/// session uuid. Older Claude Code builds keep live transcripts here.
fn transcript_session_for(cwd: &str, home: &Path, not_before: Option<u64>) -> Option<String> {
    let dir = home.join(".claude/projects").join(claude_slug(cwd));
    let newest = newest_jsonl(&dir)?;
    written_since(&newest, not_before)
        .then(|| newest.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .flatten()
}

/// Has `path` been touched since `floor` (unix seconds)? A transcript only names
/// the *live* conversation if the agent has written to it since it started.
fn written_since(path: &Path, floor: Option<u64>) -> bool {
    let Some(floor) = floor else { return true };
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .is_some_and(|d| d.as_secs() >= floor)
}

/// Read `"<key>": <value>` from the *end* of a JSON line.
///
/// The scan runs right-to-left on purpose: the first field on a history line is
/// the prompt the user typed, which can contain anything at all — including text
/// shaped like `"project":"/somewhere/else"`. Claude Code's own fields come
/// last, so only the rightmost match can be trusted.
fn json_tail<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let at = line.rfind(&format!("\"{key}\""))? + key.len() + 2;
    Some(line.get(at..)?.trim_start().strip_prefix(':')?.trim_start())
}

fn json_tail_string(line: &str, key: &str) -> Option<String> {
    let rest = json_tail(line, key)?.strip_prefix('"')?;
    let mut value = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(value),
            '\\' => value.push(chars.next()?),
            _ => value.push(c),
        }
    }
    None
}

fn json_tail_number(line: &str, key: &str) -> Option<u64> {
    let rest = json_tail(line, key)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// When a process started, in unix seconds: `/proc/<pid>/stat` field 22 counts
/// clock ticks since boot, and `/proc/stat`'s `btime` says when boot was.
fn proc_started_at(pid: u32) -> Option<u64> {
    let ticks = stat_start_ticks(&proc_read(pid, "stat"))?;
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    (hz > 0).then(|| boot_time().map(|b| b + ticks / hz as u64))?
}

/// Field 22 of `/proc/<pid>/stat`. Field 2 is the executable name in
/// parentheses and may itself contain spaces and parens, so the split has to
/// start after the *last* `)` — everything from there is field 3 onwards.
fn stat_start_ticks(stat: &str) -> Option<u64> {
    stat.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()
}

fn boot_time() -> Option<u64> {
    std::fs::read_to_string("/proc/stat")
        .ok()?
        .lines()
        .find_map(|l| l.strip_prefix("btime "))
        .and_then(|v| v.trim().parse().ok())
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

    /// The disk lookups get their own tests below; these cases are about the
    /// cmdline, so they name no process start time.
    fn resume(comm: &str, cmdline: &str, cwd: Option<&str>, home: &Path) -> Option<String> {
        agent_resume(comm, cmdline, cwd, home, None)
    }

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
            resume("claude", "claude --resume a;rm~-rf~/", Some("/tmp"), home).as_deref(),
            Some("claude --continue"),
            "unsafe id rejected, falls back to --continue"
        );
        // a plain uuid still rides through untouched
        let id = "48be90b8-5777-44b6-bb6f-1c6069205c0d";
        assert_eq!(
            resume(
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
            resume("claude", "claude --resume --verbose", Some("/tmp"), home).as_deref(),
            Some("claude --continue")
        );
    }

    #[test]
    fn resume_id_lifted_from_cmdline() {
        let home = Path::new("/nonexistent");
        assert_eq!(
            resume(
                "claude",
                "claude --resume 48be90b8-5777-44b6-bb6f-1c6069205c0d",
                Some("/tmp"),
                home
            )
            .as_deref(),
            Some("claude --resume 48be90b8-5777-44b6-bb6f-1c6069205c0d")
        );
        assert_eq!(
            resume("claude", "claude -r abc123", Some("/tmp"), home).as_deref(),
            Some("claude --resume abc123")
        );
        // bare `claude`, no transcripts on disk → cwd-scoped continue
        assert_eq!(
            resume("claude", "claude", Some("/tmp"), home).as_deref(),
            Some("claude --continue")
        );
    }

    #[test]
    fn codex_resume_forms() {
        let home = Path::new("/nonexistent");
        let id = "0196f9a1-2222-7333-8444-555566667777";
        assert_eq!(
            resume("codex", &format!("codex resume {id}"), None, home),
            Some(format!("codex resume {id}"))
        );
        assert_eq!(
            resume("codex", "codex", Some("/tmp"), home).as_deref(),
            Some("codex resume --last")
        );
    }

    #[test]
    fn non_agents_get_no_resume() {
        let home = Path::new("/nonexistent");
        assert_eq!(resume("vim", "vim src/main.rs", None, home), None);
        assert_eq!(resume("bash", "bash", None, home), None);
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
            claude_session_for("/work/x", &tmp, None).as_deref(),
            Some("bbbb-new")
        );
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    fn tmp_home(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("td-sess-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".claude")).unwrap();
        dir
    }

    fn history_line(ts_ms: u64, project: &str, id: &str) -> String {
        format!(
            "{{\"display\":\"hi\",\"pastedContents\":{{}},\"timestamp\":{ts_ms},\"project\":\"{project}\",\"sessionId\":\"{id}\"}}"
        )
    }

    const ID_A: &str = "48be90b8-5777-44b6-bb6f-1c6069205c0d";
    const ID_B: &str = "142fecf9-897f-4157-9f99-f36903f9faf0";

    #[test]
    fn history_names_the_conversation_for_a_directory() {
        let home = tmp_home("hist");
        let other = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
        std::fs::write(
            home.join(".claude/history.jsonl"),
            [
                history_line(1_700_000_100_000, "/work/x", ID_A),
                history_line(1_700_000_200_000, "/work/other", other),
                history_line(1_700_000_300_000, "/work/x", ID_B),
            ]
            .join("\n"),
        )
        .unwrap();

        // the newest line for this directory wins; other projects are invisible
        assert_eq!(
            claude_session_for("/work/x", &home, None).as_deref(),
            Some(ID_B)
        );
        assert_eq!(
            claude_session_for("/work/other", &home, None).as_deref(),
            Some(other)
        );
        assert_eq!(claude_session_for("/work/absent", &home, None), None);

        // an explicit mapping beats a transcript file, which can only guess by mtime
        let proj = home.join(".claude/projects").join(claude_slug("/work/x"));
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("cccccccc-stale.jsonl"), "{}").unwrap();
        let want = format!("claude --resume {ID_B}");
        assert_eq!(
            resume("claude", "claude", Some("/work/x"), &home).as_deref(),
            Some(want.as_str())
        );
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn a_typed_prompt_cannot_forge_the_history_fields() {
        let home = tmp_home("forge");
        // the prompt is the first field on the line and is entirely user-typed:
        // someone can paste a whole fake record into it
        let forged = format!(
            "{{\"display\":\"look: \\\"project\\\":\\\"/work/x\\\",\\\"sessionId\\\":\\\"deadbeef\\\"\",\"timestamp\":1700000300000,\"project\":\"/work/other\",\"sessionId\":\"{ID_B}\"}}"
        );
        std::fs::write(home.join(".claude/history.jsonl"), forged).unwrap();
        // reading right-to-left, only the real trailing fields are ever seen
        assert_eq!(claude_session_for("/work/x", &home, None), None);
        assert_eq!(
            claude_session_for("/work/other", &home, None).as_deref(),
            Some(ID_B)
        );
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn an_id_older_than_the_agent_belongs_to_another_conversation() {
        let home = tmp_home("stale");
        let entry = 1_700_000_300u64;
        std::fs::write(
            home.join(".claude/history.jsonl"),
            history_line(entry * 1000, "/work/x", ID_B),
        )
        .unwrap();

        // the agent was running when that prompt was typed → its conversation
        assert_eq!(
            claude_session_for("/work/x", &home, Some(entry - 10)).as_deref(),
            Some(ID_B)
        );
        // the agent started afterwards → that id is a *previous* session in the
        // same directory, so the pane restores with --continue instead
        assert_eq!(claude_session_for("/work/x", &home, Some(entry + 10)), None);
        assert_eq!(
            agent_resume("claude", "claude", Some("/work/x"), &home, Some(entry + 10)).as_deref(),
            Some("claude --continue")
        );
        // an id on the cmdline is authoritative whatever the clocks say
        let explicit = format!("claude --resume {ID_A}");
        assert_eq!(
            agent_resume("claude", &explicit, Some("/work/x"), &home, Some(entry + 10)).as_deref(),
            Some(explicit.as_str())
        );

        // the transcript scan answers to the same rule
        let proj = home.join(".claude/projects").join(claude_slug("/work/y"));
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("bbbb-new.jsonl"), "{}").unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(
            claude_session_for("/work/y", &home, Some(now - 60)).as_deref(),
            Some("bbbb-new")
        );
        assert_eq!(claude_session_for("/work/y", &home, Some(now + 60)), None);
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn start_time_survives_a_process_name_with_spaces_and_parens() {
        // field 2 of /proc/<pid>/stat is the executable name in parentheses and
        // may contain both; field 22 is counted from after the LAST ')'
        let tail: Vec<String> = (3..=22).map(|n| n.to_string()).collect();
        let stat = format!("42 (weird (name) here) {}", tail.join(" "));
        assert_eq!(stat_start_ticks(&stat), Some(22));
        assert_eq!(stat_start_ticks("nonsense"), None);
        assert_eq!(stat_start_ticks("42 (sh) S 1 2 3"), None);

        // and against the real kernel: our own start time is in the past
        let started = proc_started_at(std::process::id()).expect("our own start time");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(started <= now, "started {started} is not after now {now}");
        assert!(now - started < 60 * 60 * 24, "start time is plausible");
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
