//! 🎰 GAMBA — the slot-machine "thinking" overlay.
//!
//! Pure satire: when an AI agent in a pane is *thinking* (its "esc to
//! interrupt" spinner is up), the wait becomes a slot machine. A 3×3 board of
//! wheels fills **row by row** on an every-other-Fibonacci timer — top row
//! locks at 5s, middle at 13s, bottom at 34s — each wheel spinning down and
//! clunking into place left-to-right. A minute after a board resolves, a fresh
//! board rolls beside it (bounded to the pane width).
//!
//! Lines / X / L of three matching glyphs **blink → WINNER → lock with a gold
//! border** while the losing wheels fade; the pattern sprays bouncing Sonic-ring
//! tokens and a **10 / 100 / 1000** splash (a single line is 10, a compound
//! pattern 100, a full-board blackout 1000) and sets off a glitter bomb across
//! the whole sub-terminal. The library is tiny (4 glyphs) so you hit often.
//!
//! All GPU quads/text, non-occluding (no mouse handlers) — input passes
//! straight through to the shell.
//!
//! Gated to the GAMBA look (theme `gamba` / the RETRO colour set); `TD_GAMBA=1`
//! forces it on any theme, `TD_GAMBA_DEMO=1` rolls without a live agent.

use std::time::Instant;

use gpui::{div, hsla, point, prelude::*, px, relative, BoxShadow, Div, FontWeight, Hsla};

use crate::theme::Theme;

/// The reel library — the worst-offending AI-slop glyphs. Small on purpose so
/// matches land often. Swap freely; matches and scoring adapt to the length.
const LIB: &[&str] = &["🚀", "✨", "🔥", "🧠"];

/// Row lock times (seconds from a board's start): every OTHER Fibonacci number.
const ROW_AT: [f32; 3] = [5.0, 13.0, 34.0];
/// Within a row, wheels lock left→right with this stagger (the slot-machine clunk).
const COL_STAGGER: f32 = 0.8;
/// A wheel decelerates over this window (seconds) before its lock time.
const SPIN_WINDOW: f32 = 2.4;
/// Symbols rolled through during the deceleration window.
const SPINS: f32 = 11.0;
/// Free-spin speed (symbols/sec) before a wheel enters its decel window.
const FREE_SPEED: f32 = 9.0;
/// A board is fully locked at this time from its start.
const RESOLVE_AT: f32 = ROW_AT[2] + 2.0 * COL_STAGGER + 0.25;
/// "BLINK BLINK BLINK WINNER" runs for this long after a board resolves.
const BLINK_DUR: f32 = 1.5;
/// Wait this long after a board resolves before the next board rolls.
const NEXT_BOARD_GAP: f32 = 60.0;
/// Boards laid across the pane — capped so a row of them stays within the width.
const MAX_BOARDS: usize = 2;
/// Glitter bomb / token spray lifetime (seconds).
const FX_LIFE: f32 = 2.6;

/// Board phase timing. Production matches the brief (rows at 5/13/34s, a fresh
/// board a minute after each resolves). `TD_GAMBA_DEMO=1` uses a compressed
/// schedule so the whole 3×3 fills in seconds — for screenshots and quick looks.
#[derive(Clone, Copy)]
struct Timing {
    row_at: [f32; 3],
    stagger: f32,
    spin_window: f32,
    resolve_at: f32,
    next_gap: f32,
}
impl Timing {
    fn production() -> Self {
        Self {
            row_at: ROW_AT,
            stagger: COL_STAGGER,
            spin_window: SPIN_WINDOW,
            resolve_at: RESOLVE_AT,
            next_gap: NEXT_BOARD_GAP,
        }
    }
    fn demo() -> Self {
        let row_at = [1.6, 3.2, 5.2];
        let stagger = 0.5;
        Self {
            row_at,
            stagger,
            spin_window: 1.1,
            resolve_at: row_at[2] + 2.0 * stagger + 0.2,
            next_gap: 7.0,
        }
    }
}

/// One 3×3 board (a single "pull").
struct Board {
    start: f32,
    cells: [usize; 9], // final symbol index per cell, row-major
    scored: bool,
    winners: u16, // bitmask of cells that are part of a winning line
    value: u32,   // 0 / 10 / 100 / 1000
}

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
    boards: Vec<Board>,
    next_board_at: f32,
    tokens: Vec<Token>,
    glints: Vec<Glint>,
    splashes: Vec<Splash>,
    demo: bool,
    tm: Timing,
}

impl Reels {
    pub fn new(seed: u64) -> Self {
        let demo = std::env::var("TD_GAMBA_DEMO").is_ok();
        Self {
            thinking_since: None,
            rng: 0x9E3779B97F4A7C15 ^ seed.wrapping_mul(0xD1B54A32D192ED03).max(1),
            last_t: 0.0,
            boards: Vec::new(),
            next_board_at: 0.0,
            tokens: Vec::new(),
            glints: Vec::new(),
            splashes: Vec::new(),
            demo,
            tm: if demo {
                Timing::demo()
            } else {
                Timing::production()
            },
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

    /// Flip the thinking state. On → start a fresh first board; off → clear all.
    pub fn set_thinking(&mut self, on: bool) {
        match (on, self.thinking_since.is_some()) {
            (true, false) => {
                self.thinking_since = Some(Instant::now());
                self.last_t = 0.0;
                self.boards.clear();
                self.tokens.clear();
                self.glints.clear();
                self.splashes.clear();
                self.spawn_board(0.0);
                self.next_board_at = self.tm.resolve_at + BLINK_DUR + self.tm.next_gap;
            }
            (false, true) => {
                self.thinking_since = None;
                self.boards.clear();
                self.tokens.clear();
                self.glints.clear();
                self.splashes.clear();
            }
            _ => {}
        }
    }

    fn spawn_board(&mut self, start: f32) {
        let mut cells = [0usize; 9];
        // demo's first board is rigged to a full-board jackpot so the win FX
        // (gold highlights + token spray + glitter bomb + 1000 splash) reliably
        // shows; every other board, and all of production, is random.
        if self.demo && self.boards.is_empty() {
            let s = (self.rand() as usize) % LIB.len();
            cells = [s; 9];
        } else {
            for c in cells.iter_mut() {
                *c = (self.rand() as usize) % LIB.len();
            }
        }
        self.boards.push(Board {
            start,
            cells,
            scored: false,
            winners: 0,
            value: 0,
        });
    }

    fn elapsed(&self) -> f32 {
        self.thinking_since
            .map(|s| s.elapsed().as_secs_f32())
            .unwrap_or(0.0)
    }

    /// Advance boards, scoring, and token/glitter physics. Returns true while
    /// thinking (the overlay animates continuously → always redraw).
    pub fn tick(&mut self) -> bool {
        if self.thinking_since.is_none() {
            return false;
        }
        let t = self.elapsed();
        let dt = (t - self.last_t).clamp(0.0, 0.05);
        self.last_t = t;
        let tm = self.tm;

        // roll the next board in once the timer comes due (bounded count)
        if self.boards.len() < MAX_BOARDS && t >= self.next_board_at {
            self.spawn_board(t);
            self.next_board_at = t + tm.resolve_at + BLINK_DUR + tm.next_gap;
        }

        // score any board that just finished locking + settled past the blink
        let mut payouts: Vec<(usize, u32)> = Vec::new();
        for (bi, b) in self.boards.iter_mut().enumerate() {
            if !b.scored && t - b.start >= tm.resolve_at {
                let (winners, value) = score(&b.cells);
                b.winners = winners;
                b.value = value;
                b.scored = true;
                if value > 0 {
                    payouts.push((bi, value));
                }
            }
        }
        // spawn the celebration for each fresh payout
        for (bi, value) in payouts {
            self.celebrate(bi, value, t);
        }

        // physics: gravity + floor/wall bounce for the ring tokens
        self.tokens.retain(|tk| t - tk.born < FX_LIFE);
        for tk in self.tokens.iter_mut() {
            tk.vy += 2.0 * dt; // gravity (normalised units/s²)
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

    /// Spray tokens + glitter for a winning board.
    fn celebrate(&mut self, board_idx: usize, value: u32, t: f32) {
        let n_tokens = match value {
            1000 => 40,
            100 => 16,
            _ => 6,
        };
        // tokens erupt from roughly where the board sits (centred-ish, lower half)
        let bx = board_center_x(board_idx, self.boards.len());
        for _ in 0..n_tokens {
            let (rx, ry, rvx, rvy) = (self.randf(), self.randf(), self.randf(), self.randf());
            self.tokens.push(Token {
                x: (bx + (rx - 0.5) * 0.18).clamp(0.04, 0.96),
                y: 0.55 + ry * 0.1,
                vx: (rvx - 0.5) * 0.9,
                vy: -(0.5 + rvy * 0.7),
                born: t,
            });
        }
        // glitter bomb across the whole sub-terminal
        let n_glints = if value >= 100 { 70 } else { 36 };
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
}

/// Where a board's centre sits horizontally (0..1), for token origin.
fn board_center_x(idx: usize, total: usize) -> f32 {
    if total <= 1 {
        0.5
    } else {
        // boards laid in a centred row; idx 0 left, idx 1 right
        0.34 + (idx as f32) * 0.32
    }
}

/// Score a 3×3 board: returns (winning-cell bitmask, payout value).
/// 10 = one line · 100 = two or more lines (X / L / multi) · 1000 = full blackout.
fn score(cells: &[usize; 9]) -> (u16, u32) {
    const LINES: [[usize; 3]; 8] = [
        [0, 1, 2],
        [3, 4, 5],
        [6, 7, 8], // rows
        [0, 3, 6],
        [1, 4, 7],
        [2, 5, 8], // cols
        [0, 4, 8],
        [2, 4, 6], // diagonals
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
const CELL_H: f32 = 60.0;
const CELL_GAP: f32 = 6.0;

/// The full overlay: the board grid(s), the glitter bomb, the bouncing tokens,
/// and the payout splash. Covers the whole pane; carries no mouse handlers.
pub fn overlay(reels: &Reels, th: &Theme) -> Option<Div> {
    if !reels.is_thinking() || reels.boards.is_empty() {
        return None;
    }
    let t = reels.elapsed();
    let gold = th.accent;
    // a glassy reel face tinted to the theme (the "spinner background")
    let face = Hsla {
        h: th.surface.h,
        s: (th.surface.s + 0.05).min(0.5),
        l: (th.surface.l + 0.10).clamp(0.12, 0.30),
        a: 1.0,
    };
    let cabinet_lo = hsla(28. / 360., 0.72, 0.16, 0.97);
    let cabinet_hi = hsla(40. / 360., 0.85, 0.30, 0.97);

    // a row of boards, centred
    let mut boards_row = div()
        .flex()
        .flex_row()
        .items_start()
        .justify_center()
        .gap(px(26.));
    for b in reels.boards.iter() {
        boards_row = boards_row.child(render_board(b, t, reels.tm, gold, face));
    }

    // the cabinet card framing the board(s) — black trim + gold + raised shadow
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
        // black trim ring outside the gold for depth
        .border_color(hsla(0., 0., 0.02, 1.0))
        .border_2()
        .shadow(vec![
            // bright gold top rail (raised)
            BoxShadow {
                color: gold.alpha(0.55),
                offset: point(px(0.), px(1.5)),
                blur_radius: px(0.),
                spread_radius: px(1.),
                inset: true,
            },
            // deep black recess at the base
            BoxShadow {
                color: hsla(0., 0., 0., 0.7),
                offset: point(px(0.), px(-3.)),
                blur_radius: px(6.),
                spread_radius: px(0.),
                inset: true,
            },
            // gold halo
            BoxShadow {
                color: gold.alpha(0.5),
                offset: point(px(0.), px(0.)),
                blur_radius: px(26.),
                spread_radius: px(2.),
                inset: false,
            },
            // black drop so the whole cabinet stands proud
            BoxShadow {
                color: hsla(0., 0., 0., 0.75),
                offset: point(px(0.), px(10.)),
                blur_radius: px(24.),
                spread_radius: px(-4.),
                inset: false,
            },
        ])
        .child(marquee(t, gold, th.cursor))
        .child(boards_row);

    // bottom-anchored cabinet band
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

    // glitter bomb — bright flecks across the whole sub-terminal
    if !reels.glints.is_empty() {
        let mut layer = div().absolute().inset_0();
        for g in reels.glints.iter() {
            let age = t - g.born;
            let life = (1.0 - age / FX_LIFE).clamp(0.0, 1.0);
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

    // bouncing Sonic-ring tokens
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

    // payout splash, centred over the cabinet
    if let Some(s) = reels.splashes.last() {
        let age = t - s.born;
        let rise = (age / 1.7).clamp(0.0, 1.0);
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
                .bottom(relative(0.42 + rise * 0.12))
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

/// A 3×3 board: three rows of three wheels.
fn render_board(b: &Board, t: f32, tm: Timing, gold: Hsla, face: Hsla) -> Div {
    let bt = t - b.start;
    let resolved = bt >= tm.resolve_at;
    let blinking = resolved && (bt - tm.resolve_at) < BLINK_DUR;
    let blink_on = (t * 7.0).sin() > 0.0;
    let mut col = div().flex().flex_col().gap(px(CELL_GAP));
    for r in 0..3usize {
        let mut row = div().flex().flex_row().gap(px(CELL_GAP));
        for c in 0..3usize {
            let idx = r * 3 + c;
            let is_winner = b.winners & (1 << idx) != 0;
            row = row.child(render_cell(
                b, bt, tm, r, c, gold, face, resolved, blinking, blink_on, is_winner,
            ));
        }
        col = col.child(row);
    }
    // a WINNER ribbon during the blink phase
    if blinking && b.value > 0 {
        col = col.child(
            div().mt(px(4.)).flex().justify_center().child(
                div()
                    .px(px(8.))
                    .rounded(px(5.))
                    .bg(if blink_on {
                        gold
                    } else {
                        hsla(0., 0., 0.1, 1.0)
                    })
                    .text_color(hsla(0., 0., 0.06, 1.0))
                    .font_weight(FontWeight::EXTRA_BOLD)
                    .text_size(px(13.))
                    .child("WINNER"),
            ),
        );
    }
    col
}

/// One wheel cell: a recessed window with a vertically scrolling strip of glyphs
/// that decelerates and clunks onto its final symbol.
#[allow(clippy::too_many_arguments)]
fn render_cell(
    b: &Board,
    bt: f32,
    tm: Timing,
    r: usize,
    c: usize,
    gold: Hsla,
    face: Hsla,
    resolved: bool,
    blinking: bool,
    blink_on: bool,
    is_winner: bool,
) -> Div {
    let idx = r * 3 + c;
    let final_sym = b.cells[idx];
    let lock = tm.row_at[r] + c as f32 * tm.stagger;
    let rem = lock - bt;
    let locked = rem <= 0.0;

    // wheel position in symbol units: free-spin → ease-out → land on final
    let pos = if locked {
        final_sym as f32
    } else if rem <= tm.spin_window {
        final_sym as f32 + SPINS * (rem / tm.spin_window).powf(1.6)
    } else {
        final_sym as f32 + SPINS + (rem - tm.spin_window) * FREE_SPEED
    };
    let base = pos.floor();
    let frac = pos - base;

    // win highlight vs losing-cell fade, once the board has resolved
    let winner_glow = resolved && is_winner;
    let loser_fade = resolved && b.value > 0 && !is_winner;
    let border = if winner_glow {
        if blinking && !blink_on {
            gold.alpha(0.25)
        } else {
            gold
        }
    } else {
        hsla(0., 0., 0.02, 1.0) // black trim
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
            // recessed top shadow (the window is sunk into the cabinet)
            BoxShadow {
                color: hsla(0., 0., 0., 0.55),
                offset: point(px(0.), px(2.)),
                blur_radius: px(4.),
                spread_radius: px(0.),
                inset: true,
            },
            // bottom-edge highlight
            BoxShadow {
                color: gpui::white().alpha(0.10),
                offset: point(px(0.), px(-1.5)),
                blur_radius: px(0.),
                spread_radius: px(0.),
                inset: true,
            },
        ]);
    if winner_glow {
        cell = cell.shadow(vec![BoxShadow {
            color: gold.alpha(if blinking && !blink_on { 0.2 } else { 0.85 }),
            offset: point(px(0.), px(0.)),
            blur_radius: px(16.),
            spread_radius: px(1.),
            inset: false,
        }]);
    }

    // the scrolling strip: three glyphs offset by the fractional position
    for k in -1i32..=1 {
        let sym_idx = ((base as i32 + k).rem_euclid(LIB.len() as i32)) as usize;
        let top = (k as f32 - frac) * CELL_H;
        // a locked symbol gets a raised disc behind it so it pops off the face
        let mut slot = div()
            .absolute()
            .top(px(top))
            .left_0()
            .w(px(CELL_W))
            .h(px(CELL_H))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(if locked { 34. } else { 30. }));
        if locked && k == 0 {
            slot = slot.child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(42.))
                    .h(px(42.))
                    .rounded(px(21.))
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
                    .text_size(px(34.))
                    .child(LIB[final_sym]),
            );
        } else {
            slot = slot.child(LIB[sym_idx]);
        }
        cell = cell.child(slot);
    }

    // fade the losing wheels with a dark scrim over the glass
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
    fn row_lock_times_are_every_other_fibonacci() {
        assert_eq!(ROW_AT, [5.0, 13.0, 34.0]);
    }

    #[test]
    fn scoring_lines_x_and_blackout() {
        // one row of three → a single line, 10
        let (_, v) = score(&[0, 0, 0, 1, 2, 3, 3, 2, 1]);
        assert_eq!(v, 10);
        // both diagonals share the centre (an X) of the same glyph → 2 lines, 100
        let (_, v) = score(&[0, 1, 0, 1, 0, 1, 0, 1, 0]);
        assert_eq!(v, 100);
        // every cell identical → full blackout, 1000
        let (w, v) = score(&[2; 9]);
        assert_eq!(v, 1000);
        assert_eq!(w, 0b1_1111_1111);
        // nothing lines up → no payout
        let (_, v) = score(&[0, 1, 2, 3, 2, 1, 1, 3, 0]);
        assert_eq!(v, 0);
    }

    #[test]
    fn idle_machine_does_not_roll() {
        let mut r = Reels::new(7);
        assert!(!r.is_thinking());
        assert!(!r.tick());
        r.set_thinking(true);
        assert!(r.is_thinking() && r.boards.len() == 1);
        r.set_thinking(false);
        assert!(!r.is_thinking() && r.boards.is_empty());
    }
}
