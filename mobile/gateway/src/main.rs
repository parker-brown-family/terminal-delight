//! td-mobile-gateway — the door between a phone and a Terminal Delight session.
//!
//! ```text
//! td-mobile-gateway serve [--port 7717] [--lan]
//! td-mobile-gateway pair  [--port 7717] [--serial <adb serial>]
//! td-mobile-gateway token [--rotate]
//! ```
//!
//! `serve` listens on loopback (which `adb reverse` carries the phone to) and
//! on this machine's tailnet address; `--lan` adds every other interface, in
//! plaintext, for a phone on the home Wi-Fi without Tailscale. `pair` hands the
//! phone its token over USB. See `docs/plans/mobile/01-plan.md`.

mod host;
mod mailbox;
mod paste;
mod paths;
mod tree;
mod web;

use std::collections::HashSet;
use std::io::Read;
use std::net::{IpAddr, SocketAddr};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, RwLock};
use std::time::Duration;

const DEFAULT_PORT: u16 = 7717;
const APP_ID: &str = "ca.brownfamilysports.terminaldelight";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let value = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let port: u16 = value("--port")
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let outcome = match args.first().map(String::as_str) {
        Some("serve") => serve(port, flag("--lan")),
        Some("pair") => pair(port, value("--serial")),
        Some("token") => token(flag("--rotate")),
        _ => Err(
            "usage: td-mobile-gateway serve [--port N] [--lan] | pair [--serial S] | token [--rotate]"
                .into(),
        ),
    };
    if let Err(e) = outcome {
        eprintln!("td-mobile-gateway: {e}");
        std::process::exit(2);
    }
}

fn token_path() -> std::path::PathBuf {
    paths::gateway_dir().join("token")
}

/// The pairing token: 32 bytes from the kernel, as hex, in a 0600 file inside a
/// 0700 directory. Minted on first use; `--rotate` unpairs every phone.
fn ensure_token(rotate: bool) -> Result<String, String> {
    let path = token_path();
    if !rotate {
        if let Ok(t) = std::fs::read_to_string(&path) {
            let t = t.trim().to_string();
            if t.len() == 64 {
                return Ok(t);
            }
        }
    }
    let dir = paths::gateway_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut raw = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut raw))
        .map_err(|e| format!("/dev/urandom: {e}"))?;
    let t: String = raw.iter().map(|b| format!("{b:02x}")).collect();
    let tmp = dir.join("token.tmp");
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| format!("{}: {e}", tmp.display()))?;
        f.write_all(t.as_bytes()).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(t)
}

fn token(rotate: bool) -> Result<(), String> {
    let t = ensure_token(rotate)?;
    if rotate {
        eprintln!("rotated: every paired phone must pair again");
    }
    println!("{t}");
    Ok(())
}

fn machine_name() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|s| s.trim().to_string())
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "this machine".into())
}

/// This machine on the tailnet: its IPv4 address and its MagicDNS name. Absent
/// when Tailscale is not installed or not up — not an error, just a route the
/// phone will not be offered.
fn tailnet() -> (Option<IpAddr>, Option<String>) {
    let ip = Command::new("tailscale")
        .args(["ip", "-4"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.lines().next().and_then(|l| l.trim().parse().ok()));
    let name = Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
        .and_then(|v| v.pointer("/Self/DNSName")?.as_str().map(str::to_string))
        .map(|n| n.trim_end_matches('.').to_string())
        .filter(|n| !n.is_empty());
    (ip, name)
}

/// Every IPv4 address on this machine with the interface it sits on, for
/// `--lan`'s Host check and the pairing link's Wi-Fi routes.
fn local_ipv4s() -> Vec<(String, IpAddr)> {
    Command::new("ip")
        .args(["-4", "-o", "addr", "show"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| {
            s.lines()
                .filter_map(|l| {
                    let mut words = l.split_whitespace();
                    let iface = words.nth(1)?.to_string();
                    let cidr = words.nth(1)?;
                    Some((iface, cidr.split('/').next()?.parse().ok()?))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// An interface a phone on the home network could actually reach: not
/// loopback, not the tailnet (offered separately), not a container bridge.
fn reachable_from_lan(iface: &str) -> bool {
    !["lo", "docker", "br-", "veth", "virbr", "tailscale", "podman", "cni"]
        .iter()
        .any(|p| iface.starts_with(p))
}

fn serve(port: u16, lan: bool) -> Result<(), String> {
    let token = ensure_token(false)?;
    let machine = machine_name();

    let mut allowed: HashSet<String> = HashSet::new();
    let mut allow = |h: &str| {
        allowed.insert(format!("{}:{port}", h.to_ascii_lowercase()));
    };
    allow("127.0.0.1");
    allow("localhost");
    allow(&machine);
    let addr = if lan {
        for (_, ip) in local_ipv4s() {
            allow(&ip.to_string());
        }
        // One wildcard listener covers every interface, loopback and the
        // tailnet included.
        SocketAddr::from(([0, 0, 0, 0], port))
    } else {
        SocketAddr::from(([127, 0, 0, 1], port))
    };

    let (events, _) = tokio::sync::broadcast::channel(256);
    let gate = Arc::new(web::Gate {
        token,
        machine: machine.clone(),
        tailnet_name: RwLock::new(None),
        allowed_hosts: RwLock::new(allowed),
        events,
        watchers: AtomicUsize::new(0),
    });

    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async move {
        let app = web::router(gate.clone());
        tokio::spawn(web::poll(gate.clone()));
        let l = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| format!("{addr}: {e} — is another gateway already running?"))?;
        eprintln!("td-mobile-gateway: listening on http://{addr}");
        let loopback = app.clone();
        tokio::spawn(async move {
            let _ = axum::serve(l, loopback).await;
        });
        tokio::spawn(keep_tailnet(gate, app, port, !lan));
        std::thread::spawn(move || keep_usb_route(port));
        eprintln!(
            "td-mobile-gateway: {machine} · sessions under {}",
            paths::runtime_dir().display()
        );
        std::future::pending::<()>().await;
        Ok(())
    })
}

/// The tailnet route, which may arrive after the gateway does. At boot the
/// gateway can start in the same second as tailscaled and ask before it has an
/// address; on 2026-10-13 both started at 13:04:54 and the address came five
/// seconds later, so a route read once at startup stayed missing until
/// somebody restarted the gateway. This asks again every ten seconds until the
/// address is served, then stops. Under `--lan` the wildcard listener already
/// covers it and only the `Host` check needs to learn the names.
async fn keep_tailnet(gate: Arc<web::Gate>, app: axum::Router, port: u16, bind: bool) {
    let mut said = false;
    loop {
        let (ip, name) = tokio::task::spawn_blocking(tailnet)
            .await
            .unwrap_or((None, None));
        if let Some(ip) = ip {
            gate.allow_tailnet(ip, name.clone(), port);
            let addr = SocketAddr::new(ip, port);
            if !bind {
                return;
            }
            match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => {
                    let name = name.map(|n| format!(" ({n})")).unwrap_or_default();
                    eprintln!("td-mobile-gateway: listening on http://{addr}{name}");
                    let _ = axum::serve(l, app).await;
                    return;
                }
                Err(e) if !said => {
                    eprintln!("td-mobile-gateway: {addr}: {e}; trying every 10s");
                    said = true;
                }
                Err(_) => {}
            }
        } else if !said {
            eprintln!("td-mobile-gateway: no tailnet address yet; looking every 10s");
            said = true;
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

/// The USB route, kept. `adb reverse` lives with the cable's connection, so a
/// replug, a reboot or an adb restart drops it and the phone finds nothing at
/// its own 127.0.0.1 until `pair` runs again. This follows adb's device list
/// — it speaks only when the list changes, so nothing is polled — and carries
/// the port again for every attached phone that has the app.
fn keep_usb_route(port: u16) {
    let want = format!("tcp:{port} tcp:{port}");
    loop {
        // Starts the adb server if none is running, which is what lets the
        // cable work after a reboot without anybody typing `adb`.
        let Ok(mut child) = Command::new("adb")
            .arg("track-devices")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        else {
            eprintln!("td-mobile-gateway: no adb on PATH, so the USB route needs `pair` after every replug");
            return;
        };
        if let Some(mut out) = child.stdout.take() {
            let mut len = [0u8; 4];
            while out.read_exact(&mut len).is_ok() {
                let Some(n) = std::str::from_utf8(&len)
                    .ok()
                    .and_then(|h| usize::from_str_radix(h, 16).ok())
                else {
                    break;
                };
                let mut frame = vec![0u8; n];
                if out.read_exact(&mut frame).is_err() {
                    break;
                }
                for phone in attached(&String::from_utf8_lossy(&frame)) {
                    let serial = Some(phone.clone());
                    let has_app = adb(&serial, &["shell", "pm", "list", "packages", APP_ID])
                        .is_ok_and(|o| o.lines().any(|l| l.trim() == format!("package:{APP_ID}")));
                    let carried = adb(&serial, &["reverse", "--list"]).is_ok_and(|o| o.contains(&want));
                    if has_app && !carried {
                        let route = format!("tcp:{port}");
                        match adb(&serial, &["reverse", &route, &route]) {
                            Ok(_) => eprintln!("td-mobile-gateway: USB route: {phone} 127.0.0.1:{port} → here"),
                            Err(e) => eprintln!("td-mobile-gateway: {e}"),
                        }
                    }
                }
            }
        }
        // The adb server went away; ask again shortly, which restarts it.
        let _ = child.kill();
        let _ = child.wait();
        std::thread::sleep(Duration::from_secs(3));
    }
}

/// The serials in one `adb track-devices` frame that are ready to use — not
/// `offline`, not `unauthorized` (the phone has not accepted this computer).
fn attached(frame: &str) -> Vec<String> {
    frame
        .lines()
        .filter_map(|l| {
            let (serial, state) = l.split_once('\t')?;
            (state.trim() == "device").then(|| serial.to_string())
        })
        .collect()
}

fn adb(serial: &Option<String>, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("adb");
    if let Some(s) = serial {
        cmd.args(["-s", s]);
    }
    let out = cmd
        .args(args)
        .output()
        .map_err(|e| format!("adb: {e} — is the Android platform-tools on PATH?"))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        return Err(format!("adb {}: {}", args.join(" "), text.trim()));
    }
    Ok(text)
}

/// Pair the phone on the USB cable: carry its loopback port to ours, then hand
/// it the token as a deep link the app shows as a confirmation sheet. The link
/// is a proposal — any app can fire one — and the person accepting it on the
/// glass is the pairing.
fn pair(port: u16, serial: Option<String>) -> Result<(), String> {
    let token = ensure_token(false)?;
    let machine = machine_name();
    let (ts_ip, ts_name) = tailnet();
    adb(&serial, &["reverse", &format!("tcp:{port}"), &format!("tcp:{port}")])?;
    let mut uri = format!("tdmobile://pair?token={token}&machine={machine}&port={port}");
    if let Some(ip) = ts_ip {
        uri.push_str(&format!("&tailnet_ip={ip}"));
    }
    if let Some(n) = &ts_name {
        uri.push_str(&format!("&tailnet_name={n}"));
    }
    for (iface, ip) in local_ipv4s() {
        if let IpAddr::V4(v4) = ip {
            if v4.is_private() && reachable_from_lan(&iface) {
                uri.push_str(&format!("&lan={ip}"));
            }
        }
    }
    // The whole `am` line is one argument to the phone's shell, so the URI's
    // ampersands are quoted there rather than here.
    let shell = format!("am start -W -a android.intent.action.VIEW -d '{uri}' {APP_ID}");
    let out = adb(&serial, &["shell", &shell])?;
    if out.contains("Error") {
        return Err(format!("the phone refused the pairing link: {}", out.trim()));
    }
    println!("USB route: phone 127.0.0.1:{port} → this machine 127.0.0.1:{port}");
    println!("pairing sheet shown on the phone — accept it there");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_phone_that_accepted_this_computer_is_attached() {
        let frame = "R5CTB0B6W4T\tdevice\nemulator-5554\toffline\nZX1G22\tunauthorized\n";
        assert_eq!(attached(frame), vec!["R5CTB0B6W4T".to_string()]);
        assert!(attached("").is_empty());
    }
}
