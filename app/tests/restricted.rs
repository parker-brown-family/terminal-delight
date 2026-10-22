//! Real OS enforcement checks. Run with --ignored on a Linux host with bwrap;
//! CI installs it and explicitly runs this target. Never touches a live pane.
use std::fs::{self, File};
use std::io::Read;
use std::net::TcpListener;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_terminal-delight");
struct Fixture(PathBuf, PathBuf);
impl Fixture {
    fn new() -> Self {
        let mut random = [0; 8];
        File::open("/dev/urandom")
            .unwrap()
            .read_exact(&mut random)
            .unwrap();
        let home = Command::new("python3")
            .args([
                "-c",
                "import pwd,os; print(pwd.getpwuid(os.getuid()).pw_dir)",
            ])
            .output()
            .unwrap();
        assert!(home.status.success());
        let home = PathBuf::from(String::from_utf8(home.stdout).unwrap().trim());
        let path = home.join(format!("tr-{:016x}", u64::from_ne_bytes(random)));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        for name in ["work"] {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(path.join(name))
                .unwrap();
        }
        let runtime = PathBuf::from(format!(
            "/run/user/{}/t{:08x}",
            unsafe { libc::getuid() },
            u32::from_ne_bytes(random[..4].try_into().unwrap())
        ));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&runtime)
            .expect("prepare owned runtime; CI creates /run/user/UID first");
        Self(path, runtime)
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(BIN);
        cmd.env("XDG_RUNTIME_DIR", &self.1)
            .args(["restricted", "run", "--workdir"])
            .arg(self.0.join("work"));
        cmd
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
        let _ = fs::remove_dir_all(&self.1);
    }
}

fn wait_file(path: &std::path::Path) {
    let until = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(
            Instant::now() < until,
            "never reached boundary: {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "real Linux mount identity; CI runs explicitly with bubblewrap"]
fn a_nested_same_device_bind_mount_is_refused_before_the_command_starts() {
    let f = Fixture::new();
    let mount = f.0.join("work/nested");
    fs::create_dir(&mount).unwrap();
    let outside = f.0.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("host-file"), "not granted").unwrap();
    let output = Command::new("/usr/bin/bwrap")
        .args(["--unshare-user", "--bind", "/", "/", "--bind"])
        .arg(&outside)
        .arg(&mount)
        .arg("--")
        .arg(BIN)
        .env("XDG_RUNTIME_DIR", &f.1)
        .args(["restricted", "run", "--workdir"])
        .arg(f.0.join("work"))
        .args(["--", "sh", "-c", "touch /work/SHOULD-NOT-RUN"])
        .output()
        .unwrap();
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        error.contains("nested host mount"),
        "did not reach the intended boundary: {error}"
    );
    assert!(!f.0.join("work/SHOULD-NOT-RUN").exists());
}

#[test]
#[ignore = "real Linux gateway and operator channels; CI runs explicitly"]
fn a_pending_proposal_is_visible_and_only_the_operator_can_dismiss_it() {
    let f = Fixture::new();
    let child = f.command().args(["--", "python3", "-c", r#"
import json, os, pathlib, subprocess, time
td='terminal-delight'
def rpc(value):
    return json.loads(subprocess.run([td,'restricted','call',json.dumps(value)],capture_output=True,text=True,pass_fds=(int(os.environ['TD_GATEWAY_FD']),)).stdout)
assert rpc({'op':'request_access','reason':'Need external packages'})['granted'] is False
assert rpc({'op':'dismiss'})['ok'] is False
state=rpc({'op':'status'})
pathlib.Path('/work/ready.json').write_text(json.dumps(state))
while not pathlib.Path('/work/go').exists(): time.sleep(.02)
state=rpc({'op':'status'})
assert state['pending_access_request'] is None and state['note']=='not_granted'
assert rpc({'op':'request_access','reason':'Try another proposal'})['ok'] is False
assert rpc({'op':'note','text':'No grant appeared'})['ok'] is False
"#]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let ready = f.0.join("work/ready.json");
    wait_file(&ready);
    let state: serde_json::Value = serde_json::from_slice(&fs::read(&ready).unwrap()).unwrap();
    assert_eq!(state["pending_access_request"], "Need external packages");
    let id = state["run"].as_str().unwrap();
    let dismissed = Command::new(BIN)
        .env("XDG_RUNTIME_DIR", &f.1)
        .args(["restricted", "dismiss", id])
        .output()
        .unwrap();
    assert!(
        dismissed.status.success(),
        "{}",
        String::from_utf8_lossy(&dismissed.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&dismissed.stdout).unwrap();
    assert!(result["pending_access_request"].is_null());
    assert_eq!(result["access_request_available"], false);
    fs::write(f.0.join("work/go"), "").unwrap();
    let output = child.wait_with_output().unwrap();
    let visible = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{visible}");
    assert_eq!(visible.matches("RESTRICTED ACCESS PROPOSAL").count(), 1);
    assert!(visible.contains("Need external packages") && visible.contains("no access granted"));
    assert!(visible.contains(&format!("restricted dismiss {id}")));
}

#[test]
#[ignore = "real Linux namespace enforcement; CI runs explicitly after installing bubblewrap"]
fn offline_work_completes_without_host_files_services_processes_credentials_or_control() {
    use std::os::linux::net::SocketAddrExt;
    let f = Fixture::new();
    let secret = f.0.join("host-secret");
    fs::write(&secret, "CANARY-NOT-IN-THE-WORK-GRANT").unwrap();
    fs::write(
        f.0.join("work/source.c"),
        "#include <stdio.h>\nint main(void){puts(\"allowed build\");return 0;}\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(&secret, f.0.join("work/escape")).unwrap();
    let unix_path = f.0.join("outside.sock");
    let unix = UnixListener::bind(&unix_path).unwrap();
    unix.set_nonblocking(true).unwrap();
    let abstract_name = format!("td-boundary-{}", f.0.file_name().unwrap().to_string_lossy());
    let abstract_addr =
        std::os::unix::net::SocketAddr::from_abstract_name(abstract_name.as_bytes()).unwrap();
    let abstract_unix = UnixListener::bind_addr(&abstract_addr).unwrap();
    abstract_unix.set_nonblocking(true).unwrap();
    drop(std::os::unix::net::UnixStream::connect_addr(&abstract_addr).unwrap());
    drop(abstract_unix.accept().unwrap());
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    tcp.set_nonblocking(true).unwrap();
    let port = tcp.local_addr().unwrap().port();
    // Positive controls prove these targets actually exist and accept callers.
    drop(std::os::unix::net::UnixStream::connect(&unix_path).unwrap());
    drop(unix.accept().unwrap());
    drop(std::net::TcpStream::connect(("127.0.0.1", port)).unwrap());
    drop(tcp.accept().unwrap());
    let inherited = File::open(&secret).unwrap();
    unsafe {
        assert_eq!(libc::fcntl(inherited.as_raw_fd(), libc::F_SETFD, 0), 0);
    }
    let host_pid = std::process::id();
    let net = fs::read_link("/proc/self/ns/net").unwrap();
    let output = f
        .command()
        .env("TD_SECRET_TEST", "HOST-ENV-CANARY")
        .env("SSH_AUTH_SOCK", &unix_path)
        .env("DBUS_SESSION_BUS_ADDRESS", "host-bus-canary")
        .args(["--", "python3", "-c", PROBE])
        .arg(&secret)
        .arg(&unix_path)
        .arg(port.to_string())
        .arg(host_pid.to_string())
        .arg(net)
        .arg(abstract_name)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.stdout.contains(&0x1b) && !output.stderr.contains(&0x1b),
        "terminal protocol escaped"
    );
    assert_eq!(
        fs::read_to_string(&secret).unwrap(),
        "CANARY-NOT-IN-THE-WORK-GRANT"
    );
    assert_eq!(
        fs::read_to_string(f.0.join("work/result")).unwrap(),
        "allowed build\n"
    );
    let proof: serde_json::Value =
        serde_json::from_slice(&fs::read(f.0.join("work/proof.json")).unwrap()).unwrap();
    assert_eq!(proof["profile"], "offline-v1");
    assert_eq!(proof["network"], "none");
    assert_eq!(proof["note"], "not_granted");
    assert!(
        unix.accept().is_err() && tcp.accept().is_err() && abstract_unix.accept().is_err(),
        "restricted child reached a protected service"
    );
    let runs = f.1.join("terminal-delight/restricted");
    assert_eq!(
        fs::read_dir(runs).unwrap().count(),
        0,
        "completed grants must be removed"
    );
}

const PROBE: &str = r#"
import os,sys,socket,json,subprocess,pathlib
secret,host_socket,port,host_pid,host_net,abstract_name=sys.argv[1:]
assert not os.isatty(0) and not os.isatty(1) and not os.isatty(2)
assert os.getcwd()=='/work' and os.environ['HOME']=='/home/td'
assert set(os.listdir('/')) <= {'usr','bin','lib','lib64','etc','proc','dev','tmp','home','run','work'}
assert not any(x in os.environ for x in ['TD_SECRET_TEST','SSH_AUTH_SOCK','DBUS_SESSION_BUS_ADDRESS','TD_SESSION','TD_PANE_ID','DISPLAY','WAYLAND_DISPLAY'])
assert os.readlink('/proc/self/ns/net')!=host_net
assert 'NoNewPrivs:\t1' in pathlib.Path('/proc/self/status').read_text()
assert 'CapEff:\t0000000000000000' in pathlib.Path('/proc/self/status').read_text()
for path in [secret,'/work/escape',f'/proc/{host_pid}/environ',f'/proc/1/root{secret}',f'/proc/self/root{secret}']:
    try: open(path).read()
    except OSError: pass
    else: raise AssertionError('host read '+path)
try: open(secret,'w').write('POISON')
except OSError: pass
else: raise AssertionError('host write')
try: os.kill(int(host_pid),0)
except OSError: pass
else: raise AssertionError('host process reachable')
for fd in pathlib.Path('/proc/self/fd').iterdir():
    try: target=os.readlink(fd)
    except OSError: continue
    assert 'host-secret' not in target and 'outside.sock' not in target,target
    try: open(str(fd)+'/../host-secret').read()
    except OSError: pass
    else: raise AssertionError('work mount FD leaked outside its bind mount')
try: reaper_fds=list(pathlib.Path('/proc/1/fd').iterdir())
except OSError: reaper_fds=[]
for fd in reaper_fds:
    try: open(str(fd)+secret).read()
    except OSError: pass
    else: raise AssertionError('reaper retained a host-root descriptor')
for family,address in [(socket.AF_UNIX,host_socket),(socket.AF_UNIX,'\0'+abstract_name),(socket.AF_INET,('127.0.0.1',int(port)))]:
    try: s=socket.socket(family)
    except OSError as e:
        assert e.errno==1,e
        continue
    s.settimeout(.5)
    try: s.connect(address)
    except OSError: pass
    else: raise AssertionError('host service reachable')
    finally: s.close()
try: os.open('/dev/tty',os.O_RDWR)
except OSError: pass
else: raise AssertionError('unfiltered terminal descriptor')
try: os.unshare(os.CLONE_NEWUSER)
except OSError: pass
else: raise AssertionError('nested user namespace created')
cgroup=pathlib.Path('/proc/self/cgroup').read_text().strip()
assert cgroup=='0::/',cgroup
import ctypes,platform
libc=ctypes.CDLL(None,use_errno=True)
# keyctl(GET_KEYRING_ID, session), perf_event_open(NULL), userfaultfd(USER_MODE_ONLY), bpf(bad cmd)
nrs={'x86_64':(250,298,323,321),'aarch64':(219,241,282,280)}[platform.machine()]
for nr,args in zip(nrs,[(0,-3,0),(0,0,-1,-1,0),(1,),(9999,0,0)]):
    ctypes.set_errno(0)
    assert libc.syscall(nr,*args)==-1 and ctypes.get_errno()==1,(nr,ctypes.get_errno())
def rpc(data):
    s=socket.socket(fileno=os.dup(int(os.environ['TD_GATEWAY_FD'])))
    s.send(json.dumps(data).encode());result=json.loads(s.recv(8192));s.close();return result
status=rpc({'op':'status'});assert status['ok'] and status['network']=='none'
for data in [{'op':'note','text':'Ungrantable'}, {'op':'note','text':'Spoof','pid':1},
             {'op':'grant','note':True},{'op':'mcp','command':'mcp off'},
             {'op':'send','recipient':'standard','text':'execute'}, {'op':'revoke'}]:
    assert rpc(data)['ok'] is False,data
request=rpc({'op':'request_access','reason':'Need external packages'})
assert request['pending'] and not request['granted']
assert not rpc({'op':'request_access','reason':'Approval flood'})['ok']
assert rpc({'op':'status'})['note']=='not_granted'
cli=json.loads(subprocess.check_output(['terminal-delight','restricted','call','{"op":"status"}'], pass_fds=(int(os.environ['TD_GATEWAY_FD']),)))
assert cli['profile']=='offline-v1'
subprocess.run(['cc','source.c','-o','built'],check=True)
pathlib.Path('result').write_bytes(subprocess.check_output(['./built']))
pathlib.Path('/home/td/temp').write_text('allowed ephemeral home')
pathlib.Path('/tmp/temp').write_text('allowed ephemeral tmp')
pathlib.Path('proof.json').write_text(json.dumps(status))
os.write(1,'positive stdout é 世界\n\x1b]52;c;CANARY\x07'.encode())
os.write(2,b'positive stderr\n\x1b[31m\x9b31m')
"#;

#[test]
#[ignore = "real Linux seccomp enforcement; CI runs explicitly"]
fn a_later_host_socket_in_the_work_tree_cannot_bypass_the_gateway() {
    use std::os::unix::net::UnixDatagram;
    let f = Fixture::new();
    let script = r#"
import os,socket,pathlib,time,errno,json,ctypes,sys
pathlib.Path('ready').write_text('boundary reached')
while not pathlib.Path('go').exists(): time.sleep(.01)
assert pathlib.Path('/work/late.sock').exists(), 'must reach a real visible host socket'
s,peer=socket.socketpair(socket.AF_UNIX,socket.SOCK_DGRAM)
results=[]
for action in [lambda:s.connect('/work/late.sock'),lambda:s.sendto(b'poison','/work/late.sock'),lambda:s.sendmsg([b'poison'],[],0,'/work/late.sock')]:
    try: action()
    except OSError as e: assert e.errno==errno.EPERM,e;results.append(e.errno)
    else: raise AssertionError('late host service reachable')
libc=ctypes.CDLL(None,use_errno=True)
assert libc.syscall(int(sys.argv[1]),0,0)==-1 and ctypes.get_errno()==errno.EPERM
g=socket.socket(fileno=os.dup(int(os.environ['TD_GATEWAY_FD'])))
g.send(b'{"op":"status"}');assert json.loads(g.recv(8192))['ok']
pathlib.Path('late-proof.json').write_text(json.dumps(results))
"#;
    let child = f
        .command()
        .args(["--", "python3", "-c", script])
        .arg(libc::SYS_io_uring_setup.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_file(&f.0.join("work/ready"));
    let server = UnixDatagram::bind(f.0.join("work/late.sock")).unwrap();
    server.set_nonblocking(true).unwrap();
    UnixDatagram::unbound()
        .unwrap()
        .send_to(b"positive", f.0.join("work/late.sock"))
        .unwrap();
    let mut buf = [0; 64];
    assert_eq!(server.recv(&mut buf).unwrap(), 8);
    fs::write(f.0.join("work/go"), "visible service ready").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        server.recv(&mut buf).is_err(),
        "poison reached the host socket"
    );
    assert_eq!(
        fs::read_to_string(f.0.join("work/late-proof.json")).unwrap(),
        "[1, 1, 1]"
    );
}

#[test]
#[ignore = "real Linux namespace enforcement; CI runs explicitly after installing bubblewrap"]
fn read_only_fail_closed_and_supervisor_death_do_not_reuse_authority() {
    let f = Fixture::new();
    if let Some(home) = std::env::var_os("HOME") {
        let out = Command::new(BIN)
            .env("XDG_RUNTIME_DIR", f.1.clone())
            .env("HOME", &f.0)
            .args(["restricted", "run", "--workdir"])
            .arg(home)
            .args(["--", "true"])
            .output()
            .unwrap();
        assert!(
            !out.status.success() && String::from_utf8_lossy(&out.stderr).contains("home"),
            "HOME must not determine the protected account home"
        );
    }
    let outside = f.0.join("work/unsafe-runtime");
    let out = f
        .command()
        .env("XDG_RUNTIME_DIR", &outside)
        .args(["--", "true"])
        .output()
        .unwrap();
    assert!(
        !out.status.success()
            && String::from_utf8_lossy(&out.stderr).contains("protected /run/user/UID")
    );
    assert!(
        !outside.join("terminal-delight/restricted").exists(),
        "no authority files may be issued in another workload's work tree"
    );
    fs::write(f.0.join("outside-file"), "protected alias").unwrap();
    fs::hard_link(f.0.join("outside-file"), f.0.join("work/alias")).unwrap();
    let out = f.command().args(["--", "true"]).output().unwrap();
    assert!(!out.status.success() && String::from_utf8_lossy(&out.stderr).contains("hard-linked"));
    fs::remove_file(f.0.join("work/alias")).unwrap();
    fs::create_dir(f.0.join("work/config")).unwrap();
    let out = f
        .command()
        .env("XDG_CONFIG_HOME", f.0.join("work/config"))
        .args(["--", "true"])
        .output()
        .unwrap();
    assert!(
        !out.status.success() && String::from_utf8_lossy(&out.stderr).contains("XDG_CONFIG_HOME")
    );
    // Prepare a real positive read rather than accepting a command-routing failure.
    fs::write(f.0.join("work/source"), "readable").unwrap();
    let out = f
        .command()
        .args([
            "--read-only",
            "--",
            "sh",
            "-ec",
            "test -r /work/source; ! touch /work/forbidden",
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!f.0.join("work/forbidden").exists());
    let out = f
        .command()
        .args(["--network", "--", "touch", "/work/should-not-run"])
        .output()
        .unwrap();
    assert!(!out.status.success() && !f.0.join("work/should-not-run").exists());
    let out = f
        .command()
        .args(["--td-note", "--", "touch", "/work/should-not-run"])
        .env("TD_SESSION", "forged")
        .env("TD_PANE_ID", "1")
        .output()
        .unwrap();
    assert!(
        !out.status.success() && !f.0.join("work/should-not-run").exists(),
        "identity cannot fall back to environment"
    );
    let socket = UnixListener::bind(f.0.join("work/host.sock")).unwrap();
    let out = f
        .command()
        .args(["--", "touch", "/work/should-not-run"])
        .output()
        .unwrap();
    assert!(!out.status.success() && !f.0.join("work/should-not-run").exists());
    drop(socket);
    fs::remove_file(f.0.join("work/host.sock")).unwrap();
    let mut child = f.command().args(["--", "python3", "-c", "import os,time,pathlib\nif os.fork()==0:\n os.setsid()\n if os.fork()>0: os._exit(0)\n pathlib.Path('ready').write_text('grandchild running')\n while True: pathlib.Path('heartbeat').write_text(str(time.monotonic())); time.sleep(.02)\nwhile True: time.sleep(1)"])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    wait_file(&f.0.join("work/ready"));
    let runs = f.1.join("terminal-delight/restricted");
    let id = fs::read_dir(&runs)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name();
    let status = Command::new(BIN)
        .env("XDG_RUNTIME_DIR", f.1.clone())
        .args(["restricted", "status"])
        .arg(&id)
        .output()
        .unwrap();
    assert!(status.status.success());
    let revoke = Command::new(BIN)
        .env("XDG_RUNTIME_DIR", f.1.clone())
        .args(["restricted", "revoke"])
        .arg(&id)
        .output()
        .unwrap();
    assert!(revoke.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&revoke.stdout).unwrap()["note"],
        "revoked"
    );
    child.kill().unwrap();
    child.wait().unwrap();
    std::thread::sleep(Duration::from_millis(150));
    let heartbeat = fs::read(f.0.join("work/heartbeat")).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(
        fs::read(f.0.join("work/heartbeat")).unwrap(),
        heartbeat,
        "descendant survived supervisor death"
    );
    let stale = Command::new(BIN)
        .env("XDG_RUNTIME_DIR", f.1.clone())
        .args(["restricted", "status"])
        .arg(&id)
        .output()
        .unwrap();
    assert!(
        !stale.status.success(),
        "stale socket must not restore authority"
    );
    let fresh = f
        .command()
        .args([
            "--",
            "terminal-delight",
            "restricted",
            "call",
            "{\"op\":\"status\"}",
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        fresh.status.success(),
        "{}",
        String::from_utf8_lossy(&fresh.stderr)
    );
    let fresh: serde_json::Value = serde_json::from_slice(&fresh.stdout).unwrap();
    assert_eq!(fresh["note"], "not_granted");
    assert!(fresh["pending_access_request"].is_null());
    fs::set_permissions(f.1.clone(), fs::Permissions::from_mode(0o755)).unwrap();
    let out = f
        .command()
        .args(["--", "touch", "/work/should-not-run"])
        .output()
        .unwrap();
    assert!(!out.status.success() && !f.0.join("work/should-not-run").exists());
}
