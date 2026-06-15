//! 🎰 GAMBA — the slot-machine "thinking" overlay.
//!
//! Pure satire: when an AI agent in a pane is *thinking* (it's printing its
//! "esc to interrupt" spinner), the GAMBA theme turns that wait into a slot
//! machine. A reel drops in and locks on a symbol; the longer the agent
//! cooks, the more reels stack — on an every-OTHER-Fibonacci timer (3s, 8s,
//! 21s, 55s, 144s, …). Three of a kind pops a banner. Vibe coding, but it's
//! literally gambling. The whole thing is non-occluding GPU quads/text — it
//! carries no mouse handlers, so input passes straight through to the shell.
//!
//! Gated to the GAMBA look (theme `gamba` or the RETRO colour set); force it on
//! for any theme with `TD_GAMBA=1`, and force the reels to roll without a real
//! agent (for demos/screenshots) with `TD_GAMBA_DEMO=1`.

use std::time::Instant;

use gpui::{div, hsla, point, prelude::*, px, BoxShadow, Div, FontWeight, Hsla};

use crate::theme::Theme;

/// The reel face symbols — a degen-finance / AI-hype slot set. Order matters
/// only for the spin cycle; locks pick uniformly at random.
const SYMBOLS: &[&str] = &[
    "🍒", "7️⃣", "💰", "💎", "🚀", "🤖", "🤡", "💀", "🔥", "🧠", "🤑", "📉", "🃏", "💸",
];

/// Each locked symbol's jackpot shout when it lands three-in-a-row. The joke is
/// the *whole point*, so every symbol gets a line.
fn jackpot_line(sym: &str) -> &'static str {
    match sym {
        "🍒" => "🍒 JACKPOT 🍒",
        "7️⃣" => "LUCKY SEVENS",
        "💰" => "BIG MONEY BIG MONEY",
        "💎" => "💎 DIAMOND HANDS 💎",
        "🚀" => "TO THE MOON",
        "🤖" => "AGI ACHIEVED",
        "🤡" => "RUGGED",
        "💀" => "TOTALLY REKT",
        "🔥" => "ON TILT",
        "🧠" => "GALAXY BRAIN",
        "🤑" => "WHALE ALERT",
        "📉" => "NUMBER GO DOWN",
        "🃏" => "WILD CARD",
        "💸" => "TOKENS WAGERED",
        _ => "WINNER WINNER",
    }
}

/// One reel: when it should drop in (seconds since thinking started) and the
/// symbol it locks on once it stops spinning.
struct Reel {
    appear_at: f32,
    final_sym: usize,
}

/// How long a fresh reel spins (seconds) before it snaps to its final symbol.
const SPIN_SECS: f32 = 1.15;
/// Symbols cycled per second while spinning — fast enough to blur into a roll.
const SPIN_RATE: f32 = 13.0;
/// Glitter burst lasts this long (seconds) after a reel locks.
const BURST_SECS: f32 = 0.7;
/// Stop stacking past this many reels — by here you have *clearly* lost.
const MAX_REELS: usize = 7;

/// Per-pane slot-machine state, advanced by the pane's effects ticker.
pub struct Reels {
    /// `Some(start)` while the agent is thinking; `None` when idle.
    thinking_since: Option<Instant>,
    rng: u64,
    reels: Vec<Reel>,
    /// Next drop time (seconds) and the two terms of the every-other-Fibonacci
    /// recurrence `next = 3*b - a` that generates 3, 8, 21, 55, 144, 377, …
    next_at: f32,
    fib_a: f32,
    fib_b: f32,
}

impl Reels {
    pub fn new(seed: u64) -> Self {
        Self {
            thinking_since: None,
            rng: 0x9E3779B97F4A7C15 ^ seed,
            reels: Vec::new(),
            next_at: 3.0,
            fib_a: 3.0,
            fib_b: 8.0,
        }
    }

    fn rand(&mut self) -> u64 {
        // xorshift64* — small, no deps, plenty for picking reel faces.
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        self.rng.wrapping_mul(0x2545F4914F6CDD1D)
    }

    /// True while the machine is rolling (so the ticker keeps high-FPS frames).
    pub fn is_thinking(&self) -> bool {
        self.thinking_since.is_some()
    }

    /// Flip the thinking state. Turning on (re)starts a fresh pull; turning off
    /// clears the reels so the next think starts clean.
    pub fn set_thinking(&mut self, on: bool) {
        match (on, self.thinking_since.is_some()) {
            (true, false) => {
                self.thinking_since = Some(Instant::now());
                self.reels.clear();
                self.next_at = 3.0;
                self.fib_a = 3.0;
                self.fib_b = 8.0;
            }
            (false, true) => {
                self.thinking_since = None;
                self.reels.clear();
            }
            _ => {}
        }
    }

    /// Advance the stack: drop any reels whose time has come. Returns true while
    /// thinking (the overlay animates continuously, so always redraw).
    pub fn tick(&mut self) -> bool {
        let Some(start) = self.thinking_since else {
            return false;
        };
        let t = start.elapsed().as_secs_f32();
        while self.reels.len() < MAX_REELS && t >= self.next_at {
            let appear_at = self.next_at;
            let final_sym = (self.rand() as usize) % SYMBOLS.len();
            self.reels.push(Reel {
                appear_at,
                final_sym,
            });
            // every-other-Fibonacci step: 3, 8, 21, 55, 144, 377, 987…
            // `fib_a` is the current threshold, `fib_b` the one after it; each
            // step slides the window forward via `next = 3*b - a`.
            let next = 3.0 * self.fib_b - self.fib_a;
            self.fib_a = self.fib_b;
            self.fib_b = next;
            self.next_at = self.fib_a;
        }
        true
    }

    fn elapsed(&self) -> f32 {
        self.thinking_since
            .map(|s| s.elapsed().as_secs_f32())
            .unwrap_or(0.0)
    }
}

/// Should this pane show the GAMBA reels? True on the `gamba` theme, when the
/// RETRO colour set is active, or when `TD_GAMBA=1` forces it on any look.
pub fn look_active(th: &Theme, dynamic_is_retro: bool) -> bool {
    th.name == "gamba" || dynamic_is_retro || std::env::var("TD_GAMBA").is_ok()
}

/// The reel overlay, anchored bottom-center of the pane. `None` when nothing is
/// rolling. Non-occluding: no mouse handlers, so the shell underneath stays live.
pub fn overlay(reels: &Reels, th: &Theme) -> Option<Div> {
    if !reels.is_thinking() || reels.reels.is_empty() {
        return None;
    }
    let t = reels.elapsed();
    let gold = th.accent;
    let cream = hsla(40. / 360., 0.55, 0.92, 1.0);
    let cabinet_lo = hsla(28. / 360., 0.72, 0.18, 0.96);
    let cabinet_hi = hsla(40. / 360., 0.85, 0.34, 0.96);

    // ---- the marquee header: the satire line + wager clock ----
    let mins = (t as u64) / 60;
    let secs = (t as u64) % 60;
    let pulls = reels.reels.len();
    let clock = if mins > 0 {
        format!("{mins}m {secs:02}s")
    } else {
        format!("{secs}s")
    };
    let header = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_center()
        .gap_2()
        .pb(px(5.))
        .text_size(px(13.))
        .font_weight(FontWeight::BOLD)
        .text_color(cream)
        .child(div().text_color(gold).child("🎰 VIBE SLOTS 🎰"))
        .child(format!("feeding the machine · {clock} · {pulls} pull(s)"));

    // ---- a strip of chasing marquee bulbs above the reels ----
    let marquee = div()
        .flex()
        .flex_row()
        .justify_center()
        .gap_1()
        .pb(px(6.))
        .children((0..16).map(|k| {
            let on = (((t * 7.0) as i64 + k as i64) % 3) == 0;
            div()
                .w(px(7.))
                .h(px(7.))
                .rounded_full()
                .bg(if on { gold } else { th.cursor.alpha(0.35) })
                .shadow(vec![BoxShadow {
                    color: gold.alpha(if on { 0.8 } else { 0.0 }),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(5.),
                    spread_radius: px(0.),
                    inset: false,
                }])
        }));

    // ---- the row of reels ----
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_center()
        .gap_2();
    for (i, reel) in reels.reels.iter().enumerate() {
        let age = t - reel.appear_at;
        let spinning = age < SPIN_SECS;
        let sym = if spinning {
            // a fast roll, desynced per reel so they don't move in lockstep
            let idx = ((t * SPIN_RATE) as usize + i * 5) % SYMBOLS.len();
            SYMBOLS[idx]
        } else {
            SYMBOLS[reel.final_sym]
        };
        // raised reel-face tile: cream gradient + chunky inset bevel so the
        // symbol reads as standing proud of the cabinet.
        let mut tile = div()
            .w(px(66.))
            .h(px(76.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(10.))
            .bg(cream)
            .border_2()
            .border_color(gold)
            .text_size(px(42.))
            .shadow(vec![
                // top-left highlight — the high edge of the bevel
                BoxShadow {
                    color: gpui::white().alpha(0.85),
                    offset: point(px(1.5), px(1.5)),
                    blur_radius: px(0.),
                    spread_radius: px(0.),
                    inset: true,
                },
                // bottom-right recess — the low edge
                BoxShadow {
                    color: hsla(0., 0., 0., 0.45),
                    offset: point(px(-1.5), px(-2.)),
                    blur_radius: px(3.),
                    spread_radius: px(0.),
                    inset: true,
                },
                // outer drop so the whole reel sits proud of the cabinet
                BoxShadow {
                    color: hsla(0., 0., 0., 0.55),
                    offset: point(px(0.), px(3.)),
                    blur_radius: px(8.),
                    spread_radius: px(-2.),
                    inset: false,
                },
            ])
            .child(sym);

        // glitter burst right after a reel locks: sparkles fade over BURST_SECS
        let lock_age = age - SPIN_SECS;
        if (0.0..BURST_SECS).contains(&lock_age) {
            let fade = 1.0 - lock_age / BURST_SECS;
            tile = tile.child(spark(px(-7.), px(-7.), fade));
            tile = tile.child(spark(px(54.), px(-5.), fade * 0.8));
            tile = tile.child(spark(px(-5.), px(58.), fade * 0.7));
        } else if !spinning {
            // settled reels keep a slow twinkle so the cabinet stays alive
            let tw = (0.5 + 0.5 * ((t * 2.0 + i as f32).sin())).clamp(0.0, 1.0) * 0.6;
            tile = tile.child(spark(px(56.), px(58.), tw));
        }
        row = row.child(tile);
    }

    // ---- three-of-a-kind banner (checks the most recent three locked reels) ----
    let settled: Vec<usize> = reels
        .reels
        .iter()
        .filter(|r| t - r.appear_at >= SPIN_SECS)
        .map(|r| r.final_sym)
        .collect();
    let jackpot = (settled.len() >= 3)
        .then(|| {
            let n = settled.len();
            let (a, b, c) = (settled[n - 3], settled[n - 2], settled[n - 1]);
            (a == b && b == c).then_some(a)
        })
        .flatten()
        .map(|s| {
            let blink = (t * 6.0).sin() > 0.0;
            div()
                .mt(px(5.))
                .px(px(10.))
                .py(px(2.))
                .rounded(px(6.))
                .bg(if blink { gold } else { th.cursor })
                .text_color(hsla(0., 0., 0.08, 1.0))
                .font_weight(FontWeight::EXTRA_BOLD)
                .text_size(px(16.))
                .child(jackpot_line(SYMBOLS[s]))
        });

    // ---- the cabinet card that holds it all ----
    let card = div()
        .flex()
        .flex_col()
        .items_center()
        .px(px(14.))
        .py(px(10.))
        .rounded(px(14.))
        .bg(gpui::linear_gradient(
            165.,
            gpui::linear_color_stop(cabinet_hi, 0.),
            gpui::linear_color_stop(cabinet_lo, 1.),
        ))
        .border_2()
        .border_color(gold)
        .shadow(vec![
            BoxShadow {
                color: gold.alpha(0.55),
                offset: point(px(0.), px(0.)),
                blur_radius: px(22.),
                spread_radius: px(2.),
                inset: false,
            },
            BoxShadow {
                color: hsla(0., 0., 0., 0.6),
                offset: point(px(0.), px(8.)),
                blur_radius: px(20.),
                spread_radius: px(-4.),
                inset: false,
            },
        ])
        .child(header)
        .child(marquee)
        .child(row)
        .children(jackpot);

    // anchor bottom-center, full-width band so it centers; non-occluding.
    Some(
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(px(18.))
            .flex()
            .flex_row()
            .justify_center()
            .child(card),
    )
}

/// A single sparkle glyph, absolutely placed within a reel tile.
fn spark(left: gpui::Pixels, top: gpui::Pixels, alpha: f32) -> Div {
    div()
        .absolute()
        .left(left)
        .top(top)
        .text_size(px(14.))
        .text_color(Hsla {
            h: 48. / 360.,
            s: 1.0,
            l: 0.85,
            a: alpha.clamp(0.0, 1.0),
        })
        .child("✦")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_other_fibonacci_drop_schedule() {
        // Reels drop at 3, 8, 21, 55, 144, 377, 987s — every OTHER Fibonacci
        // number — generated by the `next = 3*b - a` recurrence used in `tick`.
        let mut r = Reels::new(1);
        let mut got = vec![r.next_at];
        for _ in 0..6 {
            let next = 3.0 * r.fib_b - r.fib_a;
            r.fib_a = r.fib_b;
            r.fib_b = next;
            r.next_at = r.fib_a;
            got.push(r.next_at);
        }
        assert_eq!(got, vec![3., 8., 21., 55., 144., 377., 987.]);
    }

    #[test]
    fn idle_machine_does_not_roll() {
        let mut r = Reels::new(7);
        assert!(!r.is_thinking());
        assert!(!r.tick());
        r.set_thinking(true);
        assert!(r.is_thinking());
        r.set_thinking(false);
        assert!(!r.is_thinking() && r.reels.is_empty());
    }
}
