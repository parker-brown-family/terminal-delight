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
//!
//! # Zoom and pan
//!
//! A zoom is either *fit* or a scale in image pixels per device pixel, so
//! `Scale(1.0)` is the image exactly as its pixels are, whatever the monitor's
//! scale factor. Zooming steps along a short ladder and keeps the image pixel
//! at the view's centre where it was, the way every image viewer does. Panning
//! moves that centre pixel, and stops where the image's edge meets the view's:
//! an image larger than the view never shows background past an edge, and one
//! smaller than the view stays centred on that axis.
//!
//! None of this listens for anything. The pane un-bends the pointer and calls
//! [`ImageDoc::press`], [`ImageDoc::drag`], [`ImageDoc::pan_by`] and
//! [`ImageDoc::zoom`] with flat, view-local numbers; the geometry is the pure
//! functions below, which is where the tests hold it.

use std::any::Any;
use std::path::Path;
use std::sync::Arc;

use gpui::{
    div, img, point, prelude::*, px, size, AnyElement, App, Bounds, Context, DevicePixels,
    ImageSource, ImgResourceLoader, ObjectFit, Pixels, Point, RenderImage, Resource, ScrollDelta,
    Size, Task, Window,
};

use super::backend::{Backend, Drawn};
use super::DocumentView;
use crate::theme::Theme;

/// How far one notch of a wheel that counts in lines moves a picture, in
/// logical pixels. Three lines of text at a common size, which is what a
/// notch scrolls in a browser.
const WHEEL_LINE_PX: f32 = 48.0;

/// How large the image is drawn.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ImageZoom {
    /// Contained in the view, and never above one image pixel per device pixel.
    Fit,
    /// Image pixels per device pixel: `Scale(1.0)` is actual size.
    Scale(f32),
}

/// The zoom ladder, in image pixels per device pixel. The steps a browser
/// uses near 1:1, spreading out towards the ends where one step at a time
/// would take all day.
pub const ZOOM_STEPS: [f32; 12] = [
    0.1, 0.25, 0.33, 0.5, 0.67, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 8.0,
];

/// Two scales closer than this are the same step.
const SAME_STEP: f32 = 1e-3;

/// One press of a zoom control.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ZoomStep {
    Out,
    /// Fit when zoomed, actual size when fitted.
    FitOrActual,
    In,
    /// Fit, whatever the zoom is now: the Document face's `0`.
    Fit,
    /// One image pixel per device pixel: the Document face's `1`.
    Actual,
}

fn img_wh(img: Size<DevicePixels>) -> (f32, f32) {
    (img.width.0.max(1) as f32, img.height.0.max(1) as f32)
}

fn view_wh(view: Size<Pixels>) -> (f32, f32) {
    (
        f32::from(view.width).max(0.0),
        f32::from(view.height).max(0.0),
    )
}

/// The scale that contains the image in the view, and never enlarges it: a
/// 64-pixel icon in a 600-pixel square is drawn 64 pixels wide, not 600.
pub fn fit_scale(img: Size<DevicePixels>, view: Size<Pixels>, scale_factor: f32) -> f32 {
    let (iw, ih) = img_wh(img);
    let (vw, vh) = view_wh(view);
    let sf = scale_factor.max(0.1);
    (vw * sf / iw).min(vh * sf / ih).clamp(SAME_STEP, 1.0)
}

fn scale_of(zoom: ImageZoom, fit: f32) -> f32 {
    match zoom {
        ImageZoom::Fit => fit,
        ImageZoom::Scale(s) => s.max(SAME_STEP),
    }
}

/// Where the image is drawn, in the view's own logical pixels.
///
/// `centre` is the image pixel that sits under the view's centre. A fitted
/// image ignores it and is centred, which is what fitting means.
pub fn image_rect(
    img: Size<DevicePixels>,
    view: Size<Pixels>,
    scale_factor: f32,
    zoom: ImageZoom,
    centre: Point<f32>,
) -> Bounds<Pixels> {
    let sf = scale_factor.max(0.1);
    let (iw, ih) = img_wh(img);
    let (vw, vh) = view_wh(view);
    let s = scale_of(zoom, fit_scale(img, view, sf));
    let c = match zoom {
        ImageZoom::Fit => point(iw / 2.0, ih / 2.0),
        ImageZoom::Scale(_) => centre,
    };
    Bounds {
        origin: point(px(vw / 2.0 - c.x * s / sf), px(vh / 2.0 - c.y * s / sf)),
        size: size(px(iw * s / sf), px(ih * s / sf)),
    }
}

/// The next zoom along the ladder.
///
/// In goes to the first step above where the image is now. Out goes to the
/// first step below it, and to *fit* once that step would be no larger than
/// fitting: zooming out past the whole image only makes it smaller, and the
/// ladder's bottom rung is not a place anyone wants to land by accident.
pub fn step_zoom(zoom: ImageZoom, fit: f32, up: bool) -> ImageZoom {
    let now = scale_of(zoom, fit);
    let next = if up {
        ZOOM_STEPS.iter().copied().find(|s| *s > now + SAME_STEP)
    } else {
        ZOOM_STEPS
            .iter()
            .rev()
            .copied()
            .find(|s| *s < now - SAME_STEP)
    };
    match next {
        Some(s) if s > fit + SAME_STEP => ImageZoom::Scale(s),
        Some(_) => ImageZoom::Fit,
        // Nothing further in: stay. Nothing further out: fit.
        None if up => zoom,
        None => ImageZoom::Fit,
    }
}

/// Keep a centre pixel where the view shows image, not background.
///
/// On an axis where the drawn image is larger than the view, the centre may
/// travel until the image's edge meets the view's edge and no further. On an
/// axis where it fits, it is the image's own centre: there is nothing to pan.
pub fn clamp_centre(
    centre: Point<f32>,
    img: Size<DevicePixels>,
    view: Size<Pixels>,
    zoom: f32,
    scale_factor: f32,
) -> Point<f32> {
    let sf = scale_factor.max(0.1);
    let s = zoom.max(SAME_STEP);
    let (iw, ih) = img_wh(img);
    let (vw, vh) = view_wh(view);
    let axis = |c: f32, i: f32, v: f32| {
        if i * s / sf <= v {
            i / 2.0
        } else {
            let half = v / 2.0 * sf / s;
            c.clamp(half, i - half)
        }
    };
    point(axis(centre.x, iw, vw), axis(centre.y, ih, vh))
}

/// A press on the image, held: where it landed and which pixel was at the
/// centre then, so the drag follows the pointer's whole travel.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PanDrag {
    start: Point<Pixels>,
    centre_at_start: Point<f32>,
}

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
    zoom: ImageZoom,
    /// The image pixel under the view's centre. `None` is the image's own
    /// centre, which is also where a fitted image always is.
    centre: Option<Point<f32>>,
    pan: Option<PanDrag>,
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
                if let Some(doc) = view.backend.downcast_mut::<ImageDoc>() {
                    doc.loaded = Some(result);
                }
                cx.notify();
            })
            .ok();
        });
        Self {
            loaded: None,
            zoom: ImageZoom::Fit,
            centre: None,
            pan: None,
            drawn: false,
            _load: load,
        }
    }

    /// The decoded image's size, once it has decoded into something drawable.
    fn image_size(&self) -> Option<Size<DevicePixels>> {
        match &self.loaded {
            Some(Ok(image)) => Some(image.size(0)),
            _ => None,
        }
    }

    pub fn zoom_now(&self) -> ImageZoom {
        self.zoom
    }

    fn centre_or_middle(&self, img: Size<DevicePixels>) -> Point<f32> {
        let (iw, ih) = img_wh(img);
        self.centre.unwrap_or(point(iw / 2.0, ih / 2.0))
    }

    /// One press of a zoom control, about the view's centre. `view` and `sf`
    /// are the view's measured size and the window's scale factor.
    pub fn zoom(&mut self, step: ZoomStep, view: Size<Pixels>, sf: f32) -> bool {
        let Some(img) = self.image_size() else {
            return false;
        };
        let fit = fit_scale(img, view, sf);
        let next = match step {
            ZoomStep::In => step_zoom(self.zoom, fit, true),
            ZoomStep::Out => step_zoom(self.zoom, fit, false),
            ZoomStep::FitOrActual => match self.zoom {
                ImageZoom::Fit => ImageZoom::Scale(1.0),
                ImageZoom::Scale(_) => ImageZoom::Fit,
            },
            ZoomStep::Fit => ImageZoom::Fit,
            ZoomStep::Actual => ImageZoom::Scale(1.0),
        };
        if next == self.zoom {
            return false;
        }
        // The pixel at the centre stays at the centre; whatever it clamps to
        // at the new scale is where the view settles.
        let centre = self.centre_or_middle(img);
        self.zoom = next;
        self.centre = match next {
            ImageZoom::Fit => None,
            ImageZoom::Scale(s) => Some(clamp_centre(centre, img, view, s, sf)),
        };
        true
    }

    /// Move the picture by `(dx, dy)` logical pixels, as a hand dragging it
    /// would: positive moves it right and down.
    pub fn pan_by(&mut self, dx: f32, dy: f32, view: Size<Pixels>, sf: f32) -> bool {
        let Some(img) = self.image_size() else {
            return false;
        };
        let s = scale_of(self.zoom, fit_scale(img, view, sf));
        let from = self.centre_or_middle(img);
        let sf = sf.max(0.1);
        let to = clamp_centre(
            point(from.x - dx * sf / s, from.y - dy * sf / s),
            img,
            view,
            s,
            sf,
        );
        if self.zoom == ImageZoom::Fit || to == from {
            return false;
        }
        self.centre = Some(to);
        true
    }

    /// A press on the image, flat and view-local. Takes it when there is a
    /// picture to hold.
    pub fn press(&mut self, at: Point<Pixels>) -> bool {
        let Some(img) = self.image_size() else {
            return false;
        };
        self.pan = Some(PanDrag {
            start: at,
            centre_at_start: self.centre_or_middle(img),
        });
        true
    }

    /// The held press moved to `at`: the picture follows the pointer.
    pub fn drag(&mut self, at: Point<Pixels>, view: Size<Pixels>, sf: f32) -> bool {
        let (Some(pan), Some(img)) = (self.pan, self.image_size()) else {
            return false;
        };
        if self.zoom == ImageZoom::Fit {
            return false;
        }
        let s = scale_of(self.zoom, fit_scale(img, view, sf));
        let sf = sf.max(0.1);
        let (dx, dy) = (f32::from(at.x - pan.start.x), f32::from(at.y - pan.start.y));
        let to = clamp_centre(
            point(
                pan.centre_at_start.x - dx * sf / s,
                pan.centre_at_start.y - dy * sf / s,
            ),
            img,
            view,
            s,
            sf,
        );
        let moved = self.centre != Some(to);
        self.centre = Some(to);
        moved
    }

    pub fn end_pan(&mut self) {
        self.pan = None;
    }

    /// `view` is the view's size as last measured, with the scale factor it
    /// was measured under; `None` before the first paint, when a fitted image
    /// is centred by layout instead.
    pub fn element(
        &mut self,
        path: &Path,
        view: Option<(Size<Pixels>, f32)>,
        window: &mut Window,
        th: &Theme,
    ) -> AnyElement {
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
        let Some((view, vsf)) = view else {
            // Not measured yet: the first frame, always fitted. Layout centres
            // it, and the next frame knows the size.
            return div()
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
                .into_any_element();
        };
        let centre = self.centre_or_middle(size);
        let r = image_rect(size, view, vsf, self.zoom, centre);
        // On whole device pixels, so actual size is actually sharp: half a
        // pixel of offset resamples every pixel of the picture.
        let snap = |v: Pixels| px((f32::from(v) * vsf).round() / vsf.max(0.1));
        div()
            .size_full()
            .relative()
            .child(
                img(ImageSource::Render(image))
                    .absolute()
                    .left(snap(r.origin.x))
                    .top(snap(r.origin.y))
                    .w(r.size.width)
                    .h(r.size.height)
                    .object_fit(ObjectFit::Fill),
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

    /// An image that has not started decoding, for tests that need a backend
    /// without a window.
    #[cfg(test)]
    pub(crate) fn unloaded() -> Self {
        Self {
            loaded: None,
            zoom: ImageZoom::Fit,
            centre: None,
            pan: None,
            drawn: false,
            _load: Task::ready(()),
        }
    }
}

// The view's calls, handed to the picture's own methods above. Where one of
// those has the trait method's name it is named with its type
// (`ImageDoc::press(self, …)`): Rust finds an inherent method before a
// trait's, so that is the picture's own and not this one calling itself.
//
// A picture pans, zooms and gives its texture back, and nothing else: it has
// no scroll to save, no fragment to land on, no file to follow and no notes,
// so those are the trait's defaults.
impl Backend for ImageDoc {
    fn element(&mut self, view: &Drawn, window: &mut Window, th: &Theme) -> AnyElement {
        ImageDoc::element(self, view.path, view.frame, window, th)
    }

    fn give_back(&mut self, path: &Path, cx: &mut App) {
        self.release(path, cx);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// The start of a pan.
    fn press(&mut self, at: Point<Pixels>, _view: &Drawn, _cx: &mut Context<DocumentView>) -> bool {
        ImageDoc::press(self, at)
    }

    fn drag(&mut self, at: Point<Pixels>, view: Size<Pixels>, sf: f32) -> bool {
        ImageDoc::drag(self, at, view, sf)
    }

    fn end_press(&mut self) {
        self.end_pan();
    }

    /// Pans a picture larger than the view.
    fn wheel(
        &mut self,
        delta: ScrollDelta,
        _line: f32,
        view: Size<Pixels>,
        sf: f32,
        _cx: &mut Context<DocumentView>,
    ) -> bool {
        let (dx, dy) = match delta {
            ScrollDelta::Pixels(p) => (f32::from(p.x), f32::from(p.y)),
            ScrollDelta::Lines(l) => (l.x * WHEEL_LINE_PX, l.y * WHEEL_LINE_PX),
        };
        self.pan_by(dx, dy, view, sf)
    }

    fn zoom(
        &mut self,
        step: ZoomStep,
        view: Size<Pixels>,
        sf: f32,
        _cx: &mut Context<DocumentView>,
    ) -> bool {
        ImageDoc::zoom(self, step, view, sf)
    }

    fn zoom_now(&self) -> Option<ImageZoom> {
        Some(ImageDoc::zoom_now(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(w: i32, h: i32) -> Size<DevicePixels> {
        size(DevicePixels(w), DevicePixels(h))
    }

    fn view(w: f32, h: f32) -> Size<Pixels> {
        size(px(w), px(h))
    }

    /// Fit contains and never enlarges: a small icon stays its own size and
    /// sharp, and a large screenshot shrinks to the square.
    #[test]
    fn fit_never_enlarges_a_small_image() {
        assert_eq!(fit_scale(dev(64, 64), view(600.0, 400.0), 1.0), 1.0);
        // On a 2x monitor the icon is 32 logical pixels, still 1:1 in pixels.
        assert_eq!(fit_scale(dev(64, 64), view(600.0, 400.0), 2.0), 1.0);
        let big = fit_scale(dev(4000, 1000), view(600.0, 400.0), 1.0);
        assert!((big - 0.15).abs() < 1e-6, "{big}");
        let r = image_rect(
            dev(64, 64),
            view(600.0, 400.0),
            1.0,
            ImageZoom::Fit,
            point(0.0, 0.0),
        );
        assert_eq!(r.size, size(px(64.0), px(64.0)));
        assert_eq!(r.origin, point(px(268.0), px(168.0)), "centred");
    }

    /// Every step in or out keeps the image pixel at the view's centre where
    /// it was, which is what makes a zoom feel like looking closer rather
    /// than being moved somewhere else.
    #[test]
    fn zooming_keeps_the_centre_where_it_was() {
        let (img, v, sf) = (dev(3000, 2000), view(500.0, 400.0), 1.25);
        let fit = fit_scale(img, v, sf);
        let centre = point(1234.0, 876.0);
        let mut zoom = ImageZoom::Scale(1.0);
        for up in [true, true, true, false, false, false, false, true] {
            zoom = step_zoom(zoom, fit, up);
            let ImageZoom::Scale(s) = zoom else {
                continue;
            };
            let r = image_rect(img, v, sf, zoom, centre);
            let under_centre = (
                (250.0 - f32::from(r.origin.x)) * sf / s,
                (200.0 - f32::from(r.origin.y)) * sf / s,
            );
            assert!(
                (under_centre.0 - centre.x).abs() < 1e-2
                    && (under_centre.1 - centre.y).abs() < 1e-2,
                "{zoom:?}: {under_centre:?}"
            );
        }
        // The ladder: in from fit is the first step above it, out from the
        // bottom is fit, and in at the top stays at the top.
        assert_eq!(step_zoom(ImageZoom::Fit, 0.3, true), ImageZoom::Scale(0.33));
        assert_eq!(
            step_zoom(ImageZoom::Scale(0.33), 0.3, false),
            ImageZoom::Fit
        );
        assert_eq!(step_zoom(ImageZoom::Fit, 0.3, false), ImageZoom::Fit);
        assert_eq!(
            step_zoom(ImageZoom::Scale(8.0), 0.3, true),
            ImageZoom::Scale(8.0)
        );
        assert_eq!(step_zoom(ImageZoom::Fit, 1.0, true), ImageZoom::Scale(1.5));
    }

    /// A pan stops where the image's edge meets the view's: no background
    /// past an edge of an image that overflows, and nothing to pan on an
    /// axis where the image fits.
    #[test]
    fn panning_stops_at_the_images_edge() {
        let (img, v) = (dev(2000, 300), view(500.0, 400.0));
        // At 1:1 the image overflows horizontally and fits vertically.
        for want in [
            point(-9000.0, -9000.0),
            point(9000.0, 9000.0),
            point(1000.0, 10.0),
        ] {
            let c = clamp_centre(want, img, v, 1.0, 1.0);
            let r = image_rect(img, v, 1.0, ImageZoom::Scale(1.0), c);
            let (x, w) = (f32::from(r.origin.x), f32::from(r.size.width));
            assert!(x <= 0.0 && x + w >= 500.0, "{want:?}: {x}..{}", x + w);
            assert_eq!(c.y, 150.0, "{want:?}: an axis that fits stays centred");
        }
        let left = clamp_centre(point(-9000.0, 0.0), img, v, 1.0, 1.0);
        assert_eq!(left.x, 250.0, "the left edge meets the view's left edge");
        // Twice as large on a 2x monitor draws the same logical size.
        let hidpi = clamp_centre(point(-9000.0, 0.0), img, v, 2.0, 2.0);
        assert_eq!(hidpi.x, 250.0);
    }

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
