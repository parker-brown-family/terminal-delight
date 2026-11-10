//! The add-a-project dialog, drawn in the composer's card.
//!
//! The model is `addproject.rs`. This file holds the dialog while it is up
//! (`Composer::adding`), its keys, the reads and runs it starts in the
//! background — a folder listed, a machine checked over ssh, a clone — and the
//! card it draws in place of the composer's own. Living inside the composer,
//! it holds the keyboard the composer holds and goes when the composer goes.
//!
//! Three steps, as in the fork it was scuttled from: where the project is (a
//! machine), which folder (a path field over the folders in it), and, from
//! either, clone or start new. Whatever is made or picked lands back on the
//! composer as its PROJECT, with MACHINE set to where it is.

use super::*;
use crate::addproject::{self as ap, Listing, NewProject};
use std::collections::HashMap;
use std::io::Read as _;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Folder rows drawn at once; the arrows and the wheel move the window.
const FOLDER_ROWS: usize = 10;

/// Machine rows drawn at once, for a long ssh config.
const MACHINE_ROWS: usize = 8;

/// What the dialog has heard about reaching a far machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Seen {
    Checking,
    Reachable,
    /// ssh's reason, already put in words for the person.
    Unreachable(String),
}

/// A row of the first step.
pub(crate) enum Place {
    /// This computer.
    Here(entry::Machine),
    /// A machine `machines.toml` lists.
    Far(entry::Machine, Seen),
    /// A host in `~/.ssh/config` that is not a machine yet.
    Host(ap::SshHost),
}

pub(crate) enum Step {
    Machines,
    /// Checking a host from `~/.ssh/config` before adding it; `then_new` when
    /// it was picked to clone or start something on.
    Connecting {
        row: usize,
        then_new: bool,
    },
    Folder(Box<Browse>),
}

/// What a folder held, as far as the dialog has heard.
enum Read {
    Pending,
    Got(Listing),
    /// Put in words for the person already.
    Failed(String),
}

/// A run under way: a folder made, a clone, a repository started.
struct Work {
    /// What the dialog says while it runs.
    label: String,
    /// The program's last line of progress.
    progress: Arc<Mutex<String>>,
    /// The running program, for Esc to stop.
    child: Arc<Mutex<Option<std::process::Child>>>,
    stopped: Arc<AtomicBool>,
    /// Where a half-made folder may be left if it is stopped.
    target: String,
    _task: gpui::Task<()>,
    _tick: gpui::Task<()>,
}

/// A run dropped before it finished (the composer closed under it) takes its
/// program down with it, rather than leaving a clone going that nobody will
/// hear the end of. A finished run's program is already gone from the slot.
impl Drop for Work {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            if let Some(child) = child.as_mut() {
                let _ = child.kill();
            }
        }
    }
}

/// The folder step.
pub(crate) struct Browse {
    machine: entry::Machine,
    field: EditBuffer,
    /// By folder, as the field spells it (`~/Work/`).
    reads: HashMap<String, Read>,
    /// The highlighted row, `..` included; none until an arrow is pressed.
    at: Option<usize>,
    /// The first row drawn.
    top: usize,
    /// Still on the folder it opened on, which is swapped for home when it is
    /// not there.
    default: bool,
    /// The clone-or-start field, while it is open.
    new: Option<EditBuffer>,
    /// Opened from the machines straight onto that field, so closing it goes
    /// back to them rather than to a folder list the person never saw.
    from_machines: bool,
    work: Option<Work>,
    /// What the dialog has to say, and whether it is a failure: failures wear
    /// the skin's danger ink, as the side panel's do, and guidance the accent.
    note: Option<(String, bool)>,
    tasks: Vec<gpui::Task<()>>,
}

/// A row under the path field.
struct Row {
    label: String,
    /// What the field becomes when the row is opened.
    target: String,
    git: bool,
    up: bool,
}

impl Browse {
    fn split(&self) -> (String, String) {
        ap::split_field(&self.field.text())
    }

    fn listing(&self, dir: &str) -> Option<&Listing> {
        match self.reads.get(dir) {
            Some(Read::Got(l)) => Some(l),
            _ => None,
        }
    }

    /// `..`, then the folders that start with what is typed after the last
    /// `/`. The way up is worked out from the field while it can be (so
    /// `~/Work/` goes up to `~/`), and from the machine's own answer past
    /// that.
    fn rows(&self) -> Vec<Row> {
        let (dir, partial) = self.split();
        let listing = self.listing(&dir);
        let up = ap::parent_field(&dir)
            .filter(|_| dir != "~/")
            .or_else(|| match listing {
                Some(Listing::At { path, .. }) => ap::parent_field(path),
                _ => None,
            });
        let mut rows = Vec::new();
        if let Some(target) = up {
            rows.push(Row {
                label: "..".into(),
                target,
                git: false,
                up: true,
            });
        }
        if let Some(Listing::At { folders, .. }) = listing {
            for f in ap::matching(folders, &partial) {
                rows.push(Row {
                    label: f.name.clone(),
                    target: format!("{dir}{}/", f.name),
                    git: f.git,
                    up: false,
                });
            }
        }
        rows
    }

    /// Whether the folder the field names is there: known yes, known no, or
    /// not heard yet.
    fn exists(&self) -> Option<bool> {
        let (dir, partial) = self.split();
        match self.listing(&dir)? {
            Listing::Missing => Some(false),
            Listing::At { folders, .. } => {
                Some(partial.is_empty() || folders.iter().any(|f| f.name == partial))
            }
        }
    }

    /// Whether the listed folder already holds one by this name.
    fn holds(&self, name: &str) -> Option<bool> {
        let (dir, _) = self.split();
        match self.listing(&dir)? {
            Listing::Missing => Some(false),
            Listing::At { folders, .. } => Some(folders.iter().any(|f| f.name == name)),
        }
    }

    fn set_field(&mut self, text: &str) {
        self.field = EditBuffer::at_end(text);
        self.at = None;
        self.top = 0;
        self.default = false;
        self.note = None;
    }

    /// Keep the highlighted row inside the drawn window.
    fn follow(&mut self) {
        if let Some(at) = self.at {
            if at < self.top {
                self.top = at;
            } else if at >= self.top + FOLDER_ROWS {
                self.top = at + 1 - FOLDER_ROWS;
            }
        }
    }
}

/// The dialog's state while it is up.
pub(crate) struct AddProject {
    pub(crate) step: Step,
    places: Vec<Place>,
    at: usize,
    /// The first machine row drawn.
    top: usize,
    git_hosts: usize,
    ssh_error: Option<String>,
    note: Option<String>,
    /// The first step was skipped because this machine was the only one;
    /// Back from the folders then closes the dialog.
    skipped: bool,
    /// Where `machines.toml` and `projects.toml` are.
    config: PathBuf,
    home: PathBuf,
    control: Option<PathBuf>,
    /// The program run as ssh: `ssh`, or a test's stand-in.
    ssh: String,
    /// A repository named in the composer's sentence, for the clone field.
    offered: Option<String>,
    checks: Vec<gpui::Task<()>>,
}

impl AddProject {
    /// Keep the highlighted machine inside the drawn window.
    fn follow(&mut self) {
        if self.at < self.top {
            self.top = self.at;
        } else if self.at >= self.top + MACHINE_ROWS {
            self.top = self.at + 1 - MACHINE_ROWS;
        }
    }
}

/// What a run made or picked.
pub(crate) struct Made {
    path: String,
    git: bool,
    machine: entry::Machine,
}

/// What a key asks for, worked out before anything is borrowed to do it.
enum Act {
    None,
    Pick { row: usize, then_new: bool },
    Read,
    Add { create: bool },
    StartNew,
}

/// Run `command` on a far machine over ssh and return what it printed, or
/// what ssh or the command said on stderr.
fn ssh_run(ssh: &str, dest: &str, control: Option<&Path>, command: &str) -> Result<String, String> {
    let out = std::process::Command::new(ssh)
        .args(ap::ssh_args(dest, control))
        .arg(command)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

/// Run a program that may take a while (a clone): its last line of stderr
/// kept where the dialog shows it, the child where Esc can stop it. Returns
/// stdout, or the tail of stderr when it fails.
fn run_watched(
    mut cmd: std::process::Command,
    progress: &Mutex<String>,
    child: &Mutex<Option<std::process::Child>>,
) -> Result<String, String> {
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut running = cmd.spawn().map_err(|e| e.to_string())?;
    let stdout = running.stdout.take();
    let stderr = running.stderr.take();
    *child.lock().unwrap() = Some(running);
    let out = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut o) = stdout {
            let _ = o.read_to_string(&mut s);
        }
        s
    });
    // git rewrites one line with `\r` as it goes; each rewrite is a line here.
    let mut tail: Vec<String> = Vec::new();
    if let Some(mut e) = stderr {
        let mut buf = [0u8; 4096];
        let mut line: Vec<u8> = Vec::new();
        let take = |line: &mut Vec<u8>, tail: &mut Vec<String>| {
            let l = String::from_utf8_lossy(line).trim().to_string();
            line.clear();
            if !l.is_empty() {
                *progress.lock().unwrap() = l.clone();
                tail.push(l);
                if tail.len() > 20 {
                    tail.remove(0);
                }
            }
        };
        loop {
            match e.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    for &b in &buf[..n] {
                        if b == b'\r' || b == b'\n' {
                            take(&mut line, &mut tail);
                        } else {
                            line.push(b);
                        }
                    }
                }
            }
        }
        take(&mut line, &mut tail);
    }
    let out = out.join().unwrap_or_default();
    let status = child.lock().unwrap().take().map(|mut c| c.wait());
    match status {
        Some(Ok(s)) if s.success() => Ok(out),
        Some(Ok(_)) | None => Err(tail.join("\n")),
        Some(Err(e)) => Err(e.to_string()),
    }
}

/// List a folder on a machine.
fn list_on(
    machine: &entry::Machine,
    dir: &str,
    home: &Path,
    ssh: &str,
    control: Option<&Path>,
) -> Result<Listing, String> {
    match machine.ssh.as_deref() {
        None => ap::list_local(&ap::expand_home(dir, home)),
        Some(dest) => {
            let out = ssh_run(
                ssh,
                dest,
                control,
                &ap::remote_command(&ap::list_script(), &[dir]),
            )?;
            ap::parse_listing(&out)
        }
    }
}

/// Why a run did not land.
enum Fail {
    /// A plain Enter's add found nothing there at the moment of adding.
    Missing,
    /// Anything else, as the program said it.
    Said(String),
}

impl From<String> for Fail {
    fn from(said: String) -> Fail {
        Fail::Said(said)
    }
}

/// What a run needs to know about where it runs.
#[derive(Clone)]
struct Runner {
    machine: entry::Machine,
    home: PathBuf,
    ssh: String,
    control: Option<PathBuf>,
}

impl Runner {
    /// Run a far script, watched; or `here` on this machine.
    fn run(
        &self,
        script: &str,
        args: &[&str],
        progress: &Mutex<String>,
        child: &Mutex<Option<std::process::Child>>,
        here: impl FnOnce(&Mutex<String>, &Mutex<Option<std::process::Child>>) -> Result<Made, Fail>,
    ) -> Result<Made, Fail> {
        let Some(dest) = self.machine.ssh.as_deref() else {
            return here(progress, child);
        };
        let mut cmd = std::process::Command::new(&self.ssh);
        cmd.args(ap::ssh_args(dest, self.control.as_deref()))
            .arg(ap::remote_command(script, args));
        let out = run_watched(cmd, progress, child)?;
        let (path, git) = ap::parse_made(&out)?.ok_or(Fail::Missing)?;
        Ok(Made {
            path,
            git,
            machine: self.machine.clone(),
        })
    }

    fn local(&self, path: &Path, git: bool) -> Made {
        let p = path.to_string_lossy();
        let trimmed = p.trim_end_matches('/');
        Made {
            path: if trimmed.is_empty() {
                "/".into()
            } else {
                trimmed.to_string()
            },
            git,
            machine: self.machine.clone(),
        }
    }

    /// Make the folder (and any missing above it) when `create`, or only
    /// look, and say what is there.
    fn make(
        &self,
        target: &str,
        create: bool,
        progress: &Mutex<String>,
        child: &Mutex<Option<std::process::Child>>,
    ) -> Result<Made, Fail> {
        let how = if create { "make" } else { "look" };
        self.run(
            &ap::make_script(),
            &[target, how],
            progress,
            child,
            |_, _| {
                let path = ap::expand_home(target, &self.home);
                if create {
                    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
                } else if !path.is_dir() {
                    return Err(Fail::Missing);
                }
                Ok(self.local(&path, path.join(".git").exists()))
            },
        )
    }

    fn clone_into(
        &self,
        url: &str,
        dest: &str,
        progress: &Mutex<String>,
        child: &Mutex<Option<std::process::Child>>,
    ) -> Result<Made, Fail> {
        self.run(
            &ap::clone_script(),
            &[url, dest],
            progress,
            child,
            |progress, child| {
                let path = ap::expand_home(dest, &self.home);
                let mut cmd = std::process::Command::new("git");
                cmd.args(["clone", "--progress", "--", url])
                    .arg(&path)
                    .env("GIT_TERMINAL_PROMPT", "0");
                run_watched(cmd, progress, child)?;
                Ok(self.local(&path, true))
            },
        )
    }

    fn init(
        &self,
        dest: &str,
        progress: &Mutex<String>,
        child: &Mutex<Option<std::process::Child>>,
    ) -> Result<Made, Fail> {
        self.run(
            &ap::init_script(),
            &[dest],
            progress,
            child,
            |progress, child| {
                let path = ap::expand_home(dest, &self.home);
                let full = std::fs::read_dir(&path).is_ok_and(|mut d| d.next().is_some());
                if full {
                    return Err(Fail::Said(format!("not empty: {}", path.display())));
                }
                std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
                let mut cmd = std::process::Command::new("git");
                cmd.args(["init", "-q"]).arg(&path);
                run_watched(cmd, progress, child)?;
                Ok(self.local(&path, true))
            },
        )
    }
}

/// ssh's complaint, in words for the person: an untrusted key gets the one
/// step that fixes it.
fn trouble_text(e: &lang::Entry, host: &str, stderr: &str) -> String {
    match ap::ssh_trouble(stderr) {
        ap::SshTrouble::KeyUnknown => lang::fill(e.add_key_unknown, &[("host", host)]),
        ap::SshTrouble::KeyChanged(line) => {
            lang::fill(e.add_key_changed, &[("host", host), ("error", &line)])
        }
        ap::SshTrouble::Other(line) => line,
    }
}

impl Workspace {
    fn adding(&mut self) -> Option<&mut AddProject> {
        self.composer.as_mut()?.adding.as_mut()
    }

    /// Open the dialog over the composer: Ctrl+O, the ＋ beside PROJECT, or
    /// the first row of its list.
    pub(crate) fn add_project_open(&mut self, cx: &mut Context<Self>) {
        let (config, home) = (instance::config_dir(), session::home_dir());
        self.add_project_open_with(config, home, "ssh".into(), cx);
    }

    /// The same, with where the config is, where home is and what runs as
    /// ssh named, so a test can keep away from the person's own.
    pub(crate) fn add_project_open_with(
        &mut self,
        config: PathBuf,
        home: PathBuf,
        ssh: String,
        cx: &mut Context<Self>,
    ) {
        let e = self.lang.strings().entry;
        let Some(c) = self.composer.as_mut() else {
            return;
        };
        let (hosts, ssh_error) = match ap::read_ssh_hosts(&home) {
            Ok(h) => (h, None),
            Err(err) => (
                ap::SshHosts::default(),
                Some(lang::fill(e.add_ssh_error, &[("error", &err)])),
            ),
        };
        let mut places: Vec<Place> = c
            .machines
            .iter()
            .map(|m| {
                if m.is_far() {
                    Place::Far(m.clone(), Seen::Checking)
                } else {
                    Place::Here(m.clone())
                }
            })
            .collect();
        places.extend(
            ap::hosts_not_added(&hosts.hosts, &c.machines)
                .into_iter()
                .map(Place::Host),
        );
        let only_here = places.len() == 1;
        let here = c.machines.first().cloned();
        c.adding = Some(AddProject {
            step: Step::Machines,
            places,
            at: 0,
            top: 0,
            git_hosts: hosts.git_hosts,
            ssh_error,
            note: None,
            skipped: only_here,
            config,
            home,
            control: ap::control_dir(),
            ssh,
            offered: ap::first_repository(&c.text.text()),
            checks: Vec::new(),
        });
        c.note = None;
        let far: Vec<usize> = c
            .adding
            .as_ref()
            .map(|a| {
                a.places
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| matches!(p, Place::Far(..)))
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default();
        for row in far {
            self.add_project_check(row, cx);
        }
        // Nothing to choose between: straight to the folders.
        if let (true, Some(here)) = (only_here, here) {
            self.add_project_browse(here, false, cx);
        }
        cx.notify();
    }

    /// Ask a far machine, or a host about to become one, whether it answers.
    fn add_project_check(&mut self, row: usize, cx: &mut Context<Self>) {
        let Some(a) = self.adding() else {
            return;
        };
        let dest = match a.places.get(row) {
            Some(Place::Far(m, _)) => m.ssh.clone(),
            Some(Place::Host(h)) => Some(h.alias().to_string()),
            _ => None,
        };
        let Some(dest) = dest else {
            return;
        };
        let (ssh, control) = (a.ssh.clone(), a.control.clone());
        let task = cx.spawn(async move |this, cx| {
            let said = cx
                .background_spawn(async move {
                    ssh_run(&ssh, &dest, control.as_deref(), "true").map(|_| ())
                })
                .await;
            this.update(cx, |ws, cx| ws.add_project_checked(row, said, cx))
                .ok();
        });
        a.checks.push(task);
    }

    fn add_project_checked(
        &mut self,
        row: usize,
        said: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        let e = self.lang.strings().entry;
        let Some(a) = self.adding() else {
            return;
        };
        match a.places.get_mut(row) {
            Some(Place::Far(m, seen)) => {
                let host = m.ssh.clone().unwrap_or_default();
                *seen = match &said {
                    Ok(()) => Seen::Reachable,
                    Err(err) => Seen::Unreachable(trouble_text(e, &host, err)),
                };
            }
            Some(Place::Host(h)) => {
                let alias = h.alias().to_string();
                let Step::Connecting { row: r, then_new } = a.step else {
                    return;
                };
                if r != row {
                    return;
                }
                match said {
                    Err(err) => {
                        a.step = Step::Machines;
                        a.note = Some(trouble_text(e, &alias, &err));
                    }
                    Ok(()) => match ap::add_machine(&a.config, &alias) {
                        Err(err) => {
                            a.step = Step::Machines;
                            a.note = Some(lang::fill(
                                e.add_machine_failed,
                                &[("alias", &alias), ("error", &err)],
                            ));
                        }
                        Ok(machine) => {
                            a.places[row] = Place::Far(machine.clone(), Seen::Reachable);
                            if let Some(c) = self.composer.as_mut() {
                                if !c.machines.iter().any(|m| m.name == machine.name) {
                                    c.machines.push(machine.clone());
                                }
                            }
                            self.add_project_browse(machine, then_new, cx);
                        }
                    },
                }
            }
            _ => {}
        }
        cx.notify();
    }

    /// Pick a row of the first step: a machine opens its folders, a host is
    /// checked and added first.
    fn add_project_pick(&mut self, row: usize, then_new: bool, cx: &mut Context<Self>) {
        let Some(a) = self.adding() else {
            return;
        };
        a.note = None;
        let machine = match a.places.get(row) {
            Some(Place::Here(m)) | Some(Place::Far(m, _)) => Some(m.clone()),
            Some(Place::Host(_)) => None,
            None => return,
        };
        match machine {
            Some(m) => self.add_project_browse(m, then_new, cx),
            None => {
                a.step = Step::Connecting { row, then_new };
                self.add_project_check(row, cx);
            }
        }
        cx.notify();
    }

    fn add_project_browse(
        &mut self,
        machine: entry::Machine,
        then_new: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(a) = self.adding() else {
            return;
        };
        let new = then_new.then(|| EditBuffer::at_end(a.offered.as_deref().unwrap_or("")));
        a.step = Step::Folder(Box::new(Browse {
            machine,
            field: EditBuffer::at_end("~/Work/"),
            reads: HashMap::new(),
            at: None,
            top: 0,
            default: true,
            new,
            from_machines: then_new,
            work: None,
            note: None,
            tasks: Vec::new(),
        }));
        self.add_project_read(cx);
    }

    /// Read the folder the field is in, once.
    fn add_project_read(&mut self, cx: &mut Context<Self>) {
        let e = self.lang.strings().entry;
        let Some(a) = self.adding() else {
            return;
        };
        let (ssh, control, home) = (a.ssh.clone(), a.control.clone(), a.home.clone());
        let Step::Folder(b) = &mut a.step else {
            return;
        };
        let (dir, _) = b.split();
        if b.reads.contains_key(&dir) {
            return;
        }
        b.reads.insert(dir.clone(), Read::Pending);
        let machine = b.machine.clone();
        let host = machine.ssh.clone().unwrap_or_default();
        let d = dir.clone();
        let task = cx.spawn(async move |this, cx| {
            let read = cx
                .background_spawn(
                    async move { list_on(&machine, &d, &home, &ssh, control.as_deref()) },
                )
                .await
                .map_err(|err| {
                    if host.is_empty() {
                        lang::fill(e.add_read_failed, &[("dir", &dir), ("error", &err)])
                    } else {
                        let said = trouble_text(e, &host, &err);
                        lang::fill(e.add_read_failed, &[("dir", &dir), ("error", &said)])
                    }
                });
            this.update(cx, |ws, cx| ws.add_project_listed(dir, read, cx))
                .ok();
        });
        b.tasks.push(task);
    }

    fn add_project_listed(
        &mut self,
        dir: String,
        read: Result<Listing, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(a) = self.adding() else {
            return;
        };
        let Step::Folder(b) = &mut a.step else {
            return;
        };
        // `~/Work/` is where the scan looks first, so the folders open there;
        // a machine without one opens on home instead of on nothing.
        let swap = b.default && dir == "~/Work/" && matches!(read, Ok(Listing::Missing));
        b.reads.insert(
            dir,
            match read {
                Ok(l) => Read::Got(l),
                Err(err) => Read::Failed(err),
            },
        );
        if swap {
            b.set_field("~/");
            self.add_project_read(cx);
        }
        cx.notify();
    }

    /// Add the folder the field names. Enter adds only a folder that is
    /// there; making one takes Ctrl+Enter or the button, so a half-typed name
    /// never becomes a folder by accident.
    fn add_project_add(&mut self, create: bool, cx: &mut Context<Self>) {
        let e = self.lang.strings().entry;
        let Some(a) = self.adding() else {
            return;
        };
        let runner = Runner {
            machine: entry::Machine::here(None, ""),
            home: a.home.clone(),
            ssh: a.ssh.clone(),
            control: a.control.clone(),
        };
        let Step::Folder(b) = &mut a.step else {
            return;
        };
        if b.work.is_some() {
            return;
        }
        let target = ap::field_target(&b.field.text());
        if !create && b.exists() != Some(true) {
            b.note = Some((lang::fill(e.add_enter_creates, &[("path", &target)]), false));
            cx.notify();
            return;
        }
        let runner = Runner {
            machine: b.machine.clone(),
            ..runner
        };
        let label = lang::fill(e.add_making, &[("path", &target)]);
        let t = target.clone();
        self.add_project_work(label, target, cx, move |progress, child| {
            runner.make(&t, create, progress, child)
        });
    }

    /// Clone or start what the clone-or-start field names, in the folder
    /// being shown.
    fn add_project_start_new(&mut self, cx: &mut Context<Self>) {
        let e = self.lang.strings().entry;
        let Some(a) = self.adding() else {
            return;
        };
        let runner = Runner {
            machine: entry::Machine::here(None, ""),
            home: a.home.clone(),
            ssh: a.ssh.clone(),
            control: a.control.clone(),
        };
        let Step::Folder(b) = &mut a.step else {
            return;
        };
        if b.work.is_some() {
            return;
        }
        let Some(field) = &b.new else {
            return;
        };
        let asked = match ap::new_project(&field.text()) {
            Ok(asked) => asked,
            Err(ap::NewTrouble::Empty) => return,
            Err(ap::NewTrouble::BadName) => {
                b.note = Some((e.add_bad_name.to_string(), true));
                cx.notify();
                return;
            }
        };
        let name = match &asked {
            NewProject::Clone { name, .. } | NewProject::Empty { name } => name.clone(),
        };
        if b.holds(&name) == Some(true) {
            b.note = Some((lang::fill(e.add_exists, &[("name", &name)]), true));
            cx.notify();
            return;
        }
        let (dir, _) = b.split();
        let dest = format!("{dir}{name}");
        let runner = Runner {
            machine: b.machine.clone(),
            ..runner
        };
        let d = dest.clone();
        match asked {
            NewProject::Clone { url, .. } => {
                let label = lang::fill(e.add_cloning, &[("path", &dest)]);
                self.add_project_work(label, dest, cx, move |progress, child| {
                    runner.clone_into(&url, &d, progress, child)
                });
            }
            NewProject::Empty { .. } => {
                let label = lang::fill(e.add_starting, &[("path", &dest)]);
                self.add_project_work(label, dest, cx, move |progress, child| {
                    runner.init(&d, progress, child)
                });
            }
        }
    }

    /// Start a run in the background, with a tick that redraws its progress
    /// line while it goes.
    fn add_project_work(
        &mut self,
        label: String,
        target: String,
        cx: &mut Context<Self>,
        job: impl FnOnce(&Mutex<String>, &Mutex<Option<std::process::Child>>) -> Result<Made, Fail>
            + Send
            + 'static,
    ) {
        let progress: Arc<Mutex<String>> = Arc::default();
        let child: Arc<Mutex<Option<std::process::Child>>> = Arc::default();
        let stopped = Arc::new(AtomicBool::new(false));
        let (p, c) = (progress.clone(), child.clone());
        let task = cx.spawn(async move |this, cx| {
            let made = cx.background_spawn(async move { job(&p, &c) }).await;
            this.update(cx, |ws, cx| ws.add_project_made(made, cx)).ok();
        });
        let tick = cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(150))
                .await;
            let running = this
                .update(cx, |ws, cx| {
                    cx.notify();
                    ws.add_project_running()
                })
                .unwrap_or(false);
            if !running {
                break;
            }
        });
        let Some(a) = self.adding() else {
            return;
        };
        if let Step::Folder(b) = &mut a.step {
            b.note = None;
            b.work = Some(Work {
                label,
                progress,
                child,
                stopped,
                target,
                _task: task,
                _tick: tick,
            });
        }
        cx.notify();
    }

    fn add_project_running(&mut self) -> bool {
        matches!(
            self.adding().map(|a| &a.step),
            Some(Step::Folder(b)) if b.work.is_some()
        )
    }

    fn add_project_made(&mut self, made: Result<Made, Fail>, cx: &mut Context<Self>) {
        let e = self.lang.strings().entry;
        let Some(a) = self.adding() else {
            return;
        };
        let Step::Folder(b) = &mut a.step else {
            return;
        };
        let work = b.work.take();
        // Esc means do not go on, even when the run reached its end before the
        // program could be stopped: nothing becomes the project behind it.
        if let Some(w) = work.as_ref().filter(|w| w.stopped.load(Ordering::Relaxed)) {
            b.note = Some((lang::fill(e.add_stopped, &[("path", &w.target)]), false));
            cx.notify();
            return;
        }
        match made {
            Ok(made) => self.add_project_take(made, cx),
            Err(Fail::Missing) => {
                let target = work.as_ref().map(|w| w.target.clone()).unwrap_or_default();
                b.note = Some((lang::fill(e.add_enter_creates, &[("path", &target)]), false));
            }
            Err(Fail::Said(err)) => {
                let host = b.machine.ssh.clone().unwrap_or_default();
                let said = if host.is_empty() {
                    ap::last_line(&err)
                } else {
                    trouble_text(e, &host, &err)
                };
                b.note = Some((lang::fill(e.add_failed, &[("error", &said)]), true));
            }
        }
        cx.notify();
    }

    /// Hand what was made or picked to the composer as its PROJECT, on its
    /// machine, and remember it if the scan cannot see it.
    fn add_project_take(&mut self, made: Made, cx: &mut Context<Self>) {
        let e = self.lang.strings().entry;
        let Some(c) = self.composer.as_mut() else {
            return;
        };
        let Some(a) = c.adding.take() else {
            return;
        };
        let far = made.machine.is_far();
        let path = PathBuf::from(&made.path);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| made.path.clone());
        let remember = if far {
            Some(ap::Added {
                path: made.path.clone(),
                machine: Some(made.machine.name.clone()),
                git: Some(made.git),
            })
        } else if !ap::scan_sees(&path, &launcher::default_roots(&a.home)) {
            Some(ap::Added {
                path: made.path.clone(),
                machine: None,
                git: None,
            })
        } else {
            None
        };
        let mut note = if far {
            lang::fill(
                e.add_added_on,
                &[("name", &name), ("machine", &made.machine.name)],
            )
        } else {
            lang::fill(e.add_added, &[("name", &name)])
        };
        if let Some(r) = remember {
            if let Err(err) = ap::remember_project(&a.config, &r) {
                note = lang::fill(e.add_remember_failed, &[("name", &name), ("error", &err)]);
            }
        }
        let project = launcher::Project {
            path,
            name,
            repo: made.git,
            machine: far.then(|| made.machine.name.clone()),
        };
        let at = match c
            .projects
            .iter()
            .position(|p| p.path == project.path && p.machine == project.machine)
        {
            Some(i) => {
                c.projects[i].repo |= project.repo;
                i
            }
            None => {
                c.projects.insert(0, project);
                0
            }
        };
        c.query.clear();
        c.project = Some(at);
        c.machine = if far {
            match c.machines.iter().position(|m| m.name == made.machine.name) {
                Some(i) => i,
                None => {
                    c.machines.push(made.machine.clone());
                    c.machines.len() - 1
                }
            }
        } else {
            0
        };
        c.focus = entry::Focus::Text;
        c.note = Some(note);
        cx.notify();
    }

    /// Esc, one layer at a time: a run is stopped, the clone field closes (to
    /// the machines when it was opened from them), a highlight clears, the
    /// folders go back to the machines, and the machines back to the
    /// composer. No window needed: the caller redraws.
    pub(crate) fn add_project_escape(&mut self) {
        let Some(c) = self.composer.as_mut() else {
            return;
        };
        let Some(a) = c.adding.as_mut() else {
            return;
        };
        match &mut a.step {
            Step::Machines => c.adding = None,
            Step::Connecting { .. } => a.step = Step::Machines,
            Step::Folder(b) => {
                if let Some(w) = &b.work {
                    w.stopped.store(true, Ordering::Relaxed);
                    if let Some(child) = w.child.lock().unwrap().as_mut() {
                        let _ = child.kill();
                    }
                } else if b.new.is_some() {
                    if b.from_machines {
                        a.step = Step::Machines;
                    } else {
                        b.new = None;
                        b.note = None;
                    }
                } else if b.at.is_some() {
                    b.at = None;
                } else if a.skipped {
                    c.adding = None;
                } else {
                    a.step = Step::Machines;
                }
            }
        }
    }

    /// The dialog's keys; it takes every one while it is up.
    pub(crate) fn add_project_key(
        &mut self,
        ks: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let m = ks.modifiers;
        let key = ks.key.as_str();
        if key == "escape" {
            self.add_project_escape();
            cx.notify();
            return true;
        }
        let pasted = (key == "v" && m.control)
            .then(|| cx.read_from_clipboard().and_then(|i| i.text()))
            .flatten();
        let Some(a) = self.adding() else {
            return false;
        };
        let act = match &mut a.step {
            Step::Machines => {
                let last = a.places.len().saturating_sub(1);
                match key {
                    "up" => {
                        a.at = a.at.saturating_sub(1);
                        a.follow();
                        Act::None
                    }
                    "down" => {
                        a.at = (a.at + 1).min(last);
                        a.follow();
                        Act::None
                    }
                    "enter" => Act::Pick {
                        row: a.at,
                        then_new: false,
                    },
                    "n" if !m.alt => Act::Pick {
                        row: a.at,
                        then_new: true,
                    },
                    _ => Act::None,
                }
            }
            Step::Connecting { .. } => Act::None,
            Step::Folder(b) => {
                if b.work.is_some() {
                    Act::None
                } else if let Some(field) = b.new.as_mut() {
                    match key {
                        "enter" => Act::StartNew,
                        _ if pasted.is_some() => {
                            let text = pasted.unwrap_or_default();
                            field.insert_capped(text.trim(), 2_000);
                            b.note = None;
                            Act::None
                        }
                        _ => {
                            field.apply(key, &m, ks.key_char.as_deref(), 2_000);
                            b.note = None;
                            Act::None
                        }
                    }
                } else {
                    let rows = b.rows();
                    match key {
                        "up" => {
                            b.at = match b.at {
                                None | Some(0) => None,
                                Some(i) => Some(i - 1),
                            };
                            b.follow();
                            Act::None
                        }
                        "down" => {
                            if !rows.is_empty() {
                                b.at = Some(b.at.map_or(0, |i| (i + 1).min(rows.len() - 1)));
                            }
                            b.follow();
                            Act::None
                        }
                        "pageup" => {
                            b.at = b.at.map(|i| i.saturating_sub(FOLDER_ROWS));
                            b.follow();
                            Act::None
                        }
                        "pagedown" => {
                            if !rows.is_empty() {
                                let to = b.at.map_or(0, |i| i + FOLDER_ROWS);
                                b.at = Some(to.min(rows.len() - 1));
                            }
                            b.follow();
                            Act::None
                        }
                        "enter" if m.control => Act::Add { create: true },
                        "enter" => match b.at.and_then(|i| rows.get(i)) {
                            Some(row) => {
                                let t = row.target.clone();
                                b.set_field(&t);
                                Act::Read
                            }
                            None => Act::Add { create: false },
                        },
                        "tab" => {
                            // The highlighted row, or the first folder that
                            // matches what is typed.
                            let pick =
                                b.at.and_then(|i| rows.get(i))
                                    .or_else(|| rows.iter().find(|r| !r.up));
                            match pick {
                                Some(row) => {
                                    let t = row.target.clone();
                                    b.set_field(&t);
                                    Act::Read
                                }
                                None => Act::None,
                            }
                        }
                        "n" if m.control => {
                            let offered = a.offered.clone().unwrap_or_default();
                            b.new = Some(EditBuffer::at_end(&offered));
                            b.note = None;
                            Act::None
                        }
                        _ if pasted.is_some() => {
                            let text = pasted.unwrap_or_default();
                            let text = text.trim();
                            // A whole path pasted replaces the field; a word
                            // goes where the caret is.
                            if text.starts_with('/') || text.starts_with('~') {
                                b.set_field(text);
                            } else {
                                b.field.insert_capped(text, 4_096);
                                b.at = None;
                                b.top = 0;
                            }
                            Act::Read
                        }
                        _ => {
                            let before = b.field.text();
                            b.field.apply(key, &m, ks.key_char.as_deref(), 4_096);
                            if b.field.text() != before {
                                b.at = None;
                                b.top = 0;
                                b.default = false;
                                b.note = None;
                            }
                            Act::Read
                        }
                    }
                }
            }
        };
        match act {
            Act::None => {}
            Act::Pick { row, then_new } => self.add_project_pick(row, then_new, cx),
            Act::Read => self.add_project_read(cx),
            Act::Add { create } => self.add_project_add(create, cx),
            Act::StartNew => self.add_project_start_new(cx),
        }
        window.focus(&self.focus_handle, cx);
        cx.notify();
        true
    }

    /// One line of an edit field, with its caret, or its placeholder.
    fn add_project_field(
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        field: &EditBuffer,
        placeholder: &str,
        on: bool,
    ) -> gpui::Div {
        let text = field.text();
        let caret = gpui::HighlightStyle {
            background_color: Some(th.cursor),
            color: Some(th.bg),
            ..Default::default()
        };
        let line: AnyElement = if text.is_empty() && !on {
            div()
                .text_color(th.text.alpha(0.38))
                .child(placeholder.to_string())
                .into_any_element()
        } else if on {
            let (shown, span) = docview::notes_ui::caret_span(&text, field.caret());
            let mut row = div().flex().flex_row();
            row = row.child(gpui::StyledText::new(shown).with_highlights(vec![(span, caret)]));
            if text.is_empty() {
                row = row.child(
                    div()
                        .text_color(th.text.alpha(0.38))
                        .child(placeholder.to_string()),
                );
            }
            row.into_any_element()
        } else {
            div().child(text).into_any_element()
        };
        div()
            .w_full()
            .flex_shrink_0()
            .px(px(10. * s))
            .py(px(7. * s))
            .rounded(sk.capped(8.))
            .bg(darken(th.bg, 0.4).alpha(0.85))
            .border(sk.box_border(1.))
            .border_color(if on {
                th.accent.alpha(0.6)
            } else {
                sk.ink.edge_rest
            })
            .text_size(sk.text_px(12.5 * s))
            .text_color(th.text)
            .overflow_hidden()
            .whitespace_nowrap()
            .child(line)
    }

    /// A folder row's measure with nothing in it.
    fn add_project_blank(sk: &skin::Skin, s: f32) -> gpui::Div {
        div()
            .w_full()
            .px(px(10. * s))
            .py(px(3. * s))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(9. * s))
            .child(
                div()
                    .w(px(16. * s))
                    .flex_none()
                    .text_size(sk.text_px(12. * s))
                    .child("\u{a0}"),
            )
            .child(div().text_size(sk.text_px(12.5 * s)).child("\u{a0}"))
    }

    /// A row of the dialog's lists: a glyph, a name, a line under it, a ›.
    #[allow(clippy::too_many_arguments)]
    fn add_project_row(
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        id: SharedString,
        glyph: &str,
        title: String,
        detail: Option<(String, Option<Hsla>)>,
        lit: bool,
        dim: bool,
    ) -> gpui::Stateful<gpui::Div> {
        let sel = id.clone();
        div()
            .id(id)
            .debug_selector(move || sel.to_string())
            .w_full()
            .px(px(10. * s))
            .py(px(if detail.is_some() { 5. } else { 3. } * s))
            .cursor_pointer()
            .when(lit, |d| d.bg(th.accent.alpha(0.22)))
            .hover(|st| st.bg(hsla(0., 0., 1., 0.06)))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(9. * s))
            .child(
                div()
                    .w(px(16. * s))
                    .flex_none()
                    .text_size(sk.text_px(12. * s))
                    .text_color(sk.ink.ink_faint)
                    .child(glyph.to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(sk.text_px(12.5 * s))
                            .text_color(if dim {
                                th.text.alpha(0.55)
                            } else if lit {
                                th.text
                            } else {
                                th.text.alpha(0.85)
                            })
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(title),
                    )
                    .children(detail.map(|(text, dot)| {
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(5. * s))
                            .text_size(sk.text_px(9.5 * s))
                            .text_color(sk.ink.ink_faint)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .children(dot.map(|c| div().text_color(c).child("\u{25cf}")))
                            .child(text)
                    })),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(sk.text_px(11. * s))
                    .text_color(sk.ink.ink_faint)
                    .child("\u{203a}"),
            )
    }

    /// The dialog's card, drawn where the composer's column would be.
    pub(crate) fn render_add_project(
        &self,
        th: &theme::Theme,
        sk: &skin::Skin,
        s: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let c = self.composer.as_ref()?;
        let a = c.adding.as_ref()?;
        let e = self.lang.strings().entry;
        let small = |text: String, color: Hsla| {
            div()
                .text_size(sk.text_px(10.5 * s))
                .text_color(color)
                .child(text)
        };
        // The key hint at the foot, the size and ink the composer's and the
        // relate dialog's hints have.
        let hint = |text: &str| {
            div()
                .text_size(sk.text_px(9.5 * s))
                .text_color(sk.ink.ink_faint)
                .child(text.to_string())
        };
        let button = |id: &'static str, label: &str, primary: bool| {
            Self::bezel_btn_s(sk, label, primary, s * 0.95)
                .id(id)
                .debug_selector(move || id.into())
        };
        let mut col = div()
            .id("add-project")
            .debug_selector(|| "add-project".into())
            // A width of its own rather than a share of the page: measured at
            // the page's width, its wrapped lines came out one short, and the
            // lists were squeezed to make up the difference.
            .w(px(640. * s))
            .max_w(gpui::relative(1.))
            .flex()
            .flex_col()
            .gap(px(10. * s))
            .p(px(22. * s))
            .map(|d| crate::entry_view::dialog_card(th, sk, d))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    window.focus(&ws.focus_handle, cx);
                }),
            );
        let title = |text: String| {
            div()
                .text_size(sk.text_px(20. * s))
                .text_color(th.text)
                .child(text)
        };
        let sub = |text: String| {
            div()
                .text_size(sk.text_px(11. * s))
                .text_color(sk.ink.ink_dim)
                .child(text)
        };
        let list_box = || {
            div()
                .w_full()
                .flex_shrink_0()
                .flex()
                .flex_col()
                .rounded(sk.capped(8.))
                .border(sk.box_border(1.))
                .border_color(sk.ink.rule)
                .bg(darken(th.bg, 0.25).alpha(0.5))
                .overflow_hidden()
        };
        let foot = || {
            div()
                .w_full()
                .flex()
                .flex_row()
                .justify_end()
                .gap(px(8. * s))
                .pt(px(4. * s))
        };
        match &a.step {
            Step::Machines | Step::Connecting { .. } => {
                if let Step::Connecting { row, .. } = &a.step {
                    let alias = match a.places.get(*row) {
                        Some(Place::Host(h)) => h.alias().to_string(),
                        _ => String::new(),
                    };
                    col = col
                        .child(title(lang::fill(e.add_connecting, &[("alias", &alias)])))
                        .child(sub(e.add_ssh_foot.to_string()))
                        .child(foot().child(
                            button("add-project-back", e.add_back, false).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                    cx.stop_propagation();
                                    ws.add_project_escape();
                                    cx.notify();
                                }),
                            ),
                        ));
                    return Some(col.into_any_element());
                }
                col = col
                    .child(title(e.add_title.to_string()))
                    .child(sub(e.add_where.to_string()));
                let mut rows = list_box()
                    .id("add-project-machines")
                    .on_scroll_wheel(cx.listener(|ws, ev: &ScrollWheelEvent, _w, cx| {
                        cx.stop_propagation();
                        let dy = f32::from(ev.delta.pixel_delta(px(20.)).y);
                        let Some(a) = ws.adding() else { return };
                        let max = a.places.len().saturating_sub(MACHINE_ROWS);
                        let steps = (dy.abs() / 20.).ceil().max(1.) as usize;
                        a.top = if dy < 0. {
                            (a.top + steps).min(max)
                        } else {
                            a.top.saturating_sub(steps)
                        };
                        cx.notify();
                    }));
                for (i, place) in a.places.iter().enumerate().skip(a.top).take(MACHINE_ROWS) {
                    let (glyph, name, detail) = match place {
                        Place::Here(m) => (
                            "\u{25a3}",
                            m.name.clone(),
                            (e.add_here.to_string(), Some(sk.ink.ok)),
                        ),
                        Place::Far(m, seen) => {
                            let ssh = m.ssh.clone().unwrap_or_default();
                            let detail = match seen {
                                Seen::Checking => (
                                    lang::fill(e.add_checking, &[("ssh", &ssh)]),
                                    Some(sk.ink.ink_faint),
                                ),
                                Seen::Reachable => (
                                    lang::fill(e.add_reachable, &[("ssh", &ssh)]),
                                    Some(sk.ink.ok),
                                ),
                                Seen::Unreachable(why) => (
                                    lang::fill(e.add_unreachable, &[("ssh", &ssh), ("error", why)]),
                                    Some(sk.ink.warn),
                                ),
                            };
                            ("\u{25c7}", m.name.clone(), detail)
                        }
                        Place::Host(h) => (
                            "\u{203a}_",
                            h.alias().to_string(),
                            (lang::fill(e.add_not_added, &[("alias", h.alias())]), None),
                        ),
                    };
                    rows = rows.child(
                        Self::add_project_row(
                            th,
                            sk,
                            s,
                            SharedString::from(format!("add-project-machine-{i}")),
                            glyph,
                            name,
                            Some(detail),
                            i == a.at,
                            false,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                if let Some(a) = ws.adding() {
                                    a.at = i;
                                }
                                ws.add_project_pick(i, false, cx);
                                window.focus(&ws.focus_handle, cx);
                            }),
                        ),
                    );
                }
                let below = a.places.len().saturating_sub(a.top + MACHINE_ROWS);
                if below > 0 {
                    rows = rows.child(
                        div()
                            .px(px(10. * s))
                            .py(px(2. * s))
                            .text_size(sk.text_px(9.5 * s))
                            .text_color(sk.ink.ink_faint)
                            .child(lang::fill(e.add_more, &[("n", &below.to_string())])),
                    );
                }
                col = col.child(rows);
                let mut notes: Vec<(String, Hsla)> = Vec::new();
                // Both are failures: a host that did not answer as it should,
                // or an ssh config that could not be read.
                if let Some(n) = &a.note {
                    notes.push((n.clone(), sk.ink.danger));
                }
                if let Some(n) = &a.ssh_error {
                    notes.push((n.clone(), sk.ink.danger));
                }
                notes.push((e.add_ssh_foot.to_string(), sk.ink.ink_faint));
                if a.git_hosts > 0 {
                    notes.push((
                        lang::fill(e.add_git_hosts, &[("n", &a.git_hosts.to_string())]),
                        sk.ink.ink_faint,
                    ));
                }
                for (text, color) in notes {
                    col = col.child(small(text, color));
                }
                col = col.child(
                    foot()
                        .child(
                            button("add-project-new", e.add_new_button, false).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    let at = ws.adding().map_or(0, |a| a.at);
                                    ws.add_project_pick(at, true, cx);
                                    window.focus(&ws.focus_handle, cx);
                                }),
                            ),
                        )
                        .child(
                            button("add-project-cancel", e.relate_cancel, false).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    if let Some(c) = ws.composer.as_mut() {
                                        c.adding = None;
                                    }
                                    window.focus(&ws.focus_handle, cx);
                                    cx.notify();
                                }),
                            ),
                        ),
                );
                col = col.child(hint(e.add_hint_machines));
            }
            Step::Folder(b) => {
                let machine = b.machine.name.clone();
                let (dir, partial) = b.split();
                if let Some(work) = &b.work {
                    let progress = work.progress.lock().unwrap().clone();
                    col = col
                        .child(title(work.label.clone()))
                        .child(
                            div()
                                .id("add-project-progress")
                                .debug_selector(|| "add-project-progress".into())
                                .text_size(sk.text_px(11. * s))
                                .text_color(sk.ink.ink_faint)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(if progress.is_empty() {
                                    "\u{2026}".to_string()
                                } else {
                                    progress
                                }),
                        )
                        .child(foot().child(
                            button("add-project-stop", e.add_stop, false).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                    cx.stop_propagation();
                                    ws.add_project_escape();
                                    cx.notify();
                                }),
                            ),
                        ))
                        .child(hint(e.add_hint_work));
                    return Some(col.into_any_element());
                }
                if let Some(field) = &b.new {
                    col = col
                        .child(title(lang::fill(e.add_new_title, &[("machine", &machine)])))
                        .child(sub(lang::fill(e.add_new_sub, &[("dir", &dir)])))
                        .child(
                            Self::add_project_field(th, sk, s, field, e.add_new_placeholder, true)
                                .id("add-project-new-field")
                                .debug_selector(|| "add-project-new-field".into()),
                        );
                    let (said, go) = match ap::new_project(&field.text()) {
                        Ok(NewProject::Clone { url, name }) => (
                            Some(lang::fill(
                                e.add_will_clone,
                                &[("url", &url), ("path", &format!("{dir}{name}"))],
                            )),
                            e.add_clone_go,
                        ),
                        Ok(NewProject::Empty { name }) => (
                            Some(lang::fill(
                                e.add_will_init,
                                &[("path", &format!("{dir}{name}"))],
                            )),
                            e.add_init_go,
                        ),
                        Err(ap::NewTrouble::Empty) => (None, e.add_clone_go),
                        Err(ap::NewTrouble::BadName) => {
                            (Some(e.add_bad_name.to_string()), e.add_clone_go)
                        }
                    };
                    if let Some(said) = said {
                        col = col.child(small(said, sk.ink.ink_dim));
                    }
                    if let Some((n, failed)) = &b.note {
                        let ink = if *failed { sk.ink.danger } else { th.accent };
                        col = col.child(small(n.clone(), ink));
                    }
                    col = col
                        .child(
                            foot()
                                .child(button("add-project-back", e.add_back, false).on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        ws.add_project_escape();
                                        window.focus(&ws.focus_handle, cx);
                                        cx.notify();
                                    }),
                                ))
                                .child(button("add-project-start", go, true).on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        ws.add_project_start_new(cx);
                                        window.focus(&ws.focus_handle, cx);
                                    }),
                                )),
                        )
                        .child(hint(e.add_hint_new));
                    return Some(col.into_any_element());
                }
                col = col
                    .child(title(lang::fill(e.add_on, &[("machine", &machine)])))
                    .child(sub(e.add_pick.to_string()))
                    .child(
                        Self::add_project_field(th, sk, s, &b.field, "~/", true)
                            .id("add-project-field")
                            .debug_selector(|| "add-project-field".into()),
                    );
                let rows = b.rows();
                let total = rows.len();
                let mut drawn = total.saturating_sub(b.top).min(FOLDER_ROWS);
                let mut list = list_box()
                    .id("add-project-folders")
                    .on_scroll_wheel(cx.listener(|ws, ev: &ScrollWheelEvent, _w, cx| {
                        cx.stop_propagation();
                        let dy = f32::from(ev.delta.pixel_delta(px(20.)).y);
                        let Some(a) = ws.adding() else { return };
                        let Step::Folder(b) = &mut a.step else { return };
                        let max = b.rows().len().saturating_sub(FOLDER_ROWS);
                        let steps = (dy.abs() / 20.).ceil().max(1.) as usize;
                        b.top = if dy < 0. {
                            (b.top + steps).min(max)
                        } else {
                            b.top.saturating_sub(steps)
                        };
                        cx.notify();
                    }));
                for (i, row) in rows.iter().enumerate().skip(b.top).take(FOLDER_ROWS) {
                    let glyph = if row.up {
                        "\u{2191}"
                    } else if row.git {
                        "\u{25c6}"
                    } else {
                        "\u{25b8}"
                    };
                    let target = row.target.clone();
                    list = list.child(
                        Self::add_project_row(
                            th,
                            sk,
                            s,
                            SharedString::from(format!("add-project-folder-{i}")),
                            glyph,
                            row.label.clone(),
                            None,
                            b.at == Some(i),
                            row.label.starts_with('.') && !row.up,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                if let Some(a) = ws.adding() {
                                    if let Step::Folder(b) = &mut a.step {
                                        b.set_field(&target);
                                    }
                                }
                                ws.add_project_read(cx);
                                window.focus(&ws.focus_handle, cx);
                                cx.notify();
                            }),
                        ),
                    );
                }
                // What the list is waiting on, or why it is empty.
                let status = match b.reads.get(&dir) {
                    None | Some(Read::Pending) => {
                        Some((e.add_reading.to_string(), sk.ink.ink_faint))
                    }
                    Some(Read::Failed(err)) => Some((err.clone(), sk.ink.danger)),
                    Some(Read::Got(Listing::Missing)) => Some((
                        lang::fill(e.add_missing_dir, &[("dir", &dir)]),
                        sk.ink.ink_faint,
                    )),
                    Some(Read::Got(Listing::At { folders, .. })) => {
                        if folders.is_empty() {
                            Some((e.add_no_folders.to_string(), sk.ink.ink_faint))
                        } else if !rows.iter().any(|r| !r.up) {
                            Some((
                                lang::fill(e.add_no_match, &[("partial", &partial)]),
                                sk.ink.ink_faint,
                            ))
                        } else {
                            None
                        }
                    }
                };
                if let Some((text, color)) = status {
                    drawn += 1;
                    list = list.child(
                        div()
                            .px(px(10. * s))
                            .py(px(4. * s))
                            .text_size(sk.text_px(10.5 * s))
                            .text_color(color)
                            .child(text),
                    );
                }
                // Held at its full height: the card is centred, so a short
                // folder moved every row, and the next click landed on a row
                // the person was not pointing at.
                for _ in drawn..FOLDER_ROWS {
                    list = list.child(Self::add_project_blank(sk, s));
                }
                let below = total.saturating_sub(b.top + FOLDER_ROWS);
                list = list.child(
                    div()
                        .px(px(10. * s))
                        .py(px(2. * s))
                        .text_size(sk.text_px(9.5 * s))
                        .text_color(sk.ink.ink_faint)
                        .child(if below > 0 {
                            lang::fill(e.add_more, &[("n", &below.to_string())])
                        } else {
                            "\u{a0}".to_string()
                        }),
                );
                col = col.child(list);
                let target = ap::field_target(&b.field.text());
                let said = match b.exists() {
                    Some(false) => lang::fill(e.add_creates, &[("path", &target)]),
                    _ => lang::fill(e.add_adds, &[("path", &target)]),
                };
                col = col.child(small(said, sk.ink.ink_dim));
                if let Some((n, failed)) = &b.note {
                    let ink = if *failed { sk.ink.danger } else { th.accent };
                    col = col.child(small(n.clone(), ink));
                }
                col = col
                    .child(
                        foot()
                            .child(button("add-project-back", e.add_back, false).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    if let Some(a) = ws.adding() {
                                        if let Step::Folder(b) = &mut a.step {
                                            b.at = None;
                                        }
                                    }
                                    ws.add_project_escape();
                                    window.focus(&ws.focus_handle, cx);
                                    cx.notify();
                                }),
                            ))
                            .child(
                                button("add-project-new-here", e.add_new_here, false)
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                                            cx.stop_propagation();
                                            if let Some(a) = ws.adding() {
                                                let offered = a.offered.clone().unwrap_or_default();
                                                if let Step::Folder(b) = &mut a.step {
                                                    b.new = Some(EditBuffer::at_end(&offered));
                                                    b.note = None;
                                                }
                                            }
                                            window.focus(&ws.focus_handle, cx);
                                            cx.notify();
                                        }),
                                    ),
                            )
                            .child(button("add-project-go", e.add_go, true).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    ws.add_project_add(true, cx);
                                    window.focus(&ws.focus_handle, cx);
                                }),
                            )),
                    )
                    .child(hint(e.add_hint_folder));
            }
        }
        Some(col.into_any_element())
    }
}

#[cfg(test)]
mod tests;
