use sha2::{Digest, Sha256};
use std::{env, path::PathBuf};
use td_agent_management::{identity, journal::Journal, state::*};
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = env::args().skip(1).collect();
    if a.len() != 4 {
        return Err("Expected JOURNAL REQUEST MARKDOWN EXPORT".into());
    }
    let mut j = Journal::open(&PathBuf::from(&a[0]))?;
    let sender = j.dispatch(&a[1])?.sender;
    identity::check_process(&sender.run.process)?;
    let body = std::fs::read_to_string(&a[2])?;
    if body.trim().is_empty()
        || body.trim().split("\n\n").count() != 1
        || body.trim().starts_with('#')
        || body.len() > 1000
    {
        return Err("File is not one short paragraph without a heading".into());
    }
    let report=Report{request:a[1].clone(),reply_key:uuid::Uuid::now_v7().to_string(),expected_run:sender.run.clone(),kind:ReportKind::Verified,text:format!("Read the {}-byte UTF-8 output. Exactly one nonempty paragraph, no heading. SHA-256 {:x}.",body.len(),Sha256::digest(body.as_bytes())),evidence:vec![a[2].clone()]};
    j.report(&report, &sender)?;
    let export = j.export(&a[1])?;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&a[3])?;
    f.write_all(serde_json::to_string_pretty(&export)?.as_bytes())?;
    f.sync_all()?;
    println!(
        "Verified {} bytes, one paragraph; evidence export {}",
        body.len(),
        a[3]
    );
    Ok(())
}
