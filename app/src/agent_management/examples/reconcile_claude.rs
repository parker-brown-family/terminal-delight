use std::{env, path::PathBuf};
use td_agent_management::{journal::Journal, observations, state::*};

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = env::args().skip(1).collect();
    if a.len() != 4 {
        return Err("Expected JOURNAL REQUEST TRANSCRIPT EXPORT".into());
    }
    let mut j = Journal::open(&PathBuf::from(&a[0]))?;
    let mut recipient = j.dispatch(&a[1])?.recipient;
    recipient.transcript = Fact::observed(
        a[2].clone(),
        "Explicit conformance path; session ID checked in record",
    );
    let evidence =
        observations::claude_consumed(&recipient, &a[1])?.ok_or("No exact consumption evidence")?;
    j.consumption(&a[1], &recipient, evidence)?;
    let v = j.export(&a[1])?;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&a[3])?;
    f.write_all(serde_json::to_string_pretty(&v)?.as_bytes())?;
    f.sync_all()?;
    println!("{}", serde_json::to_string_pretty(&j.receipt(&a[1])?)?);
    Ok(())
}
