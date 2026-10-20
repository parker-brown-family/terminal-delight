use std::{env, path::Path};
use td_agent_management::{journal::Journal, presentation};
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("Expected JOURNAL REQUEST STATE_ROOT".into());
    }
    let mut j = Journal::open(Path::new(&args[0]))?;
    presentation::publish(&mut j, &args[1], Path::new(&args[2]))?;
    println!("Published recorded relationship and evidence timeline");
    Ok(())
}
