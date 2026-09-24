//! A document drawn inside a pane: the view a floating square holds.
//!
//! # What is here, and what is not yet
//!
//! Images are drawn. Markdown and HTML are recognised by the click (see
//! [`crate::docopen`]) and open a square that says, in a sentence, that they
//! are not drawn here yet and how to open them instead. That sentence is the
//! whole of their backend until they get a real one, so a click never opens
//! an empty square and never guesses.
//!
//! # No mouse handlers, on purpose
//!
//! Nothing in this module registers a gpui mouse, scroll or hover handler. The
//! CRT pass bends the pane's pixels after layout, and gpui hit-tests the flat
//! layout, so a handler in here would fire beside what it draws. Input reaches
//! a document the way it reaches the workbench: the pane un-bends the pointer
//! and decides, then calls [`DocumentView::press`], [`DocumentView::drag`],
//! [`DocumentView::release`], [`DocumentView::wheel`] or
//! [`DocumentView::zoom`] with flat, view-local numbers. The one thing the
//! view does record for itself is its own size, measured at paint by a canvas
//! that listens to nothing. Guarded by
//! `nothing_in_the_document_view_listens_for_the_mouse`.
//!
//! # The image is ours, and it is given back
//!
//! gpui would cache a decoded image for the life of the process, keyed by its
//! path, and keep its texture in the window's atlas until somebody asks for it
//! back. Neither is acceptable for something a person opens and closes all
//! day: a 4K screenshot is 33 MB decoded and the same again on the GPU. So the
//! view fetches the image through gpui's own loader, takes it straight back
//! out of gpui's cache so the view holds the only reference, and when the view
//! is dropped it drops the texture from every window's atlas.
//!
//! That last step hangs on gpui's release hook rather than on whoever closes
//! the square, because a square can end in more ways than Escape: its pane
//! closes, its tab closes, a replica is repaired and the pane rebuilt. Every
//! one of those drops the view, and dropping the view is what gives the
//! texture back. gpui runs release hooks when its outermost update finishes,
//! after every window has been put back in its list, so one call reaches
//! every atlas.

pub mod image;

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    canvas, div, prelude::*, px, App, Context, Modifiers, Pixels, Point, ScrollDelta, SharedString,
    Size, Window,
};

use crate::docopen::{DocKind, DocTarget};

pub use image::{ImageZoom, ZoomStep};

/// How far one notch of a wheel that counts in lines moves a picture, in
/// logical pixels. Three lines of text at a common size, which is what a
/// notch scrolls in a browser.
const WHEEL_LINE_PX: f32 = 48.0;

/// A document on screen.
pub struct DocumentView {
    target: DocTarget,
    backend: Backend,
    /// The view's own size and the window's scale factor, as the last paint
    /// measured them. `None` until it has painted once: an unmeasured view is
    /// not a zero-sized one, and nothing that needs the size runs without it.
    frame: Rc<Cell<Option<Frame>>>,
}

/// A view's measured size, in logical pixels, and the scale factor it was
/// measured under.
type Frame = (Size<Pixels>, f32);

enum Backend {
    Image(image::ImageDoc),
    /// A document this build recognises but does not draw, and the sentence
    /// that says so.
    Unshown(SharedString),
}

/// What a Markdown or HTML square says until those backends exist. Names the
/// gesture that does work today, so the square is a pointer rather than a wall.
fn not_yet(kind: DocKind) -> &'static str {
    match kind {
        DocKind::Markdown => "Markdown is not drawn in the pane yet — ctrl+click the path to open it with the desktop.",
        DocKind::Html => "HTML is not drawn in the pane yet — ctrl+click the path to open it with the desktop.",
        DocKind::Image => "",
    }
}

impl DocumentView {
    pub fn new(target: DocTarget, cx: &mut Context<Self>) -> Self {
        let backend = match target.kind {
            DocKind::Image => Backend::Image(image::ImageDoc::load(&target.path, cx)),
            kind => Backend::Unshown(not_yet(kind).into()),
        };
        cx.on_release(|view, cx| view.give_back(cx)).detach();
        Self {
            target,
            backend,
            frame: Rc::new(Cell::new(None)),
        }
    }

    pub fn target(&self) -> &DocTarget {
        &self.target
    }

    /// Give back everything this view holds on the GPU. Runs from the release
    /// hook registered in [`Self::new`], once, as the view is dropped.
    fn give_back(&mut self, cx: &mut App) {
        if let Backend::Image(img) = &mut self.backend {
            img.release(&self.target.path, cx);
        }
    }

    // ── input, already un-bent by the pane ──────────────────────────────────
    //
    // Every point below is flat and relative to the view's own top-left. The
    // pane found it through the tube's inverse; nothing here asks gpui where
    // the pointer is, because gpui would answer for the flat layout and the
    // picture is bent.

    /// A press on the document. Answers whether the view took it: an image
    /// takes it as the start of a pan.
    pub fn press(
        &mut self,
        at: Point<Pixels>,
        _mods: Modifiers,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> bool {
        match &mut self.backend {
            Backend::Image(img) => img.press(at),
            Backend::Unshown(_) => false,
        }
    }

    /// The pointer moved with the left button still held since [`Self::press`].
    pub fn drag(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some((view, sf)) = self.frame.get() else {
            return;
        };
        if let Backend::Image(img) = &mut self.backend {
            if img.drag(at, view, sf) {
                cx.notify();
            }
        }
    }

    /// The held press ended, wherever the pointer is now.
    pub fn release(&mut self, _cx: &mut Context<Self>) {
        if let Backend::Image(img) = &mut self.backend {
            img.end_pan();
        }
    }

    /// A wheel turn over the document: it pans a picture larger than the
    /// view. Ctrl+wheel never arrives here; that is the pane's text dial.
    pub fn wheel(&mut self, delta: ScrollDelta, cx: &mut Context<Self>) {
        let Some((view, sf)) = self.frame.get() else {
            return;
        };
        let (dx, dy) = match delta {
            ScrollDelta::Pixels(p) => (f32::from(p.x), f32::from(p.y)),
            ScrollDelta::Lines(l) => (l.x * WHEEL_LINE_PX, l.y * WHEEL_LINE_PX),
        };
        if let Backend::Image(img) = &mut self.backend {
            if img.pan_by(dx, dy, view, sf) {
                cx.notify();
            }
        }
    }

    /// One press of a zoom control. Answers whether anything changed.
    pub fn zoom(&mut self, step: ZoomStep, cx: &mut Context<Self>) -> bool {
        let Some((view, sf)) = self.frame.get() else {
            return false;
        };
        let changed = match &mut self.backend {
            Backend::Image(img) => img.zoom(step, view, sf),
            Backend::Unshown(_) => false,
        };
        if changed {
            cx.notify();
        }
        changed
    }

    /// The zoom a picture is at, for the strip's label. `None` for a document
    /// with no zoom to show, so the strip draws no zoom controls for it.
    pub fn zoom_now(&self) -> Option<ImageZoom> {
        match &self.backend {
            Backend::Image(img) => Some(img.zoom_now()),
            Backend::Unshown(_) => None,
        }
    }
}

impl Render for DocumentView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let th = crate::theme::theme(cx);
        let frame = self.frame.get();
        let body = match &mut self.backend {
            Backend::Image(img) => img.element(&self.target.path, frame, window, &th),
            Backend::Unshown(why) => div()
                .p(px(14.))
                .text_color(th.text.alpha(0.75))
                .child(why.clone())
                .into_any_element(),
        };
        // Measured, not listened to: a canvas records the box this view was
        // given and the scale it paints at, and asks for one more frame when
        // either changed, so a zoom placed against a stale size corrects
        // itself at once.
        let store = self.frame.clone();
        let weak = cx.entity().downgrade();
        let measure = canvas(
            move |bounds, window, cx| {
                let now = Some((bounds.size, window.scale_factor()));
                if store.get() != now {
                    store.set(now);
                    let weak = weak.clone();
                    cx.defer(move |cx| {
                        let _ = weak.update(cx, |_, cx| cx.notify());
                    });
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .child(measure)
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    /// This module and everything under it, with each file's tests cut off and
    /// its comments dropped, so a sentence that mentions a handler does not
    /// count as one.
    fn view_sources() -> Vec<(&'static str, String)> {
        let strip = |src: &str| -> String {
            let live = src.split("#[cfg(test)]").next().unwrap_or(src);
            live.lines()
                .filter(|l| {
                    let t = l.trim_start();
                    !t.starts_with("//")
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        vec![
            ("docview.rs", strip(include_str!("docview.rs"))),
            ("docview/image.rs", strip(include_str!("docview/image.rs"))),
        ]
    }

    /// The CRT pass bends the pixels after layout and gpui hit-tests the flat
    /// layout, so a handler in the document view would fire beside what it
    /// draws on any curved pane — and pass every test on a flat one. The pane
    /// un-bends the pointer and decides; nothing in here may listen.
    #[test]
    fn nothing_in_the_document_view_listens_for_the_mouse() {
        for (name, src) in view_sources() {
            for listener in [
                ".on_mouse_down(",
                ".on_mouse_up(",
                ".on_mouse_move(",
                ".on_click(",
                ".on_scroll_wheel(",
                ".on_hover(",
                ".on_drag(",
                ".on_mouse_down_out(",
                ".on_mouse_up_out(",
                "overflow_y_scroll",
                "overflow_x_scroll",
                "InteractiveText",
            ] {
                assert!(
                    !src.contains(listener),
                    "{name} registers {listener}: input reaches the document view through the pane"
                );
            }
        }
    }

    /// However a square ends, dropping its view gives the texture back: the
    /// hook is registered where the view is made, not where one caller closes it.
    #[test]
    fn a_document_view_releases_itself_when_it_is_dropped() {
        let (_, src) = &view_sources()[0];
        let new = src.split("pub fn new(").nth(1).expect("DocumentView::new");
        let new = new.split("\n    }\n").next().unwrap_or(new);
        assert!(
            new.contains("cx.on_release(") && new.contains(".give_back(cx)"),
            "DocumentView::new must register its own release"
        );
    }

    #[test]
    fn a_markdown_or_html_square_says_what_opens_it_instead() {
        use crate::docopen::DocKind;
        for kind in [DocKind::Markdown, DocKind::Html] {
            let s = super::not_yet(kind);
            assert!(s.contains("ctrl+click"), "{kind:?}: {s}");
        }
    }
}
