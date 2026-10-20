//! What a NEW agent opens holding — `~/.config/terminal-delight/launch.toml`.
//!
//! The LAUNCH AGENT panel used to open on three constants written into
//! [`crate::Workspace::open_agent_launcher`]: claude, the first model in the
//! list, and the harness's own middle-high effort. Every launch that wanted
//! something else paid for it in keystrokes, every time, and nothing on this
//! machine could say otherwise. Parker, on spinning one up from the bench:
//! *"its default[s should be] configured!"*
//!
//! So the three live in a file, and the file is edited from the top of the
//! usage card — the page that already answers *what are these agents costing*,
//! which is the same question one step earlier.
//!
//! **Unknown is not zero, and undeclared is not none.** Every field is an
//! `Option` and the absence is load-bearing: `None` means *nobody has chosen*,
//! which is a different fact from *somebody chose exactly what the harness
//! would have done anyway*. The panel resolves an unset field to the shipped
//! behaviour at the point of use — it is never filled in here, because a
//! default written into the store is indistinguishable from a decision the
//! moment it is read back. The surface draws the difference (`unset · harness
//! default` against a lit chip), so the distinction survives all the way to a
//! person's eye.

use crate::launcher::Harness;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The three things the LAUNCH AGENT panel opens holding, as words.
///
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Which agent program a new session starts. `None` = nobody has chosen.
    pub harness: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
}

impl Prefs {
    /// The harness a new agent starts on, or `None` while nobody has said.
    ///
    /// A word no harness answers to reads as unset rather than as the first
    /// harness in the list — a hand-edited typo must not silently start a
    /// different program from the one the file names.
    pub fn harness(&self) -> Option<Harness> {
        Harness::from_label(self.harness.as_deref()?)
    }

    /// Which row of `h`'s model menu the panel opens on, if the stored model
    /// is one `h` offers at all.
    ///
    /// `None` covers both *nobody chose* and *the choice belongs to the other
    /// harness* — `gpt-5-codex` under Claude is not a Claude model, and the
    /// honest answer is to fall back rather than to index a different list.
    pub fn model_ix(&self, models: &[crate::model_catalog::Model]) -> Option<usize> {
        let want = self.model.as_deref()?;
        models
            .iter()
            .position(|model| model.id.as_deref() == Some(want))
    }
}

pub fn config_path() -> PathBuf {
    crate::instance::config_dir().join("launch.toml")
}

/// Load the defaults. A missing or unparsable file is [`Prefs::default`] — all
/// three unset — because a typo in a hand-edited file must not launch an agent
/// on a model nobody asked for.
pub fn load() -> Prefs {
    load_from(&config_path())
}

fn load_from(path: &Path) -> Prefs {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

/// Write the defaults whole. Last write wins across windows, exactly like
/// `notifications.toml` — two windows disagreeing about which model to open on
/// is a person changing their mind, not a conflict worth a lock.
pub fn save(p: &Prefs) {
    save_to(&config_path(), p);
}

fn save_to(path: &Path, p: &Prefs) {
    if let Ok(body) = toml::to_string(p) {
        let _ = crate::session::write_atomic(path, &body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn models(harness: Harness) -> Vec<crate::model_catalog::Model> {
        let ids = match harness {
            Harness::Claude => ["opus", "sonnet"],
            Harness::Codex => ["gpt-6.1-sol", "gpt-6-astra"],
            Harness::Local => ["local", "local"],
        };
        ids.into_iter()
            .map(|id| crate::model_catalog::Model {
                id: Some(id.into()),
                label: id.into(),
                resolved_id: None,
                efforts: None,
                default_effort: None,
            })
            .collect()
    }

    /// The branch that runs on every machine that has never opened the card.
    /// All three unset — NOT claude/opus/high, which is what a struct of
    /// concrete defaults would have made indistinguishable from a decision.
    #[test]
    fn an_absent_file_is_three_questions_nobody_has_answered() {
        let missing = PathBuf::from("/nonexistent/terminal-delight/launch.toml");
        let p = load_from(&missing);
        assert_eq!(p.harness(), None);
        assert_eq!(p.model_ix(&models(Harness::Claude)), None);
        assert_eq!(p.effort.as_deref(), None);
    }

    #[test]
    fn a_written_choice_reads_back_as_the_same_choice() {
        let dir = std::env::temp_dir().join(format!("td-launchpref-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("launch.toml");
        save_to(
            &path,
            &Prefs {
                harness: Some("codex".into()),
                model: Some("gpt-6-astra".into()),
                effort: Some("high".into()),
            },
        );
        let back = load_from(&path);
        assert_eq!(back.harness(), Some(Harness::Codex));
        assert_eq!(back.model_ix(&models(Harness::Codex)), Some(1));
        assert_eq!(back.effort.as_deref(), Some("high"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn future_effort_words_are_preserved_without_clamping() {
        let pref = Prefs {
            effort: Some("ultra".into()),
            ..Prefs::default()
        };
        let stored = toml::to_string(&pref).unwrap();
        let loaded: Prefs = toml::from_str(&stored).unwrap();
        assert_eq!(loaded.effort.as_deref(), Some("ultra"));
    }

    /// A model belonging to the other harness is not an index into this one's
    /// list. Two models deep in Claude's menu is not "the Codex one".
    #[test]
    fn a_model_from_the_other_harness_reads_as_unset_here() {
        let p = Prefs {
            model: Some("gpt-6.1-sol".into()),
            ..Prefs::default()
        };
        assert_eq!(p.model_ix(&models(Harness::Codex)), Some(0));
        assert_eq!(p.model_ix(&models(Harness::Claude)), None);
    }

    /// A hand-edited word nothing answers to is unset, not the first row.
    #[test]
    fn a_word_no_table_knows_is_unset_rather_than_the_first_entry() {
        let p = Prefs {
            harness: Some("gemini".into()),
            model: Some("o3".into()),
            effort: Some("standard".into()),
        };
        assert_eq!(p.harness(), None);
        assert_eq!(p.model_ix(&models(Harness::Claude)), None);
        assert_eq!(p.effort.as_deref(), Some("standard"));
    }

    #[test]
    fn a_half_written_file_leaves_the_other_two_unanswered() {
        let dir = std::env::temp_dir().join(format!("td-launchpref-half-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("launch.toml");
        std::fs::write(&path, "model = \"sonnet\"\n").unwrap();
        let p = load_from(&path);
        assert_eq!(p.model_ix(&models(Harness::Claude)), Some(1));
        assert_eq!(p.harness(), None, "an omitted field is unanswered");
        assert_eq!(p.effort.as_deref(), None);
        std::fs::remove_dir_all(&dir).ok();
    }
}
