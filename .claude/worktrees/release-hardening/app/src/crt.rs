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
