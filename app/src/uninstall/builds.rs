//! Old builds: the `td-*` executables in `~/.local/lib/terminal-delight`.
//! `scripts/land.sh` installs one per landing as `td-<sha>-<label>` and points
//! the launcher at it; nothing removes them afterwards, so they pile up.
//!
//! A build is in use when the launcher points at it or a process of this user
//! runs it. A running build is kept even though Linux lets a program outlive its
//! file, because TD starts its own executable again by path (session hosts,
//! helpers) and that path has to still be there. Every removal re-checks all of
//! it immediately before the unlink, since a landing or a new window can arrive
//! between the listing and the removal. Nothing in the directory that is not a
//! `td-*` ELF regular file is touched, and no symlink is followed.
use super::roots::Roots;
use serde::Serialize;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// How many of the newest builds prune keeps besides the ones in use.
pub(crate) const DEFAULT_KEEP: usize = 3;

const NOT_TD: &str = "not td-*";
const NOT_FILE: &str = "not a regular file";
const NOT_ELF: &str = "not an executable (ELF)";
const APPIMAGE: &str = "an AppImage, kept";
const UNREADABLE: &str = "could not be read";
const GONE: &str = "gone";

#[derive(Debug, Serialize, Clone)]
pub(crate) struct Build {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) bytes: u64,
    /// Seconds since the epoch; absent when the filesystem gave no time.
    pub(crate) modified_unix: Option<i64>,
    /// The launcher, `~/.local/bin/terminal-delight`, points at this build.
    pub(crate) launcher: bool,
    /// Processes of this user running this build.
    pub(crate) running: Vec<u32>,
    /// 0 = newest by modification time; equal times are ordered by name.
    pub(crate) newest_rank: usize,
}

/// An entry in the build directory that is not a build. Never touched.
#[derive(Debug, Serialize)]
pub(crate) struct Other {
    pub(crate) name: String,
    pub(crate) why: &'static str,
}

#[derive(Debug, Serialize)]
pub(crate) struct Listing {
    pub(crate) dir: PathBuf,
    pub(crate) present: bool,
    /// Newest first.
    pub(crate) builds: Vec<Build>,
    pub(crate) other: Vec<Other>,
    pub(crate) launcher_target: Option<PathBuf>,
    /// Processes of this user whose executable could not be read, so whether
    /// they run a build is unknown. Counted from `/proc` directly, because the
    /// process source hands over only the executables it could read.
    pub(crate) unreadable_procs: u32,
    /// Processes whose executable could not be read but whose name says TD
    /// (the process source hands them over with an empty path). While any is
    /// there, whether a build is in use is unknown, and nothing is pruned.
    pub(crate) uncertain: Vec<u32>,
    /// Why the directory could not be read in full; the listing is then partial.
    pub(crate) error: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct Pruned {
    pub(crate) removed: Vec<(String, u64)>,
    pub(crate) kept: Vec<(String, &'static str)>,
    pub(crate) failed: Vec<(String, String)>,
    pub(crate) freed: u64,
}

pub(crate) fn list(roots: &Roots, procs: &dyn Fn() -> Vec<(u32, PathBuf)>) -> Listing {
    let dir = roots.builds.clone();
    let mut listing = Listing {
        dir: dir.clone(),
        present: fs::metadata(&dir).is_ok_and(|m| m.is_dir()),
        builds: Vec::new(),
        other: Vec::new(),
        launcher_target: read_launcher(&roots.bin).ok().flatten(),
        unreadable_procs: scan_procs().1,
        uncertain: Vec::new(),
        error: None,
    };
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return listing,
        Err(e) => {
            listing.error = Some(format!("{}: {e}", dir.display()));
            return listing;
        }
    };
    let canon_dir = fs::canonicalize(&dir).ok();
    let running_now = procs();
    listing.uncertain = running_now
        .iter()
        .filter(|(_, exe)| exe.as_os_str().is_empty())
        .map(|(pid, _)| *pid)
        .collect();
    let mut found: Vec<(Build, Option<SystemTime>)> = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                listing
                    .error
                    .get_or_insert_with(|| format!("{}: {e}", dir.display()));
                continue;
            }
        };
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        match judge(&path) {
            Ok(meta) => {
                let canon = canon_dir.as_ref().map(|d| d.join(entry.file_name()));
                let launcher = listing
                    .launcher_target
                    .as_deref()
                    .is_some_and(|t| same_file(t, &path, canon.as_deref()));
                let running = pids_running(&running_now, &path, canon.as_deref());
                let mtime = meta.modified().ok();
                let build = Build {
                    name,
                    path,
                    bytes: meta.len(),
                    modified_unix: mtime.map(unix_secs),
                    launcher,
                    running,
                    newest_rank: 0,
                };
                found.push((build, mtime));
            }
            // Removed between the directory read and the look; nothing to say.
            Err(GONE) => {}
            Err(why) => listing.other.push(Other { name, why }),
        }
    }
    // A build with no time sorts after every dated one: it cannot be shown newer.
    found.sort_by(|(a, at), (b, bt)| bt.cmp(at).then_with(|| a.name.cmp(&b.name)));
    listing.builds = found
        .into_iter()
        .enumerate()
        .map(|(rank, (mut b, _))| {
            b.newest_rank = rank;
            b
        })
        .collect();
    listing.other.sort_by(|a, b| a.name.cmp(&b.name));
    listing
}

/// `(pid, exe)` for every process of this user whose executable link could be
/// read, with a trailing ` (deleted)` removed.
pub(crate) fn live_procs() -> Vec<(u32, PathBuf)> {
    scan_procs().0
}

pub(crate) fn prunable(listing: &Listing, keep_newest: usize) -> Vec<&Build> {
    let keep = keep_newest.max(1);
    listing
        .builds
        .iter()
        .filter(|b| keep_reason(b, keep).is_none())
        .collect()
}

pub(crate) fn prune(
    roots: &Roots,
    keep_newest: usize,
    dry_run: bool,
    procs: &dyn Fn() -> Vec<(u32, PathBuf)>,
) -> Pruned {
    let keep = keep_newest.max(1);
    let listing = list(roots, procs);
    let canon_dir = fs::canonicalize(&listing.dir).ok();
    let mut out = Pruned::default();
    // Unknown is not "not running". A process that may be TD and cannot be
    // read, or a builds directory that is a link to somewhere else, stops the
    // whole prune rather than guessing.
    if !listing.uncertain.is_empty() {
        out.failed.push((
            "builds".into(),
            format!(
                "a process that may be Terminal Delight cannot be inspected (pid {:?}), so nothing was removed",
                listing.uncertain
            ),
        ));
        return out;
    }
    // On a home that is not the account's, the builds directory has to
    // resolve inside it: a fixture never prunes the real builds.
    if !super::guard::is_account_home(&roots.home) {
        let home = fs::canonicalize(&roots.home).unwrap_or_else(|_| roots.home.clone());
        if !super::roots::resolve_existing_prefix(&listing.dir).starts_with(&home) {
            out.failed.push((
                "builds".into(),
                format!(
                    "{} resolves outside this home, so nothing was removed",
                    listing.dir.display()
                ),
            ));
            return out;
        }
    }
    if fs::symlink_metadata(&listing.dir).is_ok_and(|m| m.file_type().is_symlink()) {
        out.failed.push((
            "builds".into(),
            format!(
                "{} is a link, so nothing was removed through it",
                listing.dir.display()
            ),
        ));
        return out;
    }
    for b in &listing.builds {
        if let Some(why) = keep_reason(b, keep) {
            out.kept.push((b.name.clone(), why));
            continue;
        }
        let canon = match (&canon_dir, b.path.file_name()) {
            (Some(d), Some(n)) => Some(d.join(n)),
            _ => None,
        };
        if let Err(why) = recheck(b, &roots.bin, canon.as_deref(), procs) {
            out.kept.push((b.name.clone(), why));
            continue;
        }
        if !dry_run {
            if let Err(e) = fs::remove_file(&b.path) {
                out.failed.push((b.name.clone(), e.to_string()));
                continue;
            }
        }
        out.removed.push((b.name.clone(), b.bytes));
        out.freed += b.bytes;
    }
    out
}

pub(crate) fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["KB", "MB", "GB", "TB", "PB", "EB"];
    if n < 1000 {
        return format!("{n} B");
    }
    let mut v = n as f64 / 1000.0;
    let mut unit = 0;
    // Step up before rounding would print "1000 KB".
    while v >= 999.5 && unit + 1 < UNITS.len() {
        v /= 1000.0;
        unit += 1;
    }
    if v < 9.95 {
        format!("{v:.1} {}", UNITS[unit])
    } else {
        format!("{v:.0} {}", UNITS[unit])
    }
}

fn keep_reason(b: &Build, keep: usize) -> Option<&'static str> {
    if b.launcher {
        Some("the launcher's build")
    } else if !b.running.is_empty() {
        Some("running")
    } else if b.newest_rank < keep {
        Some("among the newest")
    } else if b.modified_unix.is_none() {
        Some("age unknown")
    } else {
        None
    }
}

/// Everything `list` decided, asked again just before the unlink.
fn recheck(
    b: &Build,
    bin: &Path,
    canon: Option<&Path>,
    procs: &dyn Fn() -> Vec<(u32, PathBuf)>,
) -> Result<(), &'static str> {
    let meta = match judge(&b.path) {
        Ok(meta) => meta,
        Err(GONE) => return Err("no longer there"),
        Err(UNREADABLE) => return Err(UNREADABLE),
        Err(_) => return Err("no longer a td-* ELF file"),
    };
    if meta.len() != b.bytes || meta.modified().ok().map(unix_secs) != b.modified_unix {
        return Err("changed since it was listed");
    }
    match read_launcher(bin) {
        Err(_) => return Err("launcher unreadable"),
        Ok(Some(target)) if same_file(&target, &b.path, canon) => {
            return Err("now the launcher's build")
        }
        Ok(_) => {}
    }
    let now = procs();
    if now.iter().any(|(_, exe)| exe.as_os_str().is_empty()) {
        return Err("a process that may be TD cannot be inspected");
    }
    if !pids_running(&now, &b.path, canon).is_empty() {
        return Err("started running");
    }
    Ok(())
}

/// The metadata of a build, or why the entry is not one.
fn judge(path: &Path) -> Result<fs::Metadata, &'static str> {
    if !path
        .file_name()
        .is_some_and(|n| n.as_bytes().starts_with(b"td-"))
    {
        return Err(NOT_TD);
    }
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(GONE),
        Err(_) => return Err(UNREADABLE),
    };
    if !meta.file_type().is_file() {
        return Err(NOT_FILE);
    }
    let mut magic = Vec::with_capacity(11);
    // O_NOFOLLOW and O_NONBLOCK: a symlink or a FIFO swapped in after the
    // look above is refused rather than followed or waited on.
    let read = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .and_then(|f| f.take(11).read_to_end(&mut magic));
    match read {
        // An AppImage is a container the person keeps, not a build: it is
        // never pruned or removed, whatever its name.
        Ok(_) if magic.starts_with(b"\x7fELF") && magic.get(8..11) == Some(b"AI\x02") => {
            Err(APPIMAGE)
        }
        Ok(_) if magic.starts_with(b"\x7fELF") => Ok(meta),
        Ok(_) => Err(NOT_ELF),
        Err(_) => Err(UNREADABLE),
    }
}

/// Where the launcher points: `None` when there is no launcher or it is not a
/// symlink, an error when that cannot be told.
fn read_launcher(bin: &Path) -> Result<Option<PathBuf>, String> {
    match fs::read_link(bin.join("terminal-delight")) {
        Ok(t) if t.is_relative() => Ok(Some(bin.join(t))),
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) if e.raw_os_error() == Some(libc::EINVAL) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Whether `target` names the build at `path`, lexically or once resolved.
fn same_file(target: &Path, path: &Path, canon: Option<&Path>) -> bool {
    target == path || canon.is_some_and(|c| fs::canonicalize(target).is_ok_and(|t| t == c))
}

fn pids_running(procs: &[(u32, PathBuf)], path: &Path, canon: Option<&Path>) -> Vec<u32> {
    let mut pids: Vec<u32> = procs
        .iter()
        .filter(|(_, exe)| {
            let exe = strip_deleted(exe);
            exe == path || Some(exe.as_path()) == canon
        })
        .map(|(pid, _)| *pid)
        .collect();
    pids.sort_unstable();
    pids.dedup();
    pids
}

fn strip_deleted(exe: &Path) -> PathBuf {
    let b = exe.as_os_str().as_bytes();
    PathBuf::from(OsStr::from_bytes(
        b.strip_suffix(b" (deleted)").unwrap_or(b),
    ))
}

/// Readable executables of this user's processes, and how many of this user's
/// processes would not say.
fn scan_procs() -> (Vec<(u32, PathBuf)>, u32) {
    let mut found = Vec::new();
    let mut unreadable = 0;
    let Ok(uid) = fs::metadata("/proc/self").map(|m| m.uid()) else {
        return (found, unreadable);
    };
    let Ok(entries) = fs::read_dir("/proc") else {
        return (found, unreadable);
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        let dir = entry.path();
        if !owned_by(&dir, uid) {
            continue;
        }
        match fs::read_link(dir.join("exe")) {
            Ok(exe) => found.push((pid, strip_deleted(&exe))),
            // Exited since the directory was read, or a zombie, which runs nothing.
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(_) => {
                unreadable += 1;
                // Its name is still readable. One that says TD is handed over
                // with an empty path, which marks it unknown.
                let comm = fs::read_to_string(dir.join("comm")).unwrap_or_default();
                let argv0 = fs::read(dir.join("cmdline"))
                    .ok()
                    .and_then(|b| {
                        b.split(|c| *c == 0)
                            .next()
                            .map(|a| String::from_utf8_lossy(a).into_owned())
                    })
                    .unwrap_or_default();
                let base = |s: &str| s.trim().rsplit('/').next().unwrap_or("").to_string();
                let td = |n: String| n == "terminal-delight" || n.starts_with("td-");
                if td(base(&comm)) || td(base(&argv0)) {
                    found.push((pid, PathBuf::new()));
                }
            }
        }
    }
    (found, unreadable)
}

pub(crate) fn owned_by(proc_dir: &Path, uid: u32) -> bool {
    match fs::metadata(proc_dir) {
        Ok(m) if m.uid() == uid => true,
        // A process that made itself undumpable shows as root's; its status
        // still names the user it runs as.
        Ok(m) if m.uid() == 0 => fs::read_to_string(proc_dir.join("status"))
            .ok()
            .and_then(|s| {
                s.lines().find_map(|l| l.strip_prefix("Uid:")).map(|ids| {
                    ids.split_whitespace()
                        .take(2)
                        .any(|id| id.parse() == Ok(uid))
                })
            })
            .unwrap_or(false),
        _ => false,
    }
}

fn unix_secs(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
        Err(e) => i64::try_from(e.duration().as_secs()).map_or(i64::MIN, |s| -s),
    }
}

const HELP: &str = "Usage: terminal-delight admin builds [--json]
       terminal-delight admin builds prune [--keep N] [--dry-run] [--json]

Lists the builds in ~/.local/lib/terminal-delight and removes old ones.
Prune keeps the launcher's build, every running build and the newest N
(default 3, at least 1), checks each again just before removing it, and never
touches anything in that directory that is not a td-* executable.";

#[derive(Debug, PartialEq)]
enum Command {
    Help,
    List {
        json: bool,
    },
    Prune {
        keep: usize,
        dry_run: bool,
        json: bool,
    },
}

/// `args` are the words after `admin builds`; a leading `builds` is tolerated
/// so the caller may pass either slice.
fn parse(args: &[String]) -> Result<Command, String> {
    let args = match args.first().map(String::as_str) {
        Some("builds") => &args[1..],
        _ => args,
    };
    let (prune, rest) = match args.first().map(String::as_str) {
        Some("help" | "--help" | "-h") if args.len() == 1 => return Ok(Command::Help),
        Some("prune") => (true, &args[1..]),
        _ => (false, args),
    };
    let (mut json, mut dry_run, mut keep) = (false, false, None);
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--json" if !json => json = true,
            "--dry-run" if prune && !dry_run => dry_run = true,
            "--keep" if prune && keep.is_none() => {
                let n = it.next().and_then(|v| v.parse::<usize>().ok());
                keep = Some(n.ok_or("--keep needs a whole number")?);
            }
            _ => return Err(format!("unknown or repeated option: {arg}")),
        }
    }
    Ok(if prune {
        Command::Prune {
            keep: keep.unwrap_or(DEFAULT_KEEP).max(1),
            dry_run,
            json,
        }
    } else {
        Command::List { json }
    })
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

fn render_list(l: &Listing, keep: usize) -> String {
    let mut out = Vec::new();
    if let Some(e) = &l.error {
        out.push(format!("Could not read all of the build directory: {e}"));
    }
    if !l.present {
        out.push(format!("No builds directory at {}.", l.dir.display()));
        return out.join("\n");
    }
    out.push(format!("Builds in {}, newest first:", l.dir.display()));
    let width = l.builds.iter().map(|b| b.name.len()).max().unwrap_or(0);
    for b in &l.builds {
        let mut uses = Vec::new();
        if b.launcher {
            uses.push("launcher".to_string());
        }
        if !b.running.is_empty() {
            let pids: Vec<String> = b.running.iter().map(u32::to_string).collect();
            uses.push(format!("running pid {}", pids.join(", ")));
        }
        let line = format!(
            "  {:width$}  {:>6}  {}",
            b.name,
            human_bytes(b.bytes),
            uses.join(", ")
        );
        out.push(line.trim_end().to_string());
    }
    if !l.other.is_empty() {
        out.push("Left alone, not builds:".to_string());
        for o in &l.other {
            out.push(format!("  {}  ({})", o.name, o.why));
        }
    }
    let total: u64 = l.builds.iter().map(|b| b.bytes).sum();
    out.push(format!(
        "{} build{}, {}",
        l.builds.len(),
        plural(l.builds.len()),
        human_bytes(total)
    ));
    let gone = prunable(l, keep);
    let gone_bytes: u64 = gone.iter().map(|b| b.bytes).sum();
    out.push(format!(
        "{} removable ({}) — keeps the launcher's build, running builds and the newest {}",
        gone.len(),
        human_bytes(gone_bytes),
        keep.max(1)
    ));
    if l.unreadable_procs > 0 {
        out.push(format!(
            "{} of your processes would not show their executable, so whether they run a build is unknown.",
            l.unreadable_procs
        ));
    }
    out.join("\n")
}

fn render_pruned(p: &Pruned, dir: &Path, dry_run: bool) -> String {
    let n = p.removed.len();
    let mut out = vec![match (n, dry_run) {
        (0, _) => format!("Nothing to remove in {}.", dir.display()),
        (_, true) => format!(
            "Dry run, nothing removed. Would remove {n} build{} from {}, freeing {}:",
            plural(n),
            dir.display(),
            human_bytes(p.freed)
        ),
        (_, false) => format!(
            "Removed {n} build{} from {}, freeing {}:",
            plural(n),
            dir.display(),
            human_bytes(p.freed)
        ),
    }];
    for (name, bytes) in &p.removed {
        out.push(format!("  {name}  {}", human_bytes(*bytes)));
    }
    if !p.kept.is_empty() {
        out.push(format!("Kept {}:", p.kept.len()));
        for (name, why) in &p.kept {
            out.push(format!("  {name}  ({why})"));
        }
    }
    if !p.failed.is_empty() {
        out.push(format!("Could not remove {}:", p.failed.len()));
        for (name, why) in &p.failed {
            out.push(format!("  {name}  {why}"));
        }
    }
    out.join("\n")
}

fn list_json(l: &Listing, keep: usize) -> String {
    let gone = prunable(l, keep);
    let mut v = serde_json::to_value(l).unwrap_or_default();
    v["keep_newest"] = keep.max(1).into();
    v["removable"] = gone
        .iter()
        .map(|b| b.name.as_str())
        .collect::<Vec<_>>()
        .into();
    v["removable_bytes"] = gone.iter().map(|b| b.bytes).sum::<u64>().into();
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

fn pruned_json(p: &Pruned, dir: &Path, keep: usize, dry_run: bool) -> String {
    let mut v = serde_json::to_value(p).unwrap_or_default();
    v["dir"] = dir.to_string_lossy().into_owned().into();
    v["keep_newest"] = keep.into();
    v["dry_run"] = dry_run.into();
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

/// `terminal-delight admin builds [--json]` and
/// `terminal-delight admin builds prune [--keep N] [--dry-run] [--json]`.
/// Exit 0 when done, 1 when anything could not be read or removed, 2 on usage.
pub(crate) fn run_cli(args: &[String]) -> i32 {
    const WHO: &str = "terminal-delight admin builds";
    let command = match parse(args) {
        Ok(Command::Help) => {
            println!("{HELP}");
            return 0;
        }
        Ok(command) => command,
        Err(why) => {
            eprintln!("{WHO}: {why}\n{HELP}");
            return 2;
        }
    };
    let roots = match Roots::current() {
        Ok(roots) => roots,
        Err(why) => {
            eprintln!("{WHO}: {why}");
            return 2;
        }
    };
    for w in &roots.warnings {
        eprintln!("{WHO}: {w}");
    }
    match command {
        Command::Prune {
            keep,
            dry_run,
            json,
        } => {
            // A process table that does not show this very process cannot show
            // which builds are running, so nothing is removed on its word.
            let me = std::process::id();
            if !live_procs().iter().any(|(pid, _)| *pid == me) {
                eprintln!(
                    "{WHO}: this user's processes cannot be read from /proc, so running builds cannot be told apart; nothing removed"
                );
                return 1;
            }
            let pruned = prune(&roots, keep, dry_run, &live_procs);
            if json {
                println!("{}", pruned_json(&pruned, &roots.builds, keep, dry_run));
            } else {
                println!("{}", render_pruned(&pruned, &roots.builds, dry_run));
            }
            i32::from(!pruned.failed.is_empty())
        }
        _ => {
            let json = command == Command::List { json: true };
            let listing = list(&roots, &live_procs);
            if json {
                println!("{}", list_json(&listing, DEFAULT_KEEP));
            } else {
                println!("{}", render_list(&listing, DEFAULT_KEEP));
            }
            i32::from(listing.error.is_some())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    const BASE: u64 = 1_800_000_000;

    struct Fixture {
        root: PathBuf,
        roots: Roots,
        /// The build directory with every link resolved, as `/proc` reports it.
        canon_lib: PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.roots.bin, fs::Permissions::from_mode(0o755));
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    impl Fixture {
        fn lib(&self) -> &Path {
            &self.roots.builds
        }

        fn canon(&self, name: &str) -> PathBuf {
            self.canon_lib.join(name)
        }

        fn exists(&self, name: &str) -> bool {
            fs::symlink_metadata(self.lib().join(name)).is_ok()
        }

        fn point_launcher_at(&self, name: &str) {
            let link = self.roots.bin.join("terminal-delight");
            let _ = fs::remove_file(&link);
            symlink(format!("../lib/terminal-delight/{name}"), link).unwrap();
        }
    }

    fn elf(dir: &Path, name: &str, len: usize, age: u64) {
        let p = dir.join(name);
        let mut bytes = b"\x7fELF".to_vec();
        bytes.resize(len, 0);
        fs::write(&p, &bytes).unwrap();
        let f = fs::OpenOptions::new().write(true).open(&p).unwrap();
        f.set_modified(UNIX_EPOCH + Duration::from_secs(BASE - age))
            .unwrap();
    }

    /// Six builds (two with the same time), a launcher at the second oldest,
    /// and four entries that are not builds. HOME is reached through a symlink,
    /// so a resolved path from `/proc` differs from the listed one.
    fn fixture() -> Fixture {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("td-builds-test-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let real = root.join("real-home");
        fs::create_dir_all(real.join(".local/lib/terminal-delight")).unwrap();
        fs::create_dir_all(real.join(".local/bin")).unwrap();
        let home = root.join("home");
        symlink(&real, &home).unwrap();
        let roots =
            Roots::from_env(&|k: &str| (k == "HOME").then(|| home.clone().into_os_string()))
                .unwrap();
        assert!(roots.builds.starts_with(&root));
        if let Some(h) = std::env::var_os("HOME") {
            assert_ne!(
                roots.builds,
                Path::new(&h).join(".local/lib/terminal-delight")
            );
        }
        let canon_lib = fs::canonicalize(&roots.builds).unwrap();
        let f = Fixture {
            root,
            roots,
            canon_lib,
        };
        let lib = f.lib().to_path_buf();
        elf(&lib, "td-aaa-one", 1100, 6000);
        elf(&lib, "td-bbb-two", 1200, 5000);
        elf(&lib, "td-ccc-three", 1300, 4000);
        elf(&lib, "td-ddd-four", 1400, 3000);
        elf(&lib, "td-eee-five", 1500, 1000);
        elf(&lib, "td-fff-six", 1600, 1000);
        fs::write(lib.join("td-text"), "#!/bin/sh\necho hi\n").unwrap();
        fs::create_dir(lib.join("td-dir")).unwrap();
        symlink("td-aaa-one", lib.join("td-link")).unwrap();
        fs::write(lib.join("notes.txt"), b"\x7fELF and more").unwrap();
        f.point_launcher_at("td-bbb-two");
        f
    }

    fn names(builds: &[&Build]) -> Vec<String> {
        builds.iter().map(|b| b.name.clone()).collect()
    }

    fn removed(p: &Pruned) -> Vec<&str> {
        p.removed.iter().map(|(n, _)| n.as_str()).collect()
    }

    fn kept_for<'a>(p: &'a Pruned, name: &str) -> Option<&'a str> {
        p.kept.iter().find(|(n, _)| n == name).map(|(_, why)| *why)
    }

    #[test]
    fn lists_builds_in_use_and_leaves_the_rest_alone() {
        let f = fixture();
        let procs = || {
            vec![
                (42, f.canon("td-ccc-three")),
                (7, PathBuf::from("/usr/bin/bash")),
            ]
        };
        let l = list(&f.roots, &procs);
        assert!(l.present);
        assert!(l.error.is_none());
        let order: Vec<&str> = l.builds.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(
            order,
            [
                "td-eee-five",
                "td-fff-six",
                "td-ddd-four",
                "td-ccc-three",
                "td-bbb-two",
                "td-aaa-one"
            ]
        );
        assert!(l.builds.iter().enumerate().all(|(i, b)| b.newest_rank == i));
        let by = |n: &str| l.builds.iter().find(|b| b.name == n).unwrap();
        assert!(by("td-bbb-two").launcher);
        assert_eq!(l.builds.iter().filter(|b| b.launcher).count(), 1);
        assert_eq!(by("td-ccc-three").running, [42]);
        assert_eq!(l.builds.iter().filter(|b| !b.running.is_empty()).count(), 1);
        assert_eq!(by("td-aaa-one").bytes, 1100);
        assert_eq!(by("td-aaa-one").modified_unix, Some(BASE as i64 - 6000));
        assert_eq!(by("td-aaa-one").path, f.lib().join("td-aaa-one"));
        assert_eq!(
            l.launcher_target,
            Some(f.roots.bin.join("../lib/terminal-delight/td-bbb-two"))
        );
        let other: Vec<(&str, &str)> = l.other.iter().map(|o| (o.name.as_str(), o.why)).collect();
        assert_eq!(
            other,
            [
                ("notes.txt", NOT_TD),
                ("td-dir", NOT_FILE),
                ("td-link", NOT_FILE),
                ("td-text", NOT_ELF)
            ]
        );
        assert_eq!(names(&prunable(&l, 2)), ["td-ddd-four", "td-aaa-one"]);
        assert_eq!(names(&prunable(&l, 3)), ["td-aaa-one"]);
        assert_eq!(names(&prunable(&l, 0)), names(&prunable(&l, 1)));
        assert_eq!(
            names(&prunable(&l, 1)),
            ["td-fff-six", "td-ddd-four", "td-aaa-one"]
        );
        let json = serde_json::to_string(&l).unwrap();
        assert!(json.contains("\"launcher\":true"), "{json}");
    }

    #[test]
    fn a_deleted_executable_still_counts_as_running_its_path() {
        let f = fixture();
        let raw = format!("{} (deleted)", f.lib().join("td-aaa-one").display());
        assert_eq!(strip_deleted(Path::new(&raw)), f.lib().join("td-aaa-one"));
        assert_eq!(strip_deleted(Path::new("/x/td-a")), Path::new("/x/td-a"));
        let procs = || vec![(9, PathBuf::from(&raw))];
        let l = list(&f.roots, &procs);
        let aaa = l.builds.iter().find(|b| b.name == "td-aaa-one").unwrap();
        assert_eq!(aaa.running, [9]);
        assert!(names(&prunable(&l, 1)).iter().all(|n| n != "td-aaa-one"));
    }

    #[test]
    fn prune_keeps_the_launcher_running_and_newest() {
        let f = fixture();
        let procs = || vec![(42, f.canon("td-ccc-three"))];
        let p = prune(&f.roots, 2, false, &procs);
        assert_eq!(removed(&p), ["td-ddd-four", "td-aaa-one"]);
        assert_eq!(p.freed, 1400 + 1100);
        assert!(p.failed.is_empty());
        assert_eq!(
            p.kept,
            [
                ("td-eee-five".to_string(), "among the newest"),
                ("td-fff-six".to_string(), "among the newest"),
                ("td-ccc-three".to_string(), "running"),
                ("td-bbb-two".to_string(), "the launcher's build"),
            ]
        );
        assert!(!f.exists("td-ddd-four") && !f.exists("td-aaa-one"));
        for n in ["td-bbb-two", "td-ccc-three", "td-eee-five", "td-fff-six"] {
            assert!(f.exists(n), "{n}");
        }
        for n in ["td-text", "td-dir", "td-link", "notes.txt"] {
            assert!(f.exists(n), "{n}");
        }
        assert!(fs::symlink_metadata(f.lib().join("td-link"))
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(fs::symlink_metadata(f.roots.bin.join("terminal-delight")).is_ok());
        assert!(f.lib().is_dir());
    }

    #[test]
    fn dry_run_removes_nothing() {
        let f = fixture();
        let procs = || vec![(42, f.canon("td-ccc-three"))];
        let p = prune(&f.roots, 2, true, &procs);
        assert_eq!(removed(&p), ["td-ddd-four", "td-aaa-one"]);
        assert_eq!(p.freed, 2500);
        assert_eq!(p.kept.len(), 4);
        for n in [
            "td-aaa-one",
            "td-bbb-two",
            "td-ccc-three",
            "td-ddd-four",
            "td-eee-five",
        ] {
            assert!(f.exists(n), "{n}");
        }
    }

    #[test]
    fn a_build_that_starts_running_before_its_removal_is_kept() {
        let f = fixture();
        let calls = Cell::new(0);
        let procs = || {
            calls.set(calls.get() + 1);
            let mut v = vec![(42, f.canon("td-ccc-three"))];
            if calls.get() > 1 {
                v.push((77, f.canon("td-aaa-one")));
            }
            v
        };
        let p = prune(&f.roots, 2, false, &procs);
        assert_eq!(removed(&p), ["td-ddd-four"]);
        assert_eq!(kept_for(&p, "td-aaa-one"), Some("started running"));
        assert!(f.exists("td-aaa-one"));
        assert!(calls.get() >= 3, "procs is asked again before each removal");
    }

    #[test]
    fn a_build_that_changes_or_becomes_the_launcher_is_kept() {
        let f = fixture();
        let calls = Cell::new(0);
        let grow = || {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                elf(f.lib(), "td-aaa-one", 5000, 6000);
            }
            Vec::new()
        };
        let p = prune(&f.roots, 2, false, &grow);
        // Nothing runs here, so the third-oldest goes too.
        assert_eq!(removed(&p), ["td-ddd-four", "td-ccc-three"]);
        assert_eq!(
            kept_for(&p, "td-aaa-one"),
            Some("changed since it was listed")
        );

        let g = fixture();
        let calls = Cell::new(0);
        let repoint = || {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                g.point_launcher_at("td-aaa-one");
            }
            Vec::new()
        };
        let p = prune(&g.roots, 2, false, &repoint);
        assert_eq!(removed(&p), ["td-ddd-four", "td-ccc-three"]);
        assert_eq!(kept_for(&p, "td-aaa-one"), Some("now the launcher's build"));
        assert!(g.exists("td-aaa-one"));
    }

    #[test]
    fn an_unreadable_launcher_keeps_every_build() {
        let f = fixture();
        if fs::metadata("/proc/self").is_ok_and(|m| m.uid() == 0) {
            return; // root reads through any permission
        }
        fs::set_permissions(&f.roots.bin, fs::Permissions::from_mode(0o000)).unwrap();
        let l = list(&f.roots, &Vec::new);
        assert_eq!(l.launcher_target, None);
        let p = prune(&f.roots, 2, false, &Vec::new);
        assert!(p.removed.is_empty(), "{:?}", p.removed);
        assert_eq!(kept_for(&p, "td-bbb-two"), Some("launcher unreadable"));
        assert_eq!(kept_for(&p, "td-aaa-one"), Some("launcher unreadable"));
        fs::set_permissions(&f.roots.bin, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(f.exists("td-aaa-one") && f.exists("td-bbb-two"));
    }

    #[test]
    fn a_missing_directory_lists_nothing_and_prunes_nothing() {
        let f = fixture();
        fs::remove_dir_all(f.lib()).unwrap();
        let l = list(&f.roots, &Vec::new);
        assert!(!l.present && l.builds.is_empty() && l.error.is_none());
        let p = prune(&f.roots, 1, false, &Vec::new);
        assert!(p.removed.is_empty() && p.kept.is_empty() && p.failed.is_empty());
        fs::write(f.lib(), "a file where the directory should be").unwrap();
        let l = list(&f.roots, &Vec::new);
        assert!(!l.present && l.error.is_some());
    }

    #[test]
    fn human_bytes_reads_like_a_person_writes() {
        for (n, want) in [
            (0, "0 B"),
            (999, "999 B"),
            (1000, "1.0 KB"),
            (4200, "4.2 KB"),
            (9_949, "9.9 KB"),
            (9_950, "10 KB"),
            (999_499, "999 KB"),
            (999_500, "1.0 MB"),
            (49_177_016, "49 MB"),
            (246_000_000, "246 MB"),
            (12_000_000_000, "12 GB"),
            (u64::MAX, "18 EB"),
        ] {
            assert_eq!(human_bytes(n), want, "{n}");
        }
    }

    #[test]
    fn parses_the_command_line() {
        let p = |a: &[&str]| parse(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(p(&[]), Ok(Command::List { json: false }));
        assert_eq!(p(&["--json"]), Ok(Command::List { json: true }));
        assert_eq!(p(&["builds", "--json"]), Ok(Command::List { json: true }));
        assert_eq!(p(&["help"]), Ok(Command::Help));
        assert_eq!(
            p(&["prune"]),
            Ok(Command::Prune {
                keep: 3,
                dry_run: false,
                json: false
            })
        );
        assert_eq!(
            p(&["prune", "--keep", "5", "--dry-run", "--json"]),
            Ok(Command::Prune {
                keep: 5,
                dry_run: true,
                json: true
            })
        );
        assert_eq!(
            p(&["prune", "--keep", "0"]),
            Ok(Command::Prune {
                keep: 1,
                dry_run: false,
                json: false
            })
        );
        for bad in [
            &["prune", "--keep"][..],
            &["prune", "--keep", "x"],
            &["prune", "--keep", "-1"],
            &["prune", "--dry-run", "--dry-run"],
            &["--dry-run"],
            &["--json", "--json"],
            &["remove"],
        ] {
            assert!(p(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_text_says_what_is_in_use_and_what_would_go() {
        let f = fixture();
        let procs = || vec![(42, f.canon("td-ccc-three")), (43, f.canon("td-ccc-three"))];
        let l = list(&f.roots, &procs);
        let text = render_list(&l, DEFAULT_KEEP);
        assert!(text.contains("launcher"), "{text}");
        assert!(text.contains("running pid 42, 43"), "{text}");
        assert!(text.contains("6 builds, 8.1 KB"), "{text}");
        assert!(
            text.contains(
                "1 removable (1.1 KB) — keeps the launcher's build, running builds and the newest 3"
            ),
            "{text}"
        );
        assert!(
            text.contains("td-text  (not an executable (ELF))"),
            "{text}"
        );
        let json: serde_json::Value = serde_json::from_str(&list_json(&l, DEFAULT_KEEP)).unwrap();
        assert_eq!(json["removable"], serde_json::json!(["td-aaa-one"]));
        assert_eq!(json["removable_bytes"], 1100);
        let p = prune(&f.roots, 2, true, &procs);
        let text = render_pruned(&p, f.lib(), true);
        assert!(
            text.starts_with("Dry run, nothing removed. Would remove 2 builds"),
            "{text}"
        );
        assert!(
            text.contains("td-bbb-two  (the launcher's build)"),
            "{text}"
        );
    }

    /// Reads the operator's real build directory and process table. Never
    /// prunes. Run by hand: `cargo test -- --ignored list_real_builds_read_only`.
    #[test]
    #[ignore = "reads the real build directory; run by hand"]
    fn list_real_builds_read_only() {
        let roots = Roots::current().unwrap();
        let l = list(&roots, &live_procs);
        let total: u64 = l.builds.iter().map(|b| b.bytes).sum();
        let in_use: Vec<&Build> = l
            .builds
            .iter()
            .filter(|b| b.launcher || !b.running.is_empty())
            .collect();
        let gone = prunable(&l, DEFAULT_KEEP);
        println!(
            "SUMMARY {} builds, {} total; {} in use ({} running, launcher {}); {} removable ({}); {} other; {} unreadable procs",
            l.builds.len(),
            human_bytes(total),
            in_use.len(),
            l.builds.iter().filter(|b| !b.running.is_empty()).count(),
            l.builds.iter().find(|b| b.launcher).map_or("none", |b| b.name.as_str()),
            gone.len(),
            human_bytes(gone.iter().map(|b| b.bytes).sum()),
            l.other.len(),
            l.unreadable_procs,
        );
        for o in &l.other {
            println!("OTHER {} ({})", o.name, o.why);
        }
        println!("{}", render_list(&l, DEFAULT_KEEP));
    }
}
