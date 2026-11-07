//! The front door: a sentence first, then where it runs and with what.
//!
//! Parker, 2026-11-07: *"THe MAIN surface being a standard CHAT that activated
//! the AGENT LAUNCHer (chooose model and effort)"*, and *"we will have all the
//! TERMINALs running, but what it feels like is an ENTRY point"*. T3 Code's
//! new-thread composer is the specimen: type what you want, the project, model
//! and effort sit as chips under it, Enter starts the thread and goes there,
//! and `mod+Enter` starts it in the background and leaves you in a fresh
//! composer for the next one.
//!
//! The LAUNCH AGENT panel (Alt+A) asks the same questions in the other order —
//! project first, because it was built as a picker. Nothing here replaces it.
//! The launch underneath is the launcher's: a [`launcher::Recipe`] with the
//! sentence as its `opener`, so a chat that starts an agent and the panel type
//! one and the same line.
//!
//! # The far machine
//!
//! T3 lets a person pick which machine every thread runs on, and Theo's whole
//! remote pitch is not having to SSH by hand. This window has no server on the
//! far box to talk to, and the connected-sessions research said what a real
//! one would owe (caller identity, consent, revocation) before any of it is
//! built. So the far machine here is the honest small version: an opt-in list
//! in `machines.toml`, a launch that is an `ssh -t` line typed into a local
//! pane, and the far box's screen in gliff — Omarchy's Hyprland remote desktop,
//! which needs nothing but ssh — when the person wants to see it. Nothing on the
//! far side is read, so a far agent's row says unknown rather than idle.
//!
//! Pure: the composer's view is in `entry_view.rs`.

use crate::launcher::{self, Harness, Reach};
use std::path::{Path, PathBuf};

/// Where an agent starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Machine {
    /// What the chip says.
    pub name: String,
    /// The ssh destination (`omarchy-bfs`, `me@host`), or `None` for this machine.
    pub ssh: Option<String>,
}

impl Machine {
    /// This machine, under the name the kernel gives it.
    pub fn here(hostname: Option<&str>, fallback: &str) -> Machine {
        Machine {
            name: hostname
                .map(str::trim)
                .filter(|h| !h.is_empty())
                .unwrap_or(fallback)
                .to_string(),
            ssh: None,
        }
    }

    pub fn is_far(&self) -> bool {
        self.ssh.is_some()
    }
}

/// Where the list of far machines is kept.
pub fn machines_path(config: &Path) -> PathBuf {
    config.join("machines.toml")
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MachinesFile {
    #[serde(default)]
    machine: Vec<MachineEntry>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MachineEntry {
    name: String,
    ssh: String,
}

/// The far machines in `machines.toml`:
///
/// ```toml
/// [[machine]]
/// name = "farbox"
/// ssh = "omarchy-bfs"
/// ```
///
/// No file is no far machines, which is not an error: the chip simply does not
/// appear. A file that cannot be read, or that names a destination ssh could
/// take for an option, is an error the composer shows, and none of its
/// machines is offered — half a list is a list somebody did not write.
pub fn parse_machines(text: &str) -> Result<Vec<Machine>, String> {
    let file: MachinesFile = toml::from_str(text).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(file.machine.len());
    for m in file.machine {
        let name = m.name.trim().to_string();
        if name.is_empty() {
            return Err("a machine needs a name".into());
        }
        if !valid_destination(&m.ssh) {
            return Err(format!("{name}: {:?} is not an ssh destination", m.ssh));
        }
        out.push(Machine {
            name,
            ssh: Some(m.ssh),
        });
    }
    Ok(out)
}

/// Read the list, treating a missing file as an empty one.
pub fn load_machines(config: &Path) -> Result<Vec<Machine>, String> {
    match std::fs::read_to_string(machines_path(config)) {
        Ok(text) => parse_machines(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.to_string()),
    }
}

/// An ssh destination this window will put on a command line: a host alias,
/// `user@host`, or `user@host:port`-free forms of the same, and nothing a
/// shell or ssh would read as something else. A leading `-` is the one that
/// matters most: `ssh -oProxyCommand=…` is an option, not a host.
pub fn valid_destination(dest: &str) -> bool {
    !dest.is_empty()
        && dest.len() <= 255
        && !dest.starts_with('-')
        && dest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "@._-".contains(c))
        && dest.matches('@').count() <= 1
}

/// The line a local pane types to start `agent_line` on `machine`, in `cwd`.
///
/// For this machine it is the agent line itself. For a far one it is
/// `ssh -t <dest> '<cd …> && <agent line>'`: `-t` because an agent is an
/// interactive program and needs a terminal at the far end; the far shell
/// runs the quoted command. A `cwd` under this machine's home is written from
/// `~` — the far box's home is not this one's, and the same repositories live
/// at the same place under it far more often than at the same absolute path.
pub fn line_on(machine: &Machine, home: &Path, cwd: &Path, agent_line: &str) -> String {
    let Some(dest) = machine.ssh.as_deref() else {
        return agent_line.to_string();
    };
    let cd = match cwd.strip_prefix(home) {
        Ok(rel) if rel.as_os_str().is_empty() => "cd ~".to_string(),
        // `~/` stays outside the quotes so the far shell expands it.
        Ok(rel) => format!("cd ~/{}", launcher::shell_quote(&rel.to_string_lossy())),
        Err(_) => format!("cd {}", launcher::shell_quote(&cwd.to_string_lossy())),
    };
    format!(
        "ssh -t {dest} {}",
        launcher::shell_quote(&format!("{cd} && {agent_line}"))
    )
}

/// What shows a far machine's screen: gliff, with a private headless output
/// sized to its own window rather than a mirror of whatever the far box's
/// monitor shows (often a lock screen — the farbox idles to one). `None` for
/// this machine.
pub fn screen_command(machine: &Machine) -> Option<Vec<String>> {
    let dest = machine.ssh.as_deref()?;
    Some(vec!["gliff".into(), "--headless".into(), dest.to_string()])
}

/// A new tab's name, from the first words of what was asked.
///
/// T3 names a project-less thread's folder after *"the first words of its
/// first message"*, and the inbox needs a name the moment the tab exists — a
/// row reading `shell` for the agent you just started is the window forgetting
/// what you said. Cut at a word, never mid-word, and never ending on a comma.
pub fn tab_name(text: &str, max_chars: usize) -> Option<String> {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return None;
    }
    let mut out = String::new();
    for word in flat.split(' ') {
        let next = if out.is_empty() {
            word.chars().count()
        } else {
            out.chars().count() + 1 + word.chars().count()
        };
        if next > max_chars {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.is_empty() {
        // One word longer than the whole budget: take its start.
        out = flat.chars().take(max_chars).collect();
    }
    let trimmed = out
        .trim_end_matches(|c: char| ",;:-–—".contains(c))
        .trim_end()
        .to_string();
    (!trimmed.is_empty()).then_some(trimmed)
}

/// Letters and digits only, lowercased: how a directory and a left-bar project
/// are compared. `terminal-delight` and `TERMINAL DELIGHT` are one project.
fn squash(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The left-bar project a directory belongs to, by name, when exactly one
/// project's name matches the directory's. Two matches is no answer: filing a
/// terminal under the wrong one of two is worse than filing it where the person
/// is standing.
pub fn project_by_name(dir: &Path, projects: &[(u32, String)]) -> Option<u32> {
    let want = squash(&dir.file_name()?.to_string_lossy());
    if want.is_empty() {
        return None;
    }
    let mut hits = projects.iter().filter(|(_, name)| squash(name) == want);
    let first = hits.next()?;
    hits.next().is_none().then_some(first.0)
}

/// What the keyboard is on inside the composer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Text,
    Project,
    Harness,
    Model,
    Effort,
    Reach,
    Machine,
}

impl Focus {
    /// The ring Tab walks. The machine chip only joins it when there is a far
    /// machine to choose.
    pub fn ring(far: bool) -> Vec<Focus> {
        let mut ring = vec![
            Focus::Text,
            Focus::Project,
            Focus::Harness,
            Focus::Model,
            Focus::Effort,
            Focus::Reach,
        ];
        if far {
            ring.push(Focus::Machine);
        }
        ring
    }

    /// The next stop on the ring, or the previous one going back.
    pub fn step(self, far: bool, back: bool) -> Focus {
        let ring = Focus::ring(far);
        let at = ring.iter().position(|f| *f == self).unwrap_or(0);
        let n = ring.len();
        ring[if back { (at + n - 1) % n } else { (at + 1) % n }]
    }
}

/// Where the person goes after sending.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Send {
    /// Enter: the agent's new tab comes forward and the composer closes.
    Open,
    /// Ctrl+Enter: the agent starts in a new tab behind, and the composer
    /// stays, empty, for the next thing.
    Background,
}

/// Step a value through a list, wrapping, forward or back. `None` in the
/// list is a real choice — the harness's own default — and steps like any
/// other.
pub fn cycle<T: Clone + PartialEq>(choices: &[T], current: &T, back: bool) -> Option<T> {
    if choices.is_empty() {
        return None;
    }
    let n = choices.len();
    let at = choices.iter().position(|c| c == current);
    let next = match (at, back) {
        (Some(i), false) => (i + 1) % n,
        (Some(i), true) => (i + n - 1) % n,
        (None, false) => 0,
        (None, true) => n - 1,
    };
    Some(choices[next].clone())
}

/// What the composer would start, before the launcher prepares a briefing for
/// it. `opener` is the sentence, trimmed; an empty sentence starts an agent
/// that waits to be told, the way the panel's launch does.
pub fn recipe(
    harness: Harness,
    model: Option<String>,
    effort: Option<String>,
    reach: Reach,
    cwd: PathBuf,
    sentence: &str,
) -> launcher::Recipe {
    let opener = sentence.trim();
    launcher::Recipe {
        harness,
        model,
        effort,
        reach,
        cwd,
        opener: (!opener.is_empty()).then(|| opener.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn far(dest: &str) -> Machine {
        Machine {
            name: "farbox".into(),
            ssh: Some(dest.into()),
        }
    }

    #[test]
    fn this_machine_runs_the_agent_line_as_it_is() {
        let here = Machine::here(Some("legion\n"), "this machine");
        assert_eq!(here.name, "legion");
        assert!(!here.is_far());
        let line = line_on(
            &here,
            Path::new("/home/p"),
            Path::new("/home/p/Work/td"),
            "claude 'go'",
        );
        assert_eq!(line, "claude 'go'");
        assert_eq!(screen_command(&here), None);
    }

    #[test]
    fn a_far_machine_runs_it_over_ssh_from_the_same_place_under_its_own_home() {
        let line = line_on(
            &far("omarchy-bfs"),
            Path::new("/home/p"),
            Path::new("/home/p/Work/terminal delight"),
            "claude --model opus 'fix it'",
        );
        assert_eq!(
            line,
            "ssh -t omarchy-bfs 'cd ~/'\\''Work/terminal delight'\\'' && claude --model opus '\\''fix it'\\'''"
        );
    }

    #[test]
    fn the_far_line_survives_a_real_shell_and_reaches_ssh_as_two_words() {
        // What ssh actually receives, by asking a shell to word-split the line
        // the way the pane's shell will: `printf '%s\n'` prints one argument a
        // line. The far end then gets the command string in one piece.
        let line = line_on(
            &far("me@box"),
            Path::new("/home/p"),
            Path::new("/home/p/it's here"),
            "claude 'a \"quoted\" ask'",
        );
        let script = line.replacen("ssh -t", "printf '%s\\n'", 1);
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(&script)
            .output()
            .expect("sh");
        let words: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect();
        assert_eq!(words.len(), 2, "{words:?}");
        assert_eq!(words[0], "me@box");
        assert_eq!(
            words[1],
            "cd ~/'it'\\''s here' && claude 'a \"quoted\" ask'"
        );
    }

    #[test]
    fn a_path_outside_home_is_written_whole_and_home_itself_is_tilde() {
        let m = far("box");
        assert_eq!(
            line_on(&m, Path::new("/home/p"), Path::new("/srv/x"), "codex"),
            "ssh -t box 'cd '\\''/srv/x'\\'' && codex'"
        );
        assert_eq!(
            line_on(&m, Path::new("/home/p"), Path::new("/home/p"), "codex"),
            "ssh -t box 'cd ~ && codex'"
        );
    }

    #[test]
    fn a_destination_ssh_would_read_as_an_option_is_refused() {
        assert!(valid_destination("omarchy-bfs"));
        assert!(valid_destination("parker@omarchy-bfs.local"));
        assert!(!valid_destination("-oProxyCommand=touch /tmp/x"));
        assert!(!valid_destination("box; rm -rf ~"));
        assert!(!valid_destination("a@b@c"));
        assert!(!valid_destination(""));
        assert!(!valid_destination("box name"));
    }

    #[test]
    fn the_machines_file_is_all_or_nothing() {
        let ok = parse_machines("[[machine]]\nname = \"farbox\"\nssh = \"omarchy-bfs\"\n").unwrap();
        assert_eq!(ok, vec![far("omarchy-bfs")]);
        assert_eq!(parse_machines("").unwrap(), vec![]);
        assert!(parse_machines(
            "[[machine]]\nname = \"a\"\nssh = \"good\"\n[[machine]]\nname = \"b\"\nssh = \"-bad\"\n"
        )
        .is_err());
        assert!(parse_machines("[[machine]]\nname = \"a\"\nssh = \"x\"\nport = 22\n").is_err());
        assert!(parse_machines("[[machine]]\nname = \" \"\nssh = \"x\"\n").is_err());
    }

    #[test]
    fn a_missing_machines_file_is_no_far_machines() {
        let dir = std::env::temp_dir().join(format!("td-entry-{}", std::process::id()));
        assert_eq!(load_machines(&dir).unwrap(), vec![]);
    }

    #[test]
    fn the_screen_is_gliffs_private_headless_output() {
        assert_eq!(
            screen_command(&far("omarchy-bfs")),
            Some(vec![
                "gliff".into(),
                "--headless".into(),
                "omarchy-bfs".into()
            ])
        );
    }

    #[test]
    fn a_tab_is_named_from_the_first_words_cut_at_a_word() {
        assert_eq!(
            tab_name("  fix the farbox   tunnel so it\nsurvives a reboot ", 24).as_deref(),
            Some("fix the farbox tunnel so")
        );
        assert_eq!(tab_name("hello, world", 6).as_deref(), Some("hello"));
        assert_eq!(
            tab_name("supercalifragilistic", 5).as_deref(),
            Some("super")
        );
        assert_eq!(tab_name("   \n ", 20), None);
    }

    #[test]
    fn a_directory_finds_its_project_only_when_one_name_matches() {
        let projects = vec![
            (1, "TERMINAL DELIGHT".to_string()),
            (2, "OMARCHY".to_string()),
            (3, "omarchy".to_string()),
        ];
        assert_eq!(
            project_by_name(Path::new("/w/terminal-delight"), &projects),
            Some(1)
        );
        assert_eq!(
            project_by_name(Path::new("/w/omarchy"), &projects),
            None,
            "two match"
        );
        assert_eq!(project_by_name(Path::new("/w/jev"), &projects), None);
        assert_eq!(project_by_name(Path::new("/"), &projects), None);
    }

    #[test]
    fn tab_walks_the_ring_and_the_machine_joins_it_only_with_a_far_machine() {
        assert_eq!(Focus::Text.step(false, false), Focus::Project);
        assert_eq!(Focus::Reach.step(false, false), Focus::Text);
        assert_eq!(Focus::Reach.step(true, false), Focus::Machine);
        assert_eq!(Focus::Text.step(true, true), Focus::Machine);
        assert_eq!(
            Focus::Machine.step(false, false),
            Focus::Project,
            "off the ring: start over"
        );
    }

    #[test]
    fn cycling_wraps_and_none_is_a_choice() {
        let c = vec![None, Some("low"), Some("high")];
        assert_eq!(cycle(&c, &None, false), Some(Some("low")));
        assert_eq!(cycle(&c, &Some("high"), false), Some(None));
        assert_eq!(cycle(&c, &None, true), Some(Some("high")));
        assert_eq!(cycle(&c, &Some("gone"), false), Some(None));
        assert_eq!(cycle::<Option<&str>>(&[], &None, false), None);
    }

    #[test]
    fn the_sentence_becomes_the_opener_and_an_empty_one_waits() {
        let r = recipe(
            Harness::Claude,
            Some("opus".into()),
            Some("max".into()),
            Reach::Anywhere,
            PathBuf::from("/w/td"),
            "  build the inbox \n",
        );
        assert_eq!(r.opener.as_deref(), Some("build the inbox"));
        assert!(r.command_line(None).ends_with("'build the inbox'"));
        let quiet = recipe(
            Harness::Codex,
            None,
            None,
            Reach::Anywhere,
            PathBuf::from("/w"),
            " ",
        );
        assert_eq!(quiet.opener, None);
    }
}
