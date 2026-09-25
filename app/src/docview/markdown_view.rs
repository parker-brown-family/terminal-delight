//! The Markdown backend's view side: the file read off the main thread and
//! read again when it changes, the pictures it embeds decoded and owned, and
//! a press looked up among its links.
//!
//! The document itself — its parse, its scroll, the pictures it holds — is
//! [`MarkdownDoc`], which lives beside the renderer in `markdown.rs` because
//! the bench draws Markdown with the same code. What is here needs the view:
//! tasks that come back to it through a weak handle, and the link text its
//! last render laid out. It sat in `docview.rs` until the view's backends went
//! behind one trait ([`super::backend`]), and moved here as it was.

use std::any::Any;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    px, AnyElement, App, Context, ImgResourceLoader, Pixels, Point, RenderImage, Resource,
    ScrollDelta, Size, Task, Window,
};

use super::backend::{Backend, Drawn};
use super::markdown::{self, MarkdownDoc};
use super::{image, resolve_link, DocumentView, FileStamp, FollowLink, LinkTarget};
use crate::docopen::DocScroll;
use crate::theme::Theme;

// The view's calls, handed to the document's own methods in `markdown.rs`.
// Where a method there has the trait method's name, it is named with its type
// (`MarkdownDoc::wheel(self, …)`): Rust finds an inherent method before a
// trait's, so that is the document's own and not this one calling itself.
impl Backend for MarkdownDoc {
    fn element(&mut self, view: &Drawn, window: &mut Window, th: &Theme) -> AnyElement {
        MarkdownDoc::element(
            self,
            view.path,
            th,
            view.frame.map(|(size, _)| size),
            view.links,
            window,
        )
    }

    /// Every picture the document holds, and every decode and read still
    /// running for it.
    fn give_back(&mut self, path: &Path, cx: &mut App) {
        self.decoding.clear();
        for (_, image) in self.images.drain() {
            if let Ok(image) = image {
                cx.drop_image(image, None);
            }
        }
        if std::env::var_os("TD_DOCDEBUG").is_some() {
            eprintln!("[doc] released {}", path.display());
        }
        self.reading = Task::ready(());
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// A press on the document, looked up among its links: a heading in this
    /// document is scrolled to here, and anything else is emitted as
    /// [`FollowLink`] for the pane to route.
    fn press(&mut self, at: Point<Pixels>, view: &Drawn, cx: &mut Context<DocumentView>) -> bool {
        let (Some(origin), Some((size, _))) = (view.painted_at, view.frame) else {
            return false;
        };
        // Above the view is the square's strip; a link scrolled up under it
        // is not what was pressed.
        if at.x < px(0.) || at.y < px(0.) || at.x >= size.width || at.y >= size.height {
            return false;
        }
        let Some(href) = markdown::link_at(&view.links.borrow(), origin + at) else {
            return false;
        };
        let dir = view.path.parent().unwrap_or(Path::new("/"));
        let view_h = Some(f32::from(size.height));
        match resolve_link(dir, &href) {
            LinkTarget::Fragment(fragment) => self.go_to_fragment(fragment, view_h),
            LinkTarget::File { path, fragment } if path == view.path => {
                if let Some(fragment) = fragment {
                    self.go_to_fragment(fragment, view_h);
                }
            }
            LinkTarget::File { path, fragment } => cx.emit(FollowLink {
                target: path.to_string_lossy().into_owned(),
                fragment,
            }),
            LinkTarget::Url(url) => cx.emit(FollowLink {
                target: url,
                fragment: None,
            }),
        }
        cx.notify();
        true
    }

    /// Scrolls the column by hand: nothing under `docview` scrolls itself.
    fn wheel(
        &mut self,
        delta: ScrollDelta,
        line: f32,
        view: Size<Pixels>,
        _sf: f32,
        _cx: &mut Context<DocumentView>,
    ) -> bool {
        MarkdownDoc::wheel(self, delta, line, Some(f32::from(view.height)))
    }

    fn scroll(&self) -> Option<DocScroll> {
        MarkdownDoc::scroll(self)
    }

    fn restore_scroll(&mut self, at: DocScroll, cx: &mut Context<DocumentView>) {
        self.restore_fraction(at.top);
        cx.notify();
    }

    fn show_fragment(
        &mut self,
        fragment: String,
        view_h: Option<f32>,
        cx: &mut Context<DocumentView>,
    ) {
        self.go_to_fragment(fragment, view_h);
        cx.notify();
    }

    /// Re-read on every change, keeping the reader's place.
    fn follows_its_file(&self) -> bool {
        true
    }

    /// The first read starts now. Its stamp comes back with it, so there is
    /// none to give the watcher yet.
    fn opened(&mut self, path: &Path, cx: &mut Context<DocumentView>) -> Option<FileStamp> {
        read_markdown(self, path, cx);
        None
    }

    fn file_changed(&mut self, path: &Path, _came_back: bool, cx: &mut Context<DocumentView>) {
        read_markdown(self, path, cx);
    }
}

/// A picture a Markdown document embeds, as decoded, or the sentence its box
/// says instead.
fn embedded<E: std::fmt::Display>(
    decoded: Result<Arc<RenderImage>, E>,
) -> Result<Arc<RenderImage>, String> {
    let image = decoded.map_err(|e| format!("could not be read: {e}"))?;
    let size = image.size(0);
    let (w, h) = (size.width.0.max(0) as u32, size.height.0.max(0) as u32);
    if image::too_large(w, h) {
        return Err(format!(
            "{w} × {h} pixels, past the {} the GPU can hold here",
            image::MAX_SIDE
        ));
    }
    Ok(image)
}

/// Read and parse the Markdown file off the main thread.
fn read_markdown(md: &mut MarkdownDoc, path: &Path, cx: &mut Context<DocumentView>) {
    let path = path.to_path_buf();
    let read = cx.background_executor().spawn(async move {
        // Stamped BEFORE reading: a write landing in between leaves a
        // stamp older than the text, and the next tick reads again, which
        // is harmless. The other order could miss that write for good.
        let stamp = FileStamp::of(&path);
        let parsed = std::fs::read(&path)
            .map(|bytes| markdown::parse(&String::from_utf8_lossy(&bytes), path.parent()))
            .map_err(|e| format!("Could not read {}: {e}", path.display()));
        (stamp, parsed)
    });
    md.reading = cx.spawn(async move |this, cx| {
        let (stamp, parsed) = read.await;
        this.update(cx, |view, cx| markdown_read(view, stamp, parsed, cx))
            .ok();
    });
}

fn markdown_read(
    view: &mut DocumentView,
    stamp: Option<FileStamp>,
    parsed: Result<markdown::MdDoc, String>,
    cx: &mut Context<DocumentView>,
) {
    view.seen = stamp;
    let Some(md) = view.backend.downcast_mut::<MarkdownDoc>() else {
        return;
    };
    match parsed {
        Ok(doc) => {
            for image in md.replace(Rc::new(doc)) {
                cx.drop_image(image, None);
            }
        }
        // A document already on screen stays there when a re-read fails:
        // an editor writing in two steps would otherwise flash an error.
        Err(why) if matches!(md.doc, Some(Ok(_))) => {
            if std::env::var_os("TD_DOCDEBUG").is_some() {
                eprintln!("[doc] kept the last render: {why}");
            }
        }
        Err(why) => {
            if std::env::var_os("TD_DOCDEBUG").is_some() {
                eprintln!("[doc] cannot read {}", view.target.path.display());
            }
            md.doc = Some(Err(why));
        }
    }
    decode_images(md, cx);
    cx.notify();
}

/// Start decoding every local picture the document draws and the view
/// does not hold yet, owned the way the image backend owns its one.
fn decode_images(md: &mut MarkdownDoc, cx: &mut Context<DocumentView>) {
    let Some(Ok(doc)) = md.doc.clone() else {
        return;
    };
    for path in markdown::image_paths(&doc) {
        if md.images.contains_key(&path) || md.decoding.contains_key(&path) {
            continue;
        }
        let resource = Resource::Path(Arc::from(path.as_path()));
        let (decode, _) = cx.fetch_asset::<ImgResourceLoader>(&resource);
        // Out of gpui's cache at once, as the image backend does: the
        // pixels are this view's alone, so dropping them frees them.
        cx.remove_asset::<ImgResourceLoader>(&resource);
        let key = path.clone();
        let task = cx.spawn(async move |this, cx| {
            let result = embedded(decode.await);
            this.update(cx, |view, cx| image_decoded(view, key, result, cx))
                .ok();
        });
        md.decoding.insert(path, task);
    }
}

fn image_decoded(
    view: &mut DocumentView,
    path: PathBuf,
    result: Result<Arc<RenderImage>, String>,
    cx: &mut Context<DocumentView>,
) {
    let Some(md) = view.backend.downcast_mut::<MarkdownDoc>() else {
        return;
    };
    // No longer wanted — a re-read stopped drawing it. It was never
    // painted, so it is in no atlas; letting it drop is the whole release.
    let Some(running) = md.decoding.remove(&path) else {
        return;
    };
    // This is that task, finishing: let it finish rather than cancel it
    // from inside itself.
    running.detach();
    md.images.insert(path, result);
    cx.notify();
}
