//! The video backend: a file played by libmpv on threads of its own
//! ([`super::mpv`]), each frame handed to gpui as a picture, and a bar along
//! the bottom that pauses, seeks and mutes it.
//!
//! # A frame is a picture that is replaced
//!
//! Every frame the render thread draws arrives here as bytes in gpui's own
//! order and becomes a `RenderImage`, drawn the way the image backend draws
//! its one. What is different is that it is replaced thirty or sixty times a
//! second, and every one of those is a texture in the window's atlas. So the
//! view holds two: the frame on screen and the one before it, which the last
//! presented frame may still be showing. When a third arrives, the oldest is
//! given back to every window's atlas at once. Closing the square gives back
//! both and stops the player.
//!
//! # Drawn at the size it is shown
//!
//! mpv scales, so the frame is drawn at the size the square shows it, in
//! device pixels, and never above the picture's own size: a 4K file in a
//! square three hundred pixels wide costs a frame three hundred pixels wide.
//! A picture smaller than its box is stretched by the GPU instead, which is
//! free. The view says the size each paint, and the render thread draws the
//! current frame again at a new one, so a paused video follows a resize.
//!
//! # The bar
//!
//! ▶ or ❚❚, the time, a track to press or drag, and the sound. Laid out from
//! the constants below and hit-tested from the same constants by
//! [`bar_hit`], because nothing in a document view listens for the mouse
//! (see `docview.rs`): the pane un-bends the pointer and hands a flat point
//! to [`Backend::press`]. A press on the picture pauses or plays it, as in
//! every player. Space, ←/→ and M do the same on the Document face; over the
//! terminal the square leaves every key but Escape to the prompt beside it.

use std::any::Any;
use std::path::Path;
use std::sync::Arc;

use futures::StreamExt;
use gpui::{
    div, img, prelude::*, px, AnyElement, App, Context, ImageSource, Keystroke, ObjectFit, Pixels,
    Point, RenderImage, Size, Task, Window,
};

use super::backend::{Backend, Drawn};
use super::mpv::{Command, Player, Status};
use super::DocumentView;
use crate::theme::Theme;

/// The bar's height, in logical pixels.
pub const BAR_H: f32 = 24.0;
/// ▶ / ❚❚ at the bar's left.
const PLAY_W: f32 = 30.0;
/// The time, `1:02 / 3:45`, after it.
const TIME_W: f32 = 92.0;
/// The sound, at the bar's right.
const SOUND_W: f32 = 56.0;
/// The track's line stops this far short of its neighbours.
const TRACK_PAD: f32 = 8.0;
/// Below this width the time is left out and the track takes its room.
const TIME_FROM_W: f32 = 240.0;
/// How far ← and → move, in seconds.
const STEP_S: f64 = 5.0;

/// Where the bar puts each of its parts, as `(left, right)` in the view's
/// logical pixels.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Bar {
    pub play: (f32, f32),
    pub time: Option<(f32, f32)>,
    /// The whole pressable span of the track.
    pub track: (f32, f32),
    pub sound: (f32, f32),
}

impl Bar {
    /// The line the track draws, inside its pressable span.
    pub fn line(&self) -> (f32, f32) {
        let (l, r) = self.track;
        (l + TRACK_PAD, (r - TRACK_PAD).max(l + TRACK_PAD))
    }

    /// The fraction of the whole a point along the track stands for, clamped
    /// to the ends, so a drag past either one holds there.
    pub fn fraction_at(&self, x: f32) -> f64 {
        let (l, r) = self.line();
        if r <= l {
            return 0.0;
        }
        f64::from(((x - l) / (r - l)).clamp(0.0, 1.0))
    }
}

/// The bar for a view `width` logical pixels wide.
pub fn bar_layout(width: f32) -> Bar {
    let w = width.max(0.0);
    let sound = ((w - SOUND_W).max(PLAY_W), w.max(PLAY_W));
    let time = (w >= TIME_FROM_W).then_some((PLAY_W, PLAY_W + TIME_W));
    let left = time.map_or(PLAY_W, |t| t.1);
    Bar {
        play: (0.0, PLAY_W),
        time,
        track: (left, sound.0.max(left)),
        sound,
    }
}

/// What a flat, view-local point is on.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Hit {
    /// Above the bar.
    Picture,
    PlayPause,
    Time,
    /// The track, at this fraction of the whole.
    Track(f64),
    Sound,
}

/// What `(x, y)` is on, in a view of `view` logical pixels. `None` outside it.
pub fn bar_hit(view: (f32, f32), x: f32, y: f32) -> Option<Hit> {
    let (w, h) = view;
    if !(0.0..w).contains(&x) || !(0.0..h).contains(&y) {
        return None;
    }
    if y < h - BAR_H {
        return Some(Hit::Picture);
    }
    let bar = bar_layout(w);
    let within = |(l, r): (f32, f32)| (l..r).contains(&x);
    Some(if within(bar.play) {
        Hit::PlayPause
    } else if within(bar.sound) {
        Hit::Sound
    } else if bar.time.is_some_and(within) {
        Hit::Time
    } else {
        Hit::Track(bar.fraction_at(x))
    })
}

/// The picture's box inside an area of `area` logical pixels: as large as
/// fits with its shape kept, centred. `(x, y, w, h)`.
pub fn picture_rect(picture: (u32, u32), area: (f32, f32)) -> (f32, f32, f32, f32) {
    let (pw, ph) = (picture.0.max(1) as f32, picture.1.max(1) as f32);
    let (aw, ah) = (area.0.max(0.0), area.1.max(0.0));
    let s = (aw / pw).min(ah / ph);
    let (w, h) = (pw * s, ph * s);
    ((aw - w) / 2.0, (ah - h) / 2.0, w, h)
}

/// The size to draw frames at, in device pixels: the box [`picture_rect`]
/// gives, never above the picture's own size. `None` for an area with no
/// room in it, where nothing is worth drawing.
pub fn draw_size(picture: (u32, u32), area: (f32, f32), scale: f32) -> Option<(u32, u32)> {
    let (_, _, w, h) = picture_rect(picture, area);
    if w < 1.0 || h < 1.0 {
        return None;
    }
    let s = (w * scale.max(0.1) / picture.0.max(1) as f32).min(1.0);
    Some((
        ((picture.0 as f32 * s).round() as u32).max(1),
        ((picture.1 as f32 * s).round() as u32).max(1),
    ))
}

/// Seconds as a clock: `0:07`, `12:34`, `1:02:03`. Unknown is a dash, never
/// `0:00`: a video whose length nobody knows is not a video with no length.
pub fn clock(secs: Option<f64>) -> String {
    let Some(s) = secs.filter(|s| s.is_finite() && *s >= 0.0) else {
        return "–:––".into();
    };
    let s = s.floor() as u64;
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// How far through, `0.0..=1.0`, when both ends are known. `None` otherwise,
/// and the track then draws no progress at all.
pub fn progress(s: &Status) -> Option<f64> {
    let (p, d) = (s.position?, s.duration?);
    (d > 0.0 && p.is_finite()).then(|| (p / d).clamp(0.0, 1.0))
}

/// A frame from the render thread as a picture gpui can draw.
fn picture_of(frame: super::mpv::Frame) -> Option<Arc<RenderImage>> {
    let buf = image::RgbaImage::from_raw(frame.width, frame.height, frame.bgra)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buf)])))
}

pub struct VideoDoc {
    /// The player, or why there is none. A player that fails after starting
    /// says so in `status.error` instead.
    player: Result<Player, String>,
    /// Where playback is, as last read from the player.
    status: Status,
    /// The frame on screen.
    shown: Option<Arc<RenderImage>>,
    /// The frame before it, which the last presented frame may still show.
    before: Option<Arc<RenderImage>>,
    /// The size frames were last asked for, in device pixels.
    asked: Option<(u32, u32)>,
    /// A press on the track is held: a drag seeks.
    scrubbing: bool,
    /// Whether a frame has been drawn yet, for `TD_DOCDEBUG`'s one line.
    drawn: bool,
    _pump: Task<()>,
}

impl VideoDoc {
    /// Start the video playing. The player wakes the view through a channel
    /// whenever it leaves a frame or a change of status; the task that
    /// listens is the view's, so it ends when the view does.
    pub fn open(path: &Path, cx: &mut Context<DocumentView>) -> Self {
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<()>();
        let player = Player::start(
            path,
            Arc::new(move || {
                let _ = tx.unbounded_send(());
            }),
        );
        let pump = cx.spawn(async move |this, cx| {
            while rx.next().await.is_some() {
                let alive = this.update(cx, |view, cx| {
                    let moved = view
                        .backend
                        .downcast_mut::<VideoDoc>()
                        .is_some_and(|doc| doc.pull(cx));
                    if moved {
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        Self {
            player,
            status: Status::default(),
            shown: None,
            before: None,
            asked: None,
            scrubbing: false,
            drawn: false,
            _pump: pump,
        }
    }

    /// Take what the player left: the newest frame, and the status. Answers
    /// whether anything drawn changed.
    fn pull(&mut self, cx: &mut App) -> bool {
        let Ok(player) = &self.player else {
            return false;
        };
        let mut changed = false;
        let status = player.status();
        if status != self.status {
            self.status = status;
            changed = true;
        }
        if let Some(picture) = player.take_frame().and_then(picture_of) {
            // Through the page's door, deferred to the end of the effect
            // cycle: a window busy with its own event is out of gpui's list
            // while it is, and a drop made then would miss its atlas.
            if let Some(oldest) = self.before.take() {
                super::page::give_back(vec![oldest], cx);
            }
            self.before = self.shown.replace(picture);
            changed = true;
        }
        changed
    }

    fn run(&self, cmd: Command) {
        if let Ok(player) = &self.player {
            player.command(cmd);
        }
    }

    /// Frames at the size this paint shows them. `view` is the whole view,
    /// bar included.
    fn ask_size(&mut self, view: Size<Pixels>, scale: f32) {
        let (Ok(player), Some(picture)) = (&self.player, self.status.picture()) else {
            return;
        };
        let area = (
            f32::from(view.width),
            (f32::from(view.height) - BAR_H).max(0.0),
        );
        if let Some(size) = draw_size(picture, area, scale) {
            if self.asked != Some(size) {
                self.asked = Some(size);
                player.size(size.0, size.1);
            }
        }
    }

    fn bar_el(&self, width: f32, th: &Theme) -> AnyElement {
        let bar = bar_layout(width);
        let acc = th.accent;
        let slot = |(l, r): (f32, f32), label: String| {
            div()
                .absolute()
                .top_0()
                .left(px(l))
                .w(px((r - l).max(0.0)))
                .h(px(BAR_H))
                .flex()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(label)
        };
        let play = match self.status.paused {
            Some(true) => "▶",
            Some(false) => "❚❚",
            None => "…",
        };
        let sound = match self.status.muted {
            Some(true) => "muted",
            Some(false) => "sound",
            None => "…",
        };
        let (l, r) = bar.line();
        let mid = BAR_H / 2.0;
        let mut track = div()
            .absolute()
            .left(px(l))
            .top(px(mid - 1.0))
            .w(px((r - l).max(0.0)))
            .h(px(2.0))
            .rounded(px(1.0))
            .bg(acc.alpha(0.25));
        if let Some(f) = progress(&self.status) {
            let at = (r - l).max(0.0) * f as f32;
            track = track
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .h(px(2.0))
                        .w(px(at))
                        .rounded(px(1.0))
                        .bg(acc),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(at - 4.0))
                        .top(px(-3.0))
                        .size(px(8.0))
                        .rounded(px(4.0))
                        .bg(acc),
                );
        }
        let mut el = div()
            .absolute()
            .left_0()
            .bottom_0()
            .w(px(width))
            .h(px(BAR_H))
            .border_t_1()
            .border_color(acc.alpha(0.4))
            .text_size(px(11.))
            .text_color(acc)
            .child(slot(bar.play, play.into()))
            .child(track)
            .child(slot(bar.sound, sound.into()));
        if let Some(t) = bar.time {
            el = el.child(slot(
                t,
                format!(
                    "{} / {}",
                    clock(self.status.position),
                    clock(self.status.duration)
                ),
            ));
        }
        el.into_any_element()
    }

    pub fn element(&mut self, view: &Drawn, th: &Theme) -> AnyElement {
        let note = |s: String| {
            div()
                .p(px(14.))
                .text_color(th.text.alpha(0.75))
                .child(s)
                .into_any_element()
        };
        if let Err(why) = &self.player {
            return note(why.clone());
        }
        if let Some(why) = &self.status.error {
            return note(why.clone());
        }
        let Some((size, scale)) = view.frame else {
            return note("opening…".into());
        };
        self.ask_size(size, scale);
        let (w, h) = (f32::from(size.width), f32::from(size.height));
        let area = (w, (h - BAR_H).max(0.0));
        let picture: AnyElement = match (&self.shown, self.status.picture()) {
            (Some(frame), Some(shape)) => {
                if std::env::var_os("TD_DOCDEBUG").is_some() && !self.drawn {
                    let f = frame.size(0);
                    eprintln!(
                        "[doc] drew {} {}x{} as {}x{}",
                        view.path.display(),
                        shape.0,
                        shape.1,
                        f.width.0,
                        f.height.0
                    );
                }
                self.drawn = true;
                let (x, y, pw, ph) = picture_rect(shape, area);
                let snap = |v: f32| (v * scale).round() / scale.max(0.1);
                img(ImageSource::Render(frame.clone()))
                    .absolute()
                    .left(px(snap(x)))
                    .top(px(snap(y)))
                    .w(px(pw))
                    .h(px(ph))
                    .object_fit(ObjectFit::Fill)
                    .into_any_element()
            }
            (_, None) if self.status.loaded => note("This file has sound but no picture.".into()),
            _ => note("opening…".into()),
        };
        div()
            .size_full()
            .relative()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .w(px(area.0))
                    .h(px(area.1))
                    .child(picture),
            )
            .child(self.bar_el(w, th))
            .into_any_element()
    }

    /// Give every frame's texture back and stop the player. The player
    /// tears itself down on its own thread; nothing here waits for it.
    pub fn release(&mut self, path: &Path, cx: &mut App) {
        for frame in [self.shown.take(), self.before.take()]
            .into_iter()
            .flatten()
        {
            cx.drop_image(frame, None);
        }
        self.player = Err("closed".into());
        self._pump = Task::ready(());
        if std::env::var_os("TD_DOCDEBUG").is_some() {
            eprintln!("[doc] released {}", path.display());
        }
    }
}

// A video plays, pauses, seeks and mutes, and gives its textures back. It
// has no zoom (the strip draws none), no scroll to save, no fragment, no
// file to follow — a file that changes under a playing video is mpv's to
// notice — and no notes, so those are the trait's defaults.
impl Backend for VideoDoc {
    fn element(&mut self, view: &Drawn, _window: &mut Window, th: &Theme) -> AnyElement {
        VideoDoc::element(self, view, th)
    }

    fn give_back(&mut self, path: &Path, cx: &mut App) {
        self.release(path, cx);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn press(&mut self, at: Point<Pixels>, view: &Drawn, _cx: &mut Context<DocumentView>) -> bool {
        let Some((size, _)) = view.frame else {
            return false;
        };
        let wh = (f32::from(size.width), f32::from(size.height));
        let Some(hit) = bar_hit(wh, f32::from(at.x), f32::from(at.y)) else {
            return false;
        };
        match hit {
            Hit::Picture | Hit::PlayPause => self.run(Command::TogglePause),
            Hit::Sound => self.run(Command::ToggleMute),
            Hit::Track(f) => {
                self.run(Command::SeekTo(f));
                self.scrubbing = true;
            }
            Hit::Time => {}
        }
        true
    }

    fn drag(&mut self, at: Point<Pixels>, view: Size<Pixels>, _sf: f32) -> bool {
        if self.scrubbing {
            let f = bar_layout(f32::from(view.width)).fraction_at(f32::from(at.x));
            self.run(Command::SeekTo(f));
        }
        false
    }

    fn end_press(&mut self) {
        self.scrubbing = false;
    }

    /// Put back by a restart: paused, so nothing plays or sounds by itself.
    /// Sent before the file has loaded, it loads paused, on its first frame.
    fn hold(&mut self) {
        self.run(Command::Hold);
    }

    /// Space, ← and →, and M, on the Document face. A floating square takes
    /// none of them: over the terminal they belong to the prompt.
    fn key(&mut self, ks: &Keystroke, floating: bool, _cx: &mut Context<DocumentView>) -> bool {
        let m = &ks.modifiers;
        if floating || m.control || m.alt || m.platform || m.shift {
            return false;
        }
        let cmd = match ks.key.as_str() {
            "space" => Command::TogglePause,
            "left" => Command::SeekBy(-STEP_S),
            "right" => Command::SeekBy(STEP_S),
            "m" => Command::ToggleMute,
            _ => return false,
        };
        self.run(cmd);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wide bar has every part, left to right, touching and inside the
    /// view; a narrow one drops the time and gives its room to the track.
    #[test]
    fn the_bar_lays_out_left_to_right_and_drops_the_time_when_narrow() {
        let wide = bar_layout(400.0);
        let t = wide.time.expect("room for the time");
        assert_eq!(wide.play, (0.0, PLAY_W));
        assert_eq!(t.0, wide.play.1);
        assert_eq!(wide.track.0, t.1);
        assert_eq!(wide.track.1, wide.sound.0);
        assert_eq!(wide.sound.1, 400.0);
        let narrow = bar_layout(180.0);
        assert_eq!(narrow.time, None);
        assert_eq!(narrow.track.0, PLAY_W);
    }

    /// Every part of the bar answers a press as what it draws, and the
    /// picture above it pauses.
    #[test]
    fn a_press_lands_on_what_the_bar_draws_there() {
        let view = (400.0, 300.0);
        let y = 300.0 - BAR_H / 2.0;
        assert_eq!(bar_hit(view, 200.0, 100.0), Some(Hit::Picture));
        assert_eq!(bar_hit(view, 10.0, y), Some(Hit::PlayPause));
        assert_eq!(bar_hit(view, 60.0, y), Some(Hit::Time));
        assert_eq!(bar_hit(view, 390.0, y), Some(Hit::Sound));
        let bar = bar_layout(400.0);
        let (l, r) = bar.line();
        match bar_hit(view, (l + r) / 2.0, y) {
            Some(Hit::Track(f)) => assert!((f - 0.5).abs() < 1e-6, "{f}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(bar_hit(view, -1.0, y), None);
        assert_eq!(bar_hit(view, 10.0, 300.0), None);
    }

    /// A drag past either end of the track holds at that end.
    #[test]
    fn the_track_clamps_at_its_ends() {
        let bar = bar_layout(400.0);
        assert_eq!(bar.fraction_at(-50.0), 0.0);
        assert_eq!(bar.fraction_at(5000.0), 1.0);
    }

    /// The picture keeps its shape and sits in the middle of the room it has.
    #[test]
    fn a_picture_is_fitted_and_centred() {
        let (x, y, w, h) = picture_rect((1920, 1080), (400.0, 400.0));
        assert_eq!((x, w), (0.0, 400.0));
        assert!((h - 225.0).abs() < 1e-3);
        assert!((y - 87.5).abs() < 1e-3);
    }

    /// Frames are drawn at the size shown, in device pixels, and never above
    /// the picture's own: a small clip in a large pane is stretched by the
    /// GPU, not by mpv.
    #[test]
    fn frames_are_drawn_at_the_size_shown_and_never_larger_than_the_file() {
        assert_eq!(
            draw_size((1920, 1080), (400.0, 400.0), 2.0),
            Some((800, 450))
        );
        assert_eq!(
            draw_size((320, 180), (1600.0, 900.0), 2.0),
            Some((320, 180))
        );
        assert_eq!(draw_size((1920, 1080), (0.0, 400.0), 2.0), None);
    }

    /// Unknown time is a dash, not zero, and the track draws no progress
    /// until both ends of it are known.
    #[test]
    fn unknown_time_is_a_dash_and_no_progress() {
        assert_eq!(clock(None), "–:––");
        assert_eq!(clock(Some(7.9)), "0:07");
        assert_eq!(clock(Some(754.0)), "12:34");
        assert_eq!(clock(Some(3723.0)), "1:02:03");
        let mut s = Status {
            position: Some(5.0),
            ..Status::default()
        };
        assert_eq!(progress(&s), None, "no duration, no progress");
        s.duration = Some(20.0);
        assert_eq!(progress(&s), Some(0.25));
        s.duration = Some(0.0);
        assert_eq!(progress(&s), None);
    }
}
