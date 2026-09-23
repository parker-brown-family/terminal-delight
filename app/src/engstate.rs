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

use serde::Serialize;
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
    /// The panes operating here. Two or more is a SHARED checkout. Empty for
    /// an idle worktree — one on disk that nothing is in.
    pub writers: Vec<Writer>,
    /// The branch this one tracks, if it tracks one.
    pub upstream: Option<String>,
    /// Commits here that its upstream does not have. `None` when there is no
    /// upstream — which is its own landing item, not zero unpushed.
    pub unpushed: Option<u32>,
    /// Every file this line changes against main — committed since the merge
    /// base, and uncommitted — sorted, unique. `None` = not measured. For the
    /// main line itself, only the uncommitted files.
    pub touched: Option<Vec<String>>,
    /// Whether this line would merge into main today. A dry run, so it can be
    /// asked every scan without touching a working tree.
    pub merge: Option<Merge>,
}

/// What `git merge-tree` said about landing a line on main.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Merge {
    /// Nothing to do: this IS the main line, or it is not ahead of it.
    Nothing,
    Clean,
    /// The files that would conflict.
    Conflicts(Vec<String>),
}

impl Checkout {
    pub fn shared(&self) -> bool {
        self.writers.len() > 1
    }
    /// A worktree on disk that no pane is in.
    pub fn idle(&self) -> bool {
        self.writers.is_empty()
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
    /// Entries in `git stash list`. A repository fact: stashes live in the
    /// common dir and every worktree sees the same list.
    pub stashes: Option<u32>,
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
    /// Worktrees of the primary repository that no pane is in, measured the
    /// same way — because an idle worktree is where lost work hides.
    pub idle: Vec<Checkout>,
    /// Panes whose cwd is inside no repository at all.
    pub no_git: Vec<Writer>,
    pub foreign: Vec<Foreign>,
    pub visitors: Vec<Visitor>,
}

/// Two lines that change the same files — converging before they conflict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Collision {
    pub a: String,
    pub b: String,
    pub files: Vec<String>,
}

/// One thing that must become true for everything in flight to be on main.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LandingItem {
    /// The line it is about, or the repository's name for a repository fact.
    pub line: String,
    /// True when the line lives in a worktree no pane is in.
    pub idle: bool,
    pub kind: LandingKind,
    /// The sentence a coordinating agent reads.
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LandingKind {
    /// Uncommitted changes on a line.
    Commit,
    /// Commits the upstream has not seen.
    Push,
    /// A line with no upstream at all.
    NoUpstream,
    /// A merge into main that would go cleanly.
    Merge,
    /// A merge into main that would conflict.
    Conflict,
    /// A merge into main that could not be checked.
    Unchecked,
    /// Main itself is behind its origin.
    Pull,
    /// Stashes in the repository.
    Stash,
    /// Two lines converging on the same files.
    Collision,
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
    /// What must become true to land everything — the count of it.
    Landing,
    /// Two lines converging on the same files.
    Collision,
    /// Idle worktrees carrying work.
    Idle,
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

/// Run one git command in `dir`, with a ceiling on how long it may take, and
/// hand back its exit code with whatever it printed. A scan runs on the
/// background executor, so a hung git would not freeze the window — but it
/// would stop every later scan behind it, which is the same outcome one
/// project at a time.
///
/// **Stdout is drained on its own thread, concurrently with the wait.** A pipe
/// holds 64 KiB; `git status --porcelain` in a tree with a couple of thousand
/// untracked files prints more than that, and a runner that reads only after
/// the child has exited waits forever on a child that is itself blocked
/// writing into a full pipe. That deadlock costs the whole budget and then
/// reports the field as *unmeasured* — a reading git was perfectly able to
/// give. See `a_wordy_git_is_still_measured`.
fn run_git(dir: &Path, args: &[&str], budget: Duration) -> Option<(i32, String)> {
    let mut child = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let pipe = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        use std::io::Read;
        let _ = std::io::BufReader::new(pipe).read_to_string(&mut out);
        out
    });
    let started = Instant::now();
    // Poll with a short backoff rather than a fixed tick. These calls really
    // cost two to seven milliseconds each; a flat 15 ms sleep rounded every
    // one of them up to 15, which across a whole scan was more time asleep
    // than in git. `try_wait` is a `waitpid(WNOHANG)` — polling it often is
    // free next to what it saves.
    let mut nap = Duration::from_micros(150);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < budget => {
                std::thread::sleep(nap);
                nap = (nap * 2).min(POLL_CEILING);
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    // Always join: the reader ends when the pipe closes, which a kill also does.
    let out = reader.join().unwrap_or_default();
    Some((status?.code().unwrap_or(-1), out))
}

/// The longest one poll may sleep. Small enough that a fast git is not rounded
/// up, large enough that a slow one is not polled thousands of times.
const POLL_CEILING: Duration = Duration::from_millis(2);

/// Run one git command, keeping its output only when it succeeded.
fn git(dir: &Path, args: &[&str], budget: Duration) -> Option<String> {
    let (code, out) = run_git(dir, args, budget)?;
    (code == 0).then_some(out)
}

/// Like [`git`], but the exit code is part of the answer: `git merge-tree`
/// says "conflicts" with status 1 and its output, which the plain runner
/// would throw away as a failure.
fn git_status(dir: &Path, args: &[&str], budget: Duration) -> Option<(i32, String)> {
    run_git(dir, args, budget)
}

/// Run `f` over `items` on a small pool of threads, giving the results back in
/// the order the items came in.
///
/// Every measurement in a scan is a git subprocess against one directory, and
/// those directories know nothing about each other — so the scan was paying
/// for a hundred round trips end to end that it could have overlapped. The
/// pool is deliberately small: this runs beside twenty agent panes, and the
/// win is already had by the time it is this wide.
fn parallel_map<T, R, F>(items: Vec<T>, f: F) -> Vec<R>
where
    T: Send,
    R: Send,
    F: Fn(T) -> R + Sync,
{
    let n = items.len();
    if n <= 1 {
        return items.into_iter().map(f).collect();
    }
    let queue: std::sync::Mutex<Vec<(usize, T)>> =
        std::sync::Mutex::new(items.into_iter().enumerate().rev().collect());
    let done: std::sync::Mutex<Vec<(usize, R)>> = std::sync::Mutex::new(Vec::with_capacity(n));
    std::thread::scope(|s| {
        for _ in 0..workers().min(n) {
            s.spawn(|| loop {
                // Taken one at a time rather than in chunks: one checkout can
                // be an order of magnitude slower than its neighbours (a cold
                // untracked cache), and a chunked split would leave the rest
                // of the scan waiting behind it.
                let Some((i, item)) = queue.lock().expect("scan queue").pop() else {
                    break;
                };
                let r = f(item);
                done.lock().expect("scan results").push((i, r));
            });
        }
    });
    let mut out = done.into_inner().expect("scan results");
    out.sort_by_key(|(i, _)| *i);
    out.into_iter().map(|(_, r)| r).collect()
}

/// How wide the scan runs. Capped, because this shares a machine with the
/// panes it is measuring.
fn workers() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 8)
}

/// What `git merge-tree --write-tree --name-only <main> HEAD` said. Exit 0
/// is a clean merge and the output is a tree id; exit 1 is conflicts and the
/// lines after the tree id name the files; anything else is not an answer.
pub fn parse_merge_tree(code: i32, out: &str) -> Option<Merge> {
    match code {
        0 => Some(Merge::Clean),
        1 => {
            let files: Vec<String> = out
                .lines()
                .skip(1)
                .take_while(|l| !l.trim().is_empty())
                .map(|l| l.trim().to_string())
                .collect();
            Some(Merge::Conflicts(files))
        }
        _ => None,
    }
}

/// Measure what a checkout is on its own: branch, head, dirt, delta, last
/// commit, upstream and what the upstream has not seen. Shared by the
/// checkouts panes are in and the idle worktrees nobody is.
///
/// Hands back the porcelain it read, because [`measure_against_main`] wants
/// the same listing and used to ask git for it a second time.
fn measure_checkout(c: &mut Checkout) -> Option<String> {
    let root = c.root.clone();
    if let Some(out) = git(&root, &["rev-parse", "--abbrev-ref", "HEAD"], BUDGET) {
        let b = out.trim();
        c.branch = (b != "HEAD" && !b.is_empty()).then(|| b.to_string());
    }
    c.head = git(&root, &["rev-parse", "--short", "HEAD"], BUDGET).map(|s| s.trim().to_string());
    let porcelain = git(
        &root,
        &["status", "--porcelain", "--untracked-files=normal"],
        BUDGET,
    );
    if let Some(st) = &porcelain {
        let (d, u) = count_status(st);
        c.dirty = Some(d);
        c.untracked = Some(u);
    }
    if let Some(ns) = git(&root, &["diff", "--numstat", "HEAD"], BUDGET) {
        c.delta = Some(sum_numstat(&ns));
    }
    c.last_commit = git(&root, &["log", "-1", "--format=%ct"], BUDGET)
        .and_then(|s| s.trim().parse::<u64>().ok());
    // the upstream, and what it has not seen. No upstream is None on both,
    // and the landing list says so rather than counting nothing as pushed.
    c.upstream = git(&root, &["rev-parse", "--abbrev-ref", "@{u}"], BUDGET)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    c.unpushed = match &c.upstream {
        Some(_) => git(&root, &["rev-list", "--count", "@{u}..HEAD"], BUDGET)
            .and_then(|s| s.trim().parse::<u32>().ok()),
        None => None,
    };
    porcelain
}

/// Measure a checkout against its repository's main line: how far ahead and
/// behind, which files it changes, and whether it would merge today.
fn measure_against_main(c: &mut Checkout, main_ref: &str, porcelain: Option<&str>) {
    let root = c.root.clone();
    let spec = format!("{main_ref}...HEAD");
    c.ahead_behind = git(
        &root,
        &["rev-list", "--left-right", "--count", &spec],
        BUDGET,
    )
    .and_then(|s| {
        let mut it = s.split_whitespace();
        let behind = it.next()?.parse::<u32>().ok()?;
        let ahead = it.next()?.parse::<u32>().ok()?;
        Some((ahead, behind))
    });
    let main_name = main_ref.rsplit('/').next().unwrap_or(main_ref);
    let is_main = c.branch.as_deref() == Some(main_name);
    // files touched: committed since the merge base (not for main itself),
    // plus whatever is uncommitted right now
    let committed = if is_main {
        Some(String::new())
    } else {
        git(&root, &["diff", "--name-only", &spec], BUDGET)
    };
    // The listing [`measure_checkout`] already read, minus the untracked
    // entries — which is exactly what `--untracked-files=no` prints, and one
    // fewer subprocess. Reading one snapshot rather than two also removes a
    // window in which a file could be counted by one call and not the other.
    let uncommitted = porcelain.map(|p| {
        p.lines()
            .filter(|l| !l.starts_with("??"))
            .collect::<Vec<_>>()
            .join("\n")
    });
    c.touched = match (committed, uncommitted) {
        (Some(cm), Some(un)) => {
            let mut files: Vec<String> = cm
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            for l in un.lines() {
                if l.len() > 3 {
                    // "XY path" — and "XY old -> new" for a rename
                    let p = l[3..].rsplit(" -> ").next().unwrap_or(&l[3..]);
                    files.push(p.trim().to_string());
                }
            }
            files.sort();
            files.dedup();
            Some(files)
        }
        _ => None,
    };
    // the dry run. Skipped for main and for a line with nothing to merge —
    // `Nothing` is an answer, not an absence.
    c.merge = if is_main || c.ahead_behind.map(|(a, _)| a) == Some(0) {
        Some(Merge::Nothing)
    } else {
        git_status(
            &root,
            &[
                "merge-tree",
                "--write-tree",
                "--name-only",
                main_ref,
                "HEAD",
            ],
            BUDGET,
        )
        .and_then(|(code, out)| parse_merge_tree(code, &out))
    };
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
    // once, and all of them at the same time. Panes share directories far more
    // often than not, and the ones they do not share know nothing about each
    // other.
    let mut distinct_dirs: Vec<PathBuf> = Vec::new();
    for dir in input
        .mine
        .iter()
        .map(|w| &w.cwd)
        .chain(input.others.iter().map(|(_, w)| &w.cwd))
    {
        if !distinct_dirs.contains(dir) {
            distinct_dirs.push(dir.clone());
        }
    }
    let root_of: HashMap<PathBuf, Option<PathBuf>> = parallel_map(distinct_dirs, |dir| {
        let root = if dir.is_dir() {
            git(&dir, &["rev-parse", "--show-toplevel"], BUDGET)
                .map(|s| PathBuf::from(s.trim()))
                .filter(|p| !p.as_os_str().is_empty())
        } else {
            None
        };
        (dir, root)
    })
    .into_iter()
    .collect();

    // Identity per root, once.
    let mut distinct_roots: Vec<PathBuf> = Vec::new();
    for root in root_of.values().flatten() {
        if !distinct_roots.contains(root) {
            distinct_roots.push(root.clone());
        }
    }
    let repo_of_root: HashMap<PathBuf, String> = parallel_map(distinct_roots, |root| {
        let common = git(&root, &["rev-parse", "--git-common-dir"], BUDGET)
            .map(|s| {
                let p = PathBuf::from(s.trim());
                if p.is_absolute() {
                    p
                } else {
                    root.join(p)
                }
            })
            .unwrap_or_else(|| root.join(".git"));
        let origin = git(&root, &["remote", "get-url", "origin"], BUDGET);
        let id = repo_identity(origin.as_deref(), &common);
        (root, id)
    })
    .into_iter()
    .collect();

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
                        upstream: None,
                        unpushed: None,
                        touched: None,
                        merge: None,
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

    // Measure each checkout on its own, all at once. Each keeps the porcelain
    // it read so the pass against main does not ask for it again.
    let measured = parallel_map(std::mem::take(&mut checkouts), |mut c| {
        let porcelain = measure_checkout(&mut c);
        (c, porcelain)
    });
    let (mut checkouts, mut porcelains): (Vec<Checkout>, Vec<Option<String>>) =
        measured.into_iter().unzip();

    // Repository-level facts, measured on the first checkout of each. The
    // worktree listing is kept: the idle sweep below wants the primary's, and
    // it is the same listing from the same root.
    let repo_roots: Vec<(String, PathBuf)> = repo_ids
        .iter()
        .filter_map(|id| {
            checkouts
                .iter()
                .find(|c| c.repo == *id)
                .map(|c| (id.clone(), c.root.clone()))
        })
        .collect();
    let scanned = parallel_map(repo_roots, |(id, root)| {
        let main_ref = ["origin/main", "main", "origin/master", "master"]
            .into_iter()
            .find(|r| git(&root, &["rev-parse", "--verify", "--quiet", r], BUDGET).is_some())
            .map(str::to_string);
        let listing = git(&root, &["worktree", "list", "--porcelain"], BUDGET);
        let worktrees_on_disk = listing
            .as_deref()
            .map(|s| s.lines().filter(|l| l.starts_with("worktree ")).count() as u32);
        // Stashes live in the common dir, so any checkout of the repository
        // sees the same list — measured here rather than in a pass of its own.
        let stashes = git(&root, &["stash", "list"], BUDGET)
            .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count() as u32);
        let pulse = git(
            &root,
            &["log", "--all", "--since=1.hour", "--format=%ct"],
            BUDGET,
        )
        .map(|s| {
            let stamps: Vec<u64> = s.lines().filter_map(|l| l.trim().parse().ok()).collect();
            bucket_pulse(&stamps, now_unix)
        });
        (
            RepoFacts {
                name: repo_name(&id, &root),
                id,
                main_ref,
                worktrees_on_disk,
                pulse,
                stashes,
            },
            listing,
        )
    });
    let mut worktree_listings: HashMap<String, Option<String>> = HashMap::new();
    let mut repos: Vec<RepoFacts> = Vec::with_capacity(scanned.len());
    for (facts, listing) in scanned {
        worktree_listings.insert(facts.id.clone(), listing);
        repos.push(facts);
    }

    // Against main — needs the repository's main line, so it comes after.
    let main_of: HashMap<&str, String> = repos
        .iter()
        .filter_map(|r| r.main_ref.clone().map(|m| (r.id.as_str(), m)))
        .collect();
    let paired: Vec<(Checkout, Option<String>)> = std::mem::take(&mut checkouts)
        .into_iter()
        .zip(std::mem::take(&mut porcelains))
        .collect();
    let mut checkouts: Vec<Checkout> = parallel_map(paired, |(mut c, porcelain)| {
        if let Some(main_ref) = main_of.get(c.repo.as_str()) {
            measure_against_main(&mut c, main_ref, porcelain.as_deref());
        }
        c
    });

    // Idle worktrees of the primary repository: on disk, nobody in them —
    // mine or anyone else's. Measured like the rest, because a worktree
    // nothing is looking at is exactly where uncommitted work gets lost.
    // Capped, since a repository can have forty and the rail is not a census
    // of them.
    let mut idle: Vec<Checkout> = Vec::new();
    if let Some(p) = &primary {
        let occupied: Vec<&PathBuf> = root_of.values().flatten().collect();
        // The listing the repository pass already read, rather than the same
        // `git worktree list` a second time on the same root.
        let listing = worktree_listings.get(p).cloned().flatten();
        let main_ref = main_of.get(p.as_str()).cloned();
        let roots: Vec<PathBuf> = listing
            .as_deref()
            .unwrap_or("")
            .lines()
            .filter_map(|l| l.strip_prefix("worktree "))
            .map(|p| PathBuf::from(p.trim()))
            .filter(|root| !occupied.contains(&root) && root.is_dir())
            .take(12)
            .collect();
        idle = parallel_map(roots, |root| {
            let mut c = Checkout {
                root,
                repo: p.clone(),
                branch: None,
                head: None,
                dirty: None,
                untracked: None,
                delta: None,
                ahead_behind: None,
                last_commit: None,
                writers: Vec::new(),
                upstream: None,
                unpushed: None,
                touched: None,
                merge: None,
            };
            let porcelain = measure_checkout(&mut c);
            if let Some(m) = &main_ref {
                measure_against_main(&mut c, m, porcelain.as_deref());
            }
            c
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
        idle,
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

    /// Nothing here is worth interrupting for: one repository, every
    /// checkout isolated, nothing uncommitted anywhere, no drift, nothing
    /// that would conflict or converge, no idle worktree carrying work. The
    /// badge says `N WT ✓` and the ticker says nothing — silence means
    /// healthy, and the rail earns trust by not screaming continuously.
    pub fn is_calm(&self) -> bool {
        self.primary().is_some()
            && self.repos.len() == 1
            && self.shared_count() == 0
            && self.foreign.is_empty()
            && self.visitors.is_empty()
            && self.no_git.is_empty()
            && self.dirty_checkouts() == Some(0)
            && self.conflicts().is_empty()
            && self.collisions().is_empty()
            && !self
                .idle
                .iter()
                .any(|c| c.is_dirty() == Some(true) || c.ahead_behind.is_some_and(|(a, _)| a > 0))
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
        let conflicts = self.conflicts().len();
        if conflicts > 0 {
            out.push(Segment {
                text: format!("\u{2717} {conflicts} CONFLICT"),
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

        // 0 · a calm project says nothing but its heartbeat, and that only
        // while it is beating. Parker's mockup of the healthy case was the
        // badge alone — "huge breathing room" — and the first cut still
        // cycled a branches frame and a sentence through it.
        if self.is_calm() {
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
            return out;
        }

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

        // 5b · what it would take to land everything, and what is converging
        let landing = self.landing();
        let count = |k: LandingKind| landing.iter().filter(|i| i.kind == k).count();
        let merges = count(LandingKind::Merge)
            + count(LandingKind::Conflict)
            + count(LandingKind::Unchecked);
        let conflicts = count(LandingKind::Conflict);
        let commits = count(LandingKind::Commit);
        let pushes = count(LandingKind::Push) + count(LandingKind::NoUpstream);
        if merges + commits + pushes > 0 {
            let mut parts = Vec::new();
            if merges > 0 {
                parts.push(plural(merges as u32, "merge", "merges"));
            }
            if commits > 0 {
                parts.push(format!("{commits} uncommitted"));
            }
            if pushes > 0 {
                parts.push(format!("{pushes} unpushed"));
            }
            if conflicts > 0 {
                parts.push(format!(
                    "{} would conflict",
                    plural(conflicts as u32, "line", "lines")
                ));
            }
            out.push(Frame {
                kind: FrameKind::Landing,
                text: format!("to land everything: {}", parts.join(" \u{00b7} ")),
                tone: if conflicts > 0 {
                    Tone::Warn
                } else {
                    Tone::Plain
                },
            });
        }
        for col in self.collisions() {
            out.push(Frame {
                kind: FrameKind::Collision,
                text: format!(
                    "\u{25c7} {} \u{00b7} {} converge on {}",
                    col.a,
                    col.b,
                    plural(col.files.len() as u32, "file", "files")
                ),
                tone: Tone::Warn,
            });
        }
        let idle_dirty = self
            .idle
            .iter()
            .filter(|c| c.is_dirty() == Some(true) || c.ahead_behind.is_some_and(|(a, _)| a > 0))
            .count();
        if idle_dirty > 0 {
            out.push(Frame {
                kind: FrameKind::Idle,
                text: format!(
                    "{} carry work nobody is looking at",
                    plural(idle_dirty as u32, "idle worktree", "idle worktrees")
                ),
                tone: Tone::Warn,
            });
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
            // One line of work that IS the shared one. Without this arm the
            // last case catches it and the bar says "1 lines of work, one
            // shared", which it did on screen — and counting the sharers is
            // noise when there is only one line for them to share.
            (1, _) => "one line of work, shared".into(),
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
        let conflicts = self.conflicts().len();
        if conflicts > 0 {
            parts.push(format!("{} would conflict with main", number(conflicts)));
        }
        let converging = self.collisions().len();
        if converging > 0 {
            parts.push(format!(
                "{} converging",
                plural(converging as u32, "pair", "pairs")
            ));
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

impl ProjectState {
    /// The lines a landing is about: the primary repository's checkouts that
    /// panes are in, then its idle worktrees — main itself excluded, since
    /// main is what everything lands ON.
    fn landing_lines(&self) -> Vec<&Checkout> {
        let main_name = self.main_name();
        self.primary_checkouts()
            .into_iter()
            .chain(self.idle.iter())
            .filter(|c| Some(c.line()) != main_name)
            .collect()
    }

    /// The main line's short name — `main` for `origin/main`.
    pub fn main_name(&self) -> Option<String> {
        self.primary()
            .and_then(|r| r.main_ref.as_deref())
            .map(|m| m.rsplit('/').next().unwrap_or(m).to_string())
    }

    /// Lines that change the same files. Two checkouts on one branch are one
    /// line and do not collide with themselves; a line whose files were not
    /// measured collides with nothing, which is not the same as being safe —
    /// the landing list carries that.
    pub fn collisions(&self) -> Vec<Collision> {
        let lines = self.landing_lines();
        let mut out = Vec::new();
        for (i, a) in lines.iter().enumerate() {
            for b in lines.iter().skip(i + 1) {
                if a.line() == b.line() {
                    continue;
                }
                let (Some(fa), Some(fb)) = (&a.touched, &b.touched) else {
                    continue;
                };
                let files: Vec<String> = fa.iter().filter(|f| fb.contains(f)).cloned().collect();
                if !files.is_empty() {
                    out.push(Collision {
                        a: a.line(),
                        b: b.line(),
                        files,
                    });
                }
            }
        }
        out
    }

    /// Lines whose merge into main would conflict today.
    pub fn conflicts(&self) -> Vec<&Checkout> {
        self.landing_lines()
            .into_iter()
            .filter(|c| matches!(c.merge, Some(Merge::Conflicts(_))))
            .collect()
    }

    /// What must become true for everything in flight to be on main, in the
    /// order a person would do it: each line's own housekeeping first
    /// (commit, push), then its merge, then the repository's loose ends, then
    /// the collisions as advisories. Every item names its line, so a
    /// coordinating agent can go straight to the checkout.
    pub fn landing(&self) -> Vec<LandingItem> {
        let mut out = Vec::new();
        let Some(primary) = self.primary() else {
            return out;
        };
        let main_name = self.main_name();
        let item = |c: &Checkout, kind: LandingKind, text: String| LandingItem {
            line: c.line(),
            idle: c.idle(),
            kind,
            text,
        };
        for c in self.landing_lines() {
            let where_ = if c.idle() {
                format!("{} (idle worktree)", c.line())
            } else {
                c.line()
            };
            if let Some(d) = c.dirty.filter(|d| *d > 0) {
                out.push(item(
                    c,
                    LandingKind::Commit,
                    format!(
                        "commit or stash {} on {where_}",
                        plural(d, "uncommitted file", "uncommitted files")
                    ),
                ));
            }
            match (&c.upstream, c.unpushed, c.ahead_behind) {
                (Some(_), Some(n), _) if n > 0 => out.push(item(
                    c,
                    LandingKind::Push,
                    format!("push {} on {where_}", plural(n, "commit", "commits")),
                )),
                (None, _, Some((a, _))) if a > 0 => out.push(item(
                    c,
                    LandingKind::NoUpstream,
                    format!(
                        "{where_} is not pushed anywhere ({} only here)",
                        plural(a, "commit", "commits")
                    ),
                )),
                _ => {}
            }
            if let Some((a, _)) = c.ahead_behind.filter(|(a, _)| *a > 0) {
                let m = main_name.as_deref().unwrap_or("main");
                let commits = plural(a, "commit", "commits");
                match &c.merge {
                    Some(Merge::Clean) => out.push(item(
                        c,
                        LandingKind::Merge,
                        format!("merge {where_} into {m} ({commits}) — merges clean"),
                    )),
                    Some(Merge::Conflicts(files)) => out.push(item(
                        c,
                        LandingKind::Conflict,
                        format!(
                            "merge {where_} into {m} ({commits}) — would conflict on {}",
                            files.join(", ")
                        ),
                    )),
                    Some(Merge::Nothing) | None => out.push(item(
                        c,
                        LandingKind::Unchecked,
                        format!("merge {where_} into {m} ({commits}) — merge not checked"),
                    )),
                }
            }
        }
        // main itself: uncommitted work on it, and its distance from origin
        if let Some(c) = self
            .primary_checkouts()
            .into_iter()
            .find(|c| Some(c.line()) == main_name)
        {
            if let Some(d) = c.dirty.filter(|d| *d > 0) {
                out.push(item(
                    c,
                    LandingKind::Commit,
                    format!(
                        "commit or stash {} on {}",
                        plural(d, "uncommitted file", "uncommitted files"),
                        c.line()
                    ),
                ));
            }
            if let Some((a, b)) = c.ahead_behind {
                if b > 0 {
                    out.push(item(
                        c,
                        LandingKind::Pull,
                        format!(
                            "{} is {} behind its origin — pull first",
                            c.line(),
                            plural(b, "commit", "commits")
                        ),
                    ));
                }
                if a > 0 {
                    out.push(item(
                        c,
                        LandingKind::Push,
                        format!("push {} on {}", plural(a, "commit", "commits"), c.line()),
                    ));
                }
            }
        }
        if let Some(n) = primary.stashes.filter(|n| *n > 0) {
            out.push(LandingItem {
                line: primary.name.clone(),
                idle: false,
                kind: LandingKind::Stash,
                text: format!("{} in {}", plural(n, "stash", "stashes"), primary.name),
            });
        }
        for col in self.collisions() {
            out.push(LandingItem {
                line: format!("{} · {}", col.a, col.b),
                idle: false,
                kind: LandingKind::Collision,
                text: format!(
                    "{} and {} both change {}: {}",
                    col.a,
                    col.b,
                    plural(col.files.len() as u32, "file", "files"),
                    col.files.join(", ")
                ),
            });
        }
        out
    }
}
// ---- the report ------------------------------------------------------------

/// A reading, as a program reads it — over MCP as `engineering_state` and on
/// the command line as `ctl rail`. Every `Option` is an `Option` on the wire
/// too: a field that was not measured is `null`, never `0`, because the
/// agent reading this is about to act on it.
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub project: Option<u32>,
    pub name: Option<String>,
    /// Whether this is the project the window is standing in.
    pub active: bool,
    pub read_secs_ago: u64,
    pub took_ms: u128,
    /// The main line's short name, `null` when the repository has none.
    pub main: Option<String>,
    pub badge: Vec<String>,
    pub sentence: Option<String>,
    pub repos: Vec<RepoReport>,
    /// Checkouts panes are in, in tab order.
    pub checkouts: Vec<CheckoutReport>,
    /// Worktrees on disk that no pane is in.
    pub idle: Vec<CheckoutReport>,
    pub no_git: Vec<WriterReport>,
    pub foreign: Vec<ForeignReport>,
    pub visitors: Vec<VisitorReport>,
    pub collisions: Vec<Collision>,
    /// What must become true for everything in flight to be on main.
    pub landing: Vec<LandingReport>,
    /// What changed between recent readings, oldest first.
    pub afterglow: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RepoReport {
    pub id: String,
    pub name: String,
    pub main_ref: Option<String>,
    pub worktrees_on_disk: Option<u32>,
    pub stashes: Option<u32>,
    /// Commits in the last hour, five-minute buckets, oldest first.
    pub pulse: Option<[u32; 12]>,
}

#[derive(Clone, Debug, Serialize)]
pub struct WriterReport {
    pub tab: usize,
    pub tab_name: String,
    pub label: String,
    pub cwd: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CheckoutReport {
    pub root: String,
    pub repo: String,
    /// The branch, or `detached @<sha>`.
    pub line: String,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub idle: bool,
    pub shared: bool,
    pub writers: Vec<WriterReport>,
    pub dirty: Option<u32>,
    pub untracked: Option<u32>,
    pub added: Option<u32>,
    pub removed: Option<u32>,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    pub last_commit: Option<u64>,
    pub upstream: Option<String>,
    pub unpushed: Option<u32>,
    pub touched: Option<Vec<String>>,
    /// `clean`, `conflicts`, `nothing`, or `null` when not checked.
    pub merge: Option<String>,
    pub conflict_files: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ForeignReport {
    pub writer: WriterReport,
    pub repo: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct VisitorReport {
    pub writer: WriterReport,
    pub filed_under: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LandingReport {
    pub line: String,
    pub idle: bool,
    pub kind: String,
    pub text: String,
}

impl Serialize for Collision {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Collision", 3)?;
        st.serialize_field("a", &self.a)?;
        st.serialize_field("b", &self.b)?;
        st.serialize_field("files", &self.files)?;
        st.end()
    }
}

fn writer_report(w: &Writer) -> WriterReport {
    WriterReport {
        tab: w.tab,
        tab_name: w.tab_name.clone(),
        label: w.label.clone(),
        cwd: w.cwd.display().to_string(),
    }
}

fn checkout_report(c: &Checkout) -> CheckoutReport {
    let (merge, conflict_files) = match &c.merge {
        Some(Merge::Clean) => (Some("clean".to_string()), Vec::new()),
        Some(Merge::Nothing) => (Some("nothing".to_string()), Vec::new()),
        Some(Merge::Conflicts(files)) => (Some("conflicts".to_string()), files.clone()),
        None => (None, Vec::new()),
    };
    CheckoutReport {
        root: c.root.display().to_string(),
        repo: c.repo.clone(),
        line: c.line(),
        branch: c.branch.clone(),
        head: c.head.clone(),
        idle: c.idle(),
        shared: c.shared(),
        writers: c.writers.iter().map(writer_report).collect(),
        dirty: c.dirty,
        untracked: c.untracked,
        added: c.delta.map(|d| d.0),
        removed: c.delta.map(|d| d.1),
        ahead: c.ahead_behind.map(|ab| ab.0),
        behind: c.ahead_behind.map(|ab| ab.1),
        last_commit: c.last_commit,
        upstream: c.upstream.clone(),
        unpushed: c.unpushed,
        touched: c.touched.clone(),
        merge,
        conflict_files,
    }
}

impl LandingKind {
    /// The wire spelling, stable for programs that switch on it.
    pub fn as_str(self) -> &'static str {
        match self {
            LandingKind::Commit => "commit",
            LandingKind::Push => "push",
            LandingKind::NoUpstream => "no_upstream",
            LandingKind::Merge => "merge",
            LandingKind::Conflict => "conflict",
            LandingKind::Unchecked => "unchecked",
            LandingKind::Pull => "pull",
            LandingKind::Stash => "stash",
            LandingKind::Collision => "collision",
        }
    }
}

impl ProjectState {
    /// The reading as a program reads it. `afterglow` is the window's, since
    /// a single reading cannot know what changed.
    pub fn report(&self, active: bool, afterglow: &[String]) -> Report {
        Report {
            project: self.project,
            name: self.name.clone(),
            active,
            read_secs_ago: self.scanned_at.elapsed().as_secs(),
            took_ms: self.took.as_millis(),
            main: self.main_name(),
            badge: self.badge().into_iter().map(|s| s.text).collect(),
            sentence: self.sentence(),
            repos: self
                .repos
                .iter()
                .map(|r| RepoReport {
                    id: r.id.clone(),
                    name: r.name.clone(),
                    main_ref: r.main_ref.clone(),
                    worktrees_on_disk: r.worktrees_on_disk,
                    stashes: r.stashes,
                    pulse: r.pulse,
                })
                .collect(),
            checkouts: self.checkouts.iter().map(checkout_report).collect(),
            idle: self.idle.iter().map(checkout_report).collect(),
            no_git: self.no_git.iter().map(writer_report).collect(),
            foreign: self
                .foreign
                .iter()
                .map(|f| ForeignReport {
                    writer: writer_report(&f.writer),
                    repo: f.repo_name.clone(),
                })
                .collect(),
            visitors: self
                .visitors
                .iter()
                .map(|v| VisitorReport {
                    writer: writer_report(&v.writer),
                    filed_under: v.filed_under.clone(),
                })
                .collect(),
            collisions: self.collisions(),
            landing: self
                .landing()
                .into_iter()
                .map(|i| LandingReport {
                    line: i.line,
                    idle: i.idle,
                    kind: i.kind.as_str().into(),
                    text: i.text,
                })
                .collect(),
            afterglow: afterglow.to_vec(),
        }
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
    fn one_line_that_is_the_shared_one_is_not_one_lines() {
        // Seen on the bar: "1 lines of work, one shared". Every other count in
        // this sentence spells its singular, and counting the sharers adds
        // nothing when there is only one line for them to share.
        let rig = Rig::new("oneshared");
        let st = scan(&ScanInput {
            project: Some(1),
            name: Some("REPO".into()),
            mine: vec![w(0, "CLAUDE", &rig.main), w(1, "CODEX", &rig.main)],
            others: vec![],
        });
        let s = st.sentence().unwrap();
        assert!(s.starts_with("one line of work, shared"), "{s}");
        assert!(!s.contains("1 lines"), "{s}");
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
        // silence means healthy: a calm project shows its badge and, while
        // it is beating, its heartbeat — nothing else
        assert!(
            st.is_calm(),
            "two isolated clean checkouts of one repository are calm"
        );
        assert!(
            kinds.iter().all(|k| *k == FrameKind::Pulse),
            "a calm project says nothing but its heartbeat: {kinds:?}"
        );
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
                stashes: None,
            }],
            checkouts,
            idle: vec![],
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

    /// An unmeasured checkout rooted at a real directory, for the tests that
    /// go and ask git rather than hand-building an answer.
    fn co_at(root: &Path) -> Checkout {
        Checkout {
            root: root.to_path_buf(),
            repo: "o/r".into(),
            branch: None,
            head: None,
            dirty: None,
            untracked: None,
            delta: None,
            ahead_behind: None,
            last_commit: None,
            writers: Vec::new(),
            upstream: None,
            unpushed: None,
            touched: None,
            merge: None,
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
            upstream: None,
            unpushed: None,
            touched: None,
            merge: None,
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

    fn commit_file(dir: &Path, name: &str, content: &str, msg: &str) {
        std::fs::write(dir.join(name), content).unwrap();
        sh(dir, &["git", "add", name]);
        sh(dir, &["git", "commit", "-q", "-m", msg]);
    }

    #[test]
    fn the_landing_list_is_what_must_become_true() {
        let rig = Rig::new("landing");
        // feature changes a.txt and commits; main changes the same line
        // differently → a conflict, and the same file → a collision with a
        // third line in an idle worktree that nobody is in
        commit_file(&rig.wt, "a.txt", "feature\n", "feature edit");
        commit_file(&rig.main, "a.txt", "main\n", "main edit");
        // main's edit has to be on ORIGIN's main: that is the line everything
        // is measured against and merged into
        sh(&rig.main, &["git", "push", "-q", "origin", "main"]);
        let idle = rig.base.join("repo-idle");
        sh(
            &rig.main,
            &[
                "git",
                "worktree",
                "add",
                "-q",
                "-b",
                "idle-line",
                idle.to_str().unwrap(),
            ],
        );
        std::fs::write(idle.join("a.txt"), "idle\n").unwrap(); // uncommitted, in an idle worktree
                                                               // a stash in the repository
        std::fs::write(rig.main.join("a.txt"), "stashed\n").unwrap();
        sh(&rig.main, &["git", "stash", "-q"]);
        // feature has an upstream one commit behind it
        sh(
            &rig.main,
            &["git", "push", "-q", "origin", "feature:feature"],
        );
        sh(
            &rig.wt,
            &["git", "branch", "-q", "--set-upstream-to=origin/feature"],
        );
        commit_file(&rig.wt, "b.txt", "more\n", "unpushed");

        let st = scan(&ScanInput {
            project: Some(1),
            name: Some("REPO".into()),
            mine: vec![w(0, "CLAUDE", &rig.main), w(1, "CODEX", &rig.wt)],
            others: vec![],
        });
        let feature = st.checkouts.iter().find(|c| c.root == rig.wt).unwrap();
        assert_eq!(feature.upstream.as_deref(), Some("origin/feature"));
        assert_eq!(feature.unpushed, Some(1));
        assert_eq!(
            feature.ahead_behind,
            Some((2, 1)),
            "two ahead of main, one behind"
        );
        assert_eq!(
            feature.touched.as_deref(),
            Some(&["a.txt".to_string(), "b.txt".to_string()][..])
        );
        assert_eq!(feature.merge, Some(Merge::Conflicts(vec!["a.txt".into()])));
        let main = st.checkouts.iter().find(|c| c.root == rig.main).unwrap();
        assert_eq!(
            main.merge,
            Some(Merge::Nothing),
            "main is what things land on"
        );
        assert_eq!(
            main.touched.as_deref(),
            Some(&[][..]),
            "main has nothing uncommitted after the stash"
        );
        assert_eq!(st.primary().unwrap().stashes, Some(1));
        // the idle worktree was found, measured, and is dirty
        assert_eq!(st.idle.len(), 1);
        let idle_c = &st.idle[0];
        assert!(idle_c.idle());
        assert_eq!(idle_c.branch.as_deref(), Some("idle-line"));
        assert_eq!(idle_c.dirty, Some(1));
        assert_eq!(idle_c.touched.as_deref(), Some(&["a.txt".to_string()][..]));
        // the collision: feature and idle-line both change a.txt
        let cols = st.collisions();
        assert_eq!(cols.len(), 1);
        assert_eq!(
            (cols[0].a.as_str(), cols[0].b.as_str()),
            ("feature", "idle-line")
        );
        assert_eq!(cols[0].files, vec!["a.txt"]);
        assert_eq!(st.conflicts().len(), 1);
        // the list, in the order a person would do it
        let texts: Vec<String> = st.landing().into_iter().map(|i| i.text).collect();
        assert_eq!(
            texts,
            vec![
                "push 1 commit on feature",
                "merge feature into main (2 commits) — would conflict on a.txt",
                "commit or stash 1 uncommitted file on idle-line (idle worktree)",
                "1 stash in repo",
                "feature and idle-line both change 1 file: a.txt",
            ]
        );
        let badge: Vec<String> = st.badge().into_iter().map(|s| s.text).collect();
        assert_eq!(badge, vec!["2 WT \u{2713}", "\u{2717} 1 CONFLICT"]);
        let kinds: Vec<FrameKind> = st.frames().iter().map(|f| f.kind).collect();
        assert!(kinds.contains(&FrameKind::Landing));
        assert!(kinds.contains(&FrameKind::Collision));
        assert!(kinds.contains(&FrameKind::Idle));
        let landing = st
            .frames()
            .into_iter()
            .find(|f| f.kind == FrameKind::Landing)
            .unwrap();
        assert_eq!(landing.text, "to land everything: 1 merge \u{00b7} 1 uncommitted \u{00b7} 1 unpushed \u{00b7} 1 line would conflict");
        assert_eq!(landing.tone, Tone::Warn);
        let s = st.sentence().unwrap();
        assert!(s.contains("one would conflict with main"), "{s}");
        assert!(s.contains("1 pair converging"), "{s}");
    }

    #[test]
    fn merge_tree_answers_are_read_by_exit_code() {
        assert_eq!(parse_merge_tree(0, "abc123\n"), Some(Merge::Clean));
        assert_eq!(
            parse_merge_tree(1, "abc123\na.txt\nsrc/b.rs\n\nCONFLICT (content): …\n"),
            Some(Merge::Conflicts(vec!["a.txt".into(), "src/b.rs".into()]))
        );
        assert_eq!(
            parse_merge_tree(128, "fatal: …"),
            None,
            "a failure is not an answer"
        );
    }

    /// The runner used to read stdout only after `try_wait` said the child had
    /// exited — and a child filling the 64 KiB a pipe holds is blocked in
    /// `write` and will never exit. A checkout with a couple of thousand
    /// untracked files (an un-ignored build directory is enough) therefore
    /// burned the whole eight-second budget per call and came back
    /// **unmeasured** on a number git had been perfectly willing to give.
    ///
    /// Measured on the merged code: 176 000 bytes of porcelain, 8082 ms,
    /// `dirty: None, untracked: None`.
    #[test]
    fn a_wordy_git_is_still_measured() {
        let r = Rig::new("wordy");
        // Untracked files at the ROOT, so each gets a porcelain line of its
        // own: `--untracked-files=normal` collapses an untracked *directory*
        // to one line and would print ten bytes, proving nothing. 40 bytes a
        // line, so about 80 KiB — comfortably past what the pipe holds.
        for i in 0..2000 {
            std::fs::write(
                r.main
                    .join(format!("a-file-with-a-longish-name-{i:05}.txt")),
                "x",
            )
            .unwrap();
        }
        let printed = git(
            &r.main,
            &["status", "--porcelain", "--untracked-files=normal"],
            BUDGET,
        )
        .map(|s| s.len())
        .unwrap_or(0);
        assert!(
            printed > 65_536,
            "the fixture has to out-talk the pipe to prove anything; printed {printed} bytes"
        );

        let mut c = co_at(&r.main);
        let began = Instant::now();
        measure_checkout(&mut c);
        assert_eq!(
            c.untracked,
            Some(2000),
            "every untracked file counted, not thrown away at the budget"
        );
        assert!(
            began.elapsed() < BUDGET,
            "the runner deadlocked and was killed at the budget: {:?}",
            began.elapsed()
        );
    }

    /// The pass against main used to run `git status` a second time with
    /// `--untracked-files=no`. That output is the listing the first pass
    /// already read with the `??` lines dropped — verified here on the shape
    /// that is easiest to get wrong, a staged rename beside an untracked file.
    #[test]
    fn the_second_status_was_the_first_one_without_its_untracked_lines() {
        let r = Rig::new("onestatus");
        std::fs::write(r.main.join("old name.txt"), "hello\n").unwrap();
        sh(&r.main, &["git", "add", "-A"]);
        sh(&r.main, &["git", "commit", "-q", "-m", "named"]);
        std::fs::rename(r.main.join("old name.txt"), r.main.join("new name.txt")).unwrap();
        std::fs::write(r.main.join("untracked.txt"), "u").unwrap();
        sh(&r.main, &["git", "add", "-A"]);

        let normal = git(
            &r.main,
            &["status", "--porcelain", "--untracked-files=normal"],
            BUDGET,
        )
        .expect("porcelain");
        let derived: Vec<&str> = normal.lines().filter(|l| !l.starts_with("??")).collect();
        let asked = git(
            &r.main,
            &["status", "--porcelain", "--untracked-files=no"],
            BUDGET,
        )
        .expect("porcelain");
        let asked: Vec<&str> = asked.lines().collect();
        assert_eq!(derived, asked, "the second call bought nothing");
        assert!(
            derived.iter().any(|l| l.starts_with('R')),
            "a rename has to be in the fixture or it proves too little: {derived:?}"
        );
    }

    /// The pool has to give every item back, in the order it took them —
    /// a dropped checkout is a pane that vanishes from the table, and a
    /// reordered one breaks the tab ordering the table reads by.
    #[test]
    fn the_pool_returns_every_item_in_order() {
        let items: Vec<usize> = (0..200).collect();
        let out = parallel_map(items.clone(), |i| {
            // uneven work, so a chunked split would finish out of order
            std::thread::sleep(Duration::from_micros((i % 7) as u64 * 50));
            i * 2
        });
        assert_eq!(out, items.iter().map(|i| i * 2).collect::<Vec<_>>());
        assert!(parallel_map(Vec::<usize>::new(), |i| i).is_empty());
        assert_eq!(parallel_map(vec![9usize], |i| i + 1), vec![10]);
    }

    #[test]
    fn the_report_is_json_with_null_where_nothing_was_measured() {
        // two commits ahead of main with no upstream and no merge check: the
        // three landing kinds a hand-built checkout can carry
        let mut c = co("/w/rail", "rail", 2, 100, 1);
        c.ahead_behind = Some((2, 1));
        let st = reading(vec![c], 1, 0);
        let json =
            serde_json::to_value(st.report(true, &["a commit landed on rail".into()])).unwrap();
        assert_eq!(json["active"], true);
        assert_eq!(json["main"], "main");
        assert_eq!(json["checkouts"][0]["line"], "rail");
        assert_eq!(json["checkouts"][0]["dirty"], 2);
        assert_eq!(json["checkouts"][0]["behind"], 1);
        assert_eq!(
            json["checkouts"][0]["upstream"],
            serde_json::Value::Null,
            "no upstream is null, not a string"
        );
        assert_eq!(
            json["checkouts"][0]["merge"],
            serde_json::Value::Null,
            "unchecked is null, not clean"
        );
        assert_eq!(json["repos"][0]["stashes"], serde_json::Value::Null);
        assert_eq!(json["foreign"][0]["repo"], "elsewhere");
        assert_eq!(json["afterglow"][0], "a commit landed on rail");
        let kinds: Vec<&str> = json["landing"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, vec!["commit", "no_upstream", "unchecked"]);
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
