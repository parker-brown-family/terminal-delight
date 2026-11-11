//! Portable record and clock checks without a live desktop.
#![allow(dead_code)]
#[path = "../src"]
mod pack {
    pub mod lang;
}
use pack::lang;
#[path = "../src/docview"]
mod docview {
    pub mod engine;
    pub mod notes;
    pub mod reading;
}

use docview::engine::{layout_hash, ReadingLayout, ReadingTarget};
use docview::reading::{self, Activity, Clock, Record};
use std::time::Duration;

/// Process boundary used by the cross-host race check; inert in an ordinary suite.
#[test]
fn checkpoint_worker() {
    let Ok(input) = std::env::var("TD_READING_TEST_INPUT") else {
        return;
    };
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
    let path = std::path::Path::new(value["path"].as_str().unwrap());
    if let Some(output) = value["outside_path"].as_str() {
        let bytes = std::fs::read(path).unwrap();
        let outside: Vec<u8> = docview::notes::outside_notes(&bytes)
            .flatten()
            .copied()
            .collect();
        std::fs::write(output, outside).unwrap();
        return;
    }
    let expected = layout_hash(&std::fs::read(path).unwrap());
    for value in value["sessions"].as_array().unwrap() {
        let session: reading::Session = serde_json::from_value(value.clone()).unwrap();
        reading::save(path, expected, &session).unwrap();
    }
}

#[test]
fn the_portable_fixture_keeps_both_hosts_unknown_and_zero() {
    let fixture = include_bytes!("fixtures/reading/record.json");
    let record: Record = serde_json::from_slice(fixture).unwrap();
    record.validate().unwrap();
    assert_eq!(record.sessions.len(), 2);
    assert_eq!(record.sessions[0].targets[1].visible_ms, Some(0));
    assert_eq!(record.sessions[1].targets[2].visible_ms, None);
    let totals = record.totals(&record.sessions[0].revision, None);
    assert_eq!(totals["section-one"], 2500);
    assert!(!totals.contains_key("figure-one"));
    let source = std::path::Path::new("/home/parker/BROWN-FAMILY-SPORTS/Software/agent-skills/briefing/tests/fixtures/reading/record.json");
    if source.exists() {
        assert_eq!(std::fs::read(source).unwrap(), fixture);
    }
}

fn layout() -> ReadingLayout {
    ReadingLayout {
        document: "test-reading".into(),
        revision: format!("sha256:{}", "1".repeat(64)),
        targets: vec![
            ReadingTarget {
                anchor: "one".into(),
                title: "One".into(),
                kind: "section".into(),
                parent: None,
                words: Some(15),
                rect: None,
            },
            ReadingTarget {
                anchor: "two".into(),
                title: "Two".into(),
                kind: "section".into(),
                parent: None,
                words: Some(15),
                rect: None,
            },
            ReadingTarget {
                anchor: "unknown".into(),
                title: "Unavailable".into(),
                kind: "figure".into(),
                parent: Some("two".into()),
                words: None,
                rect: None,
            },
        ],
    }
}
fn sample(c: &mut Clock, ms: u64, a: Activity, visible: &[&str]) {
    c.sample(
        Duration::from_millis(ms),
        a,
        &visible.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        &["one".into(), "two".into()],
    )
    .unwrap();
}

#[test]
fn still_active_hour_counts_but_an_inactive_day_does_not() {
    let mut clock = Clock::new(&layout()).unwrap();
    sample(&mut clock, 0, Activity::Active, &["one"]);
    for second in 1..=3600 {
        sample(&mut clock, second * 1000, Activity::Active, &["one"]);
    }
    assert_eq!(clock.session.targets[0].visible_ms, Some(3_600_000));
    assert_eq!(clock.session.targets[1].visible_ms, Some(0));
    assert_eq!(clock.session.targets[2].visible_ms, None);
    sample(&mut clock, 3_600_000, Activity::Inactive, &[]);
    for second in 1..=86400 {
        sample(
            &mut clock,
            3_600_000 + second * 1000,
            Activity::Inactive,
            &[],
        );
    }
    assert_eq!(clock.session.targets[0].visible_ms, Some(3_600_000));
}

#[test]
fn scroll_settles_the_old_view_and_sleep_stays_a_gap() {
    let mut c = Clock::new(&layout()).unwrap();
    sample(&mut c, 0, Activity::Active, &["one"]);
    sample(&mut c, 1000, Activity::Active, &["two"]);
    sample(&mut c, 2000, Activity::Inactive, &[]);
    assert_eq!(c.session.targets[0].visible_ms, Some(1000));
    assert_eq!(c.session.targets[1].visible_ms, Some(1000));
    sample(&mut c, 62000, Activity::Active, &["one"]);
    assert_eq!(c.session.targets[0].visible_ms, Some(1000));
    assert_eq!(c.session.gaps[0].elapsed_ms, 60000);
}

#[test]
fn a_renderer_that_was_never_active_has_no_session_to_save() {
    let mut c = Clock::new(&layout()).unwrap();
    sample(&mut c, 0, Activity::Inactive, &[]);
    sample(&mut c, 1000, Activity::Inactive, &[]);
    assert!(c.checkpoint().is_none());
}

#[test]
fn retry_merges_maxima_and_preserves_unknown_values() {
    let mut c = Clock::new(&layout()).unwrap();
    sample(&mut c, 0, Activity::Active, &["one"]);
    sample(&mut c, 1000, Activity::Active, &["one"]);
    let s = c.checkpoint().unwrap();
    let mut record = Record {
        format: 1,
        sessions: vec![],
    };
    record.merge(&s).unwrap();
    record.merge(&s).unwrap();
    assert_eq!(record.sessions[0].targets[0].visible_ms, Some(1000));
    assert_eq!(record.sessions[0].targets[2].visible_ms, None);
    let mut changed = s.clone();
    changed.host = "browser".into();
    assert!(record.merge(&changed).is_err());
}

#[test]
fn required_unknown_fields_cannot_disappear_or_become_boolean_zero() {
    let c = Clock::new(&layout()).unwrap();
    let value = serde_json::to_value(&c.session).unwrap();
    let mut missing = value.clone();
    missing["targets"][0]
        .as_object_mut()
        .unwrap()
        .remove("visible_ms");
    assert!(serde_json::from_value::<reading::Session>(missing).is_err());
    let mut boolean = value;
    boolean["targets"][0]["visible_ms"] = serde_json::json!(false);
    assert!(serde_json::from_value::<reading::Session>(boolean).is_err());
}

#[test]
fn exact_original_save_preserves_notes_and_authored_bytes_and_layout_hash() {
    let id = layout();
    let directory = std::env::temp_dir().join(format!(
        "td-reading-{}-{}",
        std::process::id(),
        Clock::new(&id).unwrap().session.session
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("brief.html");
    let before=format!("<!doctype html><html><body><h2 id=one>One</h2><script id=\"report-notes\" data-format=\"1\">{{\"one\":[{{\"text\":\"Keep this note.\",\"ts\":\"2026-11-10 00:00\"}}]}}</script><script id=\"report-concurs\" data-format=\"1\">{{}}</script><script id=\"brief-meta\">{{\"id\":\"{}\",\"revision\":\"{}\"}}</script><script id=\"report-reading\" data-format=\"1\">{{\"format\":1,\"sessions\":[]}}</script></body></html>",id.document,id.revision).into_bytes();
    std::fs::write(&path, &before).unwrap();
    let expected = layout_hash(&before);
    let mut clock = Clock::new(&id).unwrap();
    sample(&mut clock, 0, Activity::Active, &["one"]);
    sample(&mut clock, 1000, Activity::Active, &["one"]);
    let s = clock.checkpoint().unwrap();
    reading::save(&path, expected, &s).unwrap();
    reading::save(&path, expected, &s).unwrap();
    let after = std::fs::read(&path).unwrap();
    assert_eq!(layout_hash(&after), expected);
    assert_eq!(
        reading::read(&after).unwrap().sessions[0].targets[0].visible_ms,
        Some(1000)
    );
    assert!(String::from_utf8(after.clone())
        .unwrap()
        .contains("Keep this note."));
    assert_eq!(
        docview::notes::outside_notes(&before)
            .flatten()
            .copied()
            .collect::<Vec<_>>(),
        docview::notes::outside_notes(&after)
            .flatten()
            .copied()
            .collect::<Vec<_>>()
    );
    std::fs::write(&path, [after.as_slice(), b"changed"].concat()).unwrap();
    assert!(reading::save(&path, expected, &s).is_err());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn copy_map_names_what_was_scrolled_past_as_the_browser_does() {
    // Same estimate as briefing's reading.py/reading.js: 15 words needs the 4 s floor; 16 would not.
    assert_eq!(reading::reading_ms(15), 4000);
    assert_eq!(reading::reading_ms(16), 4174);
    assert_eq!(reading::reading_ms(161), 42_000);
    assert_eq!(reading::verdict(None, Some(15)), "not measured");
    assert_eq!(reading::verdict(Some(0), Some(15)), "UNSEEN");
    assert_eq!(reading::verdict(Some(1599), Some(15)), "PASSED");
    assert_eq!(reading::verdict(Some(1600), Some(15)), "had time");
    assert_eq!(reading::verdict(Some(9000), None), "words unknown");

    let totals: std::collections::HashMap<String, u64> =
        [("one".to_string(), 1000), ("two".to_string(), 6000)].into();
    let map = reading::summary(&layout(), Some(&totals));
    assert!(
        map.starts_with("\nVISIBLE TIME — test-reading · 111111111111\n"),
        "{map}"
    );
    assert!(map.contains("Scrolled past: one\n"), "{map}");
    assert!(
        map.contains("[one] One · 0:01 of ~0:04 · PASSED\n"),
        "{map}"
    );
    assert!(
        map.contains("[two] Two · 0:06 of ~0:04 · had time\n"),
        "{map}"
    );
    assert!(
        map.contains("[unknown] Unavailable · not measured\n"),
        "{map}"
    );

    let none = reading::summary(&layout(), None);
    assert!(none.contains("Nothing scrolled past.\n"), "{none}");
}
