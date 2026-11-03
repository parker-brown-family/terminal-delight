//! Keyboard navigation that is not a single keystroke: the pane marks' tap and
//! hold, the back and forward trail through recently visited panes, and the
//! line test behind Alt+E.
//!
//! Parker, 2026-11-02, on the Hands on Keyboard plan
//! (`docs/plans/hands-on-keyboard/`): *"welcome keyboard folks and give me
//! better nav not mouse tooling"*. Everything here is pure — times and pane
//! keys go in, a decision comes out — so the rules are tested without a window,
//! and the window only wires them to its events.

use std::time::{Duration, Instant};

/// How long Alt+digit must be held before it marks rather than goes.
///
/// Parker: *"300 ms will start as baseline. this is not a common action - does
/// not and should not be fast."*
pub const MARK_HOLD: Duration = Duration::from_millis(300);

/// The marks a window has: 1 to 9.
pub const MARKS: u8 = 9;

/// What a pane-mark key turned out to mean.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkAct<P> {
    /// Released before [`MARK_HOLD`]: go to the pane carrying mark `n`.
    Go(u8),
    /// Held past [`MARK_HOLD`]: put mark `n` on the pane that was focused when
    /// the key went down, which is `P`.
    Set(u8, P),
}

/// One Alt+digit press in flight.
///
/// **The go happens when the key comes up, never when it goes down.** A go on
/// key-down would move the focus before the hold could finish, and the hold
/// would then mark the pane it had just arrived at instead of the one the
/// person was standing on. Car radio presets settle it the same way: tap to
/// tune, hold to store, and the tap tunes on release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkPress<P> {
    pub n: u8,
    pub at: Instant,
    /// The pane focused when the key went down — what a hold marks. `None` when
    /// nothing held the keyboard, in which case a hold marks nothing.
    pub from: Option<P>,
}

/// The tap-or-hold gesture behind Alt+1…9.
///
/// Three events drive it: the key going down, the key coming up, and the
/// [`MARK_HOLD`] timer firing. A repeat of a held key (`is_held` on the
/// desktop) is not a new press and is answered with nothing.
#[derive(Clone, Debug, Default)]
pub struct MarkGesture<P> {
    press: Option<MarkPress<P>>,
}

impl<P: Clone + PartialEq> MarkGesture<P> {
    pub fn new() -> Self {
        Self { press: None }
    }

    /// Whether a press is waiting to become a tap or a hold.
    pub fn pending(&self) -> Option<u8> {
        self.press.as_ref().map(|p| p.n)
    }

    /// The key went down. A second digit pressed while the first is still
    /// down replaces it: the person changed their mind, and the earlier press
    /// will never see its own key come up in an order that means anything.
    pub fn down(&mut self, n: u8, at: Instant, from: Option<P>) {
        if !(1..=MARKS).contains(&n) {
            return;
        }
        self.press = Some(MarkPress { n, at, from });
    }

    /// The key came up. A release inside [`MARK_HOLD`] is a tap; a release
    /// after a hold that already fired finds nothing pending and does nothing.
    pub fn up(&mut self, n: u8, at: Instant) -> Option<MarkAct<P>> {
        let press = self.press.as_ref()?;
        if press.n != n {
            return None;
        }
        let press = self.press.take()?;
        if at.saturating_duration_since(press.at) < MARK_HOLD {
            Some(MarkAct::Go(n))
        } else {
            // The timer is late or never ran: the key was held, so it marks.
            press.from.map(|p| MarkAct::Set(n, p))
        }
    }

    /// The hold timer fired for the press that started at `started`. A timer
    /// left over from an earlier press (released, or replaced by another
    /// digit) finds a different press, or none, and does nothing.
    ///
    /// The timer firing IS the hold. It is not checked against the wall clock
    /// as well: a test executor's clock advances without the wall clock moving,
    /// and a second check there would make every hold in a test read as a tap.
    pub fn held(&mut self, started: Instant) -> Option<MarkAct<P>> {
        let press = self.press.as_ref()?;
        if press.at != started {
            return None;
        }
        let press = self.press.take()?;
        press.from.map(|p| MarkAct::Set(press.n, p))
    }
}

/// The most panes the trail remembers behind the one you are on. Parker: *"up
/// to 9? yes up to 9."*
pub const TRAIL_BACK: usize = 9;

/// Recently visited panes, walked like a browser's back and forward.
///
/// Parker, on the drawn afterglow: *"hard no"* to showing it, then *"mouse
/// browser forward and back could perform the afterglow trail without the
/// keyboard... (DO NOT SHOW NUMBERS) this will be INTRINSIC and hidden from
/// view"*. So nothing here is drawn: it is a list and a cursor.
///
/// A visit that did not come from the trail itself cuts off the forward part,
/// as a browser does after you follow a link from a page you went back to.
#[derive(Clone, Debug)]
pub struct Trail<P> {
    seen: Vec<P>,
    /// Index into `seen` of the pane you are on.
    at: usize,
    /// Set while a back or forward is landing, so the focus change it causes
    /// is not recorded as a new visit.
    walking: Option<P>,
}

impl<P: Clone + PartialEq> Default for Trail<P> {
    fn default() -> Self {
        Self {
            seen: Vec::new(),
            at: 0,
            walking: None,
        }
    }
}

impl<P: Clone + PartialEq> Trail<P> {
    /// The pane `p` now holds the focus.
    pub fn visit(&mut self, p: P) {
        if let Some(w) = self.walking.take() {
            if w == p {
                return;
            }
        }
        if self.seen.get(self.at) == Some(&p) {
            return;
        }
        if !self.seen.is_empty() {
            self.seen.truncate(self.at + 1);
        }
        // One pane appears once: revisiting moves it to the front, so going
        // back always reaches somewhere you have not just been.
        self.seen.retain(|q| *q != p);
        self.seen.push(p);
        let keep = TRAIL_BACK + 1;
        if self.seen.len() > keep {
            self.seen.drain(..self.seen.len() - keep);
        }
        self.at = self.seen.len() - 1;
    }

    /// One step back, if there is anywhere to go, keeping only panes `alive`
    /// says still exist.
    pub fn back(&mut self, alive: impl Fn(&P) -> bool) -> Option<P> {
        self.step(-1, alive)
    }

    /// One step forward, after a back.
    pub fn forward(&mut self, alive: impl Fn(&P) -> bool) -> Option<P> {
        self.step(1, alive)
    }

    fn step(&mut self, dir: isize, alive: impl Fn(&P) -> bool) -> Option<P> {
        let mut i = self.at as isize;
        loop {
            i += dir;
            if i < 0 || i as usize >= self.seen.len() {
                return None;
            }
            let p = &self.seen[i as usize];
            if alive(p) {
                self.at = i as usize;
                self.walking = Some(p.clone());
                return Some(p.clone());
            }
        }
    }

    #[cfg(test)]
    fn panes(&self) -> &[P] {
        &self.seen
    }
}

/// One key in scroll mode (Alt+[), and what it moves.
///
/// Scroll mode is tmux's copy mode for this terminal: letters stop going to
/// the program and move the view instead. Terminal Delight knows more than
/// lines — which lines are your turns and which are errors — so it moves by
/// those too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollMotion {
    /// Positive is toward older lines.
    Lines(i32),
    PageUp,
    PageDown,
    Top,
    Bottom,
    /// `[` / `]`: the previous / next block — one of your turns or an error.
    Block {
        older: bool,
    },
    /// `{` / `}`: the previous / next of your own turns.
    Turn {
        older: bool,
    },
    /// `e` / `E`: the previous / next error line.
    Error {
        older: bool,
    },
    /// `/`: search this pane, which leaves the mode for the find panel.
    Find,
    /// `q` or Escape: leave, back at the live bottom.
    Exit,
}

/// What a key does in scroll mode. `None` is a key the mode has no use for;
/// the mode still takes it, so a stray letter never reaches the program from a
/// view the person has scrolled away from.
///
/// The braces arrive two ways, like every shifted symbol: the desktop names
/// `{` and drops the shift, gpui's parser keeps the shift on `[`. Both are
/// the same press, and so are `G` and shift+`g`.
pub fn scroll_motion(key: &str, shift: bool) -> Option<ScrollMotion> {
    use ScrollMotion::*;
    Some(match (key, shift) {
        ("k" | "up", false) => Lines(1),
        ("j" | "down", false) => Lines(-1),
        ("pageup" | "u" | "b", false) => PageUp,
        ("pagedown" | "d" | "f" | "space", false) => PageDown,
        ("g" | "home", false) => Top,
        ("g", true) | ("G", _) | ("end", _) => Bottom,
        ("[", false) => Block { older: true },
        ("]", false) => Block { older: false },
        ("[", true) | ("{", _) => Turn { older: true },
        ("]", true) | ("}", _) => Turn { older: false },
        ("e", false) => Error { older: true },
        ("e", true) | ("E", _) => Error { older: false },
        ("/", _) => Find,
        ("q" | "escape", _) => Exit,
        _ => return None,
    })
}

/// The nearest line of interest above (`older`) or below `top`, the grid line
/// at the top of the view. Lines are alacritty's `Line.0`: negative is
/// scrollback, and smaller is older.
pub fn nearest(lines: &[i32], top: i32, older: bool) -> Option<i32> {
    if older {
        lines.iter().copied().filter(|&l| l < top).max()
    } else {
        lines.iter().copied().filter(|&l| l > top).min()
    }
}

/// How long ago a pane was last attended to: the newer of the person last
/// visiting it and its agent last changing state. Parker, on lookup: *"It is
/// very important to include the age of the pane's last visit, or action"*.
///
/// `None` when neither is known — a pane nobody has visited this session and
/// whose agent has not been seen to change — which draws as a dash, never as
/// a zero.
pub fn last_touch(
    visited: Option<Instant>,
    acted: Option<Instant>,
    now: Instant,
) -> Option<Duration> {
    let newest = match (visited, acted) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }?;
    Some(now.saturating_duration_since(newest))
}

/// The row a digit picks in a numbered panel — NEEDS ME or the artifact rail —
/// as an index from the top: `1` is row 0. Zero is not a row: it is Alt+0's,
/// the trail's.
pub fn row_digit(key: &str) -> Option<usize> {
    match key.as_bytes() {
        [d @ b'1'..=b'9'] => Some((d - b'1') as usize),
        _ => None,
    }
}

/// Which artifact rail is up, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailUp {
    None,
    /// The focused pane's own artifacts.
    Pane,
    /// Every pane's, the window's rail.
    All,
}

/// What Alt+O does, given what is on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactStep {
    /// The window's rail is up: the chord that climbed to it puts it away.
    CloseRail,
    /// Open what the focused pane handed over last: a page or Markdown file
    /// in the reading frame, anything else behind a question first.
    OpenNewest,
    /// Raise the focused pane's own artifacts, numbered, newest first.
    OpenPaneRail,
    /// Raise the window's rail: every pane's artifacts.
    OpenRail,
    /// Nothing has been handed over anywhere in the window: say so.
    SayNone,
}

/// Alt+O's ladder, one rung per press, and every rung about the FOCUSED pane
/// until the last. Parker, 2026-11-03: *"alt+o should still default to opening
/// the MOST RECENT ARTIFACT FOR THE PANE! but we just go stratight to the
/// GLOBAL artifactets"*, then the ladder:
///
/// 1. Alt+O opens the focused pane's newest artifact — or the question that
///    asks before it leaves for the browser or the desktop.
/// 2. Alt+O with that open (the page, or the question) raises *that pane's*
///    artifacts — *"not the Global Spine. My goodness, not the Global Spine."*
/// 3. Alt+O with the pane's artifacts up raises the window's.
/// 4. Alt+O with the window's up puts it away.
///
/// What other panes have open never decides a rung. It used to: "any document
/// open anywhere" sent the first press straight to the window's rail, because
/// in a working window some pane always has one open.
///
/// A pane that has handed nothing over skips the rungs it has nothing for,
/// rather than opening an empty list.
pub fn artifact_step(
    rail: RailUp,
    pane_showing: bool,
    pane_has: bool,
    window_has: bool,
) -> ArtifactStep {
    use ArtifactStep::*;
    match rail {
        RailUp::All => CloseRail,
        RailUp::Pane => OpenRail,
        RailUp::None if pane_has && pane_showing => OpenPaneRail,
        RailUp::None if pane_has => OpenNewest,
        RailUp::None if window_has => OpenRail,
        RailUp::None => SayNone,
    }
}

/// Whether a line of terminal output reads as an error report, for Alt+E.
///
/// A list of the shapes compilers, test runners, shells and interpreters
/// actually print, not a search for the word: "0 errors", "error handling"
/// and "errors.rs" are ordinary lines, and a key that stopped on every one of
/// them would be switched off within a day.
pub fn looks_like_error(line: &str) -> bool {
    let t = line.trim_start();
    // rustc and cargo: `error[E0425]: …`, `error: could not compile`.
    if t.starts_with("error[") || t.starts_with("error:") || t.starts_with("Error:") {
        return true;
    }
    if t.starts_with("ERROR") || t.starts_with("FATAL") || t.starts_with("fatal:") {
        return true;
    }
    // Python and Node.
    if t.starts_with("Traceback (most recent call last)") || t.starts_with("Uncaught ") {
        return true;
    }
    // cargo test, pytest and friends.
    if t.starts_with("FAILED ") || t.starts_with("--- FAIL") || t.starts_with("test result: FAILED")
    {
        return true;
    }
    if t.contains("panicked at") {
        return true;
    }
    // `foo.rs:12:5: error: …`, gcc's and many linters' shape; `bash: x: command not found`.
    if t.contains(": error:") || t.contains(": fatal error:") || t.ends_with(": command not found")
    {
        return true;
    }
    // `SomethingError: message` at the start of a line — a raised exception's last line.
    if let Some((head, _)) = t.split_once(": ") {
        if !head.contains(' ')
            && (head.ends_with("Error") || head.ends_with("Exception"))
            && head.len() > "Error".len()
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn a_quick_release_goes_and_marks_nothing() {
        let mut g = MarkGesture::new();
        let at = t0();
        g.down(3, at, Some("here"));
        assert_eq!(
            g.up(3, at + Duration::from_millis(120)),
            Some(MarkAct::Go(3))
        );
        // The timer for that press fires later and finds nothing.
        assert_eq!(g.held(at), None);
    }

    #[test]
    fn a_hold_marks_the_pane_the_key_went_down_on() {
        let mut g = MarkGesture::new();
        let at = t0();
        g.down(3, at, Some("here"));
        assert_eq!(g.held(at), Some(MarkAct::Set(3, "here")));
        // And the release after it does not also go.
        assert_eq!(g.up(3, at + Duration::from_millis(900)), None);
    }

    #[test]
    fn the_threshold_is_the_boundary_both_ways() {
        let at = t0();
        let mut g = MarkGesture::new();
        g.down(5, at, Some(1));
        assert_eq!(
            g.up(5, at + MARK_HOLD - Duration::from_millis(1)),
            Some(MarkAct::Go(5))
        );
        let mut g = MarkGesture::new();
        g.down(5, at, Some(1));
        assert_eq!(g.up(5, at + MARK_HOLD), Some(MarkAct::Set(5, 1)));
    }

    #[test]
    fn a_late_release_with_no_timer_still_marks() {
        let mut g = MarkGesture::new();
        let at = t0();
        g.down(2, at, Some("p"));
        assert_eq!(
            g.up(2, at + Duration::from_secs(1)),
            Some(MarkAct::Set(2, "p"))
        );
    }

    #[test]
    fn a_stale_timer_cannot_mark_a_newer_press() {
        let mut g = MarkGesture::new();
        let first = t0();
        g.down(1, first, Some("a"));
        let second = first + Duration::from_millis(50);
        g.down(4, second, Some("b"));
        // The first press's timer fires while the second is still down.
        assert_eq!(g.held(first), None);
        assert_eq!(g.pending(), Some(4));
        assert_eq!(
            g.up(4, second + Duration::from_millis(80)),
            Some(MarkAct::Go(4))
        );
    }

    #[test]
    fn a_release_of_another_digit_is_not_this_press() {
        let mut g = MarkGesture::new();
        let at = t0();
        g.down(7, at, Some(0));
        assert_eq!(g.up(6, at + Duration::from_millis(10)), None);
        assert_eq!(g.pending(), Some(7));
    }

    #[test]
    fn nothing_focused_means_a_hold_marks_nothing_but_a_tap_still_goes() {
        let mut g: MarkGesture<u8> = MarkGesture::new();
        let at = t0();
        g.down(3, at, None);
        assert_eq!(g.held(at), None);
        g.down(3, at, None);
        assert_eq!(
            g.up(3, at + Duration::from_millis(10)),
            Some(MarkAct::Go(3))
        );
    }

    #[test]
    fn only_one_to_nine_are_marks() {
        let mut g = MarkGesture::new();
        g.down(0, t0(), Some(1));
        assert_eq!(g.pending(), None);
        g.down(10, t0(), Some(1));
        assert_eq!(g.pending(), None);
    }

    #[test]
    fn back_and_forward_walk_the_visits_without_recording_themselves() {
        let mut t = Trail::default();
        for p in ["a", "b", "c"] {
            t.visit(p);
        }
        assert_eq!(t.back(|_| true), Some("b"));
        t.visit("b"); // the focus change the back caused
        assert_eq!(t.back(|_| true), Some("a"));
        t.visit("a");
        assert_eq!(t.back(|_| true), None);
        assert_eq!(t.forward(|_| true), Some("b"));
        t.visit("b");
        assert_eq!(t.forward(|_| true), Some("c"));
        t.visit("c");
        assert_eq!(t.forward(|_| true), None);
    }

    #[test]
    fn a_new_visit_after_going_back_drops_the_forward_part() {
        let mut t = Trail::default();
        for p in ["a", "b", "c"] {
            t.visit(p);
        }
        t.back(|_| true);
        t.visit("b");
        t.visit("d");
        assert_eq!(t.panes(), &["a", "b", "d"]);
        assert_eq!(t.forward(|_| true), None);
    }

    #[test]
    fn the_trail_keeps_nine_behind_the_current_pane() {
        let mut t = Trail::default();
        for p in 0..20 {
            t.visit(p);
        }
        assert_eq!(t.panes().len(), TRAIL_BACK + 1);
        let mut steps = 0;
        while let Some(p) = t.back(|_| true) {
            t.visit(p);
            steps += 1;
        }
        assert_eq!(steps, TRAIL_BACK);
    }

    #[test]
    fn a_revisited_pane_appears_once() {
        let mut t = Trail::default();
        for p in ["a", "b", "a", "c"] {
            t.visit(p);
        }
        assert_eq!(t.panes(), &["b", "a", "c"]);
    }

    #[test]
    fn a_closed_pane_is_skipped_rather_than_landed_on() {
        let mut t = Trail::default();
        for p in ["a", "b", "c"] {
            t.visit(p);
        }
        assert_eq!(t.back(|p| *p != "b"), Some("a"));
        // With nothing alive behind, a back stays where it is.
        let mut t = Trail::default();
        for p in ["a", "b"] {
            t.visit(p);
        }
        assert_eq!(t.back(|p| *p == "b"), None);
    }

    #[test]
    fn a_walk_that_lands_elsewhere_is_a_new_visit() {
        let mut t = Trail::default();
        for p in ["a", "b", "c"] {
            t.visit(p);
        }
        t.back(|_| true); // aims at b
        t.visit("z"); // but something else took the focus
        assert_eq!(t.panes(), &["a", "b", "z"]);
    }

    #[test]
    fn errors_are_recognised_in_the_shapes_tools_print() {
        for line in [
            "error[E0425]: cannot find function `paging_shift` in this scope",
            "error: could not compile `terminal-delight` (bin \"terminal-delight\") due to 1 previous error",
            "Error: ENOENT: no such file or directory, open 'x.json'",
            "thread 'main' panicked at src/main.rs:3:5:",
            "Traceback (most recent call last):",
            "ValueError: invalid literal for int() with base 10: 'x'",
            "test result: FAILED. 40 passed; 1 failed; 0 ignored",
            "FAILED tests/test_api.py::test_login - AssertionError",
            "fatal: not a git repository (or any of the parent directories): .git",
            "src/x.c:12:5: error: expected ';' before '}' token",
            "bash: cargo-nextest: command not found",
            "  error[E0599]: no method named `foo`",
            "--- FAIL: TestThing (0.00s)",
        ] {
            assert!(looks_like_error(line), "{line}");
        }
    }

    #[test]
    fn ordinary_lines_that_mention_errors_are_not_errors() {
        for line in [
            "Compiling terminal-delight v0.5.0",
            "0 errors, 2 warnings",
            "test result: ok. 41 passed; 0 failed",
            "No errors found.",
            "src/errors.rs | 12 +++---",
            "We need better error handling here.",
            "warning: unused variable: `x`",
            "the error rate fell to 0.2%",
            "Error",
            "MyError is a type we export",
        ] {
            assert!(!looks_like_error(line), "{line}");
        }
    }

    #[test]
    fn the_age_is_the_newer_touch_and_unknown_stays_unknown() {
        let now = t0() + Duration::from_secs(3600);
        let ten = now - Duration::from_secs(600);
        let two = now - Duration::from_secs(120);
        assert_eq!(
            last_touch(Some(ten), Some(two), now),
            Some(Duration::from_secs(120))
        );
        assert_eq!(
            last_touch(Some(two), Some(ten), now),
            Some(Duration::from_secs(120))
        );
        assert_eq!(
            last_touch(None, Some(ten), now),
            Some(Duration::from_secs(600))
        );
        assert_eq!(
            last_touch(Some(ten), None, now),
            Some(Duration::from_secs(600))
        );
        assert_eq!(last_touch(None, None, now), None, "never seen is not zero");
    }

    #[test]
    fn a_digit_names_a_row_from_the_top_and_zero_names_none() {
        assert_eq!(row_digit("1"), Some(0));
        assert_eq!(row_digit("3"), Some(2));
        assert_eq!(row_digit("9"), Some(8));
        assert_eq!(row_digit("0"), None);
        assert_eq!(row_digit("12"), None);
        assert_eq!(row_digit("a"), None);
        assert_eq!(row_digit(""), None);
    }

    #[test]
    fn alt_o_climbs_parkers_ladder_from_the_focused_pane() {
        use ArtifactStep::*;
        use RailUp as R;
        // rail up, focused pane showing its page or question, pane has, window has
        assert_eq!(
            artifact_step(R::None, false, true, true),
            OpenNewest,
            "1: the pane's newest"
        );
        assert_eq!(
            artifact_step(R::None, true, true, true),
            OpenPaneRail,
            "2: the pane's artifacts"
        );
        assert_eq!(
            artifact_step(R::Pane, true, true, true),
            OpenRail,
            "3: the window's"
        );
        assert_eq!(artifact_step(R::Pane, false, true, true), OpenRail);
        assert_eq!(
            artifact_step(R::All, true, true, true),
            CloseRail,
            "4: away"
        );
        // A pane with nothing of its own skips to the window's, or says so.
        assert_eq!(artifact_step(R::None, false, false, true), OpenRail);
        assert_eq!(artifact_step(R::None, true, false, true), OpenRail);
        assert_eq!(artifact_step(R::None, false, false, false), SayNone);
    }

    #[test]
    fn scroll_mode_keys_read_as_drawn_on_its_strip() {
        use ScrollMotion::*;
        assert_eq!(scroll_motion("k", false), Some(Lines(1)));
        assert_eq!(scroll_motion("j", false), Some(Lines(-1)));
        assert_eq!(scroll_motion("g", false), Some(Top));
        assert_eq!(scroll_motion("g", true), Some(Bottom));
        assert_eq!(scroll_motion("G", false), Some(Bottom));
        assert_eq!(scroll_motion("[", false), Some(Block { older: true }));
        assert_eq!(scroll_motion("]", false), Some(Block { older: false }));
        // The braces, as the parser and as the desktop spell them.
        assert_eq!(scroll_motion("[", true), Some(Turn { older: true }));
        assert_eq!(scroll_motion("{", false), Some(Turn { older: true }));
        assert_eq!(scroll_motion("]", true), Some(Turn { older: false }));
        assert_eq!(scroll_motion("}", false), Some(Turn { older: false }));
        assert_eq!(scroll_motion("e", false), Some(Error { older: true }));
        assert_eq!(scroll_motion("e", true), Some(Error { older: false }));
        assert_eq!(scroll_motion("/", false), Some(Find));
        assert_eq!(scroll_motion("q", false), Some(Exit));
        assert_eq!(scroll_motion("escape", false), Some(Exit));
        assert_eq!(scroll_motion("x", false), None);
        assert_eq!(scroll_motion("enter", false), None);
    }

    #[test]
    fn nearest_steps_strictly_past_the_top_line() {
        let lines = [-90, -40, -10, 5];
        assert_eq!(
            nearest(&lines, -40, true),
            Some(-90),
            "not the line already at the top"
        );
        assert_eq!(nearest(&lines, -40, false), Some(-10));
        assert_eq!(nearest(&lines, -90, true), None);
        assert_eq!(nearest(&lines, 5, false), None);
        assert_eq!(nearest(&[], 0, true), None);
    }
}
