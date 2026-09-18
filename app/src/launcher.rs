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
//!                  └─ effort    low │ medium │ high │ xhigh │ max
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
//!
//! # Effort is the harness's own dial, in the harness's own words
//!
//! The first version of this panel offered `quick / standard / hard / ultra`
//! and turned them into a sentence appended to the system prompt, on the
//! belief that Claude Code had no effort parameter. It does: `claude --effort
//! <low|medium|high|xhigh|max>` (checked against `claude --help`, 2.1.270, on
//! this machine on 2026-09-17), and Codex takes `model_reasoning_effort` with
//! `low`, `medium`, `high` and `xhigh`. Parker, on the invented scale: *"effort
//! does not correlate with ACTUAL claude efforts."* So the chips now say what
//! the flag will say, the list is the harness's own list, and no prose about
//! thinking is added on top — a dial and a plea for the same thing would be two
//! instructions that can disagree.

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

    /// The effort levels this harness actually accepts, lowest first.
    ///
    /// Claude Code's five and Codex's four, verbatim. A level a harness does
    /// not take is not offered for it, because a chip that silently maps to a
    /// different word is the exact confusion this row used to cause.
    pub fn efforts(self) -> &'static [Effort] {
        match self {
            Harness::Claude => &[
                Effort::Low,
                Effort::Medium,
                Effort::High,
                Effort::XHigh,
                Effort::Max,
            ],
            Harness::Codex => &[Effort::Low, Effort::Medium, Effort::High, Effort::XHigh],
        }
    }

    /// The level lit when the panel opens: each harness's own middle-high
    /// default, so a person who never touches the row gets what the harness
    /// would have done untouched — except that the flag is now printed, so the
    /// command line says so.
    pub fn default_effort(self) -> Effort {
        match self {
            Harness::Claude => Effort::High,
            Harness::Codex => Effort::Medium,
        }
    }

    /// The nearest level this harness offers to one chosen under another.
    ///
    /// Switching harness with `max` lit must not leave `max` lit on a harness
    /// that has no such level. Levels are ordered, so the nearest is the
    /// highest one not above the chosen — `max` on Codex becomes `xhigh`, and
    /// anything Codex offers is offered by Claude unchanged.
    pub fn clamp_effort(self, chosen: Effort) -> Effort {
        let offered = self.efforts();
        offered
            .iter()
            .rev()
            .find(|e| **e <= chosen)
            .copied()
            .unwrap_or(offered[0])
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

/// How hard to think — the union of the levels the harnesses take, ordered.
///
/// The words are the harnesses' own, and the chip says the word the flag will
/// carry. Ordered so [`Harness::clamp_effort`] can find the nearest level when
/// the harness changes under a chosen one.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Effort {
    Low,
    Medium,
    High,
    XHigh,
    Max,
}

impl Effort {
    /// The word on the chip, in the launch journal, and after the flag. One
    /// word for all three on purpose: a label that differs from the value is
    /// how `standard` came to mean nothing.
    pub fn id(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::XHigh => "xhigh",
            Effort::Max => "max",
        }
    }

    pub fn label(self) -> &'static str {
        self.id()
    }
}

/// The LAUNCH AGENT panel's height for a given number of matched projects,
/// capped to the window.
///
/// The panel used to be sized as `250 + 30 per row`, and the 250 was the
/// chrome as it stood when the panel had one chip row. Four chip rows, a
/// command preview and a key hint later it needed nearly four hundred, so a
/// filter matching three projects produced a 340-pixel panel whose fixed
/// children could not shrink — and the project list, the only child that
/// could, was squeezed to nothing. Parker, typing `ter` into it: *"I don't see
/// terminal delight"*. The rows were there; they had no height.
///
/// So the chrome is a named number that every fixed child is counted into,
/// and the test below holds that the rows always get their own room on top of
/// it. A filter that matches nothing still gets one row's worth, for the
/// "nothing matches" line.
pub fn panel_height(matched: usize, window_h: f32) -> f32 {
    let rows = matched.clamp(1, MAX_ROWS) as f32;
    (PANEL_CHROME_H + rows * ROW_H).min(window_h - PANEL_MARGIN * 2.)
}

/// Rows drawn at most; the filter is how a person reaches the rest.
pub const MAX_ROWS: usize = 40;
/// One project row.
pub const ROW_H: f32 = 30.;
/// Everything in the panel that is not a project row: the title line, the
/// filter box, five section headers, four chip rows, the command preview and
/// the key hint, with the panel's padding and gaps. Summed from the render,
/// not measured off a screenshot — each part is a number the render names.
pub const PANEL_CHROME_H: f32 = 12. * 2. // padding
    + 8. * 12. // gaps between thirteen children
    + 18. // title line
    + 26. // filter box
    + 5. * 13. // PROJECT · HARNESS · MODEL · EFFORT · REACH headers
    + 4. * 24. // the chip rows
    + 40. // the command line, which wraps to two
    + 12.; // the key hint
/// Clear space kept between the panel and the window's edge.
pub const PANEL_MARGIN: f32 = 16.;

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
                parts.push("--effort".into());
                parts.push(self.effort.id().into());
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
                parts.push(format!("model_reasoning_effort={}", self.effort.id()));
            }
        }
        if let Some(opener) = self.opener.as_ref().filter(|o| !o.trim().is_empty()) {
            parts.push(shell_quote(opener));
        }
        parts.join(" ")
    }

    /// The system prompt this window appends, if the harness takes one.
    ///
    /// The surface briefing and nothing about effort: effort is a flag on the
    /// same command line now, and a sentence asking for the same thing in
    /// prose would be a second instruction that can drift from the first.
    pub fn briefing(&self, drop_dir: &str) -> String {
        crate::surface::launch_briefing(drop_dir)
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
        let plain = recipe(Harness::Claude, Effort::High).command_line(None);
        let mut r = recipe(Harness::Claude, Effort::High);
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
        let mut c = recipe(Harness::Codex, Effort::High);
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
    fn a_claude_line_names_the_model_the_effort_and_reads_its_briefing_from_a_file() {
        let line = recipe(Harness::Claude, Effort::High)
            .command_line(Some(Path::new("/run/td/briefing.txt")));
        assert!(
            line.starts_with("claude --model opus --effort high"),
            "{line}"
        );
        assert!(line.contains("--append-system-prompt"), "{line}");
        assert!(
            line.contains("\"$(cat '/run/td/briefing.txt')\""),
            "the briefing is read, not inlined: {line}"
        );
    }

    #[test]
    fn the_chip_word_is_the_flag_word_for_every_level_on_every_harness() {
        // The whole complaint was a chip saying `standard` and the harness
        // hearing nothing. So the word on the chip is asserted to be the word
        // after the flag, level by level, on each harness's own list.
        for harness in Harness::ALL {
            for effort in harness.efforts() {
                let line = recipe(harness, *effort).command_line(None);
                let expected = match harness {
                    Harness::Claude => format!("--effort {}", effort.label()),
                    Harness::Codex => format!("model_reasoning_effort={}", effort.label()),
                };
                assert!(line.contains(&expected), "{harness:?} {effort:?}: {line}");
            }
        }
    }

    #[test]
    fn a_codex_line_sets_its_own_reasoning_effort() {
        let line = recipe(Harness::Codex, Effort::High).command_line(None);
        assert!(line.contains("-c model_reasoning_effort=high"), "{line}");
        assert!(
            !line.contains("append-system-prompt"),
            "codex has no such flag and must not be handed one: {line}"
        );
    }

    #[test]
    fn a_level_a_harness_lacks_is_clamped_to_its_nearest_when_the_harness_changes() {
        assert_eq!(Harness::Codex.clamp_effort(Effort::Max), Effort::XHigh);
        assert_eq!(Harness::Codex.clamp_effort(Effort::Low), Effort::Low);
        for effort in Harness::Codex.efforts() {
            assert_eq!(
                Harness::Claude.clamp_effort(*effort),
                *effort,
                "everything codex offers, claude offers unchanged"
            );
        }
        assert!(
            !Harness::Codex.efforts().contains(&Effort::Max),
            "codex has no max and must not be offered one"
        );
        for harness in Harness::ALL {
            assert!(harness.efforts().contains(&harness.default_effort()));
            assert!(
                harness.efforts().windows(2).all(|w| w[0] < w[1]),
                "ordered, lowest first"
            );
        }
    }

    #[test]
    fn effort_is_a_flag_and_never_also_a_sentence() {
        // The dial and a plea for the same thing would be two instructions
        // that can disagree. Every level's briefing is the same text.
        let texts: Vec<String> = Harness::Claude
            .efforts()
            .iter()
            .map(|e| recipe(Harness::Claude, *e).briefing("/run/td/7"))
            .collect();
        assert!(
            texts.windows(2).all(|w| w[0] == w[1]),
            "the briefing varies by effort"
        );
        assert!(
            !texts[0].contains("hink"),
            "no think/Think/ultrathink: {}",
            texts[0]
        );
    }

    #[test]
    fn the_panel_leaves_the_rows_their_own_room_above_the_chrome() {
        // The regression: three matches in a tall window got a panel whose
        // fixed children alone exceeded its height, and the list was squeezed
        // to nothing. Every match count up to the cap must get its rows on top
        // of the chrome, and the old formula's constant must not be enough for
        // the chrome this panel actually has — or this test guards nothing.
        for matched in 0..=MAX_ROWS {
            let h = panel_height(matched, 4000.);
            let rows = matched.clamp(1, MAX_ROWS) as f32;
            assert!(
                h - PANEL_CHROME_H >= rows * ROW_H - 0.01,
                "{matched} matches: {h} leaves {} for {rows} rows",
                h - PANEL_CHROME_H
            );
        }
        assert!(PANEL_CHROME_H > 250., "the number that shipped the bug");
        assert_eq!(
            panel_height(MAX_ROWS + 50, 4000.),
            panel_height(MAX_ROWS, 4000.),
            "past the cap the panel stops growing"
        );
        assert_eq!(
            panel_height(30, 600.),
            600. - PANEL_MARGIN * 2.,
            "capped to the window"
        );
    }

    #[test]
    fn an_opener_is_quoted_and_an_empty_one_is_not_passed_at_all() {
        let mut r = recipe(Harness::Claude, Effort::Low);
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
    fn the_briefing_carries_the_drop_directory_and_the_verb() {
        let text = recipe(Harness::Claude, Effort::Max).briefing("/run/td/surfaces/s/7");
        assert!(text.contains("/run/td/surfaces/s/7"));
        assert!(text.contains("present_surface"), "and the verb");
        assert!(
            text.contains("\"response\""),
            "and the reply shape it is asked for"
        );
    }

    #[test]
    fn only_claude_is_offered_a_briefing_by_us() {
        assert!(recipe(Harness::Claude, Effort::Low).takes_briefing());
        assert!(!recipe(Harness::Codex, Effort::Low).takes_briefing());
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
