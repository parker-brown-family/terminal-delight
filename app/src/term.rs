//! G0b seam — real shell via alacritty_terminal (Option A from docs/PLAN.md §2).
//! Written clean-room against the crate's public API (docs.rs + registry source).
//!
//! alacritty_terminal's EventLoop owns the PTY reader thread, the VTE parser
//! pump, and the writer. We provide: an EventListener proxy that ships events
//! onto an async channel (consumed by the gpui entity), and keystroke bytes in
//! via the Notifier. NOTE: Event::PtyWrite (query responses like DA/DSR) is
//! emitted through the proxy and NOT auto-routed back to the PTY — the consumer
//! must bounce it via the notifier or interactive apps hang.

use std::fs::File;
use std::io;
use std::sync::Arc;

use alacritty_terminal::{
    event::{Event as TermEvent, EventListener, WindowSize},
    event_loop::{EventLoop, Msg, Notifier},
    grid::Dimensions,
    sync::FairMutex,
    term::{Config, Term},
    tty,
};
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};

/// Grid dimensions for Term::new (the crate's own TermSize lives in its test module).
#[derive(Clone, Copy, Debug)]
pub struct GridSize {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// Forwards terminal events from the EventLoop thread onto an async channel.
#[derive(Clone)]
pub struct EventProxy(UnboundedSender<TermEvent>);

impl EventListener for EventProxy {
    fn send_event(&self, event: TermEvent) {
        let _ = self.0.unbounded_send(event);
    }
}

pub struct Session {
    pub term: Arc<FairMutex<Term<EventProxy>>>,
    pub notifier: Notifier,
    /// Taken once by the UI entity to drive event handling.
    pub events: Option<UnboundedReceiver<TermEvent>>,
    /// Our own handle on the PTY master — used to ask the kernel what the
    /// foreground process is (tcgetpgrp), powering mode detection.
    pub master: Option<File>,
    pub shell_pid: u32,
}

impl Session {
    /// Resize both the emulation grid and the PTY (SIGWINCH to the child).
    pub fn resize(&self, size: GridSize, cell_width: u16, cell_height: u16) {
        let window_size = WindowSize {
            num_lines: size.rows as u16,
            num_cols: size.cols as u16,
            cell_width,
            cell_height,
        };
        let _ = self.notifier.0.send(Msg::Resize(window_size));
        self.term.lock().resize(size);
    }
}

/// Spawn the user's default shell on a PTY, emulation wired, I/O thread running.
#[allow(dead_code)]
pub fn spawn(size: GridSize, cell_width: u16, cell_height: u16) -> io::Result<Session> {
    spawn_in(size, cell_width, cell_height, None)
}

/// `spawn`, but the shell starts in `cwd` (session restore). A vanished dir
/// falls back to the default start directory rather than failing the pane.
pub fn spawn_in(
    size: GridSize,
    cell_width: u16,
    cell_height: u16,
    cwd: Option<std::path::PathBuf>,
) -> io::Result<Session> {
    let (tx, rx) = unbounded();
    let proxy = EventProxy(tx);

    let window_size = WindowSize {
        num_lines: size.rows as u16,
        num_cols: size.cols as u16,
        cell_width,
        cell_height,
    };

    let mut options = tty::Options {
        working_directory: cwd.filter(|d| d.is_dir()),
        ..Default::default()
    };
    // A demo window runs THIS binary as every pane's program — a frozen screen of
    // lorem-ipsum styled like a real agent session — instead of the user's shell.
    // So a shared demo shows no real shell, cwd, scrollback, or secret, yet flows
    // through the normal PTY→grid→warp/grade render path for full fidelity. Gated
    // on TD_DEMO, which is set only for the spawned demo window (see main).
    if std::env::var_os("TD_DEMO").is_some() {
        if let Ok(exe) = std::env::current_exe() {
            options.shell = Some(tty::Shell::new(
                exe.to_string_lossy().into_owned(),
                vec!["--td-emit-demo".to_string()],
            ));
        }
    }
    let pty = tty::new(&options, window_size, 0)?;
    let master = pty.file().try_clone().ok();
    let shell_pid = pty.child().id();
    let term = Arc::new(FairMutex::new(Term::new(
        Config::default(),
        &size,
        proxy.clone(),
    )));
    let event_loop = EventLoop::new(term.clone(), proxy, pty, false, false)?;
    let notifier = Notifier(event_loop.channel());
    let _io_thread = event_loop.spawn(); // owns its thread; lives as long as the PTY

    Ok(Session {
        term,
        notifier,
        events: Some(rx),
        master,
        shell_pid,
    })
}
