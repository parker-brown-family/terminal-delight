//! A sticky note pinned to a pane's glass.
//!
//! `alt+s` sticks one to the top-right of the focused terminal and hands it the
//! cursor, and pressing it again puts the pen down; so does Enter, and so does
//! clicking anywhere off the paper. `alt+backspace` peels it off. It survives a
//! restart beside the pane's cwd, so coming back to a wall of twenty agents you
//! read the notes instead of scrolling each one for your last prompt.
//!
//! **Right-clicking the paper sticks a PIN through it**, and the pin surfaces on
//! the pane's tab in the mother bar. That is the reminder half of the feature: a
//! note only exists on the tab you are already looking at, so a note asking you
//! to come back to something can only ask it of a tab you are not on. Nothing
//! but a second right-click takes the pin out — see [`right_click`].
//!
//! Three things here are deliberate and easy to undo by accident:
//!
//! * **Esc never removes a posted note.** It reverts the composer, because a
//!   composer is a mode you can see you are in. Once posted the note is an
//!   object sitting on someone's terminal, and swallowing an Esc that the pane
//!   below would have read as an interrupt is the bug this feature must not
//!   ship (see [`press`]).
//! * **The note is PRE-WARPED, not cut out of the warp.** A pane's barrel
//!   distortion is a pixel post-pass, so the note is drawn through the same map
//!   the pass will later undo and comes out flat on a bent tube — see [`Warp`].
//!   The earlier approach punched a curvature-free hole for it, which cost a
//!   visible seam; the note now shares its pane's glass exactly.
//! * **Nothing under the tilt is hit-tested by gpui.** Glyph sprites and filled
//!   paths carry the rotation; layout, content masks and hit-testing stay in the
//!   flat box. Clicks resolve through [`Hit::at`], which inverts the rotation —
//!   the same contract the alt-click copy affordance works under. Clicks ignore
//!   the pre-warp on purpose: it exists to cancel out, so what the pointer sees
//!   is the note's plain untransformed box.

use gpui::{
    font, hsla, linear_color_stop, linear_gradient, point, px, size, App, Bounds, Font, FontWeight,
    Hsla, PathBuilder, Pixels, Point, Size, TextAlign, TextRun, TransformationMatrix, Window,
};

/// The hand the note is written in. Caveat (SIL OFL) is bundled and registered
/// at startup beside the crawl font, so a box with no handwriting face installed
/// still gets handwriting — and every box gets the SAME handwriting.
pub const FONT_FAMILY: &str = "Caveat";

/// How solid the paper is. A note you can read the terminal faintly through
/// reads as something laid ON the screen rather than a panel cut into it, and it
/// costs nothing: this is the fill's alpha, not a blur or a second pass.
const PAPER_ALPHA: f32 = 0.85;

/// Ink is a pen, not a printout: ask the variable face for a heavier weight than
/// body text so thin strokes survive at note size on a warped tube.
fn note_font() -> Font {
    let mut f = font(FONT_FAMILY);
    f.weight = FontWeight::SEMIBOLD;
    f
}

/// The headline's hand: the same pen pressed harder. Bold rather than a second
/// face, so an agent's title reads as the top line of one note, not a label
/// pasted onto it.
fn title_font() -> Font {
    let mut f = font(FONT_FAMILY);
    f.weight = FontWeight::BOLD;
    f
}

/// The headline is drawn a notch bigger than the body, in note-local units so
/// the ratio survives the shrink ladder.
const TITLE_SCALE: f32 = 1.12;

/// Longest note we accept. Past this it stops being a note and starts being a
/// document that wants a file.
pub const MAX_CHARS: usize = 160;

/// Rows of handwriting the paper holds before the text shrinks a notch.
const MAX_ROWS: usize = 4;

/// The note stuck to one pane.
#[derive(Clone, Debug)]
pub struct Sticky {
    /// The headline, when an agent left this note — a few shouty words
    /// ("GET MILK!") drawn as their own heavier first row. `None` on every
    /// human-written note: the composer is one unbroken piece of handwriting,
    /// and a title only enters through the MCP `leave_note` tool.
    pub title: Option<String>,
    /// What is written on it. While composing this is the live buffer's text.
    pub text: String,
    /// Fixes the tilt. Drawn once and persisted, so the angle survives a
    /// re-render, a theme change and a restart — a note that re-rolled its lean
    /// every frame would read as a twitch rather than a piece of paper.
    pub seed: u32,
    /// Present while the note holds the cursor.
    pub edit: Option<Edit>,
    /// A pin stuck through the paper: come back to this one. It rides up to the
    /// pane's TAB, which is the whole point — the note itself is only visible on
    /// the tab you are already looking at, and the thing you want to be reminded
    /// of is the tab you are not. Set and cleared by right-clicking the paper,
    /// and by nothing else: see [`right_click`].
    pub pinned: bool,
}

/// A note on its way to or from the state file — what it says, the seed that
/// fixes its lean, and whether it is pinned.
///
/// It exists so the three things that must travel together cannot be separated
/// by a tuple growing a field: the pin was added to a `(text, seed)` pair that
/// four call sites destructured positionally.
#[derive(Clone, Debug, PartialEq)]
pub struct Saved {
    /// The agent-note headline, if this note has one — see [`Sticky::title`].
    pub title: Option<String>,
    pub text: String,
    pub seed: u32,
    pub pinned: bool,
}

/// The composer, alive only while the note has the keyboard.
#[derive(Clone, Debug)]
pub struct Edit {
    pub buf: crate::EditBuffer,
    /// What Esc puts back. `None` means the note did not exist before this edit,
    /// so Esc takes the blank paper away rather than reverting it to nothing.
    pub restore: Option<String>,
}

impl Sticky {
    /// A fresh note taking the cursor, seeded with `prefill` selected (empty for
    /// a brand-new one; the last peeled text when recovering from a stray
    /// `alt+backspace`, so the first keystroke still replaces it).
    pub fn composing(prefill: &str, seed: u32) -> Self {
        Self {
            title: None,
            text: prefill.to_string(),
            seed,
            edit: Some(Edit {
                buf: crate::EditBuffer::seeded(prefill),
                restore: None,
            }),
            pinned: false,
        }
    }

    /// Re-open a posted note for editing, remembering what Esc restores.
    pub fn reopen(&mut self) {
        if self.edit.is_none() {
            self.edit = Some(Edit {
                buf: crate::EditBuffer::seeded(&self.text),
                restore: Some(self.text.clone()),
            });
        }
    }

    pub fn is_editing(&self) -> bool {
        self.edit.is_some()
    }

    /// Degrees clockwise. Alternates sign off a seed bit so two notes on
    /// neighbouring panes don't lean the same way, and never sits at 0 — a
    /// sticky that happens to land square reads as a mis-drawn rectangle.
    pub fn tilt(&self) -> f32 {
        let magnitude = 2.4 + (self.seed >> 3 & 0xff) as f32 / 255.0 * 3.4;
        if self.seed & 1 == 0 {
            magnitude
        } else {
            -magnitude
        }
    }
}

/// A pane's barrel distortion, as the map to DRAW a flat thing through.
///
/// The CRT pass is a post-process: the pixel it writes at `p` is the pixel it
/// read at `q(p)`, where `q` pushes outward from the tube's centre by
/// `f = 1 + k1·r² + k2·r⁴`. So a thing drawn at `q(D)` is a thing that APPEARS
/// at `D` — running the note's geometry through `q` cancels the bend, and the
/// note comes out flat on curved glass without cutting a hole in the glass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Warp {
    origin: (f32, f32),
    size: (f32, f32),
    k1: f32,
    k2: f32,
}

impl Warp {
    /// A pane with no curvature — or any frame where a modal has flattened the
    /// whole screen. Costs nothing and moves nothing.
    pub const FLAT: Warp = Warp {
        origin: (0.0, 0.0),
        size: (1.0, 1.0),
        k1: 0.0,
        k2: 0.0,
    };

    pub fn new(content: Bounds<Pixels>, k1: f32, k2: f32) -> Warp {
        let (sx, sy) = (
            f32::from(content.size.width),
            f32::from(content.size.height),
        );
        if sx <= 0.0 || sy <= 0.0 || (k1.abs() < 0.0005 && k2.abs() < 0.0005) {
            return Warp::FLAT;
        }
        Warp {
            origin: (f32::from(content.origin.x), f32::from(content.origin.y)),
            size: (sx, sy),
            k1,
            k2,
        }
    }

    fn is_flat(&self) -> bool {
        self.k1.abs() < 0.0005 && self.k2.abs() < 0.0005
    }

    /// `q`, exactly as `fs_crt` computes it. Applied per point, so the note's
    /// straight edges are drawn as the curves that come out straight — which is
    /// the whole reason the paper is built as polylines rather than as four
    /// lines and a Bézier.
    fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        if self.is_flat() {
            return (x, y);
        }
        let (ox, oy) = self.origin;
        let (sx, sy) = self.size;
        let cx = (x - ox) / sx - 0.5;
        let cy = (y - oy) / sy - 0.5;
        let r2 = cx * cx + cy * cy;
        let f = 1.0 + self.k1 * r2 + self.k2 * r2 * r2;
        (ox + sx * (0.5 + cx * f), oy + sy * (0.5 + cy * f))
    }

    /// The same map linearised about `about` — the only form glyphs can carry.
    ///
    /// Sprites take an affine and nothing richer, so the WRITING cannot follow
    /// the exact curve the paper does. It doesn't need to: this is fitted about
    /// the text block, which is a good deal smaller than the sheet, and the
    /// residual grows with the square of the distance from where it is pinned.
    /// Over the writing it stays inside a pixel; fitting the same affine over the
    /// whole sheet instead measured seven times worse at the corners, which is
    /// what pushed the paper onto the exact map.
    fn affine_about(&self, about: (f32, f32)) -> Affine {
        if self.is_flat() {
            return Affine::IDENTITY;
        }
        let (ox, oy) = self.origin;
        let (sx, sy) = self.size;
        let (ax, ay) = about;
        let cx = (ax - ox) / sx - 0.5;
        let cy = (ay - oy) / sy - 0.5;
        let r2 = cx * cx + cy * cy;
        let f = 1.0 + self.k1 * r2 + self.k2 * r2 * r2;
        // The outer-product term is what makes this more than a scale: the
        // stretch is stronger along the radius than across it, which is exactly
        // the distortion being undone.
        let g = 2.0 * (self.k1 + 2.0 * self.k2 * r2);
        let c = [cx, cy];
        let s = [sx, sy];
        let mut m = [[0.0f32; 2]; 2];
        for (i, row) in m.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                let delta = if i == j { f } else { 0.0 };
                *cell = delta + g * c[i] * c[j] * s[i] / s[j];
            }
        }
        let centre = self.apply(ax, ay);
        Affine {
            m,
            t: [
                centre.0 - (m[0][0] * ax + m[0][1] * ay),
                centre.1 - (m[1][0] * ax + m[1][1] * ay),
            ],
        }
    }
}

/// A 2×3 affine in window coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    /// Row-major 2×2.
    pub m: [[f32; 2]; 2],
    pub t: [f32; 2],
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        m: [[1.0, 0.0], [0.0, 1.0]],
        t: [0.0, 0.0],
    };

    /// Apply it on the CPU. Production never does — the glyph matrix is handed
    /// to the GPU, which applies it per sprite — so this exists to let the tests
    /// measure the map the renderer is about to use.
    #[cfg(test)]
    fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.m[0][0] * x + self.m[0][1] * y + self.t[0],
            self.m[1][0] * x + self.m[1][1] * y + self.t[1],
        )
    }
}

/// A note's box on the glass. `None` from [`layout`] when the pane is too small
/// to carry one without becoming a note with a terminal behind it.
pub struct Layout {
    /// Centre in window coordinates — the pivot every rotation here turns about,
    /// and where the note APPEARS, which is not where it is drawn on a bent tube.
    pub center: Point<Pixels>,
    pub size: Size<Pixels>,
    /// Degrees clockwise.
    pub tilt: f32,
    pub font_size: Pixels,
    pub line_height: Pixels,
    /// Inset from the paper edge to the writing.
    pub pad: Pixels,
    /// Cancels the pane's barrel warp. Flat until [`Layout::pre_warp`] is
    /// called, so anything that asks a geometric question — hit-testing above
    /// all — works in the plain, undistorted box the user sees.
    pub warp: Warp,
}

/// Below this the note would cover the terminal rather than annotate it. Parker
/// runs panes at 390px wide in a tiled layout, so this is a live case, not a
/// theoretical one.
const MIN_PANE: (f32, f32) = (250.0, 190.0);

impl Layout {
    /// Arm the pre-warp for painting. Callers that only measure or hit-test skip
    /// this and keep the identity.
    pub fn pre_warp(&mut self, content: Bounds<Pixels>, k1: f32, k2: f32) {
        self.warp = Warp::new(content, k1, k2);
    }

    /// Note-local (unrotated, centre-origin) to the window point to DRAW at.
    fn to_window(&self, x: f32, y: f32) -> Point<Pixels> {
        let (sin, cos) = self.tilt.to_radians().sin_cos();
        let (rx, ry) = (x * cos - y * sin, x * sin + y * cos);
        let (wx, wy) = self
            .warp
            .apply(f32::from(self.center.x) + rx, f32::from(self.center.y) + ry);
        point(px(wx), px(wy))
    }

    /// Where the writing starts, in note-local space.
    fn text_origin(&self) -> Point<Pixels> {
        point(
            -self.size.width / 2.0 + self.pad,
            -self.size.height / 2.0 + self.pad * 0.9,
        )
    }

    /// How wide the writing may run.
    ///
    /// Wider than the padding on the left, narrower on the right: the paper is
    /// drawn through the exact barrel map and the writing only through an affine
    /// fit to it, so on a bent pane the two disagree by a couple of pixels, and
    /// that disagreement is worst at the right-hand edge — furthest from where
    /// the fit is pinned and furthest out on the tube. The slack costs a
    /// character of line length and means the ink never reaches the paper's edge.
    fn text_width(&self) -> Pixels {
        self.size.width - self.pad * 2.4
    }

    /// The transform the note's glyph sprites carry: the tilt, then the pre-warp
    /// linearised about `block` — the centre of the writing in note-local space,
    /// NOT the centre of the sheet. Fitting it where the text actually is keeps
    /// the writing on the paper; fitting it at the sheet's centre would spend the
    /// residual right where it is read.
    ///
    /// Physical pixels, since that is the space sprite quads live in — only the
    /// translation scales, because the 2×2 is dimensionless.
    fn text_matrix(&self, scale_factor: f32, block: Point<Pixels>) -> TransformationMatrix {
        let (sin, cos) = self.tilt.to_radians().sin_cos();
        let (bx, by) = (f32::from(block.x), f32::from(block.y));
        let about = (
            f32::from(self.center.x) + bx * cos - by * sin,
            f32::from(self.center.y) + bx * sin + by * cos,
        );
        let warp = self.warp.affine_about(about);
        let rot = [[cos, -sin], [sin, cos]];
        let (px_, py_) = (
            f32::from(self.center.x) * scale_factor,
            f32::from(self.center.y) * scale_factor,
        );
        // Rotation about the centre, as a plain affine.
        let rt = [
            px_ - (rot[0][0] * px_ + rot[0][1] * py_),
            py_ - (rot[1][0] * px_ + rot[1][1] * py_),
        ];
        let w = &warp;
        let mut m = [[0.0f32; 2]; 2];
        for (i, row) in m.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell = w.m[i][0] * rot[0][j] + w.m[i][1] * rot[1][j];
            }
        }
        let t = [
            w.m[0][0] * rt[0] + w.m[0][1] * rt[1] + w.t[0] * scale_factor,
            w.m[1][0] * rt[0] + w.m[1][1] * rt[1] + w.t[1] * scale_factor,
        ];
        TransformationMatrix {
            rotation_scale: m,
            translation: t,
        }
    }
}

/// Fit a note into `content` — a pane's terminal area, window coordinates.
pub fn layout(content: Bounds<Pixels>, tilt: f32) -> Option<Layout> {
    let (w, h) = (
        f32::from(content.size.width),
        f32::from(content.size.height),
    );
    if w < MIN_PANE.0 || h < MIN_PANE.1 {
        return None;
    }
    // Wide enough to hold a phrase of handwriting, never so wide it owns the
    // pane. A 390px tiled pane gets the floor; a full-screen one gets the cap.
    // The FLOOR stays where it was while the cap goes up: a 390px tiled pane has
    // no room to spare, so the extra presence is something only a wide pane buys.
    let note_w = (w * 0.28).clamp(128.0, 200.0);
    let note_h = note_w * 0.92;
    let font_size = (note_w * 0.118).clamp(15.0, 22.0);
    Some(Layout {
        // Close to the corner on purpose. The first pre-warped build sat further
        // in and lost its presence: cancelling the barrel makes the note conform
        // to the glass instead of riding proud of it the way the cut-out version
        // did, so the presence has to come from where it sits rather than from
        // disagreeing with the curve.
        //
        // The extra tenth-of-a-sheet off each edge is Parker's calibration on the
        // built thing, and it is written in the note's OWN height rather than as
        // pixels so a 128px note on a tiled pane and a 200px one on a full screen
        // sit at the same place by eye instead of the same distance by number.
        center: point(
            content.origin.x + px(w - note_w * 0.5 - 24.0 - note_h * 0.10),
            content.origin.y + px(note_h * 0.5 + 22.0 + note_h * 0.10),
        ),
        size: size(px(note_w), px(note_h)),
        tilt,
        font_size: px(font_size),
        line_height: px(font_size * 1.26),
        pad: px(note_w * 0.11),
        warp: Warp::FLAT,
    })
}

pub struct Paper {
    pub base: Hsla,
    pub ink: Hsla,
    /// The pushpin's head. The paper's OPPOSITE hue, which is the one colour
    /// guaranteed to survive whatever the sheet turned out to be: a red pin is
    /// the physical object, but a red pin on the hot-pink paper a pink theme
    /// produces is a bump, not a flag.
    pub pin: Hsla,
}

/// The note's palette: a saturated sheet in the theme's own voice, written on in
/// black.
///
/// The hue comes from the theme's PRIMARY TEXT — the colour that pane is already
/// speaking in — and is then driven to full chroma at pad lightness. Saturation
/// is the point: a pastel note dissolves into a dark terminal, and the thing on
/// the desk this imitates is a hot pink or lime pad, not a cream one. The ink is
/// near-black, holding a trace of the same hue so it reads as ink on that paper
/// rather than a UI label dropped on top.
///
/// A theme whose text has no chroma at all (white on black) falls through to its
/// accent, and then to a classic sticky yellow. Grey paper reads as a dialog.
pub fn paper(text: Hsla, accent: Hsla) -> Paper {
    let hue = if text.s >= 0.10 {
        text.h
    } else if accent.s >= 0.10 {
        accent.h
    } else {
        0.128
    };
    Paper {
        base: hsla(hue, 0.88, 0.63, PAPER_ALPHA),
        ink: hsla(hue, 0.24, 0.085, 0.95),
        pin: hsla((hue + 0.5).fract(), 0.86, 0.52, 1.0),
    }
}

fn shift(c: Hsla, factor: f32) -> Hsla {
    Hsla {
        l: (c.l * factor).clamp(0.0, 1.0),
        ..c
    }
}

/// One draw from a note's seed, in `-0.5..0.5`. Independent per `slot`, so the
/// four corners wobble independently instead of in lockstep.
fn jitter(seed: u32, slot: u32) -> f32 {
    let mut x = seed
        .wrapping_mul(2_654_435_761)
        .wrapping_add(slot.wrapping_mul(0x9e37_79b9));
    x ^= x >> 15;
    x = x.wrapping_mul(0x85eb_ca6b);
    x ^= x >> 13;
    (x & 0xffff) as f32 / 65535.0 - 0.5
}

/// The sheet's four corners in note-local space, centred on the origin.
///
/// The edges are STRAIGHT and the corners SHARP — paper is cut, not moulded, and
/// bowing the sides read as a pillow. What sells it instead is that the rectangle
/// is slightly wrong: each corner takes an independent ±1.6px draw off the note's
/// seed, so no two notes are the same quadrilateral and none is a drawn rectangle.
///
/// Only the free bottom edge curves, and only a little.
struct Sheet {
    tl: (f32, f32),
    tr: (f32, f32),
    br: (f32, f32),
    bl: (f32, f32),
    /// How far the bottom edge rises through the middle as the sheet lifts.
    bow: f32,
    /// Thickness of the lifted lip along that edge.
    lip: f32,
}

impl Sheet {
    fn new(sz: Size<Pixels>, seed: u32, lift: f32) -> Sheet {
        let (hw, hh) = (f32::from(sz.width) / 2.0, f32::from(sz.height) / 2.0);
        let h = f32::from(sz.height);
        let j = |slot| jitter(seed, slot) * 3.2;
        Sheet {
            tl: (-hw + j(1), -hh + j(2)),
            tr: (hw + j(3), -hh + j(4)),
            br: (hw + j(5), hh + j(6)),
            bl: (-hw + j(7), hh + j(8)),
            bow: h * 0.014 * lift,
            lip: h * 0.030 * lift,
        }
    }

    /// Control point for the bottom edge's quadratic. The midpoint of the curve
    /// lands `bow` above the straight line, so `bow` is the real rise.
    fn bottom_ctrl(&self) -> (f32, f32) {
        (
            (self.bl.0 + self.br.0) / 2.0,
            (self.bl.1 + self.br.1) / 2.0 - self.bow * 2.0,
        )
    }

    /// The paper's outline as a note-local polyline. `grow` and `nudge` apply in
    /// note-local space, so the shadow stack scales about the sheet's own centre
    /// rather than the window's.
    fn outline(&self, grow: f32, nudge: (f32, f32)) -> Vec<(f32, f32)> {
        let mut v = Vec::with_capacity(SEG * 3 + 1);
        line(&mut v, self.tl, self.tr);
        line(&mut v, self.tr, self.br);
        quadratic(&mut v, self.br, self.bottom_ctrl(), self.bl);
        line(&mut v, self.bl, self.tl);
        for p in v.iter_mut() {
            *p = (p.0 * grow + nudge.0, p.1 * grow + nudge.1);
        }
        v
    }

    /// The lifted lip along the bottom edge.
    ///
    /// ASYMMETRIC, and that is the whole trick: on a real note one corner comes
    /// away first, so the pale underside is a wedge — thick at the bottom-left,
    /// tapering to nothing at the right. A band of even thickness across the
    /// bottom reads as a printed border, and a wide one turns the note into a
    /// ribbon. It lifts at the same corner the pointer peels from.
    fn curl(&self) -> Vec<(f32, f32)> {
        let c = self.bottom_ctrl();
        let (deep, shallow) = (self.lip, self.lip * 0.38);
        let mut v = Vec::new();
        quadratic(&mut v, self.br, c, self.bl);
        line(&mut v, self.bl, (self.bl.0, self.bl.1 - deep));
        quadratic(
            &mut v,
            (self.bl.0, self.bl.1 - deep),
            (c.0, c.1 - deep * 0.55),
            (self.br.0, self.br.1 - shallow),
        );
        v
    }

    /// The crease where the sheet leaves the glass — a hairline of shadow along
    /// the top of the lip, tapering with it. Without it the lip reads as a paler
    /// stripe painted on the paper rather than as an edge coming off it.
    fn crease(&self) -> Vec<(f32, f32)> {
        let c = self.bottom_ctrl();
        let (deep, shallow) = (self.lip, self.lip * 0.38);
        let w = (self.lip * 0.30).max(0.9);
        let mut v = Vec::new();
        quadratic(
            &mut v,
            (self.bl.0, self.bl.1 - deep),
            (c.0, c.1 - deep * 0.55),
            (self.br.0, self.br.1 - shallow),
        );
        line(
            &mut v,
            (self.br.0, self.br.1 - shallow),
            (self.br.0, self.br.1 - shallow - w),
        );
        quadratic(
            &mut v,
            (self.br.0, self.br.1 - shallow - w),
            (c.0, c.1 - deep * 0.55 - w),
            (self.bl.0, self.bl.1 - deep - w),
        );
        v
    }

    /// One layer of the drop shadow, `t` px deep.
    ///
    /// Not the outline scaled up — a scaled copy pokes out ABOVE and LEFT of the
    /// paper too, and a stack of them gave the note a dark halo that read as a
    /// blob. This pools the shadow where a sheet glued at the top actually casts
    /// one: barely at the glued edge, spreading under the lifted bottom.
    fn shadow(&self, t: f32) -> Vec<(f32, f32)> {
        let (dx, top, bot) = (t * 0.30, t * 0.22, t * 1.15);
        let c = self.bottom_ctrl();
        let mut v = Vec::new();
        line(
            &mut v,
            (self.tl.0, self.tl.1 + top),
            (self.tr.0, self.tr.1 + top),
        );
        line(
            &mut v,
            (self.tr.0, self.tr.1 + top),
            (self.br.0, self.br.1 + bot),
        );
        quadratic(
            &mut v,
            (self.br.0, self.br.1 + bot),
            (c.0, c.1 + bot),
            (self.bl.0, self.bl.1 + bot),
        );
        line(
            &mut v,
            (self.bl.0, self.bl.1 + bot),
            (self.tl.0, self.tl.1 + top),
        );
        for p in v.iter_mut() {
            p.0 += dx;
        }
        v
    }
}

/// Segments a note-local edge is flattened into before the barrel map is applied
/// per point. The map is smooth over a note-sized patch, so this is about
/// sampling density rather than accuracy — eight is already past the point where
/// a straight edge stops reading as a chain of chords.
const SEG: usize = 8;

/// Flatten a straight edge. Emits the start and the intermediate points, never
/// the end — the next edge contributes that, so a closed outline has no
/// duplicated vertices.
fn line(out: &mut Vec<(f32, f32)>, a: (f32, f32), b: (f32, f32)) {
    for i in 0..SEG {
        let t = i as f32 / SEG as f32;
        out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
    }
}

/// Flatten a quadratic Bézier, same convention.
fn quadratic(out: &mut Vec<(f32, f32)>, a: (f32, f32), c: (f32, f32), b: (f32, f32)) {
    for i in 0..SEG * 2 {
        let t = i as f32 / (SEG * 2) as f32;
        let u = 1.0 - t;
        out.push((
            u * u * a.0 + 2.0 * u * t * c.0 + t * t * b.0,
            u * u * a.1 + 2.0 * u * t * c.1 + t * t * b.1,
        ));
    }
}

/// A closed polygon in note-local space, edge by edge, so every side is
/// flattened for the barrel map the same way the sheet's are.
fn poly(pts: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut v = Vec::new();
    for (i, a) in pts.iter().enumerate() {
        line(&mut v, *a, pts[(i + 1) % pts.len()]);
    }
    v
}

/// An axis-aligned quad in note-local space.
fn quad(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<(f32, f32)> {
    let mut v = Vec::new();
    line(&mut v, (x0, y0), (x1, y0));
    line(&mut v, (x1, y0), (x1, y1));
    line(&mut v, (x1, y1), (x0, y1));
    line(&mut v, (x0, y1), (x0, y0));
    v
}

/// A circle in note-local space. Flattened four times as densely as an edge:
/// the pin's head is the smallest round thing on the sheet, and a chord that
/// reads as straight across a 200px sheet reads as a facet across a 9px head.
fn disc(x: f32, y: f32, r: f32) -> Vec<(f32, f32)> {
    let n = SEG * 4;
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            (x + r * a.cos(), y + r * a.sin())
        })
        .collect()
}

/// Fill a note-local polyline, mapping every vertex through the tilt and the
/// pre-warp. This is where the paper stops being four lines and becomes the
/// shape that comes out flat on a bent tube.
fn fill(
    lay: &Layout,
    poly: &[(f32, f32)],
    background: impl Into<gpui::Background>,
    window: &mut Window,
) {
    let Some((first, rest)) = poly.split_first() else {
        return;
    };
    let mut b = PathBuilder::fill();
    b.move_to(lay.to_window(first.0, first.1));
    for p in rest {
        b.line_to(lay.to_window(p.0, p.1));
    }
    b.close();
    if let Ok(path) = b.build() {
        window.paint_path(path, background);
    }
}

/// What a keystroke means to the note.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Press {
    /// Post it and hand the cursor back to the terminal.
    Post,
    /// Put the draft back — or, for a note that did not exist before this edit,
    /// take the blank paper away again.
    Revert,
    /// Write on the paper.
    Write,
    /// The note wants nothing to do with this key: it belongs to whatever is
    /// running underneath.
    Pass,
}

/// The single decision about whether a key reaches the PTY.
///
/// It exists as a pure function so the rule the whole feature rests on is
/// testable: **a POSTED note claims nothing.** A note lives on a pane for hours
/// and you stop seeing it, so if it could eat a key, whether Esc interrupted
/// your agent would depend on invisible state you set up this morning. Composing
/// is different — that is a mode with a caret blinking in it, and Esc unwinding
/// a mode you can see is the same contract as the inline rename box.
pub fn press(composing: bool, key: &str) -> Press {
    if !composing {
        return Press::Pass;
    }
    match key {
        "enter" => Press::Post,
        "escape" => Press::Revert,
        _ => Press::Write,
    }
}

/// What a gesture at the note does.
///
/// There are three ways in and out — `alt+s`, a click on the paper, a click
/// anywhere else — and they all have to agree with Enter about what "done"
/// means. Routing them through one enum is what stops that drifting.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    /// Give the note the cursor (or keep it, if it already has it).
    Open,
    /// Post what is written and hand the cursor back — exactly Enter.
    Post,
    /// Tear the note off.
    Peel,
    /// Push the pin in, or pull it back out — a toggle, see [`right_click`].
    Pin,
    /// Nothing to do with the note; the pane handles the gesture.
    Pass,
}

/// `alt+s` — a TOGGLE.
///
/// The chord that gives the note the cursor takes it back, so the way out is the
/// way in and Enter is not the only exit. It never destroys: a chord that both
/// created and destroyed would make a blind press a coin flip over whatever was
/// written, which is why peeling stayed a separate gesture.
pub fn alt_s(composing: bool) -> Act {
    if composing {
        Act::Post
    } else {
        Act::Open
    }
}

/// A left click, given where it landed.
///
/// Clicking away from a note that is being written POSTS it, the same way
/// clicking away from the tab-rename box saves the rename — you have moved on,
/// and an editor that keeps the keyboard after you have visibly gone somewhere
/// else is an editor that eats the next thing you type. The click still reaches
/// the terminal underneath; committing is not a reason to swallow it.
pub fn click(composing: bool, hit: Option<Hit>) -> Act {
    match hit {
        Some(Hit::Peel) => Act::Peel,
        Some(Hit::Body) => Act::Open,
        None if composing => Act::Post,
        None => Act::Pass,
    }
}

/// A right click, given where it landed.
///
/// Anywhere on the paper pins it — INCLUDING the peel corner. The corner is a
/// left-click affordance, and making the same pixels destroy a note under one
/// button while flagging it under the other is the kind of thing you find out
/// about by losing a note. Right-clicking the note therefore has exactly one
/// meaning wherever it lands, and it is the only way in or out of the pinned
/// state: no keystroke sets it, no other gesture clears it, and posting or
/// re-editing the note leaves it alone. A pin you can drop by accident is not a
/// reminder, because you stop trusting that it is still there.
///
/// A miss passes straight through to the pane, which opens its copy/paste menu
/// as it always has.
pub fn right_click(hit: Option<Hit>) -> Act {
    match hit {
        Some(_) => Act::Pin,
        None => Act::Pass,
    }
}

/// Where a click landed on the note.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    /// The lifted bottom-left corner: tear it off.
    Peel,
    /// Anywhere else on the paper: pick up the pen.
    Body,
}

impl Hit {
    /// Resolve a window-space point against a note, inverting the tilt. Returns
    /// `None` for a point that misses the paper, which is every point when there
    /// is no note — so the caller's normal click handling is unaffected.
    ///
    /// Deliberately blind to [`Layout::warp`]: the pre-warp is cancelled by the
    /// CRT pass, so the note the pointer meets is the plain box below.
    pub fn at(p: Point<Pixels>, lay: &Layout) -> Option<Hit> {
        let (dx, dy) = (f32::from(p.x - lay.center.x), f32::from(p.y - lay.center.y));
        let (sin, cos) = (-lay.tilt).to_radians().sin_cos();
        let (lx, ly) = (dx * cos - dy * sin, dx * sin + dy * cos);
        let (hw, hh) = (
            f32::from(lay.size.width) / 2.0,
            f32::from(lay.size.height) / 2.0,
        );
        if lx.abs() > hw || ly.abs() > hh {
            return None;
        }
        // The peel corner is a triangle in the bottom-left, sized so it is a
        // comfortable target at the 128px floor and doesn't swallow the writing
        // at the cap.
        let corner = (hw * 0.42).min(30.0);
        if lx < -hw + corner && ly > hh - corner && (lx + hw) + (hh - ly) < corner {
            return Some(Hit::Peel);
        }
        Some(Hit::Body)
    }
}

/// Paint the note. Called from the pane's paint pass, after the grid.
///
/// `hover_peel` lifts the bottom edge further, which is the whole teaching for
/// the mouse gesture: the corner curls under the pointer, so it reads as
/// something you can pull.
pub fn paint(
    note: &Sticky,
    lay: &Layout,
    pal: &Paper,
    hover_peel: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let lift = if hover_peel { 2.1 } else { 1.0 };
    let sheet = Sheet::new(lay.size, note.seed, lift);

    // One ambient ring first — a single grown copy, not a stack of them, which
    // is what made the first pass a blob.
    fill(
        lay,
        &sheet.outline(1.055, (0.6, 1.4)),
        hsla(0.0, 0.0, 0.0, 0.11),
        window,
    );
    // Softness is a stack of layers, since gpui fills paths flat. Each pools
    // toward the lifted bottom rather than spreading evenly.
    for i in (1..=9).rev() {
        fill(
            lay,
            &sheet.shadow(i as f32),
            hsla(0.0, 0.0, 0.0, 0.035),
            window,
        );
    }

    // The sheet. Lit from above: brighter where it lies flat against the glass,
    // falling off toward the free edge. The gradient is turned by the tilt so it
    // runs down the PAPER rather than down the screen.
    let paper_poly = sheet.outline(1.0, (0.0, 0.0));
    fill(
        lay,
        &paper_poly,
        linear_gradient(
            180.0 + lay.tilt,
            linear_color_stop(shift(pal.base, 1.10), 0.0),
            linear_color_stop(shift(pal.base, 0.88), 1.0),
        ),
        window,
    );

    // One wash down the right-hand side. The first pass had two, which pinched
    // the sheet in the middle and read as a pillow; with straight edges the
    // paper only needs the far side to fall away from the light.
    fill(
        lay,
        &paper_poly,
        linear_gradient(
            270.0 + lay.tilt,
            linear_color_stop(hsla(0.0, 0.0, 0.0, 0.062), 0.0),
            linear_color_stop(hsla(0.0, 0.0, 0.0, 0.0), 0.45),
        ),
        window,
    );

    // The adhesive band across the top reads as a slightly different sheen.
    let (hw, hh) = (
        f32::from(lay.size.width) / 2.0,
        f32::from(lay.size.height) / 2.0,
    );
    fill(
        lay,
        &quad(-hw, -hh, hw, -hh + hh * 0.26),
        linear_gradient(
            180.0 + lay.tilt,
            linear_color_stop(hsla(0.0, 0.0, 1.0, 0.09), 0.0),
            linear_color_stop(hsla(0.0, 0.0, 1.0, 0.0), 1.0),
        ),
        window,
    );

    // The crease, then the lip it belongs to: the sheet's pale underside, lit
    // because the curl tips it up toward the light.
    fill(lay, &sheet.crease(), hsla(0.0, 0.0, 0.0, 0.10), window);
    fill(
        lay,
        &sheet.curl(),
        linear_gradient(
            180.0 + lay.tilt,
            linear_color_stop(shift(pal.base, 0.98), 0.0),
            linear_color_stop(shift(pal.base, 1.14), 1.0),
        ),
        window,
    );

    paint_writing(note, lay, pal, window, cx);

    if note.pinned {
        paint_pin(lay, pal, window);
    }
}

/// The pushpin, stuck through the top edge.
///
/// It straddles the edge rather than sitting on the sheet: the head is mostly
/// ABOVE the paper and only the needle crosses into the adhesive band, so the
/// pin never lands on a word. Putting it in the middle of the sheet was the
/// first try and it covered the first line of every four-line note — the one
/// line you can read from across the room.
fn paint_pin(lay: &Layout, pal: &Paper, window: &mut Window) {
    let (hw, hh) = (
        f32::from(lay.size.width) / 2.0,
        f32::from(lay.size.height) / 2.0,
    );
    // Scaled off the sheet so a 128px tiled note and a 200px full-screen one
    // carry the same pin by eye, with a floor that keeps it a recognisable
    // object rather than a dot.
    let r = (hw * 0.17).clamp(7.0, 16.0);
    // The head sits mostly ABOVE the sheet; the needle is what crosses onto it.
    // The first build had the tip at the head's own bottom edge, so the needle
    // was drawn entirely underneath the head and the whole thing read as a
    // bubble peeking over the top of the note. The needle has to CLEAR the head
    // or there is no pin, only a dome.
    let head = -hh - r * 0.35;
    let collar = -hh + r * 0.45;
    let tip = -hh + r * 2.1;

    // Shadow, on the paper only: a pool under the head and a thin one along the
    // needle, both thrown down-right by the light the sheet is lit by.
    fill(
        lay,
        &disc(r * 0.45, -hh + r * 0.75, r * 0.85),
        hsla(0.0, 0.0, 0.0, 0.13),
        window,
    );
    fill(
        lay,
        &poly(&[
            (r * 0.12, collar),
            (r * 0.46, collar),
            (r * 0.62, tip),
            (r * 0.48, tip),
        ]),
        hsla(0.0, 0.0, 0.0, 0.10),
        window,
    );

    // The needle: a taper, not a stick, leaning with the hand that pushed it in.
    fill(
        lay,
        &poly(&[
            (-r * 0.14, collar),
            (r * 0.18, collar),
            (r * 0.30, tip),
            (r * 0.22, tip),
        ]),
        hsla(0.0, 0.0, 0.46, 0.95),
        window,
    );
    // The ferrule where the needle leaves the head — the one detail that stops
    // the head reading as a bead threaded onto a wire.
    fill(
        lay,
        &poly(&[
            (-r * 0.30, collar - r * 0.34),
            (r * 0.30, collar - r * 0.34),
            (r * 0.22, collar + r * 0.06),
            (-r * 0.20, collar + r * 0.06),
        ]),
        hsla(0.0, 0.0, 0.68, 0.95),
        window,
    );

    // The head: a rim, the dome inside it offset up-left, and one specular dot.
    // Three flat fills read as a sphere because the dot is small and high — the
    // same trick the battery pip uses.
    fill(lay, &disc(0.0, head, r), shift(pal.pin, 0.66), window);
    fill(
        lay,
        &disc(-r * 0.06, head - r * 0.07, r * 0.84),
        pal.pin,
        window,
    );
    fill(
        lay,
        &disc(-r * 0.32, head - r * 0.34, r * 0.26),
        hsla(0.0, 0.0, 1.0, 0.6),
        window,
    );
}

/// The handwriting, and the composer's selection and caret.
fn paint_writing(note: &Sticky, lay: &Layout, pal: &Paper, window: &mut Window, cx: &mut App) {
    let text = match &note.edit {
        Some(e) => e.buf.text(),
        None => note.text.clone(),
    };
    let title = note
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string);
    if text.is_empty() && title.is_none() && note.edit.is_none() {
        return;
    }

    // Shrink a notch before clipping: a note that overflows should get smaller
    // handwriting, not a truncated thought. Past the last notch it clamps, which
    // is what MAX_CHARS is there to make rare. A titled (agent) note shapes two
    // blocks per rung — the one-row headline and the body under it — and the
    // pair climbs down the ladder together, so a long title shrinks the whole
    // note rather than colliding with its own first line.
    let wrap = lay.text_width();
    let mut chosen = None;
    for scale in [1.0_f32, 0.86, 0.74] {
        let font_size = lay.font_size * scale;
        let line_height = lay.line_height * scale;
        let heading = match &title {
            None => None,
            Some(t) => {
                let run = TextRun {
                    len: t.len(),
                    font: title_font(),
                    color: pal.ink,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let Ok(lines) = window.text_system().shape_text(
                    t.clone().into(),
                    font_size * TITLE_SCALE,
                    &[run],
                    Some(wrap),
                    Some(1),
                ) else {
                    return;
                };
                lines.into_iter().next()
            }
        };
        let title_rows = usize::from(heading.is_some());
        let title_height = heading
            .as_ref()
            .map(|l| l.size(line_height * TITLE_SCALE).height)
            .unwrap_or_default();
        let body = if text.is_empty() {
            // A title-only note ("GET MILK!") has nothing to shape below the
            // headline; shaping "" would report no line and abort the paint.
            None
        } else {
            let run = TextRun {
                len: text.len(),
                font: note_font(),
                color: pal.ink,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let Ok(lines) = window.text_system().shape_text(
                text.clone().into(),
                font_size,
                &[run],
                Some(wrap),
                Some(MAX_ROWS - title_rows),
            ) else {
                return;
            };
            lines.into_iter().next()
        };
        if heading.is_none() && body.is_none() {
            return;
        }
        let body_rows = body
            .as_ref()
            .map(|l| l.wrap_boundaries().len() + 1)
            .unwrap_or(0);
        let body_height = body
            .as_ref()
            .map(|l| l.size(line_height).height)
            .unwrap_or_default();
        let fits = body_rows + title_rows <= MAX_ROWS
            && f32::from(body_height) + f32::from(title_height)
                <= f32::from(lay.size.height - lay.pad * 1.5);
        if fits || scale < 0.75 {
            chosen = Some((heading, title_height, body, line_height));
            break;
        }
    }
    let Some((heading, title_height, body, line_height)) = chosen else {
        return;
    };

    let origin_local = lay.text_origin();
    // Where the body's ink starts: directly under the headline, if there is one.
    let body_origin = point(origin_local.x, origin_local.y + title_height);
    let body_height = body
        .as_ref()
        .map(|l| l.size(line_height).height)
        .unwrap_or_default();
    // Pin the glyph affine at the middle of the WRITING, not of the sheet.
    let block = point(
        origin_local.x + wrap / 2.0,
        origin_local.y + (title_height + body_height) / 2.0,
    );
    let matrix = lay.text_matrix(window.scale_factor(), block);

    // Selection first, so the ink sits on top of it. `seeded` selects the whole
    // note when the composer opens, and a selection you cannot see is a
    // selection your next keystroke silently eats.
    if let (Some(edit), Some(line)) = (&note.edit, &body) {
        let (a, b) = edit.buf.sel_range();
        if a != b {
            let buf_text = edit.buf.text();
            let byte = |chars: usize| {
                buf_text
                    .char_indices()
                    .nth(chars)
                    .map(|(i, _)| i)
                    .unwrap_or(text.len())
            };
            if let (Some(p0), Some(p1)) = (
                line.position_for_index(byte(a), line_height),
                line.position_for_index(byte(b), line_height),
            ) {
                paint_selection(p0, p1, line_height, wrap, body_origin, lay, pal, window);
            }
        }
    }

    // The glyphs ride the same transform the paper was drawn through, so the
    // writing stays ON the paper whatever the tube is doing.
    window.with_text_transformation(matrix, |window| {
        if let Some(line) = &heading {
            let origin = point(lay.center.x + origin_local.x, lay.center.y + origin_local.y);
            if let Err(e) = line.paint(
                origin,
                line_height * TITLE_SCALE,
                TextAlign::Left,
                None,
                window,
                cx,
            ) {
                eprintln!("terminal-delight: sticky note title failed to paint: {e}");
            }
        }
        if let Some(line) = &body {
            let origin = point(lay.center.x + body_origin.x, lay.center.y + body_origin.y);
            if let Err(e) = line.paint(origin, line_height, TextAlign::Left, None, window, cx) {
                eprintln!("terminal-delight: sticky note text failed to paint: {e}");
            }
        }
    });

    // The caret is a pen resting on the paper: a short stroke on the baseline,
    // drawn as a path so it turns with the note like everything else.
    if let (Some(edit), Some(line)) = (&note.edit, &body) {
        let buf_text = edit.buf.text();
        let caret_byte = buf_text
            .char_indices()
            .nth(edit.buf.caret())
            .map(|(i, _)| i)
            .unwrap_or(text.len());
        if let Some(p) = line.position_for_index(caret_byte, line_height) {
            let x = f32::from(body_origin.x + p.x);
            let y = f32::from(body_origin.y + p.y);
            let lh = f32::from(line_height);
            fill(
                lay,
                &quad(x, y + 2.0, x + 1.9, y + lh - 3.0),
                pal.ink,
                window,
            );
        }
    }
}

/// Highlight a selected run, row by row — the note is at most four rows, so the
/// middle rows simply span the writing width.
#[allow(clippy::too_many_arguments)]
fn paint_selection(
    p0: Point<Pixels>,
    p1: Point<Pixels>,
    line_height: Pixels,
    wrap: Pixels,
    origin_local: Point<Pixels>,
    lay: &Layout,
    pal: &Paper,
    window: &mut Window,
) {
    let lh = f32::from(line_height);
    let row0 = (f32::from(p0.y) / lh).round() as i32;
    let row1 = (f32::from(p1.y) / lh).round() as i32;
    for row in row0..=row1 {
        let x0 = if row == row0 { f32::from(p0.x) } else { 0.0 };
        let x1 = if row == row1 {
            f32::from(p1.x)
        } else {
            f32::from(wrap)
        };
        if x1 - x0 < 0.5 {
            continue;
        }
        let y = row as f32 * lh;
        let (ox, oy) = (f32::from(origin_local.x), f32::from(origin_local.y));
        fill(
            lay,
            &quad(ox + x0, oy + y + 1.0, ox + x1, oy + y + lh - 1.0),
            Hsla { a: 0.22, ..pal.ink },
            window,
        );
    }
}

/// A tilt seed with no dependency on a random source: the low bits of the clock
/// mixed so consecutive notes don't land on neighbouring angles.
pub fn seed() -> u32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let mut x = nanos ^ 0x9e37_79b9;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> Bounds<Pixels> {
        Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(740.0), px(870.0)),
        }
    }

    fn lay_at(tilt: f32) -> Layout {
        Layout {
            center: point(px(500.0), px(120.0)),
            size: size(px(180.0), px(166.0)),
            tilt,
            font_size: px(20.0),
            line_height: px(25.0),
            pad: px(20.0),
            warp: Warp::FLAT,
        }
    }

    /// The barrel map, straight from `fs_crt` in `crt_pass.wgsl`. The shader is
    /// the runtime authority; this is the spec the pre-warp has to invert, and
    /// keeping a copy here is what lets the inversion be tested at all.
    fn shader_sample(content: Bounds<Pixels>, p: (f32, f32), k1: f32, k2: f32) -> (f32, f32) {
        let (sx, sy) = (
            f32::from(content.size.width),
            f32::from(content.size.height),
        );
        let cx = (p.0 - f32::from(content.origin.x)) / sx - 0.5;
        let cy = (p.1 - f32::from(content.origin.y)) / sy - 0.5;
        let r2 = cx * cx + cy * cy;
        let f = 1.0 + k1 * r2 + k2 * r2 * r2;
        (
            f32::from(content.origin.x) + sx * (0.5 + cx * f),
            f32::from(content.origin.y) + sy * (0.5 + cy * f),
        )
    }

    /// The reason this feature is not just a floating div: a pane too small for
    /// a note must get NO note, or the note becomes the pane. 390x844 is a real
    /// tiled width here, not a hypothetical.
    #[test]
    fn a_pane_too_small_carries_no_note() {
        let tiny = Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(200.), px(150.)),
        };
        assert!(layout(tiny, 4.0).is_none(), "a 200x150 pane is all note");

        let narrow = Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(390.), px(844.)),
        };
        let l = layout(narrow, 4.0).expect("a 390px tiled pane still gets a note");
        assert!(
            f32::from(l.size.width) <= 390.0 * 0.34,
            "the note must stay a corner of the pane, got {:?} of 390",
            l.size.width
        );
        assert!(f32::from(l.center.x) + f32::from(l.size.width) / 2.0 <= 390.0);
    }

    /// A tilt of zero reads as a mis-drawn rectangle, and a tilt that always
    /// leans the same way reads as a skewed widget. Both signs must occur.
    #[test]
    fn the_lean_is_never_square_and_goes_both_ways() {
        let (mut left, mut right) = (false, false);
        for seed in 0..64u32 {
            let note = Sticky {
                title: None,
                text: String::new(),
                seed,
                edit: None,
                pinned: false,
            };
            let t = note.tilt();
            assert!(
                (2.3..=6.0).contains(&t.abs()),
                "seed {seed} tilted {t}, outside the paper range"
            );
            if t < 0.0 {
                left = true;
            } else {
                right = true;
            }
        }
        assert!(left && right, "every seed leaned the same way");
    }

    /// The same note must lean the same way every frame and across a restart —
    /// the seed is persisted precisely so the angle is not re-rolled.
    #[test]
    fn the_lean_is_stable_for_a_seed() {
        let a = Sticky {
            title: None,
            text: "x".into(),
            seed: 12345,
            edit: None,
            pinned: false,
        };
        let b = Sticky {
            title: None,
            text: "different text".into(),
            seed: 12345,
            edit: None,
            pinned: false,
        };
        assert_eq!(a.tilt(), b.tilt(), "the tilt comes from the seed alone");
    }

    /// THE regression guard for this feature.
    ///
    /// A posted note must claim NOTHING — above all not Esc, which is how you
    /// stop a running agent. If this ever fails, whether Esc reaches Claude
    /// depends on whether a pane happens to be carrying a note, which is state
    /// the user set up hours ago and has stopped seeing.
    #[test]
    fn a_posted_note_claims_no_key_at_all() {
        for key in [
            "escape",
            "enter",
            "backspace",
            "c",
            "tab",
            "up",
            "left",
            "f1",
            "space",
        ] {
            assert_eq!(
                press(false, key),
                Press::Pass,
                "a posted note claimed {key} — it must reach the terminal"
            );
        }
    }

    /// While composing, the note owns the keyboard completely: a letter that
    /// leaked past the composer lands in whatever is running behind it.
    #[test]
    fn composing_owns_the_whole_keyboard() {
        assert_eq!(press(true, "enter"), Press::Post);
        assert_eq!(press(true, "escape"), Press::Revert);
        for key in ["c", "backspace", "tab", "left", "space", "f1"] {
            assert_eq!(press(true, key), Press::Write, "{key} escaped the composer");
        }
    }

    /// Alt+S is a toggle, and the way out has to be the same door as the way in.
    /// If this ever becomes "open, always", the only exits left are Enter and Esc
    /// — and Esc is deliberately not one of them.
    #[test]
    fn alt_s_lets_you_back_out_the_way_you_came_in() {
        assert_eq!(alt_s(false), Act::Open, "no note yet: pick up the pen");
        assert_eq!(alt_s(true), Act::Post, "already writing: put it down");
        assert_ne!(
            alt_s(true),
            Act::Peel,
            "the chord must never destroy a note"
        );
    }

    /// Clicking away from a note being written posts it, the same as Enter — but
    /// it must NOT be swallowed on the way. An editor that keeps the keyboard
    /// after you have visibly clicked somewhere else eats what you type next.
    #[test]
    fn clicking_away_posts_the_note_without_eating_the_click() {
        assert_eq!(click(true, None), Act::Post, "clicking off commits");
        assert_eq!(
            click(false, None),
            Act::Pass,
            "with nothing being written, a miss is just a click"
        );
        assert_eq!(click(true, Some(Hit::Body)), Act::Open, "still writing");
        assert_eq!(click(false, Some(Hit::Body)), Act::Open, "pick up the pen");
        // The peel corner peels whether or not the note is being written.
        assert_eq!(click(true, Some(Hit::Peel)), Act::Peel);
        assert_eq!(click(false, Some(Hit::Peel)), Act::Peel);
    }

    /// The pin goes in and comes out by the SAME gesture, and by no other.
    ///
    /// The peel corner is the trap: it is a live left-click target, so if right
    /// clicking it peeled — or did nothing while the rest of the sheet pinned —
    /// the one corner of the note would answer the same button differently from
    /// the rest of it, and finding that out costs a note.
    #[test]
    fn right_clicking_anywhere_on_the_paper_pins_it() {
        assert_eq!(right_click(Some(Hit::Body)), Act::Pin);
        assert_eq!(
            right_click(Some(Hit::Peel)),
            Act::Pin,
            "the peel corner must pin like the rest of the sheet, never destroy"
        );
        assert_eq!(
            right_click(None),
            Act::Pass,
            "a right click off the paper still belongs to the pane's own menu"
        );
    }

    /// Nothing but that gesture may reach the pin. A reminder you can knock off
    /// by putting the pen down is one you stop believing, so the other two mouse
    /// verbs are checked for silence here rather than left to be noticed later.
    /// Keystrokes need no check: [`Press`] has no pin variant to return.
    #[test]
    fn no_other_gesture_touches_the_pin() {
        for composing in [false, true] {
            assert_ne!(alt_s(composing), Act::Pin);
            for hit in [None, Some(Hit::Body), Some(Hit::Peel)] {
                assert_ne!(click(composing, hit), Act::Pin);
            }
        }
    }

    /// A fresh note arrives unpinned: the pin is something you decide to add,
    /// never a state a note can be born in.
    #[test]
    fn a_new_note_carries_no_pin() {
        assert!(!Sticky::composing("", 7).pinned);
    }

    /// Why the note's chords are handled BEFORE the composer rather than inside
    /// it.
    ///
    /// The composer's buffer drops alt-modified keys, so an `alt+s` that reaches
    /// it does nothing AT ALL — and does it silently. That is what shipped for a
    /// build: pressing `alt+s` again while writing was a complete no-op, because
    /// the composer claimed the key first and then discarded it, and neither half
    /// of that is visible. The ordering in `TerminalView::on_key` is the fix;
    /// this pins the fact that makes the ordering necessary.
    #[test]
    fn the_composer_silently_drops_alt_chords() {
        let mut buf = crate::EditBuffer::seeded("note");
        let alt = gpui::Modifiers {
            alt: true,
            ..Default::default()
        };
        buf.apply("s", &alt, Some("s"), MAX_CHARS);
        assert_eq!(
            buf.text(),
            "note",
            "an alt chord typed into the note instead of being dropped — the \
             composer can no longer be relied on to leave chords alone"
        );
    }

    /// Every way out of the composer agrees on what "done" means. Three gestures
    /// that each decided for themselves is how one of them ends up saving and
    /// another discarding.
    #[test]
    fn every_exit_means_the_same_thing() {
        let enter = press(true, "enter");
        assert_eq!(enter, Press::Post);
        assert_eq!(alt_s(true), Act::Post);
        assert_eq!(click(true, None), Act::Post);
    }

    /// Clicks are resolved against the ROTATED paper. A point just outside a
    /// tilted note's corner is inside its axis-aligned box, and treating it as a
    /// hit would steal a click meant for the terminal.
    #[test]
    fn hit_testing_inverts_the_tilt() {
        let l = lay_at(6.0);
        assert_eq!(
            Hit::at(l.center, &l),
            Some(Hit::Body),
            "the centre is always on the paper"
        );

        let flat_corner = point(
            l.center.x + l.size.width / 2.0 - px(2.0),
            l.center.y - l.size.height / 2.0 + px(2.0),
        );
        assert_eq!(
            Hit::at(flat_corner, &l),
            None,
            "a tilted note must not claim its flat bounding box"
        );

        assert_eq!(Hit::at(point(px(0.), px(0.)), &l), None);
    }

    /// The peel corner is bottom-LEFT and small: it must not swallow clicks
    /// meant to pick up the pen, or every attempt to edit tears the note off.
    #[test]
    fn the_peel_corner_is_a_corner_not_the_note() {
        let l = lay_at(0.0);
        let bottom_left = point(
            l.center.x - l.size.width / 2.0 + px(4.0),
            l.center.y + l.size.height / 2.0 - px(4.0),
        );
        assert_eq!(Hit::at(bottom_left, &l), Some(Hit::Peel));

        let middle_left = point(l.center.x - l.size.width / 2.0 + px(4.0), l.center.y);
        assert_eq!(
            Hit::at(middle_left, &l),
            Some(Hit::Body),
            "the left EDGE is not the peel corner"
        );

        let bottom_right = point(
            l.center.x + l.size.width / 2.0 - px(4.0),
            l.center.y + l.size.height / 2.0 - px(4.0),
        );
        assert_eq!(
            Hit::at(bottom_right, &l),
            Some(Hit::Body),
            "only ONE corner peels"
        );
    }

    /// A flat pane must pay nothing and move nothing. If this drifts, every note
    /// on every unwarped pane silently shifts.
    #[test]
    fn a_flat_pane_gets_the_identity() {
        let w = Warp::new(content(), 0.0, 0.0);
        assert_eq!(w, Warp::FLAT);
        assert_eq!(w.apply(613.0, 109.0), (613.0, 109.0));
        assert_eq!(w.affine_about((613.0, 109.0)), Affine::IDENTITY);
        let l = lay_at(4.0);
        assert_eq!(l.warp, Warp::FLAT, "layout arms no warp on its own");
    }

    /// The PAPER is drawn through the exact map, so it comes out perfectly flat
    /// on a bent tube — no residual at all, at any point of the sheet.
    ///
    /// This is why the outline is a flattened polyline rather than four lines and
    /// a Bézier: each vertex goes through the barrel map on its own, and a
    /// straight edge is drawn as the curve that reads back straight.
    #[test]
    fn the_paper_is_drawn_exactly_flat() {
        let c = content();
        let (k1, k2) = (0.14, 0.06);
        // the pane's top-right, where the distortion is largest and where the
        // note actually sits
        let center = (640.0f32, 110.0f32);
        let warp = Warp::new(c, k1, k2);
        assert_ne!(warp, Warp::FLAT, "a bent pane must warp the note");

        let mut worst = 0.0f32;
        let mut worst_raw = 0.0f32;
        for dx in [-93.0, -46.0, 0.0, 46.0, 93.0] {
            for dy in [-85.0, -42.0, 0.0, 42.0, 85.0] {
                let want = (center.0 + dx, center.1 + dy);
                // Where the pass reads the pixel it writes at `want` — i.e. where
                // this bit of the note has to be DRAWN to appear at `want`.
                let read = shader_sample(c, want, k1, k2);
                let drawn = warp.apply(want.0, want.1);
                worst = worst.max(((drawn.0 - read.0).powi(2) + (drawn.1 - read.1).powi(2)).sqrt());
                // ...against drawing it naively, with no compensation at all.
                worst_raw =
                    worst_raw.max(((want.0 - read.0).powi(2) + (want.1 - read.1).powi(2)).sqrt());
            }
        }
        assert!(
            worst_raw > 10.0,
            "this pane position should be badly distorted, got {worst_raw:.2}px — \
             the test is no longer measuring anything"
        );
        assert!(worst < 0.01, "the paper kept {worst:.3}px of bend");
    }

    /// The WRITING can only carry an affine, so it keeps a residual — and the
    /// point of pinning that affine at the text block rather than at the sheet's
    /// centre is that the residual stays under a pixel where it is read.
    ///
    /// Measured over the writing's own extent: a note is inset by its padding and
    /// rarely more than three lines tall, which is a far smaller patch than the
    /// sheet. Fitting the same affine over the whole sheet measured 7px.
    #[test]
    fn the_writing_stays_on_the_paper_under_the_bend() {
        let c = content();
        let (k1, k2) = (0.14, 0.06);
        let center = (640.0f32, 110.0f32);
        let warp = Warp::new(c, k1, k2);

        // the text block: inset from a 186x171 sheet, three lines of ~25px
        let block = (center.0, center.1 - 40.0);
        let affine = warp.affine_about(block);
        let mut worst = 0.0f32;
        for dx in [-73.0, -36.0, 0.0, 36.0, 73.0] {
            for dy in [-38.0, -19.0, 0.0, 19.0, 38.0] {
                let want = (block.0 + dx, block.1 + dy);
                let read = shader_sample(c, want, k1, k2);
                let drawn = affine.apply(want.0, want.1);
                worst = worst.max(((drawn.0 - read.0).powi(2) + (drawn.1 - read.1).powi(2)).sqrt());
            }
        }
        // ~3px at the block's far corners on the most bent pane in the most
        // distorted position. Against an unmarked sheet with 20px of padding
        // there is nothing to measure it against, so it does not read; what DID
        // read was the alternative, where paper and writing shared one affine and
        // the PAPER carried 7px of bend, on a shape whose straight edges make the
        // bend obvious.
        assert!(
            worst < 3.5,
            "the writing drifted {worst:.2}px off the paper it is written on"
        );

        // Fitting it over the whole SHEET instead is what this guards against.
        let sheet_fit = warp.affine_about(center);
        let corner = (center.0 + 93.0, center.1 - 85.0);
        let read = shader_sample(c, corner, k1, k2);
        let drawn = sheet_fit.apply(corner.0, corner.1);
        let sheet_err = ((drawn.0 - read.0).powi(2) + (drawn.1 - read.1).powi(2)).sqrt();
        assert!(
            sheet_err > worst * 2.0,
            "a sheet-wide fit ({sheet_err:.2}px) should be clearly worse than a \
             text-block fit ({worst:.2}px), or pinning it at the text is pointless"
        );
    }

    /// A flat pane must hand the glyphs a plain rotation and nothing else — no
    /// stray scale, no drift. Every note on every unwarped pane depends on it.
    #[test]
    fn a_flat_pane_leaves_the_writing_alone() {
        let l = lay_at(0.0);
        let m = l.text_matrix(1.0, point(px(0.0), px(-40.0)));
        assert_eq!(m.rotation_scale, [[1.0, 0.0], [0.0, 1.0]]);
        assert!(
            m.translation[0].abs() < 1e-4 && m.translation[1].abs() < 1e-4,
            "an untilted note on a flat pane moved its text by {:?}",
            m.translation
        );
    }

    /// The glyph transform is handed to the renderer in PHYSICAL pixels. Getting
    /// the scale factor wrong puts the writing somewhere else entirely on a
    /// fractional-scaling desktop, which is every desktop here.
    #[test]
    fn the_glyph_transform_scales_with_the_display() {
        let mut l = lay_at(5.0);
        l.center = point(px(640.0), px(110.0));
        l.pre_warp(content(), 0.14, 0.06);
        let block = point(px(0.0), px(-40.0));
        let one = l.text_matrix(1.0, block);
        let two = l.text_matrix(2.0, block);
        assert_eq!(
            one.rotation_scale, two.rotation_scale,
            "the 2x2 is dimensionless and must not scale"
        );
        for i in 0..2 {
            assert!(
                (two.translation[i] - one.translation[i] * 2.0).abs() < 0.01,
                "translation component {i} did not scale with the display"
            );
        }
    }

    /// The paper takes the theme's own voice and is SATURATED — a pastel note
    /// dissolves into a dark terminal — and it is written on in black. A theme
    /// with no colour in its text must not yield a grey note: grey reads as a
    /// dialog box, not as paper.
    #[test]
    fn the_paper_is_saturated_theme_colour_written_on_in_black() {
        let white = hsla(0.0, 0.0, 0.92, 1.0);
        let amber = hsla(0.11, 0.62, 0.60, 1.0);
        let cyan = hsla(0.52, 0.70, 0.55, 1.0);

        let p = paper(amber, white);
        assert!((p.base.h - 0.11).abs() < 1e-4, "hue comes from the text");
        assert!(p.base.s > 0.8, "the sheet is saturated, got {}", p.base.s);
        assert!(p.ink.l < 0.15, "the writing is black, got l={}", p.ink.l);
        assert!(
            (p.ink.h - p.base.h).abs() < 1e-4,
            "the ink still belongs to the paper"
        );

        // white text, coloured accent -> the accent's hue, never grey
        let p = paper(white, cyan);
        assert!(
            (p.base.h - 0.52).abs() < 1e-4,
            "falls through to the accent"
        );
        assert!(p.base.s > 0.8);

        // nothing coloured anywhere -> a sticky yellow, still not grey
        let p = paper(white, hsla(0.0, 0.0, 0.5, 1.0));
        assert!(p.base.s > 0.8, "a colourless theme still gets paper");
        assert!((p.base.h - 0.128).abs() < 1e-4);
    }

    /// You must be able to see the terminal through it, faintly. This is the one
    /// property that makes it read as laid ON the screen rather than cut into it.
    #[test]
    fn the_paper_is_slightly_see_through() {
        let p = paper(hsla(0.11, 0.62, 0.60, 1.0), hsla(0.0, 0.0, 0.9, 1.0));
        assert!(
            (0.78..0.92).contains(&p.base.a),
            "paper alpha {} is outside 'faintly see-through'",
            p.base.a
        );
        assert!(p.ink.a > 0.9, "the writing itself stays legible");
    }
}
