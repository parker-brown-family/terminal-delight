//! A brief's sections, on the progress rail: a pip where each one starts, and
//! the list of them that opens while the pointer is on the view's right edge.
//!
//! # What this replaced
//!
//! A brief's own page carries a floating sections panel, which the snapshot
//! hides because a picture of a panel cannot be pressed (`extract.js`). The
//! first native stand-in was a fixed panel over the page: a column down the
//! left at 1,100 CSS px or wider, two rows across the top below that. It could
//! not be folded or moved, and it sat on the title. Parker, 2026-11-18: *"the
//! sections become permanently visible... garbage UX"*, and then, of a drawing
//! of this: *"we already have PIPS all the way down the screen.. so the SECTION
//! denoting pips are CLICKABLE AND SHOULD LOOK CLICKABLE --- maybe 2x wider,
//! looks a bit more like a button and the accent / different colour with the
//! little § 01 - 02 - 03"*.
//!
//! So nothing sits on the page at rest. Each `h2` section has a pip on the
//! rail at the height its start is at in the document, labelled `§` and its
//! number; a press on one goes there. The list, with every passage and its
//! visible time, opens only while the pointer is within [`EDGE`] of the right
//! edge or on the list itself.
//!
//! # What is here
//!
//! The arithmetic, pure, so it is tested without a window: which sections get
//! a pip and where ([`pips`]), what a pip says ([`label`]), where the list
//! goes and how many rows it holds ([`list`]), and whether the pointer holds
//! it open ([`opens`]). [`super::page`] draws them and takes the presses.

use super::engine::ReadingTarget;
use super::progress;

/// A pip: wide enough for `§` and four characters at the chrome's small size.
pub const PIP_W: f32 = 40.0;
pub const PIP_H: f32 = 16.0;
/// Between two pips that would otherwise touch.
const PIP_GAP: f32 = 2.0;
/// The percentage sits in the rail's top corner; the first pip starts under it.
const UNDER_PERCENT: f32 = 26.0;
/// How far in from the view's right edge the pointer opens the list: the
/// pips, and a little more so the pointer need not be exact.
pub const EDGE: f32 = progress::INSET + PIP_W + 8.0;

/// The list's widest, its heading's height, one row's height, and where its
/// top is: under the percentage, like the first pip.
const LIST_W: f32 = 264.0;
pub const LIST_HEAD: f32 = 28.0;
pub const LIST_ROW: f32 = 36.0;
const LIST_TOP: f32 = UNDER_PERCENT;
/// Kept clear under the list for the notes bar and its hint.
const LIST_FOOT: f32 = 78.0;

/// One section's pip.
#[derive(Clone, PartialEq, Debug)]
pub struct Pip {
    pub anchor: String,
    /// What it says after the `§`.
    pub label: String,
    /// The pip's top, view-local.
    pub top: f32,
}

/// Pure. What a section's pip says: the number its own heading opens with
/// (`01`, `7`, `01b`), or its place among the sections when the heading has
/// none. `ordinal` counts from one.
pub fn label(title: &str, ordinal: usize) -> String {
    let word = title.split_whitespace().next().unwrap_or("");
    let digits = word.chars().take_while(char::is_ascii_digit).count();
    let rest = &word[digits..];
    let lettered = rest.len() == 1 && rest.chars().all(|c| c.is_ascii_lowercase());
    let dotted = matches!(rest, "." | ":" | ")");
    if (1..=3).contains(&digits) && (rest.is_empty() || lettered || dotted) {
        let end = if lettered { digits + 1 } else { digits };
        word[..end].to_string()
    } else {
        format!("{ordinal:02}")
    }
}

/// Pure. The pips for a page `height_css` tall, on the rail of a view
/// `view_h` tall: one per `section` that has been laid out, at the height its
/// start is at in the document, then moved only as far as it takes for no two
/// to overlap and none to sit under the percentage, or under the chrome in
/// the view's bottom corner: `floor` is the top of the notes bar and its cue
/// where they are drawn, and a pip under them could be neither read nor
/// pressed.
///
/// A section with no box has no place on the rail, and gets no pip: it is
/// still in the list. A view with no rail has no pips. When there are more
/// sections than the rail can hold apart, every `k`th one gets a pip, so what
/// is drawn can always be read and pressed.
pub fn pips(
    targets: &[ReadingTarget],
    height_css: f32,
    view_h: f32,
    floor: Option<f32>,
) -> Vec<Pip> {
    if !(height_css.is_finite() && height_css > 0.0) {
        return Vec::new();
    }
    let sections = targets.iter().filter(|t| t.kind == "section").enumerate();
    let mut all: Vec<Pip> = sections
        .filter_map(|(i, t)| {
            let y = progress::y_at(t.rect?.y / height_css, view_h)?;
            Some(Pip {
                anchor: t.anchor.clone(),
                label: label(&t.title, i + 1),
                top: y - PIP_H / 2.0,
            })
        })
        .collect();
    let Some(end) = progress::y_at(1.0, view_h) else {
        return Vec::new();
    };
    let last = floor.map_or(end, |f| f.min(end)) - PIP_H;
    let pitch = PIP_H + PIP_GAP;
    let room = (((last - UNDER_PERCENT) / pitch).floor().max(0.0) as usize) + 1;
    if all.len() > room {
        let every = all.len().div_ceil(room);
        all = all.into_iter().step_by(every).collect();
    }
    // Down, so none overlaps the one above or the percentage; then up from the
    // bottom, so the pushing has not run the last ones off the rail.
    let mut least = UNDER_PERCENT;
    for p in &mut all {
        p.top = p.top.max(least);
        least = p.top + pitch;
    }
    let mut ceiling = last;
    for p in all.iter_mut().rev() {
        p.top = p.top.min(ceiling);
        ceiling = p.top - pitch;
    }
    all
}

/// Where the open list is, view-local, and which rows it shows.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct List {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// The first target shown, and how many.
    pub start: usize,
    pub rows: usize,
}

/// Pure. The list for `targets` passages in a view `view_w` by `view_h`,
/// flush against the edge strip so the pointer crosses from one to the other
/// without leaving both. It shows as many rows as fit and keeps the passage
/// being read (`active`) in the middle, unless the wheel has moved it
/// (`offset`). `None` with nothing to list.
pub fn list(
    view_w: f32,
    view_h: f32,
    targets: usize,
    active: usize,
    offset: Option<usize>,
) -> Option<List> {
    if targets == 0 {
        return None;
    }
    let fit = ((view_h - LIST_TOP - LIST_HEAD - LIST_FOOT) / LIST_ROW)
        .floor()
        .max(1.0) as usize;
    let rows = fit.min(targets);
    let last_start = targets - rows;
    let start = offset
        .unwrap_or(active.saturating_sub(rows / 2))
        .min(last_start);
    let w = LIST_W.min(view_w - EDGE - 12.0).max(1.0);
    Some(List {
        x: view_w - EDGE - w,
        y: LIST_TOP,
        w,
        h: LIST_HEAD + rows as f32 * LIST_ROW,
        start,
        rows,
    })
}

/// Pure. Whether a pointer at `at` (view-local; `None` when it is elsewhere)
/// holds the list open: it is on the edge strip, or on the list as it was
/// last drawn (`open`, `None` while the list is shut).
pub fn opens(at: Option<(f32, f32)>, view_w: f32, open: Option<List>) -> bool {
    let Some((x, y)) = at else { return false };
    let on_edge = x >= view_w - EDGE && x <= view_w;
    let on_list = open.is_some_and(|l| x >= l.x && x <= l.x + l.w && y >= l.y && y <= l.y + l.h);
    on_edge || on_list
}

#[cfg(test)]
mod tests {
    use super::super::engine::RectCss;
    use super::*;

    fn target(kind: &str, title: &str, y: Option<f32>) -> ReadingTarget {
        ReadingTarget {
            anchor: title.to_lowercase().replace(' ', "-"),
            title: title.into(),
            kind: kind.into(),
            parent: None,
            words: None,
            rect: y.map(|y| RectCss {
                x: 40.0,
                y,
                w: 800.0,
                h: 300.0,
            }),
        }
    }

    /// A pip says the number the heading itself opens with, so `§01b` on the
    /// rail is the section the page calls 01b; a heading with no number gets
    /// its place among the sections.
    #[test]
    fn a_pip_says_the_headings_own_number_or_its_place() {
        assert_eq!(label("01 The expensive part", 1), "01");
        assert_eq!(label("01b Which practice a person may act for", 3), "01b");
        assert_eq!(label("7. Slices", 2), "7");
        assert_eq!(label("12: Open questions", 9), "12");
        assert_eq!(label("Standing: trial, active", 4), "04");
        assert_eq!(label("2026 in review", 5), "05", "a year is not a number");
        assert_eq!(label("3D printing", 6), "06");
        assert_eq!(label("", 11), "11");
    }

    /// Only an `h2` section gets a pip, at the height its start is at in the
    /// document; a figure or an entry is in the list alone.
    #[test]
    fn sections_get_pips_where_they_start_in_the_document() {
        let targets = vec![
            target("opening", "Top", Some(0.0)),
            target("section", "01 First", Some(2000.0)),
            target("figure", "01 · A drawing", Some(2200.0)),
            target("section", "02 Second", Some(5000.0)),
            target("section", "03 Third", Some(9000.0)),
        ];
        let got = pips(&targets, 10_000.0, 1012.0, None);
        assert_eq!(
            got.iter().map(|p| p.label.as_str()).collect::<Vec<_>>(),
            ["01", "02", "03"]
        );
        // The rail is 1,000 tall from 6: a fifth of the way down is 206.
        for (pip, want) in got.iter().zip([206.0, 506.0, 906.0]) {
            assert!(
                (pip.top + PIP_H / 2.0 - want).abs() < 0.01,
                "{pip:?} at {want}"
            );
        }
    }

    /// A section nobody laid out has no place on the rail, which is not the
    /// top of it: no pip, and its number is still its own.
    #[test]
    fn a_section_with_no_box_has_no_pip() {
        let targets = vec![
            target("section", "First", Some(1000.0)),
            target("section", "Second", None),
            target("section", "Third", Some(6000.0)),
        ];
        let got = pips(&targets, 10_000.0, 1012.0, None);
        assert_eq!(
            got.iter().map(|p| p.label.as_str()).collect::<Vec<_>>(),
            ["01", "03"]
        );
        assert!(
            pips(&targets, 0.0, 1012.0, None).is_empty(),
            "no height measured"
        );
        assert!(
            pips(&targets, 10_000.0, 40.0, None).is_empty(),
            "no rail to sit on"
        );
    }

    /// Sections that start close together keep their order and their pips
    /// apart, clear of the percentage at the top and inside the rail at the
    /// bottom.
    #[test]
    fn crowded_pips_are_moved_apart_and_stay_on_the_rail() {
        let targets: Vec<_> = [0.0, 10.0, 20.0, 9980.0, 9990.0, 9999.0]
            .iter()
            .enumerate()
            .map(|(i, y)| target("section", &format!("0{} s", i + 1), Some(*y)))
            .collect();
        let got = pips(&targets, 10_000.0, 1012.0, None);
        assert_eq!(got.len(), 6);
        assert!(got[0].top >= UNDER_PERCENT);
        assert!(got[5].top + PIP_H <= 1006.0 + 0.01, "{:?}", got[5]);
        for pair in got.windows(2) {
            assert!(
                pair[1].top - pair[0].top >= PIP_H + PIP_GAP - 0.01,
                "{pair:?}"
            );
        }
    }

    /// The notes bar and its cue sit in the view's bottom corner, over the end
    /// of the rail. The last sections' pips stop above them, still in order.
    #[test]
    fn pips_stop_above_the_chrome_in_the_bottom_corner() {
        let targets: Vec<_> = [1000.0, 8000.0, 9000.0, 9800.0]
            .iter()
            .enumerate()
            .map(|(i, y)| target("section", &format!("0{} s", i + 1), Some(*y)))
            .collect();
        let free = pips(&targets, 10_000.0, 1012.0, None);
        let got = pips(&targets, 10_000.0, 1012.0, Some(880.0));
        assert_eq!(got[0], free[0], "one clear of the corner stays put");
        assert!(free[3].top > 880.0, "it would have been under the bar");
        assert!(got[3].top + PIP_H <= 880.0, "{:?}", got[3]);
        for pair in got.windows(2) {
            assert!(pair[1].top - pair[0].top >= PIP_H + PIP_GAP - 0.01);
        }
    }

    /// More sections than the rail holds apart: every kth gets a pip, and
    /// those that are drawn do not overlap.
    #[test]
    fn more_sections_than_fit_are_thinned() {
        let targets: Vec<_> = (0..200)
            .map(|i| target("section", &format!("{i} s"), Some(i as f32 * 50.0)))
            .collect();
        let got = pips(&targets, 10_000.0, 512.0, None);
        assert!(!got.is_empty() && got.len() < 200, "{}", got.len());
        assert_eq!(got[0].label, "0");
        for pair in got.windows(2) {
            assert!(pair[1].top - pair[0].top >= PIP_H + PIP_GAP - 0.01);
        }
        assert!(got.last().unwrap().top + PIP_H <= 506.01);
    }

    /// The list sits against the edge strip, shows what fits, and keeps the
    /// passage being read in the middle until the wheel says otherwise.
    #[test]
    fn the_list_sits_against_the_edge_and_follows_the_reader() {
        let l = list(1200.0, 900.0, 30, 15, None).unwrap();
        assert_eq!(l.x + l.w, 1200.0 - EDGE);
        assert_eq!(l.rows, 21);
        assert_eq!(l.start, 5, "the active row in the middle");
        assert_eq!(list(1200.0, 900.0, 30, 0, None).unwrap().start, 0);
        assert_eq!(list(1200.0, 900.0, 30, 29, None).unwrap().start, 9);
        assert_eq!(list(1200.0, 900.0, 30, 15, Some(2)).unwrap().start, 2);
        assert_eq!(list(1200.0, 900.0, 30, 15, Some(99)).unwrap().start, 9);
        // A short brief shows whole, a narrow view narrows the list, and a
        // brief with nothing to list has no list.
        let short = list(1200.0, 900.0, 4, 3, None).unwrap();
        assert_eq!((short.start, short.rows), (0, 4));
        assert!(list(200.0, 900.0, 4, 0, None).unwrap().w < 160.0);
        assert_eq!(list(1200.0, 900.0, 0, 0, None), None);
    }

    /// The pointer opens the list from the edge strip, keeps it open while on
    /// the list, and shuts it anywhere else; a list that is shut is not there
    /// to be on.
    #[test]
    fn the_pointer_opens_the_list_from_the_edge_and_holds_it_from_the_list() {
        let l = list(1200.0, 900.0, 12, 0, None);
        let on_list = Some((1000.0, 100.0));
        assert!(opens(Some((1195.0, 400.0)), 1200.0, None));
        assert!(opens(Some((1200.0 - EDGE, 2.0)), 1200.0, None));
        assert!(!opens(Some((1200.0 - EDGE - 1.0, 400.0)), 1200.0, None));
        assert!(!opens(on_list, 1200.0, None), "shut, so not there");
        assert!(opens(on_list, 1200.0, l));
        assert!(!opens(Some((300.0, 100.0)), 1200.0, l));
        assert!(!opens(Some((1000.0, 880.0)), 1200.0, l), "under the list");
        assert!(!opens(None, 1200.0, l), "the pointer left the view");
    }
}
