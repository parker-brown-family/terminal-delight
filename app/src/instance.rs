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
//! Identity is now a **session key**: the Hyprland workspace the window was born
//! on (`2`, `7`, `special-magic`), or `default` anywhere else. Each key owns
//! `sessions/<key>.toml`, and ownership is a kernel-held `flock` on
//! `sessions/<key>.lock` — the first window on a workspace takes the lock and
//! restores that workspace's layout; a second window on the *same* workspace
//! finds it busy and opens as a scratch window. That is the old second-launch
//! behaviour, now scoped to one workspace instead of the whole machine. The lock
//! lives on the open file description, so the kernel drops it however the process
//! dies — a SIGKILL can never leave a workspace looking occupied.
//!
//! The key is resolved once, at launch, and never re-resolved: a window dragged
//! to another workspace keeps the session it was born with. Re-keying on the move
//! would race two windows onto one state file to no real end.
//!
//! std + libc only — no gpui — so all of it stays unit-testable.

use std::fs::{DirBuilder, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// The key used off Hyprland, and by anything that cannot name a workspace —
/// one session, exactly like every version before this one.
pub const DEFAULT_KEY: &str = "default";

/// A workspace name is only ever this many characters of filename.
const KEY_MAX: usize = 64;

/// How long to wait on the compositor before falling back to [`DEFAULT_KEY`].
/// The socket is local and answers in microseconds; this exists purely so a
/// wedged compositor cannot hang the window open.
const IPC_TIMEOUT: Duration = Duration::from_millis(250);

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
    if let Some(b) = BOUND.get() {
        if let Ok(mut lock) = b.lock.lock() {
            let _ = lock.take();
        }
    }
}

/// The session key this process was bound to (before `bind`, and in tests, the
/// default — so every path helper stays usable without a compositor).
pub fn key() -> &'static str {
    BOUND.get().map(|b| b.key.as_str()).unwrap_or(DEFAULT_KEY)
}

// ---- paths ----

pub fn config_dir() -> PathBuf {
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

/// This window's session key, in precedence order:
///  1. `$TD_SESSION` — an explicit name; also the escape hatch for keeping
///     several restorable sessions without Hyprland.
///  2. the Hyprland workspace this window is about to open on. New windows land
///     on the active workspace, so asking before the window exists is both
///     possible and right.
///  3. [`DEFAULT_KEY`].
pub fn resolve_key() -> String {
    if let Ok(explicit) = std::env::var("TD_SESSION") {
        if !explicit.trim().is_empty() {
            return sanitize_key(&explicit);
        }
    }
    hypr_active_workspace()
        .map(|w| sanitize_key(&w))
        .unwrap_or_else(|| DEFAULT_KEY.to_string())
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
    let unarbitrated = Claim {
        owned: true,
        lock: None,
    };
    if DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(sessions_dir_in(config))
        .is_err()
    {
        return unarbitrated;
    }
    let Ok(mut file) = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(lock_file_in(config, key))
    else {
        return unarbitrated;
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

/// Claim the current user's config directory.
pub fn claim(key: &str) -> Claim {
    claim_in(&config_dir(), key)
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
        assert!(first.owned && first.lock.is_some(), "first window owns ws 2");
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
}
