//! LAUNCH AN AGENT — a button where a command line used to be.
//!
//! Opening an agent in a terminal costs a person four things they have to know
//! by heart: where the project is, what the binary is called, which flag sets
//! the model, and what to type first. That is fine for someone who types
//! `cd ../W<tab>` faster than they can read a menu, and it is a wall for
//! everyone else — which is most people, including most of the people who
//! would get the most out of an agent.
//!
//! So: a pane, a button, three choices and a project.
//!
//! ```text
//!   ＋ pane  →  LAUNCH AGENT
//!                  ├─ project   terminal-delight        (existing, or new)
//!                  ├─ harness   claude │ codex
//!                  ├─ model     opus │ sonnet │ haiku
//!                  └─ effort    quick │ standard │ hard │ ultra
//!                                        ↓
//!                       the host spawns a terminal in that directory
//!                       and types one line into it
//! ```
//!
//! # This module is a string builder, and that is the point
//!
//! There is no process management here, because there does not need to be any.
//! The session host already spawns terminals and already types a *recipe* into
//! a fresh one — that is how a restored `claude --resume <id>` pane comes back
//! after a restart. A launcher is therefore a recipe builder plus a directory,
//! and the whole feature reduces to two things this window can already do.
//!
//! Everything below is pure: a command line is a value, and a value can be
//! asserted about. Shell quoting in particular is not something to find out
//! about from a directory with an apostrophe in it.
//!
//! # How the agent learns about the workbench
//!
//! Two ways, and the split matters because only one of them is ours to give.
//!
//! Claude Code takes `--append-system-prompt`, so a pane this window launched
//! carries the briefing from its first token. Codex has no equivalent flag; it
//! reads `AGENTS.md` from the working directory, which is where the
//! machine-global rule lives instead. Both then compute their own drop
//! directory from `TD_SESSION` and `TD_PANE_ID`, which the host already puts
//! in every pane's environment — so neither harness needs a path handed to it,
//! and an agent started from a plain shell is no worse off than one this
//! window launched.

use std::path::{Path, PathBuf};

/// Which agent program to start.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Harness {
    Claude,
    Codex,
}

impl Harness {
    pub const ALL: [Harness; 2] = [Harness::Claude, Harness::Codex];

    pub fn binary(self) -> &'static str {
        match self {
            Harness::Claude => "claude",
            Harness::Codex => "codex",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Harness::Claude => "claude",
            Harness::Codex => "codex",
        }
    }

    /// The models this window offers for the harness.
    ///
    /// A short list on purpose. The full set changes under us and a stale menu
    /// that silently starts the wrong model is worse than a short one — the
    /// escape hatch is the terminal, which is right there.
    pub fn models(self) -> &'static [Model] {
        match self {
            Harness::Claude => &[
                Model {
                    id: "opus",
                    label: "opus",
                },
                Model {
                    id: "sonnet",
                    label: "sonnet",
                },
                Model {
                    id: "haiku",
                    label: "haiku",
                },
            ],
            Harness::Codex => &[
                Model {
                    id: "gpt-5-codex",
                    label: "gpt-5-codex",
                },
                Model {
                    id: "gpt-5",
                    label: "gpt-5",
                },
            ],
        }
    }
}

/// One entry in a harness's model menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Model {
    /// What the flag is given.
    pub id: &'static str,
    /// What the chip says.
    pub label: &'static str,
}

/// How hard to think.
///
/// Four steps rather than a number, for the same reason [`crate::surface`]
/// weighs effort in buckets: a person choosing from a menu is picking a shape,
/// and a scale of ten invites a precision nobody has.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Effort {
    Quick,
    Standard,
    Hard,
    Ultra,
}

impl Effort {
    pub const ALL: [Effort; 4] = [Effort::Quick, Effort::Standard, Effort::Hard, Effort::Ultra];

    pub fn label(self) -> &'static str {
        match self {
            Effort::Quick => "quick",
            Effort::Standard => "standard",
            Effort::Hard => "hard",
            Effort::Ultra => "ultra",
        }
    }

    /// What Codex's own reasoning-effort setting is set to.
    fn codex_value(self) -> &'static str {
        match self {
            Effort::Quick => "low",
            Effort::Standard => "medium",
            Effort::Hard => "high",
            Effort::Ultra => "high",
        }
    }

    /// The sentence appended to Claude's system prompt.
    ///
    /// A sentence, not a flag, and the difference is worth being honest about:
    /// Claude Code has no effort parameter. What it has is a convention its
    /// own documentation describes — `think`, `think hard`, `ultrathink` —
    /// which is a request in prose. So this is a request in prose, and
    /// `quick` says nothing at all rather than pretending there is a dial for
    /// "less".
    fn claude_phrase(self) -> Option<&'static str> {
        match self {
            Effort::Quick => None,
            Effort::Standard => Some("Think before acting on anything non-trivial."),
            Effort::Hard => Some("Think hard before acting; this work is expected to be involved."),
            Effort::Ultra => {
                Some("Ultrathink. This work is foundational and expensive to get wrong.")
            }
        }
    }
}

/// A project a pane can be launched into.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Project {
    /// Absolute path.
    pub path: PathBuf,
    /// What the row says — the directory's own name.
    pub name: String,
    /// A git repository, as opposed to a directory that merely exists. Shown,
    /// not filtered on: plenty of real work happens outside a repo.
    pub repo: bool,
}

/// Where to look for projects, in order.
///
/// Roots rather than a recursive sweep of `$HOME`: the logo picker learned
/// that the deep sweep is the slow one, and a project list that takes 600ms to
/// appear is a menu people stop opening.
pub fn default_roots(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join("Work"),
        home.join("BROWN-FAMILY-SPORTS/Software"),
        home.join("BROWN-FAMILY-SPORTS"),
        home.join("INTELLIMASS"),
        home.to_path_buf(),
    ]
}

/// One level down from each root, newest first.
///
/// Depth one, deliberately. A project is a directory somebody works in, and
/// the ones at depth three are checkouts, build outputs and `node_modules` —
/// which is how a project picker turns into a file browser.
pub fn scan(roots: &[PathBuf], limit: usize) -> Vec<Project> {
    let mut out: Vec<(std::time::SystemTime, Project)> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.starts_with('.') {
                continue; // dotfile directories are not projects
            }
            if seen.contains(&path) {
                continue;
            }
            seen.push(path.clone());
            let when = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            out.push((
                when,
                Project {
                    repo: path.join(".git").exists(),
                    name: name.to_string(),
                    path,
                },
            ));
        }
    }
    out.sort_by_key(|(when, _)| std::cmp::Reverse(*when));
    out.into_iter().map(|(_, p)| p).take(limit).collect()
}

/// Rank projects against what has been typed.
///
/// Prefix beats contains, and a repository beats a bare directory at the same
/// score, because the thing being looked for is almost always a checkout.
pub fn filter(projects: &[Project], query: &str) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    let mut scored: Vec<(u8, usize)> = projects
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let name = p.name.to_lowercase();
            let rank = if q.is_empty() {
                2
            } else if name == q {
                0
            } else if name.starts_with(&q) {
                1
            } else if name.contains(&q) {
                2
            } else if p.path.to_string_lossy().to_lowercase().contains(&q) {
                3
            } else {
                return None;
            };
            Some((rank * 2 + u8::from(!p.repo), i))
        })
        .collect();
    scored.sort_by_key(|(rank, i)| (*rank, *i));
    scored.into_iter().map(|(_, i)| i).collect()
}

/// How far a launched agent may reach without asking, in the harness's own
/// terms.
///
/// `Anywhere` is the machine posture and adds nothing to the command line —
/// the launcher prints the same line it printed before this row existed. The
/// other two exist for the one launch in twenty whose first job is to read a
/// stranger's pull request: the agents that were owned in the OpenClaw
/// incidents were the ones with the widest reach reading the least trusted
/// input. It is a dial, not a policy; it changes nothing until touched.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Reach {
    /// Edits inside the project are taken; anything else asks.
    Repo,
    /// The harness's own middle setting — its classifier or its full sandbox.
    Machine,
    /// Whatever the machine's own configuration says. The default.
    #[default]
    Anywhere,
}

impl Reach {
    pub const ALL: [Reach; 3] = [Reach::Repo, Reach::Machine, Reach::Anywhere];

    pub fn label(self) -> &'static str {
        match self {
            Reach::Repo => "this repo",
            Reach::Machine => "this machine",
            Reach::Anywhere => "anywhere",
        }
    }

    /// The word in the launch journal.
    pub fn id(self) -> &'static str {
        match self {
            Reach::Repo => "repo",
            Reach::Machine => "machine",
            Reach::Anywhere => "anywhere",
        }
    }

    /// The flags this reach adds, per harness. Checked against `claude --help`
    /// (2.1.270) and `codex --help` (0.151.0) on this machine, 2026-09-17,
    /// rather than remembered: `--permission-mode` offers `acceptEdits`,
    /// `auto`, `bypassPermissions`, `manual`, `dontAsk`, `plan`; `--sandbox`
    /// offers `read-only`, `workspace-write`, `danger-full-access`.
    pub fn flags(self, harness: Harness) -> Vec<String> {
        let words: &[&str] = match (harness, self) {
            (_, Reach::Anywhere) => &[],
            (Harness::Claude, Reach::Repo) => &["--permission-mode", "acceptEdits"],
            (Harness::Claude, Reach::Machine) => &["--permission-mode", "auto"],
            (Harness::Codex, Reach::Repo) => &["--sandbox", "workspace-write"],
            (Harness::Codex, Reach::Machine) => &["--sandbox", "danger-full-access"],
        };
        words.iter().map(|w| w.to_string()).collect()
    }
}

/// Everything a launch needs.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Recipe {
    pub harness: Harness,
    pub model: &'static str,
    pub effort: Effort,
    /// How far the agent may reach without asking. See [`Reach`].
    pub reach: Reach,
    /// The directory the terminal starts in.
    pub cwd: PathBuf,
    /// What to say to the agent first, if anything. Optional on purpose: an
    /// agent launched with no instruction is a person who wants to type one.
    pub opener: Option<String>,
}

impl Recipe {
    /// The line the host types into the fresh terminal.
    ///
    /// `briefing` is a file this window wrote; when it is present and the
    /// harness can take one, it becomes an appended system prompt. Read with
    /// `$(cat …)` rather than inlined, because a briefing is several
    /// paragraphs and a several-paragraph shell argument is a quoting bug
    /// waiting for a directory with an apostrophe in it.
    pub fn command_line(&self, briefing: Option<&Path>) -> String {
        let mut parts: Vec<String> = vec![self.harness.binary().to_string()];
        match self.harness {
            Harness::Claude => {
                parts.push("--model".into());
                parts.push(self.model.to_string());
                parts.extend(self.reach.flags(self.harness));
                if let Some(path) = briefing {
                    parts.push("--append-system-prompt".into());
                    parts.push(format!(
                        "\"$(cat {})\"",
                        shell_quote(&path.to_string_lossy())
                    ));
                }
            }
            Harness::Codex => {
                parts.push("--model".into());
                parts.push(self.model.to_string());
                parts.extend(self.reach.flags(self.harness));
                parts.push("-c".into());
                parts.push(format!(
                    "model_reasoning_effort={}",
                    self.effort.codex_value()
                ));
            }
        }
        if let Some(opener) = self.opener.as_ref().filter(|o| !o.trim().is_empty()) {
            parts.push(shell_quote(opener));
        }
        parts.join(" ")
    }

    /// The system prompt this window appends, if the harness takes one.
    ///
    /// The surface briefing plus the effort sentence — one text, so an agent
    /// reads about the workbench and about how hard to think in the same
    /// breath rather than as two unrelated instructions.
    pub fn briefing(&self, drop_dir: &str) -> String {
        let mut text = crate::surface::launch_briefing(drop_dir);
        if let Some(phrase) = self.effort.claude_phrase() {
            text.push_str("\n\n");
            text.push_str(phrase);
        }
        text
    }

    /// Does this harness take a briefing from us at all?
    pub fn takes_briefing(&self) -> bool {
        matches!(self.harness, Harness::Claude)
    }
}

/// Single-quote a value for a POSIX shell, the only way that is always right.
fn shell_quote(raw: &str) -> String {
    format!("'{}'", raw.replace('\'', r"'\''"))
}

/// Write the briefing where the recipe can read it back.
///
/// Beside the pane's surfaces rather than in `/tmp`: it belongs to this pane,
/// it is useful to read when something is not working, and it is swept with
/// everything else when the session goes.
pub fn write_briefing(dir: &Path, text: &str) -> std::io::Result<PathBuf> {
    use std::io::Write as _;
    use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
    std::fs::create_dir_all(dir)?;
    let path = dir.join("briefing.txt");
    // A system prompt is an instruction to an agent: keep it to this user
    // from the first byte, and re-assert it on a file that already existed
    // with the old mode.
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)?;
    f.write_all(text.as_bytes())?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    Ok(path)
}

/// FNV-1a 64 over the briefing bytes, as sixteen hex digits.
///
/// An integrity mark for the launch journal — a briefing that changed between
/// the launcher writing it and the recipe's `$(cat …)` reading it is
/// detectable afterwards rather than silent — and not an authentication: a
/// process that can rewrite the file can recompute this, and the journal
/// line names the algorithm so nobody reads more into it than that.
pub fn fingerprint(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe(harness: Harness, effort: Effort) -> Recipe {
        Recipe {
            harness,
            model: harness.models()[0].id,
            effort,
            reach: Reach::Anywhere,
            cwd: PathBuf::from("/home/parker/Work/terminal-delight"),
            opener: None,
        }
    }

    #[test]
    fn anywhere_adds_nothing_and_the_other_reaches_add_the_harness_own_flags() {
        let plain = recipe(Harness::Claude, Effort::Standard).command_line(None);
        let mut r = recipe(Harness::Claude, Effort::Standard);
        r.reach = Reach::Anywhere;
        assert_eq!(
            r.command_line(None),
            plain,
            "anywhere is the machine posture: the line the panel always printed"
        );
        r.reach = Reach::Repo;
        assert!(
            r.command_line(None)
                .contains("--permission-mode acceptEdits"),
            "{}",
            r.command_line(None)
        );
        r.reach = Reach::Machine;
        assert!(r.command_line(None).contains("--permission-mode auto"));
        let mut c = recipe(Harness::Codex, Effort::Hard);
        c.reach = Reach::Repo;
        assert!(c.command_line(None).contains("--sandbox workspace-write"));
        c.reach = Reach::Machine;
        assert!(c
            .command_line(None)
            .contains("--sandbox danger-full-access"));
        c.reach = Reach::Anywhere;
        assert!(!c.command_line(None).contains("--sandbox"));
        assert_eq!(Reach::default(), Reach::Anywhere);
    }

    #[test]
    fn a_briefing_fingerprint_is_stable_and_tells_two_texts_apart() {
        assert_eq!(fingerprint("abc"), fingerprint("abc"));
        assert_ne!(fingerprint("abc"), fingerprint("abd"));
        assert_eq!(fingerprint("").len(), 16, "sixteen hex digits, always");
    }

    #[test]
    fn a_claude_line_names_the_model_and_reads_its_briefing_from_a_file() {
        let line = recipe(Harness::Claude, Effort::Standard)
            .command_line(Some(Path::new("/run/td/briefing.txt")));
        assert!(line.starts_with("claude --model opus"), "{line}");
        assert!(line.contains("--append-system-prompt"), "{line}");
        assert!(
            line.contains("\"$(cat '/run/td/briefing.txt')\""),
            "the briefing is read, not inlined: {line}"
        );
    }

    #[test]
    fn a_codex_line_sets_its_own_reasoning_effort() {
        let line = recipe(Harness::Codex, Effort::Hard).command_line(None);
        assert!(line.contains("-c model_reasoning_effort=high"), "{line}");
        assert!(
            !line.contains("append-system-prompt"),
            "codex has no such flag and must not be handed one: {line}"
        );
    }

    #[test]
    fn ultra_and_hard_are_the_same_to_codex_and_different_to_claude() {
        // Honest about the platform: codex's scale stops at high, so ultra and
        // hard collapse there — and they must NOT collapse for Claude, where
        // the difference is a real convention in its own documentation.
        assert_eq!(Effort::Hard.codex_value(), Effort::Ultra.codex_value());
        assert_ne!(
            Effort::Hard.claude_phrase(),
            Effort::Ultra.claude_phrase(),
            "ultrathink is a different request from think hard"
        );
    }

    #[test]
    fn quick_asks_for_nothing_rather_than_asking_for_less() {
        assert_eq!(Effort::Quick.claude_phrase(), None);
        let text = recipe(Harness::Claude, Effort::Quick).briefing("/run/td/7");
        assert!(!text.contains("Think"), "{text}");
    }

    #[test]
    fn an_opener_is_quoted_and_an_empty_one_is_not_passed_at_all() {
        let mut r = recipe(Harness::Claude, Effort::Quick);
        r.opener = Some("fix the login bug".into());
        assert!(r.command_line(None).ends_with("'fix the login bug'"));
        r.opener = Some("   ".into());
        assert!(
            !r.command_line(None).contains("''"),
            "whitespace is not an instruction"
        );
    }

    #[test]
    fn a_path_with_an_apostrophe_survives_a_real_shell() {
        // Asserted against `sh` itself rather than against a string I wrote by
        // hand. The property that matters is that the shell gives the value
        // back unchanged and as ONE argument — a hand-written expectation only
        // proves the quoting matches my idea of the quoting, which is the
        // thing in doubt.
        for raw in [
            "/home/parker/it's here/x.txt",
            "plain",
            "two words",
            "a \"double\" quote",
            "$HOME and `backticks`",
            "back\\slash",
        ] {
            let out = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("printf %s {}", shell_quote(raw)))
                .output()
                .expect("sh");
            assert_eq!(
                String::from_utf8_lossy(&out.stdout),
                raw,
                "the shell did not give back {raw:?}"
            );
        }
    }

    #[test]
    fn the_briefing_carries_the_drop_directory_and_the_effort_together() {
        let text = recipe(Harness::Claude, Effort::Ultra).briefing("/run/td/surfaces/s/7");
        assert!(text.contains("/run/td/surfaces/s/7"));
        assert!(text.contains("Ultrathink"));
        assert!(text.contains("present_surface"), "and the verb");
    }

    #[test]
    fn only_claude_is_offered_a_briefing_by_us() {
        assert!(recipe(Harness::Claude, Effort::Quick).takes_briefing());
        assert!(!recipe(Harness::Codex, Effort::Quick).takes_briefing());
    }

    #[test]
    fn every_harness_offers_at_least_one_model_and_they_are_distinct() {
        for harness in Harness::ALL {
            let models = harness.models();
            assert!(!models.is_empty(), "{harness:?} offers nothing");
            for m in models {
                assert!(!m.id.is_empty() && !m.label.is_empty());
            }
        }
        assert_ne!(
            Harness::Claude.models()[0].id,
            Harness::Codex.models()[0].id
        );
    }

    #[test]
    fn filtering_prefers_a_prefix_then_a_repository() {
        let projects = vec![
            Project {
                path: "/w/zzz-terminal".into(),
                name: "zzz-terminal".into(),
                repo: false,
            },
            Project {
                path: "/w/terminal-delight".into(),
                name: "terminal-delight".into(),
                repo: true,
            },
            Project {
                path: "/w/term-notes".into(),
                name: "term-notes".into(),
                repo: false,
            },
            Project {
                path: "/w/unrelated".into(),
                name: "unrelated".into(),
                repo: true,
            },
        ];
        let order = filter(&projects, "term");
        assert_eq!(
            projects[order[0]].name, "terminal-delight",
            "prefix + repo wins"
        );
        assert_eq!(order.len(), 3, "the unrelated one is filtered out entirely");
        assert!(
            filter(&projects, "").len() == 4,
            "an empty query shows everything"
        );
    }

    #[test]
    fn a_path_match_still_finds_a_project_whose_name_does_not_match() {
        let projects = vec![Project {
            path: "/home/parker/INTELLIMASS/scout".into(),
            name: "scout".into(),
            repo: true,
        }];
        assert_eq!(filter(&projects, "intellimass").len(), 1);
        assert_eq!(filter(&projects, "nothing-like-it").len(), 0);
    }

    #[test]
    fn scanning_skips_dotfiles_and_deduplicates_overlapping_roots() {
        let base = std::env::temp_dir().join(format!("td-launcher-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("alpha/.git")).unwrap();
        std::fs::create_dir_all(base.join("beta")).unwrap();
        std::fs::create_dir_all(base.join(".hidden")).unwrap();
        std::fs::write(base.join("a-file"), b"x").unwrap();

        // The same root twice, which is exactly what `default_roots` does when
        // a project directory IS the home directory.
        let found = scan(&[base.clone(), base.clone()], 50);
        let names: Vec<&str> = found.iter().map(|p| p.name.as_str()).collect();
        assert!(
            names.contains(&"alpha") && names.contains(&"beta"),
            "{names:?}"
        );
        assert!(!names.contains(&".hidden"), "dotfiles are not projects");
        assert!(!names.contains(&"a-file"), "files are not projects");
        assert_eq!(names.len(), 2, "one entry per directory: {names:?}");
        assert!(found.iter().find(|p| p.name == "alpha").unwrap().repo);
        assert!(!found.iter().find(|p| p.name == "beta").unwrap().repo);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_limit_is_honoured_so_a_home_directory_cannot_flood_the_menu() {
        let base = std::env::temp_dir().join(format!("td-launcher-many-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for i in 0..30 {
            std::fs::create_dir_all(base.join(format!("p{i:02}"))).unwrap();
        }
        assert_eq!(scan(std::slice::from_ref(&base), 10).len(), 10);
        let _ = std::fs::remove_dir_all(&base);
    }
}
