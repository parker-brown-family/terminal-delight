use crate::{journal::Journal, state::*};
use serde_json::json;

pub(crate) fn actor(pid: u32) -> Actor {
    Actor {
        run: RunKey {
            harness: "fixture".into(),
            conversation: format!("conversation-{pid}"),
            process: ProcessKey {
                boot_id: "boot".into(),
                pid,
                start_ticks: 100,
            },
            endpoint: format!("/fixture/{pid}"),
            endpoint_generation: "1".into(),
        },
        location: Location {
            td_session: "test".into(),
            pane: u64::from(pid),
            shell: ProcessKey {
                boot_id: "boot".into(),
                pid: pid + 100,
                start_ticks: 10,
            },
        },
        executable: Fact::NotCollected,
        version: Fact::NotCollected,
        inference_runtime: Fact::NotCollected,
        model: ModelFacts::default(),
        attribution: Fact::NotCollected,
        binding_evidence: "test fixture".into(),
        transcript: Fact::NotCollected,
        observed_at_ms: now_ms(),
    }
}

pub(crate) fn request(target: &Actor) -> SendRequest {
    SendRequest {
        key: uuid::Uuid::now_v7().to_string(),
        target: target.run.clone(),
        text: "One paragraph".into(),
        dispatch_before_ms: now_ms() + 60_000,
        parent_request: None,
        caused_by_message: None,
        expected_model: None,
        experiment: Some("regression".into()),
    }
}

#[test]
fn acceptance_survives_lost_confirmation_and_conflicting_replay_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.sqlite3");
    let sender = actor(1);
    let target = actor(2);
    let input = request(&target);
    let first = {
        let mut j = Journal::open(&path).unwrap();
        j.accept(&input, &sender, &target).unwrap()
    };
    let mut j = Journal::open(&path).unwrap();
    let repeated = j.accept(&input, &sender, &target).unwrap();
    assert_eq!(first.request, repeated.request);
    assert_eq!(first.events.len(), repeated.events.len());
    let mut changed = input;
    changed.text.push('!');
    assert!(j
        .accept(&changed, &sender, &target)
        .unwrap_err()
        .contains("conflicts"));
}

#[test]
fn dispatch_crash_is_quarantined_and_never_replayed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("journal.sqlite3");
    let sender = actor(1);
    let target = actor(2);
    let input = request(&target);
    let id = {
        let mut j = Journal::open(&path).unwrap();
        let id = j.accept(&input, &sender, &target).unwrap().request;
        j.intent(&id, &target, json!({"test":true})).unwrap();
        id
    };
    let mut j = Journal::open(&path).unwrap();
    j.recover().unwrap();
    assert_eq!(
        j.receipt(&id).unwrap().native_delivery,
        "delivery_unconfirmed"
    );
    assert!(j.next().unwrap().is_none());
    assert_eq!(j.accept(&input, &sender, &target).unwrap().request, id);
}

#[test]
fn native_acceptance_is_not_consumer_ack_or_work_result() {
    let dir = tempfile::tempdir().unwrap();
    let mut j = Journal::open(&dir.path().join("j")).unwrap();
    let s = actor(1);
    let r = actor(2);
    let id = j.accept(&request(&r), &s, &r).unwrap().request;
    j.intent(&id, &r, json!({})).unwrap();
    j.outcome(&id, "native_accepted", json!({"scope":"queue"}))
        .unwrap();
    let receipt = j.receipt(&id).unwrap();
    assert!(receipt.recipient_ack.value().is_none());
    assert!(receipt.work_reports.is_empty());
    j.accept(&request(&r), &s, &r).unwrap();
    assert!(
        j.next().unwrap().is_none(),
        "Native queue ownership must retain the run's consumer credit"
    );
    let report = Report {
        request: id,
        reply_key: uuid::Uuid::now_v7().to_string(),
        expected_run: r.run.clone(),
        kind: ReportKind::Received,
        text: "I accept responsibility".into(),
        evidence: vec![],
    };
    assert!(j
        .report(&report, &r)
        .unwrap()
        .recipient_ack
        .value()
        .is_some());
    assert!(j.next().unwrap().is_some());
}

#[test]
fn reports_bind_to_recipient_run_and_keep_manager_verification_separate() {
    let dir = tempfile::tempdir().unwrap();
    let mut j = Journal::open(&dir.path().join("j")).unwrap();
    let s = actor(1);
    let r = actor(2);
    let id = j.accept(&request(&r), &s, &r).unwrap().request;
    let report = Report {
        request: id,
        reply_key: uuid::Uuid::now_v7().to_string(),
        expected_run: r.run.clone(),
        kind: ReportKind::Result,
        text: "Wrote the file".into(),
        evidence: vec!["/tmp/paragraph.md".into()],
    };
    let mut impostor = r.clone();
    impostor.run.process.start_ticks += 1;
    assert!(j.report(&report, &impostor).is_err());
    let receipt = j.report(&report, &r).unwrap();
    assert_eq!(receipt.work_reports.len(), 1);
    assert!(receipt.resolution_reports.is_empty());
    assert_eq!(
        j.report(&report, &r).unwrap().work_reports.len(),
        1,
        "Reply retries dedupe"
    );
    let resolution = Report {
        reply_key: uuid::Uuid::now_v7().to_string(),
        expected_run: s.run.clone(),
        kind: ReportKind::Verified,
        text: "Read file and checked one paragraph".into(),
        ..report
    };
    assert_eq!(
        j.report(&resolution, &s).unwrap().resolution_reports.len(),
        1
    );
}

#[test]
fn cancel_before_dispatch_prevents_native_intent() {
    let dir = tempfile::tempdir().unwrap();
    let mut j = Journal::open(&dir.path().join("j")).unwrap();
    let s = actor(1);
    let r = actor(2);
    let id = j.accept(&request(&r), &s, &r).unwrap().request;
    assert_eq!(
        j.cancel(&id, &s).unwrap().native_delivery,
        "cancel_confirmed"
    );
    assert!(j.next().unwrap().is_none());
    assert!(j.intent(&id, &r, json!({})).is_err());
}

#[test]
fn retry_budget_is_three_and_unknown_writes_cannot_retry() {
    let dir = tempfile::tempdir().unwrap();
    let mut j = Journal::open(&dir.path().join("j")).unwrap();
    let s = actor(1);
    let r = actor(2);
    let id = j.accept(&request(&r), &s, &r).unwrap().request;
    for _ in 0..3 {
        j.intent(&id, &r, json!({})).unwrap();
        j.outcome(&id, "definite_nonacceptance", json!({})).unwrap();
    }
    assert!(j.intent(&id, &r, json!({})).is_err());
    assert_eq!(j.receipt(&id).unwrap().attempts.len(), 3);
}

#[test]
fn cursor_from_another_database_cannot_claim_coverage() {
    let dir = tempfile::tempdir().unwrap();
    let a = Journal::open(&dir.path().join("a")).unwrap();
    let b = Journal::open(&dir.path().join("b")).unwrap();
    assert!(b
        .page(Some(&format!("{}:0", a.database_id)), 64)
        .unwrap_err()
        .contains("different journal"));
}

#[test]
fn unknown_model_and_unknown_ack_are_serialized_as_unknown() {
    let value = serde_json::to_value(actor(1)).unwrap();
    assert_eq!(
        value.pointer("/model/provider_revision/state").unwrap(),
        "unknown"
    );
    assert_eq!(
        value.pointer("/model/reported/state").unwrap(),
        "not_collected"
    );
}

#[test]
fn a_failed_outbox_insert_rolls_back_the_entire_acceptance() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("j");
    let mut j = Journal::open(&path).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_outbox BEFORE INSERT ON outbox BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
    let s = actor(1);
    let r = actor(2);
    assert!(j.accept(&request(&r), &s, &r).is_err());
    for table in ["actors", "requests", "events", "outbox"] {
        let count: u32 = db
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "Partial acceptance survived in {table}");
    }
}

#[test]
fn audit_records_cannot_be_updated_or_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("j");
    let mut j = Journal::open(&path).unwrap();
    let s = actor(1);
    let r = actor(2);
    j.accept(&request(&r), &s, &r).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    for sql in [
        "UPDATE events SET kind='verified'",
        "DELETE FROM events",
        "UPDATE actors SET body='{}'",
        "DELETE FROM requests",
    ] {
        assert!(
            db.execute(sql, []).is_err(),
            "Immutable evidence was changed by {sql}"
        );
    }
}

#[test]
fn future_schema_and_corrupt_databases_are_not_reinitialized() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("j");
    drop(Journal::open(&path).unwrap());
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("PRAGMA user_version=99")
        .unwrap();
    assert!(Journal::open(&path).is_err());
    let corrupt = dir.path().join("bad");
    std::fs::write(&corrupt, b"not a sqlite database").unwrap();
    assert!(Journal::open(&corrupt).is_err());
    assert_eq!(std::fs::read(&corrupt).unwrap(), b"not a sqlite database");
}

#[test]
fn unproven_model_does_not_block_other_requests_and_explicit_evidence_releases_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut j = Journal::open(&dir.path().join("j")).unwrap();
    let s = actor(1);
    let mut r = actor(2);
    let mut input = request(&r);
    input.expected_model = Some("raw-model-revision".into());
    let first = j.accept(&input, &s, &r).unwrap().request;
    j.hold_model(&first).unwrap();
    let second = j
        .accept(&request(&actor(3)), &s, &actor(3))
        .unwrap()
        .request;
    assert_eq!(j.next().unwrap().unwrap().id, second);
    j.reconcile_models(&[r.clone()]).unwrap();
    j.cancel(&second, &s).unwrap();
    assert!(j.next().unwrap().is_none());
    r.model.reported = Fact::observed("raw-model-revision".into(), "native response");
    j.reconcile_models(&[r]).unwrap();
    assert_eq!(j.next().unwrap().unwrap().id, first);
}

#[test]
fn a_late_native_receipt_cannot_take_back_recipient_credit() {
    let dir = tempfile::tempdir().unwrap();
    let mut j = Journal::open(&dir.path().join("j")).unwrap();
    let s = actor(1);
    let r = actor(2);
    let id = j.accept(&request(&r), &s, &r).unwrap().request;
    j.intent(&id, &r, json!({})).unwrap();
    j.report(
        &Report {
            request: id.clone(),
            reply_key: uuid::Uuid::now_v7().to_string(),
            expected_run: r.run.clone(),
            kind: ReportKind::Received,
            text: "Accepted".into(),
            evidence: vec![],
        },
        &r,
    )
    .unwrap();
    j.outcome(&id, "native_accepted", json!({"late":true}))
        .unwrap();
    let second = j.accept(&request(&r), &s, &r).unwrap().request;
    assert_eq!(j.next().unwrap().unwrap().id, second);
}

#[test]
fn exports_reconstruct_relationships_without_native_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let mut j = Journal::open(&dir.path().join("j")).unwrap();
    let s = actor(1);
    let r = actor(2);
    let root = j.accept(&request(&r), &s, &r).unwrap().request;
    let mut child = request(&actor(3));
    child.parent_request = Some(root.clone());
    let id = j.accept(&child, &r, &actor(3)).unwrap().request;
    let export = j.export(&id).unwrap();
    assert_eq!(export["envelope"]["parent_request"], root);
    assert_eq!(export["sender"]["run"]["process"]["pid"], 2);
    assert_eq!(export["recipient"]["run"]["process"]["pid"], 3);
    let body = export.to_string();
    assert!(!body.contains("peerToken"));
    assert!(!body.contains("\"type\":\"auth\""));
    let objects = crate::presentation::objects(&mut j, &id).unwrap();
    assert_eq!(objects[0]["kind"], "architecture");
    assert!(objects[0]["model"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["state"] == "work result unavailable"));
}

#[test]
fn backlog_and_local_ancestry_are_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let mut j = Journal::open(&dir.path().join("j")).unwrap();
    let s = actor(1);
    let r = actor(2);
    for _ in 0..16 {
        j.accept(&request(&r), &s, &r).unwrap();
    }
    assert!(j
        .accept(&request(&r), &s, &r)
        .unwrap_err()
        .contains("backlog"));
    let mut unknown_parent = request(&actor(3));
    unknown_parent.parent_request = Some("unresolved-external".into());
    assert!(j.accept(&unknown_parent, &s, &actor(3)).is_err());
}

#[test]
fn unknown_future_events_are_kept_without_authorizing_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("j");
    let mut j = Journal::open(&path).unwrap();
    let s = actor(1);
    let r = actor(2);
    let id = j.accept(&request(&r), &s, &r).unwrap().request;
    j.intent(&id, &r, json!({})).unwrap();
    j.outcome(&id, "delivery_unconfirmed", json!({})).unwrap();
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO events(request,at_ms,kind,data) VALUES(?1,?2,'future_super_ack','{}')",
            rusqlite::params![id, now_ms()],
        )
        .unwrap();
    let receipt = j.receipt(&id).unwrap();
    assert_eq!(receipt.native_delivery, "delivery_unconfirmed");
    assert!(receipt.recipient_ack.value().is_none());
    assert!(receipt.events.iter().any(|e| e.kind == "future_super_ack"));
    assert!(j.next().unwrap().is_none());
}

#[test]
fn clock_rollback_pauses_new_work_without_rewriting_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("j");
    let mut j = Journal::open(&path).unwrap();
    let s = actor(1);
    let r = actor(2);
    let id = j.accept(&request(&r), &s, &r).unwrap().request;
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE metadata SET value=?1 WHERE key='clock_high_water_ms'",
            [(now_ms() + 600_000).to_string()],
        )
        .unwrap();
    assert!(j
        .accept(&request(&r), &s, &r)
        .unwrap_err()
        .contains("clock"));
    assert_eq!(j.receipt(&id).unwrap().events.len(), 1);
}

#[test]
fn native_claude_wire_preserves_identity_and_ignores_another_requests_receipt() {
    use sha2::{Digest, Sha256};
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::{UnixListener, UnixStream},
    };
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".claude/sessions")).unwrap();
    let endpoint = dir.path().join("recipient.sock");
    let listener = UnixListener::bind(&endpoint).unwrap();
    let callback_dir = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(callback_dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let inbox = crate::adapters::claude::Inbox::bind_fixture(callback_dir.path()).unwrap();
    let key = crate::identity::process(std::process::id()).unwrap();
    let hash = format!(
        "{:x}",
        Sha256::digest(endpoint.to_string_lossy().as_bytes())
    );
    std::fs::write(
        home.join(".claude/sessions")
            .join(format!("{}.{hash}.key", key.pid)),
        json!({"peerToken":"0123456789abcdef0123456789abcdef"}).to_string(),
    )
    .unwrap();
    let mut recipient = actor(key.pid);
    recipient.run.process = key;
    recipient.run.endpoint = endpoint.to_string_lossy().into_owned();
    recipient.run.endpoint_generation = crate::identity::socket_generation(&endpoint).unwrap();
    let native_id = uuid::Uuid::now_v7().to_string();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut r = BufReader::new(stream);
        let mut line = String::new();
        r.read_line(&mut line).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&line).unwrap()["type"],
            "auth"
        );
        line.clear();
        r.read_line(&mut line).unwrap();
        let input: serde_json::Value = serde_json::from_str(&line).unwrap();
        let path = input["from"]
            .as_str()
            .unwrap()
            .strip_prefix("uds:")
            .unwrap();
        for (id, status) in [
            ("another-request", "refused"),
            (input["msg_id"].as_str().unwrap(), "delivered"),
        ] {
            let mut s = UnixStream::connect(path).unwrap();
            writeln!(s,"{}",json!({"type":"control","action":"peer_message_status","orig_msg_id":id,"status":status})).unwrap();
        }
        input
    });
    let outcome = inbox.send(
        &home,
        &recipient,
        &native_id,
        "One exact paragraph",
        now_ms() + 10_000,
    );
    assert_eq!(outcome.kind, "native_accepted");
    assert_eq!(
        outcome.evidence["diagnostics"][0]["orig_msg_id"],
        "another-request"
    );
    let frame = server.join().unwrap();
    assert_eq!(frame["uuid"], native_id);
    assert_eq!(frame["msg_id"], native_id);
    assert_eq!(frame["session_id"], recipient.run.conversation);
    assert_eq!(frame["priority"], "next");
    assert_eq!(frame["message"]["content"], "One exact paragraph");
    assert!(!outcome.evidence.to_string().contains("0123456789abcdef"));
}

fn codex_fixture(
    version: &str,
) -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::thread::JoinHandle<Vec<String>>,
) {
    use std::os::unix::net::UnixListener;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("codex.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let version = version.to_string();
    let task = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let mut methods = vec![];
        while let Ok(tungstenite::Message::Text(t)) = socket.read() {
            let v: serde_json::Value = serde_json::from_str(&t).unwrap();
            let method = v["method"].as_str().unwrap().to_string();
            methods.push(method.clone());
            let result = match method.as_str() {
                "initialize" => json!({"userAgent":format!("codex-tui/{version} (fixture)")}),
                "initialized" => continue,
                "thread/loaded/list" => json!({"data":[],"nextCursor":null}),
                other => panic!("Unexpected native operation {other}"),
            };
            socket
                .send(tungstenite::Message::text(
                    json!({"id":v["id"],"result":result}).to_string(),
                ))
                .unwrap();
        }
        methods
    });
    (dir, path, task)
}

#[test]
fn native_codex_does_not_resume_an_unloaded_conversation() {
    let (_dir, path, server) = codex_fixture("0.163.0");
    let mut c = crate::adapters::codex::Connection::open(&path).unwrap();
    assert!(!c.loaded("not-loaded").unwrap());
    drop(c);
    assert_eq!(
        server.join().unwrap(),
        vec!["initialize", "initialized", "thread/loaded/list"]
    );
}

#[test]
fn native_codex_rejects_runtime_routing_mismatch_before_any_queue_write() {
    let (_dir, path, server) = codex_fixture("0.163.0");
    let mut c = crate::adapters::codex::Connection::open(&path).unwrap();
    let mut recipient = actor(1);
    recipient.run.process = crate::identity::process(std::process::id()).unwrap();
    recipient.run.endpoint = path.to_string_lossy().into_owned();
    recipient.run.endpoint_generation = crate::identity::socket_generation(&path).unwrap();
    recipient.location.td_session = "deliberately-different-session".into();
    assert_eq!(
        c.send(&recipient, "native-id", "Do work", now_ms() + 10_000)
            .kind,
        "native_unsupported"
    );
    drop(c);
    assert_eq!(server.join().unwrap(), vec!["initialize", "initialized"]);
}

#[test]
fn untested_codex_runtime_version_is_explicitly_unsupported() {
    let (_dir, path, server) = codex_fixture("99.0.0");
    assert!(crate::adapters::codex::Connection::open(&path).is_err());
    assert_eq!(server.join().unwrap(), vec!["initialize"]);
}

#[test]
fn recovery_rebuilds_a_damaged_outbox_and_preserves_ambiguous_delivery() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("j");
    let mut j = Journal::open(&path).unwrap();
    let s = actor(1);
    let r = actor(2);
    let id = j.accept(&request(&r), &s, &r).unwrap().request;
    j.intent(&id, &r, json!({})).unwrap();
    j.outcome(&id, "delivery_unconfirmed", json!({})).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("UPDATE outbox SET state='pending',attempts=0", [])
        .unwrap();
    j.recover().unwrap();
    assert!(j.next().unwrap().is_none());
    assert_eq!(
        j.receipt(&id).unwrap().native_delivery,
        "delivery_unconfirmed"
    );
    db.execute(
        "INSERT INTO events(request,at_ms,kind,data) VALUES(?1,?2,'model_observed','{}')",
        rusqlite::params![id, now_ms()],
    )
    .unwrap();
    j.recover().unwrap();
    assert!(
        j.next().unwrap().is_none(),
        "A model observation cannot authorize replaying an ambiguous write"
    );
}

#[test]
fn malformed_journal_records_stop_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("j");
    let mut j = Journal::open(&path).unwrap();
    let s = actor(1);
    let r = actor(2);
    let id = j.accept(&request(&r), &s, &r).unwrap().request;
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO events(request,at_ms,kind,data) VALUES(?1,?2,'future_record','malformed')",
            rusqlite::params![id, now_ms()],
        )
        .unwrap();
    assert!(j.recover().is_err());
}

#[test]
fn an_assignment_held_for_model_evidence_is_still_locally_cancelable() {
    let dir = tempfile::tempdir().unwrap();
    let mut j = Journal::open(&dir.path().join("j")).unwrap();
    let s = actor(1);
    let r = actor(2);
    let id = j.accept(&request(&r), &s, &r).unwrap().request;
    j.hold_model(&id).unwrap();
    assert_eq!(
        j.cancel(&id, &s).unwrap().native_delivery,
        "cancel_confirmed"
    );
    j.recover().unwrap();
    assert!(j.next().unwrap().is_none());
    assert!(j.intent(&id, &r, json!({})).is_err());
}
