//! Which saved session this window owns.
//!
//! Terminal Delight used to persist exactly one session: the first window wrote
//! `~/.config/terminal-delight/state.toml`, and every later launch opened a
//! throwaway scratch window that restored nothing and saved nothing. On a tiling
//! compositor that is the wrong unit of work. You keep a window per *workspace*
//! — one for the client, one for the tooling — and under the old rule every
//! window but the first was disposable, silently holding agents that no restart
//! could bring back.
//!
//! Identity is a **session id**: a stable handle TD owns (`1`, `2`, `work`),
//! never a property of the desktop. Each id owns `sessions/<id>.toml`, and
//! ownership is a kernel-held `flock` on `sessions/<id>.lock` — one live window
//! per session, every other window a scratch terminal. The lock lives on the
//! open file description, so the kernel drops it however the process dies; a
//! SIGKILL can never leave a session looking occupied.
//!
//! The first cut of this keyed sessions to *the Hyprland workspace the window was
//! born on*, which read well and failed badly: the key recorded birth while the
//! user reasons about location. Open on workspace 2, drag the window to 1, close
//! it there, reopen from there — and you got a stranger, because the session you
//! wanted was filed under a number you had left behind an hour ago. Workspaces
//! are switched, moved between, and renamed for reasons that have nothing to do
//! with sessions; ambient, user-mutable state makes a poor primary key.
//!
//! So launch *adopts* rather than invents: [`resolve_session`] takes the
//! most-recently-saved session nobody is holding, which is what "reopen my
//! terminal" has always meant. The workspace survives only as a **ranking hint**
//! — a session records where it was last saved, and a cold launch prefers the one
//! last seen here. Ambient data is fine for breaking ties and poison for naming
//! things, so a move or a rename now costs you a slightly different session
//! instead of all of them.
//!
//! std + libc only — no gpui — so all of it stays unit-testable.

use std::fs::{DirBuilder, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

/// The key used off Hyprland, and by anything that cannot name a workspace —
/// one session, exactly like every version before this one.
pub const DEFAULT_KEY: &str = "default";

/// A session id is only ever this many characters of filename.
const KEY_MAX: usize = 64;

/// How far [`fresh_session`] will count before giving up and opening a scratch
/// window. Far past any real desktop; it exists so the search always terminates.
const MAX_SESSIONS: usize = 1024;

/// How long to wait on the compositor before falling back to [`DEFAULT_KEY`].
/// The socket is local and answers in microseconds; this exists purely so a
/// wedged compositor cannot hang the window open.
const IPC_TIMEOUT: Duration = Duration::from_millis(250);

/// How long [`current_workspace`] trusts its last answer. Long enough that a
/// burst of saves costs one round trip, short enough that dragging a window to
/// another workspace is recorded almost immediately.
const WORKSPACE_TTL: Duration = Duration::from_secs(2);

// ---- process-wide binding ----

struct Bound {
    key: String,
    /// The ownership claim. Parked here for the life of the process because
    /// dropping the `File` closes the fd and releases the flock — which would
    /// hand the workspace away while our window is still on screen. Behind a
    /// mutex so [`release`] can drop it early at quit-start.
    lock: Mutex<Option<File>>,
}

static BOUND: OnceLock<Bound> = OnceLock::new();

/// Set by [`release`] at quit-start; read by the save path. Separate from the
/// lock so persistence is disarmed even when this process never held one.
static RELEASED: AtomicBool = AtomicBool::new(false);

/// Bind this process to `key`, holding `lock` open for the rest of the run.
/// Called once, from `main`, before any state is read or written.
pub fn bind(key: String, lock: Option<File>) {
    let _ = BOUND.set(Bound {
        key,
        lock: Mutex::new(lock),
    });
}

/// Release the ownership claim NOW, ahead of process exit. Wired to quit-start
/// (`on_app_quit` runs before the slow PTY/GPU teardown that keeps a closing
/// process alive for seconds), so a close → immediate reopen on the same
/// workspace re-claims the session and restores, instead of finding the dying
/// window still holding the lock and booting as a scratch terminal.
pub fn release() {
    // ORDER MATTERS. Disarm persistence BEFORE dropping the lock, never after:
    // between the two there is a window in which the session is claimable by a
    // new window while this dying process still believes it may write. A
    // periodic save landing in that window overwrites state the new owner has
    // already adopted — the corpse clobbering its successor.
    //
    // Before #195 the victim was always the same workspace, which made it look
    // like a niche race. Now that any window can adopt any free session, the
    // victim is whoever adopted next. Guarded by
    // `release_disarms_persistence_before_it_frees_the_lock`.
    RELEASED.store(true, Ordering::SeqCst);
    if let Some(b) = BOUND.get() {
        if let Ok(mut lock) = b.lock.lock() {
            let _ = lock.take();
        }
    }
}

/// True once [`release`] has run. This process has given its session up and must
/// not write to it again — the save path checks this before persisting.
pub fn released() -> bool {
    RELEASED.load(Ordering::SeqCst)
}

/// The session key this process was bound to (before `bind`, and in tests, the
/// default — so every path helper stays usable without a compositor).
pub fn key() -> &'static str {
    BOUND.get().map(|b| b.key.as_str()).unwrap_or(DEFAULT_KEY)
}

// ---- paths ----

/// THE config root, for every module. `$XDG_CONFIG_HOME/terminal-delight` when
/// that is set, else `~/.config/terminal-delight`.
///
/// This used to be derived independently in five places — `instance`, `theme`,
/// `plugins` and `dirlogo` each hardcoded `~/.config`, while `bell` alone
/// honoured `XDG_CONFIG_HOME`. On a box that sets XDG_CONFIG_HOME the bell
/// sounds and everything else lived in different directories, and every future
/// config bug had five sites to be wrong in. One accessor, one answer.
pub fn config_dir() -> PathBuf {
    if let Some(x) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(x).join("terminal-delight");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config/terminal-delight")
}

fn sessions_dir_in(config: &Path) -> PathBuf {
    config.join("sessions")
}

fn state_file_in(config: &Path, key: &str) -> PathBuf {
    sessions_dir_in(config).join(format!("{key}.toml"))
}

fn lock_file_in(config: &Path, key: &str) -> PathBuf {
    sessions_dir_in(config).join(format!("{key}.lock"))
}

/// Where this window reads and writes its layout.
pub fn state_path() -> PathBuf {
    state_file_in(&config_dir(), key())
}

/// The single state file every version before per-workspace sessions wrote.
pub fn legacy_state_path() -> PathBuf {
    config_dir().join("state.toml")
}

// ---- key resolution ----

/// A workspace name becomes a filename, so keep it to characters that cannot
/// walk out of the sessions directory: Hyprland's own `special:magic` lands as
/// `special-magic`, and a name of nothing but separators falls back to the
/// default rather than producing an empty filename.
pub fn sanitize_key(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .chars()
        .take(KEY_MAX)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if cleaned.chars().all(|c| c == '-') {
        DEFAULT_KEY.to_string()
    } else {
        cleaned
    }
}

/// The workspace this window is on, in the form a session records as its
/// `last_workspace` hint. `None` off Hyprland.
///
/// Cached, because this is read on every save and every save is a user action:
/// closing a tab, toggling a setting, dragging a split. Uncached, each one paid
/// a compositor round trip on the UI thread, with [`IPC_TIMEOUT`] as the tail. A
/// tie-break hint does not need to be fresher than a few seconds — and a window
/// dragged between workspaces is still recorded correctly, just a beat later.
pub fn current_workspace() -> Option<String> {
    static CACHE: OnceLock<Mutex<Option<Cached>>> = OnceLock::new();
    let ask = || hypr_active_workspace().map(|w| sanitize_key(&w));
    let cell = CACHE.get_or_init(|| Mutex::new(None));
    // A poisoned cache is not worth a panic over a tie-break hint: just ask.
    let Ok(mut slot) = cell.lock() else {
        return ask();
    };
    if let Some(hit) = slot.as_ref() {
        if hit.at.elapsed() < WORKSPACE_TTL {
            return hit.value.clone();
        }
    }
    let value = ask();
    *slot = Some(Cached {
        value: value.clone(),
        at: Instant::now(),
    });
    value
}

/// The last answer the compositor gave, and when it gave it.
struct Cached {
    value: Option<String>,
    at: Instant,
}

/// `$TD_SESSION`, if it names anything. The escape hatch that reaches any session
/// from any workspace — precedence #1 everywhere it appears.
fn explicit_session() -> Option<String> {
    std::env::var("TD_SESSION")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| sanitize_key(&s))
}

/// The key a *scratch* window borrows to read a theme from. It never writes, so
/// this only has to name a session whose look is the right one to inherit: the
/// session it was torn off from, which is the most recently saved one — held or
/// not. Ranking (rather than the old workspace number) is what keeps a tear-off
/// looking like its parent now that ids are not workspaces.
pub fn resolve_key() -> String {
    theme_key_in(
        &config_dir(),
        explicit_session().as_deref(),
        current_workspace().as_deref(),
    )
}

/// `explicit` and `here` are passed in rather than read from the environment —
/// the same discipline [`resolve_session_in`] follows, and for the same reason.
/// Reading `$TD_SESSION` in here made the tests non-hermetic in the one place it
/// matters most: TD exports `TD_SESSION` into every pane it opens, so running
/// `cargo test` INSIDE Terminal Delight resolved every key to the developer's
/// live session and two tests failed. They passed in CI, which has no TD_SESSION
/// — a green pipeline hiding a red desk. Guarded by
/// `the_theme_key_ignores_the_ambient_td_session_env`.
fn theme_key_in(config: &Path, explicit: Option<&str>, here: Option<&str>) -> String {
    explicit
        .map(str::to_string)
        .or_else(|| rank(scan_sessions(config), here).into_iter().next())
        .unwrap_or_else(|| DEFAULT_KEY.to_string())
}

// ---- session resolution ----

/// A saved session nobody has claimed yet — what a cold launch ranks, then tries
/// to take.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Candidate {
    id: String,
    /// The state file's mtime: how "most recent" is decided.
    saved: SystemTime,
    /// Where this session was last saved, if it recorded it. A hint, never an id.
    workspace: Option<String>,
    /// Panes the session holds, if it recorded them. `None` on files written
    /// before the field existed — counted as substantial, never demoted.
    panes: Option<usize>,
}

impl Candidate {
    /// A session worth preferring over a throwaway one. A single pane is what a
    /// "open a terminal, run one command, close it" window leaves behind; more
    /// than that is work someone arranged. Unknown (a pre-field file) counts as
    /// substantial — a session that predates the field must not be demoted by it.
    fn substantial(&self) -> bool {
        self.panes.is_none_or(|p| p > 1)
    }
}

/// Which session this window opens, and the claim that makes it ours:
///  1. `$TD_SESSION` — an explicit name, and the escape hatch for reaching any
///     session from anywhere. Honoured even if it names nothing yet.
///  2. the most-recently-saved session nobody is holding, preferring one last
///     saved on `here` when several are free.
///  3. a fresh id, when every saved session is live (or there are none).
pub fn resolve_session() -> (String, Claim) {
    resolve_session_in(
        &config_dir(),
        explicit_session().as_deref(),
        current_workspace().as_deref(),
    )
}

/// `explicit` and `here` are passed in rather than read from the environment, so
/// every branch of the precedence order is reachable from a test.
fn resolve_session_in(
    config: &Path,
    explicit: Option<&str>,
    here: Option<&str>,
) -> (String, Claim) {
    if let Some(id) = explicit {
        let claim = claim_in(config, id);
        return (id.to_string(), claim);
    }
    adopt_or_fresh(config, here)
}

/// Steps 2 and 3 of [`resolve_session`].
fn adopt_or_fresh(config: &Path, here: Option<&str>) -> (String, Claim) {
    for id in rank(scan_sessions(config), here) {
        let claim = claim_in(config, &id);
        if claim.owned {
            return (id, claim);
        }
    }
    fresh_session(config)
}

/// Every saved session on disk. Only `<id>.toml` counts: the sibling `.tmp`,
/// `.last-good` and hand-made rescue copies are not sessions and must never be
/// adopted as one.
fn scan_sessions(config: &Path) -> Vec<Candidate> {
    let Ok(entries) = std::fs::read_dir(sessions_dir_in(config)) else {
        return vec![];
    };
    entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension()? != "toml" {
                return None;
            }
            let id = path.file_stem()?.to_str()?.to_string();
            let saved = e.metadata().ok()?.modified().ok()?;
            let body = std::fs::read_to_string(&path).ok();
            let workspace = body
                .as_deref()
                .and_then(|b| toml_top_level_string(b, "last_workspace"));
            let panes = body
                .as_deref()
                .and_then(|b| toml_top_level_usize(b, "panes"));
            Some(Candidate {
                id,
                saved,
                workspace,
                panes,
            })
        })
        .collect()
}

/// Adoption order: this workspace first, then SUBSTANTIAL sessions, then newest.
/// Ties break on the id so the order is total and the tests cannot flake on two
/// files sharing a timestamp.
///
/// The substantial rule exists because recency alone gets bulldozed. Open a
/// second window to run one command, close it, and that one-pane session is now
/// the most recently saved — so the next launch adopts IT and the twelve-tab
/// session you actually work in becomes reachable only by knowing `$TD_SESSION`
/// exists. A throwaway must never displace arranged work.
///
/// It is deliberately a CLASS test, not "biggest wins": among real sessions
/// recency still decides, so today's work beats last week's. Ranking purely on
/// size would make a session you abandoned permanently sticky.
fn rank(mut cands: Vec<Candidate>, here: Option<&str>) -> Vec<String> {
    cands.sort_by(|a, b| {
        let mine = |c: &Candidate| here.is_some() && c.workspace.as_deref() == here;
        mine(b)
            .cmp(&mine(a))
            .then_with(|| b.substantial().cmp(&a.substantial()))
            .then_with(|| b.saved.cmp(&a.saved))
            .then_with(|| a.id.cmp(&b.id))
    });
    cands.into_iter().map(|c| c.id).collect()
}

/// The lowest unused ordinal, claimed. Bounded because a machine with a thousand
/// live sessions is a bug report, not a workflow — and an unbounded search here
/// would hang the window open.
fn fresh_session(config: &Path) -> (String, Claim) {
    for n in 1..=MAX_SESSIONS {
        let id = n.to_string();
        if state_file_in(config, &id).exists() {
            continue;
        }
        let claim = claim_in(config, &id);
        if claim.owned {
            return (id, claim);
        }
    }
    // Every ordinal is spoken for: open as a scratch window rather than fight.
    (
        DEFAULT_KEY.to_string(),
        Claim {
            owned: false,
            lock: None,
        },
    )
}

/// Pull `key = "value"` out of the top-level table of a TOML file, stopping at
/// the first `[table]` header so a pane's own `last_workspace` could never be
/// mistaken for the session's. Hand-rolled for the same reason [`json_string`]
/// is: one string of one file does not earn a parser in the boot path.
/// A top-level integer, read the same cheap way as [`toml_top_level_string`] —
/// scan_sessions runs on every launch and must not deserialise whole sessions.
fn toml_top_level_usize(src: &str, key: &str) -> Option<usize> {
    for line in src.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            return None;
        }
        let Some(rest) = line.strip_prefix(key) else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        return rest.trim().parse().ok();
    }
    None
}

fn toml_top_level_string(src: &str, key: &str) -> Option<String> {
    for line in src.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            return None;
        }
        let Some(rest) = line.strip_prefix(key) else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start().strip_prefix('"')?;
        return rest.find('"').map(|end| rest[..end].to_string());
    }
    None
}

// ---- ownership ----

/// The outcome of claiming a session key: whether this window owns the saved
/// session, and the lock file whose open fd is what makes that true.
pub struct Claim {
    pub owned: bool,
    pub lock: Option<File>,
}

/// Take ownership of `key`'s saved session, or report it taken.
///
/// `flock`, not a pid file: the kernel releases it when the last fd on the open
/// file description closes, so a crash, an OOM kill, or a compositor restart can
/// never strand a workspace. A config directory we cannot even create means
/// persistence is broken for everyone, and a normal window is still better than
/// a scratch one, so that case claims ownership without a lock to show for it.
pub fn claim_in(config: &Path, key: &str) -> Claim {
    // FAIL CLOSED. This used to return `owned: true, lock: None` when the lock
    // could not be arbitrated, on the reasoning that a normal window beats a
    // scratch one. That trade is wrong: an unarbitrated claim is indistinguishable
    // from a real one, so TWO windows can both believe they own a session and
    // both resume its agents — two Claude processes on one conversation, both
    // billing, both writing. A scratch window is an annoyance you can see; a
    // duplicated agent is a cost you find out about later.
    //
    // #195 widened the exposure: adoption walks a list of candidates, so a launch
    // now makes several claim attempts where it used to make one.
    let unarbitrated = |why: &str| {
        eprintln!(
            "terminal-delight: cannot arbitrate ownership of session '{key}' ({why}); \
             opening a scratch window rather than risk two windows owning it. \
             State will not be saved."
        );
        Claim {
            owned: false,
            lock: None,
        }
    };
    if DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(sessions_dir_in(config))
        .is_err()
    {
        return unarbitrated("sessions directory is not creatable");
    }
    let Ok(mut file) = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(lock_file_in(config, key))
    else {
        return unarbitrated("lock file is not openable");
    };
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Claim {
            owned: false,
            lock: None,
        };
    }
    // Purely so `cat sessions/2.lock` names the window holding the workspace.
    let _ = file.set_len(0);
    let _ = writeln!(file, "{}", std::process::id());
    Claim {
        owned: true,
        lock: Some(file),
    }
}

/// One-time upgrade from the single-session era: the old `state.toml` becomes
/// whichever session claims it first. The `rename` *is* the claim — atomic, so
/// two windows starting together cannot both inherit it, and nothing is left
/// behind for a third to adopt later.
pub fn adopt_legacy(legacy: &Path, dest: &Path) -> bool {
    if dest.exists() || !legacy.exists() {
        return false;
    }
    if let Some(dir) = dest.parent() {
        let _ = DirBuilder::new().recursive(true).mode(0o700).create(dir);
    }
    std::fs::rename(legacy, dest).is_ok()
}

/// Where every single-session build parked its machine-global master lock.
/// Probed — never kept — during the legacy upgrade: a held lock means a
/// pre-upgrade window is LIVE, still treating `state.toml` as its working
/// state and still holding the agents recorded in it.
fn legacy_master_lock_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("terminal-delight-master.lock")
}

/// True while a pre-upgrade (single-session) window is still running. Adopting
/// `state.toml` under a live master would resurrect every agent that window is
/// still running — one conversation, two processes burning tokens — and the
/// master rewrites the file on its next save anyway, re-arming the same trap
/// for the next launch. So the upgrade simply waits: once the old window
/// exits, its final save lands in `state.toml` and the next launch adopts it.
pub fn legacy_master_live() -> bool {
    legacy_master_live_at(&legacy_master_lock_path())
}

fn legacy_master_live_at(path: &Path) -> bool {
    let Ok(file) = OpenOptions::new().write(true).open(path) else {
        return false; // no lock file → no pre-upgrade window ever ran here
    };
    // Non-blocking probe. Success takes the lock for the lifetime of this fd —
    // dropping `file` at return releases it untouched, so the probe can never
    // steal a workspace from anyone.
    unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) != 0 }
}

// ---- Hyprland IPC ----

/// Ask the compositor which workspace is active. Hyprland answers on a unix
/// socket under its instance signature, so this needs no `hyprctl` on PATH and
/// puts no subprocess in the boot path.
fn hypr_active_workspace() -> Option<String> {
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let runtime = std::env::var("XDG_RUNTIME_DIR").ok();
    socket_candidates(&sig, runtime.as_deref())
        .into_iter()
        .find_map(|sock| request(&sock, "j/activeworkspace"))
        .and_then(|reply| json_string(&reply, "name"))
}

/// Hyprland ≥0.40 keeps its sockets under `$XDG_RUNTIME_DIR/hypr/<sig>/`; older
/// builds used `/tmp/hypr/<sig>/`. Try the current layout first.
fn socket_candidates(sig: &str, runtime: Option<&str>) -> Vec<PathBuf> {
    let mut out = vec![];
    if let Some(rt) = runtime.filter(|r| !r.is_empty()) {
        out.push(Path::new(rt).join("hypr").join(sig).join(".socket.sock"));
    }
    out.push(Path::new("/tmp/hypr").join(sig).join(".socket.sock"));
    out
}

/// One request/response on the compositor's control socket. Hyprland closes the
/// connection once it has answered, which is what ends the read.
fn request(sock: &Path, cmd: &str) -> Option<String> {
    let mut stream = UnixStream::connect(sock).ok()?;
    stream.set_read_timeout(Some(IPC_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(IPC_TIMEOUT)).ok()?;
    stream.write_all(cmd.as_bytes()).ok()?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply).ok()?;
    (!reply.is_empty()).then_some(reply)
}

/// Pull `"<key>": "<value>"` out of a flat JSON reply. Hyprland puts `id` and
/// `name` first, ahead of anything user-controlled like a window title, so the
/// first match is the field we asked for — and a whole JSON dependency for one
/// string of one reply would be more machinery than it earns.
fn json_string(src: &str, key: &str) -> Option<String> {
    let after_key = src.find(&format!("\"{key}\""))? + key.len() + 2;
    let rest = src.get(after_key..)?.trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("td-inst-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_workspace_name_becomes_a_safe_filename() {
        assert_eq!(sanitize_key("2"), "2");
        assert_eq!(sanitize_key(" 7 "), "7");
        assert_eq!(sanitize_key("special:magic"), "special-magic");
        assert_eq!(sanitize_key("client work"), "client-work");
        // nothing may walk out of the sessions directory
        assert_eq!(sanitize_key("../../etc/passwd"), "------etc-passwd");
        assert!(!sanitize_key("../../etc/passwd").contains('/'));
        // an empty or separator-only name is no name at all
        assert_eq!(sanitize_key(""), DEFAULT_KEY);
        assert_eq!(sanitize_key("///"), DEFAULT_KEY);
        assert_eq!(sanitize_key(&"x".repeat(200)).len(), KEY_MAX);
    }

    #[test]
    fn each_key_gets_its_own_state_file() {
        let config = Path::new("/cfg");
        assert_eq!(
            state_file_in(config, "2"),
            Path::new("/cfg/sessions/2.toml")
        );
        assert_eq!(
            lock_file_in(config, "special-magic"),
            Path::new("/cfg/sessions/special-magic.lock")
        );
        // the unbound default keeps a single-session install on one file
        assert_eq!(
            state_file_in(config, DEFAULT_KEY),
            Path::new("/cfg/sessions/default.toml")
        );
    }

    #[test]
    fn one_window_per_key_and_the_lock_dies_with_it() {
        let config = tmp("claim");
        let first = claim_in(&config, "2");
        assert!(
            first.owned && first.lock.is_some(),
            "first window owns ws 2"
        );
        // a second window on the same workspace is a scratch window
        assert!(!claim_in(&config, "2").owned, "ws 2 is taken");
        // ...but a different workspace is free
        let other = claim_in(&config, "7");
        assert!(other.owned, "ws 7 is its own session");
        // releasing the fd hands the workspace straight back — this is what makes
        // a crashed window recoverable without any cleanup pass
        drop(first);
        assert!(claim_in(&config, "2").owned, "released on drop");
        std::fs::remove_dir_all(&config).unwrap();
    }

    #[test]
    fn the_old_single_session_file_is_adopted_exactly_once() {
        let config = tmp("adopt");
        let legacy = config.join("state.toml");
        std::fs::write(&legacy, "active = 0").unwrap();
        let first = state_file_in(&config, "2");
        assert!(adopt_legacy(&legacy, &first), "first session inherits it");
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "active = 0");
        assert!(!legacy.exists(), "the claim consumes the legacy file");
        // a second workspace starting later gets a fresh session, not a copy
        assert!(!adopt_legacy(&legacy, &state_file_in(&config, "7")));
        // and an existing session is never overwritten by a stray legacy file
        std::fs::write(&legacy, "active = 9").unwrap();
        assert!(!adopt_legacy(&legacy, &first));
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "active = 0");
        std::fs::remove_dir_all(&config).unwrap();
    }

    #[test]
    fn a_live_pre_upgrade_master_defers_the_legacy_adoption() {
        let config = tmp("legacy-live");
        let lock = config.join("master.lock");
        // no lock file at all: no pre-upgrade window ever ran → adopt freely
        assert!(!legacy_master_live_at(&lock));
        // a pre-upgrade window holds the old master lock → it is LIVE
        let held = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock)
            .unwrap();
        assert_eq!(
            unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        assert!(legacy_master_live_at(&lock), "held lock reads as live");
        // the probe must not have stolen the lock from the live window
        assert!(legacy_master_live_at(&lock), "probe is non-destructive");
        // the old window exits (fd closes, kernel releases) → adoption may run
        drop(held);
        assert!(!legacy_master_live_at(&lock), "released lock reads as gone");
        std::fs::remove_dir_all(&config).unwrap();
    }

    #[test]
    fn workspace_name_is_read_out_of_the_ipc_reply() {
        let reply = r#"{"id": 2, "name": "2", "monitor": "eDP-1", "windows": 3}"#;
        assert_eq!(json_string(reply, "name").as_deref(), Some("2"));
        assert_eq!(json_string(reply, "monitor").as_deref(), Some("eDP-1"));
        // a named workspace rides through as its name
        let named = r#"{"id": -99, "name": "special:magic", "monitor": "eDP-1"}"#;
        assert_eq!(json_string(named, "name").as_deref(), Some("special:magic"));
        assert_eq!(sanitize_key("special:magic"), "special-magic");
        // absent, non-string, and malformed fields all decline rather than guess
        assert_eq!(json_string(reply, "nope"), None);
        assert_eq!(json_string(reply, "id"), None);
        assert_eq!(json_string(r#"{"name": "unterminated"#, "name"), None);
        // an escaped quote inside the value does not end it early
        assert_eq!(
            json_string(r#"{"name": "a\"b"}"#, "name").as_deref(),
            Some("a\"b")
        );
    }

    #[test]
    fn socket_lookup_prefers_the_current_hyprland_layout() {
        let paths = socket_candidates("sig123", Some("/run/user/1000"));
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/run/user/1000/hypr/sig123/.socket.sock"),
                PathBuf::from("/tmp/hypr/sig123/.socket.sock"),
            ]
        );
        // no runtime dir: the legacy path is still worth a try
        assert_eq!(socket_candidates("sig123", None).len(), 1);
        assert_eq!(socket_candidates("sig123", Some("")).len(), 1);
    }

    // ---- session resolution ----

    /// Write `sessions/<id>.toml` with an mtime `age` seconds in the past, so a
    /// test can state "this one is older" instead of sleeping to earn it.
    fn session(config: &Path, id: &str, workspace: Option<&str>, age: u64) {
        let dir = config.join("sessions");
        std::fs::create_dir_all(&dir).unwrap();
        let body = match workspace {
            Some(w) => format!("active = 0\nlast_workspace = \"{w}\"\n\n[theme]\nid = \"x\"\n"),
            None => "active = 0\n\n[theme]\nid = \"x\"\n".to_string(),
        };
        let path = dir.join(format!("{id}.toml"));
        std::fs::write(&path, body).unwrap();
        let when = SystemTime::now() - Duration::from_secs(age);
        std::fs::File::open(&path)
            .unwrap()
            .set_modified(when)
            .unwrap();
    }

    fn cand(id: &str, age: u64, workspace: Option<&str>) -> Candidate {
        cand_of(id, age, workspace, None)
    }

    fn cand_of(id: &str, age: u64, workspace: Option<&str>, panes: Option<usize>) -> Candidate {
        Candidate {
            id: id.into(),
            saved: SystemTime::now() - Duration::from_secs(age),
            workspace: workspace.map(Into::into),
            panes,
        }
    }

    #[test]
    fn a_throwaway_terminal_cannot_bulldoze_an_arranged_session() {
        // THE case this rule exists for. You keep a big multi-project session.
        // You open a second window to run one command and close it — that
        // one-pane session is now the most recently saved. Recency alone would
        // adopt it and hide the real one behind $TD_SESSION.
        let order = rank(
            vec![
                cand_of("scratch", 10, None, Some(1)), // newest, trivial
                cand_of("work", 4000, None, Some(30)), // older, arranged
            ],
            None,
        );
        assert_eq!(
            order,
            vec!["work", "scratch"],
            "a one-pane session must never displace an arranged one"
        );
    }

    #[test]
    fn among_substantial_sessions_recency_still_decides() {
        // Deliberately NOT "biggest wins": ranking purely on size would make a
        // session you abandoned weeks ago permanently sticky.
        let order = rank(
            vec![
                cand_of("huge-but-stale", 400_000, None, Some(40)),
                cand_of("todays-work", 60, None, Some(6)),
            ],
            None,
        );
        assert_eq!(
            order,
            vec!["todays-work", "huge-but-stale"],
            "among real sessions the newest still wins"
        );
    }

    #[test]
    fn a_session_predating_the_pane_count_is_never_demoted() {
        // Files written before `panes` existed report None. Treating that as
        // trivial would silently push every pre-upgrade session behind a fresh
        // one-pane window on the first launch after upgrading.
        let order = rank(
            vec![
                cand_of("fresh-trivial", 10, None, Some(1)),
                cand_of("legacy", 5000, None, None),
            ],
            None,
        );
        assert_eq!(order, vec!["legacy", "fresh-trivial"]);
    }

    #[test]
    fn the_workspace_hint_still_outranks_substance() {
        // The workspace hint is the FIRST key: a session you were just standing
        // on stays the one you get back, trivial or not.
        let order = rank(
            vec![
                cand_of("big-elsewhere", 10, Some("9"), Some(30)),
                cand_of("small-here", 5000, Some("1"), Some(1)),
            ],
            Some("1"),
        );
        assert_eq!(order, vec!["small-here", "big-elsewhere"]);
    }

    #[test]
    fn the_most_recently_saved_session_is_adopted_first() {
        let order = rank(
            vec![
                cand("1", 300, None),
                cand("2", 5, None),
                cand("7", 60, None),
            ],
            None,
        );
        assert_eq!(order, vec!["2", "7", "1"]);
    }

    #[test]
    fn the_workspace_is_only_a_tie_break() {
        // "1" is much older, but it is the session that was last open HERE
        let order = rank(
            vec![cand("1", 300, Some("3")), cand("2", 5, Some("9"))],
            Some("3"),
        );
        assert_eq!(order, vec!["1", "2"]);
        // ...and standing somewhere else, recency alone decides
        assert_eq!(
            rank(
                vec![cand("1", 300, Some("3")), cand("2", 5, Some("9"))],
                Some("4")
            ),
            vec!["2", "1"]
        );
        // off Hyprland there is no hint at all
        assert_eq!(
            rank(vec![cand("1", 300, Some("3")), cand("2", 5, None)], None),
            vec!["2", "1"]
        );
    }

    #[test]
    fn a_cold_launch_reopens_the_session_you_last_used() {
        // The regression this whole change exists for. The real work opened on
        // workspace 2, was dragged to workspace 1, and was closed there — so it
        // is filed under the id `2` while its last save records workspace `1`.
        // An older, staler session happens to be called `1`.
        let config = tmp("adopt");
        session(&config, "1", Some("1"), 300);
        session(&config, "2", Some("1"), 10);
        // Reopening from workspace 1 must hand back the work, not the session
        // whose *name* matches where you happen to be standing.
        let (id, claim) = adopt_or_fresh(&config, Some("1"));
        assert_eq!(id, "2", "the newest free session, not the one named `1`");
        assert!(claim.owned);
    }

    #[test]
    fn a_live_session_is_skipped_for_the_next_one_down() {
        let config = tmp("skip");
        session(&config, "1", None, 300);
        session(&config, "2", None, 10);
        let held = adopt_or_fresh(&config, None); // takes "2" and keeps the flock
        assert_eq!(held.0, "2");
        let (id, claim) = adopt_or_fresh(&config, None);
        assert_eq!(id, "1", "the newest is busy, so take the next-newest");
        assert!(claim.owned);
        drop(held);
    }

    #[test]
    fn every_session_live_means_a_fresh_one_not_a_scratch_window() {
        let config = tmp("fresh");
        session(&config, "1", None, 10);
        let held = adopt_or_fresh(&config, None);
        assert_eq!(held.0, "1");
        let (id, claim) = adopt_or_fresh(&config, None);
        assert_eq!(id, "2", "the lowest unused ordinal");
        assert!(claim.owned);
        drop(held);
    }

    #[test]
    fn an_empty_install_starts_at_one() {
        let config = tmp("empty");
        let (id, claim) = adopt_or_fresh(&config, Some("7"));
        assert_eq!(id, "1", "the workspace never names the session");
        assert!(claim.owned);
    }

    #[test]
    fn only_real_session_files_are_ever_adopted() {
        let config = tmp("siblings");
        let dir = config.join("sessions");
        std::fs::create_dir_all(&dir).unwrap();
        // the debris that lives alongside a session: none of it is one
        for name in ["2.toml.last-good", "2.toml.tmp", "2.toml.rescue-2026-08-29"] {
            std::fs::write(dir.join(name), "active = 0\n").unwrap();
        }
        assert!(scan_sessions(&config).is_empty());
        let (id, _) = adopt_or_fresh(&config, None);
        assert_eq!(id, "1");
    }

    #[test]
    fn an_explicit_session_name_beats_everything_on_disk() {
        let config = tmp("explicit");
        session(&config, "1", Some("1"), 300);
        session(&config, "2", Some("1"), 10);
        // $TD_SESSION is the escape hatch: it must reach the session you named,
        // not the one adoption would have picked...
        let (id, claim) = resolve_session_in(&config, Some("1"), Some("1"));
        assert_eq!(id, "1");
        assert!(claim.owned);
        // ...including one that does not exist yet, so a name can be minted.
        let (id, claim) = resolve_session_in(&config, Some("client-work"), None);
        assert_eq!(id, "client-work");
        assert!(claim.owned);
        // and with nothing named, the same call adopts as usual
        assert_eq!(resolve_session_in(&config, None, Some("1")).0, "2");
    }

    #[test]
    fn a_scratch_window_borrows_the_look_of_the_session_it_came_from() {
        let config = tmp("theme");
        session(&config, "1", Some("1"), 300);
        session(&config, "4", Some("9"), 10);
        // A tear-off inherits from the newest session even though it is LIVE —
        // that is its parent. Ranking, not the workspace number, is what finds
        // it now that ids and workspaces have nothing to do with each other.
        let held = adopt_or_fresh(&config, None);
        assert_eq!(held.0, "4");
        assert_eq!(theme_key_in(&config, None, Some("9")), "4");
        // standing on workspace 1, the session last saved there wins the tie
        assert_eq!(theme_key_in(&config, None, Some("1")), "1");
        drop(held);
    }

    /// `set_var` is process-global and races with any concurrent `getenv`, so the
    /// handful of tests that must touch the environment take this first.
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn claim_in_fails_closed_when_ownership_cannot_be_arbitrated() {
        // A config root whose `sessions` path is a FILE: the directory can never
        // be created, so ownership cannot be arbitrated. The old behaviour was to
        // claim anyway (`owned: true, lock: None`), which lets two windows both
        // believe they own a session and both resume its agents. #188.
        let config = tmp("failclosed");
        std::fs::write(config.join("sessions"), b"not a directory").unwrap();
        let claim = claim_in(&config, "1");
        assert!(
            !claim.owned,
            "an unarbitrated claim must NOT report ownership — two windows \
             owning one session duplicates its agents"
        );
        assert!(claim.lock.is_none());
    }

    #[test]
    fn a_normal_claim_still_owns_and_holds_the_lock() {
        // the fail-closed path must not have cost the happy path its lock
        let config = tmp("okclaim");
        let claim = claim_in(&config, "1");
        assert!(claim.owned);
        assert!(claim.lock.is_some(), "a real claim carries the flock");
    }

    #[test]
    fn release_disarms_persistence_before_it_frees_the_lock() {
        // #189: the save path checks `released()`. If the flag were set AFTER the
        // lock were dropped, a periodic save could land in the gap and clobber
        // state a newly-adopting window had already taken.
        assert!(!released(), "nothing has released yet in this process");
        let config = tmp("release");
        let claim = claim_in(&config, "1");
        assert!(claim.owned);
        bind("1".into(), claim.lock);
        release();
        assert!(released(), "release() must disarm persistence");
    }

    #[test]
    fn the_config_root_follows_xdg_config_home() {
        // #190: five modules used to derive this independently and only `bell`
        // honoured XDG_CONFIG_HOME, so on a box that sets it the bell sounds and
        // everything else lived in different directories.
        let _guard = env_lock();
        let base = tmp("xdg");
        // SAFETY: serialised by `env_lock`; restored before the guard drops.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &base) };
        let got = config_dir();
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        assert_eq!(got, base.join("terminal-delight"));
    }

    #[test]
    fn the_theme_key_ignores_the_ambient_td_session_env() {
        // TD exports TD_SESSION into every pane it opens, so `cargo test` run
        // inside Terminal Delight has one set. `theme_key_in` must resolve from
        // its ARGUMENTS only — otherwise the developer's live session leaks into
        // every assertion and the suite goes red on the desk while staying green
        // in CI (which has no TD_SESSION). This is the regression that cost two
        // tests; it is cheap to pin and impossible to notice by hand.
        let _guard = env_lock();
        let config = tmp("ambient");
        session(&config, "1", None, 300);
        session(&config, "4", None, 10);
        // SAFETY: single-threaded within this test; the point is precisely that
        // the function under test must not consult this.
        unsafe { std::env::set_var("TD_SESSION", "not-a-real-session") };
        let got = theme_key_in(&config, None, None);
        unsafe { std::env::remove_var("TD_SESSION") };
        assert_eq!(
            got, "4",
            "theme_key_in read $TD_SESSION instead of its explicit argument"
        );
    }

    #[test]
    fn a_scratch_window_with_no_sessions_falls_back_to_the_default_look() {
        let config = tmp("theme-empty");
        assert_eq!(theme_key_in(&config, None, Some("7")), DEFAULT_KEY);
    }

    #[test]
    fn sessions_saved_in_the_same_instant_still_have_a_total_order() {
        // Equal mtimes must not leave the order down to readdir(), or which
        // session you get would vary run to run.
        let same = SystemTime::now();
        let at = |id: &str| Candidate {
            id: id.into(),
            saved: same,
            workspace: None,
            panes: None,
        };
        assert_eq!(
            rank(vec![at("7"), at("1"), at("3")], None),
            vec!["1", "3", "7"]
        );
        assert_eq!(
            rank(vec![at("3"), at("7"), at("1")], None),
            vec!["1", "3", "7"]
        );
    }

    #[test]
    fn the_workspace_hint_is_read_off_the_top_level_table_only() {
        assert_eq!(
            toml_top_level_string("active = 0\nlast_workspace = \"2\"\n", "last_workspace")
                .as_deref(),
            Some("2")
        );
        // a pane's own key, under a table header, is not the session's
        assert_eq!(
            toml_top_level_string(
                "active = 0\n[tabs.node.Leaf]\nlast_workspace = \"9\"\n",
                "last_workspace"
            ),
            None
        );
        assert_eq!(
            toml_top_level_string("active = 0\n", "last_workspace"),
            None
        );
        // a near-miss key must not answer for the real one
        assert_eq!(
            toml_top_level_string("last_workspace_id = \"4\"\n", "last_workspace"),
            None
        );
    }
}
