//! A real pane in gpui's headless test platform, driven by the events a person
//! makes.
//!
//! Until this existed, what a [`TerminalView`] does with a press, a move, a
//! release or a key could only be held by reading `pane.rs` as text: find the
//! handler, find the call, check that one call comes before another. Those
//! scans broke on a reflowed line and agreed with code that did the wrong
//! thing. This builds the view itself — the one production builds, around a
//! real pseudoterminal running a real child — in a gpui window that exists
//! only in memory, and hands a test the same mouse and keyboard gpui hands
//! the pane.
//!
//! **What the terminal runs.** `/bin/sh -c <script>`: the script prints what
//! the test needs on screen and then `exec`s `cat`, so the pane stays alive
//! and quiet. Not the person's shell: their prompt, their rc files and their
//! aliases are not what a test is about.
//!
//! **What the window reads.** The embedded themes and skins, installed by
//! [`crate::theme::init_embedded`] and [`crate::skin::init_embedded`]; nothing
//! in `~/.config` is read or written. The window's grade is the house one
//! with the barrel warp off, so a cell is where the flat arithmetic says it is
//! and a test can aim at it without inverting a curve. The curve has its own
//! tests, against the functions that bend and un-bend.
//!
//! **Nothing reaches the desktop.** In a test build `spawn_detached` writes the
//! launch down instead of starting it ([`desktop_launches`]), so a gesture that
//! would open a browser or a file manager says so without doing it.
//!
//! **Forks are serialised.** Starting the pseudoterminal forks, and a child
//! holds the parent's file locks until it execs; the spawn takes
//! [`crate::testsync::forks_and_locks`] like every other test that forks.
//!
//! **Time is two clocks.** gpui's is the test dispatcher's and moves only when
//! a test moves it; the pseudoterminal's is the machine's, because its reader
//! is a real thread. [`Pane::wait_for`] is the one place the harness waits on
//! the second, and it waits on what is on screen rather than for a duration.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    point, px, size, Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext,
    VisualTestContext,
};

use super::TerminalView;
use crate::docopen::FloatHit;
use crate::docview::engine::{
    Anchor, Band, ConcurSupport, DialogRender, EngineError, NotesCapability, PageEngine, PageId,
    PageLayout, PageRequest, RectCss, Tile, Unavailable,
};
use crate::term;

/// Every desktop launch a pane on this thread has asked for, in order: what
/// `spawn_detached` wrote down instead of starting.
pub(super) fn desktop_launches() -> Vec<String> {
    super::DESKTOP_LAUNCHES.with(|l| l.borrow().clone())
}

/// One harness test at a time. Each runs a shell and a `cat` on a
/// pseudoterminal, and the host and instance tests beside them are timing
/// tests over forks and sockets; twenty panes starting at once is load they
/// were never written to share a machine with. Taken by the first pane a test
/// opens and held by the test's thread until it ends, so a test may open a
/// second pane without waiting on itself.
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

thread_local! {
    static HOLDING: RefCell<Option<std::sync::MutexGuard<'static, ()>>> =
        const { RefCell::new(None) };
}

fn take_the_turn() {
    HOLDING.with(|held| {
        if held.borrow().is_none() {
            let turn = ONE_AT_A_TIME
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *held.borrow_mut() = Some(turn);
        }
    });
}

/// The window a harness pane fills, in logical pixels.
const WINDOW: (f32, f32) = (1000.0, 640.0);

/// How long the terminal gets to print what a test is waiting for. Generous:
/// a loaded CI runner starting `sh` is slow, and a pass never waits this long.
const PATIENCE: Duration = Duration::from_secs(10);

/// A rectangle in window pixels, `(x, y, w, h)`.
pub(super) type Rect = (f32, f32, f32, f32);

/// A pane, its window, and the context that drives both.
pub(super) struct Pane {
    pub(super) view: Entity<TerminalView>,
    pub(super) cx: &'static mut VisualTestContext,
}

impl Pane {
    /// A pane whose terminal runs `/bin/sh -c script`, focused, in a window of
    /// its own, laid out once.
    pub(super) fn running(cx: &mut TestAppContext, script: &str) -> Pane {
        take_the_turn();
        // The pseudoterminal's reader is a real thread, and its events wake
        // the pane's pump from there. gpui's test scheduler reads a wake from
        // another thread as nondeterminism unless it is told to expect one.
        cx.executor().allow_parking();
        cx.update(|cx| {
            let mut outer = crate::theme::house_outer();
            outer.grade.warp = 0.0;
            crate::theme::init_embedded(cx, outer);
            crate::skin::init_embedded(cx);
        });
        let grid = term::GridSize {
            cols: 100,
            rows: 28,
        };
        let session = {
            let _fork = crate::testsync::forks_and_locks();
            term::spawn_program(grid, super::BORN_CELL_PX, "/bin/sh", &["-c", script])
                .expect("a pseudoterminal running /bin/sh")
        };
        // The reader thread and the child outlive the pane unless told: a
        // window-owned terminal is ended by its child, and `cat` never ends.
        let reader = session.notifier.clone();
        cx.on_quit(move || {
            reader.shutdown();
        });
        let window = cx.open_window(size(px(WINDOW.0), px(WINDOW.1)), move |_, cx| {
            TerminalView::around(
                session,
                None,
                None,
                &crate::session::PaneRestore::default(),
                grid,
                super::BORN_CELL_PX,
                cx,
            )
        });
        let view = window.root(cx).expect("the pane is the window's root view");
        let vcx = VisualTestContext::from_window(window.into(), cx).into_mut();
        vcx.update(|window, cx| {
            let focus = view.read(cx).focus_handle.clone();
            window.focus(&focus, cx);
        });
        vcx.run_until_parked();
        let mut pane = Pane { view, cx: vcx };
        pane.redraw();
        let (k, laid_out) = pane.read(|v| (v.warp_k, v.tube_rect().is_some()));
        assert!(laid_out, "the pane's screen was never laid out");
        assert_eq!(
            k,
            (0.0, 0.0),
            "the harness aims at cells with flat arithmetic, so its pane must be flat"
        );
        pane
    }

    /// Read the view.
    pub(super) fn read<R>(&mut self, f: impl FnOnce(&TerminalView) -> R) -> R {
        self.view.read_with(self.cx, |v, _| f(v))
    }

    /// Ask for a frame and let it be drawn, with everything it triggers.
    pub(super) fn redraw(&mut self) {
        self.view.update(self.cx, |_, cx| cx.notify());
        self.cx.run_until_parked();
    }

    /// The visible grid, one string per row, top first.
    pub(super) fn rows(&mut self) -> Vec<String> {
        self.read(|v| {
            v.grid_snapshot()
                .0
                .iter()
                .map(|r| r.iter().collect::<String>().trim_end().to_string())
                .collect()
        })
    }

    /// Wait for `needle` to be on screen, then for a frame that shows it.
    ///
    /// Waits on the machine's clock, because the terminal's reader runs on
    /// it, and fails with the screen as it stands after [`PATIENCE`].
    pub(super) fn wait_for(&mut self, needle: &str) {
        let deadline = Instant::now() + PATIENCE;
        loop {
            self.cx.run_until_parked();
            if self.rows().iter().any(|r| r.contains(needle)) {
                // The grid can hold the text a moment before the pane hears
                // about it; draw a frame from the grid as it is now, so where
                // the rows are painted matches what a click will read.
                self.redraw();
                return;
            }
            if Instant::now() > deadline {
                panic!(
                    "{needle:?} never appeared on the pane; it shows:\n{}",
                    self.rows().join("\n")
                );
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// The middle of `needle` as the pane draws it, in window pixels: found
    /// in the grid, carried through the row transform the last frame painted
    /// with, and placed by the flat layout the grid is drawn in — the tube's
    /// origin, the padding the grid keeps off the glass as gpui rounds it, one
    /// cell per column and one laid-out row height per row.
    pub(super) fn point_at(&mut self, needle: &str) -> Point<Pixels> {
        let (row, col) = self
            .rows()
            .iter()
            .enumerate()
            .find_map(|(r, text)| {
                let at = text.find(needle)?;
                Some((r, text[..at].chars().count()))
            })
            .unwrap_or_else(|| panic!("{needle:?} is not on the pane"));
        let middle = col + needle.chars().count() / 2;
        let scale = self.scale();
        self.read(|v| {
            let (sx, sy, sw, sh) = v.tube_rect().expect("the screen has been laid out");
            let (pad_x, pad_y) = super::grid_pad_drawn(sw, sh, 0.0, 0.0, scale);
            let painted =
                painted_row_of(v, row).unwrap_or_else(|| panic!("grid row {row} is not painted"));
            point(
                px(sx + pad_x + (middle as f32 + 0.5) * v.cell_w),
                px(sy + pad_y + (painted as f32 + 0.5) * v.cell_h),
            )
        })
    }

    /// Every place `needle` is drawn, one per row it is on, top first.
    pub(super) fn points_at_all(&mut self, needle: &str) -> Vec<Point<Pixels>> {
        let hits: Vec<(usize, usize)> = self
            .rows()
            .iter()
            .enumerate()
            .filter_map(|(r, text)| Some((r, text[..text.find(needle)?].chars().count())))
            .collect();
        let middle = needle.chars().count() / 2;
        let scale = self.scale();
        self.read(|v| {
            let (sx, sy, sw, sh) = v.tube_rect().expect("the screen has been laid out");
            let (pad_x, pad_y) = super::grid_pad_drawn(sw, sh, 0.0, 0.0, scale);
            hits.iter()
                .filter_map(|&(row, col)| {
                    let painted = painted_row_of(v, row)?;
                    Some(point(
                        px(sx + pad_x + ((col + middle) as f32 + 0.5) * v.cell_w),
                        px(sy + pad_y + (painted as f32 + 0.5) * v.cell_h),
                    ))
                })
                .collect()
        })
    }

    /// The painted row `at` lies on, counted from the top of the grid.
    pub(super) fn painted_row(&mut self, at: Point<Pixels>) -> usize {
        let scale = self.scale();
        self.read(|v| {
            let (_, sy, sw, sh) = v.tube_rect().expect("the screen has been laid out");
            let (_, pad_y) = super::grid_pad_drawn(sw, sh, 0.0, 0.0, scale);
            ((f32::from(at.y) - sy - pad_y) / v.cell_h).floor() as usize
        })
    }

    /// Dress the pane in a theme of its own — the builtin `deco`, plain —
    /// whose background is not the window's, so what the pane draws can be
    /// told apart from what the window would.
    pub(super) fn wear_own_theme(&mut self) {
        self.view.update(self.cx, |v, cx| {
            let mut own = crate::theme::house_outer();
            own.id = "deco".into();
            own.seed = None;
            v.appearance.theme = Some(crate::theme::ThemeGroup::of(&own));
            v.appearance.inherit_theme = false;
            cx.notify();
        });
        self.cx.run_until_parked();
        let (own, window) = self.view.read_with(self.cx, |v, cx| {
            (v.resolved_theme(cx).bg, crate::theme::theme(cx).bg)
        });
        assert_ne!(
            own, window,
            "the pane's own theme must not paint like the window's"
        );
    }

    /// A link pressed in `view`, as the document itself reports one.
    pub(super) fn follow(&mut self, view: &Entity<crate::docview::DocumentView>, target: &str) {
        view.update(self.cx, |_, cx| {
            cx.emit(crate::docview::FollowLink {
                target: target.to_string(),
                fragment: None,
            })
        });
        self.redraw();
    }

    /// The top and bottom of the painted row `at` lies on, in window pixels.
    pub(super) fn row_span(&mut self, at: Point<Pixels>) -> (f32, f32) {
        let scale = self.scale();
        self.read(|v| {
            let (_, sy, sw, sh) = v.tube_rect().expect("the screen has been laid out");
            let (_, pad_y) = super::grid_pad_drawn(sw, sh, 0.0, 0.0, scale);
            let row = ((f32::from(at.y) - sy - pad_y) / v.cell_h).floor();
            let top = sy + pad_y + row * v.cell_h;
            (top, top + v.cell_h)
        })
    }

    /// The window's scale factor: device pixels per logical pixel.
    pub(super) fn scale(&mut self) -> f32 {
        self.cx.update(|window, _| window.scale_factor())
    }

    /// The pane's screen — the tube everything but the header is drawn in —
    /// in window pixels.
    pub(super) fn screen(&mut self) -> Rect {
        self.read(|v| v.tube_rect().expect("the screen has been laid out"))
    }

    /// Alt held and nothing else.
    pub(super) fn alt() -> Modifiers {
        Modifiers {
            alt: true,
            ..Default::default()
        }
    }

    /// A left click at `at` with `mods` held: down, then up, where it went
    /// down.
    pub(super) fn click(&mut self, at: Point<Pixels>, mods: Modifiers) {
        self.cx.simulate_mouse_down(at, MouseButton::Left, mods);
        self.cx.simulate_mouse_up(at, MouseButton::Left, mods);
    }

    /// A right click at `at`: down, then up.
    pub(super) fn right_click(&mut self, at: Point<Pixels>) {
        self.cx
            .simulate_mouse_down(at, MouseButton::Right, Modifiers::default());
        self.cx
            .simulate_mouse_up(at, MouseButton::Right, Modifiers::default());
    }

    /// The left button goes down at `at`.
    pub(super) fn press(&mut self, at: Point<Pixels>) {
        self.cx
            .simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    }

    /// The pointer moves to `at` with the left button held.
    pub(super) fn drag_to(&mut self, at: Point<Pixels>) {
        self.cx
            .simulate_mouse_move(at, MouseButton::Left, Modifiers::default());
    }

    /// The left button comes up at `at`.
    pub(super) fn release(&mut self, at: Point<Pixels>) {
        self.cx
            .simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    }

    /// Keys, as gpui spells them: `"escape"`, `"alt-k"`, `"a b c"`.
    pub(super) fn keys(&mut self, keys: &str) {
        self.cx.simulate_keystrokes(keys);
    }

    /// The file the floating square is showing, if one is up.
    pub(super) fn float_path(&mut self) -> Option<PathBuf> {
        self.view.read_with(self.cx, |v, cx| v.float_path(cx))
    }

    /// Where the floating square is drawn, in window pixels, if one is up.
    pub(super) fn float_rect(&mut self) -> Option<Rect> {
        self.read(|v| {
            let (sx, sy, sw, sh) = v.tube_rect()?;
            let float = v.float.as_ref()?;
            let r = crate::docopen::clamp_float(float.rect, sw, sh);
            Some((sx + r.x, sy + r.y, r.w, r.h))
        })
    }

    /// Where the floating square last painted `hit`, in window pixels: the
    /// last such zone, because zones are recorded in paint order and the
    /// topmost wins.
    pub(super) fn float_zone(&mut self, hit: FloatHit) -> Option<Rect> {
        self.read(|v| {
            v.float_zones
                .borrow()
                .iter()
                .rev()
                .find(|z| z.hit == hit)
                .map(|z| (z.x, z.y, z.w, z.h))
        })
    }

    /// The middle of a rectangle.
    pub(super) fn middle(r: Rect) -> Point<Pixels> {
        point(px(r.0 + r.2 / 2.0), px(r.1 + r.3 / 2.0))
    }

    /// What is on the clipboard, as text.
    pub(super) fn clipboard(&mut self) -> Option<String> {
        self.cx.read_from_clipboard().and_then(|c| c.text())
    }

    /// Answer every "can HTML be drawn?" with `answer` from now on.
    pub(super) fn html_engine(&mut self, answer: Result<Arc<dyn PageEngine>, Unavailable>) {
        self.cx
            .update(|_, cx| crate::docview::set_engine(cx, answer));
    }

    /// What the document on the pane shows of a brief's notes: the control
    /// socket's `doc notes`.
    pub(super) fn doc_notes(&mut self) -> Result<serde_json::Value, String> {
        let line = self.view.read_with(self.cx, |v, cx| v.doc_notes(cx))?;
        serde_json::from_str(&line).map_err(|e| e.to_string())
    }

    /// Wait until the terminal takes keys: a pane refuses them for its first
    /// moments, while its child is still starting.
    pub(super) fn settle(&mut self) {
        let ready = self.read(|v| v.spawned) + Duration::from_millis(200);
        if let Some(left) = ready.checked_duration_since(Instant::now()) {
            std::thread::sleep(left);
        }
    }

    /// A wheel turn at `at` with `mods` held: `lines` notches, positive
    /// toward the top.
    pub(super) fn wheel(&mut self, at: Point<Pixels>, lines: f32, mods: Modifiers) {
        self.cx.simulate_event(gpui::ScrollWheelEvent {
            position: at,
            delta: gpui::ScrollDelta::Lines(point(0.0, lines)),
            modifiers: mods,
            touch_phase: gpui::TouchPhase::Moved,
        });
    }

    /// The pointer moves to `at` with no button held.
    pub(super) fn hover(&mut self, at: Point<Pixels>) {
        self.cx.simulate_mouse_move(at, None, Modifiers::default());
    }

    /// The pointer leaves the window altogether: gpui's own event, with no
    /// move and no new position.
    pub(super) fn leave_window(&mut self) {
        self.cx.simulate_event(gpui::MouseExitEvent {
            position: point(px(-1.0), px(-1.0)),
            pressed_button: None,
            modifiers: Modifiers::default(),
        });
    }

    /// `paths` dragged in from a file manager and dropped at `at`.
    pub(super) fn drop_files(&mut self, at: Point<Pixels>, paths: Vec<PathBuf>) {
        self.cx.simulate_event(gpui::FileDropEvent::Entered {
            position: at,
            paths: gpui::ExternalPaths(paths.into()),
        });
        self.cx
            .simulate_event(gpui::FileDropEvent::Submit { position: at });
    }

    /// How far the terminal's view is scrolled back into its history, in rows.
    pub(super) fn scrolled_back(&mut self) -> usize {
        self.read(|v| v.session.term.lock().display_offset())
    }

    /// The pseudoterminal's size as the kernel holds it, once `settled` says
    /// it is what the test is waiting for: a resize reaches the kernel from
    /// the terminal's own thread, a moment after the pane decides it.
    pub(super) fn kernel_winsize_once(
        &mut self,
        settled: impl Fn(&libc::winsize) -> bool,
    ) -> libc::winsize {
        use std::os::fd::AsRawFd;
        let fd = self.read(|v| {
            v.session
                .master
                .as_ref()
                .expect("a pane that owns its pseudoterminal")
                .as_raw_fd()
        });
        let deadline = Instant::now() + PATIENCE;
        loop {
            // SAFETY: TIOCGWINSZ writes one `winsize` through the pointer, and
            // `fd` is the pane's own master, open for as long as the pane is.
            let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
            let ok = unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws) } == 0;
            if ok && settled(&ws) || Instant::now() > deadline {
                return ws;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// The floating square's document view.
    pub(super) fn float_view(&mut self) -> Option<Entity<crate::docview::DocumentView>> {
        self.read(|v| v.float.as_ref().map(|f| f.view.clone()))
    }

    /// The Document face's view, whether or not that face is showing.
    pub(super) fn face_view(&mut self) -> Option<Entity<crate::docview::DocumentView>> {
        self.read(|v| v.doc.as_ref().map(|d| d.view.clone()))
    }

    /// Where a document view is scrolled, as a fraction of its page. `None`
    /// before it has been laid out.
    pub(super) fn scroll_of(&mut self, view: &Entity<crate::docview::DocumentView>) -> Option<f32> {
        view.read_with(self.cx, |v, _| v.scroll().map(|s| s.top))
    }

    /// The face the pane is showing.
    pub(super) fn face(&mut self) -> crate::workbench::Face {
        self.read(|v| v.bench.face())
    }

    /// Put `path` on the pane's Document face, as a Ctrl+Alt+click's split
    /// does once the workspace has made the pane.
    pub(super) fn show_document(&mut self, path: &Path) {
        let target = crate::docopen::drawable_document(path)
            .unwrap_or_else(|| panic!("{} is not a document TD draws", path.display()));
        self.view
            .update(self.cx, |v, cx| v.show_document(target, None, cx));
        self.redraw();
    }

    /// Everything the pane asks the workspace to open beside it, from now on.
    pub(super) fn asks_beside(&mut self) -> Rc<RefCell<Vec<Asked>>> {
        let asked = Rc::new(RefCell::new(Vec::new()));
        let log = asked.clone();
        self.cx.update(|_, cx| {
            cx.subscribe(&self.view, move |_, ev: &super::OpenDoc, _| {
                log.borrow_mut().push(Asked {
                    path: ev.target.path.clone(),
                    by: ev.by,
                    carry: ev.carry.as_ref().map(|v| v.entity_id()),
                    row: ev.row,
                });
            })
            .detach();
        });
        asked
    }
}

/// The painted row showing grid row `row`, through the transform the last
/// frame was painted with. The last such row: a bottom-anchored screen pads
/// above its content, and the transform clamps every padding row onto the
/// first grid row, which is drawn below them.
fn painted_row_of(v: &TerminalView, row: usize) -> Option<usize> {
    (0..v.grid.rows).rfind(|&p| v.paint_row_to_grid_row(p) == row)
}

/// One [`super::OpenDoc`] the pane emitted, in the parts a test compares.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Asked {
    pub(super) path: PathBuf,
    pub(super) by: crate::docopen::Asker,
    pub(super) carry: Option<gpui::EntityId>,
    pub(super) row: Option<usize>,
}

/// A page engine that lays a brief out without a browser.
///
/// It answers `open` with three of the anchors
/// `tests/fixtures/page/brief.html` tags, as a browser running its script
/// would, and draws nothing: every tile is answered as stale, which ends the
/// render quietly, so no picture is made and nothing is written to the page
/// cache under the person's home directory. What it exercises is everything
/// TD builds on a laid-out page — the notes layer read from the file's own
/// island, and the view and the square around it.
pub(super) struct FakeBriefEngine;

/// The anchors [`FakeBriefEngine`] lays out, by id.
pub(super) const BRIEF_ANCHORS: [(&str, &str); 3] = [
    ("verdict-tiles-not-pages", "Tiles not pages"),
    ("finding-anchors-hold", "Anchors hold"),
    ("verdict-no-network", "No network"),
];

impl PageEngine for FakeBriefEngine {
    fn name(&self) -> &'static str {
        "harness"
    }

    fn open(&self, req: &PageRequest) -> Result<PageLayout, EngineError> {
        let anchors = BRIEF_ANCHORS
            .iter()
            .enumerate()
            .map(|(i, (nid, title))| Anchor {
                nid: nid.to_string(),
                title: title.to_string(),
                tag: "section".into(),
                dialog: None,
                rect: Some(RectCss {
                    x: 28.0,
                    y: 80.0 + 600.0 * i as f32,
                    w: 520.0,
                    h: 120.0,
                }),
                button: None,
                concur_zone: None,
                has_note: Some(false),
                has_concur: Some(false),
                line: None,
            })
            .collect();
        Ok(PageLayout {
            page: Some(PageId(1)),
            generation: 1,
            rendered: req.expect,
            geometry: req.geometry,
            height_css: 2400.0,
            notes_file: None,
            capability: NotesCapability {
                notes_islands: 1,
                concurs_island: false,
                tagged: BRIEF_ANCHORS.len() as u32,
                concur: ConcurSupport::NotSupported,
            },
            anchors,
            links: vec![],
            targets: Some(vec![]),
            openers: vec![],
            dialogs: vec![],
            diagnostics: vec![],
        })
    }

    fn tile(&self, _: PageId, _: u64, _: Band) -> Result<Tile, EngineError> {
        Err(EngineError::Stale)
    }

    fn dialog(&self, _: PageId, _: u64, _: &str) -> Result<DialogRender, EngineError> {
        Err(EngineError::Closed)
    }

    fn close(&self, _: PageId) {}
}

/// A directory of its own for one test, removed when the test ends.
///
/// Not [`crate::testsync::Scratch`], whose name carries the thread's id in
/// parentheses: these paths are printed on a pane and clicked, and a person's
/// paths do not end a directory in `)`. Each test passes a tag of its own.
pub(super) struct Scratch(PathBuf);

impl Scratch {
    pub(super) fn new(tag: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("td-pane-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        Scratch(dir)
    }

    /// `name` in the directory, whether or not anything is there.
    pub(super) fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    /// A file from the repository, copied in under `name`: a short path to
    /// print, whatever directory the checkout sits in.
    pub(super) fn fixture(&self, from: &str, name: &str) -> PathBuf {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(from);
        let to = self.0.join(name);
        std::fs::copy(&source, &to).unwrap_or_else(|e| panic!("copy {}: {e}", source.display()));
        to
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
