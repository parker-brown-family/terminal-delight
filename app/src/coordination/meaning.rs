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

/// One sentence a finding can say, as a key and its numbers rather than as
/// English. [`Finding`]'s text fields are these said with the English table,
/// and that is what agents read over MCP and what "Copy assessment context"
/// copies; the overlay says the same keys in the reader's language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    SharedCheckout(usize),
    /// Working copies, and the shared path's display form when known.
    SharedFile(usize, Option<String>),
    /// Affected files, when Git reported how many.
    TargetConflict(Option<usize>),
    Unmerged(Option<usize>),
    Unexplained,
    LastMergeUnknown,
    GapScopeLimited,
    GapNotChecked,
    GapPartial,
    SigSharedCheckout,
    SigSharedFile,
    /// The integration target's name, when observed.
    SigTargetConflict(Option<String>),
    SigUnmerged,
    SigUnexplained,
    SigGap,
    OneSession,
    SharingNotSimultaneous,
    SharedPathNotConflict,
    BranchHistory,
    UncommittedExcluded,
    AffectedUnavailable,
    OperationUnknown,
    SomeUnchecked,
    EarlierObservation,
    IncompleteNotClear,
    NoneFound,
}

impl Line {
    pub fn say(&self, l: crate::lang::Lang) -> String {
        use crate::lang::fill;
        let t = l.strings().gc;
        let path = |p: &Option<String>| p.clone().unwrap_or_else(|| t.path_unavailable.into());
        match self {
            Line::SharedCheckout(n) => return l.count(&t.hl_shared_checkout, *n),
            Line::SharedFile(n, p) => {
                return fill(&l.count(&t.hl_shared_file, *n), &[("path", &path(p))]);
            }
            Line::TargetConflict(Some(n)) if *n > 0 => {
                return l.count(&t.hl_target_conflict, *n);
            }
            Line::TargetConflict(_) => t.hl_target_conflict_nocount,
            Line::Unmerged(Some(n)) => return l.count(&t.hl_unmerged, *n),
            Line::Unmerged(None) => t.hl_unmerged_nocount,
            Line::Unexplained => t.hl_unknown,
            Line::LastMergeUnknown => t.hl_last_merge_unknown,
            Line::GapScopeLimited => t.hl_gap_scope_limited,
            Line::GapNotChecked => t.hl_gap_not_checked,
            Line::GapPartial => t.hl_gap_partial,
            Line::SigSharedCheckout => t.sig_shared_checkout,
            Line::SigSharedFile => t.sig_shared_file,
            Line::SigTargetConflict(target) => {
                let target = target.clone().unwrap_or_else(|| t.checked_target.into());
                return fill(t.sig_target_conflict, &[("target", &target)]);
            }
            Line::SigUnmerged => t.sig_unmerged,
            Line::SigUnexplained => t.sig_unknown,
            Line::SigGap => t.sig_gap,
            Line::OneSession => t.lim_one_session,
            Line::SharingNotSimultaneous => t.lim_sharing_not_simultaneous,
            Line::SharedPathNotConflict => t.lim_shared_path_not_conflict,
            Line::BranchHistory => t.lim_branch_history,
            Line::UncommittedExcluded => t.lim_uncommitted_excluded,
            Line::AffectedUnavailable => t.lim_affected_unavailable,
            Line::OperationUnknown => t.lim_operation_unknown,
            Line::SomeUnchecked => t.lim_some_unchecked,
            Line::EarlierObservation => t.lim_earlier_observation,
            Line::IncompleteNotClear => t.lim_gap,
            Line::NoneFound => t.lead_none_found,
        }
        .to_string()
    }
    fn en(&self) -> String {
        self.say(crate::lang::Lang::En)
    }
}

/// A finding's sentences as [`Line`]s.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    pub headline: Line,
    pub significance: Line,
    pub limitations: Vec<Line>,
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
    /// The text above as keys. Absent on a finding read back from anywhere
    /// but [`explain`]; the English then stands, which is honest if dated.
    #[serde(skip)]
    pub said: Option<Said>,
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
    #[cfg(test)]
    pub fn qualification(&self) -> String {
        self.qualification_in(crate::lang::Lang::En)
    }
    pub fn qualification_in(&self, l: crate::lang::Lang) -> String {
        let mut parts = vec![self.attention.reason_in(l)];
        match &self.said {
            Some(said) => parts.extend(said.limitations.iter().map(|x| x.say(l))),
            None => parts.extend(self.limitations.clone()),
        }
        parts.join(" ")
    }
    pub fn headline_in(&self, l: crate::lang::Lang) -> String {
        self.said
            .as_ref()
            .map_or_else(|| self.headline.clone(), |s| s.headline.say(l))
    }
    pub fn significance_in(&self, l: crate::lang::Lang) -> String {
        self.said
            .as_ref()
            .map_or_else(|| self.significance.clone(), |s| s.significance.say(l))
    }
    /// The HUD carries recognition, consequence and the decisive uncertainty.
    /// Attribution and the full qualifications are disclosed on inspection.
    #[cfg(test)]
    pub fn glance_line(&self) -> String {
        self.glance_line_in(crate::lang::Lang::En)
    }
    pub fn glance_line_in(&self, l: crate::lang::Lang) -> String {
        let t = l.strings().gc;
        let mut parts: Vec<String> = match self.condition {
            Condition::Unverified => vec![t.glance_earlier_finding, t.glance_current_unknown],
            Condition::OutsideScope => vec![t.glance_earlier_finding, t.glance_outside_scope],
            Condition::Observed => match self.kind {
                Kind::SharedCheckout => vec![
                    t.glance_edits_affect_every_pane,
                    t.glance_concurrent_unconfirmed,
                ],
                Kind::SharedFile => {
                    vec![
                        t.glance_may_need_coordination,
                        t.glance_conflict_unconfirmed,
                    ]
                }
                Kind::TargetConflict => {
                    vec![t.glance_commits_wont_merge, t.glance_uncommitted_unchecked]
                }
                Kind::UnmergedIndex => {
                    vec![t.glance_changes_need_reconciling, t.glance_cause_unknown]
                }
                Kind::ObservationGap => vec![t.glance_shared_work_hidden],
                Kind::Unknown => vec![t.glance_meaning_unavailable, t.glance_inspect_evidence],
            },
        }
        .into_iter()
        .map(String::from)
        .collect();
        match self.attention.status {
            attention::Status::Expected => {
                if let Some(e) = &self.attention.expectation {
                    parts.push(crate::lang::fill(
                        t.glance_expected_until,
                        &[("time", &attention::expiry_label(e.expires_at_ms))],
                    ));
                }
            }
            attention::Status::Known => parts.push(
                if self.single_session {
                    t.glance_one_known_session
                } else {
                    t.glance_seen
                }
                .into(),
            ),
            _ => {}
        }
        if self.coverage != Coverage::Complete && self.kind != Kind::ObservationGap {
            parts.push(t.glance_scan_incomplete.into());
        }
        parts.join(" · ")
    }
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
                limits.push(if single_session {
                    Line::OneSession
                } else {
                    Line::SharingNotSimultaneous
                });
                (
                    Line::SharedCheckout(h.participant_ids.len()),
                    Line::SigSharedCheckout,
                )
            }
            Kind::SharedFile => {
                limits.push(Line::SharedPathNotConflict);
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
                    limits.push(Line::BranchHistory);
                }
                (
                    Line::SharedFile(contexts.len(), h.path.as_ref().map(|p| p.display.clone())),
                    Line::SigSharedFile,
                )
            }
            Kind::TargetConflict => {
                parameters.path = None;
                parameters.affected_path_count =
                    first.and_then(|c| c.target_conflicts.as_ref().map(Vec::len));
                parameters.target_ref = first.and_then(|c| c.footprint.base_ref.clone());
                limits.push(Line::UncommittedExcluded);
                if !matches!(parameters.affected_path_count, Some(n) if n > 0) {
                    limits.push(Line::AffectedUnavailable);
                }
                (
                    Line::TargetConflict(parameters.affected_path_count),
                    Line::SigTargetConflict(parameters.target_ref.clone()),
                )
            }
            Kind::UnmergedIndex => {
                parameters.path = None;
                parameters.affected_path_count = first.map(|c| c.footprint.unmerged.len());
                limits.push(Line::OperationUnknown);
                (
                    Line::Unmerged(parameters.affected_path_count),
                    Line::SigUnmerged,
                )
            }
            _ => (Line::Unexplained, Line::SigUnexplained),
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
            limits.push(Line::SomeUnchecked);
        }
        if !current {
            if kind == Kind::TargetConflict {
                headline = Line::LastMergeUnknown;
            }
            limits.insert(0, Line::EarlierObservation);
        }
        out.push(Finding {
            id,
            kind,
            meaning_key: format!("gcf.{}", kind.key()),
            catalog_version: CATALOG_VERSION,
            material_revision: signature,
            episode: 1,
            parameters,
            headline: headline.en(),
            significance: significance.en(),
            limitations: limits.iter().map(Line::en).collect(),
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
            said: Some(Said {
                headline,
                significance,
                limitations: limits,
            }),
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
            Line::GapScopeLimited
        } else if p.observed_at_ms.is_none() {
            Line::GapNotChecked
        } else if stale {
            Line::EarlierObservation
        } else {
            Line::GapPartial
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
            headline: headline.en(),
            significance: Line::SigGap.en(),
            limitations: vec![Line::IncompleteNotClear.en()],
            severity: None,
            condition: Condition::Observed,
            observed_at_ms: p.observed_at_ms,
            coverage: p.coverage,
            scope: vec![],
            evidence_ids: vec![],
            attention: attention::Presentation::default(),
            single_session: false,
            said: Some(Said {
                headline,
                significance: Line::SigGap,
                limitations: vec![Line::IncompleteNotClear],
            }),
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
    lead_in(p, crate::lang::Lang::En)
}

/// The project's leading sentence in the reader's language. The finding it
/// comes from is [`lead_finding`]; with none, the project says whether its
/// checks found nothing or have not run.
pub fn lead_in(p: &Project, l: crate::lang::Lang) -> String {
    lead_finding(p).map_or_else(
        || {
            if p.coverage == Coverage::Complete {
                Line::NoneFound
            } else {
                Line::GapNotChecked
            }
            .say(l)
        },
        |f| f.headline_in(l),
    )
}

pub fn lead_finding(p: &Project) -> Option<&Finding> {
    p.meanings
        .iter()
        .min_by_key(|f| (f.attention.rank(f.kind), std::cmp::Reverse(f.severity)))
}
