//! Compositor policy for desktop-facing glass. Unknown opacity keeps a solid
//! face; fullscreen and a measured 100% opacity also keep a solid face.
//!
//! An unboxed skin also takes the compositor's border off this process's own
//! windows, and gives it back when the skin changes: the tile is borderless on
//! the outside. Only our windows, by pid, and only the one property.
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
    let clients: serde_json::Value = serde_json::from_slice(reply).ok()?;
    Some(
        clients
            .as_array()?
            .iter()
            .filter(|c| c.get("pid").and_then(|v| v.as_u64()) == Some(u64::from(pid)))
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

#[derive(Debug, PartialEq)]
enum Border {
    Applied,
    /// No window yet, or no readable answer: worth asking again.
    NotYet,
    /// The compositor answered and said no; asking again will not change it.
    Refused,
}

fn apply_border(path: &Path, borderless: bool) -> Border {
    let Some(windows) = ask(path, "j/clients", 4 << 20)
        .and_then(|reply| own_windows(&reply, std::process::id()))
        .filter(|w| !w.is_empty())
    else {
        return Border::NotYet;
    };
    for address in windows {
        match ask(path, &border_dispatch(&address, borderless), 4096) {
            None => return Border::NotYet,
            Some(reply) if reply.trim_ascii() != b"ok" => return Border::Refused,
            Some(_) => {}
        }
    }
    Border::Applied
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
                if outcome == Border::Refused {
                    eprintln!("terminal-delight: the compositor refused border_size");
                }
                if outcome != Border::NotYet {
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
