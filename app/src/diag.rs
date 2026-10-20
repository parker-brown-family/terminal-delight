//! What Terminal Delight keeps when something goes wrong, and the report a
//! person can choose to send about it.
//!
//! # The rule this module lives under
//!
//! A terminal holds a person's secrets: what they typed, what came back, the
//! paths they work in, the names their programs gave themselves. **Nothing in
//! here reads, keeps or sends any of it.** No scrollback, no screen rows, no
//! pane titles (a program sets those), no working directories, no command
//! lines, no screenshot — a picture of the window IS its contents. What a
//! report carries is TD's own: its builds, the machine's versions, counts of
//! tabs and panes, where in TD's code a crash happened, and TD's own
//! diagnostic lines with every path, link and quoted value scrubbed out
//! ([`scrub`]).
//!
//! # The consent ladder
//!
//! Each rung moves the data one step further from the person, and each takes
//! its own click. Nothing climbs a rung by itself.
//!
//! 0. Logs and crash records are written on this machine and stay there
//!    ([`route_stderr`], [`install_crash_record`]).
//! 1. Preparing a report writes one file locally and opens it, so the person
//!    reads every word of it before anything else happens ([`write_report`]).
//!    Nothing is sent.
//! 2. Handing that file to an AI agent sends it to the agent's provider. That
//!    is a separate click, and the agent is started in the report's own
//!    directory with that one file to read ([`agent_run_line`]).
//! 3. Filing on GitHub. The agent shows the exact title and body and files
//!    only on a yes; or the person opens GitHub's page and pastes what they
//!    choose ([`NEW_ISSUE_URL`]).
//!
//! # Why the logs needed a home
//!
//! Before this, a window started from the desktop and the session host it
//! talks to both had their stderr on `/dev/null`: every diagnostic TD wrote,
//! a refused save or a pane that diverged from its host, went nowhere. And a
//! Rust panic exits without a core dump, so Omarchy's crash watcher, which
//! reads core dumps, never saw TD fall over either.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Where an issue can be filed by hand.
pub const NEW_ISSUE_URL: &str =
    "https://github.com/parker-brown-family/terminal-delight/issues/new";

/// The repository a drafted issue goes to.
const REPO: &str = "parker-brown-family/terminal-delight";

/// A log is set aside once it passes this, so a pane that warns every second
/// cannot fill a disk. One older copy is kept beside it.
const LOG_CAP: u64 = 4 * 1024 * 1024;

/// How many of TD's own lines a report carries, per log.
const REPORT_LOG_LINES: usize = 80;

/// How many crash records a report carries, newest first.
const REPORT_CRASHES: usize = 5;

/// A backtrace is cut here; the frames that matter are at the top.
const BACKTRACE_LINES: usize = 120;

/// What stands in for a value [`scrub`] removed.
const GONE: &str = "\u{2039}\u{2026}\u{203a}";

/// Crash records this process has written, for the 🐞's badge.
static CRASHES_THIS_RUN: AtomicUsize = AtomicUsize::new(0);

/// Which TD process is writing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role {
    Window,
    /// The session host, by its session key.
    Host(String),
}

impl Role {
    fn word(&self) -> &'static str {
        match self {
            Role::Window => "window",
            Role::Host(_) => "host",
        }
    }

    fn log_name(&self) -> String {
        match self {
            Role::Window => "window.log".to_string(),
            Role::Host(key) => format!("host-{}.log", file_safe(key)),
        }
    }
}

fn file_safe(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// `$XDG_STATE_HOME/terminal-delight`: state survives a reboot, which a log of
/// what went wrong before the reboot has to.
pub fn state_root() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| crate::session::home_dir().join(".local/state"))
        .join("terminal-delight")
}

pub fn logs_dir() -> PathBuf {
    state_root().join("logs")
}

pub fn crashes_dir() -> PathBuf {
    state_root().join("crashes")
}

pub fn reports_dir() -> PathBuf {
    state_root().join("bug-reports")
}

/// This build's name: the installed binary is named for its commit and
/// branch (`td-babe2dc-translations`), which is what a report needs; a
/// development build is just `terminal-delight`, so the version rides along.
pub fn build_name() -> String {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "unknown".to_string());
    format!("{exe} ({})", env!("CARGO_PKG_VERSION"))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Open `role`'s log for appending, setting the old one aside if it has grown
/// past [`LOG_CAP`], and mark where this process's lines begin.
pub fn open_log(role: &Role) -> io::Result<File> {
    let dir = logs_dir();
    fs::create_dir_all(&dir)?;
    let path = dir.join(role.log_name());
    if fs::metadata(&path)
        .map(|m| m.len() > LOG_CAP)
        .unwrap_or(false)
    {
        let _ = fs::rename(&path, path.with_extension("log.1"));
    }
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(
        file,
        "=== terminal-delight {} \u{b7} build {} \u{b7} pid {} \u{b7} started {} (unix) ===",
        role.word(),
        build_name(),
        std::process::id(),
        unix_now()
    )?;
    Ok(file)
}

/// Where `role`'s log lives.
pub fn log_path(role: &Role) -> PathBuf {
    logs_dir().join(role.log_name())
}

fn stderr_is_tty() -> bool {
    // SAFETY: isatty only inspects the descriptor.
    unsafe { libc::isatty(2) == 1 }
}

/// Whether stderr already goes to a file somebody chose.
fn stderr_is_regular_file() -> bool {
    // SAFETY: fstat writes into the zeroed struct we own.
    unsafe {
        let mut st: libc::stat = std::mem::zeroed();
        libc::fstat(2, &mut st) == 0 && (st.st_mode & libc::S_IFMT) == libc::S_IFREG
    }
}

/// Send this process's stderr to `role`'s log — unless a person is reading it
/// in a terminal, or it already goes to a file somebody chose (a test harness,
/// `2>somewhere.log`, or the host log [`crate::hostctl`] opened). What is left
/// is exactly the case that lost everything: `/dev/null`, or nothing at all.
pub fn route_stderr(role: &Role) {
    if stderr_is_tty() || stderr_is_regular_file() {
        return;
    }
    let Ok(file) = open_log(role) else {
        return;
    };
    use std::os::fd::AsRawFd;
    // SAFETY: dup2 onto stderr; `file` stays open until the call returns and
    // the duplicate keeps the description alive after.
    unsafe {
        libc::dup2(file.as_raw_fd(), 2);
    }
}

/// Crash records this process has written since it started.
pub fn crashes_this_run() -> usize {
    CRASHES_THIS_RUN.load(Ordering::Relaxed)
}

/// Keep a record of any panic, on any thread, in [`crashes_dir`].
///
/// A panic on a thread that is not the main one ends that thread and leaves
/// the window running, so a pane can stop updating with nothing said anywhere;
/// this is the one place that hears it. The message is [`scrub`]bed before it
/// is written or printed, because Rust's own panic text can quote data — a bad
/// string slice quotes the string — and that string may have come from a
/// terminal. Rust's default hook, which prints the message whole, runs only
/// when a person is watching stderr in their own terminal.
pub fn install_crash_record(role: Role) {
    let default = std::panic::take_hook();
    let tty = stderr_is_tty();
    std::panic::set_hook(Box::new(move |info| {
        let raw = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned());
        let message = match raw {
            Some(m) => scrub(&m),
            None => "(not text)".to_string(),
        };
        let at = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());
        let thread = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_string();
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        let record = crash_record(&role, &thread, &at, &message, &backtrace);
        let written = write_crash_record(&record);
        CRASHES_THIS_RUN.fetch_add(1, Ordering::Relaxed);
        eprintln!(
            "terminal-delight: PANIC on thread {thread} at {at}: {message} \u{2014} {}",
            match &written {
                Ok(p) => format!("crash record {}", p.display()),
                Err(e) => format!("no crash record ({e})"),
            }
        );
        if tty {
            default(info);
        }
    }));
}

fn crash_record(role: &Role, thread: &str, at: &str, message: &str, backtrace: &str) -> String {
    let mut out = String::new();
    out.push_str("terminal-delight crash record 1\n");
    out.push_str(&format!("build: {}\n", build_name()));
    out.push_str(&format!("role: {}\n", role.word()));
    out.push_str(&format!("pid: {}\n", std::process::id()));
    out.push_str(&format!("when: {} (unix)\n", unix_now()));
    out.push_str(&format!("thread: {thread}\n"));
    out.push_str(&format!("at: {at}\n"));
    out.push_str(&format!("message: {message}\n\nbacktrace:\n"));
    for line in backtrace.lines().take(BACKTRACE_LINES) {
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn write_crash_record(record: &str) -> io::Result<PathBuf> {
    let dir = crashes_dir();
    fs::create_dir_all(&dir)?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = dir.join(format!("{nanos}-{}.txt", std::process::id()));
    fs::write(&path, record)?;
    Ok(path)
}

/// Take every value out of one of TD's diagnostic lines that could have come
/// from a person's work: quoted spans, links and paths become `‹…›`. What is
/// left is TD's own wording, its numbers and its error kinds — the parts that
/// say what went wrong without saying anything about whose work it was.
pub fn scrub(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let prev = if i == 0 { None } else { Some(chars[i - 1]) };
        let at_token_start = prev.is_none_or(|p| !p.is_alphanumeric());

        // A quoted span: `..`, ".." anywhere, '..' only where a quote opens a
        // token, so "can't" stays a word.
        if c == '`' || c == '"' || (c == '\'' && at_token_start) {
            if let Some(end) = closing(&chars, i + 1, c) {
                out.push_str(GONE);
                i = end + 1;
                continue;
            }
        }
        if at_token_start {
            // A link: letters, then "://".
            let scheme_len = chars[i..]
                .iter()
                .take_while(|ch| ch.is_ascii_alphabetic())
                .count();
            if scheme_len > 0 && chars[i + scheme_len..].starts_with(&[':', '/', '/']) {
                out.push_str(GONE);
                i = token_end(&chars, i);
                continue;
            }
            // A path: "/x…" or "~/…".
            let is_path = (c == '/' && chars.get(i + 1).is_some_and(|n| !n.is_whitespace()))
                || (c == '~' && chars.get(i + 1) == Some(&'/'));
            if is_path {
                out.push_str(GONE);
                i = path_end(&chars, i);
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// The index of the quote closing one opened just before `from`, if it is on
/// the same line. A `'` closes only where a token ends, so an apostrophe
/// inside a quoted word does not close it early.
fn closing(chars: &[char], from: usize, quote: char) -> Option<usize> {
    (from..chars.len()).find(|&j| {
        chars[j] == quote
            && (quote != '\'' || chars.get(j + 1).is_none_or(|n| !n.is_alphanumeric()))
    })
}

/// Where a path ends. A path printed with `display()` carries no quotes and
/// may hold spaces, so it is taken to run to the next clause break — `: `,
/// ` (`, `, `, `; `, ` — ` — or the end of the line. That can take some of
/// TD's own wording with it, which is the safe way to be wrong.
fn path_end(chars: &[char], from: usize) -> usize {
    const BREAKS: [&str; 5] = [": ", " (", ", ", "; ", " \u{2014}"];
    let rest: String = chars[from..].iter().collect();
    let cut = BREAKS
        .iter()
        .filter_map(|b| rest.find(b))
        .min()
        .unwrap_or(rest.len());
    let mut j = from + rest[..cut].chars().count();
    while j > from + 1 && matches!(chars[j - 1], '.' | ':') {
        j -= 1;
    }
    j
}

/// Where a link ends: at whitespace, or at punctuation that closes the
/// sentence around it.
fn token_end(chars: &[char], from: usize) -> usize {
    let mut j = from;
    while j < chars.len() && !chars[j].is_whitespace() && !matches!(chars[j], ')' | ',' | ';' | ']')
    {
        j += 1;
    }
    // A sentence's full stop or colon is not part of the path.
    while j > from + 1 && matches!(chars[j - 1], '.' | ':') {
        j -= 1;
    }
    j
}

/// Whether a log line is one of TD's own operational lines, which are the
/// only lines a report carries. Developer traces (`[doc-hit]`, `[anchor]`,
/// `[bench-sel]`, …) are switched on by hand and quote what is under the
/// pointer, so they never qualify; nor does the line recording an agent's
/// launch command, which names a project's paths.
fn reportable(line: &str) -> bool {
    let ours = line.starts_with("terminal-delight")
        || line.starts_with("td:")
        || line.starts_with("=== terminal-delight");
    ours && !line.contains("launching \u{2014}")
}

/// The last `n` reportable lines of a log, scrubbed, with a run of identical
/// lines folded into one.
fn report_lines(text: &str, n: usize) -> Vec<String> {
    let mut folded: Vec<(String, usize)> = Vec::new();
    for line in text.lines().filter(|l| reportable(l)) {
        let clean = scrub(line);
        match folded.last_mut() {
            Some((last, count)) if *last == clean => *count += 1,
            _ => folded.push((clean, 1)),
        }
    }
    let skip = folded.len().saturating_sub(n);
    folded
        .into_iter()
        .skip(skip)
        .map(|(line, count)| {
            if count > 1 {
                format!("{line}  (\u{d7}{count})")
            } else {
                line
            }
        })
        .collect()
}

/// Counts of what this window holds. Counts only: never a title, a directory
/// or a program's name, which is why a pane running something TD does not
/// name is counted as `other` rather than by what it is running.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Census {
    pub tabs: usize,
    pub panes: usize,
    /// Panes by kind, in a fixed order.
    pub kinds: Vec<(&'static str, usize)>,
}

impl Census {
    pub fn of(tabs: usize, modes: &[crate::pane::PaneMode]) -> Self {
        use crate::pane::PaneMode;
        let names = ["shell", "claude", "codex", "remote", "other", "unknown"];
        let mut counts = [0usize; 6];
        for m in modes {
            let k = match m {
                PaneMode::Shell => 0,
                PaneMode::Claude => 1,
                PaneMode::Codex => 2,
                PaneMode::Remote => 3,
                PaneMode::Other(_) => 4,
                PaneMode::Unknown => 5,
            };
            counts[k] += 1;
        }
        Census {
            tabs,
            panes: modes.len(),
            kinds: names
                .iter()
                .zip(counts)
                .filter(|(_, n)| *n > 0)
                .map(|(name, n)| (*name, n))
                .collect(),
        }
    }
}

/// Crash records newer than the last report a person prepared, or `None` when
/// the directory cannot be read — which is not the same as none recorded.
pub fn unseen_crashes() -> Option<usize> {
    let since = fs::metadata(reports_dir().join(".last-prepared"))
        .and_then(|m| m.modified())
        .ok();
    let entries = match fs::read_dir(crashes_dir()) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Some(0),
        Err(_) => return None,
    };
    Some(
        entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "txt"))
            .filter(|e| match (since, e.metadata().and_then(|m| m.modified())) {
                (Some(since), Ok(at)) => at > since,
                _ => true,
            })
            .count(),
    )
}

fn newest_files(dir: &Path, ext: &str, n: usize) -> Vec<PathBuf> {
    let mut files: Vec<(SystemTime, PathBuf)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == ext))
        .filter_map(|p| Some((fs::metadata(&p).ok()?.modified().ok()?, p)))
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    files.into_iter().take(n).map(|(_, p)| p).collect()
}

/// The builds of the session hosts running now, read off their executables.
fn host_builds() -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .bytes()
                .all(|b| b.is_ascii_digit())
        })
        .filter_map(|e| {
            let cmd = fs::read(e.path().join("cmdline")).ok()?;
            let args: Vec<&[u8]> = cmd.split(|b| *b == 0).collect();
            (args.get(1) == Some(&&b"serve"[..])).then_some(())?;
            let exe = fs::read_link(e.path().join("exe")).ok()?;
            let name = exe.file_name()?.to_string_lossy().into_owned();
            (name.starts_with("td-") || name == "terminal-delight").then_some(name)
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

fn os_name() -> Option<String> {
    let text = fs::read_to_string("/etc/os-release").ok()?;
    text.lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|v| v.trim_matches('"').to_string())
}

fn kernel() -> Option<String> {
    fs::read_to_string("/proc/sys/kernel/osrelease")
        .ok()
        .map(|s| s.trim().to_string())
}

fn or_unknown(v: Option<String>) -> String {
    v.unwrap_or_else(|| "unknown".to_string())
}

/// Rung 1: write a report on this machine and say where. Nothing is sent.
pub fn write_report(census: &Census) -> io::Result<PathBuf> {
    let stamp = unix_now();
    let dir = reports_dir().join(stamp.to_string());
    fs::create_dir_all(&dir)?;
    let path = dir.join("report.md");
    fs::write(&path, report_text(&state_root(), census, stamp))?;
    // What "since your last report" is measured from.
    let _ = fs::write(reports_dir().join(".last-prepared"), stamp.to_string());
    Ok(path)
}

fn report_text(root: &Path, census: &Census, stamp: u64) -> String {
    let mut r = String::new();
    r.push_str("# Terminal Delight bug report\n\n");
    r.push_str(&format!(
        "Prepared at {stamp} (unix) on this machine. **Nothing in this file has been sent anywhere.**\n\n"
    ));
    r.push_str("## What happened\n\n");
    r.push_str("_Describe it here in your own words: what you did, what you expected, what you saw. This is the only part that needs you._\n\n");
    r.push_str("## What this file holds, and what it never holds\n\n");
    r.push_str("It holds TD's builds and your system's versions, counts of tabs and panes, crash records TD wrote (where in TD's code it failed), and TD's own error lines with every path, link and quoted value replaced by \u{2039}\u{2026}\u{203a}.\n\n");
    r.push_str("It never holds your terminal's input or output, scrollback, pane titles, working directories, commands or a screenshot. You can delete any line before it goes anywhere.\n\n");

    r.push_str("## System\n\n");
    r.push_str(&format!("- window build: {}\n", build_name()));
    let hosts = host_builds();
    r.push_str(&format!(
        "- session host builds running: {}\n",
        if hosts.is_empty() {
            "none found".to_string()
        } else {
            hosts.join(", ")
        }
    ));
    r.push_str(&format!("- OS: {}\n", or_unknown(os_name())));
    r.push_str(&format!("- kernel: {}\n", or_unknown(kernel())));
    r.push_str(&format!(
        "- desktop: {}\n",
        or_unknown(
            std::env::var("XDG_CURRENT_DESKTOP")
                .ok()
                .filter(|s| !s.is_empty())
        )
    ));
    r.push_str(&format!(
        "- display server: {}\n\n",
        or_unknown(
            std::env::var("XDG_SESSION_TYPE")
                .ok()
                .filter(|s| !s.is_empty())
        )
    ));

    r.push_str("## This window\n\n");
    r.push_str(&format!(
        "- tabs: {}\n- panes: {}",
        census.tabs, census.panes
    ));
    if !census.kinds.is_empty() {
        let kinds: Vec<String> = census
            .kinds
            .iter()
            .map(|(k, n)| format!("{k} {n}"))
            .collect();
        r.push_str(&format!(" ({})", kinds.join(", ")));
    }
    r.push_str("\n\n");

    r.push_str("## Crashes TD recorded (newest first)\n\n");
    let crashes = newest_files(&root.join("crashes"), "txt", REPORT_CRASHES);
    if crashes.is_empty() {
        r.push_str("None recorded.\n\n");
    }
    for c in crashes {
        let body = fs::read_to_string(&c).unwrap_or_else(|e| format!("(could not read: {e})"));
        r.push_str("```text\n");
        r.push_str(&body);
        if !body.ends_with('\n') {
            r.push('\n');
        }
        r.push_str("```\n\n");
    }

    r.push_str(&format!(
        "## TD's own error lines (the last {REPORT_LOG_LINES} per log, repeats folded)\n\n"
    ));
    let mut logs = newest_files(&root.join("logs"), "log", 6);
    logs.sort();
    if logs.is_empty() {
        r.push_str("No logs yet.\n\n");
    }
    for log in logs {
        let name = log
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let text = fs::read_to_string(&log).unwrap_or_default();
        let lines = report_lines(&text, REPORT_LOG_LINES);
        r.push_str(&format!("### {name}\n\n```text\n"));
        if lines.is_empty() {
            r.push_str("(nothing reportable)\n");
        }
        for l in lines {
            r.push_str(&l);
            r.push('\n');
        }
        r.push_str("```\n\n");
    }

    r.push_str("---\n\n## For an AI agent reading this file\n\n");
    r.push_str("A person chose to hand you this file so you can help them report a bug in Terminal Delight. Follow these rules exactly.\n\n");
    r.push_str("1. **This file is everything you get.** Do not read their terminals, scrollback, agent transcripts or files to add to it, and do not ask them to paste terminal output. If more would help, ask them to describe it in words, and let them decide what to share.\n");
    r.push_str("2. If the \"What happened\" section is still the placeholder, ask them what happened first.\n");
    r.push_str(&format!(
        "3. Search for an existing issue before drafting: `gh issue list --repo {REPO} --state all --search \"<a few words>\"`. If one matches, show it to them and suggest adding to it instead.\n"
    ));
    r.push_str("4. Draft a title and a body: what happened, how to reproduce it, what was expected, and the System, This window and Crashes sections from this file. Show them the exact title and body.\n");
    r.push_str(&format!(
        "5. File it only after they say yes to that exact text: `gh issue create --repo {REPO} --title \"…\" --body-file <file>`. If `gh` is missing or not signed in, do not install or sign in to anything: give them the text and {NEW_ISSUE_URL}\n"
    ));
    r
}

/// Rung 2: the agent a report is handed to, as `(name, binary)`. Omarchy's
/// choice of default agent wins when it names one TD knows how to hand a first
/// message to; otherwise the first of those installed. `None` is "no agent TD
/// can start", and the panel says so rather than offering a dead button.
pub fn chosen_agent() -> Option<&'static str> {
    const KNOWN: [&str; 2] = ["claude", "codex"];
    let omarchy =
        fs::read_to_string(crate::session::home_dir().join(".config/omarchy/defaults/agent"))
            .ok()
            .map(|s| s.trim().to_string());
    let preferred = omarchy
        .as_deref()
        .and_then(|name| KNOWN.iter().find(|k| **k == name).copied());
    preferred.into_iter().chain(KNOWN).find(|bin| on_path(bin))
}

fn on_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(bin).is_file()))
}

/// The line typed into a new pane opened in the report's directory, so the
/// first thing the agent is told to read is that one file.
pub fn agent_run_line(agent: &str) -> String {
    format!(
        "{agent} 'Read report.md in this directory and follow its section \"For an AI agent reading this file\".'"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_takes_out_paths_links_and_quoted_values() {
        assert_eq!(
            scrub("terminal-delight: doc here: /home/someone/secret plan.pdf is not a file TD can draw"),
            format!("terminal-delight: doc here: {GONE}"),
        );
        assert_eq!(
            scrub("terminal-delight: cannot read ~/notes/x.md: No such file (os error 2)"),
            format!("terminal-delight: cannot read {GONE}: No such file (os error 2)"),
        );
        assert_eq!(
            scrub("not following https://example.com/a?b=c from a document"),
            format!("not following {GONE} from a document"),
        );
        assert_eq!(
            scrub("session 'work' and \"Some Title\" and `ls -la`"),
            format!("session {GONE} and {GONE} and {GONE}"),
        );
    }

    #[test]
    fn scrub_leaves_tds_own_wording_and_numbers() {
        let line = "terminal-delight: pane 24 has diverged from the session host at offset 1027142 of 1027142 consumed (host 52401a8d5dea6866, window 34f56514051c0876); taking it again";
        assert_eq!(scrub(line), line);
        assert_eq!(scrub("it can't stop, 3/4 done"), "it can't stop, 3/4 done");
        assert_eq!(
            scrub("terminal-delight: /home/a/my docs/x.md: denied (os error 13)"),
            format!("terminal-delight: {GONE}: denied (os error 13)"),
        );
    }

    #[test]
    fn a_rust_slice_panic_loses_the_string_it_quoted() {
        // The shape of an older Rust's message for a bad string slice, which
        // quoted the whole string; 1.98 quotes only the character. Either may
        // have come straight out of a terminal.
        let msg = "byte index 3 is not a char boundary; it is inside 'é' (bytes 2..4) of `my secret token`";
        let clean = scrub(msg);
        assert!(!clean.contains("secret"), "{clean}");
        assert!(!clean.contains('é'), "{clean}");
        assert!(
            clean.starts_with("byte index 3 is not a char boundary"),
            "{clean}"
        );
    }

    #[test]
    fn a_report_carries_only_tds_own_lines() {
        let log = "\
=== terminal-delight window · build td-x (0.1.0) · pid 7 · started 1 (unix) ===
[doc-hit] cell=(r1,c2) token=\"hunter2\" cwd=Some(\"/home/a/b\")
[anchor] row human_input=true head=\"rm -rf mydir\"
terminal-delight: launching — claude --project /home/a/secret
terminal-delight: pane 3 diverges again
terminal-delight: pane 3 diverges again
terminal-delight: the session host would not save: refused at /home/a/x
";
        let lines = report_lines(log, 80);
        let all = lines.join("\n");
        assert!(
            !all.contains("hunter2") && !all.contains("rm -rf") && !all.contains("/home"),
            "{all}"
        );
        assert!(!all.contains("launching"), "{all}");
        assert!(all.contains("pane 3 diverges again  (\u{d7}2)"), "{all}");
        assert_eq!(lines.len(), 3, "{all}");
    }

    #[test]
    fn a_whole_report_holds_no_terminal_content_and_says_the_rules() {
        let root = std::env::temp_dir().join(format!("td-diag-report-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("logs")).unwrap();
        fs::create_dir_all(root.join("crashes")).unwrap();
        fs::write(
            root.join("logs/window.log"),
            "=== terminal-delight window · build td-x (0.1.0) · pid 7 · started 1 (unix) ===\n\
             [doc-hit] token=\"hunter2\" cwd=Some(\"/home/a/clients/acme\")\n\
             terminal-delight: cannot read /home/a/clients/acme/plan.md: denied (os error 13)\n\
             terminal-delight: pane 3 diverges again\n",
        )
        .unwrap();
        let record = crash_record(
            &Role::Window,
            "pane-reader",
            "src/pane.rs:1:1",
            &scrub("byte index 3 is not a char boundary; it is inside 'x' of `password=hunter2`"),
            "",
        );
        fs::write(root.join("crashes/1-7.txt"), record).unwrap();

        let census = Census::of(1, &[crate::pane::PaneMode::Other("secret-tool".into())]);
        let report = report_text(&root, &census, 42);
        let _ = fs::remove_dir_all(&root);

        for secret in ["hunter2", "acme", "secret-tool", "/home/a"] {
            assert!(
                !report.contains(secret),
                "the report carries {secret:?}:\n{report}"
            );
        }
        assert!(
            report.contains("terminal-delight: pane 3 diverges again"),
            "{report}"
        );
        assert!(report.contains("thread: pane-reader"), "{report}");
        assert!(report.contains("panes: 1 (other 1)"), "{report}");
        // Rungs 2 and 3, written where the agent will read them.
        assert!(
            report.contains("This file is everything you get"),
            "{report}"
        );
        assert!(
            report.contains("only after they say yes to that exact text"),
            "{report}"
        );
    }

    /// The hook itself, driven by a real panic. A panic hook is process-wide,
    /// so it is installed in a child copy of this test binary, which runs only
    /// this test with `TD_DIAG_PANIC_CHILD` set and its state directory in a
    /// temporary folder; the parent reads what the child left behind.
    #[test]
    fn a_real_panic_on_a_thread_leaves_a_scrubbed_crash_record() {
        const ME: &str = "diag::tests::a_real_panic_on_a_thread_leaves_a_scrubbed_crash_record";
        if std::env::var_os("TD_DIAG_PANIC_CHILD").is_some() {
            install_crash_record(Role::Window);
            // Two real panics, each on its own thread. A bad string slice, whose
            // message quotes the character it landed inside (Rust 1.98; older
            // versions quoted the whole string); and a formatted message that
            // quotes a value, the shape of an `expect` with a `format!`.
            let slice = std::thread::Builder::new()
                .name("drill".into())
                .spawn(|| {
                    let s = String::from("\u{2605}hunter2");
                    let cut = std::hint::black_box(1);
                    let _ = &s[..cut];
                })
                .expect("spawn");
            assert!(slice.join().is_err(), "the slice drill did not panic");
            let formatted = std::thread::Builder::new()
                .name("drill".into())
                .spawn(|| {
                    let token = std::hint::black_box("hunter2");
                    panic!("the token `{token}` was refused");
                })
                .expect("spawn");
            assert!(
                formatted.join().is_err(),
                "the formatted drill did not panic"
            );
            std::process::exit(0);
        }
        let root = std::env::temp_dir().join(format!("td-diag-panic-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", ME, "--nocapture", "--test-threads=1"])
            .env("TD_DIAG_PANIC_CHILD", "1")
            .env("XDG_STATE_HOME", &root)
            .output()
            .expect("run the child");
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let records: Vec<PathBuf> = fs::read_dir(root.join("terminal-delight/crashes"))
            .map(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).collect())
            .unwrap_or_default();
        let record: String = records
            .iter()
            .map(|p| fs::read_to_string(p).unwrap())
            .collect();
        let _ = fs::remove_dir_all(&root);

        assert!(out.status.success(), "the child failed: {stderr}");
        assert_eq!(records.len(), 2, "two panics, two records: {stderr}");
        assert!(record.contains("thread: drill"), "{record}");
        assert!(record.contains("at: src/diag.rs:"), "{record}");
        assert!(record.contains("is not a char boundary"), "{record}");
        assert!(record.contains("was refused"), "{record}");
        assert!(stderr.contains("PANIC on thread drill"), "{stderr}");
        for leak in ["hunter2", "\u{2605}"] {
            assert!(
                !record.contains(leak),
                "the record quotes {leak:?}: {record}"
            );
            assert!(!stderr.contains(leak), "stderr quotes {leak:?}: {stderr}");
        }
    }

    #[test]
    fn report_lines_keeps_the_newest() {
        let log: String = (0..200)
            .map(|i| format!("terminal-delight: line {i}\n"))
            .collect();
        let lines = report_lines(&log, 80);
        assert_eq!(lines.len(), 80);
        assert_eq!(lines.last().unwrap(), "terminal-delight: line 199");
    }

    #[test]
    fn a_census_counts_kinds_and_never_names_a_program() {
        use crate::pane::PaneMode;
        let c = Census::of(
            2,
            &[
                PaneMode::Shell,
                PaneMode::Claude,
                PaneMode::Claude,
                PaneMode::Other("secret-tool".into()),
                PaneMode::Unknown,
            ],
        );
        assert_eq!(c.panes, 5);
        assert_eq!(
            c.kinds,
            vec![("shell", 1), ("claude", 2), ("other", 1), ("unknown", 1)]
        );
        assert!(!format!("{c:?}").contains("secret-tool"));
    }

    #[test]
    fn the_agent_run_line_names_only_the_report() {
        let line = agent_run_line("claude");
        assert!(line.starts_with("claude '"));
        assert!(line.contains("report.md"));
        assert_eq!(
            line.matches('\'').count(),
            2,
            "one single-quoted argument: {line}"
        );
    }

    #[test]
    fn a_host_log_name_is_safe_for_any_session_key() {
        assert_eq!(Role::Host("a/../b".into()).log_name(), "host-a____b.log");
        assert_eq!(Role::Window.log_name(), "window.log");
    }
}
