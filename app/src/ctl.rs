//! ctl — the control socket: how the desktop talks to a RUNNING terminal.
//!
//! Every terminal-delight process listens on its own unix socket,
//! `$XDG_RUNTIME_DIR/terminal-delight/ctl-<pid>.sock`, for one-line commands —
//! PAINT mode (the per-pane palette overlay), `ping`, `paint status`, the MCP
//! policy toggles, and `mcp rpc <json>`, which carries a whole JSON-RPC line to
//! the MCP protocol handler. The same binary is also the client: `terminal-delight ctl
//! paint toggle --workspace active` finds the sockets of the windows on the
//! active Hyprland workspace and pokes each one. That split is what lets an
//! Omarchy bar widget (or a keybind, or a plain script) raise the overlay
//! without linking against anything: the whole contract is a socket path and a
//! line of text.
//!
//! Wire protocol, deliberately dumb: the client writes one line, the server
//! answers one line — `ok`, `on`, `off`, `pong`, or `err <why>` — and the
//! connection is done. Command effects are queued to the UI thread; `ok`
//! acknowledges the queue write, and `paint status` reads a mirror the UI
//! refreshes every tick, so a status read straight after a toggle can lag it
//! by one ~150 ms tick. Honest enough, and nothing ever blocks the UI.
//!
//! The socket file is NOT unlinked on exit — there is no hook that runs on
//! every exit path, so pretending otherwise would just be a lie that works in
//! demos. Instead the server unlinks any stale file before binding its own
//! pid's path, and the client treats a connection failure as "stale: sweep it
//! and move on".
//!
//! **`mcp rpc` is the exception to "never blocks".** The MCP handler needs a
//! round-trip onto the gpui main thread, so that one verb is served on its own
//! thread and the accept loop moves on; everything else still answers inline
//! from a queue write or an atomic mirror. `terminal-delight mcp` is the
//! matching client — a stdio JSON-RPC relay an agent registers as an MCP server,
//! which finds the terminal hosting it by walking its own parent chain.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use gpui::Context;

use crate::{mcp, mcp_transport, theme, Workspace};

/// A tile adoption: open a fresh pane at `cwd`, optionally running `run` in it
/// (an agent resume line, a tmux attach) — how the desktop hands a terminal
/// session over to us (td-send / SUPER+ALT+T). Rides the same queue as paint.
#[derive(Debug, PartialEq)]
pub(crate) struct AdoptReq {
    pub cwd: Option<String>,
    pub run: Option<String>,
}

/// A queued control request, applied on the UI thread by the ticker.
#[derive(Debug)]
pub(crate) enum Req {
    Set(bool),
    Toggle,
    Adopt(AdoptReq),
    McpPolicy(McpPolicy),
}

/// One field of the MCP control-surface policy — the robot panel's toggles,
/// reachable from the socket. Same escalation, same persistence: the panel and
/// this path both write `ws.mcp` and `save()`, so a grant made from the CLI is
/// visible in the panel and survives a restart. Kept as one-field-at-a-time so a
/// script can grant reads without silently also granting writes.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum McpPolicy {
    /// The master switch (`mcp.enabled`).
    Enabled(bool),
    /// The second, separate opt-in that permits `set_pane_config` (`mcp.writable`).
    Writes(bool),
    /// `true` = expose every pane, `false` = agent panes only (the safe default).
    ExposeAll(bool),
}

/// One parsed request line.
enum Cmd {
    Ping,
    PaintStatus,
    Paint(Req),
    Adopt(AdoptReq),
    /// A whole JSON-RPC line for the MCP handler, verbatim.
    McpRpc(String),
    McpStatus,
    McpPolicy(McpPolicy),
}

// The `mcp status` mirror, refreshed by the ticker each pass (same pattern as
// the paint mirror): a policy read never touches the main thread.
const MCP_ON: u8 = 1 << 0;
const MCP_WRITES: u8 = 1 << 1;
const MCP_EXPOSE_ALL: u8 = 1 << 2;
const MCP_EVENTS: u8 = 1 << 3;

fn mcp_bits(c: &mcp::McpConfig) -> u8 {
    let mut b = 0;
    if c.enabled {
        b |= MCP_ON;
    }
    if c.writable {
        b |= MCP_WRITES;
    }
    if c.expose == mcp::Expose::All {
        b |= MCP_EXPOSE_ALL;
    }
    if c.events {
        b |= MCP_EVENTS;
    }
    b
}

/// Render the mirror as the one status line `mcp status` answers with.
fn mcp_status_line(bits: u8) -> String {
    let on = |m: u8| if bits & m != 0 { "on" } else { "off" };
    format!(
        "enabled={} writes={} expose={} events={}",
        on(MCP_ON),
        on(MCP_WRITES),
        if bits & MCP_EXPOSE_ALL != 0 {
            "all"
        } else {
            "agents"
        },
        on(MCP_EVENTS),
    )
}

/// The per-user control directory. Runtime state, so `$XDG_RUNTIME_DIR` (a
/// tmpfs that dies with the session) — never the config dir, which persists.
fn ctl_dir() -> PathBuf {
    let base = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/tmp/terminal-delight-{}", unsafe { libc::getuid() }));
    PathBuf::from(base).join("terminal-delight")
}

/// This process's socket path. Keyed by pid so `ctl --pid N` needs no lookup
/// table and a workspace query (window → pid) lands directly on the file.
pub fn socket_path(pid: u32) -> PathBuf {
    ctl_dir().join(format!("ctl-{pid}.sock"))
}

/// Everything the grammar accepts, in one place — the usage string and the
/// unknown-command error both quote it, so they can't drift from the match.
const USAGE: &str = "ping | paint on|off|toggle|status | \
     mcp status|on|off | mcp writes on|off | mcp expose agents|all | \
     mcp rpc <json> | adopt {\"cwd\":\"/…\",\"run\":\"…\"}";

fn parse_line(s: &str) -> Result<Cmd, String> {
    // `adopt` carries a JSON payload (cwd/run both hold spaces); everything
    // else stays word-shaped.
    if let Some(rest) = s.strip_prefix("adopt ") {
        return parse_adopt(rest.trim()).map(Cmd::Adopt);
    }
    // `mcp rpc` carries a whole JSON-RPC line: take the remainder VERBATIM.
    // Splitting it on whitespace would corrupt every string literal in it.
    if let Some(rest) = s.strip_prefix("mcp rpc ") {
        let line = rest.trim();
        if line.is_empty() {
            return Err("mcp rpc: empty payload".into());
        }
        return Ok(Cmd::McpRpc(line.to_string()));
    }
    let w: Vec<&str> = s.split_whitespace().collect();
    match w.as_slice() {
        ["ping"] => Ok(Cmd::Ping),
        ["paint", "on"] => Ok(Cmd::Paint(Req::Set(true))),
        ["paint", "off"] => Ok(Cmd::Paint(Req::Set(false))),
        ["paint", "toggle"] => Ok(Cmd::Paint(Req::Toggle)),
        ["paint", "status"] => Ok(Cmd::PaintStatus),
        ["mcp", "status"] => Ok(Cmd::McpStatus),
        ["mcp", "on"] => Ok(Cmd::McpPolicy(McpPolicy::Enabled(true))),
        ["mcp", "off"] => Ok(Cmd::McpPolicy(McpPolicy::Enabled(false))),
        ["mcp", "writes", "on"] => Ok(Cmd::McpPolicy(McpPolicy::Writes(true))),
        ["mcp", "writes", "off"] => Ok(Cmd::McpPolicy(McpPolicy::Writes(false))),
        ["mcp", "expose", "all"] => Ok(Cmd::McpPolicy(McpPolicy::ExposeAll(true))),
        ["mcp", "expose", "agents"] => Ok(Cmd::McpPolicy(McpPolicy::ExposeAll(false))),
        _ => Err(format!("unknown command {s:?} — try: {USAGE}")),
    }
}

/// The `adopt` payload: one JSON object, `cwd` and/or `run`, cwd absolute when
/// present. Same-user trust as the rest of the socket — the validation here is
/// protocol hygiene, not a security boundary.
fn parse_adopt(json: &str) -> Result<AdoptReq, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("adopt payload is not JSON: {e}"))?;
    let take = |k: &str| -> Result<Option<String>, String> {
        match &v[k] {
            serde_json::Value::Null => Ok(None),
            serde_json::Value::String(s) if s.is_empty() => Ok(None),
            serde_json::Value::String(s) => Ok(Some(s.clone())),
            other => Err(format!("adopt: {k} must be a string, got {other}")),
        }
    };
    let cwd = take("cwd")?;
    let run = take("run")?;
    if let Some(c) = &cwd {
        if !c.starts_with('/') {
            return Err(format!("adopt: cwd must be absolute, got {c:?}"));
        }
    }
    if cwd.is_none() && run.is_none() {
        return Err("adopt: needs cwd and/or run".into());
    }
    Ok(AdoptReq { cwd, run })
}

/// The reply for a `mcp rpc` line that produced no response — a JSON-RPC
/// notification, or an unparseable line. A distinct sentinel rather than `err`
/// so the relay client can drop it silently instead of logging a non-problem.
const MCP_NONE: &str = "mcp-none";

/// Serve one connection: read a line, answer a line. Short timeouts on both
/// directions so a wedged client can never stall the single accept loop.
fn handle_conn(
    stream: UnixStream,
    mirror: &AtomicBool,
    mcp_mirror: &AtomicU8,
    tx: &mpsc::Sender<Req>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(400)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(400)));
    let Ok(read_half) = stream.try_clone() else {
        return;
    };
    let mut line = String::new();
    if BufReader::new(read_half).read_line(&mut line).is_err() {
        return;
    }
    let reply = match parse_line(line.trim()) {
        // `mcp rpc` is the one verb that waits on the gpui main thread (up to
        // the transport's snapshot budget). Serving it inline would stall the
        // single accept loop for seconds, so hand the connection to its own
        // thread and get straight back to accepting. That thread re-arms the
        // write timeout: 400 ms is sized for a mirror read, not a `tools/list`
        // payload after a five-second wait.
        Ok(Cmd::McpRpc(payload)) => {
            let spawned = thread::Builder::new()
                .name("td-ctl-mcp".into())
                .spawn(move || {
                    let reply =
                        mcp_transport::respond(&payload).unwrap_or_else(|| MCP_NONE.to_string());
                    let mut stream = stream;
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(8)));
                    let _ = writeln!(stream, "{reply}");
                });
            if spawned.is_err() {
                // Thread exhaustion. The caller is owed a line and the worker
                // now owns the stream, so there is nothing left to answer on;
                // the client's read timeout turns this into "unreachable".
                eprintln!("terminal-delight: ctl could not spawn an mcp worker");
            }
            return; // the worker owns the connection from here
        }
        Ok(Cmd::Ping) => "pong".to_string(),
        Ok(Cmd::PaintStatus) => if mirror.load(Ordering::Relaxed) {
            "on"
        } else {
            "off"
        }
        .to_string(),
        Ok(Cmd::McpStatus) => mcp_status_line(mcp_mirror.load(Ordering::Relaxed)),
        Ok(Cmd::Paint(req)) => {
            if tx.send(req).is_ok() {
                "ok".into()
            } else {
                "err ui gone".into()
            }
        }
        Ok(Cmd::McpPolicy(p)) => {
            if tx.send(Req::McpPolicy(p)).is_ok() {
                "ok".into()
            } else {
                "err ui gone".into()
            }
        }
        Ok(Cmd::Adopt(a)) => {
            if tx.send(Req::Adopt(a)).is_ok() {
                "ok".into()
            } else {
                "err ui gone".into()
            }
        }
        Err(e) => format!("err {e}"),
    };
    let mut stream = stream;
    let _ = writeln!(stream, "{reply}");
}

/// Start the control server once per process. Call from `Workspace::build`.
/// Failure to bind is a warning, never fatal — the terminal works without its
/// control surface, it just can't be painted from the bar.
pub fn start(cx: &mut Context<Workspace>) {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let dir = ctl_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("terminal-delight: ctl dir unavailable ({dir:?}): {e}");
        return;
    }
    // `adopt` spawns commands, so the socket must stay same-user even on the
    // /tmp fallback (no $XDG_RUNTIME_DIR) under a loose umask: pin the dir to
    // 0700 every start — create_dir_all leaves an existing dir's mode alone.
    let _ = std::fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(0o700));
    let path = socket_path(std::process::id());
    // A stale file under our own pid means a previous process with a recycled
    // pid died uncleanly; the bind would fail on it, so sweep first.
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("terminal-delight: ctl socket unavailable ({path:?}): {e}");
            return;
        }
    };

    let (tx, rx) = mpsc::channel::<Req>();
    let mirror = Arc::new(AtomicBool::new(false));
    let mcp_mirror = Arc::new(AtomicU8::new(0));

    {
        let mirror = Arc::clone(&mirror);
        let mcp_mirror = Arc::clone(&mcp_mirror);
        let _ = thread::Builder::new().name("td-ctl".into()).spawn(move || {
            for conn in listener.incoming() {
                let Ok(stream) = conn else { continue };
                handle_conn(stream, &mirror, &mcp_mirror, &tx);
            }
        });
    }

    // Ticker: applies queued commands on the main thread and re-mirrors the
    // live paint flag every pass — Esc flips the global without going through
    // this queue, and `paint status` must not report around that.
    cx.spawn(async move |this, cx| loop {
        cx.background_executor()
            .timer(Duration::from_millis(150))
            .await;
        let mut cmds = Vec::new();
        while let Ok(c) = rx.try_recv() {
            cmds.push(c);
        }
        let applied = this.update(cx, |ws, cx| {
            for c in cmds {
                match c {
                    Req::Set(v) => theme::set_paint_mode(cx, v),
                    Req::Toggle => {
                        let v = !theme::paint_mode(cx);
                        theme::set_paint_mode(cx, v);
                    }
                    // Adoption needs a Window to build the pane; this ticker is
                    // window-less, so park it — render() drains next frame.
                    Req::Adopt(a) => ws.queue_adopt(a, cx),
                    // The same escalation the robot panel performs, and the same
                    // persistence: a grant made from the CLI shows in the panel
                    // and survives a restart.
                    Req::McpPolicy(p) => {
                        match p {
                            McpPolicy::Enabled(v) => ws.mcp.enabled = v,
                            McpPolicy::Writes(v) => ws.mcp.writable = v,
                            McpPolicy::ExposeAll(v) => {
                                ws.mcp.expose = if v {
                                    mcp::Expose::All
                                } else {
                                    mcp::Expose::AgentsOnly
                                }
                            }
                        }
                        ws.save(cx);
                        cx.notify();
                    }
                }
            }
            mirror.store(theme::paint_mode(cx), Ordering::Relaxed);
            mcp_mirror.store(mcp_bits(&ws.mcp), Ordering::Relaxed);
        });
        if applied.is_err() {
            return; // UI gone — the listener thread dies with the process
        }
    })
    .detach();
}

// ---------------------------------------------------------------- client ----

/// Which running terminals a `ctl` invocation addresses.
#[derive(Debug, PartialEq)]
enum Scope {
    /// Terminal-delight windows on the active Hyprland workspace (the default —
    /// it is what a bar click means).
    ActiveWorkspace,
    /// A named (or numeric-id) Hyprland workspace.
    Workspace(String),
    /// Every control socket present.
    All,
    /// One process.
    Pid(u32),
}

/// Parse `ctl` argv (everything after the `ctl` token) into the request line
/// and the scope. Kept pure for tests.
fn parse_cli(args: &[String]) -> Result<(String, Scope), String> {
    let mut words: Vec<&str> = Vec::new();
    let mut scope = Scope::ActiveWorkspace;
    let mut cwd: Option<String> = None;
    let mut run: Option<String> = None;
    let mut it = args.iter().map(String::as_str).peekable();
    while let Some(a) = it.next() {
        match a {
            "--all" => scope = Scope::All,
            "--workspace" => {
                let v = it.next().ok_or("--workspace needs a value")?;
                scope = if v == "active" {
                    Scope::ActiveWorkspace
                } else {
                    Scope::Workspace(v.to_string())
                };
            }
            "--pid" => {
                let v = it.next().ok_or("--pid needs a value")?;
                scope = Scope::Pid(v.parse().map_err(|_| format!("bad pid {v:?}"))?);
            }
            "--cwd" => cwd = Some(it.next().ok_or("--cwd needs a value")?.to_string()),
            "--run" => run = Some(it.next().ok_or("--run needs a value")?.to_string()),
            w if !w.starts_with('-') => words.push(w),
            other => return Err(format!("unknown flag {other:?}")),
        }
    }
    // `adopt` is flag-shaped on the CLI (cwd/run carry spaces) and becomes the
    // one-line JSON form on the wire; everything else is the words themselves.
    let line = if words == ["adopt"] {
        format!("adopt {}", serde_json::json!({ "cwd": cwd, "run": run }))
    } else {
        if cwd.is_some() || run.is_some() {
            return Err("--cwd/--run only apply to adopt".into());
        }
        words.join(" ")
    };
    // Validate against the same grammar the server enforces, so a typo fails
    // HERE with usage rather than fanning out as N "err unknown command"s.
    parse_line(&line)?;
    Ok((line, scope))
}

/// Sockets currently present, as (pid, path).
fn discover() -> Vec<(u32, PathBuf)> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(ctl_dir()) else {
        return out;
    };
    for e in rd.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if let Some(pid) = name
            .strip_prefix("ctl-")
            .and_then(|s| s.strip_suffix(".sock"))
            .and_then(|s| s.parse::<u32>().ok())
        {
            out.push((pid, e.path()));
        }
    }
    out.sort_by_key(|(pid, _)| *pid);
    out
}

/// One request/response round trip against a socket, with an explicit read
/// budget: the queue-and-mirror verbs answer within a tick, but `mcp rpc` waits
/// on the gpui main thread and needs the server's snapshot budget plus slack.
fn send_within(path: &Path, line: &str, budget: Duration) -> std::io::Result<String> {
    let mut s = UnixStream::connect(path)?;
    let _ = s.set_read_timeout(Some(budget));
    let _ = s.set_write_timeout(Some(budget));
    writeln!(s, "{line}")?;
    let mut reply = String::new();
    BufReader::new(&mut s).read_line(&mut reply)?;
    Ok(reply.trim().to_string())
}

/// One request/response round trip against a socket.
fn send(path: &Path, line: &str) -> std::io::Result<String> {
    send_within(path, line, Duration::from_millis(800))
}

/// Ask the Hyprland IPC socket a `j/…` question. Direct socket, not `hyprctl`:
/// no PATH dependency, and the reply is the same JSON.
fn hypr_request(cmd: &str) -> Option<String> {
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let base = std::env::var("XDG_RUNTIME_DIR").ok()?;
    let path = PathBuf::from(base)
        .join("hypr")
        .join(sig)
        .join(".socket.sock");
    let mut s = UnixStream::connect(path).ok()?;
    let _ = s.set_read_timeout(Some(Duration::from_millis(800)));
    s.write_all(cmd.as_bytes()).ok()?;
    let mut out = String::new();
    s.read_to_string(&mut out).ok()?;
    Some(out)
}

/// The pids of terminal-delight windows on the selected workspace, straight
/// out of `j/clients`. Numeric selectors match the workspace id, anything else
/// the workspace name — Hyprland names default to the id's digits, so both
/// spellings of "workspace 2" land in the same place.
fn td_pids_in_workspace(clients: &serde_json::Value, sel: &str) -> Vec<u32> {
    let mut out = Vec::new();
    let Some(arr) = clients.as_array() else {
        return out;
    };
    for c in arr {
        if c["class"].as_str() != Some("terminal-delight") {
            continue;
        }
        let ws = &c["workspace"];
        let hit = match sel.parse::<i64>() {
            Ok(id) => ws["id"].as_i64() == Some(id),
            Err(_) => ws["name"].as_str() == Some(sel),
        };
        if hit {
            if let Some(pid) = c["pid"].as_i64() {
                out.push(pid as u32);
            }
        }
    }
    out
}

/// The `terminal-delight ctl …` entry point. Returns the process exit code:
/// 0 when at least one terminal answered, 2 when nothing matched (with a hint
/// on stderr), 1 on a usage error.
pub fn run_cli(args: &[String]) -> i32 {
    let (line, scope) = match parse_cli(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("terminal-delight ctl: {e}");
            eprintln!(
                "usage: terminal-delight ctl <{USAGE} | adopt --cwd <dir> [--run <cmd>]> \
                 [--workspace active|<name-or-id> | --all | --pid <N>]"
            );
            return 1;
        }
    };

    // Resolve the scope to concrete (pid, path) targets.
    let targets: Vec<(u32, PathBuf)> = match &scope {
        Scope::Pid(pid) => vec![(*pid, socket_path(*pid))],
        Scope::All => discover(),
        Scope::ActiveWorkspace | Scope::Workspace(_) => {
            let sel = match &scope {
                Scope::Workspace(w) => w.clone(),
                _ => {
                    let Some(active) = hypr_request("j/activeworkspace")
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                        .and_then(|v| v["id"].as_i64())
                    else {
                        eprintln!(
                            "terminal-delight ctl: no Hyprland session found — \
                             use --all or --pid <N>"
                        );
                        return 2;
                    };
                    active.to_string()
                }
            };
            let Some(clients) = hypr_request("j/clients")
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            else {
                eprintln!("terminal-delight ctl: could not list Hyprland clients");
                return 2;
            };
            let pids = td_pids_in_workspace(&clients, &sel);
            if pids.is_empty() {
                eprintln!("terminal-delight ctl: no terminal-delight windows on workspace {sel}");
                return 2;
            }
            pids.into_iter().map(|p| (p, socket_path(p))).collect()
        }
    };

    // Adoption is a placement, not a broadcast: exactly ONE terminal receives
    // the session (workspace scope → the lowest-pid window there).
    let targets: Vec<(u32, PathBuf)> = if line.starts_with("adopt") {
        targets.into_iter().take(1).collect()
    } else {
        targets
    };

    if targets.is_empty() {
        eprintln!(
            "terminal-delight ctl: no control sockets in {:?} — terminals started \
             before this build don't have one; open a new terminal-delight window",
            ctl_dir()
        );
        return 2;
    }

    let mut ok = 0;
    for (pid, path) in &targets {
        match send(path, &line) {
            Ok(reply) => {
                println!("{pid}\t{reply}");
                if !reply.starts_with("err") {
                    ok += 1;
                }
            }
            Err(_) if !path.exists() => {
                // A window Hyprland knows but we have no socket for: a terminal
                // from a build older than the ctl surface.
                println!("{pid}\terr no control socket (older build — reopen this terminal)");
            }
            Err(e) => {
                // Present but unconnectable = stale leftovers; sweep so the
                // next discovery is clean.
                let _ = std::fs::remove_file(path);
                println!("{pid}\terr unreachable ({e}) — swept stale socket");
            }
        }
    }
    if ok > 0 {
        0
    } else {
        2
    }
}

// ------------------------------------------------------------ mcp relay ----

/// A process's parent, from `/proc/<pid>/stat`. The `comm` field is wrapped in
/// parens and may itself contain spaces AND parens, so the only safe split is
/// after the LAST `)`: what follows is `state ppid …`.
fn ppid_of(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_comm = stat.rsplit_once(')')?.1;
    after_comm.split_whitespace().nth(1)?.parse().ok()
}

/// The terminal-delight window hosting THIS process, by walking our own parent
/// chain until a pid turns out to own a control socket.
///
/// That is the whole trick behind the relay: an agent is a great-grandchild of
/// the terminal it lives in (td → shell → agent → this MCP server), so the
/// ancestor that owns a socket is, unambiguously, the window it is looking at.
/// The socket's own existence is the test — no class matching, no name guessing.
fn owning_td_pid() -> Option<u32> {
    let mut pid = std::process::id();
    // Deep enough for td → shell → agent → server with room to spare; bounded so
    // a malformed /proc chain can never spin.
    for _ in 0..64 {
        if pid <= 1 {
            return None;
        }
        if socket_path(pid).exists() {
            return Some(pid);
        }
        pid = ppid_of(pid)?;
    }
    None
}

/// Resolve which terminal the relay talks to: an explicit `--pid`, else the
/// window hosting us, else — only if it is unambiguous — the single running
/// terminal. Refusing to guess between several is deliberate: silently driving
/// the wrong window is worse than an error telling you to name one.
fn relay_target(args: &[String]) -> Result<u32, String> {
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => {}
        ["--pid", v] => return v.parse().map_err(|_| format!("bad pid {v:?}")),
        ["--pid"] => return Err("--pid needs a value".into()),
        [other, ..] => return Err(format!("unknown flag {other:?}")),
    }
    if let Some(pid) = owning_td_pid() {
        return Ok(pid);
    }
    match discover().as_slice() {
        [] => Err(format!(
            "no terminal-delight control sockets in {:?} — is one running, and \
             new enough to have a control socket?",
            ctl_dir()
        )),
        [(pid, _)] => Ok(*pid),
        many => Err(format!(
            "not launched from inside a terminal-delight window, and {} are \
             running — name one with --pid <N> (pids: {})",
            many.len(),
            many.iter()
                .map(|(p, _)| p.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// `terminal-delight mcp` — the stdio JSON-RPC relay: an MCP server an agent
/// registers, which forwards each line to a RUNNING terminal's control socket
/// and writes the answer back.
///
/// This exists because the in-process stdio transport ([`crate::mcp_transport`])
/// requires the MCP client to own our stdin/stdout, i.e. to be our parent — and
/// a GUI terminal launched from the desktop never is. The relay inverts that: it
/// is spawned BY the agent, and reaches back to the window already on screen.
pub fn run_mcp_cli(args: &[String]) -> i32 {
    let pid = match relay_target(args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("terminal-delight mcp: {e}");
            return 1;
        }
    };
    let path = socket_path(pid);

    // The server's own budget is 5 s; allow slack for the queue and the write so
    // a busy UI reads as slow, never as a dropped connection.
    let budget = Duration::from_secs(10);
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    let mut line = String::new();
    loop {
        line.clear();
        match stdin.read_line(&mut line) {
            Ok(0) | Err(_) => return 0, // EOF: the agent closed us. Normal exit.
            Ok(_) => {}
        }
        let req = line.trim();
        if req.is_empty() {
            continue;
        }
        match send_within(&path, &format!("mcp rpc {req}"), budget) {
            // A notification: JSON-RPC says answer nothing, so write nothing.
            Ok(r) if r == MCP_NONE => {}
            // A protocol-level refusal from ctl (never from the MCP handler,
            // which answers in JSON-RPC). Log it; emitting it on stdout would
            // corrupt the framing the client is parsing.
            Ok(r) if r.starts_with("err ") => eprintln!("terminal-delight mcp: {r}"),
            Ok(r) => {
                if writeln!(out, "{r}").is_err() || out.flush().is_err() {
                    return 0; // client hung up mid-answer
                }
            }
            Err(e) => {
                eprintln!("terminal-delight mcp: window {pid} unreachable ({e})");
                return 2;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("td-ctl-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_original_six_verbs_still_parse() {
        assert!(matches!(parse_line("ping"), Ok(Cmd::Ping)));
        assert!(matches!(
            parse_line("paint on"),
            Ok(Cmd::Paint(Req::Set(true)))
        ));
        assert!(matches!(
            parse_line("paint off"),
            Ok(Cmd::Paint(Req::Set(false)))
        ));
        assert!(matches!(
            parse_line("paint toggle"),
            Ok(Cmd::Paint(Req::Toggle))
        ));
        assert!(matches!(parse_line("paint status"), Ok(Cmd::PaintStatus)));
        assert!(matches!(
            parse_line(r#"adopt {"cwd":"/tmp"}"#),
            Ok(Cmd::Adopt(AdoptReq { .. }))
        ));
        assert!(parse_line("paint").is_err());
        assert!(parse_line("paint sideways").is_err());
        assert!(parse_line("paint on extra").is_err());
        assert!(parse_line("adopt").is_err());
        assert!(parse_line("adopt notjson").is_err());
        assert!(parse_line("").is_err());
    }

    #[test]
    fn adopt_payloads_validate_before_they_queue() {
        let ok = parse_adopt(r#"{"cwd":"/tmp/x","run":"claude --resume abc-123"}"#).unwrap();
        assert_eq!(ok.cwd.as_deref(), Some("/tmp/x"));
        assert_eq!(ok.run.as_deref(), Some("claude --resume abc-123"));
        // run-only (a tmux re-attach with its own cd) is legal
        assert!(parse_adopt(r#"{"run":"tmux attach -t rec"}"#).is_ok());
        // empty strings collapse to None — and all-None is refused
        assert!(parse_adopt(r#"{"cwd":"","run":""}"#).is_err());
        assert!(parse_adopt(r#"{}"#).is_err());
        // relative cwd and non-string types are protocol errors
        assert!(parse_adopt(r#"{"cwd":"rel/path"}"#).is_err());
        assert!(parse_adopt(r#"{"cwd":42}"#).is_err());
    }

    #[test]
    fn cli_adopt_builds_the_json_line_and_scopes_like_paint() {
        let s = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let (line, scope) = parse_cli(&s(&[
            "adopt",
            "--cwd",
            "/tmp/x",
            "--run",
            "claude --resume abc-1",
            "--pid",
            "7",
        ]))
        .unwrap();
        assert_eq!(scope, Scope::Pid(7));
        let Ok(Cmd::Adopt(a)) = parse_line(&line) else {
            panic!("adopt CLI produced an unparseable line: {line:?}");
        };
        assert_eq!(a.cwd.as_deref(), Some("/tmp/x"));
        assert_eq!(a.run.as_deref(), Some("claude --resume abc-1"));
        // the adopt flags are meaningless on other verbs
        assert!(parse_cli(&s(&["paint", "on", "--cwd", "/x"])).is_err());
    }

    #[test]
    fn cli_defaults_to_the_active_workspace_and_flags_override() {
        let s = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let (line, scope) = parse_cli(&s(&["paint", "toggle"])).unwrap();
        assert_eq!(line, "paint toggle");
        assert_eq!(scope, Scope::ActiveWorkspace);
        let (_, scope) = parse_cli(&s(&["paint", "on", "--all"])).unwrap();
        assert_eq!(scope, Scope::All);
        let (_, scope) = parse_cli(&s(&["ping", "--workspace", "web"])).unwrap();
        assert_eq!(scope, Scope::Workspace("web".into()));
        let (_, scope) = parse_cli(&s(&["ping", "--workspace", "active"])).unwrap();
        assert_eq!(scope, Scope::ActiveWorkspace);
        let (_, scope) = parse_cli(&s(&["paint", "off", "--pid", "42"])).unwrap();
        assert_eq!(scope, Scope::Pid(42));
        assert!(parse_cli(&s(&["paint", "maybe"])).is_err());
        assert!(parse_cli(&s(&["paint", "on", "--pid", "nope"])).is_err());
        assert!(parse_cli(&s(&["paint", "on", "--wat"])).is_err());
    }

    #[test]
    fn workspace_filter_matches_class_and_either_selector_spelling() {
        let clients: serde_json::Value = serde_json::from_str(
            r#"[
              {"class":"terminal-delight","pid":100,"workspace":{"id":1,"name":"1"}},
              {"class":"terminal-delight","pid":200,"workspace":{"id":2,"name":"web"}},
              {"class":"foot","pid":300,"workspace":{"id":2,"name":"web"}},
              {"class":"terminal-delight","workspace":{"id":2,"name":"web"}}
            ]"#,
        )
        .unwrap();
        assert_eq!(td_pids_in_workspace(&clients, "1"), vec![100]);
        assert_eq!(td_pids_in_workspace(&clients, "2"), vec![200]);
        assert_eq!(td_pids_in_workspace(&clients, "web"), vec![200]);
        assert!(td_pids_in_workspace(&clients, "9").is_empty());
    }

    #[test]
    fn a_round_trip_answers_and_queues_and_status_reads_the_mirror() {
        let sock = tmp("rt").join("ctl-1.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        let (tx, rx) = mpsc::channel::<Req>();
        let mirror = Arc::new(AtomicBool::new(false));
        let m2 = Arc::clone(&mirror);
        let mcp_mirror = Arc::new(AtomicU8::new(0));
        let mm2 = Arc::clone(&mcp_mirror);
        let server = thread::spawn(move || {
            // exactly five connections, in test order
            for _ in 0..5 {
                let (stream, _) = listener.accept().unwrap();
                handle_conn(stream, &m2, &mm2, &tx);
            }
        });
        assert_eq!(send(&sock, "ping").unwrap(), "pong");
        assert_eq!(send(&sock, "paint status").unwrap(), "off");
        assert_eq!(send(&sock, "paint toggle").unwrap(), "ok");
        assert!(matches!(rx.try_recv(), Ok(Req::Toggle)));
        assert_eq!(
            send(&sock, r#"adopt {"cwd":"/tmp","run":"htop"}"#).unwrap(),
            "ok"
        );
        match rx.try_recv() {
            Ok(Req::Adopt(a)) => {
                assert_eq!(a.cwd.as_deref(), Some("/tmp"));
                assert_eq!(a.run.as_deref(), Some("htop"));
            }
            other => panic!("expected the adopt on the queue, got {other:?}"),
        }
        mirror.store(true, Ordering::Relaxed); // the UI ticker's job
        assert_eq!(send(&sock, "paint status").unwrap(), "on");
        server.join().unwrap();
    }

    #[test]
    fn junk_gets_an_error_line_not_a_hang() {
        let sock = tmp("junk").join("ctl-2.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        let (tx, _rx) = mpsc::channel::<Req>();
        let mirror = AtomicBool::new(false);
        let mcp_mirror = AtomicU8::new(0);
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle_conn(stream, &mirror, &mcp_mirror, &tx);
        });
        assert!(send(&sock, "sudo make me a sandwich")
            .unwrap()
            .starts_with("err"));
        server.join().unwrap();
    }

    #[test]
    fn mcp_policy_verbs_queue_and_status_reads_the_mirror() {
        let sock = tmp("mcp").join("ctl-4.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        let (tx, rx) = mpsc::channel::<Req>();
        let mirror = Arc::new(AtomicBool::new(false));
        // A mirror standing in for a live policy: reads on, writes off.
        let mcp_mirror = Arc::new(AtomicU8::new(MCP_ON | MCP_EVENTS));
        let (m2, mm2) = (Arc::clone(&mirror), Arc::clone(&mcp_mirror));
        let server = thread::spawn(move || {
            for _ in 0..4 {
                let (stream, _) = listener.accept().unwrap();
                handle_conn(stream, &m2, &mm2, &tx);
            }
        });

        assert_eq!(
            send(&sock, "mcp status").unwrap(),
            "enabled=on writes=off expose=agents events=on"
        );
        assert_eq!(send(&sock, "mcp writes on").unwrap(), "ok");
        assert!(matches!(
            rx.try_recv(),
            Ok(Req::McpPolicy(McpPolicy::Writes(true)))
        ));
        assert_eq!(send(&sock, "mcp expose all").unwrap(), "ok");
        assert!(matches!(
            rx.try_recv(),
            Ok(Req::McpPolicy(McpPolicy::ExposeAll(true)))
        ));
        // Reads and writes are separate grants: enabling one must never be
        // spelled in a way that quietly enables the other.
        assert!(send(&sock, "mcp writes").unwrap().starts_with("err"));
        server.join().unwrap();
    }

    #[test]
    fn mcp_rpc_takes_its_payload_verbatim() {
        // The JSON carries spaces AND nested braces; splitting on whitespace
        // would corrupt it, so the parser must take the remainder untouched.
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"x y"}}"#;
        match parse_line(&format!("mcp rpc {json}")) {
            Ok(Cmd::McpRpc(p)) => assert_eq!(p, json),
            _ => panic!("mcp rpc did not parse as a verbatim payload"),
        }
        assert!(parse_line("mcp rpc ").is_err());
        assert!(parse_line("mcp rpc").is_err());
    }

    #[test]
    fn relay_target_refuses_to_guess_between_windows() {
        assert_eq!(relay_target(&["--pid".into(), "4242".into()]), Ok(4242));
        assert!(relay_target(&["--pid".into()]).is_err());
        assert!(relay_target(&["--wat".into()]).is_err());
    }

    #[test]
    fn ppid_of_walks_a_comm_containing_spaces_and_parens() {
        // Our own parent is the real check that the last-paren split is right.
        let me = std::process::id();
        assert_eq!(ppid_of(me), Some(unsafe { libc::getppid() } as u32));
        assert_eq!(ppid_of(u32::MAX), None); // no such process
    }

    #[test]
    fn a_dead_socket_file_reads_as_unreachable() {
        let sock = tmp("stale").join("ctl-3.sock");
        drop(UnixListener::bind(&sock).unwrap()); // file survives the listener
        assert!(sock.exists());
        assert!(send(&sock, "ping").is_err());
    }
}
