//! `terminal-delight default-terminal [status|on|off]` — make Terminal Delight
//! the terminal `xdg-terminal-exec` opens, and put back exactly what was there
//! before.
//!
//! On Omarchy, every terminal a person or a plugin asks for goes through
//! `xdg-terminal-exec`: Super+Return, the TUI launchers, the Local AI plugin's
//! agents. Which terminal answers is one file,
//! `$XDG_CONFIG_HOME/xdg-terminals.list`, a list of desktop entry ids in order
//! of preference. `omarchy default terminal <name>` overwrites it with a single
//! id. This verb writes the same file, Terminal Delight first and every
//! previous entry after it. Before it writes, it keeps a journal of the bytes it
//! replaced, including "there was no file", so `off` restores the previous
//! default exactly rather than guessing what it was.
//!
//! A journaled, reversible switch is Splinterm's idea (`splinterm integration
//! omarchy enable|disable`), credited here; the code is ours.
//!
//! What it never does: write anything but those two files, change a desktop
//! entry, or touch the list when somebody else has changed it since we wrote
//! it. `off` then leaves the list as that person left it, because the newer
//! choice is theirs.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The id `xdg-terminal-exec` knows Terminal Delight by: the file name
/// `packaging/terminal-delight.desktop` is installed under.
pub const DESKTOP_ID: &str = "terminal-delight.desktop";

/// The four keys `xdg-terminal-exec` reads from a terminal's desktop entry to
/// translate its own flags into the terminal's, with the values Terminal
/// Delight answers to (`parse_contract` in main.rs). An entry without all four
/// is one `xdg-terminal-exec` would hand `-e` to and get the wrong thing back,
/// so `on` refuses until the installed entry carries them.
pub const KEYS: [(&str, &str); 4] = [
    ("X-TerminalArgExec", "-e"),
    ("X-TerminalArgAppId", "--app-id="),
    ("X-TerminalArgTitle", "--title="),
    ("X-TerminalArgDir", "--working-directory="),
];

/// Where the switch, its journal and the desktop entries live.
pub struct Paths {
    /// `$XDG_CONFIG_HOME/xdg-terminals.list`.
    pub list: PathBuf,
    /// What `on` replaced, so `off` can put it back.
    pub journal: PathBuf,
    /// Every `applications/` directory `xdg-terminal-exec` searches, user's first.
    pub entry_dirs: Vec<PathBuf>,
}

impl Paths {
    /// From the XDG environment, with the specification's defaults.
    pub fn from_env() -> Paths {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let dir = |var: &str, fallback: &str| {
            std::env::var_os(var)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(fallback))
        };
        let config = dir("XDG_CONFIG_HOME", ".config");
        let state = dir("XDG_STATE_HOME", ".local/state");
        let data = dir("XDG_DATA_HOME", ".local/share");
        let system = std::env::var("XDG_DATA_DIRS")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        let mut entry_dirs = vec![data.join("applications")];
        entry_dirs.extend(
            system
                .split(':')
                .filter(|d| !d.is_empty())
                .map(|d| Path::new(d).join("applications")),
        );
        Paths {
            list: config.join("xdg-terminals.list"),
            journal: state
                .join("terminal-delight")
                .join("default-terminal.journal"),
            entry_dirs,
        }
    }
}

/// What `on` replaced. `previous: None` means there was no list at all, which
/// is a different thing to put back than an empty one: no file lets
/// `xdg-terminal-exec` fall back to the first terminal it finds.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Journal {
    previous: Option<String>,
    wrote: String,
}

/// The first desktop id a list names, if it names one.
fn first_id(list: &str) -> Option<&str> {
    list.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
}

/// The list `on` writes: Terminal Delight first, then every id the previous
/// list named, in its order, so a machine without Terminal Delight's entry
/// still falls through to the terminal it had.
fn list_with_td_first(previous: Option<&str>) -> String {
    let mut out = String::from(
        "# Terminal emulator preference order for xdg-terminal-exec\n\
         # The first found and valid terminal will be used\n\
         # Written by `terminal-delight default-terminal on`; \
         `terminal-delight default-terminal off` puts back what was here.\n",
    );
    out.push_str(DESKTOP_ID);
    out.push('\n');
    for id in previous
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && *l != DESKTOP_ID)
    {
        out.push_str(id);
        out.push('\n');
    }
    out
}

/// The installed desktop entry, or why `on` cannot use it.
fn installed_entry(p: &Paths) -> Result<PathBuf, String> {
    let Some(path) = p
        .entry_dirs
        .iter()
        .map(|d| d.join(DESKTOP_ID))
        .find(|f| f.is_file())
    else {
        return Err(format!(
            "Terminal Delight's desktop entry is not installed where xdg-terminal-exec \
             looks ({}). Run scripts/install-desktop.sh first.",
            p.entry_dirs
                .first()
                .map(|d| d.join(DESKTOP_ID).display().to_string())
                .unwrap_or_default()
        ));
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let missing: Vec<String> = KEYS
        .iter()
        .filter(|(k, v)| !text.lines().any(|l| l.trim() == format!("{k}={v}")))
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "{} lacks {} — xdg-terminal-exec would not know how to pass a command. \
             Reinstall it from packaging/ with scripts/install-desktop.sh.",
            path.display(),
            missing.join(", ")
        ));
    }
    Ok(path)
}

/// Read a file that may not exist. `Ok(None)` is "no file", which is a fact
/// worth keeping, not an error.
fn read_maybe(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read(path) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| format!("{} is not UTF-8; leaving it alone", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

/// Write by rename, so a reader never sees half a file. The temporary file is
/// created new, under a name nobody can predict, without following a link,
/// and written through the descriptor that created it: whatever already sits
/// in a shared directory under a guessable name is never written through.
fn write_atomically(path: &Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let dir = path
        .parent()
        .ok_or_else(|| format!("{} has no directory", path.display()))?;
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let mut made = None;
    for n in 0..16u32 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or_default();
        let tmp = dir.join(format!(".{name}.tmp-{}-{nanos:09}-{n}", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&tmp)
        {
            Ok(f) => {
                made = Some((tmp, f));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("cannot write {}: {e}", tmp.display())),
        }
    }
    let (tmp, mut f) = made.ok_or_else(|| format!("cannot write beside {}", path.display()))?;
    if let Err(e) = f.write_all(text.as_bytes()) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("cannot write {}: {e}", tmp.display()));
    }
    drop(f);
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("cannot write {}: {e}", path.display())
    })
}

fn read_journal(p: &Paths) -> Result<Option<Journal>, String> {
    match read_maybe(&p.journal)? {
        None => Ok(None),
        Some(text) => serde_json::from_str(&text).map(Some).map_err(|e| {
            format!(
                "{} is not a journal this build can read: {e}",
                p.journal.display()
            )
        }),
    }
}

/// Make Terminal Delight the default. Returns the sentence to print.
pub fn on(p: &Paths) -> Result<String, String> {
    installed_entry(p)?;
    let current = read_maybe(&p.list)?;
    if let Some(j) = read_journal(p)? {
        if current.as_deref() == Some(j.wrote.as_str()) {
            return Ok(format!(
                "Terminal Delight is already the default terminal ({}).",
                p.list.display()
            ));
        }
        // The list changed after we wrote it, so that record describes a
        // switch somebody has already undone. What is there now is what the
        // next `off` should restore.
    }
    let wrote = list_with_td_first(current.as_deref());
    let journal = Journal {
        previous: current.clone(),
        wrote: wrote.clone(),
    };
    let journal_text = serde_json::to_string_pretty(&journal)
        .map_err(|e| format!("cannot record the journal: {e}"))?;
    // The journal first: a list written without it is a switch nobody can undo.
    write_atomically(&p.journal, &journal_text)?;
    if let Err(e) = write_atomically(&p.list, &wrote) {
        let _ = std::fs::remove_file(&p.journal);
        return Err(e);
    }
    let was = current
        .as_deref()
        .and_then(first_id)
        .map(|id| format!("it was {id}"))
        .unwrap_or_else(|| {
            "there was no list, so xdg-terminal-exec was choosing the first terminal it found"
                .into()
        });
    Ok(format!(
        "Terminal Delight is now the default terminal; {was}. \
         `terminal-delight default-terminal off` puts that back."
    ))
}

/// Put back what `on` replaced, exactly, unless somebody has changed the list
/// since. Returns the sentence to print.
pub fn off(p: &Paths) -> Result<String, String> {
    let Some(j) = read_journal(p)? else {
        return Ok(
            "Terminal Delight never switched the default terminal here, so there is \
             nothing to put back."
                .into(),
        );
    };
    let current = read_maybe(&p.list)?;
    if current.as_deref() != Some(j.wrote.as_str()) {
        let _ = std::fs::remove_file(&p.journal);
        let now = current
            .as_deref()
            .and_then(first_id)
            .unwrap_or("no terminal");
        return Ok(format!(
            "{} changed after Terminal Delight wrote it and now names {now}, which is \
             somebody's newer choice, so it is left as it is.",
            p.list.display()
        ));
    }
    match &j.previous {
        Some(text) => write_atomically(&p.list, text)?,
        None => std::fs::remove_file(&p.list)
            .or_else(|e| match e.kind() {
                std::io::ErrorKind::NotFound => Ok(()),
                _ => Err(e),
            })
            .map_err(|e| format!("cannot remove {}: {e}", p.list.display()))?,
    }
    let _ = std::fs::remove_file(&p.journal);
    Ok(match j.previous.as_deref().and_then(first_id) {
        Some(id) => format!("Put back: the default terminal is {id} again."),
        None => "Put back: there is no list again, so xdg-terminal-exec chooses the first \
                 terminal it finds, as it did before."
            .into(),
    })
}

/// What is true now, one fact per line.
pub fn status(p: &Paths) -> String {
    let current = read_maybe(&p.list);
    let default = match &current {
        Ok(Some(text)) => first_id(text)
            .map(str::to_string)
            .unwrap_or_else(|| "none listed".into()),
        Ok(None) => "no list — xdg-terminal-exec picks the first terminal it finds".into(),
        Err(e) => format!("unknown — {e}"),
    };
    let is_td = matches!(&current, Ok(Some(t)) if first_id(t) == Some(DESKTOP_ID));
    let entry = match installed_entry(p) {
        Ok(path) => format!("{} (carries all four keys)", path.display()),
        Err(why) => why,
    };
    let journal = match read_journal(p) {
        Ok(Some(j)) => format!(
            "{} — `off` puts back {}",
            p.journal.display(),
            j.previous
                .as_deref()
                .and_then(first_id)
                .unwrap_or("no list at all")
        ),
        Ok(None) => "none".into(),
        Err(e) => format!("unknown — {e}"),
    };
    format!(
        "default terminal: {default}\n\
         Terminal Delight is {}the default\n\
         desktop entry: {entry}\n\
         journal: {journal}",
        if is_td { "" } else { "not " }
    )
}

/// `terminal-delight default-terminal [status|on|off]`.
pub fn run_cli(args: &[String]) -> i32 {
    let p = Paths::from_env();
    let said = match args.first().map(String::as_str) {
        None | Some("status") => Ok(status(&p)),
        Some("on") => on(&p),
        Some("off") => off(&p),
        Some("-h" | "--help") => {
            Ok("usage: terminal-delight default-terminal [status|on|off]".into())
        }
        Some(other) => Err(format!(
            "unknown word `{other}`; usage: terminal-delight default-terminal [status|on|off]"
        )),
    };
    match said {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(why) => {
            eprintln!("terminal-delight: {why}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsync::Scratch;

    /// A machine of its own: config, state and one applications directory under
    /// a scratch root, with Terminal Delight's packaged entry installed in it.
    fn machine(tag: &str) -> (Scratch, Paths) {
        let root = Scratch::new(tag);
        let apps = root.join("data/applications");
        std::fs::create_dir_all(&apps).unwrap();
        std::fs::write(
            apps.join(DESKTOP_ID),
            include_str!("../../packaging/terminal-delight.desktop"),
        )
        .unwrap();
        let p = Paths {
            list: root.join("config/xdg-terminals.list"),
            journal: root.join("state/terminal-delight/default-terminal.journal"),
            entry_dirs: vec![apps],
        };
        (root, p)
    }

    /// A link planted under the name the temporary file used to have, in a
    /// directory another account can write, is never written through, and the
    /// list ends up a file, not that link.
    #[test]
    fn restoring_never_writes_through_a_planted_temporary_link() {
        let (root, p) = machine("dt-planted");
        std::fs::create_dir_all(p.list.parent().unwrap()).unwrap();
        std::fs::write(&p.list, "foot.desktop\n").unwrap();
        on(&p).unwrap();
        let sentinel = root.join("private.txt");
        std::fs::write(&sentinel, "mine, byte for byte\n").unwrap();
        let old_tmp = p
            .list
            .parent()
            .unwrap()
            .join(format!(".xdg-terminals.list.tmp-{}", std::process::id()));
        std::os::unix::fs::symlink(&sentinel, &old_tmp).unwrap();
        off(&p).unwrap();
        assert_eq!(
            std::fs::read_to_string(&sentinel).unwrap(),
            "mine, byte for byte\n"
        );
        assert!(std::fs::symlink_metadata(&p.list)
            .unwrap()
            .file_type()
            .is_file());
        assert_eq!(std::fs::read_to_string(&p.list).unwrap(), "foot.desktop\n");
    }

    /// With no list there before, `on` writes one naming Terminal Delight and
    /// `off` takes it away again, rather than leaving an empty list that would
    /// stop xdg-terminal-exec finding any terminal.
    ///
    /// Mutation-tested: writing an empty list on `off` instead of removing the
    /// file fails this.
    #[test]
    fn with_no_list_before_off_removes_the_list_again() {
        let (_root, p) = machine("dt-none");
        let said = on(&p).expect("on");
        assert!(said.contains("no list"), "{said}");
        let list = std::fs::read_to_string(&p.list).unwrap();
        assert_eq!(first_id(&list), Some(DESKTOP_ID));
        assert!(p.journal.is_file());
        off(&p).expect("off");
        assert!(!p.list.exists(), "the list is gone again");
        assert!(!p.journal.exists(), "and so is the journal");
    }

    /// `off` puts back the previous list byte for byte — comments, blank lines
    /// and a missing final newline included — and `on` keeps every terminal it
    /// named after Terminal Delight, in its order.
    ///
    /// Mutation-tested: restoring a list rebuilt from the ids instead of the
    /// journal's bytes fails this.
    #[test]
    fn off_puts_back_the_previous_list_byte_for_byte() {
        let (_root, p) = machine("dt-prev");
        let previous = "# mine, hand-edited\nfoot.desktop\n\n  kitty.desktop";
        std::fs::create_dir_all(p.list.parent().unwrap()).unwrap();
        std::fs::write(&p.list, previous).unwrap();
        let said = on(&p).expect("on");
        assert!(said.contains("it was foot.desktop"), "{said}");
        let list = std::fs::read_to_string(&p.list).unwrap();
        let ids: Vec<&str> = list
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect();
        assert_eq!(ids, [DESKTOP_ID, "foot.desktop", "kitty.desktop"]);
        let said = off(&p).expect("off");
        assert!(said.contains("foot.desktop"), "{said}");
        assert_eq!(std::fs::read_to_string(&p.list).unwrap(), previous);
    }

    /// A second `on` changes nothing and, above all, does not record Terminal
    /// Delight's own list as the thing to put back — or `off` would put back
    /// Terminal Delight.
    ///
    /// Mutation-tested: dropping the already-on check fails this.
    #[test]
    fn on_twice_keeps_the_first_record() {
        let (_root, p) = machine("dt-twice");
        std::fs::create_dir_all(p.list.parent().unwrap()).unwrap();
        std::fs::write(&p.list, "foot.desktop\n").unwrap();
        on(&p).expect("on");
        let said = on(&p).expect("on again");
        assert!(said.contains("already"), "{said}");
        off(&p).expect("off");
        assert_eq!(std::fs::read_to_string(&p.list).unwrap(), "foot.desktop\n");
    }

    /// If somebody switches the default after us — `omarchy default terminal
    /// ghostty` overwrites the list — `off` leaves their choice alone and
    /// forgets the stale record, instead of reverting them to ours-before.
    ///
    /// Mutation-tested: restoring regardless of what the list now says fails
    /// this.
    #[test]
    fn off_leaves_a_newer_choice_alone() {
        let (_root, p) = machine("dt-newer");
        std::fs::create_dir_all(p.list.parent().unwrap()).unwrap();
        std::fs::write(&p.list, "foot.desktop\n").unwrap();
        on(&p).expect("on");
        let theirs = "# Terminal emulator preference order for xdg-terminal-exec\ncom.mitchellh.ghostty.desktop\n";
        std::fs::write(&p.list, theirs).unwrap();
        let said = off(&p).expect("off");
        assert!(said.contains("left as it is"), "{said}");
        assert_eq!(std::fs::read_to_string(&p.list).unwrap(), theirs);
        assert!(!p.journal.exists());
    }

    /// No entry where xdg-terminal-exec looks, or one without the four keys,
    /// and `on` writes nothing: a list naming an entry that cannot take `-e`
    /// would break every launcher on the machine.
    ///
    /// Mutation-tested: skipping the key check fails this.
    #[test]
    fn on_refuses_without_an_entry_that_can_take_a_command() {
        let (_root, p) = machine("dt-entry");
        let entry = p.entry_dirs[0].join(DESKTOP_ID);
        std::fs::write(
            &entry,
            "[Desktop Entry]\nName=Terminal Delight\nExec=terminal-delight\n",
        )
        .unwrap();
        let why = on(&p).expect_err("an entry without the keys");
        assert!(why.contains("X-TerminalArgExec=-e"), "{why}");
        assert!(!p.list.exists() && !p.journal.exists(), "nothing written");
        std::fs::remove_file(&entry).unwrap();
        let why = on(&p).expect_err("no entry at all");
        assert!(why.contains("install-desktop.sh"), "{why}");
        assert!(!p.list.exists() && !p.journal.exists(), "nothing written");
    }

    /// `off` with nothing to put back says so and touches nothing.
    #[test]
    fn off_without_a_journal_changes_nothing() {
        let (_root, p) = machine("dt-never");
        std::fs::create_dir_all(p.list.parent().unwrap()).unwrap();
        std::fs::write(&p.list, "foot.desktop\n").unwrap();
        let said = off(&p).expect("off");
        assert!(said.contains("nothing to put back"), "{said}");
        assert_eq!(std::fs::read_to_string(&p.list).unwrap(), "foot.desktop\n");
    }

    /// The status reads what is true, one fact per line, and says unknown
    /// rather than guessing when it cannot read something.
    #[test]
    fn status_says_who_the_default_is() {
        let (_root, p) = machine("dt-status");
        let before = status(&p);
        assert!(before.contains("no list"), "{before}");
        assert!(before.contains("is not the default"), "{before}");
        assert!(before.contains("carries all four keys"), "{before}");
        on(&p).expect("on");
        let after = status(&p);
        assert!(
            after.contains(&format!("default terminal: {DESKTOP_ID}")),
            "{after}"
        );
        assert!(after.contains("Terminal Delight is the default"), "{after}");
        assert!(after.contains("`off` puts back no list at all"), "{after}");
    }
}
