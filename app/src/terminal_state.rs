//! Curated visual contexts. Recognition here never grants agent capabilities or
//! says that a console is at a prompt or a remote connection succeeded.
use crate::pane::PaneMode;

pub const HOVER: &[&str] = &[
    "Colour by terminal state",
    "SSH / remote · agent provider",
    "Editor · system monitor · console",
    "Only panes following outer use presets.",
    "Shells and unknown programs keep outer.",
    "Your own pane colours take precedence.",
];

/// [`HOVER`] in the reader's language, line for line.
#[allow(dead_code)] // drawn by main.rs (the toggle's hover card, ~17589) once it adopts this
pub fn hover_in(l: crate::lang::Lang) -> [&'static str; 6] {
    let t = &l.strings().parts;
    [
        t.termstate_hover_title,
        t.termstate_hover_line_remote,
        t.termstate_hover_line_editor,
        t.termstate_hover_line_presets,
        t.termstate_hover_line_shells,
        t.termstate_hover_line_precedence,
    ]
}

// Accent, default text, divider, cursor. Keep the established agent/remote inks.
const CLAUDE: [u32; 4] = [0xf59e0b, 0xfbe3b0, 0x4a3410, 0xfbbf24];
const CODEX: [u32; 4] = [0x22d3ee, 0xc3f4fc, 0x0e3a44, 0x67e8f9];
const REMOTE: [u32; 4] = [0xc084fc, 0xead9fc, 0x3b2354, 0xd8b4fe];
const LOCAL: [u32; 4] = [0xf0a6ca, 0xf9e2ef, 0x482a3c, 0xf5c6df];
const PROVIDER: [u32; 4] = [0x78dfbd, 0xd5f6e9, 0x1c4436, 0xa3efd5];
const EDITOR: [u32; 4] = [0xe6cfab, 0xf6ead7, 0x49402f, 0xf2dfc2];
const MONITOR: [u32; 4] = [0x98d870, 0xe1f3d4, 0x2d4522, 0xb6eb93];
const CONSOLE: [u32; 4] = [0x91baff, 0xdeebff, 0x263954, 0xb9d3ff];

/// The host's measured owner wins. `Other` is an exact executable name, never
/// a command line or terminal text; a Node wrapper keeps the console look until
/// the existing detector can identify its owner. None means use outer.
pub fn colours(mode: &PaneMode) -> Option<[u32; 4]> {
    Some(match mode {
        PaneMode::Claude => CLAUDE,
        PaneMode::Codex => CODEX,
        PaneMode::Remote => REMOTE,
        PaneMode::Other(name) if name == crate::launcher::LOCAL_AGENT_MODE => LOCAL,
        PaneMode::Other(name) => match name.as_str() {
            "gemini" | "opencode" | "aider" | "copilot" | "crush" | "goose" => PROVIDER,
            "vim" | "nvim" | "vi" | "hx" | "helix" | "nano" | "emacs" => EDITOR,
            "top" | "htop" | "btop" | "nvtop" | "glances" => MONITOR,
            "python" | "python3" | "ipython" | "node" | "irb" | "julia" | "R" => CONSOLE,
            _ => return None,
        },
        PaneMode::Shell | PaneMode::Unknown => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hover card's translated lines are the English ones in English.
    #[test]
    fn the_hover_card_reads_the_same_in_english() {
        assert_eq!(hover_in(crate::lang::Lang::En).as_slice(), HOVER);
    }

    #[test]
    fn every_curated_alias_has_its_family_ink_without_becoming_an_agent() {
        for (names, expected) in [
            ("gemini opencode aider copilot crush goose", 0x78dfbd),
            ("vim nvim vi hx helix nano emacs", 0xe6cfab),
            ("top htop btop nvtop glances", 0x98d870),
            ("python python3 ipython node irb julia R", 0x91baff),
        ] {
            for name in names.split_whitespace() {
                let mode = PaneMode::Other(name.into());
                assert_eq!(colours(&mode).map(|c| c[0]), Some(expected), "{name}");
                assert!(
                    !mode.is_agent(),
                    "a visual preset grants no agent capabilities"
                );
                assert_eq!(mode.at_a_prompt(), Some(false));
            }
        }
        let local = PaneMode::Other(crate::launcher::LOCAL_AGENT_MODE.into());
        assert_eq!(colours(&local).unwrap()[0], 0xf0a6ca);
        assert!(
            local.is_agent(),
            "the declared adapter retains its existing identity"
        );
    }

    #[test]
    fn filenames_substrings_and_unrecognized_programs_do_not_claim_a_context() {
        for name in [
            "",
            "rg",
            "tmux",
            "screen",
            "myvim",
            "nvimdiff",
            "gemini.log",
            "vim /tmp/gemini",
            "/tmp/opencode",
            "python-project",
            "htop-backup",
        ] {
            assert_eq!(colours(&PaneMode::Other(name.into())), None, "{name}");
        }
        assert_eq!(colours(&PaneMode::Shell), None);
        assert_eq!(colours(&PaneMode::Unknown), None);
        assert_eq!(PaneMode::Unknown.at_a_prompt(), None);
    }
}
