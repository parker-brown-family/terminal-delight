//! The decision-brief kit — the skill a Workbench-launched agent is pointed at.
//!
//! When the work an agent finishes is something a person has to read and then
//! decide on, the Workbench asks for a decision brief: one self-contained HTML
//! page, drawn rather than written, that opens in the floating square over the
//! bench, takes a note on any element, and sends the notes back into the
//! agent's prompt with ↪. The briefing ([`crate::surface::launch_briefing`])
//! says so, and names a `SKILL.md` on disk for the how.
//!
//! That file has to exist on every machine Terminal Delight runs on — an
//! AppImage with no checkout beside it included — so the kit rides inside the
//! binary. [`FILES`] embeds every file under `app/skills/decision-brief/`, and
//! [`write`] lays them down under `$XDG_DATA_HOME/terminal-delight/skills/`
//! when the launcher briefs an agent. Written on launch rather than at
//! install, so the kit on disk is always the one this build was tested with.
//!
//! The assets, references and `brief-wall` are the decision-brief skill's own,
//! vendored byte for byte by `scripts/sync-brief-skill` (its `SOURCE` names the
//! commit). `SKILL.md` is Terminal Delight's Workbench edition. Nothing here
//! is told to an agent started by hand in a terminal: only the launcher's
//! briefing names the kit.

use std::path::{Path, PathBuf};

/// One file of the kit: where it goes under the kit's directory, its bytes,
/// and whether it is a program a person or an agent runs.
pub struct KitFile {
    pub path: &'static str,
    pub bytes: &'static [u8],
    pub exec: bool,
}

macro_rules! kit_file {
    ($path:literal) => {
        kit_file!($path, false)
    };
    ($path:literal, $exec:expr) => {
        KitFile {
            path: $path,
            bytes: include_bytes!(concat!("../skills/decision-brief/", $path)),
            exec: $exec,
        }
    };
}

/// Every file of the kit. A file added to `app/skills/decision-brief/`
/// without a line here fails `every_file_in_the_kit_directory_is_embedded`.
pub const FILES: &[KitFile] = &[
    kit_file!("SKILL.md"),
    kit_file!("SOURCE"),
    kit_file!("assets/base.css"),
    kit_file!("assets/notes.css"),
    kit_file!("assets/notes.js"),
    kit_file!("reference/evidence.md"),
    kit_file!("reference/glass.md"),
    kit_file!("reference/layout.md"),
    kit_file!("reference/notes-markup.html"),
    kit_file!("reference/pictures.md"),
    kit_file!("scripts/brief-wall", true),
];

/// Where the kit is written: `$XDG_DATA_HOME/terminal-delight/skills/decision-brief`,
/// else under `$HOME/.local/share`. `None` when neither says where home is.
pub fn default_root() -> Option<PathBuf> {
    root_from(std::env::var_os("XDG_DATA_HOME"), std::env::var_os("HOME"))
}

/// [`default_root`] over the two variables, as values. A relative or empty
/// `XDG_DATA_HOME` is ignored, as the base-directory spec says it must be:
/// it would resolve against whatever directory the window happened to start
/// in.
fn root_from(xdg: Option<std::ffi::OsString>, home: Option<std::ffi::OsString>) -> Option<PathBuf> {
    let data = match xdg.map(PathBuf::from).filter(|p| p.is_absolute()) {
        Some(d) => d,
        None => PathBuf::from(home.filter(|h| !h.is_empty())?).join(".local/share"),
    };
    Some(data.join("terminal-delight/skills/decision-brief"))
}

/// Lay the kit down under `root` and answer where its `SKILL.md` is.
///
/// Only a file whose bytes differ is written, beside and renamed into place,
/// so two windows launching at once never leave half a file for an agent to
/// read, and a launch with nothing new touches nothing. The mode is asserted
/// every time: `brief-wall` is the one program, and a copy that lost its
/// execute bit would fail the agent in a way the briefing never mentioned.
pub fn write(root: &Path) -> std::io::Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt as _;
    for f in FILES {
        let path = root.join(f.path);
        let dir = path.parent().unwrap_or(root);
        if std::fs::read(&path).map_or(true, |b| b != f.bytes) {
            std::fs::create_dir_all(dir)?;
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("kit");
            let temp = dir.join(format!(".{name}.{}.part", std::process::id()));
            std::fs::write(&temp, f.bytes)?;
            std::fs::rename(&temp, &path)?;
        }
        let mode = if f.exec { 0o755 } else { 0o644 };
        if std::fs::metadata(&path)?.permissions().mode() & 0o777 != mode {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))?;
        }
    }
    Ok(root.join("SKILL.md"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("td-briefkit-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn walk(dir: &Path, base: &Path, out: &mut BTreeSet<String>) {
        for entry in std::fs::read_dir(dir).expect("the kit directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                let rel = path.strip_prefix(base).expect("under the kit");
                out.insert(rel.to_string_lossy().into_owned());
            }
        }
    }

    /// The list and the directory are the same set. A file the sync script
    /// brought in and nobody added here would be vendored, reviewed and never
    /// shipped; a line here naming a file that went away would not compile,
    /// so only the first direction needs a test.
    #[test]
    fn every_file_in_the_kit_directory_is_embedded() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/decision-brief");
        let mut on_disk = BTreeSet::new();
        walk(&base, &base, &mut on_disk);
        let embedded: BTreeSet<String> = FILES.iter().map(|f| f.path.to_string()).collect();
        assert_eq!(
            on_disk, embedded,
            "app/skills/decision-brief and FILES disagree"
        );
    }

    /// The Workbench edition names the kit's files by relative path. Every one
    /// it names must be a file the kit carries, or the agent is sent to read
    /// something that is not there.
    #[test]
    fn the_skill_names_only_files_the_kit_carries() {
        let skill = std::str::from_utf8(FILES[0].bytes).expect("utf-8");
        assert_eq!(FILES[0].path, "SKILL.md");
        let carried: BTreeSet<&str> = FILES.iter().map(|f| f.path).collect();
        let pathish = |c: char| c.is_ascii_alphanumeric() || "._-/".contains(c);
        let mut named = BTreeSet::new();
        for dir in ["assets/", "reference/", "scripts/"] {
            for (at, _) in skill.match_indices(dir) {
                if skill[..at].chars().next_back().is_some_and(pathish) {
                    continue; // the tail of a longer path, not one of ours
                }
                let len = skill[at..]
                    .find(|c: char| !pathish(c))
                    .unwrap_or(skill.len() - at);
                let path = skill[at..at + len].trim_end_matches(['.', '/']);
                if path.len() > dir.len() {
                    named.insert(path);
                }
            }
        }
        for path in &named {
            assert!(
                carried.contains(path),
                "SKILL.md names {path}, which the kit does not carry"
            );
        }
        assert!(
            named.len() >= 6,
            "the skill should point at its assets and references; found {named:?}"
        );
    }

    #[test]
    fn writing_the_kit_lays_down_every_file_byte_for_byte() {
        let root = scratch("lay");
        let skill = write(&root).expect("written");
        assert_eq!(skill, root.join("SKILL.md"));
        for f in FILES {
            let path = root.join(f.path);
            assert_eq!(std::fs::read(&path).expect(f.path), f.bytes, "{}", f.path);
            let mode = std::fs::metadata(&path).expect(f.path).permissions().mode() & 0o777;
            assert_eq!(mode, if f.exec { 0o755 } else { 0o644 }, "{}", f.path);
        }
        assert!(
            FILES
                .iter()
                .any(|f| f.exec && f.path == "scripts/brief-wall"),
            "brief-wall is the one program"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A second launch with nothing new rewrites nothing: the same inode, so a
    /// reader holding the file open is never handed a replacement.
    #[test]
    fn a_second_write_touches_nothing() {
        let root = scratch("again");
        write(&root).expect("first");
        let inode = |p: &str| std::fs::metadata(root.join(p)).expect(p).ino();
        let before: Vec<u64> = FILES.iter().map(|f| inode(f.path)).collect();
        write(&root).expect("second");
        let after: Vec<u64> = FILES.iter().map(|f| inode(f.path)).collect();
        assert_eq!(before, after);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A kit someone edited, or one an older build wrote, is put back to this
    /// build's bytes and modes.
    #[test]
    fn a_changed_file_is_put_back() {
        let root = scratch("back");
        write(&root).expect("first");
        let skill = root.join("SKILL.md");
        std::fs::write(&skill, "an older edition").expect("edit");
        let wall = root.join("scripts/brief-wall");
        std::fs::set_permissions(&wall, std::fs::Permissions::from_mode(0o644)).expect("chmod");
        write(&root).expect("second");
        assert_eq!(std::fs::read(&skill).expect("skill"), FILES[0].bytes);
        let mode = std::fs::metadata(&wall).expect("wall").permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_kit_goes_under_the_data_home_and_never_a_relative_one() {
        let os = |s: &str| Some(std::ffi::OsString::from(s));
        assert_eq!(
            root_from(os("/data"), os("/home/me")),
            Some(PathBuf::from(
                "/data/terminal-delight/skills/decision-brief"
            ))
        );
        assert_eq!(
            root_from(os("relative/data"), os("/home/me")),
            Some(PathBuf::from(
                "/home/me/.local/share/terminal-delight/skills/decision-brief"
            )),
            "a relative XDG_DATA_HOME is ignored"
        );
        assert_eq!(
            root_from(os(""), os("/home/me")),
            Some(PathBuf::from(
                "/home/me/.local/share/terminal-delight/skills/decision-brief"
            ))
        );
        assert_eq!(root_from(None, None), None, "nowhere to write says so");
        assert_eq!(root_from(None, os("")), None);
    }
}
