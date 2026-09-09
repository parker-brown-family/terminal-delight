//! A pseudoterminal that is a socket.
//!
//! When a window attaches to a pane the session host owns, it has no PTY: the
//! kernel object belongs to the host process. What it has instead is a byte
//! stream — keystrokes out, terminal output in — which is the only part of a
//! PTY an emulator actually consumes. [`SocketPty`] presents that stream as
//! the thing alacritty's own event loop already knows how to drive, so the
//! client runs the stock reader thread, the stock parser and a stock `Term`
//! over a socket, with nothing forked and nothing reimplemented.
//!
//! That is the whole point of cutting the seam here. Every other candidate
//! cut — shipping rendered frames, shipping damage rectangles — asks the
//! client to stop being a terminal. This one asks it only to read from a
//! different file descriptor.
//!
//! **Two upstream facts this file depends on, and neither is enforced by the
//! compiler.** The event loop dispatches on integer keys that alacritty keeps
//! `pub(crate)`, so they are spelled out below; and it learns that a child has
//! gone by polling a second descriptor, which a socket does not have, so one
//! is manufactured. Both are named in the constants' own documentation because
//! both are the kind of thing an upgrade breaks silently.

use std::io;
use std::os::unix::net::UnixStream;
use std::sync::Arc;

use alacritty_terminal::event::{OnResize, WindowSize};
use alacritty_terminal::tty::{ChildEvent, EventedPty, EventedReadWrite};
use polling::{Event, PollMode, Poller};

/// The key alacritty's event loop treats as "the terminal is readable or
/// writable", and the key it treats as "the child did something".
///
/// Upstream declares both `pub(crate)` (`tty/unix.rs:32` and `:35` in
/// alacritty_terminal 0.26), so an out-of-crate pseudoterminal cannot name
/// them and has to agree by value. Get this wrong and nothing fails loudly:
/// output arrives on the wrong arm and the pane simply stops printing. **Check
/// these two lines against `tty/unix.rs` on every alacritty upgrade** — the
/// round-trip test at the bottom of this file is what catches it.
const PTY_READ_WRITE_TOKEN: usize = 0;
const PTY_CHILD_EVENT_TOKEN: usize = 1;

/// Whether the far end has hung up, asked without consuming anything.
///
/// This is a peek rather than a read on purpose: the same descriptor is the
/// event loop's, and a byte taken here is a byte the terminal never draws.
fn hung_up(stream: &UnixStream) -> bool {
    use std::os::fd::AsRawFd;
    let mut probe = 0u8;
    // `UnixStream::peek` is still unstable, and this needs a peek rather than
    // a read: the byte belongs to the event loop.
    let seen = unsafe {
        libc::recv(
            stream.as_raw_fd(),
            std::ptr::from_mut(&mut probe).cast::<libc::c_void>(),
            1,
            libc::MSG_PEEK | libc::MSG_DONTWAIT,
        )
    };
    match seen {
        // Zero bytes without an error is the definition of the other end
        // having closed. Anything readable means it has not.
        0 => true,
        n if n > 0 => false,
        _ => {
            let err = io::Error::last_os_error();
            // Nothing to read yet is not an ending; anything else is.
            !matches!(
                err.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            )
        }
    }
}

/// A byte stream dressed as the pseudoterminal alacritty's event loop expects.
pub struct SocketPty {
    reader: UnixStream,
    writer: UnixStream,
    /// A second descriptor onto the same stream, registered under the child
    /// key. See [`SocketPty::next_child_event`] for why a socket needs one.
    hangup_watch: UnixStream,
    reported: bool,
    announce_resize: Box<dyn FnMut(WindowSize) + Send>,
}

impl SocketPty {
    /// Wrap a connected stream, with nowhere to send resizes. Useful where the
    /// far end cannot act on one anyway; see [`SocketPty::with_resize`].
    pub fn new(stream: UnixStream) -> io::Result<Self> {
        Self::with_resize(stream, |_| {})
    }

    /// Wrap a connected stream and say where a resize goes.
    ///
    /// A resize is the one thing a socket cannot carry the way a real
    /// pseudoterminal does: there is no kernel object here to tell, and the
    /// grid the size describes lives in another process. So it is *announced*
    /// — handed to the caller to put on the control connection — rather than
    /// applied. That is not a limitation to work around, it is the rule the
    /// architecture asked for: the terminal's size is a fact the owner is
    /// told, never one it infers, because more than one client may be looking.
    ///
    /// The socket is put in non-blocking mode: the event loop reads it on the
    /// same thread it polls on, and a blocking read there would stall the
    /// whole loop rather than just this stream.
    pub fn with_resize(
        stream: UnixStream,
        announce_resize: impl FnMut(WindowSize) + Send + 'static,
    ) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        let writer = stream.try_clone()?;
        let hangup_watch = stream.try_clone()?;
        Ok(Self {
            reader: stream,
            writer,
            hangup_watch,
            reported: false,
            announce_resize: Box::new(announce_resize),
        })
    }

    /// Whether the far end has hung up. The host closing a pane's stream is
    /// how a client learns the pane is gone.
    pub fn ended(&self) -> bool {
        hung_up(&self.hangup_watch)
    }
}

impl EventedReadWrite for SocketPty {
    type Reader = UnixStream;
    type Writer = UnixStream;

    unsafe fn register(
        &mut self,
        poll: &Arc<Poller>,
        mut interest: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        interest.key = PTY_READ_WRITE_TOKEN;
        // Safety: both descriptors are owned by this struct, which the event
        // loop owns for as long as it polls them — the lifetime requirement
        // the trait states.
        unsafe {
            poll.add_with_mode(&self.reader, interest, mode)?;
            poll.add_with_mode(
                &self.hangup_watch,
                Event::readable(PTY_CHILD_EVENT_TOKEN),
                PollMode::Level,
            )
        }
    }

    fn reregister(
        &mut self,
        poll: &Arc<Poller>,
        mut interest: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        interest.key = PTY_READ_WRITE_TOKEN;
        poll.modify_with_mode(&self.reader, interest, mode)?;
        poll.modify_with_mode(
            &self.hangup_watch,
            Event::readable(PTY_CHILD_EVENT_TOKEN),
            PollMode::Level,
        )
    }

    fn deregister(&mut self, poll: &Arc<Poller>) -> io::Result<()> {
        poll.delete(&self.reader)?;
        poll.delete(&self.hangup_watch)
    }

    fn reader(&mut self) -> &mut Self::Reader {
        &mut self.reader
    }

    fn writer(&mut self) -> &mut Self::Writer {
        &mut self.writer
    }
}

impl OnResize for SocketPty {
    fn on_resize(&mut self, window_size: WindowSize) {
        (self.announce_resize)(window_size);
    }
}

impl EventedPty for SocketPty {
    /// A socket has no child, so its ending has to be manufactured — and the
    /// obvious route does not work.
    ///
    /// A zero-length read is what ends a stream, but the event loop never
    /// performs one: a hung-up descriptor arrives with the interrupt flag set
    /// and the loop skips it deliberately, so as not to do I/O on a dead
    /// terminal (`event_loop.rs:275`). Left there, a closed socket stays
    /// readable forever and the reader thread spins on an end it is not
    /// allowed to look at. Measured, not guessed: the first version of this
    /// file signalled the end from inside `read`, and the test below hung.
    ///
    /// So the same stream is registered a second time under the child key, and
    /// the hangup is established here by peeking — asking whether the far end
    /// has closed without consuming a byte the terminal still needs to draw.
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        if self.reported || !hung_up(&self.hangup_watch) {
            return None;
        }
        self.reported = true;
        // No exit status: the process that ended is on the other side of a
        // socket and its status is the host's to know. `None` says exactly
        // that, and must not be dressed up as a successful exit.
        Some(ChildEvent::Exited(None))
    }
}

/// The seam, driven for real: a stock alacritty event loop reading a socket
/// into a stock `Term`. No PTY, no shell, no fork — one socket pair, both ends
/// in this process, which is the same shape the client will have when the
/// other end is a session host.
#[cfg(test)]
mod attach {
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use alacritty_terminal::event::{Event as TermEvent, EventListener};
    use alacritty_terminal::event_loop::{EventLoop, Msg, Notifier};
    use alacritty_terminal::index::{Column, Line};
    use alacritty_terminal::sync::FairMutex;
    use alacritty_terminal::term::{Config, Term};

    use super::SocketPty;
    use crate::term::GridSize;

    #[derive(Clone, Default)]
    struct Seen(Arc<Mutex<Vec<TermEvent>>>);
    impl EventListener for Seen {
        fn send_event(&self, event: TermEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    /// Everything a test needs to play both sides: the host end of the socket,
    /// the replica terminal, a way to type into it, and the reader thread.
    struct Attached {
        host: UnixStream,
        term: Arc<FairMutex<Term<Seen>>>,
        notifier: Notifier,
        io_thread: std::thread::JoinHandle<(
            EventLoop<SocketPty, Seen>,
            alacritty_terminal::event_loop::State,
        )>,
    }

    /// A replica terminal fed by one end of a socket pair, with the other end
    /// returned so a test can play the host.
    fn attached(cols: usize, rows: usize) -> Attached {
        let (host, client) = UnixStream::pair().expect("socket pair");
        let pty = SocketPty::new(client).expect("wrap the socket");
        let size = GridSize { cols, rows };
        let seen = Seen::default();
        let term = Arc::new(FairMutex::new(Term::new(
            Config::default(),
            &size,
            seen.clone(),
        )));
        let event_loop = EventLoop::new(term.clone(), seen, pty, false, false).expect("event loop");
        let notifier = Notifier(event_loop.channel());
        Attached {
            host,
            term,
            notifier,
            io_thread: event_loop.spawn(),
        }
    }

    /// Poll for a condition rather than sleeping a fixed time: the event loop
    /// is a real thread, and a fixed sleep is how a suite becomes flaky.
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
                term.lock().grid()[Line(1)][Column(2)].c == 'd'
            }),
            "the socket's bytes never reached the grid"
        );
        let grid = term.lock();
        let row: String = (0..5).map(|c| grid.grid()[Line(0)][Column(c)].c).collect();
        assert_eq!(row, "hello");
        assert_eq!(
            grid.grid()[Line(1)][Column(0)].fg,
            alacritty_terminal::vte::ansi::Color::Named(
                alacritty_terminal::vte::ansi::NamedColor::Red
            ),
            "colour survived the socket"
        );
    }

    #[test]
    fn keystrokes_travel_back_up_the_socket() {
        use std::io::Read;
        let mut a = attached(20, 5);
        a.notifier
            .0
            .send(Msg::Input(b"ls -la\n".to_vec().into()))
            .expect("queue input");

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
        // is in another process, so the new size has to travel as a message. A
        // client that quietly swallowed it would be a client assuming it is the
        // only one looking, which is the assumption v1 is not allowed to make.
        use alacritty_terminal::event::WindowSize;

        let (_host, client) = UnixStream::pair().expect("socket pair");
        let told = Arc::new(Mutex::new(Vec::new()));
        let sink = told.clone();
        let pty = SocketPty::with_resize(client, move |size: WindowSize| {
            sink.lock().unwrap().push((size.num_cols, size.num_lines));
        })
        .expect("wrap the socket");

        let seen = Seen::default();
        let term = Arc::new(FairMutex::new(Term::new(
            Config::default(),
            &GridSize { cols: 20, rows: 5 },
            seen.clone(),
        )));
        let event_loop = EventLoop::new(term, seen, pty, false, false).expect("event loop");
        let notifier = Notifier(event_loop.channel());
        let io_thread = event_loop.spawn();

        notifier
            .0
            .send(Msg::Resize(WindowSize {
                num_cols: 100,
                num_lines: 30,
                cell_width: 8,
                cell_height: 16,
            }))
            .expect("queue the resize");

        assert!(
            within(Duration::from_secs(5), || !told.lock().unwrap().is_empty()),
            "the resize was swallowed instead of announced"
        );
        assert_eq!(told.lock().unwrap().as_slice(), &[(100, 30)]);
        let _ = notifier.0.send(Msg::Shutdown);
        let _ = io_thread.join();
    }

    #[test]
    fn the_host_hanging_up_ends_the_replica_rather_than_spinning_it() {
        // The manufactured child event, earning its keep: without it a closed
        // socket stays readable forever, and the reader thread spins on an end
        // it has no way to recognise.
        //
        // The thread finishing is the assertion, deliberately. Upstream only
        // emits `ChildExit` when it has an exit status to report, and a socket
        // never does — the process that ended is the host's, and its status is
        // the host's to know. Asserting on an event that cannot arrive would be
        // asserting that we invented a status we do not have.
        let a = attached(20, 5);
        drop(a.host);

        assert!(
            within(Duration::from_secs(5), || a.io_thread.is_finished()),
            "the replica's reader thread never noticed the host hang up"
        );
        a.io_thread.join().expect("reader thread ended cleanly");
    }
}
