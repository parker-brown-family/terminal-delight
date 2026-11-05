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
}
