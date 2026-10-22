//! A request and its outbox entry commit together before any native write.
use crate::state::*;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

pub struct Journal {
    db: Connection,
    pub database_id: String,
}

#[derive(Clone, Debug)]
pub struct Dispatch {
    pub id: String,
    pub envelope: SendRequest,
    pub sender: Actor,
    pub recipient: Actor,
    pub attempt: u32,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl Journal {
    pub fn replay(&self, request: &SendRequest, sender: &Actor) -> Result<Option<Receipt>> {
        let canonical = serde_json::to_string(request).map_err(err)?;
        let previous: Option<(String, String)> = self
            .db
            .query_row(
                "SELECT id,canonical FROM requests WHERE sender_key=?1 AND request_key=?2",
                params![sender.conversation_key(), request.key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(err)?;
        if let Some((id, body)) = previous {
            if body != canonical {
                return Err("request key conflicts with its recorded immutable payload".into());
            }
            return self.receipt(&id).map(Some);
        }
        Ok(None)
    }
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)
                .map_err(err)?;
        }
        // O_NOFOLLOW and private mode before SQLite opens the already-created file.
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(err)?;
        let db = Connection::open(path).map_err(err)?;
        db.busy_timeout(std::time::Duration::from_secs(2))
            .map_err(err)?;
        db.execute_batch(
            "PRAGMA journal_mode=DELETE; PRAGMA synchronous=EXTRA; PRAGMA foreign_keys=ON;",
        )
        .map_err(err)?;
        let mode: String = db
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .map_err(err)?;
        let sync: u32 = db
            .query_row("PRAGMA synchronous", [], |r| r.get(0))
            .map_err(err)?;
        let fk: bool = db
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .map_err(err)?;
        if mode != "delete" || sync != 3 || !fk {
            return Err("SQLite durability settings were not applied".into());
        }
        let version: u32 = db
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(err)?;
        if version > 1 {
            return Err(format!(
                "Unsupported management journal schema {version}; mutations disabled"
            ));
        }
        db.execute_batch("
            BEGIN IMMEDIATE;
            CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS actors(id TEXT PRIMARY KEY,body TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS requests(
                id TEXT PRIMARY KEY,sender_key TEXT NOT NULL,request_key TEXT NOT NULL,
                canonical TEXT NOT NULL,digest TEXT NOT NULL,sender TEXT NOT NULL REFERENCES actors(id),
                recipient TEXT NOT NULL REFERENCES actors(id),run TEXT NOT NULL,created_ms INTEGER NOT NULL,
                UNIQUE(sender_key,request_key));
            CREATE TABLE IF NOT EXISTS events(
                seq INTEGER PRIMARY KEY AUTOINCREMENT,request TEXT NOT NULL REFERENCES requests(id),
                at_ms INTEGER NOT NULL,kind TEXT NOT NULL,data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS outbox(
                request TEXT PRIMARY KEY REFERENCES requests(id),run TEXT NOT NULL,state TEXT NOT NULL,
                attempts INTEGER NOT NULL,next_ms INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS replies(
                actor TEXT NOT NULL,key TEXT NOT NULL,canonical TEXT NOT NULL,event INTEGER NOT NULL REFERENCES events(seq),
                PRIMARY KEY(actor,key));
            CREATE INDEX IF NOT EXISTS events_request_sequence ON events(request,seq);
            CREATE INDEX IF NOT EXISTS outbox_state_next ON outbox(state,next_ms);
            CREATE INDEX IF NOT EXISTS outbox_run_state ON outbox(run,state);
            CREATE TRIGGER IF NOT EXISTS events_no_update BEFORE UPDATE ON events BEGIN SELECT RAISE(ABORT,'immutable events'); END;
            CREATE TRIGGER IF NOT EXISTS events_no_delete BEFORE DELETE ON events BEGIN SELECT RAISE(ABORT,'immutable events'); END;
            CREATE TRIGGER IF NOT EXISTS requests_no_update BEFORE UPDATE ON requests BEGIN SELECT RAISE(ABORT,'immutable requests'); END;
            CREATE TRIGGER IF NOT EXISTS requests_no_delete BEFORE DELETE ON requests BEGIN SELECT RAISE(ABORT,'immutable requests'); END;
            CREATE TRIGGER IF NOT EXISTS actors_no_update BEFORE UPDATE ON actors BEGIN SELECT RAISE(ABORT,'immutable actors'); END;
            CREATE TRIGGER IF NOT EXISTS actors_no_delete BEFORE DELETE ON actors BEGIN SELECT RAISE(ABORT,'immutable actors'); END;
            PRAGMA user_version=1;
            COMMIT;
        ").map_err(err)?;
        db.execute(
            "INSERT OR IGNORE INTO metadata VALUES('database_id',?1)",
            [uuid::Uuid::now_v7().to_string()],
        )
        .map_err(err)?;
        let database_id = db
            .query_row(
                "SELECT value FROM metadata WHERE key='database_id'",
                [],
                |r| r.get(0),
            )
            .map_err(err)?;
        Ok(Self { db, database_id })
    }

    pub fn recover(&mut self) -> Result<()> {
        let integrity: String = self
            .db
            .query_row("PRAGMA quick_check", [], |r| r.get(0))
            .map_err(err)?;
        if integrity != "ok" {
            return Err(format!("Management database integrity failed: {integrity}"));
        }
        // Rebuild the disposable outbox from immutable facts, never the reverse.
        struct Projection {
            run: String,
            state: String,
            attempts: u32,
            next_ms: u64,
            accepted: bool,
            acknowledged: bool,
        }
        let mut projections = std::collections::BTreeMap::<String, Projection>::new();
        {
            let mut statement = self.db.prepare("SELECT id,body FROM actors").map_err(err)?;
            let mut rows = statement.query([]).map_err(err)?;
            while let Some(row) = rows.next().map_err(err)? {
                let id: String = row.get(0).map_err(err)?;
                let body: String = row.get(1).map_err(err)?;
                let _: Actor = serde_json::from_str(&body).map_err(err)?;
                if id != format!("{:x}", Sha256::digest(body.as_bytes())) {
                    return Err("Actor snapshot digest mismatch".into());
                }
            }
        }
        {
            let mut statement = self
                .db
                .prepare("SELECT id,canonical,digest,run,created_ms FROM requests")
                .map_err(err)?;
            let mut rows = statement.query([]).map_err(err)?;
            while let Some(row) = rows.next().map_err(err)? {
                let id: String = row.get(0).map_err(err)?;
                let body: String = row.get(1).map_err(err)?;
                let digest: String = row.get(2).map_err(err)?;
                let request: SendRequest = serde_json::from_str(&body).map_err(err)?;
                let run: String = row.get(3).map_err(err)?;
                if digest != format!("{:x}", Sha256::digest(body.as_bytes()))
                    || run != serde_json::to_string(&request.target).map_err(err)?
                {
                    return Err("Request digest or run binding mismatch".into());
                }
                projections.insert(
                    id,
                    Projection {
                        run,
                        state: "pending".into(),
                        attempts: 0,
                        next_ms: row.get(4).map_err(err)?,
                        accepted: false,
                        acknowledged: false,
                    },
                );
            }
        }
        {
            let mut statement = self
                .db
                .prepare("SELECT seq,request,at_ms,kind,data FROM events ORDER BY seq")
                .map_err(err)?;
            let events = statement.query_map([], row_event).map_err(err)?;
            for event in events {
                let e = event.map_err(err)?;
                let p = projections
                    .get_mut(&e.request)
                    .ok_or("Journal event names an unknown request")?;
                match e.kind.as_str() {
                    "accepted" => {
                        if p.accepted {
                            return Err("Duplicate acceptance event".into());
                        }
                        p.accepted = true;
                    }
                    "dispatch_intent" => {
                        p.attempts += 1;
                        if p.attempts > 3 {
                            return Err("Journal exceeds dispatch attempt budget".into());
                        }
                        p.state = "dispatching".into();
                    }
                    "native_accepted" => p.state = "awaiting".into(),
                    "native_held" => p.state = "held".into(),
                    "delivery_unconfirmed" => p.state = "unconfirmed".into(),
                    "definite_nonacceptance" => {
                        p.state = if p.attempts < 3 {
                            "pending"
                        } else {
                            "terminal"
                        }
                        .into();
                        p.next_ms =
                            e.at_ms
                                .saturating_add(if p.attempts == 1 { 250 } else { 1000 });
                    }
                    "native_refused"
                    | "native_unsupported"
                    | "expired_before_dispatch"
                    | "target_stale"
                    | "policy_revoked"
                    | "cancel_confirmed" => p.state = "terminal".into(),
                    "recipient_acknowledged" => p.acknowledged = true,
                    "resolution_reported"
                        if e.data.pointer("/report/kind").and_then(Value::as_str)
                            == Some("abandoned") =>
                    {
                        p.acknowledged = true
                    }
                    "model_constraint_unproven" => p.state = "model_held".into(),
                    "model_observed" if p.state == "model_held" => p.state = "pending".into(),
                    _ => {}
                }
                if p.acknowledged {
                    p.state = "acknowledged".into();
                }
            }
        }
        let tx = self.db.transaction().map_err(err)?;
        tx.execute("DELETE FROM outbox", []).map_err(err)?;
        for (id, p) in projections {
            if !p.accepted {
                return Err("Request is missing its acceptance event".into());
            }
            tx.execute(
                "INSERT INTO outbox VALUES(?1,?2,?3,?4,?5)",
                params![id, p.run, p.state, p.attempts, p.next_ms],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)?;
        let ids: Vec<String> = self
            .db
            .prepare("SELECT request FROM outbox WHERE state='dispatching'")
            .map_err(err)?
            .query_map([], |r| r.get(0))
            .map_err(err)?
            .collect::<std::result::Result<_, _>>()
            .map_err(err)?;
        for id in ids {
            self.outcome(&id, "delivery_unconfirmed", json!({"reason":"Window stopped after dispatch intent; native acceptance is unknown. No automatic replay."}))?;
        }
        Ok(())
    }

    pub fn accept(
        &mut self,
        request: &SendRequest,
        sender: &Actor,
        recipient: &Actor,
    ) -> Result<Receipt> {
        let canonical = serde_json::to_string(request).map_err(err)?;
        // Look up before validating the old deadline: a lost confirm can be recovered later.
        if let Some((id, previous)) = self
            .db
            .query_row(
                "SELECT id,canonical FROM requests WHERE sender_key=?1 AND request_key=?2",
                params![sender.conversation_key(), request.key],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(err)?
        {
            if canonical != previous {
                return Err("request key conflicts with its recorded immutable payload".into());
            }
            return self.receipt(&id);
        }
        let now = now_ms();
        request.validate(now)?;
        if request.target != recipient.run {
            return Err("Selected target run changed".into());
        }
        if sender.run == recipient.run {
            return Err("Cannot assign work to the same run".into());
        }
        self.capacity()?;
        let run = serde_json::to_string(&recipient.run).map_err(err)?;
        let (total, per_run): (u32,u32) = self.db.query_row(
            "SELECT COUNT(*),COUNT(CASE WHEN run=?1 THEN 1 END) FROM outbox WHERE state IN ('pending','model_held')", [&run], |r| Ok((r.get(0)?,r.get(1)?))).map_err(err)?;
        if total >= 128 || per_run >= 16 {
            return Err("Management backlog full (128 per session, 16 per run)".into());
        }
        let root = if let Some(parent) = &request.parent_request {
            let mut id = parent.clone();
            let mut root = None;
            for _ in 0..32 {
                let body: Option<String> = self
                    .db
                    .query_row("SELECT canonical FROM requests WHERE id=?1", [&id], |r| {
                        r.get(0)
                    })
                    .optional()
                    .map_err(err)?;
                let Some(body) = body else {
                    return Err("Parent request is unknown in this journal; external ancestry is unsupported".into());
                };
                let ancestor: SendRequest = serde_json::from_str(&body).map_err(err)?;
                match ancestor.parent_request {
                    Some(next) => id = next,
                    None => {
                        root = Some(id);
                        break;
                    }
                }
            }
            Some(root.ok_or("Request ancestry exceeds 32 relationships")?)
        } else {
            None
        };
        let id = uuid::Uuid::now_v7().to_string();
        let tx = self.db.transaction().map_err(err)?;
        clock(&tx, now)?;
        let sender_id = actor(&tx, sender)?;
        let recipient_id = actor(&tx, recipient)?;
        tx.execute(
            "INSERT INTO requests VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                id,
                sender.conversation_key(),
                request.key,
                canonical,
                format!("{:x}", Sha256::digest(canonical.as_bytes())),
                sender_id,
                recipient_id,
                run,
                now
            ],
        )
        .map_err(err)?;
        append(
            &tx,
            &id,
            "accepted",
            json!({"sender":sender_id,"recipient":recipient_id,"root_request":root.as_ref().unwrap_or(&id),"parent_request":request.parent_request,"caller_attribution":sender.attribution,"trust":"cooperative local sender attribution; not authentication"}),
        )?;
        tx.execute(
            "INSERT INTO outbox VALUES(?1,?2,'pending',0,?3)",
            params![id, run, now],
        )
        .map_err(err)?;
        tx.commit().map_err(err)?;
        self.receipt(&id)
    }

    pub fn next(&self) -> Result<Option<Dispatch>> {
        let id: Option<String> = self.db.query_row(
            "SELECT o.request FROM outbox o JOIN requests r ON r.id=o.request WHERE o.state='pending' AND o.next_ms<=?1 AND NOT EXISTS(SELECT 1 FROM outbox b WHERE b.run=o.run AND b.state IN ('dispatching','awaiting','unconfirmed','held')) ORDER BY r.created_ms,o.request LIMIT 1",
            [now_ms()], |r| r.get(0)).optional().map_err(err)?;
        id.map(|id| self.dispatch(&id)).transpose()
    }

    pub fn dispatch(&self, id: &str) -> Result<Dispatch> {
        let (body, s, r, attempt): (String,String,String,u32) = self.db.query_row(
            "SELECT q.canonical,s.body,r.body,o.attempts FROM requests q JOIN actors s ON s.id=q.sender JOIN actors r ON r.id=q.recipient JOIN outbox o ON o.request=q.id WHERE q.id=?1", [id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).map_err(err)?;
        Ok(Dispatch {
            id: id.into(),
            envelope: serde_json::from_str(&body).map_err(err)?,
            sender: serde_json::from_str(&s).map_err(err)?,
            recipient: serde_json::from_str(&r).map_err(err)?,
            attempt,
        })
    }

    pub fn intent(&mut self, id: &str, observed: &Actor, adapter: Value) -> Result<()> {
        self.capacity()?;
        let dispatch = self.dispatch(id)?;
        if dispatch.envelope.dispatch_before_ms <= now_ms() {
            return Err("Local dispatch deadline passed".into());
        }
        if observed.run != dispatch.recipient.run {
            return Err("Recipient run changed before dispatch".into());
        }
        let tx = self.db.transaction().map_err(err)?;
        clock(&tx, now_ms())?;
        let changed = tx.execute("UPDATE outbox SET state='dispatching',attempts=attempts+1 WHERE request=?1 AND state='pending' AND attempts<3", [id]).map_err(err)?;
        if changed != 1 {
            return Err("Request is not dispatchable".into());
        }
        let snapshot = actor(&tx, observed)?;
        append(
            &tx,
            id,
            "dispatch_intent",
            json!({"attempt":dispatch.attempt+1,"attempt_id":uuid::Uuid::now_v7().to_string(),"native_request_id":Fact::observed(id,"outbound native correlation ID; receipt not established"),"recipient":snapshot,"adapter":adapter}),
        )?;
        tx.commit().map_err(err)
    }

    pub fn outcome(&mut self, id: &str, kind: &str, data: Value) -> Result<()> {
        if !matches!(
            kind,
            "native_accepted"
                | "native_held"
                | "native_refused"
                | "native_unsupported"
                | "delivery_unconfirmed"
                | "definite_nonacceptance"
                | "expired_before_dispatch"
                | "target_stale"
                | "policy_revoked"
                | "cancel_confirmed"
        ) {
            return Err("Invalid delivery outcome".into());
        }
        let tx = self.db.transaction().map_err(err)?;
        append(&tx, id, kind, data)?;
        let attempts: u32 = tx
            .query_row("SELECT attempts FROM outbox WHERE request=?1", [id], |r| {
                r.get(0)
            })
            .map_err(err)?;
        let prior: String = tx
            .query_row("SELECT state FROM outbox WHERE request=?1", [id], |r| {
                r.get(0)
            })
            .map_err(err)?;
        let state = if prior == "acknowledged" {
            "acknowledged"
        } else {
            match kind {
                "native_accepted" => "awaiting",
                "native_held" => "held",
                "delivery_unconfirmed" => "unconfirmed",
                "definite_nonacceptance" if attempts < 3 => "pending",
                _ => "terminal",
            }
        };
        let delay = if attempts == 1 { 250 } else { 1000 };
        tx.execute(
            "UPDATE outbox SET state=?2,next_ms=?3 WHERE request=?1",
            params![id, state, now_ms().saturating_add(delay)],
        )
        .map_err(err)?;
        tx.commit().map_err(err)
    }

    pub fn report(&mut self, report: &Report, who: &Actor) -> Result<Receipt> {
        let canonical = serde_json::to_string(report).map_err(err)?;
        let prior: Option<(String, u64)> = self
            .db
            .query_row(
                "SELECT canonical,event FROM replies WHERE actor=?1 AND key=?2",
                params![who.conversation_key(), report.reply_key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(err)?;
        if let Some((previous, _)) = prior {
            if previous != canonical {
                return Err("reply key conflicts with recorded payload".into());
            }
            return self.receipt(&report.request);
        }
        validate_key(&report.reply_key, now_ms())?;
        if report.text.len() > 64 * 1024
            || report.evidence.len() > 64
            || report.evidence.iter().any(|e| e.len() > 2048)
        {
            return Err("Report exceeds record limits".into());
        }
        if report.expected_run != who.run {
            return Err("Report expected_run does not match caller run".into());
        }
        let d = self.dispatch(&report.request)?;
        if !report.kind.is_resolution() && who.run != d.recipient.run {
            return Err("Recipient reports require the assigned recipient run".into());
        }
        self.capacity()?;
        let tx = self.db.transaction().map_err(err)?;
        clock(&tx, now_ms())?;
        let who_id = actor(&tx, who)?;
        let kind = if report.kind.is_resolution() {
            "resolution_reported"
        } else {
            "work_reported"
        };
        let seq = append(
            &tx,
            &report.request,
            kind,
            json!({"actor":who_id,"report":report}),
        )?;
        if report.kind.releases_credit() {
            if !report.kind.is_resolution() {
                append(
                    &tx,
                    &report.request,
                    "recipient_acknowledged",
                    json!({"actor":who_id,"scope":match &report.kind {ReportKind::Received=>"recipient-declared receipt",ReportKind::Started=>"recipient-declared work started",_=>"recipient-declared disposition; verification remains separate"},"report_event":seq}),
                )?;
            }
            tx.execute(
                "UPDATE outbox SET state='acknowledged' WHERE request=?1",
                [&report.request],
            )
            .map_err(err)?;
        }
        tx.execute(
            "INSERT INTO replies VALUES(?1,?2,?3,?4)",
            params![who.conversation_key(), report.reply_key, canonical, seq],
        )
        .map_err(err)?;
        tx.commit().map_err(err)?;
        self.receipt(&report.request)
    }

    pub fn cancel(&mut self, id: &str, who: &Actor) -> Result<Receipt> {
        self.capacity()?;
        let tx = self.db.transaction().map_err(err)?;
        let snapshot = actor(&tx, who)?;
        append(&tx, id, "cancel_requested", json!({"actor":snapshot}))?;
        let changed = tx
            .execute(
                "UPDATE outbox SET state='terminal' WHERE request=?1 AND state IN ('pending','model_held')",
                [id],
            )
            .map_err(err)?;
        if changed == 1 {
            append(
                &tx,
                id,
                "cancel_confirmed",
                json!({"scope":"local undispatched request"}),
            )?;
        } else {
            append(
                &tx,
                id,
                "cancel_unsupported",
                json!({"reason":"Native work may already be owned by the recipient. Cancellation was not confirmed."}),
            )?;
        }
        tx.commit().map_err(err)?;
        self.receipt(id)
    }

    pub fn consumption(&mut self, id: &str, observed: &Actor, evidence: Value) -> Result<()> {
        let d = self.dispatch(id)?;
        if d.recipient.run != observed.run {
            return Err("Consumption evidence came from a different run".into());
        }
        if self.receipt(id)?.recipient_ack.value().is_some() {
            return Ok(());
        }
        self.capacity()?;
        let tx = self.db.transaction().map_err(err)?;
        let snapshot = actor(&tx, observed)?;
        append(
            &tx,
            id,
            "native_accepted",
            json!({"scope":"reconciled input consumption","evidence":evidence}),
        )?;
        append(
            &tx,
            id,
            "recipient_acknowledged",
            json!({"actor":snapshot,"scope":"input consumed; responsibility and result remain separate","evidence":evidence}),
        )?;
        tx.execute(
            "UPDATE outbox SET state='acknowledged' WHERE request=?1",
            [id],
        )
        .map_err(err)?;
        tx.commit().map_err(err)
    }

    pub fn peer_output(&mut self, id: &str, observed: &Actor, evidence: Value) -> Result<()> {
        let d = self.dispatch(id)?;
        if d.recipient.run != observed.run {
            return Err("Output evidence came from a different run".into());
        }
        self.capacity()?;
        let tx = self.db.transaction().map_err(err)?;
        let snapshot = actor(&tx, observed)?;
        append(
            &tx,
            id,
            "peer_output",
            json!({"actor":snapshot,"evidence":evidence,"scope":"correlated native output; no automatic success classification"}),
        )?;
        tx.commit().map_err(err)
    }

    pub fn awaiting(&self) -> Result<Vec<Dispatch>> {
        let ids=self.db.prepare("SELECT request FROM outbox WHERE state IN ('awaiting','held','unconfirmed') ORDER BY next_ms LIMIT 128").map_err(err)?
            .query_map([],|r|r.get(0)).map_err(err)?.collect::<std::result::Result<Vec<String>,_>>().map_err(err)?;
        ids.iter().map(|id| self.dispatch(id)).collect()
    }

    pub fn needs_reconciliation(&self) -> Result<bool> {
        self.db.query_row("SELECT EXISTS(SELECT 1 FROM outbox WHERE state IN ('model_held','awaiting','held','unconfirmed'))",[],|r|r.get(0)).map_err(err)
    }

    pub fn hold_model(&mut self, id: &str) -> Result<()> {
        let tx = self.db.transaction().map_err(err)?;
        append(
            &tx,
            id,
            "model_constraint_unproven",
            json!({"reason":"Requested model constraint has no matching reported model evidence"}),
        )?;
        tx.execute(
            "UPDATE outbox SET state='model_held' WHERE request=?1 AND state='pending'",
            [id],
        )
        .map_err(err)?;
        tx.commit().map_err(err)
    }

    pub fn reconcile_models(&mut self, actors: &[Actor]) -> Result<()> {
        let ids = self
            .db
            .prepare("SELECT request FROM outbox WHERE state='model_held' LIMIT 128")
            .map_err(err)?
            .query_map([], |r| r.get(0))
            .map_err(err)?
            .collect::<std::result::Result<Vec<String>, _>>()
            .map_err(err)?;
        for id in ids {
            let d = self.dispatch(&id)?;
            if d.envelope.dispatch_before_ms <= now_ms() {
                self.outcome(
                    &id,
                    "expired_before_dispatch",
                    json!({"reason":"Model constraint stayed unproven until deadline"}),
                )?;
                continue;
            }
            let Some(observed) = actors.iter().find(|a| a.run == d.recipient.run) else {
                continue;
            };
            if observed.model.reported.value() == d.envelope.expected_model.as_ref() {
                let tx = self.db.transaction().map_err(err)?;
                let snapshot = actor(&tx, observed)?;
                append(
                    &tx,
                    &id,
                    "model_observed",
                    json!({"recipient":snapshot,"scope":"explicit model constraint satisfied"}),
                )?;
                tx.execute("UPDATE outbox SET state='pending' WHERE request=?1", [id])
                    .map_err(err)?;
                tx.commit().map_err(err)?;
            }
        }
        Ok(())
    }

    pub fn receipt(&self, id: &str) -> Result<Receipt> {
        let events = self
            .db
            .prepare("SELECT seq,request,at_ms,kind,data FROM events WHERE request=?1 ORDER BY seq")
            .map_err(err)?
            .query_map([id], row_event)
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        receipt(self.database_id.clone(), id.into(), events)
    }

    /// A transactionally consistent, credential-free projection for review/export.
    pub fn export(&mut self, id: &str) -> Result<Value> {
        let tx = self.db.transaction().map_err(err)?;
        let (body,digest,sender,recipient):(String,String,String,String)=tx.query_row(
            "SELECT q.canonical,q.digest,s.body,r.body FROM requests q JOIN actors s ON s.id=q.sender JOIN actors r ON r.id=q.recipient WHERE q.id=?1",[id],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(err)?;
        let events = tx
            .prepare("SELECT seq,request,at_ms,kind,data FROM events WHERE request=?1 ORDER BY seq")
            .map_err(err)?
            .query_map([id], row_event)
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        let snapshots=tx.prepare("SELECT id,body FROM actors WHERE id IN (SELECT json_extract(data,'$.recipient') FROM events WHERE request=?1 UNION SELECT json_extract(data,'$.actor') FROM events WHERE request=?1)").map_err(err)?
            .query_map([id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).map_err(err)?.collect::<std::result::Result<Vec<_>,_>>().map_err(err)?;
        let snapshots = snapshots
            .into_iter()
            .map(|(id, body)| {
                Ok(json!({"id":id,"actor":serde_json::from_str::<Value>(&body).map_err(err)?}))
            })
            .collect::<Result<Vec<_>>>()?;
        let result = json!({"schema":1,"database_id":self.database_id,"request":id,"envelope":serde_json::from_str::<Value>(&body).map_err(err)?,"content_sha256":digest,"sender":serde_json::from_str::<Value>(&sender).map_err(err)?,"recipient":serde_json::from_str::<Value>(&recipient).map_err(err)?,"additional_snapshots":snapshots,"receipt":receipt(self.database_id.clone(),id.into(),events)?});
        tx.commit().map_err(err)?;
        Ok(result)
    }

    pub fn export_to(&mut self, id: &str, path: &Path) -> Result<()> {
        use std::{io::Write, os::unix::fs::OpenOptionsExt};
        let export = self.export(id)?;
        let mut f = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(err)?;
        f.write_all(
            serde_json::to_string_pretty(&export)
                .map_err(err)?
                .as_bytes(),
        )
        .map_err(err)?;
        f.sync_all().map_err(err)
    }

    pub fn page(&self, cursor: Option<&str>, limit: usize) -> Result<Value> {
        let sequence = if let Some(cursor) = cursor {
            let (database, sequence) = cursor.rsplit_once(':').ok_or("Invalid cursor")?;
            if database != self.database_id {
                return Err("Cursor belongs to a different journal; coverage is unknown".into());
            }
            sequence.parse::<u64>().map_err(err)?
        } else {
            0
        };
        let limit = limit.clamp(1, 256);
        let rows = self
            .db
            .prepare(
                "SELECT seq,request,at_ms,kind,data FROM events WHERE seq>?1 ORDER BY seq LIMIT ?2",
            )
            .map_err(err)?
            .query_map(params![sequence, limit], row_event)
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        let last = rows.last().map(|e| e.sequence).unwrap_or(sequence);
        let high: u64 = self
            .db
            .query_row("SELECT IFNULL(MAX(seq),0) FROM events", [], |r| r.get(0))
            .map_err(err)?;
        if sequence > high {
            return Err("Cursor is beyond journal coverage".into());
        }
        Ok(
            json!({"database_id":self.database_id,"events":rows,"next_cursor":format!("{}:{last}",self.database_id),"high_water":high,"complete_through":last,"has_more":last<high,"retention":"No automatic deletion"}),
        )
    }

    fn capacity(&self) -> Result<()> {
        let bytes: u64 = self.db.query_row("SELECT (SELECT IFNULL(SUM(length(data)+length(kind)+64),0) FROM events)+(SELECT IFNULL(SUM(length(canonical)+256),0) FROM requests)+(SELECT IFNULL(SUM(length(body)+64),0) FROM actors)+(SELECT IFNULL(SUM(length(canonical)+64),0) FROM replies)",[],|r|r.get(0)).map_err(err)?;
        if bytes >= 256 * 1024 * 1024 {
            return Err("Management logical record budget exhausted; export records before accepting more work".into());
        }
        Ok(())
    }
}

fn actor(tx: &Transaction<'_>, value: &Actor) -> Result<String> {
    let body = serde_json::to_string(value).map_err(err)?;
    let id = format!("{:x}", Sha256::digest(body.as_bytes()));
    tx.execute(
        "INSERT OR IGNORE INTO actors VALUES(?1,?2)",
        params![id, body],
    )
    .map_err(err)?;
    Ok(id)
}

fn clock(tx: &Transaction<'_>, now: u64) -> Result<()> {
    let high: Option<String> = tx
        .query_row(
            "SELECT value FROM metadata WHERE key='clock_high_water_ms'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(err)?;
    let high = high
        .map(|s| s.parse::<u64>().map_err(err))
        .transpose()?
        .unwrap_or(now);
    if now.saturating_add(300_000) < high {
        return Err("Wall clock moved backwards more than five minutes; mutations paused".into());
    }
    tx.execute("INSERT INTO metadata VALUES('clock_high_water_ms',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[now.max(high).to_string()]).map_err(err)?;
    Ok(())
}

fn append(tx: &Transaction<'_>, id: &str, kind: &str, data: Value) -> Result<u64> {
    let data = serde_json::to_string(&data).map_err(err)?;
    if data.len() > 128 * 1024 {
        return Err("Event exceeds 128 KiB".into());
    }
    tx.execute(
        "INSERT INTO events(request,at_ms,kind,data) VALUES(?1,?2,?3,?4)",
        params![id, now_ms(), kind, data],
    )
    .map_err(err)?;
    Ok(tx.last_insert_rowid() as u64)
}

fn row_event(r: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    let data: String = r.get(4)?;
    let data: Value = serde_json::from_str(&data).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(Event {
        sequence: r.get(0)?,
        request: r.get(1)?,
        at_ms: r.get(2)?,
        source_time: match data
            .get("source_time")
            .or_else(|| data.pointer("/evidence/source_time"))
            .filter(|v| !v.is_null())
        {
            Some(value) => Fact::Observed {
                value: value.clone(),
                source: "native source timestamp".into(),
                at_ms: r.get(2)?,
            },
            None => Fact::Unknown {
                reason: "No source timestamp was supplied".into(),
            },
        },
        kind: r.get(3)?,
        data,
    })
}
