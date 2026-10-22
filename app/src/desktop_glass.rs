//! Compositor policy for desktop-facing glass. Unknown opacity keeps a solid
//! face; fullscreen and a measured 100% opacity also keep a solid face.
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
