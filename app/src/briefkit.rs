//! The briefing kit — the skill a Terminal Delight agent is pointed at.
//!
//! When the work an agent finishes is something a person should read, check
//! or decide, the briefing ([`crate::surface::launch_briefing`]) asks for a
//! brief: one self-contained HTML page, drawn rather than written, that opens
//! in the floating square over the bench, takes a note on any element, stamps
//! CONCUR on a decision, and sends the notes back into the agent's prompt with
//! ↪. While the work runs, the same skill records the decisions the agent
//! makes that nobody made, as judgement calls the person stamps later, and
//! re-reads what was approved. The briefing names its `SKILL.md` on disk for
//! the how.
//!
//! That file has to exist on every machine Terminal Delight runs on — an
//! AppImage with no checkout beside it included — so the kit rides inside the
//! binary. [`FILES`] embeds every file under `app/skills/`, and [`write`] lays
//! them down under `$XDG_DATA_HOME/terminal-delight/skills/` when TD briefs an
//! agent. Written on launch rather than at install, so the kit on disk is
//! always the one this build was tested with.
//!
//! Briefing builds its pages with two sibling skills and finds them beside its
//! own directory, so the kit is three directories: `briefing/` whole, and only
//! the parts of `decision-brief/` (the look, the notes engine, the wallpaper)
//! and `fancy-pants-brief/` (the section rail, the glass gate) that briefing
//! uses. Neither sibling's `SKILL.md` ships: an agent here follows briefing.
//! All of it is vendored byte for byte from agent-skills by
//! `scripts/sync-brief-skill`, and `app/skills/SOURCE` names the commit. The
//! launcher's briefing names the kit, and so does `terminal-delight briefing`,
//! which a Claude agent started by hand in a pane runs from its `SessionStart`
//! hook once the person has said yes to it ([`crate::briefhook`]).

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
            bytes: include_bytes!(concat!("../skills/", $path)),
            exec: $exec,
        }
    };
}

/// The skill an agent is sent to read, relative to the kit's root.
pub const SKILL: &str = "briefing/SKILL.md";

/// Every file of the kit, the skill an agent reads first. A file added under
/// `app/skills/` without a line here fails
/// `every_file_in_the_kit_directory_is_embedded`.
pub const FILES: &[KitFile] = &[
    kit_file!("briefing/SKILL.md"),
    kit_file!("SOURCE"),
    kit_file!("briefing/LICENSE"),
    kit_file!("briefing/README.md"),
    kit_file!("briefing/assets/feedback-bridge.js"),
    kit_file!("briefing/examples/engineering.feedback.json"),
    kit_file!("briefing/examples/engineering.html"),
    kit_file!("briefing/examples/engineering.md"),
    kit_file!("briefing/examples/engineering.src.html"),
    kit_file!("briefing/examples/evidence/anchors.md"),
    kit_file!("briefing/reference/evidence.md"),
    kit_file!("briefing/reference/feedback.md"),
    kit_file!("briefing/reference/html.md"),
    kit_file!("briefing/reference/judgement.md"),
    kit_file!("briefing/reference/markdown.md"),
    kit_file!("briefing/reference/purposes.md"),
    kit_file!("briefing/reference/refusals.md"),
    kit_file!("briefing/reference/standing-orders.md"),
    kit_file!("briefing/reference/verification.md"),
    kit_file!("briefing/scripts/brief.py"),
    kit_file!("briefing/tests/browser.mjs"),
    kit_file!("decision-brief/LICENSE"),
    kit_file!("decision-brief/assets/base.css"),
    kit_file!("decision-brief/assets/notes.css"),
    kit_file!("decision-brief/assets/notes.js"),
    kit_file!("decision-brief/reference/evidence.md"),
    kit_file!("decision-brief/reference/glass.md"),
    kit_file!("decision-brief/reference/layout.md"),
    kit_file!("decision-brief/reference/notes-markup.html"),
    kit_file!("decision-brief/reference/pictures.md"),
    kit_file!("decision-brief/scripts/brief-wall", true),
    kit_file!("fancy-pants-brief/LICENSE"),
    kit_file!("fancy-pants-brief/assets/sections.js"),
    kit_file!("fancy-pants-brief/scripts/fancy-pants", true),
];

/// What earlier builds wrote and this one does not, removed on the next write
/// so nothing on disk still offers the retired decision-brief instructions.
/// The directory is Terminal Delight's own; nobody else writes here.
const RETIRED: &[&str] = &["decision-brief/SKILL.md", "decision-brief/SOURCE"];

/// Where the kit is written: `$XDG_DATA_HOME/terminal-delight/skills`, else
/// under `$HOME/.local/share`. `None` when neither says where home is.
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
    Some(data.join("terminal-delight/skills"))
}

/// Lay the kit down under `root` and answer where its `SKILL.md` is.
///
/// Only a file whose bytes differ is written, beside and renamed into place,
/// so two windows launching at once never leave half a file for an agent to
/// read, and a launch with nothing new touches nothing. The mode is asserted
/// every time: a program that lost its execute bit would fail the agent in a
/// way the briefing never mentioned.
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
    for gone in RETIRED {
        match std::fs::remove_file(root.join(gone)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(root.join(SKILL))
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

    fn text(path: &str) -> &'static str {
        let f = FILES
            .iter()
            .find(|f| f.path == path)
            .unwrap_or_else(|| panic!("{path} is carried"));
        std::str::from_utf8(f.bytes).expect("utf-8")
    }

    /// Paths beginning with one of `dirs` named in `doc`, trimmed of trailing
    /// punctuation, skipping the tails of longer paths.
    fn named<'a>(doc: &'a str, dirs: &[&str]) -> BTreeSet<&'a str> {
        let pathish = |c: char| c.is_ascii_alphanumeric() || "._-/".contains(c);
        let mut out = BTreeSet::new();
        for dir in dirs {
            for (at, _) in doc.match_indices(dir) {
                if doc[..at].chars().next_back().is_some_and(pathish) {
                    continue;
                }
                let len = doc[at..]
                    .find(|c: char| !pathish(c))
                    .unwrap_or(doc.len() - at);
                let path = doc[at..at + len].trim_end_matches(['.', '/']);
                if path.len() > dir.len() {
                    out.insert(path);
                }
            }
        }
        out
    }

    /// The list and the directory are the same set. A file the sync script
    /// brought in and nobody added here would be vendored, reviewed and never
    /// shipped; a line here naming a file that went away would not compile,
    /// so only the first direction needs a test.
    #[test]
    fn every_file_in_the_kit_directory_is_embedded() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("skills");
        let mut on_disk = BTreeSet::new();
        walk(&base, &base, &mut on_disk);
        let embedded: BTreeSet<String> = FILES.iter().map(|f| f.path.to_string()).collect();
        assert_eq!(on_disk, embedded, "app/skills and FILES disagree");
        assert_eq!(FILES[0].path, SKILL, "the skill an agent reads comes first");
    }

    /// The skill names its own files by paths relative to its directory. Every
    /// one it names must be a file the kit carries, or the agent is sent to
    /// read something that is not there.
    #[test]
    fn the_skill_names_only_files_the_kit_carries() {
        let carried: BTreeSet<&str> = FILES.iter().map(|f| f.path).collect();
        let found = named(
            text(SKILL),
            &["assets/", "reference/", "scripts/", "tests/", "examples/"],
        );
        for path in &found {
            assert!(
                carried.contains(format!("briefing/{path}").as_str()),
                "{SKILL} names {path}, which the kit does not carry"
            );
        }
        assert!(
            found.len() >= 8,
            "the skill should point at its references; found {found:?}"
        );
    }

    /// Briefing's documents reach into the two sibling skills by path (the
    /// look, the notes engine, the wallpaper, the gate). Each of those has to
    /// be in the kit, since the siblings ship only in part.
    #[test]
    fn every_sibling_file_briefing_names_is_carried() {
        let carried: BTreeSet<&str> = FILES.iter().map(|f| f.path).collect();
        let mut found = BTreeSet::new();
        for f in FILES
            .iter()
            .filter(|f| f.path.starts_with("briefing/") && f.path.ends_with(".md"))
        {
            found.extend(named(
                text(f.path),
                &["decision-brief/", "fancy-pants-brief/"],
            ));
        }
        let files: Vec<&&str> = found
            .iter()
            .filter(|p| p.contains('.') || p.contains("scripts/"))
            .collect();
        for path in &files {
            assert!(
                carried.contains(**path),
                "briefing names {path}, which the kit does not carry"
            );
        }
        assert!(
            !files.is_empty(),
            "briefing points into its siblings; found none"
        );
        for retired in RETIRED {
            assert!(
                !carried.contains(retired),
                "{retired} is retired but still carried"
            );
        }
    }

    #[test]
    fn writing_the_kit_lays_down_every_file_byte_for_byte() {
        let root = scratch("lay");
        let skill = write(&root).expect("written");
        assert_eq!(skill, root.join("briefing/SKILL.md"));
        for f in FILES {
            let path = root.join(f.path);
            assert_eq!(std::fs::read(&path).expect(f.path), f.bytes, "{}", f.path);
            let mode = std::fs::metadata(&path).expect(f.path).permissions().mode() & 0o777;
            assert_eq!(mode, if f.exec { 0o755 } else { 0o644 }, "{}", f.path);
        }
        let programs: BTreeSet<&str> = FILES.iter().filter(|f| f.exec).map(|f| f.path).collect();
        assert_eq!(
            programs,
            BTreeSet::from([
                "decision-brief/scripts/brief-wall",
                "fancy-pants-brief/scripts/fancy-pants"
            ]),
            "the programs run by name keep their execute bit"
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
    /// build's bytes and modes, and the retired decision-brief instructions an
    /// older build left are taken away.
    #[test]
    fn a_changed_file_is_put_back_and_a_retired_one_removed() {
        let root = scratch("back");
        std::fs::create_dir_all(root.join("decision-brief")).expect("dir");
        std::fs::write(
            root.join("decision-brief/SKILL.md"),
            "the Workbench edition",
        )
        .expect("old skill");
        std::fs::write(root.join("decision-brief/SOURCE"), "an old source").expect("old source");
        write(&root).expect("first");
        assert!(
            !root.join("decision-brief/SKILL.md").exists(),
            "the retired skill is gone"
        );
        assert!(!root.join("decision-brief/SOURCE").exists());
        let skill = root.join(SKILL);
        std::fs::write(&skill, "an older edition").expect("edit");
        let wall = root.join("decision-brief/scripts/brief-wall");
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
            Some(PathBuf::from("/data/terminal-delight/skills"))
        );
        assert_eq!(
            root_from(os("relative/data"), os("/home/me")),
            Some(PathBuf::from(
                "/home/me/.local/share/terminal-delight/skills"
            )),
            "a relative XDG_DATA_HOME is ignored"
        );
        assert_eq!(
            root_from(os(""), os("/home/me")),
            Some(PathBuf::from(
                "/home/me/.local/share/terminal-delight/skills"
            ))
        );
        assert_eq!(root_from(None, None), None, "nowhere to write says so");
        assert_eq!(root_from(None, os("")), None);
    }
}
