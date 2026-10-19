//! Versioned, deterministic explanations. Inputs are already permission-filtered.
use super::*;

pub const CATALOG_VERSION: u32 = 1;
pub const STALE_MS: u64 = 30_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    SharedCheckout,
    SharedFile,
    TargetConflict,
    UnmergedIndex,
    ObservationGap,
    Unknown,
}

impl Kind {
    pub fn key(self) -> &'static str {
        match self {
            Self::SharedCheckout => "shared_checkout",
            Self::SharedFile => "shared_file",
            Self::TargetConflict => "target_conflict",
            Self::UnmergedIndex => "unmerged_index",
            Self::ObservationGap => "observation_gap",
            Self::Unknown => "unknown",
        }
    }
    pub fn sharing(self) -> bool {
        matches!(self, Self::SharedCheckout | Self::SharedFile)
    }
    pub fn conflict(self) -> bool {
        matches!(self, Self::TargetConflict | Self::UnmergedIndex)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckoutScope {
    pub checkout_id: String,
    pub branch: Option<String>,
    pub generation: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parameters {
    pub pane_count: Option<usize>,
    pub session_count: Option<usize>,
    pub checkout_count: Option<usize>,
    pub affected_path_count: Option<usize>,
    pub path: Option<PathKey>,
    pub target_ref: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    Observed,
    Unverified,
    OutsideScope,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub kind: Kind,
    pub meaning_key: String,
    pub catalog_version: u32,
    pub material_revision: String,
    pub episode: u64,
    pub parameters: Parameters,
    pub headline: String,
    pub significance: String,
    pub limitations: Vec<String>,
    pub severity: Option<Severity>,
    pub condition: Condition,
    pub observed_at_ms: Option<u64>,
    pub coverage: Coverage,
    pub scope: Vec<CheckoutScope>,
    pub evidence_ids: Vec<String>,
    pub attention: attention::Presentation,
    pub single_session: bool,
}

impl Finding {
    pub fn fresh(&self, now: u64) -> bool {
        self.condition == Condition::Observed
            && self
                .observed_at_ms
                .is_some_and(|t| now >= t && now - t <= STALE_MS)
    }
    pub fn can_expect(&self, now: u64) -> bool {
        self.kind.sharing()
            && self.fresh(now)
            && !self.scope.is_empty()
            && self
                .scope
                .iter()
                .all(|s| s.branch.is_some() && s.generation.is_some())
    }
    pub fn qualification(&self) -> String {
        let mut parts = vec![self.attention.reason.clone()];
        parts.extend(self.limitations.clone());
        parts.join(" ")
    }
    /// The HUD carries recognition, consequence and the decisive uncertainty.
    /// Attribution and the full qualifications are disclosed on inspection.
    pub fn glance_line(&self) -> String {
        let mut parts = vec![match self.condition {
            Condition::Unverified => "Earlier finding · Current result unknown",
            Condition::OutsideScope => "Earlier finding · Outside current scope",
            Condition::Observed => match self.kind {
                Kind::SharedCheckout => "Edits affect every pane · Concurrent editing unconfirmed",
                Kind::SharedFile => "May need coordination · Conflict unconfirmed",
                Kind::TargetConflict => {
                    "Checked commits won’t merge cleanly · Uncommitted edits unchecked"
                }
                Kind::UnmergedIndex => "Changes need reconciling · Cause unknown",
                Kind::ObservationGap => "Shared work or conflicts may be hidden",
                Kind::Unknown => "Meaning unavailable · Inspect evidence",
            },
        }
        .to_string()];
        match self.attention.status {
            attention::Status::Expected => {
                if let Some(e) = &self.attention.expectation {
                    parts.push(format!(
                        "Expected until {}",
                        attention::expiry_label(e.expires_at_ms)
                    ));
                }
            }
            attention::Status::Known => parts.push(
                if self.single_session {
                    "One known session"
                } else {
                    "Seen"
                }
                .into(),
            ),
            _ => {}
        }
        if self.coverage != Coverage::Complete && self.kind != Kind::ObservationGap {
            parts.push("Scan incomplete".into());
        }
        parts.join(" · ")
    }
}

fn count(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

pub fn explain(p: &Project, now: u64) -> Vec<Finding> {
    let mut out = vec![];
    for h in &p.hotspots {
        let kind = match h.kind.as_str() {
            "shared_checkout" => Kind::SharedCheckout,
            "shared_file" => Kind::SharedFile,
            "target_conflict" => Kind::TargetConflict,
            "unmerged_index" => Kind::UnmergedIndex,
            _ => Kind::Unknown,
        };
        let contexts: Vec<_> = p
            .contexts
            .iter()
            .filter(|c| h.checkout_ids.contains(&c.id))
            .collect();
        let scope: Vec<_> = contexts
            .iter()
            .map(|c| CheckoutScope {
                checkout_id: c.id.clone(),
                branch: c.branch.clone(),
                generation: c.footprint.checkout_generation.clone(),
            })
            .collect();
        let mut parameters = Parameters {
            pane_count: Some(h.participant_ids.len()),
            session_count: sessions(contexts.iter().flat_map(|c| &c.participants)),
            checkout_count: Some(contexts.len()),
            path: h.path.clone(),
            ..Parameters::default()
        };
        let first = contexts.first().copied();
        let single_session = kind == Kind::SharedCheckout
            && !contexts.is_empty()
            && contexts
                .iter()
                .flat_map(|c| &c.participants)
                .all(|p| p.agent == Some(true) && p.session.is_some())
            && parameters.session_count == Some(1);
        let mut limits = vec![];
        let (mut headline, significance) = match kind {
            Kind::SharedCheckout => {
                limits.push(
                    if single_session {
                        "These panes represent one known agent session."
                    } else {
                        "Sharing is observed; simultaneous editing is not established."
                    }
                    .into(),
                );
                (
                    format!(
                        "{} share one working folder.",
                        count(h.participant_ids.len(), "pane")
                    ),
                    "They use the same files; edits can affect the other panes.".into(),
                )
            }
            Kind::SharedFile => {
                limits.push("A shared file path does not establish a conflict.".into());
                if contexts.iter().any(|c| {
                    h.path
                        .as_ref()
                        .and_then(|path| {
                            c.footprint
                                .paths
                                .binary_search_by(|ch| ch.path.cmp(path))
                                .ok()
                        })
                        .is_some_and(|i| c.footprint.paths[i].sources.contains(&Source::Committed))
                }) {
                    limits.push("Some changes may come from shared branch history.".into());
                }
                (
                    format!(
                        "{} include changes to {}.",
                        count(contexts.len(), "working copy").replace("copys", "copies"),
                        h.path
                            .as_ref()
                            .map_or("an unavailable path", |p| p.display.as_str())
                    ),
                    "Combining their changes may require coordination.".into(),
                )
            }
            Kind::TargetConflict => {
                parameters.path = None;
                parameters.affected_path_count =
                    first.and_then(|c| c.target_conflicts.as_ref().map(Vec::len));
                parameters.target_ref = first.and_then(|c| c.footprint.base_ref.clone());
                limits.push("Uncommitted edits were not included. The comparison uses a locally observed target reference.".into());
                let headline = match parameters.affected_path_count {
                    Some(n) if n > 0 => format!(
                        "Git's merge check reports a conflict involving {}.",
                        count(n, "file")
                    ),
                    _ => {
                        limits.push("Affected-file details are unavailable.".into());
                        "Git's merge check reports a conflict.".into()
                    }
                };
                (headline, format!("The checked committed changes do not combine cleanly with {} (the assumed integration target).", parameters.target_ref.as_deref().unwrap_or("the checked target")))
            }
            Kind::UnmergedIndex => {
                parameters.path = None;
                parameters.affected_path_count = first.map(|c| c.footprint.unmerged.len());
                limits.push("The responsible operation and author may be unknown.".into());
                (
                    match parameters.affected_path_count {
                        Some(n) => format!(
                            "Git has unresolved entries for {} in this working folder.",
                            count(n, "file")
                        ),
                        None => {
                            "Git reported unresolved entries; file details are unavailable.".into()
                        }
                    },
                    "These Git changes have not been fully reconciled.".into(),
                )
            }
            _ => (
                "TD cannot explain this observation yet.".into(),
                "Inspect the evidence before drawing a conclusion.".into(),
            ),
        };
        let observed_at_ms = contexts
            .iter()
            .filter_map(|c| c.footprint.observed_at_ms)
            .min();
        let current = !contexts.is_empty()
            && contexts.iter().all(|c| {
                c.footprint
                    .observed_at_ms
                    .is_some_and(|t| now >= t && now - t <= STALE_MS)
            })
            && contexts.iter().all(|c| match kind {
                Kind::TargetConflict => c.footprint.target_check.state == TargetState::Conflict,
                Kind::UnmergedIndex => {
                    c.footprint.sources.get(&Source::Staged) == Some(&Coverage::Complete)
                }
                _ => true,
            })
            && observed_at_ms.is_some_and(|t| now >= t && now - t <= STALE_MS);
        let scope_json = serde_json::to_string(&scope).expect("scope");
        // File identity excludes participants so adding C to A/B is a changed
        // finding, not a duplicate. Repository identity prevents cross-repo joins.
        let locus = if kind == Kind::SharedFile {
            format!(
                "{}:{}",
                first
                    .and_then(|c| c.footprint.common_dir.as_deref())
                    .unwrap_or("unknown"),
                h.path.as_ref().map_or("unknown", |p| p.bytes_hex.as_str())
            )
        } else {
            h.checkout_ids.join("|")
        };
        let target = parameters.target_ref.as_deref().unwrap_or("");
        let id = stable_id("finding", &[&p.id, kind.key(), &locus, target]);
        let paths = first.and_then(|c| match kind {
            Kind::TargetConflict => c.target_conflicts.clone(),
            Kind::UnmergedIndex => Some(c.footprint.unmerged.clone()),
            _ => None,
        });
        let mut paths = paths;
        if let Some(paths) = &mut paths {
            paths.sort();
            paths.dedup();
        }
        let signature = stable_id(
            "material-v1",
            &[
                kind.key(),
                &scope_json,
                &serde_json::to_string(&paths).expect("paths"),
            ],
        );
        if p.coverage != Coverage::Complete {
            limits.push("Some working folders or changes could not be checked.".into());
        }
        if !current {
            if kind == Kind::TargetConflict {
                headline =
                    "The last merge check reported a conflict; the current result is unknown."
                        .into();
            }
            limits.insert(
                0,
                "Showing an earlier observation; a current check is unavailable.".into(),
            );
        }
        out.push(Finding {
            id,
            kind,
            meaning_key: format!("gcf.{}", kind.key()),
            catalog_version: CATALOG_VERSION,
            material_revision: signature,
            episode: 1,
            parameters,
            headline,
            significance,
            limitations: limits,
            severity: Some(h.severity),
            condition: if current {
                Condition::Observed
            } else {
                Condition::Unverified
            },
            observed_at_ms,
            coverage: p.coverage,
            scope,
            evidence_ids: vec![h.id.clone()],
            attention: attention::Presentation::default(),
            single_session,
        });
    }
    let stale = p
        .observed_at_ms
        .is_none_or(|t| now < t || now - t > STALE_MS)
        || p.contexts.iter().any(|c| {
            c.footprint
                .observed_at_ms
                .is_none_or(|t| now < t || now - t > STALE_MS)
        });
    let failed_target = p.contexts.iter().any(|c| {
        matches!(
            c.footprint.target_check.state,
            TargetState::Unavailable | TargetState::Stale
        )
    });
    if p.coverage != Coverage::Complete || stale || failed_target {
        let headline = if p.scope_limited {
            "Some evidence is unavailable in this agent-visible view."
        } else if p.observed_at_ms.is_none() {
            "Not checked yet."
        } else if stale {
            "Showing an earlier observation; a current check is unavailable."
        } else {
            "Some working folders or changes could not be checked."
        };
        let state: Vec<_> = p
            .contexts
            .iter()
            .map(|c| {
                (
                    &c.id,
                    &c.footprint.sources,
                    matches!(
                        c.footprint.target_check.state,
                        TargetState::Unavailable | TargetState::Stale
                    ),
                )
            })
            .collect();
        out.push(Finding {
            id: stable_id("finding", &[&p.id, "observation_gap"]),
            kind: Kind::ObservationGap,
            meaning_key: "gcf.observation_gap".into(),
            catalog_version: CATALOG_VERSION,
            material_revision: stable_id(
                "material-v1",
                &[&format!(
                    "{:?}:{stale}:{failed_target}:{}:{}",
                    p.coverage,
                    p.unlocated.len(),
                    serde_json::to_string(&state).expect("coverage")
                )],
            ),
            episode: 1,
            parameters: Parameters::default(),
            headline: headline.into(),
            significance: "Missing observations can hide shared work or conflicts.".into(),
            limitations: vec!["An incomplete check does not establish that work is clear.".into()],
            severity: None,
            condition: Condition::Observed,
            observed_at_ms: p.observed_at_ms,
            coverage: p.coverage,
            scope: vec![],
            evidence_ids: vec![],
            attention: attention::Presentation::default(),
            single_session: false,
        });
    }
    out
}

/// Absence needs a comparable fresh successful measurement, never a missing row.
pub fn absent(f: &Finding, p: &Project, now: u64) -> bool {
    if f.kind == Kind::ObservationGap {
        return p.coverage == Coverage::Complete
            && p.observed_at_ms
                .is_some_and(|t| now >= t && now - t <= STALE_MS)
            && p.contexts.iter().all(|c| {
                c.footprint
                    .observed_at_ms
                    .is_some_and(|t| now >= t && now - t <= STALE_MS)
                    && !matches!(
                        c.footprint.target_check.state,
                        TargetState::Unavailable | TargetState::Stale
                    )
            });
    }
    !f.scope.is_empty()
        && f.scope.iter().all(|s| {
            p.contexts
                .iter()
                .find(|c| c.id == s.checkout_id)
                .is_some_and(|c| {
                    c.branch == s.branch
                        && c.footprint.checkout_generation == s.generation
                        && c.footprint
                            .observed_at_ms
                            .is_some_and(|t| now >= t && now - t <= STALE_MS)
                        && match f.kind {
                            Kind::TargetConflict => {
                                c.footprint.target_check.state == TargetState::CheckedNone
                                    && c.footprint.base_ref == f.parameters.target_ref
                            }
                            Kind::UnmergedIndex => {
                                c.footprint.sources.get(&Source::Staged)
                                    == Some(&Coverage::Complete)
                                    && c.footprint.unmerged.is_empty()
                            }
                            Kind::SharedCheckout => c.participants.len() < 2,
                            Kind::SharedFile => p.coverage == Coverage::Complete,
                            _ => false,
                        }
                })
        })
}

pub fn lead(p: &Project) -> String {
    p.meanings
        .iter()
        .min_by_key(|f| (f.attention.rank(f.kind), std::cmp::Reverse(f.severity)))
        .map(|f| f.headline.clone())
        .unwrap_or_else(|| {
            if p.coverage == Coverage::Complete {
                "No shared-work overlap found in the checks completed.".into()
            } else {
                "Not checked yet.".into()
            }
        })
}
