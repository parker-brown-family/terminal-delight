//! The page cache: renders kept on disk, so a brief opened again draws without
//! a browser.
//!
//! `$XDG_CACHE_HOME/terminal-delight/pages/<key>/`, one directory per render:
//! `layout.json` (anchors, links, openers, dialogs), `tiles/<top>-<height>.png`
//! for every band, `dialogs/<id>.json` and `.png` as each dialog is first
//! opened, and `used`, whose mtime is when the render was last read.
//!
//! **Complete or absent.** A render is written into a `.partial` directory
//! and renamed into place only once its last tile has landed, and a lookup
//! that finds any band missing answers `None`. A brief lays out 17–29 CSS px
//! taller from a later load, so tiles from two page instances can disagree at
//! the seam; one entry holds one instance's layout and all of its tiles, and
//! nothing ever mixes two.
//!
//! **Keyed by content.** The key carries a hash of the file's bytes, so any
//! change to the file is a new key and the old entry ages out of `sweep`.
//!
//! No `crate::` paths, so the engine tests can compile this file on its own.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use super::engine::{bands, fnv64, Band, DialogRender, Geometry, LayoutHash, PageLayout};

/// 256 MiB. Page PNGs run 0.4–5.2 MB, so that is roughly 50 to 600 renders.
pub const CAP_BYTES: u64 = 256 << 20;

/// `$XDG_CACHE_HOME` when it is set and absolute, else `~/.cache`.
pub fn root() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| ".".into())).join(".cache")
        });
    base.join("terminal-delight").join("pages")
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CacheKey(pub u64);

impl CacheKey {
    fn dir(&self, root: &Path) -> PathBuf {
        root.join(format!("{:016x}", self.0))
    }
}

/// The full path, not the basename: two copies of one brief may differ, and
/// the page's notes label is derived from where it was opened.
pub fn key(
    path: &Path,
    layout: LayoutHash,
    geometry: Geometry,
    engine: &str,
    extract_version: u32,
) -> CacheKey {
    let text = format!(
        "{}\n{:016x}\n{}x{}@{}\n{engine}\n{extract_version}",
        path.display(),
        layout.0,
        geometry.css_width,
        geometry.viewport_css_height,
        geometry.scale.to_bits(),
    );
    CacheKey(fnv64(text.as_bytes()))
}

/// One render's layout and every tile of it.
pub struct Cached {
    pub layout: PageLayout,
    pub tiles: Vec<(Band, PathBuf)>,
}

fn tile_name(b: Band) -> String {
    format!("{}-{}.png", b.top_dev, b.height_dev)
}

fn dialog_stem(id: &str) -> String {
    format!("{:016x}", fnv64(id.as_bytes()))
}

fn touch(dir: &Path) {
    let used = dir.join("used");
    if let Ok(f) = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&used)
    {
        let _ = f.set_modified(SystemTime::now());
    }
}

pub fn lookup(root: &Path, key: CacheKey) -> Option<Cached> {
    let dir = key.dir(root);
    let text = std::fs::read(dir.join("layout.json")).ok()?;
    let mut layout: PageLayout = serde_json::from_slice(&text).ok()?;
    layout.page = None;
    let mut tiles = Vec::new();
    for b in bands(layout.geometry.height_dev(layout.height_css)) {
        let p = dir.join("tiles").join(tile_name(b));
        if !p.is_file() {
            return None;
        }
        tiles.push((b, p));
    }
    touch(&dir);
    Some(Cached { layout, tiles })
}

/// Write one render whole. Refuses a render that is missing a band: an
/// entry that exists is always complete.
pub fn store(
    root: &Path,
    key: CacheKey,
    layout: &PageLayout,
    tiles: &[(Band, &[u8])],
) -> io::Result<()> {
    static N: AtomicU64 = AtomicU64::new(0);
    for b in bands(layout.geometry.height_dev(layout.height_css)) {
        if !tiles.iter().any(|(t, _)| *t == b) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("band at {} is missing", b.top_dev),
            ));
        }
    }
    let done = key.dir(root);
    if done.join("layout.json").is_file() {
        return Ok(());
    }
    let partial = root.join(format!(
        "{:016x}.partial.{}.{}",
        key.0,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let write = || -> io::Result<()> {
        std::fs::create_dir_all(partial.join("tiles"))?;
        for (b, png) in tiles {
            std::fs::write(partial.join("tiles").join(tile_name(*b)), png)?;
        }
        let mut stored = layout.clone();
        stored.page = None;
        let json = serde_json::to_vec(&stored).map_err(io::Error::other)?;
        std::fs::write(partial.join("layout.json"), json)?;
        std::fs::write(partial.join("used"), b"")?;
        Ok(())
    };
    let wrote = write();
    let renamed = wrote.and_then(|()| std::fs::rename(&partial, &done));
    if renamed.is_err() {
        let _ = std::fs::remove_dir_all(&partial);
        // Another window stored the same render first: that one stands.
        if done.join("layout.json").is_file() {
            return Ok(());
        }
    }
    renamed
}

/// One dialog's render, beside the page it belongs to. Written to a temp
/// name and renamed, the PNG first, so the JSON only ever names a picture
/// that is there.
pub fn store_dialog(root: &Path, key: CacheKey, render: &DialogRender) -> io::Result<()> {
    let dir = key.dir(root).join("dialogs");
    std::fs::create_dir_all(&dir)?;
    let stem = dialog_stem(&render.id);
    let put = |name: String, bytes: &[u8]| -> io::Result<()> {
        let tmp = dir.join(format!("{name}.tmp.{}", std::process::id()));
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, dir.join(name))
    };
    put(format!("{stem}.png"), &render.png)?;
    let json = serde_json::to_vec(render).map_err(io::Error::other)?;
    put(format!("{stem}.json"), &json)
}

pub fn load_dialog(root: &Path, key: CacheKey, id: &str) -> Option<DialogRender> {
    let dir = key.dir(root).join("dialogs");
    let stem = dialog_stem(id);
    let mut render: DialogRender =
        serde_json::from_slice(&std::fs::read(dir.join(format!("{stem}.json"))).ok()?).ok()?;
    if render.id != id {
        return None;
    }
    render.png = std::fs::read(dir.join(format!("{stem}.png"))).ok()?;
    Some(render)
}

fn size_of(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => size_of(&e.path()),
            Ok(_) => e.metadata().map(|m| m.len()).unwrap_or(0),
            Err(_) => 0,
        })
        .sum()
}

/// Take one render out of the cache: its layout no longer describes the
/// file it is keyed by. Absent afterwards, never partial.
pub fn forget(root: &Path, key: CacheKey) -> io::Result<()> {
    match std::fs::remove_dir_all(key.dir(root)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Remove renders, least recently used first, until the rest fit in `cap`
/// bytes; and any `.partial` left by a write that never finished, once it is
/// an hour old. Answers how many bytes went.
pub fn sweep(root: &Path, cap: u64) -> io::Result<u64> {
    let mut entries: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    let mut freed = 0;
    for e in std::fs::read_dir(root)?.flatten() {
        let path = e.path();
        if !path.is_dir() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        if name.contains(".partial.") {
            let old = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age.as_secs() > 3600);
            if old {
                freed += size_of(&path);
                let _ = std::fs::remove_dir_all(&path);
            }
            continue;
        }
        let used = std::fs::metadata(path.join("used"))
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        entries.push((used, size_of(&path), path));
    }
    entries.sort_by_key(|(used, _, _)| *used);
    let mut total: u64 = entries.iter().map(|(_, size, _)| size).sum();
    for (_, size, path) in entries {
        if total <= cap {
            break;
        }
        if std::fs::remove_dir_all(&path).is_ok() {
            total -= size;
            freed += size;
        }
    }
    Ok(freed)
}

#[cfg(test)]
mod tests {
    use super::super::engine::{layout_hash, ConcurSupport, NotesCapability};
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("td-cache-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn geometry() -> Geometry {
        Geometry {
            css_width: 600,
            viewport_css_height: 800,
            scale: 1.6,
        }
    }

    /// A page 5,000 device rows tall: three bands.
    fn layout() -> PageLayout {
        PageLayout {
            page: None,
            generation: 7,
            rendered: LayoutHash(1),
            geometry: geometry(),
            height_css: 3125.0,
            notes_file: None,
            capability: NotesCapability {
                notes_islands: 0,
                concurs_island: false,
                tagged: 0,
                concur: ConcurSupport::Unknown,
            },
            anchors: vec![],
            links: vec![],
            openers: vec![],
            dialogs: vec!["d-one".into()],
            diagnostics: vec![],
        }
    }

    #[test]
    fn a_cache_entry_is_complete_or_absent() {
        let root = scratch("complete");
        let key = CacheKey(42);
        let l = layout();
        let all = bands(5000);
        assert_eq!(all.len(), 3);
        let png = b"png".to_vec();
        // A store missing its last band is refused, and leaves nothing.
        let two: Vec<(Band, &[u8])> = all[..2].iter().map(|b| (*b, png.as_slice())).collect();
        assert!(store(&root, key, &l, &two).is_err());
        assert!(lookup(&root, key).is_none());
        // A write interrupted before its rename is a .partial nobody reads.
        let partial = root.join(format!("{:016x}.partial.1.0", key.0));
        std::fs::create_dir_all(partial.join("tiles")).unwrap();
        std::fs::write(partial.join("layout.json"), serde_json::to_vec(&l).unwrap()).unwrap();
        assert!(lookup(&root, key).is_none());
        // An entry that lost a tile afterwards is not half-used either.
        let three: Vec<(Band, &[u8])> = all.iter().map(|b| (*b, png.as_slice())).collect();
        store(&root, key, &l, &three).unwrap();
        let hit = lookup(&root, key).expect("a complete render is found");
        assert_eq!(hit.tiles.len(), 3);
        assert_eq!(hit.layout.dialogs, vec!["d-one".to_string()]);
        assert_eq!(
            hit.layout.page, None,
            "no live page stands behind a cached layout"
        );
        std::fs::remove_file(&hit.tiles[1].1).unwrap();
        assert!(lookup(&root, key).is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn any_other_change_misses_the_cache() {
        let brief = b"<html><body><p>one</p></body></html>".to_vec();
        let mut edited = brief.clone();
        edited[20] = b'P';
        let path = Path::new("/r/brief.html");
        let a = key(path, layout_hash(&brief), geometry(), "snapshot", 1);
        let b = key(path, layout_hash(&edited), geometry(), "snapshot", 1);
        assert_ne!(a, b, "one byte changed is another render");
        assert_eq!(
            a,
            key(path, layout_hash(&brief.clone()), geometry(), "snapshot", 1)
        );
        let wider = Geometry {
            css_width: 601,
            ..geometry()
        };
        assert_ne!(a, key(path, layout_hash(&brief), wider, "snapshot", 1));
        assert_ne!(a, key(path, layout_hash(&brief), geometry(), "snapshot", 2));
        assert_ne!(
            a,
            key(
                Path::new("/elsewhere/brief.html"),
                layout_hash(&brief),
                geometry(),
                "snapshot",
                1
            )
        );
    }

    #[test]
    fn the_cache_forgets_the_oldest_pages_first() {
        let root = scratch("sweep");
        let l = layout();
        let big = vec![0u8; 1000];
        let tiles: Vec<(Band, &[u8])> = bands(5000)
            .into_iter()
            .map(|b| (b, big.as_slice()))
            .collect();
        let now = SystemTime::now();
        for (k, age) in [(1u64, 300u64), (2, 100), (3, 200)] {
            store(&root, CacheKey(k), &l, &tiles).unwrap();
            let used = std::fs::OpenOptions::new()
                .write(true)
                .open(CacheKey(k).dir(&root).join("used"))
                .unwrap();
            used.set_modified(now - std::time::Duration::from_secs(age))
                .unwrap();
        }
        let one = size_of(&CacheKey(1).dir(&root));
        // Room for two: the one used longest ago goes, and only it.
        let freed = sweep(&root, one * 2).unwrap();
        assert_eq!(freed, one);
        assert!(!CacheKey(1).dir(&root).exists(), "used 300 s ago: oldest");
        assert!(CacheKey(2).dir(&root).exists() && CacheKey(3).dir(&root).exists());
        // Room for one: next the one used 200 s ago.
        sweep(&root, one).unwrap();
        assert!(!CacheKey(3).dir(&root).exists());
        assert!(CacheKey(2).dir(&root).exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_dialog_render_comes_back_whole_and_under_its_own_id() {
        let root = scratch("dialog");
        let key = CacheKey(9);
        let render = DialogRender {
            id: "d-evidence".into(),
            width_dev: 900,
            height_dev: 1200,
            grew_viewport: true,
            anchors: vec![],
            links: vec![],
            openers: vec![],
            closers: vec![],
            png: b"\x89PNG-bytes".to_vec(),
        };
        store_dialog(&root, key, &render).unwrap();
        let back = load_dialog(&root, key, "d-evidence").expect("stored");
        assert_eq!(
            (back.width_dev, back.height_dev, back.grew_viewport),
            (900, 1200, true)
        );
        assert_eq!(back.png, render.png);
        assert!(load_dialog(&root, key, "d-books").is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
