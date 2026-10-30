//! TD's registrations as an MCP server in other programs' configuration. The
//! docs give `claude mcp add --scope user terminal-delight -- terminal-delight
//! mcp`; after an uninstall that entry makes every new Claude session try to
//! start a program that is gone. An entry is TD's when its command is TD
//! itself and its first argument is `mcp`. A wrapper script is somebody's own
//! and is reported with the line to remove by hand. Removal goes through each
//! client's own CLI, which owns its file and its locking; TD never rewrites
//! `~/.claude.json`, which Claude Code itself rewrites constantly.
use super::roots::Roots;
use serde::Serialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Client {
    Claude,
    Codex,
    Gemini,
}

impl Client {
    pub(crate) fn cli(self) -> &'static str {
        match self {
            Client::Claude => "claude",
            Client::Codex => "codex",
            Client::Gemini => "gemini",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct Entry {
    pub(crate) client: Client,
    pub(crate) file: PathBuf,
    /// `user`, `local` (a Claude project entry) or `global` (Codex).
    pub(crate) scope: &'static str,
    pub(crate) project: Option<PathBuf>,
    pub(crate) name: String,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    /// The PATH the entry sets for its own server, which is what its client
    /// resolves a bare command against.
    pub(crate) path: Option<String>,
    /// The command is TD itself, run as `mcp`.
    pub(crate) td: bool,
}

impl Entry {
    /// The variable that moves this client's configuration, and the value that
    /// points it at the file that was read: none when that file is where the
    /// client looks by default.
    fn root_pin(&self, home: &Path) -> (&'static str, Option<PathBuf>) {
        let (var, default, root) = match self.client {
            Client::Claude => (
                "CLAUDE_CONFIG_DIR",
                home.join(".claude.json"),
                self.file.parent(),
            ),
            Client::Codex => (
                "CODEX_HOME",
                home.join(".codex/config.toml"),
                self.file.parent(),
            ),
            Client::Gemini => (
                "GEMINI_CLI_HOME",
                home.join(".gemini/settings.json"),
                self.file.parent().and_then(Path::parent),
            ),
        };
        (
            var,
            (self.file != default).then(|| root.unwrap_or(home).to_path_buf()),
        )
    }

    /// The client's own removal command, and the directory to run it in. The
    /// command names the configuration root it must edit, so the client
    /// changes the very file that was read and copied, whatever the worker's
    /// own environment says.
    pub(crate) fn remove_argv(&self, home: &Path) -> (Vec<String>, Option<PathBuf>) {
        let (var, root) = self.root_pin(home);
        let mut pin: Vec<String> = vec!["/usr/bin/env".into()];
        match root {
            None => pin.extend(["-u".into(), var.into()]),
            Some(r) => pin.push(format!("{var}={}", r.display())),
        }
        let (argv, cwd) = self.client_argv();
        pin.extend(argv);
        (pin, cwd)
    }

    fn client_argv(&self) -> (Vec<String>, Option<PathBuf>) {
        let n = self.name.clone();
        match (self.client, self.scope) {
            (Client::Claude, "local") => (
                vec![
                    "claude".into(),
                    "mcp".into(),
                    "remove".into(),
                    "-s".into(),
                    "local".into(),
                    n,
                ],
                self.project.clone(),
            ),
            (Client::Claude, _) => (
                vec![
                    "claude".into(),
                    "mcp".into(),
                    "remove".into(),
                    "-s".into(),
                    "user".into(),
                    n,
                ],
                None,
            ),
            (Client::Codex, _) => (vec!["codex".into(), "mcp".into(), "remove".into(), n], None),
            (Client::Gemini, _) => (
                vec![
                    "gemini".into(),
                    "mcp".into(),
                    "remove".into(),
                    "-s".into(),
                    "user".into(),
                    n,
                ],
                None,
            ),
        }
    }

    /// What a person runs or edits when the CLI is missing or this is a wrapper.
    pub(crate) fn manual(&self, home: &Path) -> String {
        let (argv, cwd) = self.remove_argv(home);
        let cmd = argv.join(" ");
        match cwd {
            Some(d) => format!("cd {} && {cmd}", d.display()),
            None => format!(
                "{cmd}   (or delete the \"{}\" entry in {})",
                self.name,
                self.file.display()
            ),
        }
    }
}

/// Whether `command args` is TD itself run as an MCP relay.
pub(crate) fn is_td(
    command: &str,
    args: &[String],
    td_exes: &[PathBuf],
    path_var: Option<&std::ffi::OsStr>,
) -> bool {
    if args.first().map(String::as_str) != Some("mcp") {
        return false;
    }
    // A bare command is what the client would find on PATH; a name alone
    // proves nothing, since anyone can call a wrapper terminal-delight.
    let p = Path::new(command);
    let found = if p.is_absolute() {
        Some(p.to_path_buf())
    } else if !command.contains('/') {
        // Walked in the client's order. A relative or empty entry, or one
        // that still holds a `$`, depends on a launch context this cannot
        // see, so reaching one before a hit leaves the command unresolved.
        path_var.and_then(|pv| {
            for d in std::env::split_paths(pv) {
                if !d.is_absolute() || d.as_os_str().to_string_lossy().contains('$') {
                    return None;
                }
                let c = d.join(command);
                if std::fs::metadata(&c).is_ok_and(|m| m.is_file()) && executable(&c) {
                    return Some(c);
                }
            }
            None
        })
    } else {
        None
    };
    let Some(canon) = found.and_then(|f| std::fs::canonicalize(f).ok()) else {
        return false;
    };
    td_exes
        .iter()
        .any(|e| std::fs::canonicalize(e).ok().as_deref() == Some(canon.as_path()))
}

/// The PATH an entry sets for its own server, from JSON or TOML alike.
fn entry_path<T: EnvLookup>(s: &T) -> Option<String> {
    s.env_path()
}

trait EnvLookup {
    fn env_path(&self) -> Option<String>;
}

impl EnvLookup for serde_json::Value {
    fn env_path(&self) -> Option<String> {
        self.get("env")?.get("PATH")?.as_str().map(String::from)
    }
}

impl EnvLookup for toml::Value {
    fn env_path(&self) -> Option<String> {
        self.get("env")?.get("PATH")?.as_str().map(String::from)
    }
}

/// Whether this account may execute `p`, as the client's own lookup would judge.
fn executable(p: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(p.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: access reads the NUL-terminated path we own and nothing else.
    unsafe { libc::access(c.as_ptr(), libc::X_OK) == 0 }
}

/// What identifies a registration at removal time: its command, its args and
/// the PATH it sets for its own server.
pub(crate) type Identity = (String, Vec<String>, Option<String>);

/// The entry `name` as it stands in `file` right now: its command and args.
/// The worker reads this just before removal, so a registration somebody
/// replaced under the same name since the plan is never removed.
pub(crate) fn current_entry(
    client: &str,
    file: &Path,
    scope: &str,
    project: Option<&Path>,
    name: &str,
) -> Result<Option<Identity>, String> {
    // A file that cannot be read or parsed is an error, never "the entry is
    // gone": only a successfully read file can say an entry is absent.
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    if client == "codex" {
        let t: toml::Table = text
            .parse()
            .map_err(|e| format!("{}: {e}", file.display()))?;
        let Some(s) = t.get("mcp_servers").and_then(|m| m.get(name)) else {
            return Ok(None);
        };
        let command = s
            .get("command")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        let args = s
            .get("args")
            .and_then(|a| a.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        return Ok(Some((command, args, entry_path(s))));
    }
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;
    let servers = match (client, scope, project) {
        ("claude", "local", Some(p)) => p
            .to_str()
            .and_then(|p| v.get("projects")?.get(p)?.get("mcpServers")),
        _ => v.get("mcpServers"),
    };
    let Some(s) = servers.and_then(|m| m.get(name)) else {
        return Ok(None);
    };
    let command = s
        .get("command")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    Ok(Some((command, str_list(s.get("args")), entry_path(s))))
}

/// A name or command that looks like TD's but is not TD itself: worth a line.
fn looks_like_td(name: &str, command: &str) -> bool {
    let base = Path::new(command)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("");
    name.contains("terminal-delight") || base.contains("terminal-delight") || base == "td-mcp"
}

/// Every TD-related MCP entry in Claude's, Codex's and Gemini's user config.
/// Returns the entries and any config that exists but could not be read.
pub(crate) fn discover(
    roots: &Roots,
    env: &dyn Fn(&str) -> Option<OsString>,
    td_exes: &[PathBuf],
) -> (Vec<Entry>, Vec<String>) {
    let mut out = Vec::new();
    let mut problems = Vec::new();
    let path_var = env("PATH");
    let get = |k: &str| {
        env(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };

    let claude = get("CLAUDE_CONFIG_DIR")
        .map(|d| d.join(".claude.json"))
        .unwrap_or_else(|| roots.home.join(".claude.json"));
    match read_json(&claude) {
        Ok(None) => {}
        Err(e) => problems.push(e),
        Ok(Some(v)) => {
            let mut take = |servers: Option<&serde_json::Value>,
                            scope: &'static str,
                            project: Option<PathBuf>| {
                let Some(serde_json::Value::Object(m)) = servers else {
                    return;
                };
                for (name, s) in m {
                    let command = s
                        .get("command")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string();
                    let args = str_list(s.get("args"));
                    let path = entry_path(s);
                    let td = s.get("cwd").is_none()
                        && is_td(
                            &command,
                            &args,
                            td_exes,
                            path.as_ref()
                                .map(std::ffi::OsStr::new)
                                .or(path_var.as_deref()),
                        );
                    if td || looks_like_td(name, &command) {
                        out.push(Entry {
                            client: Client::Claude,
                            file: claude.clone(),
                            scope,
                            project: project.clone(),
                            name: name.clone(),
                            command,
                            args,
                            path,
                            td,
                        });
                    }
                }
            };
            take(v.get("mcpServers"), "user", None);
            if let Some(serde_json::Value::Object(projects)) = v.get("projects") {
                for (dir, p) in projects {
                    take(p.get("mcpServers"), "local", Some(PathBuf::from(dir)));
                }
            }
        }
    }

    let codex = get("CODEX_HOME")
        .unwrap_or_else(|| roots.home.join(".codex"))
        .join("config.toml");
    match std::fs::read_to_string(&codex) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => problems.push(format!("{}: {e}", codex.display())),
        Ok(text) => match text.parse::<toml::Table>() {
            Err(e) => problems.push(format!("{}: {e}", codex.display())),
            Ok(t) => {
                if let Some(toml::Value::Table(servers)) = t.get("mcp_servers") {
                    for (name, s) in servers {
                        let command = s
                            .get("command")
                            .and_then(|c| c.as_str())
                            .unwrap_or("")
                            .to_string();
                        let args: Vec<String> = s
                            .get("args")
                            .and_then(|a| a.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|x| x.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();
                        let path = entry_path(s);
                        let td = s.get("cwd").is_none()
                            && is_td(
                                &command,
                                &args,
                                td_exes,
                                path.as_ref()
                                    .map(std::ffi::OsStr::new)
                                    .or(path_var.as_deref()),
                            );
                        if td || looks_like_td(name, &command) {
                            out.push(Entry {
                                client: Client::Codex,
                                file: codex.clone(),
                                scope: "global",
                                project: None,
                                name: name.clone(),
                                command,
                                args,
                                path,
                                td,
                            });
                        }
                    }
                }
            }
        },
    }

    // GEMINI_CLI_HOME stands in for the home Gemini keeps its settings under.
    let gemini = get("GEMINI_CLI_HOME")
        .unwrap_or_else(|| roots.home.clone())
        .join(".gemini/settings.json");
    match read_json(&gemini) {
        Ok(None) => {}
        Err(e) => problems.push(e),
        Ok(Some(v)) => {
            if let Some(serde_json::Value::Object(m)) = v.get("mcpServers") {
                for (name, s) in m {
                    let command = s
                        .get("command")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string();
                    let args = str_list(s.get("args"));
                    let path = entry_path(s);
                    let td = s.get("cwd").is_none()
                        && is_td(
                            &command,
                            &args,
                            td_exes,
                            path.as_ref()
                                .map(std::ffi::OsStr::new)
                                .or(path_var.as_deref()),
                        );
                    if td || looks_like_td(name, &command) {
                        out.push(Entry {
                            client: Client::Gemini,
                            file: gemini.clone(),
                            scope: "user",
                            project: None,
                            name: name.clone(),
                            command,
                            args,
                            path,
                            td,
                        });
                    }
                }
            }
        }
    }
    (out, problems)
}

fn read_json(p: &Path) -> Result<Option<serde_json::Value>, String> {
    match std::fs::read_to_string(p) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", p.display())),
        Ok(t) => serde_json::from_str(&t)
            .map(Some)
            .map_err(|e| format!("{}: {e}", p.display())),
    }
}

fn str_list(v: Option<&serde_json::Value>) -> Vec<String> {
    v.and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn td_itself_versus_a_wrapper() {
        let dir = std::env::temp_dir().join(format!("td-mcp-exe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        let build = dir.join("td-aaa-build");
        std::fs::write(&build, b"\x7fELF").unwrap();
        std::fs::set_permissions(
            &build,
            <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o755),
        )
        .unwrap();
        std::os::unix::fs::symlink(&build, dir.join("bin/terminal-delight")).unwrap();
        let wrapper = dir.join("wrappers/terminal-delight");
        std::fs::create_dir_all(wrapper.parent().unwrap()).unwrap();
        std::fs::write(&wrapper, b"#!/bin/sh\n").unwrap();
        let exes = vec![build.clone()];
        let path = std::ffi::OsString::from(dir.join("bin"));
        let mcp = ["mcp".to_string()];
        assert!(is_td("terminal-delight", &mcp, &exes, Some(&path)));
        assert!(is_td(
            &dir.join("bin/terminal-delight").to_string_lossy(),
            &mcp,
            &exes,
            None
        ));
        assert!(!is_td("terminal-delight", &[], &exes, Some(&path)));
        assert!(
            !is_td("terminal-delight", &mcp, &exes, None),
            "unresolvable is not TD"
        );
        assert!(
            !is_td(&wrapper.to_string_lossy(), &mcp, &exes, None),
            "a wrapper named like TD is not TD"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn discovers_each_client_and_classifies() {
        let home = std::env::temp_dir().join(format!("td-mcp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        std::fs::create_dir_all(home.join(".gemini")).unwrap();
        std::fs::write(
            home.join(".claude.json"),
            r#"{"mcpServers":{"terminal-delight":{"type":"stdio","command":"terminal-delight","args":["mcp"]},"other":{"command":"x"}},
               "projects":{"/w/p":{"mcpServers":{"td":{"command":"/h/bin/td-mcp"}}}}}"#,
        )
        .unwrap();
        std::fs::write(
            home.join(".codex/config.toml"),
            "[mcp_servers.terminal-delight]\ncommand = \"/h/bin/td-mcp\"\nargs = []\n[mcp_servers.apes]\ncommand = \"x\"\n",
        )
        .unwrap();
        std::fs::write(home.join(".gemini/settings.json"), "{not json").unwrap();
        let h = home.clone();
        let roots = Roots::from_env(&move |k| (k == "HOME").then(|| OsString::from(&h))).unwrap();
        let build = home.join("td-aaa-build");
        std::fs::write(&build, b"\x7fELF").unwrap();
        std::fs::set_permissions(
            &build,
            <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o755),
        )
        .unwrap();
        std::fs::create_dir_all(home.join(".local/bin")).unwrap();
        std::os::unix::fs::symlink(&build, home.join(".local/bin/terminal-delight")).unwrap();
        let bin = OsString::from(home.join(".local/bin"));
        let (found, problems) = discover(
            &roots,
            &move |k| (k == "PATH").then(|| bin.clone()),
            &[build],
        );
        assert_eq!(found.len(), 3, "{found:?}");
        let user = found.iter().find(|e| e.scope == "user").unwrap();
        assert!(user.td);
        assert_eq!(
            user.remove_argv(&home).0.join(" "),
            "/usr/bin/env -u CLAUDE_CONFIG_DIR claude mcp remove -s user terminal-delight"
        );
        let local = found.iter().find(|e| e.scope == "local").unwrap();
        assert!(!local.td);
        assert_eq!(local.remove_argv(&home).1, Some(PathBuf::from("/w/p")));
        assert!(found.iter().any(|e| e.client == Client::Codex && !e.td));
        assert_eq!(problems.len(), 1);
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn gemini_is_read_and_edited_where_gemini_cli_home_points() {
        let home = std::env::temp_dir().join(format!("td-mcp-gemini-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let work = home.join("gemini-work");
        std::fs::create_dir_all(home.join(".gemini")).unwrap();
        std::fs::create_dir_all(work.join(".gemini")).unwrap();
        std::fs::create_dir_all(home.join(".local/lib/terminal-delight")).unwrap();
        let build = home.join(".local/lib/terminal-delight/td-x");
        std::fs::write(&build, b"\x7fELF").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&build, std::fs::Permissions::from_mode(0o755)).unwrap();
        let td = format!(
            r#"{{"mcpServers":{{"terminal-delight":{{"command":"{}","args":["mcp"]}}}}}}"#,
            build.display()
        );
        let other =
            r#"{"mcpServers":{"terminal-delight":{"command":"/opt/mine/server","args":[]}}}"#;
        // TD's entry where GEMINI_CLI_HOME points, a stranger's in the default.
        std::fs::write(work.join(".gemini/settings.json"), &td).unwrap();
        std::fs::write(home.join(".gemini/settings.json"), other).unwrap();
        let h = home.clone();
        let roots = Roots::from_env(&move |k| (k == "HOME").then(|| OsString::from(&h))).unwrap();
        let w = OsString::from(&work);
        let (found, _) = discover(
            &roots,
            &move |k| (k == "GEMINI_CLI_HOME").then(|| w.clone()),
            &[build.clone()],
        );
        let g = found.iter().find(|e| e.client == Client::Gemini).unwrap();
        assert_eq!(g.file, work.join(".gemini/settings.json"));
        assert!(g.td);
        let argv = g.remove_argv(&home).0;
        assert_eq!(
            argv[..2],
            [
                "/usr/bin/env".to_string(),
                format!("GEMINI_CLI_HOME={}", work.display())
            ]
        );
        // Without the variable the default file is read, and the command
        // unsets it so the worker's own environment cannot move the edit.
        let (found, _) = discover(&roots, &|_| None, &[build]);
        let g = found.iter().find(|e| e.client == Client::Gemini).unwrap();
        assert_eq!(g.file, home.join(".gemini/settings.json"));
        assert!(!g.td);
        assert_eq!(
            g.remove_argv(&home).0[..3],
            ["/usr/bin/env", "-u", "GEMINI_CLI_HOME"]
        );
        std::fs::remove_dir_all(home).unwrap();
    }
}
