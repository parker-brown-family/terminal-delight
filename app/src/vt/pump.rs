//! Terminal Delight's read loop: one thread per terminal, moving bytes from
//! wherever the terminal's program is into its core, and keystrokes back.
//!
//! This used to be alacritty's `EventLoop`, and TD built three things on top of
//! its private timing: the session host's attach fence (a lease on alacritty's
//! mutex, held across a read), the host's tee to an attached window (a reader
//! wrapped around the pseudoterminal), and the replica's byte count (another
//! reader, around a socket dressed up as a pseudoterminal). Owning the loop
//! turns each of them into a plain statement about one lock:
//!
//! **Every chunk is read outside the lock and then, under the lock, tapped,
//! counted and parsed in one step.** Whoever else takes the lock — the renderer,
//! the snapshot encoder, the divergence guard — sees a terminal and a byte count
//! that describe the same moment, with nothing half-way between them. The lock is
//! released fairly after every chunk, so a renderer waiting on it goes next.
//!
//! The loop does what alacritty's did, in the same order, with two exceptions
//! it states where they happen: a resize that fails is reported and survived
//! rather than ending the process, and a pseudoterminal's child is watched
//! through a pidfd rather than a process-wide `SIGCHLD` handler.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::io::{self, ErrorKind};
use std::os::fd::{AsRawFd, BorrowedFd};
use std::process::ExitStatus;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use parking_lot::MutexGuard;
use polling::{Event as Interest, Events, PollMode, Poller};

use super::{Event, Listener, Shared, WindowSize};

/// The poller key for the terminal's own descriptor.
const SOURCE: usize = 0;
/// The poller key for its child's pidfd.
const CHILD: usize = 1;

/// Bytes read per chunk, and so the most parsed under one hold of the lock.
/// At the 97 MB/s rio-vt parsed on the research bench that is under a
/// millisecond of the renderer waiting.
const CHUNK: usize = 0x1_0000;

/// Bytes read per turn before the loop goes back to its channel, so a flood
/// cannot starve keystrokes queued behind it. alacritty's limit, kept.
const TURN: usize = 0x10_0000;

/// What the rest of the application sends a terminal's loop.
#[derive(Debug)]
pub enum Msg {
    /// Bytes for the program: keystrokes, pastes, replies to its questions.
    Input(Cow<'static, [u8]>),
    /// The terminal's new size, for the source to pass on: a pseudoterminal
    /// tells the kernel, a replica announces it to its host.
    Resize(WindowSize),
    /// Stop reading and let the source go.
    Shutdown,
}

/// The way into a terminal's loop.
///
/// Cloning gives another handle on the same loop. Dropping every handle does
/// not stop it: like alacritty's, the loop runs until its program ends or it is
/// told to shut down, because a pane closing is a decision somebody makes, not
/// a side effect of a value going out of scope.
#[derive(Clone)]
pub struct Notifier {
    tx: Sender<Msg>,
    poller: Arc<Poller>,
}

impl Notifier {
    /// Queue bytes for the program. An empty write is dropped: a zero-length
    /// write to a pseudoterminal is a way to hang a terminal, not to say nothing.
    ///
    /// The terminal-input audit counts calls spelled `notifier.notify(` — see
    /// `the_manifest_counts_every_write_to_a_terminal` — which is why this is an
    /// inherent method with alacritty's name rather than something new.
    pub fn notify<B: Into<Cow<'static, [u8]>>>(&self, bytes: B) {
        let bytes = bytes.into();
        if bytes.is_empty() {
            return;
        }
        self.send(Msg::Input(bytes));
    }

    pub fn resize(&self, size: WindowSize) {
        self.send(Msg::Resize(size));
    }

    pub fn shutdown(&self) {
        self.send(Msg::Shutdown);
    }

    /// Queue a message and wake the loop. `false` when the loop has ended.
    pub fn send(&self, msg: Msg) -> bool {
        let sent = self.tx.send(msg).is_ok();
        let _ = self.poller.notify();
        sent
    }
}

/// Something that sees every chunk of output, under the terminal's lock, before
/// the core parses it: the host's copy to an attached window, the replica's
/// byte count.
pub trait Tap: Send {
    fn tap(&mut self, bytes: &[u8]);
}

/// Where a terminal's bytes come from and its keystrokes go.
pub trait Source: Send {
    /// The descriptor to wait on. Must be non-blocking.
    fn fd(&self) -> BorrowedFd<'_>;
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    fn write(&mut self, buf: &[u8]) -> io::Result<usize>;
    fn resize(&mut self, size: WindowSize) -> io::Result<()>;
    /// A descriptor that becomes readable when the program exits, if there is
    /// a program on this side to watch. A replica has none: its program is
    /// the host's.
    fn child_fd(&self) -> Option<BorrowedFd<'_>> {
        None
    }
    /// The program's exit status, once it has exited.
    fn reap(&mut self) -> Option<ExitStatus> {
        None
    }
}

/// Start a terminal's loop on its own thread.
///
/// The source is registered before the thread starts, so a descriptor the
/// poller refuses is an error here rather than a thread that ends silently.
pub fn spawn(
    term: Shared,
    listener: Arc<dyn Listener>,
    source: Box<dyn Source>,
    tap: Option<Box<dyn Tap>>,
) -> io::Result<(Notifier, JoinHandle<()>)> {
    let poller = Arc::new(Poller::new()?);
    let (tx, rx) = mpsc::channel();
    let pump = Pump {
        term,
        listener,
        source,
        tap,
        poller: poller.clone(),
        rx,
        queue: VecDeque::new(),
        reading: true,
        wants_write: false,
        deadline: None,
    };
    // Safety: the source and its child descriptor are owned by the pump, which
    // deregisters them before it drops them — the lifetime `add` asks for.
    unsafe {
        pump.poller.add_with_mode(
            pump.source.fd().as_raw_fd(),
            Interest::readable(SOURCE),
            PollMode::Level,
        )?;
        if let Some(child) = pump.source.child_fd() {
            pump.poller.add_with_mode(
                child.as_raw_fd(),
                Interest::readable(CHILD),
                PollMode::Level,
            )?;
        }
    }
    let thread = std::thread::Builder::new()
        .name("td-pump".into())
        .spawn(move || pump.run())?;
    Ok((Notifier { tx, poller }, thread))
}

/// Input on its way to the program, and how much of it has gone.
struct Writing {
    bytes: Cow<'static, [u8]>,
    written: usize,
}

/// How a read turn ended.
enum Read {
    /// The source would block: everything available was parsed.
    Drained,
    /// The turn's budget ran out with more to read.
    Budget,
    /// The source has nothing more to give, ever.
    Closed,
}

struct Pump {
    term: Shared,
    listener: Arc<dyn Listener>,
    source: Box<dyn Source>,
    tap: Option<Box<dyn Tap>>,
    poller: Arc<Poller>,
    rx: Receiver<Msg>,
    queue: VecDeque<Writing>,
    /// Whether the source is still registered for reading. A pseudoterminal
    /// whose far side has hung up reports that forever, so once it has, the
    /// loop stops asking and waits on the child instead.
    reading: bool,
    wants_write: bool,
    /// When the core's open synchronized update must be flushed, if one is
    /// open. Read under the lock after every parse, so waiting on it needs no
    /// lock at all.
    deadline: Option<Instant>,
}

impl Pump {
    fn run(mut self) {
        let mut events = Events::new();
        let mut buf = vec![0u8; CHUNK];
        'turn: loop {
            let timeout = self
                .deadline
                .map(|at| at.saturating_duration_since(Instant::now()));
            events.clear();
            if let Err(err) = self.poller.wait(&mut events, timeout) {
                if err.kind() == ErrorKind::Interrupted {
                    continue;
                }
                eprintln!("td: a terminal's loop could not wait: {err}");
                break;
            }

            // A synchronized update that has been open too long is drawn as it
            // stands, the way it would be if the program had closed it.
            if self.deadline.is_some_and(|at| Instant::now() >= at) {
                let mut term = self.term.lock();
                term.flush_sync();
                self.deadline = term.sync_deadline();
                drop(term);
                self.listener.send_event(Event::Wakeup);
            }

            loop {
                match self.rx.try_recv() {
                    Ok(Msg::Input(bytes)) => self.queue.push_back(Writing { bytes, written: 0 }),
                    Ok(Msg::Resize(size)) => {
                        // Reported and survived. alacritty exits the process
                        // here, which in a session host would end every pane
                        // it holds over one refused ioctl.
                        if let Err(err) = self.source.resize(size) {
                            eprintln!("td: a terminal refused its new size: {err}");
                        }
                    }
                    Ok(Msg::Shutdown) => break 'turn,
                    Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
                }
            }

            let mut child_gone = false;
            let mut readable = false;
            for event in events.iter() {
                match event.key {
                    CHILD => child_gone = true,
                    SOURCE => readable |= event.readable || event.is_interrupt(),
                    _ => {}
                }
            }

            if readable && self.reading {
                match self.read(&mut buf, TURN) {
                    Read::Drained | Read::Budget => {}
                    Read::Closed => {
                        if self.source.child_fd().is_none() {
                            // A stream with no program behind it has ended.
                            self.end(None);
                            break;
                        }
                        // A pseudoterminal hangs up before its child is reaped;
                        // the child's own descriptor says when it has gone.
                        self.stop_reading();
                    }
                }
            }

            if child_gone {
                // Whatever the program wrote on its way out is still in the
                // pseudoterminal; draw it before saying it has gone.
                if self.reading {
                    while let Read::Budget = self.read(&mut buf, TURN) {}
                }
                if let Some(status) = self.source.reap() {
                    self.end(Some(status));
                    break;
                }
            }

            self.write();
            self.update_interest();
        }
        let _ = self.poller.delete(self.source.fd());
        if let Some(child) = self.source.child_fd() {
            let _ = self.poller.delete(child);
        }
    }

    /// Read and parse until the source would block, closes, or `budget`
    /// bytes have gone through.
    fn read(&mut self, buf: &mut [u8], budget: usize) -> Read {
        let mut processed = 0;
        let mut synced = 0;
        let outcome = loop {
            let got = match self.source.read(buf) {
                Ok(0) => break Read::Closed,
                Ok(got) => got,
                Err(err) => match err.kind() {
                    ErrorKind::Interrupted => continue,
                    ErrorKind::WouldBlock => break Read::Drained,
                    // `EIO` is how Linux says a pseudoterminal's far side has
                    // hung up; any other error ends the stream the same way.
                    _ => break Read::Closed,
                },
            };
            let mut term = self.term.lock();
            if let Some(tap) = self.tap.as_mut() {
                tap.tap(&buf[..got]);
            }
            term.advance(&buf[..got]);
            synced = term.sync_bytes_count();
            self.deadline = term.sync_deadline();
            MutexGuard::unlock_fair(term);
            processed += got;
            if processed >= budget {
                break Read::Budget;
            }
        };
        // alacritty's rule: no redraw when every byte went into a synchronized
        // update, because the point of one is that nothing is drawn until it
        // closes.
        if processed > 0 && synced < processed {
            self.listener.send_event(Event::Wakeup);
        }
        outcome
    }

    fn stop_reading(&mut self) {
        self.reading = false;
        // Deleted rather than narrowed: epoll reports a hang-up whatever
        // interest is registered, and a level-triggered hang-up is a busy loop.
        let _ = self.poller.delete(self.source.fd());
        self.queue.clear();
    }

    /// Write queued input until the source would block.
    fn write(&mut self) {
        if !self.reading {
            self.queue.clear();
            return;
        }
        while let Some(front) = self.queue.front_mut() {
            match self.source.write(&front.bytes[front.written..]) {
                Ok(0) => break,
                Ok(n) => {
                    front.written += n;
                    if front.written == front.bytes.len() {
                        self.queue.pop_front();
                    }
                }
                Err(err)
                    if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) =>
                {
                    break
                }
                // The program's side is gone. The read side learns the same
                // thing and ends the terminal; input for it has nowhere to go.
                Err(_) => {
                    self.queue.clear();
                    break;
                }
            }
        }
    }

    /// Ask to hear about writability only while there is something to write.
    fn update_interest(&mut self) {
        let wants_write = !self.queue.is_empty();
        if !self.reading || wants_write == self.wants_write {
            return;
        }
        self.wants_write = wants_write;
        let mut interest = Interest::readable(SOURCE);
        interest.writable = wants_write;
        let _ = self
            .poller
            .modify_with_mode(self.source.fd(), interest, PollMode::Level);
    }

    /// The terminal has ended. alacritty's order: the child's status if there
    /// is one, then the end, then a last redraw.
    fn end(&mut self, status: Option<ExitStatus>) {
        if let Some(status) = status {
            self.listener.send_event(Event::ChildExit(status));
        }
        self.listener.send_event(Event::Exit);
        self.listener.send_event(Event::Wakeup);
    }
}
