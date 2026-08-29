//! ctl — the control socket: how the desktop talks to a RUNNING terminal.
//!
//! Every terminal-delight process listens on its own unix socket,
//! `$XDG_RUNTIME_DIR/terminal-delight/ctl-<pid>.sock`, for one-line commands —
//! today that is PAINT mode (the per-pane palette overlay) plus `ping` and
//! `paint status`. The same binary is also the client: `terminal-delight ctl
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

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use gpui::Context;

use crate::{theme, Workspace};

/// A queued paint request, applied on the UI thread by the ticker.
pub(crate) enum Req {
    Set(bool),
    Toggle,
}

/// One parsed request line.
enum Cmd {
    Ping,
    PaintStatus,
    Paint(Req),
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

fn parse_line(s: &str) -> Result<Cmd, String> {
    let mut w = s.split_whitespace();
    match (w.next(), w.next(), w.next()) {
        (Some("ping"), None, _) => Ok(Cmd::Ping),
        (Some("paint"), Some("on"), None) => Ok(Cmd::Paint(Req::Set(true))),
        (Some("paint"), Some("off"), None) => Ok(Cmd::Paint(Req::Set(false))),
        (Some("paint"), Some("toggle"), None) => Ok(Cmd::Paint(Req::Toggle)),
        (Some("paint"), Some("status"), None) => Ok(Cmd::PaintStatus),
        _ => Err(format!(
            "unknown command {s:?} — try: ping | paint on|off|toggle|status"
        )),
    }
}

/// Serve one connection: read a line, answer a line. Short timeouts on both
/// directions so a wedged client can never stall the single accept loop.
fn handle_conn(stream: UnixStream, mirror: &AtomicBool, tx: &mpsc::Sender<Req>) {
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
        Ok(Cmd::Ping) => "pong".to_string(),
        Ok(Cmd::PaintStatus) => if mirror.load(Ordering::Relaxed) {
            "on"
        } else {
            "off"
        }
        .to_string(),
        Ok(Cmd::Paint(req)) => {
            if tx.send(req).is_ok() {
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

    {
        let mirror = Arc::clone(&mirror);
        let _ = thread::Builder::new().name("td-ctl".into()).spawn(move || {
            for conn in listener.incoming() {
                let Ok(stream) = conn else { continue };
                handle_conn(stream, &mirror, &tx);
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
        let applied = this.update(cx, |_ws, cx| {
            for c in cmds {
                match c {
                    Req::Set(v) => theme::set_paint_mode(cx, v),
                    Req::Toggle => {
                        let v = !theme::paint_mode(cx);
                        theme::set_paint_mode(cx, v);
                    }
                }
            }
            mirror.store(theme::paint_mode(cx), Ordering::Relaxed);
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
            w if !w.starts_with('-') => words.push(w),
            other => return Err(format!("unknown flag {other:?}")),
        }
    }
    let line = words.join(" ");
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

/// One request/response round trip against a socket.
fn send(path: &Path, line: &str) -> std::io::Result<String> {
    let mut s = UnixStream::connect(path)?;
    let _ = s.set_read_timeout(Some(Duration::from_millis(800)));
    let _ = s.set_write_timeout(Some(Duration::from_millis(800)));
    writeln!(s, "{line}")?;
    let mut reply = String::new();
    BufReader::new(&mut s).read_line(&mut reply)?;
    Ok(reply.trim().to_string())
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
                "usage: terminal-delight ctl <ping | paint on|off|toggle|status> \
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
    fn the_grammar_is_exactly_five_lines_long() {
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
        assert!(parse_line("paint").is_err());
        assert!(parse_line("paint sideways").is_err());
        assert!(parse_line("paint on extra").is_err());
        assert!(parse_line("").is_err());
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
        let server = thread::spawn(move || {
            // exactly four connections, in test order
            for _ in 0..4 {
                let (stream, _) = listener.accept().unwrap();
                handle_conn(stream, &m2, &tx);
            }
        });
        assert_eq!(send(&sock, "ping").unwrap(), "pong");
        assert_eq!(send(&sock, "paint status").unwrap(), "off");
        assert_eq!(send(&sock, "paint toggle").unwrap(), "ok");
        assert!(matches!(rx.try_recv(), Ok(Req::Toggle)));
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
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle_conn(stream, &mirror, &tx);
        });
        assert!(send(&sock, "sudo make me a sandwich")
            .unwrap()
            .starts_with("err"));
        server.join().unwrap();
    }

    #[test]
    fn a_dead_socket_file_reads_as_unreachable() {
        let sock = tmp("stale").join("ctl-3.sock");
        drop(UnixListener::bind(&sock).unwrap()); // file survives the listener
        assert!(sock.exists());
        assert!(send(&sock, "ping").is_err());
    }
}
