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
//! carries the briefing from its first token. Codex receives the packet in its
//! supported initial PROMPT argument, leaving its own configured instructions
//! intact. A custom local command receives a file or context argument through
//! local-agent.toml. Every launcher path uses the same packet and bundled kit.
//! Agents compute their drop directory from TD_SESSION and TD_PANE_ID. A plain
//! hand-started agent remains outside this launcher contract.
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
    Local,
}

impl Harness {
    pub const ALL: [Harness; 3] = [Harness::Claude, Harness::Codex, Harness::Local];

    pub fn binary(self) -> &'static str {
        match self {
            Harness::Claude => "claude",
            Harness::Codex => "codex",
            Harness::Local => "local",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Harness::Claude => "claude",
            Harness::Codex => "codex",
            Harness::Local => "local",
        }
    }

    /// The chip's word in `l`. [`Self::label`] is also the stored word
    /// ([`Self::from_label`]), so it stays English; only `local` is an
    /// ordinary word — `claude` and `codex` are product names.
    pub fn label_in(self, l: crate::lang::Lang) -> &'static str {
        match self {
            Harness::Local => l.strings().parts.launcher_harness_local,
            _ => self.label(),
        }
    }

    /// The harness a stored word names, or `None` if it names none of them.
    ///
    /// The inverse of [`Self::label`], and deliberately the only one: a second
    /// table mapping words to harnesses is how a config file starts saying
    /// `claude` and meaning codex.
    pub fn from_label(word: &str) -> Option<Harness> {
        Harness::ALL.into_iter().find(|h| h.label() == word)
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
            Harness::Local => &[Model {
                id: "local-config",
                label: "local-agent.toml",
            }],
            // Codex owns its account-specific model catalogue and configuration.
            // A pinned model here can be retired while the CLI remains current.
            Harness::Codex => &[Model {
                id: "configured",
                label: "Codex default",
            }],
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
            Harness::Local => &[],
        }
    }

    /// The level lit when the panel opens: each harness's own middle-high
    /// default, so a person who never touches the row gets what the harness
    /// would have done untouched — except that the flag is now printed, so the
    /// command line says so.
    pub fn default_effort(self) -> Effort {
        match self {
            Harness::Claude => Effort::High,
            Harness::Codex | Harness::Local => Effort::Medium,
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
        if offered.is_empty() {
            return self.default_effort();
        }
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
    /// Every level, lowest first — the union, before any harness narrows it.
    ///
    /// [`Harness::efforts`] is what a harness *offers*; this is what the word
    /// list can contain at all, which is what a stored preference has to be
    /// read against. A file written while Claude was selected can hold `max`,
    /// and Codex has no such level — parsing it against Codex's own list would
    /// lose the choice rather than clamp it.
    pub const ALL: [Effort; 5] = [
        Effort::Low,
        Effort::Medium,
        Effort::High,
        Effort::XHigh,
        Effort::Max,
    ];

    /// The level a stored word names, or `None` if it names none of them.
    ///
    /// The inverse of [`Self::id`]. One table, walked, rather than a second
    /// `match` that can drift from the first — which is how `xhigh` would come
    /// to mean `high` on a round trip through a config file.
    pub fn from_id(word: &str) -> Option<Effort> {
        Effort::ALL.into_iter().find(|e| e.id() == word)
    }

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

    #[cfg(test)]
    pub fn label(self) -> &'static str {
        self.id()
    }

    /// The chip's word in `l`, for display only. [`Self::label`] stays the
    /// flag value, because the journal and the command line read it.
    pub fn label_in(self, l: crate::lang::Lang) -> &'static str {
        let p = l.strings().parts;
        match self {
            Effort::Low => p.launcher_effort_low,
            Effort::Medium => p.launcher_effort_medium,
            Effort::High => p.launcher_effort_high,
            Effort::XHigh => p.launcher_effort_xhigh,
            Effort::Max => p.launcher_effort_max,
        }
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

    #[cfg(test)]
    pub fn label(self) -> &'static str {
        match self {
            Reach::Repo => "this repo",
            Reach::Machine => "this machine",
            Reach::Anywhere => "anywhere",
        }
    }

    /// The dial's words in `l`; [`Self::label`] is the English.
    pub fn label_in(self, l: crate::lang::Lang) -> &'static str {
        let p = l.strings().parts;
        match self {
            Reach::Repo => p.launcher_reach_repo,
            Reach::Machine => p.launcher_reach_machine,
            Reach::Anywhere => p.launcher_reach_anywhere,
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
            (Harness::Local, _) => &[],
        };
        words.iter().map(|w| w.to_string()).collect()
    }
}

/// Where a launch puts the agent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Landing {
    /// Into the pane the panel was opened from, by typing the recipe at its
    /// prompt — the way a person standing in that shell would start it.
    Here,
    /// A new tab, seated in the branch the person is standing in.
    NewTab,
}

/// Which of the two a launch gets.
///
/// **The button already promised this.** `LAUNCH AGENT` is drawn on one
/// surface only — the empty bench of a pane with no agent — under the sentence
/// *"A shell has no agent to present anything. Launch one into this pane."*
/// It then opened a tab somewhere else entirely, which is how a person ends up
/// with the tab they were working in still empty and a new one at the bottom of
/// the window (#508). This is the button keeping its own label.
///
/// `at_a_prompt` is a READING of the pane the panel was opened from, and it is
/// an `Option` because the three answers are three: yes, the foreground process
/// group is the shell itself; no, something is running in there; and nobody has
/// looked — a host-owned pane is born `Unknown` and stays that way until the
/// first classification arrives. Only the first of those is a prompt. An
/// unread pane is not an empty one, and typing a command line into a terminal
/// on the strength of a default would be exactly the invisible write this
/// window exists to refuse.
pub fn landing(at_a_prompt: Option<bool>) -> Landing {
    match at_a_prompt {
        Some(true) => Landing::Here,
        _ => Landing::NewTab,
    }
}

/// The line typed into a pane that is already open, for [`Landing::Here`].
///
/// Three parts, and each one earns its place:
///
/// `^U` first. The gate says the shell is at its prompt; it does not say the
/// prompt is empty, and a half-typed line with a command line appended to it
/// runs as one command — `rm -rf build` plus our recipe is a sentence nobody
/// wrote. Readline discards the line on `^U`, so the worst case is a person
/// seeing their unfinished typing vanish, which beats the worst case of the
/// alternative by a distance.
///
/// Then `cd`, but only when the pane is not already there: a `cd` to where you
/// are is noise in the scrollback, and the scrollback is the record of what
/// this window did to somebody's terminal.
///
/// Then the recipe, and a newline, because a command line nobody pressed enter
/// on is a trap rather than a launch.
pub fn here_line(recipe_line: &str, from: Option<&Path>, to: &Path) -> String {
    let already_there = from.is_some_and(|f| f == to);
    let cd = if already_there {
        String::new()
    } else {
        format!("cd {} && ", shell_quote(&to.to_string_lossy()))
    };
    format!("\u{15}{cd}{recipe_line}\n")
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
                if self.model != "configured" {
                    parts.push("--model".into());
                    parts.push(self.model.to_string());
                }
                parts.extend(self.reach.flags(self.harness));
                parts.push("-c".into());
                parts.push(format!("model_reasoning_effort={}", self.effort.id()));
            }
            Harness::Local => unreachable!("local launches use command_line_with_adapter"),
        }
        if self.harness == Harness::Codex {
            if let Some(path) = briefing {
                parts.push(context_argument(path, self.opener.as_deref()));
                return parts.join(" ");
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
    ///
    /// `kit` is the decision-brief `SKILL.md` written for this launch, when it
    /// could be written; see [`crate::briefkit`].
    pub fn briefing(&self, drop_dir: &str, kit: Option<&Path>) -> String {
        let kit = kit.map(|p| p.display().to_string());
        crate::surface::launch_briefing(drop_dir, kit.as_deref())
    }

    /// Does this harness take a briefing from us at all?
    pub fn takes_briefing(&self) -> bool {
        true
    }

    /// Prepare the actual kit and packet before starting any supported agent.
    pub fn prepare_briefing(&self, dir: &Path, kit_root: &Path) -> Result<PathBuf, String> {
        // These errors are only ever drawn in the launcher, so they are said
        // in the reader's language; the OS error inside stays as the OS gave it.
        let p = crate::lang::current().strings().parts;
        let skill = crate::briefkit::write(kit_root)
            .map_err(|e| crate::lang::fill(p.launcher_err_kit, &[("e", &e.to_string())]))?;
        let text = self.briefing(&format!("{}/$TD_PANE_ID", dir.display()), Some(&skill));
        write_briefing(dir, &text)
            .map_err(|e| crate::lang::fill(p.launcher_err_briefing, &[("e", &e.to_string())]))
    }

    /// The same validated command is used by the launch and its preview.
    pub fn command_line_with_adapter(
        &self,
        briefing: Option<&Path>,
        adapter: Option<&LocalAdapter>,
    ) -> Result<String, String> {
        if self.harness != Harness::Local {
            return Ok(with_desktop_environment(
                &self.command_line(briefing),
                |key| std::env::var(key).ok(),
            ));
        }
        let p = crate::lang::current().strings().parts;
        if self.reach != Reach::Anywhere {
            return Err(p.launcher_err_local_reach.into());
        }
        let path = briefing.ok_or(p.launcher_err_local_briefing)?;
        let line = adapter
            .ok_or(p.launcher_err_local_configure)?
            .command_line(path, self.opener.as_deref())?;
        Ok(with_desktop_environment(&line, |key| {
            std::env::var(key).ok()
        }))
    }
}

/// A persistent host can predate the desktop login. Carry the launching
/// window's display connection to the agent, including into an existing shell.
/// Missing values remain absent; never invent a display or forward unrelated env.
fn with_desktop_environment(line: &str, read: impl Fn(&str) -> Option<String>) -> String {
    let assignments: Vec<_> = [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XDG_RUNTIME_DIR",
        "XAUTHORITY",
    ]
    .into_iter()
    .filter_map(|key| {
        read(key)
            .filter(|value| !value.is_empty())
            .map(|value| format!("{key}={}", shell_quote(&value)))
    })
    .collect();
    if assignments.is_empty() {
        line.to_string()
    } else {
        format!("env {} {line}", assignments.join(" "))
    }
}

// One reserved existing wire value, not a new host protocol variant.
pub const LOCAL_AGENT_MODE: &str = "td:local-agent";

/// A declaration scoped to the configured command, never to its enclosing shell.
/// This controls display and notes eligibility, not authorisation.
pub fn local_agent_environment(bytes: &[u8]) -> bool {
    bytes
        .split(|b| *b == 0)
        .any(|item| item == b"TD_LOCAL_AGENT=1")
}

/// Build one initial PROMPT argument without replacing the agent's own rules.
fn context_argument(path: &Path, opener: Option<&str>) -> String {
    let suffix = match opener.filter(|s| !s.trim().is_empty()) {
        Some(task) => format!("\n\nUSER TASK:\n{task}"),
        None => "\n\nThis is launch context only. Wait for the user to give a task. Acknowledge briefly; do not create a brief for this startup message.".into(),
    };
    format!(
        "\"$(cat {})\"{}",
        shell_quote(&path.to_string_lossy()),
        shell_quote(&suffix)
    )
}

/// A custom runner's argv. These are arguments, not an interpolated shell script.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalAdapter {
    pub command: Vec<String>,
}

impl LocalAdapter {
    pub fn config_path() -> PathBuf {
        crate::instance::config_dir().join("local-agent.toml")
    }

    pub fn load() -> Result<Self, String> {
        let path = Self::config_path();
        Self::load_from(&path).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn load_from(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let adapter: Self = toml::from_str(&text).map_err(|e| e.to_string())?;
        adapter.validate()?;
        Ok(adapter)
    }

    /// Every refusal here lands in the launcher's preview line, so it is
    /// worded in the reader's language. `{briefing}`, `{context}` and
    /// `{prompt}` inside them are the config's own tokens, which
    /// [`crate::lang::fill`] leaves alone because nothing fills those names.
    fn validate(&self) -> Result<(), String> {
        let p = crate::lang::current().strings().parts;
        let Some(binary) = self.command.first() else {
            return Err(p.launcher_err_cmd_executable.into());
        };
        if binary.trim().is_empty()
            || matches!(binary.as_str(), "{briefing}" | "{context}" | "{prompt}")
        {
            return Err(p.launcher_err_cmd_start.into());
        }
        if self.command.iter().any(|arg| arg.contains('\0')) {
            return Err(p.launcher_err_cmd_nul.into());
        }
        if !self
            .command
            .iter()
            .any(|arg| matches!(arg.as_str(), "{briefing}" | "{context}"))
        {
            return Err(p.launcher_err_cmd_briefing_arg.into());
        }
        for arg in &self.command {
            for token in ["{briefing}", "{context}", "{prompt}"] {
                if arg.contains(token) && arg != token {
                    return Err(crate::lang::fill(
                        p.launcher_err_token_whole,
                        &[("token", token)],
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn command_line(&self, path: &Path, opener: Option<&str>) -> Result<String, String> {
        self.validate()?;
        if opener.is_some_and(|s| !s.trim().is_empty())
            && !self
                .command
                .iter()
                .any(|a| a == "{prompt}" || a == "{context}")
        {
            return Err(crate::lang::current()
                .strings()
                .parts
                .launcher_err_cmd_prompt
                .into());
        }
        let args: Vec<String> = self
            .command
            .iter()
            .map(|arg| match arg.as_str() {
                "{briefing}" => shell_quote(&path.to_string_lossy()),
                "{context}" => context_argument(path, opener),
                "{prompt}" => shell_quote(opener.unwrap_or("")),
                _ => shell_quote(arg),
            })
            .collect();
        Ok(format!("TD_LOCAL_AGENT=1 {}", args.join(" ")))
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
                    Harness::Local => unreachable!("local effort dials are empty"),
                };
                assert!(line.contains(&expected), "{harness:?} {effort:?}: {line}");
            }
        }
    }

    /// A stored word survives the trip to disk and back as the same level and
    /// the same harness — and a word neither table knows stays unknown rather
    /// than becoming the first row of whichever list was walked.
    #[test]
    fn a_stored_word_reads_back_as_what_wrote_it_or_as_nothing() {
        for e in Effort::ALL {
            assert_eq!(Effort::from_id(e.id()), Some(e), "{e:?}");
        }
        for h in Harness::ALL {
            assert_eq!(Harness::from_label(h.label()), Some(h), "{h:?}");
        }
        assert_eq!(Effort::from_id("standard"), None);
        assert_eq!(Effort::from_id(""), None);
        assert_eq!(Harness::from_label("gemini"), None);
    }

    #[test]
    fn brief_launch_codex_receives_context_and_task_as_one_literal_argument() {
        let dir = std::env::temp_dir().join(format!("td-codex-brief-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("briefing's file.txt");
        let trap = dir.join("must-not-exist");
        let body = format!(
            "Terminal Delight\nDECISION BRIEFS\n$(touch {})\n`echo code`",
            trap.display()
        );
        std::fs::write(&path, &body).unwrap();
        let mut r = recipe(Harness::Codex, Effort::High);
        r.opener = Some(format!(
            "Compare A & B; $(touch {})\nDon't execute this",
            trap.display()
        ));
        let line = r.command_line(Some(&path));
        let output = std::process::Command::new("sh")
            .args(["-c", &format!("set -- {line}; printf '%s\\0' \"$@\"")])
            .env("HOME", &dir)
            .output()
            .unwrap();
        assert!(output.status.success());
        let argv: Vec<_> = output
            .stdout
            .split(|b| *b == 0)
            .filter(|a| !a.is_empty())
            .collect();
        let last = String::from_utf8(argv.last().unwrap().to_vec()).unwrap();
        assert!(last.starts_with(&body));
        assert!(last.ends_with(r.opener.as_deref().unwrap()));
        assert_eq!(argv.len(), 4, "codex + effort pair + one prompt");
        assert!(
            !line.contains("developer_instructions"),
            "existing rules stay configured"
        );
        assert!(
            !trap.exists(),
            "model content and user text are never executed"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn brief_launch_local_adapter_executes_with_context_and_scoped_marker() {
        let dir = std::env::temp_dir().join(format!("td-local-brief-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("packet's file.txt");
        std::fs::write(&path, "TD packet\nDECISION BRIEFS").unwrap();
        let adapter = LocalAdapter {
            command: vec![
                "sh".into(),
                "-c".into(),
                "printf '%s\\0' \"$TD_LOCAL_AGENT\" \"$@\"".into(),
                "capture".into(),
                "{context}".into(),
                "{prompt}".into(),
            ],
        };
        let line = adapter
            .command_line(&path, Some("user's task\nsecond line"))
            .unwrap();
        let output = std::process::Command::new("sh")
            .args(["-c", &line])
            .env("HOME", &dir)
            .output()
            .unwrap();
        assert!(output.status.success());
        let args: Vec<_> = output
            .stdout
            .split(|b| *b == 0)
            .filter(|a| !a.is_empty())
            .collect();
        assert_eq!(args[0], b"1");
        assert!(String::from_utf8_lossy(args[1]).starts_with("TD packet\nDECISION BRIEFS"));
        assert_eq!(args[2], b"user's task\nsecond line");
        assert!(local_agent_environment(b"X=1\0TD_LOCAL_AGENT=1\0Y=2\0"));
        for env in [
            b"X=TD_LOCAL_AGENT=1".as_slice(),
            b"TD_LOCAL_AGENT=10",
            b"TD_LOCAL_AGENT=0",
            b"",
        ] {
            assert!(!local_agent_environment(env));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn brief_launch_prepares_the_same_complete_kit_for_all_harnesses() {
        let dir = std::env::temp_dir().join(format!("td-shared-kit-{}", std::process::id()));
        let kit = dir.join("kit");
        for h in Harness::ALL {
            let path = recipe(h, Effort::Medium)
                .prepare_briefing(&dir.join(h.label()), &kit)
                .unwrap();
            let text = std::fs::read_to_string(path).unwrap();
            assert!(text.contains("DECISION BRIEFS"));
            assert!(text.contains(&kit.join("SKILL.md").display().to_string()));
            assert!(kit.join("assets/notes.js").is_file());
        }
        let blocked = dir.join("blocked");
        std::fs::write(&blocked, "a file, not a directory").unwrap();
        assert!(recipe(Harness::Codex, Effort::Medium)
            .prepare_briefing(&dir, &blocked)
            .is_err());
        let invalid = dir.join("invalid.toml");
        std::fs::write(&invalid, "command = [42]").unwrap();
        assert!(LocalAdapter::load_from(&invalid).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn brief_launch_local_config_refuses_unbriefed_and_ambiguous_commands() {
        for command in [
            vec![],
            vec!["runner"],
            vec!["{context}"],
            vec!["runner", "--prompt={context}"],
        ] {
            let a = LocalAdapter {
                command: command.into_iter().map(str::to_string).collect(),
            };
            assert!(a.command_line(Path::new("/packet"), None).is_err());
        }
        let a = LocalAdapter {
            command: vec!["runner".into(), "{briefing}".into()],
        };
        assert!(a
            .command_line(Path::new("/packet"), Some("a task"))
            .is_err());
        let r = recipe(Harness::Local, Effort::Medium);
        assert!(r
            .command_line_with_adapter(Some(Path::new("/packet")), None)
            .is_err());
        assert!(r.command_line_with_adapter(None, Some(&a)).is_err());
        assert!(LocalAdapter::load_from(Path::new("/nonexistent/local-agent.toml")).is_err());
        assert!(Harness::Local.efforts().is_empty());
    }

    #[test]
    fn a_codex_line_sets_its_own_reasoning_effort() {
        let line = recipe(Harness::Codex, Effort::High).command_line(None);
        assert!(
            !line.contains("--model"),
            "Codex must use its configured model: {line}"
        );
        assert!(line.contains("-c model_reasoning_effort=high"), "{line}");
        assert!(
            !line.contains("append-system-prompt"),
            "codex has no such flag and must not be handed one: {line}"
        );
    }

    #[test]
    fn desktop_connection_reaches_an_agent_without_shell_expansion() {
        let value = "/tmp/desktop's $(exit 19) `exit 20`";
        let line = with_desktop_environment("sh -c 'printf %s \"$XAUTHORITY\"'", |key| {
            (key == "XAUTHORITY").then(|| value.to_string())
        });
        let output = std::process::Command::new("sh")
            .args(["-c", &line])
            .env_remove("XAUTHORITY")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), value);
        assert_eq!(with_desktop_environment("codex", |_| None), "codex");
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
            if harness == Harness::Local {
                assert!(harness.efforts().is_empty());
                continue;
            }
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
            .map(|e| recipe(Harness::Claude, *e).briefing("/run/td/7", None))
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
    fn only_a_pane_somebody_has_read_and_found_idle_gets_the_agent() {
        assert_eq!(landing(Some(true)), Landing::Here, "a shell at its prompt");
        assert_eq!(
            landing(Some(false)),
            Landing::NewTab,
            "something is running in there — typing would go into its stdin"
        );
        assert_eq!(
            landing(None),
            Landing::NewTab,
            "nobody has read this pane, which is not the same as reading it and \
             finding a prompt; an unknown must never fall through to the write"
        );
    }

    #[test]
    fn the_typed_line_clears_the_prompt_first_and_only_travels_when_it_has_to() {
        let there = PathBuf::from("/home/p/work/td");
        let elsewhere = PathBuf::from("/home/p/work/apes");

        let same = here_line("claude --model opus", Some(&there), &there);
        assert!(same.starts_with('\u{15}'), "the prompt is cleared first");
        assert!(
            !same.contains("cd "),
            "a cd to where the pane already is only adds a line to the record"
        );
        assert!(same.ends_with('\n'), "and it is actually run");

        let moved = here_line("claude --model opus", Some(&elsewhere), &there);
        assert!(
            moved.contains("cd '/home/p/work/td' && claude --model opus"),
            "got {moved:?}"
        );

        // A pane whose directory nobody could read still gets the cd: the
        // launch names a project, and arriving in the wrong one is the failure
        // this is here to prevent.
        let unread = here_line("claude", None, &there);
        assert!(unread.contains("cd '/home/p/work/td' &&"), "got {unread:?}");
    }

    #[test]
    fn a_directory_with_an_apostrophe_reaches_the_shell_whole() {
        let to = std::env::temp_dir().join("td-launch it's here");
        let line = here_line("claude", Some(&PathBuf::from("/")), &to);
        // The cd half of the line, run for real: the property is that the
        // shell lands in that directory, not that the quoting matches my idea
        // of quoting.
        let cd = line
            .trim_start_matches('\u{15}')
            .split(" && ")
            .next()
            .expect("a cd")
            .to_string();
        std::fs::create_dir_all(&to).expect("a directory to land in");
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("{cd} && printf %s \"$PWD\""))
            .output()
            .expect("sh");
        std::fs::remove_dir(&to).ok();
        assert_eq!(String::from_utf8_lossy(&out.stdout), to.to_string_lossy());
    }

    /// The kit the launch wrote reaches the briefing by its path, and a launch
    /// that could not write one briefs without it.
    #[test]
    fn the_briefing_names_the_kit_the_launch_wrote() {
        let r = recipe(Harness::Claude, Effort::High);
        let kit = Path::new("/data/terminal-delight/skills/decision-brief/SKILL.md");
        assert!(r
            .briefing("/run/td/7", Some(kit))
            .contains(&*kit.to_string_lossy()));
        assert!(!r.briefing("/run/td/7", None).contains("SKILL.md"));
    }

    #[test]
    fn the_briefing_carries_the_drop_directory_and_the_verb() {
        let text = recipe(Harness::Claude, Effort::Max).briefing("/run/td/surfaces/s/7", None);
        assert!(text.contains("/run/td/surfaces/s/7"));
        assert!(text.contains("present_surface"), "and the verb");
        assert!(
            text.contains("\"response\""),
            "and the reply shape it is asked for"
        );
    }

    #[test]
    fn every_launcher_harness_is_offered_the_shared_briefing() {
        for harness in Harness::ALL {
            assert!(recipe(harness, Effort::Low).takes_briefing());
        }
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

    #[test]
    fn display_words_leave_the_stored_words_english() {
        use crate::lang::Lang;
        // The chip may be translated; the journal, the flag and from_label
        // keep reading the English word.
        for h in Harness::ALL {
            assert_eq!(h.label_in(Lang::En), h.label());
            assert_eq!(Harness::from_label(h.label()), Some(h));
        }
        for e in Effort::ALL {
            assert_eq!(e.label_in(Lang::En), e.id());
            assert_eq!(e.label(), e.id());
        }
        for r in Reach::ALL {
            assert_eq!(r.label_in(Lang::En), r.label());
        }
        // The config's own tokens survive the template untouched.
        let a = LocalAdapter {
            command: vec!["runner".into(), "{context}".into(), "x{prompt}".into()],
        };
        assert_eq!(
            a.validate().unwrap_err(),
            "{prompt} must occupy a whole command argument"
        );
    }
}
