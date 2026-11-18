//! Where Terminal Delight keeps things, resolved the way the app resolves them.
//!
//! Each of these mirrors a function in `app/src`: the runtime directory is
//! `ctl::ctl_dir`, the mailboxes are TDSP §2, the session files are
//! `session.rs`. They are restated rather than shared because the gateway is
//! not linked against the app, and they are short enough that restating them is
//! cheaper than a crate boundary.

use std::path::PathBuf;

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".into()))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    match std::env::var(var) {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => home().join(fallback),
    }
}

/// `$XDG_RUNTIME_DIR/terminal-delight`, or `/tmp/terminal-delight-<uid>` — the
/// private directory the session sockets live in.
pub fn runtime_dir() -> PathBuf {
    match std::env::var("XDG_RUNTIME_DIR") {
        Ok(v) if !v.is_empty() => PathBuf::from(v).join("terminal-delight"),
        _ => PathBuf::from(format!("/tmp/terminal-delight-{}", uid())),
    }
}

pub fn session_socket(key: &str) -> PathBuf {
    runtime_dir().join(format!("session-{key}.sock"))
}

/// A pane's mailbox: surfaces, the channel journals, the bench marker.
pub fn mailbox(session: &str, pane: u64) -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state")
        .join("terminal-delight/surfaces")
        .join(session)
        .join(pane.to_string())
}

/// The session's layout file, which the host writes and the desk's left bar is
/// drawn from.
pub fn session_file(key: &str) -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config")
        .join("terminal-delight/sessions")
        .join(format!("{key}.toml"))
}

/// The gateway's own state: the pairing token.
pub fn gateway_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("terminal-delight/mobile")
}

/// Claude Code keeps one transcript per conversation, named by its id, under a
/// directory named after the working directory. The id is enough to find it.
pub fn claude_projects() -> PathBuf {
    home().join(".claude/projects")
}

pub fn uid() -> u32 {
    // /proc/self is always ours; reading the owner of it avoids a libc
    // dependency for one number.
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self")
        .map(|m| m.uid())
        .expect("/proc/self is readable on any Linux this runs on")
}
