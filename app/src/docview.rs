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
//! and decides. Guarded by `nothing_in_the_document_view_listens_for_the_mouse`.
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

use gpui::{div, prelude::*, px, App, Context, SharedString, Window};

use crate::docopen::{DocKind, DocTarget};

/// A document on screen.
pub struct DocumentView {
    target: DocTarget,
    backend: Backend,
}

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
        cx.on_release(|view, cx| view.release(cx)).detach();
        Self { target, backend }
    }

    pub fn target(&self) -> &DocTarget {
        &self.target
    }

    /// Give back everything this view holds on the GPU. Runs from the release
    /// hook registered in [`Self::new`], once, as the view is dropped.
    fn release(&mut self, cx: &mut App) {
        if let Backend::Image(img) = &mut self.backend {
            img.release(&self.target.path, cx);
        }
    }
}

impl Render for DocumentView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let th = crate::theme::theme(cx);
        let body = match &mut self.backend {
            Backend::Image(img) => img.element(&self.target.path, window, &th),
            Backend::Unshown(why) => div()
                .p(px(14.))
                .text_color(th.text.alpha(0.75))
                .child(why.clone())
                .into_any_element(),
        };
        div().size_full().overflow_hidden().child(body)
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
            new.contains("cx.on_release(") && new.contains(".release(cx)"),
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
