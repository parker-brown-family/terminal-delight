//! A rebuildable workbench projection, never a second record store.
use crate::{journal::Journal, state::*};
use serde_json::{json, Value};
use std::{io::Write, os::unix::fs::OpenOptionsExt, path::Path};

pub fn objects(j: &mut Journal, id: &str) -> Result<Vec<Value>> {
    let export = j.export(id)?;
    let d = j.dispatch(id)?;
    let receipt = j.receipt(id)?;
    let actor_label = |a: &Actor| format!("{} · pane {}", a.run.harness, a.location.pane);
    let model = |a: &Actor| {
        a.model
            .reported
            .value()
            .or_else(|| a.model.requested.value())
            .map(|id| {
                if a.model.reported.value().is_some() {
                    format!("reported: {id}")
                } else {
                    format!("requested: {id}; response ID unavailable")
                }
            })
            .unwrap_or_else(|| "model unavailable".into())
    };
    let process = |a: &Actor| {
        format!(
            "process {} · harness {}",
            a.run.process.pid,
            a.version
                .value()
                .map(String::as_str)
                .unwrap_or("version unavailable")
        )
    };
    let acknowledgement = if receipt.recipient_ack.value().is_some() {
        "input acknowledged"
    } else {
        "input acknowledgement unavailable"
    };
    let result = receipt
        .events
        .iter()
        .rev()
        .find(|e| matches!(e.kind.as_str(), "resolution_reported" | "work_reported"))
        .and_then(|e| e.data.pointer("/report/kind"))
        .and_then(Value::as_str)
        .unwrap_or("work result unavailable");
    let title = d
        .envelope
        .text
        .lines()
        .next()
        .unwrap_or("Agent assignment")
        .chars()
        .take(90)
        .collect::<String>();
    let graph = json!({"td":"0.4","id":format!("agent-request-{id}"),"kind":"architecture","title":title,"model":{
        "nodes":[
            {"id":"sender","label":actor_label(&d.sender),"state":model(&d.sender),"group":process(&d.sender)},
            {"id":"request","label":"Recorded assignment","state":receipt.native_delivery,"group":acknowledgement},
            {"id":"recipient","label":actor_label(&d.recipient),"state":model(&d.recipient),"group":process(&d.recipient)},
            {"id":"outcome","label":"Work and verification","state":result}
        ],"edges":[{"from":"sender","to":"request","label":"requested"},{"from":"request","to":"recipient","label":"assigned to this run"},{"from":"recipient","to":"outcome","label":"evidence retained separately"}]}});
    let mut rows = vec![
        json!(["Request", id]),
        json!(["Text", d.envelope.text]),
        json!(["Native delivery", receipt.native_delivery]),
        json!(["Consumer evidence", acknowledgement]),
    ];
    for e in &receipt.events {
        rows.push(json!([
            format!("{} · {}", e.sequence, e.kind),
            serde_json::to_string(&e.data).map_err(|e| e.to_string())?
        ]));
    }
    rows.push(json!([
        "Record source",
        format!(
            "journal {} · immutable events",
            export["database_id"].as_str().unwrap_or("unavailable")
        )
    ]));
    let timeline = json!({"td":"0.4","id":format!("agent-evidence-{id}"),"kind":"table","title":"Assignment receipts and evidence","model":{"columns":["Observation","Evidence"],"rows":rows}});
    Ok(vec![graph, timeline])
}

pub fn publish(j: &mut Journal, id: &str, state_root: &Path) -> Result<()> {
    let d = j.dispatch(id)?;
    crate::identity::check_process(&d.sender.location.shell)?;
    let dir = state_root
        .join("terminal-delight/surfaces")
        .join(&d.sender.location.td_session)
        .join(d.sender.location.pane.to_string());
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for (n, object) in objects(j, id)?.into_iter().enumerate() {
        let path = dir.join(format!("agent-management-{id}-{n}.json"));
        let tmp = path.with_extension("tmp");
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        f.write_all(
            serde_json::to_string(&object)
                .map_err(|e| e.to_string())?
                .as_bytes(),
        )
        .map_err(|e| e.to_string())?;
        drop(f);
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
