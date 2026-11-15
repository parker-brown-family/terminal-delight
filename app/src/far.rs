//! Another machine's Terminal Delight, reached over ssh.
//!
//! Terminal Delight already splits in two on one machine: a session host that
//! owns the terminals, and a window that draws them over the host's socket
//! (`docs/plans/client-server/`). This module carries that same socket across
//! machines. `ssh -L` forwards a far host's `session-<key>.sock` to a socket
//! here, and a window attaches to it exactly as it attaches to a local one, so
//! what crosses the network is terminal state and keystrokes, never video.
//!
//! T3 Code is the specimen (`docs/internals/remote.md` there): an environment
//! owns the agents and their state, a client reaches it by a route, and the
//! route changes nothing about where the work runs. Their SSH route launches a
//! server and forwards a port. Ours forwards a socket that is already there,
//! and ssh is the whole of the authentication: the far host checks that its
//! peer is the same user, and an ssh login as that user is one.
//!
//! Three rules hold this together, each because the cheap version is wrong:
//!
//! - **A far session never starts a host here.** A window told to show
//!   `farbox`'s session that cannot reach it says so and stops. Starting a
//!   local host under that name would run the far layout's agents on this
//!   machine, in this machine's folders.
//! - **A far pane has no pid here.** The host reports the pid its shell has on
//!   the far machine. Reading `/proc` for it on this one returns nothing, or a
//!   stranger. A far window drops the number where it arrives, so everything
//!   keyed on it reads unknown.
//! - **The forwarded socket is kept out of the directory local sessions are
//!   found in.** A plain launch scans that directory for a host nobody is
//!   watching and adopts it, and a process inside a pane asks every socket
//!   there for its host's pid. A far host belongs in neither search.
//!
//! Pure where it can be: the scripts a far shell runs, what they print and how
//! it is read are all tested without a second machine. The forward itself is
//! one `ssh` child this window owns and restarts.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::addproject;
use crate::entry::{self, Machine};
use crate::instance;

/// The machine a far window shows, as the composer's chip names it.
pub const ENV_NAME: &str = "TD_FAR_NAME";
/// Its ssh destination.
pub const ENV_DEST: &str = "TD_FAR_DEST";
/// The session on that machine, by the key its own host uses.
pub const ENV_SESSION: &str = "TD_FAR_SESSION";

/// One session on another machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FarSession {
    /// What a person calls the machine (`farbox`).
    pub name: String,
    /// The ssh destination (`far-box`).
    pub dest: String,
    /// The session's key on the far machine (`2`).
    pub session: String,
}

impl FarSession {
    /// From the three words the `remote` verb leaves in the environment. None
    /// unless all three are present and each is something this build would
    /// have written: a destination ssh could take for an option, or a key
    /// that could walk out of a directory, is not a far session.
    pub fn from_env(get: &dyn Fn(&str) -> Option<String>) -> Option<FarSession> {
        let name = get(ENV_NAME)?;
        let dest = get(ENV_DEST)?;
        let session = get(ENV_SESSION)?;
        let name = name.trim().to_string();
        if name.is_empty() || !entry::valid_destination(&dest) || !key_ok(&session) {
            return None;
        }
        Some(FarSession {
            name,
            dest,
            session,
        })
    }

    /// The key this window binds locally: its theme, its ctl socket and its
    /// shadow of the far layout are filed under it. Never a key a local host
    /// serves, because `far-` plus a machine is not a name a workspace gives.
    pub fn local_key(&self) -> String {
        instance::sanitize_key(&format!("far-{}-{}", self.name, self.session))
    }

    /// Where the forwarded socket lives here.
    pub fn local_socket(&self, runtime: &Path) -> PathBuf {
        far_dir(runtime, &self.name).join(format!("session-{}.sock", self.session))
    }

    /// What the machine is called on screen: its name, in capitals, the way the
    /// left bar writes a project.
    pub fn badge(&self) -> String {
        self.name.to_uppercase()
    }
}

/// The far session this process is showing, if it is a far window. Read once:
/// the environment a window starts with is the whole of the claim.
///
/// A throwaway window started from a far one inherits its environment, and is
/// not a far window: it owns its own terminals on this machine. So a scratch,
/// seeded or demo launch reads as local whatever else the environment says.
pub fn current() -> Option<&'static FarSession> {
    static FAR: OnceLock<Option<FarSession>> = OnceLock::new();
    FAR.get_or_init(|| {
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        if [
            "TD_SCRATCH",
            "TD_SEED_CWD",
            "TD_SEED_RESUME",
            "TD_DEMO_STATE",
        ]
        .iter()
        .any(|k| get(k).is_some())
        {
            return None;
        }
        FarSession::from_env(&get)
    })
    .as_ref()
}

/// A session key as the far host would have minted it: what
/// [`instance::sanitize_key`] leaves alone. Anything else is refused rather
/// than cleaned, because the cleaned key names a different session.
pub fn key_ok(key: &str) -> bool {
    !key.is_empty() && instance::sanitize_key(key) == key
}

/// This user's runtime directory for Terminal Delight, as `hostproto` and
/// `ctl` both compute it.
pub fn runtime_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir).join("terminal-delight");
    }
    PathBuf::from(format!("/tmp/terminal-delight-{}", unsafe {
        libc::getuid()
    }))
}

/// Where one far machine's forwarded sockets are kept: a folder of its own
/// under the runtime directory, which no local scan reads.
pub fn far_dir(runtime: &Path, name: &str) -> PathBuf {
    runtime.join("far").join(instance::sanitize_key(name))
}

// ---- what a far shell is asked ----

/// Reports what the far machine is running, one fact per line:
///
/// ```text
/// host far-box
/// td /home/p/.local/bin/terminal-delight
/// version terminal-delight 0.5.3 (…)
/// build {"schema":"td-build/1",…}
/// runtime /run/user/1000/terminal-delight
/// session 2 window
/// session 7 idle
/// tailscale 100.123.197.50
/// gliff
/// done
/// ```
///
/// `window` says the session's record names a window whose process is alive,
/// `idle` that it does not. Neither proves the host answers: only attaching
/// does. A machine with no Terminal Delight prints no `td` line, and one
/// whose build predates `--build-info` prints no `build` line; each absence is
/// read as unknown. `done` is last, so a reply cut short is told from a
/// machine with nothing to report.
pub fn probe_script() -> String {
    r#"printf 'host %s\n' "$(hostname 2>/dev/null || uname -n)"
td=$(command -v terminal-delight 2>/dev/null)
if [ -z "$td" ] && [ -x "$HOME/.local/bin/terminal-delight" ]; then td="$HOME/.local/bin/terminal-delight"; fi
if [ -n "$td" ]; then
  printf 'td %s\n' "$td"
  v=$("$td" --version 2>/dev/null | head -n 1); [ -n "$v" ] && printf 'version %s\n' "$v"
  b=$("$td" --build-info 2>/dev/null | head -n 1); case $b in "{"*) printf 'build %s\n' "$b" ;; esac
fi
rt="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/terminal-delight"
printf 'runtime %s\n' "$rt"
for s in "$rt"/session-*.sock; do
  [ -S "$s" ] || continue
  k=${s##*/session-}; k=${k%.sock}
  w=idle
  if [ -r "$rt/session-$k.window" ]; then
    p=$(cat "$rt/session-$k.window" 2>/dev/null)
    case $p in ''|*[!0-9]*) ;; *) kill -0 "$p" 2>/dev/null && w=window ;; esac
  fi
  printf 'session %s %s\n' "$k" "$w"
done
if command -v tailscale >/dev/null 2>&1; then printf 'tailscale %s\n' "$(tailscale ip -4 2>/dev/null | head -n 1)"; fi
command -v gliff >/dev/null 2>&1 && echo gliff
echo done
"#
    .to_string()
}

/// One session the far machine listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedSession {
    pub key: String,
    /// Whether the far machine's own window is on it. `None` when the far
    /// side said a word this build does not know.
    pub window: Option<bool>,
}

/// What a far machine said about itself. Every field that the reply left out
/// is `None`: a machine that printed no version is not a machine at version
/// nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Probe {
    pub host: Option<String>,
    /// Where its `terminal-delight` is. `None`: not installed, or not found.
    pub td: Option<String>,
    /// Its version number alone (`0.5.3`).
    pub version: Option<String>,
    /// The fingerprint of the window-to-host wire it speaks.
    pub wire_proto: Option<String>,
    pub runtime: Option<String>,
    pub sessions: Vec<ListedSession>,
    /// Its tailnet address. `Some("")` is Tailscale installed and signed out.
    pub tailscale: Option<String>,
    pub gliff: bool,
}

/// Read [`probe_script`]'s reply. An error when the reply never reached its
/// last line: half a reply would list half the sessions as all of them.
pub fn parse_probe(out: &str) -> Result<Probe, String> {
    let mut p = Probe::default();
    let mut done = false;
    for line in out.lines() {
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        match word {
            "host" => p.host = Some(rest.to_string()).filter(|s| !s.is_empty()),
            "td" => p.td = Some(rest.to_string()).filter(|s| !s.is_empty()),
            "version" => p.version = version_number(rest),
            "build" => {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(rest) {
                    if let Some(w) = v.get("wire_proto").and_then(|w| w.as_str()) {
                        p.wire_proto = Some(w.to_string());
                    }
                    if p.version.is_none() {
                        p.version = v
                            .get("version")
                            .and_then(|w| w.as_str())
                            .map(str::to_string);
                    }
                }
            }
            "runtime" => p.runtime = Some(rest.to_string()).filter(|s| !s.is_empty()),
            "session" => {
                let (key, state) = rest.split_once(' ').unwrap_or((rest, ""));
                if key_ok(key) {
                    p.sessions.push(ListedSession {
                        key: key.to_string(),
                        window: match state {
                            "window" => Some(true),
                            "idle" => Some(false),
                            _ => None,
                        },
                    });
                }
            }
            "tailscale" => p.tailscale = Some(rest.to_string()),
            "gliff" => p.gliff = true,
            "done" => done = true,
            _ => {}
        }
    }
    if !done {
        return Err("the machine's answer was cut short".into());
    }
    p.sessions
        .sort_by(|a, b| natural(&a.key).cmp(&natural(&b.key)));
    Ok(p)
}

/// `terminal-delight 0.5.3 (terminal core: …)` → `0.5.3`.
fn version_number(line: &str) -> Option<String> {
    line.split_whitespace()
        .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()) && w.contains('.'))
        .map(str::to_string)
}

/// Sessions sort the way a person counts them: 2 before 10, names after
/// numbers.
fn natural(key: &str) -> (u8, u64, String) {
    match key.parse::<u64>() {
        Ok(n) => (0, n, String::new()),
        Err(_) => (1, 0, key.to_string()),
    }
}

/// Hands a far session to a window here: where its socket is, the tag its
/// panes carry, and its saved layout. `$1` is the session key.
///
/// ```text
/// socket /run/user/1000/terminal-delight/session-2.sock
/// tag a1b2c3d4
/// layout
/// <the session file, to the end>
/// ```
///
/// `missing` alone when no host has that session's socket. `nolayout` in
/// place of `layout` when the host has not saved one yet.
pub fn bootstrap_script() -> String {
    r#"k=$1
rt="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/terminal-delight"
if [ ! -S "$rt/session-$k.sock" ]; then echo missing; exit 0; fi
printf 'socket %s\n' "$rt/session-$k.sock"
if [ -r "$rt/session-$k.tag" ]; then printf 'tag %s\n' "$(head -n 1 "$rt/session-$k.tag")"; fi
f="${XDG_CONFIG_HOME:-$HOME/.config}/terminal-delight/sessions/$k.toml"
if [ -r "$f" ]; then echo layout; cat "$f"; else echo nolayout; fi
"#
    .to_string()
}

/// What [`bootstrap_script`] handed over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bootstrap {
    /// The host's socket on the far machine: what the forward points at.
    pub socket: String,
    /// The tag the far host stamped into its panes. `None` when it has not
    /// minted one, and then the window mints nothing in its place.
    pub tag: Option<String>,
    /// The far session file, as written. `None`: the host has saved none.
    pub layout: Option<String>,
}

/// Read a bootstrap reply. `Ok(None)` is the far side saying no host has that
/// session.
pub fn parse_bootstrap(out: &str) -> Result<Option<Bootstrap>, String> {
    let mut socket = None;
    let mut tag = None;
    let mut rest = out;
    loop {
        let (line, after) = match rest.split_once('\n') {
            Some(pair) => pair,
            None => (rest, ""),
        };
        match line.split_once(' ').unwrap_or((line, "")) {
            ("missing", _) => return Ok(None),
            ("socket", path) => socket = Some(path.trim().to_string()),
            ("tag", t) => {
                let t = t.trim();
                // A tag is typed in front of lines an agent trusts. Anything
                // but the plain word the host mints is not carried.
                if !t.is_empty() && t.len() <= 64 && t.chars().all(|c| c.is_ascii_alphanumeric()) {
                    tag = Some(t.to_string());
                }
            }
            ("layout", _) | ("nolayout", _) => {
                let socket = socket
                    .filter(|s: &String| s.starts_with('/') && !s.contains(['\n', ':']))
                    .ok_or("the machine did not say where its session host listens")?;
                return Ok(Some(Bootstrap {
                    socket,
                    tag,
                    layout: (line == "layout").then(|| after.to_string()),
                }));
            }
            _ => {}
        }
        if after.is_empty() {
            return Err("the machine's answer was cut short".into());
        }
        rest = after;
    }
}

/// The arguments of the one ssh that carries a far session's socket here.
///
/// A connection of its own, never the shared one the short questions ride:
/// the far host records this connection as the window watching the session,
/// and the far machine's own window takes its panes back when that process is
/// gone. A shared connection lingers after the window that used it, and the
/// far window would wait on nobody.
///
/// `ExitOnForwardFailure` so a forward that could not be set up is an exit
/// with a reason instead of a quiet connection that carries nothing, and
/// `StreamLocalBindUnlink` so a socket file left by a window that crashed does
/// not refuse the next one. The keepalive notices a dead link in about ten
/// seconds, which is what turns a closed laptop lid into "reconnecting".
pub fn forward_args(dest: &str, local: &Path, far: &str) -> Vec<String> {
    let mut args: Vec<String> = [
        "-N",
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        "StrictHostKeyChecking=yes",
        "-o",
        "ConnectTimeout=8",
        "-o",
        "ControlMaster=no",
        "-o",
        "ControlPath=none",
        "-o",
        "ExitOnForwardFailure=yes",
        "-o",
        "StreamLocalBindUnlink=yes",
        "-o",
        "ServerAliveInterval=4",
        "-o",
        "ServerAliveCountMax=2",
        "-L",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    args.push(format!("{}:{}", local.display(), far));
    // `--` so nothing after it is read as an option, whatever the destination.
    args.push("--".into());
    args.push(dest.to_string());
    args
}

/// Why a far machine could not be used, in words for the person who asked.
pub fn trouble_words(name: &str, dest: &str, stderr: &str) -> String {
    use addproject::SshTrouble as T;
    match addproject::ssh_trouble(stderr) {
        T::KeyUnknown => format!(
            "{name}: this machine has never met {dest}. Run `ssh {dest}` once in a terminal and answer its question, then try again."
        ),
        T::KeyChanged(_) => format!(
            "{name}: the key {dest} shows is not the one on file. Look into that before connecting."
        ),
        T::Other(last) if last.contains("Permission denied") => format!(
            "{name}: {dest} would not let this account in without a password. Set up a key for it (`ssh-copy-id {dest}`)."
        ),
        T::Other(last)
            if ["timed out", "No route to host", "Could not resolve", "Connection refused", "did not answer"]
                .iter()
                .any(|w| last.contains(w)) =>
        {
            format!("{name}: {dest} did not answer. Is it on, and on the network?")
        }
        T::Other(last) if last.is_empty() => {
            format!("{name}: ssh to {dest} failed without saying why.")
        }
        T::Other(last) => format!("{name}: {last}"),
    }
}

// ---- running ssh ----

/// How long a short question to a far machine may take before it is called
/// unreachable. ssh's own connect timeout is shorter; this covers a far shell
/// that connects and then says nothing.
pub const ASK_BUDGET: Duration = Duration::from_secs(20);

/// The program a far question runs through. `ssh`, unless a test or a rig
/// names a stand-in.
pub fn ssh_program() -> String {
    std::env::var("TD_SSH")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "ssh".into())
}

/// Run `script` on `dest` with `args`, over the shared connection, and return
/// what it printed. An error carries ssh's own complaint.
pub fn ask(dest: &str, script: &str, args: &[&str], budget: Duration) -> Result<String, String> {
    ask_via(&ssh_program(), dest, script, args, None, budget)
}

/// [`ask`], with `input` written to the far command's stdin.
pub fn ask_with(
    dest: &str,
    script: &str,
    args: &[&str],
    input: Option<&[u8]>,
    budget: Duration,
) -> Result<String, String> {
    ask_via(&ssh_program(), dest, script, args, input, budget)
}

/// [`ask_with`], through a named program: the seam a test hands a stand-in
/// ssh through, without setting a variable every other test would inherit.
pub fn ask_via(
    program: &str,
    dest: &str,
    script: &str,
    args: &[&str],
    input: Option<&[u8]>,
    budget: Duration,
) -> Result<String, String> {
    if !entry::valid_destination(dest) {
        return Err(format!("{dest:?} is not an ssh destination"));
    }
    let control = addproject::control_dir();
    let mut cmd = Command::new(program);
    cmd.args(addproject::ssh_args(dest, control.as_deref()))
        .arg(addproject::remote_command(script, args))
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("could not run ssh: {e}"))?;
    if let (Some(bytes), Some(mut stdin)) = (input, child.stdin.take()) {
        use std::io::Write;
        let _ = stdin.write_all(bytes);
    }
    // Drained on threads of their own: a reply larger than a pipe would
    // otherwise stall the far command against a reader that is only waiting.
    let mut out = child.stdout.take().expect("piped");
    let mut err = child.stderr.take().expect("piped");
    let out_t = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = out.read_to_string(&mut s);
        s
    });
    let err_t = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = err.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() > budget => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{dest} did not answer in {}s", budget.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(15)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let out = out_t.join().unwrap_or_default();
    let err = err_t.join().unwrap_or_default();
    if status.success() {
        Ok(out)
    } else if err.trim().is_empty() {
        Err(format!("the command on {dest} failed ({status})"))
    } else {
        Err(err)
    }
}

/// Ask a far machine what it is running.
pub fn probe(machine: &Machine) -> Result<Probe, String> {
    let dest = machine
        .ssh
        .as_deref()
        .ok_or("this machine is not a far one")?;
    let out = ask(dest, &probe_script(), &[], ASK_BUDGET)
        .map_err(|e| trouble_words(&machine.name, dest, &e))?;
    parse_probe(&out).map_err(|e| format!("{}: {e}", machine.name))
}

/// Which session `remote` opens when the person named none: the only one, or
/// the only one a window there is on. More than that is the person's choice.
pub fn pick_session(probe: &Probe, asked: Option<&str>) -> Result<String, String> {
    if let Some(key) = asked {
        return if probe.sessions.iter().any(|s| s.key == key) {
            Ok(key.to_string())
        } else {
            Err(format!(
                "no session {key:?} is running there{}",
                listed(&probe.sessions)
            ))
        };
    }
    match probe.sessions.as_slice() {
        [] => Err("no Terminal Delight session is running there".into()),
        [one] => Ok(one.key.clone()),
        many => {
            let windowed: Vec<_> = many.iter().filter(|s| s.window == Some(true)).collect();
            match windowed.as_slice() {
                [one] => Ok(one.key.clone()),
                _ => Err(format!(
                    "several sessions are running there; name one{}",
                    listed(many)
                )),
            }
        }
    }
}

fn listed(sessions: &[ListedSession]) -> String {
    if sessions.is_empty() {
        return String::new();
    }
    let keys: Vec<&str> = sessions.iter().map(|s| s.key.as_str()).collect();
    format!(" (running: {})", keys.join(", "))
}

/// Fetch what a window needs to show a far session, and put it where the
/// window will read it: the layout as this window's own session file, the tag
/// beside where the socket will be. Returns the far socket's path.
pub fn prepare(far: &FarSession, config: &Path, runtime: &Path) -> Result<String, String> {
    prepare_via(&ssh_program(), far, config, runtime)
}

/// [`prepare`], through a named program.
pub fn prepare_via(
    program: &str,
    far: &FarSession,
    config: &Path,
    runtime: &Path,
) -> Result<String, String> {
    let out = ask_via(
        program,
        &far.dest,
        &bootstrap_script(),
        &[&far.session],
        None,
        ASK_BUDGET,
    )
    .map_err(|e| trouble_words(&far.name, &far.dest, &e))?;
    let boot = parse_bootstrap(&out)
        .map_err(|e| format!("{}: {e}", far.name))?
        .ok_or_else(|| {
            format!(
                "{}: no session {:?} is running there any more",
                far.name, far.session
            )
        })?;
    let key = far.local_key();
    let sessions = config.join("sessions");
    private_dir(&sessions)?;
    let shadow = sessions.join(format!("{key}.toml"));
    match &boot.layout {
        Some(text) => write_private(&shadow, text.as_bytes())?,
        // No layout there is no layout here: a shadow left from an earlier
        // host would open tabs for panes that host took with it.
        None => {
            let _ = std::fs::remove_file(&shadow);
        }
    }
    private_dir(&far_dir(runtime, &far.name))?;
    let tag = runtime.join(format!("session-{key}.tag"));
    match &boot.tag {
        Some(t) => write_private(&tag, t.as_bytes())?,
        None => {
            let _ = std::fs::remove_file(&tag);
        }
    }
    Ok(boot.socket)
}

fn private_dir(dir: &Path) -> Result<(), String> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| format!("{}: {e}", tmp.display()))?;
    f.write_all(bytes)
        .map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Say something on the desktop, for a window that has no glass to say it on:
/// a far session that would not open, from a launch nobody is watching a
/// terminal for. Best effort; a desktop with no notifier loses nothing else.
pub fn tell_desktop(title: &str, body: &str) {
    let _ = Command::new("notify-send")
        .args([
            "-a",
            "Terminal Delight",
            "-i",
            "utilities-terminal",
            "-t",
            "12000",
            "--",
        ])
        .arg(title)
        .arg(body)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

// ---- the forward ----

/// The one ssh that carries a far session's socket here, owned by the window
/// that shows it.
pub struct Forward {
    far: FarSession,
    local: PathBuf,
    far_socket: String,
    log: PathBuf,
    child: Mutex<Option<Child>>,
}

/// How the forward stands, for the window's own notice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkState {
    Up,
    /// Down, with ssh's last word when it left one.
    Down(Option<String>),
}

impl Forward {
    /// Start carrying `far_socket` to this machine. Returns once ssh is
    /// running; [`Forward::wait_ready`] says whether the socket answers.
    pub fn start(far: &FarSession, runtime: &Path, far_socket: &str) -> Result<Forward, String> {
        let dir = far_dir(runtime, &far.name);
        let local = far.local_socket(runtime);
        // The kernel's limit on a socket's path, less its closing byte. ssh
        // would refuse the same path with a message about `sun_path`.
        if local.as_os_str().len() > 107 {
            return Err(format!(
                "{}: its name and session are too long to make a socket path here; give the machine a shorter name in machines.toml",
                far.name
            ));
        }
        private_dir(&dir)?;
        let fwd = Forward {
            far: far.clone(),
            local,
            far_socket: far_socket.to_string(),
            log: dir.join(format!("ssh-{}.log", far.session)),
            child: Mutex::new(None),
        };
        fwd.spawn()?;
        Ok(fwd)
    }

    fn spawn(&self) -> Result<(), String> {
        use std::os::unix::process::CommandExt;
        let log = std::fs::File::create(&self.log).map_err(|e| e.to_string())?;
        let mut cmd = Command::new(ssh_program());
        cmd.args(forward_args(&self.far.dest, &self.local, &self.far_socket))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log);
        // The forward dies with the window, however the window dies: the far
        // host counts this connection as the window watching its panes, and an
        // orphan would hold them from the far machine's own window for good.
        unsafe {
            cmd.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        let child = cmd.spawn().map_err(|e| format!("could not run ssh: {e}"))?;
        *self.child.lock().unwrap_or_else(|p| p.into_inner()) = Some(child);
        Ok(())
    }

    pub fn socket(&self) -> &Path {
        &self.local
    }

    pub fn far(&self) -> &FarSession {
        &self.far
    }

    /// Whether the ssh process is still running.
    pub fn running(&self) -> bool {
        let mut guard = self.child.lock().unwrap_or_else(|p| p.into_inner());
        match guard.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    /// ssh's last complaint, if it has left one.
    pub fn last_word(&self) -> Option<String> {
        let text = std::fs::read_to_string(&self.log).ok()?;
        Some(trouble_words(&self.far.name, &self.far.dest, &text))
            .filter(|_| !text.trim().is_empty())
    }

    /// Start ssh again if it has exited. Says whether it had to.
    pub fn revive(&self) -> Result<bool, String> {
        if self.running() {
            return Ok(false);
        }
        self.spawn()?;
        Ok(true)
    }

    /// Wait until the forwarded socket accepts a connection, or ssh gives up.
    pub fn wait_ready(&self, budget: Duration) -> Result<(), String> {
        let start = Instant::now();
        loop {
            if std::os::unix::net::UnixStream::connect(&self.local).is_ok() {
                return Ok(());
            }
            if !self.running() {
                return Err(self.last_word().unwrap_or_else(|| {
                    format!("{}: the connection closed before it was up", self.far.name)
                }));
            }
            if start.elapsed() > budget {
                return Err(format!(
                    "{}: {} did not answer in {}s",
                    self.far.name,
                    self.far.dest,
                    budget.as_secs()
                ));
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    }

    pub fn state(&self) -> LinkState {
        if self.running() && std::os::unix::net::UnixStream::connect(&self.local).is_ok() {
            LinkState::Up
        } else {
            LinkState::Down(self.last_word())
        }
    }

    fn stop(&self) {
        if let Some(mut child) = self.child.lock().unwrap_or_else(|p| p.into_inner()).take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Forward {
    fn drop(&mut self) {
        self.stop();
        let _ = std::fs::remove_file(&self.local);
    }
}

static FORWARD: OnceLock<Forward> = OnceLock::new();

/// This window's forward, once [`open`] has started it.
pub fn forward() -> Option<&'static Forward> {
    FORWARD.get()
}

/// How long a far session may take to answer when a window opens on it.
pub const OPEN_BUDGET: Duration = Duration::from_secs(15);

/// Bring this window's far session within reach: fetch its layout and tag,
/// start the forward, and wait for the host to answer through it. Returns the
/// socket to attach to. Called once, before the window attaches.
pub fn open(far: &FarSession) -> Result<PathBuf, String> {
    let runtime = runtime_dir();
    let far_socket = prepare(far, &instance::config_dir(), &runtime)?;
    let fwd = Forward::start(far, &runtime, &far_socket)?;
    let ready = fwd.wait_ready(OPEN_BUDGET);
    let socket = fwd.socket().to_path_buf();
    // Kept even when it failed, so the one static is never raced; a failed
    // open ends the process, and the forward with it.
    let _ = FORWARD.set(fwd);
    ready.map(|()| socket)
}

// ---- the two commands ----

const USAGE: &str =
    "usage: terminal-delight machines [--json]\n       terminal-delight remote <machine> [session]";

/// `terminal-delight machines`: every machine in `machines.toml`, and what
/// each says it is running.
pub fn run_machines_cli(args: &[String]) -> i32 {
    let json = match args {
        [] => false,
        [flag] if flag == "--json" => true,
        _ => {
            eprintln!("{USAGE}");
            return 2;
        }
    };
    let machines = match entry::load_machines(&instance::config_dir()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("terminal-delight machines: machines.toml: {e}");
            return 1;
        }
    };
    // Asked together: three machines that each take eight seconds to refuse
    // should cost eight seconds, not twenty-four.
    let probes: Vec<_> = machines
        .iter()
        .cloned()
        .map(|m| std::thread::spawn(move || probe(&m)))
        .collect();
    let probes: Vec<Result<Probe, String>> = probes
        .into_iter()
        .map(|t| t.join().unwrap_or_else(|_| Err("the probe stopped".into())))
        .collect();
    if json {
        let rows: Vec<_> = machines
            .iter()
            .zip(&probes)
            .map(|(m, p)| machine_json(m, p))
            .collect();
        println!("{}", serde_json::json!({ "machines": rows }));
        return 0;
    }
    if machines.is_empty() {
        println!(
            "No other machines yet. Add one to {}:\n\n[[machine]]\nname = \"farbox\"\nssh = \"user@host\"",
            entry::machines_path(&instance::config_dir()).display()
        );
        return 0;
    }
    for (m, p) in machines.iter().zip(&probes) {
        println!("{}", machine_words(m, p));
    }
    0
}

fn machine_json(m: &Machine, probe: &Result<Probe, String>) -> serde_json::Value {
    let mut row = serde_json::json!({ "name": m.name, "ssh": m.ssh });
    match probe {
        Ok(p) => {
            row["reachable"] = true.into();
            row["host"] = p.host.clone().into();
            row["terminal_delight"] = p.td.clone().into();
            row["version"] = p.version.clone().into();
            row["wire_proto"] = p.wire_proto.clone().into();
            row["tailscale"] = p.tailscale.clone().into();
            row["gliff"] = p.gliff.into();
            row["sessions"] = p
                .sessions
                .iter()
                .map(|s| serde_json::json!({ "key": s.key, "window": s.window }))
                .collect::<Vec<_>>()
                .into();
        }
        Err(e) => {
            row["reachable"] = false.into();
            row["trouble"] = e.clone().into();
        }
    }
    row
}

/// One machine, in a few plain lines.
pub fn machine_words(m: &Machine, probe: &Result<Probe, String>) -> String {
    let dest = m.ssh.as_deref().unwrap_or("this machine");
    let p = match probe {
        Ok(p) => p,
        Err(e) => return format!("{}  ({dest})\n  not reachable: {e}", m.name),
    };
    let mut out = format!("{}  ({dest})", m.name);
    match (&p.td, &p.version) {
        (None, _) => out.push_str("\n  Terminal Delight is not installed there"),
        (Some(_), Some(v)) => out.push_str(&format!("\n  Terminal Delight {v}")),
        (Some(_), None) => out.push_str("\n  Terminal Delight, version unknown"),
    }
    if p.td.is_some() {
        if p.sessions.is_empty() {
            out.push_str("\n  no session running");
        }
        for s in &p.sessions {
            let who = match s.window {
                Some(true) => "a window is on it there",
                Some(false) => "nobody is watching it there",
                None => "unknown whether a window is on it",
            };
            out.push_str(&format!(
                "\n  session {}: {who}  →  terminal-delight remote {} {}",
                s.key, m.name, s.key
            ));
        }
    }
    out
}

/// Which machine a person meant: its name in `machines.toml`, matched without
/// regard to case, or its ssh destination.
pub fn find_machine<'a>(machines: &'a [Machine], word: &str) -> Option<&'a Machine> {
    machines
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case(word))
        .or_else(|| machines.iter().find(|m| m.ssh.as_deref() == Some(word)))
}

/// `terminal-delight remote <machine> [session]`: open a window here on
/// another machine's session.
///
/// Decides which session, says so, and then becomes the window: the far
/// session's three words go into the environment and this binary is run again
/// with no arguments, so the window that opens is an ordinary one that knows
/// where its host is.
pub fn run_remote_cli(args: &[String]) -> i32 {
    use std::os::unix::process::CommandExt;
    let (word, asked) = match args {
        [m] => (m.as_str(), None),
        [m, s] => (m.as_str(), Some(s.as_str())),
        _ => {
            eprintln!("{USAGE}");
            return 2;
        }
    };
    let machines = match entry::load_machines(&instance::config_dir()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("terminal-delight remote: machines.toml: {e}");
            return 1;
        }
    };
    let Some(machine) = find_machine(&machines, word) else {
        let known: Vec<&str> = machines.iter().map(|m| m.name.as_str()).collect();
        eprintln!(
            "terminal-delight remote: no machine named {word:?} in {}{}",
            entry::machines_path(&instance::config_dir()).display(),
            if known.is_empty() {
                String::new()
            } else {
                format!(" (it lists: {})", known.join(", "))
            }
        );
        return 1;
    };
    let found = match probe(machine) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("terminal-delight remote: {e}");
            return 1;
        }
    };
    if found.td.is_none() {
        eprintln!(
            "terminal-delight remote: {}: Terminal Delight is not installed there",
            machine.name
        );
        return 1;
    }
    let session = match pick_session(&found, asked) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("terminal-delight remote: {}: {e}", machine.name);
            return 1;
        }
    };
    let far = FarSession {
        name: machine.name.clone(),
        dest: machine.ssh.clone().unwrap_or_default(),
        session,
    };
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("terminal-delight remote: {e}");
            return 1;
        }
    };
    eprintln!(
        "terminal-delight: opening {}'s session {} here",
        far.name, far.session
    );
    let err = Command::new(exe)
        .env(ENV_NAME, &far.name)
        .env(ENV_DEST, &far.dest)
        .env(ENV_SESSION, &far.session)
        .env("TD_SESSION", far.local_key())
        // A far window is never a scratch one and never a demo.
        .env_remove("TD_SCRATCH")
        .env_remove("TD_DEMO_STATE")
        .env_remove("TD_NO_SESSIOND")
        .exec();
    eprintln!("terminal-delight remote: {err}");
    1
}

#[cfg(test)]
mod tests;
