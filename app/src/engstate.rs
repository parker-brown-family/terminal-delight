//! The engineering state of a project, as the top rail reads it.
//!
//! The rail used to name the branch the active tab was standing in, in a box
//! pinned to exactly the tree's width, directly above the spine row that was
//! already lit for that same branch — one fact, said twice in the two most
//! prominent places in the window. Parker: *"the top outer repeats something
//! we already see on the left spine"*. What the top row should answer instead
//! is not *what branch am I on* but **what kind of engineering situation
//! exists inside this project right now** — and that is a question about the
//! PROJECT, so switching tabs does not change it and switching projects does.
//!
//! ```text
//!   TERMINAL DELIGHT   [4 WT · 1 SHARED]   │  3 dirty · +641 −188 · 19 files
//! ```
//!
//! Two truths are compared and neither is allowed to overrule the other:
//!
//! ```text
//!   DECLARED    the left bar's tree — this tab is filed under BFS / WEB DEV
//!   OBSERVED    the pane's cwd — its git root is terminal-delight
//! ```
//!
//! A pane whose observed repository is not its declared project's is FOREIGN
//! here and a VISITOR from the other project's point of view; both are
//! reported, nothing is moved. That is filesystem-aware organisational
//! linting, and it is the part of the rail that catches a real mistake.
//!
//! **Unknown is not zero, and this module is where that rule bites hardest.**
//! A number in the busiest row of the window reads as a measurement. So every
//! measured field is an `Option`, a git call that fails leaves `None`, the
//! badge says `SCANNING` until the first pass has landed, and no default ever
//! turns *not measured* into `0`. A project with no repository says `NO GIT`
//! rather than `0 dirty`.
//!
//! **Silence means healthy.** A frame is emitted only when it carries a fact
//! worth interrupting for; a project that is clean, isolated and idle shows its
//! badge and nothing else. The rail earns trust by not screaming continuously.
//!
//! Layering: this file is std only — no gpui — so the whole model is testable
//! against a temporary repository. The scan runs `git` in subprocesses and is
//! meant to be called from the background executor; `main.rs` gathers the
//! writers on the main thread, hands them here, and applies the result back.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// One pane that can write, as the scan sees it. `tab` is the strip index at
/// gather time — display only, never an identity (tabs slide).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Writer {
    pub tab: usize,
    /// The tab's name, or the shell's title when it has none.
    pub tab_name: String,
    /// What runs there — `CLAUDE`, `SHELL`, the live program's name.
    pub label: String,
    pub cwd: PathBuf,
}

/// Everything a scan needs, gathered on the main thread.
#[derive(Clone, Debug, Default)]
pub struct ScanInput {
    /// The declared project being drawn, and what the tree calls it.
    pub project: Option<u32>,
    pub name: Option<String>,
    /// Writers filed under that project.
    pub mine: Vec<Writer>,
    /// Writers filed under any OTHER branch, with what that branch is called —
    /// the only way a visitor can be recognised.
    pub others: Vec<(Option<String>, Writer)>,
}

/// What git said about one checkout — a worktree or a clone, it makes no
/// difference to the safety question, which is *how many things are writing
/// into this directory*.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkout {
    /// `git rev-parse --show-toplevel`.
    pub root: PathBuf,
    /// The repository this is a checkout OF — see [`repo_identity`].
    pub repo: String,
    /// The branch, or `None` when HEAD is detached.
    pub branch: Option<String>,
    pub head: Option<String>,
    /// Tracked files with uncommitted changes. `None` = not measured.
    pub dirty: Option<u32>,
    pub untracked: Option<u32>,
    /// Lines added and removed against HEAD, tracked files only.
    pub delta: Option<(u32, u32)>,
    /// `(ahead, behind)` against the repository's main line. `None` when the
    /// repository has no main line to measure against, or the call failed.
    pub ahead_behind: Option<(u32, u32)>,
    /// Unix seconds of the last commit on this checkout's HEAD.
    pub last_commit: Option<u64>,
    /// The panes operating here. Two or more is a SHARED checkout.
    pub writers: Vec<Writer>,
}

impl Checkout {
    pub fn shared(&self) -> bool {
        self.writers.len() > 1
    }
    /// Does this checkout carry uncommitted work? `None` when unmeasured.
    pub fn is_dirty(&self) -> Option<bool> {
        self.dirty.map(|d| d > 0)
    }
    /// What the rail calls this checkout: its branch, or its short sha.
    pub fn line(&self) -> String {
        match (&self.branch, &self.head) {
            (Some(b), _) => b.clone(),
            (None, Some(h)) => format!("detached @{h}"),
            (None, None) => "?".into(),
        }
    }
}

/// Facts about one repository that do not belong to any single checkout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoFacts {
    pub id: String,
    /// What to call it — the last path segment of the origin, or of the
    /// common dir's parent.
    pub name: String,
    /// The main line the checkouts are measured against, as git names it
    /// (`origin/main`, `main`, `master`). `None` = none found.
    pub main_ref: Option<String>,
    /// `git worktree list` on the primary checkout. Includes ones no pane is in.
    pub worktrees_on_disk: Option<u32>,
    /// Commits landed anywhere in the repository in the last hour, in twelve
    /// five-minute buckets, oldest first. The heartbeat.
    pub pulse: Option<[u32; 12]>,
}

/// A pane filed under this project but operating in another repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Foreign {
    pub writer: Writer,
    /// The repository it is actually in, by name.
    pub repo_name: String,
}

/// A pane filed under some other branch of the tree but operating in THIS
/// project's repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Visitor {
    pub writer: Writer,
    pub filed_under: Option<String>,
}

/// The engineering state of one declared project, as of one scan.
#[derive(Clone, Debug)]
pub struct ProjectState {
    pub project: Option<u32>,
    pub name: Option<String>,
    pub scanned_at: Instant,
    pub took: Duration,
    /// Every repository any of this project's panes operates in. The first is
    /// the PRIMARY — the one with the most writers.
    pub repos: Vec<RepoFacts>,
    pub checkouts: Vec<Checkout>,
    /// Panes whose cwd is inside no repository at all.
    pub no_git: Vec<Writer>,
    pub foreign: Vec<Foreign>,
    pub visitors: Vec<Visitor>,
}

/// How loud a piece of the rail is. Bound once, here, and read by the
/// renderer — the model never says a colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// A fact. The rail's ordinary voice.
    Plain,
    /// Quieter than plain — the mark for something merely noted.
    Muted,
    /// Ready, clean, or the recommended state.
    Good,
    /// Something a person should look at.
    Warn,
}

/// One segment of the persistent badge beside the project's name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub tone: Tone,
}

/// What kind of fact a ticker frame carries — so a renderer can pick a glyph,
/// and a test can say which frames appeared without matching prose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrameKind {
    Dirty,
    Branches,
    Shared,
    Foreign,
    Visitors,
    NoGit,
    Repos,
    Worktrees,
    Pulse,
    Sentence,
    /// What happened between two readings — assembled by the window from
    /// [`events`], since a single reading cannot know what changed.
    Events,
}

/// One frame of the rotating ticker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub kind: FrameKind,
    pub text: String,
    pub tone: Tone,
}

// ---- the scan ------------------------------------------------------------

/// Run one git command in `dir`, with a ceiling on how long it may take. A
/// scan runs on the background executor, so a hung git would not freeze the
/// window — but it would stop every later scan behind it, which is the same
/// outcome one project at a time.
fn git(dir: &Path, args: &[&str], budget: Duration) -> Option<String> {
    let mut child = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut out = String::new();
                use std::io::Read;
                child.stdout.take()?.read_to_string(&mut out).ok()?;
                return Some(out);
            }
            Ok(None) if started.elapsed() < budget => {
                std::thread::sleep(Duration::from_millis(15));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

const BUDGET: Duration = Duration::from_secs(8);

/// The identity of a repository across its checkouts.
///
/// Two worktrees of one clone share a common dir; two CLONES of one repository
/// do not, and yet a person would call them the same project — so the origin
/// URL is the identity when there is one, normalised so `git@github-bfs:o/r`,
/// `git@github.com:o/r.git` and `https://github.com/o/r` agree. A repository
/// with no origin is identified by its common dir, which makes each such
/// clone its own repository — the only reading available.
pub fn repo_identity(origin: Option<&str>, common_dir: &Path) -> String {
    match origin.map(str::trim).filter(|s| !s.is_empty()) {
        // a local bare repository as origin: clones of it are one repository,
        // named by the path rather than by a host's owner/repo
        Some(url) if url.starts_with('/') || url.starts_with("file://") || url.starts_with('.') => {
            let p = url.strip_prefix("file://").unwrap_or(url);
            format!("path:{}", p.trim_end_matches('/').trim_end_matches(".git"))
        }
        Some(url) => normalise_origin(url),
        None => format!("dir:{}", common_dir.display()),
    }
}

fn normalise_origin(url: &str) -> String {
    let mut s = url.to_ascii_lowercase();
    for prefix in ["ssh://", "https://", "http://", "git://"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.to_string();
        }
    }
    // git@host:owner/repo → host/owner/repo; then drop the host, which is the
    // part an ssh alias rewrites
    if let Some(rest) = s.strip_prefix("git@") {
        s = rest.replacen(':', "/", 1);
    }
    let s = s.trim_end_matches('/').trim_end_matches(".git");
    // keep owner/repo — the last two segments — and nothing host-specific
    let segs: Vec<&str> = s.split('/').filter(|p| !p.is_empty()).collect();
    match segs.len() {
        0 => String::new(),
        1 => segs[0].to_string(),
        n => format!("{}/{}", segs[n - 2], segs[n - 1]),
    }
}

/// What to call a repository, from its identity.
fn repo_name(id: &str, root: &Path) -> String {
    if id.starts_with("dir:") || id.starts_with("path:") {
        // no host to name it: the checkout's own directory is what a person
        // calls it
        return root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| id.to_string());
    }
    id.rsplit('/').next().unwrap_or(id).to_string()
}

/// Count `git status --porcelain` lines: `(dirty tracked, untracked)`.
pub fn count_status(porcelain: &str) -> (u32, u32) {
    let mut dirty = 0;
    let mut untracked = 0;
    for line in porcelain.lines() {
        if line.len() < 2 {
            continue;
        }
        if line.starts_with("??") {
            untracked += 1;
        } else if line.starts_with("!!") {
            continue;
        } else {
            dirty += 1;
        }
    }
    (dirty, untracked)
}

/// Sum a `git diff --numstat` into `(added, removed)`, skipping binaries.
pub fn sum_numstat(numstat: &str) -> (u32, u32) {
    let mut add = 0u32;
    let mut del = 0u32;
    for line in numstat.lines() {
        let mut it = line.split('\t');
        let (Some(a), Some(d)) = (it.next(), it.next()) else {
            continue;
        };
        if let (Ok(a), Ok(d)) = (a.parse::<u32>(), d.parse::<u32>()) {
            add = add.saturating_add(a);
            del = del.saturating_add(d);
        }
    }
    (add, del)
}

/// Bucket commit timestamps into twelve five-minute bins ending at `now`.
pub fn bucket_pulse(stamps: &[u64], now: u64) -> [u32; 12] {
    let mut bins = [0u32; 12];
    for &t in stamps {
        if t > now {
            continue;
        }
        let age = now - t;
        if age >= 3600 {
            continue;
        }
        let from_end = (age / 300) as usize; // 0 = newest five minutes
        bins[11 - from_end] += 1;
    }
    bins
}

/// One pass over everything `input` names. Blocking; call it off the main
/// thread. Never panics on a broken directory — a cwd that has gone away is a
/// writer with no repository, which is a fact and not a fault.
pub fn scan(input: &ScanInput) -> ProjectState {
    let started = Instant::now();
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Resolve every distinct cwd — mine and others' — to its checkout root,
    // once. Panes share directories far more often than not.
    let mut root_of: HashMap<PathBuf, Option<PathBuf>> = HashMap::new();
    let all_dirs = input
        .mine
        .iter()
        .map(|w| &w.cwd)
        .chain(input.others.iter().map(|(_, w)| &w.cwd));
    for dir in all_dirs {
        if root_of.contains_key(dir) {
            continue;
        }
        let root = if dir.is_dir() {
            git(dir, &["rev-parse", "--show-toplevel"], BUDGET)
                .map(|s| PathBuf::from(s.trim()))
                .filter(|p| !p.as_os_str().is_empty())
        } else {
            None
        };
        root_of.insert(dir.clone(), root);
    }

    // Identity per root, once.
    let mut repo_of_root: HashMap<PathBuf, String> = HashMap::new();
    for root in root_of.values().flatten() {
        if repo_of_root.contains_key(root) {
            continue;
        }
        let common = git(root, &["rev-parse", "--git-common-dir"], BUDGET)
            .map(|s| {
                let p = PathBuf::from(s.trim());
                if p.is_absolute() {
                    p
                } else {
                    root.join(p)
                }
            })
            .unwrap_or_else(|| root.join(".git"));
        let origin = git(root, &["remote", "get-url", "origin"], BUDGET);
        repo_of_root.insert(root.clone(), repo_identity(origin.as_deref(), &common));
    }

    // My checkouts, keyed by root, with their writers.
    let mut checkouts: Vec<Checkout> = Vec::new();
    let mut no_git = Vec::new();
    for w in &input.mine {
        match root_of.get(&w.cwd).cloned().flatten() {
            Some(root) => {
                if let Some(c) = checkouts.iter_mut().find(|c| c.root == root) {
                    c.writers.push(w.clone());
                } else {
                    checkouts.push(Checkout {
                        repo: repo_of_root.get(&root).cloned().unwrap_or_default(),
                        root,
                        branch: None,
                        head: None,
                        dirty: None,
                        untracked: None,
                        delta: None,
                        ahead_behind: None,
                        last_commit: None,
                        writers: vec![w.clone()],
                    });
                }
            }
            None => no_git.push(w.clone()),
        }
    }

    // The primary repository: most writers, ties to the earliest tab.
    let mut writers_per_repo: Vec<(String, usize)> = Vec::new();
    for c in &checkouts {
        match writers_per_repo.iter_mut().find(|(r, _)| *r == c.repo) {
            Some((_, n)) => *n += c.writers.len(),
            None => writers_per_repo.push((c.repo.clone(), c.writers.len())),
        }
    }
    let primary = writers_per_repo
        .iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
        .map(|(r, _)| r.clone());
    // stable: primary first, then the rest in first-seen order
    let mut repo_ids: Vec<String> = Vec::new();
    if let Some(p) = &primary {
        repo_ids.push(p.clone());
    }
    for (r, _) in &writers_per_repo {
        if !repo_ids.contains(r) {
            repo_ids.push(r.clone());
        }
    }

    // Measure each checkout.
    for c in checkouts.iter_mut() {
        let root = c.root.clone();
        if let Some(out) = git(&root, &["rev-parse", "--abbrev-ref", "HEAD"], BUDGET) {
            let b = out.trim();
            c.branch = (b != "HEAD" && !b.is_empty()).then(|| b.to_string());
        }
        c.head =
            git(&root, &["rev-parse", "--short", "HEAD"], BUDGET).map(|s| s.trim().to_string());
        if let Some(st) = git(
            &root,
            &["status", "--porcelain", "--untracked-files=normal"],
            BUDGET,
        ) {
            let (d, u) = count_status(&st);
            c.dirty = Some(d);
            c.untracked = Some(u);
        }
        if let Some(ns) = git(&root, &["diff", "--numstat", "HEAD"], BUDGET) {
            c.delta = Some(sum_numstat(&ns));
        }
        c.last_commit = git(&root, &["log", "-1", "--format=%ct"], BUDGET)
            .and_then(|s| s.trim().parse::<u64>().ok());
    }

    // Repository-level facts, measured on the first checkout of each.
    let mut repos = Vec::new();
    for id in &repo_ids {
        let Some(c) = checkouts.iter().find(|c| c.repo == *id) else {
            continue;
        };
        let root = c.root.clone();
        let main_ref = ["origin/main", "main", "origin/master", "master"]
            .into_iter()
            .find(|r| git(&root, &["rev-parse", "--verify", "--quiet", r], BUDGET).is_some())
            .map(str::to_string);
        let worktrees_on_disk = git(&root, &["worktree", "list", "--porcelain"], BUDGET)
            .map(|s| s.lines().filter(|l| l.starts_with("worktree ")).count() as u32);
        let pulse = git(
            &root,
            &["log", "--all", "--since=1.hour", "--format=%ct"],
            BUDGET,
        )
        .map(|s| {
            let stamps: Vec<u64> = s.lines().filter_map(|l| l.trim().parse().ok()).collect();
            bucket_pulse(&stamps, now_unix)
        });
        repos.push(RepoFacts {
            id: id.clone(),
            name: repo_name(id, &root),
            main_ref,
            worktrees_on_disk,
            pulse,
        });
    }

    // Ahead/behind needs the repository's main line, so it comes after.
    for c in checkouts.iter_mut() {
        let Some(main_ref) = repos
            .iter()
            .find(|r| r.id == c.repo)
            .and_then(|r| r.main_ref.clone())
        else {
            continue;
        };
        let spec = format!("{main_ref}...HEAD");
        c.ahead_behind = git(
            &c.root,
            &["rev-list", "--left-right", "--count", &spec],
            BUDGET,
        )
        .and_then(|s| {
            let mut it = s.split_whitespace();
            let behind = it.next()?.parse::<u32>().ok()?;
            let ahead = it.next()?.parse::<u32>().ok()?;
            Some((ahead, behind))
        });
    }

    // Drift, both directions, against the primary repository.
    let mut foreign = Vec::new();
    let mut visitors = Vec::new();
    if let Some(p) = &primary {
        for c in &checkouts {
            if c.repo != *p {
                for w in &c.writers {
                    foreign.push(Foreign {
                        writer: w.clone(),
                        repo_name: repo_name(&c.repo, &c.root),
                    });
                }
            }
        }
        for (filed_under, w) in &input.others {
            let Some(root) = root_of.get(&w.cwd).cloned().flatten() else {
                continue;
            };
            if repo_of_root.get(&root) == Some(p) {
                visitors.push(Visitor {
                    writer: w.clone(),
                    filed_under: filed_under.clone(),
                });
            }
        }
    }

    // Checkouts in tab order, so the table reads the way the tree does.
    checkouts.sort_by_key(|c| c.writers.iter().map(|w| w.tab).min().unwrap_or(usize::MAX));

    ProjectState {
        project: input.project,
        name: input.name.clone(),
        scanned_at: started,
        took: started.elapsed(),
        repos,
        checkouts,
        no_git,
        foreign,
        visitors,
    }
}

// ---- derivations -----------------------------------------------------------

impl ProjectState {
    pub fn primary(&self) -> Option<&RepoFacts> {
        self.repos.first()
    }

    /// The checkouts of the primary repository — the ones the badge counts.
    pub fn primary_checkouts(&self) -> Vec<&Checkout> {
        match self.primary() {
            Some(p) => self.checkouts.iter().filter(|c| c.repo == p.id).collect(),
            None => Vec::new(),
        }
    }

    pub fn shared_count(&self) -> usize {
        self.primary_checkouts()
            .iter()
            .filter(|c| c.shared())
            .count()
    }

    /// Checkouts of the primary with uncommitted tracked changes. `None` if
    /// any of them could not be measured — a partial count would read as a
    /// whole one.
    pub fn dirty_checkouts(&self) -> Option<usize> {
        // no repository means nothing was measured — not zero of it
        self.primary()?;
        let cs = self.primary_checkouts();
        if cs.iter().any(|c| c.dirty.is_none()) {
            return None;
        }
        Some(cs.iter().filter(|c| c.dirty.unwrap_or(0) > 0).count())
    }

    /// Files and lines changed across the primary's checkouts, or `None` when
    /// any checkout is unmeasured.
    pub fn churn(&self) -> Option<(u32, u32, u32)> {
        self.primary()?;
        let cs = self.primary_checkouts();
        let mut files = 0u32;
        let mut add = 0u32;
        let mut del = 0u32;
        for c in cs {
            files = files.saturating_add(c.dirty?);
            let (a, d) = c.delta?;
            add = add.saturating_add(a);
            del = del.saturating_add(d);
        }
        Some((files, add, del))
    }

    /// The persistent badge. Never empty once a scan has landed.
    pub fn badge(&self) -> Vec<Segment> {
        let mut out = Vec::new();
        if self.repos.is_empty() {
            if self.checkouts.is_empty() && self.no_git.is_empty() {
                out.push(Segment {
                    text: "NO PANES".into(),
                    tone: Tone::Muted,
                });
            } else {
                out.push(Segment {
                    text: "NO GIT".into(),
                    tone: Tone::Muted,
                });
            }
            return out;
        }
        if self.repos.len() > 1 {
            out.push(Segment {
                text: format!("{} REPOS", self.repos.len()),
                tone: Tone::Plain,
            });
        }
        let n = self.primary_checkouts().len();
        let shared = self.shared_count();
        let foreign = self.foreign.len();
        let wt = if shared == 0 && foreign == 0 && n > 0 {
            format!("{n} WT \u{2713}")
        } else {
            format!("{n} WT")
        };
        out.push(Segment {
            text: wt,
            tone: if shared == 0 && foreign == 0 {
                Tone::Good
            } else {
                Tone::Plain
            },
        });
        if shared > 0 {
            out.push(Segment {
                text: format!("{shared} SHARED"),
                tone: Tone::Warn,
            });
        }
        if foreign > 0 {
            out.push(Segment {
                text: format!("\u{26a0} {foreign} FOREIGN"),
                tone: Tone::Warn,
            });
        }
        out
    }

    /// The rotating frames, in the order they cycle. Empty is the healthy
    /// silence — nothing here was worth interrupting for.
    pub fn frames(&self) -> Vec<Frame> {
        let mut out = Vec::new();
        let Some(primary) = self.primary() else {
            if !self.no_git.is_empty() {
                out.push(Frame {
                    kind: FrameKind::NoGit,
                    text: format!("{} outside any repository", panes(self.no_git.len())),
                    tone: Tone::Muted,
                });
            }
            return out;
        };

        // 1 · what is uncommitted
        match (self.dirty_checkouts(), self.churn()) {
            (Some(d), Some((files, add, del))) if d > 0 => {
                out.push(Frame {
                    kind: FrameKind::Dirty,
                    text: format!(
                        "{d} dirty \u{00b7} +{add} \u{2212}{del} \u{00b7} {}",
                        plural(files, "file", "files")
                    ),
                    tone: Tone::Plain,
                });
            }
            (None, _) | (_, None) => out.push(Frame {
                kind: FrameKind::Dirty,
                text: "dirty: not measured".into(),
                tone: Tone::Muted,
            }),
            _ => {}
        }

        // 2 · the lines of work and where they stand against main
        let main_name = primary
            .main_ref
            .as_deref()
            .map(|m| m.rsplit('/').next().unwrap_or(m).to_string());
        let lines: Vec<String> = self
            .primary_checkouts()
            .iter()
            .filter(|c| Some(c.line()) != main_name)
            .map(|c| {
                let mut s = c.line();
                if let Some((a, b)) = c.ahead_behind {
                    if a > 0 {
                        s.push_str(&format!(" \u{2191}{a}"));
                    }
                    if b > 0 {
                        s.push_str(&format!(" \u{2193}{b}"));
                    }
                }
                s
            })
            .collect();
        if !lines.is_empty() {
            let text = match &main_name {
                Some(m) => format!("{m} \u{2190} {}", lines.join(" \u{00b7} ")),
                None => lines.join(" \u{00b7} "),
            };
            out.push(Frame {
                kind: FrameKind::Branches,
                text,
                tone: Tone::Plain,
            });
        }

        // 3 · a checkout with more than one writer
        for c in self.primary_checkouts().iter().filter(|c| c.shared()) {
            let who: Vec<String> = c.writers.iter().map(|w| w.label.to_lowercase()).collect();
            out.push(Frame {
                kind: FrameKind::Shared,
                text: format!(
                    "{} is shared by {}: {}",
                    c.line(),
                    plural(c.writers.len() as u32, "writer", "writers"),
                    who.join(" \u{00b7} ")
                ),
                tone: Tone::Warn,
            });
        }

        // 4 · drift, both directions
        for f in &self.foreign {
            out.push(Frame {
                kind: FrameKind::Foreign,
                text: format!(
                    "\u{26a0} {} is filed under {} but is operating in {}",
                    f.writer.tab_name,
                    self.name.as_deref().unwrap_or("this project"),
                    f.repo_name
                ),
                tone: Tone::Warn,
            });
        }
        if !self.visitors.is_empty() {
            let text = if self.visitors.len() == 1 {
                let v = &self.visitors[0];
                match &v.filed_under {
                    Some(u) => format!(
                        "\u{25cc} {} is operating here but is filed under {u}",
                        v.writer.tab_name
                    ),
                    None => format!("\u{25cc} {} is operating here, unfiled", v.writer.tab_name),
                }
            } else {
                format!(
                    "\u{25cc} {} external operating here",
                    panes(self.visitors.len())
                )
            };
            out.push(Frame {
                kind: FrameKind::Visitors,
                text,
                tone: Tone::Muted,
            });
        }
        if !self.no_git.is_empty() {
            out.push(Frame {
                kind: FrameKind::NoGit,
                text: format!("{} outside any repository", panes(self.no_git.len())),
                tone: Tone::Muted,
            });
        }
        if self.repos.len() > 1 {
            let names: Vec<&str> = self.repos.iter().map(|r| r.name.as_str()).collect();
            out.push(Frame {
                kind: FrameKind::Repos,
                text: format!(
                    "{} repositories: {}",
                    self.repos.len(),
                    names.join(" \u{00b7} ")
                ),
                tone: Tone::Plain,
            });
        }

        // 5 · worktrees on disk that nothing is in
        let in_use = self.primary_checkouts().len() as u32;
        if let Some(on_disk) = primary.worktrees_on_disk {
            if on_disk > in_use {
                out.push(Frame {
                    kind: FrameKind::Worktrees,
                    text: format!(
                        "{} on disk \u{00b7} {in_use} in use",
                        plural(on_disk, "worktree", "worktrees")
                    ),
                    tone: Tone::Muted,
                });
            }
        }

        // 6 · the heartbeat, as a sentence, only when it is beating
        if let Some(p) = primary.pulse {
            let total: u32 = p.iter().sum();
            if total > 0 {
                let recent: u32 = p[9..].iter().sum();
                out.push(Frame {
                    kind: FrameKind::Pulse,
                    text: format!(
                        "{} in the last hour \u{00b7} {recent} in the last fifteen minutes",
                        plural(total, "commit", "commits")
                    ),
                    tone: Tone::Plain,
                });
            }
        }

        // 7 · the derived sentence, last, so it lands after the facts it sums
        if let Some(s) = self.sentence() {
            out.push(Frame {
                kind: FrameKind::Sentence,
                text: s,
                tone: Tone::Muted,
            });
        }
        out
    }

    /// The project's situation, in English, derived and never authored.
    pub fn sentence(&self) -> Option<String> {
        let primary = self.primary()?;
        let cs = self.primary_checkouts();
        let n = cs.len();
        let shared = self.shared_count();
        let mut parts: Vec<String> = Vec::new();
        parts.push(match (n, shared) {
            (0, _) => "no checkout in use".into(),
            (1, 0) => "one line of work".into(),
            (n, 0) => format!("{n} isolated lines of work"),
            (n, s) => format!("{n} lines of work, {} shared", number(s)),
        });
        let main_name = primary
            .main_ref
            .as_deref()
            .map(|m| m.rsplit('/').next().unwrap_or(m).to_string());
        if let Some(m) = &main_name {
            if let Some(c) = cs.iter().find(|c| Some(c.line()) == main_name) {
                parts.push(match c.is_dirty() {
                    Some(false) => format!("{m} is clean"),
                    Some(true) => format!("{m} has uncommitted work"),
                    None => format!("{m} is unmeasured"),
                });
            }
        }
        match self.dirty_checkouts() {
            Some(0) => parts.push("nothing uncommitted".into()),
            Some(d) if n > 1 => parts.push(format!("{} dirty", number(d))),
            _ => {}
        }
        if !self.foreign.is_empty() {
            parts.push(format!("{} wandering", panes(self.foreign.len())));
        }
        if let Some(p) = primary.pulse {
            let total: u32 = p.iter().sum();
            if total == 0 {
                parts.push("quiet for an hour".into());
            }
        }
        Some(parts.join(" \u{00b7} "))
    }
}

/// What changed between two readings of the same project — the afterglow.
///
/// A reading is a photograph; the interesting thing is often the difference
/// between two of them: a commit landed on a line while you were in another
/// project, a checkout went dirty, main moved on under a branch, a pane
/// wandered in. Each is one sentence, in the order the checkouts are listed,
/// and nothing here is a judgement — the rail keeps them for a while as
/// evidence that something happened, not as a notification to act on.
///
/// Pure, so it is tested on hand-built readings with no git at all.
pub fn events(prev: &ProjectState, next: &ProjectState) -> Vec<String> {
    let mut out = Vec::new();
    if prev.project != next.project {
        return out;
    }
    for c in &next.checkouts {
        let Some(p) = prev.checkouts.iter().find(|p| p.root == c.root) else {
            out.push(format!("a pane arrived in {}", c.line()));
            continue;
        };
        if p.branch != c.branch && c.branch.is_some() {
            out.push(format!("{} switched to {}", short_root(&c.root), c.line()));
        }
        if let (Some(a), Some(b)) = (p.last_commit, c.last_commit) {
            if b > a {
                out.push(format!("a commit landed on {}", c.line()));
            }
        }
        match (p.is_dirty(), c.is_dirty()) {
            (Some(false), Some(true)) => out.push(format!("{} went dirty", c.line())),
            (Some(true), Some(false)) => out.push(format!("{} went clean", c.line())),
            _ => {}
        }
        if let (Some((_, pb)), Some((_, nb))) = (p.ahead_behind, c.ahead_behind) {
            if nb > pb {
                out.push(format!("main moved on: {} is now {nb} behind", c.line()));
            }
        }
    }
    for p in &prev.checkouts {
        if !next.checkouts.iter().any(|c| c.root == p.root) {
            out.push(format!("{} was left", p.line()));
        }
    }
    if next.foreign.len() > prev.foreign.len() {
        let names: Vec<&str> = next
            .foreign
            .iter()
            .filter(|f| !prev.foreign.iter().any(|g| g.writer.cwd == f.writer.cwd))
            .map(|f| f.repo_name.as_str())
            .collect();
        if names.is_empty() {
            out.push("a pane wandered off".into());
        } else {
            out.push(format!("a pane wandered into {}", names.join(" \u{00b7} ")));
        }
    } else if next.foreign.len() < prev.foreign.len() {
        out.push("a wandering pane came home".into());
    }
    if next.visitors.len() > prev.visitors.len() {
        out.push("an external pane arrived".into());
    }
    out
}

/// A checkout's directory, as a person would say it.
fn short_root(root: &Path) -> String {
    root.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.display().to_string())
}

fn plural(n: u32, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

fn panes(n: usize) -> String {
    plural(n as u32, "pane", "panes")
}

fn number(n: usize) -> String {
    match n {
        1 => "one".into(),
        2 => "two".into(),
        3 => "three".into(),
        4 => "four".into(),
        5 => "five".into(),
        n => n.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(tab: usize, label: &str, cwd: &Path) -> Writer {
        Writer {
            tab,
            tab_name: format!("tab {tab}"),
            label: label.into(),
            cwd: cwd.to_path_buf(),
        }
    }

    fn sh(dir: &Path, args: &[&str]) {
        let st = Command::new(args[0])
            .args(&args[1..])
            .current_dir(dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("spawn");
        assert!(st.success(), "{args:?} in {}", dir.display());
    }

    /// A throwaway repository with one commit on `main`, an `origin` remote
    /// pointing at a bare clone of itself, and a second worktree on a branch.
    struct Rig {
        base: PathBuf,
        main: PathBuf,
        wt: PathBuf,
        outside: PathBuf,
    }

    impl Rig {
        fn new(tag: &str) -> Rig {
            let base = std::env::temp_dir().join(format!(
                "td-engstate-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let main = base.join("repo");
            let bare = base.join("origin.git");
            let wt = base.join("repo-feature");
            let outside = base.join("plain");
            std::fs::create_dir_all(&main).unwrap();
            std::fs::create_dir_all(&outside).unwrap();
            sh(&main, &["git", "init", "-q", "-b", "main"]);
            sh(&main, &["git", "config", "user.email", "t@t"]);
            sh(&main, &["git", "config", "user.name", "t"]);
            std::fs::write(main.join("a.txt"), "one\n").unwrap();
            sh(&main, &["git", "add", "."]);
            sh(&main, &["git", "commit", "-q", "-m", "one"]);
            sh(
                &base,
                &["git", "clone", "-q", "--bare", "repo", "origin.git"],
            );
            sh(
                &main,
                &["git", "remote", "add", "origin", bare.to_str().unwrap()],
            );
            sh(&main, &["git", "fetch", "-q", "origin"]);
            sh(
                &main,
                &[
                    "git",
                    "worktree",
                    "add",
                    "-q",
                    "-b",
                    "feature",
                    wt.to_str().unwrap(),
                ],
            );
            Rig {
                base,
                main,
                wt,
                outside,
            }
        }
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    #[test]
    fn origins_spelled_three_ways_are_one_repository() {
        let d = Path::new("/x/.git");
        let a = repo_identity(
            Some("git@github-bfs:parker-brown-family/terminal-delight"),
            d,
        );
        let b = repo_identity(
            Some("git@github.com:parker-brown-family/terminal-delight.git"),
            d,
        );
        let c = repo_identity(
            Some("https://github.com/parker-brown-family/terminal-delight"),
            d,
        );
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_eq!(a, "parker-brown-family/terminal-delight");
        assert_eq!(repo_name(&a, Path::new("/x")), "terminal-delight");
        // a local bare origin names the repository by the checkout, not the host
        let p = repo_identity(Some("/tmp/rig/origin.git"), d);
        assert_eq!(p, "path:/tmp/rig/origin");
        assert_eq!(repo_name(&p, Path::new("/tmp/rig/repo")), "repo");
    }

    #[test]
    fn a_repository_with_no_origin_is_its_own_directory() {
        let id = repo_identity(None, Path::new("/home/p/thing/.git"));
        assert_eq!(id, "dir:/home/p/thing/.git");
        assert_eq!(repo_name(&id, Path::new("/home/p/thing")), "thing");
        let id = repo_identity(Some("   "), Path::new("/home/p/thing/.git"));
        assert!(id.starts_with("dir:"), "blank origin is no origin");
    }

    #[test]
    fn status_lines_are_counted_by_kind() {
        let s = " M app/src/main.rs\n?? reports/x.html\nA  new.rs\n!! ignored\nMM both\n";
        assert_eq!(count_status(s), (3, 1));
        assert_eq!(count_status(""), (0, 0));
    }

    #[test]
    fn numstat_sums_and_skips_binaries() {
        let n = "12\t3\ta.rs\n-\t-\tlogo.png\n0\t7\tb.rs\n";
        assert_eq!(sum_numstat(n), (12, 10));
    }

    #[test]
    fn the_pulse_buckets_the_last_hour_newest_last() {
        let now = 10_000;
        let stamps = [
            now - 10,
            now - 299,
            now - 301,
            now - 3599,
            now - 3600,
            now + 5,
        ];
        let p = bucket_pulse(&stamps, now);
        assert_eq!(p[11], 2, "two in the newest five minutes");
        assert_eq!(p[10], 1);
        assert_eq!(p[0], 1, "3599s ago is the oldest bucket");
        assert_eq!(
            p.iter().sum::<u32>(),
            4,
            "an hour ago and the future are out"
        );
    }

    #[test]
    fn an_empty_input_scans_to_no_panes_and_says_so() {
        let st = scan(&ScanInput::default());
        assert!(st.repos.is_empty() && st.checkouts.is_empty());
        assert_eq!(st.badge()[0].text, "NO PANES");
        assert!(st.frames().is_empty(), "nothing to say about nothing");
        assert_eq!(st.sentence(), None);
    }

    #[test]
    fn a_pane_outside_any_repository_is_no_git_not_zero_dirty() {
        let rig = Rig::new("nogit");
        let st = scan(&ScanInput {
            project: Some(1),
            name: Some("PLAIN".into()),
            mine: vec![w(0, "SHELL", &rig.outside)],
            others: vec![],
        });
        assert_eq!(st.no_git.len(), 1);
        assert!(st.checkouts.is_empty());
        assert_eq!(st.badge()[0].text, "NO GIT");
        assert!(
            st.dirty_checkouts().is_none(),
            "nothing measured is not zero"
        );
        let f = st.frames();
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].kind, FrameKind::NoGit);
        assert_eq!(f[0].text, "1 pane outside any repository");
    }

    #[test]
    fn two_worktrees_one_shared_is_read_from_the_filesystem() {
        let rig = Rig::new("shared");
        std::fs::write(rig.wt.join("a.txt"), "one\ntwo\n").unwrap();
        let st = scan(&ScanInput {
            project: Some(1),
            name: Some("REPO".into()),
            mine: vec![
                w(0, "CLAUDE", &rig.main),
                w(1, "SHELL", &rig.main),
                w(2, "CLAUDE", &rig.wt),
            ],
            others: vec![],
        });
        assert_eq!(st.repos.len(), 1, "two worktrees, one repository");
        assert_eq!(st.checkouts.len(), 2);
        assert_eq!(st.shared_count(), 1);
        let main = st.checkouts.iter().find(|c| c.root == rig.main).unwrap();
        let wt = st.checkouts.iter().find(|c| c.root == rig.wt).unwrap();
        assert_eq!(main.branch.as_deref(), Some("main"));
        assert_eq!(wt.branch.as_deref(), Some("feature"));
        assert!(main.shared() && !wt.shared());
        assert_eq!(main.dirty, Some(0));
        assert_eq!(wt.dirty, Some(1));
        assert_eq!(wt.delta, Some((1, 0)));
        assert_eq!(wt.ahead_behind, Some((0, 0)), "feature is level with main");
        assert_eq!(
            st.primary().unwrap().main_ref.as_deref(),
            Some("origin/main")
        );
        assert_eq!(st.primary().unwrap().worktrees_on_disk, Some(2));
        assert!(st.primary().unwrap().pulse.is_some());

        let badge: Vec<String> = st.badge().into_iter().map(|s| s.text).collect();
        assert_eq!(badge, vec!["2 WT", "1 SHARED"]);
        let kinds: Vec<FrameKind> = st.frames().iter().map(|f| f.kind).collect();
        assert!(kinds.contains(&FrameKind::Dirty));
        assert!(kinds.contains(&FrameKind::Branches));
        assert!(kinds.contains(&FrameKind::Shared));
        assert!(!kinds.contains(&FrameKind::Foreign));
        let dirty = st
            .frames()
            .into_iter()
            .find(|f| f.kind == FrameKind::Dirty)
            .unwrap();
        assert_eq!(dirty.text, "1 dirty \u{00b7} +1 \u{2212}0 \u{00b7} 1 file");
        let shared = st
            .frames()
            .into_iter()
            .find(|f| f.kind == FrameKind::Shared)
            .unwrap();
        assert_eq!(
            shared.text,
            "main is shared by 2 writers: claude \u{00b7} shell"
        );
        let s = st.sentence().unwrap();
        assert!(s.starts_with("2 lines of work, one shared"), "{s}");
        assert!(s.contains("main is clean"), "{s}");
    }

    #[test]
    fn isolated_and_clean_is_a_tick_and_near_silence() {
        let rig = Rig::new("clean");
        let st = scan(&ScanInput {
            project: Some(1),
            name: Some("REPO".into()),
            mine: vec![w(0, "CLAUDE", &rig.main), w(1, "CODEX", &rig.wt)],
            others: vec![],
        });
        let badge: Vec<String> = st.badge().into_iter().map(|s| s.text).collect();
        assert_eq!(badge, vec!["2 WT \u{2713}"]);
        let kinds: Vec<FrameKind> = st.frames().iter().map(|f| f.kind).collect();
        assert!(
            !kinds.contains(&FrameKind::Dirty),
            "clean says nothing about dirt"
        );
        assert!(!kinds.contains(&FrameKind::Shared));
        // the branch line and the sentence are all that is left
        assert!(kinds.contains(&FrameKind::Branches));
        assert!(kinds.contains(&FrameKind::Sentence));
        let s = st.sentence().unwrap();
        assert!(
            s.starts_with(
                "2 isolated lines of work \u{00b7} main is clean \u{00b7} nothing uncommitted"
            ),
            "{s}"
        );
    }

    #[test]
    fn a_pane_filed_here_but_working_elsewhere_is_foreign_and_a_visitor_there() {
        let a = Rig::new("drift-a");
        let b = Rig::new("drift-b");
        // project A holds two panes in repo A and one pane that wandered into repo B
        let st = scan(&ScanInput {
            project: Some(1),
            name: Some("A".into()),
            mine: vec![
                w(0, "CLAUDE", &a.main),
                w(1, "SHELL", &a.wt),
                w(2, "CLAUDE", &b.main),
            ],
            // and project B has a pane sitting in repo A
            others: vec![(Some("B".into()), w(5, "CODEX", &a.wt))],
        });
        assert_eq!(st.repos.len(), 2);
        assert_eq!(
            st.primary().unwrap().name,
            "repo",
            "the repo with more writers is primary"
        );
        assert_eq!(st.foreign.len(), 1);
        assert_eq!(st.foreign[0].writer.tab, 2);
        assert_eq!(st.visitors.len(), 1);
        assert_eq!(st.visitors[0].filed_under.as_deref(), Some("B"));
        let badge: Vec<String> = st.badge().into_iter().map(|s| s.text).collect();
        assert_eq!(badge, vec!["2 REPOS", "2 WT", "\u{26a0} 1 FOREIGN"]);
        let f = st.frames();
        let foreign = f.iter().find(|f| f.kind == FrameKind::Foreign).unwrap();
        assert_eq!(
            foreign.text,
            "\u{26a0} tab 2 is filed under A but is operating in repo"
        );
        let visitor = f.iter().find(|f| f.kind == FrameKind::Visitors).unwrap();
        assert_eq!(
            visitor.text,
            "\u{25cc} tab 5 is operating here but is filed under B"
        );
        assert!(st.sentence().unwrap().contains("1 pane wandering"));
    }

    #[test]
    fn a_directory_that_has_gone_away_is_a_fact_not_a_panic() {
        let st = scan(&ScanInput {
            project: None,
            name: None,
            mine: vec![w(0, "SHELL", Path::new("/nonexistent/td-engstate/gone"))],
            others: vec![],
        });
        assert_eq!(st.no_git.len(), 1);
        assert_eq!(st.badge()[0].text, "NO GIT");
    }

    /// A hand-built reading, so the afterglow is tested without git.
    fn reading(checkouts: Vec<Checkout>, foreign: usize, visitors: usize) -> ProjectState {
        let wr = |n: usize| w(n, "SHELL", Path::new("/x"));
        ProjectState {
            project: Some(1),
            name: Some("P".into()),
            scanned_at: Instant::now(),
            took: Duration::ZERO,
            repos: vec![RepoFacts {
                id: "o/r".into(),
                name: "r".into(),
                main_ref: Some("origin/main".into()),
                worktrees_on_disk: None,
                pulse: None,
            }],
            checkouts,
            no_git: vec![],
            foreign: (0..foreign)
                .map(|i| Foreign {
                    writer: w(10 + i, "SHELL", Path::new(&format!("/f/{i}"))),
                    repo_name: "elsewhere".into(),
                })
                .collect(),
            visitors: (0..visitors)
                .map(|i| Visitor {
                    writer: wr(20 + i),
                    filed_under: None,
                })
                .collect(),
        }
    }

    fn co(root: &str, branch: &str, dirty: u32, commit: u64, behind: u32) -> Checkout {
        Checkout {
            root: PathBuf::from(root),
            repo: "o/r".into(),
            branch: Some(branch.into()),
            head: Some("abc1234".into()),
            dirty: Some(dirty),
            untracked: Some(0),
            delta: Some((0, 0)),
            ahead_behind: Some((0, behind)),
            last_commit: Some(commit),
            writers: vec![w(0, "CLAUDE", Path::new(root))],
        }
    }

    #[test]
    fn the_afterglow_is_the_difference_between_two_readings() {
        let before = reading(
            vec![
                co("/w/main", "main", 0, 100, 0),
                co("/w/rail", "rail", 2, 100, 0),
                co("/w/gone", "old", 0, 100, 0),
            ],
            0,
            0,
        );
        let after = reading(
            vec![
                co("/w/main", "main", 3, 100, 0),   // went dirty
                co("/w/rail", "rail", 0, 200, 2),   // commit landed, went clean, main moved
                co("/w/new", "feature", 0, 100, 0), // arrived
            ],
            1,
            1,
        );
        let ev = events(&before, &after);
        assert_eq!(
            ev,
            vec![
                "main went dirty",
                "a commit landed on rail",
                "rail went clean",
                "main moved on: rail is now 2 behind",
                "a pane arrived in feature",
                "old was left",
                "a pane wandered into elsewhere",
                "an external pane arrived",
            ]
        );
    }

    #[test]
    fn two_identical_readings_have_no_afterglow_and_two_projects_never_compare() {
        let a = reading(vec![co("/w/main", "main", 0, 100, 0)], 0, 0);
        assert!(events(&a, &a).is_empty());
        let mut b = a.clone();
        b.project = Some(2);
        b.checkouts.clear();
        assert!(
            events(&a, &b).is_empty(),
            "a different project is not a change"
        );
    }

    #[test]
    fn a_branch_switch_names_the_directory_not_the_old_branch() {
        let a = reading(vec![co("/w/rail", "rail", 0, 100, 0)], 0, 0);
        let b = reading(vec![co("/w/rail", "rail-2", 0, 100, 0)], 0, 0);
        assert_eq!(events(&a, &b), vec!["rail switched to rail-2"]);
    }

    #[test]
    fn the_checkouts_come_back_in_tab_order() {
        let rig = Rig::new("order");
        let st = scan(&ScanInput {
            project: Some(1),
            name: None,
            mine: vec![w(4, "SHELL", &rig.wt), w(1, "SHELL", &rig.main)],
            others: vec![],
        });
        assert_eq!(st.checkouts[0].root, rig.main);
        assert_eq!(st.checkouts[1].root, rig.wt);
    }
}
