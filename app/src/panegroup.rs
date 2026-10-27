//! Each pane's processes in a memory group of their own — a systemd scope.
//!
//! An idle agent is a family of processes: the agent, the MCP helpers it
//! started, anything those started, and the shell under all of them. Until a
//! pane's family lives in a cgroup of its own, Terminal Delight can neither say
//! what the pane costs nor do anything about it, because every pane on this
//! machine shared one scope with the window (measured 2026-10-26: all fifteen
//! Claude agents in `app-Hyprland-env-….scope`). tmux already puts every pane in
//! a `tmux-spawn-<id>.scope` of its own; this is the same idea, done from the
//! window because the session host can never be restarted.
//!
//! **The group is made by systemd, not by us.** The user manager owns
//! `app.slice`, so a transient scope asked for over D-Bus (`StartTransientUnit`
//! with `PIDs`) lands there with files the user can write — `cgroup.freeze`,
//! `memory.reclaim` — and with no privilege at all (tested 2026-10-26 with a
//! `sleep` process: moved, frozen, thawed, collected). We call `busctl` rather
//! than link a D-Bus crate: four calls in a pane's life do not earn a dependency.
//!
//! **Only the processes named move.** A process that forks while the scope is
//! being made can be left behind in the old group, so adoption names the whole
//! tree, waits for the job, then names whatever is still outside. Every later
//! child inherits the scope by itself.
//!
//! **Act only on what is inside the group.** Nothing here finds a process by a
//! name or a pattern. A probe that did so on 2026-10-26 resumed another pane's
//! live conversation through a "newest file" fallback; the rule this module
//! keeps is that every pid it touches comes from the pane's own shell or the
//! pane's own `cgroup.procs`.
//!
//! **Unknown is not zero.** Swap accounting can be switched off, a file can be
//! unreadable, a scope can be mid-teardown. Each reading is an `Option`, and a
//! missing one stays missing all the way to the pane header, which says
//! `unmeasured` rather than printing a smaller number.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Where cgroup v2 is mounted.
const CGROUP_ROOT: &str = "/sys/fs/cgroup";

/// A pane's memory group: the systemd scope its processes live in, and the
/// cgroup directory that scope owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneGroup {
    pub unit: String,
    pub dir: PathBuf,
}

/// What a pane's processes hold right now. Each field is absent when it could
/// not be read, which is a different fact from zero.
///
/// **Read per process, not off the group.** The kernel leaves memory charged
/// to the group a page was first touched in, and a pane's processes were
/// started by the session host and moved into their own group afterwards, so
/// the group's `memory.current` holds only what was touched since the move —
/// measured 2026-10-26 on a live rig: 7 MB charged against a 264 MB Claude.
/// The sum of each process's PSS and SwapPss is right wherever the charge sits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    /// Bytes in RAM: the sum of the group's processes' PSS.
    pub resident: Option<u64>,
    /// Bytes in swap: the sum of the group's processes' SwapPss.
    pub swapped: Option<u64>,
    /// Bytes the kernel charges to the group itself (`memory.current`) — what
    /// `memory.reclaim` on it can reach.
    pub charged: Option<u64>,
    /// CPU time the group has used since it was made (`cpu.stat` usage_usec).
    pub cpu_usec: Option<u64>,
}

/// Why a pane could not be given a group of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdoptError {
    /// The pane's shell is gone or unreadable.
    NoProcess,
    /// The user manager could not be reached or refused the scope.
    Systemd(String),
    /// The scope was made, but these processes of the tree are still outside it.
    Incomplete(Vec<i32>),
}

/// A session key as it appears in a unit name: anything that is not a letter
/// or digit becomes `_`, because session keys are words a person chose.
fn unit_key(session: &str) -> String {
    session
        .chars()
        .take(32)
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// The scope name for one pane of one session, for the shell it runs now.
///
/// **The shell's pid is part of the name.** Pane ids start again at 1 when a
/// host starts, and a scope left frozen by an old host's pane holds processes
/// that cannot die until they are thawed. A name that a new pane 5 could share
/// with the old one would attach the new shell to the old frozen scope.
pub fn unit_name(session: &str, pane: u64, shell: i32) -> String {
    format!("td-pane-{}-{pane}-{shell}.scope", unit_key(session))
}

/// The pane and shell a scope of `session` was made for, from its name.
fn parse_unit(session: &str, name: &str) -> Option<(u64, i32)> {
    let rest = name
        .strip_prefix(&format!("td-pane-{}-", unit_key(session)))?
        .strip_suffix(".scope")?;
    let (pane, shell) = rest.split_once('-')?;
    Some((pane.parse().ok()?, shell.parse().ok()?))
}

/// The cgroup v2 path in `/proc/<pid>/cgroup`, from its `0::` line.
fn parse_cgroup(text: &str) -> Option<&str> {
    text.lines()
        .find_map(|line| line.strip_prefix("0::"))
        .map(str::trim)
        .filter(|path| path.starts_with('/'))
}

/// The cgroup directory `pid` lives in, or `None` when it cannot be read.
pub fn cgroup_dir(pid: i32) -> Option<PathBuf> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    let rel = parse_cgroup(&text)?;
    Some(Path::new(CGROUP_ROOT).join(rel.trim_start_matches('/')))
}

/// The group `pid` already belongs to, when it is this pane's scope.
pub fn group_of(pid: i32, session: &str, pane: u64, shell: i32) -> Option<PaneGroup> {
    let unit = unit_name(session, pane, shell);
    let dir = cgroup_dir(pid)?;
    (dir.file_name()?.to_str()? == unit).then_some(PaneGroup { unit, dir })
}

/// The children `/proc` lists for every thread of `pid`.
fn children_of(pid: i32) -> Vec<i32> {
    let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
        return Vec::new();
    };
    let mut kids = Vec::new();
    for task in tasks.flatten() {
        if let Ok(list) = std::fs::read_to_string(task.path().join("children")) {
            kids.extend(
                list.split_whitespace()
                    .filter_map(|p| p.parse::<i32>().ok()),
            );
        }
    }
    kids.sort_unstable();
    kids.dedup();
    kids
}

/// Every process in the tree rooted at `root`, root first. A root that is gone
/// gives an empty tree.
pub fn tree(root: i32) -> Vec<i32> {
    if !Path::new(&format!("/proc/{root}")).exists() {
        return Vec::new();
    }
    let mut out = vec![root];
    let mut i = 0;
    while i < out.len() {
        for kid in children_of(out[i]) {
            if !out.contains(&kid) {
                out.push(kid);
            }
        }
        i += 1;
    }
    out
}

/// `busctl --user call` against the systemd user manager, bounded by a timeout.
fn manager_call(method: &str, signature: &str, args: &[String]) -> Result<String, String> {
    if cfg!(test) {
        return Err("no systemd in a test build".into());
    }
    let out = Command::new("busctl")
        .args([
            "--user",
            "--timeout=3",
            "call",
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
            method,
            signature,
        ])
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("busctl: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// The arguments of `StartTransientUnit` for a pane scope holding `pids`.
fn start_args(unit: &str, pids: &[i32]) -> Vec<String> {
    let mut args = vec![
        unit.to_owned(),
        "fail".into(),
        "3".into(),
        "PIDs".into(),
        "au".into(),
        pids.len().to_string(),
    ];
    args.extend(pids.iter().map(i32::to_string));
    args.extend([
        "CollectMode".into(),
        "s".into(),
        "inactive-or-failed".into(),
        "Description".into(),
        "s".into(),
        "Terminal Delight pane".into(),
        "0".into(),
    ]);
    args
}

/// The arguments of `AttachProcessesToUnit` moving `pids` into `unit`.
fn attach_args(unit: &str, pids: &[i32]) -> Vec<String> {
    let mut args = vec![unit.to_owned(), String::new(), pids.len().to_string()];
    args.extend(pids.iter().map(i32::to_string));
    args
}

/// Give the pane whose shell is `shell` a memory group of its own, or find the
/// one it already has. Idempotent: a second call only names stragglers.
pub fn adopt(session: &str, pane: u64, shell: i32) -> Result<PaneGroup, AdoptError> {
    let unit = unit_name(session, pane, shell);
    let pids = tree(shell);
    if pids.is_empty() {
        return Err(AdoptError::NoProcess);
    }
    if group_of(shell, session, pane, shell).is_none() {
        match manager_call(
            "StartTransientUnit",
            "ssa(sv)a(sa(sv))",
            &start_args(&unit, &pids),
        ) {
            Ok(_) => {}
            // A window swap finds the scope its predecessor made.
            Err(e) if e.contains("already exists") || e.contains("UnitExists") => {
                manager_call("AttachProcessesToUnit", "ssau", &attach_args(&unit, &pids))
                    .map_err(AdoptError::Systemd)?;
            }
            Err(e) => return Err(AdoptError::Systemd(e)),
        }
        // The call queues a job; the move is visible once the shell's own
        // cgroup file names the scope.
        let deadline = Instant::now() + Duration::from_secs(2);
        while group_of(shell, session, pane, shell).is_none() {
            if Instant::now() > deadline {
                return Err(AdoptError::Incomplete(vec![shell]));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    let group = group_of(shell, session, pane, shell).ok_or(AdoptError::Incomplete(vec![shell]))?;
    let outside = |g: &PaneGroup| -> Vec<i32> {
        tree(shell)
            .into_iter()
            .filter(|pid| cgroup_dir(*pid).is_some_and(|d| d != g.dir))
            .collect()
    };
    let stragglers = outside(&group);
    if !stragglers.is_empty() {
        manager_call(
            "AttachProcessesToUnit",
            "ssau",
            &attach_args(&unit, &stragglers),
        )
        .map_err(AdoptError::Systemd)?;
        let left = outside(&group);
        if !left.is_empty() {
            return Err(AdoptError::Incomplete(left));
        }
    }
    Ok(group)
}

fn read_u64(path: &Path) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// A `key value` field from a flat-keyed cgroup file such as `cpu.stat`.
fn keyed(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let (k, v) = line.split_once(' ')?;
        (k == key).then(|| v.trim().parse().ok()).flatten()
    })
}

/// PSS and SwapPss are independent measurements: an absent swap field must
/// not erase a valid RAM reading, and it must not become a measured zero.
fn parse_pss(text: &str) -> (Option<u64>, Option<u64>) {
    let bytes = |key: &str| -> Option<u64> {
        text.lines().find_map(|l| {
            let rest = l.strip_prefix(key)?.strip_prefix(':')?;
            rest.split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()?
                .checked_mul(1024)
        })
    };
    (bytes("Pss"), bytes("SwapPss"))
}

fn pss_of(pid: i32) -> Option<(Option<u64>, Option<u64>)> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok()?;
    Some(parse_pss(&text))
}

/// What the group's processes hold now. A process that ends between listing
/// and reading is simply gone; any other unreadable measurement stays unknown.
pub fn usage(group: &PaneGroup) -> Usage {
    let (ram, swap) = match measured_procs(group) {
        None => (None, None),
        Some(pids) => {
            let mut ram = Some(0u64);
            let mut swap = Some(0u64);
            for pid in pids {
                match pss_of(pid) {
                    Some((p, s)) => {
                        ram = ram.zip(p).and_then(|(a, b)| a.checked_add(b));
                        swap = swap.zip(s).and_then(|(a, b)| a.checked_add(b));
                    }
                    None if !Path::new(&format!("/proc/{pid}")).exists() => {}
                    None => {
                        ram = None;
                        swap = None;
                    }
                }
            }
            (ram, swap)
        }
    };
    Usage {
        resident: ram,
        swapped: swap,
        charged: read_u64(&group.dir.join("memory.current")),
        cpu_usec: std::fs::read_to_string(group.dir.join("cpu.stat"))
            .ok()
            .and_then(|t| keyed(&t, "usage_usec")),
    }
}

/// Unlike an empty readable list, an unavailable or malformed list is unknown.
fn measured_procs(group: &PaneGroup) -> Option<Vec<i32>> {
    std::fs::read_to_string(group.dir.join("cgroup.procs"))
        .ok()?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.trim().parse::<i32>().ok().filter(|pid| *pid > 0))
        .collect()
}

/// The processes available for process-control checks. An unreadable list
/// grants no process to act on; memory accounting uses the checked read above.
pub fn procs(group: &PaneGroup) -> Vec<i32> {
    measured_procs(group).unwrap_or_default()
}

/// Whether the kernel reports the group frozen; `None` when unreadable.
pub fn frozen(group: &PaneGroup) -> Option<bool> {
    let text = std::fs::read_to_string(group.dir.join("cgroup.events")).ok()?;
    keyed(&text, "frozen").map(|v| v == 1)
}

/// Ask the kernel to freeze or thaw the group, then wait up to `within` for it
/// to say so. Returns whether the group reached the state asked for.
pub fn set_frozen(group: &PaneGroup, freeze: bool, within: Duration) -> bool {
    if std::fs::write(
        group.dir.join("cgroup.freeze"),
        if freeze { "1" } else { "0" },
    )
    .is_err()
    {
        return false;
    }
    let deadline = Instant::now() + within;
    loop {
        if frozen(group) == Some(freeze) {
            return true;
        }
        if Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Ask the kernel to push up to `bytes` of the group's memory out. Best effort:
/// the kernel answers "try again" when it reclaimed less than asked, which is
/// expected for a group already mostly in swap, so that answer is not an error.
/// Read [`usage`] afterwards for what actually happened.
pub fn reclaim(group: &PaneGroup, bytes: u64) -> std::io::Result<()> {
    match std::fs::write(group.dir.join("memory.reclaim"), bytes.to_string()) {
        Err(e) if e.raw_os_error() == Some(libc::EAGAIN) => Ok(()),
        other => other,
    }
}

/// Push a frozen group's memory out in steps, pausing between them, and stop
/// when a step frees almost nothing. One large write would do it in one go,
/// but this machine runs systemd-oomd with pressure kills on `app.slice`, and a
/// reclaim burst is pressure; a paced one is not. Returns the bytes the group's
/// RAM charge fell by, or `None` when it could not be read.
pub fn reclaim_paced(group: &PaneGroup) -> Option<u64> {
    const STEP: u64 = 64 << 20;
    const ENOUGH: u64 = 8 << 20;
    let charged = |g: &PaneGroup| read_u64(&g.dir.join("memory.current"));
    let start = charged(group)?;
    let mut before = start;
    for _ in 0..32 {
        if reclaim(group, STEP).is_err() {
            break;
        }
        let after = charged(group)?;
        if before.saturating_sub(after) < ENOUGH {
            break;
        }
        before = after;
        std::thread::sleep(Duration::from_millis(40));
    }
    Some(start.saturating_sub(charged(group)?))
}

/// The hand on a resting pane's freezer, shared by the pane (main thread) and
/// its byte stream (the pump thread).
///
/// Every write to the pane passes [`Gate::on_input`] first, so a key, a paste,
/// a mouse report or a bench write can never reach a frozen agent: whatever
/// froze it is undone before the bytes go on. The freeze and the thaw take the
/// same lock, and a freeze is refused if anything was written since the policy
/// looked, so a keystroke racing the decision wins.
#[derive(Debug, Default)]
pub struct Gate {
    held: std::sync::Mutex<Option<PathBuf>>,
    writes: std::sync::atomic::AtomicU64,
    woke: std::sync::atomic::AtomicBool,
    /// Set while the pane is asleep: nothing may reach the shell behind the
    /// sleep card, by any path — a paste, a bench line, a mouse report.
    blocked: std::sync::atomic::AtomicBool,
}

impl Gate {
    fn held(&self) -> std::sync::MutexGuard<'_, Option<PathBuf>> {
        self.held.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Called before every write to the pane.
    pub fn on_input(&self) {
        use std::sync::atomic::Ordering;
        self.writes.fetch_add(1, Ordering::SeqCst);
        let mut held = self.held();
        if let Some(dir) = held.take() {
            thaw_dir(&dir);
            self.woke.store(true, Ordering::SeqCst);
        }
    }

    /// Swallow every write while `on` (an asleep pane).
    pub fn set_blocked(&self, on: bool) {
        self.blocked.store(on, std::sync::atomic::Ordering::SeqCst);
    }

    /// Whether writes are being swallowed.
    pub fn blocks(&self) -> bool {
        self.blocked.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// How many writes have passed; the policy reads it when it looks.
    pub fn writes(&self) -> u64 {
        self.writes.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Freeze `group`, unless anything was written since `epoch`. Returns
    /// whether the group is now frozen and held by this gate.
    pub fn freeze_if_quiet(&self, group: &PaneGroup, epoch: u64) -> bool {
        let mut held = self.held();
        if self.writes() != epoch {
            return false;
        }
        if !set_frozen(group, true, Duration::from_millis(500)) {
            // Half a freeze is worse than none: put it back.
            set_frozen(group, false, Duration::from_millis(200));
            return false;
        }
        *held = Some(group.dir.clone());
        true
    }

    /// Thaw what this gate froze, without counting it as a person's wake (the
    /// policy itself is about to act on the agent, or the person looked).
    /// Returns whether it held one.
    pub fn release(&self) -> bool {
        match self.held().take() {
            Some(dir) => {
                thaw_dir(&dir);
                true
            }
            None => false,
        }
    }

    /// Take over a group a previous window left frozen, so input thaws it.
    pub fn adopt_frozen(&self, group: &PaneGroup) {
        *self.held() = Some(group.dir.clone());
    }

    /// Whether a frozen group is held by this gate now.
    pub fn holds(&self) -> bool {
        self.held().is_some()
    }

    /// Whether input thawed the pane since the last call.
    pub fn take_woke(&self) -> bool {
        self.woke.swap(false, std::sync::atomic::Ordering::SeqCst)
    }
}

/// Thaw a cgroup directory and wait briefly for the kernel to say so. Used on
/// the pump thread, so it never blocks for long: a thaw is microseconds.
pub fn thaw_dir(dir: &Path) {
    if std::fs::write(dir.join("cgroup.freeze"), "0").is_err() {
        return;
    }
    let deadline = Instant::now() + Duration::from_millis(100);
    while Instant::now() < deadline {
        let still = std::fs::read_to_string(dir.join("cgroup.events"))
            .ok()
            .and_then(|t| keyed(&t, "frozen"))
            == Some(1);
        if !still {
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Where the user manager puts transient scopes: `…/user@<uid>.service/app.slice`.
pub fn app_slice_dir() -> PathBuf {
    // SAFETY: getuid has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    Path::new(CGROUP_ROOT).join(format!(
        "user.slice/user-{uid}.slice/user@{uid}.service/app.slice"
    ))
}

/// Every scope of `session` that is not one of the `live` (pane, shell) pairs:
/// what a previous window or a previous host left behind.
pub fn orphans(session: &str, live: &[(u64, i32)]) -> Vec<PaneGroup> {
    orphans_in(&app_slice_dir(), session, live)
}

/// Thaw every frozen scope of `session`. Called as the window quits: a pane
/// stays frozen only while a window that can thaw it is up, so whatever runs
/// next — this build, or an older one that knows nothing of rest and would
/// leave a frozen agent answering no key — finds every pane awake. Returns how
/// many it thawed.
pub fn thaw_session(session: &str) -> usize {
    thaw_session_in(&app_slice_dir(), session)
}

fn thaw_session_in(dir: &Path, session: &str) -> usize {
    let mut thawed = 0;
    for group in orphans_in(dir, session, &[]) {
        let frozen = std::fs::read_to_string(group.dir.join("cgroup.events"))
            .ok()
            .and_then(|t| keyed(&t, "frozen"))
            == Some(1);
        if frozen {
            thaw_dir(&group.dir);
            thawed += 1;
        }
    }
    thawed
}

fn orphans_in(dir: &Path, session: &str, live: &[(u64, i32)]) -> Vec<PaneGroup> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PaneGroup> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let ids = parse_unit(session, &name)?;
            (!live.contains(&ids)).then(|| PaneGroup {
                unit: name,
                dir: e.path(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.unit.cmp(&b.unit));
    out
}

/// The socket inodes in the LISTEN state in a `/proc/net/tcp`-format table.
fn listen_inodes(table: &str) -> Vec<u64> {
    table
        .lines()
        .skip(1)
        .filter_map(|line| {
            let cols: Vec<&str> = line.split_whitespace().collect();
            // sl local rem st tx:rx tr:when retrnsmt uid timeout inode
            (cols.get(3) == Some(&"0A"))
                .then(|| cols.get(9)?.parse().ok())
                .flatten()
        })
        .collect()
}

/// Every TCP socket inode on the machine that is listening.
pub fn listening_inodes() -> std::collections::HashSet<u64> {
    ["/proc/net/tcp", "/proc/net/tcp6"]
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .flat_map(|t| listen_inodes(&t))
        .collect()
}

/// One process, as rest's classifier needs it: parent, argv, and whether it
/// holds a listening TCP socket.
pub fn proc_info(
    pid: i32,
    listening: &std::collections::HashSet<u64>,
) -> Option<crate::agentrest::Proc> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after = stat.get(stat.rfind(')')? + 2..)?;
    let ppid = after.split_whitespace().nth(1)?.parse().ok()?;
    let argv = std::fs::read(format!("/proc/{pid}/cmdline"))
        .ok()?
        .split(|b| *b == 0)
        .filter(|a| !a.is_empty())
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect();
    let socket_inode = |link: PathBuf| -> Option<u64> {
        let link = link.to_str()?.to_owned();
        link.strip_prefix("socket:[")?
            .strip_suffix(']')?
            .parse()
            .ok()
    };
    let listens = std::fs::read_dir(format!("/proc/{pid}/fd"))
        .map(|fds| {
            fds.flatten().any(|fd| {
                std::fs::read_link(fd.path())
                    .ok()
                    .and_then(socket_inode)
                    .is_some_and(|ino| listening.contains(&ino))
            })
        })
        .unwrap_or(false);
    Some(crate::agentrest::Proc {
        pid,
        ppid,
        argv,
        listens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_orphan_is_any_scope_of_this_session_that_is_not_live() {
        let dir = std::env::temp_dir().join(format!("td-orphans-{}", std::process::id()));
        for name in [
            "td-pane-1-4-100.scope",
            "td-pane-1-5-200.scope",
            "td-pane-1-4-999.scope",
            "td-pane-2-4-100.scope",
            "app-x.scope",
        ] {
            std::fs::create_dir_all(dir.join(name)).unwrap();
        }
        let found: Vec<String> = orphans_in(&dir, "1", &[(4, 100), (5, 200)])
            .into_iter()
            .map(|g| g.unit)
            .collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(found, vec!["td-pane-1-4-999.scope".to_string()]);
    }

    #[test]
    fn quitting_thaws_this_sessions_frozen_scopes_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("td-thaw-quit-{}", std::process::id()));
        let scope = |name: &str, frozen: bool| {
            let d = dir.join(name);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("cgroup.freeze"), if frozen { "1" } else { "0" }).unwrap();
            let events = if frozen { "frozen 1\n" } else { "frozen 0\n" };
            std::fs::write(d.join("cgroup.events"), events).unwrap();
            d
        };
        let ours_frozen = scope("td-pane-1-4-100.scope", true);
        let ours_awake = scope("td-pane-1-5-200.scope", false);
        let theirs = scope("td-pane-2-4-100.scope", true);
        let thawed = thaw_session_in(&dir, "1");
        let read = |d: &PathBuf| std::fs::read_to_string(d.join("cgroup.freeze")).unwrap();
        let (a, b, c) = (read(&ours_frozen), read(&ours_awake), read(&theirs));
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(thawed, 1);
        assert_eq!(a, "0", "this session's frozen pane is thawed");
        assert_eq!(b, "0");
        assert_eq!(
            c, "1",
            "another session's pane is not this window's to thaw"
        );
    }

    #[test]
    fn memory_distinguishes_empty_unavailable_and_malformed_process_lists() {
        let dir = std::env::temp_dir().join(format!("td-memory-list-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let group = PaneGroup {
            unit: "fixture".into(),
            dir: dir.clone(),
        };
        let missing = usage(&group);
        std::fs::write(dir.join("cgroup.procs"), "").unwrap();
        let empty = usage(&group);
        std::fs::write(dir.join("cgroup.procs"), "not-a-pid\n").unwrap();
        let malformed = usage(&group);
        std::fs::write(dir.join("cgroup.procs"), std::process::id().to_string()).unwrap();
        let live = usage(&group);
        std::fs::remove_dir_all(dir).unwrap();
        assert_eq!((missing.resident, missing.swapped), (None, None));
        assert_eq!((malformed.resident, malformed.swapped), (None, None));
        assert_eq!((empty.resident, empty.swapped), (Some(0), Some(0)));
        assert!(live.resident.is_some_and(|n| n > 0));
    }

    #[test]
    fn memory_preserves_missing_swap_without_losing_ram() {
        assert_eq!(parse_pss("Pss: 23 kB\n"), (Some(23 * 1024), None));
        assert_eq!(parse_pss("SwapPss: 0 kB\n"), (None, Some(0)));
        assert_eq!(parse_pss("Pss: 0 kB\nSwapPss: 0 kB\n"), (Some(0), Some(0)));
        assert_eq!(parse_pss("Pss: bad\nSwapPss: 8 kB\n"), (None, Some(8192)));
        assert_eq!(parse_pss("Pss: 18446744073709551615 kB\n"), (None, None));
    }

    #[test]
    fn only_listen_rows_count_as_serving() {
        let table = "  sl  local rem st tx rx tr tm retr uid timeout inode\n   0: 0100007F:2289 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 4242 1 0\n   1: 0100007F:8C6E 0100007F:2289 01 00000000:00000000 00:00000000 00000000  1000        0 5151 1 0\n";
        assert_eq!(listen_inodes(table), vec![4242]);
    }

    #[test]
    fn the_gate_counts_writes_and_refuses_a_stale_freeze() {
        // A stand-in group the kernel would call frozen as soon as asked, so
        // the write count is the only thing left that can refuse.
        let dir = std::env::temp_dir().join(format!("td-gate-stale-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("cgroup.freeze"), "0").unwrap();
        std::fs::write(dir.join("cgroup.events"), "populated 1\nfrozen 1\n").unwrap();
        let group = PaneGroup {
            unit: "td-pane-t-1-1.scope".into(),
            dir: dir.clone(),
        };
        let gate = Gate::default();
        let epoch = gate.writes();
        gate.on_input();
        assert_eq!(gate.writes(), epoch + 1);
        let refused = !gate.freeze_if_quiet(&group, epoch);
        let untouched = std::fs::read_to_string(dir.join("cgroup.freeze")).unwrap() == "0";
        let fresh = gate.freeze_if_quiet(&group, gate.writes());
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(refused, "a write since the look must refuse the freeze");
        assert!(
            untouched,
            "a refused freeze must not have written the freezer"
        );
        assert!(
            fresh && gate.holds(),
            "with no write since the look, it freezes"
        );
    }

    #[test]
    fn the_gate_thaws_what_it_holds_and_says_so_once() {
        let dir = std::env::temp_dir().join(format!("td-gate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("cgroup.freeze"), "1").unwrap();
        std::fs::write(dir.join("cgroup.events"), "populated 1\nfrozen 0\n").unwrap();
        let gate = Gate::default();
        gate.adopt_frozen(&PaneGroup {
            unit: "u".into(),
            dir: dir.clone(),
        });
        assert!(gate.holds());
        gate.on_input();
        let written = std::fs::read_to_string(dir.join("cgroup.freeze")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(written, "0");
        assert!(!gate.holds());
        assert!(gate.take_woke());
        assert!(!gate.take_woke());
    }

    #[test]
    fn a_session_key_cannot_reach_outside_its_unit_name() {
        assert_eq!(unit_name("1", 109, 498608), "td-pane-1-109-498608.scope");
        assert_eq!(unit_name("../x y", 3, 7), "td-pane-___x_y-3-7.scope");
        assert!(unit_name(&"k".repeat(200), 1, 1).len() < 70);
        assert_eq!(
            parse_unit("1", "td-pane-1-109-498608.scope"),
            Some((109, 498608))
        );
        assert_eq!(parse_unit("1", "td-pane-12-109-498608.scope"), None);
        assert_eq!(parse_unit("1", "tmux-spawn-x.scope"), None);
    }

    #[test]
    fn the_cgroup_path_comes_from_the_unified_line_only() {
        let text = "12:pids:/legacy\n0::/user.slice/user-1000.slice/app.slice/td-pane-1-4.scope\n";
        assert_eq!(
            parse_cgroup(text),
            Some("/user.slice/user-1000.slice/app.slice/td-pane-1-4.scope")
        );
        assert_eq!(parse_cgroup("1:name=systemd:/x\n"), None);
        assert_eq!(parse_cgroup("0::relative\n"), None);
    }

    #[test]
    fn the_scope_is_asked_for_with_every_pid_named() {
        let args = start_args("td-pane-1-4.scope", &[10, 11, 12]);
        assert_eq!(
            &args[..9],
            [
                "td-pane-1-4.scope",
                "fail",
                "3",
                "PIDs",
                "au",
                "3",
                "10",
                "11",
                "12"
            ]
        );
        assert_eq!(args.last().map(String::as_str), Some("0"));
        assert_eq!(
            attach_args("td-pane-1-4.scope", &[7]),
            ["td-pane-1-4.scope", "", "1", "7"]
        );
    }

    #[test]
    fn a_keyed_cgroup_field_is_read_by_its_whole_key() {
        let stat = "usage_usec 32122000\nuser_usec 1\nsystem_usec 2\n";
        assert_eq!(keyed(stat, "usage_usec"), Some(32_122_000));
        assert_eq!(keyed(stat, "usage"), None);
        assert_eq!(keyed("populated 1\nfrozen 0\n", "frozen"), Some(0));
    }

    #[test]
    fn the_tree_of_this_test_includes_a_child_it_started() {
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .unwrap();
        let me = std::process::id() as i32;
        let found = tree(me);
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(found.first(), Some(&me));
        assert!(found.contains(&(child.id() as i32)), "{found:?}");
    }

    #[test]
    fn a_tree_rooted_at_a_process_that_is_gone_is_empty() {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id() as i32;
        child.wait().unwrap();
        assert!(tree(pid).is_empty());
    }

    #[test]
    fn a_test_build_never_reaches_systemd() {
        assert!(matches!(
            adopt("1", 1, std::process::id() as i32),
            Err(AdoptError::Systemd(_))
        ));
    }
}
