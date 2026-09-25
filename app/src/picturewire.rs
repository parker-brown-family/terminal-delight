//! Pictures on the wire between a session host and its window.
//!
//! A pane on a session host is read twice: by the host's core, which owns the
//! pseudoterminal and answers the program, and by the window's replica core,
//! which draws. For text that is one byte stream parsed twice, and the two
//! agree. A picture sent *by reference* breaks that. A Kitty graphics command
//! may name a temporary file (`t=t`) or a shared-memory object (`t=s`) instead
//! of carrying its pixels, and the terminal that reads one must delete it. The
//! host reads first — it copies each chunk to the window and then parses it —
//! so by the time the window's core looks, the picture is gone, and the window
//! draws nothing in the space the host reserved for it. `kitten icat` chooses
//! exactly those two whenever the terminal accepts them. Measured 2026-09-25 in
//! a hidden window on a session host: inline data and a plain file path drew,
//! and a temporary file did not.
//!
//! [`Inliner`] is the host's answer, applied to the copy it sends the window:
//! such a command is rewritten to carry its pixels inline (`t=d`), read from
//! the file or the shared memory *before* the host's own core deletes it.
//! Everything else passes through byte for byte. The host's core still parses
//! the original, so the program is answered, and the file deleted, exactly as
//! it would be with no window attached.
//!
//! The inliner reads only a file carrying kitty's marker, under the same
//! path guards and size cap as rio-vt's own reader. A command it cannot or
//! will not read goes through unchanged, for the window's core to read, or
//! fail to, by its own rules (`vt/kitty.rs`).
//!
//! A command ends where the cores' parser ends it: at `ESC \`, and also at a
//! bare `ESC`, a bell, or a cancel. Waiting for `ESC \` alone would hold back
//! whatever followed a command that ended any other way.

use std::borrow::Cow;
use std::io::{Read, Seek, SeekFrom};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;

/// Whether `byte` ends an application command for the cores' parser.
fn ends_a_command(byte: u8) -> bool {
    matches!(byte, ESC | BEL | CAN | SUB)
}

/// The largest picture read, rio-vt's own cap (`MAX_SIZE` in its Kitty
/// graphics reader).
const MAX_PICTURE: u64 = 400 * 1024 * 1024;

/// A command's control data is a handful of `key=value` pairs; one longer
/// than this is not something to hold back waiting for.
const MAX_CONTROL: usize = 1024;

/// A reference payload is a base64 path or object name, never pixels.
const MAX_REFERENCE: usize = 8 * 1024;

/// Base64 characters per chunk of a rewritten command, the Kitty protocol's
/// limit for a chunk of inline data.
const CHUNK: usize = 4096;

#[derive(Debug, Default)]
enum State {
    /// Ordinary output. `held` may hold the start of what may become a graphics
    /// command: `ESC`, `ESC _`.
    #[default]
    Text,
    /// After `ESC _`: an application command, not yet known to be graphics.
    Apc,
    /// After `ESC _ G`: reading the control data, up to `;` or the end.
    Control,
    /// A command that names a file or shared memory: held until it ends.
    Reference,
    /// Any other command: copied as it arrives, up to its end.
    Through,
}

/// Rewrites the Kitty graphics commands that name a temporary file or a
/// shared-memory object into commands that carry their pixels, for a second
/// terminal reading the same stream. One per pane, fed every chunk in order.
#[derive(Debug, Default)]
pub struct Inliner {
    state: State,
    /// Bytes held back while deciding what they are.
    held: Vec<u8>,
    /// Dropping the rest of a command a newly attached window never saw the
    /// start of. See [`Inliner::join_mid_stream`].
    skipping: bool,
    /// No window is attached, so what this returns goes nowhere, and a
    /// picture is not worth reading for it.
    unwatched: bool,
}

impl Inliner {
    /// Whether a window is attached to read what this returns.
    pub fn watched(&mut self, watched: bool) {
        self.unwatched = !watched;
    }

    /// A window has just been attached, and its stream starts here.
    ///
    /// Its snapshot was taken between two chunks, and a command being copied
    /// through when it was — a picture's pixels, sent a chunk at a time — has
    /// its opening in the old stream and its rest in the new one. The window
    /// would draw that rest as text, a screenful of base64, so it is dropped
    /// to the command's end. A command still being held is sent whole, and
    /// needs nothing: its opening has not gone anywhere yet.
    pub fn join_mid_stream(&mut self) {
        self.skipping = matches!(self.state, State::Through);
    }

    /// The bytes to send the window for this chunk of the pane's output.
    ///
    /// Borrowed, and costing one scan, for the common chunk: no command in
    /// progress and no `ESC _ G` in it.
    pub fn feed<'a>(&mut self, bytes: &'a [u8]) -> Cow<'a, [u8]> {
        if matches!(self.state, State::Text) && self.held.is_empty() && !has_graphics_start(bytes) {
            // An `ESC` or `ESC _` at the very end could still begin a
            // command in the next chunk; only then is anything held.
            let tail = trailing_lead(bytes);
            if tail == 0 {
                return Cow::Borrowed(bytes);
            }
            self.held.extend_from_slice(&bytes[bytes.len() - tail..]);
            self.state = if tail == 2 { State::Apc } else { State::Text };
            return Cow::Borrowed(&bytes[..bytes.len() - tail]);
        }
        let mut out = Vec::with_capacity(bytes.len());
        for &byte in bytes {
            self.step(byte, &mut out);
        }
        Cow::Owned(out)
    }

    fn step(&mut self, byte: u8, out: &mut Vec<u8>) {
        match self.state {
            State::Text => {
                if self.held.is_empty() {
                    if byte == ESC {
                        self.held.push(byte);
                    } else {
                        out.push(byte);
                    }
                } else if byte == b'_' {
                    self.held.push(byte);
                    self.state = State::Apc;
                } else {
                    self.release(out);
                    self.step(byte, out);
                }
            }
            State::Apc => {
                if byte == b'G' {
                    self.held.push(byte);
                    self.state = State::Control;
                } else {
                    // Some other application command: not ours to read.
                    out.append(&mut self.held);
                    self.state = State::Through;
                    self.step(byte, out);
                }
            }
            State::Control => {
                if ends_a_command(byte) {
                    // Control data and no payload: nothing to read.
                    out.append(&mut self.held);
                    self.end_command(byte, out);
                    return;
                }
                self.held.push(byte);
                if byte == b';' {
                    if names_a_reference(&self.held) {
                        self.state = State::Reference;
                    } else {
                        out.append(&mut self.held);
                        self.state = State::Through;
                    }
                } else if self.held.len() > MAX_CONTROL {
                    // Too long to be a command anybody sends.
                    out.append(&mut self.held);
                    self.state = State::Through;
                }
            }
            State::Reference => {
                if matches!(byte, ESC | BEL) {
                    // Whole: the cores read it at this byte. Rewritten, it
                    // carries its own ends, so a bell that only ended the
                    // original goes with it; an `ESC` may begin more.
                    let command = std::mem::take(&mut self.held);
                    let rewritten = if self.unwatched {
                        None
                    } else {
                        inline(&command)
                    };
                    match rewritten {
                        Some(rewritten) => {
                            out.extend_from_slice(&rewritten);
                            self.state = State::Text;
                            if byte == ESC {
                                self.held.push(ESC);
                            }
                        }
                        None => {
                            out.extend_from_slice(&command);
                            self.end_command(byte, out);
                        }
                    }
                } else if matches!(byte, CAN | SUB) {
                    // Cancelled: the cores read nothing, and neither does this.
                    out.append(&mut self.held);
                    self.end_command(byte, out);
                } else {
                    self.held.push(byte);
                    if self.held.len() > MAX_CONTROL + MAX_REFERENCE {
                        out.append(&mut self.held);
                        self.state = State::Through;
                    }
                }
            }
            State::Through => {
                if ends_a_command(byte) {
                    self.end_command(byte, out);
                } else if !self.skipping {
                    out.push(byte);
                }
            }
        }
    }

    /// A command has ended at `byte`. A bell or a cancel is part of its end,
    /// and goes where the command went; an `ESC` also begins whatever follows,
    /// so it is held to be looked at with the byte after it.
    fn end_command(&mut self, byte: u8, out: &mut Vec<u8>) {
        let skipped = std::mem::take(&mut self.skipping);
        self.state = State::Text;
        if byte == ESC {
            self.held.push(ESC);
        } else if !skipped {
            out.push(byte);
        }
    }

    /// Let go of a held `ESC` that did not begin a command.
    fn release(&mut self, out: &mut Vec<u8>) {
        out.append(&mut self.held);
        self.state = State::Text;
    }
}

/// Whether `bytes` contains the start of a graphics command, `ESC _ G`.
fn has_graphics_start(bytes: &[u8]) -> bool {
    bytes.windows(3).any(|w| w == [ESC, b'_', b'G'])
}

/// How many bytes at the end of `bytes` could begin a graphics command in the
/// next chunk: 1 for a lone `ESC`, 2 for `ESC _`, otherwise 0.
fn trailing_lead(bytes: &[u8]) -> usize {
    match bytes {
        [.., ESC, b'_'] => 2,
        [.., ESC] => 1,
        _ => 0,
    }
}

/// The control data of a held command: between `ESC _ G` and `;`.
fn control_of(held: &[u8]) -> &str {
    let body = held.get(3..).unwrap_or_default();
    let end = body.iter().position(|&b| b == b';').unwrap_or(body.len());
    std::str::from_utf8(&body[..end]).unwrap_or_default()
}

fn keys(control: &str) -> impl Iterator<Item = (&str, &str)> {
    control.split(',').filter_map(|pair| pair.split_once('='))
}

/// Whether a command's control data names a temporary file or shared memory,
/// in one piece rather than as a chunk of a longer command.
fn names_a_reference(held: &[u8]) -> bool {
    let control = control_of(held);
    let medium = keys(control).find(|(k, _)| *k == "t").map(|(_, v)| v);
    let more = keys(control).any(|(k, v)| k == "m" && v == "1");
    matches!(medium, Some("t" | "s")) && !more
}

/// The command rewritten to carry its pixels, or `None` to send it as it was.
/// `command` is `ESC _ G <control> ; <payload>`, without whatever ended it.
fn inline(command: &[u8]) -> Option<Vec<u8>> {
    let body = command.get(3..)?;
    let split = body.iter().position(|&b| b == b';')?;
    let control = std::str::from_utf8(&body[..split]).ok()?;
    let reference = STANDARD.decode(&body[split + 1..]).ok()?;
    let reference = std::str::from_utf8(&reference).ok()?;

    let field = |key: &str| -> u64 {
        keys(control)
            .find(|(k, _)| *k == key)
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(0)
    };
    let (size, offset) = (field("S"), field("O"));
    let medium = keys(control).find(|(k, _)| *k == "t").map(|(_, v)| v)?;
    let pixels = match medium {
        "t" => read_temp_file(reference, size, offset)?,
        "s" => read_shared_memory(reference, size, offset)?,
        _ => return None,
    };

    // The same command, carrying its data: every key kept but the medium, the
    // read's size and offset, and the chunk flag, which is set again below.
    let kept: Vec<&str> = control
        .split(',')
        .filter(|pair| {
            !matches!(
                pair.split_once('=').map(|(k, _)| k),
                Some("t" | "S" | "O" | "m")
            )
        })
        .collect();
    let mut head = kept.join(",");
    if !head.is_empty() {
        head.push(',');
    }
    head.push_str("t=d");

    let encoded = STANDARD.encode(pixels);
    let chunks: Vec<&[u8]> = if encoded.is_empty() {
        vec![&[][..]]
    } else {
        encoded.as_bytes().chunks(CHUNK).collect()
    };
    let mut out = Vec::with_capacity(encoded.len() + chunks.len() * 16 + head.len());
    for (i, chunk) in chunks.iter().enumerate() {
        let more = u8::from(i + 1 < chunks.len());
        out.extend_from_slice(b"\x1b_G");
        if i == 0 {
            out.extend_from_slice(head.as_bytes());
            out.extend_from_slice(format!(",m={more};").as_bytes());
        } else {
            out.extend_from_slice(format!("m={more};").as_bytes());
        }
        out.extend_from_slice(chunk);
        out.extend_from_slice(b"\x1b\\");
    }
    Some(out)
}

/// A temporary file, read the way rio-vt reads one — and refused where it
/// would refuse. Never deleted here: the host's core deletes it, next.
fn read_temp_file(path: &str, size: u64, offset: u64) -> Option<Vec<u8>> {
    use std::os::unix::fs::OpenOptionsExt;
    let lower = path.to_lowercase();
    if !path.contains("tty-graphics-protocol")
        || lower.contains("/proc/")
        || lower.contains("/sys/")
        || lower.contains("/dev/")
    {
        return None;
    }
    // Opened without waiting: this runs under the terminal's lock, and a pipe
    // left under a marked name would otherwise hold it until written to.
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    read_span(file, size, offset)
}

/// A POSIX shared-memory object, read without unlinking it: the host's core
/// unlinks it, next. Opened without waiting and without following a link,
/// like `shm_open`'s own open, and read only if it is a plain object.
fn read_shared_memory(name: &str, size: u64, offset: u64) -> Option<Vec<u8>> {
    use std::os::fd::FromRawFd;
    let name = std::ffi::CString::new(name).ok()?;
    let fd = unsafe {
        libc::shm_open(
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW,
            0,
        )
    };
    if fd < 0 {
        return None;
    }
    read_span(unsafe { std::fs::File::from_raw_fd(fd) }, size, offset)
}

/// `size` bytes from `offset`, or the whole object when `size` is 0 — refused
/// when it is not a regular file, or the span runs past the end or past the
/// cap. The type is read from the open descriptor, so what was checked is what
/// is read.
fn read_span(mut file: std::fs::File, size: u64, offset: u64) -> Option<Vec<u8>> {
    let meta = file.metadata().ok()?;
    if !meta.is_file() {
        return None;
    }
    let length = meta.len();
    let want = if size > 0 {
        size
    } else {
        length.checked_sub(offset)?
    };
    if want > MAX_PICTURE || offset.checked_add(want)? > length {
        return None;
    }
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut data = vec![0u8; want as usize];
    file.read_exact(&mut data).ok()?;
    Some(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed `stream` split at every position in `cuts` and join the output.
    fn through(stream: &[u8], cuts: &[usize]) -> Vec<u8> {
        let mut inliner = Inliner::default();
        let mut out = Vec::new();
        let mut at = 0;
        for &cut in cuts.iter().chain(std::iter::once(&stream.len())) {
            out.extend_from_slice(&inliner.feed(&stream[at..cut]));
            at = cut;
        }
        out
    }

    fn temp_png(tag: &str) -> (std::path::PathBuf, Vec<u8>) {
        let path = std::env::temp_dir().join(format!(
            "tty-graphics-protocol-td-{tag}-{}.png",
            std::process::id()
        ));
        let pixels: Vec<u8> = (0..=255u8).cycle().take(10_000).collect();
        std::fs::write(&path, &pixels).expect("write the picture");
        (path, pixels)
    }

    fn command(control: &str, payload: &[u8]) -> Vec<u8> {
        let mut out = format!("\x1b_G{control};").into_bytes();
        out.extend_from_slice(STANDARD.encode(payload).as_bytes());
        out.extend_from_slice(b"\x1b\\");
        out
    }

    /// Every command in `stream`, as (control, decoded payload).
    fn commands(stream: &[u8]) -> Vec<(String, Vec<u8>)> {
        let text = String::from_utf8_lossy(stream);
        text.split("\x1b_G")
            .skip(1)
            .map(|part| {
                let part = part.split("\x1b\\").next().unwrap();
                let (control, payload) = part.split_once(';').unwrap_or((part, ""));
                (
                    control.to_string(),
                    STANDARD.decode(payload).unwrap_or_default(),
                )
            })
            .collect()
    }

    #[test]
    fn ordinary_output_is_passed_through_untouched_however_it_is_split() {
        let stream = b"plain \x1b[31mred\x1b[0m \x1b_Xnot graphics\x1b\\ \x1b]0;title\x07 done";
        for cut in 0..stream.len() {
            assert_eq!(through(stream, &[cut]), stream.to_vec(), "cut at {cut}");
        }
    }

    #[test]
    fn a_picture_carrying_its_pixels_is_passed_through_untouched() {
        let mut stream = b"before ".to_vec();
        stream.extend(command("a=T,f=100,t=d,m=1", b"first half"));
        stream.extend(command("m=0", b"second half"));
        stream.extend_from_slice(b" after");
        for cut in 0..stream.len() {
            assert_eq!(through(&stream, &[cut]), stream, "cut at {cut}");
        }
    }

    #[test]
    fn a_temporary_file_is_sent_as_its_pixels_and_left_for_the_core_to_delete() {
        let (path, pixels) = temp_png("temp");
        let mut stream = b"x".to_vec();
        stream.extend(command(
            "a=T,q=2,f=100,t=t,i=9,c=20,r=6",
            path.to_str().unwrap().as_bytes(),
        ));
        stream.extend_from_slice(b"y");
        let out = through(&stream, &[]);
        assert!(path.exists(), "deleting is the host core's job, after this");
        std::fs::remove_file(&path).ok();

        assert_eq!(out.first(), Some(&b'x'));
        assert_eq!(out.last(), Some(&b'y'));
        let sent = commands(&out);
        assert!(sent.len() > 1, "10,000 bytes is more than one chunk");
        let (first, _) = &sent[0];
        assert!(first.contains("t=d"), "{first}");
        assert!(!first.contains("t=t"), "{first}");
        assert!(first.contains("i=9") && first.contains("c=20") && first.contains("q=2"));
        assert!(first.ends_with("m=1"), "{first}");
        assert_eq!(sent.last().unwrap().0, "m=0");
        let joined: Vec<u8> = sent.iter().flat_map(|(_, p)| p.clone()).collect();
        assert_eq!(joined, pixels, "the pixels, whole and in order");
    }

    #[test]
    fn the_rewrite_is_the_same_wherever_the_chunks_fall() {
        let (path, _) = temp_png("split");
        let stream = command("a=T,f=100,t=t", path.to_str().unwrap().as_bytes());
        let whole = through(&stream, &[]);
        for cut in 0..stream.len() {
            assert_eq!(through(&stream, &[cut]), whole, "cut at {cut}");
        }
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn shared_memory_is_sent_as_its_pixels_and_left_for_the_core_to_unlink() {
        let name = format!("/tty-graphics-protocol-td-{}", std::process::id());
        let pixels = b"shared memory pixels".to_vec();
        let c = std::ffi::CString::new(name.clone()).unwrap();
        unsafe {
            let fd = libc::shm_open(c.as_ptr(), libc::O_CREAT | libc::O_RDWR, 0o600);
            assert!(fd >= 0, "shm_open");
            let written = libc::write(fd, pixels.as_ptr().cast(), pixels.len());
            assert_eq!(written as usize, pixels.len());
            libc::close(fd);
        }
        let out = through(&command("a=T,f=24,s=1,v=1,t=s", name.as_bytes()), &[]);
        let still_there = unsafe { libc::shm_open(c.as_ptr(), libc::O_RDONLY, 0) };
        assert!(
            still_there >= 0,
            "unlinking is the host core's job, after this"
        );
        unsafe {
            libc::close(still_there);
            libc::shm_unlink(c.as_ptr());
        }
        let sent = commands(&out);
        assert_eq!(sent.len(), 1);
        assert!(sent[0].0.contains("t=d"), "{}", sent[0].0);
        assert_eq!(sent[0].1, pixels);
    }

    #[test]
    fn what_the_core_would_refuse_is_passed_through_for_the_window_to_refuse_too() {
        for reference in [
            "/etc/hostname",                      // a temp file without the marker
            "/proc/self/tty-graphics-protocol",   // a place rio-vt will not read
            "/tmp/tty-graphics-protocol-missing", // nothing there
        ] {
            let stream = command("a=T,f=100,t=t", reference.as_bytes());
            assert_eq!(through(&stream, &[]), stream, "{reference}");
        }
    }

    /// `t=t` naming `path`, ended by `end` rather than by `ESC \`.
    fn ended_by(path: &std::path::Path, end: &[u8]) -> Vec<u8> {
        let mut out = b"\x1b_Ga=T,f=100,t=t;".to_vec();
        out.extend_from_slice(STANDARD.encode(path.to_str().unwrap()).as_bytes());
        out.extend_from_slice(end);
        out
    }

    #[test]
    fn a_command_ended_by_a_bare_escape_is_sent_and_what_follows_is_not_held() {
        // The cores end a command at any ESC. Waiting here for `ESC \` held the
        // prompt that followed until kilobytes more output arrived.
        let (path, pixels) = temp_png("bare-escape");
        let out = through(&ended_by(&path, b"\x1b[31mprompt"), &[]);
        std::fs::remove_file(&path).ok();
        assert!(
            out.ends_with(b"\x1b[31mprompt"),
            "{:?}",
            String::from_utf8_lossy(&out)
        );
        let joined: Vec<u8> = commands(&out).iter().flat_map(|(_, p)| p.clone()).collect();
        assert_eq!(joined, pixels);
    }

    #[test]
    fn a_command_ended_by_a_bell_is_sent_without_the_bell() {
        // The bell only ended the original; before the window's parser, which
        // has no command open, it would ring.
        let (path, _) = temp_png("bell");
        let out = through(&ended_by(&path, b"\x07after"), &[]);
        std::fs::remove_file(&path).ok();
        assert!(!out.contains(&0x07), "the bell reached the window");
        assert!(out.ends_with(b"after"));
    }

    #[test]
    fn a_window_joining_inside_a_picture_is_sent_none_of_the_rest_of_it() {
        let mut inliner = Inliner::default();
        let _ = inliner.feed(b"before\x1b_Ga=T,f=100,t=d,m=1;AAAA");
        inliner.join_mid_stream();
        let out = inliner.feed(b"BBBB\x1b\\after").into_owned();
        assert_eq!(
            out,
            b"\x1b\\after".to_vec(),
            "no base64 for the window to print"
        );
    }

    #[test]
    fn with_no_window_attached_no_picture_is_read() {
        let (path, _) = temp_png("unwatched");
        let stream = command("a=T,f=100,t=t", path.to_str().unwrap().as_bytes());
        let mut inliner = Inliner::default();
        inliner.watched(false);
        let out = inliner.feed(&stream).into_owned();
        std::fs::remove_file(&path).ok();
        assert_eq!(out, stream, "sent as it came, to nobody");
    }

    #[test]
    fn a_pipe_named_as_shared_memory_is_not_waited_on() {
        // Opened without waiting and refused for not being a plain object:
        // this runs under the terminal's lock.
        let name = format!("td-inliner-fifo-{}", std::process::id());
        let path = std::ffi::CString::new(format!("/dev/shm/{name}")).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0, "mkfifo");
        let stream = command("a=T,f=24,s=1,v=1,t=s", format!("/{name}").as_bytes());
        let out = through(&stream, &[]);
        std::fs::remove_file(format!("/dev/shm/{name}")).ok();
        assert_eq!(out, stream);
    }

    #[test]
    fn a_plain_file_path_is_left_for_both_cores_to_read() {
        // Nobody deletes a `t=f` file, so both terminals can read it, and
        // copying it inline would only double the bytes on the wire.
        let stream = command("a=T,f=100,t=f", b"/home/someone/picture.png");
        assert_eq!(through(&stream, &[]), stream);
    }
}
