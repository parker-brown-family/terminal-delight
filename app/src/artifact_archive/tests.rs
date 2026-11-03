use super::*;
use crate::handover::Source;
use crate::vitals::Bond;

fn rec(href: &str, time: u64) -> Rec {
    let h = Handover {
        label: "A <decision> & review".into(),
        href: href.into(),
        at_ms: time,
        source: Source::Declared,
    };
    benchstore::said(
        0,
        "review",
        &handover::surface_doc(&h),
        Bond::Declared,
        time,
    )
    .unwrap()
}

fn keep(store: &Path, root: &str, event: &Rec, href: &str) -> Observation {
    benchstore::append(store, root, event).unwrap();
    let observation = capture(store, href);
    benchstore::append(
        store,
        root,
        &Rec::ArtifactRevision {
            at_ms: 50,
            said: event.identity(),
            said_at_ms: match event {
                Rec::Said { at_ms, .. } => *at_ms,
                _ => unreachable!(),
            },
            observation: observation.clone(),
        },
    )
    .unwrap();
    observation
}

#[test]
fn closed_conversations_keep_each_revision_after_the_original_file_is_gone() {
    let scratch = crate::testsync::Scratch::new("artifact-revisions");
    let store = scratch.path().join("conversations");
    let source = scratch.path().join("decision.md");
    fs::write(&source, "# First decision\n").unwrap();
    let href = file_url(&source);
    let first = keep(&store, "conversation-a", &rec(&href, 10), &href);
    fs::write(&source, "# Revised decision\n").unwrap();
    let second = keep(&store, "conversation-a", &rec(&href, 20), &href);
    assert_ne!(first.revision, second.revision);
    assert_eq!(artifact_id(&href), artifact_id(source.to_str().unwrap()));
    fs::remove_file(&source).unwrap();
    let archive = load(&store);
    assert!(archive.problems.is_empty(), "{:?}", archive.problems);
    assert_eq!(archive.entries.len(), 2);
    let saved = snapshot_path(&store, first.snapshot.as_ref().unwrap()).unwrap();
    assert_eq!(fs::read_to_string(saved).unwrap(), "# First decision\n");
    let page = fs::read_to_string(write_page(&store).unwrap()).unwrap();
    assert!(page.contains("Current file unavailable."));
    assert!(page.contains("A &lt;decision&gt; &amp; review"));
    assert!(!page.contains("<decision>"));
}

#[test]
fn repeat_handover_of_the_same_bytes_has_one_revision_and_legacy_evidence_stays_unknown() {
    let scratch = crate::testsync::Scratch::new("artifact-repeat");
    let store = scratch.path().join("conversations");
    let source = scratch.path().join("decision.md");
    fs::write(&source, "").unwrap();
    let href = file_url(&source);
    keep(&store, "conversation-a", &rec(&href, 10), &href);
    let empty = keep(&store, "conversation-a", &rec(&href, 20), &href);
    assert_eq!(empty.bytes, Some(0));
    assert_eq!(load(&store).entries.len(), 1);
    benchstore::append(&store, "legacy", &rec(&href, 1)).unwrap();
    let legacy = load(&store)
        .entries
        .into_iter()
        .find(|e| e.origin.as_deref() == Some("legacy"))
        .unwrap();
    assert!(legacy.observation.is_none());
}

#[test]
fn unbound_artifacts_survive_reload_without_inventing_a_conversation() {
    let scratch = crate::testsync::Scratch::new("artifact-unbound");
    let store = scratch.path().join("conversations");
    let event = rec("https://example.com/decision", 10);
    file_unbound(&store, "session 1 pane 2", &event).unwrap();
    file_unbound(&store, "session 1 pane 2", &event).unwrap();
    assert!(drain());
    let archive = load(&store);
    assert_eq!(archive.entries.len(), 1);
    assert!(archive.entries[0].origin.is_none());
}

#[test]
fn missing_large_special_and_remote_files_never_become_zero_byte_revisions() {
    let scratch = crate::testsync::Scratch::new("artifact-unavailable");
    let store = scratch.path().join("conversations");
    for href in [
        "https://example.com/decision",
        "/no-such-td-artifact",
        "/dev/null",
    ] {
        let result = capture(&store, href);
        assert!(result.revision.is_none(), "{href}");
        assert!(result.bytes.is_none(), "{href}");
        assert!(result.unavailable.is_some());
    }
    let large = scratch.path().join("large.md");
    std::fs::File::create(&large)
        .unwrap()
        .set_len(CAPTURE_LIMIT + 1)
        .unwrap();
    assert!(capture(&store, &file_url(&large)).revision.is_none());
    let fifo = scratch.path().join("pipe.md");
    let name = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(capture(&store, &file_url(&fifo)).revision.is_none());
    assert!(snapshot_path(&store, "../outside.md").is_none());
    assert!(snapshot_path(&store, &format!("{}.exe", "a".repeat(64))).is_none());
}

#[test]
fn damaged_records_are_reported_instead_of_looking_like_an_empty_archive() {
    let scratch = crate::testsync::Scratch::new("artifact-corrupt");
    fs::write(scratch.path().join("broken.ws"), "{broken").unwrap();
    let archive = load(scratch.path());
    assert_eq!(archive.problems.len(), 1);
    let page = fs::read_to_string(write_page(scratch.path()).unwrap()).unwrap();
    assert!(page.contains("archive is incomplete"));
}

#[test]
fn binding_the_same_held_event_keeps_its_capture_and_removes_the_unknown_duplicate() {
    let scratch = crate::testsync::Scratch::new("artifact-binding");
    let store = scratch.path().join("conversations");
    let source = scratch.path().join("decision.md");
    fs::write(&source, "The original decision.").unwrap();
    let event = rec(&file_url(&source), 10);
    file_unbound(&store, "unknown pane", &event).unwrap();
    assert!(drain());
    benchstore::append(&store, "known-conversation", &event.clone().shifted(4)).unwrap();
    let archive = load(&store);
    assert_eq!(archive.entries.len(), 1);
    assert_eq!(
        archive.entries[0].origin.as_deref(),
        Some("known-conversation")
    );
    assert!(archive.entries[0]
        .observation
        .as_ref()
        .unwrap()
        .snapshot
        .is_some());
}

#[test]
fn a_repeat_observation_does_not_replace_or_duplicate_the_visible_shelf_card() {
    let scratch = crate::testsync::Scratch::new("artifact-shelf");
    let original = rec("https://example.com/decision", 10);
    let mut repeated = rec("https://example.com/decision", 20);
    if let Rec::Said { surface, .. } = &mut repeated {
        surface["_td_archive_only"] = serde_json::Value::Bool(true);
    }
    benchstore::append(scratch.path(), "conversation", &original).unwrap();
    benchstore::append(scratch.path(), "conversation", &repeated).unwrap();
    let loaded = benchstore::load(scratch.path(), "conversation");
    assert_eq!(loaded.surfaces.len(), 1);
    assert_eq!(load(scratch.path()).entries.len(), 1);
}

#[test]
fn a_corrupted_saved_copy_is_not_offered_as_the_captured_revision() {
    let scratch = crate::testsync::Scratch::new("artifact-integrity");
    let source = scratch.path().join("decision.md");
    fs::write(&source, "Original bytes").unwrap();
    let href = file_url(&source);
    let saved = keep(scratch.path(), "conversation", &rec(&href, 10), &href);
    fs::write(
        snapshot_path(scratch.path(), saved.snapshot.as_deref().unwrap()).unwrap(),
        "Changed bytes",
    )
    .unwrap();
    let page = fs::read_to_string(write_page(scratch.path()).unwrap()).unwrap();
    assert!(page.contains("content hash no longer matches"));
    assert!(!page.contains(">Open captured revision</a>"));
}
