use super::*;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn actor(id: &str, session: Option<&str>, exposed: bool) -> Participant {
    Participant {
        id: id.into(),
        project_id: "p".into(),
        pane_id: Some(id.bytes().map(u64::from).sum()),
        pid: Some(100),
        title: id.into(),
        mode: "CODEX".into(),
        agent: Some(true),
        session: session.map(str::to_string),
        exposed,
        full_scope: true,
    }
}
fn context(id: &str, paths: &[(&[u8], Source)]) -> Context {
    Context {
        id: id.into(),
        root: format!("/work/{id}"),
        repository: Some("same-origin".into()),
        branch: Some(format!("feature/{id}")),
        participants: vec![actor(id, Some(id), true)],
        footprint: Footprint {
            paths: paths
                .iter()
                .map(|(p, s)| PathChange {
                    path: PathKey::new(p),
                    sources: BTreeSet::from([*s]),
                })
                .collect(),
            sources: [
                Source::Committed,
                Source::Staged,
                Source::Unstaged,
                Source::Untracked,
            ]
            .into_iter()
            .map(|s| (s, Coverage::Complete))
            .collect(),
            common_dir: Some("clone-one".into()),
            head_oid: Some(format!("head-{id}")),
            base_ref: Some("main".into()),
            base_oid: Some("base".into()),
            target_oid: Some("target".into()),
            observed_at_ms: Some(1000),
            ..Footprint::default()
        },
        dirty: Some(0),
        untracked: Some(0),
        delta: Some((0, 0)),
        ahead_behind: Some((0, 0)),
        target_conflicts: Some(vec![]),
    }
}
fn project(contexts: Vec<Context>) -> Project {
    derive(
        "p",
        "Project",
        &Facts {
            contexts,
            ..Facts::default()
        },
        false,
    )
    .unwrap()
}

#[test]
fn four_checkouts_on_one_path_are_one_hotspot_not_six_alerts() {
    let p = project(
        (0..4)
            .map(|i| context(&i.to_string(), &[(b"shared.rs", Source::Unstaged)]))
            .collect(),
    );
    assert_eq!(p.hotspots.len(), 1);
    assert_eq!(p.clusters.len(), 1);
    assert_eq!(p.clusters[0].checkout_ids.len(), 4);
    assert_eq!(p.severity, Some(Severity::Warning));
}
#[test]
fn separate_files_are_clear_only_with_complete_required_coverage() {
    let mut a = context("a", &[(b"a", Source::Staged)]);
    let b = context("b", &[(b"b", Source::Untracked)]);
    assert_eq!(
        project(vec![a.clone(), b.clone()]).severity,
        Some(Severity::Clear)
    );
    a.footprint.sources.remove(&Source::Committed);
    let p = project(vec![a, b]);
    assert_eq!(p.severity, None);
    assert_eq!(p.coverage, Coverage::Partial);
    assert!(p.clusters.is_empty());
}
#[test]
fn a_known_session_in_two_panes_is_one_session_and_a_checkout_is_indexed_once() {
    let mut a = context("a", &[(b"x", Source::Unstaged)]);
    a.participants.push(actor("second", Some("a"), true));
    let p = project(vec![a, context("b", &[(b"x", Source::Staged)])]);
    assert_eq!(p.hotspots.len(), 2);
    assert_eq!(
        p.hotspots
            .iter()
            .find(|h| h.kind == "shared_file")
            .unwrap()
            .checkout_ids
            .len(),
        2
    );
    assert_eq!(p.session_count, Some(2));
    assert_eq!(p.pane_count, Some(3));
    assert_eq!(p.clusters[0].session_count, Some(2));
    assert_eq!(
        p.hotspots
            .iter()
            .find(|h| h.kind == "shared_checkout")
            .unwrap()
            .severity,
        Severity::Caution
    );
}
#[test]
fn unknown_agent_identity_does_not_become_zero_or_clear_shared_occupancy() {
    let mut a = context("a", &[]);
    a.participants[0].session = None;
    a.participants.push(actor("other", None, true));
    let p = project(vec![a]);
    assert_eq!(p.session_count, None);
    assert_eq!(p.severity, Some(Severity::Caution));
    assert_eq!(p.hotspots[0].participant_ids.len(), 2);
}
#[test]
fn overlapping_files_form_connected_groups_without_swallowing_independent_work() {
    let p = project(vec![
        context("a", &[(b"x", Source::Unstaged)]),
        context("b", &[(b"x", Source::Staged), (b"y", Source::Untracked)]),
        context("c", &[(b"y", Source::Unstaged)]),
        context("d", &[(b"z", Source::Unstaged)]),
    ]);
    assert_eq!(p.clusters.len(), 1);
    assert_eq!(p.clusters[0].checkout_ids, vec!["a", "b", "c"]);
    assert_eq!(p.hotspots.len(), 2);
}
#[test]
fn committed_only_overlap_is_qualified_for_stacked_ancestry() {
    let p = project(
        (0..4)
            .map(|i| context(&i.to_string(), &[(b"inherited", Source::Committed)]))
            .collect(),
    );
    assert_eq!(p.severity, Some(Severity::Caution));
    assert!(p.hotspots[0]
        .qualification
        .as_ref()
        .unwrap()
        .contains("inherited"));
}
#[test]
fn distinct_clones_of_one_origin_are_not_one_object_database() {
    let a = context("a", &[(b"x", Source::Staged)]);
    let mut b = context("b", &[(b"x", Source::Staged)]);
    b.footprint.common_dir = Some("another-clone".into());
    let p = project(vec![a, b]);
    assert!(p.hotspots.is_empty());
    assert_eq!(p.severity, Some(Severity::Clear));
}
#[test]
fn a_target_conflict_is_a_finding_without_an_invented_multi_party_cluster() {
    let mut a = context("a", &[]);
    a.target_conflicts = Some(vec![PathKey::new(b"conflicted")]);
    let p = project(vec![a]);
    assert_eq!(p.severity, Some(Severity::Alert));
    assert_eq!(p.findings.len(), 1);
    assert!(p.clusters.is_empty());
    assert!(p.findings[0].contains("main"));
}
#[test]
fn nul_paths_preserve_renames_tabs_newlines_unicode_and_non_utf8() {
    let mut paths = Changes::new();
    let mut renames = vec![];
    let mut unmerged = vec![];
    parse_status(
        b"R  new\tname\0old\nname\0?? \xfffile\0?? \xfefile\0?? \xce\xbb.txt\0",
        &mut paths,
        &mut renames,
        &mut unmerged,
    )
    .unwrap();
    assert_eq!(paths.len(), 5);
    assert_eq!(
        renames[0].from.bytes_hex,
        PathKey::new(b"old\nname").bytes_hex
    );
    assert_eq!(
        renames[0].to.bytes_hex,
        PathKey::new(b"new\tname").bytes_hex
    );
    assert_ne!(PathKey::new(b"\xfffile"), PathKey::new(b"\xfefile"));
    assert_eq!(PathKey::new("λ.txt".as_bytes()).display, "λ.txt");
    assert!(parse_status(b"?? truncated", &mut paths, &mut renames, &mut unmerged).is_err());
    assert!(parse_committed(b"R100\0old\0new\0", &mut paths, &mut renames).is_ok());
}
#[test]
fn publication_ids_and_deltas_ignore_hidden_only_paths_titles_and_participants() {
    let mut a = context("a", &[(b"public", Source::Staged)]);
    a.participants[0].full_scope = false;
    let mut hidden = context("hidden", &[(b"secret", Source::Staged)]);
    hidden.participants[0].exposed = false;
    let mut facts = Facts {
        contexts: vec![a.clone(), hidden.clone()],
        ..Facts::default()
    };
    let mut store = Store::new("authorized-instance");
    store.update(derive("p", "Project", &facts, true).unwrap());
    let before = serde_json::to_value(&store.snapshot).unwrap();
    let deltas = serde_json::to_value(&store.changes).unwrap();
    hidden.root = "/private/new-location".into();
    hidden.participants[0].title = "secret title".into();
    hidden.footprint.paths.push(PathChange {
        path: PathKey::new(b"another-secret"),
        sources: BTreeSet::from([Source::Unstaged]),
    });
    facts.contexts = vec![hidden.clone(), a.clone()];
    facts.truncated = true;
    store.update(derive("p", "Project", &facts, true).unwrap());
    assert_eq!(serde_json::to_value(&store.snapshot).unwrap(), before);
    assert_eq!(serde_json::to_value(&store.changes).unwrap(), deltas);
    // A denied writer moving into the permitted physical checkout still cannot
    // expose path changes or a hidden-person count through this projection.
    a.participants.push(hidden.participants[0].clone());
    a.footprint.paths = hidden.footprint.paths;
    facts.contexts = vec![a];
    store.update(derive("p", "Project", &facts, true).unwrap());
    assert_eq!(serde_json::to_value(&store.snapshot).unwrap(), before);
}
#[test]
fn a_hidden_project_cannot_be_named_through_a_permitted_visitor() {
    let mut c = context("root", &[]);
    c.participants[0].project_id = "other".into();
    assert!(derive(
        "secret-project",
        "Secret title",
        &Facts {
            contexts: vec![c],
            ..Facts::default()
        },
        true
    )
    .is_none());
}
#[test]
fn stable_cluster_ids_survive_growth_but_split_merge_get_new_ids_and_lineage() {
    let a = context("a", &[(b"x", Source::Staged)]);
    let b = context("b", &[(b"x", Source::Staged)]);
    let c = context("c", &[(b"y", Source::Staged)]);
    let d = context("d", &[(b"y", Source::Staged)]);
    let mut store = Store::new("one");
    store.update(project(vec![a.clone(), b.clone(), c.clone(), d.clone()]));
    let old: Vec<_> = store.snapshot.projects[0]
        .clusters
        .iter()
        .map(|c| c.id.clone())
        .collect();
    let bridge = context("bridge", &[(b"x", Source::Staged), (b"y", Source::Staged)]);
    store.update(project(vec![
        a.clone(),
        b.clone(),
        c.clone(),
        d.clone(),
        bridge,
    ]));
    let joined = &store.snapshot.projects[0].clusters[0];
    assert_eq!(joined.supersedes.len(), 2);
    assert!(!old.contains(&joined.id));
    let joined_id = joined.id.clone();
    store.update(project(vec![a.clone(), b.clone(), c, d]));
    assert!(store.snapshot.projects[0]
        .clusters
        .iter()
        .all(|c| c.supersedes == vec![joined_id.clone()] && !old.contains(&c.id)));
    let retained = store.snapshot.projects[0]
        .clusters
        .iter()
        .find(|c| c.checkout_ids.contains(&"a".into()))
        .unwrap()
        .id
        .clone();
    store.update(project(vec![
        a,
        b,
        context("grow", &[(b"x", Source::Staged)]),
    ]));
    assert_eq!(store.snapshot.projects[0].clusters[0].id, retained);
}
#[test]
fn scan_times_do_not_create_fact_revisions_and_history_is_bounded() {
    let mut p = project(vec![context("a", &[])]);
    let mut store = Store::new("epoch");
    store.update(p.clone());
    let version = store.snapshot.version;
    p.observed_at_ms = Some(2000);
    p.contexts[0].footprint.duration_ms = Some(999);
    p.contexts[0].footprint.observed_at_ms = Some(2000);
    store.update(p.clone());
    assert_eq!(store.snapshot.version, version);
    for i in 0..100 {
        p.name = format!("P{i}");
        store.update(p.clone());
    }
    assert_eq!(store.changes.len(), HISTORY);
}
#[test]
fn severity_order_puts_unknown_ahead_of_clear_and_ignores_selection() {
    let mut window = Snapshot::empty("cold");
    window
        .projects
        .push(Project::pending("unread".into(), "Unread".into()));
    assert_eq!(window.coverage(), Coverage::Unavailable);
    window.projects.push(project(vec![context("clear", &[])]));
    assert_eq!(
        window.severity(),
        None,
        "a clear selected project cannot turn unread projects into a clear window"
    );
    window.projects[0].severity = Some(Severity::Alert);
    assert_eq!(
        window.severity(),
        Some(Severity::Alert),
        "positive attention remains visible beside incomplete observations"
    );
    let mut snapshot = Snapshot::empty("x");
    for (name, severity) in [
        ("clear", Some(Severity::Clear)),
        ("unknown", None),
        ("caution", Some(Severity::Caution)),
        ("warning", Some(Severity::Warning)),
        ("alert", Some(Severity::Alert)),
    ] {
        let mut p = project(vec![context(name, &[])]);
        p.name = name.into();
        p.id = name.into();
        p.severity = severity;
        snapshot.projects.push(p);
    }
    snapshot.rank();
    assert_eq!(
        snapshot
            .projects
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        vec!["alert", "warning", "caution", "unknown", "clear"]
    );
}
#[test]
fn sparse_and_dense_graphs_scale_with_paths_and_keep_all_hotspots() {
    let began = Instant::now();
    let sparse = project(
        (0..100)
            .map(|i| {
                let mut c = context(&format!("c{i}"), &[]);
                c.footprint.paths = (0..100)
                    .map(|j| PathChange {
                        path: PathKey::new(format!("{i}/{j}").as_bytes()),
                        sources: BTreeSet::from([Source::Unstaged]),
                    })
                    .collect();
                c
            })
            .collect(),
    );
    assert!(sparse.hotspots.is_empty());
    let dense = project(
        (0..40)
            .map(|i| {
                context(
                    &format!("c{i}"),
                    &[
                        (b"common", Source::Staged),
                        (format!("group-{}", i / 4).as_bytes(), Source::Unstaged),
                    ],
                )
            })
            .collect(),
    );
    assert_eq!(dense.clusters.len(), 1);
    assert_eq!(dense.hotspots.len(), 11);
    assert_eq!(dense.clusters[0].checkout_ids.len(), 40);
    assert!(began.elapsed() < Duration::from_secs(5));
}
#[test]
fn navigation_returns_to_each_origin_and_then_closes() {
    let mut v = View {
        selected: 3,
        ..Default::default()
    };
    v.enter(Depth::Project("p".into()));
    v.selected = 2;
    v.enter(Depth::Cluster("p".into(), "c".into()));
    assert!(v.back());
    assert_eq!(v.selected, 2);
    assert!(matches!(v.depth, Depth::Project(_)));
    assert!(v.back());
    assert_eq!(v.selected, 3);
    assert_eq!(v.depth, Depth::Overview);
    assert!(!v.back());
    assert!(v.forward());
    assert_eq!(v.selected, 2);
    assert!(matches!(v.depth, Depth::Project(_)));
    assert!(v.forward());
    assert!(matches!(v.depth, Depth::Cluster(_, _)));
    assert!(!v.forward());
    assert!(v.back());
    v.enter(Depth::Checkouts("p".into()));
    assert!(!v.forward(), "a new navigation clears forward history");
}
#[test]
fn breadcrumb_jumps_are_visits_in_mouse_back_and_forward_history() {
    let mut view = View::default();
    view.enter(Depth::Project("p".into()));
    view.enter(Depth::Cluster("p".into(), "c".into()));

    // A project crumb is a direct target even when a group was opened from
    // the overview preview rather than from the project screen.
    view.enter(Depth::Project("p".into()));
    assert_eq!(view.depth, Depth::Project("p".into()));
    assert!(view.back());
    assert_eq!(view.depth, Depth::Cluster("p".into(), "c".into()));
    assert!(view.forward());
    assert_eq!(view.depth, Depth::Project("p".into()));

    view.enter(Depth::Overview);
    assert_eq!(view.depth, Depth::Overview);
    assert!(view.back());
    assert_eq!(view.depth, Depth::Project("p".into()));
    assert!(view.forward());
    assert_eq!(view.depth, Depth::Overview);
}
#[test]
fn launch_recipes_are_not_fabricated_session_ids() {
    assert!(session_identity(Some("codex resume --last")).is_none());
    assert!(session_identity(Some("claude")).is_none());
    assert!(session_identity(Some("codex resume 01a12322-c2ad-73f3-95ec-b16db63510e7")).is_some());
}

struct Repo {
    root: PathBuf,
}
impl Repo {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "td-coordination-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
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
    fn commit(&self) {
        self.git(&["add", "--all"]);
        self.git(&["commit", "-qm", "fixture"]);
    }
    fn checkout(&self) -> crate::engstate::Checkout {
        crate::engstate::Checkout {
            root: self.root.clone(),
            repo: "fixture".into(),
            branch: Some("feature".into()),
            head: None,
            dirty: None,
            untracked: None,
            delta: None,
            ahead_behind: None,
            last_commit: None,
            writers: vec![],
            upstream: None,
            unpushed: None,
            touched: None,
            merge: None,
        }
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
#[test]
fn real_git_collects_all_four_sources_and_rename_links_losslessly() {
    use std::os::unix::ffi::OsStringExt;
    let repo = Repo::new();
    std::fs::write(repo.root.join("base"), "base\n").unwrap();
    std::fs::write(repo.root.join("rename-from"), "rename\n").unwrap();
    repo.commit();
    repo.git(&["checkout", "-qb", "feature"]);
    std::fs::write(repo.root.join("committed"), "committed\n").unwrap();
    repo.commit();
    repo.git(&["mv", "rename-from", "rename-to"]);
    std::fs::write(repo.root.join("staged"), "staged\n").unwrap();
    repo.git(&["add", "staged"]);
    std::fs::write(repo.root.join("base"), "unstaged\n").unwrap();
    for bytes in [
        b"untracked\tfile".as_slice(),
        b"untracked\nfile",
        b"\xff-untracked",
    ] {
        std::fs::write(
            repo.root.join(std::ffi::OsString::from_vec(bytes.to_vec())),
            "new\n",
        )
        .unwrap();
    }
    let result = measure(&repo.checkout(), Some("main"));
    assert_eq!(
        result.footprint.coverage(),
        Coverage::Complete,
        "{:?}",
        result.footprint.errors
    );
    for (name, source) in [
        (b"committed".as_slice(), Source::Committed),
        (b"staged", Source::Staged),
        (b"base", Source::Unstaged),
        (b"\xff-untracked", Source::Untracked),
    ] {
        assert!(
            result
                .footprint
                .paths
                .iter()
                .any(|p| p.path == PathKey::new(name) && p.sources.contains(&source)),
            "missing {name:?}"
        );
    }
    assert!(result
        .footprint
        .renames
        .iter()
        .any(|r| r.from.display == "rename-from" && r.to.display == "rename-to"));
}
#[test]
fn unborn_head_and_missing_base_preserve_available_index_evidence() {
    let repo = Repo::new();
    std::fs::write(repo.root.join("staged"), "new\n").unwrap();
    repo.git(&["add", "staged"]);
    let c = measure(&repo.checkout(), None);
    assert_eq!(c.footprint.head_oid, None);
    assert_eq!(c.footprint.coverage(), Coverage::Partial);
    assert!(c
        .footprint
        .paths
        .iter()
        .any(|p| p.sources.contains(&Source::Staged)));
}
#[test]
fn a_missing_worktree_is_unavailable_and_never_a_clean_zero() {
    let repo = Repo::new();
    let mut c = repo.checkout();
    c.root = repo.root.join("gone");
    let c = measure(&c, Some("main"));
    assert_eq!(c.footprint.coverage(), Coverage::Unavailable);
    assert!(!c.footprint.errors.is_empty());
}
#[test]
fn oversized_git_output_is_drained_and_rejected_and_exit_errors_are_unknown() {
    let repo = Repo::new();
    std::fs::write(repo.root.join("long-name"), "x").unwrap();
    let began = Instant::now();
    let result = crate::engstate::git_bytes(
        &repo.root,
        &["status", "--porcelain=v1", "-z"],
        Duration::from_secs(2),
        4,
    );
    assert!(result.unwrap_err().contains("limit"));
    assert!(began.elapsed() < Duration::from_secs(2));
    assert!(crate::engstate::git_bytes(
        Path::new("/nonexistent-coordination-repo"),
        &["status"],
        Duration::from_millis(50),
        100
    )
    .is_err());
}

#[test]
fn cursors_bind_scope_epoch_version_tool_and_relationship_and_changes_reset_explicitly() {
    use serde_json::json;
    let mut store = Store::new("scope-a");
    let mut p = project(vec![
        context("a", &[(b"x", Source::Staged), (b"y", Source::Staged)]),
        context("b", &[(b"x", Source::Unstaged), (b"y", Source::Unstaged)]),
    ]);
    store.update(p.clone());
    let group = store.snapshot.projects[0].clusters[0].id.clone();
    let first = query(
        &store,
        "coordination_cluster",
        &json!({"project_id":"p","cluster_id":group,"limit":1}),
    )
    .unwrap();
    assert_eq!(
        first["data"]["evidence"]["items"].as_array().unwrap().len(),
        1
    );
    let cursor = first["data"]["evidence"]["next_cursor"].clone();
    assert!(query(
        &store,
        "coordination_cluster",
        &json!({"project_id":"p","cluster_id":group,"limit":1,"cursor":cursor})
    )
    .is_ok());
    assert!(query(
        &store,
        "coordination_project",
        &json!({"project_id":"p","cursor":cursor})
    )
    .is_err());
    assert!(query(
        &store,
        "coordination_cluster",
        &json!({"project_id":"other","cluster_id":group,"cursor":cursor})
    )
    .is_err());
    let before = store.snapshot.version;
    p.name = "Changed".into();
    store.update(p.clone());
    assert!(query(
        &store,
        "coordination_cluster",
        &json!({"project_id":"p","cluster_id":group,"cursor":cursor})
    )
    .is_err());
    let delta = query(
        &store,
        "coordination_changes",
        &json!({"epoch":"scope-a","since_version":before}),
    )
    .unwrap();
    assert_eq!(delta["data"]["requires_full_snapshot"], false);
    assert_eq!(delta["data"]["changes"].as_array().unwrap().len(), 1);
    assert_eq!(
        query(
            &store,
            "coordination_changes",
            &json!({"epoch":"another-scope","since_version":before})
        )
        .unwrap()["data"]["requires_full_snapshot"],
        true
    );
    for i in 0..40 {
        p.name = i.to_string();
        store.update(p.clone());
    }
    assert_eq!(
        query(
            &store,
            "coordination_changes",
            &json!({"epoch":"scope-a","since_version":0})
        )
        .unwrap()["data"]["requires_full_snapshot"],
        true
    );
    for limit in [json!(0), json!(101), json!(-1), json!("2"), json!(1.5)] {
        assert!(query(&store, "coordination_overview", &json!({"limit":limit})).is_err());
    }
}

#[test]
fn overview_gives_an_agent_the_cause_coverage_and_exact_next_read() {
    use serde_json::json;
    let mut store = Store::new("one");
    store.update(project(vec![
        context("a", &[(b"relay.rs", Source::Staged)]),
        context("b", &[(b"relay.rs", Source::Unstaged)]),
    ]));
    let value = query(&store, "coordination_overview", &json!({})).unwrap();
    let p = &value["data"]["projects"]["items"][0];
    assert!(p["causes"][0]["reason"]
        .as_str()
        .unwrap()
        .contains("relay.rs"));
    assert_eq!(p["coverage"], "complete");
    let read = &p["groups"][0]["read"];
    let evidence = query(&store, read["tool"].as_str().unwrap(), &read["arguments"]).unwrap();
    let rows = evidence["data"]["checkouts"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows
        .iter()
        .all(|r| r["branch"].is_string() && r["participants"][0]["session"].is_string()));
    let relations = evidence["data"]["evidence"]["items"][0]["relationships"]
        .as_array()
        .unwrap();
    assert_eq!(relations.len(), 2);
    assert!(relations
        .iter()
        .all(|r| r["sources"].as_array().unwrap().len() == 1));
}

#[test]
fn merge_lineage_is_stable_on_the_next_unchanged_scan() {
    let a = context("a", &[(b"x", Source::Staged)]);
    let b = context("b", &[(b"x", Source::Staged)]);
    let c = context("c", &[(b"y", Source::Staged)]);
    let d = context("d", &[(b"y", Source::Staged)]);
    let mut store = Store::new("one");
    store.update(project(vec![a.clone(), b.clone(), c.clone(), d.clone()]));
    let p = project(vec![
        a,
        b,
        c,
        d,
        context("bridge", &[(b"x", Source::Staged), (b"y", Source::Staged)]),
    ]);
    store.update(p.clone());
    let version = store.snapshot.version;
    let lineage = store.snapshot.projects[0].clusters[0].supersedes.clone();
    store.update(p);
    assert_eq!(store.snapshot.version, version);
    assert_eq!(store.snapshot.projects[0].clusters[0].supersedes, lineage);
}

#[test]
fn real_committed_conflict_names_its_exact_target_and_head_without_touching_the_checkout() {
    let repo = Repo::new();
    std::fs::write(repo.root.join("relay.rs"), "base\n").unwrap();
    repo.commit();
    repo.git(&["checkout", "-qb", "feature"]);
    std::fs::write(repo.root.join("relay.rs"), "feature\n").unwrap();
    repo.commit();
    repo.git(&["checkout", "main"]);
    std::fs::write(repo.root.join("relay.rs"), "main\n").unwrap();
    repo.commit();
    repo.git(&["checkout", "feature"]);
    let before = crate::engstate::git_bytes(
        &repo.root,
        &["status", "--porcelain=v1", "-z"],
        Duration::from_secs(2),
        1024,
    )
    .unwrap();
    let c = measure(&repo.checkout(), Some("main"));
    assert_eq!(c.footprint.target_check.state, TargetState::Conflict);
    assert_eq!(c.footprint.target_check.head_oid, c.footprint.head_oid);
    assert_eq!(c.footprint.target_check.target_oid, c.footprint.target_oid);
    assert_eq!(c.target_conflicts.unwrap()[0].display, "relay.rs");
    assert_eq!(
        crate::engstate::git_bytes(
            &repo.root,
            &["status", "--porcelain=v1", "-z"],
            Duration::from_secs(2),
            1024
        )
        .unwrap(),
        before
    );
}

#[test]
fn a_git_helper_holding_stdout_cannot_hold_the_scan_past_its_deadline() {
    let repo = Repo::new();
    let start = Instant::now();
    let result = crate::engstate::git_bytes(
        &repo.root,
        &[
            "-c",
            "alias.coordination-slow=!sleep 5",
            "coordination-slow",
        ],
        Duration::from_millis(80),
        1024,
    );
    assert!(result.unwrap_err().contains("deadline"));
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn checkout_tabs_do_not_add_a_semantic_depth_or_lose_the_origin() {
    let mut view = View {
        selected: 4,
        ..Default::default()
    };
    view.enter(Depth::Project("p".into()));
    view.enter(Depth::Checkouts("p".into()));
    view.enter(Depth::Project("p".into()));
    assert_eq!(view.history.len(), 1);
    assert!(view.back());
    assert_eq!(view.depth, Depth::Overview);
    assert_eq!(view.selected, 4);
}

#[test]
fn the_real_scanner_connects_permitted_worktrees_filed_under_different_projects() {
    let repo = Repo::new();
    std::fs::write(repo.root.join("relay.rs"), "base\n").unwrap();
    repo.commit();
    let other = Repo {
        root: repo.root.join("other-checkout"),
    };
    repo.git(&[
        "worktree",
        "add",
        "-qb",
        "feature",
        other.root.to_str().unwrap(),
    ]);
    std::fs::write(repo.root.join("relay.rs"), "main edit\n").unwrap();
    std::fs::write(other.root.join("relay.rs"), "feature edit\n").unwrap();
    other.git(&["add", "relay.rs"]);
    let a = actor("a", Some("session-a"), true);
    let mut b = actor("b", Some("session-b"), true);
    b.project_id = "q".into();
    let writer = |p: Participant, cwd: PathBuf| crate::engstate::Writer {
        participant: Some(p.clone()),
        tab: 0,
        tab_name: p.title,
        label: "CODEX".into(),
        cwd,
    };
    let input = crate::engstate::ScanInput {
        coordination_key: Some("p".into()),
        scope_key: "full".into(),
        name: Some("Project".into()),
        mine: vec![writer(a, repo.root.clone())],
        others: vec![(Some("Other project".into()), writer(b, other.root.clone()))],
        ..Default::default()
    };
    let reading = crate::engstate::scan(&input).coordination.unwrap();
    assert_eq!(reading.operator.clusters.len(), 1);
    assert_eq!(reading.operator.clusters[0].checkout_ids.len(), 2);
    assert!(reading
        .operator
        .contexts
        .iter()
        .any(|c| c.branch.as_deref() == Some("feature")));
    assert_eq!(reading.authorized.unwrap(), reading.operator);
}

#[test]
fn a_shared_checkout_exposes_its_source_labelled_files_in_bounded_evidence_pages() {
    use serde_json::json;
    let mut c = context(
        "a",
        &[(b"one", Source::Untracked), (b"two", Source::Staged)],
    );
    c.participants.push(actor("b", Some("b"), true));
    let mut store = Store::new("one");
    store.update(project(vec![c]));
    let id = &store.snapshot.projects[0].clusters[0].id;
    let first = query(
        &store,
        "coordination_cluster",
        &json!({"project_id":"p","cluster_id":id,"limit":1}),
    )
    .unwrap();
    assert_eq!(first["data"]["evidence"]["total"], 3);
    assert_eq!(
        first["data"]["checkouts"][0]["participants"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let next=query(&store,"coordination_cluster",&json!({"project_id":"p","cluster_id":id,"limit":1,"cursor":first["data"]["evidence"]["next_cursor"]})).unwrap();
    assert_eq!(
        next["data"]["evidence"]["items"][0]["path"]["display"],
        "one"
    );
    assert_eq!(
        next["data"]["evidence"]["items"][0]["relationships"][0]["sources"][0],
        "untracked"
    );
}

#[test]
fn a_detached_helper_retaining_stdout_is_bounded_too() {
    let repo = Repo::new();
    let began = Instant::now();
    let out = crate::engstate::git_bytes(
        &repo.root,
        &[
            "-c",
            "alias.coordination-detached=!setsid sh -c 'sleep 1' &",
            "coordination-detached",
        ],
        Duration::from_millis(80),
        1024,
    );
    assert!(out.is_err());
    assert!(began.elapsed() < Duration::from_millis(700));
}

#[test]
fn copying_human_only_evidence_for_an_agent_cannot_bypass_exposure_or_reuse_private_handles() {
    let mut context = context("allowed", &[(b"private-never-copy.rs", Source::Staged)]);
    context.participants[0].full_scope = false;
    let facts = Facts {
        contexts: vec![context],
        ..Default::default()
    };
    let p = derive("p", "Allowed project", &facts, true).unwrap();
    let mut store = Store::new("agent-scope");
    store.update(p);
    let depth = Depth::Cluster("p".into(), "private-operator-handle".into());
    let value = agent_context(&store, &depth, true).unwrap();
    let text = value.to_string();
    assert_eq!(value["request"]["tool"], "coordination_project");
    assert!(!text.contains("private-never-copy"));
    assert!(!text.contains("private-operator-handle"));
    assert!(text.contains("agent-scope"));
    assert!(agent_context(&store, &depth, false).is_err());
    let reset = Store::new("new-policy-epoch");
    let change = query(
        &reset,
        "coordination_changes",
        &serde_json::json!({"epoch":"agent-scope","since_version":1}),
    )
    .unwrap();
    assert_eq!(change["data"]["requires_full_snapshot"], true);
    assert_eq!(change["data"]["changes"].as_array().unwrap().len(), 0);
}

#[test]
fn unresolved_index_paths_do_not_claim_a_target_and_isolated_alerts_have_readable_evidence() {
    use serde_json::json;
    let mut c = context(
        "a",
        &[
            (b"index.rs", Source::Staged),
            (b"other.rs", Source::Committed),
        ],
    );
    c.footprint.unmerged = vec![PathKey::new(b"index.rs")];
    let mut p = project(vec![c]);
    assert_eq!(p.severity, Some(Severity::Alert));
    assert!(p.clusters.is_empty());
    assert_eq!(p.hotspots[0].kind, "unmerged_index");
    assert!(!p.hotspots[0].reason.contains("main"));
    let mut store = Store::new("one");
    store.update(p.clone());
    let view = query(&store, "coordination_project", &json!({"project_id":"p"})).unwrap();
    let read = &view["data"]["checkouts"][0]["read"];
    let detail = query(&store, read["tool"].as_str().unwrap(), &read["arguments"]).unwrap();
    assert_eq!(detail["data"]["changes"]["items"][0]["unmerged"], true);
    assert_eq!(detail["data"]["checkout"]["target_oid"], "target");
    p.contexts[0].footprint.sources.clear();
    store.update(p);
    let unknown = query(
        &store,
        "coordination_project",
        &json!({"project_id":"p","checkout_id":"a"}),
    )
    .unwrap();
    assert_eq!(unknown["data"]["changes"]["items"][0]["unmerged"], true);
    assert_eq!(
        unknown["data"]["changes"]["items"][1]["unmerged"],
        serde_json::Value::Null
    );
    assert_eq!(
        unknown["data"]["checkout"]["unmerged_count"],
        serde_json::Value::Null
    );
}

#[test]
fn checkout_evidence_cursors_cannot_be_reused_for_another_checkout() {
    use serde_json::json;
    let mut store = Store::new("one");
    store.update(project(vec![
        context(
            "a",
            &[(b"one", Source::Staged), (b"two", Source::Untracked)],
        ),
        context("b", &[(b"three", Source::Staged)]),
    ]));
    let page = query(
        &store,
        "coordination_project",
        &json!({"project_id":"p","checkout_id":"a","limit":1}),
    )
    .unwrap();
    let cursor = &page["data"]["changes"]["next_cursor"];
    assert!(cursor.is_string());
    assert!(query(
        &store,
        "coordination_project",
        &json!({"project_id":"p","checkout_id":"b","cursor":cursor})
    )
    .is_err());
    let next = query(
        &store,
        "coordination_project",
        &json!({"project_id":"p","checkout_id":"a","cursor":cursor,"limit":1}),
    )
    .unwrap();
    assert_eq!(
        next["data"]["changes"]["items"][0]["path"]["display"],
        "two"
    );
}
