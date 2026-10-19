//! Git coordination is an observation graph, never an execution policy.
//! Collection runs inside engstate's background worker; projections and history
//! contain no GPUI or Git subprocess calls.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const MAX_PATHS: usize = 20_000;
pub const MAX_CONTEXTS: usize = 128;
pub const MAX_OUTPUT: usize = 4 * 1024 * 1024;
pub const HISTORY: usize = 32;
pub const PAGE: usize = 40;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    Complete,
    Partial,
    #[default]
    Unavailable,
}

impl Coverage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Clear,
    Caution,
    Warning,
    Alert,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Clear => "CLEAR",
            Self::Caution => "CAUTION",
            Self::Warning => "WARNING",
            Self::Alert => "ALERT",
        }
    }
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Clear => "✓",
            Self::Caution => "◇",
            Self::Warning => "▲",
            Self::Alert => "!",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participant {
    pub id: String,
    pub project_id: String,
    pub pane_id: Option<u64>,
    pub pid: Option<u32>,
    pub title: String,
    pub mode: String,
    pub agent: Option<bool>,
    pub session: Option<String>,
    #[serde(skip)]
    pub exposed: bool,
    /// A fully exposed window can publish Git facts. Restricted exposure
    /// publishes authorized occupancy with unknown path coverage: Git cannot
    /// attribute a shared filesystem change to the pane permitted to reveal it.
    #[serde(skip)]
    pub full_scope: bool,
}

/// A launch recipe such as `codex resume --last` is not a session identity.
pub fn session_identity(recipe: Option<&str>) -> Option<String> {
    let words: Vec<_> = recipe?.split_whitespace().collect();
    let at = words
        .iter()
        .position(|w| matches!(*w, "--resume" | "resume"))?;
    let value = *words.get(at + 1)?;
    let bytes = value.as_bytes();
    (bytes.len() == 36
        && bytes.iter().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                *b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        }))
    .then(|| format!("{}:{value}", words.first().copied().unwrap_or("agent")))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PathKey {
    pub display: String,
    /// Exact Git path bytes, so invalid UTF-8 cannot collapse distinct files.
    pub bytes_hex: String,
}

impl PathKey {
    pub fn new(bytes: &[u8]) -> Self {
        let bytes_hex = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let display = match std::str::from_utf8(bytes) {
            Ok(s) => s
                .chars()
                .map(|c| {
                    if c.is_control() || c == '\\' {
                        c.escape_default().to_string()
                    } else {
                        c.to_string()
                    }
                })
                .collect(),
            Err(_) => bytes
                .iter()
                .map(|b| match b {
                    b' '..=b'~' if *b != b'\\' => char::from(*b).to_string(),
                    _ => format!("\\x{b:02x}"),
                })
                .collect(),
        };
        Self { display, bytes_hex }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Committed,
    Staged,
    Unstaged,
    Untracked,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Self::Committed => "committed",
            Self::Staged => "staged",
            Self::Unstaged => "unstaged",
            Self::Untracked => "untracked",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathChange {
    pub path: PathKey,
    pub sources: BTreeSet<Source>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rename {
    pub from: PathKey,
    pub to: PathKey,
    pub source: Source,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetObservation {
    pub state: TargetState,
    pub target_oid: Option<String>,
    pub head_oid: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetState {
    #[default]
    NotChecked,
    CheckedNone,
    Conflict,
    Unavailable,
    Stale,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Footprint {
    pub paths: Vec<PathChange>,
    pub renames: Vec<Rename>,
    pub sources: BTreeMap<Source, Coverage>,
    pub head_oid: Option<String>,
    pub base_ref: Option<String>,
    pub base_oid: Option<String>,
    pub target_oid: Option<String>,
    pub common_dir: Option<String>,
    pub observed_at_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub errors: Vec<String>,
    pub truncated: bool,
    pub unmerged: Vec<PathKey>,
    pub target_check: TargetObservation,
}

impl Footprint {
    pub fn coverage(&self) -> Coverage {
        if self.sources.len() == 4
            && self.sources.values().all(|c| *c == Coverage::Complete)
            && self.errors.is_empty()
            && !self.truncated
            && self.common_dir.is_some()
        {
            Coverage::Complete
        } else if self.sources.values().any(|c| *c == Coverage::Complete) {
            Coverage::Partial
        } else {
            Coverage::Unavailable
        }
    }
    fn restricted() -> Self {
        Self {
            errors: vec!["File observations unavailable in this authorized scope".into()],
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    pub id: String,
    pub root: String,
    pub repository: Option<String>,
    pub branch: Option<String>,
    pub participants: Vec<Participant>,
    pub footprint: Footprint,
    pub dirty: Option<u32>,
    pub untracked: Option<u32>,
    pub delta: Option<(u32, u32)>,
    pub ahead_behind: Option<(u32, u32)>,
    pub target_conflicts: Option<Vec<PathKey>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub contexts: Vec<Context>,
    pub unlocated: Vec<Participant>,
    pub truncated: bool,
}

#[derive(Clone, Debug)]
pub struct Reading {
    pub operator: Project,
    pub authorized: Option<Project>,
    pub scope_key: String,
}

pub fn unix_ms() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

pub fn stable_id(kind: &str, parts: &[&str]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for part in parts {
        for byte in (part.len() as u64)
            .to_le_bytes()
            .iter()
            .chain(part.as_bytes())
        {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
    }
    format!("{kind}-{hash:016x}")
}

type Changes = BTreeMap<PathKey, BTreeSet<Source>>;
fn add_path(paths: &mut Changes, bytes: &[u8], source: Source) -> Result<(), String> {
    if bytes.is_empty() {
        return Err("Empty path in Git output".into());
    }
    let key = PathKey::new(bytes);
    if paths.len() >= MAX_PATHS && !paths.contains_key(&key) {
        return Err("Path limit reached".into());
    }
    paths.entry(key).or_default().insert(source);
    Ok(())
}

fn tokens(bytes: &[u8]) -> Result<Vec<&[u8]>, String> {
    if bytes.is_empty() {
        return Ok(vec![]);
    }
    if bytes.last() != Some(&0) {
        return Err("Incomplete NUL-delimited Git output".into());
    }
    Ok(bytes[..bytes.len() - 1].split(|b| *b == 0).collect())
}

fn parse_status(
    bytes: &[u8],
    paths: &mut Changes,
    renames: &mut Vec<Rename>,
    unmerged: &mut Vec<PathKey>,
) -> Result<(), String> {
    let fields = tokens(bytes)?;
    let mut at = 0;
    while let Some(field) = fields.get(at) {
        at += 1;
        if field.len() < 4 || field[2] != b' ' {
            return Err("Malformed porcelain record".into());
        }
        let (x, y, path) = (field[0], field[1], &field[3..]);
        if x == b'?' && y == b'?' {
            add_path(paths, path, Source::Untracked)?;
            continue;
        }
        if ![x, y].iter().all(|c| b" MADRCUT".contains(c)) {
            return Err("Unsupported porcelain status".into());
        }
        if x != b' ' {
            add_path(paths, path, Source::Staged)?;
        }
        if y != b' ' {
            add_path(paths, path, Source::Unstaged)?;
        }
        if x == b'U' || y == b'U' || matches!((x, y), (b'A', b'A') | (b'D', b'D')) {
            unmerged.push(PathKey::new(path));
        }
        if matches!(x, b'R' | b'C') || matches!(y, b'R' | b'C') {
            let old = fields.get(at).ok_or("Missing rename source")?;
            at += 1;
            let source = if matches!(x, b'R' | b'C') {
                Source::Staged
            } else {
                Source::Unstaged
            };
            add_path(paths, old, source)?;
            renames.push(Rename {
                from: PathKey::new(old),
                to: PathKey::new(path),
                source,
            });
        }
    }
    Ok(())
}

fn parse_committed(
    bytes: &[u8],
    paths: &mut Changes,
    renames: &mut Vec<Rename>,
) -> Result<(), String> {
    let fields = tokens(bytes)?;
    let mut at = 0;
    while let Some(status) = fields.get(at) {
        at += 1;
        let first = fields.get(at).ok_or("Missing committed path")?;
        at += 1;
        add_path(paths, first, Source::Committed)?;
        if matches!(status.first(), Some(b'R' | b'C')) {
            let next = fields.get(at).ok_or("Missing committed rename target")?;
            at += 1;
            add_path(paths, next, Source::Committed)?;
            renames.push(Rename {
                from: PathKey::new(first),
                to: PathKey::new(next),
                source: Source::Committed,
            });
        } else if !matches!(status.first(), Some(b'A' | b'D' | b'M' | b'T' | b'U')) {
            return Err("Unsupported committed change".into());
        }
    }
    Ok(())
}

fn text(bytes: Vec<u8>) -> Option<String> {
    let mut value = String::from_utf8(bytes).ok()?;
    if value.ends_with('\n') {
        value.pop();
    }
    (!value.is_empty()).then_some(value)
}

/// Called by engstate's existing worker pool, never by a UI or MCP handler.
pub fn measure(checkout: &crate::engstate::Checkout, main_ref: Option<&str>) -> Context {
    let began = Instant::now();
    let root = checkout
        .root
        .canonicalize()
        .unwrap_or_else(|_| checkout.root.clone());
    let mut f = Footprint {
        observed_at_ms: unix_ms(),
        base_ref: main_ref.map(str::to_string),
        ..Footprint::default()
    };
    let run = |args: &[&str]| -> Result<Vec<u8>, String> {
        let remaining = Duration::from_secs(10).saturating_sub(began.elapsed());
        if remaining.is_zero() {
            return Err("Checkout scan deadline reached".into());
        }
        crate::engstate::git_bytes(
            &root,
            args,
            remaining.min(Duration::from_secs(3)),
            MAX_OUTPUT,
        )
    };
    f.common_dir = run(&["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .ok()
        .and_then(text)
        .and_then(|p| PathBuf::from(p).canonicalize().ok())
        .map(|p| p.to_string_lossy().into_owned());
    f.head_oid = run(&["rev-parse", "--verify", "HEAD"]).ok().and_then(text);
    let branch = run(&["symbolic-ref", "--quiet", "--short", "HEAD"])
        .ok()
        .and_then(text);
    let mut paths = BTreeMap::new();
    let status = run(&["status", "--porcelain=v1", "-z", "--untracked-files=all"]);
    match &status {
        Ok(bytes) => match parse_status(bytes, &mut paths, &mut f.renames, &mut f.unmerged) {
            Ok(()) => {
                for source in [Source::Staged, Source::Unstaged, Source::Untracked] {
                    f.sources.insert(source, Coverage::Complete);
                }
            }
            Err(e) => f.errors.push(e),
        },
        Err(e) => f.errors.push(format!("status: {e}")),
    }
    if let (Some(main), Some(head)) = (main_ref, &f.head_oid) {
        f.target_oid = run(&["rev-parse", "--verify", main]).ok().and_then(text);
        f.base_oid = f
            .target_oid
            .as_deref()
            .and_then(|target| run(&["merge-base", target, head]).ok().and_then(text));
        if let Some(base) = &f.base_oid {
            match run(&[
                "diff",
                "--name-status",
                "-z",
                "--find-renames",
                base,
                head,
                "--",
            ]) {
                Ok(bytes) => match parse_committed(&bytes, &mut paths, &mut f.renames) {
                    Ok(()) => {
                        f.sources.insert(Source::Committed, Coverage::Complete);
                    }
                    Err(e) => f.errors.push(e),
                },
                Err(e) => f.errors.push(format!("committed: {e}")),
            }
        } else {
            f.errors
                .push("Committed comparison base unavailable".into());
        }
    } else {
        f.errors
            .push("HEAD or integration target unavailable".into());
    }
    let mut target_conflicts = None;
    if let (Some(target), Some(head)) = (&f.target_oid, &f.head_oid) {
        f.target_check.target_oid = Some(target.clone());
        f.target_check.head_oid = Some(head.clone());
        let budget = Duration::from_secs(10)
            .saturating_sub(began.elapsed())
            .min(Duration::from_secs(3));
        let result = crate::engstate::run_git_bytes(
            &root,
            &[
                "merge-tree",
                "--write-tree",
                "--name-only",
                "-z",
                target,
                head,
            ],
            budget,
            MAX_OUTPUT,
        );
        match result {
            Ok((0, _)) => {
                f.target_check.state = TargetState::CheckedNone;
                target_conflicts = Some(vec![]);
            }
            Ok((1, bytes)) => match tokens(&bytes) {
                Ok(fields) if fields.len() > 1 => {
                    let files: Vec<_> = fields
                        .into_iter()
                        .skip(1)
                        .take_while(|p| !p.is_empty())
                        .map(PathKey::new)
                        .collect();
                    if files.is_empty() {
                        f.target_check.state = TargetState::Unavailable;
                        f.target_check.error = Some("Conflict output omitted paths".into());
                    } else {
                        f.target_check.state = TargetState::Conflict;
                        target_conflicts = Some(files);
                    }
                }
                _ => {
                    f.target_check.state = TargetState::Unavailable;
                    f.target_check.error = Some("Malformed conflict output".into());
                }
            },
            Ok((code, _)) => {
                f.target_check.state = TargetState::Unavailable;
                f.target_check.error = Some(format!("merge-tree exited {code}"));
            }
            Err(e) => {
                f.target_check.state = TargetState::Unavailable;
                f.target_check.error = Some(e);
            }
        }
    }
    let after = run(&["rev-parse", "--verify", "HEAD"]).ok().and_then(text);
    if after != f.head_oid {
        f.errors
            .push("HEAD changed during sampling; retry due".into());
        f.target_check.state = TargetState::Stale;
        target_conflicts = None;
    }
    if let Ok(before) = status {
        if run(&["status", "--porcelain=v1", "-z", "--untracked-files=all"])
            .ok()
            .as_ref()
            != Some(&before)
        {
            f.errors
                .push("Working/index state changed during sampling; retry due".into());
        }
    }
    f.paths = paths
        .into_iter()
        .map(|(path, sources)| PathChange { path, sources })
        .collect();
    f.truncated = f.errors.iter().any(|e| e.contains("limit"));
    f.duration_ms = Some(began.elapsed().as_millis() as u64);
    let id = stable_id(
        "checkout",
        &[
            f.common_dir.as_deref().unwrap_or("unknown"),
            &root.to_string_lossy(),
        ],
    );
    Context {
        id,
        root: root.to_string_lossy().into_owned(),
        repository: Some(checkout.repo.clone()),
        branch,
        participants: checkout
            .writers
            .iter()
            .filter_map(|w| w.participant.clone())
            .collect(),
        footprint: f,
        dirty: checkout.dirty,
        untracked: checkout.untracked,
        delta: checkout.delta,
        ahead_behind: checkout.ahead_behind,
        target_conflicts,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hotspot {
    pub id: String,
    pub kind: String,
    pub path: Option<PathKey>,
    pub checkout_ids: Vec<String>,
    pub participant_ids: Vec<String>,
    pub severity: Severity,
    pub reason: String,
    pub qualification: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cluster {
    pub id: String,
    pub title: String,
    pub severity: Severity,
    pub checkout_ids: Vec<String>,
    pub participant_ids: Vec<String>,
    pub session_count: Option<usize>,
    pub hotspot_ids: Vec<String>,
    pub dimensions: Vec<String>,
    pub evidence_coverage: Coverage,
    pub membership_coverage: Coverage,
    pub supersedes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub severity: Option<Severity>,
    pub coverage: Coverage,
    pub observed_at_ms: Option<u64>,
    pub checkout_count: Option<usize>,
    pub pane_count: Option<usize>,
    pub session_count: Option<usize>,
    pub contexts: Vec<Context>,
    pub unlocated: Vec<Participant>,
    pub hotspots: Vec<Hotspot>,
    pub clusters: Vec<Cluster>,
    pub findings: Vec<String>,
    pub scope_limited: bool,
    pub truncated: bool,
}

impl Project {
    pub fn pending(id: String, name: String) -> Self {
        Self {
            id,
            name,
            severity: None,
            coverage: Coverage::Unavailable,
            observed_at_ms: None,
            checkout_count: None,
            pane_count: None,
            session_count: None,
            contexts: vec![],
            unlocated: vec![],
            hotspots: vec![],
            clusters: vec![],
            findings: vec!["Background observation pending".into()],
            scope_limited: false,
            truncated: false,
        }
    }
    pub fn age_ms(&self) -> Option<u64> {
        unix_ms()?.checked_sub(self.observed_at_ms?)
    }
}

fn sessions<'a>(participants: impl Iterator<Item = &'a Participant>) -> Option<usize> {
    let agents: Vec<_> = participants.filter(|p| p.agent != Some(false)).collect();
    if agents
        .iter()
        .any(|p| p.agent.is_none() || p.session.is_none())
    {
        return None;
    }
    Some(
        agents
            .iter()
            .filter_map(|p| p.session.as_ref())
            .collect::<BTreeSet<_>>()
            .len(),
    )
}

/// Filter facts BEFORE deriving any IDs, titles, counts, sort keys or deltas.
/// Restricted modes conservatively withhold filesystem observations because
/// Git has no per-pane authorship. A denied pane's cwd or writes cannot then
/// change an authorized file group or identify its presence through metadata.
pub fn derive(id: &str, name: &str, facts: &Facts, authorized: bool) -> Option<Project> {
    let mut contexts = facts.contexts.clone();
    let mut unlocated = facts.unlocated.clone();
    let full = !authorized
        || contexts
            .iter()
            .flat_map(|c| &c.participants)
            .chain(&unlocated)
            .filter(|p| p.exposed)
            .all(|p| p.full_scope);
    if authorized {
        for c in &mut contexts {
            c.participants.retain(|p| p.exposed);
            if !full {
                c.footprint = Footprint::restricted();
                c.target_conflicts = None;
                c.dirty = None;
                c.untracked = None;
                c.delta = None;
                c.ahead_behind = None;
                c.repository = None;
                c.branch = None;
                // This ID depends only on the permitted physical root. Common
                // dirs read through denied sources do not enter this projection.
                c.id = stable_id("checkout", &["authorized-root", &c.root]);
            }
        }
        let owns_project = contexts
            .iter()
            .any(|c| c.participants.iter().any(|p| p.project_id == id))
            || unlocated.iter().any(|p| p.exposed && p.project_id == id);
        if !owns_project {
            return None;
        }
        contexts.retain(|c| {
            if full {
                !c.participants.is_empty()
            } else {
                c.participants.iter().any(|p| p.project_id == id)
            }
        });
        unlocated.retain(|p| p.exposed);
        if contexts.is_empty() && unlocated.is_empty() {
            return None;
        }
    }
    contexts.sort_by(|a, b| a.id.cmp(&b.id));
    for c in &mut contexts {
        c.footprint.paths.sort_by(|a, b| a.path.cmp(&b.path));
        c.participants.sort_by(|a, b| a.id.cmp(&b.id));
    }
    unlocated.sort_by(|a, b| a.id.cmp(&b.id));
    let truncated = if authorized && !full {
        false
    } else {
        facts.truncated
    };
    let complete = !truncated
        && unlocated.is_empty()
        && !contexts.is_empty()
        && contexts
            .iter()
            .all(|c| !c.participants.is_empty() && c.footprint.coverage() == Coverage::Complete);
    let coverage = if complete {
        Coverage::Complete
    } else if contexts.is_empty() {
        Coverage::Unavailable
    } else {
        Coverage::Partial
    };
    let mut hotspots = Vec::new();
    let mut files: BTreeMap<(String, String), Vec<(&Context, &PathChange)>> = BTreeMap::new();
    for c in &contexts {
        if c.participants.len() > 1 {
            let session_count = sessions(c.participants.iter());
            let severity = if session_count.is_some_and(|n| n > 1) {
                Severity::Warning
            } else {
                Severity::Caution
            };
            hotspots.push(Hotspot {
                id: stable_id("hotspot", &[id, "shared_checkout", &c.id]),
                kind: "shared_checkout".into(),
                path: None,
                checkout_ids: vec![c.id.clone()],
                participant_ids: c.participants.iter().map(|p| p.id.clone()).collect(),
                severity,
                reason: format!("{} panes share {}", c.participants.len(), c.root),
                qualification: Some(
                    match session_count {
                        Some(1) => "These panes represent one known session",
                        Some(_) => "Occupancy is observed; concurrent writes are not established",
                        None => "Distinct agent/session identities are unknown",
                    }
                    .into(),
                ),
            });
        }
        if let Some(common) = &c.footprint.common_dir {
            for change in &c.footprint.paths {
                files
                    .entry((common.clone(), change.path.bytes_hex.clone()))
                    .or_default()
                    .push((c, change));
            }
        }
    }
    for ((_, path), members) in files {
        if members.len() < 2 {
            continue;
        }
        let change = members[0].1;
        let committed_only = members
            .iter()
            .all(|(_, p)| p.sources.iter().all(|s| *s == Source::Committed));
        let severity = if members.len() >= 3 && !committed_only {
            Severity::Warning
        } else {
            Severity::Caution
        };
        let ids: Vec<_> = members.iter().map(|(c, _)| c.id.clone()).collect();
        hotspots.push(Hotspot{id:stable_id("hotspot", &[id,"shared_file",&ids.join("|"),&path]),kind:"shared_file".into(),path:Some(change.path.clone()),checkout_ids:ids,participant_ids:members.iter().flat_map(|(c,_)|c.participants.iter().map(|p|p.id.clone())).collect(),severity,reason:format!("{} checkouts include {}",members.len(),change.path.display),qualification:committed_only.then(||"Committed paths compared with an assumed main may be inherited by stacked branches; line conflict is not established".into())});
    }
    let mut findings = Vec::new();
    for c in &contexts {
        for (kind,paths,reason,qualification) in [
            ("unmerged_index",c.footprint.unmerged.as_slice(),format!("{} unresolved index paths in {}",c.footprint.unmerged.len(),c.root),"Current index conflicts; their merge target and agent authorship are not established."),
            ("target_conflict",c.target_conflicts.as_deref().unwrap_or(&[]),format!("{} committed conflict paths against {}",c.target_conflicts.as_ref().map_or(0,Vec::len),c.footprint.base_ref.as_deref().unwrap_or("unavailable target")),"Committed integration check; uncommitted edits and agent authorship are outside this measurement."),
        ] {
            if paths.is_empty(){continue;}
            findings.push(format!("{}: {reason}",c.root));
            hotspots.push(Hotspot{id:stable_id("hotspot",&[id,kind,&c.id]),kind:kind.into(),path:paths.first().cloned(),checkout_ids:vec![c.id.clone()],participant_ids:c.participants.iter().map(|p|p.id.clone()).collect(),severity:Severity::Alert,reason,qualification:Some(qualification.into())});
        }
    }
    hotspots.sort_by(|a, b| b.severity.cmp(&a.severity).then_with(|| a.id.cmp(&b.id)));
    let mut parent: BTreeMap<String, String> = contexts
        .iter()
        .map(|c| (c.id.clone(), c.id.clone()))
        .collect();
    fn root(parent: &BTreeMap<String, String>, id: &str) -> String {
        let mut here = id.to_string();
        while parent[&here] != here {
            here = parent[&here].clone();
        }
        here
    }
    for h in &hotspots {
        for member in h.checkout_ids.iter().skip(1) {
            let a = root(&parent, &h.checkout_ids[0]);
            let b = root(&parent, member);
            parent.insert(b, a);
        }
    }
    let mut components: BTreeMap<String, Vec<&Hotspot>> = BTreeMap::new();
    for h in &hotspots {
        if matches!(h.kind.as_str(), "target_conflict" | "unmerged_index")
            && h.checkout_ids.len() == 1
            && !hotspots.iter().any(|other| {
                matches!(other.kind.as_str(), "shared_checkout" | "shared_file")
                    && other.checkout_ids.contains(&h.checkout_ids[0])
            })
        {
            continue;
        }
        components
            .entry(root(&parent, &h.checkout_ids[0]))
            .or_default()
            .push(h);
    }
    let mut clusters = Vec::new();
    for hs in components.values() {
        let checkout_ids: Vec<_> = hs
            .iter()
            .flat_map(|h| h.checkout_ids.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let members: Vec<_> = contexts
            .iter()
            .filter(|c| checkout_ids.contains(&c.id))
            .collect();
        let hotspot_ids: Vec<_> = hs.iter().map(|h| h.id.clone()).collect();
        let evidence_coverage = if members
            .iter()
            .all(|c| c.footprint.coverage() == Coverage::Complete)
        {
            Coverage::Complete
        } else {
            Coverage::Partial
        };
        clusters.push(Cluster {
            id: stable_id("cluster", &[id, &checkout_ids.join("|")]),
            title: hs[0].reason.clone(),
            severity: hs
                .iter()
                .map(|h| h.severity)
                .max()
                .expect("component hotspot"),
            checkout_ids,
            participant_ids: members
                .iter()
                .flat_map(|c| c.participants.iter().map(|p| p.id.clone()))
                .collect(),
            session_count: sessions(members.iter().flat_map(|c| &c.participants)),
            hotspot_ids,
            dimensions: hs
                .iter()
                .map(|h| h.kind.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            evidence_coverage,
            membership_coverage: coverage,
            supersedes: vec![],
        });
    }
    clusters.sort_by(|a, b| b.severity.cmp(&a.severity).then_with(|| a.id.cmp(&b.id)));
    let severity = hotspots
        .iter()
        .map(|h| h.severity)
        .max()
        .or_else(|| complete.then_some(Severity::Clear));
    let observed_at_ms = contexts
        .iter()
        .filter_map(|c| c.footprint.observed_at_ms)
        .min();
    let participants: Vec<_> = contexts
        .iter()
        .flat_map(|c| &c.participants)
        .chain(&unlocated)
        .collect();
    let pane_count = Some(
        participants
            .iter()
            .map(|p| &p.id)
            .collect::<BTreeSet<_>>()
            .len(),
    );
    let session_count = sessions(participants.into_iter());
    Some(Project {
        id: id.into(),
        name: name.into(),
        severity,
        coverage,
        observed_at_ms,
        checkout_count: (!contexts.is_empty() || complete).then_some(contexts.len()),
        pane_count,
        session_count,
        contexts,
        unlocated,
        hotspots,
        clusters,
        findings,
        scope_limited: authorized && !full,
        truncated,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub epoch: String,
    pub version: u64,
    pub projects: Vec<Project>,
}

impl Snapshot {
    pub fn empty(epoch: impl Into<String>) -> Self {
        Self {
            schema_version: 1,
            epoch: epoch.into(),
            version: 0,
            projects: vec![],
        }
    }
    pub fn affected(&self) -> usize {
        self.projects
            .iter()
            .filter(|p| p.severity.is_some_and(|s| s >= Severity::Caution))
            .count()
    }
    pub fn coverage(&self) -> Coverage {
        if self.projects.is_empty() {
            Coverage::Unavailable
        } else if self
            .projects
            .iter()
            .all(|p| p.coverage == Coverage::Complete)
        {
            Coverage::Complete
        } else if self
            .projects
            .iter()
            .all(|p| p.coverage == Coverage::Unavailable)
        {
            Coverage::Unavailable
        } else {
            Coverage::Partial
        }
    }
    pub fn severity(&self) -> Option<Severity> {
        let observed = self.projects.iter().filter_map(|p| p.severity).max();
        if observed == Some(Severity::Clear) && self.projects.iter().any(|p| p.severity.is_none()) {
            None
        } else {
            observed
        }
    }
    pub fn rank(&mut self) {
        self.projects.sort_by(|a, b| {
            rank(a)
                .cmp(&rank(b))
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.id.cmp(&b.id))
        });
    }
}

fn rank(p: &Project) -> u8 {
    match p.severity {
        Some(Severity::Alert) => 0,
        Some(Severity::Warning) => 1,
        Some(Severity::Caution) => 2,
        None => 3,
        Some(Severity::Clear) => 4,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Delta {
    pub version: u64,
    pub project_id: String,
    pub kind: String,
    pub cluster_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Store {
    pub snapshot: Snapshot,
    pub changes: VecDeque<Delta>,
    next_cluster: u64,
}

impl Store {
    pub fn new(epoch: impl Into<String>) -> Self {
        Self {
            snapshot: Snapshot::empty(epoch),
            changes: VecDeque::new(),
            next_cluster: 0,
        }
    }
    pub fn update(&mut self, mut project: Project) {
        let previous = self.snapshot.projects.iter().find(|p| p.id == project.id);
        let preserved = if let Some(old) = previous {
            reconcile(&old.clusters, &mut project.clusters)
        } else {
            vec![false; project.clusters.len()]
        };
        for (cluster, keep) in project.clusters.iter_mut().zip(preserved) {
            if !keep {
                self.next_cluster += 1;
                cluster.id = stable_id(
                    "cluster",
                    &[
                        &self.snapshot.epoch,
                        &project.id,
                        &self.next_cluster.to_string(),
                    ],
                );
            }
        }
        let changed = previous.is_none_or(|old| fingerprint(old) != fingerprint(&project));
        if changed {
            self.snapshot.version += 1;
            let kind = match previous {
                None => "created",
                Some(p) if matches!((project.severity,p.severity),(Some(a),Some(b)) if a>b) => {
                    "severity_raised"
                }
                Some(p) if matches!((project.severity,p.severity),(Some(a),Some(b)) if a<b) => {
                    "severity_lowered"
                }
                Some(_) => "changed",
            };
            self.changes.push_back(Delta {
                version: self.snapshot.version,
                project_id: project.id.clone(),
                kind: kind.into(),
                cluster_ids: project.clusters.iter().map(|c| c.id.clone()).collect(),
            });
            while self.changes.len() > HISTORY {
                self.changes.pop_front();
            }
        }
        self.snapshot.projects.retain(|p| p.id != project.id);
        self.snapshot.projects.push(project);
        self.snapshot.rank();
    }
    pub fn remove(&mut self, id: &str) {
        if self.snapshot.projects.iter().any(|p| p.id == id) {
            self.snapshot.projects.retain(|p| p.id != id);
            self.snapshot.version += 1;
            self.changes.push_back(Delta {
                version: self.snapshot.version,
                project_id: id.into(),
                kind: "retired".into(),
                cluster_ids: vec![],
            });
            while self.changes.len() > HISTORY {
                self.changes.pop_front();
            }
        }
    }
}

fn fingerprint(project: &Project) -> String {
    let mut stable = project.clone();
    stable.observed_at_ms = None;
    for c in &mut stable.contexts {
        c.footprint.observed_at_ms = None;
        c.footprint.duration_ms = None;
    }
    serde_json::to_string(&stable).expect("coordination JSON")
}

fn reconcile(old: &[Cluster], new: &mut [Cluster]) -> Vec<bool> {
    let mut preserved = vec![false; new.len()];
    let matches: Vec<Vec<usize>> = new
        .iter()
        .map(|n| {
            old.iter()
                .enumerate()
                .filter_map(|(i, o)| {
                    n.checkout_ids
                        .iter()
                        .any(|id| o.checkout_ids.contains(id))
                        .then_some(i)
                })
                .collect()
        })
        .collect();
    for (index, n) in new.iter_mut().enumerate() {
        let candidates = &matches[index];
        if let [candidate] = candidates.as_slice() {
            let uses = matches.iter().filter(|m| m.contains(candidate)).count();
            if uses == 1 {
                n.id = old[*candidate].id.clone();
                n.supersedes = old[*candidate].supersedes.clone();
                preserved[index] = true;
                continue;
            }
        }
        n.supersedes = candidates.iter().map(|i| old[*i].id.clone()).collect();
    }
    preserved
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Depth {
    #[default]
    Overview,
    Project(String),
    Cluster(String, String),
    Checkouts(String),
}

/// Human-directed agent context obeys the same exposure boundary as MCP. A
/// human-only group never smuggles its private handles or paths into a prompt.
pub fn agent_context(
    store: &Store,
    depth: &Depth,
    enabled: bool,
) -> Result<serde_json::Value, String> {
    use serde_json::json;
    if !enabled {
        return Err("MCP exposure is disabled; agent-visible observations are unavailable".into());
    }
    let (tool, args) = match depth {
        Depth::Cluster(p, c)
            if store.snapshot.projects.iter().any(|project| {
                project.id == *p && project.clusters.iter().any(|group| group.id == *c)
            }) =>
        {
            (
                "coordination_cluster",
                json!({"project_id":p,"cluster_id":c}),
            )
        }
        Depth::Cluster(p, _) | Depth::Project(p) | Depth::Checkouts(p)
            if store
                .snapshot
                .projects
                .iter()
                .any(|project| project.id == *p) =>
        {
            ("coordination_project", json!({"project_id":p}))
        }
        _ => ("coordination_overview", json!({})),
    };
    Ok(json!({"request":{"tool":tool,"arguments":args},"observations":query(store,tool,&args)?}))
}

#[derive(Clone, Debug, Default)]
pub struct View {
    pub depth: Depth,
    pub expanded: BTreeSet<String>,
    pub selected: usize,
    pub page: usize,
    pub history: Vec<(Depth, usize)>,
}

impl View {
    pub fn enter(&mut self, depth: Depth) {
        if self.depth == depth {
            return;
        }
        let tab_switch = matches!((&self.depth,&depth),(Depth::Project(a),Depth::Checkouts(b))|(Depth::Checkouts(a),Depth::Project(b)) if a==b);
        if tab_switch {
            self.depth = depth;
            self.selected = 0;
            self.page = 0;
            return;
        }
        self.history.push((self.depth.clone(), self.selected));
        self.depth = depth;
        self.selected = 0;
        self.page = 0;
    }
    pub fn back(&mut self) -> bool {
        if let Some((depth, selected)) = self.history.pop() {
            self.depth = depth;
            self.selected = selected;
            self.page = selected / PAGE;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests;

fn context_summary(ctx: &Context, project_id: &str) -> serde_json::Value {
    serde_json::json!({"id":ctx.id,"root":ctx.root,"branch":ctx.branch,"participants":ctx.participants,
        "coverage":ctx.footprint.coverage(),"sources":ctx.footprint.sources,"head_oid":ctx.footprint.head_oid,
        "base_oid":ctx.footprint.base_oid,"target_ref":ctx.footprint.base_ref,"target_oid":ctx.footprint.target_oid,
        "target_check":ctx.footprint.target_check,"target_conflict_count":ctx.target_conflicts.as_ref().map(Vec::len),
        "unmerged_count":(ctx.footprint.sources.get(&Source::Staged)==Some(&Coverage::Complete)).then_some(ctx.footprint.unmerged.len()),"observed_at_ms":ctx.footprint.observed_at_ms,"errors":ctx.footprint.errors,
        "dirty":ctx.dirty,"untracked":ctx.untracked,"delta":ctx.delta,"ahead_behind":ctx.ahead_behind,"observed_path_count":ctx.footprint.paths.len(),
        "read":{"tool":"coordination_project","arguments":{"project_id":project_id,"checkout_id":ctx.id}}})
}

/// Cached, scope-local protocol. Cursors bind both the epoch and fact version;
/// paging a changed graph requires a fresh read rather than silently skipping rows.
pub fn query(
    store: &Store,
    tool: &str,
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    use serde_json::json;
    let s = &store.snapshot;
    let limit = match args.get("limit") {
        None => PAGE,
        Some(v) => v
            .as_u64()
            .filter(|n| (1..=100).contains(n))
            .ok_or("limit must be an integer from 1 through 100")? as usize,
    };
    let binding = stable_id(
        "read",
        &[
            tool,
            args.get("project_id")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            args.get("cluster_id")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            args.get("checkout_id")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
        ],
    );
    let offset = match args.get("cursor") {
        None | Some(serde_json::Value::Null) => 0,
        Some(v) => {
            let c = v.as_str().ok_or("cursor must be a string")?;
            let prefix = format!("{}:{}:{binding}:", s.epoch, s.version);
            c.strip_prefix(&prefix)
                .and_then(|v| v.parse::<usize>().ok())
                .ok_or("Cursor expired or belongs to another read; restart this read")?
        }
    };
    let project_id = args.get("project_id").and_then(|v| v.as_str());
    let project = || {
        s.projects
            .iter()
            .find(|p| Some(p.id.as_str()) == project_id)
            .ok_or("Project is unavailable in this authorized snapshot")
    };
    let page = |values: Vec<serde_json::Value>| -> Result<serde_json::Value, String> {
        if offset > values.len() {
            return Err("Cursor is beyond this result".into());
        }
        let end = offset.saturating_add(limit).min(values.len());
        Ok(
            json!({"items":values[offset..end],"total":values.len(),"next_cursor":(end<values.len()).then(||format!("{}:{}:{binding}:{end}",s.epoch,s.version))}),
        )
    };
    let summary = |p: &Project| json!({"id":p.id,"name":p.name,"severity":p.severity,"coverage":p.coverage,"observed_at_ms":p.observed_at_ms,"age_ms":p.age_ms(),"stale":p.age_ms().map(|age|age>30000),"checkout_count":p.checkout_count,"pane_count":p.pane_count,"session_count":p.session_count,"observed_group_count":p.clusters.len(),"scope_limited":p.scope_limited,"findings":p.findings,"causes":p.hotspots.iter().take(3).map(|h|json!({"kind":h.kind,"severity":h.severity,"reason":h.reason,"qualification":h.qualification})).collect::<Vec<_>>(),"groups":p.clusters.iter().take(3).map(|c|json!({"id":c.id,"title":c.title,"dimensions":c.dimensions,"evidence_coverage":c.evidence_coverage,"membership_coverage":c.membership_coverage,"read":{"tool":"coordination_cluster","arguments":{"project_id":p.id,"cluster_id":c.id}}})).collect::<Vec<_>>(),"read":{"tool":"coordination_project","arguments":{"project_id":p.id}}});
    let data = match tool {
        "coordination_overview" => {
            json!({"scope":"authorized projects in this window","severity":s.severity(),"coverage":s.coverage(),"affected_projects":s.affected(),"projects":page(s.projects.iter().map(summary).collect())?,"execution":"Read-only observations. Direct an agent to inspect the named group and propose a plan; no Git writes or pane input are performed."})
        }
        "coordination_project" => {
            let p = project()?;
            if let Some(id) = args.get("checkout_id").and_then(|v| v.as_str()) {
                let ctx = p
                    .contexts
                    .iter()
                    .find(|c| c.id == id)
                    .ok_or("Checkout unavailable in this authorized project")?;
                let mut paths: BTreeMap<_, Option<&PathChange>> = ctx
                    .footprint
                    .paths
                    .iter()
                    .map(|p| (&p.path, Some(p)))
                    .collect();
                for path in ctx
                    .target_conflicts
                    .iter()
                    .flatten()
                    .chain(&ctx.footprint.unmerged)
                {
                    paths.entry(path).or_insert(None);
                }
                if offset > paths.len() {
                    return Err("Cursor is beyond this result".into());
                }
                let end = offset.saturating_add(limit).min(paths.len());
                let changes:Vec<_>=paths.iter().skip(offset).take(limit).map(|(path,change)|json!({"path":path,"sources":change.map(|p|&p.sources),"unmerged":if ctx.footprint.unmerged.contains(path){Some(true)}else{(ctx.footprint.sources.get(&Source::Staged)==Some(&Coverage::Complete)).then_some(false)},"target_conflict":ctx.target_conflicts.as_ref().map(|paths|paths.contains(path)),"renames":ctx.footprint.renames.iter().filter(|r|r.from==**path||r.to==**path).collect::<Vec<_>>()})).collect();
                json!({"project":summary(p),"checkout":context_summary(ctx,&p.id),"changes":{"items":changes,"total":paths.len(),"next_cursor":(end<paths.len()).then(||format!("{}:{}:{binding}:{end}",s.epoch,s.version))}})
            } else {
                json!({"project":summary(p),"groups":page(p.clusters.iter().map(|c|json!(c)).collect())?,"checkouts":p.contexts.iter().map(|c|context_summary(c,&p.id)).collect::<Vec<_>>(),"unlocated":p.unlocated})
            }
        }
        "coordination_cluster" => {
            let p = project()?;
            let id = args
                .get("cluster_id")
                .and_then(|v| v.as_str())
                .ok_or("cluster_id is required")?;
            let c = p
                .clusters
                .iter()
                .find(|c| c.id == id)
                .ok_or("Group retired or unavailable; read the project again")?;
            let hotspot_ids: BTreeSet<_> = c.hotspot_ids.iter().collect();
            let hs: Vec<_> = p
                .hotspots
                .iter()
                .filter(|h| hotspot_ids.contains(&h.id))
                .collect();
            let contexts: Vec<_> = p
                .contexts
                .iter()
                .filter(|ctx| c.checkout_ids.contains(&ctx.id))
                .collect();
            let implicated: BTreeSet<_> = hs
                .iter()
                .filter_map(|h| h.path.as_ref().map(|p| p.bytes_hex.clone()))
                .collect();
            let shared: BTreeSet<_> = hs
                .iter()
                .filter(|h| h.kind == "shared_checkout")
                .flat_map(|h| h.checkout_ids.iter())
                .collect();
            let mut extra_paths = BTreeMap::new();
            for ctx in contexts.iter().filter(|ctx| shared.contains(&ctx.id)) {
                for change in &ctx.footprint.paths {
                    if !implicated.contains(&change.path.bytes_hex) {
                        extra_paths.insert(change.path.bytes_hex.clone(), &change.path);
                    }
                }
            }
            let total = hs.len() + extra_paths.len();
            if offset > total {
                return Err("Cursor is beyond this result".into());
            }
            let end = offset.saturating_add(limit).min(total);
            let checkouts: Vec<_> = contexts
                .iter()
                .map(|ctx| context_summary(ctx, &p.id))
                .collect();
            let rows = hs
                .iter()
                .map(|h| (Some(*h), h.path.as_ref()))
                .chain(extra_paths.values().map(|path| (None, Some(*path))));
            let evidence:Vec<_>=rows.skip(offset).take(limit).map(|(hotspot,path)|json!({"hotspot":hotspot,"path":path,
                "relationships":contexts.iter().filter(|ctx|hotspot.is_none_or(|h|h.checkout_ids.contains(&ctx.id))).filter_map(|ctx|{
                    let change=path.and_then(|path|ctx.footprint.paths.binary_search_by(|p|p.path.cmp(path)).ok().map(|i|&ctx.footprint.paths[i]));
                    if path.is_some()&&change.is_none()&&hotspot.is_none(){return None;}
                    Some(json!({"checkout_id":ctx.id,"sources":change.map(|p|&p.sources),
                        "renames":path.map(|path|ctx.footprint.renames.iter().filter(|r|r.from==*path||r.to==*path).collect::<Vec<_>>())}))
                }).collect::<Vec<_>>()
            })).collect();
            let mut group = c.clone();
            group.hotspot_ids.clear();
            json!({"project_id":p.id,"group":group,"hotspot_count":hs.len(),"checkouts":checkouts,
                "evidence":{"items":evidence,"total":total,"next_cursor":(end<total).then(||format!("{}:{}:{binding}:{end}",s.epoch,s.version))},
                "line_analysis":"not_measured","authorship":"Pane occupancy is observed; file authorship is not inferred."})
        }
        "coordination_changes" => {
            let epoch = args
                .get("epoch")
                .and_then(|v| v.as_str())
                .ok_or("epoch is required")?;
            let since = args
                .get("since_version")
                .and_then(|v| v.as_u64())
                .ok_or("since_version is required")?;
            let reset = epoch != s.epoch
                || since > s.version
                || store
                    .changes
                    .front()
                    .is_some_and(|d| since.saturating_add(1) < d.version);
            json!({"requires_full_snapshot":reset,"changes":if reset{vec![]}else{store.changes.iter().filter(|d|d.version>since).collect::<Vec<_>>()},"meaning":"A changed grouping or lower group count does not establish reduced friction."})
        }
        _ => return Err("Unknown coordination read".into()),
    };
    Ok(json!({"schema_version":s.schema_version,"epoch":s.epoch,"version":s.version,"data":data}))
}
