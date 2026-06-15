//! 🎰 GAMBA — the slot-machine "thinking" overlay.
//!
//! Pure satire: when an AI agent in a pane is *thinking* (its "esc to
//! interrupt" spinner is up), the wait becomes a slot machine — a 3×3 grid that
//! **rerolls every 10 seconds** for as long as the agent keeps cooking. On each
//! roll the reels land left→right on a fast fixed timer (0.3s / 0.8s / 1.2s),
//! the last column easing in slowest for the hopeful slot-machine pause.
//!
//! Three matching glyphs in a row / column / diagonal (or a full blackout) pay
//! out **10 / 100 / 1000** in bouncing Sonic-ring tokens, set off a glitter
//! bomb across the sub-terminal, light the winning glyphs gold — and the whole
//! terminal **rumbles for 3 seconds** as the coins spill.
//!
//! All GPU quads/text, non-occluding (no mouse handlers) — input passes
//! straight through to the shell.
//!
//! Gated to the GAMBA look (theme `gamba` / the RETRO colour set); `TD_GAMBA=1`
//! forces it on any theme, `TD_GAMBA_DEMO=1` rerolls faster + rigs the first
//! roll to a jackpot for demos/screenshots.

use std::time::Instant;

use gpui::{div, hsla, point, prelude::*, px, relative, BoxShadow, Div, FontWeight, Hsla};

use crate::theme::Theme;

/// The reel library — the worst-offending AI-slop glyphs. Small on purpose so
/// matches land often. Swap freely; matches and scoring adapt to the length.
const LIB: &[&str] = &["🚀", "✨", "🔥", "🧠"];

/// Symbols rolled through during a reel's deceleration window.
const SPINS: f32 = 11.0;
/// Free-spin speed (symbols/sec) before a reel enters its decel window.
const FREE_SPEED: f32 = 9.0;
/// Glitter bomb / token spray lifetime (seconds).
const FX_LIFE: f32 = 2.6;
/// A win rumbles the terminal this long while the coins spill.
const RUMBLE_SECS: f32 = 3.0;
/// The 3×3 grid rerolls this often (seconds). Flat — no escalation.
const ROLL_PERIOD: f32 = 10.0;

/// Within a roll the three columns lock left→right on this fast fixed timer
/// (seconds after the roll starts) — the last column lands at 1.2s and lingers
/// (see `REEL_WIN`) for the hopeful pause.
const REEL_AT: [f32; 3] = [0.3, 0.8, 1.2];
/// Per-column deceleration window. The last column eases in slowest — that's
/// the drawn-out "is it gonna hit?" tease on the final reel.
const REEL_WIN: [f32; 3] = [0.22, 0.30, 0.65];

/// A sprayed Sonic-ring token, in normalised 0..1 pane space.
struct Token {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    born: f32,
}

/// A bright fleck of the glitter bomb, normalised 0..1 pane space.
struct Glint {
    x: f32,
    y: f32,
    born: f32,
    phase: f32,
}

/// A "10/100/1000" payout splash.
struct Splash {
    value: u32,
    born: f32,
}

/// Per-pane slot-machine state, advanced by the pane's effects ticker.
pub struct Reels {
    thinking_since: Option<Instant>,
    rng: u64,
    last_t: f32,
    /// The current roll's 9 symbols, row-major.
    grid: [usize; 9],
    /// Which reroll we're on (0-based; `floor(elapsed / period)`).
    roll: usize,
    /// The last roll that was scored (-1 = none yet).
    scored_roll: i64,
    /// Winning cells of the current roll, bit `r*3+c`.
    winners: u32,
    tokens: Vec<Token>,
    glints: Vec<Glint>,
    splashes: Vec<Splash>,
    /// The terminal rumbles until this time (set on a win).
    rumble_until: f32,
    demo: bool,
    /// Reroll period (seconds) — shorter for demos.
    period: f32,
}

impl Reels {
    pub fn new(seed: u64) -> Self {
        let demo = std::env::var("TD_GAMBA_DEMO").is_ok();
        Self {
            thinking_since: None,
            rng: 0x9E3779B97F4A7C15 ^ seed.wrapping_mul(0xD1B54A32D192ED03).max(1),
            last_t: 0.0,
            grid: [0; 9],
            roll: 0,
            scored_roll: -1,
            winners: 0,
            tokens: Vec::new(),
            glints: Vec::new(),
            splashes: Vec::new(),
            rumble_until: -1.0,
            demo,
            period: if demo { 4.0 } else { ROLL_PERIOD },
        }
    }

    fn rand(&mut self) -> u64 {
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        self.rng.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn randf(&mut self) -> f32 {
        ((self.rand() >> 40) as f32) / (1u64 << 24) as f32
    }

    pub fn is_thinking(&self) -> bool {
        self.thinking_since.is_some()
    }

    /// Roll fresh symbols into the grid. Demo's first roll is rigged to a
    /// full-board jackpot so the win FX + rumble fire on screen reliably.
    fn gen_grid(&mut self, roll: usize) {
        let mut g = [0usize; 9];
        for cell in g.iter_mut() {
            *cell = (self.rand() as usize) % LIB.len();
        }
        // demo's first roll is rigged to a full-board jackpot so the win FX +
        // rumble fire on screen reliably; every other roll is random.
        if self.demo && roll == 0 {
            g = [g[0]; 9];
        }
        self.grid = g;
    }

    /// Flip the thinking state. On → roll the first 3×3; off → clear everything.
    pub fn set_thinking(&mut self, on: bool) {
        match (on, self.thinking_since.is_some()) {
            (true, false) => {
                self.thinking_since = Some(Instant::now());
                self.last_t = 0.0;
                self.roll = 0;
                self.scored_roll = -1;
                self.winners = 0;
                self.rumble_until = -1.0;
                self.tokens.clear();
                self.glints.clear();
                self.splashes.clear();
                self.gen_grid(0);
            }
            (false, true) => {
                self.thinking_since = None;
                self.winners = 0;
                self.tokens.clear();
                self.glints.clear();
                self.splashes.clear();
            }
            _ => {}
        }
    }

    fn elapsed(&self) -> f32 {
        self.thinking_since
            .map(|s| s.elapsed().as_secs_f32())
            .unwrap_or(0.0)
    }

    /// Advance: reroll the grid every period, score it once its reels land, and
    /// step the token/glitter physics. True while thinking (always redraw).
    pub fn tick(&mut self) -> bool {
        if self.thinking_since.is_none() {
            return false;
        }
        let t = self.elapsed();
        let dt = (t - self.last_t).clamp(0.0, 0.05);
        self.last_t = t;

        // reroll on the flat 10s timer
        let cur = (t / self.period) as usize;
        if cur != self.roll {
            self.roll = cur;
            self.winners = 0;
            self.gen_grid(cur);
        }

        // score this roll once its last reel has landed (once per roll)
        let roll_start = self.roll as f32 * self.period;
        if t >= roll_start + REEL_AT[2] && self.scored_roll != self.roll as i64 {
            self.scored_roll = self.roll as i64;
            let (w8, value) = score(&self.grid);
            if value > 0 {
                self.winners = w8 as u32;
                self.celebrate(value, t);
                self.rumble_until = t + RUMBLE_SECS;
            }
        }

        // physics: gravity + floor/wall bounce for the ring tokens
        self.tokens.retain(|tk| t - tk.born < FX_LIFE);
        for tk in self.tokens.iter_mut() {
            tk.vy += 2.0 * dt;
            tk.x += tk.vx * dt;
            tk.y += tk.vy * dt;
            if tk.y > 0.94 {
                tk.y = 0.94;
                tk.vy = -tk.vy * 0.55;
                tk.vx *= 0.86;
            }
            if tk.x < 0.02 {
                tk.x = 0.02;
                tk.vx = tk.vx.abs() * 0.7;
            } else if tk.x > 0.98 {
                tk.x = 0.98;
                tk.vx = -tk.vx.abs() * 0.7;
            }
        }
        self.glints.retain(|g| t - g.born < FX_LIFE);
        self.splashes.retain(|s| t - s.born < 1.7);

        true
    }

    /// Spray tokens + glitter for a win; the splash shows the value.
    fn celebrate(&mut self, value: u32, t: f32) {
        let n_tokens = match value {
            1000 => 40,
            100 => 16,
            _ => 7,
        };
        for _ in 0..n_tokens {
            let (rx, ry, rvx, rvy) = (self.randf(), self.randf(), self.randf(), self.randf());
            self.tokens.push(Token {
                x: (0.5 + (rx - 0.5) * 0.4).clamp(0.04, 0.96),
                y: 0.42 + ry * 0.18,
                vx: (rvx - 0.5) * 1.0,
                vy: -(0.5 + rvy * 0.8),
                born: t,
            });
        }
        let n_glints = if value >= 100 { 70 } else { 38 };
        for _ in 0..n_glints {
            let (gx, gy, gp) = (
                self.randf(),
                self.randf(),
                self.randf() * std::f32::consts::TAU,
            );
            self.glints.push(Glint {
                x: gx,
                y: gy,
                born: t,
                phase: gp,
            });
        }
        self.splashes.push(Splash { value, born: t });
    }

    /// The terminal-shake offset (px) while a win is rumbling — decays over
    /// [`RUMBLE_SECS`] to zero, then returns (0, 0).
    pub fn rumble_offset(&self) -> (f32, f32) {
        if self.thinking_since.is_none() {
            return (0.0, 0.0);
        }
        let t = self.elapsed();
        if t >= self.rumble_until {
            return (0.0, 0.0);
        }
        let amp = 6.0 * ((self.rumble_until - t) / RUMBLE_SECS).clamp(0.0, 1.0);
        ((t * 46.0).sin() * amp, (t * 38.0).cos() * amp)
    }
}

/// Score a 3×3 grid: 10 = one line · 100 = two+ lines (X / L / multi) ·
/// 1000 = all nine identical. Returns (winning-cell bitmask 0..8, value).
fn score(cells: &[usize; 9]) -> (u16, u32) {
    const LINES: [[usize; 3]; 8] = [
        [0, 1, 2],
        [3, 4, 5],
        [6, 7, 8],
        [0, 3, 6],
        [1, 4, 7],
        [2, 5, 8],
        [0, 4, 8],
        [2, 4, 6],
    ];
    let mut winners: u16 = 0;
    let mut lines = 0;
    for l in LINES {
        if cells[l[0]] == cells[l[1]] && cells[l[1]] == cells[l[2]] {
            lines += 1;
            for c in l {
                winners |= 1 << c;
            }
        }
    }
    let blackout = cells.iter().all(|&c| c == cells[0]);
    let value = if blackout {
        1000
    } else if lines >= 2 {
        100
    } else if lines == 1 {
        10
    } else {
        0
    };
    (winners, value)
}

/// Should this pane show the GAMBA reels?
pub fn look_active(th: &Theme, dynamic_is_retro: bool) -> bool {
    th.name == "gamba" || dynamic_is_retro || std::env::var("TD_GAMBA").is_ok()
}

// ---- rendering --------------------------------------------------------------

const CELL_W: f32 = 56.0;
const CELL_H: f32 = 56.0;
const CELL_GAP: f32 = 6.0;

/// The full overlay: the 3×3 reel grid, the glitter bomb, the bouncing tokens,
/// and the payout splash. Covers the whole pane; no mouse handlers.
pub fn overlay(reels: &Reels, th: &Theme) -> Option<Div> {
    if !reels.is_thinking() {
        return None;
    }
    let t = reels.elapsed();
    let roll_start = reels.roll as f32 * reels.period;
    let gold = th.accent;
    let face = Hsla {
        h: th.surface.h,
        s: (th.surface.s + 0.05).min(0.5),
        l: (th.surface.l + 0.10).clamp(0.12, 0.30),
        a: 1.0,
    };
    let cabinet_lo = hsla(28. / 360., 0.72, 0.16, 0.97);
    let cabinet_hi = hsla(40. / 360., 0.85, 0.30, 0.97);
    let any_win = reels.winners != 0;

    // the fixed 3×3 grid; columns land left→right (REEL_AT), last column teases
    let mut grid = div().flex().flex_col().items_center().gap(px(CELL_GAP));
    for r in 0..3 {
        let mut row = div().flex().flex_row().gap(px(CELL_GAP));
        for c in 0..3 {
            let idx = r * 3 + c;
            let win = reels.winners & (1 << idx) != 0;
            let lock = roll_start + REEL_AT[c];
            let window = REEL_WIN[c];
            row = row.child(render_cell(
                reels.grid[idx],
                t,
                lock,
                window,
                gold,
                face,
                win,
                any_win,
            ));
        }
        grid = grid.child(row);
    }

    let card = div()
        .flex()
        .flex_col()
        .items_center()
        .px(px(16.))
        .py(px(12.))
        .rounded(px(16.))
        .bg(gpui::linear_gradient(
            165.,
            gpui::linear_color_stop(cabinet_hi, 0.),
            gpui::linear_color_stop(cabinet_lo, 1.),
        ))
        .border_color(hsla(0., 0., 0.02, 1.0)) // black trim ring for depth
        .border_2()
        .shadow(vec![
            BoxShadow {
                color: gold.alpha(0.55),
                offset: point(px(0.), px(1.5)),
                blur_radius: px(0.),
                spread_radius: px(1.),
                inset: true,
            },
            BoxShadow {
                color: hsla(0., 0., 0., 0.7),
                offset: point(px(0.), px(-3.)),
                blur_radius: px(6.),
                spread_radius: px(0.),
                inset: true,
            },
            BoxShadow {
                color: gold.alpha(0.5),
                offset: point(px(0.), px(0.)),
                blur_radius: px(26.),
                spread_radius: px(2.),
                inset: false,
            },
            BoxShadow {
                color: hsla(0., 0., 0., 0.75),
                offset: point(px(0.), px(10.)),
                blur_radius: px(24.),
                spread_radius: px(-4.),
                inset: false,
            },
        ])
        .child(marquee(t, gold, th.cursor))
        .child(grid);

    let cabinet = div()
        .absolute()
        .left_0()
        .right_0()
        .bottom(px(20.))
        .flex()
        .flex_row()
        .justify_center()
        .child(card);

    let mut root = div().absolute().inset_0().child(cabinet);

    if !reels.glints.is_empty() {
        let mut layer = div().absolute().inset_0();
        for g in reels.glints.iter() {
            let life = (1.0 - (t - g.born) / FX_LIFE).clamp(0.0, 1.0);
            let tw = (0.5 + 0.5 * (t * 9.0 + g.phase).sin()) * life;
            layer = layer.child(
                div()
                    .absolute()
                    .left(relative(g.x))
                    .top(relative(g.y))
                    .text_size(px(13.))
                    .text_color(Hsla {
                        h: 48. / 360.,
                        s: 1.0,
                        l: 0.85,
                        a: tw.clamp(0.0, 1.0),
                    })
                    .child("✦"),
            );
        }
        root = root.child(layer);
    }

    if !reels.tokens.is_empty() {
        let mut layer = div().absolute().inset_0();
        for tk in reels.tokens.iter() {
            layer = layer.child(
                div()
                    .absolute()
                    .left(relative(tk.x))
                    .top(relative(tk.y))
                    .text_size(px(20.))
                    .child("🪙"),
            );
        }
        root = root.child(layer);
    }

    if let Some(s) = reels.splashes.last() {
        let rise = ((t - s.born) / 1.7).clamp(0.0, 1.0);
        let life = 1.0 - rise;
        let label = match s.value {
            1000 => "💥 1000 💥",
            100 => "✦ 100 ✦",
            _ => "+10",
        };
        root = root.child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(relative(0.45 + rise * 0.12))
                .flex()
                .justify_center()
                .child(
                    div()
                        .text_size(px(if s.value >= 1000 { 44. } else { 32. }))
                        .font_weight(FontWeight::EXTRA_BOLD)
                        .text_color(Hsla {
                            l: 0.6,
                            a: life.clamp(0.0, 1.0),
                            ..gold
                        })
                        .child(label),
                ),
        );
    }

    Some(root)
}

/// One reel cell: a recessed window with a vertically scrolling strip of glyphs
/// that decelerates and clunks onto its final symbol.
#[allow(clippy::too_many_arguments)]
fn render_cell(
    final_sym: usize,
    t: f32,
    lock: f32,
    window: f32,
    gold: Hsla,
    face: Hsla,
    win: bool,
    any_win: bool,
) -> Div {
    let rem = lock - t;
    let locked = rem <= 0.0;
    let pos = if locked {
        final_sym as f32
    } else if rem <= window {
        final_sym as f32 + SPINS * (rem / window).powf(1.6)
    } else {
        final_sym as f32 + SPINS + (rem - window) * FREE_SPEED
    };
    let base = pos.floor();
    let frac = pos - base;

    let loser_fade = locked && any_win && !win;
    let border = if locked && win {
        gold
    } else {
        hsla(0., 0., 0.02, 1.0)
    };

    let mut cell = div()
        .relative()
        .w(px(CELL_W))
        .h(px(CELL_H))
        .overflow_hidden()
        .rounded(px(8.))
        .bg(face)
        .border_2()
        .border_color(border)
        .shadow(vec![
            BoxShadow {
                color: hsla(0., 0., 0., 0.55),
                offset: point(px(0.), px(2.)),
                blur_radius: px(4.),
                spread_radius: px(0.),
                inset: true,
            },
            BoxShadow {
                color: gpui::white().alpha(0.10),
                offset: point(px(0.), px(-1.5)),
                blur_radius: px(0.),
                spread_radius: px(0.),
                inset: true,
            },
        ]);
    if locked && win {
        cell = cell.shadow(vec![BoxShadow {
            color: gold.alpha(0.85),
            offset: point(px(0.), px(0.)),
            blur_radius: px(16.),
            spread_radius: px(1.),
            inset: false,
        }]);
    }

    for k in -1i32..=1 {
        let sym_idx = ((base as i32 + k).rem_euclid(LIB.len() as i32)) as usize;
        let top = (k as f32 - frac) * CELL_H;
        let mut slot = div()
            .absolute()
            .top(px(top))
            .left_0()
            .w(px(CELL_W))
            .h(px(CELL_H))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(if locked { 32. } else { 28. }));
        if locked && k == 0 {
            // a locked symbol sits proud on a gold disc so it pops off the face
            slot = slot.child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(40.))
                    .h(px(40.))
                    .rounded(px(20.))
                    .bg(gpui::linear_gradient(
                        160.,
                        gpui::linear_color_stop(gold.alpha(0.45), 0.),
                        gpui::linear_color_stop(hsla(0., 0., 0., 0.0), 1.),
                    ))
                    .shadow(vec![BoxShadow {
                        color: hsla(0., 0., 0., 0.5),
                        offset: point(px(0.), px(2.)),
                        blur_radius: px(4.),
                        spread_radius: px(0.),
                        inset: false,
                    }])
                    .text_size(px(32.))
                    .child(LIB[final_sym]),
            );
        } else {
            slot = slot.child(LIB[sym_idx]);
        }
        cell = cell.child(slot);
    }

    if loser_fade {
        cell = cell.child(div().absolute().inset_0().bg(hsla(0., 0., 0.04, 0.62)));
    }
    cell
}

/// A strip of chasing marquee bulbs across the top of the cabinet.
fn marquee(t: f32, gold: Hsla, alt: Hsla) -> Div {
    let mut m = div().flex().flex_row().justify_center().gap_1().pb(px(7.));
    for k in 0..18 {
        let on = (((t * 7.0) as i64 + k) % 3) == 0;
        m = m.child(
            div()
                .w(px(7.))
                .h(px(7.))
                .rounded_full()
                .bg(if on { gold } else { alt.alpha(0.35) })
                .shadow(vec![BoxShadow {
                    color: gold.alpha(if on { 0.85 } else { 0.0 }),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(5.),
                    spread_radius: px(0.),
                    inset: false,
                }]),
        );
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_reroll_period() {
        // a fixed 3×3 that rerolls on a flat 10s timer — no escalation.
        assert_eq!(ROLL_PERIOD, 10.0);
    }

    #[test]
    fn reels_land_fast_left_to_right_with_a_last_reel_tease() {
        // columns lock at 0.3/0.8/1.2s after the roll — snappy — and the last
        // column eases in slowest (the hopeful pause).
        assert_eq!(REEL_AT, [0.3, 0.8, 1.2]);
        assert!(REEL_AT[0] < REEL_AT[1] && REEL_AT[1] < REEL_AT[2]);
        assert!(REEL_WIN[2] > REEL_WIN[0] && REEL_WIN[2] > REEL_WIN[1]);
    }

    #[test]
    fn scoring_lines_x_and_blackout() {
        assert_eq!(score(&[0, 0, 0, 1, 2, 3, 3, 2, 1]).1, 10);
        assert_eq!(score(&[0, 1, 0, 1, 0, 1, 0, 1, 0]).1, 100);
        let (w, v) = score(&[2; 9]);
        assert_eq!(v, 1000);
        assert_eq!(w, 0b1_1111_1111);
        assert_eq!(score(&[0, 1, 2, 3, 2, 1, 1, 3, 0]).1, 0);
    }

    #[test]
    fn idle_machine_does_not_roll_and_no_rumble() {
        let mut r = Reels::new(7);
        assert!(!r.is_thinking());
        assert!(!r.tick());
        assert_eq!(r.rumble_offset(), (0.0, 0.0));
        r.set_thinking(true);
        assert!(r.is_thinking());
        r.set_thinking(false);
        assert!(!r.is_thinking());
        assert_eq!(r.rumble_offset(), (0.0, 0.0));
    }
}
