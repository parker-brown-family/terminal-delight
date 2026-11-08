//! The right-bar flyout, drawn: the chip under the attention pill, the panel
//! docked between the screen and the spine, its tab strip, the `+` page, and
//! the three kinds of tab. The model and every parser are in `flyout.rs`; the
//! plan and the calls behind it are in docs/plans/right-flyout.
//!
//! **Docked, never over.** The panel is a flex sibling in the stage row, like
//! the left bar and the docked attention queue: opening it narrows the
//! terminals and covers none of them, so the conversation stays usable beside
//! the evidence it produced. It wears the docked queue's frame (a two-pixel
//! accent edge, an opaque fill, the floating shadows), the house recipe for a
//! flat panel beside bent glass.
//!
//! **The tabs belong to the task.** Each left-bar task keeps its own
//! [`flyout::Side`], so switching tasks switches what the flyout shows: the
//! diff and the pull request beside the conversation that made them. Whether
//! the panel is open, and how wide, is the window's.
//!
//! **Reads happen off the UI thread, and only for what is shown.** git and gh
//! run through `engstate::run_bounded`, in their own process group with a
//! deadline. The tab on screen is read again every few seconds
//! ([`CHANGES_EVERY`], [`FILE_EVERY`], [`PULL_EVERY`]) by a one-second poll;
//! nothing off screen is read at all, which is what keeps twenty panes cheap.
//!
//! **Typing never reaches a terminal while the flyout holds the keyboard.**
//! A click on the panel focuses the workspace, the way the composer does, and
//! `Workspace::on_key` hands keys here first. Esc gives the keyboard back to
//! the pane.

use super::*;
use crate::flyout::{
    self, Base, CheckState, FileDiff, FileStat, LineKind, Mark, PullRow, PullState, Status, Target,
    Tok,
};
use gpui::{
    uniform_list, FontStyle, FontWeight, HighlightStyle, ScrollStrategy, StyledText,
    UniformListScrollHandle,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// The narrowest the panel is dragged to, and the widest.
pub(crate) const FLYOUT_MIN: f32 = 320.;
pub(crate) const FLYOUT_MAX: f32 = 1600.;
/// Wide enough for a diff line and both its line numbers at chrome size.
pub(crate) const FLYOUT_DEFAULT: f32 = 560.;
/// What the terminals keep however wide the panel is dragged.
const SCREEN_KEEPS: f32 = 380.;

/// How often the tab on screen is read again.
const CHANGES_EVERY: Duration = Duration::from_secs(3);
const FILE_EVERY: Duration = Duration::from_secs(2);
const PULL_EVERY: Duration = Duration::from_secs(60);
const SUGGEST_EVERY: Duration = Duration::from_secs(30);
/// The longest one git or gh call may take.
const GIT_BUDGET: Duration = Duration::from_secs(10);
const GH_BUDGET: Duration = Duration::from_secs(30);
/// The most of a file read for its tab.
const FILE_CAP: u64 = 8 * 1024 * 1024;
/// The most a diff may print.
const PATCH_CAP: usize = 32 * 1024 * 1024;
/// Lines of one file's diff drawn before "show more".
const PATCH_ROWS: usize = 400;
/// Untracked files whose lines are counted on each read; past it, unknown.
const UNTRACKED_COUNTED: usize = 300;
/// Bytes of an untracked file read to count its lines.
const UNTRACKED_READ: u64 = 2 * 1024 * 1024;
/// Files the `+` page's finder keeps from `git ls-files`.
const FINDER_FILES: usize = 30_000;
/// Rows the finder lists.
const FINDER_ROWS: usize = 40;
/// git's empty tree: what a repository with no commits is compared against.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// A reading that can be absent, in flight, failed or had, kept apart so a
/// repository with no changes and one not read yet never draw the same.
pub(crate) struct Reading<T> {
    got: Option<T>,
    error: Option<String>,
    busy: bool,
    at: Option<Instant>,
    task: Option<gpui::Task<()>>,
}

impl<T> Default for Reading<T> {
    fn default() -> Self {
        Self {
            got: None,
            error: None,
            busy: false,
            at: None,
            task: None,
        }
    }
}

impl<T> Reading<T> {
    fn due(&self, every: Duration) -> bool {
        !self.busy && self.at.is_none_or(|t| t.elapsed() >= every)
    }

    /// An answer arrived. A failure keeps the last good answer beside it, so
    /// a moment's trouble does not blank a list a person was reading.
    fn land(&mut self, r: Result<T, String>) {
        self.busy = false;
        self.at = Some(Instant::now());
        match r {
            Ok(v) => {
                self.got = Some(v);
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }
}

/// A repository's changed files, as last read.
pub(crate) struct ChangeList {
    files: Vec<FileStat>,
    /// The branch checked out; `None` when HEAD is detached or unborn.
    branch: Option<String>,
    /// What the diff is against, as a person reads it: `origin/main`, `HEAD`.
    against: String,
    /// The revision the diff is taken against.
    rev: String,
    /// Each changed file's size and modified time, so a file edited without
    /// its counts moving still has its diff read again.
    stamps: HashMap<String, u64>,
}

#[derive(Default)]
pub(crate) struct ChangesLive {
    list: Reading<ChangeList>,
    patches: HashMap<String, Reading<FileDiff>>,
    open: HashSet<String>,
    /// Files drawn past [`PATCH_ROWS`].
    full: HashSet<String>,
    cursor: usize,
    scroll: ScrollHandle,
}

pub(crate) struct FileText {
    lines: Arc<Vec<String>>,
    /// Whether each line starts inside a block comment.
    starts: Arc<Vec<bool>>,
    syntax: Option<&'static flyout::Syntax>,
    /// The file was longer than [`FILE_CAP`].
    cut: bool,
    binary: bool,
    /// The file is not tracked by git: every line is new.
    untracked: bool,
}

pub(crate) struct FileLive {
    text: Reading<FileText>,
    marks: Rc<BTreeMap<u32, Mark>>,
    scroll: UniformListScrollHandle,
    /// The line asked for, until the text is there to scroll to it.
    jump: Option<u32>,
    /// The line last asked for, lit until another is.
    lit: Option<u32>,
    stamp: Option<(SystemTime, u64)>,
}

impl Default for FileLive {
    fn default() -> Self {
        Self {
            text: Reading::default(),
            marks: Rc::new(BTreeMap::new()),
            scroll: UniformListScrollHandle::new(),
            jump: None,
            lit: None,
            stamp: None,
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub(crate) enum PullFace {
    #[default]
    Overview,
    Files,
    Conversation,
}

#[derive(Default)]
pub(crate) struct PullLive {
    pull: Reading<flyout::Pull>,
    diff: Reading<Vec<FileDiff>>,
    face: PullFace,
    open: HashSet<String>,
    full: HashSet<String>,
    cursor: usize,
    scroll: ScrollHandle,
}

/// What one target has read. Boxed: the three differ in size by hundreds of
/// bytes, and a map of them should not pay the largest for every entry.
pub(crate) enum Live {
    Changes(Box<ChangesLive>),
    File(Box<FileLive>),
    Pull(Box<PullLive>),
}

impl Live {
    fn for_target(t: &Target) -> Live {
        match t {
            Target::Changes { .. } => Live::Changes(Box::default()),
            Target::File { .. } => Live::File(Box::default()),
            Target::Pull { .. } => Live::Pull(Box::default()),
        }
    }
}

/// What the `+` page offers for one repository.
pub(crate) struct Suggest {
    slug: Option<(String, String)>,
    branch: Option<String>,
    /// Changed and untracked paths, first in the finder.
    changed: Vec<String>,
    files: Vec<String>,
    branch_pull: Option<PullRow>,
    pulls: Vec<PullRow>,
    /// Why there is no list of pull requests, when there is none.
    pulls_note: Option<String>,
}

/// A row of the `+` page, in the order the keyboard walks them.
#[derive(Clone, Debug, PartialEq)]
enum AddRow {
    Changes(PathBuf),
    Pull {
        owner: String,
        repo: String,
        row: PullRow,
        mine: bool,
    },
    File(PathBuf, String),
}

/// The window's flyout: open or not, how wide, whether it holds the keyboard,
/// and what each target has read so far.
pub(crate) struct Flyout {
    pub(crate) open: bool,
    pub(crate) width: f32,
    /// Holding the keyboard: keys go to the flyout and never to a terminal.
    pub(crate) focused: bool,
    /// The `+` page is up in place of the current tab.
    pub(crate) adding: bool,
    query: String,
    pick: usize,
    pub(crate) resize: Option<(f32, f32)>,
    live: HashMap<Target, Live>,
    suggest: HashMap<PathBuf, Reading<Suggest>>,
    /// The repository of the shown task's pane, when the `+` page last looked.
    here: Option<PathBuf>,
    /// Folders already walked up to their repository.
    roots: RefCell<HashMap<PathBuf, Option<PathBuf>>>,
    /// When the checkout command was last copied, for the button's "Copied".
    copied: Option<Instant>,
}

impl Flyout {
    pub(crate) fn restore(saved: Option<&flyout::SavedFlyout>) -> Flyout {
        Flyout {
            open: saved.is_some_and(|s| s.open),
            width: saved
                .and_then(|s| s.width)
                .unwrap_or(FLYOUT_DEFAULT)
                .clamp(FLYOUT_MIN, FLYOUT_MAX),
            focused: false,
            adding: false,
            query: String::new(),
            pick: 0,
            resize: None,
            live: HashMap::new(),
            suggest: HashMap::new(),
            here: None,
            roots: RefCell::new(HashMap::new()),
            copied: None,
        }
    }

    /// For the state file. The width is written only once it has been dragged
    /// off the default, so a window that never touched it carries no opinion.
    pub(crate) fn saved(&self) -> Option<flyout::SavedFlyout> {
        let dragged = (self.width - FLYOUT_DEFAULT).abs() > 0.5;
        (self.open || dragged).then_some(flyout::SavedFlyout {
            open: self.open,
            width: dragged.then_some(self.width),
        })
    }

    fn root_of(&self, dir: &Path) -> Option<PathBuf> {
        if let Some(hit) = self.roots.borrow().get(dir) {
            return hit.clone();
        }
        let root = git_root(dir);
        self.roots
            .borrow_mut()
            .insert(dir.to_path_buf(), root.clone());
        root
    }
}

/// What a click asked the flyout to show: the target, and the line a
/// `path:line` named.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SideOpen {
    pub target: Target,
    pub line: Option<u32>,
}

// ---------------------------------------------------------------------------
// Reading git, gh and the disk (all off the UI thread)
// ---------------------------------------------------------------------------

/// The repository holding `dir`: the nearest folder above it with a `.git`
/// (a folder, or a worktree's file).
pub(crate) fn git_root(dir: &Path) -> Option<PathBuf> {
    let mut at = Some(dir);
    while let Some(p) = at {
        if p.join(".git").exists() {
            return Some(p.to_path_buf());
        }
        at = p.parent();
    }
    None
}

fn fl() -> &'static crate::lang::Flyout {
    crate::lang::current().strings().flyout
}

fn first_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

/// One git command's stdout, or what it said went wrong.
fn git_text(dir: &Path, args: &[&str]) -> Result<String, String> {
    match crate::engstate::run_bounded(
        "git",
        dir,
        args,
        &[("GIT_OPTIONAL_LOCKS", "0")],
        GIT_BUDGET,
        PATCH_CAP,
        4096,
    ) {
        Ok(r) if r.code == 0 => Ok(String::from_utf8_lossy(&r.out).into_owned()),
        Ok(r) => Err(first_line(&r.err).unwrap_or_else(|| format!("git exited with {}", r.code))),
        Err(e) => Err(e),
    }
}

/// The gh to run: `TD_GH` when set, else whichever `gh` is on PATH.
fn gh_program() -> String {
    std::env::var("TD_GH")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "gh".into())
}

/// One gh command's stdout, run from `dir` so an account chosen by the
/// repository's remote is the one asked; its failure in a person's words.
fn gh(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let env = [
        ("GH_PROMPT_DISABLED", "1"),
        ("GH_NO_UPDATE_NOTIFIER", "1"),
        ("GH_SPINNER_DISABLED", "1"),
        ("NO_COLOR", "1"),
        ("CLICOLOR", "0"),
    ];
    match crate::engstate::run_bounded(&gh_program(), dir, args, &env, GH_BUDGET, PATCH_CAP, 8192) {
        Ok(r) if r.code == 0 => Ok(r.out),
        Ok(r) => {
            let said = String::from_utf8_lossy(&r.err);
            if said.contains("auth login") || said.contains("not logged in") {
                Err(fl().gh_signed_out.into())
            } else {
                Err(first_line(&r.err).unwrap_or_else(|| format!("gh exited with {}", r.code)))
            }
        }
        Err(e) if e.contains("could not start") => Err(fl().gh_missing.into()),
        Err(e) => Err(e),
    }
}

/// The first `cap` bytes of a file, and whether that was all of it.
fn read_head(path: &Path, cap: u64) -> Option<(Vec<u8>, bool)> {
    use std::io::Read;
    let f = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    f.take(cap + 1).read_to_end(&mut bytes).ok()?;
    let whole = bytes.len() as u64 <= cap;
    bytes.truncate(cap as usize);
    Some((bytes, whole))
}

fn stamp_of(path: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    if let Ok(m) = std::fs::metadata(path) {
        m.len().hash(&mut h);
        if let Ok(t) = m.modified() {
            t.hash(&mut h);
        }
    }
    h.finish()
}

/// The branch a pull request from here would go into: `origin/HEAD`, else
/// the usual names.
fn default_ref(repo: &Path) -> Option<String> {
    if let Ok(r) = git_text(repo, &["rev-parse", "--abbrev-ref", "origin/HEAD"]) {
        let r = r.trim();
        if !r.is_empty() && r != "origin/HEAD" {
            return Some(r.to_string());
        }
    }
    ["origin/main", "origin/master", "main", "master"]
        .into_iter()
        .find(|r| {
            git_text(repo, &["rev-parse", "--verify", "--quiet", r])
                .is_ok_and(|o| !o.trim().is_empty())
        })
        .map(str::to_string)
}

fn read_change_list(repo: &Path, base: Base) -> Result<ChangeList, String> {
    if git_root(repo).is_none() {
        return Err(fl().not_repo.into());
    }
    let branch = git_text(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .ok()
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty());
    let head = git_text(repo, &["rev-parse", "--verify", "--quiet", "HEAD"])
        .ok()
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty());
    let (rev, against) = match (base, head) {
        // No commits yet: everything is new.
        (_, None) => (EMPTY_TREE.to_string(), "\u{2205}".to_string()),
        (Base::Uncommitted, Some(h)) => (h, "HEAD".to_string()),
        (Base::Branch, Some(h)) => match default_ref(repo) {
            Some(r) => match git_text(repo, &["merge-base", "HEAD", &r]) {
                Ok(mb) if !mb.trim().is_empty() => (mb.trim().to_string(), r),
                _ => (h, "HEAD".to_string()),
            },
            None => (h, "HEAD".to_string()),
        },
    };
    let numstat = git_text(
        repo,
        &[
            "-c",
            "core.quotepath=off",
            "diff",
            "--numstat",
            "-z",
            "-M",
            &rev,
        ],
    )?;
    let names = git_text(
        repo,
        &[
            "-c",
            "core.quotepath=off",
            "diff",
            "--name-status",
            "-z",
            "-M",
            &rev,
        ],
    )?;
    let others = git_text(
        repo,
        &[
            "-c",
            "core.quotepath=off",
            "ls-files",
            "--others",
            "--exclude-standard",
            "--directory",
            "--no-empty-directory",
            "-z",
        ],
    )?;
    let untracked: Vec<(String, Option<u64>)> = others
        .split('\0')
        .filter(|p| !p.is_empty())
        .enumerate()
        .map(|(i, p)| {
            let lines = if p.ends_with('/') || i >= UNTRACKED_COUNTED {
                None
            } else {
                read_head(&repo.join(p), UNTRACKED_READ)
                    .filter(|(_, whole)| *whole)
                    .and_then(|(b, _)| flyout::count_lines(&b))
            };
            (p.to_string(), lines)
        })
        .collect();
    let files = flyout::changed_files(&numstat, &names, &untracked);
    let stamps = files
        .iter()
        .map(|f| (f.path.clone(), stamp_of(&repo.join(&f.path))))
        .collect();
    Ok(ChangeList {
        files,
        branch,
        against,
        rev,
        stamps,
    })
}

/// A new file shown whole, every line added.
fn added_file(path: &str, bytes: &[u8]) -> FileDiff {
    if flyout::looks_binary(bytes) {
        return FileDiff {
            old_path: None,
            new_path: Some(path.to_string()),
            status: Status::Untracked,
            binary: true,
            hunks: Vec::new(),
        };
    }
    let text = String::from_utf8_lossy(bytes);
    let lines = text
        .lines()
        .enumerate()
        .map(|(i, l)| flyout::DiffLine {
            kind: LineKind::Added,
            old: None,
            new: Some(i as u32 + 1),
            text: l.to_string(),
        })
        .collect();
    FileDiff {
        old_path: None,
        new_path: Some(path.to_string()),
        status: Status::Untracked,
        binary: false,
        hunks: vec![flyout::Hunk {
            section: String::new(),
            old_start: 0,
            new_start: 1,
            lines,
        }],
    }
}

fn read_patch(repo: &Path, rev: &str, f: &FileStat) -> Result<FileDiff, String> {
    if f.status == Status::Untracked {
        let (bytes, _) = read_head(&repo.join(&f.path), FILE_CAP).ok_or(fl().file_gone)?;
        return Ok(added_file(&f.path, &bytes));
    }
    let mut args = vec![
        "-c",
        "core.quotepath=off",
        "diff",
        "--no-color",
        "--no-ext-diff",
        // Explicit, because a person's own git config can print `c/` and
        // `w/` (diff.mnemonicPrefix) or nothing (diff.noprefix) instead.
        "--src-prefix=a/",
        "--dst-prefix=b/",
        "-M",
        "-U3",
        rev,
        "--",
    ];
    if let Some(old) = &f.old_path {
        args.push(old);
    }
    args.push(&f.path);
    let text = git_text(repo, &args)?;
    let mut diffs = flyout::parse_unified(&text);
    let at = diffs.iter().position(|d| d.path() == f.path).unwrap_or(0);
    if diffs.is_empty() {
        return Err(fl().no_changes.into());
    }
    Ok(diffs.swap_remove(at))
}

/// A file's text, and which of its lines changed since the last commit.
/// `Ok(None)` when nothing on disk moved since `prev`.
type FileRead = Option<(FileText, BTreeMap<u32, Mark>, (SystemTime, u64))>;

fn read_file(
    path: &Path,
    prev: Option<(SystemTime, u64)>,
    force: bool,
) -> Result<FileRead, String> {
    let meta = std::fs::metadata(path).map_err(|_| fl().file_gone.to_string())?;
    let stamp = (
        meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        meta.len(),
    );
    if !force && prev == Some(stamp) {
        return Ok(None);
    }
    let (bytes, whole) = read_head(path, FILE_CAP).ok_or(fl().file_gone)?;
    let binary = flyout::looks_binary(&bytes);
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<String> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).replace('\t', "    "))
        .collect();
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    let syntax = flyout::syntax_for(path);
    let starts = match syntax {
        Some(sy) => {
            let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
            flyout::block_states(sy, &refs)
        }
        None => Vec::new(),
    };
    // Which lines changed: against HEAD for a tracked file; a file git does
    // not track is all new.
    let mut untracked = false;
    let marks = match path.parent().and_then(git_root) {
        Some(root) => {
            let rel = path.strip_prefix(&root).unwrap_or(path).to_string_lossy();
            let tracked = git_text(&root, &["ls-files", "--error-unmatch", "--", &rel]).is_ok();
            if tracked {
                git_text(
                    &root,
                    &[
                        "diff",
                        "--no-color",
                        "--no-ext-diff",
                        "--src-prefix=a/",
                        "--dst-prefix=b/",
                        "-U0",
                        "HEAD",
                        "--",
                        &rel,
                    ],
                )
                .map(|d| {
                    flyout::line_marks(
                        &flyout::parse_unified(&d)
                            .into_iter()
                            .flat_map(|f| f.hunks)
                            .collect::<Vec<_>>(),
                    )
                })
                .unwrap_or_default()
            } else {
                untracked = true;
                (1..=lines.len() as u32).map(|n| (n, Mark::Added)).collect()
            }
        }
        None => BTreeMap::new(),
    };
    Ok(Some((
        FileText {
            lines: Arc::new(lines),
            starts: Arc::new(starts),
            syntax,
            cut: !whole,
            binary,
            untracked,
        },
        marks,
        stamp,
    )))
}

fn read_pull(dir: &Path, owner: &str, repo: &str, number: u64) -> Result<flyout::Pull, String> {
    let slug = format!("{owner}/{repo}");
    let n = number.to_string();
    let out = gh(
        dir,
        &[
            "pr",
            "view",
            &n,
            "--repo",
            &slug,
            "--json",
            flyout::PULL_FIELDS,
        ],
    )?;
    flyout::parse_pull(&String::from_utf8_lossy(&out))
}

fn read_pull_diff(
    dir: &Path,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<Vec<FileDiff>, String> {
    let slug = format!("{owner}/{repo}");
    let n = number.to_string();
    let out = gh(
        dir,
        &["pr", "diff", &n, "--repo", &slug, "--color", "never"],
    )?;
    Ok(flyout::parse_unified(&String::from_utf8_lossy(&out)))
}

fn read_suggest(repo: &Path) -> Result<Suggest, String> {
    let branch = git_text(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .ok()
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty());
    let remote = git_text(repo, &["remote", "get-url", "origin"])
        .ok()
        .or_else(|| {
            let first = git_text(repo, &["remote"]).ok()?;
            let name = first.lines().next()?.trim().to_string();
            git_text(repo, &["remote", "get-url", &name]).ok()
        });
    let slug = remote.as_deref().and_then(flyout::github_slug);
    let changed = git_text(
        repo,
        &[
            "-c",
            "core.quotepath=off",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=normal",
        ],
    )
    .map(|o| flyout::status_paths(&o))
    .unwrap_or_default();
    let files = git_text(repo, &["-c", "core.quotepath=off", "ls-files", "-z"])
        .map(|o| {
            o.split('\0')
                .filter(|p| !p.is_empty())
                .take(FINDER_FILES)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let (pulls, branch_pull, pulls_note) = match &slug {
        Some((owner, name)) => {
            let slug = format!("{owner}/{name}");
            let rows = |extra: &[&str]| -> Result<Vec<PullRow>, String> {
                let mut args = vec!["pr", "list", "--repo", &slug];
                args.extend_from_slice(extra);
                args.extend_from_slice(&["--json", flyout::PULL_ROW_FIELDS]);
                flyout::parse_pull_rows(&String::from_utf8_lossy(&gh(repo, &args)?))
            };
            let mine = branch.as_deref().and_then(|b| {
                rows(&["--head", b, "--state", "all", "--limit", "1"])
                    .ok()
                    .and_then(|v| v.into_iter().next())
            });
            match rows(&["--state", "open", "--limit", "30"]) {
                Ok(v) => (v, mine, None),
                Err(e) => (Vec::new(), mine, Some(e)),
            }
        }
        None => (Vec::new(), None, Some(fl().no_github.to_string())),
    };
    Ok(Suggest {
        slug,
        branch,
        changed,
        files,
        branch_pull,
        pulls,
        pulls_note,
    })
}

/// What an Alt+click on `token` would show in the flyout: a GitHub pull
/// request by its address, or a text file on this disk that the floating
/// square does not draw (with the line a `path:line` names). A relative path
/// is tried against `cwd`, then against the repository holding it.
pub(crate) fn side_of(token: &str, cwd: Option<&str>) -> Option<SideOpen> {
    let token = token.trim();
    if let Some((owner, repo, number)) = flyout::pull_of_url(token) {
        return Some(SideOpen {
            target: Target::Pull {
                owner,
                repo,
                number,
            },
            line: None,
        });
    }
    let token = token.strip_prefix("file://").unwrap_or(token);
    if token.contains("://") {
        return None;
    }
    let (path, line) = flyout::split_line_ref(token);
    if path.is_empty() {
        return None;
    }
    let expanded = match path.strip_prefix("~/") {
        Some(rest) => crate::session::home_dir().join(rest),
        None => PathBuf::from(path),
    };
    let candidates: Vec<PathBuf> = if expanded.is_absolute() {
        vec![expanded]
    } else {
        let cwd = PathBuf::from(cwd?);
        let mut c = vec![cwd.join(&expanded)];
        if let Some(root) = git_root(&cwd) {
            c.push(root.join(&expanded));
        }
        c
    };
    let file = candidates
        .into_iter()
        .find(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file()))?;
    if crate::docopen::doc_kind_by_name(&file).is_some() {
        return None;
    }
    let (head, _) = read_head(&file, 8000)?;
    if flyout::looks_binary(&head) {
        return None;
    }
    let file = std::fs::canonicalize(&file).unwrap_or(file);
    Some(SideOpen {
        target: Target::File { path: file },
        line,
    })
}

/// A size in the largest unit that keeps it above one.
fn human_size(bytes: u64) -> String {
    let mut v = bytes as f64;
    for unit in ["B", "KB", "MB", "GB"] {
        if v < 1024. || unit == "GB" {
            return if unit == "B" {
                format!("{bytes} B")
            } else {
                format!("{v:.0} {unit}")
            };
        }
        v /= 1024.;
    }
    format!("{bytes} B")
}

/// An age or a date, the way the inbox says one.
fn when_label(at_ms: Option<i64>, l: crate::lang::Lang) -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    match at_ms {
        None => "\u{2014}".into(),
        Some(t) if now - t < 48 * 3600 * 1000 => {
            crate::attention::age_label_in(l, Some(Duration::from_millis((now - t).max(0) as u64)))
        }
        Some(t) => crate::handover::card_time(t.max(0) as u64, now.max(0) as u64, l)
            .unwrap_or_else(|| "\u{2014}".into()),
    }
}

// ---------------------------------------------------------------------------
// The workspace's side of it
// ---------------------------------------------------------------------------

/// Colours drawn into code: the terminal palette's bright slots, so code in
/// the flyout reads in the same colours as code in the terminal beside it.
#[derive(Clone, Copy)]
struct Ink {
    keyword: Hsla,
    string: Hsla,
    number: Hsla,
    ty: Hsla,
    comment: Hsla,
    text: Hsla,
    faint: Hsla,
    ghost: Hsla,
    ok: Hsla,
    danger: Hsla,
    warn: Hsla,
    lit: Hsla,
}

impl Ink {
    fn of(th: &theme::Theme, sk: &skin::Skin) -> Ink {
        Ink {
            keyword: th.ansi[13],
            string: th.ansi[11],
            number: th.ansi[14],
            ty: th.ansi[12],
            comment: sk.ink.ink_faint,
            text: th.text,
            faint: sk.ink.ink_faint,
            ghost: sk.ink.ink_ghost,
            ok: sk.ink.ok,
            danger: sk.ink.danger,
            warn: sk.ink.warn,
            lit: sk.ink.mark_wash,
        }
    }

    fn tok(&self, t: Tok) -> HighlightStyle {
        let color = match t {
            Tok::Keyword => self.keyword,
            Tok::String => self.string,
            Tok::Number => self.number,
            Tok::Type => self.ty,
            Tok::Comment => self.comment,
        };
        HighlightStyle {
            color: Some(color),
            font_style: (t == Tok::Comment).then_some(FontStyle::Italic),
            ..Default::default()
        }
    }
}

/// A line of code, coloured. `inside` carries a block comment across lines.
fn code_text(
    text: &str,
    sy: Option<&'static flyout::Syntax>,
    inside: &mut bool,
    ink: &Ink,
) -> StyledText {
    let shown = text.replace('\t', "    ");
    let runs = match sy {
        Some(sy) => flyout::tokens(sy, &shown, inside),
        None => Vec::new(),
    };
    StyledText::new(SharedString::from(shown))
        .with_highlights(runs.into_iter().map(|(r, t)| (r, ink.tok(t))))
}

/// Which list a diff row belongs to, so one renderer serves both the Changes
/// tab and a pull request's Files face.
#[derive(Clone, PartialEq)]
enum DiffOwner {
    Changes(Target),
    Pull(Target),
}

/// The state of one file's patch, as a row draws it.
enum Patch<'a> {
    Unread,
    Busy,
    Failed(&'a str),
    Got(&'a FileDiff),
}

impl Workspace {
    /// The shown task's flyout tabs.
    pub(crate) fn side(&self) -> Option<&flyout::Side> {
        self.tabs.get(self.active).map(|t| &t.side)
    }

    fn side_mut(&mut self) -> Option<&mut flyout::Side> {
        self.tabs.get_mut(self.active).map(|t| &mut t.side)
    }

    fn side_current(&self) -> Option<Target> {
        self.side()?.current().cloned()
    }

    /// Whether the body shows the `+` page: asked for, or nothing else to show.
    fn flyout_adding(&self) -> bool {
        self.flyout.adding || self.side().is_none_or(|s| s.tabs.is_empty())
    }

    /// The chip and Ctrl+Shift+R: open and take the keyboard; with it open,
    /// take the keyboard; holding it, close.
    pub(crate) fn flyout_toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.flyout.open {
            self.flyout.open = true;
            self.flyout_focus(window, cx);
            self.flyout_kick(true, cx);
        } else if !self.flyout.focused {
            self.flyout_focus(window, cx);
        } else {
            self.flyout_close(window, cx);
            return;
        }
        self.save(cx);
        cx.notify();
    }

    pub(crate) fn flyout_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flyout.open = false;
        self.flyout.adding = false;
        self.flyout.resize = None;
        if self.flyout.focused {
            self.flyout.focused = false;
            self.focus_active(window, cx);
        }
        self.save(cx);
        cx.notify();
    }

    fn flyout_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flyout.focused = true;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    /// Give the keyboard back to the pane.
    fn flyout_unfocus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flyout.focused = false;
        self.focus_active(window, cx);
        cx.notify();
    }

    /// The keyboard went somewhere else (a pane was clicked): the flyout
    /// stops claiming keys. Called from render.
    pub(crate) fn flyout_lost_focus(&mut self, window: &Window) {
        if self.flyout.focused && !self.focus_handle.is_focused(window) {
            self.flyout.focused = false;
        }
    }

    /// Show `target` in task `task`'s flyout and open the panel, at `line`
    /// for a file. `take_keys` moves the keyboard there; an Alt+click leaves
    /// it in the terminal it was typed into.
    pub(crate) fn flyout_show(
        &mut self,
        task: usize,
        target: Target,
        line: Option<u32>,
        take_keys: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(t) = self.tabs.get_mut(task) else {
            return;
        };
        t.side.open(target.clone());
        self.flyout.open = true;
        self.flyout.adding = false;
        if let (Some(line), Live::File(f)) = (line, self.flyout_live(&target)) {
            f.jump = Some(line);
            f.lit = Some(line);
            if f.text.got.is_some() {
                f.scroll
                    .scroll_to_item(line.saturating_sub(1) as usize, ScrollStrategy::Center);
                f.jump = None;
            }
        }
        if task == self.active {
            self.flyout_read(&target, false, cx);
        }
        if take_keys {
            self.flyout_focus(window, cx);
        }
        self.save(cx);
        cx.notify();
    }

    fn flyout_live(&mut self, t: &Target) -> &mut Live {
        self.flyout
            .live
            .entry(t.clone())
            .or_insert_with(|| Live::for_target(t))
    }

    /// The repository of the shown task's pane.
    fn flyout_here(&self, cx: &App) -> Option<PathBuf> {
        let tab = self.tabs.get(self.active)?;
        let mut leaves = Vec::new();
        tab.root.leaves(&mut leaves);
        let leaf = tab
            .focused
            .and_then(|id| leaves.iter().find(|l| l.entity_id() == id))
            .or(leaves.first())?;
        let cwd = leaf.read(cx).runtime().cwd?;
        self.flyout.root_of(Path::new(&cwd))
    }

    /// Read what is on screen, when it is due, or now with `force`. The poll
    /// calls this every second; opening, switching and the `r` key force it.
    pub(crate) fn flyout_kick(&mut self, force: bool, cx: &mut Context<Self>) {
        if !self.flyout.open {
            return;
        }
        if self.flyout_adding() {
            self.flyout.here = self.flyout_here(cx);
            self.suggest_read(force, cx);
        }
        if let Some(t) = self.side_current() {
            self.flyout_read(&t, force, cx);
        }
    }

    fn flyout_read(&mut self, t: &Target, force: bool, cx: &mut Context<Self>) {
        match t.clone() {
            Target::Changes { repo, base } => self.changes_read(t, repo, base, force, cx),
            Target::File { path } => self.file_read(t, path, force, cx),
            Target::Pull {
                owner,
                repo,
                number,
            } => self.pull_read(t, owner, repo, number, force, cx),
        }
    }

    fn changes_read(
        &mut self,
        key: &Target,
        repo: PathBuf,
        base: Base,
        force: bool,
        cx: &mut Context<Self>,
    ) {
        let Live::Changes(c) = self.flyout_live(key) else {
            return;
        };
        if !(c.list.due(CHANGES_EVERY) || force && !c.list.busy) {
            return;
        }
        c.list.busy = true;
        let k = key.clone();
        let task = cx.spawn(async move |this, cx| {
            let read = cx
                .background_spawn(async move { read_change_list(&repo, base) })
                .await;
            this.update(cx, |ws, cx| ws.changes_landed(&k, read, cx))
                .ok();
        });
        if let Live::Changes(c) = self.flyout_live(key) {
            c.list.task = Some(task);
        }
    }

    fn changes_landed(
        &mut self,
        key: &Target,
        read: Result<ChangeList, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(Live::Changes(c)) = self.flyout.live.get_mut(key) else {
            return;
        };
        // A patch is kept only while its file is unchanged: same counts, same
        // size and time on disk, same revision underneath.
        if let Ok(new) = &read {
            let old = c.list.got.as_ref();
            c.patches.retain(|path, _| {
                let Some(old) = old else { return false };
                old.rev == new.rev
                    && old.stamps.get(path) == new.stamps.get(path)
                    && old.files.iter().find(|f| &f.path == path)
                        == new.files.iter().find(|f| &f.path == path)
            });
        }
        c.list.land(read);
        // The demo shows a diff open, as a person would have it.
        if c.open.is_empty() && std::env::var("TD_FLYOUT_DEMO").is_ok_and(|v| v.contains("expand"))
        {
            if let Some(l) = &c.list.got {
                for f in l.files.iter().filter(|f| !f.path.ends_with('/')).take(2) {
                    c.open.insert(f.path.clone());
                }
            }
        }
        let n = c.list.got.as_ref().map_or(0, |l| l.files.len());
        c.cursor = c.cursor.min(n.saturating_sub(1));
        let want: Vec<FileStat> = c
            .list
            .got
            .as_ref()
            .map(|l| {
                l.files
                    .iter()
                    .filter(|f| c.open.contains(&f.path) && !c.patches.contains_key(&f.path))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        for f in want {
            self.patch_read(key, f, cx);
        }
        cx.notify();
    }

    fn patch_read(&mut self, key: &Target, f: FileStat, cx: &mut Context<Self>) {
        let Target::Changes { repo, .. } = key else {
            return;
        };
        let repo = repo.clone();
        let Some(Live::Changes(c)) = self.flyout.live.get_mut(key) else {
            return;
        };
        let Some(rev) = c.list.got.as_ref().map(|l| l.rev.clone()) else {
            return;
        };
        if f.path.ends_with('/') {
            return;
        }
        let entry = c.patches.entry(f.path.clone()).or_default();
        if entry.busy {
            return;
        }
        entry.busy = true;
        let k = key.clone();
        let path = f.path.clone();
        let filed = f.path.clone();
        let task = cx.spawn(async move |this, cx| {
            let read = cx
                .background_spawn(async move { read_patch(&repo, &rev, &f) })
                .await;
            this.update(cx, |ws, cx| {
                if let Some(Live::Changes(c)) = ws.flyout.live.get_mut(&k) {
                    c.patches.entry(path).or_default().land(read);
                }
                cx.notify();
            })
            .ok();
        });
        // Kept with its reading: a dropped task is a cancelled one.
        if let Some(Live::Changes(c)) = self.flyout.live.get_mut(key) {
            c.patches.entry(filed).or_default().task = Some(task);
        }
    }

    fn file_read(&mut self, key: &Target, path: PathBuf, force: bool, cx: &mut Context<Self>) {
        let Live::File(f) = self.flyout_live(key) else {
            return;
        };
        if !(f.text.due(FILE_EVERY) || force && !f.text.busy) {
            return;
        }
        f.text.busy = true;
        let prev = f.stamp;
        let fresh = force || f.text.got.is_none();
        let k = key.clone();
        let task = cx.spawn(async move |this, cx| {
            let read = cx
                .background_spawn(async move { read_file(&path, prev, fresh) })
                .await;
            this.update(cx, |ws, cx| {
                if let Some(Live::File(f)) = ws.flyout.live.get_mut(&k) {
                    match read {
                        Ok(Some((text, marks, stamp))) => {
                            f.stamp = Some(stamp);
                            f.marks = Rc::new(marks);
                            f.text.land(Ok(text));
                            if let Some(line) = f.jump.take() {
                                f.scroll.scroll_to_item(
                                    line.saturating_sub(1) as usize,
                                    ScrollStrategy::Center,
                                );
                            }
                        }
                        Ok(None) => {
                            f.text.busy = false;
                            f.text.at = Some(Instant::now());
                        }
                        Err(e) => f.text.land(Err(e)),
                    }
                }
                cx.notify();
            })
            .ok();
        });
        if let Live::File(f) = self.flyout_live(key) {
            f.text.task = Some(task);
        }
    }

    /// Where gh runs for `owner/repo`: a checkout of it this window knows, so
    /// an account chosen by the remote is the one asked; else the home folder.
    fn gh_dir(&self, owner: &str, repo: &str) -> PathBuf {
        self.flyout
            .suggest
            .iter()
            .find(|(_, s)| {
                s.got
                    .as_ref()
                    .and_then(|g| g.slug.as_ref())
                    .is_some_and(|(o, r)| o == owner && r == repo)
            })
            .map(|(dir, _)| dir.clone())
            .unwrap_or_else(crate::session::home_dir)
    }

    fn pull_read(
        &mut self,
        key: &Target,
        owner: String,
        repo: String,
        number: u64,
        force: bool,
        cx: &mut Context<Self>,
    ) {
        let dir = self.gh_dir(&owner, &repo);
        let Live::Pull(p) = self.flyout_live(key) else {
            return;
        };
        let wants_diff = p.face == PullFace::Files
            && (p.diff.got.is_none() && p.diff.error.is_none() || force)
            && !p.diff.busy;
        if p.pull.due(PULL_EVERY) || force && !p.pull.busy {
            p.pull.busy = true;
            let (k, d, o, r) = (key.clone(), dir.clone(), owner.clone(), repo.clone());
            let task = cx.spawn(async move |this, cx| {
                let read = cx
                    .background_spawn(async move { read_pull(&d, &o, &r, number) })
                    .await;
                this.update(cx, |ws, cx| {
                    if let Some(Live::Pull(p)) = ws.flyout.live.get_mut(&k) {
                        // A pull request that moved since its diff was read
                        // has its diff read again when next shown.
                        let moved = matches!((&p.pull.got, &read), (Some(old), Ok(new)) if old.updated_ms != new.updated_ms);
                        if moved {
                            p.diff = Reading::default();
                        }
                        p.pull.land(read);
                    }
                    cx.notify();
                })
                .ok();
            });
            if let Live::Pull(p) = self.flyout_live(key) {
                p.pull.task = Some(task);
            }
        }
        if wants_diff {
            if let Live::Pull(p) = self.flyout_live(key) {
                p.diff.busy = true;
            }
            let k = key.clone();
            let task = cx.spawn(async move |this, cx| {
                let read = cx
                    .background_spawn(async move { read_pull_diff(&dir, &owner, &repo, number) })
                    .await;
                this.update(cx, |ws, cx| {
                    if let Some(Live::Pull(p)) = ws.flyout.live.get_mut(&k) {
                        p.diff.land(read);
                    }
                    cx.notify();
                })
                .ok();
            });
            if let Live::Pull(p) = self.flyout_live(key) {
                p.diff.task = Some(task);
            }
        }
    }

    fn suggest_read(&mut self, force: bool, cx: &mut Context<Self>) {
        let Some(repo) = self.flyout.here.clone() else {
            return;
        };
        let s = self.flyout.suggest.entry(repo.clone()).or_default();
        if !(s.due(SUGGEST_EVERY) || force && !s.busy) {
            return;
        }
        s.busy = true;
        let r = repo.clone();
        let task = cx.spawn(async move |this, cx| {
            let read = cx.background_spawn(async move { read_suggest(&r) }).await;
            this.update(cx, |ws, cx| {
                ws.flyout.suggest.entry(repo).or_default().land(read);
                cx.notify();
            })
            .ok();
        });
        if let Some(here) = self.flyout.here.clone() {
            if let Some(s) = self.flyout.suggest.get_mut(&here) {
                s.task = Some(task);
            }
        }
    }

    /// `TD_FLYOUT_DEMO`: open the flyout once on the first task, for a rig
    /// that photographs it without a click. Items are `;`-separated:
    /// `changes:<repo>`, `file:<path>[:line]`, `pull:<owner>/<repo>#<n>`,
    /// `add` (the `+` page), `face:files` or `face:conversation` (a pull
    /// request's face), and `keys` (the panel holds the keyboard). The last
    /// tab named is the one shown. The Floor's and the dialogs' demo hooks,
    /// for this one.
    pub(crate) fn flyout_demo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        static SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let Some(spec) = std::env::var("TD_FLYOUT_DEMO")
            .ok()
            .filter(|s| !s.is_empty())
        else {
            return;
        };
        if self.tabs.is_empty() || SHOWN.swap(true, std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let mut face = None;
        let mut keys = false;
        for item in spec.split(';').map(str::trim) {
            let (kind, arg) = item.split_once(':').unwrap_or((item, ""));
            let target = match kind {
                "changes" => Some(Target::Changes {
                    repo: PathBuf::from(arg),
                    base: Base::Branch,
                }),
                "file" => {
                    let (path, line) = flyout::split_line_ref(arg);
                    let t = Target::File {
                        path: PathBuf::from(path),
                    };
                    if let (Some(line), Live::File(f)) = (line, self.flyout_live(&t)) {
                        f.jump = Some(line);
                        f.lit = Some(line);
                    }
                    Some(t)
                }
                "pull" => arg.split_once('/').and_then(|(owner, rest)| {
                    let (repo, n) = rest.split_once('#')?;
                    Some(Target::Pull {
                        owner: owner.into(),
                        repo: repo.into(),
                        number: n.parse().ok()?,
                    })
                }),
                "add" => {
                    self.flyout.adding = true;
                    None
                }
                "face" => {
                    face = match arg {
                        "files" => Some(PullFace::Files),
                        "conversation" => Some(PullFace::Conversation),
                        _ => Some(PullFace::Overview),
                    };
                    None
                }
                "keys" => {
                    keys = true;
                    None
                }
                _ => None,
            };
            if let Some(t) = target {
                self.tabs[0].side.open(t);
            }
        }
        if let (Some(face), Some(t)) = (face, self.tabs[0].side.current().cloned()) {
            if let Live::Pull(p) = self.flyout_live(&t) {
                p.face = face;
            }
        }
        self.flyout.open = true;
        if keys {
            self.flyout_focus(window, cx);
        }
        self.flyout_kick(true, cx);
        cx.notify();
    }

    // ---- gestures --------------------------------------------------------

    fn flyout_activate(&mut self, i: usize, cx: &mut Context<Self>) {
        if let Some(side) = self.side_mut() {
            if i < side.tabs.len() {
                side.active = i;
            }
        }
        self.flyout.adding = false;
        self.flyout_kick(true, cx);
        self.save(cx);
        cx.notify();
    }

    fn flyout_close_tab(&mut self, i: usize, cx: &mut Context<Self>) {
        let gone = self.side_mut().and_then(|s| s.close(i));
        // Drop what it read when no task still shows it.
        if let Some(t) = gone {
            let shown = self.tabs.iter().any(|tab| tab.side.tabs.contains(&t));
            if !shown {
                self.flyout.live.remove(&t);
            }
        }
        self.flyout_kick(true, cx);
        self.save(cx);
        cx.notify();
    }

    fn flyout_add(&mut self, cx: &mut Context<Self>) {
        self.flyout.adding = true;
        self.flyout.query.clear();
        self.flyout.pick = 0;
        self.flyout_kick(true, cx);
        cx.notify();
    }

    fn changes_base(&mut self, base: Base, cx: &mut Context<Self>) {
        let Some(t) = self.side_mut().and_then(|s| s.current_mut()) else {
            return;
        };
        if let Target::Changes { base: b, .. } = t {
            if *b == base {
                return;
            }
            *b = base;
        }
        self.flyout_kick(true, cx);
        self.save(cx);
        cx.notify();
    }

    /// Open or fold one file's diff, in either list.
    fn diff_toggle(&mut self, owner: &DiffOwner, path: &str, cx: &mut Context<Self>) {
        match owner {
            DiffOwner::Changes(key) => {
                let mut read = None;
                if let Some(Live::Changes(c)) = self.flyout.live.get_mut(key) {
                    if !c.open.remove(path) {
                        c.open.insert(path.to_string());
                        if !c.patches.contains_key(path) {
                            read = c
                                .list
                                .got
                                .as_ref()
                                .and_then(|l| l.files.iter().find(|f| f.path == path).cloned());
                        }
                    }
                    if let Some(i) = c
                        .list
                        .got
                        .as_ref()
                        .and_then(|l| l.files.iter().position(|f| f.path == path))
                    {
                        c.cursor = i;
                    }
                }
                if let Some(f) = read {
                    self.patch_read(key, f, cx);
                }
            }
            DiffOwner::Pull(key) => {
                if let Some(Live::Pull(p)) = self.flyout.live.get_mut(key) {
                    if !p.open.remove(path) {
                        p.open.insert(path.to_string());
                    }
                }
            }
        }
        cx.notify();
    }

    fn diff_more(&mut self, owner: &DiffOwner, path: &str, cx: &mut Context<Self>) {
        match owner {
            DiffOwner::Changes(key) => {
                if let Some(Live::Changes(c)) = self.flyout.live.get_mut(key) {
                    c.full.insert(path.to_string());
                }
            }
            DiffOwner::Pull(key) => {
                if let Some(Live::Pull(p)) = self.flyout.live.get_mut(key) {
                    p.full.insert(path.to_string());
                }
            }
        }
        cx.notify();
    }

    /// Every file of a list open, or all folded when they already are.
    fn diff_all(&mut self, owner: &DiffOwner, cx: &mut Context<Self>) {
        match owner {
            DiffOwner::Changes(key) => {
                let mut want = Vec::new();
                if let Some(Live::Changes(c)) = self.flyout.live.get_mut(key) {
                    let files: Vec<FileStat> = c
                        .list
                        .got
                        .as_ref()
                        .map(|l| {
                            l.files
                                .iter()
                                .filter(|f| !f.path.ends_with('/'))
                                .cloned()
                                .collect()
                        })
                        .unwrap_or_default();
                    if !c.open.is_empty() {
                        c.open.clear();
                    } else {
                        for f in files.into_iter().take(60) {
                            if !c.patches.contains_key(&f.path) {
                                want.push(f.clone());
                            }
                            c.open.insert(f.path);
                        }
                    }
                }
                for f in want {
                    self.patch_read(key, f, cx);
                }
            }
            DiffOwner::Pull(key) => {
                if let Some(Live::Pull(p)) = self.flyout.live.get_mut(key) {
                    if !p.open.is_empty() {
                        p.open.clear();
                    } else if let Some(d) = &p.diff.got {
                        p.open = d.iter().take(60).map(|f| f.path().to_string()).collect();
                    }
                }
            }
        }
        cx.notify();
    }

    fn pull_face(&mut self, face: PullFace, cx: &mut Context<Self>) {
        if let Some(t) = self.side_current() {
            if let Some(Live::Pull(p)) = self.flyout.live.get_mut(&t) {
                p.face = face;
            }
            self.flyout_read(&t, false, cx);
        }
        cx.notify();
    }

    /// Open what an `+` page row names.
    fn add_open(&mut self, row: AddRow, window: &mut Window, cx: &mut Context<Self>) {
        let target = match row {
            AddRow::Changes(repo) => Target::Changes {
                repo,
                base: Base::Branch,
            },
            AddRow::Pull {
                owner, repo, row, ..
            } => Target::Pull {
                owner,
                repo,
                number: row.number,
            },
            AddRow::File(path, _) => Target::File { path },
        };
        self.flyout.query.clear();
        self.flyout.pick = 0;
        let task = self.active;
        self.flyout_show(task, target, None, true, window, cx);
    }

    /// The `+` page's rows, in the order they are drawn and walked.
    fn add_rows(&self) -> Vec<AddRow> {
        let mut rows = Vec::new();
        let Some(here) = self.flyout.here.clone() else {
            return rows;
        };
        let q = self.flyout.query.trim().to_string();
        let got = self.flyout.suggest.get(&here).and_then(|s| s.got.as_ref());
        if q.is_empty() {
            rows.push(AddRow::Changes(here.clone()));
            if let Some(g) = got {
                if let Some((owner, repo)) = &g.slug {
                    if let Some(mine) = &g.branch_pull {
                        rows.push(AddRow::Pull {
                            owner: owner.clone(),
                            repo: repo.clone(),
                            row: mine.clone(),
                            mine: true,
                        });
                    }
                    for p in &g.pulls {
                        if g.branch_pull.as_ref().is_some_and(|m| m.number == p.number) {
                            continue;
                        }
                        rows.push(AddRow::Pull {
                            owner: owner.clone(),
                            repo: repo.clone(),
                            row: p.clone(),
                            mine: false,
                        });
                    }
                }
                for c in g.changed.iter().filter(|c| !c.ends_with('/')).take(12) {
                    rows.push(AddRow::File(here.join(c), c.clone()));
                }
            }
            return rows;
        }
        // Typing: a pull request by number first, then files by score.
        if let (Some(g), Ok(n)) = (got, q.trim_start_matches('#').parse::<u64>()) {
            if let Some((owner, repo)) = &g.slug {
                let row = g
                    .pulls
                    .iter()
                    .chain(g.branch_pull.iter())
                    .find(|p| p.number == n)
                    .cloned()
                    .unwrap_or(PullRow {
                        number: n,
                        title: String::new(),
                        head: None,
                        author: None,
                        draft: false,
                        updated_ms: None,
                    });
                rows.push(AddRow::Pull {
                    owner: owner.clone(),
                    repo: repo.clone(),
                    row,
                    mine: false,
                });
            }
        }
        if let Some(g) = got {
            let mut scored: Vec<(i64, &String)> = g
                .changed
                .iter()
                .filter(|c| !c.ends_with('/'))
                .chain(g.files.iter())
                .filter_map(|p| flyout::file_score(p, &q).map(|s| (s, p)))
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
            let mut seen = HashSet::new();
            for (_, p) in scored {
                if seen.insert(p.clone()) {
                    rows.push(AddRow::File(here.join(p), p.clone()));
                }
                if rows.len() >= FINDER_ROWS {
                    break;
                }
            }
        }
        rows
    }

    // ---- keys ------------------------------------------------------------

    /// The flyout's keys while it holds the keyboard. Answers false for a
    /// chord it does not use, so the window's own chords still work; every
    /// plain key is taken, so none can reach a terminal.
    pub(crate) fn flyout_key(
        &mut self,
        ks: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.flyout.open || !self.flyout.focused {
            return false;
        }
        let m = &ks.modifiers;
        let key = ks.key.as_str();
        if m.control && m.shift && key == "r" {
            self.flyout_toggle(window, cx);
            return true;
        }
        if m.control && !m.shift && !m.alt && key == "w" {
            let at = self.side().map_or(0, |s| s.active);
            self.flyout_close_tab(at, cx);
            return true;
        }
        if m.control && matches!(key, "pageup" | "pagedown") {
            if let Some(s) = self.side_mut() {
                s.step(key == "pagedown");
            }
            self.flyout_kick(true, cx);
            cx.notify();
            return true;
        }
        if m.control || m.alt || m.platform {
            return false;
        }
        if key == "escape" {
            if self.flyout.adding && self.side().is_some_and(|s| !s.tabs.is_empty()) {
                self.flyout.adding = false;
                cx.notify();
            } else {
                self.flyout_unfocus(window, cx);
            }
            return true;
        }
        if self.flyout_adding() {
            self.add_key(ks, window, cx);
            return true;
        }
        let Some(t) = self.side_current() else {
            return true;
        };
        let char = ks.key_char.as_deref().unwrap_or(key);
        match char {
            "[" => {
                if let Some(s) = self.side_mut() {
                    s.step(false);
                }
                self.flyout_kick(true, cx);
            }
            "]" => {
                if let Some(s) = self.side_mut() {
                    s.step(true);
                }
                self.flyout_kick(true, cx);
            }
            "+" | "n" | "t" => self.flyout_add(cx),
            "w" => {
                let at = self.side().map_or(0, |s| s.active);
                self.flyout_close_tab(at, cx);
            }
            "r" => self.flyout_kick(true, cx),
            "o" => {
                if let Some(url) = t.url() {
                    crate::pane::open_with_system(&url);
                }
            }
            "b" => {
                if let Target::Changes { base, .. } = &t {
                    let next = match base {
                        Base::Branch => Base::Uncommitted,
                        Base::Uncommitted => Base::Branch,
                    };
                    self.changes_base(next, cx);
                }
            }
            "e" => {
                let owner = match &t {
                    Target::Changes { .. } => Some(DiffOwner::Changes(t.clone())),
                    Target::Pull { .. } => Some(DiffOwner::Pull(t.clone())),
                    Target::File { .. } => None,
                };
                if let Some(o) = owner {
                    self.diff_all(&o, cx);
                }
            }
            d if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() && d != "0" => {
                let i = (d.as_bytes()[0] - b'1') as usize;
                self.flyout_activate(i, cx);
            }
            _ => self.flyout_move(&t, key, m.shift, cx),
        }
        cx.notify();
        true
    }

    /// Arrows, pages and Enter inside the shown tab.
    fn flyout_move(&mut self, t: &Target, key: &str, shift: bool, cx: &mut Context<Self>) {
        let page = |scroll: &ScrollHandle, down: bool| {
            let h = f32::from(scroll.bounds().size.height).max(100.);
            let max = f32::from(scroll.max_offset().y).max(0.);
            let at = -f32::from(scroll.offset().y);
            let y = (at + if down { h * 0.9 } else { -h * 0.9 }).clamp(0., max);
            scroll.set_offset(point(px(0.), px(-y)));
        };
        match t {
            Target::File { .. } => {
                if let Some(Live::File(f)) = self.flyout.live.get_mut(t) {
                    let n = f.text.got.as_ref().map_or(0, |x| x.lines.len());
                    let top = list_top(&f.scroll);
                    let to = match key {
                        "up" | "k" => top.saturating_sub(1),
                        "down" | "j" => top + 1,
                        "pageup" => top.saturating_sub(30),
                        "pagedown" | "space" => top + 30,
                        "home" => 0,
                        "end" => n,
                        _ => return,
                    };
                    f.scroll
                        .scroll_to_item(to.min(n.saturating_sub(1)), ScrollStrategy::Top);
                }
            }
            Target::Changes { .. } | Target::Pull { .. } => {
                let owner = match t {
                    Target::Changes { .. } => DiffOwner::Changes(t.clone()),
                    _ => DiffOwner::Pull(t.clone()),
                };
                if let (Target::Pull { .. }, "tab") = (t, key) {
                    if let Some(Live::Pull(p)) = self.flyout.live.get(t) {
                        let faces = [PullFace::Overview, PullFace::Files, PullFace::Conversation];
                        let at = faces.iter().position(|f| *f == p.face).unwrap_or(0);
                        let next = if shift { (at + 2) % 3 } else { (at + 1) % 3 };
                        self.pull_face(faces[next], cx);
                    }
                    return;
                }
                let (paths, cursor, scroll) = match self.flyout.live.get(t) {
                    Some(Live::Changes(c)) => (
                        c.list
                            .got
                            .as_ref()
                            .map(|l| l.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>())
                            .unwrap_or_default(),
                        c.cursor,
                        c.scroll.clone(),
                    ),
                    Some(Live::Pull(p)) if p.face == PullFace::Files => (
                        p.diff
                            .got
                            .as_ref()
                            .map(|d| d.iter().map(|f| f.path().to_string()).collect::<Vec<_>>())
                            .unwrap_or_default(),
                        p.cursor,
                        p.scroll.clone(),
                    ),
                    Some(Live::Pull(p)) => {
                        match key {
                            "pageup" | "up" | "k" => page(&p.scroll, false),
                            "pagedown" | "down" | "j" | "space" => page(&p.scroll, true),
                            "home" => p.scroll.set_offset(point(px(0.), px(0.))),
                            _ => {}
                        }
                        return;
                    }
                    _ => return,
                };
                let n = paths.len();
                let to = match key {
                    "up" | "k" => cursor.saturating_sub(1),
                    "down" | "j" => (cursor + 1).min(n.saturating_sub(1)),
                    "home" => 0,
                    "end" => n.saturating_sub(1),
                    "pageup" => return page(&scroll, false),
                    "pagedown" => return page(&scroll, true),
                    "enter" | "space" | "right" | "l" | "left" | "h" => {
                        if let Some(path) = paths.get(cursor) {
                            let open = match self.flyout.live.get(t) {
                                Some(Live::Changes(c)) => c.open.contains(path),
                                Some(Live::Pull(p)) => p.open.contains(path),
                                _ => false,
                            };
                            let wants = match key {
                                "right" | "l" => !open,
                                "left" | "h" => open,
                                _ => true,
                            };
                            if wants {
                                let path = path.clone();
                                self.diff_toggle(&owner, &path, cx);
                            }
                        }
                        return;
                    }
                    _ => return,
                };
                match self.flyout.live.get_mut(t) {
                    Some(Live::Changes(c)) => c.cursor = to,
                    Some(Live::Pull(p)) => p.cursor = to,
                    _ => {}
                }
                scroll.scroll_to_item(to);
            }
        }
    }

    /// The `+` page's keys: typing filters, arrows choose, Enter opens.
    fn add_key(&mut self, ks: &gpui::Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        let key = ks.key.as_str();
        let rows = self.add_rows();
        match key {
            "up" => self.flyout.pick = self.flyout.pick.saturating_sub(1),
            "down" => self.flyout.pick = (self.flyout.pick + 1).min(rows.len().saturating_sub(1)),
            "enter" => {
                if let Some(row) = rows.get(self.flyout.pick).cloned() {
                    self.add_open(row, window, cx);
                }
                return;
            }
            "backspace" => {
                self.flyout.query.pop();
                self.flyout.pick = 0;
            }
            _ => {
                if let Some(c) = ks
                    .key_char
                    .as_deref()
                    .filter(|c| !c.chars().any(char::is_control))
                {
                    self.flyout.query.push_str(c);
                    self.flyout.pick = 0;
                }
            }
        }
        cx.notify();
    }

    // ---- drawing ---------------------------------------------------------

    /// The panel's width in window pixels: the dragged width, short of what
    /// the terminals keep.
    fn flyout_px(&self, s: f32) -> f32 {
        let want = self.flyout.width * s;
        let Some((_, _, win_w, _)) = self.last_win else {
            return want;
        };
        let left = if self.left_bar {
            self.left_bar_w * s
        } else {
            0.
        };
        let room = win_w - left - (RIGHT_COLUMN_W + SCREEN_KEEPS) * s - 24.;
        want.min(room).max(FLYOUT_MIN * s * 0.75)
    }

    /// A target's colour: the left-bar project its repository is filed as,
    /// else the faint ink. This is the dot that tells two tabs apart.
    fn target_colour(&self, t: &Target, cx: &App) -> Hsla {
        let projects: Vec<(u32, String)> = self
            .projects
            .iter()
            .map(|p| (p.id, self.branch_label(BarBranch::Project(p.id))))
            .collect();
        let id = match t {
            Target::Pull { repo, .. } => crate::entry::project_by_name(Path::new(repo), &projects),
            _ => t
                .folder()
                .and_then(|f| self.flyout.root_of(f))
                .and_then(|root| crate::entry::project_by_name(&root, &projects)),
        };
        id.map(|id| self.branch_color(BarBranch::Project(id), cx))
            .unwrap_or_else(|| theme::theme(cx).faint)
    }

    /// A tab's label: the target's short name, and a pull request's title
    /// once it is known.
    fn tab_label(&self, t: &Target) -> String {
        match (t, self.flyout.live.get(t)) {
            (Target::Pull { number, .. }, Some(Live::Pull(p))) => match &p.pull.got {
                Some(pull) if !pull.title.is_empty() => format!("#{number} {}", pull.title),
                _ => t.short_name(),
            },
            _ => t.short_name(),
        }
    }

    /// The whole of a tab's identity, for its tooltip.
    fn tab_identity(&self, t: &Target) -> String {
        match t {
            Target::Changes { repo, .. } => repo.display().to_string(),
            Target::File { path } => path.display().to_string(),
            Target::Pull {
                owner,
                repo,
                number,
            } => format!("{owner}/{repo} #{number}"),
        }
    }

    /// The chip under the attention pill: how many tabs this task has, lit
    /// while the panel is open.
    pub(crate) fn render_flyout_chip(&self, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let s = theme::outer_choice(cx).grade.scale;
        let sk = skin::skin(cx, s);
        let th = skin::native_theme(cx, theme::theme(cx));
        let f = self.lang.strings().flyout;
        let count = self.side().map_or(0, |side| side.tabs.len());
        let open = self.flyout.open;
        let chip = sk
            .sheet()
            .id("flyout-chip")
            .debug_selector(|| "flyout-chip".into())
            .flex()
            .flex_col()
            .items_center()
            .gap(px(2. * s))
            .px(px(5. * s))
            .py(px(6. * s))
            .rounded(sk.radius())
            .border(sk.box_border(1.))
            .border_color(if open {
                th.accent.alpha(0.85)
            } else {
                sk.ink.ink.alpha(0.22)
            })
            .bg(if open {
                sk.ink.mark_wash
            } else {
                sk.ink.ink.alpha(0.06)
            })
            .map(|d| sk.finish(d))
            .cursor_pointer()
            .child(
                div()
                    .text_size(sk.text_px(12. * s))
                    .text_color(if open { th.accent } else { sk.ink.ink_dim })
                    .child("\u{25e8}"),
            )
            .when(count > 0, |d| {
                d.child(
                    div()
                        .text_size(sk.text_px(9. * s))
                        .text_color(sk.ink.ink_dim)
                        .child(count.to_string()),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    ws.flyout_toggle(window, cx);
                }),
            );
        with_tip(chip, vec![f.chip_tip.into(), f.chip_keys.into()])
    }

    /// The panel, docked beside the screen: `None` while it is closed.
    pub(crate) fn render_flyout(&self, cx: &mut Context<Self>) -> Option<gpui::Div> {
        if !self.flyout.open {
            return None;
        }
        let s = theme::outer_choice(cx).grade.scale;
        let sk = skin::skin(cx, s);
        let th = skin::native_theme(cx, theme::theme(cx));
        let w = self.flyout_px(s);
        let focused = self.flyout.focused;
        let panel = div()
            .id("flyout-panel")
            .debug_selector(|| "flyout-panel".into())
            .relative()
            .flex_1()
            .min_h_0()
            .w(px(w))
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(sk.rad_raw(8.))
            .border(sk.box_border(2.))
            .border_color(if focused {
                th.accent.alpha(0.95)
            } else {
                th.accent.alpha(0.55)
            })
            .bg(sk.region_face(darken(th.surface, 0.45)))
            .shadow(float_shadows(th.accent, th.grade.phosphor))
            .map(|d| sk.finish(d))
            // Any press on the panel takes the keyboard, ahead of the row or
            // button it lands on, so what is typed next comes here.
            .capture_any_mouse_down(cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                if !ws.flyout.focused {
                    ws.flyout_focus(window, cx);
                }
            }))
            .child(self.flyout_strip(&th, &sk, s, cx))
            .child(self.flyout_body(&th, &sk, s, cx))
            .child(self.flyout_foot(&sk, s))
            // The edge that resizes it: a hairline lit under the pointer, as
            // the left bar's own edge is.
            .child(
                div()
                    .id("flyout-resize")
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(px(5.))
                    .cursor_col_resize()
                    .hover(|st| st.bg(th.accent.alpha(0.35)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, ev: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.flyout.resize = Some((f32::from(ev.position.x), ws.flyout.width));
                        }),
                    ),
            );
        Some(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .pt(px(8. * s))
                .pl(px(4. * s))
                .child(panel),
        )
    }

    fn flyout_strip(
        &self,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let f = self.lang.strings().flyout;
        let tabs = self.side().map(|x| x.tabs.clone()).unwrap_or_default();
        let active = self.side().map_or(0, |x| x.active);
        let adding = self.flyout_adding();
        let mut strip = div()
            .id("flyout-strip")
            .flex()
            .flex_row()
            .items_center()
            .gap(px(3. * s))
            .flex_1()
            .min_w_0()
            .overflow_x_scroll();
        for (i, t) in tabs.iter().enumerate() {
            let lit = i == active && !adding;
            let colour = self.target_colour(t, cx);
            let tab = div()
                .id(SharedString::from(format!("flyout-tab-{i}")))
                .debug_selector(move || format!("flyout-tab-{i}"))
                .group(SharedString::from(format!("flyout-tab-g-{i}")))
                .flex()
                .flex_row()
                .flex_none()
                .items_center()
                .gap(px(5. * s))
                .px(px(7. * s))
                .py(px(3. * s))
                .max_w(px(200. * s))
                .map(|d| sk.choice(d, lit))
                .cursor_pointer()
                .child(div().flex_none().size(px(6. * s)).rounded_full().bg(colour))
                .child(
                    div()
                        .flex_none()
                        .text_size(sk.text_px(11. * s))
                        .text_color(if lit { th.accent } else { sk.ink.ink_dim })
                        .child(t.glyph()),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(sk.text_px(11. * s))
                        .child(self.tab_label(t)),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("flyout-tab-x-{i}")))
                        .flex_none()
                        .px(px(2. * s))
                        .text_size(sk.text_px(10. * s))
                        .text_color(if lit {
                            sk.ink.ink_faint
                        } else {
                            gpui::transparent_black()
                        })
                        .group_hover(SharedString::from(format!("flyout-tab-g-{i}")), |st| {
                            st.text_color(sk.ink.ink_faint)
                        })
                        .hover(|st| st.text_color(sk.ink.danger))
                        .child("\u{d7}")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.flyout_close_tab(i, cx);
                            }),
                        ),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                        ws.flyout_activate(i, cx);
                    }),
                )
                .on_mouse_down(
                    MouseButton::Middle,
                    cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        ws.flyout_close_tab(i, cx);
                    }),
                );
            strip = strip.child(with_tip(tab, vec![self.tab_identity(t)]));
        }
        let button = |id: &'static str, glyph: &'static str, lit: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .flex_none()
                .px(px(6. * s))
                .py(px(2. * s))
                .rounded(sk.radius())
                .text_size(sk.text_px(12. * s))
                .text_color(if lit { th.accent } else { sk.ink.ink_dim })
                .cursor_pointer()
                .hover(|st| st.bg(sk.ink.hover))
                .child(glyph)
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4. * s))
            .pl(px(9. * s))
            .pr(px(6. * s))
            .pt(px(6. * s))
            .pb(px(5. * s))
            .border_b_1()
            .border_color(sk.ink.rule)
            .child(strip)
            .child(with_tip(
                button("flyout-plus", "+", adding).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        ws.flyout_add(cx);
                    }),
                ),
                vec![f.new_tab.into()],
            ))
            .child(with_tip(
                button("flyout-close", "\u{d7}", false).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        ws.flyout_close(window, cx);
                    }),
                ),
                vec![f.close_panel.into()],
            ))
    }

    fn flyout_foot(&self, sk: &skin::Skin, s: f32) -> gpui::Div {
        let f = self.lang.strings().flyout;
        let words = if self.flyout_adding() {
            f.keys_add
        } else {
            f.keys
        };
        div()
            .flex_none()
            .px(px(10. * s))
            .py(px(4. * s))
            .border_t_1()
            .border_color(sk.ink.rule)
            .text_size(sk.text_px(9. * s))
            .text_color(if self.flyout.focused {
                sk.ink.ink_dim
            } else {
                sk.ink.ink_ghost
            })
            .truncate()
            .child(words)
    }

    fn flyout_body(
        &self,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let f = self.lang.strings().flyout;
        if self.flyout_adding() {
            return self.add_page(th, sk, s, cx);
        }
        let Some(t) = self.side_current() else {
            return self.add_page(th, sk, s, cx);
        };
        match (&t, self.flyout.live.get(&t)) {
            (Target::Changes { repo, base }, Some(Live::Changes(c))) => {
                self.changes_body(&t, repo, *base, c, th, sk, s, cx)
            }
            (Target::File { path }, Some(Live::File(fl))) => {
                self.file_body(path, fl, th, sk, s, cx)
            }
            (Target::Pull { .. }, Some(Live::Pull(p))) => self.pull_body(&t, p, th, sk, s, cx),
            _ => note(f.reading, sk.ink.ink_faint, sk, s).into_any_element(),
        }
    }

    // ---- the Changes tab -------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn changes_body(
        &self,
        key: &Target,
        repo: &Path,
        base: Base,
        c: &ChangesLive,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let f = self.lang.strings().flyout;
        let ink = Ink::of(th, sk);
        let list = c.list.got.as_ref();
        let (a, r) = list.map(|l| flyout::totals(&l.files)).unwrap_or((0, 0));
        let base_chip = |b: Base, word: &'static str| {
            let lit = b == base;
            div()
                .id(SharedString::from(format!("flyout-base-{b:?}")))
                .debug_selector(move || format!("flyout-base-{b:?}"))
                .px(px(7. * s))
                .py(px(2. * s))
                .map(|d| sk.choice(d, lit))
                .text_size(sk.text_px(10. * s))
                .cursor_pointer()
                .child(word)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, _w, cx| ws.changes_base(b, cx)),
                )
        };
        let owner = DiffOwner::Changes(key.clone());
        let owner_all = owner.clone();
        let head = div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(5. * s))
            .px(px(10. * s))
            .pt(px(8. * s))
            .pb(px(6. * s))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6. * s))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(sk.text_px(12.5 * s))
                            .text_color(th.text)
                            .child(
                                list.and_then(|l| l.branch.clone())
                                    .unwrap_or_else(|| flyout::Target::short_name(key)),
                            ),
                    )
                    .child(div().flex_1())
                    .when(list.is_some(), |d| {
                        d.child(
                            div()
                                .flex_none()
                                .text_size(sk.text_px(11. * s))
                                .text_color(ink.ok)
                                .child(format!("+{a}")),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_size(sk.text_px(11. * s))
                                .text_color(ink.danger)
                                .child(format!("\u{2212}{r}")),
                        )
                    })
                    .child(with_tip(
                        icon_button("flyout-refresh", "\u{27f3}", c.list.busy, sk, s)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                    ws.flyout_kick(true, cx)
                                }),
                            ),
                        vec![f.refresh.into()],
                    ))
                    .child(with_tip(
                        icon_button("flyout-expand", "\u{21d5}", !c.open.is_empty(), sk, s)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                    ws.diff_all(&owner_all, cx)
                                }),
                            ),
                        vec![if c.open.is_empty() {
                            f.expand_all.into()
                        } else {
                            f.collapse_all.into()
                        }],
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4. * s))
                    .child(base_chip(Base::Branch, f.base_branch))
                    .child(base_chip(Base::Uncommitted, f.base_uncommitted))
                    .child(
                        div()
                            .ml(px(4. * s))
                            .min_w_0()
                            .truncate()
                            .text_size(sk.text_px(10. * s))
                            .text_color(sk.ink.ink_faint)
                            .child(match list {
                                Some(l) => lang::fill(f.against, &[("rev", &l.against)]),
                                None => String::new(),
                            }),
                    )
                    .child(div().flex_1())
                    .when_some(list, |d, l| {
                        d.child(
                            div()
                                .flex_none()
                                .text_size(sk.text_px(10. * s))
                                .text_color(sk.ink.ink_faint)
                                .child(lang::fill(
                                    f.changed,
                                    &[("count", &l.files.len().to_string())],
                                )),
                        )
                    }),
            );
        let mut notes: Vec<AnyElement> = Vec::new();
        if let Some(e) = &c.list.error {
            notes.push(note(e, ink.danger, sk, s).into_any_element());
        }
        match list {
            None if c.list.error.is_none() => {
                notes.push(note(f.reading, sk.ink.ink_faint, sk, s).into_any_element())
            }
            Some(l) if l.files.is_empty() => {
                notes.push(note(f.no_changes, sk.ink.ink_faint, sk, s).into_any_element())
            }
            _ => {}
        }
        let mut rows = div()
            .id("flyout-changes")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&c.scroll)
            .flex()
            .flex_col()
            .px(px(4. * s))
            .pb(px(10. * s));
        if let Some(l) = list {
            for (i, file) in l.files.iter().enumerate() {
                let patch = match c.patches.get(&file.path) {
                    None => Patch::Unread,
                    Some(r) => match (&r.got, &r.error) {
                        (Some(d), _) => Patch::Got(d),
                        (None, Some(e)) => Patch::Failed(e),
                        (None, None) => Patch::Busy,
                    },
                };
                rows = rows.child(self.diff_file(
                    &owner,
                    i,
                    file,
                    Some(repo),
                    patch,
                    c.open.contains(&file.path),
                    c.full.contains(&file.path),
                    i == c.cursor && self.flyout.focused,
                    &ink,
                    th,
                    sk,
                    s,
                    cx,
                ));
            }
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(head)
            .children(notes)
            .child(rows)
            .into_any_element()
    }

    /// One file of a diff list: its row, and its hunks when open.
    #[allow(clippy::too_many_arguments)]
    fn diff_file(
        &self,
        owner: &DiffOwner,
        i: usize,
        file: &FileStat,
        repo: Option<&Path>,
        patch: Patch<'_>,
        open: bool,
        full: bool,
        cursor: bool,
        ink: &Ink,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let f = self.lang.strings().flyout;
        let dir = file.path.ends_with('/');
        let (mark_ink, mark) = match file.status {
            Status::Added | Status::Untracked => (ink.ok, file.status.mark()),
            Status::Deleted => (ink.danger, file.status.mark()),
            Status::Modified => (ink.warn, file.status.mark()),
            Status::Renamed | Status::Copied | Status::TypeChanged => {
                (sk.ink.ink_dim, file.status.mark())
            }
            Status::Unknown => (ink.faint, file.status.mark()),
        };
        // The folders dim, the file's own name bright.
        let split = file
            .path
            .trim_end_matches('/')
            .rfind('/')
            .map_or(0, |at| at + 1);
        let path_text = StyledText::new(SharedString::from(file.path.clone())).with_highlights(
            (split > 0).then(|| {
                (
                    0..split,
                    HighlightStyle {
                        color: Some(sk.ink.ink_faint),
                        ..Default::default()
                    },
                )
            }),
        );
        let counts: AnyElement = if dir {
            small(f.untracked_dir, ink.faint, sk, s).into_any_element()
        } else if file.binary() {
            small(f.binary, ink.faint, sk, s).into_any_element()
        } else {
            div()
                .flex()
                .flex_row()
                .gap(px(5. * s))
                .flex_none()
                .child(small(
                    format!("+{}", file.added.unwrap_or(0)),
                    ink.ok,
                    sk,
                    s,
                ))
                .child(small(
                    format!("\u{2212}{}", file.removed.unwrap_or(0)),
                    ink.danger,
                    sk,
                    s,
                ))
                .into_any_element()
        };
        let path = file.path.clone();
        let toggle_owner = owner.clone();
        let file_path = repo.map(|r| r.join(&file.path));
        let open_owner_file = file_path.clone();
        let row = div()
            .id(SharedString::from(format!("flyout-file-{i}")))
            .debug_selector(move || format!("flyout-file-{i}"))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6. * s))
            .px(px(6. * s))
            .py(px(4. * s))
            .rounded(sk.radius())
            .cursor_pointer()
            .hover(|st| st.bg(sk.ink.hover))
            .when(cursor, |d| {
                d.bg(sk.ink.row_active).shadow(vec![BoxShadow {
                    color: sk.ink.select.alpha(0.85),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(1.),
                    inset: true,
                }])
            })
            .child(
                div()
                    .flex_none()
                    .w(px(9. * s))
                    .text_size(sk.text_px(9. * s))
                    .text_color(sk.ink.ink_faint)
                    .child(if dir {
                        ""
                    } else if open {
                        "\u{25be}"
                    } else {
                        "\u{25b8}"
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(11. * s))
                    .text_size(sk.text_px(11. * s))
                    .text_color(mark_ink)
                    .child(mark),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(sk.text_px(11.5 * s))
                    .text_color(th.text)
                    .child(path_text),
            )
            .when_some(file.old_path.clone(), |d, old| {
                d.child(
                    div()
                        .flex_none()
                        .max_w(px(140. * s))
                        .truncate()
                        .text_size(sk.text_px(9.5 * s))
                        .text_color(sk.ink.ink_faint)
                        .child(format!("\u{2190} {old}")),
                )
            })
            .child(counts)
            .when_some(
                file_path.filter(|_| !dir && file.status != Status::Deleted),
                |d, p| {
                    d.child(with_tip(
                        div()
                            .id(SharedString::from(format!("flyout-file-open-{i}")))
                            .flex_none()
                            .px(px(3. * s))
                            .rounded(sk.radius())
                            .text_size(sk.text_px(10. * s))
                            .text_color(sk.ink.ink_faint)
                            .hover(|st| st.text_color(th.accent).bg(sk.ink.hover))
                            .child("\u{2197}")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    let task = ws.active;
                                    ws.flyout_show(
                                        task,
                                        Target::File { path: p.clone() },
                                        None,
                                        true,
                                        window,
                                        cx,
                                    );
                                }),
                            ),
                        vec![f.open_file.into()],
                    ))
                },
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, ev: &MouseDownEvent, window, cx| {
                    if ev.click_count >= 2 {
                        if let Some(p) = open_owner_file.clone() {
                            let task = ws.active;
                            ws.flyout_show(task, Target::File { path: p }, None, true, window, cx);
                            return;
                        }
                    }
                    if !dir {
                        ws.diff_toggle(&toggle_owner, &path, cx);
                    }
                }),
            );
        let mut out = div().flex().flex_col().child(row);
        if open && !dir {
            out = out.child(match patch {
                Patch::Unread | Patch::Busy => {
                    note(f.reading, sk.ink.ink_faint, sk, s).into_any_element()
                }
                Patch::Failed(e) => note(e, ink.danger, sk, s).into_any_element(),
                Patch::Got(d) if d.binary => {
                    note(f.file_binary, ink.faint, sk, s).into_any_element()
                }
                Patch::Got(d) => self.diff_hunks(owner, &file.path, d, full, ink, th, sk, s, cx),
            });
        }
        out.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn diff_hunks(
        &self,
        owner: &DiffOwner,
        path: &str,
        d: &FileDiff,
        full: bool,
        ink: &Ink,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let f = self.lang.strings().flyout;
        let sy = flyout::syntax_for(Path::new(path));
        let total: usize = d.hunks.iter().map(|h| h.lines.len() + 1).sum();
        let cap = if full { usize::MAX } else { PATCH_ROWS };
        let code_px = sk.text_px(11. * s);
        let row_h = f32::from(code_px) * 1.5;
        let digits = d
            .hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter_map(|l| l.new.or(l.old))
            .max()
            .unwrap_or(1)
            .to_string()
            .len()
            .max(2) as f32;
        let num_w = px(digits * f32::from(code_px) * 0.62 + 6. * s);
        let font = SharedString::from(th.font_family.clone());
        let mut block = div()
            .flex()
            .flex_col()
            .mx(px(6. * s))
            .mt(px(2. * s))
            .mb(px(8. * s))
            .rounded(sk.radius())
            .border_1()
            .border_color(sk.ink.rule)
            .overflow_hidden()
            .bg(darken(th.bg, 0.25).alpha(0.9))
            .font_family(font)
            .text_size(code_px);
        let mut drawn = 0usize;
        'hunks: for h in &d.hunks {
            if drawn >= cap {
                break;
            }
            block = block.child(
                div()
                    .h(px(row_h))
                    .flex()
                    .items_center()
                    .px(px(8. * s))
                    .bg(sk.ink.mark_wash)
                    .text_color(sk.ink.ink_dim)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(format!(
                        "@@ \u{2212}{} +{} @@ {}",
                        h.old_start, h.new_start, h.section
                    )),
            );
            drawn += 1;
            let mut inside = false;
            for l in &h.lines {
                if drawn >= cap {
                    break 'hunks;
                }
                drawn += 1;
                let (bg, sign, sign_ink) = match l.kind {
                    LineKind::Added => (Some(ink.ok.alpha(0.12)), "+", ink.ok),
                    LineKind::Removed => (Some(ink.danger.alpha(0.12)), "\u{2212}", ink.danger),
                    LineKind::Context => (None, " ", ink.ghost),
                    LineKind::NoNewline => (None, "", ink.faint),
                };
                let num = |n: Option<u32>| {
                    div()
                        .flex_none()
                        .w(num_w)
                        .flex()
                        .justify_end()
                        .pr(px(4. * s))
                        .text_color(ink.ghost)
                        .child(n.map(|n| n.to_string()).unwrap_or_default())
                };
                let code: AnyElement = if l.kind == LineKind::NoNewline {
                    div()
                        .text_color(ink.faint)
                        .child(l.text.clone())
                        .into_any_element()
                } else {
                    code_text(&l.text, sy, &mut inside, ink).into_any_element()
                };
                block = block.child(
                    div()
                        .h(px(row_h))
                        .flex()
                        .flex_row()
                        .items_center()
                        .when_some(bg, |d, b| d.bg(b))
                        .child(num(l.old))
                        .child(num(l.new))
                        .child(
                            div()
                                .flex_none()
                                .w(px(14. * s))
                                .text_color(sign_ink)
                                .child(sign),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_color(ink.text)
                                .child(code),
                        ),
                );
            }
        }
        if total > drawn {
            let more = owner.clone();
            let p = path.to_string();
            block = block.child(
                div()
                    .id(SharedString::from(format!("flyout-more-{path}")))
                    .px(px(10. * s))
                    .py(px(4. * s))
                    .cursor_pointer()
                    .text_color(th.accent)
                    .hover(|st| st.bg(sk.ink.hover))
                    .child(lang::fill(
                        f.more_lines,
                        &[("count", &(total - drawn).to_string())],
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            ws.diff_more(&more, &p, cx)
                        }),
                    ),
            );
        }
        block.into_any_element()
    }

    // ---- the File tab ----------------------------------------------------

    fn file_body(
        &self,
        path: &Path,
        fl: &FileLive,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let f = self.lang.strings().flyout;
        let ink = Ink::of(th, sk);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let folder = path
            .parent()
            .map(|p| {
                let home = crate::session::home_dir().display().to_string();
                let shown = p.display().to_string();
                match shown.strip_prefix(&home) {
                    Some(rest) => format!("~{rest}"),
                    None => shown,
                }
            })
            .unwrap_or_default();
        let text = fl.text.got.as_ref();
        let changed = fl.marks.len();
        let head = div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(2. * s))
            .px(px(10. * s))
            .pt(px(8. * s))
            .pb(px(6. * s))
            .border_b_1()
            .border_color(sk.ink.rule)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8. * s))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(sk.text_px(12.5 * s))
                            .text_color(th.text)
                            .child(name),
                    )
                    .child(div().flex_1())
                    .when_some(text, |d, t| {
                        d.child(small(
                            lang::fill(f.lines, &[("count", &t.lines.len().to_string())]),
                            ink.faint,
                            sk,
                            s,
                        ))
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8. * s))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(sk.text_px(9.5 * s))
                            .text_color(sk.ink.ink_faint)
                            .child(folder),
                    )
                    .child(div().flex_1())
                    .when(text.is_some_and(|t| t.untracked), |d| {
                        d.child(small(f.file_new, ink.ok, sk, s))
                    })
                    .when(text.is_some_and(|t| !t.untracked) && changed > 0, |d| {
                        d.child(small(
                            lang::fill(f.changed_lines, &[("count", &changed.to_string())]),
                            ink.warn,
                            sk,
                            s,
                        ))
                    }),
            );
        let mut notes: Vec<AnyElement> = Vec::new();
        if let Some(e) = &fl.text.error {
            notes.push(note(e, ink.danger, sk, s).into_any_element());
        }
        let Some(t) = text else {
            if fl.text.error.is_none() {
                notes.push(note(f.reading, ink.faint, sk, s).into_any_element());
            }
            return div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(head)
                .children(notes)
                .into_any_element();
        };
        if t.binary {
            notes.push(note(f.file_binary, ink.faint, sk, s).into_any_element());
        }
        if t.cut {
            notes.push(
                note(
                    &lang::fill(f.cut, &[("size", &human_size(FILE_CAP))]),
                    ink.warn,
                    sk,
                    s,
                )
                .into_any_element(),
            );
        }
        let code_px = sk.text_px(11. * s);
        let row_h = f32::from(code_px) * 1.5;
        let digits = t.lines.len().max(1).to_string().len().max(3) as f32;
        let num_w = digits * f32::from(code_px) * 0.62 + 8. * s;
        let lines = t.lines.clone();
        let starts = t.starts.clone();
        let sy = t.syntax;
        let marks = fl.marks.clone();
        let lit = fl.lit;
        let binary = t.binary;
        let font = SharedString::from(th.font_family.clone());
        let list = uniform_list(
            "flyout-file-lines",
            if binary { 0 } else { lines.len() },
            move |range, _w, _cx| {
                range
                    .map(|i| {
                        let n = i as u32 + 1;
                        let mut inside = starts.get(i).copied().unwrap_or(false);
                        let mark = marks.get(&n).copied();
                        let bar = match mark {
                            Some(Mark::Added) => Some(ink.ok),
                            Some(Mark::Modified) => Some(ink.warn),
                            Some(Mark::RemovedAbove) | None => None,
                        };
                        div()
                            .h(px(row_h))
                            .flex()
                            .flex_row()
                            .items_center()
                            .when(lit == Some(n), |d| d.bg(ink.lit))
                            .child(
                                div()
                                    .flex_none()
                                    .w(px(3. * s))
                                    .h_full()
                                    .flex()
                                    .flex_col()
                                    .when_some(bar, |d, c| d.bg(c))
                                    .when(mark == Some(Mark::RemovedAbove), |d| {
                                        d.child(div().h(px(3. * s)).w_full().bg(ink.danger))
                                    }),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .w(px(num_w))
                                    .flex()
                                    .justify_end()
                                    .pr(px(8. * s))
                                    .text_color(if mark.is_some() { ink.faint } else { ink.ghost })
                                    .child(n.to_string()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_color(ink.text)
                                    .child(code_text(&lines[i], sy, &mut inside, &ink)),
                            )
                    })
                    .collect::<Vec<_>>()
            },
        )
        .track_scroll(&fl.scroll)
        .size_full();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(head)
            .children(notes)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .font_family(font)
                    .text_size(code_px)
                    .bg(darken(th.bg, 0.25).alpha(0.9))
                    .child(list),
            )
            .into_any_element()
    }

    // ---- the Pull request tab --------------------------------------------

    fn pull_body(
        &self,
        key: &Target,
        p: &PullLive,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let f = self.lang.strings().flyout;
        let l = self.lang;
        let ink = Ink::of(th, sk);
        let Target::Pull {
            owner,
            repo,
            number,
        } = key
        else {
            return div().into_any_element();
        };
        let url = key.url().unwrap_or_default();
        let pull = p.pull.got.as_ref();
        let (state_word, state_ink) = match pull.map(|x| x.state) {
            Some(PullState::Open) => (f.pr_open, ink.ok),
            Some(PullState::Draft) => (f.pr_draft, sk.ink.ink_dim),
            Some(PullState::Merged) => (f.pr_merged, th.ansi[13]),
            Some(PullState::Closed) => (f.pr_closed, ink.danger),
            Some(PullState::Unknown) | None => (f.pr_unknown, ink.faint),
        };
        let open_url = url.clone();
        let head = div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(5. * s))
            .px(px(12. * s))
            .pt(px(9. * s))
            .pb(px(7. * s))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8. * s))
                    .child(
                        div()
                            .id("flyout-pr-link")
                            .min_w_0()
                            .truncate()
                            .text_size(sk.text_px(10.5 * s))
                            .text_color(sk.ink.ink_dim)
                            .cursor_pointer()
                            .hover(|st| st.text_color(th.accent))
                            .child(format!("{owner}/{repo} #{number} \u{2197}"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |_ws, _: &MouseDownEvent, _w, _cx| {
                                    crate::pane::open_with_system(&open_url);
                                }),
                            ),
                    )
                    .child(div().flex_1())
                    .when(pull.is_some(), |d| {
                        d.child(
                            div()
                                .flex_none()
                                .px(px(7. * s))
                                .py(px(1. * s))
                                .rounded(sk.radius_pill())
                                .border_1()
                                .border_color(state_ink.alpha(0.7))
                                .bg(state_ink.alpha(0.14))
                                .text_size(sk.text_px(10. * s))
                                .text_color(state_ink)
                                .child(state_word),
                        )
                    }),
            )
            .when_some(pull, |d, x| {
                d.child(
                    div()
                        .text_size(sk.text_px(14.5 * s))
                        .text_color(th.text)
                        .child(x.title.clone()),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap(px(6. * s))
                        .text_size(sk.text_px(10. * s))
                        .text_color(sk.ink.ink_faint)
                        .when_some(x.author.clone(), |d, a| {
                            d.child(div().text_color(sk.ink.ink_dim).child(a))
                        })
                        .child(lang::fill(
                            f.pr_updated,
                            &[("when", &when_label(x.updated_ms, l))],
                        )),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap(px(6. * s))
                        .child(branch_pill(x.base.clone().unwrap_or_default(), th, sk, s))
                        .child(small("\u{2190}", ink.faint, sk, s))
                        .child(branch_pill(x.head.clone().unwrap_or_default(), th, sk, s))
                        .child(div().flex_1())
                        .when_some(x.changed_files, |d, n| {
                            d.child(small(format!("\u{25a4} {n}"), ink.faint, sk, s))
                        })
                        .when_some(x.additions, |d, n| {
                            d.child(small(format!("+{n}"), ink.ok, sk, s))
                        })
                        .when_some(x.deletions, |d, n| {
                            d.child(small(format!("\u{2212}{n}"), ink.danger, sk, s))
                        }),
                )
                .child(self.pull_actions(x, &url, th, sk, s, cx))
            });
        let mut notes: Vec<AnyElement> = Vec::new();
        if let Some(e) = &p.pull.error {
            notes.push(note(e, ink.danger, sk, s).into_any_element());
        }
        if pull.is_none() && p.pull.error.is_none() {
            notes.push(note(f.reading, ink.faint, sk, s).into_any_element());
        }
        let faces = div()
            .flex_none()
            .flex()
            .flex_row()
            .gap(px(3. * s))
            .px(px(10. * s))
            .pb(px(6. * s))
            .border_b_1()
            .border_color(sk.ink.rule)
            .children(
                [
                    (PullFace::Overview, f.pr_overview.to_string()),
                    (
                        PullFace::Files,
                        match pull.and_then(|x| x.changed_files) {
                            Some(n) => format!("{} {n}", f.pr_files),
                            None => f.pr_files.to_string(),
                        },
                    ),
                    (
                        PullFace::Conversation,
                        match pull.map(|x| x.remarks.len()).filter(|n| *n > 0) {
                            Some(n) => format!("{} {n}", f.pr_conversation),
                            None => f.pr_conversation.to_string(),
                        },
                    ),
                ]
                .into_iter()
                .map(|(face, word)| {
                    div()
                        .id(SharedString::from(format!("flyout-face-{face:?}")))
                        .debug_selector(move || format!("flyout-face-{face:?}"))
                        .px(px(9. * s))
                        .py(px(3. * s))
                        .map(|d| sk.choice(d, p.face == face))
                        .text_size(sk.text_px(10.5 * s))
                        .cursor_pointer()
                        .child(word)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                ws.pull_face(face, cx)
                            }),
                        )
                }),
            );
        let content: AnyElement = match (p.face, pull) {
            (PullFace::Files, _) => self.pull_files(key, p, &ink, th, sk, s, cx),
            (PullFace::Overview, Some(x)) => self.pull_overview(x, p, &ink, th, sk, s, cx),
            (PullFace::Conversation, Some(x)) => self.pull_conversation(x, p, &ink, th, sk, s),
            (_, None) => div().into_any_element(),
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(head)
            .children(notes)
            .child(faces)
            .child(content)
            .into_any_element()
    }

    fn pull_actions(
        &self,
        x: &flyout::Pull,
        url: &str,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let f = self.lang.strings().flyout;
        let ink = Ink::of(th, sk);
        let open = url.to_string();
        let checkout = format!("gh pr checkout {}", x.number);
        let copied = self
            .flyout
            .copied
            .is_some_and(|t| t.elapsed() < Duration::from_secs(2));
        let review = x.review.as_deref().map(|r| match r {
            "APPROVED" => (f.pr_approved, ink.ok),
            "CHANGES_REQUESTED" => (f.pr_changes_requested, ink.danger),
            _ => (f.pr_review_required, ink.warn),
        });
        let action = |id: &'static str, word: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .px(px(8. * s))
                .py(px(3. * s))
                .rounded(sk.radius())
                .border_1()
                .border_color(sk.ink.edge)
                .text_size(sk.text_px(10. * s))
                .text_color(sk.ink.ink_dim)
                .cursor_pointer()
                .hover(|st| st.bg(sk.ink.hover).text_color(th.text))
                .child(word)
        };
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(6. * s))
            .pt(px(2. * s))
            .child(action("flyout-pr-github", f.pr_on_github).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_ws, _: &MouseDownEvent, _w, _cx| {
                    crate::pane::open_with_system(&open);
                }),
            ))
            .child(
                action(
                    "flyout-pr-checkout",
                    if copied {
                        f.pr_copied
                    } else {
                        f.pr_copy_checkout
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(checkout.clone()));
                        ws.flyout.copied = Some(Instant::now());
                        cx.notify();
                    }),
                ),
            )
            .child(div().flex_1())
            .when_some(review, |d, (word, c)| d.child(small(word, c, sk, s)))
            .when(x.mergeable.as_deref() == Some("CONFLICTING"), |d| {
                d.child(small(f.pr_conflicts, ink.danger, sk, s))
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn pull_overview(
        &self,
        x: &flyout::Pull,
        p: &PullLive,
        ink: &Ink,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let f = self.lang.strings().flyout;
        let heading = |w: &'static str| {
            div()
                .pt(px(10. * s))
                .pb(px(4. * s))
                .text_size(sk.text_px(10. * s))
                .text_color(sk.ink.ink_faint)
                .child(w.to_uppercase())
        };
        let mut col = div()
            .id("flyout-pr-overview")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&p.scroll)
            .flex()
            .flex_col()
            .px(px(12. * s))
            .pb(px(14. * s));
        col = col.child(heading(f.pr_checks));
        if x.checks.is_empty() {
            col = col.child(small(f.pr_no_checks, ink.faint, sk, s));
        }
        for (i, c) in x.checks.iter().enumerate() {
            let (glyph, c_ink) = match c.state {
                CheckState::Passed => ("\u{2713}", ink.ok),
                CheckState::Failed => ("\u{2717}", ink.danger),
                CheckState::Running => ("\u{25cf}", ink.warn),
                CheckState::Skipped => ("\u{2013}", ink.faint),
                CheckState::Neutral => ("\u{25cb}", ink.faint),
                CheckState::Unknown => ("?", ink.faint),
            };
            let target = c.url.clone();
            col = col.child(
                div()
                    .id(SharedString::from(format!("flyout-check-{i}")))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(7. * s))
                    .py(px(2. * s))
                    .rounded(sk.radius())
                    .when(target.is_some(), |d| {
                        d.cursor_pointer().hover(|st| st.bg(sk.ink.hover))
                    })
                    .child(
                        div()
                            .flex_none()
                            .w(px(12. * s))
                            .text_color(c_ink)
                            .child(glyph),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(sk.text_px(11. * s))
                            .text_color(th.text)
                            .child(c.name.clone()),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_ws, _: &MouseDownEvent, _w, _cx| {
                            if let Some(u) = &target {
                                crate::pane::open_with_system(u);
                            }
                        }),
                    ),
            );
        }
        let names = |v: &[String]| {
            if v.is_empty() {
                f.pr_none.to_string()
            } else {
                v.join(", ")
            }
        };
        col = col
            .child(heading(f.pr_reviewers))
            .child(small(names(&x.reviewers), sk.ink.ink_dim, sk, s))
            .child(heading(f.pr_labels))
            .child(if x.labels.is_empty() {
                small(f.pr_none, sk.ink.ink_dim, sk, s).into_any_element()
            } else {
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(5. * s))
                    .children(x.labels.iter().map(|label| {
                        div()
                            .px(px(7. * s))
                            .py(px(1. * s))
                            .rounded(sk.radius_pill())
                            .bg(sk.ink.mark_wash)
                            .text_size(sk.text_px(10. * s))
                            .text_color(sk.ink.ink_dim)
                            .child(label.clone())
                    }))
                    .into_any_element()
            })
            .child(heading(f.pr_description));
        if x.body.trim().is_empty() {
            col = col.child(small(f.pr_no_description, ink.faint, sk, s));
        } else {
            let doc = crate::docview::markdown::parsed(&x.body);
            let style = crate::docview::markdown::MdStyle::bench(sk, th);
            col = col.child(crate::docview::markdown::document(
                &doc, &style, None, None, None,
            ));
        }
        col.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn pull_conversation(
        &self,
        x: &flyout::Pull,
        p: &PullLive,
        ink: &Ink,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
    ) -> AnyElement {
        let f = self.lang.strings().flyout;
        let l = self.lang;
        let mut col = div()
            .id("flyout-pr-conversation")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&p.scroll)
            .flex()
            .flex_col()
            .gap(px(8. * s))
            .px(px(12. * s))
            .py(px(10. * s));
        if x.remarks.is_empty() {
            col = col.child(small(f.pr_no_comments, ink.faint, sk, s));
        }
        for r in &x.remarks {
            let verdict = r.verdict.as_deref().map(|v| match v {
                "APPROVED" => (f.pr_approved, ink.ok),
                "CHANGES_REQUESTED" => (f.pr_changes_requested, ink.danger),
                _ => (f.pr_review_required, ink.warn),
            });
            let style = crate::docview::markdown::MdStyle::bench(sk, th);
            col = col.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4. * s))
                    .p(px(8. * s))
                    .rounded(sk.radius())
                    .border_1()
                    .border_color(sk.ink.rule)
                    .bg(sk.ink.ink.alpha(0.03))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(7. * s))
                            .child(
                                div()
                                    .text_size(sk.text_px(11. * s))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(th.text)
                                    .child(r.author.clone()),
                            )
                            .child(small(when_label(r.at_ms, l), ink.faint, sk, s))
                            .child(div().flex_1())
                            .when_some(verdict, |d, (w, c)| d.child(small(w, c, sk, s))),
                    )
                    .when(!r.body.trim().is_empty(), |d| {
                        d.child(crate::docview::markdown::document(
                            &crate::docview::markdown::parsed(&r.body),
                            &style,
                            None,
                            None,
                            None,
                        ))
                    }),
            );
        }
        col.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn pull_files(
        &self,
        key: &Target,
        p: &PullLive,
        ink: &Ink,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let f = self.lang.strings().flyout;
        let owner = DiffOwner::Pull(key.clone());
        let mut col = div()
            .id("flyout-pr-files")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&p.scroll)
            .flex()
            .flex_col()
            .px(px(4. * s))
            .py(px(6. * s));
        match (&p.diff.got, &p.diff.error) {
            (Some(diffs), _) => {
                for (i, d) in diffs.iter().enumerate() {
                    let stat = d.stat();
                    col = col.child(self.diff_file(
                        &owner,
                        i,
                        &stat,
                        None,
                        Patch::Got(d),
                        p.open.contains(d.path()),
                        p.full.contains(d.path()),
                        i == p.cursor && self.flyout.focused,
                        ink,
                        th,
                        sk,
                        s,
                        cx,
                    ));
                }
            }
            (None, Some(e)) => col = col.child(note(e, ink.danger, sk, s)),
            (None, None) => col = col.child(note(f.reading, ink.faint, sk, s)),
        }
        col.into_any_element()
    }

    // ---- the + page ------------------------------------------------------

    fn add_page(
        &self,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let f = self.lang.strings().flyout;
        let ink = Ink::of(th, sk);
        let rows = self.add_rows();
        let here = self.flyout.here.clone();
        let suggest = here.as_ref().and_then(|h| self.flyout.suggest.get(h));
        let got = suggest.and_then(|r| r.got.as_ref());
        let query = self.flyout.query.clone();
        let caret = if self.flyout.focused { "\u{258f}" } else { "" };
        let field = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6. * s))
            .mx(px(10. * s))
            .mb(px(6. * s))
            .px(px(8. * s))
            .py(px(5. * s))
            .rounded(sk.radius())
            .border_1()
            .border_color(if self.flyout.focused {
                th.accent.alpha(0.6)
            } else {
                sk.ink.rule
            })
            .bg(sk.ink.panel_sunken)
            .text_size(sk.text_px(11.5 * s))
            .child(div().text_color(sk.ink.ink_faint).child("\u{2315}"))
            .child(if query.is_empty() {
                div()
                    .text_color(sk.ink.ink_ghost)
                    .child(format!("{caret}{}", f.add_filter))
            } else {
                div().text_color(th.text).child(format!("{query}{caret}"))
            });
        let mut list = div()
            .id("flyout-add")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .px(px(6. * s))
            .pb(px(10. * s));
        if here.is_none() {
            list = list.child(note(f.add_no_repo, ink.faint, sk, s));
        }
        let section = |w: &str| {
            div()
                .px(px(6. * s))
                .pt(px(10. * s))
                .pb(px(3. * s))
                .text_size(sk.text_px(9.5 * s))
                .text_color(sk.ink.ink_faint)
                .child(w.to_uppercase())
        };
        let mut last: Option<&'static str> = None;
        for (i, row) in rows.iter().enumerate() {
            let kind = match row {
                AddRow::Changes(_) => "c",
                AddRow::Pull { .. } => "p",
                AddRow::File(..) => "f",
            };
            if last != Some(kind) {
                match kind {
                    "p" => list = list.child(section(f.add_pulls)),
                    "f" => list = list.child(section(f.add_files)),
                    _ => {}
                }
                last = Some(kind);
            }
            let lit = i == self.flyout.pick;
            let (glyph, glyph_ink, title, detail): (&str, Hsla, String, String) = match row {
                AddRow::Changes(repo) => (
                    "\u{b1}",
                    ink.ok,
                    lang::fill(
                        f.add_changes,
                        &[(
                            "repo",
                            &repo
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                        )],
                    ),
                    match got.and_then(|g| g.branch.as_deref()) {
                        Some(b) => format!("{b} \u{b7} {}", f.add_changes_sub),
                        None => f.add_changes_sub.to_string(),
                    },
                ),
                AddRow::Pull { row, mine, .. } => (
                    "\u{21c4}",
                    th.ansi[13],
                    format!("#{} {}", row.number, row.title),
                    if *mine {
                        f.add_branch_pull.to_string()
                    } else {
                        [
                            row.author.clone().unwrap_or_default(),
                            row.head.clone().unwrap_or_default(),
                            if row.updated_ms.is_some() {
                                when_label(row.updated_ms, self.lang)
                            } else {
                                String::new()
                            },
                        ]
                        .into_iter()
                        .filter(|w| !w.is_empty())
                        .collect::<Vec<_>>()
                        .join(" \u{b7} ")
                    },
                ),
                AddRow::File(_, rel) => (
                    "\u{25a4}",
                    sk.ink.ink_dim,
                    rel.rsplit('/').next().unwrap_or(rel).to_string(),
                    rel.clone(),
                ),
            };
            let open_row = row.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("flyout-add-{i}")))
                    .debug_selector(move || format!("flyout-add-{i}"))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8. * s))
                    .px(px(8. * s))
                    .py(px(5. * s))
                    .rounded(sk.radius())
                    .cursor_pointer()
                    .hover(|st| st.bg(sk.ink.hover))
                    .when(lit, |d| d.bg(sk.ink.row_active))
                    .child(
                        div()
                            .flex_none()
                            .w(px(14. * s))
                            .text_size(sk.text_px(12. * s))
                            .text_color(glyph_ink)
                            .child(glyph),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(sk.text_px(11.5 * s))
                                    .text_color(th.text)
                                    .child(title),
                            )
                            .when(!detail.is_empty(), |d| {
                                d.child(
                                    div()
                                        .truncate()
                                        .text_size(sk.text_px(9.5 * s))
                                        .text_color(sk.ink.ink_faint)
                                        .child(detail),
                                )
                            }),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                            ws.add_open(open_row.clone(), window, cx);
                        }),
                    ),
            );
        }
        if !query.is_empty() && !rows.iter().any(|r| matches!(r, AddRow::File(..))) && got.is_some()
        {
            list = list.child(note(f.add_no_files, ink.faint, sk, s));
        }
        if query.is_empty() {
            if let Some(g) = got {
                if g.pulls.is_empty() && g.branch_pull.is_none() {
                    list = list.child(section(f.add_pulls)).child(note(
                        g.pulls_note.as_deref().unwrap_or(f.add_no_pulls),
                        ink.faint,
                        sk,
                        s,
                    ));
                }
            } else if here.is_some() {
                list = list.child(note(f.reading, ink.faint, sk, s));
            }
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(14. * s))
                    .pt(px(10. * s))
                    .pb(px(8. * s))
                    .text_size(sk.text_px(13. * s))
                    .text_color(th.text)
                    .child(f.add_title),
            )
            .child(field)
            .child(list)
            .child(
                div()
                    .flex_none()
                    .px(px(14. * s))
                    .pb(px(8. * s))
                    .text_size(sk.text_px(9.5 * s))
                    .text_color(sk.ink.ink_faint)
                    .child(f.add_hint),
            )
            .into_any_element()
    }
}

/// The first line a virtual list shows, from its offset and its row height:
/// gpui keeps its own reading of this for tests only.
fn list_top(h: &UniformListScrollHandle) -> usize {
    let st = h.0.borrow();
    let row = st
        .last_item_size
        .map(|s| f32::from(s.item.height))
        .filter(|h| *h > 0.)
        .unwrap_or(18.);
    (-f32::from(st.base_handle.offset().y) / row)
        .max(0.)
        .floor() as usize
}

/// A line of quiet text: a note, an error, an empty state.
fn note(text: &str, ink: Hsla, sk: &skin::Skin, s: f32) -> gpui::Div {
    div()
        .px(px(12. * s))
        .py(px(6. * s))
        .text_size(sk.text_px(10.5 * s))
        .text_color(ink)
        .child(text.to_string())
}

fn small(text: impl Into<SharedString>, ink: Hsla, sk: &skin::Skin, s: f32) -> gpui::Div {
    div()
        .flex_none()
        .text_size(sk.text_px(10. * s))
        .text_color(ink)
        .child(text.into())
}

fn branch_pill(name: String, th: &theme::Theme, sk: &skin::Skin, s: f32) -> gpui::Div {
    div()
        .flex_none()
        .max_w(px(180. * s))
        .truncate()
        .px(px(6. * s))
        .py(px(1. * s))
        .rounded(sk.radius())
        .bg(sk.ink.panel_sunken)
        .font_family(SharedString::from(th.font_family.clone()))
        .text_size(sk.text_px(10. * s))
        .text_color(sk.ink.ink_dim)
        .child(name)
}

fn icon_button(
    id: &'static str,
    glyph: &'static str,
    lit: bool,
    sk: &skin::Skin,
    s: f32,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex_none()
        .px(px(5. * s))
        .rounded(sk.radius())
        .text_size(sk.text_px(12. * s))
        .text_color(if lit { sk.ink.mark } else { sk.ink.ink_dim })
        .cursor_pointer()
        .hover(|st| st.bg(sk.ink.hover))
        .child(glyph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A scratch repository, removed when the test ends.
    struct Repo {
        root: PathBuf,
    }

    impl Repo {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "td-flyout-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            let r = Self { root };
            r.git(&["init", "-q", "-b", "main"]);
            r.git(&["config", "user.email", "fixture@example.invalid"]);
            r.git(&["config", "user.name", "Fixture"]);
            r
        }
        fn git(&self, args: &[&str]) {
            let out = Command::new("git")
                .args(args)
                .current_dir(&self.root)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        fn write(&self, rel: &str, text: &str) {
            let p = self.root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        fn commit(&self) {
            self.git(&["add", "--all"]);
            self.git(&["commit", "-qm", "fixture"]);
        }
    }

    impl Drop for Repo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn stat<'a>(list: &'a ChangeList, path: &str) -> &'a FileStat {
        list.files
            .iter()
            .find(|f| f.path == path)
            .unwrap_or_else(|| panic!("{path} not listed: {:?}", list.files))
    }

    #[test]
    fn uncommitted_changes_are_read_with_new_files_counted_and_new_folders_named() {
        let _fork = crate::testsync::forks_and_locks();
        let r = Repo::new();
        r.write("src/a.rs", "fn one() {}\nfn two() {}\n");
        r.write("keep.txt", "x\n");
        r.commit();
        r.write(
            "src/a.rs",
            "fn one() {}\nfn two() -> u8 { 2 }\nfn three() {}\n",
        );
        r.git(&["rm", "-q", "keep.txt"]);
        r.write("notes.txt", "a\nb\nc\n");
        r.write("fresh/inner.txt", "1\n");
        let list = read_change_list(&r.root, Base::Uncommitted).unwrap();
        assert_eq!(list.against, "HEAD");
        assert_eq!(list.branch.as_deref(), Some("main"));
        let a = stat(&list, "src/a.rs");
        assert_eq!(
            (a.status, a.added, a.removed),
            (Status::Modified, Some(2), Some(1))
        );
        let gone = stat(&list, "keep.txt");
        assert_eq!((gone.status, gone.removed), (Status::Deleted, Some(1)));
        let notes = stat(&list, "notes.txt");
        assert_eq!((notes.status, notes.added), (Status::Untracked, Some(3)));
        let folder = stat(&list, "fresh/");
        assert_eq!(
            folder.added, None,
            "an untracked folder's lines are not counted"
        );

        let patch = read_patch(&r.root, &list.rev, a).unwrap();
        assert_eq!(patch.counts(), (2, 1));
        assert_eq!(patch.path(), "src/a.rs");
        let added = read_patch(&r.root, &list.rev, notes).unwrap();
        assert_eq!(
            added.counts(),
            (3, 0),
            "a new file is shown whole, every line added"
        );
    }

    #[test]
    fn the_branch_base_reads_everything_since_the_default_branch() {
        let _fork = crate::testsync::forks_and_locks();
        let r = Repo::new();
        r.write("a.txt", "one\n");
        r.commit();
        r.git(&["checkout", "-q", "-b", "feature"]);
        r.write("b.txt", "committed on the branch\n");
        r.commit();
        r.write("a.txt", "one\ntwo\n");
        let branch = read_change_list(&r.root, Base::Branch).unwrap();
        assert_eq!(branch.against, "main");
        assert_eq!(branch.branch.as_deref(), Some("feature"));
        assert!(
            branch.files.iter().any(|f| f.path == "b.txt"),
            "the branch's commit"
        );
        assert!(
            branch.files.iter().any(|f| f.path == "a.txt"),
            "and what is not committed"
        );
        let uncommitted = read_change_list(&r.root, Base::Uncommitted).unwrap();
        assert_eq!(
            uncommitted
                .files
                .iter()
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>(),
            ["a.txt"]
        );
    }

    #[test]
    fn a_repository_with_no_commits_counts_everything_as_new() {
        let _fork = crate::testsync::forks_and_locks();
        let r = Repo::new();
        r.write("first.txt", "hello\n");
        let list = read_change_list(&r.root, Base::Branch).unwrap();
        assert_eq!(list.against, "\u{2205}");
        assert_eq!(stat(&list, "first.txt").status, Status::Untracked);
    }

    #[test]
    fn a_folder_outside_any_repository_says_so() {
        let dir = std::env::temp_dir().join(format!("td-flyout-bare-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Only meaningful when the temp dir itself is not inside a checkout.
        if git_root(&dir).is_none() {
            assert!(read_change_list(&dir, Base::Uncommitted).is_err());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_is_read_with_its_changed_lines_marked_and_not_read_again_unchanged() {
        let _fork = crate::testsync::forks_and_locks();
        let r = Repo::new();
        r.write("lib.rs", "fn a() {}\nfn b() {}\nfn c() {}\n");
        r.commit();
        r.write(
            "lib.rs",
            "fn a() {}\nfn b() -> u8 { 1 }\nfn c() {}\nfn d() {}\n",
        );
        let path = r.root.join("lib.rs");
        let (text, marks, stamp) = read_file(&path, None, true).unwrap().expect("read");
        assert_eq!(text.lines.len(), 4);
        assert!(text.syntax.is_some());
        assert!(!text.untracked);
        assert_eq!(marks.get(&2), Some(&Mark::Modified));
        assert_eq!(marks.get(&4), Some(&Mark::Added));
        assert_eq!(marks.get(&1), None);
        assert!(
            read_file(&path, Some(stamp), false).unwrap().is_none(),
            "nothing moved on disk, so nothing is read"
        );
        r.write("new.rs", "x\ny\n");
        let (fresh, marks, _) = read_file(&r.root.join("new.rs"), None, true)
            .unwrap()
            .unwrap();
        assert!(fresh.untracked);
        assert_eq!(
            marks.len(),
            2,
            "every line of a file git does not track is new"
        );
        assert!(read_file(&r.root.join("missing.rs"), None, true).is_err());
    }

    #[test]
    fn a_click_finds_a_code_file_at_its_line_and_leaves_documents_to_the_square() {
        let r = Repo::new();
        r.write("src/main.rs", "fn main() {}\n// two\n");
        r.write("README.md", "# doc\n");
        std::fs::write(r.root.join("blob.bin"), b"\x00\x01\x02").unwrap();
        let root = r.root.to_string_lossy().into_owned();
        let open = side_of("src/main.rs:2", Some(&root)).expect("a code file");
        assert_eq!(open.line, Some(2));
        assert_eq!(
            open.target,
            Target::File {
                path: std::fs::canonicalize(r.root.join("src/main.rs")).unwrap()
            }
        );
        // From a folder below the repository, a repo-relative path still lands.
        let below = r.root.join("src").to_string_lossy().into_owned();
        assert!(side_of("src/main.rs", Some(&below)).is_some());
        assert_eq!(
            side_of("README.md", Some(&root)),
            None,
            "the square draws Markdown"
        );
        assert_eq!(side_of("blob.bin", Some(&root)), None, "not text");
        assert_eq!(side_of("nope.rs", Some(&root)), None);
        let pr = side_of("https://github.com/o/r/pull/9/files", None).expect("a pull request");
        assert_eq!(
            pr.target,
            Target::Pull {
                owner: "o".into(),
                repo: "r".into(),
                number: 9
            }
        );
        assert_eq!(side_of("https://example.com/x.rs", None), None);
    }

    /// The flyout docks beside the panes: they narrow and none is covered.
    /// It takes the keyboard when it opens from the chord, a key it uses is
    /// handled there and reaches no terminal, and Esc gives the keyboard back
    /// to the pane.
    #[gpui::test]
    fn the_flyout_docks_beside_the_panes_takes_the_keyboard_and_gives_it_back(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        cx.executor().allow_parking();
        cx.update(|cx| {
            theme::init_embedded(cx, theme::house_outer());
            skin::init_embedded(cx);
        });
        let seed = session::PaneRestore {
            program: Some(("/bin/sh".into(), vec!["-c".into(), "exec /bin/cat".into()])),
            ..Default::default()
        };
        let window = {
            let _fork = crate::testsync::forks_and_locks();
            cx.open_window(size(px(1400.), px(900.)), |w, cx| {
                Workspace::build(true, true, Some(seed), None, w, cx)
            })
        };
        let ws = window.root(cx).unwrap();
        let cx = gpui::VisualTestContext::from_window(window.into(), cx).into_mut();
        let stops: Vec<_> = ws
            .read_with(cx, |ws, _| ws.every_view())
            .into_iter()
            .map(|p| p.read_with(cx, |v, _| v.fixture_shutdown()))
            .collect();
        cx.on_quit(move || stops.into_iter().for_each(|s| s()));
        let draw = |cx: &mut gpui::VisualTestContext| {
            cx.run_until_parked();
            cx.update(|w, cx| {
                w.refresh();
                w.draw(cx).clear();
            });
            cx.run_until_parked();
        };
        draw(cx);
        let panes = |cx: &mut gpui::VisualTestContext| {
            ws.read_with(cx, |ws, _| {
                ws.pane_bounds
                    .lock()
                    .unwrap()
                    .values()
                    .copied()
                    .collect::<Vec<Bounds<Pixels>>>()
            })
        };
        let before = panes(cx);
        assert!(!before.is_empty());
        let widest = before
            .iter()
            .map(|b| f32::from(b.size.width))
            .fold(0., f32::max);

        let dir = std::env::temp_dir().join(format!("td-flyout-ui-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("plan.rs");
        std::fs::write(&file, "fn one() {}\nfn two() {}\nfn three() {}\n").unwrap();
        cx.update(|w, cx| {
            ws.update(cx, |ws, cx| {
                ws.flyout_toggle(w, cx);
                let task = ws.active;
                ws.flyout_show(
                    task,
                    Target::File { path: file.clone() },
                    Some(2),
                    true,
                    w,
                    cx,
                );
            })
        });
        draw(cx);
        draw(cx);
        let panel = cx.debug_bounds("flyout-panel").expect("the panel is drawn");
        let after = panes(cx);
        for b in &after {
            assert!(
                b.right() <= panel.left() + px(1.),
                "a pane {b:?} runs under the flyout {panel:?}"
            );
        }
        let narrowest_now = after
            .iter()
            .map(|b| f32::from(b.size.width))
            .fold(0., f32::max);
        assert!(narrowest_now < widest, "the panes made room");
        assert!(
            cx.debug_bounds("flyout-chip").is_some(),
            "the chip is on the spine"
        );
        ws.read_with(cx, |ws, _| {
            assert!(ws.flyout.focused);
            let Some(Live::File(f)) = ws.flyout.live.get(&Target::File { path: file.clone() })
            else {
                panic!("the file tab read nothing");
            };
            assert_eq!(f.text.got.as_ref().map(|t| t.lines.len()), Some(3));
        });
        let pane_focused = |cx: &mut gpui::VisualTestContext| {
            cx.update(|w, cx| {
                ws.read(cx)
                    .every_view()
                    .iter()
                    .any(|p| p.focus_handle(cx).is_focused(w))
            })
        };
        assert!(!pane_focused(cx), "no terminal holds the keyboard");

        // `w` is the flyout's: it closes the tab, and no terminal took it.
        cx.simulate_keystrokes("w");
        draw(cx);
        ws.read_with(cx, |ws, _| {
            assert!(
                ws.side().is_some_and(|s| s.tabs.is_empty()),
                "w closed the tab"
            );
        });
        assert!(!pane_focused(cx));

        // Esc gives the keyboard back to the pane; the panel stays open.
        cx.simulate_keystrokes("escape");
        draw(cx);
        ws.read_with(cx, |ws, _| {
            assert!(!ws.flyout.focused);
            assert!(ws.flyout.open);
        });
        assert!(pane_focused(cx), "the pane has the keyboard again");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
