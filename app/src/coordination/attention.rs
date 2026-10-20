//! Operator metadata, not Git truth or an agent execution policy.
use super::{
    meaning::{CheckoutScope, Finding, Kind},
    *,
};
use std::os::unix::{
    fs::{DirBuilderExt, OpenOptionsExt},
    io::AsRawFd,
};

const MAX_RECORDS: usize = 512;
const RETENTION_MS: u64 = 7 * 24 * 60 * 60 * 1000;

struct WriterLock(std::fs::File);
impl Drop for WriterLock {
    fn drop(&mut self) {
        // Unlock explicitly: a concurrently spawned subprocess can inherit
        // this open description between fork and exec. Closing only our fd
        // leaves its transient copy holding the lock.
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    New,
    Changed,
    Known,
    Expected,
}

/// Why a finding is presented as it is, as a key rather than as English, so
/// the overlay can say it in the reader's language. [`Presentation::reason`]
/// is this said in English, and that is what agents read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    New,
    SingleSession,
    SeenConflict,
    SeenUnchanged,
    Changed,
    /// Expected until this instant (ms since the epoch).
    Expected(u64),
    Expired,
    Revoked,
    ScopeExceeded,
    Unverifiable,
    Reconfirm,
}

impl Why {
    pub fn say(self, l: crate::lang::Lang) -> String {
        let t = l.strings().gc;
        match self {
            Why::New => t.reason_new,
            Why::SingleSession => t.reason_single_session,
            Why::SeenConflict => t.reason_seen_conflict,
            Why::SeenUnchanged => t.reason_seen_unchanged,
            Why::Changed => t.reason_changed,
            Why::Expected(at) => {
                return crate::lang::fill(t.reason_expected, &[("time", &expiry_label(at))]);
            }
            Why::Expired => t.reason_expired,
            Why::Revoked => t.reason_revoked,
            Why::ScopeExceeded => t.reason_scope_exceeded,
            Why::Unverifiable => t.reason_unverifiable,
            Why::Reconfirm => t.reason_reconfirm,
        }
        .to_string()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presentation {
    pub status: Status,
    pub reason: String,
    pub expectation: Option<Expectation>,
    pub expectation_state: Option<String>,
    /// [`Presentation::reason`] as a key. Absent on a presentation read back
    /// from anywhere but [`Memory::present`]; the English then stands.
    #[serde(skip)]
    pub why: Option<Why>,
}
impl Default for Presentation {
    fn default() -> Self {
        Self::because(Status::New, Why::New)
    }
}
impl Presentation {
    fn because(status: Status, why: Why) -> Self {
        Self {
            status,
            reason: why.say(crate::lang::Lang::En),
            expectation: None,
            expectation_state: None,
            why: Some(why),
        }
    }
    fn set(&mut self, why: Why) {
        self.reason = why.say(crate::lang::Lang::En);
        self.why = Some(why);
    }
    /// The reason in the reader's language, or the English when no key came
    /// with it.
    pub fn reason_in(&self, l: crate::lang::Lang) -> String {
        self.why.map_or_else(|| self.reason.clone(), |w| w.say(l))
    }
    pub fn novel(&self) -> bool {
        matches!(self.status, Status::New | Status::Changed)
    }
    pub fn rank(&self, kind: Kind) -> u8 {
        if self.novel() {
            0
        } else if kind.conflict() {
            1
        } else {
            2
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expectation {
    pub actor: String,
    pub kind: Kind,
    pub scope: Vec<CheckoutScope>,
    pub path: Option<PathKey>,
    pub epoch: String,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
    pub revoked_at_ms: Option<u64>,
    pub participant_scope: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub project_id: String,
    pub finding_id: String,
    pub epoch: String,
    pub material_revision: String,
    pub episode: u64,
    pub acknowledged: bool,
    pub expectation: Option<Expectation>,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub schema_version: u32,
    pub revision: u64,
    pub records: Vec<Record>,
    pub last_request: Option<String>,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            records: vec![],
            last_request: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Memory {
    pub document: Document,
    pub path: Option<PathBuf>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub enum Mutation {
    Acknowledge,
    Expect(u64),
    Revoke,
}

pub fn path() -> PathBuf {
    let root = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::session::home_dir().join(".local/state"));
    root.join("terminal-delight/coordination-attention")
        .join(format!(
            "{}.json",
            super::stable_id("session", &[crate::instance::key()])
        ))
}

fn read(path: &std::path::Path) -> Result<Document, String> {
    match std::fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Document::default()),
        Err(e) => Err(format!("Expectations could not be restored: {e}")),
        Ok(bytes) => {
            if bytes.len() > 2 * 1024 * 1024 {
                return Err("Attention store exceeds its size limit".into());
            }
            let doc: Document = serde_json::from_slice(&bytes)
                .map_err(|e| format!("Expectations could not be restored: {e}"))?;
            if doc.schema_version != 1 || doc.records.len() > MAX_RECORDS {
                return Err("Unsupported or oversized attention store".into());
            }
            Ok(doc)
        }
    }
}

impl Memory {
    pub fn load(path: PathBuf) -> Self {
        let mut memory = Self {
            path: Some(path),
            ..Self::default()
        };
        memory.reload();
        memory
    }
    pub fn reload(&mut self) {
        let Some(path) = &self.path else { return };
        match read(path) {
            Ok(doc) => {
                self.document = doc;
                self.error = None;
            }
            Err(e) => {
                self.document = Document::default();
                self.error = Some(e);
            }
        }
    }
    pub fn present(&self, project: &str, f: &Finding, epoch: &str, now: u64) -> Presentation {
        let mut result = Presentation::default();
        if f.single_session {
            result.status = Status::Known;
            result.set(Why::SingleSession);
        }
        let Some(r) = self
            .document
            .records
            .iter()
            .find(|r| r.project_id == project && r.finding_id == f.id)
        else {
            return result;
        };
        if r.acknowledged
            && r.epoch == epoch
            && r.material_revision == f.material_revision
            && r.episode == f.episode
        {
            result.status = Status::Known;
            result.set(if f.kind.conflict() {
                Why::SeenConflict
            } else {
                Why::SeenUnchanged
            });
        } else if r.acknowledged {
            result.status = Status::Changed;
            result.set(Why::Changed);
        }
        if let Some(e) = &r.expectation {
            let state = if e.revoked_at_ms.is_some() {
                "revoked"
            } else if now >= e.expires_at_ms {
                "expired"
            } else if e.epoch != epoch {
                "needs_reconfirmation"
            } else if e.scope != f.scope || e.path != f.parameters.path || e.kind != f.kind {
                "scope_exceeded"
            } else if !f.fresh(now) {
                "unverifiable"
            } else {
                "applicable"
            };
            result.expectation = Some(e.clone());
            result.expectation_state = Some(state.into());
            if state == "applicable" && f.kind.sharing() {
                result.status = Status::Expected;
                result.set(Why::Expected(e.expires_at_ms));
            } else {
                // Expiry/revocation can be acknowledged once; it is not a new
                // event on every subsequent scan.
                if !r.acknowledged {
                    result.status = Status::Changed;
                }
                result.set(match state {
                    "expired" => Why::Expired,
                    "revoked" => Why::Revoked,
                    "scope_exceeded" => Why::ScopeExceeded,
                    "unverifiable" => Why::Unverifiable,
                    _ => Why::Reconfirm,
                });
            }
        }
        result
    }
    /// Operator UI only. Expected revision and a request key prevent a stale
    /// control or concurrent window from overwriting another declaration.
    // Keep the optimistic-write preconditions explicit at the UI boundary.
    #[allow(clippy::too_many_arguments)]
    pub fn mutate(
        &mut self,
        project: &str,
        f: &Finding,
        epoch: &str,
        now: u64,
        expected_revision: u64,
        request: &str,
        mutation: Mutation,
    ) -> Result<(), String> {
        if request.is_empty() {
            return Err("A request key is required".into());
        }
        if matches!(mutation, Mutation::Expect(_)) && !f.can_expect(now) {
            return Err("Fresh sharing evidence, branch and checkout identity are required".into());
        }
        if let Mutation::Expect(duration) = mutation {
            if ![900_000, 3_600_000, 14_400_000].contains(&duration) {
                return Err("Choose 15 minutes, one hour, or four hours".into());
            }
        }
        let lock = if let Some(path) = &self.path {
            let parent = path.parent().ok_or("Attention store has no parent")?;
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)
                .map_err(|e| e.to_string())?;
            let lock = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .open(path.with_extension("lock"))
                .map_err(|e| e.to_string())?;
            if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err("Another window is saving attention; retry after refreshing".into());
            }
            Some(WriterLock(lock))
        } else {
            None
        };
        let mut doc = if let Some(path) = &self.path {
            read(path)?
        } else {
            self.document.clone()
        };
        if doc.last_request.as_deref() == Some(request) {
            self.document = doc;
            return Ok(());
        }
        if doc.revision != expected_revision {
            return Err("Attention changed in another window; refresh before saving".into());
        }
        doc.records
            .retain(|r| now.saturating_sub(r.updated_at_ms) <= RETENTION_MS);
        let old = doc
            .records
            .iter()
            .find(|r| r.project_id == project && r.finding_id == f.id);
        let mut record = Record {
            project_id: project.into(),
            finding_id: f.id.clone(),
            epoch: epoch.into(),
            material_revision: f.material_revision.clone(),
            episode: f.episode,
            acknowledged: matches!(mutation, Mutation::Acknowledge),
            expectation: old.and_then(|r| r.expectation.clone()),
            updated_at_ms: now,
        };
        match mutation {
            Mutation::Acknowledge => {}
            Mutation::Expect(duration) => {
                record.expectation = Some(Expectation {
                    actor: "local_operator".into(),
                    kind: f.kind,
                    scope: f.scope.clone(),
                    path: f.parameters.path.clone(),
                    epoch: epoch.into(),
                    created_at_ms: now,
                    expires_at_ms: now.checked_add(duration).ok_or("Clock overflow")?,
                    revoked_at_ms: None,
                    participant_scope: if f.kind == Kind::SharedCheckout {
                        "any_panes_in_this_checkout"
                    } else {
                        "exact_file_and_checkout_set"
                    }
                    .into(),
                })
            }
            Mutation::Revoke => {
                record
                    .expectation
                    .as_mut()
                    .ok_or("No expectation to revoke")?
                    .revoked_at_ms = Some(now);
            }
        }
        doc.records
            .retain(|r| !(r.project_id == project && r.finding_id == f.id));
        if doc.records.len() >= MAX_RECORDS {
            return Err("Attention history is full; old records expire after seven days".into());
        }
        doc.records.push(record);
        doc.revision = doc.revision.checked_add(1).ok_or("Revision overflow")?;
        doc.last_request = Some(request.into());
        if let Some(path) = &self.path {
            crate::session::write_atomic(
                path,
                &serde_json::to_string(&doc).map_err(|e| e.to_string())?,
            )
            .map_err(|e| format!("Attention was not saved: {e}"))?;
            std::fs::File::open(path.parent().ok_or("Missing store parent")?)
                .and_then(|f| f.sync_all())
                .map_err(|e| format!("Attention durability could not be confirmed: {e}"))?;
        }
        drop(lock);
        self.document = doc;
        self.error = None;
        Ok(())
    }
}

pub fn expiry_label(ms: u64) -> String {
    // Absolute UTC is unambiguous across DST and restart; the native card also
    // names the selected duration before confirmation.
    let seconds = (ms / 1000) as libc::time_t;
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    if unsafe { libc::gmtime_r(&seconds, tm.as_mut_ptr()) }.is_null() {
        return format!("{ms} ms since epoch");
    }
    let tm = unsafe { tm.assume_init() };
    format!("{:02}:{:02} UTC", tm.tm_hour, tm.tm_min)
}
