//! Starting a program on a pseudoterminal.
//!
//! A step-for-step copy of alacritty_terminal 0.26's Unix `tty::new`, because
//! the programs TD runs have relied on its effects for as long as TD has run
//! them: the same environment (including `ALACRITTY_WINDOW_ID` and
//! `WINDOWID`, which a child may still check for), the same session and
//! controlling-terminal setup, the same six signals put back to their defaults.
//!
//! Two differences, both deliberate. A refused resize is returned as an error;
//! alacritty's `Pty::on_resize` calls `process::exit(1)`, which inside a
//! session host would end every terminal it holds. And the child is watched
//! through a pidfd, which becomes readable when it exits, instead of a
//! process-wide `SIGCHLD` handler that each terminal registers and must
//! remember to unregister.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::{self, Read, Write};
use std::mem::MaybeUninit;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};
use std::{env, ptr};

use rustix_openpty::openpty;
use rustix_openpty::rustix::termios::{self, InputModes, OptionalActions, Winsize};

use super::pump::{CloseReport, Source};
use super::WindowSize;

#[cfg(test)]
mod tests;

/// What to run, where, with what added to its environment.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// A program and its arguments. `None` runs the user's shell.
    pub shell: Option<(String, Vec<String>)>,
    /// A directory that does not exist is ignored, not an error: the shell
    /// starts wherever it would have.
    pub working_directory: Option<PathBuf>,
    pub env: HashMap<String, String>,
}

/// A program running on a pseudoterminal, seen from the side that reads it.
pub struct PtySource {
    child: Option<Child>,
    master: Option<Arc<File>>,
    /// Readable once the child has exited. `None` on a kernel without
    /// `pidfd_open` (older than 5.3), where the end of the stream is the only
    /// signal.
    pidfd: Option<OwnedFd>,
}

/// Observing a terminal must not keep its master open after its owner closes it.
#[derive(Clone)]
pub struct Monitor(Weak<File>);

impl Monitor {
    pub fn file(&self) -> Option<Arc<File>> {
        self.0.upgrade()
    }
}

impl PtySource {
    pub fn child(&self) -> &Child {
        self.child.as_ref().expect("live PTY child")
    }

    pub fn file(&self) -> &File {
        self.master.as_deref().expect("live PTY master")
    }

    pub fn monitor(&self) -> Monitor {
        Monitor(Arc::downgrade(
            self.master.as_ref().expect("live PTY master"),
        ))
    }

    /// Only this owner may signal the child. It retains the unreaped child until
    /// the last group signal, so neither its PID nor its group can be recycled
    /// between deciding to close and escalating. No saved PID is an authority.
    fn close_child(&mut self) -> Option<CloseReport> {
        let mut child = self.child.take()?;
        let mut report = CloseReport {
            hangup_sent: false,
            kill_sent: false,
            exit_observed: false,
            observed_jobs_remaining: None,
            error: None,
        };
        match child.try_wait() {
            Ok(Some(_)) => {
                self.master.take();
                report.exit_observed = true;
                return Some(report);
            }
            Err(err) => {
                self.master.take();
                report.error = Some(err.to_string());
                return Some(report);
            }
            Ok(None) => {}
        }
        let pid = child.id() as i32;
        // Interactive shells put jobs in separate process groups. Open pidfds
        // while the session leader is still ours, then signal those identities,
        // never numbers copied out of /proc after their owners have disappeared.
        let mut jobs = session_jobs(pid, Duration::from_millis(100));
        for (_, fd) in &jobs.held {
            signal_job(fd, libc::SIGHUP);
        }
        // spawn() makes the child a session and process-group leader. Signal
        // its group while that identity is still reserved by our Child.
        report.hangup_sent = unsafe { libc::kill(-pid, libc::SIGHUP) } == 0;
        self.master.take();
        // Closing the master also hangs up its foreground job. Do not reap the
        // leader during this grace period: a surviving group member still needs
        // the escalation even when the leader has already exited.
        std::thread::sleep(Duration::from_millis(250));
        // Include jobs started during the grace period. A process that has
        // deliberately left this terminal's session is outside this close.
        let fresh = session_jobs(pid, Duration::from_millis(100));
        report.error = jobs.error.take().or(fresh.error);
        for (id, fd) in fresh.held {
            if !jobs
                .held
                .iter()
                .any(|(old, old_fd)| *old == id && job_exited(old_fd) == Some(false))
            {
                jobs.held.push((id, fd));
            }
        }
        report.kill_sent = unsafe { libc::kill(-pid, libc::SIGKILL) } == 0;
        for (_, fd) in &jobs.held {
            signal_job(fd, libc::SIGKILL);
        }
        let until = Instant::now() + Duration::from_millis(250);
        loop {
            if !report.exit_observed {
                match child.try_wait() {
                    Ok(Some(_)) => report.exit_observed = true,
                    Err(err) => {
                        report.error = Some(err.to_string());
                        return Some(report);
                    }
                    Ok(None) => {}
                }
            }
            if report.error.is_none() {
                report.observed_jobs_remaining = jobs
                    .held
                    .iter()
                    .map(|(_, fd)| job_exited(fd).map(|done| usize::from(!done)))
                    .collect::<Option<Vec<_>>>()
                    .map(|n| n.into_iter().sum());
            }
            if report.exit_observed && report.observed_jobs_remaining == Some(0) {
                return Some(report);
            }
            if Instant::now() >= until {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        if report.exit_observed {
            return Some(report);
        }
        // A kernel-blocked child may not exit even after SIGKILL. Release the
        // pane promptly, retain a reaper, and report that exit is still unknown.
        report.error = Some("child has not exited after the close deadline".into());
        if let Err(err) = std::thread::Builder::new()
            .name("td-reap".into())
            .spawn(move || {
                let _ = child.wait();
            })
        {
            report.error = Some(format!("could not start child reaper: {err}"));
        }
        eprintln!(
            "td: pane cleanup pending for child {pid}: {:?}",
            report.error
        );
        Some(report)
    }
}

/// A bounded snapshot of identities in this terminal's Unix session. The
/// unreaped leader reserves the session ID for the duration of both snapshots.
struct SessionJobs {
    held: Vec<(i32, OwnedFd)>,
    error: Option<String>,
}

fn session_jobs(sid: i32, budget: Duration) -> SessionJobs {
    let until = Instant::now() + budget;
    let mut held = Vec::new();
    let scan = (|| -> io::Result<()> {
        for entry in std::fs::read_dir("/proc")? {
            if Instant::now() >= until {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "process scan deadline",
                ));
            }
            let entry = entry?;
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<i32>().ok())
            else {
                continue;
            };
            if pid == sid || unsafe { libc::getsid(pid) } != sid {
                continue;
            }
            let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
            if raw < 0 {
                let err = io::Error::last_os_error();
                if err.raw_os_error() == Some(libc::ESRCH) {
                    continue;
                }
                return Err(err);
            }
            let fd = unsafe { OwnedFd::from_raw_fd(raw as i32) };
            // Recheck after opening. A reused number cannot grant authority over
            // a process outside the still-owned terminal session.
            if unsafe { libc::getsid(pid) } == sid {
                held.push((pid, fd));
            }
        }
        Ok(())
    })();
    // Even an incomplete scan retains every identity already acquired. Losing
    // them on a timeout would abandon jobs that are safe to signal.
    SessionJobs {
        held,
        error: scan
            .err()
            .map(|e| format!("session job inspection incomplete: {e}")),
    }
}

fn signal_job(fd: &OwnedFd, signal: i32) {
    unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            fd.as_raw_fd(),
            signal,
            ptr::null::<libc::siginfo_t>(),
            0,
        );
    }
}

fn job_exited(fd: &OwnedFd) -> Option<bool> {
    let mut poll = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let result = unsafe { libc::poll(&mut poll, 1, 0) };
    (result >= 0).then_some(poll.revents & libc::POLLIN != 0)
}

impl Source for PtySource {
    fn fd(&self) -> BorrowedFd<'_> {
        self.file().as_fd()
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file().read(buf)
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file().write(buf)
    }

    fn resize(&mut self, size: WindowSize) -> io::Result<()> {
        let win = winsize(size);
        let res = unsafe {
            libc::ioctl(
                self.file().as_raw_fd(),
                libc::TIOCSWINSZ,
                &win as *const Winsize,
            )
        };
        if res < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn child_fd(&self) -> Option<BorrowedFd<'_>> {
        self.pidfd.as_ref().map(|fd| fd.as_fd())
    }

    fn reap(&mut self) -> Option<ExitStatus> {
        self.child.as_mut()?.try_wait().ok().flatten()
    }

    fn close(&mut self) -> Option<CloseReport> {
        self.close_child()
    }
}

impl Drop for PtySource {
    fn drop(&mut self) {
        self.close_child();
    }
}

/// Open a pseudoterminal of `size` and start `options`' program on it.
pub fn spawn(options: &Options, size: WindowSize) -> io::Result<PtySource> {
    let pty = openpty(None, Some(&winsize(size)))?;
    let (master, slave) = (pty.controller, pty.user);
    let master_fd = master.as_raw_fd();
    let slave_fd = slave.as_raw_fd();

    if let Ok(mut termios) = termios::tcgetattr(&master) {
        termios.input_modes.set(InputModes::IUTF8, true);
        let _ = termios::tcsetattr(&master, OptionalActions::Now, &termios);
    }

    let user = ShellUser::from_env()?;
    let mut builder = match options.shell.as_ref() {
        Some((program, args)) => {
            let mut command = Command::new(program);
            command.args(args);
            command
        }
        None => Command::new(&user.shell),
    };

    builder.stdin(slave.try_clone()?);
    builder.stderr(slave.try_clone()?);
    builder.stdout(slave);

    // The window id alacritty stamps is always 0 here, as it was: TD never had
    // an X11 window id to give, and a child checking for the variables still
    // finds them.
    builder.env("ALACRITTY_WINDOW_ID", "0");
    builder.env("USER", user.user);
    builder.env("HOME", user.home);
    builder.env("WINDOWID", "0");
    for (key, value) in &options.env {
        builder.env(key, value);
    }
    builder.env_remove("XDG_ACTIVATION_TOKEN");
    builder.env_remove("DESKTOP_STARTUP_ID");

    let working_directory = options
        .working_directory
        .as_ref()
        .filter(|dir| dir.is_dir())
        .and_then(|dir| CString::new(dir.as_os_str().as_bytes()).ok());

    unsafe {
        builder.pre_exec(move || {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            if let Some(dir) = working_directory.as_ref() {
                libc::chdir(dir.as_ptr());
            }
            #[allow(clippy::cast_lossless)]
            if libc::ioctl(slave_fd, libc::TIOCSCTTY as _, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            libc::close(slave_fd);
            libc::close(master_fd);
            for signal in [
                libc::SIGCHLD,
                libc::SIGHUP,
                libc::SIGINT,
                libc::SIGQUIT,
                libc::SIGTERM,
                libc::SIGALRM,
            ] {
                libc::signal(signal, libc::SIG_DFL);
            }
            Ok(())
        });
    }

    let child = builder.spawn().map_err(|err| {
        io::Error::new(
            err.kind(),
            format!(
                "Failed to spawn command '{}': {err}",
                builder.get_program().to_string_lossy()
            ),
        )
    })?;

    unsafe {
        let flags = libc::fcntl(master_fd, libc::F_GETFL, 0);
        libc::fcntl(master_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }
    let pidfd = pidfd_open(child.id());

    Ok(PtySource {
        child: Some(child),
        master: Some(Arc::new(File::from(master))),
        pidfd,
    })
}

/// alacritty's `setup_env`, verbatim in effect: `TERM` is `alacritty` when that
/// terminfo entry is installed and `xterm-256color` otherwise, and
/// `COLORTERM=truecolor` advertises 24-bit colour. Changing what TD calls
/// itself is its own decision; this change keeps it where it was.
pub fn setup_env() {
    let terminfo = if terminfo_exists("alacritty") {
        "alacritty"
    } else {
        "xterm-256color"
    };
    unsafe {
        env::set_var("TERM", terminfo);
        env::set_var("COLORTERM", "truecolor");
    }
}

fn terminfo_exists(terminfo: &str) -> bool {
    let first = terminfo.get(..1).unwrap_or_default();
    let first_hex = format!("{:x}", first.chars().next().unwrap_or_default() as usize);
    let found = |dir: PathBuf| {
        dir.join(first).join(terminfo).exists() || dir.join(&first_hex).join(terminfo).exists()
    };
    if let Some(dir) = env::var_os("TERMINFO") {
        if found(PathBuf::from(dir)) {
            return true;
        }
    } else if let Some(home) = env::var_os("HOME") {
        if found(PathBuf::from(home).join(".terminfo")) {
            return true;
        }
    }
    if let Ok(dirs) = env::var("TERMINFO_DIRS") {
        for dir in dirs.split(':') {
            if found(PathBuf::from(dir)) {
                return true;
            }
        }
    }
    if let Ok(prefix) = env::var("PREFIX") {
        let prefix = PathBuf::from(prefix);
        if found(prefix.join("etc/terminfo"))
            || found(prefix.join("lib/terminfo"))
            || found(prefix.join("share/terminfo"))
        {
            return true;
        }
    }
    [
        "/etc/terminfo",
        "/lib/terminfo",
        "/usr/share/terminfo",
        "/boot/system/data/terminfo",
    ]
    .into_iter()
    .any(|dir| found(PathBuf::from(dir)))
}

fn winsize(size: WindowSize) -> Winsize {
    let ws_row = size.num_lines;
    let ws_col = size.num_cols;
    Winsize {
        ws_row,
        ws_col,
        ws_xpixel: ws_col.saturating_mul(size.cell_width),
        ws_ypixel: ws_row.saturating_mul(size.cell_height),
    }
}

fn pidfd_open(pid: u32) -> Option<OwnedFd> {
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) };
    if fd < 0 {
        // Survivable — the end of the stream stands in for the child's exit —
        // but a terminal that loses its pidfds to a descriptor limit or a
        // sandbox has changed how it notices a program end, so it is said.
        eprintln!(
            "td: no pidfd for process {pid} ({}); its exit will be noticed when its terminal closes",
            std::io::Error::last_os_error()
        );
        return None;
    }
    Some(unsafe { OwnedFd::from_raw_fd(fd as i32) })
}

/// Who the shell runs as: the environment first, the password database when
/// the environment does not say.
struct ShellUser {
    user: String,
    home: String,
    shell: String,
}

impl ShellUser {
    fn from_env() -> io::Result<Self> {
        let mut buf = [0; 1024];
        let pw = passwd_entry(&mut buf);
        let pick =
            |var: &str, from_pw: for<'a> fn(&'a Passwd<'_>) -> &'a str| -> io::Result<String> {
                match env::var(var) {
                    Ok(value) => Ok(value),
                    Err(_) => match pw.as_ref() {
                        Ok(pw) => Ok(from_pw(pw).to_owned()),
                        Err(err) => Err(io::Error::new(err.kind(), err.to_string())),
                    },
                }
            };
        Ok(Self {
            user: pick("USER", |pw| pw.name)?,
            home: pick("HOME", |pw| pw.dir)?,
            shell: pick("SHELL", |pw| pw.shell)?,
        })
    }
}

struct Passwd<'a> {
    name: &'a str,
    dir: &'a str,
    shell: &'a str,
}

fn passwd_entry(buf: &mut [libc::c_char; 1024]) -> io::Result<Passwd<'_>> {
    let mut entry: MaybeUninit<libc::passwd> = MaybeUninit::uninit();
    let mut res: *mut libc::passwd = ptr::null_mut();
    let uid = unsafe { libc::getuid() };
    let status = unsafe {
        libc::getpwuid_r(
            uid,
            entry.as_mut_ptr(),
            buf.as_mut_ptr(),
            buf.len(),
            &mut res,
        )
    };
    if status < 0 {
        return Err(io::Error::other("getpwuid_r failed"));
    }
    if res.is_null() {
        return Err(io::Error::other("pw not found"));
    }
    let entry = unsafe { entry.assume_init() };
    // Safety: the entry's strings point into `buf`, which outlives the result.
    unsafe {
        Ok(Passwd {
            name: text(entry.pw_name),
            dir: text(entry.pw_dir),
            shell: text(entry.pw_shell),
        })
    }
}

/// A C string from the password entry, as UTF-8.
///
/// # Safety
///
/// `p` must point to a NUL-terminated string that lives for `'a`.
unsafe fn text<'a>(p: *const libc::c_char) -> &'a str {
    unsafe { CStr::from_ptr(p).to_str().unwrap_or_default() }
}
