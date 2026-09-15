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
/// The app's own CRT mark — `favicon.svg` rasterised at 128², which is ~7×
/// headroom over the ~18px the mother bar draws it at. A raster rather than the
/// SVG because gpui paints an SVG as a single-colour MASK, and this mark is
/// three phosphor colours in a bezel: as a mask it is a filled rounded square.
const MARK: &[u8] = include_bytes!("../assets/img/logo-mark.png");

/// Write an embedded asset to the runtime dir once (size-checked, so a
/// truncated write from a crashed run heals) and hand back the path.
///
/// Shared with [`crate::toolprop`], which makes the same bargain for the tool
/// plates: compiled in so a bare `td-<sha>` carries its own art, materialised
/// on demand because gpui's `img()` eats paths.
pub(crate) fn runtime_asset(name: &str, bytes: &[u8]) -> PathBuf {
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

/// The app's mark, for the mother bar's top-left corner.
///
/// It replaced the words `▸ TERMINAL DELIGHT`, which cost the widest slot on
/// the busiest row to say something that does not change and that the window's
/// own title already says — and which, on a session whose project happened to
/// be called Terminal Delight, printed the same three words twice, stacked.
pub fn mark_png() -> PathBuf {
    runtime_asset("terminal-delight-mark.png", MARK)
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
