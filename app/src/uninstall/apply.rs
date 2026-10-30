//! The removal worker: carries out a reviewed plan, one step at a time, and
//! writes down what happened to each. It refuses before writing anything when
//! the machine is protected or no longer matches the plan. A step that fails
//! blocks the steps that wait on it and nothing else. A run whose HOME is not
//! the account's own home may not touch anything outside that HOME, judged
//! after resolving symlinks, and may not signal a process whose executable is
//! not inside it: a fixture can never reach the operator's files, sockets or
//! programs.
//!
//! Every effect outside the filesystem goes through [`Effects`], so tests can
//! watch what the worker would have done without doing it.
use super::blocks::{self, KeyBlock};
use super::known;

/// The receipt TD writes beside the loose files it unpacks (`art::record_asset`).
pub(crate) const ASSET_MANIFEST: &str = "terminal-delight-assets.manifest";
use super::plan::{host_dirs, Op, Plan, Report, Retention};
use super::roots::{resolve_existing_prefix, Roots};
use super::settings;
use serde::Serialize;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) const SCHEMA: &str = "td-uninstall-receipt/1";

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Proc {
    pub(crate) pid: u32,
    /// The executable, with a trailing " (deleted)" already removed; empty
    /// when it could not be read.
    pub(crate) exe: PathBuf,
    pub(crate) argv: Vec<String>,
    /// `/proc/<pid>/comm`, readable even when `exe` is not.
    pub(crate) comm: String,
    /// What the running executable is, read through the process itself, so a
    /// file deleted or replaced on disk still answers for the program running.
    pub(crate) kind: ExeKind,
}

/// The running executable's own bytes, never the path's: a build is an ELF
/// that is not an AppImage, and anything unreadable is unknown rather than one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExeKind {
    Build,
    AppImage,
    Other,
    Unknown,
}

impl ExeKind {
    /// What an executable's bytes say it is, the one test every part of the
    /// removal and the old-builds tool uses. An AppImage carries "AI" and its
    /// type, 1 or 2, at offset 8; an older type 1 image may carry only the
    /// ISO 9660 volume descriptor it is built around, at 32769. An ELF with
    /// neither is a build.
    pub(crate) fn of_reader<R: std::io::Read + std::io::Seek>(
        r: &mut R,
    ) -> std::io::Result<ExeKind> {
        use std::io::{Read, SeekFrom};
        let mut head = Vec::with_capacity(11);
        (&mut *r).take(11).read_to_end(&mut head)?;
        if !head.starts_with(b"\x7fELF") {
            return Ok(ExeKind::Other);
        }
        if matches!(head.get(8..11), Some(b"AI\x01" | b"AI\x02")) {
            return Ok(ExeKind::AppImage);
        }
        let mut iso = Vec::with_capacity(5);
        r.seek(SeekFrom::Start(32769))?;
        (&mut *r).take(5).read_to_end(&mut iso)?;
        Ok(if iso == b"CD001" {
            ExeKind::AppImage
        } else {
            ExeKind::Build
        })
    }
}

pub(crate) trait Effects {
    /// HOME is the account's own home, so global effects are allowed.
    fn account_home(&self) -> bool;
    fn self_pid(&self) -> u32;
    fn processes(&mut self) -> Vec<Proc>;
    /// Signal `pid` only if it is still running `exe`; false when it is not.
    fn signal(&mut self, pid: u32, sig: i32, exe: &Path) -> bool;
    /// Ask the session host at `socket` to save and close, but only when the
    /// process listening there passes `allow`, judged by its executable.
    fn close_host(
        &mut self,
        socket: &Path,
        allow: &dyn Fn(&Path, ExeKind) -> bool,
    ) -> Result<(), String>;
    fn thaw(&mut self, session: &str) -> usize;
    /// Run a program; `Err` carries why, starting "not installed" when it is not.
    fn run(&mut self, argv: &[String], cwd: Option<&Path>) -> Result<String, String>;
    fn default_terminal_off(&mut self, list: &Path, journal: &Path) -> Result<String, String>;
    fn export(&mut self, dest: &Path) -> Result<String, String>;
    fn sleep(&mut self, d: Duration);
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
    Done,
    /// Deliberately not attempted; does not block what waits on it.
    Skipped,
    Failed,
    /// Waited on a step that did not finish.
    Blocked,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Outcome {
    pub(crate) step: usize,
    pub(crate) op: String,
    pub(crate) why: String,
    pub(crate) status: Status,
    pub(crate) detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Receipt {
    pub(crate) schema: &'static str,
    pub(crate) stamp: String,
    pub(crate) machine_id: Option<String>,
    pub(crate) hostname: Option<String>,
    pub(crate) home: PathBuf,
    pub(crate) retention: Retention,
    /// Why nothing was done, when the worker refused.
    pub(crate) refused: Option<String>,
    /// True only when every step finished. A partial removal never says complete.
    pub(crate) complete: bool,
    /// What happened to the records, read from the outcomes, not the request.
    pub(crate) records: String,
    pub(crate) outcomes: Vec<Outcome>,
    pub(crate) reports: Vec<Report>,
    pub(crate) removal_dir: PathBuf,
    /// Whether this receipt reached the disk. A run whose receipt could not be
    /// written is reported as failed, whatever its steps did. Not part of the
    /// file, which cannot truthfully describe its own writing.
    #[serde(skip)]
    pub(crate) saved: bool,
}

/// Why `plan` must not run, checked before anything is written. `fresh` is a
/// plan made just now with the same choices; what runs is what was reviewed.
pub(crate) fn refusal(plan: &Plan, fresh: &Plan, roots: &Roots, out: &Path) -> Option<String> {
    let planned_removal = |o: &Path| {
        plan.steps.iter().any(|s| match &s.op {
            Op::RemoveTree { path }
            | Op::RemoveRecords { path }
            | Op::RemoveDirIfEmpty { path } => {
                o.starts_with(path)
                    || resolve_existing_prefix(o).starts_with(resolve_existing_prefix(path))
            }
            _ => false,
        })
    };
    let home = std::fs::canonicalize(&roots.home).unwrap_or_else(|_| roots.home.clone());
    if let Some(p) = &fresh.protected {
        Some(format!("Refused: {p}."))
    } else if let Some(b) = fresh.blocked.as_ref().or(plan.blocked.as_ref()) {
        Some(format!("Refused: {b}."))
    } else if planned_removal(out) || planned_removal(&resolve_existing_prefix(out)) {
        Some("Refused: the removal directory is inside something the removal deletes.".into())
    } else if !super::guard::is_account_home(&roots.home)
        && !resolve_existing_prefix(out).starts_with(&home)
    {
        Some(
            "Refused: this home is not the account's, and the removal directory is outside it."
                .into(),
        )
    } else if plan.machine_id.is_none() || plan.machine_id != fresh.machine_id {
        Some(
            "Refused: the plan was made on another machine, or this machine's id cannot be read."
                .into(),
        )
    } else if plan.home != fresh.home || plan.home != roots.home {
        Some("Refused: the plan was made for a different home directory.".into())
    } else if !valid_stamp(&plan.stamp) {
        Some("Refused: the plan's stamp is not a timestamp.".into())
    } else if plan.retention != fresh.retention || plan.steps != fresh.steps {
        Some("Refused: TD's files changed since the plan was made. Review a fresh plan.".into())
    } else if roots.is_inside_td(out) {
        Some("Refused: the removal directory is inside a directory the removal deletes.".into())
    } else {
        None
    }
}

/// `YYYYMMDD-HHMMSS`: the only stamp a plan may carry, since it names a directory.
pub(crate) fn valid_stamp(s: &str) -> bool {
    s.len() == 15
        && s.char_indices()
            .all(|(i, c)| if i == 8 { c == '-' } else { c.is_ascii_digit() })
}

/// Run `plan`. `out` is the removal directory: it must already exist, created
/// fresh by the caller after [`refusal`] said no.
pub(crate) fn apply(
    plan: &Plan,
    fresh: &Plan,
    roots: &Roots,
    out: &Path,
    fx: &mut dyn Effects,
) -> Receipt {
    let mut receipt = Receipt {
        schema: SCHEMA,
        stamp: plan.stamp.clone(),
        machine_id: plan.machine_id.clone(),
        hostname: plan.hostname.clone(),
        home: plan.home.clone(),
        retention: plan.retention,
        refused: None,
        complete: false,
        records: "kept".into(),
        outcomes: Vec::new(),
        reports: plan.reports.clone(),
        removal_dir: out.to_path_buf(),
        saved: false,
    };
    if let Some(why) = refusal(plan, fresh, roots, out) {
        receipt.refused = Some(why);
        receipt.saved = write_receipt(out, &receipt).is_ok();
        return receipt;
    }
    let started = (|| -> std::io::Result<std::fs::File> {
        std::fs::create_dir(out.join("before"))?;
        write_new(
            &out.join("plan.json"),
            serde_json::to_string_pretty(plan)
                .unwrap_or_default()
                .as_bytes(),
        )?;
        std::fs::OpenOptions::new()
            .create_new(true)
            .append(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(out.join("journal.jsonl"))
    })();
    let mut journal = match started {
        Ok(j) => j,
        Err(e) => {
            receipt.refused = Some(format!(
                "Refused: cannot write the journal in {}: {e}",
                out.display()
            ));
            receipt.saved = write_receipt(out, &receipt).is_ok();
            return receipt;
        }
    };

    let (plan_exes, td_builds) = plan
        .steps
        .iter()
        .find_map(|s| match &s.op {
            Op::StopProcesses { exes, builds } => Some((exes.clone(), builds.clone())),
            _ => None,
        })
        .unwrap_or_else(|| (Vec::new(), roots.builds.clone()));
    let mut ctx = Ctx {
        roots,
        out,
        fx,
        n_before: 0,
        td_exes: Vec::new(),
        td_builds,
    };
    ctx.td_exes = ctx.confined(&plan_exes);
    let mut statuses: Vec<Status> = Vec::new();
    let mut journal_ok = true;
    for (i, step) in plan.steps.iter().enumerate() {
        let waiting: Vec<usize> = step
            .after
            .iter()
            .copied()
            .filter(|a| !matches!(statuses.get(*a), Some(Status::Done | Status::Skipped)))
            .collect();
        let (status, detail) = if !journal_ok {
            (
                Status::Blocked,
                "the journal could not be written, so nothing more was done".into(),
            )
        } else if !waiting.is_empty() {
            let list: Vec<String> = waiting.iter().map(|w| (w + 1).to_string()).collect();
            (
                Status::Blocked,
                format!("waits on step {} which did not finish", list.join(", ")),
            )
        } else {
            match ctx.run(&step.op) {
                Ok(d) => (Status::Done, d),
                Err(Halt::Skip(d)) => (Status::Skipped, d),
                Err(Halt::Fail(d)) => (Status::Failed, d),
            }
        };
        let o = Outcome {
            step: i + 1,
            op: summary(&step.op),
            why: step.why.clone(),
            status: status.clone(),
            detail,
        };
        if journal_ok {
            let line = serde_json::to_string(&o).unwrap_or_default();
            journal_ok = writeln!(journal, "{line}")
                .and_then(|_| journal.sync_data())
                .is_ok();
        }
        statuses.push(status);
        receipt.outcomes.push(o);
    }
    receipt.complete = journal_ok && statuses.iter().all(|s| *s == Status::Done);
    receipt.records = records_outcome(plan, &statuses);
    receipt.saved = write_receipt(out, &receipt).is_ok();
    if !receipt.saved {
        receipt.complete = false;
    }
    receipt
}

/// The records sentence, read off what the steps did, root by root.
fn records_outcome(plan: &Plan, statuses: &[Status]) -> String {
    if plan.retention == Retention::Keep {
        return "kept".into();
    }
    let export = plan
        .steps
        .iter()
        .zip(statuses)
        .find(|(s, _)| matches!(s.op, Op::Export { .. }))
        .map(|(_, st)| *st == Status::Done);
    let roots: Vec<(String, &Status)> = plan
        .steps
        .iter()
        .zip(statuses)
        .filter_map(|(s, st)| match &s.op {
            Op::RemoveRecords { path } => Some((path.display().to_string(), st)),
            _ => None,
        })
        .collect();
    let pick = |want: &dyn Fn(&Status) -> bool| -> Vec<&str> {
        roots
            .iter()
            .filter(|(_, st)| want(st))
            .map(|(p, _)| p.as_str())
            .collect()
    };
    let removed = pick(&|s| *s == Status::Done);
    // A recursive removal that failed may have taken part of the tree first.
    let partial = pick(&|s| *s == Status::Failed);
    let untouched = pick(&|s| matches!(s, Status::Blocked | Status::Skipped));
    let lead = match (plan.retention, export) {
        (Retention::Export, Some(true)) => "exported and verified; ",
        (Retention::Export, _) => "the export did not finish, so none were removed; ",
        _ => "",
    };
    let mut parts = Vec::new();
    if !removed.is_empty() {
        parts.push(if partial.is_empty() && untouched.is_empty() {
            "removed".to_string()
        } else {
            format!("removed {}", removed.join(", "))
        });
    }
    if !partial.is_empty() {
        parts.push(format!("possibly partly removed: {}", partial.join(", ")));
    }
    if !untouched.is_empty() {
        parts.push(format!("still in place: {}", untouched.join(", ")));
    }
    if parts.is_empty() {
        parts.push("still in place".into());
    }
    format!("{lead}{}", parts.join("; "))
}

enum Halt {
    Skip(String),
    Fail(String),
}

struct Ctx<'a> {
    roots: &'a Roots,
    out: &'a Path,
    fx: &'a mut dyn Effects,
    n_before: usize,
    /// The plan's TD executables, confined, and its builds directory, for
    /// judging who listens on a host socket.
    td_exes: Vec<PathBuf>,
    td_builds: PathBuf,
}

impl Ctx<'_> {
    /// Inside HOME always, judged after resolving symlinks in every ancestor;
    /// anywhere else only for the account's own home.
    fn allowed(&self, p: &Path) -> Result<(), Halt> {
        if !p.is_absolute() {
            return Err(Halt::Fail(format!(
                "{} is not an absolute path",
                p.display()
            )));
        }
        if self.fx.account_home() {
            return Ok(());
        }
        let home =
            std::fs::canonicalize(&self.roots.home).unwrap_or_else(|_| self.roots.home.clone());
        if p.starts_with(&self.roots.home) && resolve_existing_prefix(p).starts_with(&home) {
            Ok(())
        } else {
            Err(Halt::Fail(format!(
                "refused: {} is outside the fixture home",
                p.display()
            )))
        }
    }

    /// Keep a copy of a shared file before it is edited; no copy, no edit.
    fn keep_copy(&mut self, p: &Path) -> Result<(), Halt> {
        let Some(name) = p.file_name() else {
            return Err(Halt::Fail("no file name".into()));
        };
        let bytes = match std::fs::read(p) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(Halt::Fail(format!("cannot copy it first: {e}"))),
        };
        self.n_before += 1;
        let dest = self.out.join("before").join(format!(
            "{:02}-{}",
            self.n_before,
            name.to_string_lossy()
        ));
        write_new(&dest, &bytes).map_err(|e| Halt::Fail(format!("cannot copy it first: {e}")))
    }

    fn run(&mut self, op: &Op) -> Result<String, Halt> {
        let fail = |e: std::io::Error| Halt::Fail(e.to_string());
        match op {
            Op::Export { dest } => {
                self.allowed(dest)?;
                self.fx.export(dest).map_err(Halt::Fail)
            }
            Op::DefaultTerminalOff { list, journal } => {
                self.allowed(list)?;
                self.allowed(journal)?;
                self.keep_copy(journal)?;
                self.keep_copy(list)?;
                self.fx
                    .default_terminal_off(list, journal)
                    .map_err(Halt::Fail)
            }
            Op::CheckDefaultTerminal { list } => {
                self.allowed(list)?;
                match std::fs::read_to_string(list) {
                    Ok(t) if t.lines().any(|l| l.trim() == "terminal-delight.desktop") => Err(Halt::Fail(format!(
                        "TD is still a default terminal in {}; choose another terminal first, or the desktop would have none",
                        list.display()
                    ))),
                    Ok(_) => Ok("TD is not a default terminal".into()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("no terminal list".into()),
                    Err(e) => Err(fail(e)),
                }
            }
            Op::EditSettings {
                file,
                sha256,
                commands,
            } => {
                self.allowed(file)?;
                self.edit(file, sha256, |text| {
                    let s = settings::strip(text, commands, true)?;
                    let n = s.removed_hooks.len() + s.removed_allow.len();
                    Ok(s.text.map(|t| (t, format!("removed {n} entries"))))
                })
            }
            Op::StripKeyBlock { file, sha256, dest } => {
                self.allowed(file)?;
                self.edit(file, sha256, |text| {
                    match blocks::strip_key_block(text, dest) {
                        KeyBlock::Owned(t) => Ok(Some((t, "removed the key-sheet block".into()))),
                        KeyBlock::Absent => Ok(None),
                        KeyBlock::Refused(why) => Err(why),
                    }
                })
            }
            Op::McpRemove {
                argv,
                cwd,
                file,
                client,
                name,
                scope,
                project,
                command,
                args,
                path,
            } => {
                if !self.fx.account_home() {
                    return Err(Halt::Skip(
                        "not attempted: HOME is not this account's home".into(),
                    ));
                }
                self.allowed(file)?;
                match super::mcp::current_entry(client, file, scope, project.as_deref(), name) {
                    Ok(Some((c, a, pth))) if c == *command && a == *args && pth == *path => {}
                    Ok(Some(_)) => {
                        return Err(Halt::Fail(format!(
                        "the \"{name}\" entry changed since the plan; it is not TD's to remove now"
                    )))
                    }
                    Ok(None) => return Ok("already gone".into()),
                    Err(e) => return Err(Halt::Fail(format!("cannot read it now: {e}"))),
                }
                self.keep_copy(file)?;
                self.fx
                    .run(argv, cwd.as_deref())
                    .map_err(|e| Halt::Fail(format!("{e}; remove it by hand: {}", argv.join(" "))))
            }
            Op::CloseHosts { dir } => {
                self.allowed(dir)?;
                let mut socks: Vec<PathBuf> = std::fs::read_dir(dir)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.starts_with("session-") && n.ends_with(".sock"))
                    })
                    .collect();
                socks.sort();
                let exes = self.td_exes.clone();
                let builds = self.td_builds.clone();
                let allow = move |exe: &Path, kind: ExeKind| is_td_exe(exe, kind, &exes, &builds);
                let mut left = Vec::new();
                for s in &socks {
                    // A real socket in this directory, never a link to one
                    // somewhere else.
                    let real = std::fs::symlink_metadata(s).is_ok_and(|m| {
                        use std::os::unix::fs::FileTypeExt;
                        m.file_type().is_socket()
                    });
                    if !real {
                        left.push(format!("{}: not a socket here; left", s.display()));
                        continue;
                    }
                    if let Err(e) = self.fx.close_host(s, &allow) {
                        left.push(format!("{}: {e}", s.display()));
                    }
                }
                if left.is_empty() {
                    Ok(format!("{} host(s) asked to save and close", socks.len()))
                } else {
                    // A host that will not answer is left to the process stop
                    // that follows, which matches it by executable.
                    Ok(format!(
                        "{} closed; did not answer: {}",
                        socks.len() - left.len(),
                        left.join("; ")
                    ))
                }
            }
            Op::StopProcesses { exes, builds } => self.stop(exes, builds),
            Op::ThawScopes { sessions } => {
                if !self.fx.account_home() {
                    return Err(Halt::Skip(
                        "not attempted: HOME is not this account's home".into(),
                    ));
                }
                let n: usize = sessions.iter().map(|s| self.fx.thaw(s)).sum();
                Ok(format!("{n} frozen pane(s) thawed"))
            }
            Op::RemoveFile { path, sha256 } => {
                self.allowed(path)?;
                match std::fs::symlink_metadata(path) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        return Ok("already gone".into())
                    }
                    Err(e) => return Err(fail(e)),
                    Ok(m) if !m.is_file() => {
                        return Err(Halt::Fail("no longer a regular file; left".into()))
                    }
                    Ok(_) => {}
                }
                if let Some(want) = sha256 {
                    let now = known::sha256_file(path).map_err(fail)?;
                    if &now != want {
                        return Err(Halt::Fail("changed since the plan; left".into()));
                    }
                }
                std::fs::remove_file(path).map_err(fail)?;
                Ok("removed".into())
            }
            Op::RemoveBuild { path } => {
                self.allowed(path)?;
                match std::fs::symlink_metadata(path) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        return Ok("already gone".into())
                    }
                    Err(e) => return Err(fail(e)),
                    Ok(m) if !m.is_file() => {
                        return Err(Halt::Fail("no longer a regular file; left".into()))
                    }
                    Ok(_) => {}
                }
                // Read again, without following a link or waiting on a FIFO:
                // only bytes that still say build are removed.
                let kind = std::fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                    .open(path)
                    .and_then(|mut f| ExeKind::of_reader(&mut f));
                match kind {
                    Ok(ExeKind::Build) => {}
                    Ok(_) => return Err(Halt::Fail("no longer a build; left".into())),
                    Err(e) => {
                        return Err(Halt::Fail(format!(
                            "its bytes could not be read ({e}); left"
                        )))
                    }
                }
                std::fs::remove_file(path).map_err(fail)?;
                Ok("removed".into())
            }
            Op::RemoveLink { path, target } => {
                self.allowed(path)?;
                match std::fs::symlink_metadata(path) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("already gone".into()),
                    Err(e) => Err(fail(e)),
                    Ok(m) if !m.file_type().is_symlink() => {
                        Err(Halt::Fail("no longer a link; left".into()))
                    }
                    Ok(_) if std::fs::read_link(path).ok().as_ref() != Some(target) => {
                        Err(Halt::Fail("points somewhere else now; left".into()))
                    }
                    Ok(_) => std::fs::remove_file(path)
                        .map(|_| "removed".into())
                        .map_err(fail),
                }
            }
            Op::RemoveTree { path } | Op::RemoveRecords { path } => {
                self.allowed(path)?;
                if !self.tree_ok(path) {
                    return Err(Halt::Fail(format!(
                        "{} is not one of TD's directories; left",
                        path.display()
                    )));
                }
                match std::fs::symlink_metadata(path) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("already gone".into()),
                    Err(e) => Err(fail(e)),
                    Ok(m) if m.file_type().is_symlink() => {
                        Err(Halt::Fail("is a link; left".into()))
                    }
                    Ok(m) if !m.is_dir() => std::fs::remove_file(path)
                        .map(|_| "removed".into())
                        .map_err(fail),
                    Ok(_) => std::fs::remove_dir_all(path)
                        .map(|_| "removed".into())
                        .map_err(fail),
                }
            }
            Op::RemoveLooseAssets { dir, demos_only } => {
                self.allowed(dir)?;
                // Only files whose bytes still hash to what TD recorded under
                // that name when it wrote them; a TD-like name is not enough.
                let manifest = dir.join(ASSET_MANIFEST);
                // Trusted only as a regular file this account owns and nobody
                // else can write: in a shared /tmp another user could plant one.
                let recorded = read_receipt(&manifest);
                let uid = unsafe { libc::getuid() };
                let (mut n, mut unproven, mut failed) = (0, 0, Vec::new());
                for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let wanted = if *demos_only {
                        demo_layout(&name)
                    } else {
                        loose_asset(&name)
                    };
                    let mine = std::fs::symlink_metadata(e.path()).is_ok_and(|m| {
                        m.is_file() && std::os::unix::fs::MetadataExt::uid(&m) == uid
                    });
                    if !wanted || !mine {
                        continue;
                    }
                    let proven = known::sha256_file(&e.path())
                        .is_ok_and(|h| recorded.iter().any(|(rh, rn)| *rh == h && *rn == name));
                    if !proven {
                        unproven += 1;
                        continue;
                    }
                    match std::fs::remove_file(e.path()) {
                        Ok(()) => n += 1,
                        Err(err) => failed.push(format!("{name}: {err}")),
                    }
                }
                // The receipt goes once nothing it names is left.
                if recorded.iter().all(|(_, rn)| !dir.join(rn).exists()) && !recorded.is_empty() {
                    let _ = std::fs::remove_file(&manifest);
                }
                let kept = if unproven > 0 {
                    format!("; {unproven} kept: nothing records that TD wrote them (in a runtime directory they go at logout)")
                } else {
                    String::new()
                };
                if failed.is_empty() {
                    Ok(format!("{n} file(s) removed{kept}"))
                } else {
                    Err(Halt::Fail(format!(
                        "{n} removed; could not remove {}{kept}",
                        failed.join(", ")
                    )))
                }
            }
            Op::RemoveDirIfEmpty { path } => {
                self.allowed(path)?;
                match std::fs::remove_dir(path) {
                    Ok(()) => Ok("removed".into()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("already gone".into()),
                    Err(_) => Err(Halt::Skip("kept: something else is still in it".into())),
                }
            }
            Op::RefreshCaches {
                applications,
                icons,
            } => {
                self.allowed(applications)?;
                self.allowed(icons)?;
                let mut said = Vec::new();
                let mut failed = false;
                for argv in [
                    vec![
                        "update-desktop-database".to_string(),
                        applications.to_string_lossy().into_owned(),
                    ],
                    vec![
                        "gtk-update-icon-cache".into(),
                        "-f".into(),
                        "-t".into(),
                        icons.to_string_lossy().into_owned(),
                    ],
                ] {
                    match self.fx.run(&argv, None) {
                        Ok(_) => said.push(format!("{} ran", argv[0])),
                        Err(e)
                            if e.starts_with("not installed") || e.contains("is not installed") =>
                        {
                            said.push(format!("{}: not installed, nothing to refresh", argv[0]))
                        }
                        Err(e) => {
                            failed = true;
                            said.push(format!("{}: {e}", argv[0]));
                        }
                    }
                }
                if failed {
                    Err(Halt::Fail(said.join("; ")))
                } else {
                    Ok(said.join("; "))
                }
            }
        }
    }

    /// Edit a shared file: unchanged since the plan, a copy kept first, the new
    /// bytes written to a new private file beside it with the original's mode,
    /// the original checked once more, then renamed over it. No backup is left
    /// beside the person's file; the copy lives in the removal directory.
    fn edit(
        &mut self,
        file: &Path,
        want: &str,
        f: impl Fn(&str) -> Result<Option<(String, String)>, String>,
    ) -> Result<String, Halt> {
        let meta = std::fs::symlink_metadata(file).map_err(|e| Halt::Fail(e.to_string()))?;
        if !meta.is_file() {
            return Err(Halt::Fail("not a regular file; left".into()));
        }
        let bytes = std::fs::read(file).map_err(|e| Halt::Fail(e.to_string()))?;
        if known::sha256_bytes(&bytes) != want {
            return Err(Halt::Fail("changed since the plan; left".into()));
        }
        let text = String::from_utf8(bytes).map_err(|_| Halt::Fail("not UTF-8; left".into()))?;
        let Some((new, said)) = f(&text).map_err(Halt::Fail)? else {
            return Ok("nothing of TD's left in it".into());
        };
        self.keep_copy(file)?;
        let dir = file
            .parent()
            .ok_or_else(|| Halt::Fail("no parent directory".into()))?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp = dir.join(format!(
            ".{}.td-uninstall-{}-{nonce:x}",
            file.file_name().unwrap_or_default().to_string_lossy(),
            std::process::id()
        ));
        let written = (|| -> std::io::Result<()> {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&tmp)?;
            f.write_all(new.as_bytes())?;
            f.sync_all()?;
            std::fs::set_permissions(&tmp, meta.permissions())?;
            Ok(())
        })();
        if let Err(e) = written {
            let _ = std::fs::remove_file(&tmp);
            return Err(Halt::Fail(e.to_string()));
        }
        let still = std::fs::symlink_metadata(file).is_ok_and(|m| m.is_file())
            && known::sha256_file(file).ok().as_deref() == Some(want);
        if !still {
            let _ = std::fs::remove_file(&tmp);
            return Err(Halt::Fail("changed while being edited; left".into()));
        }
        std::fs::rename(&tmp, file).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            Halt::Fail(e.to_string())
        })?;
        Ok(said)
    }

    fn tree_ok(&self, p: &Path) -> bool {
        let r = self.roots;
        let mut allowed = r.td_dirs();
        allowed.extend(host_dirs(r));
        allowed.push(r.home.join(".config/terminal-delight"));
        allowed.push(r.home.join(".local/state/terminal-delight"));
        let forbidden = [&r.home, &r.config_base, &r.data_base, &r.cache_base];
        !forbidden.iter().any(|f| p == f.as_path()) && allowed.iter().any(|a| p.starts_with(a))
    }

    /// The executables this run may act on: for a fixture, only those that
    /// resolve inside its home, so a fixture's link to a live build cannot
    /// make the live build a target.
    fn confined(&self, exes: &[PathBuf]) -> Vec<PathBuf> {
        let home =
            std::fs::canonicalize(&self.roots.home).unwrap_or_else(|_| self.roots.home.clone());
        let account = self.fx.account_home();
        let mut v: Vec<PathBuf> = exes
            .iter()
            .flat_map(|e| {
                let c = std::fs::canonicalize(e).unwrap_or_else(|_| e.clone());
                [e.clone(), c]
            })
            .filter(|e| account || e.starts_with(&home))
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// Close TD's windows and relays: every process whose executable is one of
    /// the plan's, never by name, never this worker. TERM, ten seconds, then
    /// KILL, each sent only while the pid still runs that executable. Anything
    /// still running afterwards, any TD process the plan did not cover, or a
    /// process that may be TD and cannot be read blocks the file removals.
    fn stop(&mut self, exes: &[PathBuf], builds: &Path) -> Result<String, Halt> {
        let me = self.fx.self_pid();
        let exes = self.confined(exes);
        let builds = builds.to_path_buf();
        let ours = |p: &Proc| p.pid != me && is_td_exe(&p.exe, p.kind, &exes, &builds);
        let targets: Vec<(u32, PathBuf)> = self
            .fx
            .processes()
            .iter()
            .filter(|p| ours(p))
            .map(|p| (p.pid, p.exe.clone()))
            .collect();
        for (pid, exe) in &targets {
            self.fx.signal(*pid, libc::SIGTERM, exe);
        }
        let alive = |fx: &mut dyn Effects| -> Vec<(u32, PathBuf)> {
            let live = fx.processes();
            targets
                .iter()
                .filter(|(pid, exe)| live.iter().any(|p| p.pid == *pid && p.exe == *exe))
                .cloned()
                .collect()
        };
        let mut killed = false;
        for round in 0..120 {
            if alive(self.fx).is_empty() {
                break;
            }
            if round == 100 {
                for (pid, exe) in alive(self.fx) {
                    self.fx.signal(pid, libc::SIGKILL, &exe);
                }
                killed = true;
            }
            self.fx.sleep(Duration::from_millis(100));
        }
        let left = alive(self.fx);
        if !left.is_empty() {
            return Err(Halt::Fail(format!(
                "still running: {:?}",
                left.iter().map(|(p, _)| p).collect::<Vec<_>>()
            )));
        }
        // A last look: anything started from the plan's executables since, and
        // any other TD this run can see, keeps TD's files in place.
        let account = self.fx.account_home();
        let home =
            std::fs::canonicalize(&self.roots.home).unwrap_or_else(|_| self.roots.home.clone());
        let blockers: Vec<String> = self
            .fx
            .processes()
            .into_iter()
            .filter(|p| p.pid != me)
            .filter(|p| {
                if ours(p) {
                    return true;
                }
                let base = |s: &str| s.trim().rsplit('/').next().unwrap_or("").to_string();
                let named_td = |s: &str| base(s) == "terminal-delight";
                let versioned = |s: &str| base(s).starts_with("td-");
                let unreadable = p.exe.as_os_str().is_empty();
                // Where TD's builds run from, but its bytes cannot be read: a
                // TD nobody can rule out, so it is neither signalled nor ignored.
                let unproven = !unreadable
                    && p.kind == ExeKind::Unknown
                    && is_td_exe(&p.exe, ExeKind::Build, &exes, &builds);
                // A process whose executable cannot be read but whose name
                // says TD is a TD nobody can rule out, in either kind of run.
                let unknown_td = unreadable
                    && (named_td(&p.comm)
                        || versioned(&p.comm)
                        || p.argv.first().is_some_and(|a| named_td(a) || versioned(a)));
                if !account {
                    return unknown_td || unproven || (!unreadable && p.exe.starts_with(&home));
                }
                let exe_td = p
                    .exe
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(named_td);
                exe_td || unknown_td || unproven
            })
            .map(|p| {
                let what = if p.exe.as_os_str().is_empty() {
                    p.comm.clone()
                } else {
                    p.exe.display().to_string()
                };
                format!("{} ({what})", p.pid)
            })
            .collect();
        if !blockers.is_empty() {
            return Err(Halt::Fail(format!(
                "another Terminal Delight is still running and was left alone: {}. Close it and run the removal again.",
                blockers.join(", ")
            )));
        }
        let n = targets.len();
        Ok(match (n, killed) {
            (0, _) => "none running".into(),
            (_, false) => format!("{n} process(es) closed"),
            (_, true) => format!("{n} process(es) closed, some only after a kill"),
        })
    }
}

/// Whether a process running `exe` is TD's: one of the plan's executables, or
/// a `td-*` build in TD's builds directory, including one that has since been
/// replaced or removed (its path then still names that directory).
pub(crate) fn is_td_exe(exe: &Path, kind: ExeKind, exes: &[PathBuf], builds: &Path) -> bool {
    // Only a running program proven to be a build is TD's to signal; an
    // AppImage, an unreadable one or anything else never is.
    if exe.as_os_str().is_empty() || kind != ExeKind::Build {
        return false;
    }
    let canon_builds = std::fs::canonicalize(builds).unwrap_or_else(|_| builds.to_path_buf());
    exes.iter().any(|e| e == exe)
        || ((exe.parent() == Some(builds) || exe.parent() == Some(canon_builds.as_path()))
            && exe
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("td-")))
}

/// TD's loose-file receipt, opened once without following a link or blocking,
/// judged on that very descriptor (a regular file this account owns that no
/// one else can write) and read from it, so the file judged is the file read.
fn read_receipt(path: &Path) -> Vec<(String, String)> {
    use std::io::Read;
    use std::os::unix::fs::MetadataExt;
    let Ok(mut f) = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    else {
        return Vec::new();
    };
    let trusted = f.metadata().is_ok_and(|m| {
        m.is_file() && m.uid() == unsafe { libc::getuid() } && m.mode() & 0o022 == 0
    });
    let mut text = String::new();
    if !trusted || f.read_to_string(&mut text).is_err() {
        return Vec::new();
    }
    text.lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(h, n)| (h.to_string(), n.to_string()))
        .collect()
}

/// `td-demo-<pid>-<nanos>.toml`, the layout a demo window is started from.
pub(crate) fn demo_layout(name: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    name.strip_prefix("td-demo-")
        .and_then(|r| r.strip_suffix(".toml"))
        .and_then(|r| r.split_once('-'))
        .is_some_and(|(a, b)| digits(a) && digits(b))
}

/// Files TD unpacks beside its runtime directory, or in the temp directory
/// when there is none, by the exact names it gives them.
pub(crate) fn loose_asset(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("terminal-delight-") else {
        return demo_layout(name);
    };
    let simple = |r: &str, ext: &str| {
        r.strip_suffix(ext).is_some_and(|s| {
            !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
    };
    matches!(
        rest,
        "robot.png"
            | "mark.png"
            | "blinker.png"
            | "pace-flame.svg"
            | "concur-stamp.svg"
            | "ping.mp3"
    ) || rest
        .strip_prefix("glyph-")
        .is_some_and(|r| simple(r, ".svg"))
        || rest
            .strip_prefix("prop-")
            .is_some_and(|r| simple(r, ".png"))
        || rest
            .strip_prefix("scene-")
            .is_some_and(|r| simple(r, ".webp"))
}

pub(crate) fn summary(op: &Op) -> String {
    match op {
        Op::Export { dest } => format!("export to {}", dest.display()),
        Op::DefaultTerminalOff { list, .. } => {
            format!("restore the default terminal in {}", list.display())
        }
        Op::CheckDefaultTerminal { list } => format!(
            "check TD is no longer a default terminal in {}",
            list.display()
        ),
        Op::EditSettings { file, .. } => format!("edit {}", file.display()),
        Op::McpRemove { argv, .. } => argv.join(" "),
        Op::StripKeyBlock { file, .. } => format!("edit {}", file.display()),
        Op::CloseHosts { dir } => format!("close session hosts in {}", dir.display()),
        Op::StopProcesses { exes, .. } => {
            format!("stop processes running {} TD executable(s)", exes.len())
        }
        Op::ThawScopes { sessions } => {
            format!("thaw frozen panes of {} session(s)", sessions.len())
        }
        Op::RemoveFile { path, .. } | Op::RemoveLink { path, .. } => {
            format!("remove {}", path.display())
        }
        Op::RemoveBuild { path } => format!("remove build {}", path.display()),
        Op::RemoveTree { path } => format!("remove {}/", path.display()),
        Op::RemoveRecords { path } => format!("remove records in {}/", path.display()),
        Op::RemoveLooseAssets { dir, .. } => {
            format!("remove TD's loose files in {}", dir.display())
        }
        Op::RemoveDirIfEmpty { path } => format!("remove {}/ if empty", path.display()),
        Op::RefreshCaches { .. } => "refresh desktop and icon caches".into(),
    }
}

/// Write a new file that must not exist yet, privately, never through a link.
pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}

fn write_receipt(out: &Path, r: &Receipt) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(r).unwrap_or_default();
    // Replace an earlier receipt only by renaming a fresh file over it.
    for (name, bytes) in [
        ("receipt.md", receipt_markdown(r).into_bytes()),
        ("receipt.json", json.into_bytes()),
    ] {
        let tmp = out.join(format!(".{name}.{}", std::process::id()));
        let _ = std::fs::remove_file(&tmp);
        write_new(&tmp, &bytes)?;
        std::fs::rename(&tmp, out.join(name))?;
    }
    Ok(())
}

pub(crate) fn receipt_markdown(r: &Receipt) -> String {
    let mut s = String::new();
    let head = match (&r.refused, r.complete) {
        (Some(_), _) => "Terminal Delight was not removed",
        (None, true) => "Terminal Delight was removed",
        (None, false) => "Terminal Delight was removed in part",
    };
    s.push_str(&format!("# {head}\n\n"));
    if let Some(why) = &r.refused {
        s.push_str(&format!("{why}\n\nNothing was changed.\n"));
        return s;
    }
    let count = |st: Status| r.outcomes.iter().filter(|o| o.status == st).count();
    s.push_str(&format!(
        "{} steps done, {} not attempted, {} failed, {} blocked. Records: {}.\n\n",
        count(Status::Done),
        count(Status::Skipped),
        count(Status::Failed),
        count(Status::Blocked),
        r.records
    ));
    s.push_str("## What happened\n\n");
    for o in &r.outcomes {
        let mark = match o.status {
            Status::Done => "done",
            Status::Skipped => "not attempted",
            Status::Failed => "FAILED",
            Status::Blocked => "blocked",
        };
        s.push_str(&format!(
            "- **{mark}** — {} — {}. {}\n",
            o.op, o.why, o.detail
        ));
    }
    if !r.reports.is_empty() {
        s.push_str("\n## Left for you\n\n");
        for p in &r.reports {
            s.push_str(&format!("- **{}** ({}) — {}", p.what, p.owner, p.why));
            if let Some(path) = &p.path {
                s.push_str(&format!(" `{}`", path.display()));
            }
            if let Some(m) = &p.manual {
                s.push_str(&format!(". To do it: `{m}`"));
            }
            s.push('\n');
        }
    }
    s.push_str(&format!(
        "\nThis receipt, the plan and a copy of every shared file before it was edited are in `{}`. Delete that directory when you no longer need it.\n",
        r.removal_dir.display()
    ));
    s
}
