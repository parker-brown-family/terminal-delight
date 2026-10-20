//! One live assignment to an already-loaded Codex process. No daemon bootstrap.
use serde_json::json;
use std::{
    env,
    path::{Path, PathBuf},
};
use td_agent_management::{adapters::codex::Connection, identity, journal::Journal, state::*};
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = env::args().skip(1).collect();
    if a.len() != 10 {
        return Err("Expected JOURNAL HOME MANAGER_PID MANAGER_SHELL MANAGER_SESSION TARGET_PID TARGET_SHELL TARGET_SESSION OUTPUT EXPORT".into());
    }
    let home = PathBuf::from(&a[1]);
    let location = |pane, shell| -> Result<Location> {
        Ok(Location {
            td_session: "1".into(),
            pane,
            shell: identity::process(shell)?,
        })
    };
    let sender = identity::codex(&home, location(41, a[3].parse()?)?, a[2].parse()?, &a[4])?;
    let mut recipient = identity::codex(&home, location(16, a[6].parse()?)?, a[5].parse()?, &a[7])?;
    let mut c = Connection::open(Path::new(&recipient.run.endpoint))?;
    if !c.loaded(&recipient.run.conversation)? {
        return Err("Selected target is not already loaded".into());
    }
    let thread = c.call(
        "thread/read",
        json!({"threadId":recipient.run.conversation,"includeTurns":false}),
    )?;
    if thread
        .pointer("/thread/status/type")
        .and_then(serde_json::Value::as_str)
        != Some("idle")
    {
        return Err("Conformance target is busy; do not steer it".into());
    }
    recipient.inference_runtime = Fact::observed(
        json!({"process":c.runtime,"initialize":c.initialize}),
        "Existing app server handshake over same-user UDS",
    );
    if let Some(model) = thread
        .pointer("/thread/model")
        .and_then(serde_json::Value::as_str)
    {
        recipient.model.requested = Fact::observed(
            model.into(),
            "thread/read model configuration; provider response ID unavailable",
        );
    }
    let text=format!("Parker authorized building Terminal Delight management of existing agents, and this is one conformance assignment to the already-running Codex pane 16. Please create {} containing one short Lorem ipsum paragraph, no heading, then report the path here. Keep it quick. We already delivered the original assignment to the existing Claude in pane 42 and verified its file. Implementation is isolated in /home/parker/Work/td-agent-management, branch feat/agent-management. This message is for that loaded process only; if this is the wrong pane, decline. No installation or merge is requested.",a[8]);
    let request = SendRequest {
        key: uuid::Uuid::now_v7().to_string(),
        target: recipient.run.clone(),
        text,
        dispatch_before_ms: now_ms() + 600_000,
        parent_request: None,
        caused_by_message: None,
        expected_model: None,
        experiment: Some("existing-codex-native-queue".into()),
    };
    let mut j = Journal::open(&PathBuf::from(&a[0]))?;
    j.recover()?;
    let receipt = j.accept(&request, &sender, &recipient)?;
    j.intent(&receipt.request,&recipient,json!({"revision":"codex-queue-r1","runtime":recipient.inference_runtime,"td_build":"conformance harness; window integration pending"}))?;
    let outcome = c.send(
        &recipient,
        &receipt.request,
        &format!("TD request {}.\n{}", receipt.request, request.text),
        request.dispatch_before_ms,
    );
    j.outcome(&receipt.request, outcome.kind, outcome.evidence)?;
    j.export_to(&receipt.request, Path::new(&a[9]))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&j.receipt(&receipt.request)?)?
    );
    Ok(())
}
