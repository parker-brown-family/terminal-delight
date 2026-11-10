//! Portable foreground-exposure records. No UI, no browser clock and no inference of attention.
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize};

use super::engine::{layout_hash, LayoutHash, ReadingLayout, RectCss};
use super::notes;

const MAX: u64 = (1_u64 << 53) - 1;

/// Positive exposed area after clipping and subtracting the real native chrome.
pub fn exposed(rect: RectCss, viewport: RectCss, covers: &[RectCss]) -> bool {
    let intersect = |a: RectCss, b: RectCss| {
        let x = a.x.max(b.x);
        let y = a.y.max(b.y);
        let w = (a.x + a.w).min(b.x + b.w) - x;
        let h = (a.y + a.h).min(b.y + b.h) - y;
        (a.w > 0.0 && a.h > 0.0 && b.w > 0.0 && b.h > 0.0 && w > 0.0 && h > 0.0)
            .then_some(RectCss { x, y, w, h })
    };
    let Some(initial) = intersect(rect, viewport) else {
        return false;
    };
    let mut pieces = vec![initial];
    for cover in covers {
        let mut next = vec![];
        for p in pieces {
            if let Some(i) = intersect(p, *cover) {
                for remaining in [
                    RectCss {
                        x: p.x,
                        y: p.y,
                        w: p.w,
                        h: i.y - p.y,
                    },
                    RectCss {
                        x: p.x,
                        y: i.y + i.h,
                        w: p.w,
                        h: p.y + p.h - i.y - i.h,
                    },
                    RectCss {
                        x: p.x,
                        y: i.y,
                        w: i.x - p.x,
                        h: i.h,
                    },
                    RectCss {
                        x: i.x + i.w,
                        y: i.y,
                        w: p.x + p.w - i.x - i.w,
                        h: i.h,
                    },
                ] {
                    if remaining.w > 0.0 && remaining.h > 0.0 {
                        next.push(remaining);
                    }
                }
            } else {
                next.push(p);
            }
        }
        pieces = next;
        if pieces.is_empty() {
            return false;
        }
    }
    !pieces.is_empty()
}

fn required_option<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub anchor: String,
    pub title: String,
    pub kind: String,
    #[serde(deserialize_with = "required_option")]
    pub parent: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub visible_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gap {
    pub sequence: u64,
    pub reason: String,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub session: String,
    pub document: String,
    pub revision: String,
    pub host: String,
    pub method: u32,
    pub targets: Vec<Target>,
    pub gaps: Vec<Gap>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub format: u32,
    pub sessions: Vec<Session>,
}

fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.as_bytes()[0].is_ascii_alphanumeric()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn revision(s: &str) -> bool {
    s.len() == 71
        && s.starts_with("sha256:")
        && s.as_bytes()[7..]
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
}

impl Session {
    pub fn validate(&self) -> Result<(), String> {
        if !identifier(&self.session)
            || !identifier(&self.document)
            || !revision(&self.revision)
            || !matches!(self.host.as_str(), "browser" | "terminal-delight")
            || self.method != 1
            || self.targets.len() > 2000
            || self.gaps.len() > 10000
        {
            return Err("Unsupported reading session identity or limits".into());
        }
        let mut ids = HashSet::new();
        for t in &self.targets {
            if !identifier(&t.anchor)
                || !ids.insert(t.anchor.as_str())
                || t.title.chars().count() > 500
                || !matches!(t.kind.as_str(), "opening" | "section" | "figure" | "entry")
                || t.visible_ms.is_some_and(|n| n > MAX)
            {
                return Err("Invalid reading target".into());
            }
        }
        let parents: HashMap<_, _> = self
            .targets
            .iter()
            .map(|t| (t.anchor.as_str(), t.parent.as_deref()))
            .collect();
        for t in &self.targets {
            let mut seen = HashSet::from([t.anchor.as_str()]);
            let mut parent = t.parent.as_deref();
            while let Some(p) = parent {
                if !ids.contains(p) || !seen.insert(p) {
                    return Err("Missing or cyclic reading parent".into());
                }
                parent = parents[p];
            }
        }
        let mut gaps = HashSet::new();
        for g in &self.gaps {
            if g.sequence > MAX
                || g.elapsed_ms > MAX
                || !gaps.insert(g.sequence)
                || !matches!(g.reason.as_str(), "clock_gap" | "activity_unknown")
            {
                return Err("Invalid reading observation gap".into());
            }
        }
        Ok(())
    }
}

impl Record {
    pub fn validate(&self) -> Result<(), String> {
        if self.format != 1 || self.sessions.len() > 10000 {
            return Err("Unsupported reading record".into());
        }
        let mut ids = HashSet::new();
        for s in &self.sessions {
            s.validate()?;
            if !ids.insert(&s.session) {
                return Err("Duplicate reading session".into());
            }
        }
        Ok(())
    }

    pub fn merge(&mut self, incoming: &Session) -> Result<(), String> {
        self.validate()?;
        incoming.validate()?;
        if let Some(old) = self
            .sessions
            .iter_mut()
            .find(|s| s.session == incoming.session)
        {
            if old.document != incoming.document
                || old.revision != incoming.revision
                || old.host != incoming.host
                || old.method != incoming.method
            {
                return Err("Reading session identity changed".into());
            }
            for t in &incoming.targets {
                if let Some(before) = old.targets.iter_mut().find(|v| v.anchor == t.anchor) {
                    if before.title != t.title || before.kind != t.kind || before.parent != t.parent
                    {
                        return Err("Reading target identity changed".into());
                    }
                    if let Some(after) = t.visible_ms {
                        before.visible_ms = Some(before.visible_ms.map_or(after, |n| n.max(after)));
                    }
                } else {
                    old.targets.push(t.clone());
                }
            }
            for g in &incoming.gaps {
                if let Some(before) = old.gaps.iter_mut().find(|v| v.sequence == g.sequence) {
                    if before.reason != g.reason {
                        return Err("Reading gap identity changed".into());
                    }
                    before.elapsed_ms = before.elapsed_ms.max(g.elapsed_ms);
                } else {
                    old.gaps.push(g.clone());
                }
            }
        } else {
            self.sessions.push(incoming.clone());
        }
        self.validate()
    }

    /// Per-target totals, explicitly across sessions. Overlapping targets are never summed together.
    pub fn totals(&self, revision: &str, live: Option<&Session>) -> HashMap<String, u64> {
        let mut out = HashMap::new();
        for s in self
            .sessions
            .iter()
            .filter(|s| s.revision == revision && live.is_none_or(|l| s.session != l.session))
            .chain(live)
        {
            for t in &s.targets {
                if let Some(ms) = t.visible_ms {
                    let total = out.entry(t.anchor.clone()).or_insert(0_u64);
                    *total = total.saturating_add(ms);
                }
            }
        }
        out
    }
}

pub fn read(bytes: &[u8]) -> Result<Record, String> {
    let (island, count) = notes::find_island(bytes, b"report-reading");
    let i = island
        .filter(|i| count == 1 && i.close_end.is_some())
        .ok_or("No unique, complete reading-data island")?;
    if notes::attr(i.open_tag(bytes), "data-format").as_deref() != Some("1") {
        return Err("Unsupported reading-data format".into());
    }
    let record: Record = serde_json::from_slice(i.text(bytes)).map_err(|e| e.to_string())?;
    record.validate()?;
    let (document, _) = identity(bytes)?;
    if record.sessions.iter().any(|s| s.document != document) {
        return Err("Reading record contains another document".into());
    }
    Ok(record)
}

#[derive(Deserialize)]
struct Meta {
    id: String,
    revision: String,
}

pub fn identity(bytes: &[u8]) -> Result<(String, String), String> {
    let (island, count) = notes::find_island(bytes, b"brief-meta");
    let i = island
        .filter(|i| count == 1 && i.close_end.is_some())
        .ok_or("No unique brief identity")?;
    let m: Meta = serde_json::from_slice(i.text(bytes)).map_err(|e| e.to_string())?;
    if !identifier(&m.id) || !revision(&m.revision) {
        return Err("Invalid brief identity".into());
    }
    Ok((m.id, m.revision))
}

/// Stable lock inode shared with Python's fcntl.flock, never the atomically replaced HTML inode.
pub struct ReviewLock {
    _file: File,
}

pub fn review_lock(path: &Path) -> Result<ReviewLock, String> {
    review_lock_mode(path, true)?.ok_or_else(|| "Review lock is busy".into())
}

fn review_lock_mode(path: &Path, blocking: bool) -> Result<Option<ReviewLock>, String> {
    let mut name = std::ffi::OsString::from(".");
    name.push(path.file_name().ok_or("The brief has no file name")?);
    name.push(".brief-review.lock");
    let lock = path.with_file_name(name);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(lock)
        .map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    // SAFETY: geteuid has no preconditions; flock receives the live descriptor owned by file.
    if !meta.is_file()
        || meta.nlink() != 1
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.permissions().mode() & 0o777 != 0o600
    {
        return Err("Unsafe review lock file".into());
    }
    // SAFETY: file owns a live descriptor throughout this blocking flock call.
    if unsafe {
        libc::flock(
            file.as_raw_fd(),
            libc::LOCK_EX | if blocking { 0 } else { libc::LOCK_NB },
        )
    } != 0
    {
        let error = std::io::Error::last_os_error();
        if !blocking && error.kind() == std::io::ErrorKind::WouldBlock {
            return Ok(None);
        }
        return Err(error.to_string());
    }
    Ok(Some(ReviewLock { _file: file }))
}

pub fn save(
    path: &Path,
    expected: LayoutHash,
    session: &Session,
) -> Result<(Record, notes::Written), String> {
    let _lock = review_lock(path)?;
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if layout_hash(&bytes) != expected {
        return Err(
            "The brief changed; its reading time is waiting for the displayed version".into(),
        );
    }
    let (doc, rev) = identity(&bytes)?;
    if session.document != doc || session.revision != rev {
        return Err("Reading time belongs to another document version".into());
    }
    let mut record = read(&bytes)?;
    if record.sessions.iter().any(|s| s.document != doc) {
        return Err("Reading record contains another document".into());
    }
    record.merge(session)?;
    let i = notes::find_island(&bytes, b"report-reading")
        .0
        .ok_or("Missing reading island")?;
    let end = i.close_end.ok_or("Torn reading island")?;
    let json = serde_json::to_string_pretty(&record)
        .map_err(|e| e.to_string())?
        .replace("</", "<\\/")
        .replace("<!", "\\u003c!");
    let block = format!(
        "<script type=\"application/json\" id=\"report-reading\" data-format=\"1\">{json}</script>"
    )
    .into_bytes();
    let mut out = bytes[..i.tag_start].to_vec();
    out.extend_from_slice(&block);
    out.extend_from_slice(&bytes[end..]);
    let back = notes::read(&bytes);
    let notes = back
        .notes
        .ok_or("The brief has no notes island")?
        .map_err(|e| e.0)?;
    let concurs = Some(back.concurs.map_err(|e| e.0)?);
    let plan = notes::WritePlan {
        out,
        splices: vec![notes::Splice {
            what: "reading-island",
            range: i.tag_start..end,
            bytes: block,
        }],
        notes,
        concurs,
    };
    notes::verify(&bytes, &plan)?;
    if read(&plan.out)? != record {
        return Err("Reading data did not read back as planned".into());
    }
    let written =
        notes::commit(path, &bytes, &plan, &notes::backups_for(path)).map_err(|e| e.sentence())?;
    Ok((record, written))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    path: std::path::PathBuf,
    expected: u64,
    session: Session,
}

fn pending_dir(path: &Path) -> Result<std::path::PathBuf, String> {
    use std::os::unix::ffi::OsStrExt;
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/state"))
        })
        .ok_or("No local state directory")?;
    Ok(state.join("terminal-delight/reading-pending").join(format!(
        "{:016x}",
        layout_hash(path.as_os_str().as_bytes()).0
    )))
}

/// A durable retry journal precedes each original-file write. An unacknowledged
/// checkpoint survives closing the view or restarting TD; success removes only
/// that session's acknowledged journal. Call on the background executor.
pub fn persist(
    path: &Path,
    expected: LayoutHash,
    session: &Session,
) -> Result<(Record, notes::Written), String> {
    persist_in(path, expected, session, &pending_dir(path)?)
}

fn persist_in(
    path: &Path,
    expected: LayoutHash,
    session: &Session,
    dir: &Path,
) -> Result<(Record, notes::Written), String> {
    session.validate()?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let journal = dir.join(format!("{}.json", session.session));
    let temp = dir.join(format!("{}.{}.tmp", session.session, std::process::id()));
    let _journal_lock = review_lock(&dir.join(&session.session))?;
    let mut pending = Pending {
        path: path.to_path_buf(),
        expected: expected.0,
        session: session.clone(),
    };
    if let Some(old) = std::fs::read(&journal)
        .ok()
        .and_then(|b| serde_json::from_slice::<Pending>(&b).ok())
    {
        if old.path != path {
            return Err("Pending reading version changed".into());
        }
        let mut merged = Record {
            format: 1,
            sessions: vec![old.session],
        };
        merged.merge(session)?;
        pending.session = merged.sessions.remove(0);
    }
    let bytes = serde_json::to_vec(&pending).map_err(|e| e.to_string())?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    std::fs::rename(&temp, &journal).map_err(|e| e.to_string())?;
    File::open(dir)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    // Retry other sessions from this file, retaining any whose authored version changed.
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let p = entry.path();
        if p == journal || p.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Some(name) = p
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| identifier(s))
        else {
            continue;
        };
        // Another active writer owns its journal. A nonblocking lock avoids
        // two sessions waiting on each other while retaining the current one.
        let Some(_other_lock) = review_lock_mode(&dir.join(name), false)? else {
            continue;
        };
        let Some(old) = std::fs::read(&p)
            .ok()
            .and_then(|b| serde_json::from_slice::<Pending>(&b).ok())
        else {
            continue;
        };
        if old.path == path
            && old.session.session == name
            && old.session.document == session.document
            && old.session.revision == session.revision
            && save(path, expected, &old.session).is_ok()
        {
            let _ = std::fs::remove_file(p);
        }
    }
    let saved = save(path, expected, &pending.session)?;
    std::fs::remove_file(journal).map_err(|e| e.to_string())?;
    File::open(dir)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(saved)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Active,
    Inactive,
    Unknown,
}

pub struct Clock {
    pub session: Session,
    pub started: bool,
    last: Option<(u64, Activity, HashSet<String>)>,
    sequence: u64,
    open_gap: Option<usize>,
}

impl Clock {
    pub fn new(layout: &ReadingLayout) -> Result<Self, String> {
        let mut random = [0_u8; 16];
        File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut random))
            .map_err(|e| e.to_string())?;
        let session = Session {
            session: format!(
                "td-{}",
                random
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            ),
            document: layout.document.clone(),
            revision: layout.revision.clone(),
            host: "terminal-delight".into(),
            method: 1,
            targets: layout
                .targets
                .iter()
                .map(|t| Target {
                    anchor: t.anchor.clone(),
                    title: t.title.clone(),
                    kind: t.kind.clone(),
                    parent: t.parent.clone(),
                    visible_ms: None,
                })
                .collect(),
            gaps: vec![],
        };
        session.validate()?;
        Ok(Self {
            session,
            started: false,
            last: None,
            sequence: 0,
            open_gap: None,
        })
    }

    pub fn sample(
        &mut self,
        now: Duration,
        activity: Activity,
        visible: &[String],
        eligible: &[String],
    ) -> Result<(), String> {
        let now = u64::try_from(now.as_millis()).map_err(|_| "Monotonic clock overflow")?;
        if let Some((before, previous, targets)) = &self.last {
            let delta = now
                .checked_sub(*before)
                .ok_or("Monotonic clock moved backwards")?;
            let reason = if delta > 3000 {
                Some("clock_gap")
            } else if *previous == Activity::Unknown {
                Some("activity_unknown")
            } else {
                None
            };
            if let Some(reason) = reason {
                match self
                    .open_gap
                    .filter(|&i| self.session.gaps[i].reason == reason)
                {
                    Some(i) => {
                        self.session.gaps[i].elapsed_ms =
                            self.session.gaps[i].elapsed_ms.saturating_add(delta)
                    }
                    None => {
                        self.sequence += 1;
                        self.session.gaps.push(Gap {
                            sequence: self.sequence,
                            reason: reason.into(),
                            elapsed_ms: delta,
                        });
                        self.open_gap = Some(self.session.gaps.len() - 1);
                    }
                }
            } else {
                self.open_gap = None;
                if *previous == Activity::Active {
                    for t in &mut self.session.targets {
                        if targets.contains(&t.anchor) {
                            if let Some(ms) = &mut t.visible_ms {
                                *ms = ms.saturating_add(delta);
                            }
                        }
                    }
                }
            }
        }
        if activity == Activity::Active {
            self.started = true;
            for t in &mut self.session.targets {
                if eligible.contains(&t.anchor) && t.visible_ms.is_none() {
                    t.visible_ms = Some(0);
                }
            }
        }
        self.last = Some((now, activity, visible.iter().cloned().collect()));
        Ok(())
    }

    pub fn checkpoint(&self) -> Option<Session> {
        self.started.then(|| self.session.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_chrome_can_cover_a_target_without_hiding_its_neighbour() {
        let viewport = RectCss {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
        };
        let bar = RectCss {
            x: 50.0,
            y: 70.0,
            w: 50.0,
            h: 30.0,
        };
        let target = RectCss {
            x: 60.0,
            y: 80.0,
            w: 20.0,
            h: 15.0,
        };
        assert!(!exposed(target, viewport, &[bar]));
        assert!(exposed(RectCss { x: 10.0, ..target }, viewport, &[bar]));
        assert!(exposed(RectCss { y: 60.0, ..target }, viewport, &[bar]));
        assert!(!exposed(target, RectCss { h: 0.0, ..viewport }, &[]));
        let left = RectCss {
            x: 0.0,
            y: 0.0,
            w: 50.0,
            h: 100.0,
        };
        let right = RectCss {
            x: 50.0,
            y: 0.0,
            w: 50.0,
            h: 100.0,
        };
        assert!(!exposed(viewport, viewport, &[left, right]));
    }

    #[test]
    fn pending_time_survives_a_runtime_rebuild_of_the_same_authored_revision() {
        let layout = ReadingLayout {
            document: "rebuild-fixture".into(),
            revision: format!("sha256:{}", "b".repeat(64)),
            targets: vec![],
        };
        let mut clock = Clock::new(&layout).unwrap();
        clock
            .sample(Duration::ZERO, Activity::Active, &[], &[])
            .unwrap();
        let first = clock.checkpoint().unwrap();
        let root = std::env::temp_dir().join(&first.session);
        std::fs::create_dir_all(&root).unwrap();
        let page = root.join("rebuild.html");
        let original = format!("<html><head><style>old runtime</style></head><body><script id=\"report-notes\" data-format=\"1\">{{}}</script><script id=\"report-concurs\" data-format=\"1\">{{}}</script><script id=\"brief-meta\">{{\"id\":\"{}\",\"revision\":\"{}\"}}</script><script id=\"report-reading\" data-format=\"1\">{{\"format\":1,\"sessions\":[]}}</script></body></html>", layout.document, layout.revision);
        let expected = layout_hash(original.as_bytes());
        let rebuilt = original.replace("old runtime", "new runtime");
        std::fs::write(&page, &rebuilt).unwrap();
        let pending = root.join("pending");
        assert!(persist_in(&page, expected, &first, &pending).is_err());
        let current = layout_hash(rebuilt.as_bytes());
        persist_in(&page, current, &first, &pending).unwrap();
        assert_eq!(
            read(&std::fs::read(&page).unwrap()).unwrap().sessions,
            vec![first.clone()]
        );
        // A second reader can recover the same revision's retained checkpoint too.
        std::fs::write(&page, &original).unwrap();
        assert!(persist_in(&page, current, &first, &pending).is_err());
        let mut reopened = Clock::new(&layout).unwrap();
        reopened
            .sample(Duration::ZERO, Activity::Active, &[], &[])
            .unwrap();
        persist_in(&page, expected, &reopened.checkpoint().unwrap(), &pending).unwrap();
        assert_eq!(
            read(&std::fs::read(&page).unwrap()).unwrap().sessions.len(),
            2
        );
        assert!(!pending.join(format!("{}.json", first.session)).exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_failed_checkpoint_survives_reopen_and_is_removed_only_after_a_verified_save() {
        let layout = ReadingLayout {
            document: "retry-fixture".into(),
            revision: format!("sha256:{}", "a".repeat(64)),
            targets: vec![],
        };
        let mut first = Clock::new(&layout).unwrap();
        first
            .sample(Duration::ZERO, Activity::Active, &[], &[])
            .unwrap();
        let first = first.checkpoint().unwrap();
        let root = std::env::temp_dir().join(&first.session);
        std::fs::create_dir_all(&root).unwrap();
        let page = root.join("retry.html");
        let original = format!("<html><body><script id=\"report-notes\" data-format=\"1\">{{}}</script><script id=\"report-concurs\" data-format=\"1\">{{}}</script><script id=\"brief-meta\">{{\"id\":\"{}\",\"revision\":\"{}\"}}</script><script id=\"report-reading\" data-format=\"1\">{{\"format\":1,\"sessions\":[]}}</script></body></html>", layout.document, layout.revision);
        let expected = layout_hash(original.as_bytes());
        std::fs::write(&page, format!("{original}changed")).unwrap();
        let pending = root.join("pending");
        assert!(persist_in(&page, expected, &first, &pending).is_err());
        let journal = pending.join(format!("{}.json", first.session));
        assert!(journal.exists());
        std::fs::write(&page, original).unwrap();
        let mut reopened = Clock::new(&layout).unwrap();
        reopened
            .sample(Duration::ZERO, Activity::Active, &[], &[])
            .unwrap();
        persist_in(&page, expected, &reopened.checkpoint().unwrap(), &pending).unwrap();
        assert!(!journal.exists());
        assert_eq!(
            read(&std::fs::read(&page).unwrap()).unwrap().sessions.len(),
            2
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
