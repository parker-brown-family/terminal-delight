//! CONFIG › ADMIN › Export: a verified copy of TD's settings, and optionally its
//! records, in a new directory outside every TD root, so a person can keep them
//! before an uninstall or carry them to another machine.
//!
//! An export either exists whole and verified or does not exist at all. Every
//! byte is written into a sibling `<dest>.partial-<pid>`, every copy is re-read
//! and its hash matched, the manifest is written last, and only then does the
//! directory take its real name. Any failure removes the partial directory.
use super::roots::Roots;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const SCHEMA: &str = "td-export/1";

const TOKEN: &str = "credential: phone pairing token";
const LOCK: &str = "lock file";
const TEMP: &str = "temporary file";
const MACHINE: &str = "bound to this machine";
const SESSION_BACKUPS: &str = "older copies of sessions; the current sessions are included";
const OTHER_SETTINGS: &str = "copy of another program's settings";
const NOT_REGULAR: &str = "not a regular file";

const HELP: &str = "Usage: terminal-delight admin export [--records] [--to DIR] [--json]

Copies Terminal Delight's settings into a new directory, by default
~/terminal-delight-export-<UTC date-time>, with a manifest.json naming every
file's size, sha256 and mode. Every copy is re-read and checked before the
export is reported. Nothing in the source is changed.

--records  also copy TD's records (its state directory)
--to DIR   the export directory itself; it must not exist yet
--json     print the report as JSON

Left out, each named with its reason in the manifest: the phone pairing token,
lock and temporary files, and files bound to this machine.";

pub(crate) struct ExportOptions {
    pub(crate) records: bool,
    pub(crate) dest: Option<PathBuf>,
    /// `YYYYMMDD-HHMMSS`, UTC, supplied by the caller so tests can fix it.
    pub(crate) stamp: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ExportReport {
    pub(crate) dir: PathBuf,
    pub(crate) files: u64,
    pub(crate) bytes: u64,
    pub(crate) symlinks: u64,
    pub(crate) excluded: Vec<Excluded>,
    pub(crate) verified: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct Excluded {
    /// Relative to the export, e.g. `config/mobile/token`.
    pub(crate) path: String,
    pub(crate) reason: &'static str,
}

#[derive(Serialize)]
struct FileEntry {
    path: String,
    bytes: u64,
    sha256: String,
    mode: String,
    #[serde(skip)]
    bits: u32,
}

#[derive(Serialize)]
struct LinkEntry {
    path: String,
    target: String,
}

#[derive(Serialize)]
struct Manifest<'a> {
    schema: &'static str,
    td_version: &'a str,
    created_utc: &'a str,
    hostname: Option<&'a str>,
    home: &'a Path,
    roots: ManifestRoots<'a>,
    records_included: bool,
    files: &'a [FileEntry],
    symlinks: &'a [LinkEntry],
    excluded: &'a [Excluded],
    verified: bool,
}

/// A root that is not there is null, never an empty list: absent and empty are
/// different answers to "what did this machine have".
#[derive(Serialize)]
struct ManifestRoots<'a> {
    config: Option<&'a Path>,
    state: Option<&'a Path>,
}

#[derive(Clone, Copy)]
enum Tree {
    Config,
    State,
}

impl Tree {
    fn prefix(self) -> &'static str {
        match self {
            Tree::Config => "config",
            Tree::State => "state",
        }
    }

    /// Subtrees left out whole; everything beneath carries the same reason.
    fn subtree(self, rel: &str) -> Option<&'static str> {
        match (self, rel) {
            (Tree::Config, "mobile") => Some(TOKEN),
            (Tree::Config, "sessions/backups") => Some(SESSION_BACKUPS),
            (Tree::State, "backups") => Some(OTHER_SETTINGS),
            _ => None,
        }
    }

    /// Single files left out by their path or name.
    fn loose(self, rel: &str, name: &str) -> Option<&'static str> {
        let machine: &[&str] = match self {
            Tree::Config => &[
                "hotkey-terminal-key.bak",
                "hotkey-terminal-key.owned",
                "protected",
            ],
            Tree::State => &["default-terminal.journal"],
        };
        // The whole hotkey undo family is machine-bound, including the
        // `.intent` an interrupted install leaves and whatever comes next.
        let hotkey = matches!(self, Tree::Config) && rel.starts_with("hotkey-terminal-key.");
        if machine.contains(&rel) || hotkey {
            Some(MACHINE)
        } else if name.ends_with(".lock") {
            Some(LOCK)
        } else if name.ends_with(".tmp")
            || name.ends_with(".part")
            || (name.starts_with('.') && name.contains(".td-save-"))
        {
            Some(TEMP)
        } else {
            None
        }
    }
}

#[derive(Default)]
struct Found {
    files: Vec<FileEntry>,
    symlinks: Vec<LinkEntry>,
    excluded: Vec<Excluded>,
}

fn label(tree: Tree, rel: &str) -> String {
    format!("{}/{rel}", tree.prefix())
}

/// Export TD's config, and its state when `opts.records`, into a new directory.
/// The source is never modified: files are opened read-only, symlinks are
/// recorded rather than followed, and nothing is written outside the new
/// directory and its partial sibling.
pub(crate) fn export(
    roots: &Roots,
    opts: &ExportOptions,
    td_version: &str,
    hostname: Option<&str>,
) -> Result<ExportReport, String> {
    check_stamp(&opts.stamp)?;
    let dest: PathBuf = match &opts.dest {
        Some(d) => d.components().collect(),
        None => roots
            .home
            .join(format!("terminal-delight-export-{}", opts.stamp)),
    };
    let partial = partial_path(&dest);
    check_dest(roots, &dest, &partial)?;
    // Creating it is what makes it ours to remove: an existing directory of
    // that name fails here and is left alone.
    fs::create_dir(&partial).map_err(|e| format!("cannot create {}: {e}", partial.display()))?;
    let result = fs::set_permissions(&partial, fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("cannot set the mode of {}: {e}", partial.display()))
        .and_then(|_| build(roots, opts, td_version, hostname, &partial))
        .and_then(|report| {
            // Checked again because rename would quietly replace an empty
            // directory created there since the export began.
            if fs::symlink_metadata(&dest).is_ok() {
                return Err(format!("{} appeared while exporting", dest.display()));
            }
            fs::rename(&partial, &dest).map_err(|e| {
                format!(
                    "cannot rename {} to {}: {e}",
                    partial.display(),
                    dest.display()
                )
            })?;
            Ok(ExportReport {
                dir: dest.clone(),
                ..report
            })
        });
    result.map_err(|why| match fs::remove_dir_all(&partial) {
        Ok(()) => why,
        Err(e) => format!(
            "{why}; the unfinished copy {} could not be removed: {e}",
            partial.display()
        ),
    })
}

/// What `export` would copy, from metadata alone, so the panel can show the
/// size before anything is written. An estimate: where this walk meets
/// something it cannot read it stops counting that tree, and `export` refuses
/// there with the reason.
pub(crate) fn plan_size(roots: &Roots, records: bool) -> (u64, u64) {
    let mut found = Found::default();
    let mut trees = vec![(Tree::Config, &roots.config)];
    if records {
        trees.push((Tree::State, &roots.state));
    }
    for (tree, root) in trees {
        if let Ok(Some(_)) = root_dir(root) {
            let _ = walk(tree, root, "", None, None, &mut found);
        }
    }
    (
        found.files.len() as u64,
        found.files.iter().map(|f| f.bytes).sum(),
    )
}

fn build(
    roots: &Roots,
    opts: &ExportOptions,
    td_version: &str,
    hostname: Option<&str>,
    partial: &Path,
) -> Result<ExportReport, String> {
    let mut found = Found::default();
    let config = copy_root(Tree::Config, &roots.config, partial, &mut found)?;
    let state = if opts.records {
        copy_root(Tree::State, &roots.state, partial, &mut found)?
    } else {
        // Not copied, but still reported as present unless it is known to be absent.
        !matches!(root_dir(&roots.state), Ok(None))
    };
    found.files.sort_by(|a, b| a.path.cmp(&b.path));
    found.symlinks.sort_by(|a, b| a.path.cmp(&b.path));
    found.excluded.sort_by(|a, b| a.path.cmp(&b.path));
    verify(partial, &found.files)?;

    let manifest = Manifest {
        schema: SCHEMA,
        td_version,
        created_utc: &opts.stamp,
        hostname,
        home: &roots.home,
        roots: ManifestRoots {
            config: config.then_some(roots.config.as_path()),
            state: state.then_some(roots.state.as_path()),
        },
        records_included: opts.records,
        files: &found.files,
        symlinks: &found.symlinks,
        excluded: &found.excluded,
        verified: true,
    };
    let mut json = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| format!("cannot write the manifest: {e}"))?;
    json.push(b'\n');
    let path = partial.join("manifest.json");
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .and_then(|mut f| f.write_all(&json))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    flush(partial)?;

    Ok(ExportReport {
        dir: partial.to_path_buf(),
        files: found.files.len() as u64,
        bytes: found.files.iter().map(|f| f.bytes).sum(),
        symlinks: found.symlinks.len() as u64,
        excluded: found.excluded,
        verified: true,
    })
}

/// The mode of a root that is a directory, `None` when it is absent. A root
/// reached through a symlink is followed, because a dotfiles checkout linked
/// into place is still the config; links inside it are not.
fn root_dir(root: &Path) -> Result<Option<u32>, String> {
    match fs::metadata(root) {
        Ok(m) if m.is_dir() => Ok(Some(m.permissions().mode())),
        Ok(_) => Err(format!("{} is not a directory", root.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read {}: {e}", root.display())),
    }
}

fn copy_root(tree: Tree, root: &Path, partial: &Path, found: &mut Found) -> Result<bool, String> {
    let Some(mode) = root_dir(root)? else {
        return Ok(false);
    };
    let out = partial.join(tree.prefix());
    make_dir(&out, mode)?;
    walk(tree, root, "", Some(&out), None, found)?;
    Ok(true)
}

/// Walks one tree in name order without following symlinks. With `out` it
/// copies into it; without, it only counts.
fn walk(
    tree: Tree,
    src: &Path,
    rel: &str,
    out: Option<&Path>,
    inherited: Option<&'static str>,
    found: &mut Found,
) -> Result<(), String> {
    let entries = match (fs::read_dir(src), inherited) {
        (Ok(entries), _) => entries,
        // An excluded subtree is only listed, so one that cannot be listed is
        // still named as left out rather than stopping the export.
        (Err(_), Some(why)) => {
            found.excluded.push(Excluded {
                path: label(tree, rel),
                reason: why,
            });
            return Ok(());
        }
        (Err(e), None) => return Err(format!("cannot read the directory {}: {e}", src.display())),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|e| format!("cannot read the directory {}: {e}", src.display()))?;
        names.push(entry.file_name());
    }
    names.sort();
    for name in names {
        let path = src.join(&name);
        let name = name.into_string().map_err(|_| {
            format!(
                "{} has a name that is not UTF-8; rename it and export again",
                path.display()
            )
        })?;
        let rel = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        let meta = fs::symlink_metadata(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let kind = meta.file_type();
        let reason = inherited.or_else(|| tree.subtree(&rel)).or_else(|| {
            if kind.is_dir() {
                None
            } else {
                tree.loose(&rel, &name)
            }
        });
        let target = out.map(|o| o.join(&rel));
        if kind.is_dir() {
            if let (None, Some(target)) = (reason, &target) {
                make_dir(target, meta.permissions().mode())?;
            }
            walk(tree, &path, &rel, out, reason, found)?;
        } else if let Some(why) = reason {
            found.excluded.push(Excluded {
                path: label(tree, &rel),
                reason: why,
            });
        } else if kind.is_symlink() {
            let link = fs::read_link(&path)
                .map_err(|e| format!("cannot read the link {}: {e}", path.display()))?;
            let link = link
                .into_os_string()
                .into_string()
                .map_err(|_| format!("{} points at a path that is not UTF-8", path.display()))?;
            found.symlinks.push(LinkEntry {
                path: label(tree, &rel),
                target: link,
            });
        } else if kind.is_file() {
            let bits = meta.permissions().mode() & 0o7777;
            let (bytes, sha256) = match &target {
                Some(target) => copy_file(&path, target, bits)?,
                None => (meta.len(), String::new()),
            };
            found.files.push(FileEntry {
                path: label(tree, &rel),
                bytes,
                sha256,
                mode: format!("{bits:04o}"),
                bits,
            });
        } else {
            found.excluded.push(Excluded {
                path: label(tree, &rel),
                reason: NOT_REGULAR,
            });
        }
    }
    Ok(())
}

/// Directories keep their group and other bits, so a private directory stays
/// private; the owner always gets rwx so the export can be read and removed.
fn make_dir(path: &Path, mode: u32) -> Result<(), String> {
    fs::create_dir(path)
        .and_then(|_| {
            fs::set_permissions(path, fs::Permissions::from_mode((mode & 0o7777) | 0o700))
        })
        .map_err(|e| format!("cannot create {}: {e}", path.display()))
}

/// Opens without following a symlink or blocking on a FIFO, then insists on a
/// regular file: the walk saw one, but something may have replaced it since.
fn open_regular(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if file.metadata()?.is_file() {
        Ok(file)
    } else {
        Err(io::Error::other("no longer a regular file"))
    }
}

/// Reads to the end in fixed chunks, hashing as it goes, so a record of several
/// gigabytes never has to fit in memory.
fn stream(
    input: &mut File,
    name: &Path,
    mut sink: impl FnMut(&[u8]) -> Result<(), String>,
) -> Result<(u64, String), String> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut bytes = 0u64;
    loop {
        let n = match input.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("cannot read {}: {e}", name.display())),
        };
        hasher.update(&buf[..n]);
        sink(&buf[..n])?;
        bytes += n as u64;
    }
    Ok((bytes, format!("{:x}", hasher.finalize())))
}

/// An unreadable file stops the whole export: one with silent holes is worse
/// than none, because a removal may follow it.
fn copy_file(src: &Path, dst: &Path, bits: u32) -> Result<(u64, String), String> {
    let mut input = open_regular(src).map_err(|e| format!("cannot read {}: {e}", src.display()))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dst)
        .map_err(|e| format!("cannot write {}: {e}", dst.display()))?;
    let (bytes, sha256) = stream(&mut input, src, |chunk| {
        output
            .write_all(chunk)
            .map_err(|e| format!("cannot write {}: {e}", dst.display()))
    })?;
    output
        .set_permissions(fs::Permissions::from_mode(bits))
        .map_err(|e| format!("cannot set the mode of {}: {e}", dst.display()))?;
    Ok((bytes, sha256))
}

fn verify(partial: &Path, files: &[FileEntry]) -> Result<(), String> {
    for f in files {
        let copy = partial.join(&f.path);
        let mut input =
            open_regular(&copy).map_err(|e| format!("cannot re-read {}: {e}", copy.display()))?;
        let bits = input
            .metadata()
            .map_err(|e| format!("cannot re-read {}: {e}", copy.display()))?
            .permissions()
            .mode()
            & 0o7777;
        let (bytes, sha256) = stream(&mut input, &copy, |_| Ok(()))?;
        if bytes != f.bytes || sha256 != f.sha256 || bits != f.bits {
            return Err(format!(
                "the copy of {} did not match its source when re-read",
                f.path
            ));
        }
    }
    Ok(())
}

/// One flush for the filesystem rather than one per file: records can be
/// thousands of files, and the export must be on disk before it is named
/// complete, because a removal may follow it.
fn flush(dir: &Path) -> Result<(), String> {
    let handle = File::open(dir).map_err(|e| format!("cannot open {}: {e}", dir.display()))?;
    // SAFETY: `handle` is an open descriptor for the whole call.
    if unsafe { libc::syncfs(handle.as_raw_fd()) } == 0 {
        Ok(())
    } else {
        Err(format!(
            "cannot flush {} to disk: {}",
            dir.display(),
            io::Error::last_os_error()
        ))
    }
}

fn partial_path(dest: &Path) -> PathBuf {
    let mut name = dest.as_os_str().to_owned();
    name.push(format!(".partial-{}", std::process::id()));
    PathBuf::from(name)
}

fn check_dest(roots: &Roots, dest: &Path, partial: &Path) -> Result<(), String> {
    if !dest.is_absolute()
        || dest.file_name().is_none()
        || dest.components().any(|c| matches!(c, Component::ParentDir))
    {
        return Err(format!(
            "{} is not an absolute path to a new directory",
            dest.display()
        ));
    }
    if roots.is_inside_td(dest) || roots.is_inside_td(partial) {
        return Err(format!(
            "{} is inside Terminal Delight's own directories, which an uninstall removes",
            dest.display()
        ));
    }
    match fs::symlink_metadata(dest) {
        Ok(_) => {
            return Err(format!(
                "{} already exists; an export only makes a new directory",
                dest.display()
            ))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("cannot check {}: {e}", dest.display())),
    }
    match dest.parent() {
        Some(parent) if parent.is_dir() => Ok(()),
        _ => Err(format!(
            "the directory to hold {} does not exist",
            dest.display()
        )),
    }
}

/// The stamp names a directory, so anything but `YYYYMMDD-HHMMSS` is refused
/// rather than allowed to add a path component.
fn check_stamp(stamp: &str) -> Result<(), String> {
    let ok = stamp.len() == 15
        && stamp.bytes().enumerate().all(|(i, c)| {
            if i == 8 {
                c == b'-'
            } else {
                c.is_ascii_digit()
            }
        });
    if ok {
        Ok(())
    } else {
        Err(format!("the stamp {stamp:?} is not YYYYMMDD-HHMMSS"))
    }
}

/// `YYYYMMDD-HHMMSS` for seconds since the Unix epoch, UTC.
pub(crate) fn stamp_from_unix(secs: u64) -> String {
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    let s = secs % 86_400;
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        s / 3600,
        s % 3600 / 60,
        s % 60
    )
}

/// Days since 1970-01-01 to a proleptic Gregorian date, after Howard Hinnant's
/// `civil_from_days`; written out because the crate has no date dependency.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// Decimal units, as the panel shows them: `1.1 GB`.
pub(crate) fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    if n < 1000 {
        return format!("{n} B");
    }
    let mut v = n as f64;
    let mut i = 0;
    while v >= 999.5 && i < UNITS.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    if v < 9.95 {
        format!("{v:.1} {}", UNITS[i])
    } else {
        format!("{v:.0} {}", UNITS[i])
    }
}

#[derive(Debug, Default, PartialEq)]
struct Cli {
    records: bool,
    to: Option<PathBuf>,
    json: bool,
}

fn parse(args: &[String]) -> Result<Cli, String> {
    let mut cli = Cli::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--records" if !cli.records => cli.records = true,
            "--json" if !cli.json => cli.json = true,
            "--to" if cli.to.is_none() => match it.next() {
                Some(dir) if !dir.is_empty() && !dir.starts_with('-') => {
                    cli.to = Some(PathBuf::from(dir))
                }
                _ => return Err("--to needs the export directory".into()),
            },
            _ => return Err(format!("Unknown or repeated option: {arg}")),
        }
    }
    Ok(cli)
}

/// `terminal-delight admin export …`; `args` are the words after `export`.
pub(crate) fn run_cli(args: &[String]) -> i32 {
    if matches!(args, [a] if a == "help" || a == "--help" || a == "-h") {
        println!("{HELP}");
        return 0;
    }
    let cli = match parse(args) {
        Ok(cli) => cli,
        Err(why) => {
            eprintln!("terminal-delight admin export: {why}\n\n{HELP}");
            return 2;
        }
    };
    match run(cli) {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(why) => {
            eprintln!("terminal-delight admin export: {why}");
            1
        }
    }
}

fn run(cli: Cli) -> Result<String, String> {
    let roots = Roots::current()?;
    for warning in &roots.warnings {
        eprintln!("terminal-delight admin export: {warning}");
    }
    let dest = match cli.to {
        Some(dir) if dir.is_relative() => Some(
            std::env::current_dir()
                .map_err(|e| format!("cannot read the working directory: {e}"))?
                .join(dir),
        ),
        other => other,
    };
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "the system clock is set before 1970")?
        .as_secs();
    let opts = ExportOptions {
        records: cli.records,
        dest,
        stamp: stamp_from_unix(secs),
    };
    let hostname = fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let report = export(
        &roots,
        &opts,
        env!("CARGO_PKG_VERSION"),
        hostname.as_deref(),
    )?;
    if cli.json {
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())
    } else {
        Ok(render(&report))
    }
}

fn render(r: &ExportReport) -> String {
    let mut s = format!(
        "Exported to {}\n{} {}, {}, every copy re-read and its sha256 matched\n",
        r.dir.display(),
        r.files,
        if r.files == 1 { "file" } else { "files" },
        human_bytes(r.bytes)
    );
    if r.symlinks > 0 {
        s.push_str(&format!(
            "{} symbolic {} recorded in manifest.json, not followed\n",
            r.symlinks,
            if r.symlinks == 1 { "link" } else { "links" }
        ));
    }
    if r.excluded.is_empty() {
        s.push_str("Nothing was left out.");
        return s;
    }
    s.push_str("Left out, each listed in manifest.json:");
    let mut by_reason: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for e in &r.excluded {
        by_reason.entry(e.reason).or_default().push(&e.path);
    }
    for (reason, paths) in by_reason {
        let shown = paths.iter().take(3).copied().collect::<Vec<_>>().join(", ");
        let more = match paths.len().saturating_sub(3) {
            0 => String::new(),
            n => format!(" and {n} more"),
        };
        s.push_str(&format!("\n  {reason}: {shown}{more}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::ffi::{CString, OsString};
    use std::os::unix::ffi::OsStrExt;
    use std::sync::atomic::{AtomicU64, Ordering};

    const STAMP: &str = "20261030-120000";

    struct Fixture {
        home: PathBuf,
        roots: Roots,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.home);
        }
    }

    fn fixture() -> Fixture {
        static N: AtomicU64 = AtomicU64::new(0);
        let home = std::env::temp_dir().join(format!(
            "tdx-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();
        let h: OsString = home.clone().into();
        let roots = Roots::from_env(&move |k| (k == "HOME").then(|| h.clone())).unwrap();
        Fixture { home, roots }
    }

    fn put(path: &Path, body: &[u8], mode: u32) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    fn big() -> Vec<u8> {
        (0..3 * 65_536 + 17)
            .map(|i: u32| (i * 7 % 251) as u8)
            .collect()
    }

    fn populate(r: &Roots) {
        let (c, s) = (&r.config, &r.state);
        put(&c.join("theme.toml"), b"name = \"ember\"\n", 0o644);
        put(&c.join("secret.toml"), b"key = 1\n", 0o600);
        put(&c.join("mobile/token"), b"pairing-secret", 0o600);
        put(&c.join("sessions/x.toml"), b"[session]\n", 0o644);
        put(&c.join("sessions/x.lock"), b"", 0o644);
        put(&c.join("sessions/backups/y"), b"old", 0o644);
        put(&c.join("sessions/.x.toml.td-save-123"), b"half", 0o644);
        put(&c.join("ui.toml.tmp"), b"half", 0o644);
        put(&c.join("protected"), b"machine-id = \"abc\"\n", 0o644);
        put(&c.join("hotkey-terminal-key.bak"), b"bind", 0o644);
        put(&c.join("hotkey-terminal-key.intent"), b"intent", 0o644);
        std::os::unix::fs::symlink("theme.toml", c.join("current-theme")).unwrap();
        let fifo = CString::new(c.join("pipe").as_os_str().as_bytes()).unwrap();
        // SAFETY: a valid NUL-terminated path for the duration of the call.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        put(&s.join("default-terminal.journal"), b"was: foot\n", 0o644);
        put(&s.join("wake/a.sh"), b"#!/bin/sh\necho hi\n", 0o600);
        put(&s.join("conversations/c.jsonl"), &big(), 0o644);
        put(&s.join("backups/hypr.conf"), b"bind = SUPER, T\n", 0o644);
    }

    fn opts(records: bool) -> ExportOptions {
        ExportOptions {
            records,
            dest: None,
            stamp: STAMP.into(),
        }
    }

    fn sha(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn manifest(dir: &Path) -> serde_json::Value {
        serde_json::from_slice(&fs::read(dir.join("manifest.json")).unwrap()).unwrap()
    }

    /// Mode, contents (when readable) and link target of one entry.
    type Seen = (u32, Option<Vec<u8>>, Option<PathBuf>);

    /// Every entry under `dir`.
    fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Seen> {
        let mut out = BTreeMap::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                let m = fs::symlink_metadata(&p).unwrap();
                if m.is_dir() {
                    stack.push(p.clone());
                }
                let body = if m.is_file() { fs::read(&p).ok() } else { None };
                out.insert(
                    p.clone(),
                    (m.permissions().mode(), body, fs::read_link(&p).ok()),
                );
            }
        }
        out
    }

    fn leftovers(home: &Path) -> Vec<String> {
        fs::read_dir(home)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("terminal-delight-export-") || n.contains(".partial-"))
            .collect()
    }

    #[test]
    fn exports_byte_copies_with_modes_hashes_and_reasons() {
        let f = fixture();
        populate(&f.roots);
        let before = snapshot(&f.home);
        let report = export(&f.roots, &opts(true), "0.5.0", Some("legion")).unwrap();
        let dir = f.home.join(format!("terminal-delight-export-{STAMP}"));
        assert_eq!(report.dir, dir);
        assert!(report.verified);
        assert_eq!(
            leftovers(&f.home),
            vec![format!("terminal-delight-export-{STAMP}")]
        );
        assert_eq!(
            fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );

        let m = manifest(&dir);
        assert_eq!(m["schema"], "td-export/1");
        assert_eq!(m["td_version"], "0.5.0");
        assert_eq!(m["created_utc"], STAMP);
        assert_eq!(m["hostname"], "legion");
        assert_eq!(m["home"], f.home.to_str().unwrap());
        assert_eq!(m["roots"]["config"], f.roots.config.to_str().unwrap());
        assert_eq!(m["roots"]["state"], f.roots.state.to_str().unwrap());
        assert_eq!(m["records_included"], true);
        assert_eq!(m["verified"], true);

        let files = m["files"].as_array().unwrap();
        let paths: Vec<&str> = files.iter().map(|e| e["path"].as_str().unwrap()).collect();
        assert_eq!(
            paths,
            [
                "config/secret.toml",
                "config/sessions/x.toml",
                "config/theme.toml",
                "state/conversations/c.jsonl",
                "state/wake/a.sh"
            ]
        );
        assert_eq!(report.files, 5);
        for e in files {
            let rel = e["path"].as_str().unwrap();
            let (tree, rest) = rel.split_once('/').unwrap();
            let root = if tree == "config" {
                &f.roots.config
            } else {
                &f.roots.state
            };
            let source = fs::read(root.join(rest)).unwrap();
            let copy = fs::read(dir.join(rel)).unwrap();
            assert_eq!(copy, source, "{rel}");
            assert_eq!(e["sha256"], sha(&source), "{rel}");
            assert_eq!(e["bytes"], source.len() as u64, "{rel}");
            let src_mode = fs::metadata(root.join(rest)).unwrap().permissions().mode() & 0o7777;
            let copy_mode = fs::metadata(dir.join(rel)).unwrap().permissions().mode() & 0o7777;
            assert_eq!(copy_mode, src_mode, "{rel}");
            assert_eq!(e["mode"], format!("{src_mode:04o}"), "{rel}");
        }
        let mode_of = |rel: &str| files.iter().find(|e| e["path"] == rel).unwrap()["mode"].clone();
        assert_eq!(mode_of("config/secret.toml"), "0600");
        assert_eq!(mode_of("state/wake/a.sh"), "0600");
        assert_eq!(
            report.bytes,
            files
                .iter()
                .map(|e| e["bytes"].as_u64().unwrap())
                .sum::<u64>()
        );

        assert_eq!(report.symlinks, 1);
        assert_eq!(
            m["symlinks"],
            serde_json::json!([{ "path": "config/current-theme", "target": "theme.toml" }])
        );
        assert!(fs::symlink_metadata(dir.join("config/current-theme")).is_err());

        let excluded: BTreeSet<(String, &str)> = report
            .excluded
            .iter()
            .map(|e| (e.path.clone(), e.reason))
            .collect();
        let want: BTreeSet<(String, &str)> = [
            ("config/sessions/.x.toml.td-save-123", TEMP),
            ("config/hotkey-terminal-key.bak", MACHINE),
            ("config/hotkey-terminal-key.intent", MACHINE),
            ("config/mobile/token", TOKEN),
            ("config/pipe", NOT_REGULAR),
            ("config/protected", MACHINE),
            ("config/sessions/backups/y", SESSION_BACKUPS),
            ("config/sessions/x.lock", LOCK),
            ("config/ui.toml.tmp", TEMP),
            ("state/backups/hypr.conf", OTHER_SETTINGS),
            ("state/default-terminal.journal", MACHINE),
        ]
        .into_iter()
        .map(|(p, r)| (p.to_string(), r))
        .collect();
        assert_eq!(excluded, want);
        assert_eq!(m["excluded"].as_array().unwrap().len(), want.len());
        for gone in [
            "config/mobile",
            "config/sessions/backups",
            "config/pipe",
            "state/backups",
        ] {
            assert!(fs::symlink_metadata(dir.join(gone)).is_err(), "{gone}");
        }

        let mut after = snapshot(&f.home);
        after.retain(|p, _| !p.starts_with(&dir));
        assert_eq!(after, before, "the source changed");
        assert_eq!(plan_size(&f.roots, true), (report.files, report.bytes));
    }

    #[test]
    fn without_records_state_is_left_but_still_reported_present() {
        let f = fixture();
        populate(&f.roots);
        let report = export(&f.roots, &opts(false), "0.5.0", None).unwrap();
        assert!(!report.dir.join("state").exists());
        assert!(report
            .excluded
            .iter()
            .all(|e| e.path.starts_with("config/")));
        let m = manifest(&report.dir);
        assert_eq!(m["records_included"], false);
        assert_eq!(m["hostname"], serde_json::Value::Null);
        assert_eq!(m["roots"]["state"], f.roots.state.to_str().unwrap());
        assert!(m["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["path"].as_str().unwrap().starts_with("config/")));
        assert_eq!(plan_size(&f.roots, false), (report.files, report.bytes));
        assert!(plan_size(&f.roots, true).1 > report.bytes);
    }

    #[test]
    fn an_absent_root_is_null_and_an_empty_one_is_present() {
        let f = fixture();
        put(&f.roots.config.join("theme.toml"), b"x", 0o644);
        let report = export(&f.roots, &opts(true), "0.5.0", None).unwrap();
        let m = manifest(&report.dir);
        assert_eq!(m["roots"]["state"], serde_json::Value::Null);
        assert!(!report.dir.join("state").exists());

        fs::create_dir_all(&f.roots.state).unwrap();
        let o = ExportOptions {
            dest: Some(f.home.join("second")),
            ..opts(true)
        };
        let report = export(&f.roots, &o, "0.5.0", None).unwrap();
        assert_eq!(
            manifest(&report.dir)["roots"]["state"],
            f.roots.state.to_str().unwrap()
        );
        assert!(report.dir.join("state").is_dir());
    }

    #[test]
    fn refuses_a_destination_inside_a_td_root_even_through_a_link() {
        let f = fixture();
        populate(&f.roots);
        std::os::unix::fs::symlink(&f.roots.config, f.home.join("cfg")).unwrap();
        fs::create_dir_all(&f.roots.cache).unwrap();
        for dest in [
            f.roots.config.join("out"),
            f.roots.state.join("out"),
            f.roots.cache.join("out"),
            f.home.join("cfg/out"),
        ] {
            let o = ExportOptions {
                dest: Some(dest.clone()),
                ..opts(true)
            };
            let err = export(&f.roots, &o, "0.5.0", None).unwrap_err();
            assert!(err.contains("inside Terminal Delight"), "{err}");
            assert!(fs::symlink_metadata(&dest).is_err());
            assert!(fs::symlink_metadata(partial_path(&dest)).is_err());
        }
    }

    #[test]
    fn refuses_an_existing_destination_a_missing_parent_and_a_relative_path() {
        let f = fixture();
        populate(&f.roots);
        fs::create_dir(f.home.join("dir")).unwrap();
        put(&f.home.join("file"), b"keep", 0o644);
        std::os::unix::fs::symlink("nowhere", f.home.join("dangling")).unwrap();
        for name in ["dir", "file", "dangling"] {
            let o = ExportOptions {
                dest: Some(f.home.join(name)),
                ..opts(true)
            };
            let err = export(&f.roots, &o, "0.5.0", None).unwrap_err();
            assert!(err.contains("already exists"), "{err}");
        }
        assert_eq!(fs::read_dir(f.home.join("dir")).unwrap().count(), 0);
        assert_eq!(fs::read(f.home.join("file")).unwrap(), b"keep");
        for dest in [f.home.join("no/such/out"), PathBuf::from("relative/out")] {
            let o = ExportOptions {
                dest: Some(dest),
                ..opts(true)
            };
            assert!(export(&f.roots, &o, "0.5.0", None).is_err());
        }
        export(&f.roots, &opts(false), "0.5.0", None).unwrap();
        let err = export(&f.roots, &opts(false), "0.5.0", None).unwrap_err();
        assert!(err.contains("already exists"), "{err}");
    }

    #[test]
    fn an_unreadable_file_aborts_leaves_no_partial_and_no_trace_in_the_source() {
        let f = fixture();
        populate(&f.roots);
        let locked = f.roots.state.join("conversations/zz.jsonl");
        put(&locked, b"private", 0o000);
        if File::open(&locked).is_ok() {
            return; // running as root, which reads anything
        }
        let before = snapshot(&f.home);
        let err = export(&f.roots, &opts(true), "0.5.0", None).unwrap_err();
        assert!(err.contains("zz.jsonl"), "{err}");
        assert!(leftovers(&f.home).is_empty(), "{:?}", leftovers(&f.home));
        assert_eq!(snapshot(&f.home), before);
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(fs::read(&locked).unwrap(), b"private");
    }

    #[test]
    fn an_existing_partial_directory_is_refused_and_left_alone() {
        let f = fixture();
        populate(&f.roots);
        let dest = f.home.join("out");
        put(&partial_path(&dest).join("theirs"), b"x", 0o644);
        let o = ExportOptions {
            dest: Some(dest.clone()),
            ..opts(true)
        };
        assert!(export(&f.roots, &o, "0.5.0", None).is_err());
        assert_eq!(fs::read(partial_path(&dest).join("theirs")).unwrap(), b"x");
        assert!(!dest.exists());
    }

    #[test]
    fn stamps_are_utc_civil_dates_and_bad_ones_are_refused() {
        assert_eq!(stamp_from_unix(0), "19700101-000000");
        assert_eq!(stamp_from_unix(951_782_400), "20000229-000000");
        assert_eq!(stamp_from_unix(1_700_000_000), "20231114-221320");
        assert_eq!(stamp_from_unix(4_102_444_799), "20991231-235959");
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(-719_468), (0, 3, 1));
        let f = fixture();
        for bad in [
            "2026-10-30",
            "20261030-12000",
            "20261030/120000",
            "../../etc/x",
        ] {
            let o = ExportOptions {
                stamp: bad.into(),
                ..opts(false)
            };
            assert!(export(&f.roots, &o, "0.5.0", None).is_err(), "{bad}");
        }
        assert!(leftovers(&f.home).is_empty());
    }

    #[test]
    fn bytes_read_as_decimal_units() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(999), "999 B");
        assert_eq!(human_bytes(1000), "1.0 kB");
        assert_eq!(human_bytes(999_999), "1.0 MB");
        assert_eq!(human_bytes(25_000_000), "25 MB");
        assert_eq!(human_bytes(1_100_000_000), "1.1 GB");
    }

    #[test]
    fn options_parse_and_refuse_ambiguity() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(parse(&args(&[])).unwrap(), Cli::default());
        assert_eq!(
            parse(&args(&["--records", "--to", "/x", "--json"])).unwrap(),
            Cli {
                records: true,
                to: Some(PathBuf::from("/x")),
                json: true
            }
        );
        for bad in [
            &["--to"][..],
            &["--to", "--json"],
            &["--records", "--records"],
            &["--all"],
        ] {
            assert!(parse(&args(bad)).is_err(), "{bad:?}");
        }
        assert_eq!(run_cli(&args(&["--bogus"])), 2);
    }

    #[test]
    fn the_text_report_groups_what_was_left_out() {
        let r = ExportReport {
            dir: PathBuf::from("/h/terminal-delight-export-x"),
            files: 1,
            bytes: 1_100_000_000,
            symlinks: 0,
            excluded: (0..5)
                .map(|i| Excluded {
                    path: format!("config/{i}.lock"),
                    reason: LOCK,
                })
                .collect(),
            verified: true,
        };
        let text = render(&r);
        assert!(text.contains("1 file, 1.1 GB"), "{text}");
        assert!(
            text.contains("lock file: config/0.lock, config/1.lock, config/2.lock and 2 more"),
            "{text}"
        );
    }
}
