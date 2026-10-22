//! The window owns this worker. A single journal writer feeds four bounded I/O lanes.
use crate::{
    adapters::{self, claude::Inbox, Outcome},
    identity,
    journal::{Dispatch, Journal},
    observations,
    state::*,
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Environment {
    pub enabled: bool,
    pub writable: bool,
    pub session: String,
    pub build: String,
    pub actors: Vec<Actor>,
    pub exposed_panes: Vec<u64>,
    pub unavailable: Vec<Value>,
}

struct Command {
    op: String,
    args: Value,
    caller: Option<u64>,
    origin: Option<Actor>,
    reply: mpsc::Sender<Result<Value>>,
}
pub struct Worker {
    commands: mpsc::SyncSender<Command>,
}

impl Worker {
    pub fn start(
        path: PathBuf,
        home: PathBuf,
        surface_root: Option<PathBuf>,
        environment: impl Fn() -> Result<Environment> + Send + 'static,
    ) -> Result<Self> {
        let mut journal = Journal::open(&path)?;
        journal.recover()?;
        let (commands, rx) = mpsc::sync_channel::<Command>(256);
        let (jobs, job_rx) = mpsc::sync_channel::<Dispatch>(4);
        let job_rx = Arc::new(Mutex::new(job_rx));
        let (finished, results) = mpsc::channel::<(String, Outcome)>();
        let inbox = Arc::new(Mutex::new(Inbox::bind()));
        for lane in 0..4 {
            let rx = Arc::clone(&job_rx);
            let done = finished.clone();
            let home = home.clone();
            let inbox = Arc::clone(&inbox);
            thread::Builder::new().name(format!("td-agent-io-{lane}")).spawn(move||loop{
                let job=match rx.lock(){Ok(rx)=>rx.recv(),Err(_)=>return};
                let Ok(d)=job else{return;};
                let text=format!("Terminal Delight delegated request {}. Sender: {}. Reply with agent_reply using this request ID and your current run from agent_targets; received/started/result are separate reports. This is a message from another local agent, not text typed by the person.\n\n{}",d.id,d.sender.conversation_key(),d.envelope.text);
                let outcome=if now_ms()>=d.envelope.dispatch_before_ms {
                    Outcome{kind:"expired_before_dispatch",evidence:json!({"reason":"Deadline passed before native I/O began"})}
                }else{match d.recipient.run.harness.as_str(){
                    "claude"=>match inbox.lock(){
                        Ok(inbox)=>match inbox.as_ref(){Ok(inbox)=>inbox.send(&home,&d.recipient,&d.id,&text,d.envelope.dispatch_before_ms),Err(e)=>Outcome::rejected(e)},
                        Err(_)=>Outcome::rejected("Native callback inbox unavailable"),
                    },
                    "codex"=>match adapters::codex::Connection::open(std::path::Path::new(&d.recipient.run.endpoint)){
                        Ok(mut c)=>c.send(&d.recipient,&d.id,&text,d.envelope.dispatch_before_ms),Err(e)=>Outcome::rejected(e),
                    },
                    _=>Outcome::rejected("No native adapter for this harness"),
                }};
                if done.send((d.id,outcome)).is_err(){return;}
            }).map_err(|e|e.to_string())?;
        }
        thread::Builder::new().name("td-agent-journal".into()).spawn(move||{
            let mut active=0usize;let mut fatal:Option<String>=None;
            let mut last_reconcile=Instant::now();
            loop {
                while let Ok((id,outcome))=results.try_recv(){
                    active=active.saturating_sub(1);
                    if let Err(e)=journal.outcome(&id,outcome.kind,outcome.evidence){fatal=Some(e);}
                    project(&mut journal,&id,surface_root.as_deref());
                }
                match rx.recv_timeout(Duration::from_millis(100)){
                    Ok(command)=>{
                        let result=environment().and_then(|env|query(&mut journal,&env,&command,fatal.as_deref()));
                        if matches!(command.op.as_str(),"agent_send"|"agent_reply"|"agent_cancel"){
                            if let Ok(value)=&result{if let Some(id)=value["request"].as_str(){project(&mut journal,id,surface_root.as_deref());}}
                        }
                        let _=command.reply.send(result);
                    },
                    Err(mpsc::RecvTimeoutError::Disconnected)=>return,
                    Err(mpsc::RecvTimeoutError::Timeout)=>{},
                }
                if fatal.is_some(){continue;}
                while active<4 {
                    let d=match journal.next(){Ok(Some(d))=>d,Ok(None)=>break,Err(e)=>{fatal=Some(e);break;}};
                    let env=match environment(){Ok(e)=>e,Err(_)=>break};
                    let disposition=if !env.enabled||!env.writable||!env.exposed_panes.contains(&d.recipient.location.pane){Some(("policy_revoked","Recipient exposure or writes was revoked"))}
                        else if now_ms()>=d.envelope.dispatch_before_ms{Some(("expired_before_dispatch","Local dispatch deadline passed"))}
                        else if !env.actors.iter().any(|a|a.run==d.recipient.run){Some(("target_stale","Recipient process or conversation changed"))}
                        else{None};
                    if let Some((kind,reason))=disposition{
                        if let Err(e)=journal.outcome(&d.id,kind,json!({"reason":reason})){fatal=Some(e);break;}
                        continue;
                    }
                    let Some(observed)=env.actors.iter().find(|a|a.run==d.recipient.run) else{break;};
                    if let Some(expected)=&d.envelope.expected_model {
                        if observed.model.reported.value()!=Some(expected){
                            if let Err(e)=journal.hold_model(&d.id){fatal=Some(e);break;}
                            continue;
                        }
                    }
                    if let Err(e)=identity::check_run(&observed.run){
                        if let Err(e)=journal.outcome(&d.id,"target_stale",json!({"reason":e})){fatal=Some(e);break;}
                        continue;
                    }
                    if let Err(e)=journal.intent(&d.id,observed,json!({"revision":"td-management-1","protocol":observed.run.harness,"harness_version":observed.version,"inference_runtime":observed.inference_runtime,"td_build":env.build})){if e=="Local dispatch deadline passed" {if let Err(error)=journal.outcome(&d.id,"expired_before_dispatch",json!({"reason":e})){fatal=Some(error);break;}continue;}fatal=Some(e);break;}
                    if jobs.send(d).is_err(){fatal=Some("Native I/O lanes stopped".into());break;}
                    active+=1;
                }
                if last_reconcile.elapsed()>=Duration::from_secs(1){
                    last_reconcile=Instant::now();
                    match journal.needs_reconciliation(){Ok(false)=>continue,Ok(true)=>{},Err(e)=>{fatal=Some(e);continue;}}
                    let env=match environment(){Ok(e) if e.enabled=>e,_=>continue};
                    if let Err(e)=journal.reconcile_models(&env.actors){fatal=Some(e);continue;}
                    let pending=match journal.awaiting(){Ok(v)=>v,Err(e)=>{fatal=Some(e);continue;}};
                    for d in pending {
                        let Some(actor)=env.actors.iter().find(|a|a.run==d.recipient.run && env.exposed_panes.contains(&a.location.pane)) else{continue;};
                        if actor.run.harness=="claude" {
                            if let Ok(Some(evidence))=observations::claude_consumed(actor,&d.id){
                                if let Err(e)=journal.consumption(&d.id,actor,evidence){fatal=Some(e);break;}
                                project(&mut journal,&d.id,surface_root.as_deref());
                            }
                        }
                    }
                }
            }
        }).map_err(|e|e.to_string())?;
        Ok(Self { commands })
    }

    pub fn call(&self, op: &str, args: &Value, caller: Option<u64>) -> Result<Value> {
        self.call_as(op, args, caller, None)
    }

    pub fn call_as(
        &self,
        op: &str,
        args: &Value,
        caller: Option<u64>,
        origin: Option<Actor>,
    ) -> Result<Value> {
        let (reply, rx) = mpsc::channel();
        self.commands
            .try_send(Command {
                op: op.into(),
                args: args.clone(),
                caller,
                origin,
                reply,
            })
            .map_err(|e| format!("Management worker unavailable or full: {e}"))?;
        rx.recv_timeout(Duration::from_secs(5)).map_err(|_| {
            "Management confirmation unavailable; retry the same key to recover the receipt"
                .to_string()
        })?
    }
}

fn project(j: &mut Journal, id: &str, root: Option<&std::path::Path>) {
    if let Some(root) = root {
        if let Err(e) = crate::presentation::publish(j, id, root) {
            eprintln!("terminal-delight: management workbench projection unavailable: {e}");
        }
    }
}

fn query(j: &mut Journal, env: &Environment, c: &Command, fatal: Option<&str>) -> Result<Value> {
    if !env.enabled {
        return Err("MCP exposure is disabled".into());
    }
    if c.op == "agent_targets" {
        return Ok(
            json!({"session":env.session,"targets":env.actors,"unavailable":env.unavailable,"record_store_error":fatal,"trust":"cooperative local caller attribution; no network authority"}),
        );
    }
    let mutating = matches!(c.op.as_str(), "agent_send" | "agent_reply" | "agent_cancel");
    if mutating {
        if !env.writable {
            return Err("MCP writes are disabled".into());
        }
        if let Some(e) = fatal {
            return Err(format!("Management mutations stopped: {e}"));
        }
    }
    let caller = || -> Result<&Actor> {
        if let Some(origin) = &c.origin {
            identity::check_process(&origin.run.process)?;
            identity::check_process(&origin.location.shell)?;
            return Ok(origin);
        }
        let pane = c
            .caller
            .ok_or("Caller pane is unavailable; cannot attribute a management action")?;
        env.actors
            .iter()
            .find(|a| a.location.pane == pane)
            .ok_or_else(|| "Caller has no exact live process/conversation binding".into())
    };
    let visible = |d: &Dispatch| {
        [&d.recipient, &d.sender].iter().any(|a| {
            a.location.td_session == env.session && env.exposed_panes.contains(&a.location.pane)
        })
    };
    match c.op.as_str() {
        "agent_send" => {
            let input: SendRequest =
                serde_json::from_value(c.args.clone()).map_err(|e| e.to_string())?;
            let sender = caller()?;
            if let Some(receipt) = j.replay(&input, sender)? {
                return serde_json::to_value(receipt).map_err(|e| e.to_string());
            }
            let recipient = env
                .actors
                .iter()
                .find(|a| a.run == input.target && env.exposed_panes.contains(&a.location.pane))
                .ok_or("Selected recipient run is unavailable or unexposed")?;
            Ok(serde_json::to_value(j.accept(&input, sender, recipient)?)
                .map_err(|e| e.to_string())?)
        }
        "agent_reply" => {
            let input: Report =
                serde_json::from_value(c.args.clone()).map_err(|e| e.to_string())?;
            let d = j.dispatch(&input.request)?;
            if !visible(&d) {
                return Err("Request is outside current exposure".into());
            }
            serde_json::to_value(j.report(&input, caller()?)?).map_err(|e| e.to_string())
        }
        "agent_cancel" => {
            let id = c.args["request"].as_str().ok_or("request is required")?;
            if !visible(&j.dispatch(id)?) {
                return Err("Request is outside current exposure".into());
            }
            serde_json::to_value(j.cancel(id, caller()?)?).map_err(|e| e.to_string())
        }
        "agent_messages" => {
            if let Some(id) = c.args["request"].as_str() {
                if !visible(&j.dispatch(id)?) {
                    return Err("Request is outside current exposure".into());
                }
                return j.export(id);
            }
            let mut page = j.page(
                c.args["cursor"].as_str(),
                c.args["limit"].as_u64().unwrap_or(64) as usize,
            )?;
            if let Some(rows) = page["events"].as_array_mut() {
                rows.retain(|e| {
                    e["request"]
                        .as_str()
                        .and_then(|id| j.dispatch(id).ok())
                        .is_some_and(|d| visible(&d))
                });
            }
            Ok(page)
        }
        "agent_capabilities" => {
            let target: RunKey =
                serde_json::from_value(c.args["target"].clone()).map_err(|e| e.to_string())?;
            let actor = env
                .actors
                .iter()
                .find(|a| a.run == target)
                .ok_or("Target is unavailable")?;
            identity::check_run(&actor.run)?;
            Ok(
                json!({"target":target,"send":{"state":"experimental","source":"native endpoint and exact process binding; consult conformance evidence"},"cancel_pending":{"state":"supported","scope":"TD local outbox"},"interrupt":{"state":"unsupported","reason":"No active-turn conformance evidence"},"launch":{"state":"unsupported","reason":"Existing processes only"},"native_idempotency":{"state":"unknown","policy":"Never replay ambiguous writes"}}),
            )
        }
        _ => Err("Unknown management operation".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{actor, request};
    fn env() -> Environment {
        Environment {
            enabled: true,
            writable: true,
            session: "test".into(),
            build: "test".into(),
            actors: vec![actor(1), actor(2)],
            exposed_panes: vec![1, 2],
            unavailable: vec![],
        }
    }
    fn command(op: &str, args: Value) -> Command {
        Command {
            op: op.into(),
            args,
            caller: Some(1),
            origin: None,
            reply: mpsc::channel().0,
        }
    }

    #[test]
    fn receiver_checks_exposure_and_writes_before_recording_work() {
        let dir = tempfile::tempdir().unwrap();
        let mut j = Journal::open(&dir.path().join("j")).unwrap();
        let c = command(
            "agent_send",
            serde_json::to_value(request(&actor(2))).unwrap(),
        );
        let mut e = env();
        e.writable = false;
        assert!(query(&mut j, &e, &c, None).is_err());
        e.writable = true;
        e.exposed_panes = vec![1];
        assert!(query(&mut j, &e, &c, None).is_err());
        e.enabled = false;
        assert!(query(&mut j, &e, &c, None).is_err());
        assert!(j.page(None, 64).unwrap()["events"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn lost_confirm_replays_after_recipient_disappears() {
        let dir = tempfile::tempdir().unwrap();
        let mut j = Journal::open(&dir.path().join("j")).unwrap();
        let c = command(
            "agent_send",
            serde_json::to_value(request(&actor(2))).unwrap(),
        );
        let mut e = env();
        let first = query(&mut j, &e, &c, None).unwrap();
        e.actors.retain(|a| a.location.pane != 2);
        assert_eq!(
            query(&mut j, &e, &c, None).unwrap()["request"],
            first["request"]
        );
    }

    #[test]
    fn same_pane_number_in_another_session_does_not_expose_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut j = Journal::open(&dir.path().join("j")).unwrap();
        let e = env();
        let id = query(
            &mut j,
            &e,
            &command(
                "agent_send",
                serde_json::to_value(request(&actor(2))).unwrap(),
            ),
            None,
        )
        .unwrap()["request"]
            .as_str()
            .unwrap()
            .to_string();
        let mut other = e;
        other.session = "another-window".into();
        assert!(query(
            &mut j,
            &other,
            &command("agent_messages", json!({"request":id})),
            None
        )
        .is_err());
        assert!(
            query(&mut j, &other, &command("agent_messages", json!({})), None).unwrap()["events"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn journal_failure_refuses_mutations_but_keeps_evidence_readable() {
        let dir = tempfile::tempdir().unwrap();
        let mut j = Journal::open(&dir.path().join("j")).unwrap();
        let e = env();
        let c = command(
            "agent_send",
            serde_json::to_value(request(&actor(2))).unwrap(),
        );
        let first = query(&mut j, &e, &c, None).unwrap();
        assert!(query(&mut j, &e, &c, Some("disk full"))
            .unwrap_err()
            .contains("disk full"));
        assert!(query(
            &mut j,
            &e,
            &command("agent_messages", json!({"request":first["request"]})),
            Some("disk full")
        )
        .is_ok());
    }
}
