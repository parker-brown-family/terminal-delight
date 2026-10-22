use serde_json::json;
use std::{env, path::PathBuf};
use td_agent_management::adapters::codex::Connection;
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let home = env::var("HOME")?;
    let path = PathBuf::from(home)
        .join(".codex/app-server-control/app-server-control.sock")
        .canonicalize()?;
    let mut c = Connection::open(&path)?;
    println!(
        "{}",
        json!({"endpoint":path,"runtime":c.runtime,"initialize":c.initialize})
    );
    let list = c.call("thread/loaded/list", json!({"limit":256}))?;
    println!("{list}");
    if let Some(thread) = env::args().nth(1) {
        let v = c.call("thread/queue/list", json!({"threadId":thread}))?;
        println!("queue response {v}");
    }
    for id in list["data"].as_array().ok_or("missing loaded list")? {
        let v = c.call("thread/read", json!({"threadId":id,"includeTurns":false}))?;
        println!(
            "{}",
            json!({"id":id,"cwd":v.pointer("/thread/cwd"),"status":v.pointer("/thread/status"),"model":v.pointer("/thread/model"),"version":v.pointer("/thread/cliVersion")})
        );
    }
    Ok(())
}
