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
//!      ~/.codex/sessions/**.jsonl). Anything recovered from disk by mtime or
//!      history must be newer than the agent process, or it is some earlier
//!      conversation in the same directory and we fall back to `--continue`
//!      rather than reopen it.
//!
//! Restore = spawn the shell in `cwd`, then type `resume` into the PTY.
//! Everything here is std+libc only — no gpui — so it stays testable.

use std::fs::File;
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// What a pane needs at spawn time to pick its work back up.
#[derive(Clone, Default, Debug)]
pub struct PaneRestore {
    pub cwd: Option<String>,
    /// Command typed into the fresh shell (newline appended by the caller's PTY writer).
    pub resume: Option<String>,
    /// Filesystem path to a user-chosen header logo image (png/jpg/jpeg/svg), if set.
    /// Shown to the left of the program label in this pane's header; a user setting
    /// (not kernel-captured), so it lives only here, threaded through save/restore.
    pub logo: Option<String>,
    /// The sticky note on this pane's glass — its text, its tilt seed and its
    /// pin. Like `logo` it is a user setting rather than something captured from
    /// the kernel — and like `logo` it must survive a restart, because a note
    /// that only lives as long as the window is a note you have to write again
    /// every morning.
    pub note: Option<crate::sticky::Saved>,
}

/// What capture() learned about a live pane. Field-for-field what we persist.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct PaneRuntime {
    pub cwd: Option<String>,
    pub resume: Option<String>,
}

/// Just the pane's live cwd — the cheap slice of `capture` (one tcgetpgrp +
/// one readlink, no agent-session scan) for callers that POLL, like the
/// workspace's per-directory-logo sweep.
pub fn capture_cwd(master: Option<&File>, shell_pid: u32) -> Option<String> {
    let fg = master.and_then(fg_pgid).unwrap_or(shell_pid);
    proc_cwd(fg).or_else(|| proc_cwd(shell_pid))
}

/// Snapshot one live pane from its PTY master + shell pid.
pub fn capture(master: Option<&File>, shell_pid: u32) -> PaneRuntime {
    let fg = master.and_then(fg_pgid).unwrap_or(shell_pid);
    let cwd = proc_cwd(fg).or_else(|| proc_cwd(shell_pid));
    let resume = if fg != shell_pid {
        let comm = proc_read(fg, "comm");
        let cmdline = proc_cmdline(fg);
        agent_resume(&comm, &cmdline, cwd.as_deref(), Path::new(&home()), fg)
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

/// The push-style agent-session ledger. `td-agent-ledger` (a Claude Code
/// SessionStart/SessionEnd hook, wired by scripts/install-recovery-hook.sh)
/// records each live agent process's CURRENT session id at
/// `<home>/.local/state/terminal-delight/agent-ledger/<pid>.json` on every id
/// mint — launch, `/clear`, in-pane `/resume`, compaction — and removes it at
/// SessionEnd. Reading it beats forensics because rotation can never go
/// stale; the forensic chain below stays as the fallback for agents running
/// without the hook.
fn ledger_dir(home: &Path) -> PathBuf {
    home.join(".local/state/terminal-delight/agent-ledger")
}

/// The ledger's session id for a live agent pid — present, parseable, and
/// shell-safe, or nothing. A forged/corrupt entry must fall through to
/// forensics, never into a command line.
fn ledger_session_for(pid: u32, home: &Path) -> Option<String> {
    let body = std::fs::read_to_string(ledger_dir(home).join(format!("{pid}.json"))).ok()?;
    crate::recover::json_str(&body, "session_id").filter(|id| safe_resume_id(id))
}

/// CLI flags worth re-asserting on a resume line. `claude --resume` restores
/// the conversation but NOT flag-specified modes (documented behavior), so a
/// pane launched `claude --permission-mode plan` must resurrect with the same
/// flag or come back in the wrong mode — and the same goes for the model it was
/// pinned to and the effort level it was launched at. Allowlisted and sanitized:
/// the result is typed into a fresh shell.
fn carried_flags(cmdline: &str) -> String {
    fn safe_val(v: &str) -> bool {
        !v.is_empty()
            && v.len() <= 64
            && v.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
    }
    let mut out = String::new();
    let mut words = cmdline.split_whitespace().peekable();
    while let Some(w) = words.next() {
        match w {
            "--permission-mode" | "--model" | "--effort" => {
                if let Some(v) = words.peek().copied().filter(|v| safe_val(v)) {
                    out.push_str(&format!(" {w} {v}"));
                    words.next();
                }
            }
            "--dangerously-skip-permissions" => out.push_str(" --dangerously-skip-permissions"),
            _ => {}
        }
    }
    out
}

/// Synthesize the resume command for an agent foreground process, or None if
/// the process isn't an agent we know how to resume.
fn agent_resume(
    comm: &str,
    cmdline: &str,
    cwd: Option<&str>,
    home: &Path,
    pid: u32,
) -> Option<String> {
    let c = comm.trim();
    if c == "claude" || cmdline.contains("/claude") || cmdline.starts_with("claude ") {
        // Both sources (a cmdline arg, a transcript filename stem) end up typed
        // into a shell, so reject anything that isn't a plain id before use.
        let id = ledger_session_for(pid, home)
            // The push-style ledger names the CURRENT id even across a
            // /clear-rotation — when present it beats every forensic source.
            .or_else(|| claude_session_from_fds(pid, cwd, home))
            // The transcript the live process holds OPEN was the ground truth —
            // but Claude Code >= 2.1.195 opens-appends-closes and never keeps it
            // open, so this is usually None now and the matches below carry it.
            .or_else(|| arg_after(cmdline, &["--resume", "-r", "--session-id"]).map(str::to_string))
            // A FRESH `claude` has no id on its cmdline: bind by the transcript
            // BORN at this process's start (a new session's <id>.jsonl is created
            // when the process starts). This restores the per-pane binding the
            // open-fd scan gave us, so two fresh panes in one cwd never collapse
            // onto the same newest file (issue #157).
            .or_else(|| cwd.and_then(|d| claude_session_by_start(pid, d, home)))
            // ~/.claude/history.jsonl NAMES the newest conversation for a cwd
            // outright, where the file scans can only infer one from timestamps.
            .or_else(|| cwd.and_then(|d| history_session_for(d, home, proc_start_unix(pid))))
            // Last resort: newest transcript for this cwd — ambiguous if several
            // share a cwd (the collision the start-time match above prevents).
            .or_else(|| cwd.and_then(|d| claude_session_for(d, home, proc_start_unix(pid))))
            .filter(|id| safe_resume_id(id));
        // `--resume` restores the conversation but not flag-specified modes —
        // re-assert the allowlisted flags the live process was launched with.
        let flags = carried_flags(cmdline);
        Some(match id {
            Some(id) => format!("claude --resume {id}{flags}"),
            // --continue picks the most recent conversation for this cwd
            None => format!("claude --continue{flags}"),
        })
    } else if c == "codex" || cmdline.contains("/codex") || cmdline.starts_with("codex ") {
        let id = codex_session_from_fds(pid, home)
            .or_else(|| {
                arg_after(cmdline, &["resume", "--resume"])
                    .filter(|v| looks_like_uuid(v))
                    .map(str::to_string)
            })
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
pub(crate) fn safe_resume_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The session id inside a resume command — `claude --resume <id>` / `-r <id>`
/// / `codex resume <id>` — used to match a live pane against its on-disk
/// transcript (so the recover manifest can tell live agents from dead ones).
pub fn resume_session_id(resume: &str) -> Option<String> {
    arg_after(resume, &["--resume", "-r", "resume"])
        .map(str::to_string)
        .filter(|id| safe_resume_id(id))
}

/// Claude Code's per-project transcript dir slug: every non-alphanumeric
/// character of the absolute cwd becomes '-'.
pub(crate) fn claude_slug(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Most recent Claude Code session id for `cwd`: newest *.jsonl in
/// ~/.claude/projects/<slug>/ — the file stem IS the session uuid.
///
/// Refuses to answer with a transcript older than the agent process itself
/// (`not_before`, unix seconds). An id last written *before* this agent started
/// belongs to some earlier conversation in that directory — a previous pane's,
/// or yesterday's — and typing it into the restored shell would silently reopen
/// the wrong history. Declining leaves `claude --continue`, which asks Claude
/// Code the same question at restore time, when it can actually answer it.
fn claude_session_for(cwd: &str, home: &Path, not_before: Option<u64>) -> Option<String> {
    let dir = home.join(".claude/projects").join(claude_slug(cwd));
    let newest = newest_jsonl(&dir)?;
    written_since(&newest, not_before)
        .then(|| newest.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .flatten()
}

/// `~/.claude/history.jsonl` records every prompt as
/// `{"display":…,"timestamp":<ms>,"project":"<cwd>","sessionId":"<uuid>"}`.
/// The newest line for this directory *names* the conversation the agent is in,
/// where the transcript scans can only infer it from file times. Answers to the
/// same `not_before` staleness rule as [`claude_session_for`].
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

/// Has `path` been touched since `floor` (unix seconds)? A transcript only names
/// the *live* conversation if the agent has written to it since it started.
fn written_since(path: &Path, floor: Option<u64>) -> bool {
    let Some(floor) = floor else { return true };
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .is_some_and(|d| d.as_secs() >= floor)
}

/// [`proc_start_time`] as plain unix seconds — the staleness floor the disk
/// lookups compare against. `None` (unreadable /proc, or the pid 0 the tests
/// pass) means we could not tell, and every source is trusted as it was before.
pub(crate) fn proc_start_unix(pid: u32) -> Option<u64> {
    proc_start_time(pid)?
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
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

/// Bind a FRESH `claude` pid (no --resume on its cmdline) to its session by
/// matching the process start time to the transcript file BORN closest to it: a
/// new session's `<id>.jsonl` is created when the process starts, so each same-cwd
/// pane maps to its OWN file. This restores the per-pane binding the open-fd scan
/// gave us before Claude Code >= 2.1.195 stopped holding the transcript open
/// (issue #157). None if start/birth times are unavailable — the caller then
/// falls back to the newest-jsonl heuristic, so this is never worse than before.
fn claude_session_by_start(pid: u32, cwd: &str, home: &Path) -> Option<String> {
    let start = proc_start_time(pid)?;
    let dir = home.join(".claude/projects").join(claude_slug(cwd));
    let mut cands: Vec<(String, SystemTime)> = Vec::new();
    for entry in std::fs::read_dir(&dir).ok()?.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(born) = entry.metadata().and_then(|m| m.created()) else {
            continue;
        };
        if let Some(id) = p.file_stem().map(|s| s.to_string_lossy().into_owned()) {
            cands.push((id, born));
        }
    }
    pick_session_by_birth(start, &cands, Duration::from_secs(120))
}

/// Pure pick: the session id whose transcript was born closest to `start`, within
/// `window` (so an unrelated OLD transcript can't match a fresh process). Ties
/// break on the id for determinism. None if nothing is in-window. Split out so the
/// binding logic is unit-testable without `/proc` or the filesystem.
fn pick_session_by_birth(
    start: SystemTime,
    cands: &[(String, SystemTime)],
    window: Duration,
) -> Option<String> {
    cands
        .iter()
        .filter(|(id, _)| safe_resume_id(id))
        .filter_map(|(id, born)| {
            // absolute |born - start|, regardless of which is later
            let d = born.duration_since(start).unwrap_or_else(|e| e.duration());
            (d <= window).then_some((d, id.clone()))
        })
        .min_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)))
        .map(|(_, id)| id)
}

/// Wall-clock start time of process `pid`, from `/proc/<pid>/stat` field 22
/// (clock ticks since boot) + `/proc/stat` `btime`. None if either is unreadable.
fn proc_start_time(pid: u32) -> Option<SystemTime> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // comm (field 2) is wrapped in parens and may itself contain ')', so the
    // numeric fields begin after the LAST ')'. starttime is field 22 → index 19.
    let rest = stat.rsplit_once(')')?.1;
    let ticks: u64 = rest.split_whitespace().nth(19)?.parse().ok()?;
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if hz <= 0 {
        return None;
    }
    let btime = boot_time()?;
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(btime + ticks / hz as u64))
}

/// Seconds-since-epoch the machine booted, from `/proc/stat` `btime`.
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
    codex_rollout_for(cwd, home).and_then(|p| rollout_uuid(&p))
}

/// The on-disk rollout *file* for `cwd` (the path `codex_session_for` lifts its
/// uuid from). Shared so the read-only MCP event tailer can follow the same
/// transcript instead of re-deriving the layout.
fn codex_rollout_for(cwd: &str, home: &Path) -> Option<PathBuf> {
    let root = home.join(".codex/sessions");
    let mut rollouts: Vec<PathBuf> = vec![];
    collect_jsonl(&root, &mut rollouts, 4);
    rollouts.sort_by_key(|p| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH)
    });
    rollouts.into_iter().rev().take(20).find(|p| {
        let head = std::fs::read(p)
            .map(|b| String::from_utf8_lossy(&b[..b.len().min(4096)]).into_owned())
            .unwrap_or_default();
        head.contains(cwd)
    })
}

/// Path to the Claude Code transcript JSONL a pane is actually using. Prefers
/// the exact session id carried in the pane's `resume` command — that id was
/// resolved from the agent's open file descriptor in [`capture`], so it points
/// at the *right* conversation even when several share a cwd or a newer-but-
/// unrelated transcript exists. Falls back to newest-by-mtime only when there's
/// no usable id (e.g. a bare `claude --continue`). Public for the MCP tailer.
pub fn claude_transcript(cwd: &str, resume: Option<&str>, home: &Path) -> Option<PathBuf> {
    let dir = home.join(".claude/projects").join(claude_slug(cwd));
    if let Some(id) = resume.and_then(claude_resume_id) {
        let exact = dir.join(format!("{id}.jsonl"));
        if exact.is_file() {
            return Some(exact);
        }
    }
    newest_jsonl(&dir)
}

/// The session id embedded in a `claude --resume <id>` / `-r <id>` command, if
/// it's a plain (shell-safe) id — the transcript's filename stem.
fn claude_resume_id(resume: &str) -> Option<String> {
    arg_after(resume, &["--resume", "-r"])
        .map(str::to_string)
        .filter(|id| safe_resume_id(id))
}

/// Path to the newest Codex rollout JSONL for `cwd`, or None. Companion to
/// [`claude_transcript`] for the MCP event tailer.
pub fn codex_transcript(cwd: &str, home: &Path) -> Option<PathBuf> {
    codex_rollout_for(cwd, home)
}

/// `$HOME` as a path (or `.`), exposed so callers that already know a pane's
/// cwd can resolve its transcript without re-reading the env themselves.
pub fn home_dir() -> PathBuf {
    PathBuf::from(home())
}

/// `rollout-2026-06-12T10-00-00-<uuid>.jsonl` → uuid (the last 36 chars of the stem).
pub(crate) fn rollout_uuid(p: &Path) -> Option<String> {
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

/// The Claude session id the live process is *actively holding open* — read
/// straight from its open file descriptors (`/proc/<pid>/fd` → the `<id>.jsonl`
/// transcript it has open). Unlike the mtime scan, this binds the id to THIS
/// process, so two panes in the same cwd resolve to their own sessions instead
/// of both grabbing whichever transcript was touched last.
fn claude_session_from_fds(pid: u32, cwd: Option<&str>, home: &Path) -> Option<String> {
    // Confine the match to this cwd's project dir when we know it, so an
    // unrelated transcript the process happens to have open can't leak in.
    let root = match cwd {
        Some(d) => home.join(".claude/projects").join(claude_slug(d)),
        None => home.join(".claude/projects"),
    };
    open_jsonl_under(pid, &root)
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
}

/// Codex equivalent: the rollout `<uuid>.jsonl` the live process holds open.
fn codex_session_from_fds(pid: u32, home: &Path) -> Option<String> {
    let root = home.join(".codex/sessions");
    open_jsonl_under(pid, &root)
        .as_deref()
        .and_then(rollout_uuid)
}

/// First open file descriptor of `pid` that resolves to a `*.jsonl` under
/// `root`. Returns None when `/proc/<pid>/fd` is unreadable (no such pid, or the
/// agent has the transcript closed right now) — callers fall back to the
/// on-disk mtime scan.
fn open_jsonl_under(pid: u32, root: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(format!("/proc/{pid}/fd")).ok()?;
    for e in entries.flatten() {
        if let Ok(target) = std::fs::read_link(e.path()) {
            if target.extension().is_some_and(|x| x == "jsonl") && target.starts_with(root) {
                return Some(target);
            }
        }
    }
    None
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

pub(crate) fn collect_jsonl(dir: &Path, out: &mut Vec<PathBuf>, depth: u8) {
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
    fn birth_match_binds_each_pane_to_its_own_session_not_the_newest() {
        // #157: the collision the open-fd scan used to prevent. Two FRESH same-cwd
        // panes start at different times; each session's transcript is born at its
        // own pane's start. The newest file must NOT win for the older pane.
        let secs = |s: u64| SystemTime::UNIX_EPOCH + Duration::from_secs(s);
        let win = Duration::from_secs(120);
        let cands = vec![
            ("aaaaaaaa".to_string(), secs(1_000_004)), // pane A: born 4s after start
            ("bbbbbbbb".to_string(), secs(1_003_400)), // pane B: born ~57min later
            ("01dsessn".to_string(), secs(900_000)),   // an ancient transcript
        ];
        // Pane A (start 1_000_000) → its OWN session, not the newest (bbbb).
        assert_eq!(
            pick_session_by_birth(secs(1_000_000), &cands, win).as_deref(),
            Some("aaaaaaaa")
        );
        // Pane B (start ~1_003_398) → bbbb.
        assert_eq!(
            pick_session_by_birth(secs(1_003_398), &cands, win).as_deref(),
            Some("bbbbbbbb")
        );
        // A process whose start matches nothing in-window → None (caller falls
        // back to the newest-jsonl heuristic — never worse than before).
        assert_eq!(pick_session_by_birth(secs(2_000_000), &cands, win), None);
        // The ancient transcript is out of window for a fresh-ish start.
        assert_eq!(pick_session_by_birth(secs(950_000), &cands, win), None);
    }

    #[test]
    fn resume_id_must_be_shell_safe() {
        let home = Path::new("/nonexistent");
        // a cmdline arg carrying shell metacharacters must NOT be typed into the
        // shell — fall back to the safe cwd-scoped resume instead.
        assert_eq!(
            agent_resume(
                "claude",
                "claude --resume a;rm~-rf~/",
                Some("/tmp"),
                home,
                0
            )
            .as_deref(),
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
                home,
                0,
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
            agent_resume("claude", "claude --resume --verbose", Some("/tmp"), home, 0).as_deref(),
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
                home,
                0,
            )
            .as_deref(),
            Some("claude --resume 48be90b8-5777-44b6-bb6f-1c6069205c0d")
        );
        assert_eq!(
            agent_resume("claude", "claude -r abc123", Some("/tmp"), home, 0).as_deref(),
            Some("claude --resume abc123")
        );
        // bare `claude`, no transcripts on disk → cwd-scoped continue
        assert_eq!(
            agent_resume("claude", "claude", Some("/tmp"), home, 0).as_deref(),
            Some("claude --continue")
        );
    }

    #[test]
    fn codex_resume_forms() {
        let home = Path::new("/nonexistent");
        let id = "0196f9a1-2222-7333-8444-555566667777";
        assert_eq!(
            agent_resume("codex", &format!("codex resume {id}"), None, home, 0),
            Some(format!("codex resume {id}"))
        );
        assert_eq!(
            agent_resume("codex", "codex", Some("/tmp"), home, 0).as_deref(),
            Some("codex resume --last")
        );
    }

    #[test]
    fn non_agents_get_no_resume() {
        let home = Path::new("/nonexistent");
        assert_eq!(agent_resume("vim", "vim src/main.rs", None, home, 0), None);
        assert_eq!(agent_resume("bash", "bash", None, home, 0), None);
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
            history_session_for("/work/x", &home, None).as_deref(),
            Some(ID_B)
        );
        assert_eq!(
            history_session_for("/work/other", &home, None).as_deref(),
            Some(other)
        );
        assert_eq!(history_session_for("/work/absent", &home, None), None);

        // an explicit mapping beats a transcript file, which can only guess by
        // mtime (pid 0 here: no fd scan and no birth match, so the fresh-claude
        // path falls through to history vs newest-mtime)
        let proj = home.join(".claude/projects").join(claude_slug("/work/x"));
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("cccccccc-stale.jsonl"), "{}").unwrap();
        let want = format!("claude --resume {ID_B}");
        assert_eq!(
            agent_resume("claude", "claude", Some("/work/x"), &home, 0).as_deref(),
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
        assert_eq!(history_session_for("/work/x", &home, None), None);
        assert_eq!(
            history_session_for("/work/other", &home, None).as_deref(),
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
            history_session_for("/work/x", &home, Some(entry - 10)).as_deref(),
            Some(ID_B)
        );
        // the agent started afterwards → that id is a *previous* session in the
        // same directory, so the pane restores with --continue instead
        assert_eq!(
            history_session_for("/work/x", &home, Some(entry + 10)),
            None
        );

        // the transcript scan answers to the same rule
        let proj = home.join(".claude/projects").join(claude_slug("/work/y"));
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("bbbb-new.jsonl"), "{}").unwrap();
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(
            claude_session_for("/work/y", &home, Some(now - 60)).as_deref(),
            Some("bbbb-new")
        );
        assert_eq!(claude_session_for("/work/y", &home, Some(now + 60)), None);

        // our own start time is readable and plausible — the floor is real
        let started = proc_start_unix(std::process::id()).expect("our own start time");
        assert!(started <= now, "started {started} is not after now {now}");
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn open_fd_scan_binds_id_to_the_live_process() {
        // The whole point of the fd scan: the session id comes from a transcript
        // the *running* process actually holds open, not from whichever file in
        // the cwd was touched last. Prove it against our own open fd.
        let tmp = std::env::temp_dir().join(format!("td-fd-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let live = tmp.join("48be90b8-aaaa-bbbb-cccc-1c6069205c0d.jsonl");
        let _held = std::fs::File::create(&live).unwrap(); // keep the fd open
        let pid = std::process::id();
        assert_eq!(open_jsonl_under(pid, &tmp).as_deref(), Some(live.as_path()));
        // a different root must not match this process's fd
        assert_eq!(open_jsonl_under(pid, Path::new("/nonexistent")), None);
        // a dead/unknown pid yields nothing (callers fall back to the mtime scan)
        assert_eq!(open_jsonl_under(0, &tmp), None);
        drop(_held);
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn claude_transcript_follows_the_panes_own_session_not_newest() {
        // The MCP tailer must read the conversation a pane is actually in. Given
        // the pane's resume id, follow THAT file even when a newer, unrelated
        // transcript exists in the same cwd; only `--continue` falls back to it.
        let tmp = std::env::temp_dir().join(format!("td-tx-{}", std::process::id()));
        let proj = tmp.join(".claude/projects").join(claude_slug("/work/y"));
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("aaaa-mine.jsonl"), "{}").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(proj.join("bbbb-newer.jsonl"), "{}").unwrap(); // newest by mtime
        let mine = claude_transcript("/work/y", Some("claude --resume aaaa-mine"), &tmp).unwrap();
        assert!(mine.ends_with("aaaa-mine.jsonl"), "followed my own session");
        let cont = claude_transcript("/work/y", Some("claude --continue"), &tmp).unwrap();
        assert!(
            cont.ends_with("bbbb-newer.jsonl"),
            "no id ⇒ newest fallback"
        );
        std::fs::remove_dir_all(&tmp).ok();
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

// ---------------------------------------------------------------- probe ----
// td-send forensics: given the pid of a tile's shell (or the tile's direct
// child), work out what is actually running in it and whether TD could re-run
// it faithfully. Read-only /proc work — no ptrace, no PTY fds (we don't own
// them); tpgid from stat is the kernel telling us the foreground group.

/// What a probed tile is running, by migration class.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProbeKind {
    /// An idle shell prompt — faithful: reopen at cwd.
    Idle,
    /// A resumable agent (claude/codex) — faithful: re-run the resume line.
    Agent,
    /// A tmux client attached (or `new -A`) to a session — faithful: re-attach.
    Tmux,
    /// Anything else in the foreground (vim, htop…) — NOT faithful; refuse.
    Other,
}

impl ProbeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ProbeKind::Idle => "idle",
            ProbeKind::Agent => "agent",
            ProbeKind::Tmux => "tmux",
            ProbeKind::Other => "other",
        }
    }
}

pub struct ProbeReport {
    pub shell_pid: u32,
    pub fg_pid: u32,
    pub kind: ProbeKind,
    pub comm: String,
    pub cwd: Option<String>,
    pub cmdline: String,
    pub resume: Option<String>,
}

/// The numeric field `idx` positions after the comm in a /proc stat line
/// (state=1, ppid=2, pgrp=3, session=4, tty_nr=5, tpgid=6). comm may contain
/// spaces and parens ("tmux: client"), so split after the LAST ')'.
fn stat_field_after_comm(stat: &str, idx: usize) -> Option<i64> {
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(idx - 1)?.parse().ok()
}

/// Shells whose bare prompt makes a tile "idle" (faithfully reopenable).
fn is_shell_comm(comm: &str) -> bool {
    matches!(
        comm,
        "bash" | "zsh" | "fish" | "sh" | "dash" | "nu" | "nushell"
    )
}

/// A tmux invocation that re-runs faithfully: an attach (any spelling) or a
/// `new-session -A`. A bare `tmux` would mint a NEW session — not faithful.
fn tmux_attach_shaped(cmdline: &str) -> bool {
    let mut t = cmdline.split_whitespace();
    if t.next().map(|w| w.rsplit('/').next().unwrap_or(w)) != Some("tmux") {
        return false;
    }
    let words: Vec<&str> = t.collect();
    let attach = words
        .iter()
        .any(|w| matches!(*w, "attach" | "attach-session" | "a" | "at"));
    let new_a = (words.contains(&"new-session") || words.contains(&"new")) && words.contains(&"-A");
    attach || new_a
}

/// Classify a foreground process for migration. Pure — tested without /proc.
/// The agent check runs first: a derived resume line is the strongest evidence
/// and holds whether the agent sits in a shell or IS the tile's direct child.
fn classify(
    fg_is_probed_group: bool,
    comm: &str,
    cmdline: &str,
    resume: Option<&str>,
) -> ProbeKind {
    if resume.is_some() {
        return ProbeKind::Agent;
    }
    if comm.starts_with("tmux") && tmux_attach_shaped(cmdline) {
        return ProbeKind::Tmux;
    }
    if fg_is_probed_group && is_shell_comm(comm) {
        return ProbeKind::Idle;
    }
    ProbeKind::Other
}

/// Probe someone else's terminal by its shell (or direct-child) pid.
pub fn probe_external(shell_pid: u32, home: &Path) -> Result<ProbeReport, String> {
    let stat = proc_read(shell_pid, "stat");
    if stat.is_empty() {
        return Err(format!("no such process: {shell_pid}"));
    }
    let pgrp = stat_field_after_comm(&stat, 3).ok_or("unreadable stat (pgrp)")?;
    let tpgid = stat_field_after_comm(&stat, 6).ok_or("unreadable stat (tpgid)")?;
    if tpgid <= 0 {
        return Err(format!(
            "pid {shell_pid} has no controlling terminal (tpgid {tpgid})"
        ));
    }
    let fg_pid = tpgid as u32;
    let comm = proc_read(fg_pid, "comm").trim().to_string();
    if comm.is_empty() {
        return Err(format!("foreground group {fg_pid} vanished mid-probe"));
    }
    let cmdline = proc_cmdline(fg_pid);
    let cwd = proc_cwd(fg_pid).or_else(|| proc_cwd(shell_pid));
    let resume = agent_resume(&comm, &cmdline, cwd.as_deref(), home, fg_pid);
    let kind = classify(tpgid == pgrp, &comm, &cmdline, resume.as_deref());
    Ok(ProbeReport {
        shell_pid,
        fg_pid,
        kind,
        comm,
        cwd,
        cmdline,
        resume,
    })
}

#[cfg(test)]
mod probe_tests {
    use super::*;

    #[test]
    fn stat_fields_survive_hostile_comms() {
        // pid (comm-with-spaces-and-parens) state ppid pgrp session tty tpgid
        let stat = "4242 (a) b) (c)) S 100 4300 4242 34823 4400 4194304";
        assert_eq!(stat_field_after_comm(stat, 2), Some(100)); // ppid
        assert_eq!(stat_field_after_comm(stat, 3), Some(4300)); // pgrp
        assert_eq!(stat_field_after_comm(stat, 6), Some(4400)); // tpgid
        assert_eq!(stat_field_after_comm("garbage no parens", 6), None);
    }

    #[test]
    fn tmux_reruns_only_when_attach_shaped() {
        assert!(tmux_attach_shaped("tmux attach -t rec"));
        assert!(tmux_attach_shaped("/usr/bin/tmux a"));
        assert!(tmux_attach_shaped("tmux new-session -A -s main"));
        assert!(!tmux_attach_shaped("tmux"));
        assert!(!tmux_attach_shaped("tmux new-session -s fresh"));
        assert!(!tmux_attach_shaped("vim tmux.conf"));
    }

    #[test]
    fn migration_classes_are_conservative() {
        assert_eq!(classify(true, "bash", "bash", None), ProbeKind::Idle);
        assert_eq!(classify(true, "zsh", "-zsh", None), ProbeKind::Idle);
        // an agent wins regardless of grouping — the resume line is the evidence
        assert_eq!(
            classify(
                false,
                "claude",
                "claude --resume abc",
                Some("claude --resume abc")
            ),
            ProbeKind::Agent
        );
        // tmux client comm is truncated ("tmux: client"): prefix match + shape
        assert_eq!(
            classify(false, "tmux: client", "tmux attach -t rec", None),
            ProbeKind::Tmux
        );
        // vim in the foreground must never be "faithful"
        assert_eq!(
            classify(false, "vim", "vim notes.md", None),
            ProbeKind::Other
        );
        // a shell that is NOT the foreground group (stopped-job weirdness) stays Other
        assert_eq!(classify(false, "bash", "bash", None), ProbeKind::Other);
    }
}

#[cfg(test)]
mod ledger_tests {
    use super::*;

    fn tmp_home(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("td-ledger-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_ledger(home: &Path, pid: u32, sid: &str) {
        let dir = ledger_dir(home);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{pid}.json")),
            format!(r#"{{"session_id":"{sid}","pid":{pid},"ts":1}}"#),
        )
        .unwrap();
    }

    const ID: &str = "11111111-2222-3333-4444-555555555555";

    #[test]
    fn ledger_id_wins_over_every_forensic_source() {
        let home = tmp_home("wins");
        write_ledger(&home, 4242, ID);
        let r = agent_resume("claude", "claude", Some("/tmp/proj"), &home, 4242).unwrap();
        assert_eq!(r, format!("claude --resume {ID}"));
    }

    #[test]
    fn a_forged_ledger_id_never_reaches_a_shell() {
        let home = tmp_home("forged");
        write_ledger(&home, 4242, "$(rm -rf ~)");
        let r = agent_resume("claude", "claude", None, &home, 4242).unwrap();
        // the forged id is rejected and the chain falls through to --continue
        assert_eq!(r, "claude --continue");
    }

    #[test]
    fn a_missing_ledger_leaves_forensics_unchanged() {
        let home = tmp_home("gone");
        let r = agent_resume("claude", "claude", None, &home, 4242).unwrap();
        assert_eq!(r, "claude --continue");
    }

    #[test]
    fn recorded_cli_flags_ride_the_resume() {
        let home = tmp_home("flags");
        write_ledger(&home, 7, ID);
        let r = agent_resume(
            "claude",
            "claude --permission-mode plan --dangerously-skip-permissions",
            None,
            &home,
            7,
        )
        .unwrap();
        assert_eq!(
            r,
            format!("claude --resume {ID} --permission-mode plan --dangerously-skip-permissions")
        );
    }

    #[test]
    fn hostile_flag_values_are_dropped_sane_ones_carried() {
        assert_eq!(
            carried_flags("claude --model claude-opus-4.5"),
            " --model claude-opus-4.5"
        );
        assert_eq!(carried_flags("claude --permission-mode $(evil)"), "");
        assert_eq!(carried_flags("claude --permission-mode"), "");
        assert_eq!(carried_flags("claude --resume abc"), "");
        // the effort a pane was launched at is a flag, not conversation state,
        // so a resume that drops it comes back thinking at a different level
        assert_eq!(carried_flags("claude --effort max"), " --effort max");
        assert_eq!(carried_flags("claude --effort $(evil)"), "");
        assert_eq!(carried_flags("codex"), "");
    }
}
