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

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".config/terminal-delight/dir-logos.toml")
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

/// Map `dir` (and its subtree) to `logo`. Read-modify-write of the whole file:
/// re-reading first means two windows setting DIFFERENT dirs don't clobber
/// each other; same-dir writes are last-write-wins, which is what a user
/// changing their mind means anyway.
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
}
