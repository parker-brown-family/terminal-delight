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
use std::{env, ptr};

use rustix_openpty::openpty;
use rustix_openpty::rustix::termios::{self, InputModes, OptionalActions, Winsize};

use super::pump::Source;
use super::WindowSize;

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
    child: Child,
    master: File,
    /// Readable once the child has exited. `None` on a kernel without
    /// `pidfd_open` (older than 5.3), where the end of the stream is the only
    /// signal.
    pidfd: Option<OwnedFd>,
}

impl PtySource {
    pub fn child(&self) -> &Child {
        &self.child
    }

    pub fn file(&self) -> &File {
        &self.master
    }
}

impl Source for PtySource {
    fn fd(&self) -> BorrowedFd<'_> {
        self.master.as_fd()
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.master.read(buf)
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.master.write(buf)
    }

    fn resize(&mut self, size: WindowSize) -> io::Result<()> {
        let win = winsize(size);
        let res = unsafe {
            libc::ioctl(
                self.master.as_raw_fd(),
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
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status),
            // Without a pidfd this is only asked once the stream has closed,
            // when the child is on its way out; waiting for it is then brief
            // and is the only way to learn its status.
            Ok(None) if self.pidfd.is_none() => self.child.wait().ok(),
            _ => None,
        }
    }
}

impl Drop for PtySource {
    fn drop(&mut self) {
        // alacritty's hang-up on the way out: the shell is told its terminal
        // has gone, and waited for so it does not linger as a zombie.
        unsafe {
            libc::kill(self.child.id() as i32, libc::SIGHUP);
        }
        let _ = self.child.wait();
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
        child,
        master: File::from(master),
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
