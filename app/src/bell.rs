//! The agent-finished ping: fixed sound, zero configuration.
//!
//! v1 shipped a per-pane config tray — sound picker, dual-pip trim scrubber,
//! loop, volume, import dialog — and the overhead buried the feature until it
//! was killed outright. The point was never the configuration; it was knowing
//! when an agent finishes. So v2 is the opposite shape: the alert clip is
//! EMBEDDED in the binary (no sounds dir, no seeding, no picker), plays once at
//! a fixed volume through the first audio player found (`ffplay`, then
//! `pw-play`), and nothing about it is configurable. The visible surfaces are
//! the tab 🔔 badge, the header "● done" status, and the system notification —
//! all driven by the pane's `bell` flag, acknowledged by focusing the pane.
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// The bundled ping (2KB mp3, CC0). Compiled in so a deployed bare binary —
/// there is no assets dir next to `~/.local/lib/terminal-delight/td-<sha>` —
/// still rings without any install step.
const PING: &[u8] = include_bytes!("../assets/sounds/alert.mp3");

/// Where the embedded ping is materialised for the player. Runtime dir (tmpfs,
/// per-user) when available, else the tmp dir. Written once per boot.
pub fn ping_path() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("terminal-delight-ping.mp3")
}

/// Ensure the ping file exists on disk (idempotent; size-checked so a truncated
/// write from a crashed run heals). Returns the playable path.
fn ensure_ping() -> PathBuf {
    let p = ping_path();
    let ok = std::fs::metadata(&p).map(|m| m.len() == PING.len() as u64);
    if !matches!(ok, Ok(true)) {
        let _ = std::fs::write(&p, PING);
    }
    p
}

/// The player command for the ping: program + args, first available wins.
/// Pure over the probe result so the mapping is unit-testable.
pub fn player_cmd(have_ffplay: bool, file: &Path) -> Option<(&'static str, Vec<String>)> {
    let f = file.to_string_lossy().into_owned();
    if have_ffplay {
        // -volume 70 ≈ the fixed volume the old default used
        Some((
            "ffplay",
            vec![
                "-nodisp".into(),
                "-autoexit".into(),
                "-loglevel".into(),
                "quiet".into(),
                "-volume".into(),
                "70".into(),
                f,
            ],
        ))
    } else {
        Some(("pw-play", vec![f]))
    }
}

fn have(prog: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(prog).is_file()))
        .unwrap_or(false)
}

/// Owns the live player child for one pane. Dropping (or `stop`) hard-kills it.
#[derive(Default)]
pub struct BellPlayer {
    child: Option<Child>,
}
impl BellPlayer {
    /// Play the ping once. A ring that arrives while the last one is still
    /// sounding restarts it (stop-then-spawn), so overlapping agents don't
    /// stack audio.
    pub fn play(&mut self) {
        self.stop();
        let file = ensure_ping();
        let Some((prog, args)) = player_cmd(have("ffplay"), &file) else {
            return;
        };
        let mut cmd = Command::new(prog);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // own process group so it ignores our terminal signals; we keep the Child to kill
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        self.child = cmd.spawn().ok();
    }
    pub fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
    /// Reap a clip that finished on its own (`-autoexit`), so a long session
    /// doesn't accumulate `<defunct>` children. Cheap; called from the per-pane
    /// tick.
    pub fn reap(&mut self) {
        if let Some(c) = self.child.as_mut() {
            if matches!(c.try_wait(), Ok(Some(_))) {
                self.child = None;
            }
        }
    }
}
impl Drop for BellPlayer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ping ships inside the binary: non-trivial, and actually an MPEG
    /// stream (ID3 tag or a raw frame-sync header), so a bad asset path or a
    /// truncated commit fails HERE and not silently at ring time.
    #[test]
    fn the_embedded_ping_is_a_real_clip() {
        assert!(PING.len() > 500, "embedded ping suspiciously small");
        let id3 = PING.starts_with(b"ID3");
        let sync = PING.len() >= 2 && PING[0] == 0xFF && (PING[1] & 0xE0) == 0xE0;
        assert!(id3 || sync, "embedded ping is not an mp3 stream");
    }

    #[test]
    fn player_mapping_prefers_ffplay_and_falls_back() {
        let f = Path::new("/run/x.mp3");
        let (prog, args) = player_cmd(true, f).unwrap();
        assert_eq!(prog, "ffplay");
        assert!(args.contains(&"-autoexit".to_string()));
        assert!(!args.iter().any(|a| a == "-loop"), "the ping never loops");
        assert_eq!(args.last().unwrap(), "/run/x.mp3");
        let (prog, args) = player_cmd(false, f).unwrap();
        assert_eq!(prog, "pw-play");
        assert_eq!(args, vec!["/run/x.mp3".to_string()]);
    }
}
