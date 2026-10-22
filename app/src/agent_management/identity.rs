use crate::state::*;
use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;

pub fn process(pid: u32) -> Result<ProcessKey> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).map_err(|e| e.to_string())?;
    let (_, fields) = stat.rsplit_once(") ").ok_or("Malformed process stat")?;
    let start_ticks = fields
        .split_whitespace()
        .nth(19)
        .ok_or("Missing process start ticks")?
        .parse::<u64>()
        .map_err(|e| e.to_string())?;
    let boot_id = fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .map_err(|e| e.to_string())?
        .trim()
        .to_owned();
    Ok(ProcessKey {
        boot_id,
        pid,
        start_ticks,
    })
}

pub fn check_process(key: &ProcessKey) -> Result<()> {
    if process(key.pid)? != *key {
        return Err("Process exited or PID was reused".into());
    }
    Ok(())
}

pub fn socket_generation(path: &Path) -> Result<String> {
    let m = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !m.file_type().is_socket() || m.uid() != unsafe { libc::geteuid() } {
        return Err("Native endpoint is not a same-user Unix socket".into());
    }
    Ok(format!(
        "{}:{}:{}:{}",
        m.dev(),
        m.ino(),
        m.ctime(),
        m.ctime_nsec()
    ))
}

pub fn check_run(run: &RunKey) -> Result<()> {
    check_process(&run.process)?;
    if socket_generation(Path::new(&run.endpoint))? != run.endpoint_generation {
        return Err("Native endpoint generation changed".into());
    }
    Ok(())
}

pub fn executable(pid: u32) -> Fact<String> {
    match fs::read_link(format!("/proc/{pid}/exe")) {
        Ok(p) => Fact::observed(p.to_string_lossy().into_owned(), format!("/proc/{pid}/exe")),
        Err(e) => Fact::Unknown {
            reason: e.to_string(),
        },
    }
}

/// The caller supplies a conversation already bound by explicit process evidence.
pub fn codex(home: &Path, location: Location, pid: u32, conversation: &str) -> Result<Actor> {
    let endpoint = home
        .join(".codex/app-server-control/app-server-control.sock")
        .canonicalize()
        .map_err(|_| "Existing Codex app server endpoint unavailable; bootstrap forbidden")?;
    Ok(Actor{run:RunKey{harness:"codex".into(),conversation:conversation.into(),process:process(pid)?,endpoint:endpoint.to_string_lossy().into_owned(),endpoint_generation:socket_generation(&endpoint)?},location,executable:executable(pid),version:Fact::Unknown{reason:"Running TUI version has not been measured; conversation creation version is not the current process version".into()},inference_runtime:Fact::Unknown{reason:"Connect-only probe supplies separate daemon identity per attempt".into()},model:ModelFacts::default(),attribution: Fact::NotCollected, binding_evidence:"Explicit live process/conversation binding supplied by TD".into(),transcript:Fact::NotCollected,observed_at_ms:now_ms()})
}

/// Resolve a Claude inbox only from the selected process's own registry record.
pub fn claude(home: &Path, location: Location, pid: u32, conversation: &str) -> Result<Actor> {
    let path = home.join(".claude/sessions").join(format!("{pid}.json"));
    let body = fs::read(&path).map_err(|e| e.to_string())?;
    if body.len() > 32 * 1024 {
        return Err("Claude session record exceeds limit".into());
    }
    let meta: serde_json::Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    let process = process(pid)?;
    if meta["pid"].as_u64() != Some(u64::from(pid))
        || meta["sessionId"].as_str() != Some(conversation)
        || meta["procStart"].as_str() != Some(process.start_ticks.to_string().as_str())
    {
        return Err("Claude registry does not match the live process and conversation".into());
    }
    if meta["peerProtocol"].as_u64() != Some(1) || meta["version"].as_str() != Some("2.1.296") {
        return Err("Claude native protocol/version has not passed TD conformance".into());
    }
    let endpoint = meta["messagingSocketPath"]
        .as_str()
        .ok_or("Claude has no native inbox")?
        .to_owned();
    let expected = format!("{}.sock", pid);
    let endpoint_path = Path::new(&endpoint);
    if endpoint_path.file_name().and_then(|s| s.to_str()) != Some(&expected)
        || endpoint_path.parent()
            != Some(Path::new(&format!("/run/user/{}/cc-socks", unsafe {
                libc::geteuid()
            })))
    {
        return Err("Custom Claude inbox namespace is not supported by this adapter".into());
    }
    let endpoint_generation = socket_generation(endpoint_path)?;
    Ok(Actor {
        run: RunKey {
            harness: "claude".into(),
            conversation: conversation.into(),
            process,
            endpoint,
            endpoint_generation,
        },
        location,
        executable: executable(pid),
        version: Fact::observed("2.1.296".into(), path.to_string_lossy()),
        inference_runtime: Fact::Unknown {
            reason: "Claude exposes no separate local inference runtime identity".into(),
        },
        model: ModelFacts::default(),
        attribution: Fact::NotCollected,
        binding_evidence: path.to_string_lossy().into_owned(),
        transcript: Fact::NotCollected,
        observed_at_ms: now_ms(),
    })
}
