use std::{env, path::Path};
use td_agent_management::{adapters::codex::Connection, journal::Journal};
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = env::args().skip(1).collect();
    if a.len() != 3 {
        return Err("Expected JOURNAL REQUEST EXPORT".into());
    }
    let mut j = Journal::open(Path::new(&a[0]))?;
    let actor = j.dispatch(&a[1])?.recipient;
    let mut c = Connection::open(Path::new(&actor.run.endpoint))?;
    let evidence = c
        .observe(&actor, &a[1])?
        .ok_or("No native user-message correlation found in bounded turn page")?;
    j.consumption(&a[1], &actor, evidence.clone())?;
    j.peer_output(&a[1], &actor, evidence)?;
    j.export_to(&a[1], Path::new(&a[2]))?;
    println!("{}", serde_json::to_string_pretty(&j.receipt(&a[1])?)?);
    Ok(())
}
