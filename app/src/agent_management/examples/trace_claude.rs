//! Explicit live conformance run. All actor/process arguments are required.
//! cargo run --example trace_claude -- JOURNAL HOME MANAGER_PID MANAGER_SHELL MANAGER_SESSION TARGET_PID TARGET_SHELL TARGET_SESSION OUTPUT EXPORT
use serde_json::json;
use std::{env, path::PathBuf};
use td_agent_management::{adapters::claude::Inbox, identity, journal::Journal, state::*};

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = env::args().skip(1).collect();
    if a.len() != 10 {
        return Err("Expected ten explicit conformance arguments".into());
    }
    let home = PathBuf::from(&a[1]);
    let manager_pid: u32 = a[2].parse()?;
    let shell: u32 = a[3].parse()?;
    let target: u32 = a[5].parse()?;
    let target_shell: u32 = a[6].parse()?;
    let location = |pane, shell| -> Result<Location> {
        Ok(Location {
            td_session: "1".into(),
            pane,
            shell: identity::process(shell)?,
        })
    };
    let endpoint = home
        .join(".codex/app-server-control/app-server-control.sock")
        .canonicalize()?;
    let manager=Actor{run:RunKey{harness:"codex".into(),conversation:a[4].clone(),process:identity::process(manager_pid)?,endpoint:endpoint.to_string_lossy().into_owned(),endpoint_generation:identity::socket_generation(&endpoint)?},location:location(41,shell)?,executable:identity::executable(manager_pid),version:Fact::NotCollected,inference_runtime:Fact::NotCollected,model:ModelFacts::default(),attribution: Fact::NotCollected, binding_evidence:"Explicit live conformance arguments supplied by managing session; cooperative attribution".into(),transcript:Fact::NotCollected,observed_at_ms:now_ms()};
    let recipient = identity::claude(&home, location(42, target_shell)?, target, &a[7])?;
    let mut journal = Journal::open(&PathBuf::from(&a[0]))?;
    journal.recover()?;
    let key = uuid::Uuid::now_v7().to_string();
    let text=format!("Parker asked the managing agent in pane 41 to send this assignment to your existing process in pane 42 (task 1). Request {key}. Please create {} containing exactly one short paragraph of Lorem ipsum, with no heading. Keep it quick. Then reply with the output path and this request ID. This is the single task, not a request to start an agent. If this is not the task 1 pane, decline it.",a[8]);
    let request = SendRequest {
        key,
        target: recipient.run.clone(),
        text,
        dispatch_before_ms: now_ms() + 600_000,
        parent_request: None,
        caused_by_message: None,
        expected_model: None,
        experiment: Some("original-markdown-assignment".into()),
    };
    let receipt = journal.accept(&request, &manager, &recipient)?;
    journal.intent(&receipt.request,&recipient,json!({"revision":"claude-peer-1-r1","vendor_version":recipient.version,"td_build":"conformance harness; window integration pending"}))?;
    let inbox = Inbox::bind()?;
    let outcome = inbox.send(
        &home,
        &recipient,
        &receipt.request,
        &request.text,
        request.dispatch_before_ms,
    );
    journal.outcome(&receipt.request, outcome.kind, outcome.evidence)?;
    let export = journal.export(&receipt.request)?;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&a[9])?;
    f.write_all(serde_json::to_string_pretty(&export)?.as_bytes())?;
    f.sync_all()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&journal.receipt(&receipt.request)?)?
    );
    Ok(())
}
