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

use crate::launcher::{Effort, Harness};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The three things the LAUNCH AGENT panel opens holding, as words.
///
/// Words rather than the enums, because this is a file a person may open: a
/// line reading `effort = "xhigh"` is the same string the flag carries and the
/// same string the chip says. See [`Effort::id`] and [`Harness::label`], which
/// are the only tables that translate.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Which agent program a new session starts. `None` = nobody has chosen.
    pub harness: Option<String>,
    /// The model id the panel opens on — `opus`, `gpt-5-codex`. `None` = the
    /// first row of whichever harness is selected, which is what ships.
    pub model: Option<String>,
    /// The effort word the panel opens on. `None` = the harness's own
    /// [`Harness::default_effort`], and that is NOT the same as `"high"` being
    /// stored: one is a decision and the other is the absence of one.
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
    pub fn model_ix(&self, h: Harness) -> Option<usize> {
        let want = self.model.as_deref()?;
        h.models().iter().position(|m| m.id == want)
    }

    /// The effort level the panel opens on, already clamped to what `h` takes.
    ///
    /// Parsed against [`Effort::ALL`] rather than against `h.efforts()`: a file
    /// written while Claude was selected can hold `max`, Codex has no such
    /// level, and clamping it to `xhigh` keeps the choice where parsing against
    /// Codex's own list would have thrown it away.
    pub fn effort(&self, h: Harness) -> Option<Effort> {
        Effort::from_id(self.effort.as_deref()?).map(|e| h.clamp_effort(e))
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

    /// The branch that runs on every machine that has never opened the card.
    /// All three unset — NOT claude/opus/high, which is what a struct of
    /// concrete defaults would have made indistinguishable from a decision.
    #[test]
    fn an_absent_file_is_three_questions_nobody_has_answered() {
        let missing = PathBuf::from("/nonexistent/terminal-delight/launch.toml");
        let p = load_from(&missing);
        assert_eq!(p.harness(), None);
        assert_eq!(p.model_ix(Harness::Claude), None);
        assert_eq!(p.effort(Harness::Claude), None);
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
                model: Some("gpt-5".into()),
                effort: Some("high".into()),
            },
        );
        let back = load_from(&path);
        assert_eq!(back.harness(), Some(Harness::Codex));
        assert_eq!(back.model_ix(Harness::Codex), Some(1));
        assert_eq!(back.effort(Harness::Codex), Some(Effort::High));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A level one harness offers and the other does not survives the switch
    /// as the nearest thing the new harness takes, rather than as nothing.
    #[test]
    fn a_level_the_other_harness_lacks_is_clamped_and_not_dropped() {
        let p = Prefs {
            effort: Some("max".into()),
            ..Prefs::default()
        };
        assert_eq!(p.effort(Harness::Claude), Some(Effort::Max));
        assert_eq!(p.effort(Harness::Codex), Some(Effort::XHigh));
    }

    /// A model belonging to the other harness is not an index into this one's
    /// list. Two models deep in Claude's menu is not "the Codex one".
    #[test]
    fn a_model_from_the_other_harness_reads_as_unset_here() {
        let p = Prefs {
            model: Some("gpt-5-codex".into()),
            ..Prefs::default()
        };
        assert_eq!(p.model_ix(Harness::Codex), Some(0));
        assert_eq!(p.model_ix(Harness::Claude), None);
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
        assert_eq!(p.model_ix(Harness::Claude), None);
        assert_eq!(p.effort(Harness::Claude), None);
    }

    #[test]
    fn a_half_written_file_leaves_the_other_two_unanswered() {
        let dir = std::env::temp_dir().join(format!("td-launchpref-half-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("launch.toml");
        std::fs::write(&path, "model = \"sonnet\"\n").unwrap();
        let p = load_from(&path);
        assert_eq!(p.model_ix(Harness::Claude), Some(1));
        assert_eq!(p.harness(), None, "an omitted field is unanswered");
        assert_eq!(p.effort(Harness::Claude), None);
        std::fs::remove_dir_all(&dir).ok();
    }
}
