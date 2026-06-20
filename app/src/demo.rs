//! "Share a demo of this layout" — the content side.
//!
//! A demo window (see `Workspace::share_demo` + the `TD_DEMO_STATE` boot path in
//! `main`) clones the live tab/split tree and per-pane appearance, but instead of
//! a real shell every pane runs THIS binary in `--td-emit-demo` mode. The
//! emitter ([`emit_and_block`]) reads its PTY window size, prints a screenful of
//! lorem-ipsum styled to look exactly like a real Claude-Code agent session —
//! commit lines, a tool-call block, a results listing, a box-drawn Links table,
//! a recap footer, the shortcuts bar — then blocks so the screen stays frozen.
//!
//! The point is twofold: it's gorgeous to **share** (a faithful twin of your
//! wall with not one real path, prompt, or secret on screen), and the content
//! flows through the *real* render pipeline — alacritty grid → `styled_lines` →
//! the CRT warp/grade/crawl shaders — so it bends and glows identically to a
//! live pane. Content **scales to the pane**: a tall pane gets the full rich
//! transcript; a short one gets just a table and a few lines.
//!
//! [`ipsum`] (the generator) is pure and unit-tested; [`emit_and_block`] is the
//! tiny IO shell around it.

use std::io::{Read, Write};

/// Entry point for the `--td-emit-demo` subcommand. Reads the PTY size, prints
/// the generated screen, then blocks reading stdin so the pane holds its content
/// until terminal-delight closes the PTY (read returns 0/err ⇒ we exit). Never
/// returns under normal use.
pub fn emit_and_block() -> ! {
    let (cols, rows) = pty_size().unwrap_or((100, 28));
    let seed = std::env::var("TD_DEMO_SEED")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_else(|| std::process::id() as u64);

    let lines = ipsum(cols as usize, rows as usize, seed);
    let mut out = std::io::stdout();
    // \r\n: the PTY is in raw-ish cooked mode; be explicit so columns line up.
    let body = lines.join("\r\n");
    let _ = out.write_all(body.as_bytes());
    let _ = out.flush();

    // Hold the pane: drain stdin until EOF/err (i.e. the PTY closed). A plain
    // park() would also work, but reading means we notice the close promptly.
    let mut buf = [0u8; 256];
    let mut stdin = std::io::stdin();
    loop {
        match stdin.read(&mut buf) {
            Ok(0) | Err(_) => std::process::exit(0),
            Ok(_) => {}
        }
    }
}

/// Query the controlling PTY's size via `TIOCGWINSZ` on stdout. `None` if stdout
/// is not a tty (so callers fall back to a sensible default).
fn pty_size() -> Option<(u16, u16)> {
    #[repr(C)]
    struct WinSize {
        ws_row: u16,
        ws_col: u16,
        ws_xpixel: u16,
        ws_ypixel: u16,
    }
    // TIOCGWINSZ is 0x5413 on Linux.
    const TIOCGWINSZ: u64 = 0x5413;
    let mut ws = WinSize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let rc = unsafe { libc::ioctl(1, TIOCGWINSZ, &mut ws as *mut WinSize) };
    if rc == 0 && ws.ws_col > 0 && ws.ws_row > 0 {
        Some((ws.ws_col, ws.ws_row))
    } else {
        None
    }
}

// ---- ANSI styling (kept tiny; the pane's grade recolours, but bold/dim/hue
// give the transcript its structure and survive grading). ----

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const BGREEN: &str = "\x1b[92m";
const CYAN: &str = "\x1b[36m";
const GREY: &str = "\x1b[90m";

/// A tiny deterministic PRNG (xorshift64*) so a pane's content is stable for a
/// given seed but each pane on a wall differs. We can't use `rand`/`Math.random`
/// here and want reproducible tests anyway.
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        // avoid the zero fixed-point
        Rng(seed ^ 0x9e37_79b9_7f4a_7c15 | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[(self.next() as usize) % xs.len()]
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next() % (hi - lo + 1)
    }
}

/// Visible width of a string, ignoring ANSI SGR escapes (so we can fit/truncate
/// to the column count). Good enough for our ASCII + box-drawing content.
fn visible_width(s: &str) -> usize {
    let mut w = 0;
    let mut in_esc = false;
    for c in s.chars() {
        if in_esc {
            if c == 'm' {
                in_esc = false;
            }
            continue;
        }
        if c == '\x1b' {
            in_esc = true;
            continue;
        }
        w += 1;
    }
    w
}

/// Truncate a (possibly styled) line to `cols` visible chars, re-appending RESET
/// if we cut inside a styled run. Keeps long lines from wrapping raggedly.
fn fit(s: &str, cols: usize) -> String {
    if visible_width(s) <= cols {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0;
    let mut in_esc = false;
    for c in s.chars() {
        if in_esc {
            out.push(c);
            if c == 'm' {
                in_esc = false;
            }
            continue;
        }
        if c == '\x1b' {
            in_esc = true;
            out.push(c);
            continue;
        }
        if w >= cols.saturating_sub(1) {
            break;
        }
        out.push(c);
        w += 1;
    }
    out.push_str(RESET);
    out
}

// ---- content fragments (lorem-ipsum that reads like an agent at work) ----

const VERBS: &[&str] = &[
    "refactor",
    "wire up",
    "harden",
    "reconcile",
    "instrument",
    "audit",
    "memoize",
    "debounce",
    "vectorise",
    "back-fill",
    "untangle",
    "shim",
];
const NOUNS: &[&str] = &[
    "the render loop",
    "the snapshot path",
    "the grade group",
    "the warp pass",
    "the tab reaper",
    "the session restore",
    "the event proxy",
    "the shader fork",
    "the pane tree",
    "the notifier",
    "the OSD tray",
    "the hit-test map",
];
const FILES: &[&str] = &[
    "src/main.rs",
    "src/pane.rs",
    "src/theme.rs",
    "src/warp.rs",
    "src/term.rs",
    "src/session.rs",
    "src/crt.rs",
    "crt_pass.wgsl",
];
const MODULES: &[&str] = &[
    "lorem",
    "ipsum",
    "dolor",
    "amet",
    "consectetur",
    "tempor",
    "labore",
    "aliqua",
    "veniam",
    "nostrud",
    "commodo",
    "cupidatat",
];
const TASKS: &[&str] = &[
    "lorem ipsum dolor sit amet, consectetur adipiscing elit",
    "sed do eiusmod tempor incididunt ut labore et dolore magna",
    "ut enim ad minim veniam, quis nostrud exercitation ullamco",
    "duis aute irure dolor in reprehenderit in voluptate velit",
    "excepteur sint occaecat cupidatat non proident, sunt in culpa",
];

fn sgr(color: &str, weight: &str, text: &str) -> String {
    format!("{color}{weight}{text}{RESET}")
}

/// Build a pool of transcript lines (newest last), then [`emit_and_block`] /
/// callers slice the last `rows` so the pane shows the "bottom" like a real
/// terminal. Pure and deterministic for `(cols, rows, seed)`.
///
/// Scaling: a tall pane (`rows >= 18`) gets the full rich transcript; a medium
/// pane drops the prose preamble; a short pane (`rows < 12`) shows essentially
/// just a table + footer. Every result is fitted to `cols` and is exactly `rows`
/// lines tall, so the pane reads as *full* at any size.
pub fn ipsum(cols: usize, rows: usize, seed: u64) -> Vec<String> {
    let mut r = Rng::new(seed);
    let cols = cols.max(20);
    let rows = rows.max(3);
    let mut pool: Vec<String> = Vec::new();

    let rich = rows >= 18;
    let medium = rows >= 12;

    // 1) prose preamble (only when there's room to spare)
    if rich {
        for _ in 0..r.range(2, 3) {
            pool.push(sgr("", "", &cap(r.pick(TASKS))));
            pool.push(sgr(
                "",
                "",
                &format!(
                    "I'll {} {} — {}.",
                    r.pick(VERBS),
                    r.pick(NOUNS),
                    r.pick(TASKS)
                ),
            ));
        }
        pool.push(String::new());
    }

    // 2) a tool-call block: a Bash header + a long pipeline + a couple result rows
    if medium {
        let cmd = format!(
            "cargo build --release --manifest-path app/Cargo.toml 2>&1 | tail -{}",
            r.range(3, 9)
        );
        pool.push(format!(
            "{} {}",
            sgr(GREEN, BOLD, "●"),
            sgr(BGREEN, "", &format!("Bash({cmd})"))
        ));
        pool.push(format!(
            "  {} {}",
            sgr(GREY, "", "⎿"),
            sgr(DIM, "", "Compiling terminal-delight v0.1.0")
        ));
        pool.push(format!(
            "    {}",
            sgr(
                GREEN,
                "",
                &format!(
                    "Finished `release` profile in {}.{:02}s",
                    r.range(2, 9),
                    r.range(10, 99)
                )
            )
        ));
        pool.push(format!("    {}", sgr(BGREEN, BOLD, "BOOTED OK")));
        pool.push(String::new());
    }

    // 3) status bullets — the "all in and landed" beats
    let bullets = if rich { 5 } else { 3 };
    for _ in 0..bullets {
        let f = r.pick(FILES);
        pool.push(format!(
            "{} {} {} {}",
            sgr(GREEN, "", "●"),
            sgr("", BOLD, &cap(r.pick(VERBS))),
            sgr(CYAN, "", f),
            sgr(DIM, "", r.pick(TASKS))
        ));
    }
    pool.push(String::new());

    // 4) a box-drawn results table (the showpiece — always present)
    pool.extend(table(&mut r, cols));
    pool.push(String::new());

    // 5) recap footer + a fresh prompt + the shortcuts bar (bottom-anchored)
    pool.push(sgr(
        GREEN,
        "",
        &format!("✳ Cooked for {}m {}s", r.range(1, 12), r.range(0, 59)),
    ));
    pool.push(sgr(
        DIM,
        "",
        &format!(
            "✳ recap: {} {}; {} next.",
            r.pick(VERBS),
            r.pick(NOUNS),
            r.pick(TASKS)
        ),
    ));
    pool.push(String::new());
    pool.push(sgr(BGREEN, BOLD, "❯ "));
    pool.push(sgr(GREY, "", "  ? for shortcuts · ⏵⏵ for agents"));

    // Fit every line to the column width.
    for l in pool.iter_mut() {
        *l = fit(l, cols);
    }

    // Make the pane read as exactly `rows` tall: pad at the TOP (older history
    // scrolled off) or take the last `rows` lines if we overflowed.
    if pool.len() < rows {
        let pad = rows - pool.len();
        let mut padded = vec![String::new(); pad];
        padded.extend(pool);
        padded
    } else {
        pool[pool.len() - rows..].to_vec()
    }
}

/// A box-drawn two-column table ("Links"-style), fitted to `cols`.
fn table(r: &mut Rng, cols: usize) -> Vec<String> {
    // two columns; left ~40%, capped so even a wide pane stays tidy
    let inner = cols.saturating_sub(7).clamp(24, 96);
    let lw = (inner * 2 / 5).clamp(10, 40);
    let rw = inner.saturating_sub(lw).max(8);
    let top = format!("┌{}┬{}┐", "─".repeat(lw + 1), "─".repeat(rw + 1));
    let sep = format!("├{}┼{}┤", "─".repeat(lw + 1), "─".repeat(rw + 1));
    let bot = format!("└{}┴{}┘", "─".repeat(lw + 1), "─".repeat(rw + 1));
    let row = |l: &str, rr: &str| {
        format!(
            "│ {:<lw$}│ {:<rw$}│",
            clip(l, lw - 1),
            clip(rr, rw - 1),
            lw = lw,
            rw = rw
        )
    };
    let mut out = vec![sgr(GREEN, "", &top)];
    out.push(sgr("", BOLD, &row("Target", "What it is")));
    out.push(sgr(GREEN, "", &sep));
    let n = r.range(2, 4);
    for _ in 0..n {
        let target = format!(
            "{}.{}",
            r.pick(MODULES),
            r.pick(&["rs", "wgsl", "toml", "md"])
        );
        let what = format!("{} {}", r.pick(VERBS), r.pick(NOUNS));
        out.push(row(&target, &cap(&what)));
    }
    out.push(sgr(GREEN, "", &bot));
    out
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n.saturating_sub(1)).collect::<String>() + "…"
    }
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipsum_exactly_fills_the_pane_at_any_size() {
        for &(c, rrows) in &[(120usize, 40usize), (80, 24), (60, 12), (40, 8), (24, 4)] {
            let lines = ipsum(c, rrows, 7);
            assert_eq!(lines.len(), rrows, "{c}x{rrows}: must be exactly rows tall");
            for l in &lines {
                assert!(
                    visible_width(l) <= c,
                    "{c}x{rrows}: line wider than cols: {:?} (w={})",
                    l,
                    visible_width(l)
                );
            }
        }
    }

    #[test]
    fn ipsum_is_deterministic_per_seed_but_varies_across_seeds() {
        assert_eq!(
            ipsum(80, 24, 1),
            ipsum(80, 24, 1),
            "same seed ⇒ same content"
        );
        assert_ne!(
            ipsum(80, 24, 1),
            ipsum(80, 24, 2),
            "different seeds ⇒ different content (a wall looks alive)"
        );
    }

    #[test]
    fn ipsum_carries_no_real_paths_or_home() {
        // The whole point of the SHARE demo: nothing real on screen.
        let blob = ipsum(100, 30, 42).join("\n");
        assert!(!blob.contains("/home/"), "no real home path");
        assert!(!blob.contains("pbrown"), "no real username");
    }

    #[test]
    fn small_panes_still_show_the_table_and_footer() {
        // a short pane drops prose but keeps the showpiece table + shortcuts bar
        let lines = ipsum(70, 11, 3);
        let blob = lines.join("\n");
        assert!(blob.contains("┐") || blob.contains("│"), "table present");
        assert!(blob.contains("shortcuts"), "shortcuts bar present");
    }

    #[test]
    fn visible_width_ignores_ansi() {
        assert_eq!(visible_width(&sgr(GREEN, BOLD, "hello")), 5);
        assert_eq!(visible_width("plain"), 5);
    }
}
