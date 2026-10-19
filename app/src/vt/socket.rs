//! A terminal whose program is in another process: the session host's.
//!
//! When a window attaches to a pane the host owns, it has no pseudoterminal —
//! the kernel object is the host's — only a byte stream, output in and
//! keystrokes out, which is the only part of a pseudoterminal an emulator
//! consumes. [`SocketSource`] is that stream as a [`Source`], so the replica
//! runs the same loop, the same core and the same `Term` as a pane this window
//! started, over a different file descriptor.
//!
//! This replaces `socketpty.rs`, which had to dress the socket up as the
//! pseudoterminal alacritty's loop expected: it named two of that loop's
//! private poll keys by value, registered the socket twice so a hang-up could
//! be noticed, and invented a child event to end on. With the loop TD's own,
//! a closed stream is simply a read that returns nothing.

use std::io::{self, Read, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::pump::{Source, Tap};
use super::WindowSize;

/// A connected stream, read and written by a terminal's loop.
pub struct SocketSource {
    stream: UnixStream,
    announce_resize: Box<dyn FnMut(WindowSize) + Send>,
}

impl SocketSource {
    /// Wrap a connected stream, with nowhere to send resizes.
    #[cfg(test)]
    pub fn new(stream: UnixStream) -> io::Result<Self> {
        Self::with_resize(stream, |_| {})
    }

    /// Wrap a connected stream and say where a resize goes.
    ///
    /// A resize is the one thing a socket cannot carry the way a real
    /// pseudoterminal does: there is no kernel object to tell, and the grid
    /// the size describes lives in another process. So it is *announced* —
    /// handed to the caller for the control connection — never applied. The
    /// terminal's size is a fact its owner is told, because more than one
    /// window may be looking.
    pub fn with_resize(
        stream: UnixStream,
        announce_resize: impl FnMut(WindowSize) + Send + 'static,
    ) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self {
            stream,
            announce_resize: Box::new(announce_resize),
        })
    }
}

impl Source for SocketSource {
    fn fd(&self) -> BorrowedFd<'_> {
        self.stream.as_fd()
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.write(buf)
    }

    fn resize(&mut self, size: WindowSize) -> io::Result<()> {
        (self.announce_resize)(size);
        Ok(())
    }
}

/// Every byte handed to a replica's parser, counted under the same lock the
/// parse happens under.
///
/// The count is the window's half of a shared clock. The host counts what it
/// put into this pane's stream; a socket delivers in order and drops nothing,
/// so when the two numbers agree the two terminals have seen exactly the same
/// bytes, and comparing their grids means something. Compare grids without it
/// and a replica that is merely *behind* looks exactly like one that is
/// *wrong*.
pub struct CountingTap(pub Arc<AtomicU64>);

impl Tap for CountingTap {
    fn tap(&mut self, bytes: &[u8]) {
        self.0.fetch_add(bytes.len() as u64, Ordering::Relaxed);
    }
}

/// A replica core fed by one end of a socket pair, the other end played by
/// the test — no host, no pseudoterminal, no fork. The seam driven for real.
#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use super::SocketSource;
    use crate::vt::{
        self, Color, Column, Event, Line, Listener, NamedColor, Notifier, Shared, Term, TermSize,
        WindowSize,
    };

    #[derive(Clone, Default)]
    struct Seen(Arc<Mutex<Vec<Event>>>);
    impl Listener for Seen {
        fn send_event(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    struct Attached {
        host: UnixStream,
        seen: Arc<Seen>,
        term: Shared,
        notifier: Notifier,
        thread: std::thread::JoinHandle<()>,
    }

    fn attached(cols: usize, rows: usize) -> Attached {
        let (host, client) = UnixStream::pair().expect("socket pair");
        let source = SocketSource::new(client).expect("wrap the socket");
        let seen = Arc::new(Seen::default());
        let term = vt::shared(Term::new(TermSize::new(cols, rows, 8, 16), seen.clone()));
        let (notifier, thread) =
            vt::pump::spawn(term.clone(), seen.clone(), Box::new(source), None).expect("loop");
        Attached {
            host,
            seen,
            term,
            notifier,
            thread,
        }
    }

    /// Poll for a condition rather than sleeping a fixed time: the loop is a
    /// real thread, and a fixed sleep is how a suite becomes flaky.
    fn within(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + limit;
        while Instant::now() < deadline {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        done()
    }

    #[test]
    fn output_written_to_the_socket_lands_in_the_replica_grid() {
        let mut a = attached(20, 5);
        a.host
            .write_all(b"hello\r\n\x1b[31mred\x1b[0m")
            .expect("host writes");

        let term = a.term.clone();
        assert!(
            within(Duration::from_secs(5), || {
                term.lock().cell(vt::Point::new(Line(1), Column(2))).c == 'd'
            }),
            "the socket's bytes never reached the grid"
        );
        let grid = term.lock();
        let row: String = (0..5).map(|c| grid.row(Line(0))[Column(c)].c).collect();
        assert_eq!(row, "hello");
        assert_eq!(
            grid.row(Line(1))[Column(0)].fg,
            Color::Named(NamedColor::Red),
            "colour survived the socket"
        );
    }

    #[test]
    fn keystrokes_travel_back_up_the_socket() {
        let mut a = attached(20, 5);
        a.notifier.notify(b"ls -la\n".to_vec());

        a.host
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut buf = [0u8; 32];
        let read = a.host.read(&mut buf).expect("host reads the keystrokes");
        assert_eq!(&buf[..read], b"ls -la\n");
    }

    #[test]
    fn a_resize_is_announced_rather_than_applied() {
        // There is no kernel object here to resize and the authoritative grid
        // is in another process, so the new size has to travel as a message.
        let (_host, client) = UnixStream::pair().expect("socket pair");
        let told = Arc::new(Mutex::new(Vec::new()));
        let sink = told.clone();
        let source = SocketSource::with_resize(client, move |size: WindowSize| {
            sink.lock().unwrap().push((size.num_cols, size.num_lines));
        })
        .expect("wrap the socket");

        let seen = Arc::new(Seen::default());
        let term = vt::shared(Term::new(TermSize::new(20, 5, 8, 16), seen.clone()));
        let (notifier, thread) = vt::pump::spawn(term, seen, Box::new(source), None).expect("loop");

        notifier.resize(WindowSize {
            num_cols: 100,
            num_lines: 30,
            cell_width: 8,
            cell_height: 16,
        });

        assert!(
            within(Duration::from_secs(5), || !told.lock().unwrap().is_empty()),
            "the resize was swallowed instead of announced"
        );
        assert_eq!(told.lock().unwrap().as_slice(), &[(100, 30)]);
        notifier.shutdown();
        let _ = thread.join();
    }

    #[test]
    fn the_host_hanging_up_ends_the_replica_rather_than_spinning_it() {
        // The thread finishing is the assertion. A socket has no child and no
        // exit status — the process that ended is the host's — so the end is
        // an `Exit` and never a `ChildExit` with an invented status.
        let a = attached(20, 5);
        drop(a.host);

        assert!(
            within(Duration::from_secs(5), || a.thread.is_finished()),
            "the replica's loop never noticed the host hang up"
        );
        a.thread.join().expect("the loop ended cleanly");
        let seen = a.seen.0.lock().unwrap();
        assert!(
            seen.iter().any(|e| matches!(e, Event::Exit)),
            "the end was never announced: {seen:?}"
        );
        assert!(
            !seen.iter().any(|e| matches!(e, Event::ChildExit(_))),
            "a replica invented an exit status: {seen:?}"
        );
    }
}
