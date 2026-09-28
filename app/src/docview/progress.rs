//! How far through a document the reader is: a rail of phosphor blocks down
//! the view's right edge, and the percentage beside its top.
//!
//! # What the rail says
//!
//! A long report gives no sense of its length from inside it. A scrollbar
//! would, but nothing under `docview` scrolls itself (see "No mouse handlers,
//! on purpose" in `docview.rs`), and a page drawn by headless Chromium has its
//! own hidden. So the view draws one, as a column of blocks, the way a copy
//! dialog draws its progress across:
//!
//! - **here** — the blocks the view is showing now, lit in full;
//! - **passed** — the blocks above it, already read, lit dimmer;
//! - **ahead** — the blocks below it, still to read, barely lit.
//!
//! The percentage is how much of the document has come into view: where the
//! bottom of the view is, as a fraction of the document. So the lit blocks and
//! the number always agree, a report opens on the share of it its first screen
//! is, and the number reaches 100% at the last line and not before.
//!
//! # What it does not say
//!
//! A document that fits the view has nothing to scroll, and draws no rail. A
//! document not yet laid out has no place to show, which is not the top of
//! it: [`super::backend::Backend::reading`] answers `None`, and nothing is
//! drawn rather than a rail reading 0%.
//!
//! # Where it sits
//!
//! Down the right edge, [`GUTTER`] wide. The chrome a document draws in its
//! own bottom-right corner — a brief's notes bar, a PDF's page counter — keeps
//! [`GUTTER`] clear of the edge, so the rail runs the whole height beside it.

use gpui::{div, point, prelude::*, px, AnyElement, BoxShadow};

use crate::theme::Theme;

/// From the view's right edge to the rail.
const INSET: f32 = 4.0;
/// How wide the rail is.
const WIDTH: f32 = 6.0;
/// Room the rail keeps on the right: its inset, its width, and a gap. Corner
/// chrome sits this far in from the edge.
pub const GUTTER: f32 = INSET + WIDTH + 4.0;
/// Above and below the rail.
const END: f32 = 6.0;
/// One block's height and the gap under it.
const BLOCK: f32 = 10.0;
const GAP: f32 = 3.0;
/// Blocks, whatever the view's height. Too few and a screen's worth is
/// invisible; too many and the blocks are a dotted line.
const FEWEST: usize = 6;
const MOST: usize = 120;

/// Which part of a document the view shows: its top and bottom edges, each as
/// a fraction of the document's height, `0.0..=1.0`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Reading {
    pub top: f32,
    pub bottom: f32,
}

impl Reading {
    /// Pure. A view `view` tall, scrolled `top` into a document `content`
    /// tall, all three in whatever unit the backend measures in. `None` when
    /// the document has no height, which is nothing measured.
    pub fn of(top: f32, view: f32, content: f32) -> Option<Reading> {
        let measured = content.is_finite()
            && content > 0.0
            && view.is_finite()
            && view >= 0.0
            && top.is_finite();
        if !measured {
            return None;
        }
        let top = (top / content).clamp(0.0, 1.0);
        let bottom = (top + view / content).clamp(top, 1.0);
        Some(Reading { top, bottom })
    }

    /// Whether the whole document is in view, so there is nothing to scroll.
    pub fn fits(&self) -> bool {
        self.top <= 0.0 && self.bottom >= 1.0
    }

    /// Pure. How much of the document has come into view, in whole percent,
    /// rounded down, so 100 is the last line and not the one before it.
    pub fn percent(&self) -> u32 {
        // The last scroll lands a hair short of 1.0 in f32; that is the end.
        ((self.bottom * 100.0 + 0.01).floor() as u32).min(100)
    }
}

/// One block of the rail.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Block {
    /// Above the view: read.
    Passed,
    /// In the view.
    Here,
    /// Below the view: still to read.
    Ahead,
}

/// Pure. How many blocks a rail `track` tall holds.
pub fn count(track: f32) -> usize {
    (((track + GAP) / (BLOCK + GAP)).floor().max(0.0) as usize).clamp(FEWEST, MOST)
}

/// Pure. The rail's `n` blocks, top to bottom. Block `i` stands for
/// `i/n..(i+1)/n` of the document; any block the view reaches into is
/// [`Block::Here`], so a view is never too small to light one.
pub fn blocks(r: Reading, n: usize) -> Vec<Block> {
    let n = n.max(1);
    let edge = 1e-4;
    (0..n)
        .map(|i| {
            let (from, to) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
            if to <= r.top + edge {
                Block::Passed
            } else if from >= r.bottom - edge {
                Block::Ahead
            } else {
                Block::Here
            }
        })
        .collect()
}

/// The rail and its percentage, over a view `view_h` tall. Nothing for a
/// document that fits, or a view too short to hold a rail.
pub fn rail(r: Reading, view_h: f32, th: &Theme) -> Option<AnyElement> {
    let track = view_h - 2.0 * END;
    if r.fits() || track < FEWEST as f32 * (BLOCK + GAP) {
        return None;
    }
    let lit = th.accent;
    let column = blocks(r, count(track)).into_iter().map(|b| {
        let (fill, rim) = match b {
            Block::Here => (lit, lit),
            Block::Passed => (lit.alpha(0.32), lit.alpha(0.5)),
            Block::Ahead => (lit.alpha(0.07), lit.alpha(0.22)),
        };
        let block = div()
            .flex_1()
            .min_h(px(1.))
            .rounded(px(1.5))
            .bg(fill)
            .border_1()
            .border_color(rim);
        // The view's own blocks cast light, as a lit phosphor does, so where
        // the reader is stands out from what they have read. That light rides
        // the phosphor gauge like every other; turned all the way down, the
        // block's own fill and rim still say where the reader is.
        match b {
            Block::Here => block.shadow(
                crate::theme::phosphor(
                    th.grade.phosphor,
                    BoxShadow {
                        color: lit.alpha(0.6),
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(6.),
                        spread_radius: px(0.),
                        inset: false,
                    },
                )
                .into_iter()
                .collect(),
            ),
            _ => block,
        }
    });
    let rail = div()
        .absolute()
        .top(px(END))
        .bottom(px(END))
        .right(px(INSET))
        .w(px(WIDTH))
        .flex()
        .flex_col()
        .gap(px(GAP))
        .children(column);
    let percent = div()
        .absolute()
        .top(px(END - 2.0))
        .right(px(GUTTER))
        .px(px(6.))
        .py(px(1.))
        .rounded(px(4.))
        .bg(th.bg.alpha(0.85))
        .border_1()
        .border_color(lit.alpha(0.4))
        .text_size(px(11.))
        .text_color(lit)
        .child(format!("{}%", r.percent()));
    Some(
        div()
            .absolute()
            .inset_0()
            .child(rail)
            .child(percent)
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The view's edges as fractions of the document, whatever unit the
    /// backend measures in; a document nobody has measured has no reading.
    #[test]
    fn a_reading_is_the_views_edges_as_fractions_of_the_document() {
        let r = Reading::of(250.0, 100.0, 1000.0).expect("measured");
        assert!(
            (r.top - 0.25).abs() < 1e-6 && (r.bottom - 0.35).abs() < 1e-6,
            "{r:?}"
        );
        assert_eq!(Reading::of(0.0, 100.0, 0.0), None, "no height, no reading");
        assert_eq!(Reading::of(f32::NAN, 100.0, 1000.0), None);
        assert_eq!(Reading::of(0.0, f32::NAN, 1000.0), None);
    }

    /// A report opens on the share of it its first screen is, and reads 100%
    /// at the last line and not the one before it.
    #[test]
    fn the_percentage_is_how_much_has_come_into_view() {
        let pct = |top, view, content| Reading::of(top, view, content).unwrap().percent();
        assert_eq!(
            pct(0.0, 100.0, 2500.0),
            4,
            "the first screen of a long report"
        );
        assert_eq!(pct(900.0, 100.0, 2000.0), 50);
        // The last scroll, computed as the backends compute it, in f32.
        let (view, content) = (731.3_f32, 18_457.9_f32);
        assert_eq!(pct(content - view, view, content), 100);
        assert_eq!(pct(content - view - 200.0, view, content), 98);
    }

    /// A document that fits the view has nothing to scroll: no rail.
    #[test]
    fn a_document_that_fits_draws_no_rail() {
        let r = Reading::of(0.0, 900.0, 600.0).unwrap();
        assert!(r.fits());
        assert_eq!(r.percent(), 100);
        assert!(!Reading::of(0.0, 900.0, 901.0).unwrap().fits());
    }

    /// Blocks above the view are passed, those it reaches into are here, and
    /// those below are ahead; the lit ones end where the percentage says.
    #[test]
    fn the_blocks_are_passed_here_and_ahead() {
        use Block::*;
        let r = Reading::of(300.0, 200.0, 1000.0).unwrap();
        assert_eq!(
            blocks(r, 10),
            vec![Passed, Passed, Passed, Here, Here, Ahead, Ahead, Ahead, Ahead, Ahead]
        );
        let lit = blocks(r, 10).iter().filter(|b| **b != Ahead).count();
        assert_eq!(lit * 10, r.percent() as usize);
    }

    /// A view much shorter than one block's share of the document still
    /// lights the block it is in.
    #[test]
    fn a_view_is_never_too_small_to_light_a_block() {
        let r = Reading::of(5_000.0, 50.0, 100_000.0).unwrap();
        let b = blocks(r, 40);
        assert_eq!(b.iter().filter(|b| **b == Block::Here).count(), 1);
        assert_eq!(b[2], Block::Here);
        // At the very top and the very bottom.
        let top = blocks(Reading::of(0.0, 50.0, 100_000.0).unwrap(), 40);
        assert_eq!(top[0], Block::Here);
        let end = blocks(Reading::of(99_950.0, 50.0, 100_000.0).unwrap(), 40);
        assert_eq!(end[39], Block::Here);
        assert!(end[..39].iter().all(|b| *b == Block::Passed));
    }

    /// The rail holds as many blocks as its height, within bounds.
    #[test]
    fn the_rail_holds_blocks_by_its_height() {
        assert_eq!(count(13.0 * 50.0 - 3.0), 50);
        assert_eq!(count(20.0), FEWEST);
        assert_eq!(count(100_000.0), MOST);
    }
}
