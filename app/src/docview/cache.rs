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
//! **Clips a page carries inside itself** sit beside the renders, in
//! `clip-<key>/clip.<ext>` with a `used` of their own, so `sweep` weighs them
//! with everything else (`keep_clips`).
//!
//! No `crate::` paths, so the engine tests can compile this file on its own.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use super::engine::{
    bands, fnv64, Band, DialogRender, Geometry, LayoutHash, PageLayout, PageVideo,
};

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
    // A render whose kept clip was swept is as incomplete as one missing a
    // band: drawing it again keeps the clip again.
    let clips: Vec<PathBuf> = layout
        .videos
        .iter()
        .flatten()
        .filter_map(|v| kept_clip(root, v.src.as_deref()?))
        .collect();
    if clips.iter().any(|c| !c.is_file()) {
        return None;
    }
    touch(&dir);
    for c in &clips {
        if let Some(d) = c.parent() {
            touch(d);
        }
    }
    Some(Cached { layout, tiles })
}

/// A page whose `<video>` or `<audio>` plays a `data:` address carries the
/// film or sound inside itself — a page that must survive being sent as one
/// file does — and that address is no file a clip can be opened from. Each
/// one is kept here as a file, once per address, and its `src` becomes that
/// file's `file://` address, so a press on it opens a clip like any other.
/// Every render of the page, at any width, shares the one copy.
///
/// One that cannot be kept — not base64, not a video or sound, or refused by
/// the disk — names no file, as one a script feeds does, and why is
/// answered for the page's diagnostics.
pub fn keep_clips(root: &Path, videos: &mut [PageVideo]) -> Vec<String> {
    let mut why = Vec::new();
    for v in videos {
        let Some(address) = v.src.as_deref() else {
            continue;
        };
        if !address
            .get(..5)
            .is_some_and(|s| s.eq_ignore_ascii_case("data:"))
        {
            continue;
        }
        match keep_clip(root, address, v.sound) {
            Ok(file) => v.src = Some(file_url(&file)),
            Err(e) => {
                let what = if v.sound { "a sound" } else { "a video" };
                why.push(format!("{what} the page carries was not kept: {e}"));
                v.src = None;
                v.unkept = Some(e.to_string());
            }
        }
    }
    why
}

fn keep_clip(root: &Path, address: &str, sound: bool) -> io::Result<PathBuf> {
    static N: AtomicU64 = AtomicU64::new(0);
    let refuse = |why: String| io::Error::new(io::ErrorKind::InvalidData, why);
    let (head, body) = address[5..]
        .split_once(',')
        .ok_or_else(|| refuse("a data: address with no comma".into()))?;
    let mut params = head.split(';');
    let mime = params.next().unwrap_or("").trim().to_ascii_lowercase();
    if !params.any(|p| p.trim().eq_ignore_ascii_case("base64")) {
        return Err(refuse(format!("{mime:?} is not base64")));
    }
    // The first bytes say what the file is when the address does not.
    let first = super::cdp::base64_decode(body.get(..16).unwrap_or(body)).unwrap_or_default();
    let ext = clip_ext(&mime, &first, sound)
        .ok_or_else(|| refuse(format!("{mime:?} is not a video or sound TD plays")))?;
    let name = format!("clip-{:016x}", fnv64(address.as_bytes()));
    let done = root.join(&name);
    let file = done.join(format!("clip.{ext}"));
    if file.is_file() {
        touch(&done);
        return Ok(file);
    }
    let bytes = super::cdp::base64_decode(body).map_err(refuse)?;
    let partial = root.join(format!(
        "{name}.partial.{}.{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let write = || -> io::Result<()> {
        std::fs::create_dir_all(&partial)?;
        std::fs::write(partial.join(format!("clip.{ext}")), &bytes)?;
        std::fs::write(partial.join("used"), b"")?;
        Ok(())
    };
    let renamed = write().and_then(|()| std::fs::rename(&partial, &done));
    if let Err(e) = renamed {
        let _ = std::fs::remove_dir_all(&partial);
        // Another window kept the same clip first: that one stands.
        if !file.is_file() {
            return Err(e);
        }
    }
    Ok(file)
}

/// The extension TD opens a video or sound by, from the address's media
/// type, else from the file's own first bytes. The extension is what decides
/// whether it opens as a film or as a sound (`docopen::doc_kind`). A media
/// type says which for itself; bytes that could be either (MP4 and M4A, WebM,
/// Ogg) are named for the element that plays them, a `<video>` or an
/// `<audio>`.
fn clip_ext(mime: &str, first: &[u8], sound: bool) -> Option<&'static str> {
    let either = |film: &'static str, song: &'static str| Some(if sound { song } else { film });
    match mime {
        "video/mp4" | "video/x-m4v" => Some("mp4"),
        "video/webm" => Some("webm"),
        "video/ogg" => Some("ogv"),
        "video/quicktime" => Some("mov"),
        "video/x-matroska" => Some("mkv"),
        "video/x-msvideo" | "video/avi" => Some("avi"),
        "audio/mpeg" | "audio/mp3" => Some("mp3"),
        "audio/mp4" | "audio/x-m4a" | "audio/m4a" => Some("m4a"),
        "audio/aac" | "audio/aacp" => Some("aac"),
        "audio/ogg" | "audio/vorbis" => Some("ogg"),
        "audio/opus" => Some("opus"),
        "audio/flac" | "audio/x-flac" => Some("flac"),
        "audio/wav" | "audio/wave" | "audio/x-wav" | "audio/vnd.wave" => Some("wav"),
        "audio/webm" => Some("weba"),
        "" | "application/octet-stream" => match first {
            [_, _, _, _, b'f', b't', b'y', b'p', ..] => either("mp4", "m4a"),
            [0x1a, 0x45, 0xdf, 0xa3, ..] => either("webm", "weba"),
            [b'O', b'g', b'g', b'S', ..] => either("ogv", "ogg"),
            [b'R', b'I', b'F', b'F', _, _, _, _, b'A', b'V', b'I', b' ', ..] => Some("avi"),
            [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'A', b'V', b'E', ..] => Some("wav"),
            [b'I', b'D', b'3', ..] => Some("mp3"),
            [b'f', b'L', b'a', b'C', ..] => Some("flac"),
            _ => None,
        },
        _ => None,
    }
}

/// `path` as a `file://` address `resolve_link` reads back to it: only the
/// two characters it would take for something else are escaped.
fn file_url(path: &Path) -> String {
    let mut out = String::from("file://");
    for c in path.to_string_lossy().chars() {
        match c {
            '%' => out.push_str("%25"),
            '#' => out.push_str("%23"),
            c => out.push(c),
        }
    }
    out
}

/// The kept clip a video's `src` names, when it names one of this cache's.
fn kept_clip(root: &Path, src: &str) -> Option<PathBuf> {
    let rest = src.strip_prefix(&file_url(root))?.strip_prefix('/')?;
    rest.starts_with("clip-").then(|| root.join(rest))
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
            targets: None,
            openers: vec![],
            dialogs: vec!["d-one".into()],
            diagnostics: vec![],
            videos: None,
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

    /// The first bytes of an MP4, as the pane's own check reads a video.
    const FILM: &[u8] = b"\0\0\0\x20ftypisom\0\0\x02\0isomiso2avc1mp41";

    fn data(mime: &str, bytes: &[u8]) -> String {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        format!("data:{mime};base64,{b64}")
    }

    fn video(src: &str) -> PageVideo {
        PageVideo {
            sound: false,
            src: Some(src.into()),
            rect: None,
            unkept: None,
        }
    }

    fn sound(src: &str) -> PageVideo {
        PageVideo {
            sound: true,
            ..video(src)
        }
    }

    /// A sound the page carries is kept as a file named so it opens as a
    /// sound: by its media type when the address gives one, and by its bytes
    /// and the `<audio>` that plays it when it does not, so the same Ogg
    /// bytes a `<video>` plays are kept as a film.
    #[test]
    fn a_sound_the_page_carries_is_kept_under_a_name_that_opens_as_a_sound() {
        let root = scratch("clips-sound");
        let opus = b"OggS\0\x02\0\0\0\0\0\0\0\0\0\0\x9b\xc7\xfa\xbfOpusHead";
        let mut kept = vec![
            sound(&data("audio/ogg", opus)),
            sound(&data("", opus)),
            // Bytes cut shorter, so a different address: one address is
            // kept once, under the name it was first kept by.
            video(&data("", &opus[..18])),
            sound(&data("audio/mpeg", b"ID3\x04\0\0\0\0\0\x23TSSE")),
        ];
        assert!(keep_clips(&root, &mut kept).is_empty());
        let ends = |v: &PageVideo, ext: &str| {
            v.src
                .as_deref()
                .is_some_and(|s| s.ends_with(&format!("/clip.{ext}")))
        };
        assert!(ends(&kept[0], "ogg"), "{:?}", kept[0]);
        assert!(ends(&kept[1], "ogg"), "{:?}", kept[1]);
        assert!(ends(&kept[2], "ogv"), "{:?}", kept[2]);
        assert!(ends(&kept[3], "mp3"), "{:?}", kept[3]);
        let file = kept_clip(&root, kept[0].src.as_deref().unwrap()).unwrap();
        assert_eq!(std::fs::read(file).unwrap(), opus);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_video_the_page_carries_is_kept_once_as_a_file_its_src_then_names() {
        let root = scratch("clips");
        let mut videos = vec![
            video(&data("video/mp4", FILM)),
            video("file:///films/beside.mp4"),
            PageVideo {
                sound: false,
                src: None,
                rect: None,
                unkept: None,
            },
            // No media type: the bytes say MP4.
            video(&data("", FILM)),
        ];
        let why = keep_clips(&root, &mut videos);
        assert!(why.is_empty(), "{why:?}");
        let kept = videos[0].src.clone().expect("names its kept file");
        let file = kept_clip(&root, &kept).expect("one of this cache's clips");
        assert!(
            kept.starts_with("file:///") && kept.ends_with("/clip.mp4"),
            "{kept}"
        );
        assert_eq!(std::fs::read(&file).unwrap(), FILM, "the film itself");
        assert_eq!(videos[1].src.as_deref(), Some("file:///films/beside.mp4"));
        assert_eq!(videos[2].src, None, "a fed video still names nothing");
        assert!(
            videos.iter().all(|v| v.unkept.is_none()),
            "nothing was refused"
        );
        assert!(videos[3]
            .src
            .as_deref()
            .is_some_and(|s| s.ends_with("/clip.mp4")));
        // The same address again, as a wider render of the page asks: the
        // same file, still the one copy.
        let mut again = vec![video(&data("video/mp4", FILM))];
        keep_clips(&root, &mut again);
        assert_eq!(again[0].src.as_deref(), Some(kept.as_str()));
        let clips = std::fs::read_dir(&root).unwrap().count();
        assert_eq!(clips, 2, "one per address, nothing left half-written");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_data_address_that_is_not_a_base64_video_names_no_file_and_says_why() {
        let root = scratch("clips-refused");
        let mut videos = vec![
            // Not marked base64, though every character could be.
            video("data:video/mp4,AAAAIGZ0eXBpc29t"),
            video(&data("text/plain", FILM)),
            video(&data("", b"just some words")),
            video("data:video/mp4;base64,not*base64"),
        ];
        let why = keep_clips(&root, &mut videos);
        assert!(videos.iter().all(|v| v.src.is_none()), "{videos:?}");
        assert!(
            videos.iter().all(|v| v.unkept.is_some()),
            "each says why, for a press on it to say: {videos:?}"
        );
        assert_eq!(why.len(), 4, "{why:?}");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0, "nothing kept");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_render_whose_kept_clip_was_swept_is_not_found() {
        let root = scratch("clip-swept");
        let mut l = layout();
        let mut videos = vec![video(&data("video/mp4", FILM))];
        keep_clips(&root, &mut videos);
        let file = kept_clip(&root, videos[0].src.as_deref().unwrap()).unwrap();
        l.videos = Some(videos);
        let png = b"png".to_vec();
        let tiles: Vec<(Band, &[u8])> = bands(5000)
            .into_iter()
            .map(|b| (b, png.as_slice()))
            .collect();
        store(&root, CacheKey(9), &l, &tiles).unwrap();
        assert!(
            lookup(&root, CacheKey(9)).is_some(),
            "whole while its clip is there"
        );
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
        assert!(
            lookup(&root, CacheKey(9)).is_none(),
            "a render whose clip went is drawn again, which keeps the clip again"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
