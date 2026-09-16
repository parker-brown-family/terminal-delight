//! What a finished agent is allowed to do to the rest of the desktop —
//! `~/.config/terminal-delight/notifications.toml`.
//!
//! Terminal Delight has always posted a system notification when an agent
//! finishes in a pane nobody is watching (see `notify`, and `agent_done` in
//! `main`). On omarchy that lands in Quickshell's daemon, which is the right
//! behaviour when the window is buried behind a browser and the wrong one when
//! twenty agent panes are finishing in front of you. There was no way to say
//! so: the bump to the system level was wired in, not chosen.
//!
//! This is the choice, and it lives inside Terminal Delight rather than in the
//! desktop's own notification settings — turning the daemon off there would
//! silence every application on the machine to quiet one.
//!
//! **Unknown is not zero.** A missing file means *nobody has chosen*, and the
//! answer to that is the behaviour that already shipped, not `false` for every
//! field: [`Prefs::default`] therefore reads `system = true`, and the absent
//! file path is the one the tests below pin. A serde `#[serde(default)]` on a
//! `bool` would have quietly made the unconfigured case "off" — the one branch
//! that runs on every machine that has never opened the panel.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The notification switches, one struct per file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Bump an unwatched agent's completion (and its question) to the system
    /// notification daemon. Off keeps the news inside Terminal Delight: the
    /// tab still wears its 🔔 badge and the pane still pings.
    pub system: bool,
    /// Raise the in-window marquee — the chasing-bulb banner across the top of
    /// the window — when an agent finishes. Off by default: it is the loud
    /// one, and nothing that loud should arrive unasked.
    pub marquee: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        // The shipped behaviour, not a row of falses. See the module note.
        Self {
            system: true,
            marquee: false,
        }
    }
}

pub fn config_path() -> PathBuf {
    crate::instance::config_dir().join("notifications.toml")
}

/// Load the switches. A missing or unparsable file is [`Prefs::default`],
/// never an error — a hand-edited typo must not silence a machine's
/// notifications, and it must not turn on the loud one either.
pub fn load() -> Prefs {
    load_from(&config_path())
}

fn load_from(path: &Path) -> Prefs {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

/// Write the switches whole. Last write wins across windows, exactly like
/// `dir-logos.toml` — two windows disagreeing about notifications is a user
/// changing their mind, not a conflict worth a lock.
pub fn save(p: Prefs) {
    save_to(&config_path(), p);
}

fn save_to(path: &Path, p: Prefs) {
    if let Ok(body) = toml::to_string(&p) {
        let _ = crate::session::write_atomic(path, &body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The branch that runs on every machine that has never opened the panel.
    /// It is NOT the all-false struct a derived `Default` would have given.
    #[test]
    fn an_absent_file_is_the_behaviour_that_already_shipped() {
        let missing = PathBuf::from("/nonexistent/terminal-delight/notifications.toml");
        let p = load_from(&missing);
        assert!(p.system, "an unconfigured machine still bumps to the system");
        assert!(!p.marquee, "and does not start shouting a marquee unasked");
    }

    #[test]
    fn a_half_written_file_keeps_the_default_for_the_field_it_omits() {
        let dir = std::env::temp_dir().join(format!("td-notifpref-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("half.toml");
        std::fs::write(&path, "marquee = true\n").unwrap();
        let p = load_from(&path);
        assert!(p.marquee);
        assert!(p.system, "the omitted field falls back to the default, not false");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_choice_survives_the_round_trip() {
        let dir = std::env::temp_dir().join(format!("td-notifpref-rt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("notifications.toml");
        let want = Prefs {
            system: false,
            marquee: true,
        };
        save_to(&path, want);
        assert_eq!(load_from(&path), want);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn garbage_reads_as_the_default_rather_than_a_panic() {
        let dir = std::env::temp_dir().join(format!("td-notifpref-junk-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("junk.toml");
        std::fs::write(&path, "this is not toml = = =\n").unwrap();
        assert_eq!(load_from(&path), Prefs::default());
        std::fs::remove_dir_all(&dir).ok();
    }
}
