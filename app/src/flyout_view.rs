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
    uniform_list, Bounds, FontStyle, FontWeight, HighlightStyle, ListHorizontalSizingBehavior,
    ScrollStrategy, StyledText, UniformListScrollHandle,
};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;
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
/// Lines of one file's diff drawn before "show more", and how many more each
/// press of it draws.
const PATCH_ROWS: usize = 400;
const MORE_ROWS: usize = 2000;
/// Files a list draws; past it, the count of the rest.
const LIST_ROWS: usize = 1500;
/// Bytes of one line drawn; the rest is past the panel's edge anyway.
const LINE_DRAWN: usize = 1000;
/// Untracked files whose lines are counted on each read; past it, unknown.
const UNTRACKED_COUNTED: usize = 300;
/// Bytes of an untracked file read to count its lines.
const UNTRACKED_READ: u64 = 2 * 1024 * 1024;
/// Files the `+` page's finder keeps from `git ls-files`.
const FINDER_FILES: usize = 30_000;
/// Rows the finder lists.
const FINDER_ROWS: usize = 40;
/// What the `+` page lists of what this task's agents handed over.
const ARTIFACT_ROWS: usize = 8;
/// Places a find in a file counts before it stops counting.
const FIND_HITS: usize = 10_000;
/// The longest GitHub's per-file list may take, every page of it.
const GH_PAGES_BUDGET: Duration = Duration::from_secs(90);
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

/// A list's filter: what is typed, whether keys go to it, and what it leaves.
#[derive(Default)]
pub(crate) struct Filter {
    query: String,
    /// Keys go to the field.
    typing: bool,
    /// The rows that answer, as (index into the list, where its letters
    /// fell), best first. `None` while nothing is typed: every row, in the
    /// list's own order.
    hits: Option<Vec<(usize, Vec<usize>)>>,
}

impl Filter {
    /// Work out what answers, over the list's paths.
    fn apply<'a>(&mut self, paths: impl IntoIterator<Item = &'a str>) {
        self.hits = (!self.query.trim().is_empty()).then(|| {
            flyout::rank_paths(paths, &self.query)
                .into_iter()
                .map(|(i, f)| (i, f.at))
                .collect()
        });
    }

    /// The list's rows in the order drawn: (index into the list, letters lit).
    fn rows(&self, len: usize) -> Vec<(usize, &[usize])> {
        match &self.hits {
            Some(h) => h
                .iter()
                .filter(|(i, _)| *i < len)
                .map(|(i, at)| (*i, at.as_slice()))
                .collect(),
            None => (0..len).map(|i| (i, &[][..])).collect(),
        }
    }
}

/// A find in a File tab's text.
#[derive(Default)]
pub(crate) struct Find {
    query: String,
    typing: bool,
    /// Every place it appears, as (line index, byte range), up to
    /// [`FIND_HITS`], in order: shared with the list that lights them.
    hits: Rc<Vec<(u32, Range<usize>)>>,
    /// The place last stepped to.
    at: usize,
}

/// A File tab's drawn face: the floating square's own document view, for a
/// page, a Markdown file, a picture or a PDF. The view takes no mouse of its
/// own (docview's rule), so the panel hands it presses, drags and the wheel.
pub(crate) struct DocFace {
    view: Entity<crate::docview::DocumentView>,
    /// Where the view was last drawn, in window pixels: a press becomes a
    /// point on the page from it.
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// A press on the page is held: moves drag it, a release lets go.
    holding: bool,
    /// What the view was last told: on screen or not.
    shown: bool,
    _subs: Vec<gpui::Subscription>,
}

#[derive(Default)]
pub(crate) struct ChangesLive {
    list: Reading<ChangeList>,
    patches: HashMap<String, Reading<FileDiff>>,
    open: HashSet<String>,
    /// Extra rows drawn past [`PATCH_ROWS`], per file, from "show more".
    more: HashMap<String, usize>,
    /// The row the keyboard is on, counted in the order drawn.
    cursor: usize,
    scroll: ScrollHandle,
    filter: Filter,
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
    /// The line drawn widest, which sets how far the list scrolls sideways.
    widest: usize,
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
    find: Find,
    /// The page, for a file TD draws: made the first time the tab is shown.
    face: Option<DocFace>,
    /// Whether making the face was tried, so a file that cannot have one is
    /// not asked again every second.
    face_tried: bool,
    /// Why the file shows as text only, when it has a kind TD draws.
    face_note: Option<String>,
    /// The text is shown in place of the page.
    source: bool,
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
            find: Find::default(),
            face: None,
            face_tried: false,
            face_note: None,
            source: false,
        }
    }
}

impl FileLive {
    /// The page is what is on screen.
    fn paged(&self) -> bool {
        self.face.is_some() && !self.source
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
    diff: Reading<flyout::PullDiff>,
    face: PullFace,
    open: HashSet<String>,
    more: HashMap<String, usize>,
    cursor: usize,
    scroll: ScrollHandle,
    filter: Filter,
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
    /// Something an agent of this task handed over: a declared deliverable,
    /// an artifact on its bench, or a `Deliverable:` line it printed.
    Artifact {
        pane: EntityId,
        key: String,
        label: String,
        href: String,
        at_ms: u64,
        opened: bool,
        /// The pane's name, as a person reads it.
        who: String,
    },
    Changes(PathBuf),
    Pull {
        owner: String,
        repo: String,
        row: PullRow,
        mine: bool,
    },
    /// A file, its path in the repository, and the letters a query lit.
    File(PathBuf, String, Vec<usize>),
}

/// The finder's rows for one query over one reading, kept so a frame does
/// not score thirty thousand paths again.
struct FinderRows {
    query: String,
    here: Option<PathBuf>,
    read_at: Option<Instant>,
    rows: Vec<AddRow>,
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
    finder: RefCell<Option<FinderRows>>,
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
            finder: RefCell::new(None),
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

/// Where gh runs for `owner/repo`: the first candidate whose `origin` is
/// that GitHub repository, so a gh that chooses its account by the remote
/// (a wrapper that does) asks the right one; else the home folder.
fn gh_dir(candidates: &[PathBuf], owner: &str, repo: &str) -> PathBuf {
    candidates
        .iter()
        .find(|dir| {
            git_text(dir, &["remote", "get-url", "origin"])
                .ok()
                .and_then(|u| flyout::github_slug(&u))
                .is_some_and(|(o, r)| o.eq_ignore_ascii_case(owner) && r.eq_ignore_ascii_case(repo))
        })
        .cloned()
        .unwrap_or_else(crate::session::home_dir)
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
    gh_with(&gh_program(), dir, args, GH_BUDGET)
}

/// [`gh`], with the program and the time it may take named: a test hands it
/// a stand-in, and GitHub's per-file list gets the time its pages need.
fn gh_with(program: &str, dir: &Path, args: &[&str], budget: Duration) -> Result<Vec<u8>, String> {
    let env = [
        ("GH_PROMPT_DISABLED", "1"),
        ("GH_NO_UPDATE_NOTIFIER", "1"),
        ("GH_SPINNER_DISABLED", "1"),
        ("NO_COLOR", "1"),
        ("CLICOLOR", "0"),
    ];
    match crate::engstate::run_bounded(program, dir, args, &env, budget, PATCH_CAP, 8192) {
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
            changed: Vec::new(),
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
    let widest = lines
        .iter()
        .enumerate()
        .max_by_key(|(_, l)| l.chars().count().min(LINE_DRAWN))
        .map_or(0, |(i, _)| i);
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
            widest,
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
) -> Result<flyout::PullDiff, String> {
    read_pull_diff_with(&gh_program(), dir, owner, repo, number)
}

/// A pull request's files: its whole diff in one piece, or, when GitHub
/// refuses that for its size (past 20,000 lines it answers 406), its
/// per-file list, every page, each file with its own patch.
fn read_pull_diff_with(
    program: &str,
    dir: &Path,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<flyout::PullDiff, String> {
    let slug = format!("{owner}/{repo}");
    let n = number.to_string();
    match gh_with(
        program,
        dir,
        &["pr", "diff", &n, "--repo", &slug, "--color", "never"],
        GH_BUDGET,
    ) {
        Ok(out) => Ok(flyout::PullDiff::whole(flyout::parse_unified(
            &String::from_utf8_lossy(&out),
        ))),
        Err(e) if flyout::refused_for_size(&e) => {
            let path = format!("repos/{owner}/{repo}/pulls/{number}/files?per_page=100");
            let out = gh_with(program, dir, &["api", "--paginate", &path], GH_PAGES_BUDGET)?;
            flyout::parse_pull_files(&String::from_utf8_lossy(&out))
        }
        Err(e) => Err(e),
    }
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

/// The kind of document a File tab draws as a page rather than as text:
/// what the floating square draws, but a clip or a sound, which play there.
fn drawn_kind(path: &Path) -> Option<crate::docopen::DocKind> {
    use crate::docopen::DocKind;
    match crate::docopen::doc_kind_by_name(path)? {
        k @ (DocKind::Markdown | DocKind::Html | DocKind::Image | DocKind::Pdf) => Some(k),
        DocKind::Video | DocKind::Audio => None,
    }
}

/// Where something an agent handed over opens in the flyout: a pull request
/// as a pull request, a page, a picture or a PDF drawn, a code file as text.
/// `None` for anything the flyout does not show (a web page, a clip, a file
/// that is gone), which goes where a handover always went.
pub(crate) fn artifact_target(href: &str) -> Option<SideOpen> {
    if let Some((owner, repo, number)) = flyout::pull_of_url(href.trim()) {
        return Some(SideOpen {
            target: Target::Pull {
                owner,
                repo,
                number,
            },
            line: None,
        });
    }
    let path = crate::handover::file_path(href)?;
    if !path.is_file() {
        return None;
    }
    if drawn_kind(&path).is_some() {
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        return Some(SideOpen {
            target: Target::File { path },
            line: None,
        });
    }
    side_of(&path.to_string_lossy(), None)
}

/// Whether the rail offers "◨ side" for a deliverable, by its address alone:
/// the rail paints every frame and never touches the disk to decide.
pub(crate) fn side_offers(href: &str) -> bool {
    if flyout::pull_of_url(href.trim()).is_some() {
        return true;
    }
    crate::handover::file_path(href)
        .is_some_and(|p| drawn_kind(&p).is_some() || flyout::syntax_for(&p).is_some())
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
/// `marks` are byte ranges of `text` lit with `mark` over the colours: the
/// words that changed inside a changed line, or the places a find found.
fn code_text(
    text: &str,
    sy: Option<&'static flyout::Syntax>,
    inside: &mut bool,
    ink: &Ink,
    marks: &[Range<usize>],
    mark: HighlightStyle,
) -> StyledText {
    // A tab is drawn as four spaces, so a mark after one moves three bytes
    // right for each: the marks were found in the text as it was written.
    let tabs_before = |at: usize| {
        text[..at.min(text.len())]
            .bytes()
            .filter(|b| *b == b'\t')
            .count()
            * 3
    };
    let mut marks: Vec<Range<usize>> = if text.contains('\t') {
        marks
            .iter()
            .map(|r| r.start + tabs_before(r.start)..r.end + tabs_before(r.end))
            .collect()
    } else {
        marks.to_vec()
    };
    let mut shown = text.replace('\t', "    ");
    // A minified file is one line of a megabyte, and shaping it every frame
    // would stall the window for text nobody can see past the panel's edge.
    if shown.len() > LINE_DRAWN {
        let mut cut = LINE_DRAWN;
        while !shown.is_char_boundary(cut) {
            cut -= 1;
        }
        shown.truncate(cut);
        shown.push('\u{2026}');
        marks.retain(|r| r.start < cut);
        for r in &mut marks {
            r.end = r.end.min(cut);
        }
    }
    let runs = match sy {
        Some(sy) => flyout::tokens(sy, &shown, inside),
        None => Vec::new(),
    };
    let ranges: Vec<Range<usize>> = runs.iter().map(|(r, _)| r.clone()).collect();
    let pieces = flyout::overlay(&ranges, &marks);
    StyledText::new(SharedString::from(shown)).with_highlights(pieces.into_iter().map(
        |(r, run, marked)| {
            let base = run.map(|k| ink.tok(runs[k].1)).unwrap_or_default();
            (r, if marked { base.highlight(mark) } else { base })
        },
    ))
}

/// A path, its folders dimmed and the letters a query matched lit.
fn path_text(path: &str, lit: &[usize], faint: Hsla, accent: Hsla) -> StyledText {
    let split = path.trim_end_matches('/').rfind('/').map_or(0, |at| at + 1);
    let runs: Vec<Range<usize>> = (split > 0)
        .then_some(Range {
            start: 0,
            end: split,
        })
        .into_iter()
        .collect();
    let marks = flyout::letter_ranges(path, lit);
    StyledText::new(SharedString::from(path.to_string())).with_highlights(
        flyout::overlay(&runs, &marks)
            .into_iter()
            .map(|(r, run, marked)| {
                let mut st = HighlightStyle::default();
                if run.is_some() {
                    st.color = Some(faint);
                }
                if marked {
                    st.color = Some(accent);
                    st.font_weight = Some(FontWeight::BOLD);
                }
                (r, st)
            }),
    )
}

/// A path in at most `fit` characters: whole when it fits, else its end —
/// the file's name and the folders nearest it — after an ellipsis, cut at a
/// folder's edge where one is near. The letters a filter lit move with it;
/// any in the part cut away are dropped.
fn elide_path(path: &str, fit: usize, lit: &[usize]) -> (String, Vec<usize>) {
    let n = path.chars().count();
    if n <= fit || fit < 8 {
        return (path.to_string(), lit.to_vec());
    }
    let keep = fit - 2;
    let mut cut = path.char_indices().nth(n - keep).map_or(0, |(b, _)| b);
    let name_at = path.rfind('/').map_or(0, |i| i + 1);
    if let Some(slash) = path[cut..].find('/') {
        if cut + slash < name_at {
            cut += slash;
        }
    }
    let mark = "\u{2026}";
    let shown = format!("{mark}{}", &path[cut..]);
    let lit = lit
        .iter()
        .filter(|&&a| a >= cut)
        .map(|&a| a - cut + mark.len())
        .collect();
    (shown, lit)
}

/// Which open file a diff list is scrolled into past its own row, and where
/// a pinned copy of that row sits: at the top, or pushed up by the file
/// below as it arrives. Read from the last frame's layout, which a scroll
/// redraws.
fn pinned(scroll: &ScrollHandle, row_h: f32, open: impl Fn(usize) -> bool) -> Option<(usize, f32)> {
    if scroll.children_count() == 0 {
        return None;
    }
    let (pos, _) = scroll.logical_scroll_top();
    if !open(pos) {
        return None;
    }
    let view = scroll.bounds();
    let item = scroll.bounds_for_item(pos)?;
    let off = scroll.offset().y;
    let top = f32::from(item.top() + off - view.top());
    let bottom = f32::from(item.bottom() + off - view.top());
    (top < -1. && bottom > 0.).then(|| (pos, (bottom - row_h).min(0.)))
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
    /// GitHub sent no patch for this file; its page on GitHub has it.
    Withheld(Option<String>),
}

/// A diff list's row height: the list's rows and a pinned copy agree on it.
fn diff_row_h(sk: &skin::Skin, s: f32) -> f32 {
    f32::from(sk.text_px(11.5 * s)) * 1.5 + 8. * s
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
        self.flyout_faces(cx);
        if self.flyout.focused {
            self.flyout.focused = false;
            self.focus_active(window, cx);
        }
        self.save(cx);
        cx.notify();
    }

    /// The composer and the relate dialog take every key first, so a panel or
    /// a field lit under one of them promised typing that went to the card.
    /// They are modal: Esc puts either away, and then the panel can have it.
    fn card_holds_keys(&self) -> bool {
        self.composer.is_some() || self.relate.is_some()
    }

    fn flyout_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.card_holds_keys() {
            return;
        }
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
        self.flyout_put(task, target, line, cx);
        if take_keys {
            self.flyout_focus(window, cx);
        }
    }

    /// [`Self::flyout_show`] without touching the keyboard: for what a page in
    /// the flyout itself opened, where the keyboard already is.
    pub(crate) fn flyout_put(
        &mut self,
        task: usize,
        target: Target,
        line: Option<u32>,
        cx: &mut Context<Self>,
    ) {
        let Some(t) = self.tabs.get_mut(task) else {
            return;
        };
        t.side.open(target.clone());
        self.flyout.open = true;
        self.flyout.adding = false;
        if let (Some(line), Live::File(f)) = (line, self.flyout_live(&target)) {
            // A line is a place in the text, so a page asked for at a line
            // shows its source there.
            f.source = true;
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
        self.flyout_mark_opened(&target, cx);
        self.flyout_faces(cx);
        self.save(cx);
        cx.notify();
    }

    // ---- what agents handed over ------------------------------------------

    /// Every handover of `path` in the window: the pane, the ledger's own
    /// key for it, and what the agent called it. A ledger keys a file by the
    /// path the agent wrote, which can name it differently from the flyout (a
    /// symlink, a `..`), so a key that does not match is tried again by the
    /// file it names — only for an entry with the same file name, which keeps
    /// it to a string compare on every other.
    fn handed(&self, path: &Path, cx: &App) -> Vec<(Entity<TerminalView>, String, String)> {
        let key = path.to_string_lossy();
        let name = path.file_name();
        let mut out = Vec::new();
        for t in &self.tabs {
            let mut leaves = Vec::new();
            t.root.leaves(&mut leaves);
            for l in leaves {
                for h in l.read(cx).handovers().newest_first() {
                    let k = h.key();
                    let same = k == key
                        || h.path().is_some_and(|p| {
                            p.file_name() == name
                                && std::fs::canonicalize(&p).ok().is_some_and(|c| {
                                    std::fs::canonicalize(path).ok().as_ref() == Some(&c)
                                })
                        });
                    if same {
                        out.push((l.clone(), k, h.label.clone()));
                    }
                }
            }
        }
        out
    }

    /// The pane that handed `path` over, wherever it sits in the window, and
    /// what it called it: what makes a tab wear ✦, and who its notes go to.
    fn delivered(&self, path: &Path, cx: &App) -> Option<(Entity<TerminalView>, String)> {
        self.handed(path, cx)
            .into_iter()
            .next()
            .map(|(pane, _, label)| (pane, label))
    }

    /// The pane `id`, wherever it sits.
    fn leaf_of(&self, id: EntityId) -> Option<Entity<TerminalView>> {
        for t in &self.tabs {
            let mut leaves = Vec::new();
            t.root.leaves(&mut leaves);
            if let Some(l) = leaves.into_iter().find(|l| l.entity_id() == id) {
                return Some(l.clone());
            }
        }
        None
    }

    /// A file an agent handed over has now been opened, as the header chip
    /// and the history page count it, whichever road it was opened by.
    fn flyout_mark_opened(&mut self, target: &Target, cx: &mut Context<Self>) {
        let Target::File { path } = target else {
            return;
        };
        let unread: Vec<(Entity<TerminalView>, String)> = self
            .handed(path, cx)
            .into_iter()
            .filter(|(l, key, _)| !l.read(cx).handovers().is_opened(key))
            .map(|(l, key, _)| (l, key))
            .collect();
        for (l, key) in unread {
            l.update(cx, |v, cx| {
                v.mark_handover_opened(&key);
                cx.notify();
            });
        }
    }

    /// Who ↪ on a page in the flyout sends to, and the name the bar gives
    /// them: the agent that handed the file over, wherever it is; else the
    /// shown task's agent, its focused pane first.
    fn flyout_notes_target(&self, path: &Path, cx: &App) -> Option<(Entity<TerminalView>, String)> {
        let name_of = |p: &TerminalView| {
            p.name
                .clone()
                .filter(|n| !n.is_empty())
                .or_else(|| (!p.title.is_empty()).then(|| p.title.clone()))
                .unwrap_or_else(|| p.mode.label().to_string())
        };
        if let Some((pane, _)) = self.delivered(path, cx) {
            if pane.read(cx).mode.is_agent() {
                let who = name_of(pane.read(cx));
                return Some((pane, who));
            }
        }
        let tab = self.tabs.get(self.active)?;
        let mut leaves = Vec::new();
        tab.root.leaves(&mut leaves);
        let focused = tab
            .focused
            .and_then(|id| leaves.iter().find(|l| l.entity_id() == id).copied());
        let pick = focused
            .filter(|l| l.read(cx).mode.is_agent())
            .or_else(|| leaves.iter().find(|l| l.read(cx).mode.is_agent()).copied())?
            .clone();
        let who = name_of(pick.read(cx));
        Some((pick, who))
    }

    /// ↪ on a page in the flyout: the notes map goes into the agent's prompt
    /// — pasted, never sent — and the page's bar says where it landed.
    fn flyout_send_notes(
        &mut self,
        key: &Target,
        ev: &crate::docview::SendNotes,
        cx: &mut Context<Self>,
    ) {
        use crate::docview::notes_ui::{Said, Words};
        let Target::File { path } = key else {
            return;
        };
        let said = match self.flyout_notes_target(path, cx) {
            None => Said::Refused(Words::said(|l| l.strings().ui.doc_no_agent_beside.into())),
            Some((pane, who)) => match pane.update(cx, |v, cx| v.send_notes(&ev.map, cx)) {
                Ok(landed) => notes_landed_said(landed, &who, ev.unsaved, ev.pictures),
                Err(why) => Said::Refused(why),
            },
        };
        if let Some(view) = self.face_of(key) {
            view.update(cx, |v, cx| v.notes_said(said, cx));
        }
    }

    /// What this task's agents handed over, newest first: the `+` page's
    /// first rows.
    fn artifact_rows(&self, cx: &App) -> Vec<AddRow> {
        let Some(tab) = self.tabs.get(self.active) else {
            return Vec::new();
        };
        let mut leaves = Vec::new();
        tab.root.leaves(&mut leaves);
        let mut rows = Vec::new();
        for l in leaves {
            let v = l.read(cx);
            let who = v.handover_pane_name();
            for h in v.handovers().newest_first() {
                let key = h.key();
                rows.push(AddRow::Artifact {
                    pane: l.entity_id(),
                    opened: v.handovers().is_opened(&key),
                    key,
                    label: h.label.clone(),
                    href: h.href.clone(),
                    at_ms: h.at_ms,
                    who: who.clone(),
                });
            }
        }
        rows.sort_by_key(|r| match r {
            AddRow::Artifact { at_ms, .. } => std::cmp::Reverse(*at_ms),
            _ => std::cmp::Reverse(0),
        });
        rows
    }

    /// Open what an agent handed over: in the flyout when it shows that kind
    /// of thing, else the road a handover always took from its own pane.
    fn flyout_open_artifact(
        &mut self,
        pane: EntityId,
        key: &str,
        href: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match artifact_target(href) {
            Some(open) => {
                let task = self.active;
                self.flyout_show(task, open.target, open.line, true, window, cx);
                if let Some(l) = self.leaf_of(pane) {
                    let key = key.to_string();
                    l.update(cx, |v, cx| {
                        v.mark_handover_opened(&key);
                        cx.notify();
                    });
                }
            }
            None => {
                if let Some(l) = self.leaf_of(pane) {
                    l.update(cx, |v, cx| v.ask_handover_by_key(key, cx));
                }
            }
        }
    }

    /// "◨ side" on a rail row: the deliverable opens in the flyout of the
    /// task the person is looking at, as the rail's own click opens it where
    /// the person already is. Answers whether it opened.
    pub(crate) fn rail_open_side(
        &mut self,
        href: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(open) = artifact_target(href) else {
            return false;
        };
        let task = self.active;
        self.flyout_show(task, open.target, open.line, true, window, cx);
        true
    }

    // ---- pages -------------------------------------------------------------

    /// The page a File tab draws, while it is the page on screen.
    fn face_of(&self, t: &Target) -> Option<Entity<crate::docview::DocumentView>> {
        match self.flyout.live.get(t) {
            Some(Live::File(f)) => f.face.as_ref().map(|d| d.view.clone()),
            _ => None,
        }
    }

    /// The page, when the page (not its source) is on screen.
    fn paged_face(&self, t: &Target) -> Option<Entity<crate::docview::DocumentView>> {
        match self.flyout.live.get(t) {
            Some(Live::File(f)) if f.paged() => f.face.as_ref().map(|d| d.view.clone()),
            _ => None,
        }
    }

    /// The page on screen and a window point on it, as the view's own flat
    /// point from its top-left.
    fn face_point(
        &self,
        t: &Target,
        at: Point<Pixels>,
    ) -> Option<(Entity<crate::docview::DocumentView>, Point<Pixels>)> {
        let Some(Live::File(f)) = self.flyout.live.get(t) else {
            return None;
        };
        let face = f.face.as_ref().filter(|_| f.paged())?;
        let b = face.bounds.get()?;
        Some((
            face.view.clone(),
            point(at.x - b.origin.x, at.y - b.origin.y),
        ))
    }

    fn face_held(&self, t: &Target) -> bool {
        matches!(self.flyout.live.get(t), Some(Live::File(f)) if f.face.as_ref().is_some_and(|d| d.holding))
    }

    fn face_hold(&mut self, t: &Target, held: bool) {
        if let Some(Live::File(f)) = self.flyout.live.get_mut(t) {
            if let Some(face) = f.face.as_mut() {
                face.holding = held;
            }
        }
    }

    /// Make a File tab's page the first time it is read, for a file TD draws.
    /// A machine that cannot draw it (no Chromium for a page, no poppler for
    /// a PDF) shows the text and says why.
    fn file_face(&mut self, key: &Target, path: &Path, cx: &mut Context<Self>) {
        use crate::docopen::DocKind;
        let Some(kind) = drawn_kind(path) else {
            return;
        };
        if !matches!(self.flyout.live.get(key), Some(Live::File(f)) if !f.face_tried) {
            return;
        }
        let l = self.lang;
        let ready = match kind {
            DocKind::Html => crate::docview::html_ready(cx).map_err(|u| u.reason_in(l)),
            DocKind::Pdf => crate::docview::pdf_ready(cx).map_err(|m| m.reason_in(l)),
            _ => Ok(()),
        };
        let face = ready.map(|()| {
            let doc = crate::docopen::DocTarget {
                path: path.to_path_buf(),
                kind,
            };
            let view = cx.new(|cx| crate::docview::DocumentView::new(doc, cx));
            view.update(cx, |v, cx| v.set_seat(crate::docopen::DocSeat::Face, cx));
            let links = cx.subscribe(&view, |ws, _, ev: &crate::docview::FollowLink, cx| {
                ws.flyout_follow(ev.clone(), cx)
            });
            let k = key.clone();
            let notes = cx.subscribe(&view, move |ws, _, ev: &crate::docview::SendNotes, cx| {
                ws.flyout_send_notes(&k, ev, cx)
            });
            let k = key.clone();
            let cannot = cx.subscribe(&view, move |ws, _, ev: &crate::docview::CannotShow, cx| {
                ws.flyout_cannot(&k, ev.reason.clone(), cx)
            });
            DocFace {
                view,
                bounds: Rc::new(Cell::new(None)),
                holding: false,
                shown: false,
                _subs: vec![links, notes, cannot],
            }
        });
        if let Some(Live::File(f)) = self.flyout.live.get_mut(key) {
            f.face_tried = true;
            match face {
                Ok(face) => f.face = Some(face),
                Err(why) => f.face_note = Some(why),
            }
        }
        self.flyout_faces(cx);
    }

    /// The page said it cannot be drawn after all (a browser that will not
    /// start): the tab shows the text, saying why, and lets the page go.
    fn flyout_cannot(&mut self, key: &Target, reason: String, cx: &mut Context<Self>) {
        if let Some(Live::File(f)) = self.flyout.live.get_mut(key) {
            f.face = None;
            f.face_note = Some(reason);
        }
        cx.notify();
    }

    /// A link pressed on a page in the flyout: a file opens beside it in a new
    /// tab (drawn, when TD draws it), a pull request as a pull request, and
    /// anything else in the browser.
    fn flyout_follow(&mut self, link: crate::docview::FollowLink, cx: &mut Context<Self>) {
        let Some(open) = artifact_target(&link.target) else {
            crate::pane::open_with_system(&link.target);
            return;
        };
        let task = self.active;
        let target = open.target.clone();
        self.flyout_put(task, open.target, open.line, cx);
        if let (Some(fragment), Some(view)) = (link.fragment, self.face_of(&target)) {
            view.update(cx, |v, cx| v.show_fragment(fragment, cx));
        }
    }

    /// Tell each page whether it is on screen, and the one that is who its
    /// ↪ sends to.
    fn flyout_faces(&mut self, cx: &mut Context<Self>) {
        let current = (self.flyout.open && !self.flyout_adding())
            .then(|| self.side_current())
            .flatten();
        let beside = match &current {
            Some(Target::File { path }) => self.flyout_notes_target(path, cx).map(|(_, who)| who),
            _ => None,
        };
        for (t, live) in self.flyout.live.iter_mut() {
            let Live::File(f) = live else {
                continue;
            };
            let paged = f.paged();
            let Some(face) = f.face.as_mut() else {
                continue;
            };
            let shown = paged && current.as_ref() == Some(t);
            if face.shown != shown {
                face.shown = shown;
                face.view.update(cx, |v, cx| v.visible(shown, cx));
            }
            if shown {
                let b = beside.clone();
                face.view.update(cx, |v, cx| v.set_beside(b, cx));
            }
        }
    }

    /// `s`: a page's source, or back to the page.
    fn flyout_source(&mut self, t: &Target, cx: &mut Context<Self>) {
        if let Some(Live::File(f)) = self.flyout.live.get_mut(t) {
            if f.face.is_some() {
                f.source = !f.source;
                if !f.source {
                    f.find.typing = false;
                }
            }
        }
        self.flyout_faces(cx);
        cx.notify();
    }

    // ---- finding -----------------------------------------------------------

    /// The filter of the list a tab shows, if it shows one.
    fn list_filter(&mut self, t: &Target) -> Option<&mut Filter> {
        match self.flyout.live.get_mut(t)? {
            Live::Changes(c) => Some(&mut c.filter),
            Live::Pull(p) => Some(&mut p.filter),
            Live::File(_) => None,
        }
    }

    /// Work a list's filter out again, over the list as last read. `fresh`
    /// when the query changed: the keyboard goes back to the best row.
    fn refilter(&mut self, t: &Target, fresh: bool) {
        match self.flyout.live.get_mut(t) {
            Some(Live::Changes(c)) => {
                let paths = c
                    .list
                    .got
                    .as_ref()
                    .map(|l| l.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>())
                    .unwrap_or_default();
                c.filter.apply(paths);
                let n = c
                    .list
                    .got
                    .as_ref()
                    .map_or(0, |l| c.filter.rows(l.files.len()).len());
                c.cursor = if fresh {
                    0
                } else {
                    c.cursor.min(n.saturating_sub(1))
                };
                // A new query is read from its best answer, at the top.
                if fresh {
                    c.scroll.set_offset(point(px(0.), px(0.)));
                }
            }
            Some(Live::Pull(p)) => {
                let paths = p
                    .diff
                    .got
                    .as_ref()
                    .map(|d| {
                        d.files
                            .iter()
                            .map(|f| f.stat.path.as_str())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                p.filter.apply(paths);
                let n = p
                    .diff
                    .got
                    .as_ref()
                    .map_or(0, |d| p.filter.rows(d.files.len()).len());
                p.cursor = if fresh {
                    0
                } else {
                    p.cursor.min(n.saturating_sub(1))
                };
                if fresh {
                    p.scroll.set_offset(point(px(0.), px(0.)));
                }
            }
            _ => {}
        }
    }

    /// Find a file's text again: the query changed, or the file did.
    fn refind(&mut self, t: &Target) {
        if let Some(Live::File(f)) = self.flyout.live.get_mut(t) {
            f.find.hits = Rc::new(match &f.text.got {
                Some(text) if !f.find.query.is_empty() => {
                    flyout::find_text(&text.lines, &f.find.query, FIND_HITS)
                }
                _ => Vec::new(),
            });
            f.find.at = f.find.at.min(f.find.hits.len().saturating_sub(1));
        }
    }

    /// Go to the next place found, or the one before, and light it.
    fn find_step(&mut self, t: &Target, forward: bool) {
        if let Some(Live::File(f)) = self.flyout.live.get_mut(t) {
            let n = f.find.hits.len();
            if n == 0 {
                return;
            }
            f.find.at = if forward {
                (f.find.at + 1) % n
            } else {
                (f.find.at + n - 1) % n
            };
            let line = f.find.hits[f.find.at].0;
            f.scroll
                .scroll_to_item(line as usize, ScrollStrategy::Center);
        }
    }

    /// The first place found at or below the line on top: where a find
    /// starts, as a browser's does.
    fn find_from_top(&mut self, t: &Target) {
        if let Some(Live::File(f)) = self.flyout.live.get_mut(t) {
            let top = list_top(&f.scroll) as u32;
            if let Some(i) = f.find.hits.iter().position(|(line, _)| *line >= top) {
                f.find.at = i;
            } else {
                f.find.at = 0;
            }
            if let Some((line, _)) = f.find.hits.get(f.find.at) {
                f.scroll
                    .scroll_to_item(*line as usize, ScrollStrategy::Center);
            }
        }
    }

    /// `/` or Ctrl+F: a list's filter takes the keys, or a find in a file.
    /// A pull request turns to its files first; a page shows its text, which
    /// is what a find reads.
    fn search_start(&mut self, t: &Target, cx: &mut Context<Self>) {
        if self.card_holds_keys() {
            return;
        }
        match t {
            Target::Pull { .. } => {
                if !matches!(self.flyout.live.get(t), Some(Live::Pull(p)) if p.face == PullFace::Files)
                {
                    self.pull_face(PullFace::Files, cx);
                }
                if let Some(f) = self.list_filter(t) {
                    f.typing = true;
                }
            }
            Target::Changes { .. } => {
                if let Some(f) = self.list_filter(t) {
                    f.typing = true;
                }
            }
            Target::File { .. } => {
                if let Some(Live::File(f)) = self.flyout.live.get_mut(t) {
                    if f.face.is_some() {
                        f.source = true;
                    }
                    f.find.typing = true;
                }
                self.flyout_faces(cx);
            }
        }
        cx.notify();
    }

    /// Whether the tab's filter or find has the keys.
    fn search_typing(&self, t: &Target) -> bool {
        match self.flyout.live.get(t) {
            Some(Live::Changes(c)) => c.filter.typing,
            Some(Live::Pull(p)) => p.filter.typing && p.face == PullFace::Files,
            Some(Live::File(f)) => f.find.typing,
            None => false,
        }
    }

    /// Esc on a tab with a filter or a find: put it away. Answers whether
    /// there was one, so a second Esc gives the keyboard back.
    fn search_clear(&mut self, t: &Target, cx: &mut Context<Self>) -> bool {
        let had = match self.flyout.live.get_mut(t) {
            Some(Live::Changes(c)) => {
                let had = c.filter.typing || !c.filter.query.is_empty();
                c.filter.query.clear();
                c.filter.typing = false;
                had
            }
            Some(Live::Pull(p)) => {
                let had = p.filter.typing || !p.filter.query.is_empty();
                p.filter.query.clear();
                p.filter.typing = false;
                had
            }
            Some(Live::File(f)) => {
                let had = f.find.typing || !f.find.query.is_empty();
                f.find = Find::default();
                had
            }
            None => false,
        };
        if had {
            self.refilter(t, true);
            cx.notify();
        }
        had
    }

    /// Keys while a filter or a find has them: typing narrows or finds,
    /// arrows choose or step, Enter opens or steps, Esc puts it away.
    fn search_key(&mut self, t: &Target, ks: &gpui::Keystroke, cx: &mut Context<Self>) {
        let key = ks.key.as_str();
        let shift = ks.modifiers.shift;
        let typed = ks
            .key_char
            .as_deref()
            .filter(|c| !c.is_empty() && !c.chars().any(char::is_control))
            .map(str::to_string);
        if let Target::File { .. } = t {
            match key {
                "enter" => self.find_step(t, !shift),
                "down" => self.find_step(t, true),
                "up" => self.find_step(t, false),
                "backspace" => {
                    if let Some(Live::File(f)) = self.flyout.live.get_mut(t) {
                        f.find.query.pop();
                    }
                    self.refind(t);
                    self.find_from_top(t);
                }
                _ => {
                    if let Some(c) = typed {
                        self.search_type(t, &c);
                    }
                }
            }
            cx.notify();
            return;
        }
        match key {
            "enter" => {
                if let Some(f) = self.list_filter(t) {
                    f.typing = false;
                }
                self.flyout_move(t, "enter", false, cx);
            }
            "up" | "down" | "pageup" | "pagedown" | "home" | "end" => {
                self.flyout_move(t, key, false, cx)
            }
            "backspace" => {
                if let Some(f) = self.list_filter(t) {
                    f.query.pop();
                }
                self.refilter(t, true);
            }
            _ => {
                if let Some(c) = typed {
                    self.search_type(t, &c);
                }
            }
        }
        cx.notify();
    }

    /// Text typed or pasted into the tab's filter or find.
    fn search_type(&mut self, t: &Target, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Target::File { .. } = t {
            if let Some(Live::File(f)) = self.flyout.live.get_mut(t) {
                f.find.query.push_str(text);
            }
            self.refind(t);
            self.find_from_top(t);
        } else {
            if let Some(f) = self.list_filter(t) {
                f.query.push_str(text);
            }
            self.refilter(t, true);
        }
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
        // Readings for tabs no task shows any more go, with their tasks.
        let shown: HashSet<&Target> = self.tabs.iter().flat_map(|t| t.side.tabs.iter()).collect();
        let stale: Vec<Target> = self
            .flyout
            .live
            .keys()
            .filter(|k| !shown.contains(k))
            .cloned()
            .collect();
        for k in stale {
            self.flyout.live.remove(&k);
        }
        if self.flyout_adding() {
            // The + page is what is on screen: the tab behind it waits.
            self.flyout.here = self.flyout_here(cx);
            self.suggest_read(force, cx);
        } else if let Some(t) = self.side_current() {
            self.flyout_read(&t, force, cx);
        }
        self.flyout_faces(cx);
    }

    fn flyout_read(&mut self, t: &Target, force: bool, cx: &mut Context<Self>) {
        match t.clone() {
            Target::Changes { repo, base } => self.changes_read(t, repo, base, force, cx),
            Target::File { path } => {
                self.file_face(t, &path, cx);
                self.file_read(t, path, force, cx)
            }
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
        self.refilter(key, false);
        let Some(Live::Changes(c)) = self.flyout.live.get_mut(key) else {
            return;
        };
        // The demo shows a diff open, as a person would have it: the first
        // rows as drawn, a filter's best first.
        if c.open.is_empty() && std::env::var("TD_FLYOUT_DEMO").is_ok_and(|v| v.contains("expand"))
        {
            if let Some(l) = &c.list.got {
                for (i, _) in c
                    .filter
                    .rows(l.files.len())
                    .into_iter()
                    .filter(|(i, _)| !l.files[*i].path.ends_with('/'))
                    .take(2)
                {
                    c.open.insert(l.files[i].path.clone());
                }
            }
        }
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
                let mut again = false;
                let mut first = false;
                if let Some(Live::File(f)) = ws.flyout.live.get_mut(&k) {
                    match read {
                        Ok(Some((text, marks, stamp))) => {
                            first = f.text.got.is_none();
                            f.stamp = Some(stamp);
                            f.marks = Rc::new(marks);
                            f.text.land(Ok(text));
                            if let Some(line) = f.jump.take() {
                                f.scroll.scroll_to_item(
                                    line.saturating_sub(1) as usize,
                                    ScrollStrategy::Center,
                                );
                            }
                            // The file changed under a find: find it again.
                            again = !f.find.query.is_empty();
                        }
                        Ok(None) => {
                            f.text.busy = false;
                            f.text.at = Some(Instant::now());
                        }
                        Err(e) => f.text.land(Err(e)),
                    }
                }
                if again {
                    ws.refind(&k);
                    // A find typed before the text arrived starts at its first
                    // place; one standing while the file changes stays put.
                    if first {
                        ws.find_from_top(&k);
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

    /// Folders that may be a checkout of `owner/repo`, best first: one the
    /// `+` page already read the remote of, then the repositories this
    /// window's panes stand in, the shown task's first. [`gh_dir`] settles
    /// which, off the UI thread.
    fn gh_candidates(&self, owner: &str, repo: &str, cx: &App) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self
            .flyout
            .suggest
            .iter()
            .filter(|(_, s)| {
                s.got
                    .as_ref()
                    .and_then(|g| g.slug.as_ref())
                    .is_some_and(|(o, r)| o == owner && r == repo)
            })
            .map(|(dir, _)| dir.clone())
            .collect();
        let mut leaves = Vec::new();
        for (i, t) in self.tabs.iter().enumerate() {
            let mut ls = Vec::new();
            t.root.leaves(&mut ls);
            if i == self.active {
                leaves.splice(0..0, ls);
            } else {
                leaves.extend(ls);
            }
        }
        for leaf in leaves {
            if let Some(root) = leaf
                .read(cx)
                .runtime()
                .cwd
                .and_then(|c| self.flyout.root_of(Path::new(&c)))
            {
                if !out.contains(&root) {
                    out.push(root);
                }
            }
            if out.len() >= 12 {
                break;
            }
        }
        out
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
        let candidates = self.gh_candidates(&owner, &repo, cx);
        let Live::Pull(p) = self.flyout_live(key) else {
            return;
        };
        let wants_diff = p.face == PullFace::Files
            && (p.diff.got.is_none() && p.diff.error.is_none() || force)
            && !p.diff.busy;
        if p.pull.due(PULL_EVERY) || force && !p.pull.busy {
            p.pull.busy = true;
            let (k, c, o, r) = (key.clone(), candidates.clone(), owner.clone(), repo.clone());
            let task = cx.spawn(async move |this, cx| {
                let read = cx
                    .background_spawn(async move { read_pull(&gh_dir(&c, &o, &r), &o, &r, number) })
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
                    .background_spawn(async move {
                        read_pull_diff(&gh_dir(&candidates, &owner, &repo), &owner, &repo, number)
                    })
                    .await;
                this.update(cx, |ws, cx| {
                    if let Some(Live::Pull(p)) = ws.flyout.live.get_mut(&k) {
                        p.diff.land(read);
                        // The demo shows a diff open, as a person would have it.
                        if p.open.is_empty()
                            && std::env::var("TD_FLYOUT_DEMO").is_ok_and(|v| v.contains("expand"))
                        {
                            if let Some(d) = &p.diff.got {
                                for f in d.files.iter().filter(|f| f.diff.is_some()).take(2) {
                                    p.open.insert(f.stat.path.clone());
                                }
                            }
                        }
                    }
                    ws.refilter(&k, false);
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
        let mut filter = None;
        let mut find = None;
        let mut source = false;
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
                "filter" => {
                    filter = Some(arg.to_string());
                    None
                }
                "find" => {
                    find = Some(arg.to_string());
                    None
                }
                "source" => {
                    source = true;
                    None
                }
                // `handed:<label>|<address>`: the first pane hands something
                // over, as a declaration would, so a rig with no agent in it
                // can show what the + page and a tab make of one.
                "handed" => {
                    if let Some((label, href)) = arg.split_once('|') {
                        let mut leaves = Vec::new();
                        self.tabs[0].root.leaves(&mut leaves);
                        if let Some(first) = leaves.first().map(|l| (*l).clone()) {
                            let (label, href) = (label.to_string(), href.to_string());
                            first.update(cx, |v, cx| {
                                v.hand_over(
                                    Some(label),
                                    href,
                                    crate::handover::Source::Declared,
                                    cx,
                                );
                            });
                        }
                    }
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
        if let Some(t) = self.tabs[0].side.current().cloned() {
            match self.flyout_live(&t) {
                Live::Changes(c) => {
                    if let Some(q) = &filter {
                        c.filter.query = q.clone();
                        c.filter.typing = true;
                    }
                }
                Live::Pull(p) => {
                    if let Some(q) = &filter {
                        p.filter.query = q.clone();
                        p.filter.typing = true;
                    }
                }
                Live::File(f) => {
                    if let Some(q) = &find {
                        f.find.query = q.clone();
                        f.find.typing = true;
                    }
                    f.source |= source;
                }
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
        // A brief with notes not yet saved into its file keeps itself open
        // once, saying what would be lost, as it does in a pane; the tab is
        // shown so that is seen. Only when no other task shows the same
        // file, since closing one of two tabs loses nothing.
        if let Some(t) = self.side().and_then(|s| s.tabs.get(i)).cloned() {
            let elsewhere = self
                .tabs
                .iter()
                .enumerate()
                .any(|(k, tab)| k != self.active && tab.side.tabs.contains(&t));
            if !elsewhere {
                if let Some(view) = self.face_of(&t) {
                    if view.update(cx, |v, cx| v.guard_close(cx)) {
                        if let Some(side) = self.side_mut() {
                            side.active = i;
                        }
                        self.flyout.adding = false;
                        if let Some(Live::File(f)) = self.flyout.live.get_mut(&t) {
                            f.source = false;
                        }
                        self.flyout_faces(cx);
                        cx.notify();
                        return;
                    }
                }
            }
        }
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
                    // The keyboard follows the press, counted as the rows are drawn.
                    if let Some(i) = c.list.got.as_ref().and_then(|l| {
                        c.filter
                            .rows(l.files.len())
                            .iter()
                            .position(|(i, _)| l.files[*i].path == path)
                    }) {
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
                    if let Some(i) = p.diff.got.as_ref().and_then(|d| {
                        p.filter
                            .rows(d.files.len())
                            .iter()
                            .position(|(i, _)| d.files[*i].stat.path == path)
                    }) {
                        p.cursor = i;
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
                    *c.more.entry(path.to_string()).or_default() += MORE_ROWS;
                }
            }
            DiffOwner::Pull(key) => {
                if let Some(Live::Pull(p)) = self.flyout.live.get_mut(key) {
                    *p.more.entry(path.to_string()).or_default() += MORE_ROWS;
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
                        p.open = d
                            .files
                            .iter()
                            .filter(|f| f.diff.is_some())
                            .take(60)
                            .map(|f| f.stat.path.clone())
                            .collect();
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
        self.flyout.query.clear();
        self.flyout.pick = 0;
        let target = match row {
            AddRow::Artifact {
                pane, key, href, ..
            } => {
                self.flyout_open_artifact(pane, &key, &href, window, cx);
                return;
            }
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
            AddRow::File(path, ..) => Target::File { path },
        };
        let task = self.active;
        self.flyout_show(task, target, None, true, window, cx);
    }

    /// The `+` page's rows, in the order they are drawn and walked: what this
    /// task's agents handed over first — the work is what the panel is for —
    /// then the repository's changes, its pull requests and its files.
    fn add_rows(&self, cx: &App) -> Vec<AddRow> {
        let q = self.flyout.query.trim().to_string();
        let mut rows: Vec<AddRow> = self
            .artifact_rows(cx)
            .into_iter()
            .filter(|r| match r {
                AddRow::Artifact { label, href, .. } => {
                    q.is_empty()
                        || flyout::find_path(label, &q).is_some()
                        || flyout::find_path(href, &q).is_some()
                }
                _ => false,
            })
            .take(ARTIFACT_ROWS)
            .collect();
        let Some(here) = self.flyout.here.clone() else {
            return rows;
        };
        let reading = self.flyout.suggest.get(&here);
        let read_at = reading.and_then(|r| r.at);
        if let Some(c) = self.flyout.finder.borrow().as_ref() {
            if c.query == q && c.here.as_ref() == Some(&here) && c.read_at == read_at {
                rows.extend(c.rows.iter().cloned());
                return rows;
            }
        }
        let rest = repo_rows(&here, reading.and_then(|r| r.got.as_ref()), &q);
        *self.flyout.finder.borrow_mut() = Some(FinderRows {
            query: q,
            here: Some(here),
            read_at,
            rows: rest.clone(),
        });
        rows.extend(rest);
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
        let current = if self.flyout_adding() {
            None
        } else {
            self.side_current()
        };
        // A note being written on a page has every key, chords and all —
        // Ctrl+Shift+R too, which would take the note away mid-word.
        if let Some(view) = current.as_ref().and_then(|t| self.paged_face(t)) {
            if view.read(cx).has_caret() {
                view.update(cx, |v, cx| v.key(ks, cx));
                return true;
            }
        }
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
        if m.control && !m.shift && !m.alt && key == "f" {
            if let Some(t) = &current {
                self.search_start(t, cx);
                return true;
            }
        }
        // A paste into a field that has the keys — the + page's finder, a
        // list's filter, a find — by any of the three chords a person uses:
        // its first line, as typed.
        if crate::workbench::is_paste_chord(&ks.key, m.control, m.shift) {
            let adding = self.flyout_adding();
            if adding || current.as_ref().is_some_and(|t| self.search_typing(t)) {
                let line: String = cx
                    .read_from_clipboard()
                    .and_then(|item| item.text())
                    .and_then(|t| t.lines().next().map(|l| l.trim().to_string()))
                    .unwrap_or_default()
                    .chars()
                    .take(200)
                    .collect();
                if adding {
                    self.flyout.query.push_str(&line);
                    self.flyout.pick = 0;
                } else if let Some(t) = &current {
                    self.search_type(t, &line);
                }
                cx.notify();
                return true;
            }
        }
        if m.control || m.alt || m.platform {
            return false;
        }
        if key == "escape" {
            // A page's own note box or dialog closes first, then a filter or
            // a find, then the + page; then the keyboard goes back.
            if let Some(view) = current.as_ref().and_then(|t| self.paged_face(t)) {
                if view.update(cx, |v, cx| v.key(ks, cx)) {
                    return true;
                }
            }
            if let Some(t) = &current {
                if self.search_clear(t, cx) {
                    return true;
                }
            }
            if self.flyout.adding && self.side().is_some_and(|s| !s.tabs.is_empty()) {
                self.flyout.adding = false;
                self.flyout_faces(cx);
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
        let Some(t) = current else {
            return true;
        };
        if self.search_typing(&t) {
            self.search_key(&t, ks, cx);
            return true;
        }
        let char = ks.key_char.as_deref().unwrap_or(key);
        let found =
            matches!(self.flyout.live.get(&t), Some(Live::File(f)) if !f.find.hits.is_empty());
        match char {
            "/" => self.search_start(&t, cx),
            "s" => self.flyout_source(&t, cx),
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
            // A find put away still steps with Enter, as a browser's does.
            _ if key == "enter" && found => self.find_step(&t, !m.shift),
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
                if let Some(view) = self.paged_face(t) {
                    use crate::keylayer::{GlowMove, Paging};
                    view.update(cx, |v, cx| match key {
                        "up" | "k" => v.glow(GlowMove::Step(-1), cx),
                        "down" | "j" => v.glow(GlowMove::Step(1), cx),
                        "pageup" => v.page(Paging::PageUp, cx),
                        "pagedown" | "space" => v.page(Paging::PageDown, cx),
                        "home" => v.page(Paging::Top, cx),
                        "end" => v.page(Paging::Bottom, cx),
                        _ => {}
                    });
                    return;
                }
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
                // The rows as drawn: a filter's best first, else the list's order.
                let (paths, cursor, scroll) = match self.flyout.live.get(t) {
                    Some(Live::Changes(c)) => (
                        c.list
                            .got
                            .as_ref()
                            .map(|l| {
                                c.filter
                                    .rows(l.files.len())
                                    .into_iter()
                                    .map(|(i, _)| l.files[i].path.clone())
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default(),
                        c.cursor,
                        c.scroll.clone(),
                    ),
                    Some(Live::Pull(p)) if p.face == PullFace::Files => (
                        p.diff
                            .got
                            .as_ref()
                            .map(|d| {
                                p.filter
                                    .rows(d.files.len())
                                    .into_iter()
                                    .map(|(i, _)| d.files[i].stat.path.clone())
                                    .collect::<Vec<_>>()
                            })
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
                // Only the rows drawn can be walked to.
                let paths = &paths[..paths.len().min(LIST_ROWS)];
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
                                // Opened from the keyboard: its row goes to the
                                // top, so what opened is on screen under it.
                                if !open {
                                    scroll.scroll_to_top_of_item(cursor);
                                }
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
        let rows = self.add_rows(cx);
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
        panel_width(want, win_w - left - RIGHT_COLUMN_W * s - 24., s)
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

    /// A tab's tooltip: the whole of its identity, and who handed it over.
    fn tab_tip(&self, t: &Target, cx: &App) -> Vec<String> {
        let mut lines = vec![self.tab_identity(t)];
        if let Target::File { path } = t {
            if let Some((pane, label)) = self.delivered(path, cx) {
                let who = pane.read(cx).handover_pane_name();
                lines.push(lang::fill(
                    self.lang.strings().flyout.delivered_by,
                    &[("who", &who), ("label", &label)],
                ));
            }
        }
        lines
    }

    /// A tab's kind mark and its colour: a pull request's ⇄ in its state's
    /// colour, as GitHub colours its own; a file an agent handed over wears ✦.
    fn tab_glyph(
        &self,
        t: &Target,
        lit: bool,
        th: &theme::Theme,
        sk: &skin::Skin,
        cx: &App,
    ) -> (&'static str, Hsla) {
        let plain = if lit { th.accent } else { sk.ink.ink_dim };
        match (t, self.flyout.live.get(t)) {
            (Target::Pull { .. }, Some(Live::Pull(p))) => (
                t.glyph(),
                match p.pull.got.as_ref().map(|x| x.state) {
                    Some(PullState::Open) => sk.ink.ok,
                    Some(PullState::Draft) => sk.ink.ink_faint,
                    Some(PullState::Merged) => th.ansi[13],
                    Some(PullState::Closed) => sk.ink.danger,
                    Some(PullState::Unknown) | None => plain,
                },
            ),
            (Target::File { path }, _) if self.delivered(path, cx).is_some() => {
                ("\u{2726}", th.accent)
            }
            // A file drawn as a page, as the + page marks one.
            (Target::File { path }, _) if drawn_kind(path).is_some() => ("\u{25e7}", plain),
            _ => (t.glyph(), plain),
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
            let (glyph, glyph_ink) = self.tab_glyph(t, lit, th, sk, cx);
            let group = SharedString::from(format!("flyout-tab-g-{i}"));
            // The tab's ✕, at its left: quiet until the pointer is on the tab,
            // red when it is on the ✕ itself. Its press stops there, so it
            // closes the tab without showing it first.
            let close = div()
                .id(SharedString::from(format!("flyout-tab-x-{i}")))
                .debug_selector(move || format!("flyout-tab-x-{i}"))
                .flex_none()
                .size(px(15. * s))
                .flex()
                .items_center()
                .justify_center()
                .rounded(sk.radius())
                .text_size(sk.text_px(11. * s))
                .text_color(if lit {
                    sk.ink.ink_faint
                } else {
                    sk.ink.ink_ghost
                })
                .group_hover(group.clone(), |st| st.text_color(sk.ink.ink_dim))
                .hover(|st| st.text_color(sk.ink.danger).bg(sk.ink.hover))
                .child("\u{d7}")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        ws.flyout_close_tab(i, cx);
                    }),
                );
            let tab = div()
                .id(SharedString::from(format!("flyout-tab-{i}")))
                .debug_selector(move || format!("flyout-tab-{i}"))
                .group(group)
                .flex()
                .flex_row()
                .flex_none()
                .items_center()
                .gap(px(4. * s))
                .pl(px(3. * s))
                .pr(px(8. * s))
                .py(px(2. * s))
                .max_w(px(220. * s))
                .map(|d| sk.choice(d, lit))
                .cursor_pointer()
                .child(with_tip(close, vec![f.close_tab.into()]))
                .child(div().flex_none().size(px(6. * s)).rounded_full().bg(colour))
                .child(
                    div()
                        .flex_none()
                        .text_size(sk.text_px(11. * s))
                        .text_color(glyph_ink)
                        .child(glyph),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(sk.text_px(11. * s))
                        .child(self.tab_label(t)),
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
            strip = strip.child(with_tip(tab, self.tab_tip(t, cx)));
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

    /// The field over a list or a file: what is typed, a caret while it has
    /// the keys, how much answers, and a ✕ that puts it away. Idle, it says
    /// what it does and that `/` starts it. A press on it gives it the keys.
    #[allow(clippy::too_many_arguments)]
    fn search_field(
        &self,
        id: &'static str,
        query: &str,
        typing: bool,
        placeholder: &str,
        count: Option<String>,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let caret = if typing && self.flyout.focused {
            "\u{258f}"
        } else {
            ""
        };
        let active = typing || !query.is_empty();
        div()
            .id(id)
            .debug_selector(move || id.into())
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6. * s))
            .mx(px(8. * s))
            .my(px(4. * s))
            .px(px(8. * s))
            .py(px(3. * s))
            .rounded(sk.radius())
            .border_1()
            .border_color(if typing {
                th.accent.alpha(0.6)
            } else {
                sk.ink.rule
            })
            .bg(sk.ink.panel_sunken)
            .text_size(sk.text_px(11. * s))
            .cursor(gpui::CursorStyle::IBeam)
            .child(
                div()
                    .flex_none()
                    .text_color(sk.ink.ink_faint)
                    .child("\u{2315}"),
            )
            .child(if query.is_empty() {
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(sk.ink.ink_ghost)
                    .child(format!("{caret}{placeholder}"))
            } else {
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(th.text)
                    .child(format!("{query}{caret}"))
            })
            .when_some(count, |d, c| d.child(small(c, sk.ink.ink_faint, sk, s)))
            .when(!active, |d| {
                d.child(
                    div()
                        .flex_none()
                        .px(px(4. * s))
                        .rounded(sk.radius())
                        .border_1()
                        .border_color(sk.ink.rule)
                        .text_size(sk.text_px(9. * s))
                        .text_color(sk.ink.ink_faint)
                        .child("/"),
                )
            })
            .when(active, |d| {
                d.child(
                    div()
                        .id(SharedString::from(format!("{id}-clear")))
                        .flex_none()
                        .px(px(3. * s))
                        .rounded(sk.radius())
                        .text_color(sk.ink.ink_faint)
                        .cursor_pointer()
                        .hover(|st| st.text_color(sk.ink.danger).bg(sk.ink.hover))
                        .child("\u{d7}")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                if let Some(t) = ws.side_current() {
                                    ws.search_clear(&t, cx);
                                }
                            }),
                        ),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                    if let Some(t) = ws.side_current() {
                        ws.search_start(&t, cx);
                    }
                }),
            )
    }

    fn flyout_foot(&self, sk: &skin::Skin, s: f32) -> gpui::Div {
        let f = self.lang.strings().flyout;
        // The keys of what is on screen: the + page, a field taking keys, a
        // file, a list, or a pull request's other faces.
        let words = match (self.flyout_adding(), self.side_current()) {
            (true, _) | (_, None) => f.keys_add,
            (_, Some(t)) if self.search_typing(&t) => match t {
                Target::File { .. } => f.keys_find,
                _ => f.keys_filter,
            },
            (_, Some(Target::File { .. })) => f.keys_file,
            (_, Some(t @ Target::Pull { .. })) if !matches!(self.flyout.live.get(&t), Some(Live::Pull(p)) if p.face == PullFace::Files) => {
                f.keys
            }
            (_, Some(_)) => f.keys_list,
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
                self.file_body(&t, path, fl, th, sk, s, cx)
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
        let totals = list.map(|l| flyout::totals(&l.files));
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
                    // Only what git counted is summed; files it could not
                    // count are said apart, never folded in as zero.
                    .when_some(totals.as_ref().and_then(|t| t.counted), |d, (a, r)| {
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
                    .when_some(totals.as_ref().filter(|t| t.uncounted > 0), |d, t| {
                        d.child(small(
                            lang::fill(f.uncounted, &[("count", &t.uncounted.to_string())]),
                            ink.faint,
                            sk,
                            s,
                        ))
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
        let order = list
            .map(|l| c.filter.rows(l.files.len()))
            .unwrap_or_default();
        let field = list.filter(|l| !l.files.is_empty()).map(|l| {
            self.search_field(
                "flyout-filter",
                &c.filter.query,
                c.filter.typing,
                f.filter_files,
                c.filter.hits.as_ref().map(|_| {
                    lang::fill(
                        f.filter_count,
                        &[
                            ("shown", &order.len().to_string()),
                            ("total", &l.files.len().to_string()),
                        ],
                    )
                }),
                th,
                sk,
                s,
                cx,
            )
        });
        let mut rows = div()
            .id("flyout-changes")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&c.scroll)
            .flex()
            .flex_col()
            .px(px(4. * s))
            .pb(px(10. * s));
        if let Some(l) = list {
            for (pos, (i, lit)) in order.iter().enumerate().take(LIST_ROWS) {
                let file = &l.files[*i];
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
                    pos,
                    file,
                    lit,
                    Some(repo),
                    patch,
                    c.open.contains(&file.path),
                    c.more.get(&file.path).copied().unwrap_or(0),
                    pos == c.cursor && self.flyout.focused,
                    &ink,
                    th,
                    sk,
                    s,
                    cx,
                ));
            }
            if order.is_empty() && !l.files.is_empty() {
                rows = rows.child(note(f.add_no_files, ink.faint, sk, s));
            }
        }
        if let Some(rest) = Some(order.len()).filter(|n| *n > LIST_ROWS) {
            rows = rows.child(note(
                &lang::fill(f.more_files, &[("count", &(rest - LIST_ROWS).to_string())]),
                ink.faint,
                sk,
                s,
            ));
        }
        // The open file the list is scrolled into keeps its row at the top.
        let row_h = diff_row_h(sk, s);
        let pin = list.and_then(|l| {
            let (pos, y) = pinned(&c.scroll, row_h, |pos| {
                order
                    .get(pos)
                    .is_some_and(|(i, _)| c.open.contains(&l.files[*i].path))
            })?;
            let (i, lit) = order[pos];
            Some(self.diff_pin(
                &owner,
                pos,
                &l.files[i],
                lit,
                Some(repo),
                y,
                &ink,
                th,
                sk,
                s,
                cx,
            ))
        });
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(head)
            .children(notes)
            .children(field)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(rows)
                    .children(pin),
            )
            .into_any_element()
    }

    /// A pinned copy of an open file's row, over the top of its list: a press
    /// folds the file and brings its own row back to the top.
    #[allow(clippy::too_many_arguments)]
    fn diff_pin(
        &self,
        owner: &DiffOwner,
        pos: usize,
        file: &FileStat,
        lit: &[usize],
        repo: Option<&Path>,
        y: f32,
        ink: &Ink,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let fold = owner.clone();
        let path = file.path.clone();
        div()
            .absolute()
            .left_0()
            .right_0()
            .top(px(y))
            .px(px(4. * s))
            .bg(sk.region_face(darken(th.surface, 0.45)))
            .border_b_1()
            .border_color(sk.ink.rule)
            .shadow(vec![BoxShadow {
                color: gpui::black().alpha(0.35),
                offset: point(px(0.), px(2. * s)),
                blur_radius: px(6. * s),
                spread_radius: px(0.),
                inset: false,
            }])
            .child(
                self.diff_row(
                    owner,
                    "flyout-pin",
                    pos,
                    file,
                    lit,
                    repo,
                    true,
                    false,
                    ink,
                    th,
                    sk,
                    s,
                    cx,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        ws.diff_fold_to(&fold, &path, pos, cx);
                    }),
                ),
            )
            .into_any_element()
    }

    /// One file of a diff list: its row, and its hunks when open.
    #[allow(clippy::too_many_arguments)]
    fn diff_file(
        &self,
        owner: &DiffOwner,
        pos: usize,
        file: &FileStat,
        lit: &[usize],
        repo: Option<&Path>,
        patch: Patch<'_>,
        open: bool,
        more: usize,
        cursor: bool,
        ink: &Ink,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let f = self.lang.strings().flyout;
        let dir = file.path.ends_with('/');
        let path = file.path.clone();
        let toggle_owner = owner.clone();
        let open_on_disk = repo.map(|r| r.join(&file.path));
        let row = self
            .diff_row(
                owner,
                "flyout-file",
                pos,
                file,
                lit,
                repo,
                open,
                cursor,
                ink,
                th,
                sk,
                s,
                cx,
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, ev: &MouseDownEvent, window, cx| {
                    if ev.click_count >= 2 {
                        if let Some(p) = open_on_disk.clone() {
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
                Patch::Got(d) => {
                    let at = match (owner, repo) {
                        (DiffOwner::Changes(_), Some(r)) if file.status != Status::Deleted => {
                            Some(r.join(&file.path))
                        }
                        _ => None,
                    };
                    self.diff_hunks(owner, &file.path, d, more, at, ink, th, sk, s, cx)
                }
                Patch::Withheld(url) => {
                    // What GitHub's per-file list says about a file it sent no
                    // patch for: only moved (a real zero), too large to send
                    // (counted), or nothing counted at all (binary, most likely).
                    let moved = matches!(file.status, Status::Renamed | Status::Copied);
                    let said = match (file.added, file.removed) {
                        (None, None) if moved => f.pr_moved,
                        (Some(_), _) | (_, Some(_)) => f.pr_withheld,
                        (None, None) => f.pr_no_patch,
                    };
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(8. * s))
                        .pr(px(10. * s))
                        .child(div().flex_1().child(note(said, ink.faint, sk, s)))
                        .when_some(url, |d, url| {
                            d.child(
                                div()
                                    .id(SharedString::from(format!("flyout-withheld-{pos}")))
                                    .flex_none()
                                    .px(px(6. * s))
                                    .rounded(sk.radius())
                                    .text_size(sk.text_px(10. * s))
                                    .text_color(th.accent)
                                    .cursor_pointer()
                                    .hover(|st| st.bg(sk.ink.hover))
                                    .child(format!("{} \u{2197}", f.pr_on_github))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |_ws, _: &MouseDownEvent, _w, _cx| {
                                            crate::pane::open_with_system(&url);
                                        }),
                                    ),
                            )
                        })
                        .into_any_element()
                }
            });
        }
        out.into_any_element()
    }

    /// A diff list's row for one file: fold mark, status, path (folders dim,
    /// a filter's letters lit), old name, counts, and ↗ to open it. `id`
    /// keeps the list's rows and a pinned copy apart.
    #[allow(clippy::too_many_arguments)]
    fn diff_row(
        &self,
        _owner: &DiffOwner,
        id: &'static str,
        pos: usize,
        file: &FileStat,
        lit: &[usize],
        repo: Option<&Path>,
        open: bool,
        cursor: bool,
        ink: &Ink,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let f = self.lang.strings().flyout;
        let dir = file.path.ends_with('/');
        let mark_ink = match file.status {
            Status::Added | Status::Untracked => ink.ok,
            Status::Deleted => ink.danger,
            Status::Modified => ink.warn,
            Status::Renamed | Status::Copied | Status::TypeChanged => sk.ink.ink_dim,
            Status::Unknown => ink.faint,
        };
        let counts: AnyElement = if dir {
            small(f.untracked_dir, ink.faint, sk, s).into_any_element()
        } else if file.binary() && matches!(file.status, Status::Renamed | Status::Copied) {
            // Moved, with nothing counted: the old name beside it says what
            // is known, and "binary" would claim what is not.
            div().into_any_element()
        } else if file.binary() {
            small(f.binary, ink.faint, sk, s).into_any_element()
        } else {
            div()
                .flex()
                .flex_row()
                .gap(px(5. * s))
                .flex_none()
                .when_some(file.added, |d, a| {
                    d.child(small(format!("+{a}"), ink.ok, sk, s))
                })
                .when_some(file.removed, |d, r| {
                    d.child(small(format!("\u{2212}{r}"), ink.danger, sk, s))
                })
                .into_any_element()
        };
        let file_path = repo.map(|r| r.join(&file.path));
        div()
            .id(SharedString::from(format!("{id}-{pos}")))
            .debug_selector(move || format!("{id}-{pos}"))
            .h(px(diff_row_h(sk, s)))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6. * s))
            .px(px(6. * s))
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
                    .child(file.status.mark()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(sk.text_px(11.5 * s))
                    .text_color(th.text)
                    .child({
                        // A deep path keeps its end: the file's name and the
                        // folders nearest it, which is what was looked for.
                        let taken = if file.old_path.is_some() { 300. } else { 160. };
                        let (shown, lit) = elide_path(&file.path, self.path_fit(sk, s, taken), lit);
                        path_text(&shown, &lit, sk.ink.ink_faint, th.accent)
                    }),
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
                            .id(SharedString::from(format!("{id}-open-{pos}")))
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
    }

    /// About how many characters of a path fit in a row of the panel once
    /// `taken` pixels (at chrome size) go to the row's other parts. An
    /// estimate from the font's size; the row still truncates what is left.
    fn path_fit(&self, sk: &skin::Skin, s: f32, taken: f32) -> usize {
        let char_px = f32::from(sk.text_px(11.5 * s)) * 0.62;
        ((self.flyout_px(s) - taken * s) / char_px).max(16.) as usize
    }

    /// Fold an open file from its pinned row, and bring its own row back to
    /// the top of the list, where the press was.
    fn diff_fold_to(&mut self, owner: &DiffOwner, path: &str, pos: usize, cx: &mut Context<Self>) {
        let scroll = match owner {
            DiffOwner::Changes(k) | DiffOwner::Pull(k) => match self.flyout.live.get(k) {
                Some(Live::Changes(c)) => Some(c.scroll.clone()),
                Some(Live::Pull(p)) => Some(p.scroll.clone()),
                _ => None,
            },
        };
        self.diff_toggle(owner, path, cx);
        if let Some(scroll) = scroll {
            scroll.scroll_to_top_of_item(pos);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn diff_hunks(
        &self,
        owner: &DiffOwner,
        path: &str,
        d: &FileDiff,
        more: usize,
        open_at: Option<PathBuf>,
        ink: &Ink,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let f = self.lang.strings().flyout;
        let sy = flyout::syntax_for(Path::new(path));
        let total: usize = d.hunks.iter().map(|h| h.lines.len() + 1).sum();
        let cap = PATCH_ROWS + more;
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
        // Words that changed inside a changed line, lit over its tint.
        let word = |c: Hsla| HighlightStyle {
            background_color: Some(c.alpha(0.42)),
            ..Default::default()
        };
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
                let (bg, sign, sign_ink, mark) = match l.kind {
                    LineKind::Added => (Some(ink.ok.alpha(0.17)), "+", ink.ok, word(ink.ok)),
                    LineKind::Removed => (
                        Some(ink.danger.alpha(0.17)),
                        "\u{2212}",
                        ink.danger,
                        word(ink.danger),
                    ),
                    LineKind::Context => (None, " ", ink.ghost, HighlightStyle::default()),
                    LineKind::NoNewline => (None, "", ink.faint, HighlightStyle::default()),
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
                // A new line's number opens the file there, in its own tab.
                let new_num = match (&open_at, l.new) {
                    (Some(file), Some(n)) => {
                        let file = file.clone();
                        num(l.new)
                            .cursor_pointer()
                            .hover(|st| st.text_color(th.accent))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    let task = ws.active;
                                    ws.flyout_show(
                                        task,
                                        Target::File { path: file.clone() },
                                        Some(n),
                                        true,
                                        window,
                                        cx,
                                    );
                                }),
                            )
                    }
                    _ => num(l.new),
                };
                let code: AnyElement = if l.kind == LineKind::NoNewline {
                    div()
                        .text_color(ink.faint)
                        .child(l.text.clone())
                        .into_any_element()
                } else {
                    code_text(&l.text, sy, &mut inside, ink, &l.changed, mark).into_any_element()
                };
                block = block.child(
                    div()
                        .h(px(row_h))
                        .flex()
                        .flex_row()
                        .items_center()
                        .when_some(bg, |d, b| d.bg(b))
                        .child(num(l.old))
                        .child(new_num)
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

    #[allow(clippy::too_many_arguments)]
    fn file_body(
        &self,
        key: &Target,
        path: &Path,
        fl: &FileLive,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
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
        let paged = fl.paged();
        // Page | Source, for a file TD draws: the page by default.
        let switch = fl.face.is_some().then(|| {
            let chip = |page: bool, word: &'static str| {
                let lit = page == paged;
                let k = key.clone();
                div()
                    .id(SharedString::from(format!("flyout-face-page-{page}")))
                    .debug_selector(move || format!("flyout-face-page-{page}"))
                    .px(px(7. * s))
                    .py(px(1. * s))
                    .map(|d| sk.choice(d, lit))
                    .text_size(sk.text_px(10. * s))
                    .cursor_pointer()
                    .child(word)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            let now_paged =
                                matches!(ws.flyout.live.get(&k), Some(Live::File(f)) if f.paged());
                            if now_paged != page {
                                ws.flyout_source(&k, cx);
                            }
                        }),
                    )
            };
            div()
                .flex()
                .flex_row()
                .gap(px(3. * s))
                .flex_none()
                .child(chip(true, f.face_page))
                .child(chip(false, f.face_source))
        });
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
                    .children(switch)
                    .when_some(text.filter(|_| !paged), |d, t| {
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
        if let (true, Some(face)) = (paged, fl.face.as_ref()) {
            return div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(head)
                .child(self.page_host(face, cx))
                .into_any_element();
        }
        let mut notes: Vec<AnyElement> = Vec::new();
        if let Some(why) = &fl.face_note {
            notes.push(
                note(
                    &lang::fill(f.face_cannot, &[("why", why)]),
                    ink.faint,
                    sk,
                    s,
                )
                .into_any_element(),
            );
        }
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
        let find = &fl.find;
        let field = (!t.binary).then(|| {
            let count = if find.query.is_empty() {
                None
            } else if find.hits.is_empty() {
                Some(f.find_none.to_string())
            } else {
                let mut total = find.hits.len().to_string();
                if find.hits.len() >= FIND_HITS {
                    total.push('+');
                }
                Some(lang::fill(
                    f.find_count,
                    &[("at", &(find.at + 1).to_string()), ("count", &total)],
                ))
            };
            self.search_field(
                "flyout-find",
                &find.query,
                find.typing,
                f.find_file,
                count,
                th,
                sk,
                s,
                cx,
            )
        });
        let code_px = sk.text_px(11. * s);
        let row_h = f32::from(code_px) * 1.5;
        let digits = t.lines.len().max(1).to_string().len().max(3) as f32;
        let num_w = digits * f32::from(code_px) * 0.62 + 8. * s;
        let lines = t.lines.clone();
        let starts = t.starts.clone();
        let sy = t.syntax;
        let marks = fl.marks.clone();
        let hits = find.hits.clone();
        let here = hits.get(find.at).map(|(line, _)| *line);
        let lit = fl.lit;
        let binary = t.binary;
        let found = HighlightStyle {
            background_color: Some(th.accent.alpha(0.32)),
            ..Default::default()
        };
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
                        // The places a find found on this line, all lit; the
                        // line of the one stepped to washed like a line asked for.
                        let from = hits.partition_point(|(l, _)| (*l as usize) < i);
                        let on_line: Vec<Range<usize>> = hits[from..]
                            .iter()
                            .take_while(|(l, _)| *l as usize == i)
                            .map(|(_, r)| r.clone())
                            .collect();
                        div()
                            .h(px(row_h))
                            .flex()
                            .flex_row()
                            .items_center()
                            .when(lit == Some(n) || here == Some(i as u32), |d| d.bg(ink.lit))
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
                                    .flex_none()
                                    .pr(px(12. * s))
                                    .whitespace_nowrap()
                                    .text_color(ink.text)
                                    .child(code_text(
                                        &lines[i],
                                        sy,
                                        &mut inside,
                                        &ink,
                                        &on_line,
                                        found,
                                    )),
                            )
                    })
                    .collect::<Vec<_>>()
            },
        )
        // Long lines scroll sideways rather than stop at the panel's edge:
        // every row is as wide as the widest.
        .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
        .with_width_from_item(Some(t.widest))
        .track_scroll(&fl.scroll)
        .size_full();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(head)
            .children(notes)
            .children(field)
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

    /// Where a File tab's page is drawn: the document view itself, with a
    /// canvas noting where it lies, and the panel handing it the wheel and
    /// the pointer, since the view listens to nothing (docview's rule).
    fn page_host(&self, face: &DocFace, cx: &mut Context<Self>) -> AnyElement {
        let bounds = face.bounds.clone();
        div()
            .id("flyout-page")
            .debug_selector(|| "flyout-page".into())
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .child(
                canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            .child(face.view.clone())
            .on_scroll_wheel(cx.listener(|ws, ev: &ScrollWheelEvent, _w, cx| {
                let Some(view) = ws.side_current().and_then(|t| ws.paged_face(&t)) else {
                    return;
                };
                cx.stop_propagation();
                if ev.modifiers.control {
                    let notches = theme::wheel_notches(ev.delta);
                    view.update(cx, |v, cx| {
                        v.zoom_by_wheel(notches, cx);
                    });
                } else {
                    let delta = ev.delta;
                    view.update(cx, |v, cx| v.wheel(delta, cx));
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, ev: &MouseDownEvent, window, cx| {
                    let Some(t) = ws.side_current() else {
                        return;
                    };
                    let Some((view, at)) = ws.face_point(&t, ev.position) else {
                        return;
                    };
                    let mods = ev.modifiers;
                    let held = view.update(cx, |v, cx| v.press(at, mods, window, cx));
                    ws.face_hold(&t, held);
                }),
            )
            .on_mouse_move(cx.listener(|ws, ev: &MouseMoveEvent, _w, cx| {
                let Some(t) = ws.side_current() else {
                    return;
                };
                let Some((view, at)) = ws.face_point(&t, ev.position) else {
                    return;
                };
                if ws.face_held(&t) {
                    if ev.pressed_button == Some(MouseButton::Left) {
                        view.update(cx, |v, cx| v.drag(at, cx));
                    } else {
                        ws.face_hold(&t, false);
                        view.update(cx, |v, cx| v.release(cx));
                    }
                }
                view.update(cx, |v, cx| v.hover(Some(at), cx));
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseUpEvent, _w, cx| {
                    let Some(t) = ws.side_current() else {
                        return;
                    };
                    if ws.face_held(&t) {
                        ws.face_hold(&t, false);
                        if let Some(view) = ws.paged_face(&t) {
                            view.update(cx, |v, cx| v.release(cx));
                        }
                    }
                }),
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
        let state_pill = |word: &'static str| {
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
                .child(word)
        };
        // Reading the files or the conversation, the header is one line, so
        // the diff gets the room; Overview keeps all of it.
        if p.face != PullFace::Overview {
            let mut notes: Vec<AnyElement> = Vec::new();
            if let Some(e) = &p.pull.error {
                notes.push(note(e, ink.danger, sk, s).into_any_element());
            }
            let line = div()
                .flex_none()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(7. * s))
                .px(px(12. * s))
                .pt(px(8. * s))
                .pb(px(6. * s))
                .child(
                    div()
                        .id("flyout-pr-link")
                        .flex_none()
                        .text_size(sk.text_px(11. * s))
                        .text_color(sk.ink.ink_dim)
                        .cursor_pointer()
                        .hover(|st| st.text_color(th.accent))
                        .child(format!("#{number}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |_ws, _: &MouseDownEvent, _w, _cx| {
                                crate::pane::open_with_system(&open_url);
                            }),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(sk.text_px(12.5 * s))
                        .text_color(th.text)
                        .child(pull.map(|x| x.title.clone()).unwrap_or_default()),
                )
                .when_some(pull.and_then(|x| x.additions), |d, n| {
                    d.child(small(format!("+{n}"), ink.ok, sk, s))
                })
                .when_some(pull.and_then(|x| x.deletions), |d, n| {
                    d.child(small(format!("\u{2212}{n}"), ink.danger, sk, s))
                })
                .when(pull.is_some(), |d| d.child(state_pill(state_word)));
            let faces = self.pull_faces(p, pull, sk, s, cx);
            let content: AnyElement = match (p.face, pull) {
                (PullFace::Files, _) => self.pull_files(key, p, &ink, th, sk, s, cx),
                (PullFace::Conversation, Some(x)) => self.pull_conversation(x, p, &ink, th, sk, s),
                _ => note(f.reading, ink.faint, sk, s).into_any_element(),
            };
            return div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(line)
                .children(notes)
                .child(faces)
                .child(content)
                .into_any_element();
        }
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
                    .when(pull.is_some(), |d| d.child(state_pill(state_word))),
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
        let faces = self.pull_faces(p, pull, sk, s, cx);
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

    /// Overview, Files and Conversation, with what each holds.
    fn pull_faces(
        &self,
        p: &PullLive,
        pull: Option<&flyout::Pull>,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let f = self.lang.strings().flyout;
        div()
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
            )
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
        let column = div().flex_1().min_h_0().flex().flex_col();
        let Some(d) = &p.diff.got else {
            return column
                .child(match &p.diff.error {
                    Some(e) => note(e, ink.danger, sk, s),
                    None => note(f.reading, ink.faint, sk, s),
                })
                .into_any_element();
        };
        let files_url = key.url().map(|u| format!("{u}/files"));
        let order = p.filter.rows(d.files.len());
        let mut notes: Vec<AnyElement> = Vec::new();
        if let Some(e) = &p.diff.error {
            notes.push(note(e, ink.danger, sk, s).into_any_element());
        }
        if d.by_file {
            notes.push(note(f.pr_by_file, ink.faint, sk, s).into_any_element());
        }
        // GitHub lists at most 3,000 files: past that, say how many it left out.
        let listed = d.files.len() as u64;
        if let Some(total) = p.pull.got.as_ref().and_then(|x| x.changed_files) {
            if d.by_file && total > listed {
                notes.push(
                    note(
                        &lang::fill(f.pr_unlisted, &[("count", &(total - listed).to_string())]),
                        ink.warn,
                        sk,
                        s,
                    )
                    .into_any_element(),
                );
            }
        }
        let field = (!d.files.is_empty()).then(|| {
            self.search_field(
                "flyout-filter",
                &p.filter.query,
                p.filter.typing,
                f.filter_files,
                p.filter.hits.as_ref().map(|_| {
                    lang::fill(
                        f.filter_count,
                        &[
                            ("shown", &order.len().to_string()),
                            ("total", &d.files.len().to_string()),
                        ],
                    )
                }),
                th,
                sk,
                s,
                cx,
            )
        });
        let mut rows = div()
            .id("flyout-pr-files")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&p.scroll)
            .flex()
            .flex_col()
            .px(px(4. * s))
            .py(px(4. * s));
        for (pos, (i, lit)) in order.iter().enumerate().take(LIST_ROWS) {
            let file = &d.files[*i];
            let patch = match &file.diff {
                Some(diff) => Patch::Got(diff),
                None => Patch::Withheld(files_url.clone()),
            };
            rows = rows.child(self.diff_file(
                &owner,
                pos,
                &file.stat,
                lit,
                None,
                patch,
                p.open.contains(&file.stat.path),
                p.more.get(&file.stat.path).copied().unwrap_or(0),
                pos == p.cursor && self.flyout.focused,
                ink,
                th,
                sk,
                s,
                cx,
            ));
        }
        if order.is_empty() && !d.files.is_empty() {
            rows = rows.child(note(f.add_no_files, ink.faint, sk, s));
        }
        if order.len() > LIST_ROWS {
            rows = rows.child(note(
                &lang::fill(
                    f.more_files,
                    &[("count", &(order.len() - LIST_ROWS).to_string())],
                ),
                ink.faint,
                sk,
                s,
            ));
        }
        let row_h = diff_row_h(sk, s);
        let pin = pinned(&p.scroll, row_h, |pos| {
            order
                .get(pos)
                .is_some_and(|(i, _)| p.open.contains(&d.files[*i].stat.path))
        })
        .map(|(pos, y)| {
            let (i, lit) = order[pos];
            self.diff_pin(
                &owner,
                pos,
                &d.files[i].stat,
                lit,
                None,
                y,
                ink,
                th,
                sk,
                s,
                cx,
            )
        });
        column
            .children(notes)
            .children(field)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(rows)
                    .children(pin),
            )
            .into_any_element()
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
        let rows = self.add_rows(cx);
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
                AddRow::Artifact { .. } => "a",
                AddRow::Changes(_) => "c",
                AddRow::Pull { .. } => "p",
                AddRow::File(..) => "f",
            };
            if last != Some(kind) {
                match kind {
                    "a" => list = list.child(section(f.add_delivered)),
                    "p" => list = list.child(section(f.add_pulls)),
                    "f" => list = list.child(section(f.add_files)),
                    // The repository's own rows after what was delivered.
                    "c" if last.is_some() => list = list.child(section(f.add_repo)),
                    _ => {}
                }
                last = Some(kind);
            }
            let lit = i == self.flyout.pick;
            let plain = |t: String| div().child(t).into_any_element();
            let (glyph, glyph_ink, title, detail, unread): (
                &str,
                Hsla,
                AnyElement,
                Option<AnyElement>,
                bool,
            ) = match row {
                AddRow::Artifact {
                    label,
                    href,
                    at_ms,
                    opened,
                    who,
                    ..
                } => (
                    "\u{2726}",
                    th.accent,
                    plain(label.clone()),
                    Some(plain(
                        [
                            who.clone(),
                            crate::handover::kind_of(href),
                            when_label(Some(*at_ms as i64), self.lang),
                        ]
                        .into_iter()
                        .filter(|w| !w.is_empty())
                        .collect::<Vec<_>>()
                        .join(" \u{b7} "),
                    )),
                    !opened,
                ),
                AddRow::Changes(repo) => (
                    "\u{b1}",
                    ink.ok,
                    plain(lang::fill(
                        f.add_changes,
                        &[(
                            "repo",
                            &repo
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                        )],
                    )),
                    Some(plain(match got.and_then(|g| g.branch.as_deref()) {
                        Some(b) => format!("{b} \u{b7} {}", f.add_changes_sub),
                        None => f.add_changes_sub.to_string(),
                    })),
                    false,
                ),
                AddRow::Pull { row, mine, .. } => (
                    "\u{21c4}",
                    th.ansi[13],
                    plain(format!("#{} {}", row.number, row.title)),
                    Some(plain(if *mine {
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
                    })),
                    false,
                ),
                AddRow::File(_, rel, at) => {
                    // The name lit where the query fell in it, and the
                    // whole path under it lit the same.
                    let name_at = rel.rfind('/').map_or(0, |i| i + 1);
                    let in_name: Vec<usize> = at
                        .iter()
                        .filter(|i| **i >= name_at)
                        .map(|i| i - name_at)
                        .collect();
                    (
                        if drawn_kind(Path::new(rel)).is_some() {
                            "\u{25e7}"
                        } else {
                            "\u{25a4}"
                        },
                        sk.ink.ink_dim,
                        path_text(&rel[name_at..], &in_name, sk.ink.ink_faint, th.accent)
                            .into_any_element(),
                        Some({
                            let (shown, lit) = elide_path(rel, self.path_fit(sk, s, 90.), at);
                            path_text(&shown, &lit, sk.ink.ink_faint, th.accent).into_any_element()
                        }),
                        false,
                    )
                }
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
                            .when_some(detail, |d, detail| {
                                d.child(
                                    div()
                                        .truncate()
                                        .text_size(sk.text_px(9.5 * s))
                                        .text_color(sk.ink.ink_faint)
                                        .child(detail),
                                )
                            }),
                    )
                    // Not opened yet: the dot the header chip counts.
                    .when(unread, |d| {
                        d.child(
                            div()
                                .flex_none()
                                .size(px(6. * s))
                                .rounded_full()
                                .bg(th.accent),
                        )
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                            ws.add_open(open_row.clone(), window, cx);
                        }),
                    ),
            );
        }
        // Under what this task's agents delivered, which needs no repository.
        if here.is_none() {
            list = list.child(note(f.add_no_repo, ink.faint, sk, s));
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

/// How wide the panel draws, given the width wanted and the room beside the
/// left bar and the spine. The terminals keep [`SCREEN_KEEPS`] whenever the
/// window has it to give; a window too narrow for both splits what there is,
/// the panel taking at most half, so neither side is squeezed to nothing and
/// the panes are never pushed under the reserve by the panel's own minimum.
fn panel_width(want: f32, room: f32, s: f32) -> f32 {
    let keep = SCREEN_KEEPS * s;
    let floor = FLYOUT_MIN * s * 0.75;
    if room - keep >= floor {
        want.min(room - keep).max(floor)
    } else {
        want.min(room / 2.).max(0.)
    }
}

/// The `+` page's rows for a repository: with nothing typed, its changes,
/// its pull requests and the files changed in it; typing, a pull request by
/// its number, then files by how well they answer, best first.
fn repo_rows(here: &Path, got: Option<&Suggest>, q: &str) -> Vec<AddRow> {
    let mut rows = Vec::new();
    if q.is_empty() {
        rows.push(AddRow::Changes(here.to_path_buf()));
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
                rows.push(AddRow::File(here.join(c), c.clone(), Vec::new()));
            }
        }
        return rows;
    }
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
        // Clips and sounds are the floating square's to play; a picture, a
        // PDF and a page the flyout draws now, so they are found here too.
        let shown = |p: &&String| {
            !matches!(
                crate::docopen::doc_kind_by_name(Path::new(p.as_str())),
                Some(crate::docopen::DocKind::Video | crate::docopen::DocKind::Audio)
            )
        };
        let mut scored: Vec<(i64, &String, Vec<usize>)> = g
            .changed
            .iter()
            .filter(|c| !c.ends_with('/'))
            .chain(g.files.iter())
            .filter(shown)
            .filter_map(|p| flyout::find_path(p, q).map(|f| (f.score, p, f.at)))
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
        let mut seen = HashSet::new();
        for (_, p, at) in scored {
            if seen.insert(p.clone()) {
                rows.push(AddRow::File(here.join(p), p.clone(), at));
            }
            if rows.len() >= FINDER_ROWS {
                break;
            }
        }
    }
    rows
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

    /// The terminals keep their reserve whenever the window has it; a window
    /// too narrow for both splits what there is, and the panel's own minimum
    /// never pushes the panes under the reserve (found by the Codex review).
    #[test]
    fn the_panel_never_takes_the_room_the_terminals_keep() {
        // Wide: the width asked for.
        assert_eq!(panel_width(560., 1800., 1.), 560.);
        // Dragged wider than the room: what is left after the reserve.
        assert_eq!(panel_width(1500., 1800., 1.), 1800. - SCREEN_KEEPS);
        // Room for both: the panes keep the whole reserve.
        assert!(1000. - panel_width(900., 1000., 1.) >= SCREEN_KEEPS);
        // A 640 px tile with the left bar open is too narrow for both. The
        // panel's own minimum used to win here and leave the panes 138 px;
        // now the two split what there is.
        let room = 640. - 208. - RIGHT_COLUMN_W - 24.;
        let w = panel_width(560., room, 1.);
        assert!(
            room - w >= room / 2. - 0.5,
            "the panes keep at least half: {w}"
        );
        assert!(panel_width(560., 0., 1.) >= 0.);
    }

    /// gh runs from a checkout of the pull request's repository when one is
    /// among the candidates, so a gh that picks its account by the remote
    /// asks the right one (found by the Codex review).
    #[test]
    fn gh_runs_from_a_checkout_of_the_pull_requests_repository() {
        let _fork = crate::testsync::forks_and_locks();
        let other = Repo::new();
        other.git(&["remote", "add", "origin", "git@github.com:someone/else.git"]);
        let mine = Repo::new();
        mine.git(&[
            "remote",
            "add",
            "origin",
            "git@github-work:Parker-Brown-Family/Terminal-Delight.git",
        ]);
        let picked = gh_dir(
            &[other.root.clone(), mine.root.clone()],
            "parker-brown-family",
            "terminal-delight",
        );
        assert_eq!(picked, mine.root);
        assert_eq!(
            gh_dir(
                std::slice::from_ref(&other.root),
                "parker-brown-family",
                "terminal-delight"
            ),
            crate::session::home_dir(),
            "no checkout of it: the home folder"
        );
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

    /// A stand-in `gh`: answers `pr diff` with `diff_says` (an exit code and
    /// what it prints), answers `api` with two pages of GitHub's per-file
    /// list, and writes every call it gets to `calls`.
    fn fake_gh(dir: &Path, diff_code: i32, diff_out: &str) -> PathBuf {
        let calls = dir.join("calls");
        let page = dir.join("pages.json");
        std::fs::write(
            &page,
            r#"[{"filename":"src/a.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-x\n+y"}][{"filename":"big.json","status":"added","additions":30000,"deletions":0}]"#,
        )
        .unwrap();
        let out = dir.join("diff.out");
        std::fs::write(&out, diff_out).unwrap();
        let script = dir.join("gh");
        let _fork = crate::testsync::forks_and_locks();
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\n[ \"$1\" = probe ] && exit 0\necho \"$*\" >> '{calls}'\ncase \"$1 $2\" in\n  \"pr diff\") \
                 if [ {code} -eq 0 ]; then cat '{out}'; else cat '{out}' >&2; fi; exit {code};;\n  \
                 \"api --paginate\") cat '{page}';;\n  *) exit 2;;\nesac\n",
                calls = calls.display(),
                code = diff_code,
                out = out.display(),
                page = page.display(),
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        drop(_fork);
        // A file just written cannot be run while a child forked on another
        // thread still holds it open for writing (ETXTBSY): wait until it can.
        assert!(
            crate::testsync::soon(|| Command::new(&script).arg("probe").status().is_ok()),
            "the stand-in gh never became runnable"
        );
        script
    }

    /// GitHub refuses a whole diff past 20,000 lines with a 406: the files are
    /// then read from its per-file list, every page, each with its own patch,
    /// and a file it sent no patch for keeps its counts and no diff. A diff
    /// that comes whole never asks the list, and any other failure is said as
    /// it is rather than retried.
    #[test]
    fn a_refused_pull_request_diff_is_read_file_by_file() {
        let base = std::env::temp_dir().join(format!("td-flyout-gh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let refused = base.join("refused");
        std::fs::create_dir_all(&refused).unwrap();
        let gh = fake_gh(
            &refused,
            1,
            "could not find pull request diff: HTTP 406: Sorry, the diff exceeded the maximum \
             number of lines (20000) (https://api.github.com/repos/o/r/pulls/9)\n",
        );
        let d = read_pull_diff_with(&gh.to_string_lossy(), &refused, "o", "r", 9).unwrap();
        assert!(d.by_file);
        assert_eq!(d.files.len(), 2);
        assert_eq!(d.files[0].stat.path, "src/a.rs");
        assert_eq!(d.files[0].diff.as_ref().map(|x| x.hunks.len()), Some(1));
        assert_eq!(d.files[1].stat.added, Some(30000));
        assert!(d.files[1].diff.is_none(), "withheld, not an empty diff");
        let calls = std::fs::read_to_string(refused.join("calls")).unwrap();
        let calls: Vec<&str> = calls.lines().collect();
        assert_eq!(calls.len(), 2, "{calls:?}");
        assert!(calls[0].starts_with("pr diff 9 --repo o/r"), "{calls:?}");
        assert_eq!(
            calls[1],
            "api --paginate repos/o/r/pulls/9/files?per_page=100"
        );

        let whole = base.join("whole");
        std::fs::create_dir_all(&whole).unwrap();
        let gh = fake_gh(
            &whole,
            0,
            "diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n@@ -1 +1 @@\n-a\n+b\n",
        );
        let d = read_pull_diff_with(&gh.to_string_lossy(), &whole, "o", "r", 9).unwrap();
        assert!(!d.by_file);
        assert_eq!(d.files.len(), 1);
        let calls = std::fs::read_to_string(whole.join("calls")).unwrap();
        assert_eq!(calls.lines().count(), 1, "a whole diff never asks the list");

        let missing = base.join("missing");
        std::fs::create_dir_all(&missing).unwrap();
        let gh = fake_gh(&missing, 1, "HTTP 404: Not Found\n");
        let e = read_pull_diff_with(&gh.to_string_lossy(), &missing, "o", "r", 9).unwrap_err();
        assert!(e.contains("404"), "{e}");
        let calls = std::fs::read_to_string(missing.join("calls")).unwrap();
        assert_eq!(
            calls.lines().count(),
            1,
            "a refusal for another reason is not retried"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// What an agent handed over opens in the flyout when the flyout shows
    /// that kind of thing — a pull request, a page, a code file — and goes
    /// its usual road otherwise; the rail offers "◨ side" by the address alone.
    #[test]
    fn a_handed_over_artifact_opens_in_the_flyout_by_its_kind() {
        let dir = std::env::temp_dir().join(format!("td-flyout-art-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let page = dir.join("brief.html");
        let code = dir.join("main.rs");
        let clip = dir.join("cut.mp4");
        for p in [&page, &code, &clip] {
            std::fs::write(p, "x\n").unwrap();
        }
        let url = "https://github.com/parker-brown-family/terminal-delight/pull/844";
        assert_eq!(
            artifact_target(url).map(|o| o.target),
            Some(Target::Pull {
                owner: "parker-brown-family".into(),
                repo: "terminal-delight".into(),
                number: 844
            })
        );
        let canon = |p: &Path| std::fs::canonicalize(p).unwrap();
        assert_eq!(
            artifact_target(&format!("file://{}", page.display())).map(|o| o.target),
            Some(Target::File { path: canon(&page) }),
            "a page is drawn in the flyout"
        );
        assert_eq!(
            artifact_target(&code.to_string_lossy()).map(|o| o.target),
            Some(Target::File { path: canon(&code) })
        );
        assert_eq!(
            artifact_target(&clip.to_string_lossy()),
            None,
            "a clip plays in the square"
        );
        assert_eq!(
            artifact_target(&dir.join("gone.html").to_string_lossy()),
            None
        );
        assert_eq!(artifact_target("https://example.com/notes"), None);

        assert!(side_offers(url));
        assert!(side_offers("/home/p/reports/a.html"));
        assert!(side_offers("file:///home/p/reports/a%20brief.md"));
        assert!(side_offers("/home/p/src/main.rs"));
        assert!(!side_offers("/home/p/cut.mp4"));
        assert!(!side_offers("https://example.com/notes"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A path too long for its row keeps its end — the file's name and the
    /// folders nearest it — and the letters a filter lit stay lit there.
    #[test]
    fn a_deep_path_keeps_its_end_and_its_lit_letters() {
        let p = "mobile/app/android/app/src/main/kotlin/ca/brownfamilysports/terminal_delight/MainActivity.kt";
        let at = flyout::find_path(p, "mainact").unwrap().at;
        let (shown, lit) = elide_path(p, 40, &at);
        assert!(shown.starts_with("\u{2026}/"), "{shown}");
        assert!(
            shown.ends_with("terminal_delight/MainActivity.kt"),
            "{shown}"
        );
        assert!(shown.chars().count() <= 40, "{shown}");
        let lit_text: String = lit
            .iter()
            .map(|&i| shown[i..].chars().next().unwrap())
            .collect();
        assert_eq!(lit_text, "MainAct");
        assert_eq!(
            elide_path("a/b.rs", 40, &[2]),
            ("a/b.rs".to_string(), vec![2])
        );
    }

    /// A window with the flyout open: a fixture shared by the window tests.
    fn flyout_window(
        cx: &mut gpui::TestAppContext,
    ) -> (gpui::Entity<Workspace>, &mut gpui::VisualTestContext) {
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
        (ws, cx)
    }

    fn redraw(cx: &mut gpui::VisualTestContext) {
        for _ in 0..2 {
            cx.run_until_parked();
            cx.update(|w, cx| {
                w.refresh();
                w.draw(cx).clear();
            });
        }
        cx.run_until_parked();
    }

    /// A tab's ✕ sits at its left and closes it. A Markdown file opens as a
    /// page drawn by the floating square's own view, `s` shows its text, and
    /// `/` finds in the text, every place counted, Enter stepping on.
    #[gpui::test]
    fn a_tab_closes_from_its_left_and_a_page_shows_its_source_and_finds(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx) = flyout_window(cx);
        redraw(cx);
        let dir = std::env::temp_dir().join(format!("td-flyout-page-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let code = dir.join("code.rs");
        let notes = dir.join("notes.md");
        std::fs::write(&code, "fn main() {}\n").unwrap();
        std::fs::write(&notes, "# Notes\n\none\ntwo\nthree two\n").unwrap();
        cx.update(|w, cx| {
            ws.update(cx, |ws, cx| {
                ws.flyout_toggle(w, cx);
                let task = ws.active;
                ws.flyout_show(task, Target::File { path: code.clone() }, None, true, w, cx);
                ws.flyout_show(
                    task,
                    Target::File {
                        path: notes.clone(),
                    },
                    None,
                    true,
                    w,
                    cx,
                );
            })
        });
        redraw(cx);
        let page = Target::File {
            path: notes.clone(),
        };
        ws.read_with(cx, |ws, _| {
            let Some(Live::File(f)) = ws.flyout.live.get(&page) else {
                panic!("no reading for the page");
            };
            assert!(f.face.is_some() && f.paged(), "a Markdown file opens drawn");
        });
        assert!(
            cx.debug_bounds("flyout-page").is_some(),
            "the page is drawn in the panel"
        );
        let tab = cx.debug_bounds("flyout-tab-0").expect("tab drawn");
        let x = cx
            .debug_bounds("flyout-tab-x-0")
            .expect("its close mark drawn");
        assert!(
            x.left() - tab.left() < px(10.) && x.right() < tab.center().x,
            "the close mark sits at the tab's left: {x:?} in {tab:?}"
        );

        cx.simulate_keystrokes("s");
        redraw(cx);
        assert!(cx.debug_bounds("flyout-page").is_none(), "s shows the text");
        assert!(
            cx.debug_bounds("flyout-find").is_some(),
            "with a find over it"
        );
        cx.simulate_keystrokes("/ t w o");
        redraw(cx);
        ws.read_with(cx, |ws, _| {
            let Some(Live::File(f)) = ws.flyout.live.get(&page) else {
                panic!();
            };
            assert!(f.find.typing);
            assert_eq!(f.find.query, "two");
            assert_eq!(f.find.hits.len(), 2, "{:?}", f.find.hits);
        });
        let at = |cx: &mut gpui::VisualTestContext| {
            ws.read_with(cx, |ws, _| match ws.flyout.live.get(&page) {
                Some(Live::File(f)) => f.find.at,
                _ => usize::MAX,
            })
        };
        let first = at(cx);
        cx.simulate_keystrokes("enter");
        assert_eq!(at(cx), (first + 1) % 2, "Enter steps to the next place");
        cx.simulate_keystrokes("escape");
        ws.read_with(cx, |ws, _| {
            let Some(Live::File(f)) = ws.flyout.live.get(&page) else {
                panic!();
            };
            assert!(
                f.find.query.is_empty() && !f.find.typing,
                "Esc puts the find away"
            );
            assert!(ws.flyout.focused, "and keeps the keyboard for a second Esc");
        });

        cx.simulate_click(x.center(), gpui::Modifiers::none());
        redraw(cx);
        ws.read_with(cx, |ws, _| {
            let tabs = &ws.side().unwrap().tabs;
            assert_eq!(tabs, &vec![page.clone()], "the ✕ closed the first tab");
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `open_document` "side" on the window: with the panel shut, the agent's
    /// page opens it without taking the keyboard; while the person reads
    /// another tab there, a second one goes in beside it and the answer is
    /// "held"; from a tab that is not on screen it waits in that tab's own
    /// panel and the window's panel is left as it was.
    ///
    /// Mutation-tested: dropping the `reading` branch (so a send always
    /// shows) fails the held assertions; dropping the not-on-screen branch
    /// puts the page in the wrong tab's panel.
    #[gpui::test]
    fn an_agent_opens_its_page_in_its_own_tabs_side_panel_and_never_over_the_reader(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx) = flyout_window(cx);
        redraw(cx);
        let dir = std::env::temp_dir().join(format!("td-flyout-side-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = |name: &str| {
            let p = dir.join(name);
            std::fs::write(&p, "# made\n").unwrap();
            Target::File { path: p }
        };
        let (report, second, later) = (file("report.md"), file("second.md"), file("later.md"));
        let pane_of = |ws: &Workspace, tab: usize| {
            let mut leaves = vec![];
            ws.tabs[tab].root.leaves(&mut leaves);
            leaves[0].entity_id()
        };

        let said = cx.update(|_, cx| {
            ws.update(cx, |ws, cx| {
                let pane = pane_of(ws, ws.active);
                ws.mcp_open_side(pane, report.clone(), cx)
            })
        });
        redraw(cx);
        assert!(said.starts_with("ok side"), "{said}");
        ws.read_with(cx, |ws, _| {
            assert!(ws.flyout.open, "the shut panel opens on the agent's page");
            assert!(!ws.flyout.focused, "and the keyboard stays where it was");
            assert_eq!(ws.side().unwrap().current(), Some(&report));
        });

        let code = dir.join("code.rs");
        std::fs::write(&code, "fn main() {}\n").unwrap();
        let code = Target::File { path: code };
        cx.update(|w, cx| {
            ws.update(cx, |ws, cx| {
                let task = ws.active;
                ws.flyout_show(task, code.clone(), None, false, w, cx);
            })
        });
        let said = cx.update(|_, cx| {
            ws.update(cx, |ws, cx| {
                let pane = pane_of(ws, ws.active);
                ws.mcp_open_side(pane, second.clone(), cx)
            })
        });
        assert!(said.starts_with("held side"), "{said}");
        ws.read_with(cx, |ws, _| {
            let side = ws.side().unwrap();
            assert_eq!(side.current(), Some(&code), "the reader stays on the code");
            assert!(side.tabs.contains(&second), "the page went in beside it");
        });

        let first_tab = cx.update(|w, cx| {
            ws.update(cx, |ws, cx| {
                let first = ws.active;
                ws.new_tab(w, cx);
                first
            })
        });
        redraw(cx);
        let said = cx.update(|_, cx| {
            ws.update(cx, |ws, cx| {
                assert_ne!(ws.active, first_tab, "a new tab is on screen");
                let pane = pane_of(ws, first_tab);
                ws.mcp_open_side(pane, later.clone(), cx)
            })
        });
        assert!(said.contains("not on screen"), "{said}");
        ws.read_with(cx, |ws, _| {
            assert!(
                ws.tabs[first_tab].side.tabs.contains(&later),
                "it waits in the agent's own tab's panel"
            );
            assert!(
                !ws.side().unwrap().tabs.contains(&later),
                "and not in the panel on screen"
            );
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `/` over a Changes list narrows it as letters are typed, best match
    /// first, and Esc gives every file back.
    #[gpui::test]
    fn a_changes_list_narrows_to_the_best_match_as_letters_are_typed(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let repo = Repo::new();
        repo.write("app/src/main.rs", "fn main() {}\n");
        repo.write("app/src/flyout_view.rs", "fn view() {}\n");
        repo.write("docs/fly/overview.md", "# fly\n");
        repo.commit();
        repo.write("app/src/main.rs", "fn main() { go() }\n");
        repo.write("app/src/flyout_view.rs", "fn view() { draw() }\n");
        repo.write("docs/fly/overview.md", "# fly over\n");
        let (ws, cx) = flyout_window(cx);
        redraw(cx);
        let key = Target::Changes {
            repo: repo.root.clone(),
            base: Base::Uncommitted,
        };
        cx.update(|w, cx| {
            ws.update(cx, |ws, cx| {
                ws.flyout_toggle(w, cx);
                let task = ws.active;
                ws.flyout_show(task, key.clone(), None, true, w, cx);
            })
        });
        redraw(cx);
        let rows = |cx: &mut gpui::VisualTestContext| {
            ws.read_with(cx, |ws, _| match ws.flyout.live.get(&key) {
                Some(Live::Changes(c)) => c
                    .list
                    .got
                    .as_ref()
                    .map(|l| {
                        c.filter
                            .rows(l.files.len())
                            .into_iter()
                            .map(|(i, _)| l.files[i].path.clone())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
                _ => Vec::new(),
            })
        };
        assert_eq!(rows(cx).len(), 3, "every changed file, before typing");
        assert!(
            cx.debug_bounds("flyout-filter").is_some(),
            "the filter is offered"
        );
        cx.simulate_keystrokes("/ f l y v");
        redraw(cx);
        assert_eq!(
            rows(cx),
            ["app/src/flyout_view.rs"],
            "letters strewn into another folder's words are no match"
        );
        cx.simulate_keystrokes("backspace backspace");
        redraw(cx);
        let r = rows(cx);
        assert_eq!(r.len(), 2, "{r:?}");
        assert_eq!(
            r[0], "app/src/flyout_view.rs",
            "the name that starts with it first"
        );
        cx.simulate_keystrokes("escape");
        redraw(cx);
        assert_eq!(rows(cx).len(), 3, "Esc gives every file back");
    }

    /// What an agent of the task handed over leads the `+` page, unopened;
    /// Enter opens it drawn beside the conversation, its tab knows who handed
    /// it over, and the pane's chip counts it opened.
    #[gpui::test]
    fn what_an_agent_handed_over_leads_the_plus_page_and_opens_drawn(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx) = flyout_window(cx);
        redraw(cx);
        let dir = std::env::temp_dir().join(format!("td-flyout-handed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("real")).unwrap();
        std::fs::write(dir.join("real/brief.md"), "# The brief\n\nWhat was made.\n").unwrap();
        // The agent names the file through a symlinked folder; the flyout
        // opens the file itself. The two must still be one handover.
        std::os::unix::fs::symlink(dir.join("real"), dir.join("link")).unwrap();
        let brief = std::fs::canonicalize(dir.join("real/brief.md")).unwrap();
        let key = dir.join("link/brief.md").to_string_lossy().into_owned();
        let pane = ws.read_with(cx, |ws, _| {
            let mut leaves = Vec::new();
            ws.tabs[ws.active].root.leaves(&mut leaves);
            leaves[0].clone()
        });
        cx.update(|w, cx| {
            pane.update(cx, |v, cx| {
                v.hand_over(
                    Some("The brief".into()),
                    format!("file://{key}"),
                    crate::handover::Source::Declared,
                    cx,
                );
            });
            ws.update(cx, |ws, cx| ws.flyout_toggle(w, cx));
        });
        redraw(cx);
        let rows = ws.read_with(cx, |ws, cx| ws.add_rows(cx));
        assert!(
            matches!(&rows[0], AddRow::Artifact { label, opened: false, .. } if label == "The brief"),
            "the handover leads the + page, not yet opened: {:?}",
            rows.first()
        );
        cx.simulate_keystrokes("enter");
        redraw(cx);
        let target = Target::File {
            path: brief.clone(),
        };
        ws.read_with(cx, |ws, cx| {
            assert_eq!(ws.side_current(), Some(target.clone()));
            let Some(Live::File(f)) = ws.flyout.live.get(&target) else {
                panic!("no reading for the brief");
            };
            assert!(f.paged(), "a Markdown brief opens drawn");
            assert_eq!(
                ws.delivered(&brief, cx).map(|(_, label)| label),
                Some("The brief".into()),
                "the tab knows who handed it over"
            );
            assert_eq!(
                ws.tab_glyph(&target, true, &theme::theme(cx), &skin::skin(cx, 1.), cx)
                    .0,
                "\u{2726}"
            );
            assert!(
                pane.read(cx).handovers().is_opened(&key),
                "and it counts as opened"
            );
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Under the composer, a press on the panel or on a file's find field
    /// takes nothing: the composer takes every key first, so a lit field
    /// promised typing that went to the card (found on the mouse tour). With
    /// the composer put away, the same press takes the keys.
    #[gpui::test]
    fn the_panel_and_its_fields_wait_while_a_card_holds_the_keys(cx: &mut gpui::TestAppContext) {
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
        let dir = std::env::temp_dir().join(format!("td-flyout-card-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("plan.rs");
        std::fs::write(&file, "fn one() {}\n").unwrap();
        let target = Target::File { path: file.clone() };
        cx.update(|w, cx| {
            ws.update(cx, |ws, cx| {
                ws.flyout_toggle(w, cx);
                let task = ws.active;
                ws.flyout_show(task, target.clone(), Some(1), false, w, cx);
                ws.flyout.focused = false;
                ws.open_composer(w, cx);
            })
        });
        draw(cx);
        draw(cx);
        let field = cx
            .debug_bounds("flyout-find")
            .expect("the find field is drawn");
        cx.simulate_click(field.center(), gpui::Modifiers::none());
        draw(cx);
        ws.read_with(cx, |ws, _| {
            assert!(!ws.flyout.focused, "the panel waits");
            assert!(!ws.search_typing(&target), "and so does its field");
            assert!(ws.composer.is_some(), "the composer stays up");
        });
        cx.update(|_, cx| ws.update(cx, |ws, _| ws.composer_put_away()));
        draw(cx);
        let field = cx
            .debug_bounds("flyout-find")
            .expect("the find field is drawn");
        cx.simulate_click(field.center(), gpui::Modifiers::none());
        draw(cx);
        ws.read_with(cx, |ws, _| {
            assert!(
                ws.flyout.focused,
                "with no card up the panel takes the keys"
            );
            assert!(ws.search_typing(&target), "and the field types");
        });
        let _ = std::fs::remove_dir_all(&dir);
    }
}
