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
///
/// Also the logo a new install's panes wear: [`crate::dirlogo::seed_if_absent`]
/// writes these bytes beside the config it seeds.
pub(crate) const MARK: &[u8] = include_bytes!("../assets/img/logo-mark.png");

/// The pace glyph — a flame over a bowl, drawn once in `assets/img/pace-flame.svg`
/// and tinted per subscription by the band its pace falls in (cool, on pace, hot).
/// An SVG rather than a raster, unlike `MARK`: gpui paints an SVG as a
/// single-colour mask, which is exactly the point here — one shape, three
/// meanings, carried entirely by `text_color`.
pub(crate) const PACE_FLAME: &[u8] = include_bytes!("../assets/img/pace-flame.svg");

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
    record_asset(&base, name, bytes);
    p
}

/// The receipt for what [`runtime_asset`] unpacks: one `<sha256>\t<name>` line
/// per file, in `terminal-delight-assets.manifest` beside the files, written
/// once per name per process. The uninstaller removes a loose file only when
/// its bytes still hash to what TD recorded under that name, so a file another
/// program happened to give a TD-like name is never TD's to delete.
pub(crate) fn record_asset(base: &std::path::Path, name: &str, bytes: &[u8]) {
    use sha2::{Digest, Sha256};
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    static SEEN: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    let first = SEEN
        .get_or_init(Default::default)
        .lock()
        .map(|mut seen| seen.insert(format!("{}/{name}", base.display())))
        .unwrap_or(false);
    if !first {
        return;
    }
    let line = format!("{:x}\t{name}\n", Sha256::digest(bytes));
    // Never blocks (a FIFO in its place fails to open instead of waiting for a
    // reader) and writes only into a regular file this account owns, private
    // to it; anything else there is left alone and nothing is recorded.
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(base.join(ASSET_MANIFEST))
    else {
        return;
    };
    let ours = f.metadata().is_ok_and(|m| {
        use std::os::unix::fs::MetadataExt;
        m.is_file() && m.uid() == unsafe { libc::getuid() } && m.mode() & 0o022 == 0
    });
    if ours {
        // One short write under O_APPEND lands whole, beside other windows'.
        let _ = f.write_all(line.as_bytes());
    }
}

/// The name of [`record_asset`]'s receipt.
pub(crate) const ASSET_MANIFEST: &str = "terminal-delight-assets.manifest";

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

/// The pace flame, materialised. See [`PACE_FLAME`]'s own doc for why it's an
/// SVG, not a PNG like the rest of this file.
pub fn pace_flame_svg() -> PathBuf {
    runtime_asset("terminal-delight-pace-flame.svg", PACE_FLAME)
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
