use super::*;
use attention::{Memory, Mutation, Status};
use meaning::{Condition, Kind};

fn live(id: &str, paths: &[(&[u8], Source)]) -> Context {
    let mut c = context(id, paths);
    c.footprint.observed_at_ms = unix_ms();
    c.footprint.checkout_generation = Some(format!("generation-{id}"));
    c.footprint.target_check.state = TargetState::CheckedNone;
    c
}
fn shared() -> Context {
    let mut c = live("a", &[]);
    c.participants.push(actor("b", None, true));
    c
}
fn finding(store: &Store, kind: Kind) -> meaning::Finding {
    store.snapshot.projects[0]
        .meanings
        .iter()
        .find(|f| f.kind == kind)
        .unwrap()
        .clone()
}
fn act(store: &mut Store, kind: Kind, mutation: Mutation) {
    let f = finding(store, kind);
    let revision = store.memory.document.revision;
    store
        .memory
        .mutate(
            "p",
            &f,
            &store.attention_epoch,
            unix_ms().unwrap(),
            revision,
            &format!("request-{revision}"),
            mutation,
        )
        .unwrap();
    store.operator_memory(store.memory.clone());
}

#[test]
fn forty_panes_describe_occupancy_and_one_known_session_stays_quiet() {
    let mut c = live("a", &[]);
    c.participants = (0..40)
        .map(|i| actor(&format!("p{i}"), None, true))
        .collect();
    let mut store = Store::new("test");
    store.update(project(vec![c.clone()]));
    let f = finding(&store, Kind::SharedCheckout);
    assert_eq!(f.headline, "40 panes share one working folder.");
    assert_eq!(f.parameters.session_count, None);
    assert!(f.attention.novel());
    for p in &mut c.participants {
        p.session = Some("one-session".into());
    }
    store.update(project(vec![c]));
    let f = finding(&store, Kind::SharedCheckout);
    assert_eq!(f.parameters.session_count, Some(1));
    assert_eq!(f.attention.status, Status::Known);
    assert_eq!(f.severity, Some(Severity::Caution));
    assert!(f.glance_line().contains("One known session"));
    assert!(!f.glance_line().contains("Seen"));
}

#[test]
fn one_member_without_an_observation_age_cannot_make_sharing_expectable() {
    let a = live("a", &[(b"shared.rs", Source::Unstaged)]);
    let mut b = live("b", &[(b"shared.rs", Source::Unstaged)]);
    b.footprint.observed_at_ms = None;
    let mut store = Store::new("test");
    store.update(project(vec![a, b]));
    let f = finding(&store, Kind::SharedFile);
    assert_eq!(f.condition, Condition::Unverified);
    assert!(!f.can_expect(unix_ms().unwrap()));
    assert!(finding(&store, Kind::ObservationGap).attention.novel());
}

#[test]
fn expected_folder_survives_pane_growth_titles_and_commits_but_not_new_conflicts() {
    let mut c = shared();
    let mut store = Store::new("test");
    store.update(project(vec![c.clone()]));
    act(
        &mut store,
        Kind::SharedCheckout,
        Mutation::Expect(3_600_000),
    );
    let original = finding(&store, Kind::SharedCheckout);
    c.participants.push(actor("new-pane", None, true));
    c.participants[0].title = "renamed".into();
    c.footprint.head_oid = Some("next".into());
    store.update(project(vec![c.clone()]));
    let f = finding(&store, Kind::SharedCheckout);
    assert_eq!(f.material_revision, original.material_revision);
    assert_eq!(f.attention.status, Status::Expected);
    assert_eq!(store.attention_summary(), "Git · No new findings");
    assert_eq!(store.snapshot.attention_severity(), None);
    c.footprint.unmerged = vec![PathKey::new(b"a.rs")];
    c.footprint.target_check.state = TargetState::Conflict;
    c.target_conflicts = Some(vec![PathKey::new(b"b.rs"), PathKey::new(b"c.rs")]);
    store.update(project(vec![c]));
    assert_eq!(
        finding(&store, Kind::SharedCheckout).attention.status,
        Status::Expected
    );
    assert!(finding(&store, Kind::UnmergedIndex).attention.novel());
    let target = finding(&store, Kind::TargetConflict);
    assert_eq!(
        target.headline,
        "Git's merge check reports a conflict involving 2 files."
    );
    assert!(target.attention.novel());
    assert_eq!(store.snapshot.attention_severity(), Some(Severity::Alert));
    act(&mut store, Kind::TargetConflict, Mutation::Acknowledge);
    act(&mut store, Kind::UnmergedIndex, Mutation::Acknowledge);
    assert_eq!(
        store.attention_summary(),
        "Git · 1 project with a conflict · seen"
    );
    assert_eq!(store.snapshot.attention_severity(), None);
}

#[test]
fn exact_file_scope_does_not_spread_to_a_third_checkout_or_a_bridging_file() {
    let a = live("a", &[(b"shared.rs", Source::Unstaged)]);
    let mut b = live("b", &[(b"shared.rs", Source::Committed)]);
    let mut store = Store::new("test");
    store.update(project(vec![a.clone(), b.clone()]));
    act(&mut store, Kind::SharedFile, Mutation::Expect(3_600_000));
    let original = finding(&store, Kind::SharedFile);
    b.participants.push(actor("another-view", None, true));
    store.update(project(vec![a.clone(), b.clone()]));
    assert_eq!(
        finding(&store, Kind::SharedFile).attention.status,
        Status::Expected
    );
    let c = live(
        "c",
        &[
            (b"shared.rs", Source::Unstaged),
            (b"bridge.rs", Source::Unstaged),
        ],
    );
    b.footprint.paths.push(PathChange {
        path: PathKey::new(b"bridge.rs"),
        sources: BTreeSet::from([Source::Unstaged]),
    });
    store.update(project(vec![a, b, c]));
    let changed = store.snapshot.projects[0]
        .meanings
        .iter()
        .find(|f| f.id == original.id)
        .unwrap();
    assert_eq!(
        changed.attention.expectation_state.as_deref(),
        Some("scope_exceeded")
    );
    assert!(changed.attention.novel());
    assert!(store.snapshot.projects[0]
        .meanings
        .iter()
        .filter(|f| f.kind == Kind::SharedFile)
        .all(|f| f.attention.novel()));
}

#[test]
fn failure_is_not_resolution_and_verified_recurrence_is_a_new_episode() {
    let mut c = live("a", &[]);
    c.footprint.target_check.state = TargetState::Conflict;
    c.target_conflicts = Some(vec![PathKey::new(b"a")]);
    let mut store = Store::new("test");
    store.update(project(vec![c.clone()]));
    act(&mut store, Kind::TargetConflict, Mutation::Acknowledge);
    let before = finding(&store, Kind::TargetConflict);
    let mut failed = c.clone();
    failed.footprint.target_check.state = TargetState::Unavailable;
    failed.target_conflicts = None;
    store.update(project(vec![failed]));
    assert_eq!(
        finding(&store, Kind::TargetConflict).condition,
        Condition::Unverified
    );
    assert_eq!(
        finding(&store, Kind::TargetConflict).attention.status,
        Status::Known
    );
    assert!(finding(&store, Kind::ObservationGap).attention.novel());
    store.update(project(vec![c.clone()]));
    assert_eq!(
        finding(&store, Kind::TargetConflict).episode,
        before.episode
    );
    assert_eq!(
        finding(&store, Kind::TargetConflict).attention.status,
        Status::Known
    );
    let mut clean = c.clone();
    clean.footprint.target_check.state = TargetState::CheckedNone;
    clean.target_conflicts = Some(vec![]);
    store.update(project(vec![clean]));
    assert!(!store.snapshot.projects[0]
        .meanings
        .iter()
        .any(|f| f.kind == Kind::TargetConflict));
    store.update(project(vec![c]));
    assert!(finding(&store, Kind::TargetConflict).episode > before.episode);
    assert!(finding(&store, Kind::TargetConflict).attention.novel());
}

#[test]
fn target_reference_updates_are_evidence_changes_without_repeated_novelty() {
    let mut c = live("a", &[]);
    c.footprint.target_check.state = TargetState::Conflict;
    c.target_conflicts = Some(vec![PathKey::new(b"a")]);
    let mut store = Store::new("test");
    store.update(project(vec![c.clone()]));
    act(&mut store, Kind::TargetConflict, Mutation::Acknowledge);
    let version = store.snapshot.version;
    c.footprint.head_oid = Some("advanced".into());
    c.footprint.target_check.head_oid = Some("advanced".into());
    store.update(project(vec![c.clone()]));
    assert!(store.snapshot.version > version);
    assert_eq!(
        finding(&store, Kind::TargetConflict).attention.status,
        Status::Known
    );
    c.target_conflicts
        .as_mut()
        .unwrap()
        .push(PathKey::new(b"new-path"));
    store.update(project(vec![c.clone()]));
    assert!(finding(&store, Kind::TargetConflict).attention.novel());
    act(&mut store, Kind::TargetConflict, Mutation::Acknowledge);
    c.footprint.base_ref = Some("release".into());
    store.update(project(vec![c]));
    assert!(store.snapshot.projects[0]
        .meanings
        .iter()
        .any(|f| f.kind == Kind::TargetConflict
            && f.parameters.target_ref.as_deref() == Some("release")
            && f.attention.novel()));
}

#[test]
fn pathless_conflict_is_preserved_but_malformed_output_is_not_a_conflict() {
    let bytes = format!("{}\0\0", "a".repeat(40));
    assert_eq!(conflict_paths(bytes.as_bytes()).unwrap(), None);
    assert!(conflict_paths(b"fatal conflict\0").is_err());
    let mut c = live("a", &[]);
    c.footprint.target_check.state = TargetState::Conflict;
    c.target_conflicts = None;
    let mut store = Store::new("test");
    store.update(project(vec![c]));
    let f = finding(&store, Kind::TargetConflict);
    assert_eq!(f.parameters.affected_path_count, None);
    let legacy = &store.snapshot.projects[0].findings;
    assert!(legacy
        .iter()
        .any(|reason| reason.contains("affected paths unavailable")));
    assert!(!legacy
        .iter()
        .any(|reason| reason.contains("0 committed conflict paths")));
    assert_eq!(f.headline, "Git's merge check reports a conflict.");
    assert!(f
        .limitations
        .iter()
        .any(|l| l.contains("Affected-file details")));
    assert_eq!(f.severity, Some(Severity::Alert));
}

#[test]
fn identical_rescans_and_reading_do_not_acknowledge_or_move_the_cursor() {
    let mut c = shared();
    let mut store = Store::new("test");
    store.update(project(vec![c.clone()]));
    let version = store.snapshot.version;
    c.footprint.observed_at_ms = unix_ms();
    c.footprint.duration_ms = Some(123);
    store.update(project(vec![c]));
    assert_eq!(store.snapshot.version, version);
    for _ in 0..3 {
        query(&store, "coordination_overview", &serde_json::json!({})).unwrap();
        agent_context(&store, &Depth::Project("p".into()), true).unwrap();
    }
    assert!(finding(&store, Kind::SharedCheckout).attention.novel());
    assert_eq!(store.memory.document.revision, 0);
    let facts = store.snapshot.fact_version;
    act(
        &mut store,
        Kind::SharedCheckout,
        Mutation::Expect(3_600_000),
    );
    assert_eq!(store.snapshot.fact_version, facts);
    assert!(store.snapshot.version > version);
    assert_eq!(store.changes.back().unwrap().kind, "attention_changed");
    assert!(query(
        &store,
        "coordination_set_expected",
        &serde_json::json!({"actor":"human"})
    )
    .is_err());
}

#[test]
fn expiry_revocation_branch_and_generation_changes_never_transfer_expectedness() {
    let c = shared();
    let p = project(vec![c.clone()]);
    let now = unix_ms().unwrap();
    let f = meaning::explain(&p, now).remove(0);
    let mut memory = Memory::default();
    memory
        .mutate(
            "p",
            &f,
            "epoch",
            now,
            0,
            "expect",
            Mutation::Expect(900_000),
        )
        .unwrap();
    assert_eq!(
        memory.present("p", &f, "epoch", now + 1).status,
        Status::Expected
    );
    for (epoch, time, state) in [
        ("other", now, "needs_reconfirmation"),
        ("epoch", now + 900_000, "expired"),
    ] {
        let shown = memory.present("p", &f, epoch, time);
        assert_ne!(shown.status, Status::Expected);
        assert_eq!(shown.expectation_state.as_deref(), Some(state));
    }
    for branch_change in [true, false] {
        let mut c = c.clone();
        if branch_change {
            c.branch = Some("other".into())
        } else {
            c.footprint.checkout_generation = Some("replacement".into())
        };
        let f = meaning::explain(&project(vec![c]), now).remove(0);
        assert_eq!(
            memory
                .present("p", &f, "epoch", now)
                .expectation_state
                .as_deref(),
            Some("scope_exceeded")
        );
    }
    memory
        .mutate("p", &f, "epoch", now, 1, "revoke", Mutation::Revoke)
        .unwrap();
    assert_eq!(
        memory
            .present("p", &f, "epoch", now)
            .expectation_state
            .as_deref(),
        Some("revoked")
    );
    memory
        .mutate(
            "p",
            &f,
            "epoch",
            now,
            2,
            "seen-revocation",
            Mutation::Acknowledge,
        )
        .unwrap();
    assert_eq!(memory.present("p", &f, "epoch", now).status, Status::Known);
}

#[test]
fn durable_cas_idempotency_corrupt_restore_and_save_failure_preserve_observations() {
    let repo = Repo::new();
    let path = repo.root.join("metadata/attention.json");
    let p = project(vec![shared()]);
    let now = unix_ms().unwrap();
    let f = meaning::explain(&p, now).remove(0);
    let mut a = Memory::load(path.clone());
    let mut b = Memory::load(path.clone());
    a.mutate("p", &f, "e", now, 0, "one", Mutation::Expect(3_600_000))
        .unwrap();
    a.mutate("p", &f, "e", now, 0, "one", Mutation::Expect(3_600_000))
        .unwrap();
    assert_eq!(a.document.revision, 1);
    assert!(b
        .mutate("p", &f, "e", now, 0, "two", Mutation::Acknowledge)
        .is_err());
    b.reload();
    assert_eq!(b.document.revision, 1);
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    std::fs::write(&path, "{broken").unwrap();
    b.reload();
    assert!(b.error.is_some());
    assert!(b.document.records.is_empty());
    assert!(b
        .mutate("p", &f, "e", now, 0, "three", Mutation::Acknowledge)
        .is_err());
    let invalid = repo.root.join("not-a-directory");
    std::fs::write(&invalid, "file").unwrap();
    let mut failed = Memory::load(invalid.join("attention.json"));
    assert!(failed
        .mutate("p", &f, "e", now, 0, "failure", Mutation::Expect(3_600_000))
        .is_err());
    assert_eq!(failed.present("p", &f, "e", now).status, Status::New);
}

#[test]
fn restricted_projection_has_no_private_intent_revision_or_hidden_change_signal() {
    let mut visible = shared();
    for p in &mut visible.participants {
        p.full_scope = false;
    }
    let mut hidden = live("hidden", &[(b"private.rs", Source::Unstaged)]);
    for p in &mut hidden.participants {
        p.exposed = false;
    }
    let mut facts = Facts {
        contexts: vec![visible, hidden],
        ..Facts::default()
    };
    let mut store = Store::new("restricted");
    store.update(derive("p", "Project", &facts, true).unwrap());
    let before = query(&store, "coordination_overview", &serde_json::json!({})).unwrap();
    facts.contexts[1]
        .participants
        .push(actor("secret", Some("secret-session"), false));
    facts.contexts[1].root = "private-renamed".into();
    store.update(derive("p", "Project", &facts, true).unwrap());
    let after = query(&store, "coordination_overview", &serde_json::json!({})).unwrap();
    assert_eq!(before["version"], after["version"]);
    assert_eq!(before["attention_revision"], after["attention_revision"]);
    assert_eq!(
        before["data"]["projects"]["items"][0]["meanings"],
        after["data"]["projects"]["items"][0]["meanings"]
    );
    assert_eq!(after["attention_audience"], "independent_reader");
    assert!(!after.to_string().contains("secret"));
}

#[test]
fn forty_worktrees_use_one_shared_file_finding_and_bounded_context() {
    let contexts = (0..40)
        .map(|i| live(&format!("c{i}"), &[(b"common.rs", Source::Unstaged)]))
        .collect();
    let mut store = Store::new("test");
    store.update(project(contexts));
    assert_eq!(
        store.snapshot.projects[0]
            .meanings
            .iter()
            .filter(|f| f.kind == Kind::SharedFile)
            .count(),
        1
    );
    let value = query(
        &store,
        "coordination_project",
        &serde_json::json!({"project_id":"p"}),
    )
    .unwrap();
    assert_eq!(
        value["data"]["project"]["meanings"][0]["parameters"]["checkout_count"],
        40
    );
}

#[test]
fn hud_uses_one_short_qualification_line_and_keeps_the_explanation_for_inspection() {
    let mut store = Store::new("hud");
    let mut p = project(vec![
        live("a", &[(b"relay.rs", Source::Committed)]),
        live("b", &[(b"relay.rs", Source::Unstaged)]),
        live("c", &[(b"relay.rs", Source::Unstaged)]),
    ]);
    p.coverage = Coverage::Partial;
    store.update(p);
    let f = finding(&store, Kind::SharedFile);
    assert_eq!(f.headline, "3 working copies include changes to relay.rs.");
    assert_eq!(
        f.glance_line(),
        "May need coordination · Conflict unconfirmed · Scan incomplete"
    );
    assert!(
        f.headline.split_whitespace().count() + f.glance_line().split_whitespace().count() <= 20
    );
    assert!(f.qualification().contains("shared branch history"));
    assert!(!f.glance_line().contains("acknowledgement"));
    let now = unix_ms().unwrap();
    let mut conflict = f.clone();
    conflict.kind = Kind::TargetConflict;
    assert!(Memory::default()
        .mutate(
            "p",
            &conflict,
            "hud",
            now,
            0,
            "forbidden",
            Mutation::Expect(3_600_000)
        )
        .is_err());
}
