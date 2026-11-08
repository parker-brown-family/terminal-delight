//! The right-bar flyout's model: what a tab holds, and the pure readers of
//! what `git` and `gh` print. Drawn by `flyout_view.rs`.
//!
//! The flyout sits beside the panes on the right (docs/plans/right-flyout).
//! Each left-bar task keeps its own tabs ([`Side`]), and each tab shows one
//! [`Target`]: a repository's changes, a file, or a GitHub pull request.
//! Everything here is a pure function of text, so it is tested without a
//! window, a repository or the network.
//!
//! **Unknown is not zero.** git counts no lines in a binary file, and that is
//! not "no lines changed": a binary file's counts are `None`. A pull request
//! whose additions `gh` did not report has `None` additions, not 0.

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// What a tab shows
// ---------------------------------------------------------------------------

/// The diff a Changes tab is read against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Base {
    /// Everything this branch changed since it left the default branch,
    /// committed or not: the pull request's view, and T3's default.
    #[default]
    Branch,
    /// Only what is not committed yet: the working tree and the index against
    /// `HEAD`.
    Uncommitted,
}

/// What one flyout tab shows. Two tabs showing one target are one tab
/// ([`Target::same_tab`]).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    /// A repository's changes, by its top-level folder.
    Changes {
        repo: PathBuf,
        #[serde(default)]
        base: Base,
    },
    /// A file on this disk.
    File { path: PathBuf },
    /// A GitHub pull request.
    Pull {
        owner: String,
        repo: String,
        number: u64,
    },
}

impl Target {
    /// Whether opening `other` should land on this tab rather than add one. A
    /// repository has one Changes tab whatever its base: the base is a toggle
    /// inside the tab, not a second tab.
    pub fn same_tab(&self, other: &Target) -> bool {
        match (self, other) {
            (Target::Changes { repo: a, .. }, Target::Changes { repo: b, .. }) => a == b,
            _ => self == other,
        }
    }

    /// The tab's short name: the repository's folder, the file's name, or the
    /// pull request's number.
    pub fn short_name(&self) -> String {
        match self {
            Target::Changes { repo, .. } => file_name(repo),
            Target::File { path } => file_name(path),
            Target::Pull { number, .. } => format!("#{number}"),
        }
    }

    /// The kind's glyph, drawn ahead of the name on the tab.
    pub fn glyph(&self) -> &'static str {
        match self {
            Target::Changes { .. } => "\u{b1}",
            Target::File { .. } => "\u{25a4}",
            Target::Pull { .. } => "\u{21c4}",
        }
    }

    /// The folder this target is about, for finding its project colour and
    /// running `git` in: a repository, a file's folder, or nothing for a pull
    /// request, which lives on GitHub.
    pub fn folder(&self) -> Option<&Path> {
        match self {
            Target::Changes { repo, .. } => Some(repo),
            Target::File { path } => path.parent(),
            Target::Pull { .. } => None,
        }
    }

    /// The pull request's web address.
    pub fn url(&self) -> Option<String> {
        match self {
            Target::Pull {
                owner,
                repo,
                number,
            } => Some(format!("https://github.com/{owner}/{repo}/pull/{number}")),
            _ => None,
        }
    }
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string_lossy().into_owned())
}

/// One left-bar task's flyout tabs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Side {
    pub tabs: Vec<Target>,
    /// Index into `tabs`; meaningless while `tabs` is empty.
    pub active: usize,
}

impl Side {
    /// Show `t`: the tab already showing it, else a new one after the
    /// current tab. A Changes tab asked for with a different base keeps its
    /// place and takes the new base. Returns the tab's index.
    pub fn open(&mut self, t: Target) -> usize {
        if let Some(i) = self.tabs.iter().position(|have| have.same_tab(&t)) {
            self.tabs[i] = t;
            self.active = i;
            return i;
        }
        let at = if self.tabs.is_empty() {
            0
        } else {
            (self.active + 1).min(self.tabs.len())
        };
        self.tabs.insert(at, t);
        self.active = at;
        at
    }

    /// Close tab `i`; the tab to its left takes over, or the new first one.
    pub fn close(&mut self, i: usize) -> Option<Target> {
        if i >= self.tabs.len() {
            return None;
        }
        let gone = self.tabs.remove(i);
        if self.active > i || (self.active == i && self.active > 0) {
            self.active -= 1;
        }
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
        Some(gone)
    }

    pub fn current(&self) -> Option<&Target> {
        self.tabs.get(self.active)
    }

    pub fn current_mut(&mut self) -> Option<&mut Target> {
        self.tabs.get_mut(self.active)
    }

    /// The next or previous tab, wrapping.
    pub fn step(&mut self, forward: bool) {
        let n = self.tabs.len();
        if n == 0 {
            return;
        }
        self.active = if forward {
            (self.active + 1) % n
        } else {
            (self.active + n - 1) % n
        };
    }

    pub fn saved(&self) -> Option<SavedSide> {
        (!self.tabs.is_empty()).then(|| SavedSide {
            tabs: self.tabs.clone(),
            active: self.active,
        })
    }

    pub fn restore(saved: Option<&SavedSide>) -> Side {
        let Some(s) = saved else {
            return Side::default();
        };
        let tabs = s.tabs.clone();
        let active = s.active.min(tabs.len().saturating_sub(1));
        Side { tabs, active }
    }
}

/// A task's flyout tabs on disk. Absent on every file written before the
/// flyout, which reads as no tabs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedSide {
    #[serde(default)]
    pub tabs: Vec<Target>,
    #[serde(default)]
    pub active: usize,
}

/// The window's flyout on disk: whether it was open and how wide. Absent is
/// a window that never opened it, which reads as closed at the default width.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedFlyout {
    #[serde(default)]
    pub open: bool,
    /// Logical pixels at scale 1.0; absent = never dragged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
}

// ---------------------------------------------------------------------------
// What changed: git's numstat and name-status
// ---------------------------------------------------------------------------

/// How a file changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    /// The file became a link, or a link a file.
    TypeChanged,
    /// Not tracked yet: new, and counted as added.
    Untracked,
    /// A letter this reader does not know. Drawn as such, never as Modified.
    Unknown,
}

impl Status {
    fn of_letter(s: &str) -> Status {
        match s.chars().next() {
            Some('A') => Status::Added,
            Some('M') => Status::Modified,
            Some('D') => Status::Deleted,
            Some('R') => Status::Renamed,
            Some('C') => Status::Copied,
            Some('T') => Status::TypeChanged,
            _ => Status::Unknown,
        }
    }

    /// The one-character mark drawn in a file row.
    pub fn mark(self) -> &'static str {
        match self {
            Status::Added | Status::Untracked => "+",
            Status::Modified => "\u{25cf}",
            Status::Deleted => "\u{2212}",
            Status::Renamed | Status::Copied => "\u{2192}",
            Status::TypeChanged => "~",
            Status::Unknown => "?",
        }
    }
}

/// One changed file in a list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileStat {
    /// The path now (the new name of a renamed file), relative to the repo.
    pub path: String,
    /// The old name of a renamed or copied file.
    pub old_path: Option<String>,
    pub status: Status,
    /// Lines added; `None` for a binary file, whose lines git does not count.
    pub added: Option<u64>,
    pub removed: Option<u64>,
}

impl FileStat {
    pub fn binary(&self) -> bool {
        self.added.is_none() && self.removed.is_none()
    }
}

/// One numstat row: lines added, lines removed, the path, and a renamed
/// file's old path.
type NumstatRow = (Option<u64>, Option<u64>, String, Option<String>);

/// `git diff --numstat -z -M`: `added\tremoved\tpath\0`, or for a rename
/// `added\tremoved\t\0old\0new\0`. A binary file counts `-\t-`.
fn parse_numstat_z(out: &str) -> Vec<NumstatRow> {
    let mut rows = Vec::new();
    let mut parts = out.split('\0');
    while let Some(head) = parts.next() {
        if head.is_empty() {
            continue;
        }
        let mut cols = head.splitn(3, '\t');
        let (Some(a), Some(r), Some(path)) = (cols.next(), cols.next(), cols.next()) else {
            continue;
        };
        let count = |c: &str| c.parse::<u64>().ok();
        if path.is_empty() {
            let old = parts.next().unwrap_or_default().to_string();
            let new = parts.next().unwrap_or_default().to_string();
            rows.push((count(a), count(r), new, Some(old)));
        } else {
            rows.push((count(a), count(r), path.to_string(), None));
        }
    }
    rows
}

/// `git diff --name-status -z -M`: `M\0path\0`, or `R087\0old\0new\0`.
fn parse_name_status_z(out: &str) -> Vec<(Status, String, Option<String>)> {
    let mut rows = Vec::new();
    let mut parts = out.split('\0').filter(|p| !p.is_empty());
    while let Some(letter) = parts.next() {
        let status = Status::of_letter(letter);
        let Some(first) = parts.next() else { break };
        if matches!(status, Status::Renamed | Status::Copied) {
            let new = parts.next().unwrap_or_default().to_string();
            rows.push((status, new, Some(first.to_string())));
        } else {
            rows.push((status, first.to_string(), None));
        }
    }
    rows
}

/// The changed files, from git's two readings and the untracked list, in
/// path order. `untracked` carries each new file's line count when it could
/// be read (`None` for a binary or unreadable one).
pub fn changed_files(
    numstat: &str,
    name_status: &str,
    untracked: &[(String, Option<u64>)],
) -> Vec<FileStat> {
    let statuses: std::collections::HashMap<String, (Status, Option<String>)> =
        parse_name_status_z(name_status)
            .into_iter()
            .map(|(s, path, old)| (path, (s, old)))
            .collect();
    let mut out: Vec<FileStat> = parse_numstat_z(numstat)
        .into_iter()
        .map(|(added, removed, path, old)| {
            let (status, old_path) = match statuses.get(&path) {
                Some((s, o)) => (*s, o.clone().or(old)),
                None if old.is_some() => (Status::Renamed, old),
                None => (Status::Unknown, None),
            };
            FileStat {
                path,
                old_path,
                status,
                added,
                removed,
            }
        })
        .collect();
    for (path, lines) in untracked {
        if out.iter().any(|f| &f.path == path) {
            continue;
        }
        out.push(FileStat {
            path: path.clone(),
            old_path: None,
            status: Status::Untracked,
            added: *lines,
            removed: lines.map(|_| 0),
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// The list's totals: lines added and removed over the files git counted.
/// Binary files add nothing to either, and say so in their own rows.
pub fn totals(files: &[FileStat]) -> (u64, u64) {
    files.iter().fold((0, 0), |(a, r), f| {
        (a + f.added.unwrap_or(0), r + f.removed.unwrap_or(0))
    })
}

/// Lines in a file read whole, the way git would count them as added.
pub fn count_lines(bytes: &[u8]) -> Option<u64> {
    if looks_binary(bytes) {
        return None;
    }
    let n = bytes.iter().filter(|b| **b == b'\n').count() as u64;
    Some(if bytes.last().is_some_and(|b| *b != b'\n') {
        n + 1
    } else {
        n
    })
}

/// Git's own test: a NUL in the first 8000 bytes makes a file binary.
pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|b| *b == 0)
}

/// The paths `git status --porcelain=v1 -z` lists, in its order. A rename
/// or a copy is followed by its old name, which is not listed.
pub fn status_paths(out: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut parts = out.split('\0');
    while let Some(entry) = parts.next() {
        if entry.len() < 4 {
            continue;
        }
        let xy = &entry[..2];
        paths.push(entry[3..].to_string());
        if xy.contains('R') || xy.contains('C') {
            parts.next();
        }
    }
    paths
}

/// How well `path` answers a typed `query`, higher being better; `None` when
/// it does not answer it at all. The file's own name counts for more than its
/// folders, and letters in order count for something.
pub fn file_score(path: &str, query: &str) -> Option<i64> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Some(0);
    }
    let lower = path.to_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    let short = -(path.len() as i64) / 8;
    if name == q {
        return Some(10_000 + short);
    }
    if name.starts_with(&q) {
        return Some(8_000 + short);
    }
    if let Some(at) = name.find(&q) {
        return Some(6_000 - at as i64 + short);
    }
    if let Some(at) = lower.find(&q) {
        return Some(4_000 - at as i64 / 4 + short);
    }
    // Letters in order, anywhere: `mrs` finds `main.rs`.
    let mut gaps = 0i64;
    let mut it = lower.chars();
    for c in q.chars() {
        let mut found = false;
        for d in it.by_ref() {
            if d == c {
                found = true;
                break;
            }
            gaps += 1;
        }
        if !found {
            return None;
        }
    }
    Some(2_000 - gaps.min(1_900) + short)
}

// ---------------------------------------------------------------------------
// The diff itself: unified format
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
    /// `\ No newline at end of file`.
    NoNewline,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    /// The line's number in the old file; `None` on an added line.
    pub old: Option<u32>,
    /// Its number in the new file; `None` on a removed line.
    pub new: Option<u32>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    /// What git prints after the second `@@`: usually the enclosing function.
    pub section: String,
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDiff {
    /// `None` for an added file.
    pub old_path: Option<String>,
    /// `None` for a deleted file.
    pub new_path: Option<String>,
    pub status: Status,
    pub binary: bool,
    pub hunks: Vec<Hunk>,
}

impl FileDiff {
    /// The path a person knows the file by: its new name, or its old one if
    /// it was deleted.
    pub fn path(&self) -> &str {
        self.new_path
            .as_deref()
            .or(self.old_path.as_deref())
            .unwrap_or_default()
    }

    /// Lines added and removed, counted from the hunks.
    pub fn counts(&self) -> (u64, u64) {
        let mut a = 0;
        let mut r = 0;
        for l in self.hunks.iter().flat_map(|h| &h.lines) {
            match l.kind {
                LineKind::Added => a += 1,
                LineKind::Removed => r += 1,
                _ => {}
            }
        }
        (a, r)
    }

    /// As a list row, for a pull request's file list.
    pub fn stat(&self) -> FileStat {
        let (a, r) = self.counts();
        FileStat {
            path: self.path().to_string(),
            old_path: match self.status {
                Status::Renamed | Status::Copied => self.old_path.clone(),
                _ => None,
            },
            status: self.status,
            added: (!self.binary).then_some(a),
            removed: (!self.binary).then_some(r),
        }
    }
}

/// git's quoting of an unusual path: `"a/sp\303\251cial\tname"`. A path
/// without quotes is returned as it is.
pub fn unquote(s: &str) -> String {
    let Some(inner) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return s.to_string();
    };
    let mut bytes = Vec::with_capacity(inner.len());
    let raw = inner.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] != b'\\' || i + 1 >= raw.len() {
            bytes.push(raw[i]);
            i += 1;
            continue;
        }
        let c = raw[i + 1];
        match c {
            b'0'..=b'7' => {
                let digits: Vec<u8> = raw[i + 1..]
                    .iter()
                    .take(3)
                    .take_while(|d| (b'0'..=b'7').contains(d))
                    .copied()
                    .collect();
                let v = digits
                    .iter()
                    .fold(0u32, |acc, d| acc * 8 + u32::from(d - b'0'));
                bytes.push(v as u8);
                i += 1 + digits.len();
                continue;
            }
            b'n' => bytes.push(b'\n'),
            b't' => bytes.push(b'\t'),
            b'r' => bytes.push(b'\r'),
            b'"' => bytes.push(b'"'),
            b'\\' => bytes.push(b'\\'),
            b'a' => bytes.push(7),
            b'b' => bytes.push(8),
            b'f' => bytes.push(12),
            b'v' => bytes.push(11),
            other => {
                bytes.push(b'\\');
                bytes.push(other);
            }
        }
        i += 2;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// A `--- a/x` or `+++ b/x` path: `None` for `/dev/null`, the prefix taken off.
fn side_path(rest: &str) -> Option<String> {
    let p = unquote(rest.trim_end_matches('\t').trim_end());
    if p == "/dev/null" {
        return None;
    }
    Some(
        p.strip_prefix("a/")
            .or_else(|| p.strip_prefix("b/"))
            .unwrap_or(&p)
            .to_string(),
    )
}

/// `@@ -12,7 +12,9 @@ fn name` → (12, 12, "fn name").
fn hunk_header(line: &str) -> Option<(u32, u32, String)> {
    let rest = line.strip_prefix("@@ ")?;
    let (ranges, section) = rest.split_once(" @@").unwrap_or((rest, ""));
    let mut it = ranges.split_whitespace();
    let old = it.next()?.strip_prefix('-')?;
    let new = it.next()?.strip_prefix('+')?;
    let start = |r: &str| r.split(',').next().and_then(|n| n.parse::<u32>().ok());
    Some((start(old)?, start(new)?, section.trim().to_string()))
}

/// Every file in a unified diff, as `git diff` and `gh pr diff` print it.
pub fn parse_unified(text: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut old_no = 0u32;
    let mut new_no = 0u32;
    let fresh = || FileDiff {
        old_path: None,
        new_path: None,
        status: Status::Modified,
        binary: false,
        hunks: Vec::new(),
    };
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let mut f = fresh();
            // The header names both sides; `---`/`+++` correct it below, and
            // a binary or mode-only change has nothing else to go on.
            if let Some((a, b)) = split_git_header(rest) {
                f.old_path = Some(a);
                f.new_path = Some(b);
            }
            files.push(f);
            continue;
        }
        let Some(f) = files.last_mut() else {
            continue;
        };
        // Header lines come before a file's first hunk. After it, a line
        // reading `--- x` is a removed line that began with two dashes.
        if f.hunks.is_empty() {
            if let Some(rest) = line.strip_prefix("--- ") {
                f.old_path = side_path(rest);
                if f.old_path.is_none() {
                    f.status = Status::Added;
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("+++ ") {
                f.new_path = side_path(rest);
                if f.new_path.is_none() {
                    f.status = Status::Deleted;
                }
                continue;
            }
            if line.starts_with("new file mode") {
                f.status = Status::Added;
                continue;
            }
            if line.starts_with("deleted file mode") {
                f.status = Status::Deleted;
                continue;
            }
            if let Some(p) = line.strip_prefix("rename from ") {
                f.status = Status::Renamed;
                f.old_path = Some(unquote(p));
                continue;
            }
            if let Some(p) = line.strip_prefix("rename to ") {
                f.status = Status::Renamed;
                f.new_path = Some(unquote(p));
                continue;
            }
            if let Some(p) = line.strip_prefix("copy from ") {
                f.status = Status::Copied;
                f.old_path = Some(unquote(p));
                continue;
            }
            if let Some(p) = line.strip_prefix("copy to ") {
                f.status = Status::Copied;
                f.new_path = Some(unquote(p));
                continue;
            }
            if line.starts_with("Binary files ") || line.starts_with("GIT binary patch") {
                f.binary = true;
                continue;
            }
        }
        if line.starts_with("@@ ") {
            if let Some((o, n, section)) = hunk_header(line) {
                old_no = o;
                new_no = n;
                f.hunks.push(Hunk {
                    section,
                    old_start: o,
                    new_start: n,
                    lines: Vec::new(),
                });
            }
            continue;
        }
        let Some(h) = f.hunks.last_mut() else {
            continue;
        };
        let (kind, body) = match line.as_bytes().first() {
            Some(b'+') => (LineKind::Added, &line[1..]),
            Some(b'-') => (LineKind::Removed, &line[1..]),
            Some(b' ') => (LineKind::Context, &line[1..]),
            Some(b'\\') => (LineKind::NoNewline, line),
            // An empty line inside a hunk is a context line whose one space
            // an editor or a mail client stripped.
            None => (LineKind::Context, ""),
            _ => continue,
        };
        let (old, new) = match kind {
            LineKind::Added => {
                new_no += 1;
                (None, Some(new_no - 1))
            }
            LineKind::Removed => {
                old_no += 1;
                (Some(old_no - 1), None)
            }
            LineKind::Context => {
                old_no += 1;
                new_no += 1;
                (Some(old_no - 1), Some(new_no - 1))
            }
            LineKind::NoNewline => (None, None),
        };
        h.lines.push(DiffLine {
            kind,
            old,
            new,
            text: body.to_string(),
        });
    }
    files
}

/// `a/x b/y` from a `diff --git` line, quoted or not. Ambiguous for an
/// unquoted path holding ` b/`; the `---`/`+++` lines settle those.
fn split_git_header(rest: &str) -> Option<(String, String)> {
    if let Some(quoted) = rest.strip_prefix('"') {
        // `"a/..." "b/..."` or `"a/..." b/...`
        let close = quoted.find("\" ").map(|i| i + 1)?;
        let a = unquote(&rest[..=close]);
        let b = unquote(rest[close + 2..].trim());
        return Some((strip_side(&a), strip_side(&b)));
    }
    let at = rest.find(" b/")?;
    let a = &rest[..at];
    let b = unquote(rest[at + 1..].trim());
    Some((strip_side(a), strip_side(&b)))
}

fn strip_side(p: &str) -> String {
    p.strip_prefix("a/")
        .or_else(|| p.strip_prefix("b/"))
        .unwrap_or(p)
        .to_string()
}

// ---------------------------------------------------------------------------
// The gutter: which lines of a file changed
// ---------------------------------------------------------------------------

/// How a line of the file on disk differs from `HEAD`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// A line that is new.
    Added,
    /// A line that replaced one or more old lines.
    Modified,
    /// Lines were removed just above this one.
    RemovedAbove,
}

/// The new-side line numbers of a file that changed, from its hunks. A run of
/// removals followed by additions is a modification; additions alone are new
/// lines; removals alone mark the line that now stands where they were.
pub fn line_marks(hunks: &[Hunk]) -> BTreeMap<u32, Mark> {
    let mut marks = BTreeMap::new();
    for h in hunks {
        let mut i = 0;
        let lines = &h.lines;
        // The new-side number the next line would have.
        let mut next_new = h.new_start.max(1);
        while i < lines.len() {
            match lines[i].kind {
                LineKind::Context => {
                    next_new = lines[i].new.map_or(next_new, |n| n + 1);
                    i += 1;
                }
                LineKind::NoNewline => i += 1,
                LineKind::Added | LineKind::Removed => {
                    let start = i;
                    while i < lines.len()
                        && matches!(
                            lines[i].kind,
                            LineKind::Added | LineKind::Removed | LineKind::NoNewline
                        )
                    {
                        i += 1;
                    }
                    let block = &lines[start..i];
                    let removed = block.iter().any(|l| l.kind == LineKind::Removed);
                    let added: Vec<u32> = block
                        .iter()
                        .filter(|l| l.kind == LineKind::Added)
                        .filter_map(|l| l.new)
                        .collect();
                    if added.is_empty() {
                        marks.entry(next_new.max(1)).or_insert(Mark::RemovedAbove);
                    } else {
                        let mark = if removed { Mark::Modified } else { Mark::Added };
                        for n in &added {
                            marks.insert(*n, mark);
                        }
                        next_new = added.last().map_or(next_new, |n| n + 1);
                    }
                }
            }
        }
    }
    marks
}

// ---------------------------------------------------------------------------
// GitHub: addresses and remotes
// ---------------------------------------------------------------------------

/// `https://github.com/OWNER/REPO/pull/N`, with or without a trailing page
/// (`/files`), a query or a fragment.
pub fn pull_of_url(url: &str) -> Option<(String, String, u64)> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    let rest = rest.strip_prefix("github.com/")?;
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let mut parts = rest.split('/');
    let owner = parts.next().filter(|s| !s.is_empty())?;
    let repo = parts.next().filter(|s| !s.is_empty())?;
    if parts.next()? != "pull" {
        return None;
    }
    let number = parts.next()?.parse::<u64>().ok().filter(|n| *n > 0)?;
    Some((owner.to_string(), repo.to_string(), number))
}

/// A remote's GitHub `(owner, repo)`: `git@github.com:o/r.git`, an SSH host
/// alias that says github (`git@github-work:o/r.git`), `ssh://` and
/// `https://` forms. `None` for anything that is not GitHub.
pub fn github_slug(remote: &str) -> Option<(String, String)> {
    let remote = remote.trim();
    let path = if let Some(rest) = remote
        .strip_prefix("https://")
        .or_else(|| remote.strip_prefix("http://"))
    {
        let (host, path) = rest.split_once('/')?;
        let host = host.rsplit('@').next().unwrap_or(host);
        if !host
            .trim_start_matches("www.")
            .eq_ignore_ascii_case("github.com")
        {
            return None;
        }
        path
    } else if let Some(rest) = remote.strip_prefix("ssh://") {
        let (host, path) = rest.split_once('/')?;
        let host = host.rsplit('@').next().unwrap_or(host);
        let host = host.split(':').next().unwrap_or(host);
        if !host.to_ascii_lowercase().contains("github") {
            return None;
        }
        path
    } else {
        let (user_host, path) = remote.split_once(':')?;
        let host = user_host.rsplit('@').next().unwrap_or(user_host);
        if !host.to_ascii_lowercase().contains("github") {
            return None;
        }
        path
    };
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut it = path.split('/');
    let owner = it.next().filter(|s| !s.is_empty())?;
    let repo = it.next().filter(|s| !s.is_empty())?;
    it.next()
        .is_none()
        .then(|| (owner.to_string(), repo.to_string()))
}

/// A `path:line` or `path:line:col` reference, as compilers and agents print
/// them: the path, and the line when one was given.
pub fn split_line_ref(s: &str) -> (&str, Option<u32>) {
    let mut path = s;
    let mut line = None;
    for _ in 0..2 {
        if let Some((head, tail)) = path.rsplit_once(':') {
            if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
                line = tail.parse::<u32>().ok().filter(|n| *n > 0).or(line);
                path = head;
                continue;
            }
        }
        break;
    }
    (path, line)
}

// ---------------------------------------------------------------------------
// A pull request, from `gh pr view --json`
// ---------------------------------------------------------------------------

/// The fields asked of `gh pr view --json`.
pub const PULL_FIELDS: &str = "number,title,state,isDraft,author,url,baseRefName,headRefName,\
additions,deletions,changedFiles,body,createdAt,updatedAt,reviewDecision,mergeable,labels,\
reviewRequests,statusCheckRollup,comments,reviews,files,commits";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PullState {
    Open,
    Draft,
    Merged,
    Closed,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckState {
    Passed,
    Failed,
    Running,
    Skipped,
    Neutral,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    pub state: CheckState,
    pub url: Option<String>,
}

/// A comment, or a review with words or a verdict, in the conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remark {
    pub author: String,
    pub body: String,
    /// Milliseconds since the epoch, when the timestamp could be read.
    pub at_ms: Option<i64>,
    /// A review's verdict (`APPROVED`, `CHANGES_REQUESTED`, ...), `None` for a
    /// plain comment.
    pub verdict: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullFile {
    pub path: String,
    pub added: Option<u64>,
    pub removed: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pull {
    pub number: u64,
    pub title: String,
    pub state: PullState,
    pub author: Option<String>,
    pub url: Option<String>,
    pub base: Option<String>,
    pub head: Option<String>,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub changed_files: Option<u64>,
    pub commits: Option<u64>,
    pub body: String,
    pub created_ms: Option<i64>,
    pub updated_ms: Option<i64>,
    /// `APPROVED`, `CHANGES_REQUESTED`, `REVIEW_REQUIRED`; `None` when GitHub
    /// says nothing.
    pub review: Option<String>,
    /// `MERGEABLE`, `CONFLICTING`, `UNKNOWN`.
    pub mergeable: Option<String>,
    pub labels: Vec<String>,
    pub reviewers: Vec<String>,
    pub checks: Vec<Check>,
    pub remarks: Vec<Remark>,
    pub files: Vec<PullFile>,
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn n(v: &Value, k: &str) -> Option<u64> {
    v.get(k).and_then(Value::as_u64)
}

fn login(v: &Value) -> Option<String> {
    v.get("author")
        .and_then(|a| s(a, "login").or_else(|| s(a, "name")))
}

pub fn parse_pull(json: &str) -> Result<Pull, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let number = n(&v, "number").ok_or("no pull request number in gh's answer")?;
    let draft = v.get("isDraft").and_then(Value::as_bool).unwrap_or(false);
    let state = match s(&v, "state").as_deref() {
        Some("OPEN") if draft => PullState::Draft,
        Some("OPEN") => PullState::Open,
        Some("MERGED") => PullState::Merged,
        Some("CLOSED") => PullState::Closed,
        _ => PullState::Unknown,
    };
    let list = |k: &str| {
        v.get(k)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let labels = list("labels").iter().filter_map(|l| s(l, "name")).collect();
    let reviewers = list("reviewRequests")
        .iter()
        .filter_map(|r| {
            s(r, "login")
                .or_else(|| s(r, "name"))
                .or_else(|| s(r, "slug"))
        })
        .collect();
    let checks = list("statusCheckRollup")
        .iter()
        .map(|c| {
            let name = s(c, "name")
                .or_else(|| s(c, "context"))
                .unwrap_or_else(|| "?".into());
            let state = check_state(
                s(c, "status").as_deref(),
                s(c, "conclusion").as_deref(),
                s(c, "state").as_deref(),
            );
            Check {
                name,
                state,
                url: s(c, "detailsUrl").or_else(|| s(c, "targetUrl")),
            }
        })
        .collect();
    let mut remarks: Vec<Remark> = list("comments")
        .iter()
        .map(|c| Remark {
            author: login(c).unwrap_or_else(|| "?".into()),
            body: s(c, "body").unwrap_or_default(),
            at_ms: s(c, "createdAt").and_then(|t| rfc3339_ms(&t)),
            verdict: None,
        })
        .collect();
    for r in list("reviews") {
        let verdict = s(&r, "state");
        let body = s(&r, "body").unwrap_or_default();
        // A review that only commented on lines says nothing at this level.
        if body.is_empty() && matches!(verdict.as_deref(), Some("COMMENTED") | None) {
            continue;
        }
        remarks.push(Remark {
            author: login(&r).unwrap_or_else(|| "?".into()),
            body,
            at_ms: s(&r, "submittedAt").and_then(|t| rfc3339_ms(&t)),
            verdict,
        });
    }
    remarks.sort_by_key(|r| r.at_ms.unwrap_or(i64::MAX));
    let files = list("files")
        .iter()
        .filter_map(|f| {
            Some(PullFile {
                path: s(f, "path")?,
                added: n(f, "additions"),
                removed: n(f, "deletions"),
            })
        })
        .collect();
    Ok(Pull {
        number,
        title: s(&v, "title").unwrap_or_default(),
        state,
        author: login(&v),
        url: s(&v, "url"),
        base: s(&v, "baseRefName"),
        head: s(&v, "headRefName"),
        additions: n(&v, "additions"),
        deletions: n(&v, "deletions"),
        changed_files: n(&v, "changedFiles"),
        commits: v
            .get("commits")
            .and_then(Value::as_array)
            .map(|c| c.len() as u64),
        body: s(&v, "body").unwrap_or_default(),
        created_ms: s(&v, "createdAt").and_then(|t| rfc3339_ms(&t)),
        updated_ms: s(&v, "updatedAt").and_then(|t| rfc3339_ms(&t)),
        review: s(&v, "reviewDecision"),
        mergeable: s(&v, "mergeable"),
        labels,
        reviewers,
        checks,
        remarks,
        files,
    })
}

/// A check run's `status`/`conclusion`, or a status context's `state`.
fn check_state(status: Option<&str>, conclusion: Option<&str>, state: Option<&str>) -> CheckState {
    if let Some(c) = conclusion {
        return match c {
            "SUCCESS" => CheckState::Passed,
            "FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" | "STARTUP_FAILURE" => {
                CheckState::Failed
            }
            "SKIPPED" => CheckState::Skipped,
            "NEUTRAL" | "STALE" => CheckState::Neutral,
            _ => CheckState::Unknown,
        };
    }
    if let Some(s) = status {
        return match s {
            "QUEUED" | "IN_PROGRESS" | "PENDING" | "WAITING" | "REQUESTED" => CheckState::Running,
            "COMPLETED" => CheckState::Unknown,
            _ => CheckState::Unknown,
        };
    }
    match state {
        Some("SUCCESS") => CheckState::Passed,
        Some("FAILURE") | Some("ERROR") => CheckState::Failed,
        Some("PENDING") | Some("EXPECTED") => CheckState::Running,
        _ => CheckState::Unknown,
    }
}

/// One pull request in a list (`gh pr list --json`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRow {
    pub number: u64,
    pub title: String,
    pub head: Option<String>,
    pub author: Option<String>,
    pub draft: bool,
    pub updated_ms: Option<i64>,
}

pub const PULL_ROW_FIELDS: &str = "number,title,headRefName,author,isDraft,updatedAt";

pub fn parse_pull_rows(json: &str) -> Result<Vec<PullRow>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let rows = v.as_array().ok_or("gh did not answer with a list")?;
    Ok(rows
        .iter()
        .filter_map(|r| {
            Some(PullRow {
                number: n(r, "number")?,
                title: s(r, "title").unwrap_or_default(),
                head: s(r, "headRefName"),
                author: login(r),
                draft: r.get("isDraft").and_then(Value::as_bool).unwrap_or(false),
                updated_ms: s(r, "updatedAt").and_then(|t| rfc3339_ms(&t)),
            })
        })
        .collect())
}

/// `2026-11-07T23:01:02Z` (or with an offset, or fractional seconds) as
/// milliseconds since the epoch. `None` for anything else.
pub fn rfc3339_ms(t: &str) -> Option<i64> {
    let b = t.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || (b[10] != b'T' && b[10] != b' ') {
        return None;
    }
    let num = |r: Range<usize>| t.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let mut rest = &t[19..];
    let mut ms = 0i64;
    if let Some(frac) = rest.strip_prefix('.') {
        let digits: String = frac.chars().take_while(|c| c.is_ascii_digit()).collect();
        let mut scaled = digits.chars().take(3).collect::<String>();
        while scaled.len() < 3 {
            scaled.push('0');
        }
        ms = scaled.parse().ok()?;
        rest = &frac[digits.len()..];
    }
    let offset_s = match rest {
        "Z" | "z" => 0,
        o if o.len() == 6 && (o.starts_with('+') || o.starts_with('-')) => {
            let sign = if o.starts_with('-') { -1 } else { 1 };
            let oh = o.get(1..3)?.parse::<i64>().ok()?;
            let om = o.get(4..6)?.parse::<i64>().ok()?;
            sign * (oh * 3600 + om * 60)
        }
        _ => return None,
    };
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    // Days from the civil date (Howard Hinnant's algorithm).
    let yy = if mo <= 2 { y - 1 } else { y };
    let era = if yy >= 0 { yy } else { yy - 399 } / 400;
    let yoe = yy - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 86_400 + h * 3600 + mi * 60 + sec) - offset_s) * 1000 + ms)
}

// ---------------------------------------------------------------------------
// Syntax: a small colourer, enough to read code by
// ---------------------------------------------------------------------------

/// What a run of code is, for its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tok {
    Keyword,
    String,
    Comment,
    Number,
    /// A name that starts with a capital: a type, usually.
    Type,
}

/// How one family of languages writes comments, strings and keywords.
#[derive(Debug)]
pub struct Syntax {
    pub line: &'static [&'static str],
    pub block: Option<(&'static str, &'static str)>,
    pub quotes: &'static [char],
    pub keywords: &'static [&'static str],
    /// Whether a capitalised name reads as a type.
    pub types: bool,
}

const RUST: Syntax = Syntax {
    line: &["//"],
    block: Some(("/*", "*/")),
    quotes: &['"'],
    keywords: &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
        "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait",
        "true", "type", "unsafe", "use", "where", "while", "Some", "None", "Ok", "Err",
    ],
    types: true,
};
const C_LIKE: Syntax = Syntax {
    line: &["//"],
    block: Some(("/*", "*/")),
    quotes: &['"', '\''],
    keywords: &[
        "auto",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "constexpr",
        "continue",
        "default",
        "delete",
        "do",
        "else",
        "enum",
        "extern",
        "false",
        "final",
        "for",
        "goto",
        "if",
        "inline",
        "interface",
        "namespace",
        "new",
        "null",
        "nullptr",
        "override",
        "package",
        "private",
        "protected",
        "public",
        "return",
        "sizeof",
        "static",
        "struct",
        "switch",
        "template",
        "this",
        "throw",
        "true",
        "try",
        "typedef",
        "union",
        "using",
        "virtual",
        "void",
        "volatile",
        "while",
        "import",
        "extends",
        "implements",
        "fun",
        "val",
        "var",
        "func",
        "go",
        "defer",
        "chan",
        "select",
        "range",
        "type",
        "map",
        "struct",
        "let",
        "guard",
    ],
    types: true,
};
const JS: Syntax = Syntax {
    line: &["//"],
    block: Some(("/*", "*/")),
    quotes: &['"', '\'', '`'],
    keywords: &[
        "async",
        "await",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "default",
        "delete",
        "do",
        "else",
        "enum",
        "export",
        "extends",
        "false",
        "finally",
        "for",
        "from",
        "function",
        "if",
        "import",
        "in",
        "instanceof",
        "interface",
        "let",
        "new",
        "null",
        "of",
        "return",
        "static",
        "super",
        "switch",
        "this",
        "throw",
        "true",
        "try",
        "type",
        "typeof",
        "undefined",
        "var",
        "void",
        "while",
        "yield",
        "implements",
        "readonly",
    ],
    types: true,
};
const PYTHON: Syntax = Syntax {
    line: &["#"],
    block: None,
    quotes: &['"', '\''],
    keywords: &[
        "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
        "elif", "else", "except", "False", "finally", "for", "from", "global", "if", "import",
        "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "True",
        "try", "while", "with", "yield", "self",
    ],
    types: true,
};
const SHELL: Syntax = Syntax {
    line: &["#"],
    block: None,
    quotes: &['"', '\''],
    keywords: &[
        "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac",
        "in", "function", "return", "local", "export", "set", "unset", "source", "echo", "exit",
        "readonly", "shift", "true", "false",
    ],
    types: false,
};
const CONFIG: Syntax = Syntax {
    line: &["#"],
    block: None,
    quotes: &['"', '\''],
    keywords: &["true", "false", "null", "yes", "no", "on", "off"],
    types: false,
};
const JSON: Syntax = Syntax {
    line: &[],
    block: None,
    quotes: &['"'],
    keywords: &["true", "false", "null"],
    types: false,
};
const LUA_SQL: Syntax = Syntax {
    line: &["--"],
    block: None,
    quotes: &['"', '\''],
    keywords: &[
        "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in",
        "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
        "select", "from", "where", "join", "left", "right", "inner", "outer", "on", "group", "by",
        "order", "insert", "into", "values", "update", "delete", "create", "table", "index",
        "drop", "alter", "as", "is", "null", "limit", "SELECT", "FROM", "WHERE", "JOIN", "ON",
        "GROUP", "BY", "ORDER", "INSERT", "INTO", "VALUES", "UPDATE", "DELETE", "CREATE", "TABLE",
        "AND", "OR", "NOT", "NULL", "AS", "LIMIT",
    ],
    types: false,
};
const CSS: Syntax = Syntax {
    line: &[],
    block: Some(("/*", "*/")),
    quotes: &['"', '\''],
    keywords: &["important", "inherit", "initial", "none", "auto"],
    types: false,
};
const MARKUP: Syntax = Syntax {
    line: &[],
    block: Some(("<!--", "-->")),
    quotes: &['"'],
    keywords: &[],
    types: false,
};

/// The colourer for a file, by its name. `None` for a file read as plain text.
pub fn syntax_for(path: &Path) -> Option<&'static Syntax> {
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    if matches!(name.as_str(), "makefile" | "dockerfile" | "justfile") {
        return Some(&SHELL);
    }
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" => &RUST,
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "java" | "kt" | "kts" | "go"
        | "swift" | "cs" | "scala" | "zig" | "dart" | "m" | "mm" => &C_LIKE,
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" => &JS,
        "py" | "pyi" => &PYTHON,
        "sh" | "bash" | "zsh" | "fish" | "nix" => &SHELL,
        "toml" | "yaml" | "yml" | "ini" | "conf" | "cfg" | "env" | "desktop" | "service" => &CONFIG,
        "json" | "jsonc" | "json5" | "lock" => &JSON,
        "lua" | "sql" => &LUA_SQL,
        "css" | "scss" | "less" => &CSS,
        "html" | "htm" | "xml" | "svg" | "vue" | "svelte" => &MARKUP,
        _ => return None,
    })
}

/// Whether each line STARTS inside a block comment, for every line of a file.
/// The colourer reads one line at a time and needs to know where it begins.
pub fn block_states(sy: &Syntax, lines: &[&str]) -> Vec<bool> {
    let mut out = Vec::with_capacity(lines.len());
    let mut inside = false;
    for line in lines {
        out.push(inside);
        let _ = tokens(sy, line, &mut inside);
    }
    out
}

fn ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The coloured runs of one line, as byte ranges. `inside` says whether the
/// line starts inside a block comment, and is left saying whether the next
/// one does. Plain runs are not listed.
pub fn tokens(sy: &Syntax, line: &str, inside: &mut bool) -> Vec<(Range<usize>, Tok)> {
    let mut out = Vec::new();
    let mut i = 0;
    let b = line.as_bytes();
    while i < line.len() {
        if *inside {
            let (_, close) = sy.block.expect("inside a block needs a block syntax");
            match line[i..].find(close) {
                Some(at) => {
                    let end = i + at + close.len();
                    out.push((i..end, Tok::Comment));
                    i = end;
                    *inside = false;
                }
                None => {
                    out.push((i..line.len(), Tok::Comment));
                    return out;
                }
            }
            continue;
        }
        let rest = &line[i..];
        if let Some((open, _)) = sy.block {
            if rest.starts_with(open) {
                *inside = true;
                out.push((i..i + open.len(), Tok::Comment));
                i += open.len();
                continue;
            }
        }
        if sy.line.iter().any(|c| rest.starts_with(c)) {
            out.push((i..line.len(), Tok::Comment));
            return out;
        }
        let c = rest.chars().next().unwrap_or(' ');
        if sy.quotes.contains(&c) {
            // A Rust lifetime ('a) is not a string: only a quote that closes
            // within a few characters counts as a char literal there.
            let mut j = i + c.len_utf8();
            let mut closed = false;
            while j < line.len() {
                let d = line[j..].chars().next().unwrap_or(' ');
                if d == '\\' {
                    j += 1 + line[j + 1..].chars().next().map_or(0, char::len_utf8);
                    continue;
                }
                j += d.len_utf8();
                if d == c {
                    closed = true;
                    break;
                }
            }
            if c == '\'' && std::ptr::eq(sy, &RUST) && !(closed && j - i <= 4) {
                i += 1;
                continue;
            }
            out.push((i..j.min(line.len()), Tok::String));
            i = j.min(line.len());
            continue;
        }
        if c.is_ascii_digit() && (i == 0 || !ident_char(line[..i].chars().last().unwrap_or(' '))) {
            let mut j = i;
            while j < line.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'.' || b[j] == b'_') {
                j += 1;
            }
            out.push((i..j, Tok::Number));
            i = j;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let mut j = i;
            while j < line.len() {
                let d = line[j..].chars().next().unwrap_or(' ');
                if !ident_char(d) {
                    break;
                }
                j += d.len_utf8();
            }
            let word = &line[i..j];
            if sy.keywords.contains(&word) {
                out.push((i..j, Tok::Keyword));
            } else if sy.types && word.chars().next().is_some_and(char::is_uppercase) {
                out.push((i..j, Tok::Type));
            }
            i = j;
            continue;
        }
        i += c.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rename_and_a_binary_file_are_read_from_numstat_and_name_status() {
        let numstat = "3\t1\tsrc/a.rs\0-\t-\tlogo.png\0\
                       2\t0\t\0old/name.rs\0new/name.rs\0";
        let names = "M\0src/a.rs\0A\0logo.png\0R087\0old/name.rs\0new/name.rs\0";
        let untracked = vec![("notes.md".to_string(), Some(4))];
        let files = changed_files(numstat, names, &untracked);
        assert_eq!(
            files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            ["logo.png", "new/name.rs", "notes.md", "src/a.rs"]
        );
        let png = &files[0];
        assert_eq!(png.status, Status::Added);
        assert!(
            png.binary(),
            "a binary file counts no lines, which is not zero"
        );
        let renamed = &files[1];
        assert_eq!(renamed.status, Status::Renamed);
        assert_eq!(renamed.old_path.as_deref(), Some("old/name.rs"));
        assert_eq!((renamed.added, renamed.removed), (Some(2), Some(0)));
        assert_eq!(files[2].status, Status::Untracked);
        assert_eq!(files[2].added, Some(4));
        assert_eq!(
            totals(&files),
            (9, 1),
            "the binary file adds nothing to the totals"
        );
    }

    #[test]
    fn an_untracked_binary_file_keeps_its_unknown_count() {
        let files = changed_files("", "", &[("blob.bin".into(), None)]);
        assert_eq!(files[0].added, None);
        assert_eq!(files[0].removed, None);
    }

    #[test]
    fn lines_are_counted_the_way_git_counts_an_added_file() {
        assert_eq!(count_lines(b"a\nb\n"), Some(2));
        assert_eq!(
            count_lines(b"a\nb"),
            Some(2),
            "a last line with no newline still counts"
        );
        assert_eq!(count_lines(b""), Some(0));
        assert_eq!(count_lines(b"a\0b"), None);
    }

    const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,4 +1,5 @@ mod top
 fn one() {}
-fn two() {}
+fn two() -> u8 { 2 }
+fn three() {}
 fn four() {}
--- this removed line looks like a header
@@ -10,3 +11,2 @@ fn later()
 a
-b
 c
diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,2 @@
+hello
+world
\\ No newline at end of file
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
diff --git a/img.png b/img.png
Binary files a/img.png and b/img.png differ
diff --git a/old.rs b/renamed.rs
similarity index 90%
rename from old.rs
rename to renamed.rs
";

    #[test]
    fn a_unified_diff_is_read_into_files_hunks_and_numbered_lines() {
        let files = parse_unified(DIFF);
        assert_eq!(files.len(), 5);
        let lib = &files[0];
        assert_eq!(lib.path(), "src/lib.rs");
        assert_eq!(lib.status, Status::Modified);
        assert_eq!(lib.hunks.len(), 2);
        let h = &lib.hunks[0];
        assert_eq!((h.old_start, h.new_start), (1, 1));
        assert_eq!(h.section, "mod top");
        let kinds: Vec<_> = h.lines.iter().map(|l| (l.kind, l.old, l.new)).collect();
        assert_eq!(
            kinds,
            [
                (LineKind::Context, Some(1), Some(1)),
                (LineKind::Removed, Some(2), None),
                (LineKind::Added, None, Some(2)),
                (LineKind::Added, None, Some(3)),
                (LineKind::Context, Some(3), Some(4)),
                (LineKind::Removed, Some(4), None),
            ]
        );
        assert_eq!(
            h.lines[5].text, "-- this removed line looks like a header",
            "a removed line that starts with dashes stays in its hunk"
        );
        assert_eq!(lib.counts(), (2, 3));

        let new = &files[1];
        assert_eq!((new.status, new.path()), (Status::Added, "new.txt"));
        assert_eq!(new.old_path, None);
        assert_eq!(new.hunks[0].lines.last().unwrap().kind, LineKind::NoNewline);
        assert_eq!(new.counts(), (2, 0));

        let gone = &files[2];
        assert_eq!((gone.status, gone.path()), (Status::Deleted, "gone.txt"));
        assert_eq!(gone.new_path, None);

        assert!(files[3].binary);
        assert_eq!(
            files[3].stat().added,
            None,
            "a binary file's count is unknown"
        );

        let renamed = &files[4];
        assert_eq!(renamed.status, Status::Renamed);
        assert_eq!(renamed.old_path.as_deref(), Some("old.rs"));
        assert_eq!(renamed.path(), "renamed.rs");
    }

    #[test]
    fn a_quoted_path_is_unquoted_including_its_octal_bytes() {
        assert_eq!(
            unquote("\"a/caf\\303\\251 \\\"x\\\".rs\""),
            "a/café \"x\".rs"
        );
        assert_eq!(unquote("plain/path.rs"), "plain/path.rs");
        let files = parse_unified(
            "diff --git \"a/sp ace.rs\" \"b/sp ace.rs\"\n--- \"a/sp ace.rs\"\n+++ \"b/sp ace.rs\"\n@@ -1 +1 @@\n-x\n+y\n",
        );
        assert_eq!(files[0].path(), "sp ace.rs");
    }

    #[test]
    fn the_gutter_tells_new_lines_from_changed_ones_and_marks_removals() {
        let files = parse_unified(DIFF);
        let marks = line_marks(&files[0].hunks);
        assert_eq!(marks.get(&2), Some(&Mark::Modified));
        assert_eq!(marks.get(&3), Some(&Mark::Modified));
        assert_eq!(marks.get(&1), None);
        assert_eq!(marks.get(&4), None);
        // `-b` between `a` (11) and `c` (12): the removal sits above line 12.
        assert_eq!(marks.get(&12), Some(&Mark::RemovedAbove));
        let added = line_marks(&files[1].hunks);
        assert_eq!(added.get(&1), Some(&Mark::Added));
        assert_eq!(added.get(&2), Some(&Mark::Added));
    }

    #[test]
    fn a_pull_request_address_is_read_with_or_without_its_page() {
        let want = Some(("parker-brown-family".to_string(), "t3code".to_string(), 1));
        assert_eq!(
            pull_of_url("https://github.com/parker-brown-family/t3code/pull/1"),
            want
        );
        assert_eq!(
            pull_of_url("https://github.com/parker-brown-family/t3code/pull/1/files#diff-x"),
            want
        );
        assert_eq!(
            pull_of_url("https://www.github.com/parker-brown-family/t3code/pull/1?w=1"),
            want
        );
        assert_eq!(pull_of_url("https://github.com/o/r/issues/1"), None);
        assert_eq!(pull_of_url("https://github.com/o/r/pull/0"), None);
        assert_eq!(pull_of_url("https://gitlab.com/o/r/pull/3"), None);
    }

    #[test]
    fn a_remote_names_its_github_repository_through_an_ssh_alias_too() {
        let want = Some(("parker-brown-family".into(), "terminal-delight".into()));
        for url in [
            "git@github.com:parker-brown-family/terminal-delight.git",
            "git@github-bfs:parker-brown-family/terminal-delight.git",
            "https://github.com/parker-brown-family/terminal-delight",
            "https://github.com/parker-brown-family/terminal-delight.git",
            "ssh://git@github.com/parker-brown-family/terminal-delight.git",
            "https://token@github.com/parker-brown-family/terminal-delight/",
        ] {
            assert_eq!(github_slug(url), want, "{url}");
        }
        assert_eq!(github_slug("git@gitlab.com:o/r.git"), None);
        assert_eq!(github_slug("https://example.com/o/r"), None);
        assert_eq!(github_slug("/home/parker/repo"), None);
    }

    #[test]
    fn a_path_with_a_line_and_column_gives_its_line() {
        assert_eq!(
            split_line_ref("src/main.rs:120"),
            ("src/main.rs", Some(120))
        );
        assert_eq!(
            split_line_ref("src/main.rs:120:7"),
            ("src/main.rs", Some(120))
        );
        assert_eq!(split_line_ref("src/main.rs"), ("src/main.rs", None));
        assert_eq!(split_line_ref("a:b/c.rs"), ("a:b/c.rs", None));
        assert_eq!(split_line_ref("x.rs:0"), ("x.rs", None));
    }

    #[test]
    fn a_pull_request_is_read_from_gh_with_absent_fields_left_unknown() {
        let json = r#"{
            "number": 1, "title": "A brief stops flashing", "state": "OPEN", "isDraft": false,
            "author": {"login": "parker-brown-family"}, "url": "https://github.com/o/r/pull/1",
            "baseRefName": "main", "headRefName": "brief-notes", "additions": 461,
            "changedFiles": 10, "body": "**Flash.** Fixed.",
            "createdAt": "2026-11-07T22:00:00Z", "updatedAt": "2026-11-07T23:00:00Z",
            "reviewDecision": "", "labels": [{"name": "ui"}],
            "reviewRequests": [{"login": "nixfred"}],
            "statusCheckRollup": [
              {"__typename": "CheckRun", "name": "rust", "status": "COMPLETED", "conclusion": "SUCCESS"},
              {"__typename": "CheckRun", "name": "lint", "status": "IN_PROGRESS", "conclusion": ""},
              {"__typename": "StatusContext", "context": "deploy", "state": "FAILURE"}
            ],
            "comments": [{"author": {"login": "a"}, "body": "looks good", "createdAt": "2026-11-07T22:30:00Z"}],
            "reviews": [
              {"author": {"login": "b"}, "state": "APPROVED", "body": "", "submittedAt": "2026-11-07T22:45:00Z"},
              {"author": {"login": "c"}, "state": "COMMENTED", "body": "", "submittedAt": "2026-11-07T22:50:00Z"}
            ],
            "files": [{"path": "a.rs", "additions": 3, "deletions": 1}],
            "commits": [{}, {}]
        }"#;
        let p = parse_pull(json).unwrap();
        assert_eq!(p.state, PullState::Open);
        assert_eq!(p.additions, Some(461));
        assert_eq!(p.deletions, None, "gh did not say, so it is not zero");
        assert_eq!(p.review, None, "an empty decision is no decision");
        assert_eq!(p.commits, Some(2));
        assert_eq!(
            p.checks.iter().map(|c| c.state).collect::<Vec<_>>(),
            [CheckState::Passed, CheckState::Running, CheckState::Failed]
        );
        assert_eq!(
            p.remarks.len(),
            2,
            "a review with neither words nor verdict is left out"
        );
        assert_eq!(p.remarks[1].verdict.as_deref(), Some("APPROVED"));
        assert_eq!(p.labels, ["ui"]);
        assert_eq!(p.reviewers, ["nixfred"]);
        let draft = parse_pull(r#"{"number": 2, "state": "OPEN", "isDraft": true}"#).unwrap();
        assert_eq!(draft.state, PullState::Draft);
        assert!(parse_pull("{}").is_err());
    }

    #[test]
    fn timestamps_are_read_to_the_millisecond_with_any_offset() {
        assert_eq!(rfc3339_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(rfc3339_ms("2026-11-08T00:00:00Z"), Some(1_794_096_000_000));
        assert_eq!(
            rfc3339_ms("2026-11-08T02:00:00+02:00"),
            rfc3339_ms("2026-11-08T00:00:00Z")
        );
        assert_eq!(
            rfc3339_ms("2026-11-08T00:00:00.250Z"),
            Some(1_794_096_000_250)
        );
        assert_eq!(rfc3339_ms("yesterday"), None);
    }

    #[test]
    fn a_task_keeps_one_tab_per_target_and_one_changes_tab_per_repo() {
        let mut side = Side::default();
        let repo = PathBuf::from("/r");
        side.open(Target::Changes {
            repo: repo.clone(),
            base: Base::Branch,
        });
        side.open(Target::Pull {
            owner: "o".into(),
            repo: "r".into(),
            number: 7,
        });
        let again = side.open(Target::Changes {
            repo: repo.clone(),
            base: Base::Uncommitted,
        });
        assert_eq!(again, 0);
        assert_eq!(side.tabs.len(), 2);
        assert_eq!(
            side.tabs[0],
            Target::Changes {
                repo,
                base: Base::Uncommitted
            },
            "the base is a toggle inside the tab"
        );
        side.step(false);
        assert_eq!(side.active, 1, "stepping back from the first wraps");
        side.close(1);
        assert_eq!(side.active, 0);
        side.close(0);
        assert!(side.current().is_none());
        assert_eq!(side.saved(), None, "no tabs saves nothing");
    }

    #[test]
    fn a_saved_side_survives_toml_and_a_bad_index_is_clamped() {
        let side = Side {
            tabs: vec![
                Target::File {
                    path: "/r/src/a.rs".into(),
                },
                Target::Pull {
                    owner: "o".into(),
                    repo: "r".into(),
                    number: 3,
                },
            ],
            active: 1,
        };
        #[derive(Serialize, Deserialize)]
        struct Holder {
            side: SavedSide,
        }
        let text = toml::to_string(&Holder {
            side: side.saved().unwrap(),
        })
        .unwrap();
        let back: Holder = toml::from_str(&text).unwrap();
        assert_eq!(Side::restore(Some(&back.side)), side);
        let wild = SavedSide {
            tabs: side.tabs.clone(),
            active: 9,
        };
        assert_eq!(Side::restore(Some(&wild)).active, 1);
    }

    #[test]
    fn code_is_coloured_by_keyword_string_comment_number_and_type() {
        let sy = syntax_for(Path::new("x.rs")).unwrap();
        let line = r#"let n: Vec<u8> = vec![1, 2]; // "not a string""#;
        let mut inside = false;
        let toks = tokens(sy, line, &mut inside);
        let said: Vec<(&str, Tok)> = toks.iter().map(|(r, t)| (&line[r.clone()], *t)).collect();
        assert!(said.contains(&("let", Tok::Keyword)));
        assert!(said.contains(&("Vec", Tok::Type)));
        assert!(said.contains(&("1", Tok::Number)));
        assert_eq!(said.last().unwrap().1, Tok::Comment);
        assert!(!said.iter().any(|(_, t)| *t == Tok::String), "{said:?}");
        // A lifetime is not a string.
        let life = "fn f<'a>(x: &'a str) -> &'a str { \"s\" }";
        let t = tokens(sy, life, &mut inside);
        let strings: Vec<&str> = t
            .iter()
            .filter(|(_, k)| *k == Tok::String)
            .map(|(r, _)| &life[r.clone()])
            .collect();
        assert_eq!(strings, ["\"s\""]);
    }

    #[test]
    fn a_block_comment_carries_across_lines() {
        let sy = syntax_for(Path::new("x.ts")).unwrap();
        let lines = ["let a = 1; /* start", "still inside", "end */ let b = 2;"];
        let states = block_states(sy, &lines);
        assert_eq!(states, [false, true, true]);
        let mut inside = true;
        let t = tokens(sy, lines[2], &mut inside);
        assert_eq!(t[0], (0..6, Tok::Comment));
        assert!(!inside);
        assert!(t
            .iter()
            .any(|(r, k)| *k == Tok::Keyword && &lines[2][r.clone()] == "let"));
    }

    #[test]
    fn status_lists_paths_and_skips_a_renamed_files_old_name() {
        let out = " M src/a.rs\0R  new.rs\0old.rs\0?? notes/\0A  b.rs\0";
        assert_eq!(status_paths(out), ["src/a.rs", "new.rs", "notes/", "b.rs"]);
    }

    #[test]
    fn the_finder_ranks_a_files_own_name_above_its_folders() {
        let mut paths = vec![
            "app/src/main.rs",
            "docs/main/notes.md",
            "app/src/domain.rs",
            "app/src/flyout_view.rs",
        ];
        paths.sort_by_key(|p| std::cmp::Reverse(file_score(p, "main")));
        assert_eq!(paths[0], "app/src/main.rs");
        assert!(
            file_score("app/src/main.rs", "mrs").is_some(),
            "letters in order"
        );
        assert_eq!(file_score("app/src/main.rs", "zzz"), None);
        assert_eq!(file_score("anything", "  "), Some(0));
        assert!(
            file_score("a/flyout_view.rs", "flyout") > file_score("flyout/a.rs", "flyout"),
            "a name that starts with the query beats a folder holding it"
        );
    }

    #[test]
    fn a_file_with_no_known_syntax_is_plain() {
        assert!(syntax_for(Path::new("notes.txt")).is_none());
        assert!(syntax_for(Path::new("Makefile")).is_some());
    }
}
