//! The image backend: decoded by gpui's own loader, owned by the view, and
//! given back when the view closes.
//!
//! gpui decodes PNG, JPEG, WebP, GIF, BMP and SVG on a background task already,
//! so this backend does not decode anything itself. What it changes is who
//! owns the result. `img(path)` would leave the decoded pixels in gpui's asset
//! cache for the life of the process and the texture in the atlas until
//! somebody removes it; here the view takes the decode out of the cache the
//! moment it is requested, holds the only reference, and drops the texture
//! from every window's atlas in [`ImageDoc::release`].
//!
//! Drawn to fit, never above one image pixel per device pixel: a small icon
//! stays small and sharp, and a large screenshot shrinks to the square.

use std::path::Path;
use std::sync::Arc;

use gpui::{
    div, img, prelude::*, px, AnyElement, App, Context, ImageSource, ImgResourceLoader, ObjectFit,
    RenderImage, Resource, Task, Window,
};

use super::{Backend, DocumentView};
use crate::theme::Theme;

/// The largest side TD will hand the atlas. gpui gives the app no texture
/// limit to ask for; its atlas clamps a texture to the device's limit and then
/// fails to allocate, so a larger image would draw nothing at all and say
/// nothing about why. 16,384 is the lower of the two device limits measured
/// while planning this (the other was 32,768).
pub const MAX_SIDE: u32 = 16_384;

/// Whether an image is too large to draw, by its pixel size.
pub fn too_large(w: u32, h: u32) -> bool {
    w > MAX_SIDE || h > MAX_SIDE
}

pub struct ImageDoc {
    /// `None` while decoding. The error is a sentence for the square.
    loaded: Option<Result<Arc<RenderImage>, String>>,
    /// Whether this image has been drawn yet, for `TD_DOCDEBUG`'s one line.
    drawn: bool,
    _load: Task<()>,
}

impl ImageDoc {
    pub fn load(path: &Path, cx: &mut Context<DocumentView>) -> Self {
        let resource = Resource::Path(Arc::from(path));
        let (decode, _) = cx.fetch_asset::<ImgResourceLoader>(&resource);
        // Out of gpui's cache at once: the task keeps decoding for us, and the
        // pixels it produces are referenced by this view and nothing else.
        cx.remove_asset::<ImgResourceLoader>(&resource);
        let load = cx.spawn(async move |this, cx| {
            let result = match decode.await {
                Ok(image) => {
                    let size = image.size(0);
                    let (w, h) = (size.width.0.max(0) as u32, size.height.0.max(0) as u32);
                    if too_large(w, h) {
                        Err(format!(
                            "This image is {w} × {h} pixels. The largest side the GPU can hold here is {MAX_SIDE}, so ctrl+click the path to open it with the desktop."
                        ))
                    } else {
                        Ok(image)
                    }
                }
                Err(e) => Err(format!("Could not read this image: {e}")),
            };
            this.update(cx, |view, cx| {
                if let Backend::Image(doc) = &mut view.backend {
                    doc.loaded = Some(result);
                }
                cx.notify();
            })
            .ok();
        });
        Self {
            loaded: None,
            drawn: false,
            _load: load,
        }
    }

    pub fn element(&mut self, path: &Path, window: &mut Window, th: &Theme) -> AnyElement {
        let note = |s: String| {
            div()
                .p(px(14.))
                .text_color(th.text.alpha(0.75))
                .child(s)
                .into_any_element()
        };
        let image = match &self.loaded {
            None => return note("decoding…".into()),
            Some(Err(why)) => return note(why.clone()),
            Some(Ok(image)) => image.clone(),
        };
        let size = image.size(0);
        let sf = window.scale_factor().max(0.1);
        if std::env::var_os("TD_DOCDEBUG").is_some() && !self.drawn {
            eprintln!(
                "[doc] drew {} {}x{}",
                path.display(),
                size.width.0,
                size.height.0
            );
        }
        self.drawn = true;
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                img(ImageSource::Render(image))
                    .w_full()
                    .h_full()
                    // One image pixel per device pixel at most: the box is
                    // never larger than the image's own size in logical pixels.
                    .max_w(px(size.width.0 as f32 / sf))
                    .max_h(px(size.height.0 as f32 / sf))
                    .object_fit(ObjectFit::Contain),
            )
            .into_any_element()
    }

    /// Drop the texture from every window's atlas and let the pixels go.
    ///
    /// Also cancels a decode still running, so a square closed the instant it
    /// opened does not finish decoding a large file for nobody. Called from the
    /// view's release hook, when no window is mid-update, so naming none reaches
    /// them all.
    pub fn release(&mut self, path: &Path, cx: &mut App) {
        if let Some(Ok(image)) = self.loaded.take() {
            cx.drop_image(image, None);
        }
        self._load = Task::ready(());
        if std::env::var_os("TD_DOCDEBUG").is_some() {
            eprintln!("[doc] released {}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_image_past_the_atlas_limit_is_refused_rather_than_drawn_blank() {
        assert!(!too_large(3840, 2160));
        assert!(!too_large(MAX_SIDE, MAX_SIDE));
        assert!(too_large(MAX_SIDE + 1, 10));
        assert!(too_large(10, 40_000));
    }

    /// Closing a square has to take the texture out of the atlas: gpui keeps
    /// it there otherwise for as long as the window lives. The soak run counts
    /// this on real hardware; this is the cheap guard that the call is there.
    #[test]
    fn releasing_an_image_drops_its_texture() {
        let src = include_str!("image.rs");
        let live = src.split("#[cfg(test)]").next().unwrap_or(src);
        let body = live
            .split("pub fn release(")
            .nth(1)
            .expect("ImageDoc::release exists");
        let body = body.split("\n    }\n").next().unwrap_or(body);
        assert!(
            body.contains("drop_image("),
            "release must drop the texture"
        );
        assert!(
            live.contains("remove_asset::<ImgResourceLoader>"),
            "the decode must be taken out of gpui's cache, or it outlives the view"
        );
    }
}
