//! An opt-in offline command boundary. Standard terminals retain account access.
//!
//! The supervisor is outside the sandbox. The child receives one pinned work
//! directory and a per-run typed gateway, never the operator socket or generic
//! TD control. All child I/O is piped; output is text, not terminal protocol.
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::os::unix::net::{UnixDatagram, UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

mod seccomp;
const MAX_REQUEST: usize = 2048;
const TIMEOUT: Duration = Duration::from_millis(250);
const HELP: &str =
    "terminal-delight restricted run --workdir PATH [--read-only] [--td-note] -- PROGRAM ARGS...\n\
terminal-delight restricted status RUN\n\
terminal-delight restricted revoke RUN\n\
Inside the command: terminal-delight restricted call '{\"op\":\"status\"}'\n\
  also {\"op\":\"note\",\"text\":\"...\"} (with --td-note; 1-10 words, one a second)\n\
  and {\"op\":\"request_access\",\"reason\":\"...\"} (a proposal; it grants nothing)\n\n\
Needs /usr/bin/bwrap 0.12.0+, Linux 5.11+, x86_64 or aarch64; otherwise no command starts.\n\
Offline, line-oriented commands only. /work is the chosen directory; home and\n\
tmp are temporary. No network, host services, credentials outside the chosen\n\
directory (anything inside it is granted), TUIs or automatic agent handoffs. Output is plain text. --td-note grants only an own-pane note,\n\
subject to the existing MCP switches. Status shows one pending access request;\n\
wider access needs an explicit new run. Standard terminals keep account access.\n\
Shared writable output can influence a later privileged consumer. This is not\n\
protection from host-user malware, kernel bugs or complete resource exhaustion.";

#[derive(Debug, PartialEq)]
struct Launch {
    work: PathBuf,
    read_only: bool,
    note: bool,
    argv: Vec<String>,
}

fn parse_run(args: &[String]) -> Result<Launch, String> {
    let mut work = None;
    let mut read_only = false;
    let mut note = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--workdir" if work.is_none() => {
                work = Some(PathBuf::from(it.next().ok_or("--workdir needs a path")?));
            }
            "--read-only" if !read_only => read_only = true,
            "--td-note" if !note => note = true,
            "--" => {
                let argv: Vec<_> = it.cloned().collect();
                if argv.is_empty() {
                    return Err("name the program after --; no command was started".into());
                }
                return Ok(Launch {
                    work: work.ok_or("--workdir is required")?,
                    read_only,
                    note,
                    argv,
                });
            }
            _ => return Err(format!("unsupported or repeated option {arg:?}")),
        }
    }
    Err("run needs --workdir PATH -- PROGRAM ARGS...".into())
}

fn private_dir(path: &Path) -> io::Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => (),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e),
    }
    let m = fs::symlink_metadata(path)?;
    // Do not follow a pre-created symlink or silently repair someone else's dir.
    if !m.is_dir() || m.uid() != unsafe { libc::getuid() } || m.mode() & 0o077 != 0 {
        return Err(io::Error::other(
            "restricted runtime must be an owned 0700 directory",
        ));
    }
    Ok(())
}

fn root() -> io::Result<PathBuf> {
    let protected = PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() }));
    private_dir(&protected)?;
    let protected = fs::canonicalize(protected)?;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| protected.clone());
    private_dir(&runtime)?;
    let runtime = fs::canonicalize(runtime)?;
    // All workloads exclude the whole fixed root, including other runs' setup
    // files and operator sockets. Overrides into a shared work tree are unsafe.
    if !runtime.starts_with(&protected) {
        return Err(io::Error::other(
            "XDG_RUNTIME_DIR must be inside the protected /run/user/UID root; no fallback",
        ));
    }
    let td = runtime.join("terminal-delight");
    private_dir(&td)?;
    let root = td.join("restricted");
    private_dir(&root)?;
    fs::canonicalize(root)
}

struct RunDir(PathBuf);
impl RunDir {
    fn create(root: &Path) -> io::Result<Self> {
        let mut random = [0; 16];
        File::open("/dev/urandom")?.read_exact(&mut random)?;
        let id: String = random.iter().map(|n| format!("{n:02x}")).collect();
        let path = root.join(id);
        // Collision fails, rather than adopting an existing grant directory.
        fs::DirBuilder::new().mode(0o700).create(&path)?;
        Ok(Self(path))
    }

    fn id(&self) -> &str {
        self.0
            .file_name()
            .and_then(|s| s.to_str())
            .expect("hex run id")
    }
}
impl Drop for RunDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn overlaps(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

fn account_home() -> io::Result<PathBuf> {
    use std::ffi::{CStr, OsString};
    use std::os::unix::ffi::OsStringExt;
    let mut storage = vec![0u8; 65536];
    // SAFETY: getpwuid_r writes within the live buffer and initialized output.
    // Copy its result before storage drops. HOME is not identity evidence.
    unsafe {
        let mut entry: libc::passwd = std::mem::zeroed();
        let mut found = std::ptr::null_mut();
        let code = libc::getpwuid_r(
            libc::getuid(),
            &mut entry,
            storage.as_mut_ptr().cast(),
            storage.len(),
            &mut found,
        );
        if code != 0 {
            return Err(io::Error::from_raw_os_error(code));
        }
        if found.is_null() || entry.pw_dir.is_null() {
            return Err(io::Error::other(
                "account home unavailable; Restricted launch refused",
            ));
        }
        Ok(PathBuf::from(OsString::from_vec(
            CStr::from_ptr(entry.pw_dir).to_bytes().to_vec(),
        )))
    }
}

/// Pin the resource before starting user code; a renamed path cannot substitute
/// another directory between validation and bubblewrap's mount.
fn work_dir(path: &Path, runtime: &Path, home: Option<&Path>) -> io::Result<(PathBuf, File)> {
    let canonical = fs::canonicalize(path)?;
    let pinned = File::open(&canonical)?;
    if !pinned.metadata()?.is_dir() {
        return Err(io::Error::other("workdir must be a directory"));
    }
    for system in [
        "/", "/home", "/tmp", "/var", "/run", "/etc", "/usr", "/bin", "/lib", "/lib64", "/proc",
        "/sys", "/dev",
    ] {
        let p = Path::new(system);
        if canonical == p
            || (!matches!(system, "/" | "/home" | "/tmp" | "/var") && canonical.starts_with(p))
        {
            return Err(io::Error::other(
                "choose a dedicated work directory, not a system directory",
            ));
        }
    }
    if overlaps(&canonical, runtime) {
        return Err(io::Error::other("workdir overlaps protected runtime state"));
    }
    if let Some(home) = home {
        let home = fs::canonicalize(home)?;
        if home.starts_with(&canonical) {
            return Err(io::Error::other("grant a project directory, not your home"));
        }
        for name in [
            ".ssh", ".gnupg", ".config", ".local", ".claude", ".codex", ".gemini",
        ] {
            if overlaps(&canonical, &home.join(name)) {
                return Err(io::Error::other("workdir overlaps protected user state"));
            }
        }
    }
    // A pre-existing socket/device/submount in a chosen tree is another host
    // channel, not an ordinary file grant. Do not follow symlinks while scanning.
    let mounts = fs::read_to_string("/proc/self/mountinfo")?;
    for line in mounts.lines() {
        if let Some(mount) = line.split_whitespace().nth(4) {
            let mount = mount
                .replace("\\040", " ")
                .replace("\\011", "\t")
                .replace("\\012", "\n")
                .replace("\\134", "\\");
            let mount = Path::new(&mount);
            if mount != canonical && mount.starts_with(&canonical) {
                return Err(io::Error::other("workdir contains a nested host mount"));
            }
        }
    }
    let mut pending = vec![canonical.clone()];
    let mut seen = 0;
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            seen += 1;
            if seen > 200_000 {
                return Err(io::Error::other(
                    "workdir validation exceeds 200000 entries; choose a smaller tree",
                ));
            }
            let ty = entry.file_type()?;
            if ty.is_dir() {
                pending.push(entry.path());
            } else if ty.is_file() && entry.metadata()?.nlink() != 1 {
                return Err(io::Error::other(
                    "workdir contains a hard-linked file; use an independent copy",
                ));
            } else if !(ty.is_file() || ty.is_symlink()) {
                return Err(io::Error::other(
                    "workdir contains a socket, pipe or device; remove host channels first",
                ));
            }
        }
    }
    Ok((canonical, pinned))
}

fn sandbox(
    launch: &Launch,
    pinned: &File,
    run: &RunDir,
    gateway: &UnixDatagram,
    filter: &File,
) -> io::Result<Command> {
    use std::os::unix::process::CommandExt;
    // An absolute system executable: a project's PATH cannot replace the
    // enforcement component. Missing bubblewrap is an unavailable launch.
    let version = Command::new("/usr/bin/bwrap")
        .env_clear()
        .current_dir("/")
        .arg("--version")
        .output()?;
    // 0.12.0 includes the safe setup-path resolution fix (GHSA-pxhw-h44j-8pfx)
    // and descriptor mounts. Older or unrecognizable helpers are unavailable.
    if !version.status.success()
        || !supported_bwrap_version(&String::from_utf8_lossy(&version.stdout))
    {
        return Err(io::Error::other(
            "Restricted requires bubblewrap 0.12.0 or newer",
        ));
    }
    let mut cmd = Command::new("/usr/bin/bwrap");
    cmd.env_clear().current_dir("/");
    cmd.args([
        "--unshare-user",
        // No user namespace inside this one: a nested namespace would hand the
        // workload a full capability set against the kernel's namespaced code,
        // the usual first step of an exploit, and no offline build needs it.
        "--disable-userns",
        // The host's cgroup path names the desktop session and its scopes.
        "--unshare-cgroup",
        "--unshare-pid",
        "--unshare-net",
        "--unshare-ipc",
        "--unshare-uts",
        "--die-with-parent",
        "--new-session",
        "--cap-drop",
        "ALL",
        "--clearenv",
        "--ro-bind",
        "/usr",
        "/usr",
    ]);
    for path in ["/bin", "/lib", "/lib64"] {
        if Path::new(path).exists() {
            cmd.args(["--ro-bind", path, path]);
        }
    }
    // Debian system-tool aliases (including cc) resolve through alternatives.
    // Expose those read-only aliases, not the rest of host /etc.
    for path in ["/etc/ld.so.cache", "/etc/alternatives"] {
        if Path::new(path).exists() {
            cmd.args(["--ro-bind", path, path]);
        }
    }
    cmd.args([
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--size",
        "67108864",
        "--tmpfs",
        "/tmp",
        "--size",
        "67108864",
        "--tmpfs",
        "/home/td",
        "--dir",
        "/run/td/bin",
        "--setenv",
        "PATH",
        "/run/td/bin:/usr/bin:/bin",
        "--setenv",
        "HOME",
        "/home/td",
        "--setenv",
        "TMPDIR",
        "/tmp",
        "--setenv",
        "LANG",
        "C.UTF-8",
        "--setenv",
        "TERM",
        "dumb",
        "--setenv",
        "TD_RESTRICTED",
        "offline-v1",
    ]);
    // bubblewrap consumes and closes this explicitly preserved setup FD. A
    // parent-/proc path is denied by ptrace policy after user-namespace entry.
    // Unknown --bind-fd on an older bwrap is a fail-closed unavailable profile.
    let work_fd = pinned.as_raw_fd();
    let gateway_fd = gateway.as_raw_fd();
    let filter_fd = filter.as_raw_fd();
    cmd.args([
        "--setenv",
        "TD_GATEWAY_FD",
        &gateway_fd.to_string(),
        "--seccomp",
        &filter_fd.to_string(),
    ]);
    cmd.arg(if launch.read_only {
        "--ro-bind-fd"
    } else {
        "--bind-fd"
    })
    .arg(work_fd.to_string())
    .arg("/work");
    // SAFETY: after fork, mark every non-stdio FD CLOEXEC, including descriptors
    // inherited from a caller that forgot that flag. Preserve only the pinned
    // setup FD, which bwrap consumes and closes. These syscalls do not allocate
    // or take Rust locks, and affect only this child's descriptor table.
    unsafe {
        cmd.pre_exec(move || {
            // CLOSE_RANGE_CLOEXEC (Linux 5.11+). Unsupported kernels refuse the
            // launch rather than falling back to leaking caller descriptors.
            if libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, 4u32) == -1 {
                return Err(io::Error::last_os_error());
            }
            for fd in [work_fd, gateway_fd, filter_fd] {
                if libc::fcntl(fd, libc::F_SETFD, 0) == -1 {
                    return Err(io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    cmd.arg("--ro-bind")
        .arg(std::env::current_exe()?)
        .arg("/run/td/bin/terminal-delight");
    for (name, dest) in [("passwd", "/etc/passwd"), ("group", "/etc/group")] {
        cmd.arg("--ro-bind").arg(run.0.join(name)).arg(dest);
    }
    cmd.args(["--chdir", "/work", "--"]).args(&launch.argv);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(cmd)
}

fn supported_bwrap_version(version: &str) -> bool {
    let Some(version) = version.trim().strip_prefix("bubblewrap ") else {
        return false;
    };
    let parts: Vec<_> = version.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    let (Ok(major), Ok(minor), Ok(patch)) = (
        parts[0].parse::<u32>(),
        parts[1].parse::<u32>(),
        parts[2].split('-').next().unwrap_or("").parse::<u32>(),
    ) else {
        return false;
    };
    (major, minor, patch) >= (0, 12, 0)
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Status,
    Note {
        text: String,
    },
    #[serde(rename = "request_access")]
    AskAccess {
        reason: String,
    },
    Revoke,
}

struct Grants {
    revoked: bool,
    note: bool,
    pending: Option<String>,
    last_note: Option<Instant>,
    work: PathBuf,
    read_only: bool,
}

fn failure(reason: impl ToString) -> Value {
    json!({"ok":false, "error":reason.to_string()})
}

fn text_ok(text: &str, words: usize) -> bool {
    !text.trim().is_empty()
        && text.len() <= 512
        && text.split_whitespace().count() <= words
        && !text.chars().any(char::is_control)
}

impl Grants {
    fn status(&self) -> Value {
        json!({"ok":true, "profile":"offline-v1", "workdir":self.work,
            "work_access":if self.read_only {"read_only"} else {"read_write"},
            "network":"none", "host_services":"none", "output":"plain_text",
            "global_mcp_enabled":Value::Null, "global_mcp_writes":Value::Null, "global_policy_check":"on_each_note",
            "system_read_only_policy":"/usr plus existing system bin/lib paths and linker cache",
            "note":if self.revoked {"revoked"} else if self.note {"granted"} else {"not_granted"},
            "automatic_handoffs":"unavailable", "pending_access_request":self.pending,
            "wider_access":"explicit_new_run_required", "resource_quotas":"unavailable"})
    }

    fn apply(
        &mut self,
        req: Request,
        operator: bool,
        note: &dyn Fn(&str) -> Result<(), String>,
    ) -> Value {
        match req {
            Request::Status => self.status(),
            Request::Revoke if operator => {
                self.revoked = true;
                self.pending = None;
                self.status()
            }
            Request::Revoke => failure("only the outside operator channel may revoke grants"),
            Request::Note { .. } | Request::AskAccess { .. } if operator => failure(
                "operator channel accepts only status and revoke; new grants require a new run",
            ),
            Request::Note { text } => {
                if self.revoked || !self.note {
                    return failure("own-pane note access is not granted or has been revoked");
                }
                if !text_ok(&text, 10) {
                    return failure(
                        "a note needs 1-10 words, at most 512 bytes, without control characters",
                    );
                }
                if self
                    .last_note
                    .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
                {
                    return failure("note rate limit: at most one attempt per second");
                }
                self.last_note = Some(Instant::now());
                match note(&text) {
                    Ok(()) => json!({"ok":true}),
                    Err(e) => failure(e),
                }
            }
            Request::AskAccess { reason } => {
                if self.revoked {
                    return failure("this run's TD access has been revoked");
                }
                if !text_ok(&reason, 40) {
                    return failure(
                        "a reason needs 1-40 words, at most 512 bytes, without control characters",
                    );
                }
                if let Some(pending) = &self.pending {
                    if pending != &reason {
                        return failure(
                            "one access request is already pending; scope has not changed",
                        );
                    }
                } else {
                    self.pending = Some(reason);
                }
                json!({"ok":true,"pending":true,"granted":false,"requires_new_run":true})
            }
        }
    }
}

/// Read exactly one bounded line. An endless or slow peer cannot retain the
/// broker indefinitely or allocate an unbounded JSON string.
fn read_frame(stream: &mut UnixStream) -> io::Result<Vec<u8>> {
    stream.set_read_timeout(Some(TIMEOUT))?;
    let deadline = Instant::now() + TIMEOUT;
    let mut bytes = Vec::new();
    let mut byte = [0];
    loop {
        if Instant::now() >= deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "request deadline"));
        }
        if stream.read(&mut byte)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "request needs a newline",
            ));
        }
        if byte[0] == b'\n' {
            return Ok(bytes);
        }
        if bytes.len() == MAX_REQUEST {
            return Err(io::Error::other("request exceeds 2048 bytes"));
        }
        bytes.push(byte[0]);
    }
}

pub(crate) type Note = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;
struct Broker {
    gateway: UnixDatagram,
    stop: Arc<AtomicBool>,
    threads: Vec<thread::JoinHandle<()>>,
}
fn dispatch(bytes: &[u8], grants: &Mutex<Grants>, operator: bool, note: &Note) -> Value {
    if bytes.len() > MAX_REQUEST {
        return failure("request exceeds 2048 bytes");
    }
    match serde_json::from_slice::<Request>(bytes) {
        Ok(req) => grants.lock().expect("grants").apply(req, operator, &**note),
        Err(_) => failure("unknown operation, field or invalid JSON"),
    }
}
impl Broker {
    fn start(run: &RunDir, grants: Grants, note: Note) -> io::Result<Self> {
        let (server, gateway) = UnixDatagram::pair()?;
        server.set_read_timeout(Some(Duration::from_millis(100)))?;
        server.set_write_timeout(Some(TIMEOUT))?;
        let operator = UnixListener::bind(run.0.join("operator.sock"))?;
        operator.set_nonblocking(true)?;
        let grants = Arc::new(Mutex::new(grants));
        let stop = Arc::new(AtomicBool::new(false));
        let mut threads = Vec::new();
        {
            let (grants, stop, note) = (grants.clone(), stop.clone(), note.clone());
            threads.push(thread::spawn(move || {
                let mut bytes = [0; MAX_REQUEST + 1];
                while !stop.load(Ordering::Acquire) {
                    match server.recv(&mut bytes) {
                        Ok(n) => {
                            let result = dispatch(&bytes[..n], &grants, false, &note);
                            let _ = server.send(result.to_string().as_bytes());
                        }
                        Err(e)
                            if matches!(
                                e.kind(),
                                io::ErrorKind::WouldBlock
                                    | io::ErrorKind::TimedOut
                                    | io::ErrorKind::Interrupted
                            ) => {}
                        Err(_) => break,
                    }
                }
            }));
        }
        {
            let (grants, stop) = (grants.clone(), stop.clone());
            threads.push(thread::spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    match operator.accept() {
                        Ok((mut stream, _)) => {
                            let _ = stream.set_write_timeout(Some(TIMEOUT));
                            let result = match read_frame(&mut stream) {
                                Ok(bytes) => dispatch(&bytes, &grants, true, &note),
                                Err(e) => failure(e),
                            };
                            let _ = writeln!(stream, "{result}");
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(_) => break,
                    }
                }
            }));
        }
        Ok(Self {
            gateway,
            stop,
            threads,
        })
    }
}
impl Drop for Broker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

/// Decode a bounded stream incrementally, preserving split UTF-8. Never pass
/// ESC, C1 or other control characters to the outer terminal, on either stream.
fn text_stream(mut input: impl Read, mut output: impl Write) -> io::Result<()> {
    let mut pending = Vec::new();
    let mut buf = [0; 4096];
    loop {
        let count = input.read(&mut buf)?;
        pending.extend_from_slice(&buf[..count]);
        let mut used = 0;
        while used < pending.len() {
            match std::str::from_utf8(&pending[used..]) {
                Ok(s) => {
                    write_text(s, &mut output)?;
                    used = pending.len();
                }
                Err(e) => {
                    let good = e.valid_up_to();
                    write_text(
                        std::str::from_utf8(&pending[used..used + good]).expect("valid prefix"),
                        &mut output,
                    )?;
                    used += good;
                    if let Some(bad) = e.error_len() {
                        output.write_all("\u{fffd}".as_bytes())?;
                        used += bad;
                    } else if count == 0 {
                        output.write_all("\u{fffd}".as_bytes())?;
                        used = pending.len();
                    } else {
                        break;
                    }
                }
            }
        }
        pending.drain(..used);
        output.flush()?;
        if count == 0 {
            return Ok(());
        }
    }
}

/// Bidirectional controls and the line/paragraph separators reorder or break
/// what a reader sees without being terminal protocol: a line could display a
/// different command than the one a person copies from it.
fn reorders_display(c: char) -> bool {
    matches!(
        c,
        '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
            | '\u{2028}' | '\u{2029}'
    )
}

fn write_text(s: &str, out: &mut impl Write) -> io::Result<()> {
    for ch in s
        .chars()
        .filter(|c| (!c.is_control() || matches!(c, '\n' | '\r' | '\t')) && !reorders_display(*c))
    {
        out.write_all(ch.encode_utf8(&mut [0; 4]).as_bytes())?;
    }
    Ok(())
}

fn input_stream(mut output: impl Write, stop: &AtomicBool) {
    let mut buf = [0; 4096];
    while !stop.load(Ordering::Acquire) {
        let mut poll = libc::pollfd {
            fd: 0,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: poll points to one initialized descriptor; read writes only
        // into this buffer. No borrowed Rust stdin lock is shared with a thread.
        let ready = unsafe { libc::poll(&mut poll, 1, 100) };
        if ready < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return;
        }
        if ready == 0 {
            continue;
        }
        let count = unsafe { libc::read(0, buf.as_mut_ptr().cast(), buf.len()) };
        if count <= 0 || output.write_all(&buf[..count as usize]).is_err() {
            return;
        }
    }
}

fn run(launch: Launch) -> Result<i32, String> {
    // This qualified profile assumes an ordinary unprivileged account.
    if unsafe {
        libc::getuid() == 0
            || libc::getuid() != libc::geteuid()
            || libc::getgid() != libc::getegid()
    } {
        return Err(
            "Restricted is unavailable for root or set-ID execution; no command started".into(),
        );
    }
    let runtime = root().map_err(|e| e.to_string())?;
    let home = account_home().map_err(|e| e.to_string())?;
    if !home.is_absolute() || home == Path::new("/") {
        return Err("account home cannot be safely established; no command started".into());
    }
    let protected_runtime = fs::canonicalize(format!("/run/user/{}", unsafe { libc::getuid() }))
        .map_err(|e| e.to_string())?;
    let (work, pinned) =
        work_dir(&launch.work, &protected_runtime, Some(&home)).map_err(|e| e.to_string())?;
    for key in ["XDG_CONFIG_HOME", "XDG_STATE_HOME"] {
        if let Some(path) = std::env::var_os(key).filter(|s| !s.is_empty()) {
            let protected = fs::canonicalize(path)
                .map_err(|e| format!("{key} unavailable; launch refused: {e}"))?;
            if overlaps(&work, &protected) {
                return Err(format!(
                    "workdir overlaps protected {key}; choose an independent work tree"
                ));
            }
        }
    }
    // Resolve the outside target before executing untrusted code. The closure
    // holds only this grant; request content cannot select a pane or operation.
    let note: Note = if launch.note {
        crate::ctl::restricted_note_target()?
    } else {
        Arc::new(|_| Err("own-pane note access was not granted".into()))
    };
    let run = RunDir::create(&runtime).map_err(|e| e.to_string())?;
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    fs::write(
        run.0.join("passwd"),
        format!("td:x:{uid}:{gid}:Restricted:/home/td:/bin/sh\n"),
    )
    .map_err(|e| e.to_string())?;
    fs::write(run.0.join("group"), format!("td:x:{gid}:\n")).map_err(|e| e.to_string())?;
    let broker = Broker::start(
        &run,
        Grants {
            revoked: false,
            note: launch.note,
            pending: None,
            last_note: None,
            work: work.clone(),
            read_only: launch.read_only,
        },
        note,
    )
    .map_err(|e| e.to_string())?;
    eprintln!(
        "Starting Restricted offline run {}: work={} access={} TD={} output=plain-text",
        run.id(),
        json!(work),
        if launch.read_only {
            "read-only"
        } else {
            "read-write"
        },
        if launch.note { "own-note" } else { "none" }
    );
    eprintln!("Inspect or revoke from Standard: terminal-delight restricted status {} / restricted revoke {}", run.id(), run.id());
    let filter = seccomp::write(&run.0.join("syscalls.bpf")).map_err(|e| e.to_string())?;
    let mut child = sandbox(&launch, &pinned, &run, &broker.gateway, &filter)
        .map_err(|e| e.to_string())?
        .spawn()
        .map_err(|e| format!("Restricted unavailable; no Standard fallback: {e}"))?;
    let input = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let stop = AtomicBool::new(false);
    let result = thread::scope(|scope| {
        let incoming = scope.spawn(|| input_stream(input, &stop));
        let out = scope.spawn(|| text_stream(stdout, io::stdout()));
        let err = scope.spawn(|| text_stream(stderr, io::stderr()));
        let status = child.wait();
        stop.store(true, Ordering::Release);
        let _ = incoming.join();
        // On a closed parent pipe, end the sandbox via --die-with-parent when
        // the CLI exits. Other I/O failures are returned, not silent success.
        out.join()
            .map_err(|_| "stdout relay failed".to_string())?
            .map_err(|e| e.to_string())?;
        err.join()
            .map_err(|_| "stderr relay failed".to_string())?
            .map_err(|e| e.to_string())?;
        status.map_err(|e| e.to_string())
    })?;
    use std::os::unix::process::ExitStatusExt;
    Ok(result
        .code()
        .unwrap_or_else(|| 128 + result.signal().unwrap_or(0)))
}

fn exchange(path: &Path, frame: &str) -> Result<Value, String> {
    if frame.len() > MAX_REQUEST || frame.contains('\n') {
        return Err("request must be one JSON line of at most 2048 bytes".into());
    }
    let mut stream = UnixStream::connect(path).map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(TIMEOUT))
        .map_err(|e| e.to_string())?;
    writeln!(stream, "{frame}").map_err(|e| e.to_string())?;
    // Status can include one bounded reason plus paths; cap the response too.
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    let mut byte = [0];
    while bytes.len() < 8192 {
        if stream.read(&mut byte).map_err(|e| e.to_string())? == 0 || byte[0] == b'\n' {
            return serde_json::from_slice(&bytes).map_err(|e| e.to_string());
        }
        bytes.push(byte[0]);
    }
    Err("response exceeds 8192 bytes".into())
}

fn packet_exchange(socket: &UnixDatagram, frame: &str) -> Result<Value, String> {
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    socket
        .set_write_timeout(Some(TIMEOUT))
        .map_err(|e| e.to_string())?;
    socket.send(frame.as_bytes()).map_err(|e| e.to_string())?;
    let mut buf = [0; 8192];
    let n = socket.recv(&mut buf).map_err(|e| e.to_string())?;
    serde_json::from_slice(&buf[..n]).map_err(|e| e.to_string())
}
fn gateway_call(frame: &str) -> Result<Value, String> {
    if frame.len() > MAX_REQUEST || frame.contains('\n') {
        return Err("one JSON request of at most 2048 bytes is required".into());
    }
    let fd: i32 = std::env::var("TD_GATEWAY_FD")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|n| *n >= 3)
        .ok_or("no inherited Restricted gateway; never reconnect through host sockets")?;
    // Each CLI opens its own lock description. It serializes cooperative calls
    // within this one workload; the lock never grants authority or isolates its
    // mutually authorized children from one another.
    let lock = File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open("/tmp/.td-gateway-lock")
        .map_err(|e| e.to_string())?;
    // SAFETY: duplicate a live descriptor and transfer only the duplicate to
    // Rust ownership. getsockopt checks its type before any request is sent.
    let socket = unsafe {
        let copy = libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3);
        if copy < 0 {
            return Err(io::Error::last_os_error().to_string());
        }
        let socket = UnixDatagram::from_raw_fd(copy);
        let mut ty = 0i32;
        let mut len = std::mem::size_of_val(&ty) as libc::socklen_t;
        if libc::getsockopt(
            copy,
            libc::SOL_SOCKET,
            libc::SO_TYPE,
            (&mut ty as *mut i32).cast(),
            &mut len,
        ) != 0
            || ty != libc::SOCK_DGRAM
        {
            return Err("inherited gateway is not a datagram socket".into());
        }
        if libc::flock(lock.as_raw_fd(), libc::LOCK_EX) != 0 {
            return Err(io::Error::last_os_error().to_string());
        }
        socket
    };
    packet_exchange(&socket, frame)
}

fn operator_path(id: &str) -> Result<PathBuf, String> {
    if id.len() != 32
        || !id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err("RUN must be the 32-character lowercase run ID printed at launch".into());
    }
    Ok(root()
        .map_err(|e| e.to_string())?
        .join(id)
        .join("operator.sock"))
}

pub fn run_cli(args: &[String]) -> i32 {
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["help"] | ["--help"] => {
            println!("{HELP}");
            return 0;
        }
        ["run", ..] => parse_run(&args[1..]).and_then(run),
        [verb @ ("status" | "revoke"), id] => operator_path(id).and_then(|path| {
            exchange(&path, &json!({"op":verb}).to_string()).map(|v| {
                println!("{v}");
                if v["ok"] == true {
                    0
                } else {
                    2
                }
            })
        }),
        ["call", frame] => gateway_call(frame).map(|v| {
            println!("{v}");
            if v["ok"] == true {
                0
            } else {
                2
            }
        }),
        _ => Err(format!("unsupported restricted command\n{HELP}")),
    };
    match result {
        Ok(code) => code,
        Err(why) => {
            eprintln!("terminal-delight restricted: {why}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_helper_must_include_safe_setup_path_resolution() {
        for version in [
            "bubblewrap 0.12.0\n",
            "bubblewrap 0.12.1-1",
            "bubblewrap 1.0.0",
        ] {
            assert!(super::supported_bwrap_version(version), "{version}");
        }
        for version in [
            "bubblewrap 0.11.1",
            "bubblewrap 0.9.0",
            "",
            "0.12.0",
            "bubblewrap 0.12",
            "bubblewrap 0.12.x",
        ] {
            assert!(!super::supported_bwrap_version(version), "{version}");
        }
    }
    use super::*;
    use std::os::unix::fs::symlink;
    use std::sync::atomic::AtomicUsize;

    fn fixture() -> RunDir {
        RunDir::create(&std::env::temp_dir()).unwrap()
    }

    fn grants(note: bool) -> Grants {
        Grants {
            revoked: false,
            note,
            pending: None,
            last_note: None,
            work: PathBuf::from("/chosen/work"),
            read_only: false,
        }
    }

    #[test]
    fn a_restricted_launch_is_explicit_and_never_interprets_command_arguments() {
        let args = [
            "--workdir",
            "/some work",
            "--td-note",
            "--read-only",
            "--",
            "sh",
            "-c",
            "$(literal)",
        ]
        .map(String::from);
        let launch = parse_run(&args).unwrap();
        assert_eq!(launch.work, Path::new("/some work"));
        assert!(launch.note && launch.read_only);
        assert_eq!(launch.argv, ["sh", "-c", "$(literal)"]);
        for args in [
            vec!["--", "sh"],
            vec!["--workdir", "/x", "--"],
            vec!["--workdir", "/x", "--network", "--", "sh"],
            vec!["--workdir", "/x", "--workdir", "/y", "--", "sh"],
        ] {
            assert!(parse_run(&args.into_iter().map(String::from).collect::<Vec<_>>()).is_err());
        }
    }

    #[test]
    fn the_work_grant_excludes_runtime_home_special_files_and_unmounted_symlink_targets() {
        let temp = fixture();
        let home = temp.0.join("home");
        let work = home.join("project");
        let runtime = temp.0.join("runtime");
        fs::create_dir_all(&work).unwrap();
        fs::create_dir_all(&runtime).unwrap();
        fs::create_dir_all(home.join(".ssh")).unwrap();
        assert!(work_dir(&home, &runtime, Some(&home)).is_err());
        assert!(work_dir(&runtime, &runtime, Some(&home)).is_err());
        assert!(work_dir(&home.join(".ssh"), &runtime, Some(&home)).is_err());
        assert!(work_dir(Path::new("/"), &runtime, None).is_err());
        symlink("/not-mounted/secret", work.join("escape")).unwrap();
        assert!(
            work_dir(&work, &runtime, Some(&home)).is_ok(),
            "a link grants no unmounted target"
        );
        let _socket = UnixListener::bind(work.join("host.sock")).unwrap();
        assert!(work_dir(&work, &runtime, Some(&home))
            .unwrap_err()
            .to_string()
            .contains("socket"));
    }

    #[test]
    fn runtime_symlinks_and_broad_permissions_cannot_be_adopted() {
        use std::os::unix::fs::PermissionsExt;
        let temp = fixture();
        let bad = temp.0.join("bad");
        fs::create_dir(&bad).unwrap();
        fs::set_permissions(&bad, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(private_dir(&bad).is_err());
        let link = temp.0.join("link");
        symlink(&temp.0, &link).unwrap();
        assert!(private_dir(&link).is_err());
        assert!(operator_path("../../operator.sock").is_err());
        assert!(operator_path(&"A".repeat(32)).is_err());
    }

    #[test]
    fn a_text_only_gateway_cannot_select_a_target_grant_or_delegate() {
        let temp = fixture();
        let count = Arc::new(AtomicUsize::new(0));
        let copy = count.clone();
        let broker = Broker::start(
            &temp,
            grants(true),
            Arc::new(move |_| {
                copy.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        )
        .unwrap();
        let socket = &broker.gateway;
        for frame in [
            r#"{"op":"note","text":"hello","pid":999}"#,
            r#"{"op":"grant","note":true}"#,
            r#"{"op":"mcp","method":"mcp off"}"#,
            r#"{"op":"send","recipient":"stronger","text":"execute"}"#,
            r#"{"op":"note","text":"hello","pin":true}"#,
            r#"{"op":"revoke"}"#,
        ] {
            assert_eq!(
                packet_exchange(socket, frame).unwrap()["ok"],
                false,
                "{frame}"
            );
        }
        assert_eq!(count.load(Ordering::SeqCst), 0);
        assert_eq!(
            packet_exchange(socket, r#"{"op":"note","text":"Build complete"}"#).unwrap()["ok"],
            true
        );
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(
            packet_exchange(socket, r#"{"op":"note","text":"Flood"}"#).unwrap()["ok"],
            false
        );
        assert_eq!(
            exchange(&temp.0.join("operator.sock"), r#"{"op":"revoke"}"#).unwrap()["note"],
            "revoked"
        );
        let denied = packet_exchange(socket, r#"{"op":"note","text":"After revoke"}"#).unwrap();
        assert_eq!(denied["ok"], false);
        assert!(
            denied["error"].as_str().unwrap().contains("revoked"),
            "must hit revocation, not the rate limit: {denied}"
        );
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn access_requests_are_bounded_proposals_and_never_change_grants() {
        let mut grants = grants(false);
        let never = |_: &str| -> Result<(), String> { panic!("no note grant") };
        assert_eq!(
            grants.apply(
                Request::Note {
                    text: "hello".into()
                },
                false,
                &never
            )["ok"],
            false
        );
        for _ in 0..2 {
            let v = grants.apply(
                Request::AskAccess {
                    reason: "Need external packages".into(),
                },
                false,
                &never,
            );
            assert_eq!(v["granted"], false);
            assert_eq!(v["pending"], true);
        }
        assert_eq!(
            grants.apply(
                Request::AskAccess {
                    reason: "Different request".into()
                },
                false,
                &never
            )["ok"],
            false
        );
        assert_eq!(grants.status()["note"], "not_granted");
        assert_eq!(
            grants.status()["pending_access_request"],
            "Need external packages"
        );
        grants.apply(Request::Revoke, true, &never);
        assert_eq!(
            grants.apply(
                Request::AskAccess {
                    reason: "Try again".into()
                },
                false,
                &never
            )["ok"],
            false
        );
        assert!(grants.status()["pending_access_request"].is_null());
        assert!(
            super::tests::grants(false).status()["pending_access_request"].is_null(),
            "new run does not inherit an ask"
        );
    }

    #[test]
    fn oversized_slow_and_invalid_peers_do_not_poison_the_next_request() {
        let temp = fixture();
        let broker = Broker::start(&temp, grants(false), Arc::new(|_| Ok(()))).unwrap();
        let socket = &broker.gateway;
        let oversized = json!({"op":"note","text":"x".repeat(MAX_REQUEST)}).to_string();
        let result = packet_exchange(socket, &oversized).unwrap();
        assert!(result["error"].as_str().unwrap().contains("exceeds 2048"));
        assert_eq!(packet_exchange(socket, "{").unwrap()["ok"], false);
        let _slow = UnixStream::connect(temp.0.join("operator.sock")).unwrap();
        assert_eq!(
            packet_exchange(socket, r#"{"op":"status"}"#).unwrap()["ok"],
            true
        );
        drop(_slow);
        assert_eq!(
            exchange(&temp.0.join("operator.sock"), r#"{"op":"status"}"#).unwrap()["ok"],
            true,
            "a slow workload cannot hold the independent operator channel"
        );
        assert_eq!(
            packet_exchange(socket, r#"{"op":"status"}"#).unwrap()["profile"],
            "offline-v1"
        );
    }

    #[test]
    fn revoke_acknowledges_after_an_accepted_note_and_denies_subsequent_effects() {
        let temp = fixture();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let release = Mutex::new(release_rx);
        let broker = Broker::start(
            &temp,
            grants(true),
            Arc::new(move |_| {
                entered_tx.send(()).unwrap();
                release.lock().unwrap().recv().unwrap();
                Ok(())
            }),
        )
        .unwrap();
        let socket = &broker.gateway;
        let note_socket = socket.try_clone().unwrap();
        let note = thread::spawn(move || {
            packet_exchange(&note_socket, r#"{"op":"note","text":"In flight"}"#).unwrap()
        });
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let operator = temp.0.join("operator.sock");
        let (revoked_tx, revoked_rx) = std::sync::mpsc::channel();
        let revoke = thread::spawn(move || {
            revoked_tx
                .send(exchange(&operator, r#"{"op":"revoke"}"#).unwrap())
                .unwrap()
        });
        assert!(
            revoked_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "revoke cannot outrun the accepted effect"
        );
        release_tx.send(()).unwrap();
        assert_eq!(note.join().unwrap()["ok"], true);
        assert_eq!(
            revoked_rx.recv_timeout(Duration::from_secs(2)).unwrap()["note"],
            "revoked"
        );
        revoke.join().unwrap();
        let late = packet_exchange(socket, r#"{"op":"note","text":"Late"}"#).unwrap();
        assert_eq!(late["ok"], false);
        assert!(late["error"].as_str().unwrap().contains("revoked"));
    }

    #[test]
    fn output_cannot_invoke_terminal_protocol_even_when_utf8_is_split() {
        struct Bytes(Vec<u8>);
        impl Read for Bytes {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                if self.0.is_empty() {
                    return Ok(0);
                }
                buf[0] = self.0.remove(0);
                Ok(1)
            }
        }
        let input = "hello é\u{1b}]52;c;SECRET\u{7}\u{9b}31m\n\t世界\r";
        let mut out = Vec::new();
        text_stream(Bytes(input.as_bytes().to_vec()), &mut out).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "hello é]52;c;SECRET31m\n\t世界\r"
        );
        let mut out = Vec::new();
        text_stream(&b"\xff\x1b[31m\xc3"[..], &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "�[31m�");
        assert!(!text_ok("a\u{1b}b", 10));
    }

    #[test]
    fn output_cannot_reorder_what_a_reader_sees() {
        // "rm -rf ~ #" displayed backwards behind a right-to-left override is
        // the Trojan Source shape; the joiner inside the emoji is ordinary text.
        let input = "safe \u{202e}~ fr- mr\u{202c} \u{2066}x\u{2069}\u{200f}\u{2028}👩\u{200d}💻\n";
        let mut out = Vec::new();
        text_stream(input.as_bytes(), &mut out).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "safe ~ fr- mr x👩\u{200d}💻\n"
        );
    }
}
