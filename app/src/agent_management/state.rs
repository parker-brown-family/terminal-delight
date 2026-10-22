use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Fact<T> {
    Observed {
        value: T,
        source: String,
        at_ms: u64,
    },
    Unknown {
        reason: String,
    },
    #[default]
    NotCollected,
}

impl<T> Fact<T> {
    pub fn observed(value: T, source: impl Into<String>) -> Self {
        Self::Observed {
            value,
            source: source.into(),
            at_ms: now_ms(),
        }
    }
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Observed { value, .. } => Some(value),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessKey {
    pub boot_id: String,
    pub pid: u32,
    pub start_ticks: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunKey {
    pub harness: String,
    pub conversation: String,
    pub process: ProcessKey,
    pub endpoint: String,
    pub endpoint_generation: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub td_session: String,
    pub pane: u64,
    pub shell: ProcessKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelFacts {
    pub requested: Fact<String>,
    pub reported: Fact<String>,
    pub provider_revision: Fact<String>,
    pub effort: Fact<String>,
}

impl Default for ModelFacts {
    fn default() -> Self {
        Self {
            requested: Fact::NotCollected,
            reported: Fact::NotCollected,
            provider_revision: Fact::Unknown {
                reason: "Provider revision is not exposed by the local harness".into(),
            },
            effort: Fact::NotCollected,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    pub run: RunKey,
    pub location: Location,
    pub executable: Fact<String>,
    pub version: Fact<String>,
    pub inference_runtime: Fact<Value>,
    pub model: ModelFacts,
    pub binding_evidence: String,
    #[serde(default)]
    pub attribution: Fact<Value>,
    #[serde(default)]
    pub transcript: Fact<String>,
    pub observed_at_ms: u64,
}

impl Actor {
    pub fn conversation_key(&self) -> String {
        format!("{}:{}", self.run.harness, self.run.conversation)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendRequest {
    pub key: String,
    pub target: RunKey,
    pub text: String,
    pub dispatch_before_ms: u64,
    pub parent_request: Option<String>,
    pub caused_by_message: Option<String>,
    pub expected_model: Option<String>,
    pub experiment: Option<String>,
}

impl SendRequest {
    pub fn validate(&self, now: u64) -> Result<()> {
        validate_key(&self.key, now)?;
        if self.text.trim().is_empty() || self.text.len() > 32 * 1024 {
            return Err("text must contain 1..32768 UTF-8 bytes".into());
        }
        if self.dispatch_before_ms <= now
            || self.dispatch_before_ms > now.saturating_add(86_400_000)
        {
            return Err("dispatch_before_ms must be in the next 24 hours".into());
        }
        if self.experiment.as_ref().is_some_and(|s| s.len() > 256) {
            return Err("experiment exceeds 256 bytes".into());
        }
        Ok(())
    }
}

pub fn validate_key(key: &str, now: u64) -> Result<()> {
    let id = uuid::Uuid::parse_str(key).map_err(|_| "key must be a UUIDv7")?;
    if id.get_version_num() != 7 {
        return Err("key must be a UUIDv7".into());
    }
    let (seconds, nanos) = id
        .get_timestamp()
        .ok_or("UUIDv7 timestamp missing")?
        .to_unix();
    let time = seconds.saturating_mul(1000) + u64::from(nanos / 1_000_000);
    if time > now.saturating_add(300_000) || now.saturating_sub(time) > 7 * 86_400_000 {
        return Err(
            "key timestamp is outside the seven-day replay horizon or five-minute clock allowance"
                .into(),
        );
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportKind {
    Received,
    Started,
    Progress,
    Question,
    Result,
    Failed,
    Declined,
    Verified,
    RejectedResult,
    Abandoned,
}

impl ReportKind {
    pub fn is_resolution(&self) -> bool {
        matches!(
            self,
            Self::Verified | Self::RejectedResult | Self::Abandoned
        )
    }
    pub fn releases_credit(&self) -> bool {
        matches!(
            self,
            Self::Received
                | Self::Started
                | Self::Result
                | Self::Failed
                | Self::Declined
                | Self::Abandoned
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub request: String,
    pub reply_key: String,
    pub expected_run: RunKey,
    pub kind: ReportKind,
    pub text: String,
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub sequence: u64,
    pub request: String,
    pub at_ms: u64,
    #[serde(default)]
    pub source_time: Fact<Value>,
    pub kind: String,
    pub data: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Receipt {
    pub database_id: String,
    pub request: String,
    pub confirmed_by_td: u64,
    pub native_delivery: String,
    pub recipient_ack: Fact<u64>,
    pub work_reports: Vec<u64>,
    pub resolution_reports: Vec<u64>,
    pub attempts: Vec<u64>,
    pub events: Vec<Event>,
}

/// Independent dimensions: a transport acceptance never means consumer ACK or success.
pub fn receipt(database_id: String, request: String, events: Vec<Event>) -> Result<Receipt> {
    let accepted = events
        .iter()
        .find(|e| e.kind == "accepted")
        .ok_or("request has no acceptance record")?;
    let mut out = Receipt {
        database_id,
        request,
        confirmed_by_td: accepted.sequence,
        native_delivery: "pending".into(),
        recipient_ack: Fact::Unknown {
            reason: "No correlated consumption evidence or recipient acknowledgement".into(),
        },
        work_reports: vec![],
        resolution_reports: vec![],
        attempts: vec![],
        events: vec![],
    };
    for e in &events {
        match e.kind.as_str() {
            "dispatch_intent" => {
                out.attempts.push(e.sequence);
                out.native_delivery = "dispatching".into();
            }
            "native_accepted"
            | "native_held"
            | "native_refused"
            | "native_unsupported"
            | "delivery_unconfirmed"
            | "definite_nonacceptance"
            | "expired_before_dispatch"
            | "target_stale"
            | "policy_revoked"
            | "cancel_confirmed" => out.native_delivery = e.kind.clone(),
            "recipient_acknowledged" => {
                out.recipient_ack = Fact::Observed {
                    value: e.sequence,
                    source: e.data["scope"]
                        .as_str()
                        .unwrap_or("correlated acknowledgement")
                        .into(),
                    at_ms: e.at_ms,
                }
            }
            "work_reported" => out.work_reports.push(e.sequence),
            "resolution_reported" => out.resolution_reports.push(e.sequence),
            _ => {} // Future records remain visible without changing execution state.
        }
    }
    out.events = events;
    Ok(out)
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock predates the Unix epoch; management cannot timestamp records")
        .as_millis()
        .try_into()
        .expect("system clock exceeds management timestamp range")
}
