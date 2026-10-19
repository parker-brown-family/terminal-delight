//! Per-directory default logos — `~/.config/terminal-delight/dir-logos.toml`.
//!
//! One flat map of absolute directory → absolute image path. A pane whose live
//! cwd sits AT or UNDER a mapped directory wears that directory's logo; the
//! NEAREST mapped ancestor wins, so mapping a child dir overrides its parent's
//! logo for that subtree. Picking a logo in the picker WRITES the pane's cwd
//! here — persistence across sessions and inheritance by child dirs is the
//! default behaviour, not an option. An explicit per-pane logo (MCP
//! `set_pane_config`, or one saved by an older session) still shadows the map
//! for that pane until it's removed.
//!
//! The file is tiny and re-read on the workspace's 2s sweep, so edits from a
//! second window (or your `$EDITOR`) take effect without a restart — the same
//! hot-file contract as `theme.toml`.
//!
//! A new install starts with ONE mapping, `/` → Terminal Delight's own mark
//! ([`seed_if_absent`]), so its panes wear the app's logo rather than the bare
//! `＋ logo` placeholder. It is an ordinary entry: repoint it, delete it, or ✕ it
//! from the picker like any other.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn config_path() -> PathBuf {
    crate::instance::config_dir().join("dir-logos.toml")
}

/// Where the seeded map's catch-all points: Terminal Delight's mark, written
/// beside the config it serves, the way `sounds/` is. Not the runtime dir that
/// [`crate::art::runtime_asset`] uses — that is emptied at every reboot, and a
/// durable file must not name an image that vanishes with it.
pub fn default_logo_path() -> PathBuf {
    crate::instance::config_dir()
        .join("logos")
        .join("terminal-delight.png")
}

/// What a seeded `dir-logos.toml` says above its one line. The picker rewrites
/// the whole file on its first pick, which drops this; by then the person has
/// found the feature this was here to explain.
const SEED_HEADER: &str = "\
# Per-directory default logos. A pane wears the logo of the nearest mapped
# directory at or above its cwd; picking a logo from a pane's header maps that
# pane's directory. Re-read every 2s, so edits land without a restart.
#
# \"/\" is the catch-all every unmapped directory inherits: Terminal Delight's own
# mark. Point it at another image to change the default, or delete the line to
# show the bare \"+ logo\" placeholder instead.
";

/// First run: map `/` to Terminal Delight's mark, so a new install's panes wear
/// it. Called once at startup, before any workspace loads the map.
///
/// Only when the file is ABSENT. A file that exists — even an empty one — is
/// somebody's answer: the picker's ✕ on the last mapping leaves exactly that
/// empty file behind, and reseeding it would hand back the logo the person just
/// removed. Absent means nobody has ever answered, which is the one case a
/// default is for.
pub fn seed_if_absent() {
    seed_in(&config_path(), &default_logo_path(), crate::art::MARK);
}

/// Testable core of [`seed_if_absent`]. True when it wrote a map.
fn seed_in(map: &Path, logo: &Path, bytes: &[u8]) -> bool {
    if map.exists() {
        return false;
    }
    if let Some(dir) = logo.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // No image, no mapping: a seed naming a file that is not there would be
    // skipped by `resolve_entry` anyway, and would stop a later run reseeding.
    if std::fs::write(logo, bytes).is_err() {
        return false;
    }
    let mut m = HashMap::new();
    m.insert("/".to_string(), logo.to_string_lossy().into_owned());
    let Ok(body) = toml::to_string(&m) else {
        return false;
    };
    if let Some(dir) = map.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    crate::session::write_atomic(map, &format!("{SEED_HEADER}{body}")).is_ok()
}

/// Load the map. A missing or unparsable file is an EMPTY map, never an error —
/// the picker must keep working even if the config was hand-edited badly.
pub fn load() -> HashMap<String, String> {
    load_from(&config_path())
}

fn load_from(path: &Path) -> HashMap<String, String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

/// Map `dir` (and its subtree) to `logo`. Read-modify-write of the whole file.
///
/// Re-reading first **narrows** the window in which two windows setting
/// different directories clobber each other; it does not close it. Both can
/// still load, edit their own copy and write, and the second write wins whole —
/// a check on a snapshot with the write happening afterwards, which is only
/// ever exact in the process that does both. Same-dir writes are
/// last-write-wins, which is what a user changing their mind means anyway.
///
/// Every window on the machine shares this one path, and `write_atomic` is not
/// atomic against another writer (see its own note), so two simultaneous
/// changes can also lose one to a failed rename — swallowed by the `let _ =`
/// below, because a logo that will not save is not worth failing a window over.
/// The consequence either way is a setting that silently does not stick. See
/// #342; nothing here is worth a lock until somebody has actually lost a logo.
pub fn set(dir: &str, logo: &str) {
    mutate(|m| {
        m.insert(norm(dir), logo.to_string());
    });
}

/// Remove `dir`'s mapping (its subtree falls back to the next ancestor's).
pub fn clear(dir: &str) {
    mutate(|m| {
        m.remove(&norm(dir));
    });
}

fn mutate(f: impl FnOnce(&mut HashMap<String, String>)) {
    let path = config_path();
    let mut m = load_from(&path);
    f(&mut m);
    if let Ok(body) = toml::to_string(&m) {
        let _ = crate::session::write_atomic(&path, &body);
    }
}

/// Trailing-slash-insensitive dir key (`/a/b/` ≡ `/a/b`; bare `/` stays `/`).
fn norm(dir: &str) -> String {
    let d = dir.trim_end_matches('/');
    if d.is_empty() {
        "/".into()
    } else {
        d.into()
    }
}

/// The `(mapped dir, logo)` that applies to `cwd`: the LONGEST mapped ancestor
/// on whole-path-component boundaries (`/a/bc` never inherits from `/a/b`).
/// Entries whose image is missing on disk are SKIPPED, not deleted — a logo on
/// an unmounted drive comes back when the drive does.
pub fn resolve_entry<'a>(
    map: &'a HashMap<String, String>,
    cwd: &str,
) -> Option<(&'a str, &'a str)> {
    let cwd = norm(cwd);
    let mut best: Option<(&'a str, &'a str, usize)> = None;
    for (dir, logo) in map {
        let d = norm(dir);
        let applies = cwd == d
            || d == "/"
            || (cwd.starts_with(&d) && cwd.as_bytes().get(d.len()) == Some(&b'/'));
        if applies && Path::new(logo).exists() && best.is_none_or(|(_, _, blen)| blen < d.len()) {
            best = Some((dir.as_str(), logo.as_str(), d.len()));
        }
    }
    best.map(|(d, l, _)| (d, l))
}

/// Just the logo that applies to `cwd`, if any.
pub fn resolve(map: &HashMap<String, String>, cwd: &str) -> Option<String> {
    resolve_entry(map, cwd).map(|(_, l)| l.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A map whose logo paths all exist (the existence filter must not hide
    /// the case under test), keyed by the given dirs. Each call gets its OWN
    /// tmp dir — tests run in parallel and each deletes its dir at the end.
    fn map_with_real_logos(dirs: &[&str]) -> (HashMap<String, String>, PathBuf) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let tmp = std::env::temp_dir().join(format!(
            "td-dirlogo-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let mut m = HashMap::new();
        for (i, d) in dirs.iter().enumerate() {
            let img = tmp.join(format!("logo-{i}.png"));
            std::fs::write(&img, b"x").unwrap();
            m.insert((*d).to_string(), img.to_string_lossy().into_owned());
        }
        (m, tmp)
    }

    #[test]
    fn exact_dir_and_children_inherit_but_prefix_siblings_do_not() {
        let (m, tmp) = map_with_real_logos(&["/a/b"]);
        assert!(resolve(&m, "/a/b").is_some(), "exact dir");
        assert!(resolve(&m, "/a/b/").is_some(), "trailing slash");
        assert!(resolve(&m, "/a/b/deep/child").is_some(), "children inherit");
        assert!(resolve(&m, "/a/bc").is_none(), "/a/bc is NOT under /a/b");
        assert!(resolve(&m, "/a").is_none(), "parents don't inherit down-up");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn nearest_mapped_ancestor_wins() {
        let (m, tmp) = map_with_real_logos(&["/proj", "/proj/sub"]);
        let parent = m.get("/proj").unwrap().clone();
        let child = m.get("/proj/sub").unwrap().clone();
        assert_eq!(
            resolve(&m, "/proj/other"),
            Some(parent),
            "parent covers siblings"
        );
        assert_eq!(
            resolve(&m, "/proj/sub/deeper"),
            Some(child),
            "the child override shadows the parent for its subtree"
        );
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn missing_image_is_skipped_so_the_ancestor_shows_through() {
        let (mut m, tmp) = map_with_real_logos(&["/proj"]);
        let parent = m.get("/proj").unwrap().clone();
        m.insert("/proj/sub".into(), "/nonexistent/gone.png".into());
        assert_eq!(
            resolve(&m, "/proj/sub"),
            Some(parent),
            "a dangling child entry must not black-hole the subtree"
        );
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn root_mapping_is_a_global_fallback() {
        let (m, tmp) = map_with_real_logos(&["/"]);
        assert!(resolve(&m, "/anywhere/at/all").is_some());
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn toml_roundtrip_of_the_map_shape() {
        let mut m = HashMap::new();
        m.insert("/home/x/proj".to_string(), "/home/x/logo.png".to_string());
        let body = toml::to_string(&m).unwrap();
        let back: HashMap<String, String> = toml::from_str(&body).unwrap();
        assert_eq!(back, m);
    }

    /// A fresh config dir of its own, nothing in it — the new-install case.
    fn empty_config_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!(
            "td-dirlogo-seed-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_new_install_maps_every_directory_to_the_mark() {
        let cfg = empty_config_dir();
        let map = cfg.join("dir-logos.toml");
        let logo = cfg.join("logos").join("terminal-delight.png");
        assert!(
            seed_in(&map, &logo, crate::art::MARK),
            "absent map is seeded"
        );
        assert_eq!(
            std::fs::read(&logo).unwrap(),
            crate::art::MARK,
            "the image the seed names is the mark the app ships"
        );
        let m = load_from(&map);
        assert_eq!(m.len(), 1, "one catch-all, nothing else: {m:?}");
        let want = logo.to_string_lossy().into_owned();
        assert_eq!(resolve(&m, "/home/someone/project"), Some(want.clone()));
        assert_eq!(resolve(&m, "/tmp"), Some(want), "outside $HOME too");
        std::fs::remove_dir_all(&cfg).ok();
    }

    #[test]
    fn an_existing_map_is_never_reseeded_even_when_empty() {
        let cfg = empty_config_dir();
        std::fs::create_dir_all(&cfg).unwrap();
        let map = cfg.join("dir-logos.toml");
        let logo = cfg.join("logos").join("terminal-delight.png");
        std::fs::write(&map, "").unwrap();
        assert!(!seed_in(&map, &logo, crate::art::MARK));
        assert_eq!(std::fs::read_to_string(&map).unwrap(), "", "left as found");
        assert!(
            !logo.exists(),
            "no image written for a map that was not seeded"
        );
        std::fs::remove_dir_all(&cfg).ok();
    }

    /// The case the absent/empty distinction exists for: ✕ on the seeded
    /// catch-all empties the map, and the next start must not put it back.
    #[test]
    fn removing_the_seeded_default_survives_a_restart() {
        let cfg = empty_config_dir();
        let map = cfg.join("dir-logos.toml");
        let logo = cfg.join("logos").join("terminal-delight.png");
        assert!(seed_in(&map, &logo, crate::art::MARK));
        let mut m = load_from(&map);
        m.remove("/");
        std::fs::write(&map, toml::to_string(&m).unwrap()).unwrap();
        assert!(!seed_in(&map, &logo, crate::art::MARK), "second start");
        assert!(resolve(&load_from(&map), "/home/someone").is_none());
        std::fs::remove_dir_all(&cfg).ok();
    }
}
