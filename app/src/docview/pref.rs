//! How HTML documents are drawn — `~/.config/terminal-delight/documents.toml`.
//!
//! ```toml
//! [html]
//! engine = "snapshot"            # unset = snapshot; "off" = always the desktop
//! chromium = "/usr/bin/chromium" # unset = the first of four names on PATH
//! gpu = "vulkan"                 # unset = headless defaults
//! network = "allowed"            # unset = blocked
//! ```
//!
//! **Unset is not a value.** Every field is an `Option`, and each module that
//! reads one decides what "nobody has said" means there, in one place. A word
//! TD does not know is an error that names it, never a quiet default: a typo
//! in `engine` must not silently pick an engine, and a typo in `network` must
//! not silently open the page to the internet.
//!
//! No `crate::` paths, so the engine tests can compile this file on its own;
//! where the file lives is `docview::prefs_path`.

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
pub struct Prefs {
    #[serde(default)]
    pub html: Html,
}

#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
pub struct Html {
    pub engine: Option<String>,
    pub chromium: Option<PathBuf>,
    pub gpu: Option<String>,
    pub network: Option<String>,
}

/// Which engine `documents.toml` asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineChoice {
    Snapshot,
    Off,
    Unknown(String),
}

impl Html {
    pub fn engine_choice(&self) -> EngineChoice {
        match self.engine.as_deref().map(str::trim) {
            None | Some("snapshot") => EngineChoice::Snapshot,
            Some("off") => EngineChoice::Off,
            Some(other) => EngineChoice::Unknown(other.to_string()),
        }
    }

    /// Whether the page may reach the network. Only the word "allowed" lets
    /// it; unset and "blocked" do not, and anything else is refused by
    /// [`load`] before this is ever asked.
    pub fn network_allowed(&self) -> bool {
        self.network.as_deref().map(str::trim) == Some("allowed")
    }

    pub fn vulkan(&self) -> bool {
        self.gpu.as_deref().map(str::trim) == Some("vulkan")
    }
}

/// Read the preferences. A missing file is every field unset; a file that
/// does not parse, or names a value TD does not know, is an error naming it.
pub fn load(path: &Path) -> Result<Prefs, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Prefs::default()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    parse(&text)
}

pub fn parse(text: &str) -> Result<Prefs, String> {
    let prefs: Prefs = toml::from_str(text).map_err(|e| e.message().to_string())?;
    if let Some(n) = prefs.html.network.as_deref().map(str::trim) {
        if n != "allowed" && n != "blocked" {
            return Err(format!(
                "network = \"{n}\" is neither \"allowed\" nor \"blocked\""
            ));
        }
    }
    if let Some(g) = prefs.html.gpu.as_deref().map(str::trim) {
        if g != "vulkan" {
            return Err(format!("gpu = \"{g}\" is not \"vulkan\""));
        }
    }
    Ok(prefs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_engine_name_is_an_error_not_a_quiet_default() {
        let servo = parse("[html]\nengine = \"servo\"\n").unwrap();
        assert_eq!(
            servo.html.engine_choice(),
            EngineChoice::Unknown("servo".into())
        );
        let off = parse("[html]\nengine = \"off\"\n").unwrap();
        assert_eq!(off.html.engine_choice(), EngineChoice::Off);
        assert_eq!(
            parse("").unwrap().html.engine_choice(),
            EngineChoice::Snapshot
        );
        let missing = load(Path::new("/nonexistent/terminal-delight/documents.toml")).unwrap();
        assert_eq!(missing.html.engine_choice(), EngineChoice::Snapshot);
    }

    #[test]
    fn the_page_is_off_the_network_unless_someone_says_allowed() {
        assert!(!parse("").unwrap().html.network_allowed());
        assert!(!parse("[html]\nnetwork = \"blocked\"\n")
            .unwrap()
            .html
            .network_allowed());
        assert!(parse("[html]\nnetwork = \"allowed\"\n")
            .unwrap()
            .html
            .network_allowed());
        // A typo is refused rather than read as either answer.
        assert!(parse("[html]\nnetwork = \"allow\"\n").is_err());
        assert!(parse("[html]\ngpu = \"cuda\"\n").is_err());
        assert!(parse("[html\n").is_err());
    }
}
