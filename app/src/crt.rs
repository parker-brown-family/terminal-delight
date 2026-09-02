//! The CRT "glass" — one workspace-wide overlay, ported value-for-value from
//! the IMT hacker theme CSS (static/css/hacker-theme.css, the TPS-report CRT
//! layer): 4px-period scanlines, curved-glass inset shadows, center phosphor
//! bloom, a 160px tracking band sweeping down, stepped flicker, and a rare
//! 1–2px vertical jiggle. Every effect scales with a theme dial; all of it is
//! GPU quads/shadows — nothing touches the input path.

use std::time::Instant;

use gpui::{
    canvas, div, fill, hsla, linear_color_stop, linear_gradient, point, prelude::*, px, size,
    Bounds, BoxShadow, Hsla,
};

use crate::theme::Theme;

/// Animated state, advanced by the Workspace ticker.
pub struct Fx {
    started: Instant,
    rng: u64,
    /// 0..1 progress of the current tracking sweep, if one is running.
    pub band: Option<f32>,
    next_band_at: f32,
    /// current flicker opacity multiplier; 1.0 except during occasional bursts
    pub flicker_mul: f32,
    flicker_burst_until: f32,
    next_flicker_at: f32,
    /// vertical hop in px, ±, usually 0
    pub jiggle_px: f32,
    jiggle_until: f32,
    next_jiggle_at: f32,
}

const BAND_H: f32 = 160.0;

impl Fx {
    /// Seed gives every screen its own desynced rhythm.
    pub fn new(seed: u64) -> Self {
        let mut fx = Self {
            started: Instant::now(),
            rng: 0x5DEECE66D ^ seed,
            band: None,
            next_band_at: 0.,
            flicker_mul: 1.0,
            flicker_burst_until: 0.,
            next_flicker_at: 0.,
            jiggle_px: 0.,
            jiggle_until: 0.,
            next_jiggle_at: 0.,
        };
        fx.next_band_at = 1.0 + fx.rand() * 7.0;
        fx.next_flicker_at = 2.0 + fx.rand() * 12.0;
        fx.next_jiggle_at = 4.0 + fx.rand() * 8.0;
        fx
    }

    fn rand(&mut self) -> f32 {
        self.rng = self
            .rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.rng >> 33) as f32) / (u32::MAX as f32 / 2.0)
    }

    /// Advance; returns true if something visible changed (=> notify).
    pub fn tick(&mut self, th: &Theme) -> bool {
        let t = self.started.elapsed().as_secs_f32();
        let mut changed = false;

        // tracking band: slow sweep (theme-dialed), then rest for the period
        let sweep = th.tracking_sweep;
        if th.tracking > 0.001 {
            match self.band {
                Some(_) => {
                    let progress = (t - (self.next_band_at - sweep)) / sweep;
                    if progress >= 1.0 {
                        self.band = None;
                        self.next_band_at =
                            t + (th.tracking_period - sweep).max(1.0) + self.rand() * 2.0;
                    } else {
                        self.band = Some(progress);
                    }
                    changed = true;
                }
                None if t >= self.next_band_at - sweep => {
                    self.band = Some(0.);
                    changed = true;
                }
                None => {}
            }
        }

        // flicker: OCCASIONAL — a ~0.45s burst of stepped dips every ~9-25s
        if th.flicker > 0.001 {
            if t >= self.next_flicker_at && self.flicker_burst_until < t {
                self.flicker_burst_until = t + 0.45;
                self.next_flicker_at = t + 9.0 + self.rand() * 8.0;
            }
            let target = if t < self.flicker_burst_until {
                // stepped dip pattern within the burst
                let ph = ((self.flicker_burst_until - t) / 0.45 * 5.0) as i32;
                let step = match ph {
                    4 => 0.86,
                    3 => 1.06,
                    2 => 0.90,
                    1 => 1.03,
                    _ => 0.95,
                };
                1.0 + (step - 1.0) * th.flicker
            } else {
                1.0
            };
            if (target - self.flicker_mul).abs() > 0.001 {
                self.flicker_mul = target;
                changed = true;
            }
        }

        // jiggle: a 2-frame ±1–2px vertical hop every ~6–12s
        if th.jiggle > 0.001 {
            if self.jiggle_px != 0. && t >= self.jiggle_until {
                self.jiggle_px = 0.;
                changed = true;
            } else if self.jiggle_px == 0. && t >= self.next_jiggle_at {
                let dir = if self.rand() > 1.0 { 1. } else { -1. };
                self.jiggle_px = dir * (1.0 + self.rand()).min(2.0) * th.jiggle;
                self.jiggle_until = t + 0.09;
                self.next_jiggle_at = t + 6.0 + self.rand() * 3.0;
                changed = true;
            }
        }

        changed
    }

    /// True while an animation needs frame-rate ticks (else the ticker can idle).
    pub fn active(&self) -> bool {
        self.band.is_some() || self.jiggle_px != 0. || self.flicker_mul != 1.0
    }
}

/// A raised metallic "bezel" framing the pane edge: a bright top/left rail and a
/// dark bottom/right recess (the classic emboss), plus a soft outer drop so the
/// frame reads as standing proud of the surrounding surface. Scales with the
/// theme's `bezel` dial; non-occluding like the glass overlay — it carries no
/// mouse handlers, so input passes straight through to the pane below.
pub fn bezel(th: &Theme) -> impl IntoElement {
    let b = th.bezel;
    let accent = th.accent;
    div()
        .absolute()
        .inset_0()
        .rounded_lg()
        .border_1()
        // outer dark seam where the frame meets the surface
        .border_color(hsla(0., 0., 0., 0.55 * b))
        .shadow(vec![
            // bright top rail — the molding catching the room light (accent-tinted)
            BoxShadow {
                color: accent.alpha(0.40 * b),
                offset: point(px(0.), px(1.)),
                blur_radius: px(0.),
                spread_radius: px(0.),
                inset: true,
            },
            // top-left white highlight, the high edge of the bevel
            BoxShadow {
                color: gpui::white().alpha(0.16 * b),
                offset: point(px(1.), px(1.)),
                blur_radius: px(0.),
                spread_radius: px(0.),
                inset: true,
            },
            // bottom-right dark recess, the low edge of the bevel
            BoxShadow {
                color: hsla(0., 0., 0., 0.60 * b),
                offset: point(px(-1.), px(-2.)),
                blur_radius: px(3.),
                spread_radius: px(0.),
                inset: true,
            },
            // soft outer lift so the bezel stands proud of the surface
            BoxShadow {
                color: hsla(0., 0., 0., 0.45 * b),
                offset: point(px(0.), px(3.)),
                blur_radius: px(10.),
                spread_radius: px(-3.),
                inset: false,
            },
        ])
}

/// The full glass overlay: scanlines + tracking band canvas, vignette shadows,
/// center bloom. Non-occluding — mouse/keys pass through to the panes below.
pub fn glass(th: &Theme, fx: &Fx) -> impl IntoElement {
    let scan_alpha = th.scanline_opacity * fx.flicker_mul;
    let step = th.scanline_step;
    let accent = th.accent;
    let band = fx.band;
    let tracking = th.tracking;
    let vignette = th.vignette * fx.flicker_mul;
    let bloom = th.bloom;

    div()
        .absolute()
        .inset_0()
        // upper-left specular: the room's light source catching the glass
        .when(vignette > 0.001, |el| {
            el.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(px(260.))
                    .h(px(150.))
                    .rounded(px(60.))
                    .bg(linear_gradient(
                        135.,
                        linear_color_stop(gpui::white().alpha(0.05 * vignette), 0.),
                        linear_color_stop(gpui::white().alpha(0.0), 0.75),
                    )),
            )
        })
        .when(bloom > 0.001, |el| {
            // center phosphor bloom (CSS: radial at 50% 42%) approximated with
            // a vertical gradient band — soft and cheap
            el.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(0.))
                    .bottom(px(0.))
                    .bg(linear_gradient(
                        180.,
                        linear_color_stop(accent.alpha(0.0), 0.05),
                        linear_color_stop(accent.alpha(0.05 * bloom), 0.42),
                    )),
            )
        })
        // scanlines + tracking band, one canvas
        .when(std::env::var("TD_NOCANVAS").is_err(), |el| {
            el.child(
                canvas(
                    |_, _, _| (),
                    move |bounds: Bounds<gpui::Pixels>, _, window, _| {
                        let top = f32::from(bounds.origin.y);
                        let bottom = f32::from(bounds.bottom());
                        let x = bounds.origin.x;
                        let w = bounds.size.width;
                        // scanlines: per 4px period — 1px black + 1px faint phosphor
                        if scan_alpha > 0.001 {
                            let dark = hsla(0., 0., 0., scan_alpha);
                            let tint = accent.alpha(scan_alpha * 0.22);
                            let mut y = top;
                            while y < bottom {
                                window.paint_quad(fill(
                                    Bounds::new(point(x, px(y)), size(w, px(1.))),
                                    dark,
                                ));
                                if y + 1. < bottom {
                                    window.paint_quad(fill(
                                        Bounds::new(point(x, px(y + 1.)), size(w, px(1.))),
                                        tint,
                                    ));
                                }
                                y += step;
                            }
                        }
                        // tracking band (CSS: 160px, phosphor .048 / white .018 core)
                        if let (Some(p), true) = (band, tracking > 0.001) {
                            let span = (bottom - top) + BAND_H * 2.;
                            let band_top = top - BAND_H + p * span;
                            let rows = (BAND_H / 2.) as i32;
                            for i in 0..rows {
                                let y = band_top + (i as f32) * 2.;
                                if y < top || y >= bottom {
                                    continue;
                                }
                                // triangle profile peaking at band center
                                let d = 1. - ((i as f32 / rows as f32) - 0.5).abs() * 2.;
                                let a = d * d * 0.05 * tracking;
                                let core = d > 0.92;
                                let color: Hsla = if core {
                                    hsla(0., 0., 1., 0.018 * tracking)
                                } else {
                                    accent.alpha(a)
                                };
                                window.paint_quad(fill(
                                    Bounds::new(point(x, px(y)), size(w, px(1.))),
                                    color,
                                ));
                                // band-local darker scanline (every other row)
                                if i % 2 == 0 {
                                    window.paint_quad(fill(
                                        Bounds::new(point(x, px(y + 1.)), size(w, px(1.))),
                                        hsla(0., 0., 0., 0.10 * tracking * d),
                                    ));
                                }
                            }
                        }
                    },
                )
                .size_full(),
            )
        })
        // curved-glass edge fade (CSS: inset 80px/.78 + 180px/.56 + phosphor 34px)
        .when(vignette > 0.001, |el| {
            el.child(div().absolute().inset_0().rounded_lg().shadow(vec![
                BoxShadow {
                    color: hsla(0., 0., 0., 0.78 * vignette),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(80.),
                    spread_radius: px(-12.),
                    inset: true,
                },
                BoxShadow {
                    color: hsla(0., 0., 0., 0.56 * vignette),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(180.),
                    spread_radius: px(0.),
                    inset: true,
                },
                BoxShadow {
                    color: accent.alpha(0.06 * vignette),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(34.),
                    spread_radius: px(0.),
                    inset: true,
                },
            ]))
        })
}

// ── Ignition ────────────────────────────────────────────────────────────────
//
// A tube does not simply appear. It fires: the phosphor floods from the top and
// bottom edges, the flood collapses into a single scan line, and the line pinches
// to a point that fades — the beam converging, run in reverse of a power-down.
//
// Deliberately short (`IGNITION_MS`) — punctuation on a new pane, not a cutscene.
// It plays ONLY on a tube with barrel warp, because the flat pane isn't pretending
// to be a CRT and a flash on it would just be a flash.
//
// Everything here draws plain quads and gradients INSIDE the pane's registered
// warp tube, so the shader bends it and the glass lays scanlines over it for
// free — the curvature and the lines in the reference frames are not drawn here,
// they are what the tube does to whatever it is shown.

/// How long a tube takes to come up.
pub const IGNITION_MS: u64 = 300;

/// The flood has reached the middle by here; the screen is fully lit.
const BLOOM_END: f32 = 0.40;
/// The lit screen has collapsed to a single line by here.
const COLLAPSE_END: f32 = 0.68;
/// Thickness of the star's arms, as a fraction of the screen's short axis.
const ARM: f32 = 0.012;

/// Which of the three movements the ignition is in. Each carries only the
/// geometry that movement needs, so an impossible combination (a bloom with a
/// star width) cannot be represented.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum IgnitionPhase {
    /// Light sweeping IN from the top and bottom edges. `lit` is how far each
    /// edge has travelled toward the middle, `0.0` → `0.5` of the height; at
    /// `0.5` the two halves meet and the screen is whole.
    Bloom { lit: f32 },
    /// The lit screen collapsing to a scan line. `half` is the band's
    /// half-height, `0.5` → `0.0`.
    Collapse { half: f32 },
    /// The line pinching to a point. `width` is the horizontal streak's extent,
    /// `1.0` → `0.0`; `spike` is the vertical flare's half-length, which is what
    /// turns the streak into a four-pointed star rather than a dash.
    Pinch { width: f32, spike: f32 },
}

/// The whole ignition at one instant.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Ignition {
    pub phase: IgnitionPhase,
    /// Brightness of the flash itself, `0..1`.
    pub glow: f32,
    /// Opacity of the dark tube face the flash sits on, `1..0`. It starts opaque
    /// — a tube that has not lit yet shows nothing — and is gone by the end, so
    /// the terminal emerges from *behind* the collapse instead of popping in
    /// after it.
    pub ground: f32,
}

/// Pin the ignition at one instant instead of letting it run, so a transient
/// effect can be framed headlessly. `TD_IGNITION_FREEZE=0.55` holds the arc at
/// 55% for as long as the app is up; unset, the ignition plays normally and
/// this costs one failed env lookup per new pane.
///
/// The same shape as `TD_FOCUS_DEMO`, and for the same reason: an effect that
/// only exists for 300ms cannot be reviewed by a human with a screenshot key,
/// and until it can be captured, "it looks right" is nobody's claim to make.
pub fn ignition_freeze() -> Option<f32> {
    std::env::var("TD_IGNITION_FREEZE")
        .ok()?
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|t| (0.0..1.0).contains(t))
}

/// The ignition at normalised time `t` (0 at the pane's birth, 1 at
/// `IGNITION_MS`). `None` once it is over, which is what stops the overlay
/// being built at all for the rest of the pane's life.
///
/// Pure, so the arc is testable without a window: the phases are ordered, the
/// ground only ever falls (the terminal cannot re-hide once it has begun to
/// show), and every value stays in range.
pub fn ignition(t: f32) -> Option<Ignition> {
    if t >= 1.0 {
        return None;
    }
    let t = t.max(0.0);
    let (phase, glow, ground) = if t < BLOOM_END {
        // Ease OUT: the phosphor floods fast and settles, the way a tube's
        // brightness overshoots and levels rather than ramping linearly.
        let p = t / BLOOM_END;
        let e = 1.0 - (1.0 - p) * (1.0 - p);
        (IgnitionPhase::Bloom { lit: 0.5 * e }, 0.35 + 0.45 * e, 1.0)
    } else if t < COLLAPSE_END {
        // Ease IN: convergence accelerates — slow to let go, then all at once.
        let p = (t - BLOOM_END) / (COLLAPSE_END - BLOOM_END);
        let e = p * p;
        (
            IgnitionPhase::Collapse {
                half: 0.5 * (1.0 - e),
            },
            0.8 + 0.2 * e,
            1.0,
        )
    } else {
        let p = (t - COLLAPSE_END) / (1.0 - COLLAPSE_END);
        let e = p * p;
        (
            IgnitionPhase::Pinch {
                width: 1.0 - e,
                spike: 0.26 * (1.0 - e),
            },
            // the star stays hot while it shrinks and dies only at the end —
            // `p²` keeps it near full for the first half of the pinch
            1.0 - e,
            // …while the terminal comes up behind it, linearly, so the reveal
            // is already well under way before the star is gone
            1.0 - p,
        )
    };
    Some(Ignition {
        phase,
        glow: glow.clamp(0.0, 1.0),
        ground: ground.clamp(0.0, 1.0),
    })
}

/// The ignition overlay for one frame. Sits above the grid and below the glass,
/// inside the pane's screen area, so it is clipped to the tube and picks up the
/// scanlines and curvature rather than drawing its own.
///
/// Gradient angles follow CSS: `0.` points at the top, so stop 0 sits at the
/// BOTTOM and stop 1 at the top; `180.` is top-to-bottom, `90.` left-to-right,
/// `270.` right-to-left. Each arm is laid bright-at-the-centre, fading outward.
pub fn ignition_flash(ign: Ignition) -> impl IntoElement {
    let hot = gpui::white().alpha(ign.glow);
    let gone = gpui::white().alpha(0.0);
    // one arm of the flash: a positioned quad carrying a two-stop fade
    let lit = move |angle: f32| {
        div().absolute().bg(linear_gradient(
            angle,
            linear_color_stop(hot, 0.),
            linear_color_stop(gone, 1.),
        ))
    };

    let parts: Vec<gpui::AnyElement> = match ign.phase {
        IgnitionPhase::Bloom { lit: reach } => vec![
            // flooding down from the top edge…
            lit(180.)
                .left_0()
                .right_0()
                .top_0()
                .h(gpui::relative(reach))
                .into_any_element(),
            // …and up from the bottom, to meet in the middle
            lit(0.)
                .left_0()
                .right_0()
                .bottom_0()
                .h(gpui::relative(reach))
                .into_any_element(),
        ],
        IgnitionPhase::Collapse { half } => vec![
            // the band is brightest along its centre line, so it is drawn as two
            // halves fading away from that seam
            lit(0.)
                .left_0()
                .right_0()
                .top(gpui::relative(0.5 - half))
                .h(gpui::relative(half))
                .into_any_element(),
            lit(180.)
                .left_0()
                .right_0()
                .top(gpui::relative(0.5))
                .h(gpui::relative(half))
                .into_any_element(),
        ],
        IgnitionPhase::Pinch { width, spike } => vec![
            // left and right arms of the star
            lit(270.)
                .right(gpui::relative(0.5))
                .w(gpui::relative(width * 0.5))
                .top(gpui::relative(0.5 - ARM * 0.5))
                .h(gpui::relative(ARM))
                .into_any_element(),
            lit(90.)
                .left(gpui::relative(0.5))
                .w(gpui::relative(width * 0.5))
                .top(gpui::relative(0.5 - ARM * 0.5))
                .h(gpui::relative(ARM))
                .into_any_element(),
            // and the vertical flare that makes it a star instead of a dash
            lit(0.)
                .bottom(gpui::relative(0.5))
                .h(gpui::relative(spike))
                .left(gpui::relative(0.5 - ARM * 0.5))
                .w(gpui::relative(ARM))
                .into_any_element(),
            lit(180.)
                .top(gpui::relative(0.5))
                .h(gpui::relative(spike))
                .left(gpui::relative(0.5 - ARM * 0.5))
                .w(gpui::relative(ARM))
                .into_any_element(),
        ],
    };

    div()
        .absolute()
        .inset_0()
        .overflow_hidden()
        // the unlit tube face, which the flash is happening ON
        .child(
            div()
                .absolute()
                .inset_0()
                .bg(gpui::black().alpha(ign.ground)),
        )
        .children(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The arc runs bloom → collapse → pinch and then stops existing. `None` is
    /// what keeps a pane from carrying a dead overlay for the rest of its life.
    #[test]
    fn ignition_runs_three_movements_then_ends() {
        assert!(matches!(
            ignition(0.0).unwrap().phase,
            IgnitionPhase::Bloom { .. }
        ));
        assert!(matches!(
            ignition(0.5).unwrap().phase,
            IgnitionPhase::Collapse { .. }
        ));
        assert!(matches!(
            ignition(0.8).unwrap().phase,
            IgnitionPhase::Pinch { .. }
        ));
        assert_eq!(ignition(1.0), None, "over at t=1");
        assert_eq!(ignition(4.2), None, "and stays over");
    }

    /// The tube is DARK at the instant a pane is born. If the ground started
    /// even slightly transparent you would read the terminal's first frame
    /// through the flash, which is the one thing the effect exists to prevent.
    #[test]
    fn a_new_tube_starts_opaque_and_only_ever_opens_up() {
        assert_eq!(ignition(0.0).unwrap().ground, 1.0);
        let mut prev = 1.0;
        for i in 0..=1000 {
            let Some(ign) = ignition(i as f32 / 1000.0) else {
                continue;
            };
            assert!(
                ign.ground <= prev + 1e-6,
                "the terminal must never re-hide: {} → {} at t={}",
                prev,
                ign.ground,
                i as f32 / 1000.0
            );
            prev = ign.ground;
        }
        // and by the last frame the pane is fully in the clear
        assert!(ignition(0.999).unwrap().ground < 0.02);
    }

    /// Each movement sweeps its own geometry the whole way, in the right
    /// direction: the flood closes to the middle, the band closes to a line, the
    /// line closes to a point. A movement that stopped short would leave a
    /// visible seam at the handover.
    #[test]
    fn every_movement_completes_its_travel() {
        let lit_at = |t| match ignition(t).unwrap().phase {
            IgnitionPhase::Bloom { lit } => lit,
            other => panic!("expected bloom at {t}, got {other:?}"),
        };
        assert!(lit_at(0.0) < 0.01, "starts dark");
        assert!(lit_at(0.399) > 0.49, "the two halves meet");

        let half_at = |t| match ignition(t).unwrap().phase {
            IgnitionPhase::Collapse { half } => half,
            other => panic!("expected collapse at {t}, got {other:?}"),
        };
        assert!(half_at(0.401) > 0.49, "picks up a full screen");
        assert!(half_at(0.679) < 0.01, "and hands over a line");

        let width_at = |t| match ignition(t).unwrap().phase {
            IgnitionPhase::Pinch { width, .. } => width,
            other => panic!("expected pinch at {t}, got {other:?}"),
        };
        assert!(width_at(0.681) > 0.99, "picks up a full-width line");
        assert!(width_at(0.999) < 0.01, "and pinches to nothing");
    }

    /// Nothing leaves 0..1 anywhere in the sweep — an alpha above 1 clips to a
    /// flat white frame, one below 0 silently drops the element.
    #[test]
    fn the_whole_sweep_stays_in_range() {
        for i in 0..=1000 {
            let t = i as f32 / 1000.0;
            let Some(ign) = ignition(t) else { continue };
            assert!((0.0..=1.0).contains(&ign.glow), "glow {} at {t}", ign.glow);
            assert!((0.0..=1.0).contains(&ign.ground));
            match ign.phase {
                IgnitionPhase::Bloom { lit } => assert!((0.0..=0.5).contains(&lit)),
                IgnitionPhase::Collapse { half } => assert!((0.0..=0.5).contains(&half)),
                IgnitionPhase::Pinch { width, spike } => {
                    assert!((0.0..=1.0).contains(&width));
                    assert!((0.0..=1.0).contains(&spike));
                }
            }
        }
    }

    /// A negative `t` can arrive from a clock that has not ticked yet; it must
    /// read as the very start, never as a panic or a phase skip.
    #[test]
    fn a_clock_that_has_not_started_reads_as_the_beginning() {
        assert_eq!(ignition(-0.5), ignition(0.0));
    }
}
