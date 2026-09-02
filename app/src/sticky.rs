//! A sticky note pinned to a pane's glass.
//!
//! `alt+s` sticks one to the top-right of the focused terminal and hands it the
//! cursor; `alt+backspace` peels it off. It survives a restart beside the pane's
//! cwd, so coming back to a wall of twenty agents you read the notes instead of
//! scrolling each one for your last prompt.
//!
//! Three things here are deliberate and easy to undo by accident:
//!
//! * **Esc never removes a posted note.** It reverts the composer, because a
//!   composer is a mode you can see you are in. Once posted the note is an
//!   object sitting on someone's terminal, and swallowing an Esc that the pane
//!   below would have read as an interrupt is the bug this feature must not
//!   ship (see [`crate::pane::TerminalView::sticky_key`]).
//! * **The note is painted FLAT.** A pane's barrel warp is a pixel post-pass
//!   over the tube rect ([`crate::warp`]), so a note drawn over it would bow
//!   with the glass. The pane registers the note's box as its own flat tube
//!   FIRST, and the shader's first-rect-wins loop passes those pixels through
//!   untouched. The paper's curve is drawn here, in the paper.
//! * **Nothing under the tilt is hit-tested by gpui.** Glyph sprites and filled
//!   paths carry the rotation; layout, content masks and hit-testing stay in the
//!   flat box. Clicks resolve through [`Hit::at`], which inverts the rotation —
//!   the same contract the alt-click copy affordance works under.

use gpui::{
    font, hsla, linear_color_stop, linear_gradient, point, px, radians, size, App, Bounds, Font,
    FontWeight, Hsla, Path, PathBuilder, Pixels, Point, Size, TextAlign, TextRun,
    TransformationMatrix, Window,
};

use crate::theme::Theme;

/// The hand the note is written in. Caveat (SIL OFL) is bundled and registered
/// at startup beside the crawl font, so a box with no handwriting face installed
/// still gets handwriting — and every box gets the SAME handwriting.
pub const FONT_FAMILY: &str = "Caveat";

/// Ink is a pen, not a printout: ask the variable face for a heavier weight than
/// body text so thin strokes survive at note size on a warped tube.
fn note_font() -> Font {
    let mut f = font(FONT_FAMILY);
    f.weight = FontWeight::SEMIBOLD;
    f
}

/// Longest note we accept. Past this it stops being a note and starts being a
/// document that wants a file.
pub const MAX_CHARS: usize = 160;

/// Rows of handwriting the paper holds before the text shrinks a notch.
const MAX_ROWS: usize = 4;

/// The note stuck to one pane.
#[derive(Clone, Debug)]
pub struct Sticky {
    /// What is written on it. While composing this is the live buffer's text.
    pub text: String,
    /// Fixes the tilt. Drawn once and persisted, so the angle survives a
    /// re-render, a theme change and a restart — a note that re-rolled its lean
    /// every frame would read as a twitch rather than a piece of paper.
    pub seed: u32,
    /// Present while the note holds the cursor.
    pub edit: Option<Edit>,
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
            text: prefill.to_string(),
            seed,
            edit: Some(Edit {
                buf: crate::EditBuffer::seeded(prefill),
                restore: None,
            }),
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

/// A note's box on the glass. `None` from [`layout`] when the pane is too small
/// to carry one without becoming a note with a terminal behind it.
pub struct Layout {
    /// Centre in window coordinates — the pivot every rotation here turns about.
    pub center: Point<Pixels>,
    pub size: Size<Pixels>,
    /// Degrees clockwise.
    pub tilt: f32,
    pub font_size: Pixels,
    pub line_height: Pixels,
    /// Inset from the paper edge to the writing.
    pub pad: Pixels,
}

/// Below this the note would cover the terminal rather than annotate it. Parker
/// runs panes at 390px wide in a tiled layout, so this is a live case, not a
/// theoretical one.
const MIN_PANE: (f32, f32) = (250.0, 190.0);

impl Layout {
    /// Where the writing starts, in note-local (unrotated, centre-origin) space.
    fn text_origin(&self) -> Point<Pixels> {
        point(
            -self.size.width / 2.0 + self.pad,
            -self.size.height / 2.0 + self.pad * 0.9,
        )
    }

    fn text_width(&self) -> Pixels {
        self.size.width - self.pad * 2.0
    }

    /// The rotation this note's glyph sprites carry. Physical pixels, since that
    /// is the space sprite quads live in.
    fn text_matrix(&self, scale_factor: f32) -> TransformationMatrix {
        let pivot = self.center.scale(scale_factor);
        TransformationMatrix::unit()
            .translate(pivot)
            .rotate(radians(self.tilt.to_radians()))
            .translate(point(
                gpui::ScaledPixels(-pivot.x.0),
                gpui::ScaledPixels(-pivot.y.0),
            ))
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
    let note_w = (w * 0.26).clamp(128.0, 186.0);
    let note_h = note_w * 0.92;
    let font_size = (note_w * 0.118).clamp(15.0, 22.0);
    Some(Layout {
        center: point(
            content.origin.x + px(w - note_w * 0.5 - 34.0),
            content.origin.y + px(note_h * 0.5 + 30.0),
        ),
        size: size(px(note_w), px(note_h)),
        tilt,
        font_size: px(font_size),
        line_height: px(font_size * 1.26),
        pad: px(note_w * 0.11),
    })
}

pub struct Paper {
    pub base: Hsla,
    pub ink: Hsla,
}

/// The note's palette, derived from the theme rather than from a hardcoded
/// Post-it yellow. `source` is the theme's own bright yellow (`ansi[11]`).
///
/// Pulled to paper: high lightness, chroma dialled back, ink a dark tint of the
/// SAME hue so the note reads as one object. On a green-phosphor or amber theme
/// it comes out a pale green or amber sheet — which is what "it belongs on this
/// screen" means. A theme with no colour at all falls back to cream, because a
/// grey note reads as a dialog box.
pub fn paper(source: Hsla) -> Paper {
    let (hue, sat) = if source.s < 0.12 {
        (0.128, 0.42)
    } else {
        (source.h, (source.s * 0.62).clamp(0.30, 0.58))
    };
    Paper {
        base: hsla(hue, sat, 0.795, 1.0),
        ink: hsla(hue, (sat * 1.15).min(0.85), 0.185, 1.0),
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
/// bowing the sides was what made the first pass read as a pillow. What sells it
/// instead is that the rectangle is slightly wrong: each corner takes an
/// independent ±1.6px draw off the note's seed, so no two notes are the same
/// quadrilateral and none of them is a drawn rectangle.
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

    /// The paper's outline. A builder rather than a path because a built path is
    /// consumed and the shadow needs this shape several times over.
    fn outline(&self) -> PathBuilder {
        let mut b = PathBuilder::fill();
        let c = self.bottom_ctrl();
        b.move_to(point(px(self.tl.0), px(self.tl.1)));
        b.line_to(point(px(self.tr.0), px(self.tr.1)));
        b.line_to(point(px(self.br.0), px(self.br.1)));
        b.curve_to(point(px(self.bl.0), px(self.bl.1)), point(px(c.0), px(c.1)));
        b.close();
        b
    }

    /// The lifted lip along the bottom edge.
    ///
    /// ASYMMETRIC, and that is the whole trick: on a real note one corner comes
    /// away first, so the pale underside is a wedge — thick at the bottom-left,
    /// tapering to nothing at the right. A band of even thickness across the
    /// bottom reads as a printed border, and a wide one turns the note into a
    /// ribbon. It lifts at the same corner the pointer peels from.
    fn curl(&self) -> PathBuilder {
        let mut b = PathBuilder::fill();
        let c = self.bottom_ctrl();
        let (deep, shallow) = (self.lip, self.lip * 0.38);
        b.move_to(point(px(self.br.0), px(self.br.1)));
        b.curve_to(point(px(self.bl.0), px(self.bl.1)), point(px(c.0), px(c.1)));
        b.line_to(point(px(self.bl.0), px(self.bl.1 - deep)));
        b.curve_to(
            point(px(self.br.0), px(self.br.1 - shallow)),
            point(px(c.0), px(c.1 - deep * 0.55)),
        );
        b.close();
        b
    }

    /// The crease where the sheet leaves the glass — a hairline of shadow along
    /// the top of the lip, tapering with it. Without it the lip reads as a paler
    /// stripe painted on the paper rather than as an edge coming off it.
    fn crease(&self) -> PathBuilder {
        let mut b = PathBuilder::fill();
        let c = self.bottom_ctrl();
        let (deep, shallow) = (self.lip, self.lip * 0.38);
        let w = (self.lip * 0.30).max(0.9);
        b.move_to(point(px(self.bl.0), px(self.bl.1 - deep)));
        b.curve_to(
            point(px(self.br.0), px(self.br.1 - shallow)),
            point(px(c.0), px(c.1 - deep * 0.55)),
        );
        b.line_to(point(px(self.br.0), px(self.br.1 - shallow - w)));
        b.curve_to(
            point(px(self.bl.0), px(self.bl.1 - deep - w)),
            point(px(c.0), px(c.1 - deep * 0.55 - w)),
        );
        b.close();
        b
    }

    /// One layer of the drop shadow, `t` px deep.
    ///
    /// Not the outline scaled up — that was the first pass's mistake: a scaled
    /// copy pokes out ABOVE and LEFT of the paper too, and six of them stacked
    /// gave the note a dark halo that read as a blob. This pools the shadow where
    /// a sheet glued at the top actually casts one: barely at the glued edge,
    /// spreading under the lifted bottom.
    fn shadow(&self, t: f32) -> PathBuilder {
        let mut b = PathBuilder::fill();
        let (dx, top, bot) = (t * 0.30, t * 0.22, t * 1.15);
        let c = self.bottom_ctrl();
        b.move_to(point(px(self.tl.0 + dx), px(self.tl.1 + top)));
        b.line_to(point(px(self.tr.0 + dx), px(self.tr.1 + top)));
        b.line_to(point(px(self.br.0 + dx), px(self.br.1 + bot)));
        b.curve_to(
            point(px(self.bl.0 + dx), px(self.bl.1 + bot)),
            point(px(c.0 + dx), px(c.1 + bot)),
        );
        b.close();
        b
    }
}

/// An axis-aligned quad in note-local space, for the washes and the caret.
fn quad(x0: f32, y0: f32, x1: f32, y1: f32) -> PathBuilder {
    let mut b = PathBuilder::fill();
    b.move_to(point(px(x0), px(y0)));
    b.line_to(point(px(x1), px(y0)));
    b.line_to(point(px(x1), px(y1)));
    b.line_to(point(px(x0), px(y1)));
    b.close();
    b
}

/// Place a note-local path onto the glass: rotate about the origin, then move
/// the origin to the note's centre. `grow` fattens it (the shadow stack).
fn place(
    mut b: PathBuilder,
    lay: &Layout,
    grow: f32,
    nudge: Point<Pixels>,
) -> Option<Path<Pixels>> {
    if (grow - 1.0).abs() > f32::EPSILON {
        b.scale(grow);
    }
    b.rotate(lay.tilt);
    b.translate(point(lay.center.x + nudge.x, lay.center.y + nudge.y));
    b.build().ok()
}

/// No offset — for paths already positioned in note-local space.
fn here() -> Point<Pixels> {
    point(px(0.0), px(0.0))
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
#[allow(clippy::too_many_arguments)]
pub fn paint(
    note: &Sticky,
    lay: &Layout,
    th: &Theme,
    hover_peel: bool,
    window: &mut Window,
    cx: &mut App,
) {
    // The theme's own bright yellow is the paper's source — see `paper`.
    let pal = paper(th.ansi[11]);
    let lift = if hover_peel { 2.1 } else { 1.0 };

    // Shadow: gpui fills paths flat, so the softness is a stack of the same
    // outline stepped down-right at low alpha rather than a blur. It also has to
    // reach the corners of the flat cutout the pane punched in the warp — an
    // unwarped sliver of terminal beside the paper would read as a seam.
    let sheet = Sheet::new(lay.size, note.seed, lift);

    // Shadow: gpui fills paths flat, so the softness is a stack of layers rather
    // than a blur. Each layer pools toward the lifted bottom instead of being
    // the outline scaled up — a scaled copy pokes out above and left of the
    // paper, and six of those stacked gave the note a dark halo.
    // One ambient ring first — a single grown copy, not a stack of them. It also
    // does real work at the seam: the flat cutout's corners poke a few px past
    // the tilted paper, and terminal content crossing that line steps sideways
    // (flat inside, warped outside). A soft dark gradient over the join reads as
    // contact shadow instead of as an edge.
    if let Some(p) = place(sheet.outline(), lay, 1.055, point(px(0.6), px(1.4))) {
        window.paint_path(p, hsla(0.0, 0.0, 0.0, 0.11));
    }
    for i in (1..=9).rev() {
        let t = i as f32;
        if let Some(p) = place(sheet.shadow(t), lay, 1.0, here()) {
            window.paint_path(p, hsla(0.0, 0.0, 0.0, 0.035));
        }
    }

    // The sheet. Lit from above: brighter where it lies flat against the glass,
    // falling off toward the free edge. The gradient is turned by the tilt so it
    // runs down the PAPER rather than down the screen.
    if let Some(p) = place(sheet.outline(), lay, 1.0, here()) {
        window.paint_path(
            p,
            linear_gradient(
                180.0 + lay.tilt,
                linear_color_stop(shift(pal.base, 1.055), 0.0),
                linear_color_stop(shift(pal.base, 0.93), 1.0),
            ),
        );
    }

    // One wash down the right-hand side. The first pass had two, which pinched
    // the sheet in the middle and read as a pillow; with straight edges the
    // paper only needs the far side to fall away from the light.
    if let Some(p) = place(sheet.outline(), lay, 1.0, here()) {
        window.paint_path(
            p,
            linear_gradient(
                270.0 + lay.tilt,
                linear_color_stop(hsla(0.0, 0.0, 0.0, 0.062), 0.0),
                linear_color_stop(hsla(0.0, 0.0, 0.0, 0.0), 0.45),
            ),
        );
    }

    // The adhesive band across the top reads as a slightly different sheen.
    let (hw, hh) = (
        f32::from(lay.size.width) / 2.0,
        f32::from(lay.size.height) / 2.0,
    );
    if let Some(p) = place(quad(-hw, -hh, hw, -hh + hh * 0.26), lay, 1.0, here()) {
        window.paint_path(
            p,
            linear_gradient(
                180.0 + lay.tilt,
                linear_color_stop(hsla(0.0, 0.0, 1.0, 0.09), 0.0),
                linear_color_stop(hsla(0.0, 0.0, 1.0, 0.0), 1.0),
            ),
        );
    }

    // The crease, then the lip it belongs to: the sheet's pale underside, lit
    // because the curl tips it up toward the light.
    if let Some(p) = place(sheet.crease(), lay, 1.0, here()) {
        window.paint_path(p, hsla(0.0, 0.0, 0.0, 0.10));
    }
    if let Some(p) = place(sheet.curl(), lay, 1.0, here()) {
        window.paint_path(
            p,
            linear_gradient(
                180.0 + lay.tilt,
                linear_color_stop(shift(pal.base, 0.98), 0.0),
                linear_color_stop(shift(pal.base, 1.07), 1.0),
            ),
        );
    }

    paint_writing(note, lay, &pal, window, cx);
}

/// The handwriting, and the composer's selection and caret.
fn paint_writing(note: &Sticky, lay: &Layout, pal: &Paper, window: &mut Window, cx: &mut App) {
    let text = match &note.edit {
        Some(e) => e.buf.text(),
        None => note.text.clone(),
    };
    if text.is_empty() && note.edit.is_none() {
        return;
    }

    // Shrink a notch before clipping: a note that overflows should get smaller
    // handwriting, not a truncated thought. Past the last notch it clamps, which
    // is what MAX_CHARS is there to make rare.
    let wrap = lay.text_width();
    let mut chosen = None;
    for scale in [1.0_f32, 0.86, 0.74] {
        let font_size = lay.font_size * scale;
        let line_height = lay.line_height * scale;
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
            Some(MAX_ROWS),
        ) else {
            return;
        };
        let Some(line) = lines.into_iter().next() else {
            return;
        };
        let rows = line.wrap_boundaries().len() + 1;
        let fits = rows <= MAX_ROWS
            && f32::from(line.size(line_height).height)
                <= f32::from(lay.size.height - lay.pad * 1.5);
        if fits || scale < 0.75 {
            chosen = Some((line, line_height));
            break;
        }
    }
    let Some((line, line_height)) = chosen else {
        return;
    };

    let origin_local = lay.text_origin();
    let matrix = lay.text_matrix(window.scale_factor());

    // Selection first, so the ink sits on top of it. `seeded` selects the whole
    // note when the composer opens, and a selection you cannot see is a
    // selection your next keystroke silently eats.
    if let Some(edit) = &note.edit {
        let (a, b) = edit.buf.sel_range();
        if a != b {
            let byte = |chars: usize| {
                edit.buf
                    .text()
                    .char_indices()
                    .nth(chars)
                    .map(|(i, _)| i)
                    .unwrap_or(text.len())
            };
            if let (Some(p0), Some(p1)) = (
                line.position_for_index(byte(a), line_height),
                line.position_for_index(byte(b), line_height),
            ) {
                paint_selection(p0, p1, line_height, wrap, origin_local, lay, pal, window);
            }
        }
    }

    window.with_text_transformation(matrix, |window| {
        let origin = point(lay.center.x + origin_local.x, lay.center.y + origin_local.y);
        if let Err(e) = line.paint(origin, line_height, TextAlign::Left, None, window, cx) {
            eprintln!("terminal-delight: sticky note text failed to paint: {e}");
        }
    });

    // The caret is a pen resting on the paper: a short stroke on the baseline,
    // drawn as a path so it turns with the note like everything else.
    if let Some(edit) = &note.edit {
        let caret_byte = edit
            .buf
            .text()
            .char_indices()
            .nth(edit.buf.caret())
            .map(|(i, _)| i)
            .unwrap_or(text.len());
        if let Some(p) = line.position_for_index(caret_byte, line_height) {
            let x = f32::from(origin_local.x + p.x);
            let y = f32::from(origin_local.y + p.y);
            let lh = f32::from(line_height);
            if let Some(path) = place(quad(x, y + 2.0, x + 1.9, y + lh - 3.0), lay, 1.0, here()) {
                window.paint_path(path, pal.ink);
            }
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
        let band = quad(ox + x0, oy + y + 1.0, ox + x1, oy + y + lh - 1.0);
        if let Some(path) = place(band, lay, 1.0, here()) {
            window.paint_path(path, Hsla { a: 0.22, ..pal.ink });
        }
    }
}

/// The rect the pane registers as a FLAT warp tube so the note doesn't bow with
/// the glass — the note's box grown to cover its rotated corners and its shadow.
/// Physical pixels, the space [`crate::warp`] speaks.
pub fn cutout(lay: &Layout, scale_factor: f32) -> [f32; 4] {
    let (w, h) = (f32::from(lay.size.width), f32::from(lay.size.height));
    let (sin, cos) = lay.tilt.to_radians().sin_cos();
    // Half-extents of the ROTATED box and nothing more.
    //
    // The margin used to be 8x12px "for the shadow", which was backwards. A
    // warped tube is painted ~16% hotter than a flat one, so the cutout is a
    // visible brightness step wherever terminal shows through it — every pixel
    // of margin is seam. The shadow is soft and low-contrast, so letting its
    // outer edge bow with the glass costs nothing, while the paper and the
    // writing (which must not bend) sit well inside.
    let ex = (w * cos.abs() + h * sin.abs()) / 2.0 + 1.0;
    let ey = (w * sin.abs() + h * cos.abs()) / 2.0 + 1.0;
    [
        (f32::from(lay.center.x) - ex) * scale_factor,
        (f32::from(lay.center.y) - ey) * scale_factor,
        ex * 2.0 * scale_factor,
        ey * 2.0 * scale_factor,
    ]
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

    fn lay_at(tilt: f32) -> Layout {
        Layout {
            center: point(px(500.0), px(120.0)),
            size: size(px(180.0), px(166.0)),
            tilt,
            font_size: px(20.0),
            line_height: px(25.0),
            pad: px(20.0),
        }
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
        // and it sits inside the pane, not off its right edge
        assert!(f32::from(l.center.x) + f32::from(l.size.width) / 2.0 <= 390.0);
    }

    /// A tilt of zero reads as a mis-drawn rectangle, and a tilt that always
    /// leans the same way reads as a skewed widget. Both signs must occur.
    #[test]
    fn the_lean_is_never_square_and_goes_both_ways() {
        let mut left = false;
        let mut right = false;
        for seed in 0..64u32 {
            let note = Sticky {
                text: String::new(),
                seed,
                edit: None,
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
            text: "x".into(),
            seed: 12345,
            edit: None,
        };
        let b = Sticky {
            text: "different text".into(),
            seed: 12345,
            edit: None,
        };
        assert_eq!(a.tilt(), b.tilt(), "the tilt comes from the seed alone");
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

        // top-right corner of the FLAT box; at +6° the paper has swung away
        let flat_corner = point(
            l.center.x + l.size.width / 2.0 - px(2.0),
            l.center.y - l.size.height / 2.0 + px(2.0),
        );
        assert_eq!(
            Hit::at(flat_corner, &l),
            None,
            "a tilted note must not claim its flat bounding box"
        );

        // far outside is never a hit, whatever the tilt
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

    /// The cutout is what keeps the note flat on a bent tube. It has to cover
    /// the rotated corners — a cutout sized to the upright note leaves the
    /// paper's own corners inside the warp, which is exactly the bow we're
    /// removing.
    #[test]
    fn the_flat_cutout_covers_the_rotated_note() {
        let l = lay_at(6.0);
        let [x, y, w, h] = cutout(&l, 1.0);
        let (hw, hh) = (
            f32::from(l.size.width) / 2.0,
            f32::from(l.size.height) / 2.0,
        );
        let (sin, cos) = l.tilt.to_radians().sin_cos();
        for (cx_, cy_) in [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)] {
            let px_ = f32::from(l.center.x) + cx_ * cos - cy_ * sin;
            let py_ = f32::from(l.center.y) + cx_ * sin + cy_ * cos;
            assert!(
                px_ >= x && px_ <= x + w && py_ >= y && py_ <= y + h,
                "rotated corner ({px_},{py_}) escaped the cutout {:?}",
                [x, y, w, h]
            );
        }
    }

    /// The cutout is handed to the renderer in PHYSICAL pixels; getting the
    /// scale factor wrong puts the flat patch somewhere else entirely on a
    /// fractional-scaling desktop.
    #[test]
    fn the_cutout_is_in_physical_pixels() {
        let l = lay_at(3.0);
        let one = cutout(&l, 1.0);
        let two = cutout(&l, 2.0);
        for i in 0..4 {
            assert!(
                (two[i] - one[i] * 2.0).abs() < 1e-3,
                "component {i} did not scale: {} vs {}",
                two[i],
                one[i]
            );
        }
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

    /// A theme with no colour must not produce a grey note — grey reads as a
    /// dialog. A theme WITH colour must lend the note its hue, so the paper
    /// belongs to the screen it is stuck to.
    #[test]
    fn the_paper_borrows_the_themes_hue_and_never_goes_grey() {
        let p = paper(hsla(0.0, 0.0, 0.9, 1.0)); // a mono theme
        assert!(p.base.s > 0.2, "a mono theme still gets cream, not grey");

        let p = paper(hsla(0.33, 0.95, 0.6, 1.0)); // a green phosphor theme
        assert!(
            (p.base.h - 0.33).abs() < 1e-4,
            "the paper must take the theme's hue"
        );
        assert!(
            p.base.s < 0.95,
            "and dial the chroma back to paper, got {}",
            p.base.s
        );
        assert!(p.base.l > 0.6, "paper is light");
        assert!(p.ink.l < 0.3, "ink is dark");
        assert!(
            (p.ink.h - p.base.h).abs() < 1e-4,
            "ink and paper share a hue, so the note reads as one object"
        );
    }
}
