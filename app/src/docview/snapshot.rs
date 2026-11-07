//! The first page engine: a snapshot of the page, drawn by headless Chromium.
//!
//! # One browser per TD process
//!
//! Started on the first HTML document and shared by every page after it, one
//! page per open document, each in a browser context of its own. The browser
//! is driven over the DevTools pipe ([`super::cdp`]), so no port is opened, and
//! it runs on an empty profile made for it and removed after it, so a page's
//! `localStorage` is always empty and a brief's own notes island is what it
//! shows. After five minutes without a call it is closed; the next document
//! starts it again (155–163 ms, measured).
//!
//! It is launched from a thread of its own, "td-page-engine", that lives as
//! long as the engine: the child asks for SIGKILL when the thread that forked
//! it exits, which is a thread and not the process, so a launch from a pool
//! thread could lose the browser whenever the pool retired that thread. The
//! same thread keeps the idle clock.
//!
//! # The network is off
//!
//! A brief runs its own script inside the engine. With `network` unset, every
//! page is told to block http, https, ws and wss, and the browser is pointed
//! at a proxy that does not exist and at a resolver that finds nothing. Both
//! are needed: the page-level block lets a WebSocket through (measured), and
//! the proxy is what stops it. A brief that loads a web font renders with the
//! fallback, unless `documents.toml` says `network = "allowed"`.
//!
//! # Fresh context per page
//!
//! A brief laid out 17–29 CSS px taller from its third load in one browser
//! context (measured in the snapshot spike; the cause was not established).
//! A context per page makes every load a first load, and it keeps one page's
//! storage from ever reaching another.
//!
//! No `crate::` paths, so `app/tests/snapshot_engine.rs` can compile this file
//! and drive a real Chromium with it.

use std::collections::{HashMap, VecDeque};
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Value};

use super::cdp::{self, Cdp, CdpError, SessionId};
use super::engine::{
    layout_hash, png_size, Anchor, Band, DialogRender, EngineError, Geometry, Link,
    NotesCapability, Opener, PageEngine, PageId, PageLayout, PageRequest, PageVideo, RectCss,
    Target, Tile, Unavailable,
};
use super::pref;

pub const EXTRACT_JS: &str = include_str!("extract.js");
/// Part of the cache key: a render made by an older probe is not reused.
/// 2: the rule an anchor with notes carries is left out of the picture.
/// 3: each anchor says whether the page shows a note and a stamp on it.
/// 4: concurs are supported only where a script makes concur zones.
/// 5: every id and `<a name>` says where it starts, for a fragment that
///    arrives from another document.
/// 6: every `<video>` says where it is and what file it plays, so a press on
///    it opens the file as a clip over the page.
/// 7: a film the page carries inside itself is reported as its `data:`
///    address instead of as no file, for TD to keep as one.
/// 8: every `<audio>` is reported beside the videos, saying it is a sound,
///    so a press on it opens the sound as a clip too.
/// 9: viewport-sized bands and a validated local-resource inventory.
pub const EXTRACT_VERSION: u32 = 9;
/// With no call for this long, the browser is closed.
pub const IDLE_SHUTDOWN: Duration = Duration::from_secs(300);
/// The names looked for on PATH, in order.
pub const BROWSER_NAMES: [&str; 4] = [
    "chromium",
    "chromium-browser",
    "google-chrome-stable",
    "google-chrome",
];

/// Collects the page's own errors from the first byte of its script on, for
/// the render's diagnostics.
const ERROR_HOOK: &str = "window.__tdErrors = []; \
    window.addEventListener('error', function (e) { window.__tdErrors.push(String(e.message)); }); \
    window.addEventListener('unhandledrejection', function (e) { window.__tdErrors.push(String(e.reason)); });";

/// What the probe hides from the picture: the page's own notes chrome, and
/// the rule it draws beside an anchor with notes (extract.js says why). The
/// probe hides them again with the same rule; a test holds the two together.
const NOTES_UI_HIDE: &str =
    ".notebar, .note-btn, .concur-zone { visibility: hidden !important; }\n\
    .notable.has-note, tr.notable.has-note > td:first-child { box-shadow: none !important; }";

/// Run before the page's own first byte, beside [`ERROR_HOOK`]: the notes
/// chrome is hidden from the first frame on, so the frame drawn at load is the
/// frame photographed and nothing is drawn twice. Hiding it only once the
/// probe ran repainted what the load had already drawn. A page that refuses
/// the sheet is still hidden by the probe; this is the fast way, not the only.
fn new_document_script() -> String {
    let css = serde_json::to_string(NOTES_UI_HIDE).unwrap_or_default();
    format!(
        "{ERROR_HOOK}\ntry {{ var s = new CSSStyleSheet(); s.replaceSync({css}); \
         document.adoptedStyleSheets = document.adoptedStyleSheets.concat([s]); }} catch (e) {{}}"
    )
}

fn viewport_params(g: Geometry, height_css: u32) -> Value {
    json!({
        "width": g.css_width,
        "height": height_css.max(1),
        "deviceScaleFactor": g.scale,
        "mobile": false,
    })
}

/// The screenshot of one band. One function, so the first band photographed
/// while the page is still being read and a band asked for later are the
/// same request.
///
/// The first screen is photographed as the page already shows it, with no
/// clip: a clip makes Chromium re-emulate the viewport for the capture and
/// put it back after, and each of those is a fresh layout, raster and draw
/// of every glass pane on the CPU (four full draws were traced for one
/// clipped first screen). The same pixels came back both ways on every brief
/// compared. `exact` asks for the clipped form anyway, for the rare window
/// whose view Chromium sizes a row differently from [`Geometry::height_dev`].
fn screenshot_params(g: Geometry, band: Band, exact: bool) -> Value {
    if !exact && is_first_screen(g, band) {
        return json!({
            "format": "png",
            "optimizeForSpeed": true,
            "captureBeyondViewport": false,
            "fromSurface": true,
        });
    }
    let (y, h) = g.band_css(band);
    json!({
        "format": "png",
        "optimizeForSpeed": true,
        "captureBeyondViewport": band.top_dev != 0
            || band.height_dev > g.height_dev(g.viewport_css_height as f32),
        "fromSurface": true,
        "clip": { "x": 0, "y": y, "width": g.css_width, "height": h, "scale": 1 },
    })
}

/// Whether `band` is exactly the screen the page is showing.
fn is_first_screen(g: Geometry, band: Band) -> bool {
    band.top_dev == 0 && band.height_dev == g.height_dev(g.viewport_css_height as f32)
}

const CALL: Duration = Duration::from_secs(10);
/// How long a browser just spawned has to give its first answer. That answer
/// measures the process starting, not a call, and a cold start is slower than
/// any call: on GitHub's runner the first two browsers started at once, and
/// the runner's Chromium took longer than `CALL` to answer either of them.
const START: Duration = Duration::from_secs(30);
const LOAD: Duration = Duration::from_secs(20);
const CAPTURE: Duration = Duration::from_secs(30);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

struct Browser {
    cdp: Arc<Cdp>,
    child: Child,
    profile: PathBuf,
    stderr: Arc<Mutex<VecDeque<u8>>>,
}

struct LivePage {
    /// The first band's screenshot, asked for while the page was being read,
    /// until a [`PageEngine::tile`] for that band collects it.
    first: Option<(Band, cdp::Pending)>,
    started: std::time::SystemTime,
    context: String,
    target: String,
    session: SessionId,
    generation: u64,
    geometry: Geometry,
    cdp: Arc<Cdp>,
}

/// A page in a context of its own, set up the way every page is, resting at
/// `about:blank` until an open takes it. Making one starts a renderer, which
/// was 60–240 ms of an open depending on how busy the box was; one waiting
/// costs that renderer, about 15 MiB, while the browser lives.
struct Blank {
    context: String,
    target: String,
    session: SessionId,
    cdp: Arc<Cdp>,
}

struct Shared {
    browser: Mutex<Option<Browser>>,
    pages: Mutex<HashMap<PageId, Arc<Mutex<LivePage>>>>,
    /// At most one ready page, made after a page closes for the next open.
    /// It goes with the browser: nothing keeps a browser alive to keep it.
    ready: Mutex<Option<Blank>>,
    making: AtomicBool,
    last_call: Mutex<Instant>,
    in_flight: AtomicUsize,
    idle: Duration,
    runtime: PathBuf,
}

type Spawned = std::io::Result<(Child, Arc<Cdp>)>;

enum Job {
    Launch {
        binary: PathBuf,
        args: Vec<OsString>,
        reply: mpsc::Sender<Spawned>,
    },
}

pub struct SnapshotEngine {
    pref: pref::Html,
    binary: PathBuf,
    shared: Arc<Shared>,
    next_page: AtomicU64,
    next_generation: AtomicU64,
    jobs: Mutex<Option<mpsc::Sender<Job>>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

/// Where fresh profiles go: `$XDG_RUNTIME_DIR/terminal-delight`, which is
/// per-user, in memory and cleared at logout; else the temp dir.
pub fn runtime_dir() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
        Some(d) => PathBuf::from(d).join("terminal-delight"),
        None => {
            std::env::temp_dir().join(format!("terminal-delight-{}", unsafe { libc::getuid() }))
        }
    }
}

fn executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only asks whether the process exists.
    pid > 0
        && (unsafe { libc::kill(pid, 0) } == 0
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
}

/// `file://` for a path, percent-encoding everything a URL would misread.
pub fn file_url(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut out = String::from("file://");
    for &b in path.as_os_str().as_bytes() {
        let keep = b.is_ascii_alphanumeric() || b"/-._~!$&'()*+,;=:@".contains(&b);
        if keep {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn local_resource(url: &str) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let rest = url.strip_prefix("file://")?;
    if !rest.starts_with('/') {
        return None;
    }
    let bytes = rest.split(['?', '#']).next()?.as_bytes();
    let mut decoded = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    Some(PathBuf::from(OsString::from_vec(decoded)))
}

/// What `__terminalDelight.extract()` returns.
#[derive(Deserialize)]
struct Extracted {
    height_css: f32,
    notes_file: Option<String>,
    capability: NotesCapability,
    anchors: Vec<Anchor>,
    links: Vec<Link>,
    targets: Vec<Target>,
    openers: Vec<Opener>,
    dialogs: Vec<String>,
    diagnostics: Vec<String>,
    videos: Vec<PageVideo>,
}

#[derive(Deserialize)]
struct DialogOpened {
    ok: bool,
    overflow: f64,
}

#[derive(Deserialize)]
struct DialogMeasured {
    rect: RectCss,
    anchors: Vec<Anchor>,
    links: Vec<Link>,
    openers: Vec<Opener>,
    closers: Vec<Option<RectCss>>,
}

fn protocol(e: CdpError, what: &'static str) -> EngineError {
    match e {
        CdpError::Timeout => EngineError::Timeout(what),
        CdpError::Closed => EngineError::Closed,
        other => EngineError::Protocol(format!("{what}: {other}")),
    }
}

fn str_field(v: &Value, key: &str) -> Result<String, EngineError> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| EngineError::Protocol(format!("no {key} in {v}")))
}

impl SnapshotEngine {
    pub fn new(pref: pref::Html, binary: PathBuf) -> Self {
        Self::with_idle(pref, binary, IDLE_SHUTDOWN, runtime_dir())
    }

    /// The same engine with its idle time and profile directory chosen: the
    /// tests shut a browser down in seconds rather than minutes, in a
    /// directory they own.
    pub fn with_idle(pref: pref::Html, binary: PathBuf, idle: Duration, runtime: PathBuf) -> Self {
        let shared = Arc::new(Shared {
            browser: Mutex::new(None),
            pages: Mutex::new(HashMap::new()),
            ready: Mutex::new(None),
            making: AtomicBool::new(false),
            last_call: Mutex::new(Instant::now()),
            in_flight: AtomicUsize::new(0),
            idle,
            runtime,
        });
        let (tx, rx) = mpsc::channel();
        let weak = Arc::downgrade(&shared);
        let thread = std::thread::Builder::new()
            .name("td-page-engine".into())
            .spawn(move || engine_thread(rx, weak))
            .ok();
        Self {
            pref,
            binary,
            shared,
            next_page: AtomicU64::new(1),
            next_generation: AtomicU64::new(1),
            jobs: Mutex::new(Some(tx)),
            thread: Mutex::new(thread),
        }
    }

    /// The configured `chromium` if one is set — and only it, since a path
    /// someone wrote down is a decision, not a hint — else the first of
    /// [`BROWSER_NAMES`] found executable on `path_var`.
    pub fn locate(pref: &pref::Html, path_var: Option<&OsStr>) -> Result<PathBuf, Unavailable> {
        if let Some(p) = &pref.chromium {
            return if executable(p) {
                Ok(p.clone())
            } else {
                Err(Unavailable::NoBrowser {
                    searched: vec![p.clone()],
                    configured: true,
                })
            };
        }
        let dirs: Vec<PathBuf> = path_var
            .map(|p| std::env::split_paths(p).collect())
            .unwrap_or_default();
        for name in BROWSER_NAMES {
            for dir in &dirs {
                let c = dir.join(name);
                if executable(&c) {
                    return Ok(c);
                }
            }
        }
        Err(Unavailable::NoBrowser {
            searched: BROWSER_NAMES.iter().map(PathBuf::from).collect(),
            configured: false,
        })
    }

    pub fn launch_args(profile: &Path, pref: &pref::Html) -> Vec<OsString> {
        let mut a: Vec<OsString> = [
            "--headless",
            "--remote-debugging-pipe",
            "--no-startup-window",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-extensions",
            "--disable-sync",
            "--disable-background-networking",
            "--disable-component-update",
            "--disable-default-apps",
            "--disable-breakpad",
            // Never ask the desktop's keyring for anything: on Linux a
            // browser can raise an unlock prompt on the person's screen.
            "--password-store=basic",
            "--use-mock-keychain",
            "--mute-audio",
            "--hide-scrollbars",
            "--force-color-profile=srgb",
            // What headless Chromium runs that no page here ever uses, and
            // what it costs (Chromium 152, measured 2026-11-07, interleaved
            // runs on a loaded box). Each new browser context loaded the
            // address bar's own dropdown, chrome://omnibox-popup.top-chrome,
            // into a renderer of its own: about 190 ms of CPU and 80 MiB per
            // open, for a window that never exists. Turning it off took 70 ms
            // off the median open. And Chromium keeps a spare renderer
            // warm for the next navigation, which a page in a new context of
            // its own can never use: one process and about 10 MiB, for
            // nothing. TD keeps its own ready page instead (`make_ready`).
            "--disable-features=PreloadTopChromeWebUI,WebUIOmniboxAimPopup,WebUIOmniboxPopup,WebUIOmniboxFullPopup,SpareRendererForSitePerProcess",
            // The GPU process (37 MiB idle), a component extension's renderer
            // (30 MiB) and the network service (25 MiB) each ran as a process
            // of their own. Software compositing does the same work in the
            // browser process; a page here reaches no network; no brief uses
            // an extension. Idle 241 -> 190 MiB, a page open 329 -> 289 MiB,
            // with no change in how long an open takes. The GPU and network
            // halves are below, beside the switches they share.
            "--disable-component-extensions-with-background-pages",
        ]
        .iter()
        .map(OsString::from)
        .collect();
        let mut dir = OsString::from("--user-data-dir=");
        dir.push(profile);
        a.push(dir);
        if !pref.network_allowed() {
            for s in [
                // Under Network.setBlockedURLs, and not optional: a
                // WebSocket gets past that call (measured), so every request
                // is also sent to a proxy that is not there, loopback
                // included, and no name resolves.
                "--proxy-server=http://127.0.0.1:9",
                "--proxy-bypass-list=<-loopback>",
                "--host-resolver-rules=MAP * ~NOTFOUND",
                "--force-webrtc-ip-handling-policy=disable_non_proxied_udp",
            ] {
                a.push(s.into());
            }
        }
        // One switch: Chromium keeps only the last `--enable-features` it is
        // given, so a second one would silently drop the first.
        let mut features = vec!["NetworkServiceInProcess2"];
        if pref.vulkan() {
            // A real GPU keeps its process: the Vulkan path is the one
            // measured that way, and a driver fault stays out of the browser.
            features.extend(["Vulkan", "VulkanFromANGLE", "DefaultANGLEVulkan"]);
            for s in [
                "--use-angle=vulkan",
                "--ignore-gpu-blocklist",
                "--enable-gpu",
            ] {
                a.push(s.into());
            }
        } else {
            a.push("--in-process-gpu".into());
        }
        a.push(format!("--enable-features={}", features.join(",")).into());
        a
    }

    /// The running browser's process id, if one is running. The engine
    /// tests check by this pid that nothing they started outlives them.
    #[allow(dead_code)] // read by app/tests/snapshot_engine.rs
    pub fn browser_pid(&self) -> Option<u32> {
        lock(&self.shared.browser).as_ref().map(|b| b.child.id())
    }

    /// Start the browser now, if none runs, and say whether it started.
    #[allow(dead_code)] // called by app/tests/snapshot_engine.rs
    pub fn warm(&self) -> Result<(), EngineError> {
        self.call(|| self.cdp().map(|_| ()))
    }

    /// Whether a page is set up and waiting for the next open.
    #[allow(dead_code)] // read by app/tests/snapshot_engine.rs
    pub fn page_ready(&self) -> bool {
        lock(&self.shared.ready).is_some()
    }

    /// The running browser's profile directory, if one is running.
    #[allow(dead_code)] // read by app/tests/snapshot_engine.rs
    pub fn profile_dir(&self) -> Option<PathBuf> {
        lock(&self.shared.browser)
            .as_ref()
            .map(|b| b.profile.clone())
    }

    /// Run `f` as one engine call: counted as in flight, so the idle clock
    /// never closes the browser under it, and restarting the clock after.
    fn call<T>(&self, f: impl FnOnce() -> Result<T, EngineError>) -> Result<T, EngineError> {
        self.shared.in_flight.fetch_add(1, Ordering::SeqCst);
        *lock(&self.shared.last_call) = Instant::now();
        let out = f();
        *lock(&self.shared.last_call) = Instant::now();
        self.shared.in_flight.fetch_sub(1, Ordering::SeqCst);
        out
    }

    /// The running browser's client, launching one if none runs.
    fn cdp(&self) -> Result<Arc<Cdp>, EngineError> {
        let mut slot = lock(&self.shared.browser);
        if let Some(b) = slot.as_mut() {
            let exited = !matches!(b.child.try_wait(), Ok(None));
            if !exited && !b.cdp.is_closed() {
                return Ok(b.cdp.clone());
            }
        }
        if let Some(dead) = slot.take() {
            lock(&self.shared.pages).clear();
            lock(&self.shared.ready).take();
            finish(dead);
        }
        let _ = std::fs::create_dir_all(&self.shared.runtime);
        remove_stale_profiles(&self.shared.runtime);
        let profile = fresh_profile(&self.shared.runtime)
            .map_err(|e| EngineError::Launch(format!("no profile directory: {e}")))?;
        let args = Self::launch_args(&profile, &self.pref);
        let (tx, rx) = mpsc::channel();
        let sent = lock(&self.jobs).as_ref().map(|jobs| {
            jobs.send(Job::Launch {
                binary: self.binary.clone(),
                args,
                reply: tx,
            })
        });
        if !matches!(sent, Some(Ok(()))) {
            let _ = std::fs::remove_dir_all(&profile);
            return Err(EngineError::Launch("the engine thread is gone".into()));
        }
        let (mut child, cdp) = match rx.recv_timeout(CALL) {
            Ok(Ok(spawned)) => spawned,
            Ok(Err(e)) => {
                let _ = std::fs::remove_dir_all(&profile);
                return Err(EngineError::Launch(format!(
                    "{}: {e}",
                    self.binary.display()
                )));
            }
            Err(_) => {
                let _ = std::fs::remove_dir_all(&profile);
                return Err(EngineError::Launch(
                    "the engine thread did not answer".into(),
                ));
            }
        };
        let stderr = Arc::new(Mutex::new(VecDeque::new()));
        let mut drain = None;
        if let Some(mut err) = child.stderr.take() {
            let tail = stderr.clone();
            drain = std::thread::Builder::new()
                .name("td-chromium-stderr".into())
                .spawn(move || {
                    let mut buf = [0u8; 4096];
                    while let Ok(n) = err.read(&mut buf) {
                        if n == 0 {
                            break;
                        }
                        let mut t = lock(&tail);
                        t.extend(&buf[..n]);
                        // Enough for a crash's reason and the trace after it.
                        while t.len() > 32 << 10 {
                            t.pop_front();
                        }
                    }
                })
                .ok();
        }
        let browser = Browser {
            cdp: cdp.clone(),
            child,
            profile,
            stderr,
        };
        if let Err(e) = cdp.call(None, "Browser.getVersion", json!({}), START) {
            // Read the reason only once the browser is gone and its standard
            // error has been drained to the end: a browser that hangs rather
            // than exits says why only as it is killed.
            let said = browser.stderr.clone();
            finish(browser);
            if let Some(drain) = drain {
                let until = Instant::now() + Duration::from_secs(2);
                while !drain.is_finished() && Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
            let bytes: Vec<u8> = lock(&said).iter().copied().collect();
            let tail = why_it_died(&String::from_utf8_lossy(&bytes));
            return Err(EngineError::Launch(launch_failure(
                matches!(e, CdpError::Timeout),
                &tail,
                &e.to_string(),
            )));
        }
        if std::env::var_os("TD_DOCDEBUG").is_some() {
            eprintln!(
                "[page] chromium pid {} profile {}",
                browser.child.id(),
                browser.profile.display()
            );
        }
        *slot = Some(browser);
        Ok(cdp)
    }

    fn live(&self, page: PageId) -> Result<Arc<Mutex<LivePage>>, EngineError> {
        lock(&self.shared.pages)
            .get(&page)
            .cloned()
            .ok_or(EngineError::Closed)
    }

    fn evaluate(
        cdp: &Cdp,
        session: &SessionId,
        expression: &str,
        what: &'static str,
    ) -> Result<Value, EngineError> {
        let r = cdp
            .call(
                Some(session),
                "Runtime.evaluate",
                json!({ "expression": expression, "returnByValue": true, "awaitPromise": true }),
                CALL,
            )
            .map_err(|e| protocol(e, what))?;
        if let Some(ex) = r.get("exceptionDetails") {
            let text = ex
                .pointer("/exception/description")
                .and_then(Value::as_str)
                .or_else(|| ex.get("text").and_then(Value::as_str))
                .unwrap_or("an exception");
            return Err(EngineError::Page(format!("{what}: {text}")));
        }
        Ok(r.pointer("/result/value").cloned().unwrap_or(Value::Null))
    }

    fn set_viewport(
        cdp: &Cdp,
        session: &SessionId,
        g: Geometry,
        height_css: u32,
    ) -> Result<(), EngineError> {
        cdp.call(
            Some(session),
            "Emulation.setDeviceMetricsOverride",
            viewport_params(g, height_css),
            CALL,
        )
        .map(|_| ())
        .map_err(|e| protocol(e, "size the page"))
    }

    /// A page in a context of its own, navigated to `path`, with the probe
    /// installed: the ready one if one is waiting, else one made now. Closed
    /// again if any step fails.
    fn load(&self, cdp: &Arc<Cdp>, path: &Path, g: Geometry) -> Result<LivePage, EngineError> {
        let ready = lock(&self.shared.ready)
            .take()
            .filter(|b| Arc::ptr_eq(&b.cdp, cdp));
        let blank = match ready {
            Some(b) => b,
            None => make_blank(cdp, &self.pref)?,
        };
        let page = LivePage {
            first: None,
            started: std::time::SystemTime::now(),
            context: blank.context,
            target: blank.target,
            session: blank.session,
            generation: 0,
            geometry: g,
            cdp: cdp.clone(),
        };
        match Self::navigate(cdp, &page, path) {
            Ok(()) => Ok(page),
            Err(e) => {
                close_page(&page);
                Err(e)
            }
        }
    }

    /// Size a set-up page, load `path` into it and install the probe. The
    /// caller closes the page on an error.
    fn navigate(cdp: &Arc<Cdp>, page: &LivePage, path: &Path) -> Result<(), EngineError> {
        let s = &page.session;
        Self::set_viewport(cdp, s, page.geometry, page.geometry.viewport_css_height)?;
        let loaded = cdp.expect(Some(s), "Page.loadEventFired");
        let nav = cdp
            .call(
                Some(s),
                "Page.navigate",
                json!({ "url": file_url(path) }),
                CALL,
            )
            .map_err(|e| protocol(e, "load the file"))?;
        if let Some(err) = nav.get("errorText").and_then(Value::as_str) {
            return Err(EngineError::Page(format!("{}: {err}", path.display())));
        }
        loaded
            .wait(LOAD)
            .map_err(|e| protocol(e, "load the page"))?;
        // Height at load and after fonts.ready was identical in all 119
        // files measured, but a font that lands late would move every anchor.
        // The probe goes in once they are ready, in the same round trip; a
        // font set that never settles is waited out no longer than before.
        Self::evaluate(
            cdp,
            s,
            &format!("document.fonts.ready.catch(() => true).then(() => {{\n{EXTRACT_JS}\n}})"),
            "install the probe",
        )?;
        Ok(())
    }

    /// Make the next open's page now, off the caller's thread, unless one is
    /// ready or being made. Not counted as a call: a ready page never keeps
    /// an idle browser running.
    fn make_ready(&self) {
        let shared = self.shared.clone();
        if lock(&shared.ready).is_some() || shared.making.swap(true, Ordering::SeqCst) {
            return;
        }
        let pref = self.pref.clone();
        let spawned = std::thread::Builder::new()
            .name("td-page-ready".into())
            .spawn(move || {
                let cdp = lock(&shared.browser)
                    .as_ref()
                    .filter(|b| !b.cdp.is_closed())
                    .map(|b| b.cdp.clone());
                if let Some(cdp) = cdp {
                    // Held in flight while it is made, so the idle clock
                    // cannot close the browser under it.
                    shared.in_flight.fetch_add(1, Ordering::SeqCst);
                    if let Ok(blank) = make_blank(&cdp, &pref) {
                        // Kept only for the browser that is still running,
                        // and while still counted in flight: an idle close
                        // or a relaunch that came first has already let its
                        // ready page go, and must not find one put back.
                        let current = lock(&shared.browser)
                            .as_ref()
                            .is_some_and(|b| Arc::ptr_eq(&b.cdp, &cdp));
                        let mut slot = lock(&shared.ready);
                        if slot.is_none() && current && !cdp.is_closed() {
                            *slot = Some(blank);
                        } else {
                            drop(slot);
                            close_blank(&blank);
                        }
                    }
                    shared.in_flight.fetch_sub(1, Ordering::SeqCst);
                }
                shared.making.store(false, Ordering::SeqCst);
            });
        if spawned.is_err() {
            self.shared.making.store(false, Ordering::SeqCst);
        }
    }
}

/// A new page in a new context, attached and set up the way every page is
/// set up, resting at `about:blank`. Closed again if any step fails.
fn make_blank(cdp: &Arc<Cdp>, pref: &pref::Html) -> Result<Blank, EngineError> {
    let ctx = cdp
        .call(None, "Target.createBrowserContext", json!({}), CALL)
        .map_err(|e| protocol(e, "make a browser context"))?;
    let context = str_field(&ctx, "browserContextId")?;
    let made = cdp.call(
        None,
        "Target.createTarget",
        json!({ "url": "about:blank", "browserContextId": context }),
        CALL,
    );
    let target = match made
        .map_err(|e| protocol(e, "open a page"))
        .and_then(|t| str_field(&t, "targetId"))
    {
        Ok(t) => t,
        Err(e) => {
            let _ = cdp.call(
                None,
                "Target.disposeBrowserContext",
                json!({ "browserContextId": context }),
                CALL,
            );
            return Err(e);
        }
    };
    let mut blank = Blank {
        context,
        target,
        session: SessionId(String::new()),
        cdp: cdp.clone(),
    };
    match set_up(cdp, pref, &mut blank) {
        Ok(()) => Ok(blank),
        Err(e) => {
            close_blank(&blank);
            Err(e)
        }
    }
}

/// Attach to a new page and set it up the way every page is set up. Its size
/// is set when it is given a file. The caller closes the page on an error.
fn set_up(cdp: &Arc<Cdp>, pref: &pref::Html, blank: &mut Blank) -> Result<(), EngineError> {
    let attached = cdp
        .call(
            None,
            "Target.attachToTarget",
            json!({ "targetId": blank.target, "flatten": true }),
            CALL,
        )
        .map_err(|e| protocol(e, "attach to the page"))?;
    blank.session = SessionId(str_field(&attached, "sessionId")?);
    let s = blank.session.clone();
    cdp.watch_resources(&s);
    // Sent together and answered together: none depends on another's answer,
    // and one at a time they cost a round trip each to a browser that may be
    // busy.
    let mut batch = vec![
        ("enable the page", "Page.enable", json!({})),
        ("enable resource observation", "Network.enable", json!({})),
    ];
    if !pref.network_allowed() {
        // Measured on Chromium 151: `urls` stops every http and https request
        // a page makes (images, fetch, beacons, stylesheets, localhost), the
        // newer `urlPatterns` with these patterns stops none of them, and
        // neither stops a WebSocket. The proxy flags in `launch_args` are
        // what stop that.
        batch.push((
            "block the network",
            "Network.setBlockedURLs",
            json!({ "urls": ["http://*", "https://*", "ws://*", "wss://*"] }),
        ));
    }
    batch.extend([
        // Something animates on at least one brief, and a capture should not
        // catch it mid-flight.
        (
            "ask for reduced motion",
            "Emulation.setEmulatedMedia",
            json!({ "features": [{ "name": "prefers-reduced-motion", "value": "reduce" }] }),
        ),
        (
            "hook the page's errors",
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": new_document_script() }),
        ),
    ]);
    let mut sent = Vec::with_capacity(batch.len());
    for (what, method, params) in batch {
        sent.push((
            what,
            cdp.send_call(Some(&s), method, params)
                .map_err(|e| protocol(e, what))?,
        ));
    }
    for (what, reply) in sent {
        reply.wait(CALL).map_err(|e| protocol(e, what))?;
    }
    Ok(())
}

fn close_blank(b: &Blank) {
    b.cdp.forget_resources(&b.session);
    let short = Duration::from_secs(2);
    let _ = b.cdp.call(
        None,
        "Target.closeTarget",
        json!({ "targetId": b.target }),
        short,
    );
    let _ = b.cdp.call(
        None,
        "Target.disposeBrowserContext",
        json!({ "browserContextId": b.context }),
        short,
    );
}

/// The line of a browser's standard error that says why it would not start.
///
/// A crash ends in a stack trace, so the last line is `[end of stack trace]`
/// and says nothing. The last FATAL or ERROR line, or the sandbox's own
/// complaint, is the reason; failing those, the last line that is not part
/// of a trace.
pub fn why_it_died(stderr: &str) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    // A stack frame, a register dump, or the trace's own bookends.
    let in_trace = |l: &str| {
        l.starts_with('#')
            || l.starts_with("Received signal")
            || l.contains("end of stack trace")
            || (l.starts_with('r') && l.contains(": 0"))
    };
    // In order of how much they explain: the fatal line, the sandbox's own
    // refusal, then any error but the crash reporter's own complaints, which
    // follow every crash and explain none.
    let last = |pick: &dyn Fn(&str) -> bool| lines.iter().rev().copied().find(|l| pick(l));
    let line = last(&|l| l.contains("FATAL"))
        .or_else(|| last(&|l| l.contains("No usable sandbox")))
        .or_else(|| last(&|l| l.contains("ERROR") && !l.contains("crashpad")))
        .or_else(|| last(&|l| !in_trace(l)))
        .unwrap_or("");
    line.chars().take(400).collect()
}

/// The sentence for a browser that was started and never answered.
///
/// A browser that crashed says why in its standard error, and that line is the
/// reason. One that hung may have printed nothing but noise: every headless
/// Chromium warns that it cannot reach D-Bus, and a CI run once reported that
/// warning as the reason a browser "would not start" when it had simply not
/// answered in time. So a hang is named as a hang, and what the browser last
/// said follows it rather than standing in for it.
fn launch_failure(timed_out: bool, tail: &str, otherwise: &str) -> String {
    let waited = format!("it did not answer within {} s", START.as_secs());
    match (timed_out, tail.is_empty()) {
        (true, true) => waited,
        (true, false) => format!("{waited}; the last thing it said was: {tail}"),
        (false, true) => otherwise.to_string(),
        (false, false) => tail.to_string(),
    }
}

fn close_page(p: &LivePage) {
    p.cdp.forget_resources(&p.session);
    let short = Duration::from_secs(2);
    let _ = p.cdp.call(
        None,
        "Target.closeTarget",
        json!({ "targetId": p.target }),
        short,
    );
    let _ = p.cdp.call(
        None,
        "Target.disposeBrowserContext",
        json!({ "browserContextId": p.context }),
        short,
    );
}

/// Every process under `root` right now, from /proc: a browser's renderers,
/// GPU process and zygotes, which outlive it by a moment.
fn descendants(root: u32) -> Vec<u32> {
    let mut procs = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for e in entries.flatten() {
            let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
                continue;
            };
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                continue;
            };
            let ppid = stat
                .rfind(')')
                .and_then(|i| stat.get(i + 2..))
                .and_then(|rest| rest.split(' ').nth(1))
                .and_then(|p| p.parse::<u32>().ok());
            if let Some(ppid) = ppid {
                procs.push((pid, ppid));
            }
        }
    }
    let mut out = vec![root];
    let mut grew = true;
    while grew {
        grew = false;
        for (pid, ppid) in &procs {
            if !out.contains(pid) && out.contains(ppid) {
                out.push(*pid);
                grew = true;
            }
        }
    }
    out.retain(|p| *p != root);
    out
}

/// Close a browser and everything it leaves behind: ask it to close, give it
/// three seconds, kill it if it has not gone, reap it, wait for its helper
/// processes, and remove its profile. The helpers can still be writing into
/// the profile for a moment after the browser exits, so the removal waits for
/// them and tries again until the directory is gone.
fn finish(mut b: Browser) {
    let helpers = descendants(b.child.id());
    let _ = b
        .cdp
        .call(None, "Browser.close", json!({}), Duration::from_secs(2));
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match b.child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            _ => {
                let _ = b.child.kill();
                let _ = b.child.wait();
                break;
            }
        }
    }
    let settle = Instant::now() + Duration::from_secs(3);
    while helpers.iter().any(|p| alive(*p as i32)) && Instant::now() < settle {
        std::thread::sleep(Duration::from_millis(25));
    }
    for _ in 0..20 {
        let _ = std::fs::remove_dir_all(&b.profile);
        if !b.profile.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if std::env::var_os("TD_DOCDEBUG").is_some() {
        eprintln!("[page] chromium pid {} closed", b.child.id());
    }
}

const PROFILE_PREFIX: &str = "chromium-";

/// `chromium-<td pid>-<n>`: the pid says whose it is, so a later TD can tell
/// a live one from one a crash left behind.
fn fresh_profile(runtime: &Path) -> std::io::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    static N: AtomicU64 = AtomicU64::new(0);
    let dir = runtime.join(format!(
        "{PROFILE_PREFIX}{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
    Ok(dir)
}

/// Remove every profile whose TD is gone. A TD killed outright takes its
/// browser with it, but the directory stays until somebody clears it.
pub fn remove_stale_profiles(runtime: &Path) {
    let Ok(entries) = std::fs::read_dir(runtime) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        let Some(rest) = name.to_str().and_then(|n| n.strip_prefix(PROFILE_PREFIX)) else {
            continue;
        };
        let Some(pid) = rest.split('-').next().and_then(|p| p.parse::<i32>().ok()) else {
            continue;
        };
        if !alive(pid) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

fn engine_thread(rx: mpsc::Receiver<Job>, shared: Weak<Shared>) {
    loop {
        let wait = match shared.upgrade() {
            Some(s) => {
                let since = lock(&s.last_call).elapsed();
                s.idle.saturating_sub(since).max(Duration::from_millis(50))
            }
            None => return,
        };
        match rx.recv_timeout(wait) {
            Ok(Job::Launch {
                binary,
                args,
                reply,
            }) => {
                let _ = reply.send(Cdp::spawn(&binary, &args));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let Some(s) = shared.upgrade() else { return };
                let idle = lock(&s.last_call).elapsed() >= s.idle;
                if idle && s.in_flight.load(Ordering::SeqCst) == 0 {
                    let dead = lock(&s.browser).take();
                    if let Some(b) = dead {
                        lock(&s.pages).clear();
                        lock(&s.ready).take();
                        finish(b);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

impl SnapshotEngine {
    /// A band, photographed clipped to exactly its rows however the view
    /// stands: the form every band but the first screen takes. The engine
    /// tests compare the first screen with it.
    #[allow(dead_code)] // called by app/tests/snapshot_engine.rs
    pub fn tile_clipped(
        &self,
        page: PageId,
        generation: u64,
        band: Band,
    ) -> Result<Tile, EngineError> {
        self.capture_band(page, generation, band, true)
    }

    fn capture_band(
        &self,
        page: PageId,
        generation: u64,
        band: Band,
        exact: bool,
    ) -> Result<Tile, EngineError> {
        self.call(|| {
            let live = self.live(page)?;
            let mut p = lock(&live);
            if p.generation != generation {
                return Err(EngineError::Stale);
            }
            let early = match p.first.take() {
                Some((b, shot)) if b == band && !exact => Some(shot),
                // Asked for another band first (a link into the middle of
                // the page): the early screen is not wanted.
                _ => None,
            };
            let capture = |exact: bool| {
                p.cdp.call(
                    Some(&p.session),
                    "Page.captureScreenshot",
                    screenshot_params(p.geometry, band, exact),
                    CAPTURE,
                )
            };
            let decode = |shot: Value| -> Result<(Vec<u8>, (u32, u32)), EngineError> {
                let data = shot.get("data").and_then(Value::as_str).unwrap_or("");
                let png = cdp::base64_decode(data).map_err(EngineError::Protocol)?;
                let size = png_size(&png)
                    .ok_or_else(|| EngineError::Protocol("a tile that is not a PNG".into()))?;
                Ok((png, size))
            };
            let shot = match early {
                Some(shot) => shot.wait(CAPTURE),
                None => capture(exact),
            }
            .map_err(|e| protocol(e, "capture a tile"))?;
            let (mut png, (mut width_dev, mut height_dev)) = decode(shot)?;
            if !exact && height_dev != band.height_dev && is_first_screen(p.geometry, band) {
                // The view came back a row off the band: ask for the band.
                let shot = capture(true).map_err(|e| protocol(e, "capture a tile"))?;
                (png, (width_dev, height_dev)) = decode(shot)?;
            }
            Ok(Tile {
                band,
                png,
                width_dev,
                height_dev,
            })
        })
    }
}

impl PageEngine for SnapshotEngine {
    fn name(&self) -> &'static str {
        match (self.pref.vulkan(), self.pref.network_allowed()) {
            (false, false) => "snapshot",
            (true, false) => "snapshot-vulkan",
            (false, true) => "snapshot-network",
            (true, true) => "snapshot-vulkan-network",
        }
    }

    fn dependencies(&self, page: PageId) -> Option<Vec<super::engine::Dependency>> {
        if self.pref.network_allowed() {
            return None;
        }
        let live = self.live(page).ok()?;
        let p = lock(&live);
        // Subframes can have their own CDP sessions. Until those are observed,
        // their dependencies are unknown and this render is not reusable.
        let framed = Self::evaluate(
            &p.cdp,
            &p.session,
            "!!document.querySelector('iframe,frame')",
            "check subframes",
        )
        .ok()?;
        if framed.as_bool()? {
            return None;
        }
        let urls = p.cdp.resources(&p.session)?;
        let mut dependencies = Vec::new();
        for url in urls {
            if url.starts_with("http:") || url.starts_with("https:") {
                continue;
            } // blocked by policy
            let path = local_resource(&url)?;
            if let Ok(meta) = std::fs::metadata(&path) {
                if !meta.is_file() || meta.modified().ok()? > p.started {
                    return None;
                }
            }
            dependencies.push(super::engine::Dependency::read(path).ok()?);
        }
        Some(dependencies)
    }

    fn open(&self, req: &PageRequest) -> Result<PageLayout, EngineError> {
        self.call(|| {
            let cdp = self.cdp()?;
            let mut page = self.load(&cdp, &req.path, req.geometry)?;
            // The probe is in, so the page is as it will be photographed.
            // The first screen is asked for now, to be drawn while extract()
            // reads the page, and kept for the tile call that wants it. A page
            // shorter than a screen has a shorter first band, and drops it.
            let first = Band {
                top_dev: 0,
                height_dev: req.geometry.band_step(),
            };
            let early = cdp
                .send_call(
                    Some(&page.session),
                    "Page.captureScreenshot",
                    screenshot_params(req.geometry, first, false),
                )
                .ok();
            let value = match Self::evaluate(
                &cdp,
                &page.session,
                "window.__terminalDelight.extract()",
                "read the page",
            ) {
                Ok(v) => v,
                Err(e) => {
                    close_page(&page);
                    return Err(e);
                }
            };
            let got: Extracted = match serde_json::from_value(value) {
                Ok(g) => g,
                Err(e) => {
                    close_page(&page);
                    return Err(EngineError::Protocol(format!("the probe's answer: {e}")));
                }
            };
            // The bytes rendered must be the bytes the caller read, or the
            // anchors describe a file that is no longer there.
            let now = std::fs::read(&req.path).map(|b| layout_hash(&b));
            if now.as_ref().ok() != Some(&req.expect) {
                close_page(&page);
                return Err(EngineError::FileChanged);
            }
            let id = PageId(self.next_page.fetch_add(1, Ordering::SeqCst));
            let generation = self.next_generation.fetch_add(1, Ordering::SeqCst);
            page.generation = generation;
            let layout = PageLayout {
                page: Some(id),
                generation,
                rendered: req.expect,
                geometry: req.geometry,
                height_css: got.height_css,
                notes_file: got.notes_file,
                capability: got.capability,
                anchors: got.anchors,
                links: got.links,
                targets: Some(got.targets),
                openers: got.openers,
                dialogs: got.dialogs,
                diagnostics: got.diagnostics,
                videos: Some(got.videos),
            };
            if layout.bands().first() == Some(&first) {
                page.first = early.map(|shot| (first, shot));
            }
            lock(&self.shared.pages).insert(id, Arc::new(Mutex::new(page)));
            Ok(layout)
        })
    }

    fn tile(&self, page: PageId, generation: u64, band: Band) -> Result<Tile, EngineError> {
        self.capture_band(page, generation, band, false)
    }

    fn dialog(&self, page: PageId, generation: u64, id: &str) -> Result<DialogRender, EngineError> {
        self.call(|| {
            let live = self.live(page)?;
            let mut p = lock(&live);
            if p.generation != generation {
                return Err(EngineError::Stale);
            }
            // Opening a dialog changes what is on screen: an early capture
            // still on its way is let finish first, and given up.
            if let Some((_, shot)) = p.first.take() {
                let _ = shot.wait(CAPTURE);
            }
            let (cdp, s, g) = (&p.cdp, &p.session, p.geometry);
            let quoted = serde_json::to_string(id).unwrap_or_default();
            let opened: DialogOpened = serde_json::from_value(Self::evaluate(
                cdp,
                s,
                &format!("window.__terminalDelight.dialogOpen({quoted})"),
                "open the dialog",
            )?)
            .map_err(|e| EngineError::Protocol(e.to_string()))?;
            if !opened.ok {
                return Err(EngineError::Page(format!("the page has no dialog called {id}")));
            }
            // A dialog whose body scrolls at this viewport is not all in one
            // picture: 30 of 285 in the corpus do. Grow the viewport until
            // nothing inside scrolls, as the spike did, then capture.
            let mut height = g.viewport_css_height;
            let mut need = opened.overflow;
            let mut grew = false;
            for _ in 0..6 {
                if need <= 1.0 {
                    break;
                }
                height = (f64::from(height) + need * 1.25).ceil() as u32;
                Self::set_viewport(cdp, s, g, height)?;
                grew = true;
                let m = Self::evaluate(
                    cdp,
                    s,
                    &format!("window.__terminalDelight.dialogMeasure({quoted})"),
                    "measure the dialog",
                )?;
                need = m.get("overflow").and_then(Value::as_f64).unwrap_or(0.0);
            }
            let measured = Self::evaluate(
                cdp,
                s,
                &format!("window.__terminalDelight.dialogMeasure({quoted})"),
                "measure the dialog",
            );
            let result = measured.and_then(|m| {
                let m: DialogMeasured =
                    serde_json::from_value(m).map_err(|e| EngineError::Protocol(e.to_string()))?;
                let shot = cdp
                    .call(
                        Some(s),
                        "Page.captureScreenshot",
                        json!({
                            "format": "png",
                            "optimizeForSpeed": true,
                            "captureBeyondViewport": false,
                            "fromSurface": true,
                            "clip": { "x": m.rect.x, "y": m.rect.y, "width": m.rect.w, "height": m.rect.h, "scale": 1 },
                        }),
                        CAPTURE,
                    )
                    .map_err(|e| protocol(e, "capture the dialog"))?;
                let png = cdp::base64_decode(shot.get("data").and_then(Value::as_str).unwrap_or(""))
                    .map_err(EngineError::Protocol)?;
                let (w, h) = png_size(&png).ok_or_else(|| EngineError::Protocol("a dialog that is not a PNG".into()))?;
                Ok(DialogRender {
                    id: id.to_string(),
                    width_dev: w,
                    height_dev: h,
                    grew_viewport: grew,
                    anchors: m.anchors,
                    links: m.links,
                    openers: m.openers,
                    closers: m.closers.into_iter().flatten().filter(RectCss::is_place).collect(),
                    png,
                })
            });
            let _ = Self::evaluate(
                cdp,
                s,
                &format!("window.__terminalDelight.dialogClose({quoted})"),
                "close the dialog",
            );
            if grew {
                let _ = Self::set_viewport(cdp, s, g, g.viewport_css_height);
            }
            result
        })
    }

    fn close(&self, page: PageId) {
        let gone = lock(&self.shared.pages).remove(&page);
        if let Some(live) = gone {
            self.call(|| {
                close_page(&lock(&live));
                Ok(())
            })
            .ok();
            self.make_ready();
        }
    }

    fn shutdown(&self) {
        lock(&self.shared.pages).clear();
        lock(&self.shared.ready).take();
        let running = lock(&self.shared.browser).take();
        if let Some(b) = running {
            finish(b);
        }
    }
}

impl Drop for SnapshotEngine {
    fn drop(&mut self) {
        self.shutdown();
        lock(&self.jobs).take();
        if let Some(t) = lock(&self.thread).take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_browser_is_driven_over_a_pipe_and_never_a_port() {
        let runtime = runtime_dir();
        let profile = runtime.join("chromium-1-0");
        let args = SnapshotEngine::launch_args(&profile, &pref::Html::default());
        let has = |s: &str| args.iter().any(|a| a == s);
        assert!(has("--remote-debugging-pipe"));
        assert!(
            !args.iter().any(|a| a.to_string_lossy().starts_with("--remote-debugging-port")),
            "a port listens on 127.0.0.1 for any local process to drive a browser that reads file://"
        );
        let dir = args
            .iter()
            .find_map(|a| a.to_str().and_then(|s| s.strip_prefix("--user-data-dir=")))
            .expect("a profile of its own");
        assert!(
            Path::new(dir).starts_with(&runtime),
            "{dir} is not under {}",
            runtime.display()
        );
        assert!(
            has("--password-store=basic"),
            "never reach for the desktop's keyring"
        );
    }

    /// Chromium keeps only the last `--enable-features` it is given, so the
    /// in-process network service and the Vulkan features share one switch,
    /// and the GPU process folds into the browser only off the Vulkan path.
    #[test]
    fn one_switch_carries_every_feature_and_only_software_folds_the_gpu_in() {
        let p = Path::new("/run/user/1000/terminal-delight/chromium-1-0");
        let count = |args: &[OsString], prefix: &str| {
            args.iter()
                .filter(|a| a.to_string_lossy().starts_with(prefix))
                .count()
        };
        let plain = SnapshotEngine::launch_args(p, &pref::Html::default());
        let vulkan = SnapshotEngine::launch_args(
            p,
            &pref::Html {
                gpu: Some("vulkan".into()),
                ..Default::default()
            },
        );
        for args in [&plain, &vulkan] {
            assert_eq!(count(args, "--enable-features="), 1);
            assert_eq!(count(args, "--disable-features="), 1);
            assert!(args.iter().any(|a| a
                .to_string_lossy()
                .starts_with("--enable-features=NetworkServiceInProcess2")));
            assert!(args
                .iter()
                .any(|a| a.to_string_lossy().contains("WebUIOmniboxAimPopup")));
        }
        assert!(vulkan
            .iter()
            .any(|a| a.to_string_lossy() == "--enable-features=NetworkServiceInProcess2,Vulkan,VulkanFromANGLE,DefaultANGLEVulkan"));
        assert!(plain.iter().any(|a| a == "--in-process-gpu"));
        assert!(!vulkan.iter().any(|a| a == "--in-process-gpu"));
    }

    /// The first screen is photographed as the page shows it, and every other
    /// band, or the first screen asked for exactly, clipped to its rows.
    #[test]
    fn only_the_first_screen_is_photographed_without_a_clip() {
        let g = Geometry {
            css_width: 968,
            viewport_css_height: 1400,
            scale: 1.6,
        };
        let screen = Band {
            top_dev: 0,
            height_dev: 2240,
        };
        assert!(is_first_screen(g, screen));
        assert!(screenshot_params(g, screen, false).get("clip").is_none());
        let exact = screenshot_params(g, screen, true);
        assert_eq!(exact["clip"]["height"], 1400.0);
        assert_eq!(exact["captureBeyondViewport"], false);
        let below = Band {
            top_dev: 2240,
            height_dev: 2240,
        };
        let p = screenshot_params(g, below, false);
        assert_eq!(p["clip"]["y"], 1400.0);
        assert_eq!(p["captureBeyondViewport"], true);
        // A page shorter than the screen has a shorter first band: clipped.
        let short = Band {
            top_dev: 0,
            height_dev: 900,
        };
        assert!(!is_first_screen(g, short));
        assert!(screenshot_params(g, short, false).get("clip").is_some());
    }

    /// The notes chrome is hidden from the first frame by the script run
    /// before the page's own, and again by the probe: the same rule, or the
    /// frame drawn at load and the frame photographed would differ.
    #[test]
    fn the_early_sheet_and_the_probe_hide_the_same_notes_ui() {
        for rule in NOTES_UI_HIDE.split('\n') {
            assert!(
                EXTRACT_JS.contains(rule.trim()),
                "extract.js has no `{rule}`"
            );
        }
        let script = new_document_script();
        assert!(script.starts_with(ERROR_HOOK));
        assert!(script.contains("adoptedStyleSheets"));
        assert!(script.contains(".notebar, .note-btn, .concur-zone"));
    }

    #[test]
    fn a_missing_browser_names_every_place_it_looked() {
        let none = SnapshotEngine::locate(&pref::Html::default(), Some(OsStr::new("")));
        assert_eq!(
            none,
            Err(Unavailable::NoBrowser {
                searched: BROWSER_NAMES.iter().map(PathBuf::from).collect(),
                configured: false,
            })
        );
        assert!(SnapshotEngine::locate(&pref::Html::default(), None).is_err());
        let configured = pref::Html {
            chromium: Some(PathBuf::from("/nonexistent/chrome")),
            ..Default::default()
        };
        // A configured path is the only place looked, even with a browser on
        // PATH: somebody wrote that path down on purpose.
        assert_eq!(
            SnapshotEngine::locate(&configured, Some(OsStr::new("/usr/bin:/bin"))),
            Err(Unavailable::NoBrowser {
                searched: vec![PathBuf::from("/nonexistent/chrome")],
                configured: true,
            })
        );
    }

    #[test]
    fn a_browser_is_found_on_path_in_the_order_the_names_are_listed() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("td-locate-{}", std::process::id()));
        let (a, b) = (root.join("a"), root.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let make = |p: &Path, mode| {
            std::fs::write(p, b"#!/bin/sh\n").unwrap();
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)).unwrap();
        };
        make(&a.join("google-chrome"), 0o755);
        make(&b.join("chromium"), 0o644); // not executable: not a browser
        let path = std::env::join_paths([&a, &b]).unwrap();
        assert_eq!(
            SnapshotEngine::locate(&pref::Html::default(), Some(&path)),
            Ok(a.join("google-chrome"))
        );
        make(&b.join("chromium"), 0o755);
        assert_eq!(
            SnapshotEngine::locate(&pref::Html::default(), Some(&path)),
            Ok(b.join("chromium")),
            "chromium is asked for before google-chrome, whichever directory holds it"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_network_flags_are_there_unless_the_network_is_allowed() {
        let p = Path::new("/run/user/1000/terminal-delight/chromium-1-0");
        let blocked = SnapshotEngine::launch_args(p, &pref::Html::default());
        assert!(blocked
            .iter()
            .any(|a| a == "--proxy-bypass-list=<-loopback>"));
        assert!(blocked
            .iter()
            .any(|a| a.to_string_lossy().starts_with("--host-resolver-rules=")));
        let allowed = SnapshotEngine::launch_args(
            p,
            &pref::Html {
                network: Some("allowed".into()),
                ..Default::default()
            },
        );
        assert!(!allowed
            .iter()
            .any(|a| a.to_string_lossy().starts_with("--proxy")));
    }

    #[test]
    fn a_path_becomes_a_file_url_a_browser_reads_back_as_the_same_path() {
        assert_eq!(
            file_url(Path::new("/tmp/a b/#1 100%.html")),
            "file:///tmp/a%20b/%231%20100%25.html"
        );
        assert_eq!(
            file_url(Path::new("/r/2026-09-24-x.html")),
            "file:///r/2026-09-24-x.html"
        );
        assert_eq!(file_url(Path::new("/r/é.html")), "file:///r/%C3%A9.html");
    }

    #[test]
    fn a_browser_that_hung_is_named_as_hung_not_by_its_last_warning() {
        let dbus = "[7168:7189:0925/002600.448623:ERROR:dbus/bus.cc:405] Failed to connect to the bus: Could not parse server address";
        let hung = launch_failure(true, dbus, "the browser did not answer in time");
        assert!(hung.starts_with("it did not answer within 30 s"), "{hung}");
        assert!(
            hung.contains("Failed to connect to the bus"),
            "what it said still follows: {hung}"
        );
        assert_eq!(
            launch_failure(true, "", "x"),
            "it did not answer within 30 s"
        );
        // A browser that died says why, and that line is the whole reason. The
        // sandbox's refusal must survive either way: the engine tests skip on it.
        assert_eq!(
            launch_failure(false, "FATAL: No usable sandbox!", "x"),
            "FATAL: No usable sandbox!"
        );
        assert!(launch_failure(true, "No usable sandbox!", "x").contains("No usable sandbox"));
        assert_eq!(
            launch_failure(false, "", "the pipe closed"),
            "the pipe closed"
        );
    }

    #[test]
    fn a_browser_that_will_not_start_says_why_rather_than_where_its_trace_ended() {
        // The shape of a sandbox refusal on a host that restricts user
        // namespaces: the reason, a stack trace, and the trace's last line.
        let crash = "\
[5155:5155:0924/232132.101:FATAL:zygote_host_impl_linux.cc(132)] No usable sandbox! If you are running on Ubuntu 23.10+ ...
#0 0x55d0c0c2a1b2 base::debug::CollectStackTrace()
#1 0x55d0c0c1f8c3 base::debug::StackTrace::StackTrace()
Received signal 6
#0 0x55d0c0c2a1b2 base::debug::CollectStackTrace()
  r8: 0000000000000000  r9: 00007ffd4f0e8d10
[end of stack trace]
[0924/232731.767322:ERROR:third_party/crashpad/crashpad/util/file/file_io_posix.cc:145] open /sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq: No such file or directory (2)
";
        let why = why_it_died(crash);
        assert!(why.contains("No usable sandbox"), "{why}");
        // With no FATAL line, the last line that is not part of a trace.
        assert_eq!(
            why_it_died("something happened\n#0 0xabc foo()\n[end of stack trace]\n"),
            "something happened"
        );
        assert_eq!(why_it_died(""), "");
    }

    #[test]
    fn a_profile_left_by_a_dead_td_is_removed_and_a_live_ones_is_kept() {
        let root = std::env::temp_dir().join(format!("td-profiles-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        // A pid that cannot be running: above the kernel's pid_max ceiling.
        let dead = root.join("chromium-4194305-0");
        let mine = root.join(format!("chromium-{}-3", std::process::id()));
        let other = root.join("not-a-profile-1");
        for d in [&dead, &mine, &other] {
            std::fs::create_dir_all(d).unwrap();
        }
        remove_stale_profiles(&root);
        assert!(!dead.exists());
        assert!(mine.exists() && other.exists());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
