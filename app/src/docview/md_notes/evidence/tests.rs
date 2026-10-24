use super::*;
use crate::docview::markdown::parse;
use crate::docview::notes::NoteEdit;

fn frame(text: &str) -> Frame {
    Frame {
        revision: Some(crate::artifact_archive::digest(text.as_bytes())),
        anchors: anchors(&parse(text, None)),
    }
}

fn add(frame: &Frame, at: usize, text: &str) -> (NoteEdit, EditContext) {
    let a = &frame.anchors[at];
    let edit = NoteEdit::Add {
        nid: a.nid.clone(),
        title: a.title.clone(),
        text: text.into(),
        ts: "2026-10-24T12:00:00.000Z".into(),
    };
    let context = edit_context(Some(frame), None, &edit);
    (edit, context)
}

fn raw(frame: &Frame, at: usize) -> NoteMap {
    let (edit, context) = add(frame, at, "Please check this.");
    let mut map = NoteMap::default();
    let NoteEdit::Add { nid, .. } = &edit else {
        unreachable!()
    };
    notes::apply(
        &mut map,
        &mut notes::ConcurMap::default(),
        std::slice::from_ref(&edit),
        &[nid],
        false,
    )
    .unwrap();
    enrich_last(&mut map, &edit, &context);
    map
}

#[test]
fn moving_a_unique_passage_keeps_original_words_and_revision() {
    let before = frame("# Original plan\n\nThe worker owns the child process.\n\nOther text.\n");
    let raw = raw(&before, 1);
    let after = frame("# Reordered plan\n\nOther text.\n\nThe worker owns the child process.\n");
    let shown = display(&raw, &after);
    assert_eq!(shown.on(&after.anchors[2].nid).len(), 1);
    let summary = summaries(&shown, &after.anchors[2].nid).join("\n");
    assert!(summary.contains("The worker owns the child process."));
    assert!(summary.contains(before.revision.as_ref().unwrap()));
    assert!(summary.contains("quotation matched"));
    assert_eq!(display(&raw, &before).count(), 1);
}

#[test]
fn equal_duplicate_passages_are_unresolved_even_if_the_old_slug_still_exists() {
    let before = frame("Repeated words worth reviewing carefully.");
    let raw = raw(&before, 0);
    let after = frame(
        "Repeated words worth reviewing carefully.\n\nRepeated words worth reviewing carefully.",
    );
    let shown = display(&raw, &after);
    assert!(after.anchors.iter().all(|a| shown.on(&a.nid).is_empty()));
    assert_eq!(shown.count(), 1);
    let (nid, _) = shown.0.iter().next().unwrap();
    assert!(summaries(&shown, nid)[0].contains("ambiguous"));
    assert_eq!(display(&raw, &before).on(&before.anchors[0].nid).len(), 1);
}

#[test]
fn neighbouring_words_can_disambiguate_duplicates_without_choosing_the_first() {
    let before = frame("# Chosen section\n\nRepeated reviewed paragraph.\n\nChosen end.");
    let raw = raw(&before, 1);
    let after = frame("# Other section\n\nRepeated reviewed paragraph.\n\nOther end.\n\n# Chosen section\n\nRepeated reviewed paragraph.\n\nChosen end.");
    let shown = display(&raw, &after);
    assert!(shown.on(&after.anchors[1].nid).is_empty());
    assert_eq!(shown.on(&after.anchors[4].nid).len(), 1);
}

#[test]
fn a_small_wording_change_needs_context_and_preserves_the_original_quote() {
    let before =
        frame("# Context\n\nWe will keep these original review notes for every reader.\n\nEnd.");
    let raw = raw(&before, 1);
    let after =
        frame("# Context\n\nYou will keep these original review notes for every reader.\n\nEnd.");
    let shown = display(&raw, &after);
    assert_eq!(shown.on(&after.anchors[1].nid).len(), 1);
    let summary = summaries(&shown, &after.anchors[1].nid).join("\n");
    assert!(summary.contains("We will keep"));
    assert!(summary.contains("wording changed"));
    let unrelated = frame("# New context\n\nWe will keep entirely different assertions in this replacement.\n\nNew end.");
    let unresolved = display(&raw, &unrelated);
    assert!(unresolved.on(&unrelated.anchors[1].nid).is_empty());
    assert_eq!(unresolved.count(), 1);
}

#[test]
fn queued_save_keeps_the_creation_revision_and_delete_follows_the_saved_note() {
    let scratch = crate::testsync::Scratch::new("quoted-note-keep");
    let doc = scratch.path().join("plan.md");
    let store = scratch.path().join("notes.json");
    let old =
        frame("# Context\n\nWe will keep these original review notes for every reader.\n\nEnd.");
    let (edit, context) = add(&old, 1, "Check this before shipping.");
    std::fs::write(&doc, "the file changed while save was queued").unwrap();
    let raw = keep_with_evidence(&store, &doc, &[edit], &[context]).unwrap();
    let after =
        frame("# Context\n\nYou will keep these original review notes for every reader.\n\nEnd.");
    let shown = display(&raw, &after);
    let nid = &after.anchors[1].nid;
    assert!(summaries(&shown, nid)[0].contains(old.revision.as_ref().unwrap()));
    let note = shown.on(nid)[0];
    let delete = NoteEdit::Delete {
        nid: nid.clone(),
        text: note.text.into(),
        ts: note.ts.map(str::to_owned),
    };
    let context = edit_context(Some(&after), Some(&shown), &delete);
    let kept = keep_with_evidence(&store, &doc, &[delete], &[context]).unwrap();
    assert_eq!(kept.count(), 0);
}

#[test]
fn legacy_notes_survive_format_upgrade_without_invented_evidence() {
    let scratch = crate::testsync::Scratch::new("quoted-note-legacy");
    let doc = scratch.path().join("plan.md");
    let store = scratch.path().join("notes.json");
    let f = frame("First paragraph.\n\nSecond paragraph.");
    let legacy = format!(
        r#"{{"format":1,"notes":{{"{}":[{{"text":"Old note","title":"Old title","custom":"keep me"}}]}}}}"#,
        f.anchors[0].nid
    );
    std::fs::write(&store, legacy).unwrap();
    let (edit, context) = add(&f, 1, "New note");
    let kept = keep_with_evidence(&store, &doc, &[edit], &[context]).unwrap();
    assert_eq!(kept.count(), 2);
    let shown = display(&kept, &f);
    assert!(summaries(&shown, &f.anchors[0].nid)[0].contains("unavailable"));
    let bytes = std::fs::read_to_string(&store).unwrap();
    assert!(bytes.contains("keep me"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&bytes).unwrap()["format"],
        2
    );
    std::fs::write(&store, "{\"format\":99,\"notes\":{}}").unwrap();
    let before = std::fs::read(&store).unwrap();
    assert!(keep_with_evidence(&store, &doc, &[], &[]).is_err());
    assert_eq!(std::fs::read(&store).unwrap(), before);
}

#[test]
fn concurrent_writers_keep_both_notes_and_their_evidence() {
    let scratch = crate::testsync::Scratch::new("quoted-note-concurrent");
    let doc = scratch.path().join("plan.md");
    let store = scratch.path().join("notes.json");
    let f = frame("The reviewed passage stays the same.");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let joins: Vec<_> = (0..4)
        .map(|i| {
            let (doc, store, f, barrier) = (doc.clone(), store.clone(), f.clone(), barrier.clone());
            std::thread::spawn(move || {
                let (edit, context) = add(&f, 0, &format!("Reader {i}"));
                barrier.wait();
                keep_with_evidence(&store, &doc, &[edit], &[context]).unwrap();
            })
        })
        .collect();
    for join in joins {
        join.join().unwrap();
    }
    let kept = read(&store).unwrap();
    assert_eq!(kept.count(), 4);
    assert_eq!(summaries(&display(&kept, &f), &f.anchors[0].nid).len(), 4);
}
