//! libmpv, found when the first video opens, and the two threads a playing
//! video runs on.
//!
//! # Why libmpv, and why it is loaded rather than linked
//!
//! A video is a file format, a codec, a clock, a sound card and a scaler, and
//! mpv is all five, with the codecs FFmpeg brings. Its render API has a
//! software renderer that draws the current frame into memory TD owns, at the
//! size TD asks for, in the byte order gpui's textures already use — so the
//! square gets frames the way it gets a decoded picture, and nothing in TD has
//! to know what H.264 is.
//!
//! It is opened with `dlopen` the first time a video is asked for, the way
//! the HTML engine looks for Chromium on PATH the first time a page is: a
//! machine without mpv still runs TD, and a video there goes to the desktop
//! with a sentence saying why ([`available`]). Linking it would make every
//! build of TD need libmpv to start at all, to play a file most sessions never
//! open.
//!
//! # Two threads, because libmpv says so
//!
//! The render API forbids the thread that renders from calling anything on
//! the player's handle, and forbids either callback from calling libmpv at
//! all (`render.h`, "Threading"). So a video runs on two threads of its own:
//!
//! - **control** owns the handle. It starts the player, loads the file, runs
//!   the person's commands, reads the player's events, and at the end tears
//!   the whole thing down, in the order libmpv requires: render context
//!   first, player second. It blocks on one Rust channel, which both the
//!   view's commands and libmpv's wakeup callback feed.
//! - **render** owns the render context and nothing else. mpv's update
//!   callback wakes it when a frame is due; it draws that frame at the size
//!   the view last asked for and leaves it where the view picks it up.
//!
//! The view never calls libmpv. It sends a command or a size and reads what
//! the threads left in [`Shared`], so dropping a [`Player`] can never race a
//! call on a handle that is being destroyed.
//!
//! # Unknown is not zero
//!
//! Every number the player reports arrives as an `Option`. A file whose
//! duration mpv cannot tell reports none, and the bar then draws no progress
//! at all rather than a video that is always at its start.
//!
//! No `crate::` paths: this file knows mpv and the standard library, nothing
//! of gpui or of TD.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ── the library ─────────────────────────────────────────────────────────────

/// `mpv_render_param`: a tag and a pointer to its value.
#[repr(C)]
struct RenderParam {
    kind: c_int,
    data: *mut c_void,
}

/// The first four fields of `mpv_event`, which is all of it.
#[repr(C)]
struct MpvEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

/// `mpv_event_property`.
#[repr(C)]
struct EventProperty {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}

/// The first two fields of `mpv_event_end_file`, the only two read.
#[repr(C)]
struct EventEndFile {
    reason: c_int,
    error: c_int,
}

// The numbers below are libmpv's ABI, copied from `client.h` and `render.h`
// of mpv 0.41 (client API 2.5). They have not moved since the software
// renderer arrived; a library that lacks it answers `render_create` with an
// error, which becomes the sentence in the square.
const PARAM_END: c_int = 0;
const PARAM_API_TYPE: c_int = 1;
const PARAM_SW_SIZE: c_int = 17;
const PARAM_SW_FORMAT: c_int = 18;
const PARAM_SW_STRIDE: c_int = 19;
const PARAM_SW_POINTER: c_int = 20;
const UPDATE_FRAME: u64 = 1;
const FORMAT_FLAG: c_int = 3;
const FORMAT_INT64: c_int = 4;
const FORMAT_DOUBLE: c_int = 5;
const EVENT_NONE: c_int = 0;
const EVENT_SHUTDOWN: c_int = 1;
const EVENT_END_FILE: c_int = 7;
const EVENT_FILE_LOADED: c_int = 8;
const EVENT_PROPERTY_CHANGE: c_int = 22;
const END_FILE_ERROR: c_int = 4;

type Callback = Option<unsafe extern "C" fn(*mut c_void)>;

/// The libmpv functions a video uses, looked up once.
struct Lib {
    create: unsafe extern "C" fn() -> *mut c_void,
    initialize: unsafe extern "C" fn(*mut c_void) -> c_int,
    terminate_destroy: unsafe extern "C" fn(*mut c_void),
    set_option_string: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    command: unsafe extern "C" fn(*mut c_void, *mut *const c_char) -> c_int,
    observe_property: unsafe extern "C" fn(*mut c_void, u64, *const c_char, c_int) -> c_int,
    wait_event: unsafe extern "C" fn(*mut c_void, f64) -> *mut MpvEvent,
    set_wakeup_callback: unsafe extern "C" fn(*mut c_void, Callback, *mut c_void),
    error_string: unsafe extern "C" fn(c_int) -> *const c_char,
    render_create: unsafe extern "C" fn(*mut *mut c_void, *mut c_void, *mut RenderParam) -> c_int,
    render_set_update_callback: unsafe extern "C" fn(*mut c_void, Callback, *mut c_void),
    render_update: unsafe extern "C" fn(*mut c_void) -> u64,
    render_render: unsafe extern "C" fn(*mut c_void, *mut RenderParam) -> c_int,
    render_free: unsafe extern "C" fn(*mut c_void),
}

/// The names tried, newest first. `libmpv.so.2` is mpv 0.35 and later; the
/// software renderer is older than that, so `.so.1` still has it.
const SONAMES: [&str; 3] = ["libmpv.so.2", "libmpv.so.1", "libmpv.so"];

/// How long an answer of "no libmpv" stands before the disk is asked again,
/// so installing mpv does not need a TD restart. The HTML engine's interval.
const ASK_AGAIN: Duration = Duration::from_secs(30);

/// Why no video can be played here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Missing {
    /// No libmpv under any of [`SONAMES`].
    NotFound,
    /// A libmpv without this function in it.
    TooOld(String),
}

impl Missing {
    /// Why, in a sentence, and nothing about what happened instead.
    pub fn reason(&self) -> String {
        match self {
            Missing::NotFound => "No libmpv found (install mpv to play video in TD).".into(),
            Missing::TooOld(name) => {
                format!("The libmpv found has no {name}; it is older than TD needs.")
            }
        }
    }

    /// One sentence for the person who clicked, ending in what happened
    /// instead: the HTML engine's sentence, for a video.
    pub fn sentence(&self) -> String {
        format!("{} Opened with the desktop.", self.reason())
    }

    /// The reason in a few words, for the chip on the row that was clicked.
    pub fn short_reason(&self) -> String {
        match self {
            Missing::NotFound => "no libmpv".into(),
            Missing::TooOld(_) => "libmpv too old".into(),
        }
    }
}

/// A library once found is kept for the life of the process; one not found
/// is asked about again after [`ASK_AGAIN`].
static LIB: Mutex<Option<Result<&'static Lib, (Missing, Instant)>>> = Mutex::new(None);

fn lib() -> Result<&'static Lib, Missing> {
    let mut slot = LIB.lock().unwrap_or_else(|e| e.into_inner());
    match &*slot {
        Some(Ok(lib)) => return Ok(lib),
        Some(Err((why, at))) if at.elapsed() < ASK_AGAIN => return Err(why.clone()),
        _ => {}
    }
    let answer = load().map(|lib| &*Box::leak(Box::new(lib)));
    *slot = Some(answer.clone().map_err(|why| (why, Instant::now())));
    answer
}

/// Whether a video can be played here: libmpv found and every function a
/// video needs present in it. Asked before a square is placed, so a machine
/// without mpv never opens one that could not fill. Nothing plays.
pub fn available() -> Result<(), Missing> {
    lib().map(|_| ())
}

fn load() -> Result<Lib, Missing> {
    let handle = SONAMES
        .iter()
        .find_map(|name| {
            let name = CString::new(*name).ok()?;
            // SAFETY: dlopen with a valid C string; the handle is never
            // closed, because the functions looked up in it are kept.
            let h = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
            (!h.is_null()).then_some(h)
        })
        .ok_or(Missing::NotFound)?;
    /// A function out of the library, as the type it is declared with.
    ///
    /// SAFETY (for every use below): `T` is the function's C signature as
    /// `client.h` and `render.h` declare it, and a data pointer and a function
    /// pointer are the same size on every platform TD builds for.
    unsafe fn sym<T: Copy>(h: *mut c_void, name: &CStr) -> Result<T, Missing> {
        let p = unsafe { libc::dlsym(h, name.as_ptr()) };
        if p.is_null() {
            return Err(Missing::TooOld(name.to_string_lossy().into_owned()));
        }
        Ok(unsafe { std::mem::transmute_copy::<*mut c_void, T>(&p) })
    }
    unsafe {
        Ok(Lib {
            create: sym(handle, c"mpv_create")?,
            initialize: sym(handle, c"mpv_initialize")?,
            terminate_destroy: sym(handle, c"mpv_terminate_destroy")?,
            set_option_string: sym(handle, c"mpv_set_option_string")?,
            command: sym(handle, c"mpv_command")?,
            observe_property: sym(handle, c"mpv_observe_property")?,
            wait_event: sym(handle, c"mpv_wait_event")?,
            set_wakeup_callback: sym(handle, c"mpv_set_wakeup_callback")?,
            error_string: sym(handle, c"mpv_error_string")?,
            render_create: sym(handle, c"mpv_render_context_create")?,
            render_set_update_callback: sym(handle, c"mpv_render_context_set_update_callback")?,
            render_update: sym(handle, c"mpv_render_context_update")?,
            render_render: sym(handle, c"mpv_render_context_render")?,
            render_free: sym(handle, c"mpv_render_context_free")?,
        })
    }
}

impl Lib {
    fn error(&self, code: c_int) -> String {
        // SAFETY: mpv_error_string returns a static string for any code.
        let s = unsafe { (self.error_string)(code) };
        if s.is_null() {
            return format!("error {code}");
        }
        unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned()
    }
}

// ── what the view sees ──────────────────────────────────────────────────────

/// One frame, drawn: `width × height` pixels in gpui's order, B G R A, with
/// every alpha opaque.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// Where playback is, as the player last said. Every field is `None` until
/// the player has said it, and a field the player cannot know (a stream's
/// duration) stays `None`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    /// Seconds from the start.
    pub position: Option<f64>,
    pub duration: Option<f64>,
    pub paused: Option<bool>,
    pub muted: Option<bool>,
    /// The picture's width and height as it is meant to be shown (mpv's
    /// `dwidth` and `dheight`), aspect already applied, reported one at a
    /// time. Read them together through [`Status::picture`].
    pub picture_w: Option<u32>,
    pub picture_h: Option<u32>,
    /// The file has been opened and its tracks read.
    pub loaded: bool,
    /// Why nothing will play, in a sentence.
    pub error: Option<String>,
}

impl Status {
    /// The picture's size once both sides are known. `None` while either is
    /// still to arrive, and for a file with no picture: half a size is not a
    /// size with a zero in it.
    pub fn picture(&self) -> Option<(u32, u32)> {
        Some((self.picture_w?, self.picture_h?))
    }
}

/// What the person can ask a playing video to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    TogglePause,
    /// Paused, whatever it was.
    Hold,
    ToggleMute,
    /// Seconds forward, or back when negative.
    SeekBy(f64),
    /// A fraction of the whole, `0.0..=1.0`.
    SeekTo(f64),
}

impl Command {
    /// The command as mpv's own words.
    fn words(self) -> Vec<String> {
        match self {
            Command::TogglePause => vec!["cycle".into(), "pause".into()],
            Command::Hold => vec!["set".into(), "pause".into(), "yes".into()],
            Command::ToggleMute => vec!["cycle".into(), "mute".into()],
            Command::SeekBy(s) => vec!["seek".into(), format!("{s:.3}"), "relative".into()],
            Command::SeekTo(f) => vec![
                "seek".into(),
                format!("{:.3}", f.clamp(0.0, 1.0) * 100.0),
                "absolute-percent+exact".into(),
            ],
        }
    }
}

/// What both threads leave for the view.
#[derive(Default)]
struct Shared {
    /// The newest frame drawn and not yet taken. A frame the view had no
    /// time to take is replaced, never queued: a late frame is a wrong one.
    frame: Option<Frame>,
    status: Status,
}

/// Called from either thread after it leaves something in [`Shared`], to
/// wake the view. Must not block.
pub type Poke = Arc<dyn Fn() + Send + Sync>;

enum ControlMsg {
    /// libmpv has events waiting.
    Wake,
    Run(Command),
    Stop,
}

enum RenderMsg {
    /// mpv's update callback fired.
    Update,
    /// Draw at this many device pixels from now on.
    Size(u32, u32),
    Stop,
}

/// The largest side a frame is drawn at. A frame is drawn at the size it is
/// shown, never above its own, so this only bounds a mistake.
const MAX_SIDE: u32 = 8192;

/// A video playing on its own two threads. Dropping it stops playback and
/// tears the player down on the control thread, without waiting here.
pub struct Player {
    control: Sender<ControlMsg>,
    render: Sender<RenderMsg>,
    shared: Arc<Mutex<Shared>>,
}

impl Player {
    /// Start playing `path`, looping, with sound. Answers at once: the file is
    /// opened on the control thread, and anything that goes wrong there is
    /// said in [`Status::error`], after a poke.
    pub fn start(path: &Path, poke: Poke) -> Result<Player, String> {
        let lib = lib().map_err(|why| why.reason())?;
        let path = CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|_| "The path has a NUL byte in it.".to_string())?;
        let (control_tx, control_rx) = mpsc::channel();
        let (render_tx, render_rx) = mpsc::channel();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let wake = control_tx.clone();
        let (render_for_control, shared_for_control) = (render_tx.clone(), shared.clone());
        std::thread::Builder::new()
            .name("td-video-control".into())
            .spawn(move || {
                control(
                    lib,
                    path,
                    Channels {
                        rx: control_rx,
                        wake,
                        render_tx: render_for_control,
                        render_rx,
                    },
                    shared_for_control,
                    poke,
                )
            })
            .map_err(|e| format!("Could not start a thread to play the video: {e}"))?;
        Ok(Player {
            control: control_tx,
            render: render_tx,
            shared,
        })
    }

    pub fn command(&self, cmd: Command) {
        let _ = self.control.send(ControlMsg::Run(cmd));
    }

    /// Draw frames at `width × height` device pixels from now on, and draw the
    /// current one again at that size, so a paused video follows a resize.
    pub fn size(&self, width: u32, height: u32) {
        let (w, h) = (width.clamp(1, MAX_SIDE), height.clamp(1, MAX_SIDE));
        let _ = self.render.send(RenderMsg::Size(w, h));
    }

    /// The newest frame, if one was drawn since the last take.
    pub fn take_frame(&self) -> Option<Frame> {
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .frame
            .take()
    }

    pub fn status(&self) -> Status {
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .status
            .clone()
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.control.send(ControlMsg::Stop);
    }
}

// ── the control thread ──────────────────────────────────────────────────────

struct Channels {
    rx: Receiver<ControlMsg>,
    /// A sender to `rx`, for libmpv's wakeup callback.
    wake: Sender<ControlMsg>,
    render_tx: Sender<RenderMsg>,
    render_rx: Receiver<RenderMsg>,
}

/// A raw pointer that is moved to one other thread and used only there.
struct SendPtr(*mut c_void);
// SAFETY: the render context is used by the render thread alone once it has
// been handed over; libmpv allows its render functions on any one thread.
unsafe impl Send for SendPtr {}

unsafe extern "C" fn on_wakeup(data: *mut c_void) {
    // SAFETY: `data` is the boxed sender `control` keeps alive until after
    // the player is destroyed, which is when libmpv stops calling this.
    let tx = unsafe { &*(data as *const Sender<ControlMsg>) };
    let _ = tx.send(ControlMsg::Wake);
}

unsafe extern "C" fn on_update(data: *mut c_void) {
    // SAFETY: as `on_wakeup`, until the render context is freed.
    let tx = unsafe { &*(data as *const Sender<RenderMsg>) };
    let _ = tx.send(RenderMsg::Update);
}

fn fail(shared: &Mutex<Shared>, poke: &Poke, why: String) {
    shared
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .status
        .error = Some(why);
    poke();
}

/// The properties watched, by the number each is reported under.
const WATCHED: [(u64, &CStr, c_int); 6] = [
    (1, c"time-pos", FORMAT_DOUBLE),
    (2, c"duration", FORMAT_DOUBLE),
    (3, c"pause", FORMAT_FLAG),
    (4, c"mute", FORMAT_FLAG),
    (5, c"dwidth", FORMAT_INT64),
    (6, c"dheight", FORMAT_INT64),
];

/// The options every video is played with. The person's own `mpv.conf` is
/// not read: it can name a video output that opens a window of its own,
/// which is the one thing a video in a square must never do.
const OPTIONS: [(&CStr, &CStr); 11] = [
    (c"config", c"no"),
    (c"vo", c"libmpv"),
    (c"terminal", c"no"),
    (c"input-default-bindings", c"no"),
    (c"input-vo-keyboard", c"no"),
    (c"osc", c"no"),
    (c"load-scripts", c"no"),
    (c"ytdl", c"no"),
    (c"hwdec", c"no"),
    // A clip loops, the way a GIF in the same square does.
    (c"loop-file", c"inf"),
    // A file with only sound draws nothing rather than its cover art.
    (c"audio-display", c"no"),
];

fn control(lib: &'static Lib, path: CString, ch: Channels, shared: Arc<Mutex<Shared>>, poke: Poke) {
    // SAFETY: every call below is on the handle this thread created and
    // destroys, with C strings that outlive the call.
    unsafe {
        let mpv = (lib.create)();
        if mpv.is_null() {
            return fail(&shared, &poke, "mpv could not start.".into());
        }
        for (name, value) in OPTIONS {
            (lib.set_option_string)(mpv, name.as_ptr(), value.as_ptr());
        }
        let r = (lib.initialize)(mpv);
        if r < 0 {
            (lib.terminate_destroy)(mpv);
            return fail(
                &shared,
                &poke,
                format!("mpv could not start: {}.", lib.error(r)),
            );
        }
        // Kept until the player is destroyed: libmpv holds the pointer.
        let wake: Box<Sender<ControlMsg>> = Box::new(ch.wake);
        (lib.set_wakeup_callback)(mpv, Some(on_wakeup), &*wake as *const _ as *mut c_void);

        let mut sw = *b"sw\0";
        let mut params = [
            RenderParam {
                kind: PARAM_API_TYPE,
                data: sw.as_mut_ptr().cast(),
            },
            RenderParam {
                kind: PARAM_END,
                data: std::ptr::null_mut(),
            },
        ];
        let mut rctx: *mut c_void = std::ptr::null_mut();
        let r = (lib.render_create)(&mut rctx, mpv, params.as_mut_ptr());
        if r < 0 || rctx.is_null() {
            (lib.terminate_destroy)(mpv);
            drop(wake);
            return fail(
                &shared,
                &poke,
                format!("mpv cannot draw into TD here: {}.", lib.error(r)),
            );
        }
        let update: Box<Sender<RenderMsg>> = Box::new(ch.render_tx.clone());
        (lib.render_set_update_callback)(
            rctx,
            Some(on_update),
            &*update as *const _ as *mut c_void,
        );
        let ctx = SendPtr(rctx);
        let (render_shared, render_poke) = (shared.clone(), poke.clone());
        let render_rx = ch.render_rx;
        let renderer = std::thread::Builder::new()
            .name("td-video-render".into())
            .spawn(move || {
                let ctx = ctx;
                render(lib, ctx.0, render_rx, &render_shared, &render_poke)
            });
        let renderer = match renderer {
            Ok(r) => r,
            Err(e) => {
                (lib.render_free)(rctx);
                (lib.terminate_destroy)(mpv);
                drop((wake, update));
                return fail(
                    &shared,
                    &poke,
                    format!("Could not start a thread to draw the video: {e}"),
                );
            }
        };

        for (id, name, format) in WATCHED {
            (lib.observe_property)(mpv, id, name.as_ptr(), format);
        }
        let mut load = [c"loadfile".as_ptr(), path.as_ptr(), std::ptr::null()];
        let r = (lib.command)(mpv, load.as_mut_ptr());
        if r < 0 {
            fail(
                &shared,
                &poke,
                format!("mpv would not open the file: {}.", lib.error(r)),
            );
        }

        'run: while let Ok(msg) = ch.rx.recv() {
            match msg {
                ControlMsg::Stop => break,
                ControlMsg::Run(cmd) => {
                    let words: Vec<CString> = cmd
                        .words()
                        .into_iter()
                        .filter_map(|w| CString::new(w).ok())
                        .collect();
                    let mut argv: Vec<*const c_char> = words.iter().map(|w| w.as_ptr()).collect();
                    argv.push(std::ptr::null());
                    (lib.command)(mpv, argv.as_mut_ptr());
                }
                ControlMsg::Wake => {}
            }
            let mut changed = false;
            loop {
                let ev = (lib.wait_event)(mpv, 0.0);
                if ev.is_null() {
                    break;
                }
                match (*ev).event_id {
                    EVENT_NONE => break,
                    EVENT_SHUTDOWN => break 'run,
                    EVENT_PROPERTY_CHANGE if !(*ev).data.is_null() => {
                        let p = &*((*ev).data as *const EventProperty);
                        let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                        changed |= observe(&mut s.status, (*ev).reply_userdata, p);
                    }
                    EVENT_FILE_LOADED => {
                        shared
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .status
                            .loaded = true;
                        changed = true;
                    }
                    EVENT_END_FILE if !(*ev).data.is_null() => {
                        let end = &*((*ev).data as *const EventEndFile);
                        if end.reason == END_FILE_ERROR {
                            shared
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .status
                                .error = Some(format!(
                                "mpv could not play this file: {}.",
                                lib.error(end.error)
                            ));
                            changed = true;
                        }
                    }
                    _ => {}
                }
            }
            if changed {
                poke();
            }
        }

        // The order libmpv requires: the render context goes before the
        // player, and it goes on the thread that renders with it.
        let _ = ch.render_tx.send(RenderMsg::Stop);
        let _ = renderer.join();
        (lib.terminate_destroy)(mpv);
        drop((wake, update));
    }
}

/// One side of the picture, as mpv reports it: a size no larger than zero is
/// no picture on that side, which is unknown rather than zero wide.
fn side(v: Option<i64>) -> Option<u32> {
    v.filter(|v| *v > 0)
        .map(|v| v.min(i64::from(u32::MAX)) as u32)
}

/// A watched property changed. Answers whether the status did.
///
/// # Safety
/// `p.data` must point at a value of `p.format`, as libmpv promises for the
/// event's lifetime.
unsafe fn observe(s: &mut Status, id: u64, p: &EventProperty) -> bool {
    let double = || {
        (p.format == FORMAT_DOUBLE && !p.data.is_null()).then(|| unsafe { *(p.data as *const f64) })
    };
    let flag = || {
        (p.format == FORMAT_FLAG && !p.data.is_null())
            .then(|| unsafe { *(p.data as *const c_int) } != 0)
    };
    let int = || {
        (p.format == FORMAT_INT64 && !p.data.is_null()).then(|| unsafe { *(p.data as *const i64) })
    };
    let before = s.clone();
    match id {
        1 => s.position = double(),
        2 => s.duration = double(),
        3 => s.paused = flag(),
        4 => s.muted = flag(),
        5 => s.picture_w = side(int()),
        6 => s.picture_h = side(int()),
        _ => {}
    }
    *s != before
}

// ── the render thread ───────────────────────────────────────────────────────

/// Sixty-four bytes aligned to sixty-four, the unit the frame buffer is made
/// of: libmpv asks for a pointer and a stride on that boundary, and copies
/// the whole frame once more when it does not get them.
#[repr(C, align(64))]
#[derive(Clone, Copy)]
struct Line([u8; 64]);

/// Bytes per row for `width` pixels, rounded up to the alignment.
pub fn stride_for(width: u32) -> usize {
    (width as usize * 4).next_multiple_of(64)
}

/// Rows of `stride` bytes, cut to `width` pixels each and made opaque: the
/// fourth byte of mpv's `bgr0` is whatever was there, and gpui reads it as
/// alpha.
pub fn pack(src: &[u8], width: u32, height: u32, stride: usize) -> Vec<u8> {
    let row = width as usize * 4;
    let mut out = Vec::with_capacity(row * height as usize);
    for y in 0..height as usize {
        out.extend_from_slice(&src[y * stride..y * stride + row]);
    }
    for px in out.chunks_exact_mut(4) {
        px[3] = 0xff;
    }
    out
}

fn render(
    lib: &'static Lib,
    rctx: *mut c_void,
    rx: Receiver<RenderMsg>,
    shared: &Mutex<Shared>,
    poke: &Poke,
) {
    let mut size: Option<(u32, u32)> = None;
    let mut buf: Vec<Line> = Vec::new();
    'run: while let Ok(first) = rx.recv() {
        let mut draw = false;
        for msg in std::iter::once(first).chain(rx.try_iter()) {
            match msg {
                RenderMsg::Stop => break 'run,
                RenderMsg::Update => {
                    // SAFETY: this thread is the only one calling the
                    // render functions, and never from a callback.
                    if unsafe { (lib.render_update)(rctx) } & UPDATE_FRAME != 0 {
                        draw = true;
                    }
                }
                RenderMsg::Size(w, h) => {
                    if size != Some((w, h)) {
                        size = Some((w, h));
                        draw = true;
                    }
                }
            }
        }
        // Nothing is drawn until the view has said how large: an unmeasured
        // view has no size, and a guess would be a frame at the wrong one.
        let Some((w, h)) = size.filter(|_| draw) else {
            continue;
        };
        let stride = stride_for(w);
        buf.resize(stride * h as usize / 64, Line([0; 64]));
        let mut dims = [w as c_int, h as c_int];
        let mut stride_v = stride;
        let format = c"bgr0";
        let mut params = [
            RenderParam {
                kind: PARAM_SW_SIZE,
                data: dims.as_mut_ptr().cast(),
            },
            RenderParam {
                kind: PARAM_SW_FORMAT,
                data: format.as_ptr() as *mut c_void,
            },
            RenderParam {
                kind: PARAM_SW_STRIDE,
                data: (&mut stride_v as *mut usize).cast(),
            },
            RenderParam {
                kind: PARAM_SW_POINTER,
                data: buf.as_mut_ptr().cast(),
            },
            RenderParam {
                kind: PARAM_END,
                data: std::ptr::null_mut(),
            },
        ];
        // SAFETY: `buf` holds `stride * h` bytes at a 64-byte boundary, and
        // every parameter outlives the call.
        let r = unsafe { (lib.render_render)(rctx, params.as_mut_ptr()) };
        if r < 0 {
            continue;
        }
        // SAFETY: `buf` is `stride * h` initialised bytes.
        let bytes =
            unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), stride * h as usize) };
        let frame = Frame {
            width: w,
            height: h,
            bgra: pack(bytes, w, h, stride),
        };
        shared.lock().unwrap_or_else(|e| e.into_inner()).frame = Some(frame);
        poke();
    }
    // SAFETY: freed on the thread that rendered with it, before the control
    // thread destroys the player, which is waiting on this thread to end.
    unsafe { (lib.render_free)(rctx) };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A row is padded out to 64 bytes, so a narrow frame still hands libmpv
    /// a stride on its boundary, and a wide one that is already a multiple
    /// is left alone.
    #[test]
    fn a_stride_is_rounded_up_to_sixty_four_bytes() {
        assert_eq!(stride_for(1), 64);
        assert_eq!(stride_for(16), 64);
        assert_eq!(stride_for(17), 128);
        assert_eq!(stride_for(1920), 7680);
    }

    /// Packing drops each row's padding and makes every pixel opaque,
    /// whatever mpv left in the fourth byte.
    #[test]
    fn packing_drops_the_padding_and_makes_every_pixel_opaque() {
        let (w, h) = (2u32, 2u32);
        let stride = stride_for(w);
        let mut src = vec![0xAAu8; stride * h as usize];
        // Row 0: two pixels, B G R X with X garbage.
        src[..8].copy_from_slice(&[1, 2, 3, 0, 4, 5, 6, 7]);
        src[stride..stride + 8].copy_from_slice(&[8, 9, 10, 0, 11, 12, 13, 99]);
        let out = pack(&src, w, h, stride);
        assert_eq!(
            out,
            vec![1, 2, 3, 255, 4, 5, 6, 255, 8, 9, 10, 255, 11, 12, 13, 255]
        );
    }

    /// Seeking to a fraction names a percentage, clamped, and seeks exactly:
    /// a press on the bar lands where it was pressed, not on the keyframe
    /// before it.
    #[test]
    fn a_seek_to_a_fraction_is_an_exact_percentage() {
        assert_eq!(
            Command::SeekTo(0.25).words(),
            vec!["seek", "25.000", "absolute-percent+exact"]
        );
        assert_eq!(Command::SeekTo(1.7).words()[1], "100.000");
        assert_eq!(Command::SeekTo(-1.0).words()[1], "0.000");
        assert_eq!(
            Command::SeekBy(-5.0).words(),
            vec!["seek", "-5.000", "relative"]
        );
    }

    fn prop(format: c_int, data: *mut c_void) -> EventProperty {
        EventProperty {
            name: std::ptr::null(),
            format,
            data,
        }
    }

    /// A property the player cannot know arrives with no value, and the
    /// status says unknown rather than zero; a known one is read as its type.
    #[test]
    fn a_property_with_no_value_is_unknown_not_zero() {
        let mut s = Status::default();
        let mut secs = 12.5f64;
        assert!(unsafe {
            observe(
                &mut s,
                1,
                &prop(FORMAT_DOUBLE, (&mut secs as *mut f64).cast()),
            )
        });
        assert_eq!(s.position, Some(12.5));
        assert!(unsafe { observe(&mut s, 1, &prop(0, std::ptr::null_mut())) });
        assert_eq!(s.position, None, "unavailable is None, never 0.0");
        let mut yes: c_int = 1;
        assert!(unsafe {
            observe(
                &mut s,
                3,
                &prop(FORMAT_FLAG, (&mut yes as *mut c_int).cast()),
            )
        });
        assert_eq!(s.paused, Some(true));
        assert!(
            !unsafe {
                observe(
                    &mut s,
                    3,
                    &prop(FORMAT_FLAG, (&mut yes as *mut c_int).cast()),
                )
            },
            "the same value again changes nothing"
        );
    }

    /// The picture's size is known once both sides are: one side alone is not
    /// a size with a zero in it. A side that goes away (a file with no
    /// picture) makes the whole size unknown again.
    #[test]
    fn the_picture_size_is_both_sides_or_none() {
        let mut s = Status::default();
        let (mut w, mut h) = (1280i64, 720i64);
        unsafe { observe(&mut s, 5, &prop(FORMAT_INT64, (&mut w as *mut i64).cast())) };
        assert_eq!(s.picture(), None, "width alone");
        unsafe { observe(&mut s, 6, &prop(FORMAT_INT64, (&mut h as *mut i64).cast())) };
        assert_eq!(s.picture(), Some((1280, 720)));
        unsafe { observe(&mut s, 5, &prop(0, std::ptr::null_mut())) };
        assert_eq!(s.picture(), None);
        let mut zero = 0i64;
        unsafe {
            observe(
                &mut s,
                5,
                &prop(FORMAT_INT64, (&mut zero as *mut i64).cast()),
            )
        };
        assert_eq!(s.picture_w, None, "zero wide is no picture, not a width");
    }
}
