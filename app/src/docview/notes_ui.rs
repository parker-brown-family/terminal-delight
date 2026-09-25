//! The gpui half of a brief's notes: the buttons, the concur stamps, the note
//! box and the notes bar TD draws over a page.
//!
//! # TD's layer over the page, the page's own hidden
//!
//! The page engine hides the brief's own notes chrome before it takes the
//! picture (`extract.js`), because a picture of it would be a picture of
//! buttons that do nothing and badges that go stale. What the brief would draw
//! there, TD draws here instead, from the file's islands: a button on every
//! anchor, where the brief's own hidden button keeps its box, with the count
//! of its notes, shown while the pointer is over the anchor or always once it
//! has a note; the rule down an anchor's left edge that the brief's stylesheet
//! gives an anchor with notes; the dashed space that takes a CONCUR stamp,
//! and the stamp at the angle the brief's own notes.js would give it; and a
//! bar that counts them and copies the map.
//!
//! # What a browser shows
//!
//! The layer shows what a browser opened fresh shows from the same file (the
//! pure rules are in [`super::notes`] and held to the skill's fixtures). A
//! file with no notes island takes no notes, and the bar says so in those
//! words rather than counting nothing: read-only is not empty.
//!
//! # No handlers
//!
//! Like everything under `docview`, this registers no mouse handler. The pane
//! un-bends a press and hands it to the view, which asks [`hit`] and
//! [`NotesLayer::press`]; the bar and the note box are laid out by gpui, so
//! where they landed is recorded at paint by canvases that listen to nothing.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::OnceLock;

use gpui::{
    canvas, div, hsla, prelude::*, px, radians, rgb, svg, AnyElement, App, Bounds, ClipboardItem,
    FontWeight, Hsla, Pixels, Point, SharedString, Transformation,
};

use super::engine::{Anchor, ConcurSupport, RectCss};
use super::notes::{build_map, stamp_pose, ConcurMap, NoteMap, NotesRead};
use super::page::{place, PageToView};
use crate::theme::Theme;

/// notes.css: the brief's note button is 26 × 26 CSS px, 8 px in from its
/// anchor's top-right corner.
const BUTTON_CSS: f32 = 26.0;
const BUTTON_INSET_CSS: f32 = 8.0;
/// notes.css: the rule down the left edge of an anchor with notes.
const RULE_CSS: f32 = 3.0;
/// notes.css: a stamp overhangs its concur space by 12 CSS px on every side.
const STAMP_OVERHANG_CSS: f32 = 12.0;
/// notes.js: the stamp's ink.
const STAMP_INK: u32 = 0x35c27a;

const STAMP_SVG: &[u8] = include_bytes!("../../assets/img/concur-stamp.svg");

/// What the layer can show of a brief's notes, from the file as a browser
/// opened fresh would show them.
#[derive(Clone, Debug, PartialEq)]
pub enum Shown {
    /// The file has no notes island: it takes no notes. Read-only, which is
    /// a different thing from a brief with none.
    NoIsland,
    /// The island sits after the page's notes script, which reads it before
    /// the parser gets there: no browser ever shows what it holds.
    AfterScript,
    /// The page has an island and no notes script ran, so nothing on it is
    /// annotatable in a browser either.
    NoScript,
    /// The island is there and cannot be read.
    Unreadable(String),
    Notes {
        notes: NoteMap,
        concurs: ConcurMap,
    },
}

/// The note box: one anchor's notes, open over the page.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteBox {
    pub nid: String,
    pub title: String,
}

/// A part of the bar or the note box, as the last paint laid it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    CopyMap,
    /// Anywhere on the bar that is not a button.
    Bar,
    /// Anywhere inside the note box.
    Box,
    CloseBox,
}

/// Something the layer draws for one anchor, in the view's own flat
/// coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    /// The rule down the left edge of an anchor with notes.
    Rule { at: Bounds<Pixels> },
    /// The note button: the count on it, or none while it only shows because
    /// the pointer is over its anchor.
    Button {
        nid: String,
        title: String,
        at: Bounds<Pixels>,
        count: usize,
        open: bool,
    },
    /// An empty space on a decision that takes a CONCUR stamp.
    ConcurSpace { nid: String, at: Bounds<Pixels> },
    /// A stamp: its box before it is turned, and how far it is turned.
    Stamp {
        nid: String,
        at: Bounds<Pixels>,
        degrees: f32,
    },
}

/// What a press on the marks landed on.
#[derive(Clone, Debug, PartialEq)]
pub enum MarkHit {
    Open { nid: String, title: String },
    Concur(String),
}

/// A brief's notes, as TD shows them over its page.
pub struct NotesLayer {
    /// The page's `NOTES_FILE`, what the map's header names.
    label: String,
    shown: Shown,
    concur: ConcurSupport,
    note_box: Option<NoteBox>,
    /// The last thing the bar has to say, until something replaces it.
    said: Option<String>,
    /// Where the bar and the note box were painted, in window pixels.
    zones: Zones,
}

/// Where each part of the bar and the note box was painted, by the canvases
/// that recorded it.
type Zones = Rc<RefCell<Vec<(Bounds<Pixels>, Zone)>>>;

/// What a press on the layer did.
#[derive(Debug, PartialEq)]
pub enum LayerPress {
    /// Not the layer's: the page under it may have it.
    Pass,
    /// Taken, with nothing more for the view to do.
    Took,
}

fn rect_css(x: f32, y: f32, w: f32, h: f32) -> RectCss {
    RectCss { x, y, w, h }
}

/// Pure. The anchor a pointer is over, by its own box or its button's.
fn hot(anchor: &Anchor, map: &PageToView, at: Option<Point<Pixels>>) -> bool {
    let Some(at) = at else { return false };
    [anchor.rect, button_rect(anchor)]
        .into_iter()
        .flatten()
        .filter_map(|r| place(r, map))
        .any(|b| b.contains(&at))
}

/// The page's own (hidden) button, else where notes.css would put it.
fn button_rect(anchor: &Anchor) -> Option<RectCss> {
    anchor.button.or_else(|| {
        anchor.rect.map(|r| {
            rect_css(
                r.x + r.w - BUTTON_INSET_CSS - BUTTON_CSS,
                r.y + BUTTON_INSET_CSS,
                BUTTON_CSS,
                BUTTON_CSS,
            )
        })
    })
}

impl NotesLayer {
    /// What the page's own notes would show, read from the bytes the page
    /// was drawn from. `notes_file` and `concur` come from the brief's own
    /// notes.js as it ran in the engine; `tagged` is how many anchors it made.
    pub fn new(
        read: NotesRead,
        notes_file: Option<String>,
        concur: ConcurSupport,
        tagged: u32,
        path: &Path,
    ) -> NotesLayer {
        let label = notes_file.unwrap_or_else(|| {
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        let hidden = read.regions.island_after_script();
        let no_script = read.regions.notes_js.is_none() && tagged == 0;
        let shown = match read.notes {
            None => Shown::NoIsland,
            Some(_) if hidden => Shown::AfterScript,
            Some(_) if no_script => Shown::NoScript,
            Some(Err(e)) => Shown::Unreadable(e.0),
            // A concurs island a browser cannot read is an empty one to it.
            Some(Ok(notes)) => Shown::Notes {
                notes,
                concurs: read.concurs.unwrap_or_default(),
            },
        };
        NotesLayer {
            label,
            shown,
            concur,
            note_box: None,
            said: None,
            zones: Rc::new(RefCell::new(Vec::new())),
        }
    }

    #[cfg(test)]
    pub fn shown(&self) -> &Shown {
        &self.shown
    }

    pub fn note_box(&self) -> Option<&NoteBox> {
        self.note_box.as_ref()
    }

    fn notes(&self) -> Option<&NoteMap> {
        match &self.shown {
            Shown::Notes { notes, .. } => Some(notes),
            _ => None,
        }
    }

    /// The concurs a browser draws: none on a brief whose notes.js predates
    /// them, whatever an island holds.
    fn concurs(&self) -> Option<&ConcurMap> {
        match (&self.shown, self.concur) {
            (Shown::Notes { concurs, .. }, ConcurSupport::Supported) => Some(concurs),
            _ => None,
        }
    }

    /// The notes on one anchor.
    pub fn count_on(&self, nid: &str) -> usize {
        self.notes().map_or(0, |n| n.on(nid).len())
    }

    /// Why this page takes no notes, in words, or `None` when it shows them.
    pub fn read_only(&self) -> Option<String> {
        match &self.shown {
            Shown::NoIsland => Some("read-only · this page has no notes island".into()),
            Shown::AfterScript => Some(
                "read-only · its notes island sits after the notes script, where no browser reads it"
                    .into(),
            ),
            Shown::NoScript => Some("read-only · no notes script ran on this page".into()),
            Shown::Unreadable(why) => Some(format!("read-only · {why}")),
            Shown::Notes { .. } => None,
        }
    }

    /// The bar's counts, as a browser's notebar counts them: notes on the
    /// page's anchors, and every concur, where the brief takes concurs.
    pub fn counts(&self, anchors: &[Anchor]) -> (usize, Option<usize>) {
        let notes = anchors.iter().map(|a| self.count_on(&a.nid)).sum();
        (notes, self.concurs().map(ConcurMap::count))
    }

    /// The map notes.js's copy map would give for this page. `None` when the
    /// page shows no notes; empty of blocks when it has none yet.
    pub fn map(&self, anchors: &[Anchor]) -> Option<String> {
        let notes = self.notes()?;
        let empty = ConcurMap::default();
        let pairs: Vec<(&str, &str)> = anchors
            .iter()
            .map(|a| (a.nid.as_str(), a.title.as_str()))
            .collect();
        Some(build_map(
            &self.label,
            notes,
            self.concurs().unwrap_or(&empty),
            &pairs,
        ))
    }

    /// Whether there is anything to copy: the brief disables its own copy
    /// map until a note or a concur exists.
    fn mappable(&self, anchors: &[Anchor]) -> bool {
        let (notes, concurs) = self.counts(anchors);
        self.notes().is_some() && notes + concurs.unwrap_or(0) > 0
    }

    /// Pure. Everything the layer draws for these anchors through this
    /// mapping, and which of them the pointer lights. Nothing for an anchor
    /// with no place (inside a closed dialog), nothing scrolled out of the
    /// view, and nothing at all on a page that shows no notes.
    pub fn marks(
        &self,
        anchors: &[Anchor],
        map: &PageToView,
        pointer: Option<Point<Pixels>>,
    ) -> Vec<Mark> {
        let mut out = Vec::new();
        if self.notes().is_none() {
            return out;
        }
        for a in anchors {
            let Some(rect) = a.rect else { continue };
            let count = self.count_on(&a.nid);
            if count > 0 {
                if let Some(at) = place(rect_css(rect.x, rect.y, RULE_CSS, rect.h), map) {
                    out.push(Mark::Rule { at });
                }
            }
            if let (Some(zone), Some(concurs)) = (a.concur_zone, self.concurs()) {
                if concurs.has(&a.nid) {
                    let pose = stamp_pose(&a.nid);
                    let o = STAMP_OVERHANG_CSS;
                    let r = rect_css(
                        zone.x - o + pose.dx,
                        zone.y - o + pose.dy,
                        zone.w + 2.0 * o,
                        zone.h + 2.0 * o,
                    );
                    if let Some(at) = place(r, map) {
                        out.push(Mark::Stamp {
                            nid: a.nid.clone(),
                            at,
                            degrees: pose.degrees,
                        });
                    }
                } else if let Some(at) = place(zone, map) {
                    out.push(Mark::ConcurSpace {
                        nid: a.nid.clone(),
                        at,
                    });
                }
            }
            let open = self.note_box.as_ref().is_some_and(|b| b.nid == a.nid);
            if count > 0 || open || hot(a, map, pointer) {
                if let Some(at) = button_rect(a).and_then(|r| place(r, map)) {
                    out.push(Mark::Button {
                        nid: a.nid.clone(),
                        title: a.title.clone(),
                        at,
                        count,
                        open,
                    });
                }
            }
        }
        out
    }

    /// The nids the pointer lights: a change here is a change in what is
    /// drawn, and nothing else about a move is.
    pub fn lit(
        anchors: &[Anchor],
        map: &PageToView,
        pointer: Option<Point<Pixels>>,
    ) -> Vec<String> {
        anchors
            .iter()
            .filter(|a| hot(a, map, pointer))
            .map(|a| a.nid.clone())
            .collect()
    }

    /// Open the note box on an anchor. Read-only in this build: it lists
    /// what the file holds.
    pub fn open(&mut self, nid: String, title: String) {
        self.note_box = Some(NoteBox { nid, title });
    }

    /// Escape: the note box closes before anything else does. Answers
    /// whether it was open.
    pub fn escape(&mut self) -> bool {
        self.note_box.take().is_some()
    }

    /// A press, `at` in window pixels as the last paint laid things out.
    /// The note box first, which takes every press while it is open — one
    /// outside it puts it away, as a click on a browser dialog's backdrop
    /// does — then the bar.
    pub fn press(&mut self, at: Point<Pixels>, anchors: &[Anchor], cx: &mut App) -> LayerPress {
        let zones = self.zones.borrow().clone();
        let under = |z: Zone| zones.iter().any(|(b, k)| *k == z && b.contains(&at));
        if self.note_box.is_some() {
            if under(Zone::CloseBox) || !under(Zone::Box) {
                self.note_box = None;
            }
            return LayerPress::Took;
        }
        if under(Zone::CopyMap) {
            self.copy_map(anchors, cx);
            return LayerPress::Took;
        }
        if under(Zone::Bar) {
            return LayerPress::Took;
        }
        LayerPress::Pass
    }

    /// Put the map on the clipboard, exactly as notes.js builds it.
    pub fn copy_map(&mut self, anchors: &[Anchor], cx: &mut App) {
        if !self.mappable(anchors) {
            return;
        }
        let Some(map) = self.map(anchors) else { return };
        cx.write_to_clipboard(ClipboardItem::new_string(map.clone()));
        cx.write_to_primary(ClipboardItem::new_string(map.clone()));
        self.said = Some(format!(
            "map copied · {} characters ≈ {} tokens",
            map.chars().count(),
            map.len().div_ceil(4)
        ));
    }

    /// What the bar says, for the control socket and the tests.
    pub fn report(&self, anchors: &[Anchor]) -> serde_json::Value {
        let (notes, concurs) = self.counts(anchors);
        let state = match &self.shown {
            Shown::NoIsland => "no-island",
            Shown::AfterScript => "after-script",
            Shown::NoScript => "no-script",
            Shown::Unreadable(_) => "unreadable",
            Shown::Notes { .. } => "notes",
        };
        serde_json::json!({
            "state": state,
            "label": self.label,
            "notes": notes,
            "concurs": concurs,
            "anchors": anchors.len(),
            "read_only": self.read_only(),
            "map": self.map(anchors),
            "open": self.note_box.as_ref().map(|b| b.nid.clone()),
        })
    }

    // ── drawing ─────────────────────────────────────────────────────────────

    /// Forget where things were drawn: called at the top of every render,
    /// before the canvases below record the new places.
    pub fn clear_zones(&self) {
        self.zones.borrow_mut().clear();
    }

    /// A canvas that records where its parent was painted as `zone`.
    fn record(&self, zone: Zone) -> impl IntoElement {
        let zones = self.zones.clone();
        canvas(
            move |bounds, _, _| zones.borrow_mut().push((bounds, zone)),
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
    }

    /// The marks, drawn: under the bar and the note box.
    pub fn draw_marks(&self, marks: &[Mark], th: &Theme) -> Vec<AnyElement> {
        let accent = th.accent;
        marks
            .iter()
            .map(|m| match m {
                Mark::Rule { at } => at_rect(div(), *at).bg(accent).into_any_element(),
                Mark::Button {
                    at, count, open, ..
                } => {
                    let side = f32::from(at.size.height);
                    let (bg, fg, label): (Hsla, Hsla, SharedString) = if *count > 0 {
                        (accent, th.bg, count.to_string().into())
                    } else {
                        (th.surface, th.text.alpha(0.8), "💬".into())
                    };
                    at_rect(div(), *at)
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(side * 0.23))
                        .bg(bg)
                        .border_1()
                        .border_color(if *open { th.text } else { accent })
                        .text_color(fg)
                        .text_size(px((side * 0.5).max(8.0)))
                        .font_weight(FontWeight::BOLD)
                        .child(label)
                        .into_any_element()
                }
                Mark::ConcurSpace { at, .. } => {
                    let side = f32::from(at.size.width);
                    let ink = th.text.alpha(0.38);
                    at_rect(div(), *at)
                        .rounded(px(side * 0.117))
                        .border_1()
                        .border_dashed()
                        .border_color(ink)
                        .text_color(ink)
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .child(div().text_size(px(side * 0.38)).child("?"))
                        .child(div().text_size(px((side * 0.085).max(7.0))).child("CONCUR"))
                        .into_any_element()
                }
                Mark::Stamp { at, degrees, .. } => svg()
                    .external_path(stamp_path())
                    .absolute()
                    .left(at.origin.x)
                    .top(at.origin.y)
                    .w(at.size.width)
                    .h(at.size.height)
                    .text_color(rgb(STAMP_INK))
                    .with_transformation(Transformation::rotate(radians(degrees.to_radians())))
                    .into_any_element(),
            })
            .collect()
    }

    /// The bar, bottom-right and fixed in the view.
    pub fn draw_bar(&self, anchors: &[Anchor], th: &Theme) -> AnyElement {
        let text = th.font_size * 0.85;
        let (notes, concurs) = self.counts(anchors);
        let mut row = div()
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .px(px(10.))
            .py(px(5.))
            .rounded(px(6.))
            .bg(th.surface.alpha(0.96))
            .border_1()
            .border_color(th.accent.alpha(0.45))
            .shadow(crate::float_shadows(th.accent))
            .font_family(SharedString::from(th.font_family.clone()))
            .text_size(px(text))
            .text_color(th.text.alpha(0.8))
            .child(self.record(Zone::Bar));
        match self.read_only() {
            Some(why) => row = row.child(div().child(why)),
            None => {
                row = row.child(
                    div()
                        .flex()
                        .gap(px(4.))
                        .child(div().text_color(th.accent).child(notes.to_string()))
                        .child(if notes == 1 { "note" } else { "notes" }),
                );
                if let Some(c) = concurs {
                    row = row.child(format!(
                        "· {c} {}",
                        if c == 1 { "concur" } else { "concurs" }
                    ));
                }
                let live = self.mappable(anchors);
                row = row.child(
                    div()
                        .relative()
                        .px(px(7.))
                        .py(px(1.))
                        .rounded(px(4.))
                        .border_1()
                        .border_color(th.accent.alpha(if live { 1.0 } else { 0.3 }))
                        .text_color(th.accent.alpha(if live { 1.0 } else { 0.4 }))
                        .child("⎘ copy map")
                        .child(self.record(Zone::CopyMap)),
                );
            }
        }
        if let Some(said) = &self.said {
            row = row.child(div().text_color(th.accent).child(said.clone()));
        }
        div()
            .absolute()
            .right(px(10.))
            .bottom(px(10.))
            .max_w(px(900.))
            .child(row)
            .into_any_element()
    }

    /// The note box, over everything, when it is open: the anchor's title,
    /// its id, and the notes the file holds on it, oldest first.
    pub fn draw_box(&self, view: gpui::Size<Pixels>, th: &Theme) -> Option<AnyElement> {
        let b = self.note_box.as_ref()?;
        let notes = self.notes().map(|n| n.on(&b.nid)).unwrap_or_default();
        let (vw, vh) = (f32::from(view.width), f32::from(view.height));
        let w = (vw * 0.76).clamp(220.0_f32.min(vw), 620.0);
        let body = th.font_size;
        let mut list = div().flex().flex_col().gap(px(8.));
        if notes.is_empty() {
            list = list.child(
                div()
                    .text_color(th.text.alpha(0.6))
                    .child("No notes on this yet."),
            );
        }
        for n in notes {
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .pl(px(9.))
                    .py(px(6.))
                    .border_l_2()
                    .border_color(th.accent)
                    .bg(th.bg.alpha(0.5))
                    .child(
                        div()
                            .text_size(px(body * 0.72))
                            .text_color(th.faint)
                            .child(n.ts.unwrap_or("").to_string()),
                    )
                    .child(div().child(n.text.to_string())),
            );
        }
        let close = div()
            .relative()
            .px(px(9.))
            .py(px(2.))
            .rounded(px(4.))
            .border_1()
            .border_color(th.accent)
            .text_color(th.accent)
            .child("Close")
            .child(self.record(Zone::CloseBox));
        let panel = div()
            .absolute()
            .left(px(((vw - w) / 2.0).max(0.0)))
            .top(px((vh * 0.1).min(110.0)))
            .w(px(w))
            .max_h(px(vh * 0.8))
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(th.surface)
            .border_2()
            .border_color(th.accent)
            .rounded(px(10.))
            .shadow(crate::float_shadows(th.accent))
            .text_size(px(body))
            .text_color(th.text)
            .child(self.record(Zone::Box))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .px(px(12.))
                    .py(px(8.))
                    .border_b_1()
                    .border_color(th.faint)
                    .child(div().font_weight(FontWeight::BOLD).child(b.title.clone()))
                    .child(
                        div()
                            .text_size(px(body * 0.75))
                            .text_color(th.faint)
                            .font_family(SharedString::from(th.font_family.clone()))
                            .child(format!("#{}", b.nid)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .px(px(12.))
                    .py(px(10.))
                    .child(list)
                    .child(div().flex().child(close)),
            );
        Some(
            div()
                .absolute()
                .inset_0()
                .bg(hsla(0., 0., 0., 0.45))
                .child(panel)
                .into_any_element(),
        )
    }
}

/// Pure. What a press on the marks lands on: a button first, which the
/// brief draws above everything in its anchor, then a concur space or stamp.
pub fn hit(marks: &[Mark], at: Point<Pixels>) -> Option<MarkHit> {
    let button = marks.iter().rev().find_map(|m| match m {
        Mark::Button {
            nid, title, at: b, ..
        } if b.contains(&at) => Some(MarkHit::Open {
            nid: nid.clone(),
            title: title.clone(),
        }),
        _ => None,
    });
    button.or_else(|| {
        marks.iter().rev().find_map(|m| match m {
            Mark::ConcurSpace { nid, at: b } | Mark::Stamp { nid, at: b, .. }
                if b.contains(&at) =>
            {
                Some(MarkHit::Concur(nid.clone()))
            }
            _ => None,
        })
    })
}

fn at_rect(d: gpui::Div, at: Bounds<Pixels>) -> gpui::Div {
    d.absolute()
        .left(at.origin.x)
        .top(at.origin.y)
        .w(at.size.width)
        .h(at.size.height)
}

/// The stamp's drawing, written once to the runtime directory: gpui draws an
/// SVG from a path.
fn stamp_path() -> SharedString {
    static PATH: OnceLock<SharedString> = OnceLock::new();
    PATH.get_or_init(|| {
        crate::art::runtime_asset("terminal-delight-concur-stamp.svg", STAMP_SVG)
            .to_string_lossy()
            .into_owned()
            .into()
    })
    .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docview::notes;
    use gpui::{point, size};

    fn anchor(nid: &str, rect: Option<RectCss>, concur: bool) -> Anchor {
        Anchor {
            nid: nid.into(),
            title: format!("{nid} title"),
            tag: "div".into(),
            dialog: None,
            rect,
            button: rect.map(|r| rect_css(r.x + r.w - 34.0, r.y + 8.0, 26.0, 26.0)),
            concur_zone: (concur && rect.is_some()).then(|| {
                let r = rect.unwrap();
                rect_css(r.x + r.w - 134.0, r.y + 42.0, 120.0, 120.0)
            }),
        }
    }

    fn view() -> PageToView {
        PageToView {
            origin: point(px(0.), px(0.)),
            px_per_css: 1.0,
            scroll: px(0.),
            clip: Bounds {
                origin: point(px(0.), px(0.)),
                size: size(px(800.), px(2000.)),
            },
        }
    }

    fn layer(island: &str, concurs: &str, support: ConcurSupport) -> NotesLayer {
        let html = format!(
            "<script type=\"application/json\" id=\"report-notes\">{island}</script>\
             <script type=\"application/json\" id=\"report-concurs\">{concurs}</script>\
             <script>function tag() {{}} // reader notes</script>"
        );
        NotesLayer::new(
            notes::read(html.as_bytes()),
            Some("b.html".into()),
            support,
            3,
            Path::new("/r/b.html"),
        )
    }

    const ONE_NOTE: &str = r#"{"a":[{"text":"hello","title":"a title","ts":"2026-09-24 10:00"}]}"#;

    #[test]
    fn concur_is_offered_only_where_the_brief_offers_it() {
        let r = |y| Some(rect_css(0.0, y, 700.0, 200.0));
        let anchors = vec![anchor("a", r(0.0), false), anchor("ask-b", r(300.0), true)];
        let l = layer(ONE_NOTE, r#"{}"#, ConcurSupport::Supported);
        let marks = l.marks(&anchors, &view(), None);
        let spaces: Vec<&str> = marks
            .iter()
            .filter_map(|m| match m {
                Mark::ConcurSpace { nid, .. } => Some(nid.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            spaces,
            ["ask-b"],
            "only the decision the page made a space on"
        );
        let inside = point(px(700. - 134. + 60.), px(300. + 42. + 60.));
        assert_eq!(hit(&marks, inside), Some(MarkHit::Concur("ask-b".into())));
        assert_eq!(hit(&marks, point(px(600.), px(100.))), None);
        // A brief whose notes.js predates concurs draws no space and no
        // stamp, even with a concurs island holding one.
        let old = layer(
            ONE_NOTE,
            r#"{"ask-b":"2026-09-24 10:00"}"#,
            ConcurSupport::NotSupported,
        );
        let marks = old.marks(&anchors, &view(), None);
        assert!(!marks
            .iter()
            .any(|m| matches!(m, Mark::ConcurSpace { .. } | Mark::Stamp { .. })));
        assert_eq!(old.counts(&anchors), (1, None), "no concur count at all");
        // Where it offers them, a concurred decision shows its stamp at the
        // browser's angle instead of the empty space.
        let stamped = layer(
            ONE_NOTE,
            r#"{"ask-b":"2026-09-24 10:00"}"#,
            ConcurSupport::Supported,
        );
        let marks = stamped.marks(&anchors, &view(), None);
        let pose = notes::stamp_pose("ask-b");
        assert!(marks.iter().any(|m| matches!(m,
            Mark::Stamp { nid, degrees, at } if nid == "ask-b" && *degrees == pose.degrees
                && at.size == size(px(144.), px(144.)))));
        assert_eq!(stamped.counts(&anchors), (1, Some(1)));
    }

    #[test]
    fn an_anchor_in_a_closed_dialog_has_no_button() {
        let anchors = vec![anchor("a", None, true)];
        let l = layer(ONE_NOTE, "{}", ConcurSupport::Supported);
        let marks = l.marks(&anchors, &view(), Some(point(px(0.), px(0.))));
        assert!(marks.is_empty(), "{marks:?}");
        // Its notes still count, as they do in the browser's notebar.
        assert_eq!(l.counts(&anchors), (1, Some(0)));
    }

    /// A button shows always once its anchor has a note, and otherwise only
    /// while the pointer is over the anchor, as notes.css shows it.
    #[test]
    fn a_note_button_shows_its_count_and_otherwise_only_under_the_pointer() {
        let r = |y| Some(rect_css(10.0, y, 600.0, 100.0));
        let anchors = vec![anchor("a", r(0.0), false), anchor("b", r(200.0), false)];
        let l = layer(ONE_NOTE, "{}", ConcurSupport::NotSupported);
        let buttons = |p: Option<Point<Pixels>>| {
            l.marks(&anchors, &view(), p)
                .into_iter()
                .filter_map(|m| match m {
                    Mark::Button { nid, count, .. } => Some((nid, count)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(buttons(None), [("a".to_string(), 1)]);
        assert_eq!(
            buttons(Some(point(px(50.), px(250.)))),
            [("a".to_string(), 1), ("b".to_string(), 0)]
        );
        let marks = l.marks(&anchors, &view(), None);
        assert!(
            marks.iter().any(|m| matches!(m, Mark::Rule { .. })),
            "a has a note"
        );
        let on_a = point(px(10. + 600. - 34. + 5.), px(8. + 5.));
        assert_eq!(
            hit(&marks, on_a),
            Some(MarkHit::Open {
                nid: "a".into(),
                title: "a title".into()
            })
        );
    }

    #[test]
    fn escape_closes_the_note_box_before_anything_else() {
        let mut l = layer(ONE_NOTE, "{}", ConcurSupport::Supported);
        assert!(!l.escape(), "nothing open: Escape is not the layer's");
        l.open("a".into(), "a title".into());
        assert_eq!(l.note_box().map(|b| b.nid.as_str()), Some("a"));
        assert!(l.escape(), "the note box takes Escape");
        assert!(l.note_box().is_none());
        assert!(!l.escape());
    }

    /// No island is read-only and says so; it never reads as "0 notes".
    #[test]
    fn a_page_without_an_island_says_it_is_read_only() {
        let none = NotesLayer::new(
            notes::read(b"<html><body><p>just a page</p></body></html>"),
            None,
            ConcurSupport::Unknown,
            0,
            Path::new("/r/page.html"),
        );
        assert_eq!(none.shown(), &Shown::NoIsland);
        let why = none.read_only().expect("read-only");
        assert!(why.contains("no notes island"), "{why}");
        assert_eq!(none.map(&[]), None, "nothing to map, not an empty map");
        assert!(none
            .marks(
                &[anchor("a", Some(rect_css(0., 0., 9., 9.)), true)],
                &view(),
                None
            )
            .is_empty());
        assert_eq!(none.report(&[])["state"], "no-island");
        assert_eq!(none.report(&[])["label"], "page.html");
        // An island whose script never ran is read-only too, and so is one
        // the browser cannot read.
        let unread = layer("{\"a\":", "{}", ConcurSupport::Supported);
        assert!(matches!(unread.shown(), Shown::Unreadable(_)));
        assert!(unread.read_only().is_some());
        let script_less = NotesLayer::new(
            notes::read(b"<script id=\"report-notes\">{}</script>"),
            None,
            ConcurSupport::Unknown,
            0,
            Path::new("/r/p.html"),
        );
        assert_eq!(script_less.shown(), &Shown::NoScript);
    }
}
