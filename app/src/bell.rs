//! Per-pane "agent finished" bell: pick a sound, trim a clip with two scrubbers,
//! optionally loop, and play it through `ffplay` (no in-process audio deps — keeps
//! the binary lean and works wherever ffmpeg is installed). Stopping is a hard kill,
//! so the always-visible bell-off / SNOOZE controls are instant.
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const AUDIO_EXTS: &[&str] = &["mp3", "ogg", "oga", "wav", "flac", "m4a", "opus", "aac"];

/// One pane's bell settings. `file = None` falls back to the default alert.
#[derive(Clone, Debug, PartialEq)]
pub struct BellConfig {
    pub file: Option<PathBuf>,
    /// Trim window in seconds (the two scrubbers). `end <= start` ⇒ play to the end.
    pub start: f32,
    pub end: f32,
    pub looping: bool,
    pub volume: f32, // 0.0..=1.5
    /// Master per-pane switch — the always-visible bell toggles this.
    pub enabled: bool,
}
impl Default for BellConfig {
    fn default() -> Self {
        Self {
            file: None,
            start: 0.0,
            end: 0.0,
            looping: false,
            volume: 0.7,
            enabled: true,
        }
    }
}

/// `$XDG_CONFIG_HOME/terminal-delight/sounds` (or `~/.config/...`). User drops
/// their own mp3s here; the seeded defaults live here too.
pub fn sounds_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        });
    base.join("terminal-delight").join("sounds")
}

/// Audio files in the sounds dir, sorted. Creates the dir if missing.
pub fn list_sounds() -> Vec<PathBuf> {
    let dir = sounds_dir();
    let _ = std::fs::create_dir_all(&dir);
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
                .unwrap_or(false)
        })
        .collect();
    v.sort();
    v
}

/// On first run, copy the bundled PD/CC0 default clips into the user sounds dir
/// if it's empty — so the defaults are present without a manual install step.
/// Best-effort: tries `assets/sounds` next to the binary and `$TD_SOUNDS`.
pub fn ensure_seeded() {
    let dir = sounds_dir();
    let _ = std::fs::create_dir_all(&dir);
    if !list_sounds().is_empty() {
        return;
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        for a in exe.ancestors() {
            candidates.push(a.join("assets/sounds"));
        }
    }
    if let Some(env) = std::env::var_os("TD_SOUNDS") {
        candidates.push(PathBuf::from(env));
    }
    for src in candidates {
        if !src.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&src).into_iter().flatten().flatten() {
            let p = entry.path();
            let is_audio = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
                .unwrap_or(false);
            if is_audio {
                if let Some(name) = p.file_name() {
                    let _ = std::fs::copy(&p, dir.join(name));
                }
            }
        }
        if !list_sounds().is_empty() {
            return;
        }
    }
}

pub fn default_alert() -> Option<PathBuf> {
    let p = sounds_dir().join("alert.mp3");
    if p.exists() {
        Some(p)
    } else {
        list_sounds().into_iter().next()
    }
}

pub fn display_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("sound")
        .to_string()
}

/// Clip length in seconds via ffprobe (for the scrubber track). None if unknown.
pub fn duration(path: &Path) -> Option<f32> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// The ffplay arguments (after the program name) for a config + resolved file.
/// Pure so the trim/loop/volume mapping is unit-testable.
pub fn ffplay_args(cfg: &BellConfig, file: &Path) -> Vec<String> {
    let mut a = vec![
        "-nodisp".into(),
        "-autoexit".into(),
        "-loglevel".into(),
        "quiet".into(),
        "-volume".into(),
        ((cfg.volume.clamp(0.0, 1.5) * 100.0).round() as i32).to_string(),
    ];
    if cfg.start > 0.01 {
        a.push("-ss".into());
        a.push(format!("{:.3}", cfg.start));
    }
    if cfg.end > cfg.start + 0.05 {
        a.push("-t".into());
        a.push(format!("{:.3}", cfg.end - cfg.start));
    }
    if cfg.looping {
        a.push("-loop".into());
        a.push("0".into());
    }
    a.push(file.to_string_lossy().into_owned());
    a
}

/// Owns the live ffplay child for one pane. Dropping (or `stop`) hard-kills it.
#[derive(Default)]
pub struct BellPlayer {
    child: Option<Child>,
}
impl BellPlayer {
    pub fn play(&mut self, cfg: &BellConfig) {
        self.stop();
        let Some(file) = cfg.file.clone().or_else(default_alert) else {
            return;
        };
        let mut cmd = Command::new("ffplay");
        cmd.args(ffplay_args(cfg, &file))
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
}
impl Drop for BellPlayer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ffplay_args_map_trim_loop_volume() {
        let f = Path::new("/s/x.mp3");
        // default: no trim, no loop, volume 70, file last
        let a = ffplay_args(&BellConfig::default(), f);
        assert!(a.contains(&"70".to_string()));
        assert!(!a.iter().any(|s| s == "-loop"));
        assert!(!a.iter().any(|s| s == "-ss"));
        assert_eq!(a.last().unwrap(), "/s/x.mp3");
        // trimmed + looping
        let cfg = BellConfig {
            start: 12.0,
            end: 22.0,
            looping: true,
            volume: 1.0,
            ..Default::default()
        };
        let a = ffplay_args(&cfg, f);
        let s = a.join(" ");
        assert!(s.contains("-ss 12.000"));
        assert!(s.contains("-t 10.000")); // end-start
        assert!(s.contains("-loop 0"));
        assert!(s.contains("100")); // volume
    }
}
