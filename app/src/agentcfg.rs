//! How idle agents rest and sleep — `~/.config/terminal-delight/agents.toml`.
//!
//! Set from the CONFIG › Agents page. Parker, 2026-10-26, on what the page is
//! for: *"this will store ALL of our agents settings"*. The same rule as
//! `launch.toml`: **every field is an `Option`, and unset means nobody chose.**
//! The shipped behaviour — rest on after ten minutes, sleep on after two hours
//! of rest — is applied where the value is used and never written in, so a
//! file can always tell a decision from a default.

use crate::agentrest::Thresholds;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The shipped answers, used only where a field is unset.
pub const REST_AFTER_MIN: u32 = 10;
pub const SLEEP_AFTER_REST_MIN: u32 = 120;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Idle agents rest (freeze, memory pushed out). `None` = nobody chose.
    pub rest: Option<bool>,
    /// Minutes untouched and quiet before an agent rests.
    pub rest_after_min: Option<u32>,
    /// Resting agents fall asleep (ended, conversation kept).
    pub sleep: Option<bool>,
    /// Minutes of rest before an agent sleeps.
    pub sleep_after_rest_min: Option<u32>,
}

impl Prefs {
    pub fn rest_on(&self) -> bool {
        self.rest.unwrap_or(true)
    }
    pub fn sleep_on(&self) -> bool {
        self.rest_on() && self.sleep.unwrap_or(true)
    }
    pub fn rest_after_min(&self) -> u32 {
        self.rest_after_min
            .unwrap_or(REST_AFTER_MIN)
            .clamp(1, 24 * 60)
    }
    pub fn sleep_after_rest_min(&self) -> u32 {
        self.sleep_after_rest_min
            .unwrap_or(SLEEP_AFTER_REST_MIN)
            .clamp(1, 7 * 24 * 60)
    }
    pub fn thresholds(&self) -> Thresholds {
        Thresholds {
            rest_after: Duration::from_secs(u64::from(self.rest_after_min()) * 60),
            sleep_after_rest: Duration::from_secs(u64::from(self.sleep_after_rest_min()) * 60),
        }
    }
}

pub fn config_path() -> PathBuf {
    crate::instance::config_dir().join("agents.toml")
}

/// A missing or unparsable file is all unset: a typo must not switch a
/// person's agents off, or on.
pub fn load() -> Prefs {
    load_from(&config_path())
}

fn load_from(path: &Path) -> Prefs {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

/// Write the whole file. Last write wins across windows, like `launch.toml`.
pub fn save(p: &Prefs) {
    if let Ok(body) = toml::to_string(p) {
        let _ = crate::session::write_atomic(&config_path(), &body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_file_is_unset_and_resolves_to_the_shipped_answers() {
        let p = load_from(Path::new("/nonexistent/agents.toml"));
        assert_eq!(p, Prefs::default());
        assert!(p.rest_on() && p.sleep_on());
        assert_eq!(p.thresholds(), Thresholds::default());
        assert_eq!(toml::to_string(&p).unwrap().trim(), "");
    }

    #[test]
    fn a_choice_round_trips_and_sleep_needs_rest() {
        let p = Prefs {
            rest: Some(false),
            sleep: Some(true),
            sleep_after_rest_min: Some(45),
            ..Default::default()
        };
        let back: Prefs = toml::from_str(&toml::to_string(&p).unwrap()).unwrap();
        assert_eq!(back, p);
        assert!(!back.sleep_on(), "nothing sleeps that cannot rest first");
        assert_eq!(
            back.thresholds().sleep_after_rest,
            Duration::from_secs(45 * 60)
        );
    }

    #[test]
    fn a_half_written_file_reads_as_unset() {
        let dir = std::env::temp_dir().join(format!("td-agentcfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("agents.toml");
        std::fs::write(&path, "rest = tr").unwrap();
        let p = load_from(&path);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(p, Prefs::default());
    }
}
