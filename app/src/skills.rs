//! Global, additive skills and evidence from the current agent invocation.
//!
//! The provider bridge is shipped here rather than installed into a person's
//! provider config. The host exposes it to shells; the launcher uses it too.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const BRIDGE: &str = include_str!("skills/bridge.py");
const FACTORY: &str = include_str!("skills/software-factory.md");
pub const PROVIDERS: [&str; 5] = ["claude", "codex", "gemini", "opencode", "crush"];

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub active: Option<bool>,
    pub briefing: Option<bool>,
    pub factory: Option<bool>,
    pub sources: Vec<Source>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub origin: String,
    pub revision: String,
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl Source {
    pub fn target(&self) -> Option<String> {
        self.url.clone().or_else(|| {
            if self.origin.starts_with("https://") {
                Some(self.origin.clone())
            } else {
                self.path
                    .is_absolute()
                    .then(|| crate::docview::snapshot::file_url(&self.path))
            }
        })
    }
}

pub const BUNDLED_SOURCE: &str = "https://github.com/parker-brown-family/terminal-delight";

impl Prefs {
    pub fn active(&self) -> bool {
        self.active == Some(true)
    }
    pub fn briefing(&self) -> bool {
        self.briefing != Some(false)
    }
    pub fn factory(&self) -> bool {
        self.factory != Some(false)
    }

    /// An explicit import refreshes this source; keep the person's switch.
    pub fn import_sources(&mut self, sources: Vec<Source>) {
        for mut source in sources {
            if let Some(existing) = self.sources.iter_mut().find(|s| {
                s.path == source.path || (s.origin == source.origin && s.name == source.name)
            }) {
                source.enabled = existing.enabled;
                *existing = source;
            } else {
                self.sources.push(source);
            }
        }
    }
}

pub fn config_path() -> PathBuf {
    crate::instance::config_dir().join("skills.json")
}

pub fn load() -> Result<Prefs, String> {
    load_from(&config_path())
}

/// Legacy launcher/session briefings use the same global switch as the bridge.
pub fn briefing_root() -> Option<PathBuf> {
    let prefs = load().ok()?;
    (prefs.active() && prefs.briefing())
        .then(crate::briefkit::default_root)
        .flatten()
}

fn load_from(path: &Path) -> Result<Prefs, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Prefs::default()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn save(prefs: &Prefs) -> Result<(), String> {
    let json = serde_json::to_string_pretty(prefs).map_err(|e| e.to_string())?;
    crate::session::write_atomic(&config_path(), &json).map_err(|e| e.to_string())
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn data_root() -> Option<PathBuf> {
    crate::briefkit::default_root().and_then(|p| p.parent().map(|p| p.join("skill-manager")))
}

/// Version the helper and its entire bundle, so an older session host cannot
/// overwrite a newer window's source while an agent is using it.
pub fn install() -> io::Result<PathBuf> {
    static INSTALLED: std::sync::OnceLock<Result<PathBuf, String>> = std::sync::OnceLock::new();
    INSTALLED
        .get_or_init(|| install_inner().map_err(|e| e.to_string()))
        .clone()
        .map_err(io::Error::other)
}

fn install_inner() -> io::Result<PathBuf> {
    use std::os::unix::fs::{symlink, PermissionsExt};
    if !std::process::Command::new("python3")
        .arg("--version")
        .output()?
        .status
        .success()
    {
        return Err(io::Error::other("TD skills requires Python 3"));
    }
    let mut hash = Sha256::new();
    hash.update(BRIDGE.as_bytes());
    hash.update(FACTORY.as_bytes());
    for file in crate::briefkit::FILES {
        hash.update(file.path.as_bytes());
        hash.update(file.bytes);
    }
    let key = format!("{:x}", hash.finalize());
    let root = data_root()
        .ok_or_else(|| io::Error::other("no data directory for TD skills"))?
        .join(&key[..20]);
    std::fs::create_dir_all(root.join("bin"))?;
    let helper = root.join("bridge.py");
    if !helper.exists() {
        crate::session::write_atomic(&helper, BRIDGE)?;
    }
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755))?;
    crate::briefkit::write(&root.join("bundle"))?;
    let factory = root.join("bundle/software-factory/SKILL.md");
    if !factory.exists() {
        crate::session::write_atomic(&factory, FACTORY)?;
    }
    for provider in PROVIDERS {
        let path = root.join("bin").join(provider);
        if !path.exists() {
            match symlink("../bridge.py", &path) {
                Err(e) if e.kind() != io::ErrorKind::AlreadyExists => return Err(e),
                _ => {}
            }
        }
    }
    crate::session::write_atomic(&root.join("activate.bash"), &bash_activation(&root))?;
    Ok(root)
}

/// For a person's own startup file, on a host older than the bridge. `function`
/// and not `name()`, which an alias of that name turns into a syntax error that
/// ends the script before the remaining providers.
fn bash_activation(root: &Path) -> String {
    let quote = crate::launcher::shell_quote;
    let mut script = format!(
        "# TD shell integration; personal aliases and functions retain priority.\nexport TD_SKILLS_CONFIG={}\nexport TD_SKILLS_SESSION=\"$TD_SESSION\" TD_SKILLS_PANE_ID=\"$TD_PANE_ID\"\n",
        quote(&config_path().to_string_lossy())
    );
    for provider in PROVIDERS {
        let shim = quote(&root.join("bin").join(provider).to_string_lossy());
        script.push_str(&format!(
            "if ! declare -F {provider} >/dev/null || [[ $(declare -f {provider}) == *'/skill-manager/'* ]]; then\n  function {provider} {{ {shim} \"$@\"; }}\n  export -f {provider}\nfi\n"
        ));
    }
    script
}

/// Add discovery to a newly spawned shell without editing its rc files. Bash
/// functions survive PATH initialization (including mise); an alias or explicit
/// binary still belongs to its person and may bypass the bridge.
pub fn shell_environment(env: &mut HashMap<String, String>) -> io::Result<()> {
    let root = install()?;
    let bin = root.join("bin");
    let path = env
        .get("PATH")
        .cloned()
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default();
    env.insert("PATH".into(), format!("{}:{path}", bin.display()));
    env.insert(
        "TD_SKILLS_CONFIG".into(),
        config_path().display().to_string(),
    );
    // A host's explicit pane identity supersedes one inherited from the
    // terminal that happened to start the host.
    if let (Some(session), Some(pane)) = (env.get("TD_SESSION"), env.get("TD_PANE_ID")) {
        let (session, pane) = (session.clone(), pane.clone());
        env.insert("TD_SKILLS_SESSION".into(), session);
        env.insert("TD_SKILLS_PANE_ID".into(), pane);
    }
    for provider in PROVIDERS {
        let key = format!("BASH_FUNC_{provider}%%");
        if env
            .get(&key)
            .cloned()
            .or_else(|| std::env::var(&key).ok())
            .is_some_and(|body| !body.contains("/skill-manager/"))
        {
            continue;
        }
        let shim = crate::launcher::shell_quote(&bin.join(provider).to_string_lossy());
        env.insert(key, format!("() {{ {shim} \"$@\"; }}"));
    }
    Ok(())
}

/// Quick windows own their PTYs, with no host pane number. Give only the skill
/// receipt its own namespace; never borrow the launching agent's pane identity.
pub type LocalEnvironment = (Vec<(String, String)>, Option<(String, u64)>);

pub fn local_environment() -> LocalEnvironment {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let identity = (
        format!("local-{}", std::process::id()),
        NEXT.fetch_add(1, Ordering::Relaxed),
    );
    let mut env = HashMap::from([
        ("TD_SKILLS_SESSION".into(), identity.0.clone()),
        ("TD_SKILLS_PANE_ID".into(), identity.1.to_string()),
    ]);
    if let Err(error) = shell_environment(&mut env) {
        eprintln!("TD skill command bridge unavailable: {error}");
    }
    (env.into_iter().collect(), Some(identity))
}

/// Launcher and typed commands enter the same bridge. No shell replacement of
/// arguments: only the known first program token is changed.
pub fn launch_line(line: String, provider: &str) -> String {
    if !matches!(provider, "claude" | "codex") {
        return line;
    }
    match install() {
        Ok(root) => line.strip_prefix(provider).map_or_else(
            || line.clone(),
            |rest| {
                format!(
                    "{}{rest}",
                    crate::launcher::shell_quote(
                        &root.join("bin").join(provider).to_string_lossy()
                    )
                )
            },
        ),
        Err(e) => {
            eprintln!("TD skills command bridge unavailable: {e}");
            line
        }
    }
}

/// The launch line a sleeping Claude wakes through, read from the one it ran.
///
/// The bridge starts Claude as `claude --add-dir <run> --settings
/// <run>/settings.json …`, where `<run>` is a folder under the receipts'
/// `runs/` named for that process, and the next launch prunes it once the
/// process has ended. Sleep ends it, so replaying those words names a folder
/// that is gone, and Claude will not start: "Settings file not found", for all
/// seven bridged sleepers on 2026-11-07. The wake starts through the bridge
/// again instead, which makes a new folder, and carries inline whatever the
/// person's own `--settings` had put in the old file beside the bridge's hook:
/// the settings the agent was running with, since the bridge had already read
/// any file of theirs when it started. A line the bridge did not write comes
/// back unchanged.
pub fn wake_launch_line(argv: &[String]) -> Result<Vec<String>, String> {
    let shim = install().ok().map(|root| root.join("bin/claude"));
    unbridged(argv, shim.as_deref())
}

/// The longest single word Linux will pass to a program (`MAX_ARG_STRLEN`,
/// counting its terminating NUL).
const LONGEST_WORD: usize = 32 * 4096;

/// A folder the bridge made for one launch: `<pid>-<random>` directly inside
/// `terminal-delight/skill-receipts/runs`, the shape `clean_run` in bridge.py
/// accepts. Matched by shape rather than against this window's state root,
/// because a pane may set its own `XDG_STATE_HOME` before it starts Claude.
fn bridge_run(run: &Path) -> bool {
    let name = run.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let shaped = name.split_once('-').is_some_and(|(pid, random)| {
        !pid.is_empty()
            && pid.bytes().all(|b| b.is_ascii_digit())
            && !random.is_empty()
            && random
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    });
    shaped
        && run
            .parent()
            .is_some_and(|p| p.ends_with("terminal-delight/skill-receipts/runs"))
}

fn unbridged(argv: &[String], shim: Option<&Path>) -> Result<Vec<String>, String> {
    let [program, add, run, set, settings, rest @ ..] = argv else {
        return Ok(argv.to_vec());
    };
    let run = Path::new(run);
    if add != "--add-dir"
        || set != "--settings"
        || !bridge_run(run)
        || Path::new(settings) != run.join("settings.json")
    {
        return Ok(argv.to_vec());
    }
    // Read while the agent still runs, so its folder is still there. Unread,
    // the person's settings are unknown, and a wake without them is not theirs.
    let text = std::fs::read_to_string(settings)
        .map_err(|e| format!("the skill bridge's settings for this agent: {e}"))?;
    let mut supplied: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("the skill bridge's settings for this agent: {e}"))?;
    let run_text = run.to_string_lossy();
    // The bridge's own acknowledgement names a file in the old folder.
    let acknowledgement = |entry: &serde_json::Value| {
        entry["hooks"].as_array().is_some_and(|hooks| {
            hooks.iter().any(|hook| {
                hook["command"]
                    .as_str()
                    .is_some_and(|c| c.contains(run_text.as_ref()))
            })
        })
    };
    if let Some(object) = supplied.as_object_mut() {
        if let Some(hooks) = object.get_mut("hooks").and_then(|h| h.as_object_mut()) {
            if let Some(start) = hooks.get_mut("SessionStart").and_then(|s| s.as_array_mut()) {
                start.retain(|entry| !acknowledgement(entry));
                if start.is_empty() {
                    hooks.remove("SessionStart");
                }
            }
            if hooks.is_empty() {
                object.remove("hooks");
            }
        }
    }
    let program = shim.map_or_else(|| program.clone(), |s| s.to_string_lossy().into_owned());
    let mut line = vec![program];
    if supplied.as_object().is_some_and(|o| !o.is_empty()) {
        let carried = supplied.to_string();
        if carried.len() >= LONGEST_WORD {
            return Err(format!(
                "this agent's settings are {} bytes, too long to carry into a wake line",
                carried.len()
            ));
        }
        line.extend(["--settings".to_owned(), carried]);
    }
    line.extend(rest.iter().cloned());
    Ok(line)
}

/// The launch line a sleeping Codex wakes through, read from the one it ran.
///
/// The bridge runs the Codex TUI as `codex --remote unix://<run>/codex.sock …`
/// against an app-server it owns, in a folder pruned once the bridge has
/// ended. Replaying that socket names a server that is gone, so the wake
/// starts through the bridge again, which opens a new one (#1386). A line the
/// bridge did not write comes back unchanged.
pub fn codex_wake_launch_line(argv: &[String]) -> Vec<String> {
    let shim = install().ok().map(|root| root.join("bin/codex"));
    codex_unbridged(argv, shim.as_deref())
}

/// `unix://<run>/codex.sock` for a folder the bridge made, the endpoint
/// `run_codex` in bridge.py opens.
fn bridge_socket(value: &str) -> bool {
    value
        .strip_prefix("unix://")
        .map(Path::new)
        .is_some_and(|socket| {
            socket.file_name().is_some_and(|n| n == "codex.sock")
                && socket.parent().is_some_and(bridge_run)
        })
}

fn codex_unbridged(argv: &[String], shim: Option<&Path>) -> Vec<String> {
    let [program, remote, socket, rest @ ..] = argv else {
        return argv.to_vec();
    };
    if remote != "--remote" || !bridge_socket(socket) {
        return argv.to_vec();
    }
    let program = shim.map_or_else(|| program.clone(), |s| s.to_string_lossy().into_owned());
    let mut line = vec![program];
    line.extend(rest.iter().cloned());
    line
}

/// A wake line written before wakes went back through the bridge, repaired at
/// the moment it is needed: `Some` with the line to run instead, or `None`
/// when the line still starts as written.
///
/// A window older than the fix replays a bridged agent's launch line word for
/// word. Once the folder those words name is pruned, Claude answers "Settings
/// file not found" and a Codex finds no server. The repaired line starts
/// through the bridge, which makes a new folder. Any settings of the person's
/// that lived only in the pruned file are gone by then; the bridge's cleanup
/// keeps a folder a wake script names, so this is the last resort, not the
/// usual path.
pub fn rebridge_lost(argv: &[String]) -> Option<Vec<String>> {
    let root = install().ok();
    rebridge_lost_with(argv, root.as_deref())
}

fn rebridge_lost_with(argv: &[String], root: Option<&Path>) -> Option<Vec<String>> {
    let through = |name: &str| {
        root.map_or_else(
            || argv[0].clone(),
            |r| r.join("bin").join(name).to_string_lossy().into_owned(),
        )
    };
    match argv {
        [_, add, run, set, settings, rest @ ..]
            if add == "--add-dir"
                && set == "--settings"
                && bridge_run(Path::new(run))
                && Path::new(settings) == Path::new(run).join("settings.json")
                && !Path::new(run).exists() =>
        {
            Some(
                std::iter::once(through("claude"))
                    .chain(rest.iter().cloned())
                    .collect(),
            )
        }
        [_, remote, socket, rest @ ..]
            if remote == "--remote"
                && bridge_socket(socket)
                && socket
                    .strip_prefix("unix://")
                    .and_then(|s| Path::new(s).parent())
                    .is_some_and(|run| !run.exists()) =>
        {
            Some(
                std::iter::once(through("codex"))
                    .chain(rest.iter().cloned())
                    .collect(),
            )
        }
        _ => None,
    }
}

/// Run only on a worker: repository fetch and folder enumeration must not
/// block a native render or input event.
pub fn import(source: &str, folder: &str, revision: &str) -> Result<Vec<Source>, String> {
    let root = install().map_err(|e| e.to_string())?;
    let output = std::process::Command::new("python3")
        .arg(root.join("bridge.py"))
        .args(["import", source, folder, revision])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(1000)
            .collect());
    }
    serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Attached,
    Partial,
    NotAttached,
    #[default]
    Unknown,
}

impl Status {
    pub fn label(self, ui: &crate::lang::Chrome) -> &'static str {
        match self {
            Self::Attached => ui.skill_attached,
            Self::Partial => ui.skill_partial,
            Self::NotAttached => ui.skill_absent,
            Self::Unknown => ui.skill_unknown,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Missing {
    pub name: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Receipt {
    pub provider: String,
    pub status: Status,
    pub pid: u32,
    pub start_tick: Option<u64>,
    pub revision: String,
    pub run: String,
    pub available: Option<Vec<Source>>,
    pub missing: Vec<Missing>,
    pub reason: Option<String>,
    pub method: Option<String>,
    pub agent_session: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Info {
    pub status: Status,
    pub pending_restart: bool,
    pub receipt: Option<Receipt>,
    pub bridge_ready: Option<bool>,
}

fn state_root() -> PathBuf {
    let root = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| crate::session::home_dir().join(".local/state"));
    root.join("terminal-delight/skill-receipts")
}

fn receipt_path(session: &str, pane: u64) -> PathBuf {
    state_root()
        .join(&digest(session.as_bytes())[..24])
        .join(format!("{pane}.json"))
}

fn proc_fields(stat: &str) -> Option<(u64, i32)> {
    let fields: Vec<_> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    Some((fields.get(19)?.parse().ok()?, fields.get(5)?.parse().ok()?))
}

fn identity_matches(
    receipt: &Receipt,
    tick: Option<u64>,
    group: Option<i32>,
    foreground: Option<i32>,
) -> bool {
    receipt.start_tick.is_some()
        && receipt.start_tick == tick
        && group.is_some_and(|g| g > 0 && Some(g) == foreground)
}

fn measured_status(receipt: &Receipt) -> Status {
    match receipt.status {
        Status::Attached | Status::Partial => match &receipt.available {
            Some(available) if !available.is_empty() => {
                if receipt.missing.is_empty() {
                    Status::Attached
                } else {
                    Status::Partial
                }
            }
            _ => Status::Unknown,
        },
        other => other,
    }
}

fn read_info(session: &str, pane: u64, shell: u32) -> Info {
    let read = || -> Option<Info> {
        let path = receipt_path(session, pane);
        let meta = std::fs::metadata(&path).ok()?;
        if meta.len() > 256 * 1024 {
            return None;
        }
        let receipt: Receipt = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        let tick = std::fs::read_to_string(format!("/proc/{}/stat", receipt.pid))
            .ok()
            .as_deref()
            .and_then(proc_fields)
            .map(|f| f.0);
        let foreground = std::fs::read_to_string(format!("/proc/{shell}/stat"))
            .ok()
            .as_deref()
            .and_then(proc_fields)
            .map(|f| f.1);
        let group = unsafe { libc::getpgid(receipt.pid as i32) };
        if !identity_matches(&receipt, tick, (group > 0).then_some(group), foreground) {
            return None;
        }
        let revision = match std::fs::read(config_path()) {
            Ok(bytes) => digest(&bytes),
            Err(e) if e.kind() == io::ErrorKind::NotFound => "absent".into(),
            Err(_) => return None,
        };
        // A success requires a nonempty measured catalog/context. A malformed
        // success cannot turn unknown into zero or a missing list into proof.
        let status = measured_status(&receipt);
        Some(Info {
            status,
            pending_restart: receipt.revision != revision,
            receipt: Some(receipt),
            bridge_ready: Some(true),
        })
    };
    read().unwrap_or_else(|| {
        let foreground = std::fs::read_to_string(format!("/proc/{shell}/stat"))
            .ok()
            .as_deref()
            .and_then(proc_fields)
            .map(|v| v.1);
        let bridge_ready = foreground
            .filter(|pid| *pid > 0)
            .and_then(|pid| std::fs::read(format!("/proc/{pid}/environ")).ok())
            .map(|bytes| {
                bytes
                    .split(|b| *b == 0)
                    .any(|part| part.starts_with(b"TD_SKILLS_CONFIG="))
            });
        Info {
            bridge_ready,
            ..Info::default()
        }
    })
}

#[derive(Default)]
pub struct Memo {
    checked: Option<Instant>,
    pub info: Info,
}

impl Memo {
    pub fn refresh(
        &mut self,
        session: Option<&str>,
        pane: Option<u64>,
        shell: Option<u32>,
    ) -> bool {
        if self
            .checked
            .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
        {
            return false;
        }
        self.checked = Some(Instant::now());
        let info = match (session, pane, shell) {
            (Some(session), Some(pane), Some(shell)) => read_info(session, pane, shell),
            _ => Info::default(),
        };
        let changed = self.info != info;
        self.info = info;
        changed
    }
}

pub fn run_cli(args: &[String]) -> i32 {
    let result = (|| -> Result<(), String> {
        match args.first().map(String::as_str) {
            Some("on" | "off") => {
                let mut prefs = load()?;
                prefs.active = Some(args[0] == "on");
                save(&prefs)?;
            }
            Some("import") => {
                let source = args
                    .get(1)
                    .ok_or("skills import needs a repository or local folder")?;
                let sources = import(
                    source,
                    args.get(2).map_or("", String::as_str),
                    args.get(3).map_or("HEAD", String::as_str),
                )?;
                let mut prefs = load()?;
                prefs.import_sources(sources);
                save(&prefs)?;
            }
            Some("path") => println!("{}", config_path().display()),
            Some("status") | None => println!(
                "{}",
                serde_json::to_string_pretty(&load()?).map_err(|e| e.to_string())?
            ),
            _ => {
                return Err(
                    "skills [status|on|off|path|import SOURCE [SUBFOLDER [REVISION]]]".into(),
                )
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("terminal-delight skills: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> Receipt {
        serde_json::from_value(serde_json::json!({
            "provider":"codex", "status":"attached", "pid":42, "start_tick":123,
            "revision":"r", "run":"one", "available":[], "missing":[]
        }))
        .unwrap()
    }

    #[test]
    fn unchosen_is_distinct_from_inactive_and_bad_config_is_an_error() {
        let prefs = Prefs::default();
        assert_eq!(prefs.active, None);
        assert!(!prefs.active());
        assert!(prefs.briefing() && prefs.factory());
        assert!(serde_json::from_str::<Prefs>(r#"{"active":"true"}"#).is_err());
        assert_eq!(serde_json::from_str::<Prefs>("{}").unwrap(), prefs);
    }

    #[test]
    fn a_reused_pid_a_missing_tick_or_another_foreground_cannot_claim_attachment() {
        let receipt = example();
        assert!(identity_matches(&receipt, Some(123), Some(42), Some(42)));
        assert!(!identity_matches(&receipt, Some(124), Some(42), Some(42)));
        assert!(!identity_matches(&receipt, None, Some(42), Some(42)));
        assert!(!identity_matches(&receipt, Some(123), Some(42), Some(43)));
        assert!(!identity_matches(&receipt, Some(123), None, Some(42)));
    }

    #[test]
    fn proc_identity_handles_spaces_and_parentheses_in_the_command() {
        let mut fields = vec!["S"; 20];
        fields[5] = "58";
        fields[19] = "123";
        assert_eq!(
            proc_fields(&format!("42 (my (agent)) {}", fields.join(" "))),
            Some((123, 58))
        );
        assert_eq!(proc_fields("42 (broken) S"), None);
    }

    fn source(revision: &str, path: &str) -> Source {
        Source {
            name: "one".into(),
            description: "A skill".into(),
            path: path.into(),
            origin: "https://fixture.invalid/skills".into(),
            revision: revision.into(),
            enabled: Some(true),
            url: None,
        }
    }

    #[test]
    fn importing_an_update_preserves_the_persons_selection_and_replaces_only_its_source() {
        let mut prefs = Prefs::default();
        let mut old = source("old", "/old/one");
        old.enabled = Some(false);
        prefs.sources.push(old);
        prefs.import_sources(vec![source("new", "/new/one")]);
        assert_eq!(prefs.sources.len(), 1);
        assert_eq!(prefs.sources[0].revision, "new");
        assert_eq!(prefs.sources[0].enabled, Some(false));
    }

    #[test]
    fn source_links_keep_browser_origins_and_encode_local_paths() {
        let mut skill = source("local", "/tmp/a skill/#one");
        skill.origin = "/tmp/a skill".into();
        assert_eq!(
            skill.target().as_deref(),
            Some("file:///tmp/a%20skill/%23one")
        );
        skill.url = Some("https://example.invalid/repo/tree/revision/skill".into());
        assert_eq!(skill.target(), skill.url);
        let old = serde_json::to_value(source("old", "/tmp/skill")).unwrap();
        assert!(
            old.get("url").is_none(),
            "old configurations remain readable"
        );
    }

    #[test]
    fn shell_activation_preserves_personal_functions_and_aliases() {
        let root = install().unwrap();
        let output = std::process::Command::new("/bin/bash")
            .args(["--noprofile", "--norc", "-c"])
            .arg(format!(
                "shopt -s expand_aliases\nalias gemini='printf aliased'\nclaude() {{ printf personal; }}\n. {}\nclaude\ngemini\ndeclare -f crush",
                crate::launcher::shell_quote(&root.join("activate.bash").to_string_lossy())
            ))
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("TD_SESSION", "shell-test")
            .env("TD_PANE_ID", "5")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stderr), "");
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.starts_with("personalaliased"));
        assert!(text.contains("/skill-manager/") && text.contains("/bin/crush"));
    }

    #[test]
    fn success_needs_measured_skills_and_missing_selections_are_partial() {
        let mut value = example();
        assert_eq!(measured_status(&value), Status::Unknown);
        value.available = None;
        assert_eq!(measured_status(&value), Status::Unknown);
        value.available = Some(vec![source("one", "/one")]);
        assert_eq!(measured_status(&value), Status::Attached);
        value.missing.push(Missing {
            name: "two".into(),
            reason: "excluded".into(),
        });
        assert_eq!(measured_status(&value), Status::Partial);
        value.status = Status::Unknown;
        assert_eq!(measured_status(&value), Status::Unknown);
    }

    #[test]
    fn an_ordinary_bash_command_routes_even_after_path_initialization() {
        let _fork = crate::testsync::forks_and_locks();
        let dir = std::env::temp_dir().join(format!("td-skills-bash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("skills.json");
        std::fs::write(&config, r#"{"active":false}"#).unwrap();
        let fake = dir.join("provider.py");
        std::fs::write(
            &fake,
            "#!/usr/bin/python3\nimport json,sys\nprint(json.dumps(sys.argv[1:]))\nsys.exit(27)\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut env = HashMap::from([("PATH".into(), "/usr/bin:/bin".into())]);
        shell_environment(&mut env).unwrap();
        assert!(env["PATH"].ends_with(":/usr/bin:/bin"));
        let output = std::process::Command::new("/bin/bash")
            .args([
                "--noprofile",
                "--norc",
                "-c",
                "PATH=/usr/bin:/bin; claude --model 'user model' '' '$(touch should-not-exist)'",
            ])
            .envs(env)
            .env("TD_SKILLS_CONFIG", config)
            .env("TD_SKILLS_REAL_CLAUDE", fake)
            .env("TD_SESSION", "bash-fixture")
            .env("TD_PANE_ID", "1")
            .env("TD_SKILLS_SESSION", "bash-fixture")
            .env("TD_SKILLS_PANE_ID", "1")
            .env("XDG_STATE_HOME", dir.join("state"))
            .current_dir(&dir)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(27),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let args: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            args,
            ["--model", "user model", "", "$(touch should-not-exist)"]
        );
        assert!(!dir.join("should-not-exist").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn host_and_quick_window_receipts_never_borrow_the_launching_pane() {
        let mut host = HashMap::from([
            ("TD_SESSION".into(), "host-fixture".into()),
            ("TD_PANE_ID".into(), "7".into()),
            ("TD_SKILLS_SESSION".into(), "someone-else".into()),
            ("TD_SKILLS_PANE_ID".into(), "9".into()),
        ]);
        shell_environment(&mut host).unwrap();
        assert_eq!(host["TD_SKILLS_SESSION"], "host-fixture");
        assert_eq!(host["TD_SKILLS_PANE_ID"], "7");
        let (env, first) = local_environment();
        let (_, second) = local_environment();
        assert_ne!(first, second);
        let first = first.unwrap();
        assert!(env.contains(&("TD_SKILLS_SESSION".into(), first.0)));
        assert!(env.contains(&("TD_SKILLS_PANE_ID".into(), first.1.to_string())));
    }

    const SLEEPER: &str = "f5ae3d94-e006-45e5-9ec4-93b1934624c2";
    const SHIM: &str = "/data/terminal-delight/skill-manager/k/bin/claude";

    /// A state folder of its own for one test, which is not this window's:
    /// the run is recognised by its shape wherever the pane's state lives.
    fn state(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("td-wake-{tag}-{}", std::process::id()))
    }

    fn run_in(state: &Path) -> PathBuf {
        state.join("terminal-delight/skill-receipts/runs/790289-omeil8k1")
    }

    /// The bridge's acknowledgement hook, as it writes one for `run`.
    fn acknowledgement(run: &Path) -> serde_json::Value {
        serde_json::json!({"hooks": [{"type": "command", "timeout": 10, "command":
            format!("python3 /data/skill-manager/k/bridge.py acknowledge {}/ack.json", run.display())}]})
    }

    /// A run folder as the bridge leaves it, with `settings` in its file, and
    /// the launch line the bridge gave the Claude it started there.
    fn bridged(run: &Path, settings: Option<serde_json::Value>) -> Vec<String> {
        std::fs::create_dir_all(run).unwrap();
        if let Some(settings) = settings {
            std::fs::write(run.join("settings.json"), settings.to_string()).unwrap();
        }
        vec![
            "/home/p/.local/share/mise/installs/claude/latest/claude".to_owned(),
            "--add-dir".to_owned(),
            run.to_string_lossy().into_owned(),
            "--settings".to_owned(),
            run.join("settings.json").to_string_lossy().into_owned(),
            "--model".to_owned(),
            "opus".to_owned(),
        ]
    }

    #[test]
    fn a_bridged_sleeper_wakes_through_the_bridge_and_names_no_old_folder() {
        let state = state("plain");
        let run = run_in(&state);
        let ack = acknowledgement(&run);
        let line = bridged(
            &run,
            Some(serde_json::json!({"hooks": {"SessionStart": [ack]}})),
        );
        let woke = unbridged(&line, Some(Path::new(SHIM))).unwrap();
        assert_eq!(woke, [SHIM, "--model", "opus"]);
        let wake = crate::agentrest::wake_argv(&woke, SLEEPER).unwrap();
        assert_eq!(wake, [SHIM, "--model", "opus", "--resume", SLEEPER]);
        // The folder the line named is pruned once the agent has ended; the
        // wake must not depend on it.
        std::fs::remove_dir_all(&state).unwrap();
        let script = crate::agentrest_io::wake_script(&wake, None);
        assert!(!script.contains(&*run.to_string_lossy()), "{script}");
    }

    #[test]
    fn the_persons_own_settings_survive_the_wake_and_the_bridges_hook_does_not() {
        let state = state("own");
        let run = run_in(&state);
        let ack = acknowledgement(&run);
        let mine = serde_json::json!({"hooks": [{"type": "command", "command": "my-start-hook"}]});
        let stop = serde_json::json!({"hooks": [{"type": "command", "command": "my-stop-hook"}]});
        let line = bridged(
            &run,
            Some(
                serde_json::json!({"model": "personal", "permissions": {"deny": ["Read(secret)"]},
                "hooks": {"SessionStart": [mine, ack], "Stop": [stop]}}),
            ),
        );
        let woke = unbridged(&line, Some(Path::new(SHIM))).unwrap();
        assert_eq!(woke.len(), 5, "{woke:?}");
        assert_eq!((woke[0].as_str(), woke[1].as_str()), (SHIM, "--settings"));
        assert_eq!(&woke[3..], ["--model", "opus"]);
        let carried: serde_json::Value = serde_json::from_str(&woke[2]).unwrap();
        assert_eq!(
            carried,
            serde_json::json!({"model": "personal", "permissions": {"deny": ["Read(secret)"]},
                "hooks": {"SessionStart": [mine], "Stop": [stop]}})
        );
        assert!(crate::agentrest::wake_argv(&woke, SLEEPER).is_ok());
        std::fs::remove_dir_all(&state).unwrap();
    }

    #[test]
    fn a_run_path_json_must_escape_still_loses_the_bridges_hook() {
        let state = state("quote\"and\\slash");
        let run = run_in(&state);
        let ack = acknowledgement(&run);
        let line = bridged(
            &run,
            Some(serde_json::json!({"hooks": {"SessionStart": [ack]}})),
        );
        assert_eq!(
            unbridged(&line, Some(Path::new(SHIM))).unwrap(),
            [SHIM, "--model", "opus"]
        );
        std::fs::remove_dir_all(&state).unwrap();
    }

    #[test]
    fn a_line_the_bridge_did_not_write_wakes_word_for_word() {
        let state = state("other");
        let shim = Some(Path::new(SHIM));
        let plain: Vec<String> = ["claude", "--model", "opus"].map(String::from).to_vec();
        assert_eq!(unbridged(&plain, shim).unwrap(), plain);
        // Folders that are not one launch's run folder, each with a readable
        // settings file, so only the shape can tell them apart.
        let runs = state.join("terminal-delight/skill-receipts/runs");
        for folder in [
            state.join("notes"),
            runs.join("manual"),
            runs.join("790289-"),
            runs.join("790289-omeil8k1/nested-1"),
            state.join("elsewhere/runs/790289-omeil8k1"),
        ] {
            let theirs = bridged(&folder, Some(serde_json::json!({"model": "theirs"})));
            assert_eq!(unbridged(&theirs, shim).unwrap(), theirs, "{folder:?}");
        }
        // The bridge's folder, but not the bridge's pair of words.
        let run = run_in(&state).to_string_lossy().into_owned();
        let half: Vec<String> = vec![
            "claude".into(),
            "--add-dir".into(),
            run,
            "-c".into(),
            "x".into(),
        ];
        assert_eq!(unbridged(&half, shim).unwrap(), half);
        std::fs::remove_dir_all(&state).unwrap();
    }

    /// Rest refuses sleep on any error here (`agentrest::may_sleep`, tested
    /// there), so these are agents that stay awake.
    #[test]
    fn a_bridged_sleeper_whose_settings_cannot_be_carried_gets_no_wake_line() {
        let state = state("gone");
        let run = run_in(&state);
        let line = bridged(&run, None);
        let err = unbridged(&line, Some(Path::new(SHIM))).unwrap_err();
        assert!(err.contains("skill bridge"), "{err}");
        std::fs::write(run.join("settings.json"), "not json").unwrap();
        assert!(unbridged(&line, Some(Path::new(SHIM))).is_err());
        // Too long for one word of a command line.
        let long = "x".repeat(LONGEST_WORD);
        bridged(&run, Some(serde_json::json!({"env": {"LONG": long}})));
        let err = unbridged(&line, Some(Path::new(SHIM))).unwrap_err();
        assert!(err.contains("too long"), "{err}");
        std::fs::remove_dir_all(&state).unwrap();
    }

    #[test]
    fn without_the_bridge_the_wake_runs_the_program_it_slept_with() {
        let state = state("unbridged");
        let run = run_in(&state);
        let ack = acknowledgement(&run);
        let line = bridged(
            &run,
            Some(serde_json::json!({"hooks": {"SessionStart": [ack]}})),
        );
        assert_eq!(
            unbridged(&line, None).unwrap(),
            [line[0].as_str(), "--model", "opus"]
        );
        std::fs::remove_dir_all(&state).unwrap();
    }

    /// The bridge's Codex socket is dropped, and the wake starts through the
    /// bridge; a socket the bridge did not open is somebody's own, and stays.
    #[test]
    fn a_codex_the_bridge_started_wakes_through_the_bridge_and_no_other_does() {
        let s = |w: &[&str]| w.iter().map(|x| x.to_string()).collect::<Vec<String>>();
        const CODEX: &str = "/data/terminal-delight/skill-manager/k/bin/codex";
        let runs = "/s/terminal-delight/skill-receipts/runs";
        let line = |socket: &str| s(&["/usr/bin/codex", "--remote", socket, "--model", "o3"]);
        let ours = format!("unix://{runs}/88-ab_c/codex.sock");
        assert_eq!(
            codex_unbridged(&line(&ours), Some(Path::new(CODEX))),
            [CODEX, "--model", "o3"]
        );
        assert_eq!(
            codex_unbridged(&line(&ours), None),
            ["/usr/bin/codex", "--model", "o3"]
        );
        for theirs in [
            format!("unix://{runs}/88-ab_c/other.sock"),
            format!("unix://{runs}/ab-88/codex.sock"),
            format!("unix://{runs}/88-ab/deeper/codex.sock"),
            "unix:///s/terminal-delight/elsewhere/88-ab/codex.sock".to_owned(),
            format!("ws://{runs}/88-ab/codex.sock"),
            "ws://127.0.0.1:4500".to_owned(),
        ] {
            assert_eq!(
                codex_unbridged(&line(&theirs), Some(Path::new(CODEX))),
                line(&theirs)
            );
        }
        // Not first: the bridge always puts its socket first.
        let later = s(&["/usr/bin/codex", "--model", "o3", "--remote", &ours]);
        assert_eq!(codex_unbridged(&later, Some(Path::new(CODEX))), later);
    }

    /// The last resort, for a line an older window wrote: only a bridge folder
    /// that is really gone is replaced, and only by the matching shim.
    #[test]
    fn a_lost_bridge_folder_is_replaced_only_when_it_is_gone() {
        let state = state("lost");
        let run = run_in(&state);
        std::fs::create_dir_all(&run).unwrap();
        let s = |w: &[&str]| w.iter().map(|x| x.to_string()).collect::<Vec<String>>();
        let root = Path::new("/data/terminal-delight/skill-manager/k");
        let settings = format!("{}/settings.json", run.display());
        let claude = s(&[
            "/c",
            "--add-dir",
            &run.to_string_lossy(),
            "--settings",
            &settings,
            "--resume",
            SLEEPER,
        ]);
        let socket = format!("unix://{}/codex.sock", run.display());
        let codex = s(&["/x", "--remote", &socket, "resume", SLEEPER]);
        assert_eq!(rebridge_lost_with(&claude, Some(root)), None, "still there");
        assert_eq!(rebridge_lost_with(&codex, Some(root)), None, "still there");
        std::fs::remove_dir_all(&state).unwrap();
        assert_eq!(
            rebridge_lost_with(&claude, Some(root)).unwrap(),
            [
                &*root.join("bin/claude").to_string_lossy(),
                "--resume",
                SLEEPER
            ]
        );
        assert_eq!(
            rebridge_lost_with(&codex, Some(root)).unwrap(),
            [
                &*root.join("bin/codex").to_string_lossy(),
                "resume",
                SLEEPER
            ]
        );
        assert_eq!(
            rebridge_lost_with(&claude, None).unwrap(),
            ["/c", "--resume", SLEEPER]
        );
        let theirs = s(&[
            "/c",
            "--add-dir",
            "/gone/x",
            "--settings",
            "/gone/x/settings.json",
        ]);
        assert_eq!(
            rebridge_lost_with(&theirs, Some(root)),
            None,
            "not a bridge folder"
        );
    }

    /// The round trip that failed in the field: the real bridge starts a Claude,
    /// it ends, the next launch prunes its folder, and its wake script starts
    /// it anyway, through the shell, carrying settings the shell could mangle.
    #[test]
    fn a_wake_goes_back_through_the_real_bridge_after_its_folder_is_pruned() {
        use std::os::unix::fs::PermissionsExt;
        let root = install().unwrap();
        let shim = root.join("bin/claude");
        let dir = std::env::temp_dir().join(format!("td-wake-round-trip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let skill = dir.join("skill");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: td-wake-fixture\ndescription: A fixture.\n---\nBody.\n",
        )
        .unwrap();
        let config = dir.join("skills.json");
        let source = serde_json::json!({"path": skill, "origin": "fixture", "revision": "v1", "enabled": true});
        let settings = serde_json::json!({"active": true, "briefing": false, "factory": false, "sources": [source]});
        std::fs::write(&config, settings.to_string()).unwrap();
        // Stands in for Claude: records its words and the settings file they
        // name, which will not outlive it.
        let fake = dir.join("claude");
        std::fs::write(
            &fake,
            "#!/usr/bin/env python3\nimport json,os,sys\nfrom pathlib import Path\na=sys.argv[1:]\n\
             s=Path(a[a.index('--settings')+1]).read_text()\n\
             Path(os.environ['ARGV_LOG']).write_text(json.dumps({'argv':a,'settings':json.loads(s)}))\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let log = dir.join("argv.json");
        let launch = |mut command: std::process::Command| -> (Vec<String>, serde_json::Value) {
            let output = command
                .current_dir(&dir)
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .env("HOME", &dir)
                .env("XDG_STATE_HOME", dir.join("state"))
                .env("XDG_CONFIG_HOME", dir.join("config"))
                .env("TD_SKILLS_SESSION", "wake-fixture")
                .env("TD_SKILLS_PANE_ID", "7")
                .env("TD_SKILLS_CONFIG", &config)
                .env("TD_SKILLS_REAL_CLAUDE", &fake)
                .env("ARGV_LOG", &log)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let record: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&log).unwrap()).unwrap();
            let argv = serde_json::from_value(record["argv"].clone()).unwrap();
            (argv, record["settings"].clone())
        };
        let ours = |settings: &serde_json::Value| settings["hooks"]["SessionStart"].clone();
        let awkward = "it's $HOME \"q\" `true` \\n";
        let theirs = serde_json::json!({"model": "personal", "env": {"TD_WAKE_FIXTURE": awkward}});

        let mut first_launch = std::process::Command::new("python3");
        first_launch
            .arg(&shim)
            .args(["--settings", &theirs.to_string(), "--model", "opus"]);
        let (first, first_settings) = launch(first_launch);
        assert_eq!(
            (first[0].as_str(), first[2].as_str()),
            ("--add-dir", "--settings")
        );
        let old = PathBuf::from(&first[1]);
        assert_eq!(first_settings["env"]["TD_WAKE_FIXTURE"], awkward);
        assert_eq!(ours(&first_settings).as_array().map(Vec::len), Some(1));

        let mut slept = vec![fake.to_string_lossy().into_owned()];
        slept.extend(first);
        let woke = unbridged(&slept, Some(&shim)).unwrap();
        let wake = crate::agentrest::wake_argv(&woke, SLEEPER).unwrap();
        assert_eq!(PathBuf::from(&wake[0]), shim);
        let script = dir.join("wake.sh");
        std::fs::write(&script, crate::agentrest_io::wake_script(&wake, None)).unwrap();

        let mut woken = std::process::Command::new("bash");
        woken.arg(&script);
        let (second, second_settings) = launch(woken);
        assert!(!old.exists(), "the ended agent's folder was pruned");
        assert_eq!(second[0], "--add-dir");
        let new = PathBuf::from(&second[1]);
        assert_ne!(new, old);
        assert!(new.is_dir());
        assert_eq!(&second[4..], ["--model", "opus", "--resume", SLEEPER]);
        assert_eq!(second_settings["model"], "personal");
        assert_eq!(second_settings["env"]["TD_WAKE_FIXTURE"], awkward);
        let hooks = ours(&second_settings);
        assert_eq!(hooks.as_array().map(Vec::len), Some(1), "{hooks}");
        assert!(hooks.to_string().contains(&*new.to_string_lossy()));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
