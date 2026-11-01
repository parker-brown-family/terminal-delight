//! `terminal-delight briefing` — tell an agent the person started by TYPING
//! what a LAUNCH AGENT pane is told, and the hook that asks for it.
//!
//! Terminal Delight's first-launch page tells a newcomer to *type `claude` at a
//! prompt*, and until this module an agent started that way was told nothing:
//! the launcher's briefing (the workbench, how to present work, and the
//! BRIEFS AND JUDGEMENT CALLS paragraph naming the kit) only ever rode on a command line
//! the launcher wrote itself, as `--append-system-prompt`. A person who typed
//! got an agent that had never heard of the bench, and a bench that never
//! filled.
//!
//! Claude Code runs a `SessionStart` hook at the start of every session and
//! hands what it prints to the model as context. So the same briefing reaches a
//! typed agent through one hook that runs this verb:
//!
//! - [`briefing_for`] prints exactly what the launcher writes into a
//!   LAUNCH AGENT pane's `briefing.txt` ([`crate::launcher::session_briefing`]
//!   is the one function both call), after laying the kit down the way the
//!   launcher does. It prints NOTHING outside a Terminal Delight pane (no
//!   `TD_SESSION`), and nothing in a pane the launcher already briefed (its
//!   command line carries [`crate::launcher::BRIEFED_ENV`]), so the hook is
//!   safe in every Claude session on the machine.
//! - [`install`] and [`uninstall`] merge that hook into Claude Code's
//!   `settings.json` the way `scripts/install-agent-hooks.sh` merges its own:
//!   appended beside whatever is there, a timestamped backup first, idempotent,
//!   and taken out again without touching anyone else's hooks. The AppImage
//!   carries no source tree, so the binary has to be the installer.
//! - Nothing writes `settings.json` until the person says yes: the window
//!   offers it once, on the bench of an agent it did not brief
//!   ([`offer_due`]), and the answer, yes or no, is remembered in
//!   `briefing.toml` beside the window's other settings ([`answered`]). Its
//!   own file rather than a field of `launch.toml`, because the window holds
//!   that one in memory and writes it whole on every press, which would erase
//!   an answer written behind its back.
//!
//! What it never does: type into a terminal (it writes one JSON file, never a
//! pty — `docs/security/terminal-input.md` needs no row), edit any hook it did
//! not write, or write a `settings.json` it cannot parse.

use std::path::{Path, PathBuf};

/// What marks the one hook entry this module owns, inside its command. A
/// shell comment, so it rides in the command string Claude Code runs with no
/// effect, and `uninstall` finds exactly its own entry and nobody else's.
pub const MARKER: &str = "# terminal-delight:briefing";

/// The briefing a typed agent is owed, or `None` when it is owed nothing.
///
/// `env` reads the hook's environment, which is the agent's, which is the
/// pane's. `surfaces_root` is [`crate::surfacefeed::surfaces_root`] and
/// `kit_root` [`crate::briefkit::default_root`], passed in so a test can point
/// them somewhere of its own.
///
/// Without a kit the briefing still goes out, minus its BRIEFS AND JUDGEMENT
/// CALLS paragraph — the same rule [`crate::surface::launch_briefing`] applies,
/// because an agent sent to read a file that is not there does worse than one
/// never sent.
pub fn briefing_for(
    env: impl Fn(&str) -> Option<String>,
    surfaces_root: &Path,
    kit_root: Option<&Path>,
) -> Option<String> {
    let session = env("TD_SESSION").filter(|s| !s.trim().is_empty())?;
    if env(crate::launcher::BRIEFED_ENV).as_deref() == Some("1") {
        return None;
    }
    let dir = surfaces_root.join(crate::surfacefeed::sanitise(&session));
    let skill = kit_root.and_then(|root| crate::briefkit::write(root).ok());
    Some(crate::launcher::session_briefing(&dir, skill.as_deref()))
}

/// Where Claude Code keeps its user settings: `$CLAUDE_SETTINGS` (the name
/// `install-agent-hooks.sh` already answers to), else
/// `$CLAUDE_CONFIG_DIR/settings.json`, else `~/.claude/settings.json`.
pub fn settings_path(env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let set = |k: &str| env(k).filter(|v| !v.is_empty());
    if let Some(p) = set("CLAUDE_SETTINGS") {
        return Some(PathBuf::from(p));
    }
    if let Some(d) = set("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(d).join("settings.json"));
    }
    set("HOME").map(|h| PathBuf::from(h).join(".claude/settings.json"))
}

/// Whether the hook is in a settings file. Three answers, because *we could
/// not read it* is not *it is not there*: a file that does not parse is never
/// offered a write, and never counts as a yes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookState {
    Installed,
    Absent,
    Unreadable(String),
}

pub fn status(settings: &Path) -> HookState {
    match read_settings(settings) {
        Ok(None) => HookState::Absent,
        Ok(Some(v)) if has_ours(&v) => HookState::Installed,
        Ok(Some(_)) => HookState::Absent,
        Err(e) => HookState::Unreadable(e),
    }
}

/// What an install or uninstall did. `backup` is the copy kept of the file as
/// it was, when there was a file to copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Changed { backup: Option<PathBuf> },
    Unchanged,
}

/// Add the hook, or refresh its command if the binary has moved since.
///
/// `command` is [`hook_command`]'s. An entry already carrying exactly that
/// command is left alone and nothing is written, so running this twice is
/// running it once.
pub fn install(settings: &Path, command: &str, now_secs: u64) -> Result<Outcome, String> {
    let existed = settings.exists();
    let mut root = read_settings(settings)?.unwrap_or_else(|| serde_json::json!({}));
    if ours(&root).iter().any(|c| c == command) && ours(&root).len() == 1 {
        return Ok(Outcome::Unchanged);
    }
    let obj = root
        .as_object_mut()
        .ok_or("its top level is not a JSON object")?;
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or("its \"hooks\" is not a JSON object")?;
    let starts = hooks
        .entry("SessionStart")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .ok_or("its \"hooks.SessionStart\" is not a JSON array")?;
    strip_ours(starts);
    starts.push(serde_json::json!({
        "hooks": [{ "type": "command", "command": command, "timeout": 15 }]
    }));
    let backup = existed.then(|| backup_path(settings, now_secs));
    write_settings(settings, &root, backup.as_deref())?;
    Ok(Outcome::Changed { backup })
}

/// Take the hook out again, and nothing else. A `SessionStart` list it leaves
/// empty goes with it; every other hook, and every other key, stays as found.
pub fn uninstall(settings: &Path, now_secs: u64) -> Result<Outcome, String> {
    let Some(mut root) = read_settings(settings)? else {
        return Ok(Outcome::Unchanged);
    };
    if !has_ours(&root) {
        return Ok(Outcome::Unchanged);
    }
    if let Some(hooks) = root.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        let empty = match hooks.get_mut("SessionStart").and_then(|s| s.as_array_mut()) {
            Some(starts) => {
                strip_ours(starts);
                starts.is_empty()
            }
            None => false,
        };
        if empty {
            hooks.remove("SessionStart");
        }
    }
    let backup = backup_path(settings, now_secs);
    write_settings(settings, &root, Some(&backup))?;
    Ok(Outcome::Changed {
        backup: Some(backup),
    })
}

/// The command the hook runs: this binary, by the path a person would use to
/// reach it, then `briefing`, made silent if that binary has since gone.
///
/// A missing binary must not put an error in front of every Claude session on
/// the machine, so stderr is dropped and the exit is always 0. Quoted for a
/// POSIX shell, which is what Claude Code hands a hook command to.
pub fn hook_command(exe: &Path) -> String {
    format!(
        "{} briefing 2>/dev/null || true  {MARKER}",
        shell_quote(&exe.to_string_lossy())
    )
}

/// The path the hook should name for this binary.
///
/// An AppImage runs from a mount that changes on every launch, so `$APPIMAGE`
/// (the file the person downloaded) wins. Otherwise the `terminal-delight` on
/// `PATH`, when it is this very binary: an install that swaps the build behind
/// a stable symlink then reaches the new build without the hook being
/// rewritten. Otherwise this binary's own path.
pub fn hook_exe(env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    if let Some(app) = env("APPIMAGE")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && p.is_file())
    {
        return Some(app);
    }
    let me = std::env::current_exe().ok()?;
    let real = std::fs::canonicalize(&me).unwrap_or_else(|_| me.clone());
    let on_path = env("PATH").into_iter().flat_map(|p| {
        p.split(':')
            .filter(|d| !d.is_empty())
            .map(|d| Path::new(d).join("terminal-delight"))
            .collect::<Vec<_>>()
    });
    for candidate in on_path {
        if std::fs::canonicalize(&candidate).is_ok_and(|c| c == real) {
            return Some(candidate);
        }
    }
    Some(real)
}

/// Whether the window should offer the hook at all, right now: nobody has
/// answered, and the settings file says it is not there. Cached for a few
/// seconds, because the bench asks on every frame it draws an unbriefed agent
/// and the answer changes only when somebody writes one of two files.
pub fn offer_due() -> bool {
    use std::time::Duration;
    let mut cache = OFFER.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, due)) = *cache {
        if at.elapsed() < Duration::from_secs(3) {
            return due;
        }
    }
    let due = answered().is_none()
        && settings_path(|k| std::env::var(k).ok())
            .is_some_and(|p| status(&p) == HookState::Absent);
    *cache = Some((std::time::Instant::now(), due));
    due
}

static OFFER: std::sync::Mutex<Option<(std::time::Instant, bool)>> = std::sync::Mutex::new(None);

/// Forget [`offer_due`]'s cached answer, after this window wrote one of the
/// two files it reads, so every bench stops offering on its next frame.
pub fn forget_offer() {
    *OFFER.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// The person's answer to the offer, carried out: yes installs the hook and
/// remembers the yes; no remembers the no. Either way the offer is not made
/// again. A failed install is NOT remembered as a yes, so the offer comes back.
pub fn answer(yes: bool) -> Result<(), String> {
    let env = |k: &str| std::env::var(k).ok();
    answer_in(
        settings_path(env).as_deref(),
        hook_exe(env).as_deref(),
        &answer_path(),
        yes,
        now_secs(),
    )?;
    forget_offer();
    Ok(())
}

/// [`answer`] over explicit paths. A no writes the answer file and nothing
/// else; only a yes touches the settings file.
fn answer_in(
    settings: Option<&Path>,
    exe: Option<&Path>,
    answer_file: &Path,
    yes: bool,
    now_secs: u64,
) -> Result<(), String> {
    if yes {
        let settings = settings.ok_or("no home directory to find Claude Code's settings in")?;
        let exe = exe.ok_or("could not find this program's own path")?;
        install(settings, &hook_command(exe), now_secs)?;
    }
    remember(answer_file, yes);
    Ok(())
}

/// Whether a pane's bench shows the offer card. Only for a Claude agent
/// known to have NOT been briefed — *could not tell* (`None`) offers nothing,
/// and neither does a launched agent. Then either the offer is due
/// ([`offer_due`], asked last because it reads files), or the person already
/// answered on this pane and the card is saying what came of it.
pub fn offer_shows(
    is_claude: bool,
    briefed: Option<bool>,
    answered_here: bool,
    due: impl FnOnce() -> bool,
) -> bool {
    is_claude && briefed == Some(false) && (answered_here || due())
}

/// What the person said to the offer: `None` while nobody has answered,
/// which is a different fact from a no.
pub fn answered() -> Option<bool> {
    answered_in(&answer_path())
}

fn answer_path() -> PathBuf {
    crate::instance::config_dir().join("briefing.toml")
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
struct Answer {
    /// Tell agents the person starts by typing? `None` = never asked.
    typed: Option<bool>,
}

fn answered_in(path: &Path) -> Option<bool> {
    let text = std::fs::read_to_string(path).ok()?;
    toml::from_str::<Answer>(&text).ok()?.typed
}

fn remember(path: &Path, yes: bool) {
    if let Ok(body) = toml::to_string(&Answer { typed: Some(yes) }) {
        let _ = crate::session::write_atomic(path, &body);
    }
}

pub fn run_cli(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        None => {
            let env = |k: &str| std::env::var(k).ok();
            let kit = crate::briefkit::default_root();
            if let Some(text) =
                briefing_for(env, &crate::surfacefeed::surfaces_root(), kit.as_deref())
            {
                print!("{text}");
            }
            0
        }
        Some("hook") => hook_cli(args.get(1).map(String::as_str)),
        Some(other) => {
            eprintln!("terminal-delight briefing: unknown argument `{other}`\n\n{USAGE}");
            2
        }
    }
}

const USAGE: &str = "\
Usage:
  terminal-delight briefing              print the briefing a LAUNCH AGENT pane gets, for this pane
                                         (nothing outside a Terminal Delight pane, or in a pane
                                         the launcher already briefed)
  terminal-delight briefing hook status  whether Claude Code runs it at the start of every session
  terminal-delight briefing hook on      add that SessionStart hook to Claude Code's settings
  terminal-delight briefing hook off     take it out again";

fn hook_cli(verb: Option<&str>) -> i32 {
    let env = |k: &str| std::env::var(k).ok();
    let Some(settings) = settings_path(env) else {
        eprintln!("terminal-delight briefing: no HOME, so no Claude Code settings to read");
        return 1;
    };
    let shown = settings.display();
    match verb.unwrap_or("status") {
        "status" => match status(&settings) {
            HookState::Installed => {
                println!("on — {shown} runs `terminal-delight briefing` at the start of every Claude session");
                0
            }
            HookState::Absent => {
                println!("off — {shown} has no briefing hook");
                0
            }
            HookState::Unreadable(e) => {
                eprintln!("unknown — {shown} could not be read: {e}");
                1
            }
        },
        "on" => {
            let Some(exe) = hook_exe(env) else {
                eprintln!("terminal-delight briefing: could not find this program's own path");
                return 1;
            };
            match install(&settings, &hook_command(&exe), now_secs()) {
                Ok(Outcome::Unchanged) => println!("already on in {shown}"),
                Ok(Outcome::Changed { backup }) => {
                    println!("on — added the SessionStart hook to {shown}");
                    if let Some(b) = backup {
                        println!("the file as it was is kept as {}", b.display());
                    }
                }
                Err(e) => {
                    eprintln!("terminal-delight briefing: left {shown} as it was: {e}");
                    return 1;
                }
            }
            0
        }
        "off" => match uninstall(&settings, now_secs()) {
            Ok(Outcome::Unchanged) => {
                println!("already off in {shown}");
                0
            }
            Ok(Outcome::Changed { backup }) => {
                println!("off — took the briefing hook out of {shown}");
                if let Some(b) = backup {
                    println!("the file as it was is kept as {}", b.display());
                }
                0
            }
            Err(e) => {
                eprintln!("terminal-delight briefing: left {shown} as it was: {e}");
                1
            }
        },
        other => {
            eprintln!("terminal-delight briefing hook: unknown `{other}`\n\n{USAGE}");
            2
        }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The file as JSON, `None` when there is no file, and an error for a file
/// that is there and is not a JSON object — which this module never writes.
fn read_settings(path: &Path) -> Result<Option<serde_json::Value>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    if text.trim().is_empty() {
        return Ok(None);
    }
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("it is not valid JSON ({e})"))?;
    if !v.is_object() {
        return Err("its top level is not a JSON object".into());
    }
    Ok(Some(v))
}

/// Every command of ours in the file's `SessionStart` hooks.
fn ours(root: &serde_json::Value) -> Vec<String> {
    root.pointer("/hooks/SessionStart")
        .and_then(|s| s.as_array())
        .into_iter()
        .flatten()
        .filter_map(|group| group.get("hooks").and_then(|h| h.as_array()))
        .flatten()
        .filter_map(|h| h.get("command").and_then(|c| c.as_str()))
        .filter(|c| c.contains(MARKER))
        .map(str::to_string)
        .collect()
}

fn has_ours(root: &serde_json::Value) -> bool {
    !ours(root).is_empty()
}

/// Remove our hook from each group, and a group it leaves with no hooks.
fn strip_ours(starts: &mut Vec<serde_json::Value>) {
    let is_ours = |h: &serde_json::Value| {
        h.get("command")
            .and_then(|c| c.as_str())
            .is_some_and(|c| c.contains(MARKER))
    };
    starts.retain_mut(
        |group| match group.get_mut("hooks").and_then(|h| h.as_array_mut()) {
            Some(hooks) => {
                let had = hooks.len();
                hooks.retain(|h| !is_ours(h));
                !(hooks.is_empty() && had > 0)
            }
            None => true,
        },
    );
}

fn backup_path(settings: &Path, now_secs: u64) -> PathBuf {
    let name = settings
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "settings.json".into());
    settings.with_file_name(format!("{name}.td-backup.{now_secs}"))
}

/// Write `root` over `path`: the backup first, then the new text beside the
/// file and renamed over it, so a reader never sees half a settings file. The
/// file keeps the mode it had.
fn write_settings(
    path: &Path,
    root: &serde_json::Value,
    backup: Option<&Path>,
) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mode = std::fs::metadata(path).ok().map(|m| m.permissions().mode());
    if let Some(b) = backup {
        std::fs::copy(path, b).map_err(|e| format!("could not keep a backup: {e}"))?;
    }
    let mut text = serde_json::to_string_pretty(root).map_err(|e| e.to_string())?;
    text.push('\n');
    let tmp = path.with_file_name(format!(
        ".{}.td-{}.part",
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        std::process::id()
    ));
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    if let Some(m) = mode {
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(m));
    }
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn shell_quote(raw: &str) -> String {
    format!("'{}'", raw.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "td-briefhook-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    /// The whole point: what a typed agent is told is, byte for byte, what a
    /// LAUNCH AGENT pane in the same session is told. A second wording would
    /// be a second briefing to keep in step, and the first to drift.
    #[test]
    fn a_typed_agent_is_told_exactly_what_the_launcher_writes() {
        let d = scratch("same");
        let surfaces = d.join("surfaces");
        let kit = d.join("kit");
        // A key the sanitiser changes, so a hook that skipped it would name
        // a directory the window never watches.
        let session_dir = surfaces.join(crate::surfacefeed::sanitise("special:work"));
        let recipe = crate::launcher::Recipe {
            harness: crate::launcher::Harness::Claude,
            model: None,
            effort: None,
            reach: crate::launcher::Reach::Anywhere,
            cwd: d.clone(),
            opener: None,
        };
        let written = recipe.prepare_briefing(&session_dir, &kit).unwrap();
        let launcher = std::fs::read_to_string(written).unwrap();
        let typed = briefing_for(
            env_of(&[("TD_SESSION", "special:work"), ("TD_PANE_ID", "7")]),
            &surfaces,
            Some(&kit),
        )
        .expect("a pane with a session is told");
        assert_eq!(typed, launcher);
        assert!(typed.contains("BRIEFS AND JUDGEMENT CALLS"), "{typed}");
        assert!(typed.contains(&kit.join("briefing/SKILL.md").display().to_string()));
    }

    /// Outside a Terminal Delight pane the hook runs in every Claude session
    /// on the machine, and must say nothing in any of them.
    #[test]
    fn outside_a_pane_it_says_nothing() {
        let d = scratch("outside");
        for env in [env_of(&[]), env_of(&[("TD_SESSION", "  ")])] {
            assert_eq!(briefing_for(env, &d, Some(&d.join("kit"))), None);
        }
        assert!(
            !d.join("kit").exists(),
            "nothing is written for an agent that is told nothing"
        );
    }

    /// A pane the launcher started already has the briefing in its system
    /// prompt; a second copy from the hook would be the same page twice.
    #[test]
    fn a_pane_the_launcher_briefed_is_not_briefed_twice() {
        let d = scratch("twice");
        let env = env_of(&[("TD_SESSION", "work"), (crate::launcher::BRIEFED_ENV, "1")]);
        assert_eq!(briefing_for(env, &d, Some(&d.join("kit"))), None);
    }

    /// No kit, still a briefing — about the bench, without the paragraph that
    /// would send the agent to a file that is not there.
    #[test]
    fn without_a_kit_the_briefing_leaves_out_the_brief_paragraph() {
        let d = scratch("nokit");
        let text = briefing_for(env_of(&[("TD_SESSION", "work")]), &d, None).unwrap();
        assert!(text.contains("WORKBENCH"), "{text}");
        assert!(!text.contains("BRIEFS AND JUDGEMENT CALLS"), "{text}");
    }

    #[test]
    fn the_settings_file_is_found_where_claude_code_looks() {
        let p = |pairs: &[(&str, &str)]| settings_path(env_of(pairs));
        assert_eq!(
            p(&[("HOME", "/h")]),
            Some(PathBuf::from("/h/.claude/settings.json"))
        );
        assert_eq!(
            p(&[("HOME", "/h"), ("CLAUDE_CONFIG_DIR", "/c")]),
            Some(PathBuf::from("/c/settings.json"))
        );
        assert_eq!(
            p(&[
                ("HOME", "/h"),
                ("CLAUDE_CONFIG_DIR", "/c"),
                ("CLAUDE_SETTINGS", "/s.json")
            ]),
            Some(PathBuf::from("/s.json"))
        );
        assert_eq!(p(&[]), None);
    }

    const THEIRS: &str = r#"{
  "model": "opus",
  "hooks": {
    "SessionStart": [
      { "hooks": [ { "type": "command", "command": "their-boot" } ] }
    ],
    "Stop": [ { "hooks": [ { "type": "command", "command": "their-stop" } ] } ]
  },
  "zeta": true
}
"#;

    /// Appended beside what is there, the file kept first, every other key and
    /// hook untouched and in its order — and a second run writes nothing.
    #[test]
    fn install_appends_keeps_a_backup_and_is_idempotent() {
        let d = scratch("install");
        let s = d.join("settings.json");
        std::fs::write(&s, THEIRS).unwrap();
        let cmd = hook_command(Path::new("/opt/td/terminal-delight"));
        let out = install(&s, &cmd, 1000).unwrap();
        let backup = d.join("settings.json.td-backup.1000");
        assert_eq!(
            out,
            Outcome::Changed {
                backup: Some(backup.clone())
            }
        );
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), THEIRS);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&s).unwrap()).unwrap();
        let starts = v
            .pointer("/hooks/SessionStart")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(starts.len(), 2, "theirs, then ours");
        assert_eq!(starts[0].pointer("/hooks/0/command").unwrap(), "their-boot");
        assert_eq!(starts[1].pointer("/hooks/0/command").unwrap(), cmd.as_str());
        assert_eq!(
            v.pointer("/hooks/Stop/0/hooks/0/command").unwrap(),
            "their-stop"
        );
        let keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["model", "hooks", "zeta"], "key order survives");
        assert_eq!(status(&s), HookState::Installed);

        let before = std::fs::read_to_string(&s).unwrap();
        assert_eq!(install(&s, &cmd, 2000).unwrap(), Outcome::Unchanged);
        assert_eq!(std::fs::read_to_string(&s).unwrap(), before);
        assert!(!d.join("settings.json.td-backup.2000").exists());
    }

    /// A binary that moved gets its hook rewritten, not a second one beside it.
    #[test]
    fn a_moved_binary_replaces_its_hook_rather_than_adding_another() {
        let d = scratch("moved");
        let s = d.join("settings.json");
        install(&s, &hook_command(Path::new("/old/terminal-delight")), 1).unwrap();
        install(&s, &hook_command(Path::new("/new/terminal-delight")), 2).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&s).unwrap()).unwrap();
        assert_eq!(
            ours(&v),
            vec![hook_command(Path::new("/new/terminal-delight"))]
        );
    }

    /// Off takes out ours and only ours, and leaves the file as it was found
    /// otherwise.
    #[test]
    fn uninstall_takes_out_only_ours() {
        let d = scratch("uninstall");
        let s = d.join("settings.json");
        std::fs::write(&s, THEIRS).unwrap();
        install(&s, &hook_command(Path::new("/opt/td")), 1).unwrap();
        assert!(matches!(uninstall(&s, 2).unwrap(), Outcome::Changed { .. }));
        let after: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&s).unwrap()).unwrap();
        let theirs: serde_json::Value = serde_json::from_str(THEIRS).unwrap();
        assert_eq!(after, theirs);
        assert_eq!(status(&s), HookState::Absent);
        assert_eq!(uninstall(&s, 3).unwrap(), Outcome::Unchanged);
    }

    /// A settings file with only our hook comes back to no SessionStart at all.
    #[test]
    fn uninstall_drops_a_session_start_list_it_empties() {
        let d = scratch("empties");
        let s = d.join("settings.json");
        install(&s, &hook_command(Path::new("/opt/td")), 1).unwrap();
        uninstall(&s, 2).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&s).unwrap()).unwrap();
        assert_eq!(v, serde_json::json!({ "hooks": {} }));
    }

    /// A file that does not parse is somebody's, half-edited: it is never
    /// overwritten, and its state is unknown rather than absent.
    #[test]
    fn a_file_that_does_not_parse_is_never_written() {
        let d = scratch("broken");
        let s = d.join("settings.json");
        std::fs::write(&s, "{ \"model\": ").unwrap();
        assert!(install(&s, &hook_command(Path::new("/opt/td")), 1).is_err());
        assert_eq!(std::fs::read_to_string(&s).unwrap(), "{ \"model\": ");
        assert!(matches!(status(&s), HookState::Unreadable(_)));
        std::fs::write(&s, "[]").unwrap();
        assert!(install(&s, &hook_command(Path::new("/opt/td")), 1).is_err());
    }

    /// The command quotes a path with spaces and an apostrophe, carries the
    /// marker, and can never fail a session.
    #[test]
    fn the_hook_command_is_quoted_silent_and_marked() {
        let c = hook_command(Path::new("/home/o'neil/My Apps/td.AppImage"));
        assert!(
            c.starts_with(r"'/home/o'\''neil/My Apps/td.AppImage' briefing"),
            "{c}"
        );
        assert!(c.contains("2>/dev/null || true"), "{c}");
        assert!(c.ends_with(MARKER), "{c}");
    }

    /// Never asked, a no and a yes are three answers, and a file nobody wrote
    /// is the first of them.
    #[test]
    fn the_answer_to_the_offer_has_three_states() {
        let d = scratch("answer");
        let p = d.join("briefing.toml");
        assert_eq!(answered_in(&p), None);
        remember(&p, false);
        assert_eq!(answered_in(&p), Some(false));
        remember(&p, true);
        assert_eq!(answered_in(&p), Some(true));
        std::fs::write(&p, "typed = ").unwrap();
        assert_eq!(
            answered_in(&p),
            None,
            "a file that does not parse is not an answer"
        );
    }

    /// The offer is made only on an agent known to be unbriefed, and only
    /// while it is due — unless this pane already answered, when the card
    /// stays to say what came of it.
    #[test]
    fn the_offer_shows_only_on_an_agent_known_to_be_unbriefed() {
        let never = || panic!("due is not asked when the answer is already no");
        assert!(
            !offer_shows(true, None, false, never),
            "could not tell: no offer"
        );
        assert!(
            !offer_shows(true, Some(true), false, never),
            "launched: no offer"
        );
        assert!(
            !offer_shows(false, Some(false), false, never),
            "not claude: no offer"
        );
        assert!(offer_shows(true, Some(false), false, || true));
        assert!(!offer_shows(true, Some(false), false, || false));
        assert!(
            offer_shows(true, Some(false), true, || false),
            "says what came of it"
        );
    }

    /// No writes the answer and nothing else; yes writes the hook too. Until
    /// one of them is pressed, nothing is written anywhere.
    #[test]
    fn a_no_never_touches_the_settings_file_and_a_yes_does() {
        let d = scratch("answer-in");
        let s = d.join("settings.json");
        let a = d.join("briefing.toml");
        assert_eq!(status(&s), HookState::Absent);
        assert!(!s.exists() && !a.exists(), "asking is not writing");
        answer_in(Some(&s), Some(Path::new("/opt/td")), &a, false, 1).unwrap();
        assert!(!s.exists(), "a no writes no settings");
        assert_eq!(answered_in(&a), Some(false));
        answer_in(Some(&s), Some(Path::new("/opt/td")), &a, true, 2).unwrap();
        assert_eq!(status(&s), HookState::Installed);
        assert_eq!(answered_in(&a), Some(true));
    }

    /// A yes that cannot write the hook is not remembered as a yes.
    #[test]
    fn a_failed_yes_is_not_remembered() {
        let d = scratch("failed-yes");
        let s = d.join("settings.json");
        std::fs::write(&s, "not json").unwrap();
        let a = d.join("briefing.toml");
        assert!(answer_in(Some(&s), Some(Path::new("/opt/td")), &a, true, 1).is_err());
        assert_eq!(answered_in(&a), None);
    }

    /// A missing binary, as a real shell runs it: no output, exit 0.
    #[test]
    fn a_hook_whose_binary_is_gone_is_silent_and_succeeds() {
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(hook_command(Path::new("/nonexistent/terminal-delight")))
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        assert!(out.stdout.is_empty() && out.stderr.is_empty(), "{out:?}");
    }
}
