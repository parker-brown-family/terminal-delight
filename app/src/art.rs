//! The TD mascot, embedded — Parker's robot and its yellow "HEY" blinker.
//!
//! Two same-canvas layers cut from one piece of art (1254² source, shipped at
//! 64² — the tab renders it ~15px, so that is already 4× headroom): the robot
//! body stands steady while the blinker layer is overlaid at identical bounds
//! and BLINKED for the needs-input state. Compiled into the binary like the
//! bell ping, so a bare deployed `td-<sha>` carries its own face; materialised
//! to the runtime dir on demand because gpui's `img()` eats paths.
use std::path::PathBuf;

const ROBOT: &[u8] = include_bytes!("../assets/img/robot-only.png");
const BLINKER: &[u8] = include_bytes!("../assets/img/blinker-only.png");

/// Write an embedded asset to the runtime dir once (size-checked, so a
/// truncated write from a crashed run heals) and hand back the path.
fn runtime_asset(name: &str, bytes: &[u8]) -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let p = base.join(name);
    let ok = std::fs::metadata(&p).map(|m| m.len() == bytes.len() as u64);
    if !matches!(ok, Ok(true)) {
        let _ = std::fs::write(&p, bytes);
    }
    p
}

/// The robot body (no blinker) — the WORKING glyph, and the steady base under
/// the blinking needs-input overlay.
pub fn robot_png() -> PathBuf {
    runtime_asset("terminal-delight-robot.png", ROBOT)
}

/// The yellow HEY blinker (bulb + rays, transparent elsewhere) — overlaid on
/// the robot at identical bounds and hard-BLINKED, never throbbed.
pub fn blinker_png() -> PathBuf {
    runtime_asset("terminal-delight-blinker.png", BLINKER)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both layers ship inside the binary as real PNGs — a bad asset path or a
    /// stripped commit fails HERE, not as an invisible mascot at runtime.
    #[test]
    fn the_embedded_mascot_layers_are_real_pngs() {
        for (name, bytes) in [("robot", ROBOT), ("blinker", BLINKER)] {
            assert!(bytes.len() > 500, "{name} suspiciously small");
            assert!(
                bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
                "{name} is not a PNG stream"
            );
        }
    }
}
