use crate::launcher::Harness;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const MAX_OUTPUT: u64 = 2 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(15);
const EXPIRY: Duration = Duration::from_secs(60);
static ACTIVE_DISCOVERIES: AtomicUsize = AtomicUsize::new(0);

struct DiscoverySlot;

impl Drop for DiscoverySlot {
    fn drop(&mut self) {
        ACTIVE_DISCOVERIES.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    pub id: Option<String>,
    pub label: String,
    pub resolved_id: Option<String>,
    pub efforts: Option<Vec<String>>,
    pub default_effort: Option<String>,
}

impl Model {
    pub fn agent_default() -> Self {
        Self {
            id: None,
            label: "Agent default".into(),
            resolved_id: None,
            efforts: None,
            default_effort: None,
        }
    }

    pub fn matches(&self, raw: &str) -> bool {
        self.resolved_id.as_deref().or(self.id.as_deref()) == Some(raw)
            || self.id.as_deref() == Some(raw)
    }
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub models: Vec<Model>,
    pub status: String,
    pub ready: bool,
}

#[derive(Clone, Debug)]
pub struct DialChoice {
    pub value: Option<String>,
    pub label: String,
}

impl Snapshot {
    pub fn model(&self, id: Option<&str>) -> Option<&Model> {
        self.models.iter().find(|model| model.id.as_deref() == id)
    }

    pub fn efforts(&self, id: Option<&str>) -> Vec<String> {
        self.model(id)
            .and_then(|model| model.efforts.clone())
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Scope {
    harness: Harness,
    cwd: PathBuf,
    shell_pid: Option<u32>,
    codex_home: Option<std::ffi::OsString>,
    claude_home: Option<std::ffi::OsString>,
    path: Option<std::ffi::OsString>,
}

struct Entry {
    snapshot: Snapshot,
    requested: Instant,
    loading: bool,
}

fn cache() -> &'static Mutex<HashMap<Scope, Entry>> {
    static CACHE: OnceLock<Mutex<HashMap<Scope, Entry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn snapshot(harness: Harness, cwd: &Path, shell_pid: Option<u32>) -> Snapshot {
    if harness == Harness::Local {
        return Snapshot {
            models: vec![Model::agent_default()],
            status: "local-agent.toml".into(),
            ready: true,
        };
    }
    let scope = Scope {
        harness,
        cwd: cwd.to_path_buf(),
        shell_pid,
        codex_home: std::env::var_os("CODEX_HOME"),
        claude_home: std::env::var_os("CLAUDE_CONFIG_DIR"),
        path: std::env::var_os("PATH"),
    };
    let mut entries = cache().lock().unwrap_or_else(|error| error.into_inner());
    let entry = entries.entry(scope.clone()).or_insert_with(|| Entry {
        snapshot: Snapshot {
            models: vec![Model::agent_default()],
            status: "Loading models…".into(),
            ready: false,
        },
        requested: Instant::now() - EXPIRY,
        loading: false,
    });
    if !entry.loading
        && entry.requested.elapsed() >= EXPIRY
        && ACTIVE_DISCOVERIES
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < 2).then_some(active + 1)
            })
            .is_ok()
    {
        entry.loading = true;
        entry.requested = Instant::now();
        entry.snapshot.ready = false;
        entry.snapshot.status = "Loading models…".into();
        std::thread::spawn(move || {
            let _slot = DiscoverySlot;
            let result = discover(&scope);
            let mut entries = cache().lock().unwrap_or_else(|error| error.into_inner());
            if let Some(entry) = entries.get_mut(&scope) {
                entry.snapshot = match result {
                    Ok(models) => Snapshot {
                        models: std::iter::once(Model::agent_default())
                            .chain(models)
                            .collect(),
                        status: "Installed agent catalog".into(),
                        ready: true,
                    },
                    Err(reason) => Snapshot {
                        models: vec![Model::agent_default()],
                        status: format!("Models unavailable · {reason}"),
                        ready: false,
                    },
                };
                entry.loading = false;
            }
        });
    }
    entry.snapshot.clone()
}

pub fn retry(harness: Harness, cwd: &Path, shell_pid: Option<u32>) {
    let mut entries = cache().lock().unwrap_or_else(|error| error.into_inner());
    for (scope, entry) in entries.iter_mut() {
        if scope.harness == harness && scope.cwd == cwd && scope.shell_pid == shell_pid {
            entry.requested = Instant::now() - EXPIRY;
        }
    }
}

fn discover(scope: &Scope) -> Result<Vec<Model>, String> {
    if scope.harness == Harness::Local {
        return Ok(Vec::new());
    }
    let mut command = if let Some(shell_pid) = scope.shell_pid {
        let agent = live_agent(shell_pid, scope.harness)
            .ok_or_else(|| "running executable not identified".to_string())?;
        let mut command = Command::new(format!("/proc/{agent}/exe"));
        let environment = std::fs::read(format!("/proc/{agent}/environ"))
            .map_err(|_| "running profile unavailable".to_string())?;
        for key in ["CODEX_HOME", "CLAUDE_CONFIG_DIR"] {
            command.env_remove(key);
            for item in environment.split(|byte| *byte == 0) {
                if let Some(value) = item.strip_prefix(format!("{key}=").as_bytes()) {
                    use std::os::unix::ffi::OsStrExt;
                    command.env(key, std::ffi::OsStr::from_bytes(value));
                }
            }
        }
        command
    } else {
        Command::new(scope.harness.binary())
    };
    command.current_dir(&scope.cwd);
    match scope.harness {
        Harness::Codex => {
            command.arg("app-server");
        }
        Harness::Claude => {
            command.args([
                "--print",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--verbose",
                "--no-session-persistence",
                "--strict-mcp-config",
                "--mcp-config",
                "{\"mcpServers\":{}}",
            ]);
        }
        Harness::Local => unreachable!(),
    }
    let mut transport = Transport::start(&mut command)?;
    match scope.harness {
        Harness::Codex => {
            transport.send(json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"terminal-delight","version":env!("CARGO_PKG_VERSION")}}}))?;
            transport.response(1)?;
            transport.send(json!({"method":"initialized"}))?;
            codex_pages(&mut transport)
        }
        Harness::Claude => {
            transport.send(json!({"type":"control_request","request_id":"td-model-catalog","request":{"subtype":"initialize"}}))?;
            loop {
                let message = transport.next()?;
                if message
                    .pointer("/response/request_id")
                    .and_then(Value::as_str)
                    != Some("td-model-catalog")
                {
                    continue;
                }
                if message.pointer("/response/subtype").and_then(Value::as_str) != Some("success") {
                    return Err("agent rejected catalog request".into());
                }
                let response = message
                    .pointer("/response/response")
                    .ok_or("missing catalog response")?;
                return nonempty(parse_claude(response)?);
            }
        }
        Harness::Local => unreachable!(),
    }
}

fn codex_pages(transport: &mut Transport) -> Result<Vec<Model>, String> {
    let mut models = Vec::new();
    let mut cursor = None;
    let mut seen = std::collections::HashSet::new();
    for page in 0..32 {
        let id = page + 2;
        transport.send(json!({"id":id,"method":"model/list","params":{"limit":100,"includeHidden":false,"cursor":cursor}}))?;
        let response = transport.response(id)?;
        models.extend(parse_codex(&response)?);
        cursor = response
            .get("nextCursor")
            .and_then(Value::as_str)
            .map(str::to_string);
        match &cursor {
            None => return nonempty(models),
            Some(cursor) if !seen.insert(cursor.clone()) => {
                return Err("repeated catalog cursor".into())
            }
            Some(_) => {}
        }
    }
    Err("too many catalog pages".into())
}

fn nonempty(models: Vec<Model>) -> Result<Vec<Model>, String> {
    if models.is_empty() {
        Err("agent returned no visible models".into())
    } else {
        Ok(models)
    }
}

fn live_agent(pid: u32, harness: Harness) -> Option<u32> {
    let mut pending = vec![pid];
    for _ in 0..128 {
        let candidate = pending.pop()?;
        let comm = std::fs::read_to_string(format!("/proc/{candidate}/comm")).ok();
        if comm.as_deref().map(str::trim) == Some(harness.binary()) {
            return Some(candidate);
        }
        if let Ok(children) =
            std::fs::read_to_string(format!("/proc/{candidate}/task/{candidate}/children"))
        {
            pending.extend(
                children
                    .split_whitespace()
                    .filter_map(|child| child.parse::<u32>().ok()),
            );
        }
    }
    None
}

fn choice(value: &Value) -> Result<String, String> {
    value
        .as_str()
        .filter(|word| !word.trim().is_empty() && !word.chars().any(char::is_control))
        .map(str::to_string)
        .ok_or_else(|| "invalid catalog choice".into())
}

fn words(value: Option<&Value>) -> Result<Option<Vec<String>>, String> {
    value
        .map(|value| {
            value
                .as_array()
                .ok_or_else(|| "invalid effort list".to_string())?
                .iter()
                .map(choice)
                .collect()
        })
        .transpose()
}

pub fn parse_codex(response: &Value) -> Result<Vec<Model>, String> {
    let rows = response
        .get("data")
        .and_then(Value::as_array)
        .ok_or("missing Codex model list")?;
    rows.iter()
        .filter(|row| row.get("hidden").and_then(Value::as_bool) != Some(true))
        .map(|row| {
            let id = choice(row.get("model").ok_or("missing model ID")?)?;
            let efforts = row
                .get("supportedReasoningEfforts")
                .map(|value| {
                    let levels = value.as_array().ok_or("invalid effort list")?;
                    levels
                        .iter()
                        .map(|level| {
                            choice(level.get("reasoningEffort").ok_or("invalid effort level")?)
                        })
                        .collect::<Result<Vec<_>, String>>()
                })
                .transpose()?;
            Ok(Model {
                id: Some(id.clone()),
                label: label(&id).unwrap_or_else(|| id.clone()),
                resolved_id: Some(id),
                efforts,
                default_effort: row
                    .get("defaultReasoningEffort")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect()
}

pub fn parse_claude(response: &Value) -> Result<Vec<Model>, String> {
    let rows = response
        .get("models")
        .and_then(Value::as_array)
        .ok_or("missing Claude model list")?;
    rows.iter()
        .map(|row| {
            let id = choice(row.get("value").ok_or("missing model ID")?)?;
            let resolved = row
                .get("resolvedModel")
                .and_then(Value::as_str)
                .map(str::to_string);
            let mut display = resolved
                .as_deref()
                .and_then(label)
                .or_else(|| {
                    id.chars()
                        .any(|character| character.is_ascii_digit())
                        .then(|| label(&id))
                        .flatten()
                })
                .unwrap_or_else(|| {
                    format!(
                        "{} · version unknown",
                        row.get("displayName")
                            .and_then(Value::as_str)
                            .unwrap_or(&id)
                    )
                });
            if id == "default" {
                display = format!("Recommended · {display}");
            }
            Ok(Model {
                id: Some(id),
                label: display,
                resolved_id: resolved,
                efforts: if row.get("supportsEffort").and_then(Value::as_bool) == Some(false) {
                    Some(Vec::new())
                } else {
                    words(row.get("supportedEffortLevels"))?
                },
                default_effort: None,
            })
        })
        .collect()
}

pub fn label(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "<synthetic>" {
        return None;
    }
    let lower = raw.to_ascii_lowercase();
    let id = lower.split('[').next()?.trim();
    let pieces: Vec<_> = id
        .split(['-', ' ', '.'])
        .filter(|piece| !piece.is_empty())
        .collect();
    let family = pieces.iter().find(|piece| {
        [
            "sol", "astra", "luna", "terra", "opus", "fable", "sonnet", "haiku",
        ]
        .contains(piece)
    });
    let Some(family) = family else {
        return Some(raw.to_string());
    };
    let version: Vec<_> = pieces
        .iter()
        .filter(|piece| {
            piece.len() <= 2 && piece.chars().all(|character| character.is_ascii_digit())
        })
        .copied()
        .collect();
    Some(if version.is_empty() {
        family.to_string()
    } else {
        format!("{family} {}", version.join("."))
    })
}

pub fn compact_label(raw: &str) -> Option<String> {
    let label = label(raw)?;
    let family = label.split_whitespace().next()?;
    Some(
        if [
            "sol", "astra", "luna", "terra", "opus", "fable", "sonnet", "haiku",
        ]
        .contains(&family)
        {
            label.to_ascii_uppercase()
        } else {
            label
        },
    )
}

struct Transport {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<Result<Value, String>>,
    deadline: Instant,
}

impl Transport {
    fn start(command: &mut Command) -> Result<Self, String> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .map_err(|error| error.to_string())?;
        let input = child.stdin.take().ok_or("agent input unavailable")?;
        let stdout = child.stdout.take().ok_or("agent output unavailable")?;
        let (sender, output) = mpsc::sync_channel(8);
        std::thread::spawn(move || {
            use std::io::Read;
            let mut reader = BufReader::new(stdout.take(MAX_OUTPUT + 1));
            let mut consumed = 0;
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(size) => {
                        consumed += size as u64;
                        if consumed > MAX_OUTPUT {
                            let _ = sender.send(Err("catalog output exceeded limit".into()));
                            break;
                        }
                        let value =
                            serde_json::from_str(&line).map_err(|_| "invalid catalog JSON".into());
                        if sender.send(value).is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        let _ = sender.send(Err("cannot read agent catalog".into()));
                        break;
                    }
                }
            }
        });
        Ok(Self {
            child,
            input,
            output,
            deadline: Instant::now() + DEADLINE,
        })
    }

    fn send(&mut self, value: Value) -> Result<(), String> {
        writeln!(self.input, "{value}").map_err(|_| "cannot request agent catalog".to_string())?;
        self.input
            .flush()
            .map_err(|_| "cannot flush catalog request".into())
    }

    fn next(&self) -> Result<Value, String> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        self.output.recv_timeout(remaining).map_err(|error| {
            match error {
                mpsc::RecvTimeoutError::Timeout => "catalog request timed out",
                mpsc::RecvTimeoutError::Disconnected => "agent closed catalog connection",
            }
            .to_string()
        })?
    }

    fn response(&self, id: u64) -> Result<Value, String> {
        loop {
            let message = self.next()?;
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if message.get("error").is_some() {
                return Err("agent rejected catalog request".into());
            }
            return message
                .get("result")
                .cloned()
                .ok_or("missing catalog result".into());
        }
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_families_keep_their_reported_versions() {
        for (raw, expected) in [
            ("gpt-6.1-sol", "SOL 6.1"),
            ("gpt-6-astra", "ASTRA 6"),
            ("gpt-6-luna", "LUNA 6"),
            ("gpt-5.6-terra", "TERRA 5.6"),
            ("claude-opus-5-5", "OPUS 5.5"),
            ("claude-3-5-sonnet-20241022", "SONNET 3.5"),
        ] {
            assert_eq!(compact_label(raw).as_deref(), Some(expected));
        }
        assert_eq!(label("opus").as_deref(), Some("opus"));
        assert_eq!(
            label("provider/unfamiliar-v9").as_deref(),
            Some("provider/unfamiliar-v9")
        );
        assert_eq!(label("<synthetic>"), None);
    }

    #[test]
    fn catalogs_preserve_exact_ids_versions_and_unknown_efforts() {
        let codex = parse_codex(&json!({"data":[{"model":"gpt-6.1-sol","supportedReasoningEfforts":[{"reasoningEffort":"ultra"},{"reasoningEffort":"future"}]},{"model":"hidden","hidden":true},{"model":"gpt-5.6-sol"}]})).unwrap();
        assert_eq!(codex.len(), 2);
        assert_eq!(
            codex[0].efforts,
            Some(vec!["ultra".into(), "future".into()])
        );
        assert_eq!(codex[1].efforts, None);
        assert!(!codex[0].matches("gpt-5.6-sol"));
        let claude = parse_claude(&json!({"models":[{"value":"sonnet","resolvedModel":"claude-sonnet-5-5","supportedEffortLevels":[]},{"value":"unknown"},{"value":"haiku","supportsEffort":false}]})).unwrap();
        assert_eq!(claude[0].label, "sonnet 5.5");
        assert_eq!(claude[0].id.as_deref(), Some("sonnet"));
        assert!(claude[0].matches("claude-sonnet-5-5"));
        assert_eq!(claude[0].efforts, Some(vec![]));
        assert_eq!(claude[1].efforts, None);
        assert!(claude[1].label.contains("version unknown"));
        assert_eq!(claude[2].efforts, Some(vec![]));
    }

    #[test]
    fn launch_default_has_no_model_id_to_send() {
        assert_eq!(Model::agent_default().id, None);
        assert!(!Model::agent_default().matches("configured"));
        assert!(parse_codex(&json!({"data":[{}]})).is_err());
    }

    #[test]
    fn invalid_choices_cannot_become_live_commands_or_empty_capabilities() {
        assert!(parse_codex(&json!({"data":[{"model":"sol\n/help"}]})).is_err());
        assert!(
            parse_codex(&json!({"data":[{"model":"sol","supportedReasoningEfforts":[{}]}]}))
                .is_err()
        );
        assert!(
            parse_claude(&json!({"models":[{"value":"opus","supportedEffortLevels":[123]}]}))
                .is_err()
        );
        assert!(parse_claude(
            &json!({"models":[{"value":"opus","supportedEffortLevels":["high\n/help"]}]})
        )
        .is_err());
    }

    #[test]
    fn transport_handles_metadata_responses_and_stops_the_child() {
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "read request; printf '%s\\n' '{\"id\":1,\"result\":{\"data\":[]}}'; read request",
        ]);
        let pid;
        {
            let mut transport = Transport::start(&mut command).unwrap();
            pid = transport.child.id();
            transport
                .send(json!({"id":1,"method":"model/list"}))
                .unwrap();
            assert_eq!(transport.response(1).unwrap(), json!({"data":[]}));
        }
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }

    #[test]
    fn transport_rejects_malformed_output_and_has_a_deadline() {
        let mut command = Command::new("sh");
        command.args(["-c", "printf '%s\\n' broken; read request"]);
        let transport = Transport::start(&mut command).unwrap();
        assert_eq!(transport.next().unwrap_err(), "invalid catalog JSON");
        let mut command = Command::new("sh");
        command.args(["-c", "read request"]);
        let mut transport = Transport::start(&mut command).unwrap();
        transport.deadline = Instant::now() + Duration::from_millis(20);
        assert_eq!(transport.next().unwrap_err(), "catalog request timed out");
    }

    #[test]
    fn codex_follows_pages_without_creating_a_conversation() {
        let mut command = Command::new("sh");
        command.args(["-c", "read request; case \"$request\" in *model/list*) ;; *) exit 9;; esac; printf '%s\\n' '{\"id\":2,\"result\":{\"data\":[{\"model\":\"gpt-6.1-sol\"}],\"nextCursor\":\"second\"}}'; read request; case \"$request\" in *second*) ;; *) exit 10;; esac; printf '%s\\n' '{\"id\":3,\"result\":{\"data\":[{\"model\":\"gpt-6-astra\"}],\"nextCursor\":null}}'"]);
        let mut transport = Transport::start(&mut command).unwrap();
        let models = codex_pages(&mut transport).unwrap();
        assert_eq!(
            models
                .iter()
                .filter_map(|model| model.id.as_deref())
                .collect::<Vec<_>>(),
            vec!["gpt-6.1-sol", "gpt-6-astra"]
        );
    }

    #[test]
    fn missing_agent_and_repeated_cursor_are_reported_as_unavailable() {
        assert!(Transport::start(&mut Command::new("/nonexistent/td-model-agent")).is_err());
        let mut command = Command::new("sh");
        command.args(["-c", "read request; printf '%s\\n' '{\"id\":2,\"result\":{\"data\":[{\"model\":\"model-a\"}],\"nextCursor\":\"again\"}}'; read request; printf '%s\\n' '{\"id\":3,\"result\":{\"data\":[{\"model\":\"model-b\"}],\"nextCursor\":\"again\"}}'"]);
        let mut transport = Transport::start(&mut command).unwrap();
        assert_eq!(
            codex_pages(&mut transport).unwrap_err(),
            "repeated catalog cursor"
        );
    }
}
