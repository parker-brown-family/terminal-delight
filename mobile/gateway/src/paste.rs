//! A picture from the phone, saved where the desk saves its own.
//!
//! A pseudoterminal carries bytes, so a picture cannot be typed into one. What
//! can be typed is its path, and Claude Code turns a pasted picture path into
//! `[Image #n]`. The desk's bench saves a pasted picture the same way
//! (`write_paste` in `app/src/pane/bench.rs`): in the pane's own `pastes/`
//! directory, named `pasted-<n>.<ext>` and counted up, with the content hash in
//! a `.<hash>` sidecar so the same picture pasted twice is one file. This
//! restates it, so a phone paste and a desk paste land side by side and read
//! the same.

use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The extension a picture of this type is saved under, for the types the
/// phone may send. Only what Claude Code reads from a pasted path (`.png`,
/// `.jpg`, `.gif`, `.webp`): a picture saved under any other name would arrive
/// as a path the agent does not look at.
pub fn ext_of(mime: &str) -> Option<&'static str> {
    Some(
        match mime.split(';').next()?.trim().to_ascii_lowercase().as_str() {
            "image/png" => "png",
            "image/jpeg" | "image/jpg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            _ => return None,
        },
    )
}

/// Save `bytes` in `dir` and answer where. The same picture twice answers the
/// file the first one made.
pub fn save(dir: &Path, bytes: &[u8], ext: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    let stamp = dir.join(format!(".{:016x}", h.finish()));
    if let Ok(known) = std::fs::read_to_string(&stamp) {
        let path = dir.join(known.trim());
        if path.exists() {
            return Ok(path);
        }
    }
    for n in 1.. {
        let name = format!("pasted-{n}.{ext}");
        let path = dir.join(&name);
        // `create_new`, so two pastes arriving together take two numbers
        // rather than one overwriting the other.
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                f.write_all(bytes)?;
                let _ = std::fs::write(&stamp, &name);
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    unreachable!("an unbounded count always finds a free name")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_picture_claude_reads_from_a_path_is_taken() {
        assert_eq!(ext_of("image/png"), Some("png"));
        assert_eq!(ext_of("IMAGE/JPEG; charset=binary"), Some("jpg"));
        assert_eq!(ext_of("image/webp"), Some("webp"));
        assert_eq!(ext_of("image/heic"), None);
        assert_eq!(ext_of("image/svg+xml"), None);
        assert_eq!(ext_of("text/plain"), None);
        assert_eq!(ext_of(""), None);
    }

    #[test]
    fn pastes_count_up_and_the_same_picture_is_one_file() {
        let dir = std::env::temp_dir().join(format!("td-paste-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = save(&dir, b"first", "png").unwrap();
        let b = save(&dir, b"second", "png").unwrap();
        let again = save(&dir, b"first", "png").unwrap();
        assert_eq!(a.file_name().unwrap(), "pasted-1.png");
        assert_eq!(b.file_name().unwrap(), "pasted-2.png");
        assert_eq!(again, a);
        assert_eq!(std::fs::read(&b).unwrap(), b"second");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
