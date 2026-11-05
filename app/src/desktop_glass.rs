//! Compositor policy for desktop-facing glass. Unknown opacity keeps a solid
//! face; fullscreen and a measured 100% opacity also keep a solid face.
//!
//! An unboxed skin also takes the compositor's border off this process's own
//! windows, and gives it back when the skin changes: the tile is borderless on
//! the outside. Only our windows, by pid, and only the one property.
//!
//! On Omarchy every window of ours also carries the `terminal` tag, because
//! its Super+C reads that tag at the press: a tagged window gets Ctrl+Insert,
//! which copies, and any other gets Ctrl+C, which interrupts the agent in the
//! pane. Omarchy's list of terminals doesn't name us on 4.0, so nothing else
//! tags us there.
use gpui::{App, Global, Window};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Copy, Default, PartialEq)]
struct Policy {
    active: Option<f32>,
    inactive: Option<f32>,
}
impl Global for Policy {}

fn opacity(reply: &[u8]) -> Option<f32> {
    let reply: serde_json::Value = serde_json::from_slice(reply).ok()?;
    let value = reply.get("float")?.as_f64()? as f32;
    (value.is_finite() && (0. ..=1.).contains(&value)).then_some(value)
}

fn query(path: &Path, key: &str) -> Option<f32> {
    let mut peer = UnixStream::connect(path).ok()?;
    let timeout = Some(Duration::from_millis(100));
    peer.set_read_timeout(timeout).ok()?;
    peer.set_write_timeout(timeout).ok()?;
    peer.write_all(format!("j/getoption decoration:{key}_opacity").as_bytes())
        .ok()?;
    // An option reply is tiny. Bound it even if a different socket answers.
    let mut reply = Vec::new();
    peer.take(4096).read_to_end(&mut reply).ok()?;
    opacity(&reply)
}

fn permits(opacity: Option<f32>, fullscreen: bool) -> bool {
    !fullscreen && opacity.is_some_and(|value| value < 1.)
}

pub fn allowed(window: &Window, cx: &App) -> bool {
    let Some(policy) = cx.try_global::<Policy>() else {
        return false;
    };
    permits(
        if window.is_window_active() {
            policy.active
        } else {
            policy.inactive
        },
        window.is_fullscreen(),
    )
}

/// One request on Hyprland's command socket, its answer bounded.
fn ask(path: &Path, command: &str, limit: u64) -> Option<Vec<u8>> {
    let mut peer = UnixStream::connect(path).ok()?;
    let timeout = Some(Duration::from_millis(250));
    peer.set_read_timeout(timeout).ok()?;
    peer.set_write_timeout(timeout).ok()?;
    peer.write_all(command.as_bytes()).ok()?;
    let mut reply = Vec::new();
    peer.take(limit).read_to_end(&mut reply).ok()?;
    Some(reply)
}

/// Every window this process owns, by address. Several Terminal Delights share
/// one desktop, so a window is ours by pid, never by class or title. `None`
/// when the reply is unreadable, which is not the same as owning none.
fn own_windows(reply: &[u8], pid: u32) -> Option<Vec<String>> {
    windows_where(reply, pid, |_| true)
}

/// Those of them not yet a terminal. A rule's tag reads `terminal*`, ours
/// `terminal`, and either is enough for Omarchy's binding.
fn untagged_windows(reply: &[u8], pid: u32) -> Option<Vec<String>> {
    windows_where(reply, pid, |client| {
        !client
            .get("tags")
            .and_then(|tags| tags.as_array())
            .is_some_and(|tags| {
                tags.iter()
                    .filter_map(|tag| tag.as_str())
                    .any(|tag| tag.trim_end_matches('*') == "terminal")
            })
    })
}

fn windows_where(
    reply: &[u8],
    pid: u32,
    keep: impl Fn(&serde_json::Value) -> bool,
) -> Option<Vec<String>> {
    let clients: serde_json::Value = serde_json::from_slice(reply).ok()?;
    Some(
        clients
            .as_array()?
            .iter()
            .filter(|c| c.get("pid").and_then(|v| v.as_u64()) == Some(u64::from(pid)))
            .filter(|c| keep(c))
            .filter_map(|c| c.get("address")?.as_str())
            // The address goes into a Lua string below; take only the shape
            // Hyprland gives it.
            .filter(|a| {
                a.strip_prefix("0x")
                    .is_some_and(|h| !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit()))
            })
            .map(str::to_owned)
            .collect(),
    )
}

/// Drop one window's compositor border, or hand it back to the config. `unset`
/// rather than a number, so Parker's own `general:border_size` returns.
fn border_dispatch(address: &str, borderless: bool) -> String {
    let value = if borderless { "0" } else { "unset" };
    format!(
        "dispatch hl.dsp.window.set_prop({{ window = \"address:{address}\", \
         prop = \"border_size\", value = \"{value}\" }})"
    )
}

/// Make one window a terminal. `+` sets the tag where a bare name would toggle
/// it off a window that already has it.
fn tag_dispatch(address: &str) -> String {
    format!("dispatch hl.dsp.window.tag({{ tag = \"+terminal\", window = \"address:{address}\" }})")
}

#[derive(Debug, PartialEq)]
enum Sent {
    Applied,
    /// No window yet, or no readable answer: worth asking again.
    NotYet,
    /// The compositor answered and said no; asking again will not change it.
    Refused,
}

fn send_each(path: &Path, windows: Vec<String>, dispatch: impl Fn(&str) -> String) -> Sent {
    for address in windows {
        match ask(path, &dispatch(&address), 4096) {
            None => return Sent::NotYet,
            Some(reply) if reply.trim_ascii() != b"ok" => return Sent::Refused,
            Some(_) => {}
        }
    }
    Sent::Applied
}

fn apply_border(path: &Path, borderless: bool) -> Sent {
    let Some(windows) = ask(path, "j/clients", 4 << 20)
        .and_then(|reply| own_windows(&reply, std::process::id()))
        .filter(|w| !w.is_empty())
    else {
        return Sent::NotYet;
    };
    send_each(path, windows, |address| {
        border_dispatch(address, borderless)
    })
}

/// Tag every window of `pid` the compositor knows. A window we opened a moment
/// ago may not be mapped yet, so settling takes seeing all `expected` of them:
/// an untagged window missed here would stay one until the next window opens.
fn apply_tags(path: &Path, pid: u32, expected: usize) -> Sent {
    let Some(reply) = ask(path, "j/clients", 4 << 20) else {
        return Sent::NotYet;
    };
    let (Some(mapped), Some(untagged)) = (own_windows(&reply, pid), untagged_windows(&reply, pid))
    else {
        return Sent::NotYet;
    };
    match send_each(path, untagged, tag_dispatch) {
        Sent::Applied if mapped.len() < expected => Sent::NotYet,
        outcome => outcome,
    }
}

pub fn init(cx: &mut App) {
    cx.set_global(Policy::default());
    let (Some(runtime), Some(signature)) = (
        std::env::var_os("XDG_RUNTIME_DIR"),
        std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE"),
    ) else {
        return;
    };
    let path = std::path::PathBuf::from(runtime)
        .join("hypr")
        .join(signature)
        .join(".socket.sock");
    let border_path = path.clone();
    if crate::palette::omarchy_installed() {
        let tag_path = path.clone();
        cx.spawn(async move |cx| {
            // Hyprland is asked only while our window list differs from the
            // one last settled, so a window at rest costs no request.
            let mut settled = Vec::new();
            loop {
                let windows: Vec<_> = cx.update(|cx| {
                    cx.windows()
                        .iter()
                        .map(|window| window.window_id())
                        .collect()
                });
                if windows != settled {
                    let path = tag_path.clone();
                    let expected = windows.len();
                    let outcome = cx
                        .background_executor()
                        .spawn(async move { apply_tags(&path, std::process::id(), expected) })
                        .await;
                    if outcome == Sent::Refused {
                        eprintln!("terminal-delight: the compositor refused the terminal tag");
                    }
                    if outcome != Sent::NotYet {
                        settled = windows;
                    }
                }
                cx.background_executor().timer(Duration::from_secs(1)).await;
            }
        })
        .detach();
    }
    cx.spawn(async move |cx| {
        // The state the compositor last settled on for us. `None` until one
        // has: a border we never took away has nothing to give back.
        let mut settled: Option<bool> = None;
        loop {
            let want = cx.update(|cx| crate::skin::skin(cx, 1.).unboxed());
            if settled != Some(want) && (want || settled.is_some()) {
                let path = border_path.clone();
                let outcome = cx
                    .background_executor()
                    .spawn(async move { apply_border(&path, want) })
                    .await;
                if outcome == Sent::Refused {
                    eprintln!("terminal-delight: the compositor refused border_size");
                }
                if outcome != Sent::NotYet {
                    settled = Some(want);
                }
            }
            cx.background_executor().timer(Duration::from_secs(1)).await;
        }
    })
    .detach();
    cx.spawn(async move |cx| loop {
        if !cx.update(|cx| crate::skin::skin(cx, 1.).m.frost > 0.) {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            continue;
        }
        let path = path.clone();
        let policy = cx
            .background_executor()
            .spawn(async move {
                Policy {
                    active: query(&path, "active"),
                    inactive: query(&path, "inactive"),
                }
            })
            .await;
        cx.update(|cx| {
            if *cx.global::<Policy>() != policy {
                cx.set_global(policy);
                cx.refresh_windows();
            }
        });
        cx.background_executor().timer(Duration::from_secs(1)).await;
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_full_opacity_and_fullscreen_keep_the_backing() {
        assert!(!permits(None, false));
        assert!(!permits(Some(1.), false));
        assert!(!permits(Some(0.86), true));
        assert!(permits(Some(0.86), false));
        assert!(
            permits(Some(0.999), false),
            "only full opacity closes the glass"
        );
    }
    #[test]
    fn only_this_processes_windows_lose_their_border() {
        let reply = br#"[
            {"address":"0x57761de70370","pid":42,"class":"terminal-delight"},
            {"address":"0xabc","pid":7,"class":"terminal-delight"},
            {"address":"0xdef\") os.exit() --","pid":42},
            {"address":"0x1","pid":42}
        ]"#;
        assert_eq!(
            own_windows(reply, 42),
            Some(vec!["0x57761de70370".to_owned(), "0x1".to_owned()]),
            "another instance's window and a malformed address are left alone"
        );
        assert_eq!(own_windows(b"[]", 42), Some(vec![]));
        assert_eq!(own_windows(b"ok", 42), None, "unreadable is not none");
    }
    #[test]
    fn the_border_dispatch_is_the_form_hyprland_accepted() {
        // Both strings were sent to Hyprland 0.56 on 2026-11-02; each answered
        // `ok`, and `getprop … border_size` read 0, then the config's 3 again.
        assert_eq!(
            border_dispatch("0x57761de70370", true),
            r#"dispatch hl.dsp.window.set_prop({ window = "address:0x57761de70370", prop = "border_size", value = "0" })"#
        );
        assert!(border_dispatch("0x1", false).contains(r#"value = "unset""#));
    }
    #[test]
    fn only_this_processes_untagged_windows_are_tagged() {
        let reply = br#"[
            {"address":"0xa","pid":42,"tags":["default-opacity*"]},
            {"address":"0xb","pid":42,"tags":["terminal"]},
            {"address":"0xc","pid":42,"tags":["terminal*"]},
            {"address":"0xd","pid":42,"tags":["terminals","not-terminal"]},
            {"address":"0xe","pid":42},
            {"address":"0xf","pid":7,"tags":[]}
        ]"#;
        assert_eq!(
            untagged_windows(reply, 42),
            Some(vec!["0xa".to_owned(), "0xd".to_owned(), "0xe".to_owned()]),
            "a tag by us or by a rule counts, a name that contains it does not"
        );
        assert_eq!(untagged_windows(b"ok", 42), None, "unreadable is not none");
    }
    #[test]
    fn the_tag_dispatch_is_the_form_hyprland_accepted() {
        // Sent to Hyprland 0.56.2 on Omarchy 4.0.4 on 2026-11-04: it answered
        // `ok`, and `j/clients` then read `terminal` among the window's tags.
        assert_eq!(
            tag_dispatch("0x5f328f10aa00"),
            r#"dispatch hl.dsp.window.tag({ tag = "+terminal", window = "address:0x5f328f10aa00" })"#
        );
    }
    /// A Hyprland that answers `j/clients` with `clients` and every dispatch
    /// with `answer`, and hands back what it was asked.
    fn compositor(
        name: &str,
        clients: &'static str,
        answer: &'static str,
        requests: usize,
    ) -> (std::path::PathBuf, std::thread::JoinHandle<Vec<String>>) {
        let path =
            std::env::temp_dir().join(format!("td-glass-{name}-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let socket = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let heard = std::thread::spawn(move || {
            (0..requests)
                .map(|_| {
                    let (mut peer, _) = socket.accept().unwrap();
                    let mut request = [0; 512];
                    let n = peer.read(&mut request).unwrap();
                    let request = String::from_utf8_lossy(&request[..n]).into_owned();
                    let reply = if request == "j/clients" {
                        clients
                    } else {
                        answer
                    };
                    peer.write_all(reply.as_bytes()).unwrap();
                    request
                })
                .collect()
        });
        (path, heard)
    }
    #[test]
    fn a_window_not_mapped_yet_keeps_the_tagging_open() {
        let clients = r#"[{"address":"0xa","pid":42,"tags":[]}]"#;
        let (path, heard) = compositor("unmapped", clients, "ok", 2);
        assert_eq!(apply_tags(&path, 42, 2), Sent::NotYet);
        assert_eq!(
            heard.join().unwrap(),
            ["j/clients".to_owned(), tag_dispatch("0xa")],
            "the window that is there is tagged while the other is awaited"
        );
    }
    #[test]
    fn tagged_windows_settle_without_a_dispatch() {
        let clients = r#"[{"address":"0xa","pid":42,"tags":["terminal*"]}]"#;
        let (path, heard) = compositor("tagged", clients, "unused", 1);
        assert_eq!(apply_tags(&path, 42, 1), Sent::Applied);
        assert_eq!(heard.join().unwrap(), ["j/clients"]);
    }
    #[test]
    fn a_compositor_without_the_dispatcher_is_not_asked_again() {
        let clients = r#"[{"address":"0xa","pid":42,"tags":[]}]"#;
        let (path, heard) = compositor("refused", clients, "Invalid dispatcher", 2);
        assert_eq!(apply_tags(&path, 42, 1), Sent::Refused);
        heard.join().unwrap();
    }
    #[test]
    fn unreadable_or_out_of_range_policy_stays_unknown() {
        assert_eq!(opacity(br#"{"float":0.94}"#), Some(0.94));
        for input in [
            br#"{}"#.as_slice(),
            br#"{"float":null}"#,
            br#"{"float":2}"#,
            b"unavailable",
        ] {
            assert_eq!(opacity(input), None);
        }
    }
}
