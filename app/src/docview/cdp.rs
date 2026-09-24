//! TD's own DevTools client, over the pipe Chromium opens for
//! `--remote-debugging-pipe`.
//!
//! # Why TD owns this instead of a crate
//!
//! The two maintained Rust clients both drive a browser through a debugging
//! PORT, which listens on 127.0.0.1 where any local process can connect and
//! drive a browser that reads `file://`. One needs tokio and 32 packages, the
//! other 35 with a TLS stack for a local connection. The pipe needs neither: a
//! child launched with `--remote-debugging-pipe` reads commands on fd 3 and
//! writes replies and events on fd 4, one JSON message per NUL byte, and only
//! TD holds the other ends. The price is that the protocol names TD uses are
//! TD's to keep working — the snapshot engine uses nineteen: the program
//! design's sixteen, plus a browser context per page (`createBrowserContext`,
//! `disposeBrowserContext`) and `Browser.getVersion` to tell a browser that
//! started from one that did not.
//!
//! # Shape
//!
//! One reader thread per browser ("td-cdp-read") splits the byte stream into
//! messages and hands each to whoever is waiting: a reply goes to the call
//! that sent its `id`, an event to the first [`Expect`] registered for its
//! method and session. Calls block the caller's thread, with a timeout, which
//! is the house pattern for work off the UI thread (`ctl.rs`): the snapshot
//! engine runs on gpui's background pool, never on the foreground.
//!
//! The reader answers one event itself: a page that opens `alert`, `confirm`
//! or `prompt` blocks its own main thread until someone answers, and nothing
//! in TD ever will, so the dialog is dismissed the moment it opens.
//!
//! No `crate::` paths, so `app/tests/snapshot_engine.rs` can compile this file
//! into a test that drives a real Chromium.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

use serde_json::{json, Value};

/// A flattened target session, as `Target.attachToTarget { flatten: true }`
/// returns it. Commands carrying it go to that page; events carrying it came
/// from that page.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SessionId(pub String);

#[derive(Debug, Clone, PartialEq)]
pub enum CdpError {
    /// The browser went away, or was never there: the pipe reached its end.
    Closed,
    /// No answer within the time the caller allowed.
    Timeout,
    /// Chromium answered with an error object.
    Remote {
        code: i64,
        message: String,
    },
    Io(String),
    Parse(String),
}

impl std::fmt::Display for CdpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CdpError::Closed => write!(f, "the browser closed its end of the pipe"),
            CdpError::Timeout => write!(f, "the browser did not answer in time"),
            CdpError::Remote { code, message } => {
                write!(f, "the browser refused ({code}): {message}")
            }
            CdpError::Io(e) => write!(f, "the pipe to the browser failed: {e}"),
            CdpError::Parse(e) => write!(f, "the browser sent something that is not JSON: {e}"),
        }
    }
}

/// Someone waiting for one event.
struct Waiter {
    id: u64,
    session: Option<SessionId>,
    method: &'static str,
    tx: mpsc::Sender<Value>,
}

pub struct Cdp {
    out: Mutex<Box<dyn Write + Send>>,
    waiting: Mutex<HashMap<u64, mpsc::Sender<Result<Value, CdpError>>>>,
    expects: Mutex<Vec<Waiter>>,
    next: AtomicU64,
    /// Set by the reader when the pipe ends, under the `waiting` lock, so a
    /// call either sees it or is drained by it — never neither.
    closed: AtomicBool,
}

/// An event registered for before the call that causes it.
pub struct Expect {
    rx: mpsc::Receiver<Value>,
    id: u64,
    cdp: Weak<Cdp>,
}

impl Expect {
    /// The event's `params`, once it arrives.
    pub fn wait(self, within: Duration) -> Result<Value, CdpError> {
        match self.rx.recv_timeout(within) {
            Ok(v) => Ok(v),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(CdpError::Timeout),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(CdpError::Closed),
        }
    }
}

impl Drop for Expect {
    /// An expectation nobody waits for any more must not match the next event
    /// of its kind and swallow it.
    fn drop(&mut self) {
        if let Some(cdp) = self.cdp.upgrade() {
            lock(&cdp.expects).retain(|w| w.id != self.id);
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Split a byte stream into messages, keeping an unfinished tail in `pending`.
///
/// Pure. A read can end anywhere — inside a message, between two, or with
/// several in one buffer — and a 4 MB screenshot arrives in dozens of reads,
/// so only the new bytes are searched for the terminator.
pub fn frames(pending: &mut Vec<u8>, incoming: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut rest = incoming;
    while let Some(at) = rest.iter().position(|&b| b == 0) {
        pending.extend_from_slice(&rest[..at]);
        out.push(std::mem::take(pending));
        rest = &rest[at + 1..];
    }
    pending.extend_from_slice(rest);
    out
}

impl Cdp {
    /// Launch `binary` with the DevTools pipe on fds 3 and 4.
    ///
    /// Both pipes are made close-on-exec, and the child's ends are moved onto
    /// 3 and 4 after the fork, which leaves only those two open across the
    /// exec. The child also asks the kernel for SIGKILL when the thread that
    /// forked it exits, so a TD that crashes takes its browser with it — which
    /// is why the snapshot engine launches from a thread that lives as long as
    /// TD does, and never from a pool thread that may be retired.
    ///
    /// Standard error is piped for the caller to drain: a launch that fails
    /// says why there, and an undrained pipe would stall the browser.
    pub fn spawn(
        binary: &std::path::Path,
        args: &[std::ffi::OsString],
    ) -> io::Result<(std::process::Child, Arc<Cdp>)> {
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        use std::os::unix::process::CommandExt;

        fn pipe() -> io::Result<(OwnedFd, OwnedFd)> {
            let mut fds = [0 as libc::c_int; 2];
            // SAFETY: `fds` is two writable ints, which is what pipe2 fills.
            if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: pipe2 succeeded, so both are fresh descriptors we own.
            Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
        }

        let (child_reads, we_write) = pipe()?;
        let (we_read, child_writes) = pipe()?;
        let (r, w) = (child_reads.as_raw_fd(), child_writes.as_raw_fd());
        let parent = std::process::id() as libc::pid_t;
        let mut cmd = std::process::Command::new(binary);
        cmd.args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped());
        // SAFETY: only async-signal-safe calls (fcntl, dup2, prctl, getppid)
        // run between fork and exec.
        unsafe {
            cmd.pre_exec(move || {
                // Out of the 3..=4 range first, so placing one end cannot
                // overwrite the other if the kernel happened to number it 3 or 4.
                let r2 = libc::fcntl(r, libc::F_DUPFD_CLOEXEC, 10);
                let w2 = libc::fcntl(w, libc::F_DUPFD_CLOEXEC, 10);
                if r2 < 0 || w2 < 0 {
                    return Err(io::Error::last_os_error());
                }
                // dup2 clears close-on-exec on the new descriptor, so these
                // two, and only these two, survive the exec.
                if libc::dup2(r2, 3) < 0 || libc::dup2(w2, 4) < 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0) != 0 {
                    return Err(io::Error::last_os_error());
                }
                // The parent may have died between the fork and the prctl,
                // in which case the signal has already been missed.
                if libc::getppid() != parent {
                    return Err(io::Error::from_raw_os_error(libc::ESRCH));
                }
                Ok(())
            });
        }
        let child = cmd.spawn()?;
        drop(child_reads);
        drop(child_writes);
        let cdp = Cdp::over(
            Box::new(std::fs::File::from(we_read)),
            Box::new(std::fs::File::from(we_write)),
        );
        Ok((child, cdp))
    }

    /// The same client over any stream pair: the tests drive it through a
    /// `UnixStream::pair` standing in for a browser.
    pub fn over(read: Box<dyn Read + Send>, write: Box<dyn Write + Send>) -> Arc<Cdp> {
        let cdp = Arc::new(Cdp {
            out: Mutex::new(write),
            waiting: Mutex::new(HashMap::new()),
            expects: Mutex::new(Vec::new()),
            next: AtomicU64::new(1),
            closed: AtomicBool::new(false),
        });
        let reader = cdp.clone();
        let spawned = std::thread::Builder::new()
            .name("td-cdp-read".into())
            .spawn(move || reader.read_loop(read));
        if spawned.is_err() {
            // No reader means no replies: say so to every caller at once
            // rather than letting each one wait out its timeout.
            cdp.close_all();
        }
        cdp
    }

    /// Whether the pipe has ended.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// Send `method` and wait for its reply.
    pub fn call(
        &self,
        session: Option<&SessionId>,
        method: &str,
        params: Value,
        within: Duration,
    ) -> Result<Value, CdpError> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        {
            let mut waiting = lock(&self.waiting);
            if self.is_closed() {
                return Err(CdpError::Closed);
            }
            waiting.insert(id, tx);
        }
        let mut msg = json!({ "id": id, "method": method, "params": params });
        if let Some(s) = session {
            msg["sessionId"] = json!(s.0);
        }
        if let Err(e) = self.send(&msg) {
            lock(&self.waiting).remove(&id);
            return Err(e);
        }
        match rx.recv_timeout(within) {
            Ok(answer) => answer,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                lock(&self.waiting).remove(&id);
                Err(CdpError::Timeout)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(CdpError::Closed),
        }
    }

    /// Register for an event before sending the command that causes it: the
    /// event can arrive before the command's own reply, and one that arrives
    /// before anybody is listening is gone.
    pub fn expect(self: &Arc<Self>, session: Option<&SessionId>, method: &'static str) -> Expect {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        lock(&self.expects).push(Waiter {
            id,
            session: session.cloned(),
            method,
            tx,
        });
        Expect {
            rx,
            id,
            cdp: Arc::downgrade(self),
        }
    }

    fn send(&self, msg: &Value) -> Result<(), CdpError> {
        let mut bytes = serde_json::to_vec(msg).map_err(|e| CdpError::Parse(e.to_string()))?;
        bytes.push(0);
        let mut out = lock(&self.out);
        out.write_all(&bytes)
            .and_then(|()| out.flush())
            .map_err(|e| CdpError::Io(e.to_string()))
    }

    fn read_loop(self: Arc<Self>, mut read: Box<dyn Read + Send>) {
        let mut pending = Vec::new();
        let mut buf = vec![0u8; 256 << 10];
        loop {
            match read.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for msg in frames(&mut pending, &buf[..n]) {
                        self.dispatch(&msg);
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        self.close_all();
    }

    fn close_all(&self) {
        let drained: Vec<_> = {
            let mut waiting = lock(&self.waiting);
            self.closed.store(true, Ordering::SeqCst);
            waiting.drain().collect()
        };
        for (_, tx) in drained {
            let _ = tx.send(Err(CdpError::Closed));
        }
        // Dropping the senders wakes every Expect with Closed.
        lock(&self.expects).clear();
    }

    fn dispatch(&self, raw: &[u8]) {
        let Ok(msg) = serde_json::from_slice::<Value>(raw) else {
            return;
        };
        if let Some(id) = msg.get("id").and_then(Value::as_u64) {
            let Some(tx) = lock(&self.waiting).remove(&id) else {
                return;
            };
            let answer = match msg.get("error") {
                Some(err) => Err(CdpError::Remote {
                    code: err.get("code").and_then(Value::as_i64).unwrap_or(0),
                    message: err
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                }),
                None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
            };
            let _ = tx.send(answer);
            return;
        }
        let Some(method) = msg.get("method").and_then(Value::as_str) else {
            return;
        };
        let session = msg
            .get("sessionId")
            .and_then(Value::as_str)
            .map(|s| SessionId(s.to_string()));
        if method == "Page.javascriptDialogOpening" {
            // An unanswered alert/confirm/prompt blocks the page's main thread,
            // and every later call on that page would time out behind it.
            let id = self.next.fetch_add(1, Ordering::SeqCst);
            let mut answer = json!({
                "id": id,
                "method": "Page.handleJavaScriptDialog",
                "params": { "accept": false },
            });
            if let Some(s) = &session {
                answer["sessionId"] = json!(s.0);
            }
            let _ = self.send(&answer);
        }
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let mut expects = lock(&self.expects);
        let mut i = 0;
        while i < expects.len() {
            let w = &expects[i];
            if w.method == method && w.session == session {
                let w = expects.remove(i);
                if w.tx.send(params.clone()).is_ok() {
                    return;
                }
                // Its receiver is gone; the next waiter may still want it.
                continue;
            }
            i += 1;
        }
    }
}

/// Standard base64, as `Page.captureScreenshot` returns its PNG. Whitespace is
/// skipped; anything else outside the alphabet is an error.
pub fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    fn value(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a') as u32 + 26),
            b'0'..=b'9' => Some((c - b'0') as u32 + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0u32;
    for &c in s.as_bytes() {
        if c == b'=' {
            break;
        }
        if c.is_ascii_whitespace() {
            continue;
        }
        let v = value(c).ok_or_else(|| format!("not base64: byte {c:#04x}"))?;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::os::unix::net::UnixStream;

    #[test]
    fn a_devtools_message_is_one_json_value_ended_by_a_nul() {
        let mut pending = Vec::new();
        // Two messages in one read.
        let got = frames(&mut pending, b"{\"id\":1}\0{\"id\":2}\0");
        assert_eq!(got, vec![b"{\"id\":1}".to_vec(), b"{\"id\":2}".to_vec()]);
        assert!(pending.is_empty());
        // One message split across three reads.
        assert!(frames(&mut pending, b"{\"me").is_empty());
        assert!(frames(&mut pending, b"thod\":\"A.b\"").is_empty());
        let got = frames(&mut pending, b"}\0{\"id\":3");
        assert_eq!(got, vec![b"{\"method\":\"A.b\"}".to_vec()]);
        // ...and an unfinished tail is kept, not lost or delivered early.
        assert_eq!(pending, b"{\"id\":3".to_vec());
        assert_eq!(frames(&mut pending, b"}\0"), vec![b"{\"id\":3}".to_vec()]);
    }

    /// A fake browser: the far end of a socket pair, reading NUL-framed
    /// commands and writing whatever the test scripts back.
    struct Fake {
        reader: BufReader<UnixStream>,
        writer: UnixStream,
    }

    impl Fake {
        fn pair() -> (Arc<Cdp>, Fake) {
            let (ours, theirs) = UnixStream::pair().unwrap();
            // A read that never comes back is a failure, not a hang: the fake
            // gives up after five seconds, and the test says so.
            theirs.set_read_timeout(Some(T)).unwrap();
            let cdp = Cdp::over(Box::new(ours.try_clone().unwrap()), Box::new(ours));
            let fake = Fake {
                reader: BufReader::new(theirs.try_clone().unwrap()),
                writer: theirs,
            };
            (cdp, fake)
        }
        fn read(&mut self) -> Value {
            let mut buf = Vec::new();
            self.reader
                .read_until(0, &mut buf)
                .expect("the client sent nothing within five seconds");
            buf.pop();
            serde_json::from_slice(&buf).unwrap()
        }
        fn write(&mut self, v: Value) {
            let mut b = serde_json::to_vec(&v).unwrap();
            b.push(0);
            self.writer.write_all(&b).unwrap();
        }
    }

    const T: Duration = Duration::from_secs(5);

    #[test]
    fn a_reply_reaches_the_call_that_asked_even_when_events_arrive_first() {
        let (cdp, mut fake) = Fake::pair();
        // Two callers in flight at once, the first one asking first.
        let a = {
            let cdp = cdp.clone();
            std::thread::spawn(move || cdp.call(None, "A.one", json!({}), T))
        };
        let first = fake.read();
        let b = {
            let cdp = cdp.clone();
            std::thread::spawn(move || cdp.call(None, "B.two", json!({}), T))
        };
        let second = fake.read();
        assert_eq!(
            (&first["method"], &second["method"]),
            (&json!("A.one"), &json!("B.two"))
        );
        let (a_id, b_id) = (
            first["id"].as_u64().unwrap(),
            second["id"].as_u64().unwrap(),
        );
        // Events interleaved with the replies, and the replies in the opposite
        // order to the calls: the later caller is answered first.
        fake.write(json!({ "method": "Page.frameNavigated", "params": {} }));
        fake.write(json!({ "id": b_id, "result": { "who": "b" } }));
        fake.write(json!({ "method": "Network.loadingFinished", "params": {} }));
        fake.write(json!({ "id": a_id, "result": { "who": "a" } }));
        assert_eq!(a.join().unwrap().unwrap()["who"], "a");
        assert_eq!(b.join().unwrap().unwrap()["who"], "b");
    }

    #[test]
    fn an_event_awaited_before_its_cause_is_not_missed() {
        let (cdp, mut fake) = Fake::pair();
        let s = SessionId("S1".into());
        let loaded = cdp.expect(Some(&s), "Page.loadEventFired");
        let nav = {
            let cdp = cdp.clone();
            let s = s.clone();
            std::thread::spawn(move || cdp.call(Some(&s), "Page.navigate", json!({}), T))
        };
        let asked = fake.read();
        assert_eq!(asked["sessionId"], "S1");
        // The event lands before the command's own reply, as it can for a
        // small file, and one for another session must not satisfy it.
        fake.write(json!({ "method": "Page.loadEventFired", "sessionId": "OTHER", "params": { "timestamp": 1 } }));
        fake.write(json!({ "method": "Page.loadEventFired", "sessionId": "S1", "params": { "timestamp": 2 } }));
        fake.write(json!({ "id": asked["id"], "result": {} }));
        assert!(nav.join().unwrap().is_ok());
        assert_eq!(loaded.wait(T).unwrap()["timestamp"], 2);
    }

    #[test]
    fn a_dialog_the_page_opens_is_dismissed_without_blocking() {
        let (_cdp, mut fake) = Fake::pair();
        fake.write(json!({
            "method": "Page.javascriptDialogOpening",
            "sessionId": "PAGE",
            "params": { "type": "confirm", "message": "Delete every note in this brief?" },
        }));
        let answer = fake.read();
        assert_eq!(answer["method"], "Page.handleJavaScriptDialog");
        assert_eq!(answer["sessionId"], "PAGE");
        assert_eq!(answer["params"]["accept"], false);
    }

    #[test]
    fn a_browser_that_goes_away_answers_every_caller_at_once() {
        let (cdp, mut fake) = Fake::pair();
        let pending = {
            let cdp = cdp.clone();
            std::thread::spawn(move || cdp.call(None, "Browser.getVersion", json!({}), T))
        };
        let _ = fake.read();
        let waiting = cdp.expect(None, "Target.targetCreated");
        drop(fake);
        let started = std::time::Instant::now();
        assert_eq!(pending.join().unwrap(), Err(CdpError::Closed));
        assert_eq!(waiting.wait(T), Err(CdpError::Closed));
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "nobody waited out a timeout"
        );
        assert_eq!(cdp.call(None, "X.y", json!({}), T), Err(CdpError::Closed));
    }

    #[test]
    fn a_remote_error_says_what_the_browser_said() {
        let (cdp, mut fake) = Fake::pair();
        let call = {
            let cdp = cdp.clone();
            std::thread::spawn(move || cdp.call(None, "No.such", json!({}), T))
        };
        let asked = fake.read();
        fake.write(json!({ "id": asked["id"], "error": { "code": -32601, "message": "'No.such' wasn't found" } }));
        assert_eq!(
            call.join().unwrap(),
            Err(CdpError::Remote {
                code: -32601,
                message: "'No.such' wasn't found".into()
            })
        );
    }

    #[test]
    fn a_screenshot_arrives_as_base64() {
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64_decode("aGVsbG8h").unwrap(), b"hello!");
        assert_eq!(base64_decode("iVBO\nRw0K").unwrap(), b"\x89PNG\r\n");
        assert_eq!(base64_decode("").unwrap(), b"");
        assert!(base64_decode("a*b=").is_err());
    }
}
