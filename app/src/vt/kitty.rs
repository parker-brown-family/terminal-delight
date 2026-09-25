//! What of the Kitty graphics protocol reaches the core, and what stops here.
//!
//! A Kitty picture travels as an application program command, an APC:
//! `ESC _ G <control> ; <payload> ESC \`. rio-vt parses those and keeps the
//! pictures, and two things it does with them are unsafe in a terminal whose
//! output TD does not control, and worse in a session host, the process that
//! holds every pane and is never restarted. The guard in front of rio-vt's
//! parser stops both, without changing anything a well-behaved program sees.
//!
//! **An APC with no end.** rio-vt buffers every byte of an APC until it is
//! terminated, with no limit, so a stream that opens one and never closes it —
//! a crafted file sent to `cat`, a hostile server's output — grows the
//! terminal's memory until the kernel kills it. Measured in review on
//! 2026-09-25: 768 MB of unterminated payload held 837 MB. The protocol caps a
//! chunk at 4096 bytes, so the guard lets an APC grow to [`MAX_APC`], then ends
//! it for the core, which fails the truncated command, and drops the rest.
//!
//! **A temporary file that is not one.** A picture sent as a temporary file
//! (`t=t`) is read and then deleted by whoever reads it, and kitty deletes only
//! a file in a real temporary directory whose name carries the marker
//! `tty-graphics-protocol`. rio-vt checks only that the marker appears
//! somewhere in the path, so `/tmp/tty-graphics-protocol-x/../../home/you/notes`
//! passes, and is read and deleted. The guard hands the core every `t=t` as
//! `t=f`, which it reads and leaves alone, and deletes the file itself, by
//! kitty's rule, once the core has read it.
//!
//! Everything else passes through as it arrived, and in the chunks it arrived
//! in. The guard holds back only a picture command's control data, never more
//! than [`MAX_CONTROL`] bytes, until its `;` says what the command is.

use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
/// Cancel: ends an escape sequence or a string for the core, with nothing
/// after it.
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;

/// The longest APC the core is given. The protocol splits a picture into
/// chunks of at most 4096 bytes; a thousand times that is room for programs
/// that do not, and caps what one pane can make the core hold.
pub(super) const MAX_APC: usize = 4 * 1024 * 1024;

/// A command's control data is a handful of `key=value` pairs; more than this
/// before the `;` is not a picture command anybody sends.
pub(super) const MAX_CONTROL: usize = 1024;

/// A temporary file's payload is a base64 path, never pixels.
const MAX_PATH: usize = 8 * 1024;

/// The marker kitty requires in the name of a temporary file it deletes.
const MARKER: &str = "tty-graphics-protocol";

/// Where the core's parser is, as far as the guard needs to know.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Ordinary output.
    #[default]
    Ground,
    /// After an `ESC`, which the core already has: `_` would open an APC.
    Escape,
    /// After `ESC _`: the next byte says whether it is a picture command.
    Opened,
    /// After `ESC _ G`: its control data, held until the `;`.
    Control,
    /// A command naming shared memory (`t=s`), held whole until its end: the
    /// name decides whether the core may open it.
    Shared,
    /// An APC's payload, passed through and counted, up to its end.
    Payload,
    /// An APC the core has been told is over, swallowed to its real end.
    Swallowing,
}

/// The guard. One per terminal, fed every chunk of its output in order.
#[derive(Debug, Default)]
pub(super) struct Guard {
    state: State,
    /// `_ G` and the control data after it, held until the guard knows what
    /// the command is. The `ESC` before them has already gone to the core.
    held: Vec<u8>,
    /// Payload bytes the core has taken for the open APC.
    taken: usize,
    /// The payload so far of a `t=t` command, the path the guard deletes once
    /// the core has read it; `None` for any other command.
    path: Option<Vec<u8>>,
    /// Temporary files whose commands the core has been handed whole, to be
    /// deleted once it has parsed them — which is not yet while a synchronized
    /// update holds them back. See [`Guard::delete_what_the_core_read`].
    handed_over: Vec<String>,
}

fn ends_a_string(byte: u8) -> bool {
    matches!(byte, ESC | BEL | CAN | SUB)
}

impl Guard {
    /// Hand `bytes` to `core`, less what the guard stops, in as few calls as
    /// it can: a chunk with no picture command in it is handed over whole.
    pub(super) fn feed(&mut self, bytes: &[u8], core: &mut dyn FnMut(&[u8])) {
        // `bytes[pass..i]` has been looked at and goes to the core unchanged.
        let mut pass = 0;
        let mut i = 0;
        while i < bytes.len() {
            match self.state {
                State::Ground => match bytes[i..].iter().position(|&b| b == ESC) {
                    Some(at) => {
                        i += at + 1;
                        self.state = State::Escape;
                    }
                    None => i = bytes.len(),
                },
                State::Escape => match bytes[i] {
                    b'_' => {
                        emit(core, &bytes[pass..i]);
                        self.held.clear();
                        self.held.push(b'_');
                        i += 1;
                        pass = i;
                        self.state = State::Opened;
                    }
                    // A second `ESC` starts the sequence again, for the core too.
                    ESC => i += 1,
                    _ => self.state = State::Ground,
                },
                State::Opened => {
                    if bytes[i] == b'G' {
                        self.held.push(b'G');
                        i += 1;
                        pass = i;
                        self.state = State::Control;
                    } else {
                        // Some other APC, or an empty one: the core's business,
                        // and still no longer than the limit.
                        self.open_payload(core, None);
                    }
                }
                State::Control => {
                    let end = bytes[i..]
                        .iter()
                        .position(|&b| b == b';' || ends_a_string(b))
                        .map_or(bytes.len(), |at| i + at);
                    self.held.extend_from_slice(&bytes[i..end]);
                    i = end;
                    pass = i;
                    if self.held.len() > MAX_CONTROL {
                        // Nothing of it has reached the core but the `ESC`,
                        // which a cancel takes back.
                        emit(core, &[CAN]);
                        self.held.clear();
                        self.state = State::Swallowing;
                        continue;
                    }
                    let Some(&next) = bytes.get(i) else {
                        break;
                    };
                    if next == b';' {
                        i += 1;
                        pass = i;
                        if self.control_has("t=s") {
                            self.held.push(b';');
                            self.state = State::Shared;
                        } else {
                            let path = self.rewrite_temporary_file();
                            self.held.push(b';');
                            self.open_payload(core, path);
                        }
                    } else {
                        // Control data and no payload: the core's to answer.
                        self.open_payload(core, None);
                    }
                }
                State::Shared => {
                    let end = bytes[i..]
                        .iter()
                        .position(|&b| ends_a_string(b))
                        .map_or(bytes.len(), |at| i + at);
                    self.held.extend_from_slice(&bytes[i..end]);
                    i = end;
                    pass = i;
                    let whole = bytes.get(i).copied();
                    if self.held.len() > MAX_CONTROL + MAX_PATH {
                        // No name is that long: nothing of it reaches the core.
                        emit(core, &[CAN]);
                        self.held.clear();
                        self.state = State::Swallowing;
                        continue;
                    }
                    let Some(last) = whole else {
                        break;
                    };
                    if matches!(last, ESC | BEL) && self.names_shared_memory_to_read() {
                        // The core gets it whole; `Payload` hands over its end.
                        emit(core, &self.held);
                        self.held.clear();
                        self.taken = 0;
                        self.path = None;
                        self.state = State::Payload;
                    } else {
                        // A pipe, which would hold the core's parser — and the
                        // terminal's lock — until something wrote to it; a
                        // device; nothing at all; or a cancelled command. The
                        // core never sees it, and `Swallowing` sees its end.
                        emit(core, &[CAN]);
                        self.held.clear();
                        self.state = State::Swallowing;
                    }
                }
                State::Payload => {
                    let end = bytes[i..]
                        .iter()
                        .position(|&b| ends_a_string(b))
                        .map_or(bytes.len(), |at| i + at);
                    let room = MAX_APC - self.taken;
                    if end - i > room {
                        // Over the limit. The core gets what fits and a cancel,
                        // which ends the APC for it; the rest goes nowhere.
                        emit(core, &bytes[pass..i + room]);
                        emit(core, &[CAN]);
                        self.path = None;
                        i += room;
                        pass = i;
                        self.state = State::Swallowing;
                        continue;
                    }
                    self.taken += end - i;
                    if let Some(path) = self.path.as_mut() {
                        if path.len() + (end - i) <= MAX_PATH {
                            path.extend_from_slice(&bytes[i..end]);
                        } else {
                            self.path = None;
                        }
                    }
                    i = end;
                    let Some(&last) = bytes.get(i) else {
                        break;
                    };
                    // The APC's end, which the core has to have parsed before
                    // the file it names is deleted. An `ESC` also begins what
                    // follows, so it is looked at again from there. A cancel
                    // abandons the command, so nothing was read to delete.
                    i += 1;
                    emit(core, &bytes[pass..i]);
                    pass = i;
                    self.state = if last == ESC {
                        State::Escape
                    } else {
                        State::Ground
                    };
                    let path = self.path.take();
                    if matches!(last, ESC | BEL) {
                        self.hand_over(path);
                    }
                }
                State::Swallowing => match bytes[i..].iter().position(|&b| ends_a_string(b)) {
                    Some(at) => {
                        let end = bytes[i + at];
                        // A bell or a cancel ended nothing the core still has
                        // open, so it goes too; an `ESC` begins something.
                        i += at;
                        if end != ESC {
                            i += 1;
                        }
                        pass = i;
                        self.state = State::Ground;
                    }
                    None => {
                        i = bytes.len();
                        pass = i;
                    }
                },
            }
        }
        emit(core, &bytes[pass..]);
    }

    /// Whether the held control data carries `pair`, such as `t=s`.
    fn control_has(&self, pair: &str) -> bool {
        self.held
            .get(2..)
            .and_then(|control| std::str::from_utf8(control).ok())
            .is_some_and(|control| control.split(',').any(|p| p == pair))
    }

    /// Whether a held `t=s` command names a shared-memory object the core can
    /// open without waiting: a regular file under `/dev/shm`, where
    /// `shm_open` looks, and not a pipe or a device someone left there.
    fn names_shared_memory_to_read(&self) -> bool {
        let Some(at) = self.held.iter().position(|&b| b == b';') else {
            return false;
        };
        let Some(name) = STANDARD
            .decode(&self.held[at + 1..])
            .ok()
            .and_then(|name| String::from_utf8(name).ok())
        else {
            return false;
        };
        let name = name.trim_start_matches('/');
        !name.is_empty()
            && !name.contains('/')
            && Path::new("/dev/shm")
                .join(name)
                .symlink_metadata()
                .is_ok_and(|meta| meta.is_file())
    }

    /// Hand the core the held opening and start counting the payload.
    fn open_payload(&mut self, core: &mut dyn FnMut(&[u8]), path: Option<Vec<u8>>) {
        emit(core, &self.held);
        self.held.clear();
        self.taken = 0;
        self.path = path;
        self.state = State::Payload;
    }

    /// A held `t=t` command's control data, rewritten to `t=f`, and an empty
    /// path to collect its payload into. `None`, and nothing rewritten, for
    /// any other command. A command that says more chunks follow (`m=1`) is
    /// rewritten but never deleted: the core has not read it when it ends.
    fn rewrite_temporary_file(&mut self) -> Option<Vec<u8>> {
        let control = std::str::from_utf8(self.held.get(2..)?).ok()?;
        let pairs: Vec<&str> = control.split(',').collect();
        if !pairs.contains(&"t=t") {
            return None;
        }
        let more = pairs.contains(&"m=1");
        let rewritten = pairs
            .iter()
            .map(|pair| if *pair == "t=t" { "t=f" } else { pair })
            .collect::<Vec<_>>()
            .join(",");
        self.held.truncate(2);
        self.held.extend_from_slice(rewritten.as_bytes());
        (!more).then(Vec::new)
    }

    /// Note a `t=t` command's path as handed to the core, whole.
    fn hand_over(&mut self, payload: Option<Vec<u8>>) {
        let path = payload
            .and_then(|payload| STANDARD.decode(payload).ok())
            .and_then(|path| String::from_utf8(path).ok());
        self.handed_over.extend(path);
    }

    /// Delete the temporary files handed to the core that kitty's rule lets a
    /// terminal delete. Call only once the core has parsed everything it was
    /// handed: while a synchronized update is open the core's parser holds
    /// its bytes back, and a file deleted then would never be drawn.
    pub(super) fn delete_what_the_core_read(&mut self) {
        for path in self.handed_over.drain(..) {
            if is_a_temporary_file(Path::new(&path)) {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

fn emit(core: &mut dyn FnMut(&[u8]), bytes: &[u8]) {
    if !bytes.is_empty() {
        core(bytes);
    }
}

/// kitty's rule for a temporary file a terminal may delete: a regular file,
/// not a link to one, whose name carries the marker, directly inside a
/// directory that really is a temporary one — judged after following every
/// link and `..` in the path, which is what the rule's copy in rio-vt left out.
pub(super) fn is_a_temporary_file(path: &Path) -> bool {
    let named = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.contains(MARKER));
    if !named || !path.symlink_metadata().is_ok_and(|meta| meta.is_file()) {
        return false;
    }
    let Some(parent) = path.parent().and_then(|dir| dir.canonicalize().ok()) else {
        return false;
    };
    temporary_directories().contains(&parent)
}

/// The directories a temporary file may be in: `/tmp`, `/dev/shm`, and
/// `$TMPDIR` when it is set, each as the path it resolves to.
fn temporary_directories() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("/tmp"), PathBuf::from("/dev/shm")];
    if let Some(dir) = std::env::var_os("TMPDIR") {
        dirs.push(PathBuf::from(dir));
    }
    dirs.push(std::env::temp_dir());
    dirs.into_iter()
        .filter_map(|dir| dir.canonicalize().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed `stream` split at `cuts`, and return what the core was handed and
    /// in how many calls.
    fn guarded(stream: &[u8], cuts: &[usize]) -> (Vec<u8>, usize) {
        let mut guard = Guard::default();
        let (mut out, mut calls) = (Vec::new(), 0);
        let mut at = 0;
        for &cut in cuts.iter().chain(std::iter::once(&stream.len())) {
            guard.feed(&stream[at..cut], &mut |bytes| {
                out.extend_from_slice(bytes);
                calls += 1;
            });
            at = cut;
        }
        guard.delete_what_the_core_read();
        (out, calls)
    }

    fn command(control: &str, payload: &[u8]) -> Vec<u8> {
        let mut out = format!("\x1b_G{control};").into_bytes();
        out.extend_from_slice(STANDARD.encode(payload).as_bytes());
        out.extend_from_slice(b"\x1b\\");
        out
    }

    /// A marked file directly in the temporary directory, as `kitten icat`
    /// writes one.
    fn temporary_file(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "{MARKER}-td-guard-{tag}-{}.rgb",
            std::process::id()
        ));
        std::fs::write(&path, [0xff, 0, 0]).expect("write the picture");
        path
    }

    #[test]
    fn ordinary_output_reaches_the_core_unchanged_and_in_one_piece() {
        let stream =
            b"plain \x1b[31mred\x1b[0m \x1b]0;title\x07 \x1bPq#0~\x1b\\ \x1b\x1b[1m done \x1b";
        let (out, calls) = guarded(stream, &[]);
        assert_eq!(out, stream.to_vec());
        assert_eq!(calls, 1, "no command in it, so nothing to split it at");
        for cut in 0..stream.len() {
            assert_eq!(guarded(stream, &[cut]).0, stream.to_vec(), "cut at {cut}");
        }
    }

    #[test]
    fn a_picture_carrying_its_pixels_reaches_the_core_unchanged() {
        let mut stream = b"before ".to_vec();
        stream.extend(command("a=T,f=100,t=d,m=1", b"first half"));
        stream.extend(command("m=0", b"second half"));
        stream.extend_from_slice(b"\x1b_Xnot a picture\x07 after");
        for cut in 0..stream.len() {
            assert_eq!(guarded(&stream, &[cut]).0, stream, "cut at {cut}");
        }
    }

    #[test]
    fn an_application_command_with_no_end_stops_growing_at_the_limit() {
        let mut stream = b"\x1b_Ga=T,f=100;".to_vec();
        stream.resize(stream.len() + 3 * MAX_APC, b'A');
        let (out, _) = guarded(&stream, &[MAX_APC / 3, MAX_APC, 2 * MAX_APC + 7]);
        assert!(
            out.len() <= MAX_APC + 32,
            "the core was handed {} bytes of one command",
            out.len()
        );
        assert_eq!(out.last(), Some(&CAN), "and told that it is over");

        // Its real end, and what follows, reach the core as they always did.
        let mut guard = Guard::default();
        let mut after = Vec::new();
        guard.feed(&stream, &mut |b| after.extend_from_slice(b));
        after.clear();
        guard.feed(b"AAAA\x1b\\\x1b[31mnext", &mut |b| {
            after.extend_from_slice(b)
        });
        assert_eq!(after, b"\x1b\\\x1b[31mnext".to_vec());
    }

    #[test]
    fn control_data_too_long_to_be_a_picture_command_never_reaches_the_core() {
        let mut stream = b"a\x1b_G".to_vec();
        stream.resize(stream.len() + 2 * MAX_CONTROL, b'x');
        stream.extend_from_slice(b"\x07b");
        let (out, _) = guarded(&stream, &[5, MAX_CONTROL]);
        assert_eq!(
            out,
            b"a\x1b\x18b".to_vec(),
            "the ESC, taken back, and the text after"
        );
    }

    #[test]
    fn a_temporary_file_is_read_as_a_file_and_deleted_by_the_guard_once_read() {
        let path = temporary_file("read");
        let stream = command(
            "a=T,q=2,f=24,s=1,v=1,t=t",
            path.to_str().unwrap().as_bytes(),
        );
        let mut guard = Guard::default();
        let mut out = Vec::new();
        let mut existed_when_handed_over = false;
        guard.feed(&stream, &mut |bytes| {
            out.extend_from_slice(bytes);
            existed_when_handed_over |= path.exists();
        });
        assert!(
            existed_when_handed_over,
            "the core is handed it before it goes"
        );
        assert!(path.exists(), "and it waits until the core has parsed it");
        guard.delete_what_the_core_read();
        assert!(!path.exists(), "then it goes, as kitty would delete it");
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("t=f") && !text.contains("t=t"), "{text}");
    }

    /// A name under `/dev/shm`, made with `make`, removed when dropped.
    struct ShmName(String);

    impl ShmName {
        fn new(tag: &str, make: impl FnOnce(&std::ffi::CStr)) -> Self {
            let name = format!("td-guard-{tag}-{}", std::process::id());
            let path = std::ffi::CString::new(format!("/dev/shm/{name}")).unwrap();
            make(&path);
            Self(name)
        }
    }

    impl Drop for ShmName {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(Path::new("/dev/shm").join(&self.0));
        }
    }

    #[test]
    fn shared_memory_that_is_a_pipe_never_reaches_the_core() {
        // `shm_open` on a pipe waits for a writer, and the core would wait
        // holding the terminal's lock — for ever, if nobody writes.
        let fifo = ShmName::new("fifo", |path| unsafe {
            assert_eq!(libc::mkfifo(path.as_ptr(), 0o600), 0, "mkfifo");
        });
        let mut stream = b"a".to_vec();
        stream.extend(command("a=T,f=24,s=1,v=1,t=s", fifo.0.as_bytes()));
        stream.extend_from_slice(b"b");
        let (out, _) = guarded(&stream, &[]);
        assert_eq!(
            out,
            b"a\x1b\x18\x1b\\b".to_vec(),
            "only the ESC, taken back"
        );
    }

    #[test]
    fn shared_memory_that_is_a_file_reaches_the_core_unchanged() {
        let object = ShmName::new("object", |path| {
            std::fs::write(Path::new(path.to_str().unwrap()), [0xff, 0, 0]).expect("the object");
        });
        let stream = command("a=T,f=24,s=1,v=1,t=s", format!("/{}", object.0).as_bytes());
        for cut in [0, 3, 10, stream.len() - 1] {
            assert_eq!(guarded(&stream, &[cut]).0, stream, "cut at {cut}");
        }
    }

    #[test]
    fn a_cancelled_command_deletes_nothing() {
        let path = temporary_file("cancelled");
        let mut stream = command("a=T,f=24,s=1,v=1,t=t", path.to_str().unwrap().as_bytes());
        // Its end replaced by a cancel: the core abandons it unread.
        stream.truncate(stream.len() - 2);
        stream.push(CAN);
        guarded(&stream, &[]);
        assert!(path.exists(), "an abandoned command's file was deleted");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_temporary_file_that_is_not_one_is_read_and_never_deleted() {
        // A marked directory in the temporary directory, and a file beside it
        // that is not marked, reached through the marked one by `..`: rio-vt's
        // own check would read it and delete it.
        let root = std::env::temp_dir().join(format!("td-guard-victim-{}", std::process::id()));
        let marked = root.join(format!("{MARKER}-dir"));
        std::fs::create_dir_all(&marked).expect("the marked directory");
        let victim = root.join("notes.txt");
        std::fs::write(&victim, b"precious").expect("the victim");
        let through = marked.join("..").join("notes.txt");
        // And a marked file one level down, which is not directly in a
        // temporary directory.
        let nested = root.join(format!("{MARKER}-nested.rgb"));
        std::fs::write(&nested, [0, 0, 0]).expect("the nested file");

        for path in [&through, &nested] {
            let stream = command("a=T,f=24,s=1,v=1,t=t", path.to_str().unwrap().as_bytes());
            let (out, _) = guarded(&stream, &[]);
            assert!(String::from_utf8_lossy(&out).contains("t=f"));
        }
        assert!(victim.exists(), "a file outside the rule was deleted");
        assert!(
            nested.exists(),
            "a file not directly in a temporary directory was deleted"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_temporary_file_sent_in_chunks_is_never_deleted_by_the_first() {
        let path = temporary_file("chunked");
        let stream = command(
            "a=T,f=24,s=1,v=1,t=t,m=1",
            path.to_str().unwrap().as_bytes(),
        );
        guarded(&stream, &[]);
        assert!(
            path.exists(),
            "the core has not read it when the first chunk ends"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn the_rewrite_is_the_same_wherever_the_chunks_fall() {
        let path = temporary_file("split");
        let stream = command("a=T,f=24,s=1,v=1,t=t", path.to_str().unwrap().as_bytes());
        let (whole, _) = {
            // The first run deletes the file; the rest only compare bytes.
            guarded(&stream, &[])
        };
        for cut in 0..stream.len() {
            assert_eq!(guarded(&stream, &[cut]).0, whole, "cut at {cut}");
        }
        std::fs::remove_file(&path).ok();
    }
}
