//! TerminalView — one pane: a real shell with themed rendering, selection,
//! scrollback, clipboard, CRT-lite effects, and the TD_LATENCY probe.

use std::cell::RefCell;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::crt;
use crate::term;
use crate::theme::{self, PaneTheme, Theme};
use alacritty_terminal::{
    event::{Event as TermEvent, Notify},
    grid::{Dimensions, Scroll},
    index::{Column, Line, Point as TermPoint, Side},
    selection::{Selection, SelectionType},
    term::{cell::Flags, viewport_to_point, TermMode},
    vte::ansi::{Color as AnsiColor, NamedColor},
};
use futures::StreamExt;
use gpui::{
    anchored, canvas, deferred, div, font, linear_color_stop, linear_gradient, point, prelude::*,
    px, rgb, App, Bounds, BoxShadow, ClipboardItem, Context, FocusHandle, Focusable, Font,
    FontStyle, FontWeight, Hsla, KeyDownEvent, Keystroke, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, ScrollWheelEvent, StyledText, TextRun, UnderlineStyle,
    Window,
};

/// What the tube is showing — drives the per-pane screen colour.
/// Recommended phosphor quartet: green=shell, amber=claude, ice-cyan=codex,
/// violet=remote (you are not local).
#[derive(Clone, PartialEq, Debug)]
pub enum PaneMode {
    Shell,
    Claude,
    Codex,
    Remote,
    Other(String),
}

impl PaneMode {
    fn classify(comm: &str, cmdline: &str) -> PaneMode {
        let c = comm.trim();
        if c == "claude" || cmdline.contains("/claude") {
            PaneMode::Claude
        } else if c == "codex" || cmdline.contains("/codex") {
            PaneMode::Codex
        } else if matches!(c, "ssh" | "mosh-client" | "et" | "telnet") {
            PaneMode::Remote
        } else if matches!(c, "bash" | "zsh" | "fish" | "sh" | "dash" | "nu") {
            PaneMode::Shell
        } else {
            PaneMode::Other(c.to_string())
        }
    }

    pub fn label(&self) -> &str {
        match self {
            PaneMode::Shell => "SHELL",
            PaneMode::Claude => "CLAUDE",
            PaneMode::Codex => "CODEX",
            PaneMode::Remote => "REMOTE",
            PaneMode::Other(name) => name,
        }
    }

    /// Localised header label for the active UI language. SHELL and REMOTE
    /// translate; CLAUDE / CODEX stay (proper nouns) and `Other` keeps the live
    /// program name. The plain `label()` stays English for MCP/data callers.
    pub fn label_i18n(&self) -> std::borrow::Cow<'static, str> {
        use std::borrow::Cow;
        let st = crate::lang::current().strings();
        match self {
            PaneMode::Shell => Cow::Borrowed(st.ph_shell),
            PaneMode::Remote => Cow::Borrowed(st.ph_remote),
            PaneMode::Claude => Cow::Borrowed("CLAUDE"),
            PaneMode::Codex => Cow::Borrowed("CODEX"),
            PaneMode::Other(name) => Cow::Owned(name.clone()),
        }
    }

    /// True when this pane is running a conversational agent (Claude or Codex) —
    /// the modes where "your own input" is a meaningful, navigable, colourable
    /// thing distinct from the agent's replies.
    pub fn is_agent(&self) -> bool {
        matches!(self, PaneMode::Claude | PaneMode::Codex)
    }
}

/// Does this rendered grid row look like one of *the user's own* input lines in
/// an agent (claude/codex) TUI? Heuristic: agent TUIs echo the human's submitted
/// turn behind a prompt caret — `❯ `/`> ` (Claude Code) or `▌ ` (some Codex
/// builds). We match the first non-blank glyph so indentation/box-drawing around
/// the prompt doesn't fool it. Pure + cheap so it's unit-testable and runs per
/// row per paint only while a pane is in agent mode.
pub fn is_human_input_line(text: &str) -> bool {
    let mut chars = text.trim_start().chars();
    match chars.next() {
        // The prompt caret glyphs agent CLIs use for the human's turn.
        Some('❯') | Some('▌') | Some('»') => {
            // Require a space (or end) after the caret so we don't catch e.g. a
            // `❯`-decorated banner with no following text.
            matches!(chars.next(), Some(' ') | None)
        }
        // Plain ASCII '>' is also a quote/redirect marker, so require "> " AND
        // that what follows isn't another '>' (avoids `>>` heredocs / git diffs).
        Some('>') => matches!(chars.next(), Some(' ')) && chars.next() != Some('>'),
        _ => false,
    }
}

/// Mark which rows belong to *the user's own turn*, spanning a wrapped multi-line
/// message — not just the caret row. An agent TUI prints the human's turn behind
/// a prompt caret (see `is_human_input_line`) and indents any wrapped
/// continuation rows under that text. So once a caret row opens a turn we keep
/// marking the rows that follow as long as they read as indented continuation
/// (lead with whitespace and carry real text); a blank row, a left-margin row
/// (the agent's reply / a status line), or a fresh caret row closes the turn.
/// This is what colours the *entire* message in `th.human`, not just its first
/// line. Pure + cheap so it's unit-testable and runs once per paint in agent mode.
pub fn human_input_rows(rows: &[String]) -> Vec<bool> {
    let mut marks = vec![false; rows.len()];
    let mut in_turn = false;
    for (i, text) in rows.iter().enumerate() {
        if is_human_input_line(text) {
            in_turn = true; // a caret row opens (or continues) the turn
        } else if in_turn {
            // Stay in the turn only for indented, non-blank continuation rows;
            // a blank or column-0 row hands the screen back to the agent.
            in_turn = !text.trim_end().is_empty() && text.starts_with(' ');
        }
        marks[i] = in_turn;
    }
    marks
}

/// Foreground process of the PTY, the honest kernel answer.
fn foreground_mode(master: &std::fs::File, shell_pid: u32) -> PaneMode {
    use std::os::fd::AsRawFd;
    let pgid = unsafe { libc::tcgetpgrp(master.as_raw_fd()) };
    if pgid <= 0 {
        return PaneMode::Shell;
    }
    let comm = std::fs::read_to_string(format!("/proc/{pgid}/comm")).unwrap_or_default();
    let cmdline = std::fs::read_to_string(format!("/proc/{pgid}/cmdline"))
        .unwrap_or_default()
        .replace('\0', " ");
    if pgid as u32 == shell_pid {
        return PaneMode::Shell;
    }
    PaneMode::classify(&comm, &cmdline)
}

/// The consistent header icon size (≈2× the old glyphs).
pub const HICON: f32 = 28.0;

/// A small EQ-waveform glyph — a row of bars at varying heights — used as the
/// consistent monitor/display icon. Drawn (not an emoji) so it can be wider than
/// a square and read as "the screen / levels" control.
pub fn eq_icon(accent: gpui::Hsla, scale: f32) -> gpui::Div {
    use gpui::{div, px};
    let bars = [8.0f32, 17.0, 12.0, 22.0, 14.0, 19.0, 9.0];
    let mut row = div()
        .flex()
        .flex_row()
        .items_end()
        .gap(px(2. * scale))
        .h(px(HICON * scale));
    for h in bars {
        row = row.child(
            div()
                .w(px(3. * scale))
                .h(px(h * scale))
                .rounded_sm()
                .bg(accent),
        );
    }
    row
}

/// A small line-art retro robot — a dish antenna, a boxy head with two round
/// eyes and a mouth slit. Drawn from divs (deliberately NOT the 🤖 emoji) so it
/// inherits the accent colour and scales crisply with the menu bar. Marks the
/// read-only MCP "watch the agents" control on the mother bar.
pub fn robot_icon(accent: gpui::Hsla, scale: f32) -> gpui::Div {
    use gpui::{div, px};
    let s = scale;
    let eye = || {
        div()
            .w(px(3.5 * s))
            .h(px(3.5 * s))
            .rounded_full()
            .bg(accent)
    };
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(1.5 * s))
        .h(px(HICON * s))
        .child(
            // antenna: a dot on a short stem
            div()
                .flex()
                .flex_col()
                .items_center()
                .child(
                    div()
                        .w(px(3.5 * s))
                        .h(px(3.5 * s))
                        .rounded_full()
                        .bg(accent),
                )
                .child(div().w(px(1.5 * s)).h(px(3. * s)).bg(accent.alpha(0.8))),
        )
        .child(
            // head: rounded outline with two eyes over a mouth slit
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(2. * s))
                .w(px(20. * s))
                .h(px(15. * s))
                .rounded_md()
                .border_1()
                .border_color(accent)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(4. * s))
                        .child(eye())
                        .child(eye()),
                )
                .child(
                    div()
                        .w(px(9. * s))
                        .h(px(1.6 * s))
                        .rounded_sm()
                        .bg(accent.alpha(0.85)),
                ),
        )
}

/// New value for a dragged bell-trim pip: move start or end to time `t`, keeping
/// a 0.2s gap and staying within [0, dur]. `end <= start` means "to the clip
/// end", so the effective end is `dur`. Pure so the drag math is unit-testable.
fn trim_drag_value(is_end: bool, t: f32, start: f32, end: f32, dur: f32) -> f32 {
    let eff_end = if end > start { end } else { dur };
    if is_end {
        t.clamp(start + 0.2, dur.max(start + 0.2))
    } else {
        t.clamp(0.0, (eff_end - 0.2).max(0.0))
    }
}

/// A shift-clickable target lifted out of the grid: a web/file URL handed
/// straight to the system opener, or a filesystem path resolved against the
/// pane's cwd before opening.
#[derive(Debug, PartialEq)]
enum Link {
    Url(String),
    Path(String),
}

/// Peel wrapping brackets/quotes and trailing sentence punctuation off a token
/// so `(https://x.com),` clicks as `https://x.com`.
fn trim_link_delims(s: &str) -> String {
    let mut s = s.trim().to_string();
    loop {
        let before = s.clone();
        // a fully-wrapping pair: ( … ), " … ", etc.
        let ch: Vec<char> = s.chars().collect();
        if ch.len() >= 2
            && matches!(
                (ch[0], ch[ch.len() - 1]),
                ('(', ')') | ('[', ']') | ('{', '}') | ('<', '>') | ('"', '"') | ('\'', '\'')
            )
        {
            s = ch[1..ch.len() - 1].iter().collect();
        }
        // trailing sentence punctuation
        while matches!(s.chars().last(), Some('.' | ',' | ';' | ':' | '!' | '?')) {
            s.pop();
        }
        // a stray closing bracket with no opener left inside (e.g. "x)" once the
        // comma is gone) — but keep balanced ones like a wikipedia "(foo)" URL
        while let Some(c) = s.chars().last() {
            let opener = match c {
                ')' => '(',
                ']' => '[',
                '}' => '{',
                '>' => '<',
                _ => break,
            };
            if s.contains(opener) {
                break;
            }
            s.pop();
        }
        if s == before {
            break;
        }
    }
    s
}

/// Collapse `.`/`..` segments lexically (no filesystem touch) so a joined
/// relative link is a clean absolute path.
fn lexical_normalize(path: &str) -> String {
    use std::path::{Component, PathBuf};
    let mut out = PathBuf::new();
    for comp in std::path::Path::new(path).components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            c => out.push(c.as_os_str()),
        }
    }
    out.to_string_lossy().into_owned()
}

/// The link sitting under column `col` of a row of terminal text, if any. Pure:
/// expands the whitespace-delimited token, trims delimiters, then classifies it
/// as a URL (known scheme or `www.`) or a filesystem path (`/`, `~/`, `./`, `..`).
fn link_at(line: &str, col: usize) -> Option<Link> {
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return None;
    }
    let col = col.min(chars.len() - 1);
    if chars[col].is_whitespace() {
        return None;
    }
    let mut start = col;
    while start > 0 && !chars[start - 1].is_whitespace() {
        start -= 1;
    }
    let mut end = col;
    while end + 1 < chars.len() && !chars[end + 1].is_whitespace() {
        end += 1;
    }
    let tok = trim_link_delims(&chars[start..=end].iter().collect::<String>());
    if tok.is_empty() {
        return None;
    }
    let lower = tok.to_ascii_lowercase();
    const SCHEMES: &[&str] = &[
        "http://", "https://", "file://", "ftp://", "ftps://", "mailto:",
    ];
    if SCHEMES.iter().any(|s| lower.starts_with(s)) {
        return Some(Link::Url(tok));
    }
    if let Some(rest) = lower.strip_prefix("www.") {
        if rest.contains('.') {
            return Some(Link::Url(format!("https://{tok}")));
        }
    }
    if tok.starts_with('/')
        || tok.starts_with("~/")
        || tok.starts_with("./")
        || tok.starts_with("../")
    {
        return Some(Link::Path(tok));
    }
    None
}

/// Stitch a click on a soft-wrapped row back into its full logical line. A
/// terminal wraps a long URL/path mid-token with no space and marks the last
/// cell of each wrapped row with `WRAPLINE`; `wraps[r]` carries that flag. We
/// walk up while the row above wraps into us and down while we keep wrapping,
/// concatenate those rows, and return the stitched line together with the
/// absolute column of the original click within it — so `link_at` sees the whole
/// token instead of a truncated fragment. Pure: testable without a live grid.
fn stitch_wrapped_line(
    rows: &[Vec<char>],
    wraps: &[bool],
    vrow: usize,
    vcol: usize,
) -> (String, usize) {
    if rows.is_empty() {
        return (String::new(), vcol);
    }
    let vrow = vrow.min(rows.len() - 1);
    // first row of the logical line: walk up while the row above wraps into us
    let mut top = vrow;
    while top > 0 && wraps[top - 1] {
        top -= 1;
    }
    // last row: walk down while the current row wraps into the next
    let mut bot = vrow;
    while bot + 1 < rows.len() && wraps[bot] {
        bot += 1;
    }
    let mut line = String::new();
    for row in &rows[top..=bot] {
        line.extend(row.iter());
    }
    // click column within the stitched line = chars in the rows above it + vcol
    let offset: usize = rows[top..vrow].iter().map(|r| r.len()).sum();
    (line, offset + vcol)
}

/// Turn a path link into an absolute path: expand a leading `~`, and join a
/// relative `./`/`../` onto the pane's cwd. Returns None if it can't be anchored.
fn resolve_path(p: &str, cwd: Option<&str>) -> Option<String> {
    let expanded = if p == "~" {
        std::env::var("HOME").ok()?
    } else if let Some(rest) = p.strip_prefix("~/") {
        format!("{}/{}", std::env::var("HOME").ok()?, rest)
    } else {
        p.to_string()
    };
    let full = if expanded.starts_with('/') {
        expanded
    } else {
        let base = cwd?;
        std::path::Path::new(base)
            .join(&expanded)
            .to_string_lossy()
            .into_owned()
    };
    Some(lexical_normalize(&full))
}

/// Hand a URL/path to the system default tool (`xdg-open`), detached so it
/// outlives the click and never blocks the UI.
fn open_with_system(target: &str) {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let mut cmd = Command::new("xdg-open");
    cmd.arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let _ = cmd.spawn();
}

/// Screen→content barrel map — identical to the per-rect warp in
/// `gpui_wgpu/src/crt_pass.wgsl` (`fs_crt`): the content displayed at a
/// rect-local screen point `(sx, sy)` ∈ [0,1]² is sampled from
/// `0.5 + (s − 0.5)·f`, with `f = 1 + k1·r² + k2·r⁴` and `r²` in that same
/// rect-local space. The shader is a *gather*, so hit-testing applies the SAME
/// forward map (no inverse) to land a click on the exact cell shown under it.
/// `f == 1` when curvature is zero, so this is the identity for a flat pane.
pub(crate) fn warp_screen_to_content(sx: f32, sy: f32, k1: f32, k2: f32) -> (f32, f32) {
    let cu = sx - 0.5;
    let cv = sy - 0.5;
    let r2 = cu * cu + cv * cv;
    let f = 1.0 + k1 * r2 + k2 * r2 * r2;
    (0.5 + cu * f, 0.5 + cv * f)
}

/// Apply the mode's screen colour over the structural theme.
fn mode_theme(base: &Theme, mode: &PaneMode) -> Theme {
    let mut th = base.clone();
    let (accent, text, faint, cursor) = match mode {
        PaneMode::Shell | PaneMode::Other(_) => return th,
        // amber phosphor — Claude (P3 tube, Anthropic-warm)
        PaneMode::Claude => (0xf59e0bu32, 0xfbe3b0u32, 0x4a3410u32, 0xfbbf24u32),
        // ice cyan — Codex
        PaneMode::Codex => (0x22d3eeu32, 0xc3f4fcu32, 0x0e3a44u32, 0x67e8f9u32),
        // violet — Remote: you are NOT local
        PaneMode::Remote => (0xc084fcu32, 0xead9fcu32, 0x3b2354u32, 0xd8b4feu32),
    };
    let acc: gpui::Hsla = gpui::rgb(accent).into();
    th.accent = acc;
    th.text = gpui::rgb(text).into();
    th.faint = gpui::rgb(faint).into();
    th.cursor = gpui::rgb(cursor).into();
    // tint the tube's depths toward the mode hue
    let mut bg = acc;
    bg.s = 0.35;
    bg.l = 0.035;
    th.bg = bg;
    let mut surface = acc;
    surface.s = 0.32;
    surface.l = 0.07;
    th.surface = surface;
    // default fg/ANSI-7-ish stays app-controlled; swap the green slots' default fg
    th.ansi[7] = th.text;
    th
}

const HEADER_H: f32 = 40.0;
/// The smallest breathing border the grid ever keeps off any edge (px). On a
/// flat or tiny pane the 2% term falls below this and the floor takes over.
const PAD_MIN: f32 = 4.0;

/// Padding (px) that frames the terminal grid inside its tube, returned as
/// `(pad_x, pad_y)` for the left/right and top/bottom insets. Two terms add up:
///
/// 1. **Breathing border** — `max(PAD_MIN, 2%)` of the axis, so text never hugs
///    the glass and the border scales with the pane instead of being a fixed
///    sliver that looks cramped on a large pane.
/// 2. **Barrel-warp overscan** — the CRT warp is a framebuffer gather
///    (`fs_crt`, mirrored by [`warp_screen_to_content`]): each edge pixel samples
///    content from `0.5 + (s−0.5)·f`, `f = 1 + k1·r² + k2·r⁴`, so the outer
///    `~0.5·(f−1)` band of each axis maps *past* the content and smears into an
///    overscan border. Without compensation that band eats the edge rows — and
///    the **prompt lives on the bottom row**, so it was the visible casualty
///    (see the curve-bottom-cutoff bug). We reserve that band on every side so
///    the edge rows/cols sit inside the warp's visible region. `r²≈0.25` is the
///    mid-edge; the `1.15` nudges the inset toward the harder-bowing corners.
///
/// Curvature is symmetric top/bottom, so the frame reads even all around. A flat
/// pane (`k1=k2=0`) collapses term 2 and keeps just the breathing border.
/// Used by the renderer, [`Self::sync_size`] (grid fit) and
/// [`Self::viewport_cell`] (hit-test) so all three agree on where the grid sits.
fn grid_pad(w: f32, h: f32, k1: f32, k2: f32) -> (f32, f32) {
    let over = 0.5 * (0.25 * k1 + 0.0625 * k2) * 1.15;
    (
        (w * (0.02 + over)).max(PAD_MIN),
        (h * (0.02 + over)).max(PAD_MIN),
    )
}

/// The real xterm 16-colour palette. Cells always derive from these *true*
/// colours; the active [`ColorMode`] decides how they're finally painted (see
/// [`shape`]). The theme's own `ansi` array is reserved for chrome.
const XTERM: [u32; 16] = [
    0x000000, 0xcd0000, 0x00cd00, 0xcdcd00, 0x0000ee, 0xcd00cd, 0x00cdcd, 0xe5e5e5, 0x7f7f7f,
    0xff0000, 0x00ff00, 0xffff00, 0x5c5cff, 0xff00ff, 0x00ffff, 0xffffff,
];

/// Fractional part of a hue — keeps it in `[0, 1)`.
fn wrap01(h: f32) -> f32 {
    h - h.floor()
}

/// Signed shortest distance of a hue from 0, in turns: `(-0.5, 0.5]`.
fn signed_turn(h: f32) -> f32 {
    let d = wrap01(h);
    if d > 0.5 {
        d - 1.0
    } else {
        d
    }
}

/// The colour-shape algorithm: map a *real* terminal colour through the pane's
/// active [`ColorMode`].
///
/// - `Default` — untouched, the honest xterm palette.
/// - `Monochrome` — collapse onto the theme's phosphor: adopt the text hue and
///   saturation, keep the source lightness so structure (bold/bright) survives.
/// - `OnTheme` — fold the whole ANSI hue wheel onto a harmonic arc centred on
///   the seed accent. The classic terminal green lands *on* the seed; warm hues
///   fan one way, cool hues the other, so the program's colour *structure* is
///   preserved while the palette becomes one coherent family. Greys stay grey.
fn shape(c: Hsla, th: &Theme) -> Hsla {
    use crate::theme::ColorMode;
    match th.color_mode {
        ColorMode::Default => c,
        ColorMode::Monochrome => Hsla {
            h: th.text.h,
            s: th.text.s,
            l: c.l,
            a: c.a,
        },
        ColorMode::OnTheme => {
            // ±~99° fan around the seed; greens are the anchor so a stock
            // terminal's prompt-green becomes the seed colour itself.
            const ARC: f32 = 0.55;
            const GREEN: f32 = 1.0 / 3.0;
            if c.s < 0.08 {
                // near-grey: keep it neutral, just breathe the seed hue in
                return Hsla {
                    h: th.accent.h,
                    s: c.s,
                    l: c.l,
                    a: c.a,
                };
            }
            let d = signed_turn(c.h - GREEN);
            Hsla {
                h: wrap01(th.accent.h + d * ARC),
                s: (c.s * 0.55 + th.accent.s * 0.55).clamp(0.25, 1.0),
                l: c.l,
                a: c.a,
            }
        }
    }
}

/// The real colour for an ANSI palette index (pre-[`shape`]). `<16` is the
/// xterm base; `16..232` the 6×6×6 cube; `232..` the greyscale ramp.
fn idx_color(i: u8) -> Hsla {
    if (i as usize) < 16 {
        return rgb(XTERM[i as usize]).into();
    }
    if i >= 232 {
        let v = 8 + 10 * (i - 232) as u32;
        return rgb(v << 16 | v << 8 | v).into();
    }
    let i = i - 16;
    let lv = |n: u8| -> u32 {
        if n == 0 {
            0
        } else {
            55 + 40 * n as u32
        }
    };
    let (r, g, b) = (lv(i / 36), lv((i / 6) % 6), lv(i % 6));
    rgb(r << 16 | g << 8 | b).into()
}

fn ansi_to_hsla(color: AnsiColor, th: &Theme, default: Hsla) -> Hsla {
    match color {
        AnsiColor::Named(named) => match named {
            // Unstyled text is always the theme's text colour (the wheel's `T`
            // target). The ColorMode axis governs *program-emitted* colour only,
            // not default-fg — so `T` reads in every mode (ansi/mono/theme),
            // resolving the old collision where the mode picked this colour. The
            // `code`/syntax overlay layers on top of this (see the loop above).
            // bg + cursor stay structural so the UI never loses contrast.
            NamedColor::Foreground => th.text,
            NamedColor::Background => th.bg,
            NamedColor::Cursor => th.cursor,
            n => {
                let i = n as usize;
                if i < 16 {
                    shape(rgb(XTERM[i]).into(), th)
                } else {
                    default
                }
            }
        },
        AnsiColor::Spec(c) => shape(
            rgb((c.r as u32) << 16 | (c.g as u32) << 8 | c.b as u32).into(),
            th,
        ),
        AnsiColor::Indexed(i) => shape(idx_color(i), th),
    }
}

/// A short set of words worth popping in the accent (shell verbs + common
/// language keywords). Kept small on purpose — generic highlighting, not a
/// per-language grammar.
fn is_keyword(w: &str) -> bool {
    matches!(
        w,
        "fn" | "let"
            | "mut"
            | "pub"
            | "use"
            | "mod"
            | "impl"
            | "struct"
            | "enum"
            | "trait"
            | "match"
            | "if"
            | "else"
            | "for"
            | "while"
            | "loop"
            | "return"
            | "const"
            | "async"
            | "await"
            | "move"
            | "true"
            | "false"
            | "null"
            | "nil"
            | "None"
            | "Some"
            | "Ok"
            | "Err"
            | "self"
            | "import"
            | "from"
            | "def"
            | "class"
            | "function"
            | "var"
            | "echo"
            | "cd"
            | "ls"
            | "git"
            | "cargo"
            | "sudo"
            | "export"
            | "rm"
            | "cp"
            | "mv"
            | "grep"
            | "cat"
            | "sed"
            | "awk"
            | "make"
    )
}

/// Token classes the generic highlighter recognises. `Word` is the default
/// (rendered in the theme's plain foreground); the rest each get a hue.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tok {
    Word,
    Num,
    Str,
    Path,
    Flag,
    Op,
    Punct,
    Comment,
    Keyword,
}

/// Tokenise one line, returning a class per `char` (1:1 with `line.chars()`).
/// Pure and theme-free, so it's unit-testable on its own.
fn classify_line(line: &str) -> Vec<Tok> {
    let ch: Vec<char> = line.chars().collect();
    let n = ch.len();
    let mut out = vec![Tok::Word; n];
    let paint = |out: &mut [Tok], a: usize, b: usize, t: Tok| {
        out[a..b].iter_mut().for_each(|p| *p = t);
    };
    let boundary = |i: usize| i == 0 || ch[i - 1].is_whitespace();

    let mut i = 0;
    while i < n {
        let c = ch[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // shell-style comment: # to end of line, only at a token boundary
        if c == '#' && boundary(i) {
            paint(&mut out, i, n, Tok::Comment);
            break;
        }
        // quoted string (single/double/back), runs to the matching quote
        if c == '"' || c == '\'' || c == '`' {
            let mut j = i + 1;
            while j < n && ch[j] != c {
                j += 1;
            }
            j = (j + 1).min(n); // include the closing quote if present
            paint(&mut out, i, j, Tok::Str);
            i = j;
            continue;
        }
        // flag: -x or --long, at a token boundary
        if c == '-'
            && boundary(i)
            && i + 1 < n
            && (ch[i + 1].is_ascii_alphabetic() || ch[i + 1] == '-')
        {
            let mut j = i;
            while j < n && !ch[j].is_whitespace() {
                j += 1;
            }
            paint(&mut out, i, j, Tok::Flag);
            i = j;
            continue;
        }
        // number: a digit-led run (handles 1, 1.5, 0xff, 12px, 3:14)
        if c.is_ascii_digit() {
            let mut j = i;
            while j < n
                && (ch[j].is_ascii_alphanumeric() || matches!(ch[j], '.' | ':' | '_' | 'x' | 'X'))
            {
                j += 1;
            }
            paint(&mut out, i, j, Tok::Num);
            i = j;
            continue;
        }
        // standalone operators / brackets
        if "=+|&;<>!*%^~".contains(c) {
            out[i] = Tok::Op;
            i += 1;
            continue;
        }
        if "()[]{},:.".contains(c) {
            out[i] = Tok::Punct;
            i += 1;
            continue;
        }
        // otherwise a word/path run: chars that hang together in a token
        let start = i;
        let mut j = i;
        while j < n && (ch[j].is_alphanumeric() || matches!(ch[j], '_' | '/' | '.' | '-' | '@')) {
            j += 1;
        }
        if j == start {
            i += 1; // unclassified single char — leave as Word, advance
            continue;
        }
        let word: String = ch[start..j].iter().collect();
        if word.contains('/') || word.starts_with('~') {
            paint(&mut out, start, j, Tok::Path);
        } else if is_keyword(word.trim_matches(|c: char| !c.is_alphanumeric())) {
            paint(&mut out, start, j, Tok::Keyword);
        }
        i = j;
    }
    out
}

/// Per-character foreground colours for one line under the `syntax` overlay:
/// classify the raw text, then paint each token class its own hue on the seed
/// arc. Returns one `Hsla` per `char` in `line` (so it maps 1:1 onto the row's
/// cells). The renderer only applies these to cells the program left at default
/// fg — cells with explicit ANSI colour still flow through [`ansi_to_hsla`].
fn syntax_colors(line: &str, th: &Theme) -> Vec<Hsla> {
    use crate::theme::SyntaxScheme;
    let roles: Vec<Role> = match th.syntax_scheme {
        SyntaxScheme::Code => classify_line(line).into_iter().map(tok_to_role).collect(),
        SyntaxScheme::Agentic => classify_agentic(line),
        SyntaxScheme::Logs => classify_logs(line),
        SyntaxScheme::Markdown => classify_markdown(line),
    };
    roles.into_iter().map(|r| role_color(r, th)).collect()
}

/// The shared 6-slot palette every syntax SCHEME maps its grammar into, so all
/// schemes are coloured identically by PROGRAM COLOUR (see [`role_color`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Role {
    Text,       // body / unclassified
    Primary,    // loudest accent — keyword · callout · error · heading
    Secondary,  // string · link/reference · ok/pass · bold
    Tertiary,   // number · tool-call · warn · code-span
    Quaternary, // path · structure/title · timestamp · italic · url
    Muted,      // operators/punct/flags · list markers · debug · quotes
    Comment,    // comments / faint asides
}

/// Paint `out[a..b]` (char-indexed) with role `r`, clamped to bounds.
fn paint_roles(out: &mut [Role], a: usize, b: usize, r: Role) {
    let b = b.min(out.len());
    if a < b {
        out[a..b].iter_mut().for_each(|p| *p = r);
    }
}

/// `code`-scheme token → palette role.
fn tok_to_role(t: Tok) -> Role {
    match t {
        Tok::Word => Role::Text,
        Tok::Keyword => Role::Primary,
        Tok::Str => Role::Secondary,
        Tok::Num => Role::Tertiary,
        Tok::Path => Role::Quaternary,
        Tok::Flag | Tok::Op | Tok::Punct => Role::Muted,
        Tok::Comment => Role::Comment,
    }
}

/// Colour for one role, derived from the pane's PROGRAM COLOUR mode so the two
/// controls compose: `ansi` = vivid full-spectrum (a distinct hue per role on
/// the seed arc); `mono` = shades of the text phosphor (structure, no colour);
/// `theme` = the main roles in the ACTUAL selected palette, rest derived.
fn role_color(role: Role, th: &Theme) -> Hsla {
    use crate::theme::ColorMode;
    if role == Role::Text {
        return th.text;
    }
    if role == Role::Comment {
        return Hsla { a: 0.7, ..th.faint };
    }
    match th.color_mode {
        ColorMode::Default => {
            let dark = th.bg.l < 0.5;
            let l = if dark { 0.72 } else { 0.40 };
            let hue = |off: f32, a: f32| Hsla {
                h: wrap01(th.accent.h + off),
                s: th.accent.s.clamp(0.45, 0.95),
                l,
                a,
            };
            match role {
                Role::Primary => th.accent,
                Role::Secondary => hue(0.17, 1.0),
                Role::Tertiary => hue(0.09, 1.0),
                Role::Quaternary => hue(-0.09, 1.0),
                Role::Muted => hue(0.28, 0.80),
                _ => th.text,
            }
        }
        ColorMode::Monochrome => {
            let base = th.text;
            let shade = |dl: f32, a: f32| Hsla {
                h: base.h,
                s: base.s,
                l: (base.l + dl).clamp(0.05, 0.97),
                a,
            };
            match role {
                Role::Primary => shade(0.14, 1.0),
                Role::Secondary => shade(0.07, 1.0),
                Role::Tertiary => shade(0.04, 1.0),
                Role::Quaternary => shade(-0.05, 0.95),
                Role::Muted => shade(-0.12, 0.78),
                _ => base,
            }
        }
        ColorMode::OnTheme => {
            let nudge = |from: Hsla, off: f32| Hsla {
                h: wrap01(from.h + off),
                s: from.s,
                l: from.l,
                a: 1.0,
            };
            match role {
                Role::Primary => th.accent,
                Role::Secondary => th.complement,
                Role::Tertiary => th.human,
                Role::Quaternary => nudge(th.accent, 0.05),
                Role::Muted => th.faint,
                _ => th.text,
            }
        }
    }
}

/// First non-whitespace char index, or `n` if the line is blank.
fn lead_idx(ch: &[char]) -> usize {
    ch.iter()
        .position(|c| !c.is_whitespace())
        .unwrap_or(ch.len())
}

/// AGENTIC scheme — agent-watch markers: callouts, tool calls, links/files,
/// structure/titles, list & step markers. Heuristic + line-oriented.
fn classify_agentic(line: &str) -> Vec<Role> {
    let ch: Vec<char> = line.chars().collect();
    let n = ch.len();
    let mut out = vec![Role::Text; n];
    let lead = lead_idx(&ch);
    if lead == n {
        return out;
    }
    // structure: heading run of '#'
    if ch[lead] == '#' {
        paint_roles(&mut out, lead, n, Role::Quaternary);
        return out;
    }
    // structure: a table/separator rule (only box-drawing / dashes / pipes)
    let only_rule = ch
        .iter()
        .all(|c| matches!(c, '|' | '-' | '+' | '=' | '─' | '│' | '┼' | '╶' | ' '));
    if n >= 4 && only_rule && ch.iter().any(|c| !c.is_whitespace()) {
        paint_roles(&mut out, 0, n, Role::Quaternary);
        return out;
    }
    // inline: links / paths (Secondary) · tool-call Name( (Tertiary) · ALL-CAPS (Quaternary)
    let mut i = 0;
    while i < n {
        if ch[i].is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        while i < n && !ch[i].is_whitespace() {
            i += 1;
        }
        let tok: String = ch[start..i].iter().collect();
        let lower = tok.to_ascii_lowercase();
        if lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("file://")
            || lower.starts_with("www.")
            || (tok.contains('/') && tok.len() > 2 && !tok.ends_with(':'))
        {
            paint_roles(&mut out, start, i, Role::Secondary); // link / file reference
        } else if let Some(p) = tok.chars().position(|c| c == '(') {
            let name = &tok[..tok
                .char_indices()
                .nth(p)
                .map(|(b, _)| b)
                .unwrap_or(tok.len())];
            if name.len() >= 2
                && name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                paint_roles(&mut out, start, start + p, Role::Tertiary); // tool call
            }
        } else if tok.chars().count() >= 2
            && tok
                .chars()
                .all(|c| c.is_ascii_uppercase() || matches!(c, '_' | '-'))
            && tok.chars().any(|c| c.is_ascii_uppercase())
        {
            paint_roles(&mut out, start, i, Role::Quaternary); // ALL-CAPS title-ish
        }
    }
    // sequence markers at the start (paint just the marker): 1. / 1) / - / * / • / phase|step|part|stage N
    if ch[lead].is_ascii_digit() {
        let mut k = lead;
        while k < n && ch[k].is_ascii_digit() {
            k += 1;
        }
        if k < n && (ch[k] == '.' || ch[k] == ')') {
            paint_roles(&mut out, lead, k + 1, Role::Muted);
        }
    } else if matches!(ch[lead], '-' | '*' | '•' | '·')
        && lead + 1 < n
        && ch[lead + 1].is_whitespace()
    {
        paint_roles(&mut out, lead, lead + 1, Role::Muted);
    }
    let rest_lower: String = ch[lead..].iter().collect::<String>().to_ascii_lowercase();
    for kw in ["phase ", "step ", "part ", "stage "] {
        if rest_lower.starts_with(kw) {
            let mut end = lead + kw.chars().count();
            while end < n && (ch[end].is_ascii_digit() || ch[end] == '.') {
                end += 1;
            }
            paint_roles(&mut out, lead, end, Role::Muted);
        }
    }
    // callout label at the start (wins on the label): KnownWord ':'
    let rest: String = ch[lead..].iter().collect();
    if let Some(colon) = rest.chars().position(|c| c == ':') {
        let word: String = rest.chars().take(colon).collect();
        const CALLOUTS: &[&str] = &[
            "recommendation",
            "recap",
            "goal",
            "note",
            "next",
            "why",
            "plan",
            "todo",
            "summary",
            "tip",
            "warning",
            "result",
            "caveat",
            "takeaway",
            "key",
            "fix",
            "action",
            "status",
            "context",
        ];
        let w = word.trim();
        if CALLOUTS.iter().any(|c| w.eq_ignore_ascii_case(c)) {
            paint_roles(&mut out, lead, lead + colon + 1, Role::Primary);
        }
    }
    out
}

/// LOGS scheme — error/warn/ok levels, timestamps, durations, paths, ✓/✗.
fn classify_logs(line: &str) -> Vec<Role> {
    let ch: Vec<char> = line.chars().collect();
    let n = ch.len();
    let mut out = vec![Role::Text; n];
    let mut i = 0;
    while i < n {
        let c = ch[i];
        if !(c.is_alphanumeric() || matches!(c, ':' | '/' | '.' | '-' | '_')) {
            i += 1;
            continue;
        }
        let start = i;
        while i < n && (ch[i].is_alphanumeric() || matches!(ch[i], ':' | '/' | '.' | '-' | '_')) {
            i += 1;
        }
        let tok: String = ch[start..i].iter().collect();
        let role = match tok.to_ascii_uppercase().as_str() {
            "ERROR" | "ERR" | "FAIL" | "FAILED" | "FATAL" | "PANIC" | "CRITICAL" => {
                Some(Role::Primary)
            }
            "WARN" | "WARNING" => Some(Role::Tertiary),
            "OK" | "PASS" | "PASSED" | "DONE" | "SUCCESS" | "READY" | "UP" => Some(Role::Secondary),
            "INFO" | "DEBUG" | "TRACE" | "NOTE" | "DEBUG:" => Some(Role::Muted),
            _ => None,
        };
        if let Some(r) = role {
            paint_roles(&mut out, start, i, r);
        } else if tok.contains(':')
            && tok.chars().any(|c| c.is_ascii_digit())
            && tok
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, ':' | '.' | '-' | 'T' | 'Z'))
        {
            paint_roles(&mut out, start, i, Role::Muted); // timestamp
        } else if tok.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            paint_roles(&mut out, start, i, Role::Tertiary); // number / duration
        } else if tok.contains('/') && tok.len() > 2 {
            paint_roles(&mut out, start, i, Role::Quaternary); // path
        }
    }
    for (idx, c) in ch.iter().enumerate() {
        match c {
            '✓' | '✔' => out[idx] = Role::Secondary,
            '✗' | '✘' | '×' => out[idx] = Role::Primary,
            _ => {}
        }
    }
    out
}

/// MARKDOWN scheme — headings, bold/italic, code spans, links, quotes, lists.
fn classify_markdown(line: &str) -> Vec<Role> {
    let ch: Vec<char> = line.chars().collect();
    let n = ch.len();
    let mut out = vec![Role::Text; n];
    let lead = lead_idx(&ch);
    if lead < n && ch[lead] == '#' {
        paint_roles(&mut out, 0, n, Role::Primary);
        return out;
    }
    if lead < n && ch[lead] == '>' {
        paint_roles(&mut out, lead, n, Role::Muted);
        return out;
    }
    // list markers
    if lead < n && matches!(ch[lead], '-' | '*' | '+') && lead + 1 < n && ch[lead + 1] == ' ' {
        paint_roles(&mut out, lead, lead + 1, Role::Muted);
    } else if lead < n && ch[lead].is_ascii_digit() {
        let mut k = lead;
        while k < n && ch[k].is_ascii_digit() {
            k += 1;
        }
        if k < n && (ch[k] == '.' || ch[k] == ')') {
            paint_roles(&mut out, lead, k + 1, Role::Muted);
        }
    }
    // inline spans
    let mut i = 0;
    while i < n {
        if ch[i] == '`' {
            let mut j = i + 1;
            while j < n && ch[j] != '`' {
                j += 1;
            }
            let j = (j + 1).min(n);
            paint_roles(&mut out, i, j, Role::Tertiary);
            i = j;
        } else if i + 1 < n && ch[i] == '*' && ch[i + 1] == '*' {
            let mut j = i + 2;
            while j + 1 < n && !(ch[j] == '*' && ch[j + 1] == '*') {
                j += 1;
            }
            let j = (j + 2).min(n);
            paint_roles(&mut out, i, j, Role::Secondary); // **bold**
            i = j;
        } else if matches!(ch[i], '*' | '_') {
            let q = ch[i];
            let mut j = i + 1;
            while j < n && ch[j] != q {
                j += 1;
            }
            if j < n && j > i + 1 {
                paint_roles(&mut out, i, j + 1, Role::Quaternary); // *em*
                i = j + 1;
            } else {
                i += 1;
            }
        } else if ch[i] == '[' {
            let mut j = i + 1;
            while j < n && ch[j] != ']' {
                j += 1;
            }
            if j + 1 < n && ch[j + 1] == '(' {
                let mut k = j + 2;
                while k < n && ch[k] != ')' {
                    k += 1;
                }
                let k = (k + 1).min(n);
                paint_roles(&mut out, i, j + 1, Role::Secondary); // [text]
                paint_roles(&mut out, j + 1, k, Role::Quaternary); // (url)
                i = k;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Which independent level a graded cell takes: foreground text vs background.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Channel {
    Text,
    Bg,
}

/// Apply the monitor-OSD [`Grade`](crate::theme::Grade) to one final cell colour,
/// in HSLA, at paint time — the last step before a cell is committed so the whole
/// composited display is graded uniformly (text and background still take their
/// own levels). Each slider is `0..=1` with 0.5 neutral; a neutral grade is the
/// identity (and is the common case, so it short-circuits).
fn graded(c: Hsla, g: &crate::theme::Grade, ch: Channel) -> Hsla {
    if g.is_neutral() {
        return c;
    }
    // 0.5 → 1.0; the slider spans a 0..2 multiplier around neutral.
    let f = |v: f32| v / 0.5;
    let s = (c.s * f(g.colour)).clamp(0.0, 1.0);
    let mut l = c.l.clamp(0.0, 1.0);
    // gamma: 0.5 → exponent 1.0 (identity); <0.5 lifts mid-tones, >0.5 deepens.
    let gamma = 2f32.powf((0.5 - g.gamma) * 2.0);
    l = l.powf(gamma);
    // contrast pushes lightness away from (or toward) mid-grey…
    l = (l - 0.5) * f(g.contrast) + 0.5;
    // …then master brightness lights the SCREEN. The background field has dark
    // headroom and brightens fully; TEXT is already near the top of the lightness
    // range, where raising L in HSL just washes any hue toward white — so for text
    // brightness only ever DIMS (multiplier capped at 1.0). Turning brightness up
    // thus lights the screen without bleaching the text; brightening the text
    // itself is the per-channel `text` slider's job.
    let bri = f(g.brightness);
    l *= match ch {
        Channel::Text => bri.min(1.0),
        Channel::Bg => bri,
    };
    // …then the per-channel text/background level scales it.
    l *= match ch {
        Channel::Text => f(g.text),
        Channel::Bg => f(g.background),
    };
    Hsla {
        h: c.h,
        s,
        l: l.clamp(0.0, 1.0),
        a: c.a,
    }
}

/// Frame-constant [`Grade`](crate::theme::Grade) coefficients, precomputed once
/// per pane render so the per-cell paint loop ([`TerminalView::styled_lines`])
/// doesn't redo identical work on every one of the thousands of cells in a
/// frame. The grade is fixed for the whole frame, so the gamma exponent's
/// `powf` and the six `÷0.5` channel scalings are loop-invariant — hoisting them
/// here leaves the per-cell hot path with a single `l.powf(gamma)` (its base
/// genuinely varies per cell) plus a few multiplies.
///
/// [`Self::apply`] is **bit-for-bit identical** to calling [`graded`] per cell
/// with the same grade: the precomputed terms equal their inlined originals
/// (same inputs ⇒ same `powf`/division), and the per-cell operation order is
/// unchanged — so no float result moves by even one ULP. The
/// `grade_coeffs_match_graded` test pins this across a grade × colour × channel
/// sweep; `graded` stays the single source of truth the fast path is checked
/// against.
#[derive(Clone, Copy)]
struct GradeCoeffs {
    /// A neutral grade is the identity; `apply` returns the colour untouched —
    /// the same short-circuit (and the same [`Grade::is_neutral`] predicate)
    /// `graded` takes.
    neutral: bool,
    colour_mul: f32,   // f(g.colour)
    gamma_exp: f32,    // 2^((0.5 − g.gamma) · 2)
    contrast_mul: f32, // f(g.contrast)
    text_bri: f32,     // f(g.brightness).min(1.0) — Text-channel brightness
    bg_bri: f32,       // f(g.brightness)           — Bg-channel brightness
    text_lvl: f32,     // f(g.text)
    bg_lvl: f32,       // f(g.background)
}

impl GradeCoeffs {
    /// Compute the per-frame coefficients from a stored grade. Mirrors the
    /// loop-invariant expressions in [`graded`] exactly.
    fn new(g: &crate::theme::Grade) -> Self {
        // 0.5 → 1.0; the slider spans a 0..2 multiplier around neutral — the
        // same `f` closure `graded` uses.
        let f = |v: f32| v / 0.5;
        let bri = f(g.brightness);
        Self {
            neutral: g.is_neutral(),
            colour_mul: f(g.colour),
            gamma_exp: 2f32.powf((0.5 - g.gamma) * 2.0),
            contrast_mul: f(g.contrast),
            text_bri: bri.min(1.0),
            bg_bri: bri,
            text_lvl: f(g.text),
            bg_lvl: f(g.background),
        }
    }

    /// Per-cell application — the hot path. Step-for-step the same arithmetic as
    /// [`graded`], with the frame-constant terms already resolved.
    #[inline]
    fn apply(&self, c: Hsla, ch: Channel) -> Hsla {
        if self.neutral {
            return c;
        }
        let s = (c.s * self.colour_mul).clamp(0.0, 1.0);
        let mut l = c.l.clamp(0.0, 1.0);
        l = l.powf(self.gamma_exp);
        l = (l - 0.5) * self.contrast_mul + 0.5;
        l *= match ch {
            Channel::Text => self.text_bri,
            Channel::Bg => self.bg_bri,
        };
        l *= match ch {
            Channel::Text => self.text_lvl,
            Channel::Bg => self.bg_lvl,
        };
        Hsla {
            h: c.h,
            s,
            l: l.clamp(0.0, 1.0),
            a: c.a,
        }
    }
}

/// Cached `resolved_theme` result + the inputs it was computed from
/// (effective choice, mode, inherit_theme, theme generation).
type ThemeMemo = Option<(theme::ThemeChoice, PaneMode, bool, u64, Theme)>;

pub struct TerminalView {
    focus_handle: FocusHandle,
    session: term::Session,
    /// The OSC-driven shell title (apps overwrite it via the title sequence).
    pub title: String,
    /// A user-set name (right-click the header to rename). Wins over `title`
    /// and survives OSC title updates; persisted per leaf in the state file.
    pub name: Option<String>,
    /// Filesystem path to a user-chosen header logo image (png/jpg/jpeg/svg). Shown
    /// to the left of the program label; click the logo (or the `＋ logo`
    /// placeholder when unset) to pick one. Persisted per leaf in the state file.
    pub logo: Option<String>,
    /// Active inline-rename buffer; `Some` steals the keyboard from the PTY.
    renaming: Option<String>,
    pub exited: bool,
    grid: term::GridSize,
    cell_w: f32,
    cell_h: f32,
    scroll_accum: f32,
    selecting: bool,
    /// Drag-select auto-scroll: signed lines/tick (>0 = up into history), 0 idle.
    autoscroll: f32,
    /// True while the auto-scroll ticker loop is spinning (kept to exactly one).
    autoscroll_running: bool,
    /// Latest cursor position during a selection drag — the ticker re-extends
    /// the selection at this point as the viewport scrolls under it.
    last_mouse: gpui::Point<Pixels>,
    pending_input: Option<Instant>,
    latency_log: bool,
    /// Written by the measuring canvas during prepaint; read by sync_size.
    content_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
    spawned: Instant,
    /// This pane's own CRT rhythm — desynced from every other pane.
    pub fx: crt::Fx,
    /// Barrel coefficients for the optional renderer patch. Public upstream
    /// GPUI builds keep this at zero so mouse hit testing stays linear.
    warp_k: (f32, f32),
    /// Painted-row → grid-viewport-row transform recorded each frame so hit-test
    /// (`cell_at` / `link_under`) and wheel scrolling can invert the same visual
    /// transform the render applied. `paint_offset` is the `bottom_anchor_rows`
    /// shift; `paint_inverted` is true in anchor-to-top inverted mode (the rows
    /// were bottom-anchored THEN reversed, so the prompt sits on top). Default
    /// `(0, false)` ⇒ no-op, byte-identical to the un-anchored path.
    paint_offset: usize,
    paint_inverted: bool,
    /// In wrap-aware inverted mode, `paint_to_grid[p]` = the grid viewport row
    /// drawn at painted row `p` (logical-line reverse permutes rows non-uniformly,
    /// so a formula won't do). `None` ⇒ use the `paint_offset`/`paint_inverted`
    /// formula (default + crawl).
    paint_to_grid: Option<Vec<usize>>,
    pub mode: PaneMode,
    /// Per-pane appearance: retained theme/grade overrides plus two independent
    /// follow-outer switches. A pristine pane inherits both groups (+ mode tint).
    pub appearance: PaneTheme,
    /// Debounced PTY resize: (target grid, when it stabilized).
    pending_grid: Option<(term::GridSize, Instant)>,
    /// Scroll-settle debounce: (display_offset, when last seen). Prevents spurious
    /// agent-done notifications when Alt+up/down navigation scrolls away from the prompt.
    last_scroll_offset: Option<(i32, Instant)>,
    /// Per-pane memo for `resolved_theme`, keyed on (effective choice, mode,
    /// inherit_theme, theme generation). resolve() deep-clones, recolours and
    /// grade-transforms the palette, and render() calls it every frame; this
    /// reuses the last result until one of those inputs actually changes.
    theme_cache: RefCell<ThemeMemo>,
    /// Right-click context menu (Copy / Paste / Open link) anchor, window-space.
    ctx_menu: Option<gpui::Point<Pixels>>,
    /// A bell rang (agent finished): a SNOOZE bar shows across the pane top and
    /// the configured sound plays, until SNOOZE or the always-visible bell-off.
    bell: bool,
    /// Per-pane bell settings (sound file, trim window, loop, volume, on/off).
    bell_cfg: crate::bell::BellConfig,
    /// The live ffplay child for this pane (hard-killed on stop/drop).
    bell_player: crate::bell::BellPlayer,
    /// The BELL+ config tray is open.
    bell_menu: bool,
    /// Responsive header: when the pane narrows, controls tuck into a ⋯ overflow
    /// menu. `Some(pos)` = that menu is open, anchored at the ⋯ click. None = shut.
    hdr_overflow: Option<gpui::Point<Pixels>>,
    /// Cached duration (s) of the selected sound, for the scrubber track.
    bell_dur: Option<f32>,
    /// Last-known OS focus, for edge-detected focus reporting (CSI I / CSI O).
    was_focused: bool,
    /// 🎰 GAMBA slot-machine reels — rolled while an agent in this pane is
    /// "thinking", on the gamba theme / retro colour set. Pure satire.
    gamba: crate::gamba::Reels,
    /// Throttle for the (cheap) grid scan that detects the agent spinner.
    last_think_scan: Instant,
    /// True while this pane is the one mirrored in the FOCUS modal — a plain Esc
    /// then closes the modal instead of reaching the PTY. Set by the workspace.
    being_read: bool,
    /// Keyboard-driven selection state: `(anchor, active end)` in absolute grid
    /// points. `shift+←/→` (char) and `shift+ctrl+←/→` (word) move the active end
    /// while the anchor stays put — combinative, never resetting. `None` until a
    /// shift-arrow starts one (seeding from the cursor or an existing mouse
    /// selection); cleared whenever a normal key or a fresh mouse-down resets the
    /// selection.
    kbd_sel: Option<(TermPoint, TermPoint)>,
    /// When the current agent "thinking" spell began — used to ring the bell on
    /// the thinking→done edge (agents don't reliably emit a terminal BEL).
    think_since: Option<Instant>,
    /// When the agent transitioned to not-thinking; used to debounce false positives
    /// from transient state changes (e.g., error messages clearing). Only ring the bell
    /// if not-thinking persists for at least 300ms.
    not_thinking_since: Option<Instant>,
    /// Which bell-trim pip is being dragged (false = start, true = end); None idle.
    bell_drag: Option<bool>,
    /// Window-space bounds of the bell trim track, for pip drag math.
    bell_track_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
    /// Agent-wall HUD token accounting (agent panes only). `tokens_banked` sums
    /// the peak token count of every *completed* turn this session;
    /// `turn_peak_tokens` is the running peak of the turn in flight;
    /// `tok_was_working` edge-detects turn end to bank the peak. Fed by
    /// [`TerminalView::accrue_tokens`], read by the agent-wall HUD.
    tokens_banked: u64,
    turn_peak_tokens: u64,
    tok_was_working: bool,
}

/// Click on the header's theme icon — the workspace opens the breakout menu.
/// Carries the window-space click position so the tray opens at the icon that
/// was clicked (each sub-tab's icon lives in its own header), not a fixed spot.
pub struct OpenThemeMenu {
    pub at: gpui::Point<gpui::Pixels>,
}
impl gpui::EventEmitter<OpenThemeMenu> for TerminalView {}

/// Click on the header's display icon — the workspace opens the monitor-OSD
/// tray for this pane. Like [`OpenThemeMenu`], carries the window-space click
/// position so the tray anchors at the icon that was clicked.
pub struct OpenDisplayMenu {
    pub at: gpui::Point<gpui::Pixels>,
}
impl gpui::EventEmitter<OpenDisplayMenu> for TerminalView {}

/// The user grabbed this sub-tab's header to drag it. The workspace takes over
/// from here (window-level move/up) and decides where it lands. Carries the
/// window-space press position so the drag has an anchor.
pub struct DragPaneStart {
    pub at: gpui::Point<gpui::Pixels>,
}
impl gpui::EventEmitter<DragPaneStart> for TerminalView {}

/// The × on this sub-tab's header was clicked — close just this pane.
pub struct ClosePane;
impl gpui::EventEmitter<ClosePane> for TerminalView {}

/// Ctrl+W in this pane — the workspace closes the whole active tab, always via
/// the serious confirmation dialog (never a silent close).
pub struct RequestCloseTab;
impl gpui::EventEmitter<RequestCloseTab> for TerminalView {}

/// This sub-tab's name just changed (rename committed) — the workspace
/// persists the layout so the custom name survives a restart.
pub struct PaneRenamed;
impl gpui::EventEmitter<PaneRenamed> for TerminalView {}

/// Click on this pane's header logo (or the `＋ logo` placeholder when none is
/// set) — ask the workspace to open the image-file picker scoped to this pane.
pub struct OpenLogoPicker;
impl gpui::EventEmitter<OpenLogoPicker> for TerminalView {}

/// F1 was pressed in this pane — ask the workspace to open the help modal.
pub struct OpenHelp;
impl gpui::EventEmitter<OpenHelp> for TerminalView {}

/// Ctrl+Shift+A in this pane — ask the workspace to open the agent-watch (MCP)
/// panel. It spans every pane, so the workspace owns it; the pane just signals.
pub struct OpenAgentPanel;
impl gpui::EventEmitter<OpenAgentPanel> for TerminalView {}

/// Ctrl+F (`global = false`) / Ctrl+Shift+F (`global = true`) was pressed in this
/// pane — ask the workspace to open the find panel. In-pane find searches just
/// this pane (and the panel centres over it); global find searches every pane.
pub struct OpenFind {
    pub global: bool,
}
impl gpui::EventEmitter<OpenFind> for TerminalView {}

/// One matched line inside a pane's grid: its absolute grid line index, the line
/// text (built from column 0 so a char index is also its column), and the fuzzy
/// score + matched char positions — for the snippet highlight and the jump-time
/// selection of the hit span.
pub struct GridHit {
    pub line: i32,
    pub text: String,
    pub score: i64,
    pub positions: Vec<usize>,
}

/// A lightweight fzf-style fuzzy subsequence match. `needle` must already be
/// lowercased; `hay` is compared case-insensitively (ASCII fold). Returns `None`
/// unless every needle char appears in order; otherwise `(score, positions)` —
/// higher score is better (contiguous runs + word-start hits weigh more) and
/// `positions` are the char indices in `hay` that matched, for highlighting. An
/// empty needle never matches.
pub(crate) fn fuzzy_match(hay: &str, needle: &str) -> Option<(i64, Vec<usize>)> {
    if needle.is_empty() {
        return None;
    }
    let needle: Vec<char> = needle.chars().collect();
    let mut positions = Vec::with_capacity(needle.len());
    let mut ni = 0usize;
    let mut score: i64 = 0;
    let mut prev_match: Option<usize> = None;
    let mut prev_char: Option<char> = None;
    for (hi, hc) in hay.chars().enumerate() {
        if ni >= needle.len() {
            break;
        }
        if hc.to_ascii_lowercase() == needle[ni] {
            score += 8;
            // contiguity bonus: adjacent to the previously matched char
            if prev_match == Some(hi.wrapping_sub(1)) {
                score += 14;
            }
            // word-start bonus: first char, or preceded by a non-alphanumeric
            if prev_char.map(|c| !c.is_alphanumeric()).unwrap_or(true) {
                score += 10;
            }
            positions.push(hi);
            prev_match = Some(hi);
            ni += 1;
        }
        prev_char = Some(hc);
    }
    if ni == needle.len() {
        // tighter (shorter) haystacks edge out sprawling ones at equal matches
        score -= (hay.chars().count() as i64) / 16;
        Some((score, positions))
    } else {
        None
    }
}

/// The 👓 (reading-glasses) icon on this sub-tab's header was clicked — the
/// workspace opens a FOCUS modal: an 80%-of-window mirror of this pane's live
/// screen, with the rest of the window dimmed back. No anchor: the modal is
/// always centred in the window.
pub struct OpenFocusRead;
impl gpui::EventEmitter<OpenFocusRead> for TerminalView {}

/// Esc was pressed while this pane is the one being focus-read — close the
/// modal. Routed through the pane (not the workspace) because the mirrored pane
/// keeps keyboard focus so you can keep typing into it while you read.
pub struct CloseFocusRead;
impl gpui::EventEmitter<CloseFocusRead> for TerminalView {}

/// A read-only snapshot the workspace paints into the FOCUS modal. It's just the
/// same styled rows [`styled_lines`] already builds for the live pane, plus the
/// metrics needed to scale them up to fill the modal — so the mirror costs one
/// extra (cheap) grid scan of a single pane, never a second terminal or PTY.
pub struct MirrorSnapshot {
    pub lines: Vec<(String, Vec<TextRun>)>,
    pub bg: Hsla,
    pub text: Hsla,
    pub accent: Hsla,
    pub font_family: String,
    /// The live base glyph size (font_size × the pane's effective scale).
    pub base_size: f32,
    pub cell_w: f32,
    pub cell_h: f32,
    pub cols: usize,
    pub rows: usize,
    pub title: String,
    /// Crawl mode is on for this pane — the FOCUS modal inherits the look: the
    /// rows are already in the crawl font (baked into `lines`' runs) and the
    /// modal centres each row, matching the live pane.
    pub crawl: bool,
    /// This pane's resolved barrel-warp shader coefficients (`k1`, `k2`) and
    /// screen-glare strength. The FOCUS reader uses them when "Inherit theme" is
    /// on, registering the panel as a warp tube so it bends + glares like the
    /// pane it mirrors (identity `0`/`0`/`0` for a flat pane → no change).
    pub k1: f32,
    pub k2: f32,
    pub glare: f32,
}

impl TerminalView {
    /// The theme this pane actually renders with: each appearance group
    /// (theme, grade) resolved to the pane's own override or the live outer
    /// scope, then — when the theme group follows outer — tinted by what's
    /// running (mode).
    pub fn resolved_theme(&self, cx: &App) -> Theme {
        let outer = theme::outer_choice(cx);
        let eff = self.appearance.effective(&outer);
        let inherit = self.appearance.inherit_theme;
        let gen = theme::theme_gen(cx);
        // Per-frame memo: resolve() deep-clones + recolours + grade-transforms the
        // palette and render() calls this every frame, so reuse the last result
        // while every input is unchanged. The generation counter covers the two
        // global inputs a ThemeChoice doesn't carry (custom hot-reload, tracking
        // override), so this can't serve a stale look.
        if let Some((k_eff, k_mode, k_inherit, k_gen, th)) = &*self.theme_cache.borrow() {
            if *k_gen == gen && *k_inherit == inherit && *k_mode == self.mode && *k_eff == eff {
                return th.clone();
            }
        }
        let base = (*theme::resolve(cx, &eff)).clone();
        // The mode tint (what's running in the pane) applies only while the
        // theme group follows outer — an explicit per-pane theme is a deliberate
        // look the tint shouldn't stomp. The grade rides along untouched either
        // way (mode_theme leaves `grade`/`color_mode` alone).
        let mut out = if inherit {
            mode_theme(&base, &self.mode)
        } else {
            base
        };
        // Terminal text-size: scale the GRID font + cell height by the pane's
        // effective text-size grade so the terminal reflows (sync_size measures
        // cell_w from font_size and cell_h from this). Chrome is untouched —
        // that's `grade.scale` (the menu-bar slider). Neutral 1.0 = config size.
        let ts = eff.grade.text_size;
        if (ts - 1.0).abs() > f32::EPSILON {
            out.font_size *= ts;
            out.cell_h *= ts;
        }
        *self.theme_cache.borrow_mut() = Some((eff, self.mode.clone(), inherit, gen, out.clone()));
        out
    }

    /// Build the read-only [`MirrorSnapshot`] the workspace paints into the
    /// FOCUS modal. Reuses the exact same styled rows the live pane renders, so
    /// the mirror is pixel-identical and stays live (the workspace re-renders
    /// whenever this pane notifies). No second terminal, no extra PTY work.
    pub fn mirror_snapshot(&self, cx: &App) -> MirrorSnapshot {
        let th = self.resolved_theme(cx);
        let mut lines = self.styled_lines(&th);
        // Mirror the live pane's anchor-to-top inverted read: bottom-anchor the
        // rows (prompt to the bottom) THEN reverse, so the FOCUS reader shows the
        // prompt on TOP with older output flowing down, exactly like the pane.
        // Crawl keeps its own bottom-anchor look (handled in the modal), so it is
        // excluded — matching the live render's `anchor_top() && !th.crawl` gate.
        // Off (the default) leaves `lines` untouched → byte-identical to before.
        if anchor_top() && !th.crawl {
            let block_mode = self.mode.is_agent();
            let wraps = if block_mode {
                Vec::new()
            } else {
                self.row_wraps()
            };
            let (new_lines, _perm) = invert_logical_read(lines, &wraps, block_mode);
            lines = new_lines;
        }
        // The pane's own resolved CRT curvature + glare, so the FOCUS reader can
        // inherit the look on demand (flat 0/0/0 for a flat pane → no-op).
        let (k1, k2) = crate::theme::warp_coeffs(th.warp);
        MirrorSnapshot {
            lines,
            bg: th.bg,
            text: th.text,
            accent: th.accent,
            font_family: th.font_family.clone(),
            // The grid renders at its native size now (the scrubber sizes the
            // menu bar, not the terminal), so the mirror matches it untouched.
            base_size: th.font_size,
            cell_w: self.cell_w,
            cell_h: self.cell_h,
            cols: self.grid.cols,
            rows: self.grid.rows,
            title: self.name.clone().unwrap_or_else(|| self.title.clone()),
            crawl: th.crawl,
            k1,
            k2,
            glare: th.screen_glare,
        }
    }

    /// Toggle whether this pane is the one currently mirrored in the FOCUS modal.
    /// When set, a plain Esc closes the modal instead of reaching the PTY.
    pub fn set_being_read(&mut self, on: bool) {
        self.being_read = on;
    }

    /// What this pane is doing right now — cwd + resumable agent session —
    /// captured from the kernel for the workspace snapshot.
    pub fn runtime(&self) -> crate::session::PaneRuntime {
        crate::session::capture(self.session.master.as_ref(), self.session.shell_pid)
    }

    /// This pane's shell pid — the kernel handle behind its identity. Ephemeral
    /// (recycles across a resume); the durable key is the agent session. Read by
    /// the read-only MCP snapshot.
    pub fn shell_pid(&self) -> u32 {
        self.session.shell_pid
    }

    /// Plain spawn (no restore context); kept for `cx.new(TerminalView::new)`.
    #[allow(dead_code)]
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self::new_restored(crate::session::PaneRestore::default(), cx)
    }

    /// Spawn with session-restore context: shell starts in `restore.cwd`, and
    /// a resumable agent (`claude --resume <id>` / `codex resume <id>`) is
    /// typed into the PTY — the kernel queues it until the first prompt reads.
    pub fn new_restored(restore: crate::session::PaneRestore, cx: &mut Context<Self>) -> Self {
        let grid = term::GridSize {
            cols: 100,
            rows: 28,
        };
        let logo = restore.logo.clone();
        let cwd = restore.cwd.clone().map(std::path::PathBuf::from);
        let mut session = term::spawn_in(grid, 8, 20, cwd).expect("spawn shell");
        if let Some(cmd) = restore.resume.as_deref() {
            session.notifier.notify(format!("{cmd}\n").into_bytes());
        }

        let mut events = session.events.take().expect("events taken once");
        cx.spawn(async move |this, cx| {
            while let Some(event) = events.next().await {
                let keep_going = this
                    .update(cx, |view: &mut TerminalView, cx| {
                        view.handle_term_event(event, cx)
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        })
        .detach();

        // foreground-process watcher: what is this tube showing?
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(800))
                .await;
            let alive = this
                .update(cx, |view: &mut TerminalView, cx| {
                    if let Some(master) = view.session.master.as_ref() {
                        let mode = foreground_mode(master, view.session.shell_pid);
                        if mode != view.mode {
                            view.mode = mode;
                            cx.notify();
                        }
                    }
                    !view.exited
                })
                .unwrap_or(false);
            if !alive {
                break;
            }
        })
        .detach();

        // per-pane effects clock
        cx.spawn(async move |this, cx| {
            loop {
                let active = this.update(cx, |view: &mut TerminalView, cx| {
                    let th = theme::theme(cx);
                    if view.fx.tick(&th) {
                        cx.notify();
                    }
                    // 🎰 GAMBA: poll the grid (throttled) for the agent
                    // spinner, then advance the reel stack while it rolls.
                    if view.last_think_scan.elapsed() > std::time::Duration::from_millis(120) {
                        view.last_think_scan = Instant::now();
                        // Agent-wall HUD: accrue per-turn → session token totals off
                        // the live status line every tick (independent of the bell's
                        // scroll-settle gate below, which would otherwise skip it).
                        view.accrue_tokens();
                        // Scroll-settle debounce: Alt+up/down scrollback navigation
                        // moves the "esc to interrupt" line off-screen and would trip
                        // a false agent-done bell. Only run the thinking-scan once the
                        // display offset has held steady for 200ms.
                        let cur_offset = view.session.term.lock().grid().display_offset() as i32;
                        let scroll_settled = match view.last_scroll_offset {
                            Some((off, since)) if off == cur_offset => {
                                since.elapsed() > std::time::Duration::from_millis(200)
                            }
                            _ => {
                                view.last_scroll_offset = Some((cur_offset, Instant::now()));
                                false
                            }
                        };
                        if scroll_settled {
                            let thinking = view.agent_is_thinking();
                            if thinking != view.gamba.is_thinking() {
                                view.gamba.set_thinking(thinking);
                                if thinking {
                                    view.think_since = Some(Instant::now());
                                    view.not_thinking_since = None;
                                } else {
                                    // Transitioned to not-thinking; debounce to avoid false
                                    // positives from transient state changes (error messages, etc).
                                    view.not_thinking_since = Some(Instant::now());
                                }
                            }
                            // Only ring the bell if we've been not-thinking for 300ms+ AND
                            // the original thinking period was real (> 1200ms).
                            if !thinking && view.bell_cfg.enabled && view.mode.is_agent() {
                                if let Some(not_since) = view.not_thinking_since {
                                    if not_since.elapsed() > std::time::Duration::from_millis(300) {
                                        // "Real" = the thinking spell itself lasted
                                        // > 1200ms (measure start→end, not start→now,
                                        // so the debounce delay doesn't skew it).
                                        let real = match (view.think_since, view.not_thinking_since)
                                        {
                                            (Some(start), Some(end)) => {
                                                end.duration_since(start)
                                                    > std::time::Duration::from_millis(1200)
                                            }
                                            _ => false,
                                        };
                                        if real && !view.bell {
                                            view.bell = true;
                                            view.bell_player.play(&view.bell_cfg);
                                            view.think_since = None;
                                            view.not_thinking_since = None;
                                            cx.notify();
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if view.gamba.tick() {
                        cx.notify();
                    }
                    // reap a finished bell clip so ffplay zombies don't pile up
                    view.bell_player.reap();
                    // debounced PTY resize: fire once the drag settles
                    if let Some((grid, since)) = view.pending_grid {
                        if since.elapsed() > std::time::Duration::from_millis(140) {
                            view.pending_grid = None;
                            view.grid = grid;
                            view.session
                                .resize(grid, view.cell_w as u16, view.cell_h as u16);
                            cx.notify();
                        }
                    }
                    // Stay at frame-rate only while something is actually
                    // moving — CRT fx, or GAMBA reels/FX/rumble in motion.
                    // A landed-but-thinking board falls through to the idle
                    // cadence (no 30fps repaint of a static slot grid).
                    view.fx.active() || view.gamba.is_animating()
                });
                // `this` is weak: once this pane's TerminalView is dropped (close
                // a pane / tab / window) the update errors — break so the ticker
                // ends instead of waking forever on a dead entity. Without this,
                // every closed pane leaks a permanent background loop and idle
                // CPU climbs over a session.
                let Ok(active) = active else { break };
                let ms = if active { 33 } else { 150 };
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(ms))
                    .await;
            }
        })
        .detach();

        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
            .unwrap_or(7);
        Self {
            focus_handle: cx.focus_handle(),
            session,
            title: "shell".into(),
            name: None,
            logo,
            renaming: None,
            exited: false,
            grid,
            cell_w: 8.4,
            cell_h: 20.,
            scroll_accum: 0.,
            selecting: false,
            autoscroll: 0.,
            autoscroll_running: false,
            last_mouse: point(px(0.), px(0.)),
            pending_input: None,
            latency_log: std::env::var("TD_LATENCY").is_ok(),
            content_bounds: Arc::new(Mutex::new(None)),
            spawned: Instant::now(),
            fx: crt::Fx::new(seed),
            warp_k: (0., 0.),
            paint_offset: 0,
            paint_inverted: false,
            paint_to_grid: None,
            mode: PaneMode::Shell,
            appearance: PaneTheme::default(),
            ctx_menu: None,
            bell: false,
            bell_cfg: crate::bell::BellConfig::default(),
            bell_player: crate::bell::BellPlayer::default(),
            bell_menu: false,
            hdr_overflow: None,
            bell_dur: None,
            was_focused: false,
            pending_grid: None,
            last_scroll_offset: None,
            theme_cache: RefCell::new(None),
            gamba: crate::gamba::Reels::new(seed),
            last_think_scan: Instant::now()
                .checked_sub(std::time::Duration::from_secs(1))
                .unwrap_or_else(Instant::now),
            being_read: false,
            kbd_sel: None,
            think_since: None,
            not_thinking_since: None,
            bell_drag: None,
            bell_track_bounds: Arc::new(Mutex::new(None)),
            tokens_banked: 0,
            turn_peak_tokens: 0,
            tok_was_working: false,
        }
    }

    /// Is the agent in this pane "thinking" right now? We scan the visible grid
    /// for the spinner hint Claude/Codex print while a turn runs ("esc to
    /// interrupt"). `TD_GAMBA_DEMO=1` forces it on for demos/screenshots.
    fn agent_is_thinking(&self) -> bool {
        if std::env::var("TD_GAMBA_DEMO").is_ok() {
            return true;
        }
        if !self.mode.is_agent() {
            return false;
        }
        let term = self.session.term.lock();
        // Scan the LIVE bottom screen directly (Line(0)..screen_lines), NOT
        // `renderable_content().display_iter` — that honours the display offset, so
        // when Alt+↑ scrolls back to a human message the running agent's "esc to
        // interrupt" spinner leaves the *viewport* and the scan falsely reads
        // "done". The agent is still working at the buffer bottom, so detection
        // must read the live screen regardless of how far the user has scrolled up.
        let grid = term.grid();
        let rows = grid.screen_lines();
        let cols = grid.columns();
        for line in 0..rows as i32 {
            let row = &grid[Line(line)];
            let mut s = String::with_capacity(cols);
            for col in 0..cols {
                let cell = &row[Column(col)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                s.push(if cell.c == '\0' { ' ' } else { cell.c });
            }
            let low = s.to_ascii_lowercase();
            if low.contains("esc to interrupt") || low.contains("interrupt)") {
                return true;
            }
        }
        false
    }

    /// Snapshot the live bottom screen as plain-text rows (top→bottom) — the same
    /// region [`TerminalView::agent_is_thinking`] scans. Feeds the HUD parser.
    fn live_rows(&self) -> Vec<String> {
        let term = self.session.term.lock();
        let grid = term.grid();
        let rows = grid.screen_lines();
        let cols = grid.columns();
        let mut out = Vec::with_capacity(rows);
        for line in 0..rows as i32 {
            let row = &grid[Line(line)];
            let mut s = String::with_capacity(cols);
            for col in 0..cols {
                let cell = &row[Column(col)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                s.push(if cell.c == '\0' { ' ' } else { cell.c });
            }
            out.push(s);
        }
        out
    }

    /// The last `n` non-blank rows of the live bottom screen, top→bottom — the
    /// agent's most recent output, for the dashboard card's mini "chat scroller".
    /// Trimmed of trailing whitespace; blank rows dropped so the feed stays dense.
    pub fn recent_lines(&self, n: usize) -> Vec<String> {
        let mut lines: Vec<String> = self
            .live_rows()
            .into_iter()
            .map(|r| r.trim_end().to_string())
            .filter(|r| !r.trim().is_empty())
            .collect();
        let start = lines.len().saturating_sub(n);
        lines.drain(..start);
        lines
    }

    /// Parse this pane's live status line into an [`crate::hud::AgentStatus`] for
    /// the agent-wall HUD. Non-agent panes read Idle; an otherwise-idle agent with
    /// an unacknowledged finish bell is promoted to `Finished`.
    pub fn agent_status(&self) -> crate::hud::AgentStatus {
        if !self.mode.is_agent() {
            return crate::hud::AgentStatus::default();
        }
        let mut st = crate::hud::parse_status_line(&self.live_rows());
        if st.state == crate::hud::AgentState::Idle && self.bell {
            st.state = crate::hud::AgentState::Finished;
        }
        st
    }

    /// Tokens this agent has spent this session: banked completed turns plus the
    /// running peak of any turn in flight. Zero for non-agent panes.
    pub fn session_tokens(&self) -> u64 {
        self.tokens_banked.saturating_add(self.turn_peak_tokens)
    }

    /// Drive HUD token accounting off the live status line: track the current
    /// turn's peak token count and, on the working→idle edge, bank it into the
    /// session total. Called (throttled) from the per-pane effects clock.
    fn accrue_tokens(&mut self) {
        if !self.mode.is_agent() {
            return;
        }
        let st = self.agent_status();
        let working = st.working();
        if working {
            if let Some(t) = st.turn_tokens {
                self.turn_peak_tokens = self.turn_peak_tokens.max(t);
            }
        } else if self.tok_was_working {
            self.tokens_banked = self.tokens_banked.saturating_add(self.turn_peak_tokens);
            self.turn_peak_tokens = 0;
        }
        self.tok_was_working = working;
    }

    /// Does this pane have an unacknowledged "agent finished" bell raised? Read by
    /// the workspace to badge the owning tab; cleared by the in-terminal ack click
    /// (see [`snooze_bell`]).
    pub fn has_bell(&self) -> bool {
        self.bell
    }

    fn handle_term_event(&mut self, event: TermEvent, cx: &mut Context<Self>) -> bool {
        match event {
            TermEvent::Wakeup => {
                if let Some(t) = self.pending_input.take() {
                    if self.latency_log {
                        eprintln!("td_latency_us={}", t.elapsed().as_micros());
                    }
                }
                cx.notify();
            }
            TermEvent::PtyWrite(text) => self.session.notifier.notify(text.into_bytes()),
            TermEvent::Title(title) => {
                self.title = title;
                cx.notify();
            }
            // An agent rang the bell — raise the alert and play this pane's sound.
            // Gated to agent (claude/codex) panes: the card literally reads "agent
            // finished", so a plain shell BEL (e.g. readline's "cannot perform that
            // action" beep on a failed tab-complete) must NOT trigger it.
            TermEvent::Bell if self.bell_cfg.enabled && self.mode.is_agent() => {
                self.bell = true;
                self.bell_player.play(&self.bell_cfg);
                cx.notify();
            }
            TermEvent::Exit | TermEvent::ChildExit(_) => {
                self.exited = true;
                cx.notify();
                return false;
            }
            _ => {}
        }
        true
    }

    /// Measure the real cell metrics from the active theme, fit grid to window.
    /// Fit the PTY grid to the measured content area. The terminal's own text
    /// size is fixed (the global scrubber now sizes the menu bar, not the grid),
    /// so this always measures at the theme's native cell/font metrics.
    fn sync_size(&mut self, th: &Theme, window: &mut Window) {
        self.cell_h = th.cell_h;
        let font = grid_font(th, FontWeight::NORMAL);
        if let Ok(w) = window.text_system().advance(
            window.text_system().resolve_font(&font),
            px(th.font_size),
            'M',
        ) {
            if f32::from(w.width) > 1.0 {
                self.cell_w = f32::from(w.width);
            }
        }
        // Fit the grid to the tube minus its (curvature-aware) frame, so the
        // padding and the row/col count never disagree. `th` gives the exact warp
        // for this frame — no dependence on `self.warp_k`'s render-time update.
        let (k1, k2) = theme::warp_coeffs(th.warp);
        let stored = *self.content_bounds.lock().unwrap();
        let (tube_w, tube_h) = match stored {
            Some(b) => (f32::from(b.size.width), f32::from(b.size.height)),
            None => {
                let viewport = window.viewport_size();
                (
                    f32::from(viewport.width),
                    f32::from(viewport.height) - HEADER_H,
                )
            }
        };
        let (pad_x, pad_y) = grid_pad(tube_w, tube_h, k1, k2);
        let (avail_w, avail_h) = (tube_w - pad_x * 2., tube_h - pad_y * 2.);
        let cols = ((avail_w / self.cell_w).floor() as usize).max(10);
        let rows = ((avail_h / self.cell_h).floor() as usize).max(3);
        let target = term::GridSize { cols, rows };
        if target.cols != self.grid.cols || target.rows != self.grid.rows {
            // stage it; the effects clock applies once the size stops moving
            match self.pending_grid {
                Some((g, _)) if g.cols == cols && g.rows == rows => {}
                _ => self.pending_grid = Some((target, Instant::now())),
            }
        } else {
            self.pending_grid = None;
        }
    }

    /// Window-space anchor for a keyboard-opened header menu (theme/display):
    /// the pane's top-right, level with the header icons, so a Ctrl+Shift+G/D
    /// chord opens the tray in the same spot the icon click would. Falls back to
    /// the top-left header line before the first layout caches the bounds.
    fn header_anchor(&self) -> gpui::Point<Pixels> {
        match *self.content_bounds.lock().unwrap() {
            Some(b) => point(b.origin.x + b.size.width, b.origin.y),
            None => point(px(0.), px(HEADER_H)),
        }
    }

    /// Map a screen point to a viewport cell (row, col in 0..rows/cols) plus the
    /// side of the cell, inverting the tube's barrel warp so hit-testing follows
    /// the curved glass. Shared by selection (`cell_at`) and link hit-testing.
    fn viewport_cell(&self, pos: gpui::Point<Pixels>) -> (usize, usize, Side) {
        let bounds = *self.content_bounds.lock().unwrap();
        let (bx, by, bw, bh) = match bounds {
            Some(b) => (
                f32::from(b.origin.x),
                f32::from(b.origin.y),
                f32::from(b.size.width).max(1.),
                f32::from(b.size.height).max(1.),
            ),
            None => (0., HEADER_H, 1000., 1000.),
        };
        // Normalise the click into rect-local [0,1], apply the SAME barrel map
        // the shader gathers with, then convert content-local back to a cell.
        let (k1, k2) = self.warp_k;
        let (lx, ly) = warp_screen_to_content(
            (f32::from(pos.x) - bx) / bw,
            (f32::from(pos.y) - by) / bh,
            k1,
            k2,
        );
        // Same frame the renderer laid the grid into, so a click maps to the
        // cell shown under it (the grid starts at pad_x/pad_y inside the tube).
        let (pad_x, pad_y) = grid_pad(bw, bh, k1, k2);
        let fx = (lx * bw - pad_x) / self.cell_w;
        let y = ((ly * bh - pad_y) / self.cell_h).max(0.) as usize;
        let col = (fx.max(0.) as usize).min(self.grid.cols.saturating_sub(1));
        let row = y.min(self.grid.rows.saturating_sub(1));
        if std::env::var("TD_HITDEBUG").is_ok() {
            // Fractional row/col BEFORE flooring: a value landing near .0 (a cell
            // boundary) is where an off-by-one shows up. Click a KNOWN link row and
            // compare `frac=rN.NN` to its visual row: a consistent offset at the
            // bottom of tall panes means the grid is painted off from where the
            // hit-test models it (a placement delta), not a warp-coefficient delta.
            let frac_row = (ly * bh - pad_y) / self.cell_h;
            let frac_col = (lx * bw - pad_x) / self.cell_w;
            eprintln!(
                "hit pos=({:.0},{:.0}) rect=({:.0},{:.0},{:.0},{:.0}) k={:?} local=({:.3},{:.3}) frac=(r{frac_row:.2},c{frac_col:.2}) cell=(r{row},c{col}) cellhw=({:.1},{:.1}) pad=({:.1},{:.1}) rows={}",
                f32::from(pos.x),
                f32::from(pos.y),
                bx,
                by,
                bw,
                bh,
                self.warp_k,
                lx,
                ly,
                self.cell_w,
                self.cell_h,
                pad_x,
                pad_y,
                self.grid.rows,
            );
        }
        let side = if fx.fract() < 0.5 {
            Side::Left
        } else {
            Side::Right
        };
        (row, col, side)
    }

    /// Invert the per-frame paint transform: a PAINTED/visual viewport row `p`
    /// (what `viewport_cell` returns) → the GRID viewport row `g` the renderer
    /// drew there. The render either bottom-anchored (`g = p - offset`, incl.
    /// crawl) or, in anchor-to-top inverted mode, bottom-anchored THEN reversed
    /// (`g = (rows-1 - p) - offset`). Clamped to `0..rows-1`. With the default
    /// `paint_offset == 0 && !paint_inverted`, this is the identity (`g == p`),
    /// so the un-anchored path is byte-identical to before.
    fn paint_row_to_grid_row(&self, p: usize) -> usize {
        if let Some(perm) = &self.paint_to_grid {
            return perm
                .get(p)
                .copied()
                .unwrap_or(0)
                .min(self.grid.rows.saturating_sub(1));
        }
        paint_row_to_grid_row_impl(p, self.grid.rows, self.paint_offset, self.paint_inverted)
    }

    /// Per-row WRAPLINE flags in grid viewport order: `wraps[r]` ⇒ grid row `r`
    /// soft-wraps into `r+1`. Lets the wrap-aware inverted read keep a wrapped
    /// logical line grouped. Cheap: one term lock + a `display_iter` pass.
    fn row_wraps(&self) -> Vec<bool> {
        let term = self.session.term.lock();
        let content = term.renderable_content();
        let display_offset = content.display_offset;
        let rows = self.grid.rows;
        let mut wraps = vec![false; rows];
        for indexed in content.display_iter {
            let r = indexed.point.line.0 + display_offset as i32;
            if r < 0 || r as usize >= rows {
                continue;
            }
            if indexed.cell.flags.contains(Flags::WRAPLINE) {
                wraps[r as usize] = true;
            }
        }
        wraps
    }

    fn cell_at(&self, pos: gpui::Point<Pixels>, display_offset: usize) -> (TermPoint, Side) {
        let (row, col, side) = self.viewport_cell(pos);
        let row = self.paint_row_to_grid_row(row);
        (
            viewport_to_point(display_offset, TermPoint::new(row, Column(col))),
            side,
        )
    }

    /// The shift-clickable link under a screen point, if any: read the clicked
    /// row out of the visible grid, scan around the column, and resolve a path
    /// against the pane's cwd (only returning paths that actually exist).
    fn link_under(&self, pos: gpui::Point<Pixels>) -> Option<String> {
        let (vrow, vcol, _) = self.viewport_cell(pos);
        // Map the painted/visual row back to the grid viewport row it shows
        // (identity in the default un-anchored path; inverts the anchor-to-top
        // flip + any bottom-anchor offset otherwise).
        let vrow = self.paint_row_to_grid_row(vrow);
        // Read the whole visible grid plus per-row soft-wrap flags, then stitch
        // the clicked row to its neighbours so a URL/path wrapped across rows is
        // recognised as one token (see `stitch_wrapped_line`).
        let (line, col) = {
            let term = self.session.term.lock();
            let content = term.renderable_content();
            let display_offset = content.display_offset;
            let cols = self.grid.cols;
            let rows = self.grid.rows;
            let mut grid: Vec<Vec<char>> = vec![vec![' '; cols]; rows];
            let mut wraps: Vec<bool> = vec![false; rows];
            for indexed in content.display_iter {
                let r = indexed.point.line.0 + display_offset as i32;
                if r < 0 || r as usize >= rows {
                    continue;
                }
                let r = r as usize;
                if indexed.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                if indexed.cell.flags.contains(Flags::WRAPLINE) {
                    wraps[r] = true;
                }
                let c = indexed.point.column.0;
                if c < cols {
                    grid[r][c] = if indexed.cell.c == '\0' {
                        ' '
                    } else {
                        indexed.cell.c
                    };
                }
            }
            stitch_wrapped_line(&grid, &wraps, vrow, vcol)
        };
        match link_at(&line, col)? {
            Link::Url(u) => Some(u),
            Link::Path(p) => {
                let cwd = self.runtime().cwd;
                resolve_path(&p, cwd.as_deref()).filter(|a| std::path::Path::new(a).exists())
            }
        }
    }

    /// While drag-selecting, the signed scroll rate (lines/tick) for cursor
    /// `pos`: positive = up into history (cursor at/above the top edge),
    /// negative = down toward live (at/below the bottom). 0 inside the safe
    /// band. The rate ramps up the further past the edge the cursor goes.
    fn autoscroll_rate(&self, pos: gpui::Point<Pixels>) -> f32 {
        let Some(b) = *self.content_bounds.lock().unwrap() else {
            return 0.0;
        };
        let top = f32::from(b.origin.y);
        let bottom = top + f32::from(b.size.height);
        let y = f32::from(pos.y);
        let band = self.cell_h.max(1.0); // arm within ~one row of an edge
        if y < top + band {
            (1.0 + (top + band - y).max(0.0) / self.cell_h)
                .ceil()
                .min(6.0)
        } else if y > bottom - band {
            -((1.0 + (y - (bottom - band)).max(0.0) / self.cell_h)
                .ceil()
                .min(6.0))
        } else {
            0.0
        }
    }

    /// Spin a ticker that scrolls the scrollback and drags the selection edge
    /// along with it, so a selection can run past the visible region while the
    /// cursor sits at (or beyond) an edge. Idempotent — only one loop runs; it
    /// exits when the drag ends or the cursor returns inside the band.
    fn ensure_autoscroll(&mut self, cx: &mut Context<Self>) {
        if self.autoscroll_running {
            return;
        }
        self.autoscroll_running = true;
        cx.spawn(async move |this, cx| loop {
            let keep = this
                .update(cx, |view: &mut TerminalView, cx| {
                    if !view.selecting || view.autoscroll == 0.0 {
                        view.autoscroll_running = false;
                        return false;
                    }
                    let lines = view.autoscroll.round() as i32;
                    if lines != 0 {
                        view.session
                            .term
                            .lock()
                            .scroll_display(Scroll::Delta(lines));
                        let offset = view.session.term.lock().grid().display_offset();
                        let (point, side) = view.cell_at(view.last_mouse, offset);
                        if let Some(sel) = view.session.term.lock().selection.as_mut() {
                            sel.update(point, side);
                        }
                        cx.notify();
                    }
                    true
                })
                .unwrap_or(false);
            if !keep {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(45))
                .await;
        })
        .detach();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        // F1 opens the help modal (handled by the workspace), never the PTY.
        if ks.key.as_str() == "f1" {
            cx.emit(OpenHelp);
            return;
        }
        // Escape closes the right-click menu before anything else.
        if self.ctx_menu.is_some() && ks.key.as_str() == "escape" {
            self.ctx_menu = None;
            cx.notify();
            return;
        }
        // Escape closes the ⋯ header overflow menu before reaching the PTY.
        if self.hdr_overflow.is_some() && ks.key.as_str() == "escape" {
            self.hdr_overflow = None;
            cx.notify();
            return;
        }
        // Escape closes the bell/sound config tray instead of reaching the PTY —
        // otherwise the ESC byte hits the running agent and kills it.
        if self.bell_menu && ks.key.as_str() == "escape" {
            self.bell_menu = false;
            cx.notify();
            return;
        }
        // While this pane is mirrored in the FOCUS modal, a plain Esc closes the
        // modal (the workspace handles it) rather than reaching the PTY — every
        // OTHER keystroke still flows straight to this terminal, so you keep
        // directing the agent while you read it big.
        if self.being_read && ks.key.as_str() == "escape" {
            cx.emit(CloseFocusRead);
            return;
        }
        // The inline rename box owns the keyboard while open — keystrokes edit
        // the name instead of reaching the PTY. Mirrors the main-tab rename.
        if let Some(mut buf) = self.renaming.take() {
            match ks.key.as_str() {
                "enter" => {
                    self.name = (!buf.trim().is_empty()).then(|| buf.trim().to_string());
                    cx.emit(PaneRenamed);
                }
                "escape" => {}
                "backspace" => {
                    buf.pop();
                    self.renaming = Some(buf);
                }
                _ => {
                    if let Some(ch) = ks.key_char.as_ref() {
                        if buf.chars().count() < 24 {
                            buf.push_str(ch);
                        }
                    }
                    self.renaming = Some(buf);
                }
            }
            cx.notify();
            return;
        }
        if self.exited || self.spawned.elapsed() < Duration::from_millis(150) {
            return;
        }
        let m = &ks.modifiers;
        // Ctrl+W closes the whole tab (always confirmed by the workspace). We
        // intercept it here so it never reaches the PTY as werase (^W) — the
        // workspace owns this chord, like new-tab/copy/paste below.
        if m.control && !m.shift && !m.alt && ks.key.as_str() == "w" {
            cx.emit(RequestCloseTab);
            return;
        }
        // Ctrl+X = CUT the selection: copy it, and when it's the trailing run on
        // the live input line, erase it there too (see `cut_selection`). Gated on
        // an actual selection so a bare Ctrl+X still reaches the shell as the
        // readline prefix key (C-x C-e, etc.).
        if m.control && !m.shift && !m.alt && ks.key.as_str() == "x" && self.has_selection() {
            self.cut_selection(cx);
            return;
        }
        // Ctrl+F = find in THIS pane; Ctrl+Shift+F = find across ALL panes. Both
        // open a workspace-owned find panel (so it can search siblings and centre
        // itself); intercepted here so the chord never reaches the PTY.
        if m.control && !m.alt && ks.key.as_str() == "f" {
            cx.emit(OpenFind { global: m.shift });
            return;
        }
        if m.control && m.shift {
            match ks.key.as_str() {
                // workspace chords: new tab
                "t" => return,
                "c" => {
                    self.copy_selection(cx);
                    return;
                }
                "v" => {
                    self.paste_clipboard(cx);
                    return;
                }
                "k" => {
                    self.clear_scrollback(cx);
                    return;
                }
                // Ctrl+Shift+A → agent-watch (MCP) panel; Ctrl+Shift+D → this
                // pane's DESIGN menu (theme); Ctrl+Shift+G → this pane's GAUGES
                // tray (display). The Shift guard keeps raw Ctrl+A/D/G (line-start
                // / EOF / BEL) reaching the PTY. The menus anchor at this pane's
                // top-right, under the header, where the icon click opens them.
                "a" => {
                    cx.emit(OpenAgentPanel);
                    return;
                }
                "d" => {
                    cx.emit(OpenThemeMenu {
                        at: self.header_anchor(),
                    });
                    return;
                }
                "g" => {
                    cx.emit(OpenDisplayMenu {
                        at: self.header_anchor(),
                    });
                    return;
                }
                _ => {}
            }
        }
        // Keyboard-driven visual selection: shift+←/→ extends TD's own selection
        // by a character, shift+ctrl+←/→ by a word — combinative (anchor fixed,
        // active end moves), seeded from the cursor or an existing mouse selection.
        // Shells don't bind shift-arrows, so this never steals shell word-nav
        // (plain ctrl+arrow still reaches the PTY) or ordinary typing. Works in the
        // FOCUS reader too (the mirror repaints the highlight via the pane notify).
        if m.shift && !m.alt && matches!(ks.key.as_str(), "left" | "right") {
            self.extend_kbd_selection(ks.key.as_str() == "right", m.control, cx);
            return;
        }
        if let Some(bytes) = keystroke_bytes(ks) {
            {
                let mut term = self.session.term.lock();
                term.selection = None;
                term.scroll_display(Scroll::Bottom);
            }
            // a real keystroke ends any keyboard selection in progress
            self.kbd_sel = None;
            self.pending_input = Some(Instant::now());
            self.session.notifier.notify(bytes);
            cx.notify();
        }
    }

    /// Grow TD's visual selection one step from the keyboard. `right` picks the
    /// direction; `word` jumps by a semantic word (else one cell). The anchor is
    /// fixed and only the active end moves, so repeated presses extend the same
    /// selection (combinative) instead of starting a new one. Seeds from any live
    /// keyboard selection, else an existing mouse selection's range, else the
    /// cursor. The highlight is rendered by the normal grid scan, so it shows in
    /// both the live pane and the FOCUS mirror.
    fn extend_kbd_selection(&mut self, right: bool, word: bool, cx: &mut Context<Self>) {
        let last_col = self.grid.cols.saturating_sub(1);
        // one cell in the requested direction, clamped to the row (no line-wrap:
        // command-line selection is single-row; mouse handles multi-line spans).
        let step = |p: TermPoint| -> TermPoint {
            if right {
                if p.column.0 < last_col {
                    TermPoint::new(p.line, Column(p.column.0 + 1))
                } else {
                    p
                }
            } else if p.column.0 > 0 {
                TermPoint::new(p.line, Column(p.column.0 - 1))
            } else {
                p
            }
        };
        let next = {
            let mut term = self.session.term.lock();
            let (anchor, active) = match self.kbd_sel {
                Some(ae) => ae,
                None => {
                    if let Some(r) = term.selection.as_ref().and_then(|s| s.to_range(&*term)) {
                        (r.start, r.end)
                    } else {
                        let c = term.renderable_content().cursor.point;
                        (c, c)
                    }
                }
            };
            let active = if word {
                let np = if right {
                    term.semantic_search_right(active)
                } else {
                    term.semantic_search_left(active)
                };
                if np == active {
                    // already on a word boundary — step one cell into the next word
                    let s = step(active);
                    if s == active {
                        active
                    } else if right {
                        term.semantic_search_right(s)
                    } else {
                        term.semantic_search_left(s)
                    }
                } else {
                    np
                }
            } else {
                step(active)
            };
            // anchor on the trailing edge, active on the leading edge, so the run
            // is inclusive in whichever direction it grew.
            let (a_side, e_side) = if active >= anchor {
                (Side::Left, Side::Right)
            } else {
                (Side::Right, Side::Left)
            };
            let mut sel = Selection::new(SelectionType::Simple, anchor, a_side);
            sel.update(active, e_side);
            term.selection = Some(sel);
            (anchor, active)
        };
        self.kbd_sel = Some(next);
        cx.notify();
    }

    fn on_wheel(&mut self, ev: &ScrollWheelEvent, _w: &mut Window, cx: &mut Context<Self>) {
        self.scroll_by_wheel(ev, cx);
    }

    /// Scroll the terminal scrollback from a wheel event. Public so the FOCUS
    /// reading modal (rendered by the Workspace) can route its wheel events here:
    /// the modal's locking scrim `.occlude()`s the pane behind it and would
    /// otherwise swallow the wheel, leaving the mirror un-scrollable.
    pub fn scroll_by_wheel(&mut self, ev: &ScrollWheelEvent, cx: &mut Context<Self>) {
        if ev.modifiers.control {
            return; // workspace handles ctrl+wheel = text-size scrub
        }
        let mut dy = match ev.delta {
            gpui::ScrollDelta::Lines(l) => l.y * 3.0,
            gpui::ScrollDelta::Pixels(p) => f32::from(p.y) / self.cell_h,
        };
        // In inverted (anchor-to-top) mode "older is DOWN", so a scroll-DOWN
        // gesture should reveal OLDER lines. Flip the sign so the wheel feels
        // natural; the default path is untouched.
        if self.paint_inverted {
            dy = -dy;
        }
        self.scroll_accum += dy;
        let lines = self.scroll_accum.trunc() as i32;
        if lines != 0 {
            self.scroll_accum -= lines as f32;
            self.session
                .term
                .lock()
                .scroll_display(Scroll::Delta(lines));
            cx.notify();
        }
    }

    /// Fuzzy-search this pane's grid (scrollback history + visible screen) for
    /// `needle` (already lowercased). Scans at most the most-recent `cap` lines so
    /// a deep buffer can't stall the per-keystroke search across many panes. Each
    /// line is built from column 0 (so a matched char index is also its column);
    /// blank lines are skipped. Returns the matches newest-last (grid order).
    pub fn search_grid(&self, needle: &str, cap: usize) -> Vec<GridHit> {
        if needle.is_empty() {
            return Vec::new();
        }
        let term = self.session.term.lock();
        let grid = term.grid();
        let cols = grid.columns();
        let bot = grid.bottommost_line().0;
        let start = (bot - cap as i32 + 1).max(grid.topmost_line().0);
        let mut hits = Vec::new();
        let mut buf = String::with_capacity(cols);
        for l in start..=bot {
            buf.clear();
            let row = &grid[Line(l)];
            for c in 0..cols {
                let ch = row[Column(c)].c;
                buf.push(if ch == '\0' { ' ' } else { ch });
            }
            let trimmed = buf.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            if let Some((score, positions)) = fuzzy_match(trimmed, needle) {
                hits.push(GridHit {
                    line: l,
                    text: trimmed.to_string(),
                    score,
                    positions,
                });
            }
        }
        hits
    }

    /// Like [`search_grid`](Self::search_grid) but EXACT (case-insensitive
    /// substring) — for the MCP `grep` tool, where an agent wants precise matches
    /// rather than the interactive fuzzy ranker. One hit per matching line, with
    /// the FIRST match's char range in `positions` (`score` unused). Bounded to
    /// the most-recent `cap` lines so a grep across a busy wall stays cheap.
    pub fn grep_grid(&self, needle: &str, cap: usize) -> Vec<GridHit> {
        if needle.is_empty() {
            return Vec::new();
        }
        let ndl: Vec<char> = needle.chars().collect();
        let term = self.session.term.lock();
        let grid = term.grid();
        let cols = grid.columns();
        let bot = grid.bottommost_line().0;
        let start = (bot - cap as i32 + 1).max(grid.topmost_line().0);
        let mut hits = Vec::new();
        let mut buf = String::with_capacity(cols);
        for l in start..=bot {
            buf.clear();
            let row = &grid[Line(l)];
            for c in 0..cols {
                let ch = row[Column(c)].c;
                buf.push(if ch == '\0' { ' ' } else { ch });
            }
            let trimmed = buf.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            let hay: Vec<char> = trimmed.chars().collect();
            if ndl.len() > hay.len() {
                continue;
            }
            let mut matched = None;
            'outer: for i in 0..=(hay.len() - ndl.len()) {
                for j in 0..ndl.len() {
                    if !hay[i + j].eq_ignore_ascii_case(&ndl[j]) {
                        continue 'outer;
                    }
                }
                matched = Some(i);
                break;
            }
            if let Some(i) = matched {
                hits.push(GridHit {
                    line: l,
                    text: trimmed.to_string(),
                    score: 0,
                    positions: (i..i + ndl.len()).collect(),
                });
            }
        }
        hits
    }

    /// Scroll this pane so grid line `line` sits at the top of the viewport, and
    /// (when `sel` is given) select that inclusive column span so a find-jump
    /// lands with the hit highlighted. Mirrors `scroll_to_human`'s offset math.
    pub fn scroll_to_line(
        &mut self,
        line: i32,
        sel: Option<(usize, usize)>,
        cx: &mut Context<Self>,
    ) {
        {
            let mut term = self.session.term.lock();
            let hist = term.grid().history_size() as i32;
            let off = (-line).clamp(0, hist);
            let cur = term.grid().display_offset() as i32;
            term.scroll_display(Scroll::Delta(off - cur));
            if let Some((lo, hi)) = sel {
                let a = TermPoint::new(Line(line), Column(lo));
                let b = TermPoint::new(Line(line), Column(hi));
                let mut s = Selection::new(SelectionType::Simple, a, Side::Left);
                s.update(b, Side::Right);
                term.selection = Some(s);
            }
        }
        cx.notify();
    }

    /// Part 1: grid-line indices (alacritty `Line.0`) of the user's own input
    /// lines across the full scrollback + screen, oldest→newest. Only the first
    /// columns are read (the prompt caret sits at the line start), so a scan is
    /// cheap even on deep history. Agent panes only — call sites gate on mode.
    fn human_line_indices(&self) -> Vec<i32> {
        let term = self.session.term.lock();
        let grid = term.grid();
        let cols = grid.columns().min(24); // prompt caret is near the start
        let mut out = Vec::new();
        for l in grid.topmost_line().0..=grid.bottommost_line().0 {
            let row = &grid[Line(l)];
            let mut s = String::with_capacity(cols);
            for c in 0..cols {
                let ch = row[Column(c)].c;
                s.push(if ch == '\0' { ' ' } else { ch });
            }
            if is_human_input_line(&s) {
                out.push(l);
            }
        }
        out
    }

    /// Part 1: jump the viewport to the previous (`next = false`) or next
    /// (`next = true`) of *your own* messages. The viewport top is grid line
    /// `-display_offset`; we step to the nearest human line above/below it and
    /// scroll so it lands at the top. Stepping past the newest snaps to live.
    /// Driven by the ▲/▼ header buttons and the `Alt+↑/↓` hotkeys (Workspace).
    pub fn scroll_to_human(&mut self, next: bool, cx: &mut Context<Self>) {
        // In the anchor-top INVERTED read the newest message sits at the TOP, so
        // the ▲/▼ (and Alt+↑/↓) directions flip: "up" steps toward NEWER, "down"
        // toward OLDER — the opposite of the default bottom-anchored read. The
        // overshoot snap-to-live still lands on the newest (rendered at top).
        let next = next ^ self.paint_inverted;
        let idx = self.human_line_indices();
        if idx.is_empty() {
            return;
        }
        let mut term = self.session.term.lock();
        let top = -(term.grid().display_offset() as i32);
        let target = if next {
            idx.iter().copied().filter(|&l| l > top).min()
        } else {
            idx.iter().copied().filter(|&l| l < top).max()
        };
        match target {
            Some(l) => {
                let hist = term.grid().history_size() as i32;
                let off = (-l).clamp(0, hist);
                let cur = term.grid().display_offset() as i32;
                term.scroll_display(Scroll::Delta(off - cur));
            }
            // Already at/below the newest message → snap to the live bottom.
            None if next => term.scroll_display(Scroll::Bottom),
            None => {}
        }
        drop(term);
        cx.notify();
    }

    /// Copy the current selection to the system clipboard (no-op if empty).
    fn copy_selection(&self, cx: &mut Context<Self>) {
        if let Some(text) = self.session.term.lock().selection_to_string() {
            if !text.is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                // Mirror to the X11 PRIMARY selection so middle-click paste works
                // in other apps. No-op on platforms without a primary selection.
                cx.write_to_primary(ClipboardItem::new_string(text));
            }
        }
    }
    /// Cut: copy the selection to the clipboard, then — only when it's safe to —
    /// delete it from the live shell input line. Scrollback is read-only, so the
    /// delete fires *only* when the selection sits on the on-screen input line
    /// (display at bottom) and ends right at the cursor, i.e. it's the run of
    /// characters immediately to the cursor's left. In that case `n` DELs erase
    /// exactly those cells (readline backspaces the chars before the cursor and
    /// shifts the tail left — a true cut). Anywhere else it's a plain copy, so a
    /// cut over history or a mid-line non-adjacent selection can never corrupt
    /// the buffer. Bound to Ctrl+X, and only when something is selected (so a bare
    /// Ctrl+X still reaches the shell as the readline prefix key).
    fn cut_selection(&mut self, cx: &mut Context<Self>) {
        let text = match self.session.term.lock().selection_to_string() {
            Some(t) if !t.is_empty() => t,
            _ => return,
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        cx.write_to_primary(ClipboardItem::new_string(text));
        // Decide whether the selection is the trailing run on the live input line.
        let erase = {
            let term = self.session.term.lock();
            let content = term.renderable_content();
            let cur = content.cursor.point;
            if content.display_offset != 0 {
                None
            } else {
                term.selection
                    .as_ref()
                    .and_then(|s| s.to_range(&*term))
                    // single row, on the cursor's row, ending immediately left of it
                    .filter(|r| {
                        r.start.line == r.end.line
                            && r.end.line == cur.line
                            && r.end.column.0 + 1 == cur.column.0
                    })
                    .map(|r| r.end.column.0 - r.start.column.0 + 1)
            }
        };
        if let Some(n) = erase {
            self.session.notifier.notify(vec![0x7f; n]); // n × DEL (erase char left)
            self.session.term.lock().selection = None;
            self.kbd_sel = None;
        }
        cx.notify();
    }
    /// Paste the clipboard into the PTY, honouring bracketed-paste mode.
    fn paste_clipboard(&self, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
            let bracketed = self
                .session
                .term
                .lock()
                .mode()
                .contains(TermMode::BRACKETED_PASTE);
            let bytes = if bracketed {
                [b"\x1b[200~", text.as_bytes(), b"\x1b[201~"].concat()
            } else {
                text.into_bytes()
            };
            self.session.notifier.notify(bytes);
        }
    }
    fn has_selection(&self) -> bool {
        self.session
            .term
            .lock()
            .selection_to_string()
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    }
    /// Clear the saved scrollback history. This is NOT the shell's Ctrl+L (which
    /// just clears the visible screen) — it drops the lines you scroll back to.
    fn clear_scrollback(&self, cx: &mut Context<Self>) {
        {
            let mut term = self.session.term.lock();
            term.grid_mut().clear_history();
            term.scroll_display(Scroll::Bottom);
        }
        cx.notify();
    }

    // ---- bell controls -------------------------------------------------------
    /// SNOOZE: silence the current sound and drop the bar (bell stays enabled).
    fn snooze_bell(&mut self, cx: &mut Context<Self>) {
        self.bell = false;
        self.bell_player.stop();
        self.not_thinking_since = None;
        cx.notify();
    }
    /// The always-visible bell toggle: mute/unmute this pane's bell and stop any
    /// sound that's ringing right now.
    fn toggle_bell_enabled(&mut self, cx: &mut Context<Self>) {
        self.bell_cfg.enabled = !self.bell_cfg.enabled;
        if !self.bell_cfg.enabled {
            self.bell = false;
            self.bell_player.stop();
        }
        cx.notify();
    }
    /// Choose this pane's sound; caches the clip length and resets the trim to a
    /// short opening sample (full tracks are minutes long — the scrubber widens it).
    fn set_bell_file(&mut self, file: Option<std::path::PathBuf>, cx: &mut Context<Self>) {
        self.bell_dur = file.as_deref().and_then(crate::bell::duration);
        self.bell_cfg.start = 0.0;
        // default to the first ~12s (or the whole clip if it's shorter)
        self.bell_cfg.end = self.bell_dur.map(|d| d.min(12.0)).unwrap_or(0.0);
        self.bell_cfg.file = file;
        cx.notify();
    }
    /// Preview the current clip once (ignores loop so the button can't run away).
    fn preview_bell(&mut self) {
        let mut c = self.bell_cfg.clone();
        c.looping = false;
        // a preview should always be audible even if the pane's bell is muted
        c.enabled = true;
        self.bell_player.play(&c);
    }

    /// "+ Add": open a native file picker (zenity), copy the chosen audio into
    /// the user sounds dir so it persists + shows up in the picker, then select
    /// it. The dialog runs off the UI thread so the window keeps painting.
    fn import_bell_file(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let picked = cx
                .background_executor()
                .spawn(async {
                    use std::process::Command;
                    let out = Command::new("zenity")
                        .args([
                            "--file-selection",
                            "--title=Add a notification sound",
                            "--file-filter=Audio | *.mp3 *.ogg *.oga *.wav *.flac *.m4a *.opus *.aac *.MP3 *.WAV *.FLAC *.OGG",
                            "--file-filter=All files | *",
                        ])
                        .output()
                        .ok()?;
                    if !out.status.success() {
                        return None; // user cancelled
                    }
                    let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    (!p.is_empty()).then(|| std::path::PathBuf::from(p))
                })
                .await;
            let Some(src) = picked else { return };
            // copy into the sounds dir (best-effort) so it persists across runs
            let dest = cx
                .background_executor()
                .spawn(async move {
                    let dir = crate::bell::sounds_dir();
                    let _ = std::fs::create_dir_all(&dir);
                    let name = src.file_name()?;
                    let dest = dir.join(name);
                    if dest != src {
                        std::fs::copy(&src, &dest).ok()?;
                    }
                    Some(dest)
                })
                .await;
            if let Some(dest) = dest {
                let _ = this.update(cx, |view, cx| {
                    view.set_bell_file(Some(dest), cx);
                });
            }
        })
        .detach();
    }

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if std::env::var("TD_KEYDEBUG").is_ok() {
            eprintln!("pane mousedown at {:?}", ev.position);
        }
        // Clicking into a terminal makes it the focused leaf, so keystrokes and
        // the split buttons (which target the focused pane) follow the pane the
        // user is actually working in — not whichever pane happened to start focused.
        window.focus(&self.focus_handle, cx);
        // A ringing "agent finished" alert is acknowledged by clicking anywhere
        // in this pane: stop the sound and dismiss the card — no button to chase.
        // The click is consumed (it doesn't also start a selection) so it reads
        // purely as "dismiss". Left-click only; right-click still opens the menu.
        if self.bell && ev.button == MouseButton::Left {
            self.snooze_bell(cx);
            return;
        }
        // right-click → copy/paste context menu at the cursor
        if ev.button == MouseButton::Right {
            self.ctx_menu = Some(ev.position);
            cx.notify();
            return;
        }
        // any other click dismisses an open menu (then proceeds normally)
        if self.ctx_menu.take().is_some() {
            cx.notify();
        }
        if self.hdr_overflow.take().is_some() {
            cx.notify();
        }
        // Shift- or Ctrl-click opens a link/path under the cursor with the system
        // default tool, instead of starting a selection. A modified click that
        // isn't on a link falls through to normal selection behaviour.
        if (ev.modifiers.shift || ev.modifiers.control) && ev.button == MouseButton::Left {
            if let Some(target) = self.link_under(ev.position) {
                open_with_system(&target);
                cx.notify();
                return;
            }
        }
        let offset = self.session.term.lock().grid().display_offset();
        let (point, side) = self.cell_at(ev.position, offset);
        let ty = match ev.click_count {
            2 => SelectionType::Semantic,
            n if n >= 3 => SelectionType::Lines,
            _ => SelectionType::Simple,
        };
        self.session.term.lock().selection = Some(Selection::new(ty, point, side));
        // a fresh mouse selection supersedes any keyboard-extension anchor; the
        // next shift-arrow re-seeds from this new selection's range.
        self.kbd_sel = None;
        self.selecting = true;
        self.last_mouse = ev.position;
        self.autoscroll = 0.;
        cx.notify();
    }

    /// Map a window-space x to a clip time (seconds) over the bell trim track.
    fn bell_time_from_pos(&self, x: Pixels) -> Option<f32> {
        let b = (*self.bell_track_bounds.lock().unwrap())?;
        let dur = self.bell_dur?;
        let w = f32::from(b.size.width);
        if dur <= 0.0 || w <= 0.0 {
            return None;
        }
        let r = ((f32::from(x) - f32::from(b.origin.x)) / w).clamp(0.0, 1.0);
        Some(r * dur)
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, _w: &mut Window, cx: &mut Context<Self>) {
        // dragging a bell-trim pip: move start/end along the track, keep a gap
        if let Some(is_end) = self.bell_drag {
            if ev.pressed_button == Some(MouseButton::Left) {
                if let Some(t) = self.bell_time_from_pos(ev.position.x) {
                    let dur = self.bell_dur.unwrap_or(t);
                    let v = trim_drag_value(is_end, t, self.bell_cfg.start, self.bell_cfg.end, dur);
                    if is_end {
                        self.bell_cfg.end = v;
                    } else {
                        self.bell_cfg.start = v;
                    }
                    cx.notify();
                }
            }
            return;
        }
        if !self.selecting || ev.pressed_button != Some(MouseButton::Left) {
            return;
        }
        self.last_mouse = ev.position;
        let offset = self.session.term.lock().grid().display_offset();
        let (point, side) = self.cell_at(ev.position, offset);
        if let Some(sel) = self.session.term.lock().selection.as_mut() {
            sel.update(point, side);
        }
        // dragging to/over an edge arms the auto-scroll ticker (which keeps
        // scrolling even if the cursor then holds still at the edge).
        self.autoscroll = self.autoscroll_rate(ev.position);
        if self.autoscroll != 0.0 {
            self.ensure_autoscroll(cx);
        }
        cx.notify();
    }

    fn on_mouse_up(&mut self, _ev: &MouseUpEvent, _w: &mut Window, cx: &mut Context<Self>) {
        if self.bell_drag.take().is_some() {
            cx.notify();
            return;
        }
        self.selecting = false;
        self.autoscroll = 0.;
        // Finishing a drag publishes the selection to the X11 PRIMARY selection
        // (classic select-to-copy → middle-click paste). Empty selections (plain
        // clicks) are skipped; no-op on platforms without a primary selection.
        if let Some(text) = self.session.term.lock().selection_to_string() {
            if !text.is_empty() {
                cx.write_to_primary(ClipboardItem::new_string(text));
            }
        }
    }

    /// Snapshot the viewport into one styled line per row.
    fn styled_lines(&self, th: &Theme) -> Vec<(String, Vec<TextRun>)> {
        let term = self.session.term.lock();
        let content = term.renderable_content();
        let display_offset = content.display_offset;
        let selection = content.selection;
        let cursor = content.cursor;
        let show_cursor = content.mode.contains(TermMode::SHOW_CURSOR) && display_offset == 0;

        // Each row fills toward `cols` chars and at most `cols` style runs, so
        // size both buffers up front — a row never reallocates mid-paint.
        let cols = self.grid.cols;
        let mut lines: Vec<(String, Vec<TextRun>)> = (0..self.grid.rows)
            .map(|_| (String::with_capacity(cols), Vec::with_capacity(cols)))
            .collect();

        // The `syntax` overlay tokenises the literal text, so it needs each full
        // row up front. Collect the cells once, build per-row colour palettes,
        // then paint cell-by-cell with a per-row cursor that stays in lock-step
        // with pass one (identical row-clamp + spacer skip ⇒ ordinals line up).
        let cells: Vec<_> = content.display_iter.collect();
        let syntax = th.syntax;
        // In an agent (claude/codex) pane, the user's own input lines are painted
        // in `th.human` so they stand out from the agent's replies (Part 2).
        let agent = self.mode.is_agent();
        // Build per-row literal text once if either the syntax overlay or the
        // human-input highlighting needs it.
        let rows_text: Vec<String> = if syntax || agent {
            let mut rows_text = vec![String::new(); self.grid.rows];
            for indexed in &cells {
                let row = indexed.point.line.0 + display_offset as i32;
                if row < 0 || row as usize >= self.grid.rows {
                    continue;
                }
                let cell = &indexed.cell;
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                rows_text[row as usize].push(if cell.c == '\0' { ' ' } else { cell.c });
            }
            rows_text
        } else {
            Vec::new()
        };
        let palettes: Vec<Vec<Hsla>> = if syntax {
            rows_text.iter().map(|t| syntax_colors(t, th)).collect()
        } else {
            Vec::new()
        };
        // Which rows are the user's own input (only computed in agent mode).
        // Span-aware: the whole wrapped message is marked, not just the caret row.
        let human_rows: Vec<bool> = if agent {
            human_input_rows(&rows_text)
        } else {
            Vec::new()
        };
        let mut ords = vec![0usize; self.grid.rows];
        // Hoist the frame-constant grade math out of the per-cell loop below.
        // `grade.apply(..)` is bit-identical to `graded(.., &th.grade, ..)` but
        // computes the gamma exponent and channel scalars once, not per cell.
        let grade = GradeCoeffs::new(&th.grade);

        for indexed in &cells {
            let row = indexed.point.line.0 + display_offset as i32;
            if row < 0 || row as usize >= self.grid.rows {
                continue;
            }
            let cell = &indexed.cell;
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            // Overlay rule: the token hue lands only on cells the program left
            // at its default foreground; anything the program explicitly
            // coloured still flows through `color_mode` (so ls/git/vim keep
            // their palette). The ord cursor advances on every non-spacer cell
            // regardless, keeping the palette aligned with the row text.
            let mut fg = if syntax {
                let ord = ords[row as usize];
                ords[row as usize] += 1;
                if matches!(cell.fg, AnsiColor::Named(NamedColor::Foreground)) {
                    palettes[row as usize].get(ord).copied().unwrap_or(th.text)
                } else {
                    ansi_to_hsla(cell.fg, th, th.text)
                }
            } else {
                ansi_to_hsla(cell.fg, th, th.text)
            };
            // Part 2: your own input in an agent session is recoloured to
            // `th.human` (whole-line), overriding syntax/ANSI so your turns pop.
            // Selection-inverse and the cursor below still apply on top.
            if agent && human_rows.get(row as usize).copied().unwrap_or(false) {
                fg = th.human;
            }
            let mut bg: Option<Hsla> = match cell.bg {
                AnsiColor::Named(NamedColor::Background) => None,
                other => Some(ansi_to_hsla(other, th, th.bg)),
            };
            let mut flags = cell.flags;
            if selection.is_some_and(|s| s.contains(indexed.point)) {
                flags.insert(Flags::INVERSE);
            }
            if flags.contains(Flags::INVERSE) {
                let new_fg = bg.unwrap_or(th.bg);
                bg = Some(fg);
                fg = new_fg;
            }
            // themed block cursor on top of everything
            if show_cursor
                && cursor.point.line == indexed.point.line
                && cursor.point.column == indexed.point.column
            {
                bg = Some(th.cursor);
                fg = th.bg;
            }
            if flags.contains(Flags::DIM) {
                fg.a *= 0.6;
            }

            // Monitor OSD: grade the final colours (text + background take their
            // own levels). Neutral grade is the identity, so the default render
            // is byte-for-byte unchanged.
            fg = grade.apply(fg, Channel::Text);
            bg = bg.map(|c| grade.apply(c, Channel::Bg));

            let weight = if flags.contains(Flags::BOLD) {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            };
            let underline = flags.contains(Flags::UNDERLINE).then(|| UnderlineStyle {
                thickness: px(1.),
                color: Some(fg),
                wavy: false,
            });

            let (text, runs) = &mut lines[row as usize];
            let ch = if cell.c == '\0' { ' ' } else { cell.c };
            let ch_len = ch.len_utf8();
            text.push(ch);

            let matches_last = runs.last().is_some_and(|r: &TextRun| {
                r.color == fg
                    && r.background_color == bg
                    && r.font.weight == weight
                    && r.underline.is_some() == underline.is_some()
            });
            if matches_last {
                runs.last_mut().unwrap().len += ch_len;
            } else {
                runs.push(TextRun {
                    len: ch_len,
                    font: grid_font(th, weight),
                    color: fg,
                    background_color: bg,
                    underline,
                    strikethrough: None,
                });
            }
        }
        lines
    }
}

/// Global "anchor terminal content to TOP" toggle. When `false` (the default)
/// panes hug their content to the BOTTOM of the pane via [`bottom_anchor_rows`];
/// when `true` that bottom pad is skipped, so the grid's naturally top-anchored
/// rows are left as-is and the prompt/typing area sits near the TOP of the pane
/// (easier on the neck on a tall monitor). This is a single GLOBAL setting, not
/// per-pane: the pane render can't reach `&Workspace`, so the workspace publishes
/// the live value into this process-global atomic each frame (mirrors
/// [`crate::warp::set_suppressed`] / [`crate::lang::set_current`]).
static ANCHOR_TOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Publish the live global anchor-to-top setting (called from `Workspace::render`
/// each frame, beside `lang::set_current` / `warp::set_suppressed`).
pub fn set_anchor_top(top: bool) {
    ANCHOR_TOP.store(top, std::sync::atomic::Ordering::Relaxed);
}

/// Read the global anchor-to-top setting. `true` ⇒ content hugs the TOP (skip the
/// bottom pad); `false` (default) ⇒ content hugs the BOTTOM.
pub fn anchor_top() -> bool {
    ANCHOR_TOP.load(std::sync::atomic::Ordering::Relaxed)
}

/// Bottom-anchor painted rows: slide content down until the last non-blank row
/// sits on the bottom (near) edge, with blank padding pushed to the top. This is
/// what makes a crawl pane read as a Star-Wars crawl — prompt at the near edge,
/// output stacking up into the distance — and, in normal mode, keeps a short
/// session's prompt hugging the bottom of the pane (the default). When the global
/// [`anchor_top`] toggle is on, callers skip this so content stays top-aligned.
/// Row count is preserved (layout height unchanged). No-op when the screen is
/// full (offset 0) or all-blank. Pure. Returns the `offset` it applied (0 when
/// no-op) so the render can record it for the hit-test inverse.
fn bottom_anchor_rows(lines: &mut Vec<(String, Vec<TextRun>)>, rows: usize) -> usize {
    let Some(last) = lines.iter().rposition(|(t, _)| !t.trim_end().is_empty()) else {
        return 0;
    };
    let offset = rows.saturating_sub(last + 1);
    if offset == 0 {
        return 0;
    }
    lines.truncate(last + 1); // drop the trailing blank rows we're re-adding on top
    let mut shifted: Vec<(String, Vec<TextRun>)> =
        std::iter::repeat_with(|| (String::new(), Vec::new()))
            .take(offset)
            .collect();
    shifted.append(lines); // moves content below the blank padding
    *lines = shifted;
    offset
}

/// Invert the per-frame paint transform (pure): a PAINTED/visual viewport row
/// `p` → the GRID viewport row `g` the renderer drew there, given the grid
/// `rows`, the `bottom_anchor_rows` `offset`, and whether the render `inverted`
/// (anchor-to-top: bottom-anchored THEN reversed). Result clamped to `0..rows-1`.
///   inverted: g = (rows-1 - p) - offset
///   else:     g = p - offset
/// With `offset == 0 && !inverted` this is the identity, so the un-anchored path
/// is byte-identical to before the feature. Split out so it's unit-testable
/// without a live `Pane`.
fn paint_row_to_grid_row_impl(p: usize, rows: usize, offset: usize, inverted: bool) -> usize {
    let last = rows.max(1) - 1;
    let g = if inverted {
        last.saturating_sub(p).saturating_sub(offset)
    } else {
        p.saturating_sub(offset)
    };
    g.min(last)
}

/// Anchor-to-top INVERTED read (pure): reverse the ORDER of logical groups so the
/// live input/prompt lands on top and older content flows down, while keeping each
/// group's rows in natural reading order. Two grouping modes:
///
/// - **`block_mode` (agent panes):** a group is a maximal run of consecutive
///   NON-BLANK rows — a whole message or the input BOX. Agents draw multi-row
///   input/output by cursor positioning (no soft-wrap flag), so line-level reverse
///   flips them bottom-to-top; block grouping keeps each box/message UPRIGHT and
///   lets the input grow DOWN as you type. Blanks separate the reversed blocks.
/// - **line mode (shells):** a group is a soft-wrapped logical line (WRAPLINE-
///   chained via `wraps`), so a wrapped line stays in order but each line reverses.
///
/// Returns the reordered lines + `perm`, where `perm[p]` is the grid viewport row
/// drawn at painted row `p` (the hit-test inverts via this). Row count preserved.
fn invert_logical_read(
    lines: Vec<(String, Vec<TextRun>)>,
    wraps: &[bool],
    block_mode: bool,
) -> (Vec<(String, Vec<TextRun>)>, Vec<usize>) {
    let n = lines.len();
    let is_blank: Vec<bool> = lines.iter().map(|(t, _)| t.trim_end().is_empty()).collect();
    let Some(last) = (0..n).rev().find(|&i| !is_blank[i]) else {
        return (lines, (0..n).collect()); // all blank → identity
    };
    // Build logical groups over the content rows 0..=last.
    let mut groups: Vec<Vec<usize>> = Vec::new();
    if block_mode {
        let mut i = 0;
        while i <= last {
            if is_blank[i] {
                i += 1;
                continue;
            }
            let start = i;
            while i <= last && !is_blank[i] {
                i += 1;
            }
            groups.push((start..i).collect());
        }
    } else {
        let mut cur: Vec<usize> = Vec::new();
        for i in 0..=last {
            cur.push(i);
            if !wraps.get(i).copied().unwrap_or(false) {
                groups.push(std::mem::take(&mut cur));
            }
        }
        if !cur.is_empty() {
            groups.push(cur);
        }
    }
    // Reverse group ORDER (newest on top), rows within a group natural. Blank rows
    // become padding: one separator between reversed blocks (block mode breathing
    // room), the remainder at the bottom so the input/prompt hugs the top.
    let mut blanks: Vec<usize> = (0..n).filter(|&i| is_blank[i]).collect();
    let mut perm: Vec<usize> = Vec::with_capacity(n);
    let g = groups.len();
    for (bi, grp) in groups.iter().rev().enumerate() {
        perm.extend(grp.iter().copied());
        if block_mode && bi + 1 < g {
            if let Some(b) = blanks.pop() {
                perm.push(b);
            }
        }
    }
    perm.extend(blanks); // remaining blanks pad the bottom
    while perm.len() < n {
        perm.push(perm.len().min(n - 1)); // safety; normally never hit
    }
    perm.truncate(n);
    let new_lines: Vec<(String, Vec<TextRun>)> = perm.iter().map(|&gi| lines[gi].clone()).collect();
    (new_lines, perm)
}

/// Font families installed on this system, captured once at startup so the grid
/// can fall back deliberately instead of letting gpui pick a silent substitute
/// (a past bug shipped DejaVu Sans without anyone noticing).
static AVAILABLE_FONTS: OnceLock<Vec<String>> = OnceLock::new();

/// Common monospace families to try, in order, when the requested one is absent.
const MONO_FALLBACKS: &[&str] = &[
    "JetBrains Mono",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Noto Sans Mono",
    "Source Code Pro",
    "Ubuntu Mono",
    "monospace",
];

/// Record the system's available font families. Call once at startup with
/// `cx.text_system().all_font_names()`.
pub fn init_font_registry(names: Vec<String>) {
    let _ = AVAILABLE_FONTS.set(names);
}

fn font_available(name: &str) -> bool {
    match AVAILABLE_FONTS.get() {
        Some(list) => list.iter().any(|n| n.eq_ignore_ascii_case(name)),
        // Registry not populated (e.g. unit tests) — assume present, don't rewrite.
        None => true,
    }
}

/// Resolve the requested family against what's actually installed, falling back
/// through a chain of common monospace families. Returns the family to request.
pub fn resolve_family(requested: &str) -> String {
    if font_available(requested) {
        return requested.to_string();
    }
    for fb in MONO_FALLBACKS {
        if !fb.eq_ignore_ascii_case(requested) && font_available(fb) {
            return (*fb).to_string();
        }
    }
    // Nothing matched; hand back the request and let gpui do its own fallback.
    requested.to_string()
}

/// Startup diagnostic: if the ship-default family isn't installed, describe the
/// fallback that will be used (so a silent substitution can never hide again).
/// Returns None when the default is present. Call after `init_font_registry`.
pub fn font_diagnostic() -> Option<String> {
    let want = "JetBrains Mono";
    let got = resolve_family(want);
    if got == want {
        return None;
    }
    let n = AVAILABLE_FONTS.get().map(|v| v.len()).unwrap_or(0);
    Some(format!(
        "font '{want}' not installed; falling back to '{got}' ({n} families available). \
         Install JetBrains Mono for the intended look."
    ))
}

/// Family names handed to gpui as PER-GLYPH fallbacks so scripts the primary mono
/// font lacks — CJK ideographs (中文), kana/kanji (日本語), hangul (한글), and
/// Devanagari (हिन्दी) — render real glyphs instead of tofu (□) boxes. gpui's
/// cosmic-text system tries these in order for any glyph missing from the grid
/// font; the Latin path is untouched (a fallback only fires on a miss, so the
/// default look never changes). Ordered mono-first to keep grid metrics closest.
const SCRIPT_FALLBACKS: &[&str] = &[
    "Noto Sans Mono CJK SC",
    "Noto Sans Mono CJK JP",
    "Noto Sans Mono CJK KR",
    "Noto Sans CJK SC",
    "Noto Sans CJK JP",
    "Noto Sans Devanagari",
    "Noto Sans Mono",
];

/// The installed subset of [`SCRIPT_FALLBACKS`], built once into a gpui
/// `FontFallbacks`. Filtered to what's actually present (the same discipline as
/// [`resolve_family`]) so we never request an absent family. `None` when the box
/// has no non-Latin coverage at all — a missing glyph still tofus then, but
/// nothing regresses. Built lazily, after [`init_font_registry`] has run.
pub(crate) fn script_fallbacks() -> Option<gpui::FontFallbacks> {
    static FB: OnceLock<Option<gpui::FontFallbacks>> = OnceLock::new();
    FB.get_or_init(|| {
        let present: Vec<String> = SCRIPT_FALLBACKS
            .iter()
            .filter(|f| font_available(f))
            .map(|f| (*f).to_string())
            .collect();
        (!present.is_empty()).then(|| gpui::FontFallbacks::from_fonts(present))
    })
    .clone()
}

fn grid_font(th: &Theme, weight: FontWeight) -> Font {
    // Crawl mode swaps the whole grid to the bundled News-Gothic crawl font,
    // italic, for that iconic recede-into-the-distance look. The perspective
    // itself is the renderer's job (the tube's crawl warp); here we only change
    // the typeface. Lines are shaped as runs, so the proportional font lays out
    // correctly even though the grid advances per cell.
    let family = if th.crawl {
        resolve_family(crate::theme::CRAWL_FONT_FAMILY)
    } else {
        resolve_family(&th.font_family)
    };
    let mut f = font(family);
    f.weight = weight;
    // Per-glyph fallback so CJK / Devanagari content renders real glyphs instead
    // of tofu boxes; only fires for glyphs the primary mono font is missing.
    f.fallbacks = script_fallbacks();
    if th.crawl {
        f.style = FontStyle::Italic;
    }
    f
}

/// Crawl-mode row centring: alacritty fills each row to full width with blank
/// cells, so trim the trailing blanks (clamping the runs to match) and hand back
/// the visible content to be justify-centred. Returns `None` for a blank row.
/// Shared by the live pane and the FOCUS mirror so both centre identically.
pub(crate) fn crawl_centered_runs(
    text: String,
    runs: Vec<TextRun>,
) -> Option<(String, Vec<TextRun>)> {
    let keep = text.trim_end_matches(' ').len();
    if keep == 0 {
        return None;
    }
    let mut acc = 0usize;
    let mut cut = Vec::with_capacity(runs.len());
    for mut r in runs {
        if acc >= keep {
            break;
        }
        if acc + r.len > keep {
            r.len = keep - acc;
        }
        acc += r.len;
        cut.push(r);
    }
    Some((text[..keep].to_string(), cut))
}

/// One on-screen row of the wrapped FOCUS reader.
///
/// The reader NEVER scrolls sideways: every source grid row is soft-wrapped to
/// the panel's glyph width at the current zoom, so a long line stacks vertically
/// instead of running off the edge. Each `VisualRow` carries the styled slice to
/// paint plus the source coordinates it came from — `src_row` indexes the
/// mirror's grid rows and `src_col0` is the column in that row where this visual
/// row's first glyph sits — so a click on a wrapped row maps back to a real cell
/// for selection + copy.
pub struct VisualRow {
    pub text: String,
    pub runs: Vec<TextRun>,
    pub src_row: usize,
    pub src_col0: usize,
    /// Glyph count painted on this visual row (for hit-clamping a click).
    pub cols: usize,
}

/// Slice the styled runs covering bytes `[start, end)` out of `runs`, clamping the
/// two boundary runs. Mirrors the clamp idiom in [`crawl_centered_runs`].
fn slice_runs(runs: &[TextRun], start: usize, end: usize) -> Vec<TextRun> {
    let mut out = Vec::new();
    let mut acc = 0usize;
    for r in runs {
        let (r0, r1) = (acc, acc + r.len);
        acc = r1;
        if r1 <= start {
            continue;
        }
        if r0 >= end {
            break;
        }
        let (s, e) = (r0.max(start), r1.min(end));
        if e > s {
            let mut nr = r.clone();
            nr.len = e - s;
            out.push(nr);
        }
    }
    out
}

/// Soft-wrap one source grid row to at most `fit_cols` glyph columns, breaking at
/// the last space inside the window and hard-breaking an over-long token, so the
/// reader can never overflow horizontally. Trailing blank cells are trimmed; the
/// break space between wrapped rows is swallowed so a continuation never starts
/// with a stray space. A blank source row yields one empty visual row so
/// paragraph spacing survives. Pushes the resulting rows onto `out`.
fn wrap_source_row(
    src_row: usize,
    text: &str,
    runs: &[TextRun],
    fit_cols: usize,
    out: &mut Vec<VisualRow>,
) {
    let fit_cols = fit_cols.max(1);
    let keep = text.trim_end_matches(' ').len();
    let chars: Vec<(usize, char)> = text[..keep].char_indices().collect();
    let n = chars.len();
    if n == 0 {
        out.push(VisualRow {
            text: String::new(),
            runs: Vec::new(),
            src_row,
            src_col0: 0,
            cols: 0,
        });
        return;
    }
    let mut i = 0usize;
    while i < n {
        let mut end = (i + fit_cols).min(n);
        // Prefer a word boundary: break before the last space inside the window
        // (keeps words whole). With no space the hard cap stands, so an over-long
        // token still breaks at exactly `fit_cols` and never spills off the edge.
        if end < n {
            if let Some(sp) = (i + 1..=end).rev().find(|&k| chars[k].1 == ' ') {
                end = sp;
            }
        }
        let byte_start = chars[i].0;
        let byte_end = if end < n { chars[end].0 } else { keep };
        out.push(VisualRow {
            text: text[byte_start..byte_end].to_string(),
            runs: slice_runs(runs, byte_start, byte_end),
            src_row,
            src_col0: i,
            cols: end - i,
        });
        i = end;
        while i < n && chars[i].1 == ' ' {
            i += 1; // swallow the break space(s) so the next row's head is a glyph
        }
    }
}

/// Soft-wrap every mirror row to `fit_cols` glyph columns for the FOCUS reader.
/// The result is the exact set of on-screen rows, in order — so its length × the
/// line height is the precise content height (no measuring), and `(src_row,
/// src_col0)` lets a click on any wrapped row resolve to a real source cell.
pub fn wrap_focus_lines(lines: &[(String, Vec<TextRun>)], fit_cols: usize) -> Vec<VisualRow> {
    let mut out = Vec::new();
    for (r, (text, runs)) in lines.iter().enumerate() {
        wrap_source_row(r, text, runs, fit_cols, &mut out);
    }
    out
}

/// Paint a selection background over glyph columns `[from, to)` of a wrapped
/// row's styled runs, splitting the two boundary runs so ONLY the selected glyphs
/// are tinted (the surrounding text keeps its own styling). `from`/`to` are char
/// offsets into `text`. Used by the FOCUS reader to draw a click-drag selection.
pub fn highlight_runs(
    text: &str,
    runs: &[TextRun],
    from: usize,
    to: usize,
    bg: Hsla,
) -> Vec<TextRun> {
    if from >= to {
        return runs.to_vec();
    }
    // char offset → byte offset (clamped to the string end past the last glyph)
    let byte_of = |c: usize| {
        text.char_indices()
            .nth(c)
            .map(|(b, _)| b)
            .unwrap_or(text.len())
    };
    let (fb, tb) = (byte_of(from), byte_of(to));
    let mut out = Vec::with_capacity(runs.len() + 2);
    let mut acc = 0usize;
    for r in runs {
        let (r0, r1) = (acc, acc + r.len);
        acc = r1;
        // Split this run at the selection edges that fall inside it, then tint the
        // piece that lies within [fb, tb).
        let mut cuts = vec![r0, r1];
        if fb > r0 && fb < r1 {
            cuts.push(fb);
        }
        if tb > r0 && tb < r1 {
            cuts.push(tb);
        }
        cuts.sort_unstable();
        cuts.dedup();
        for w in cuts.windows(2) {
            let (s, e) = (w[0], w[1]);
            if e <= s {
                continue;
            }
            let mut nr = r.clone();
            nr.len = e - s;
            if s >= fb && e <= tb {
                nr.background_color = Some(bg);
            }
            out.push(nr);
        }
    }
    out
}

/// gpui Keystroke → PTY bytes.
fn keystroke_bytes(ks: &Keystroke) -> Option<Vec<u8>> {
    let m = &ks.modifiers;
    // Enter: plain submits (CR); shift/alt+enter sends a literal newline (LF) so
    // multi-line input in claude/codex inserts a line break instead of submitting.
    if ks.key.as_str() == "enter" && !m.control {
        return Some(if m.shift || m.alt {
            vec![b'\n']
        } else {
            vec![b'\r']
        });
    }
    if m.alt {
        // alt+arrows switch panes; ctrl+alt chords split — both owned by Workspace
        if matches!(ks.key.as_str(), "left" | "right" | "up" | "down") || m.control {
            return None;
        }
        // other alt+<char>: ESC prefix for readline (alt+b, alt+f, alt+.)
        let base = ks.key_char.as_ref().map(|s| s.as_bytes().to_vec())?;
        let mut out = vec![0x1b];
        out.extend(base);
        return Some(out);
    }
    if m.control && ks.key.chars().count() == 1 {
        let c = ks.key.chars().next().unwrap().to_ascii_lowercase();
        if c.is_ascii_lowercase() {
            return Some(vec![c as u8 - b'a' + 1]);
        }
    }
    if m.control && matches!(ks.key.as_str(), "pageup" | "pagedown") {
        return None; // workspace: tab switching
    }
    // Cursor & nav keys carry modifiers in xterm's CSI 1;<mod> form, so
    // ctrl+→/← skip by word, shift+→/← extend selection, etc. (alt+arrows are
    // workspace pane-focus chords and already returned None above.)
    if let Some(fin) = match ks.key.as_str() {
        "up" => Some(b'A'),
        "down" => Some(b'B'),
        "right" => Some(b'C'),
        "left" => Some(b'D'),
        "home" => Some(b'H'),
        "end" => Some(b'F'),
        _ => None,
    } {
        // xterm modifier code: 1 + shift(1) + alt(2) + ctrl(4)
        let code = 1 + u8::from(m.shift) + u8::from(m.alt) * 2 + u8::from(m.control) * 4;
        return Some(if code == 1 {
            vec![0x1b, b'[', fin]
        } else {
            format!("\x1b[1;{code}{}", fin as char).into_bytes()
        });
    }
    let seq: &[u8] = match ks.key.as_str() {
        "enter" => b"\r",
        "backspace" => &[0x7f],
        "tab" => b"\t",
        "escape" => &[0x1b],
        "pageup" => b"\x1b[5~",
        "pagedown" => b"\x1b[6~",
        "delete" => b"\x1b[3~",
        "space" => b" ",
        _ => return ks.key_char.as_ref().map(|s| s.as_bytes().to_vec()),
    };
    Some(seq.to_vec())
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let th = self.resolved_theme(cx);
        // right-click context menu (Copy / Paste / Open link), anchored at the cursor
        let ctx_menu_el = self.ctx_menu.map(|pos| {
            let link = self.link_under(pos);
            let has_sel = self.has_selection();
            let (acc, surf, txt, faint, ff) = (
                th.accent,
                th.surface,
                th.text,
                th.faint,
                th.font_family.clone(),
            );
            let row = |label: &str, lit: bool| {
                div()
                    .px(px(13.))
                    .py(px(5.))
                    .cursor_pointer()
                    .text_color(if lit { txt } else { faint })
                    .hover(move |s| s.bg(acc.alpha(0.22)))
                    .child(label.to_string())
            };
            let mut menu = div()
                .flex()
                .flex_col()
                .min_w(px(168.))
                .py(px(4.))
                .bg(surf)
                .border_1()
                .border_color(acc.alpha(0.55))
                .rounded(px(8.))
                .occlude()
                .text_size(px(13.))
                .font_family(ff)
                .shadow_md();
            if let Some(l) = link {
                menu = menu.child(row("Open link  ↗", true).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |v, _, _, cx| {
                        open_with_system(&l);
                        v.ctx_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ));
            }
            menu = menu
                .child(row("Copy", has_sel).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|v, _, _, cx| {
                        v.copy_selection(cx);
                        v.ctx_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ))
                .child(row("Paste", true).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|v, _, _, cx| {
                        v.paste_clipboard(cx);
                        v.ctx_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ))
                .child(row("Clear scrollback", true).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|v, _, _, cx| {
                        v.clear_scrollback(cx);
                        v.ctx_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ));
            deferred(anchored().position(pos).snap_to_window().child(menu))
        });
        // Menu-bar size rides the grade group: a pane uses its own scale when its
        // grade is detached, else the live outer (Mother) scale. This scrubber
        // sizes the HEADER (height + glyphs/icons), never the terminal grid.
        let scale = self
            .appearance
            .effective(&theme::outer_choice(cx))
            .grade
            .scale;
        self.sync_size(&th, window);
        // Warp curvature is PER-PANE (it rides the grade group): keep this pane's
        // hit-test coefficients in sync with its OWN resolved warp, so clicks land
        // correctly whether this pane is bent and its neighbour flat, or vice versa.
        self.warp_k = theme::warp_coeffs(th.warp);
        // edge-detected focus reporting (CSI I / CSI O) for apps that ask for it.
        // The bell intentionally persists until SNOOZE / bell-off (so you never
        // miss which agent finished while you were away).
        let focused_now = self.focus_handle(cx).is_focused(window);
        if focused_now != self.was_focused {
            self.was_focused = focused_now;
            if self
                .session
                .term
                .lock()
                .mode()
                .contains(TermMode::FOCUS_IN_OUT)
            {
                self.session.notifier.notify(if focused_now {
                    b"\x1b[I".to_vec()
                } else {
                    b"\x1b[O".to_vec()
                });
            }
        }
        let mut lines = self.styled_lines(&th);
        // Crawl mode reads as a Star-Wars crawl: the prompt belongs at the near
        // (bottom) edge with output stacking UP into the distance. The grid
        // paints top-anchored, so after a clear/Ctrl+L the prompt would land at
        // the far/small top instead. Bottom-anchor the painted rows: slide them
        // down until the last non-blank row hugs the bottom edge, with the blank
        // padding pushed to the (receding) top. The row count is unchanged, so
        // layout height is identical; a full screen (vim/less, all rows used)
        // gives offset 0 and is left exactly as-is. Visual only — the grid model,
        // PTY, and shell are untouched (so the perspective shader composes on top).
        // Crawl mode ALWAYS bottom-anchors (the prompt belongs at the near edge).
        // In normal mode, content hugs the bottom too UNLESS the global
        // anchor-to-top toggle is on — then we INVERT the read: bottom-anchor
        // first (push the prompt to the grid bottom) THEN reverse the rows, so
        // the prompt lands on TOP, recent output just under it, older output
        // flowing DOWN, blank padding at the bottom ("neck looks up a tiny bit").
        //
        // Record the transform on `self` so the hit-test (`cell_at` /
        // `link_under`) and wheel scrolling can invert it: painted row `p` shows
        //   inverted: g = (rows-1 - p) - offset
        //   else:     g = p - offset            (incl. crawl)
        // The default un-anchored path leaves `(0, false)` ⇒ identity, so it is
        // byte-identical to before this feature.
        let inverted = anchor_top() && !th.crawl;
        if th.crawl || !anchor_top() {
            // Bottom-anchor (crawl + default normal mode).
            self.paint_offset = bottom_anchor_rows(&mut lines, self.grid.rows);
            self.paint_inverted = false;
            self.paint_to_grid = None;
        } else if inverted {
            // anchor-to-top inverted read → the live input/prompt lands on top,
            // older content flows down. Agent panes group by message/box (so the
            // input box stays upright + grows DOWN as you type); shells reverse by
            // soft-wrapped logical line.
            let block_mode = self.mode.is_agent();
            let wraps = if block_mode {
                Vec::new()
            } else {
                self.row_wraps()
            };
            let (new_lines, perm) = invert_logical_read(lines, &wraps, block_mode);
            lines = new_lines;
            self.paint_to_grid = Some(perm);
            self.paint_inverted = true;
            self.paint_offset = 0;
        } else {
            self.paint_offset = 0;
            self.paint_inverted = false;
            self.paint_to_grid = None;
        }
        let ps = crate::lang::current().strings();
        let status = if self.bell {
            format!("● {}", ps.ph_done)
        } else if self.exited {
            ps.ph_exited.to_string()
        } else {
            ps.ph_live.to_string()
        };
        let grid_label = format!("{}×{}", self.grid.cols, self.grid.rows);
        let glow = th.glow;

        // ── Responsive header ────────────────────────────────────────────────
        // As the pane narrows, the right-side controls tuck into a ⋯ overflow
        // menu in priority order. The × (close) NEVER collapses; 👓 FOCUS is the
        // LAST to go. Driven by the measured content width (one frame stale —
        // imperceptible) so the header reflows live as panes split/resize.
        let pane_w = self
            .content_bounds
            .lock()
            .unwrap()
            .map(|b| f32::from(b.size.width))
            .unwrap_or(f32::MAX);
        let show_human = pane_w >= 470.; // 1st to hide: 👤 ▲▼ message-nav
        let show_eq = pane_w >= 410.; //    2nd: EQ / display
        let show_theme = pane_w >= 360.; //  3rd: 🎨 theme
        let show_bell = pane_w >= 310.; //   4th: 🔔 notifications
        let show_focus = pane_w >= 264.; //  5th & last: 👓 FOCUS
                                         // ⋯ shows only once something is actually tucked (👤-nav is agent-only).
        let overflow = !show_focus
            || !show_bell
            || !show_theme
            || !show_eq
            || (!show_human && self.mode.is_agent());

        // The ⋯ overflow menu lists exactly the controls hidden at this width, in
        // the same order they collapse. Mirrors the right-click menu's look.
        let overflow_el = self.hdr_overflow.map(|pos| {
            let (acc, surf, txt, human, ff) = (
                th.accent,
                th.surface,
                th.text,
                th.human,
                th.font_family.clone(),
            );
            let item = move |icon: &str, label: &str| {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px(px(12.))
                    .py(px(6.))
                    .cursor_pointer()
                    .text_color(txt)
                    .hover(move |s| s.bg(acc.alpha(0.22)))
                    .child(div().w(px(22.)).child(icon.to_string()))
                    .child(label.to_string())
            };
            let mut menu = div()
                .flex()
                .flex_col()
                .min_w(px(196.))
                .py(px(4.))
                .bg(surf)
                .border_1()
                .border_color(acc.alpha(0.55))
                .rounded(px(8.))
                .occlude()
                .text_size(px(13.))
                .font_family(ff)
                .shadow_md();
            // 👤 ▲▼ message-nav keeps its live steppers inline so you can step
            // repeatedly; this row does not dismiss the menu.
            if !show_human && self.mode.is_agent() {
                let step = |glyph: &'static str, next: bool, cx: &mut Context<Self>| {
                    div()
                        .px(px(7.))
                        .py(px(1.))
                        .rounded_sm()
                        .border_1()
                        .border_color(human.alpha(0.6))
                        .text_color(human)
                        .cursor_pointer()
                        .child(glyph)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, _ev: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                view.scroll_to_human(next, cx);
                            }),
                        )
                };
                menu = menu.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .px(px(12.))
                        .py(px(6.))
                        .child(div().w(px(22.)).child("👤"))
                        .child("Your messages")
                        .child(div().flex_1())
                        .child(step("▲", false, cx))
                        .child(step("▼", true, cx)),
                );
            }
            if !show_eq {
                menu = menu.child(item("📊", "Display").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |v, _ev: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        v.hdr_overflow = None;
                        cx.emit(OpenDisplayMenu { at: pos });
                        cx.notify();
                    }),
                ));
            }
            if !show_theme {
                menu = menu.child(item("🎨", "Theme").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |v, _ev: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        v.hdr_overflow = None;
                        cx.emit(OpenThemeMenu { at: pos });
                        cx.notify();
                    }),
                ));
            }
            if !show_bell {
                menu = menu.child(item("🔔", "Notifications").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |v, _ev: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        v.bell_dur = v.bell_cfg.file.as_deref().and_then(crate::bell::duration);
                        v.bell_menu = true;
                        v.hdr_overflow = None;
                        cx.notify();
                    }),
                ));
            }
            if !show_focus {
                menu = menu.child(item("👓", "Focus — read this pane").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |v, _ev: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        v.hdr_overflow = None;
                        cx.emit(OpenFocusRead);
                        cx.notify();
                    }),
                ));
            }
            deferred(anchored().position(pos).snap_to_window().child(menu))
        });

        // The sub-tab header is this pane's TITLE — painted in the theme's
        // complement (the wheel's `C` target; defaults to the accent's opposite
        // hue, or the active dynamic's complement). Lightness is floored so a
        // dark complement override stays legible on the header.
        let bar_fg = Hsla {
            l: th.complement.l.clamp(0.5, 0.92),
            ..th.complement
        };

        // The global text-size scrubber now drives the MENU BAR: the bar height,
        // its glyphs/icons, and its title text all scale by `scale` together, so
        // the whole header grows/shrinks smoothly as one piece. (0.7..1.6 → a
        // 28..64px tall bar.)
        let header_h = HEADER_H * scale;
        let hicon = HICON * scale;
        let hpad = px(12. * scale); // header horizontal padding / control gap

        // solid, reflective header: gradient face + crisp top reflection line
        let mut lighter = th.surface;
        lighter.l = (lighter.l * 1.9).min(0.9);
        // a per-pane hover group so the ✎ affordance only reveals for THIS header
        let hdr_grp = gpui::SharedString::from(format!("pane-hdr-{}", cx.entity_id()));

        // Per-pane header LOGO, immediately left of the `▸ {label}` text. When a
        // logo is set we render it cover-cropped into a fixed square (a non-square
        // image fills + centre-crops via `.size_full()` inside an `.overflow_hidden()`
        // box). When none is set we show a dim, clickable `＋ logo` placeholder.
        // Either way a left-click emits `OpenLogoPicker` so the workspace opens the
        // image picker scoped to this pane. The square scales with the header.
        let logo_box = (header_h - 10. * scale).max(12.);
        let logo_el = {
            let base = div()
                .flex_none()
                .h(px(logo_box))
                .flex()
                .flex_row()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_view, _ev: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        cx.emit(OpenLogoPicker);
                    }),
                );
            if let Some(path) = self.logo.clone() {
                base.w(px(logo_box))
                    .overflow_hidden()
                    .rounded(px(4. * scale))
                    .border_1()
                    .border_color(th.accent.alpha(0.35))
                    .child(
                        gpui::img(std::path::PathBuf::from(path))
                            .size_full()
                            .object_fit(gpui::ObjectFit::Cover),
                    )
                    .into_any_element()
            } else {
                // Dim, tasteful placeholder: a `＋` upload glyph + tiny label that
                // brightens on header hover (shares the per-pane hover group).
                base.gap_1()
                    .px(px(5. * scale))
                    .rounded(px(4. * scale))
                    .border_1()
                    .border_color(bar_fg.alpha(0.18))
                    .text_color(bar_fg.alpha(0.4))
                    .group_hover(hdr_grp.clone(), move |s| {
                        s.text_color(bar_fg.alpha(0.85))
                            .border_color(th.accent.alpha(0.5))
                    })
                    .child(div().text_size(px(13. * scale)).child("\u{ff0b}"))
                    .child(div().text_size(px(9.5 * scale)).child("logo"))
                    .into_any_element()
            }
        };

        let mut header = div()
            .group(hdr_grp.clone())
            .h(px(header_h))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px(hpad)
            .bg(linear_gradient(
                180.,
                linear_color_stop(lighter, 0.),
                linear_color_stop(th.surface, 1.),
            ))
            .border_b_1()
            .border_color(th.accent.alpha(0.5))
            .text_color(bar_fg)
            // the title / status / grid-label text scales with the bar
            .text_size(px(th.font_size * scale))
            .child(if let Some(buf) = self.renaming.clone() {
                // inline rename box: a left-click anywhere else commits via
                // focus loss is not wired, so enter/escape (in on_key) close it
                div()
                    .flex_1()
                    // min-width:0 lets the title actually shrink (a nowrap flex
                    // child keeps min-width:auto otherwise) — so it clips instead
                    // of shoving the controls (and the ×) off the right edge.
                    .min_w(px(0.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(logo_el)
                    .child(format!("▸ {} · {buf}", self.mode.label_i18n()))
                    .child(div().w(px(6.)).h(px(13.)).bg(th.cursor))
                    .into_any_element()
            } else {
                // the title doubles as the drag handle: grab it to move this
                // sub-tab onto another tab, or drop it on a pane to split there.
                // Right-click renames it (custom name wins over the OSC title).
                let label = self.name.clone().unwrap_or_else(|| self.title.clone());
                div()
                    .flex_1()
                    // min-width:0 lets the title actually shrink (a nowrap flex
                    // child keeps min-width:auto otherwise) — so it clips instead
                    // of shoving the controls (and the ×) off the right edge.
                    .min_w(px(0.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .child(logo_el)
                    .child(format!("▸ {} · {label}", self.mode.label_i18n()))
                    // hover-revealed ✎ affordance (invites the rename)
                    .child(
                        div()
                            .text_size(px(11. * scale))
                            .text_color(Hsla {
                                h: 0.,
                                s: 0.,
                                l: 0.,
                                a: 0.,
                            })
                            .group_hover(hdr_grp.clone(), move |s| s.text_color(bar_fg.alpha(0.85)))
                            .child("✎"),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, ev: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            if ev.click_count >= 2 {
                                // double-click to rename (the file-manager gesture)
                                view.renaming = Some(view.name.clone().unwrap_or_default());
                                window.focus(&view.focus_handle, cx);
                                cx.notify();
                            } else {
                                cx.emit(DragPaneStart { at: ev.position });
                            }
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|view, _ev: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            view.renaming = Some(view.name.clone().unwrap_or_default());
                            window.focus(&view.focus_handle, cx);
                            cx.notify();
                        }),
                    )
                    .into_any_element()
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    // the control cluster keeps its natural width — only the title
                    // (min-w:0) shrinks, so these controls never get squeezed off.
                    .flex_shrink_0()
                    // roomier spacing between the header glyphs — scales with the bar
                    .gap(hpad)
                    .child(grid_label)
                    // Part 1: only in an agent (claude/codex) pane — jump between
                    // *your own* messages. Coloured like your input (`th.human`).
                    // FIRST control to tuck into the ⋯ overflow as the pane narrows.
                    .when(show_human && self.mode.is_agent(), |row| {
                        // jump between YOUR messages: a 👤 bust groups the ▲/▼
                        // steppers into one unit so it reads as "your turns",
                        // not two stray arrows.
                        let step = |glyph: &'static str, next: bool, cx: &mut Context<Self>| {
                            div()
                                .px(px(2.))
                                .rounded_sm()
                                .cursor_pointer()
                                .child(glyph)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |view, _ev: &MouseDownEvent, _w, cx| {
                                        cx.stop_propagation();
                                        view.scroll_to_human(next, cx);
                                    }),
                                )
                        };
                        row.child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(1.))
                                .px_1()
                                .rounded_sm()
                                .border_1()
                                .border_color(th.human.alpha(0.6))
                                .text_color(th.human)
                                // the bust matches the consistent 2× glyph set
                                .child(
                                    div()
                                        .text_size(px(hicon))
                                        .line_height(px(hicon))
                                        .mr(px(1.))
                                        .child("👤"),
                                )
                                .child(step("▲", false, cx))
                                .child(step("▼", true, cx)),
                        )
                    })
                    // 👓 FOCUS: mirror just this pane, big, with the rest of the
                    // window dimmed back. The LAST control to collapse (kept the
                    // longest, per the tuck order) — only hides on the narrowest panes.
                    .when(show_focus, |row| {
                        row.child(
                            div()
                                .px_1()
                                .rounded_sm()
                                .border_1()
                                .border_color(th.accent.alpha(0.5))
                                .cursor_pointer()
                                // the FOCUS lens reads +50% over the other 2× glyphs
                                .text_size(px(hicon * 1.5))
                                .line_height(px(hicon * 1.5))
                                .child("👓")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|_, _ev: &MouseDownEvent, _w, cx| {
                                        cx.stop_propagation();
                                        cx.emit(OpenFocusRead);
                                    }),
                                ),
                        )
                    })
                    // theme: a consistent 🎨 (click for the theme breakout)
                    .when(show_theme, |row| {
                        row.child(
                            div()
                                .px_1()
                                .rounded_sm()
                                .border_1()
                                .border_color(th.accent.alpha(0.5))
                                .cursor_pointer()
                                .text_size(px(hicon))
                                .line_height(px(hicon))
                                .child("🎨")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|_, ev: &MouseDownEvent, _w, cx| {
                                        cx.stop_propagation();
                                        cx.emit(OpenThemeMenu { at: ev.position });
                                    }),
                                ),
                        )
                    })
                    // display: a consistent EQ-waveform (click for monitor-OSD)
                    .when(show_eq, |row| {
                        row.child(
                            div()
                                .px_1()
                                .flex()
                                .items_center()
                                .rounded_sm()
                                .border_1()
                                .border_color(th.accent.alpha(0.5))
                                .cursor_pointer()
                                .child(eq_icon(th.accent, scale))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|_, ev: &MouseDownEvent, _w, cx| {
                                        cx.stop_propagation();
                                        cx.emit(OpenDisplayMenu { at: ev.position });
                                    }),
                                ),
                        )
                    })
                    // notification bell 🔔 (click → config tray; the ENABLE
                    // toggle and trim live in there). Lights when ringing.
                    .when(show_bell, |row| {
                        row.child(
                            div()
                                .px_1()
                                .rounded_sm()
                                .border_1()
                                .border_color(if self.bell_cfg.enabled {
                                    th.accent.alpha(0.5)
                                } else {
                                    th.faint
                                })
                                .text_color(if self.bell {
                                    th.accent
                                } else if self.bell_cfg.enabled {
                                    bar_fg
                                } else {
                                    th.faint
                                })
                                .cursor_pointer()
                                .text_size(px(hicon))
                                .line_height(px(hicon))
                                .child("🔔")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|v, _: &MouseDownEvent, _w, cx| {
                                        cx.stop_propagation();
                                        v.bell_dur = v
                                            .bell_cfg
                                            .file
                                            .as_deref()
                                            .and_then(crate::bell::duration);
                                        v.bell_menu = true;
                                        cx.notify();
                                    }),
                                ),
                        )
                    })
                    // ⋯ overflow: appears once anything has been tucked away. Tap to
                    // open the menu of hidden controls (built above as overflow_el).
                    .when(overflow, |row| {
                        row.child(
                            div()
                                .px_1()
                                .rounded_sm()
                                .border_1()
                                .border_color(th.accent.alpha(0.5))
                                .cursor_pointer()
                                .text_size(px(hicon))
                                .line_height(px(hicon))
                                .child("⋯")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|v, ev: &MouseDownEvent, _w, cx| {
                                        cx.stop_propagation();
                                        v.hdr_overflow = if v.hdr_overflow.is_some() {
                                            None
                                        } else {
                                            Some(ev.position)
                                        };
                                        cx.notify();
                                    }),
                                ),
                        )
                    })
                    .child(status)
                    .child(
                        // close just this sub-tab (×): ends this pane's shell.
                        // Big, borderless, full-height — a generous click target;
                        // a soft hover tint stands in for the dropped border.
                        div()
                            .id("close-pane")
                            .h_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .px_4()
                            .rounded_md()
                            .text_color(bar_fg)
                            .cursor_pointer()
                            // much bigger than the other header glyphs
                            .text_size(px(hicon + 10.))
                            .line_height(px(hicon + 10.))
                            .hover(|s| s.bg(bar_fg.alpha(0.18)))
                            .child("×")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|_, _ev: &MouseDownEvent, _w, cx| {
                                    cx.stop_propagation();
                                    cx.emit(ClosePane);
                                }),
                            ),
                    ),
            );
        {
            let mut shadows = vec![
                // the reflection: bright inner top edge
                BoxShadow {
                    color: gpui::white().alpha(0.16),
                    offset: point(px(1.), px(1.)),
                    blur_radius: px(0.),
                    spread_radius: px(0.),
                    inset: true,
                },
            ];
            if glow > 0.001 {
                shadows.push(BoxShadow {
                    color: bar_fg.alpha(glow * 0.5),
                    offset: point(px(0.), px(1.)),
                    blur_radius: px(16.),
                    spread_radius: px(0.),
                    inset: false,
                });
            }
            header = header.shadow(shadows);
        }

        // ---- SNOOZE bar: spans the top of the pane while the bell is ringing ----
        let (acc, txt, faint) = (th.accent, th.text, th.faint);
        let ff = th.font_family.clone();
        // ---- "agent finished" alert: a flat, bordered card floating at the top-
        // centre of the pane (mirrors the HELP box look). There is NO button —
        // clicking anywhere in this terminal acknowledges it (see on_mouse_down):
        // the sound stops and the card disappears. The bell stays enabled for the
        // next turn; mute lives in the 🔔 tray.
        let agent_alert = self.bell.then(|| {
            let name = self
                .bell_cfg
                .file
                .as_deref()
                .map(crate::bell::display_name)
                .unwrap_or_else(|| "alert".to_string());
            // solid dark surface, same recipe as the HELP panel (surface · 0.45)
            let panel_bg = Hsla {
                l: th.surface.l * 0.45,
                ..th.surface
            };
            div()
                // a full-width, click-through positioning layer: the card is
                // centred horizontally and sits just below the header. Mouse
                // events fall through to the pane's own handler, which is what
                // acknowledges the bell.
                .absolute()
                .top(px(header_h + 16.))
                .left_0()
                .right_0()
                .flex()
                .flex_row()
                .justify_center()
                .child(
                    div()
                        .px_4()
                        .py(px(8.))
                        .rounded_lg()
                        .border_1()
                        .border_color(acc.alpha(0.7))
                        .bg(panel_bg)
                        .text_color(txt)
                        .text_size(px(12.))
                        .font_family(ff.clone())
                        .shadow(vec![BoxShadow {
                            color: Hsla {
                                h: 0.,
                                s: 0.,
                                l: 0.,
                                a: 0.55,
                            },
                            offset: point(px(0.), px(6.)),
                            blur_radius: px(22.),
                            spread_radius: px(0.),
                            inset: false,
                        }])
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_color(acc)
                                        .font_weight(FontWeight::BOLD)
                                        .child("♪ ▸"),
                                )
                                .child(
                                    div()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(th.complement)
                                        .child(format!("AGENT FINISHED · {name}")),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(10.5))
                                .text_color(txt.alpha(0.6))
                                .child("click anywhere in this terminal to acknowledge"),
                        ),
                )
        });

        // ---- BELL+ config tray: pick a sound, trim it, loop, volume, preview ----
        let bell_tray = self.bell_menu.then(|| {
            let cfg = self.bell_cfg.clone();
            let dur = self.bell_dur.unwrap_or(0.0);
            let s = cfg.start;
            let e = if cfg.end > cfg.start { cfg.end } else { dur };
            let mini = move |label: String| {
                div()
                    .px(px(7.))
                    .py(px(1.))
                    .rounded_sm()
                    .border_1()
                    .border_color(acc.alpha(0.55))
                    .text_color(txt)
                    .cursor_pointer()
                    .child(label)
            };
            // sound list (default alert + every file in the sounds dir)
            let mut list = div().flex().flex_col().gap_1().child(
                div()
                    .px_2()
                    .py(px(3.))
                    .rounded_sm()
                    .cursor_pointer()
                    .bg(if cfg.file.is_none() {
                        acc.alpha(0.25)
                    } else {
                        acc.alpha(0.0)
                    })
                    .text_color(txt)
                    .child("◦ default alert")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|v, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            v.set_bell_file(None, cx);
                        }),
                    ),
            );
            for path in crate::bell::list_sounds() {
                let sel = cfg.file.as_deref() == Some(path.as_path());
                let nm = crate::bell::display_name(&path);
                let p = path.clone();
                list = list.child(
                    div()
                        .px_2()
                        .py(px(3.))
                        .rounded_sm()
                        .cursor_pointer()
                        .bg(if sel { acc.alpha(0.25) } else { acc.alpha(0.0) })
                        .text_color(if sel { txt } else { txt.alpha(0.85) })
                        .child(format!("♪ {nm}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |v, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                v.set_bell_file(Some(p.clone()), cx);
                            }),
                        ),
                );
            }
            // visual trim track: highlight the [start,end] window over the clip
            let frac = |t: f32| {
                if dur > 0.0 {
                    (t / dur).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            };
            // dual-pip scrubber: drag the two pips; the highlighted span between
            // them is the slice of the clip that plays. (Pip drag is handled in
            // on_mouse_move via bell_track_bounds, captured by the canvas below.)
            let track_store = self.bell_track_bounds.clone();
            let pip_ring = th.surface;
            let pip = move |at: f32, is_end: bool, cx: &mut Context<Self>| {
                div()
                    .absolute()
                    .top(px(-5.))
                    .left(gpui::relative(at))
                    .ml(px(-8.))
                    .w(px(16.))
                    .h(px(16.))
                    .rounded_full()
                    .bg(acc)
                    .border_2()
                    .border_color(pip_ring)
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |v, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            v.bell_drag = Some(is_end);
                            cx.notify();
                        }),
                    )
            };
            let track = div()
                .relative()
                .w_full()
                .h(px(12.))
                .my(px(8.))
                .rounded_full()
                .bg(faint.alpha(0.35))
                .cursor_pointer()
                // click anywhere on the track → grab the nearer pip and start a drag
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|v, ev: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        if let Some(t) = v.bell_time_from_pos(ev.position.x) {
                            let dur = v.bell_dur.unwrap_or(t);
                            let end = if v.bell_cfg.end > v.bell_cfg.start {
                                v.bell_cfg.end
                            } else {
                                dur
                            };
                            // grab whichever pip is nearer the click, then drag it
                            let is_end = (t - v.bell_cfg.start).abs() > (t - end).abs();
                            v.bell_drag = Some(is_end);
                            let nv =
                                trim_drag_value(is_end, t, v.bell_cfg.start, v.bell_cfg.end, dur);
                            if is_end {
                                v.bell_cfg.end = nv;
                            } else {
                                v.bell_cfg.start = nv;
                            }
                            cx.notify();
                        }
                    }),
                )
                // the play-span between the pips
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .rounded_full()
                        .bg(acc.alpha(0.7))
                        .left(gpui::relative(frac(s)))
                        .w(gpui::relative((frac(e) - frac(s)).max(0.01))),
                )
                // invisible canvas just to record the track's window-space bounds
                .child(
                    canvas(
                        move |bounds, _, _| {
                            *track_store.lock().unwrap() = Some(bounds);
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .child(pip(frac(s), false, cx))
                .child(pip(frac(e), true, cx));
            let labeled = move |lbl: &'static str, val: String| {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(52.))
                            .text_color(faint)
                            .text_size(px(10.))
                            .child(lbl),
                    )
                    .child(
                        div()
                            .min_w(px(52.))
                            .text_color(txt)
                            .text_size(px(11.))
                            .child(val),
                    )
            };
            let panel = div()
                .w(px(330.))
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(acc.alpha(0.6))
                .bg(th.surface)
                .text_color(txt)
                .text_size(px(11.))
                .font_family(ff.clone())
                .flex()
                .flex_col()
                .gap_2()
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _: &MouseDownEvent, _w, cx| cx.stop_propagation()),
                )
                // the panel occludes the pane, so the trim-pip drag is driven from
                // here (move + release) rather than the pane root behind it
                .on_mouse_move(cx.listener(Self::on_mouse_move))
                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
                // header
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(acc)
                                .child("🔔 NOTIFICATIONS"),
                        )
                        .child(mini("done".into()).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|v, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                v.bell_menu = false;
                                cx.notify();
                            }),
                        )),
                )
                // ENABLE toggle — was the header ♪; styled like the display-config
                // "follow outer" toggle (filled when on)
                .child(
                    div()
                        .px_2()
                        .py(px(3.))
                        .rounded_sm()
                        .border_1()
                        .border_color(acc.alpha(0.6))
                        .bg(if cfg.enabled {
                            acc.alpha(0.22)
                        } else {
                            acc.alpha(0.0)
                        })
                        .text_color(if cfg.enabled { txt } else { faint })
                        .cursor_pointer()
                        .child(if cfg.enabled {
                            "🔔 notifications ON — ring on agent completion"
                        } else {
                            "🔕 notifications OFF — click to enable"
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|v, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                v.toggle_bell_enabled(cx);
                            }),
                        ),
                )
                .child(list)
                // bring your own: pick any audio file → copied into the sounds
                // dir and selected (full-width, dashed-feel "add" affordance)
                .child(
                    div()
                        .px_2()
                        .py(px(3.))
                        .rounded_sm()
                        .border_1()
                        .border_color(acc.alpha(0.5))
                        .text_color(acc)
                        .cursor_pointer()
                        .child("＋ Add audio file…")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|v, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                v.import_bell_file(cx);
                            }),
                        ),
                )
                // TRIM readout + the dual-pip scrubber (drag the pips; the lit
                // span between them is what plays)
                .child(labeled("TRIM", format!("{s:.1}s – {e:.1}s")))
                .child(track)
                // loop + volume + preview row
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            mini(if cfg.looping {
                                "↻ loop on".into()
                            } else {
                                "↻ loop off".into()
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|v, _: &MouseDownEvent, _w, cx| {
                                    cx.stop_propagation();
                                    v.bell_cfg.looping = !v.bell_cfg.looping;
                                    cx.notify();
                                }),
                            ),
                        )
                        .child(mini("vol −".into()).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|v, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                v.bell_cfg.volume = (v.bell_cfg.volume - 0.1).max(0.0);
                                cx.notify();
                            }),
                        ))
                        .child(
                            div()
                                .min_w(px(34.))
                                .text_color(txt)
                                .child(format!("{}%", (cfg.volume * 100.0).round() as i32)),
                        )
                        .child(mini("vol +".into()).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|v, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                v.bell_cfg.volume = (v.bell_cfg.volume + 0.1).min(1.5);
                                cx.notify();
                            }),
                        )),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .child(mini("▶ preview".into()).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|v, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                v.preview_bell();
                            }),
                        ))
                        .child(mini("■ stop".into()).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|v, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                v.bell_player.stop();
                            }),
                        )),
                );
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(th.bg.alpha(0.55))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|v, _: &MouseDownEvent, _w, cx| {
                        v.bell_menu = false;
                        cx.notify();
                    }),
                )
                .child(panel)
        });

        let jiggle = self.fx.jiggle_px;
        // 🎰 GAMBA reels — shown only on the gamba look while the agent thinks.
        let gamba_look = {
            let is_retro = matches!(
                self.appearance.effective(&theme::outer_choice(cx)).dynamic,
                theme::Dynamic::Retro
            );
            crate::gamba::look_active(&th, is_retro)
        };
        let gamba_overlay = gamba_look
            .then(|| crate::gamba::overlay(&self.gamba, &th))
            .flatten();
        // a win rumbles the whole terminal for 3s as the coins spill
        let (rumble_dx, rumble_dy) = if gamba_look {
            self.gamba.rumble_offset()
        } else {
            (0.0, 0.0)
        };
        let shake_y = jiggle + rumble_dy;
        // The frame the grid sits in: a 2%/4px breathing border plus a
        // curvature-proportional inset so the bottom prompt clears the barrel
        // overscan. Same `grid_pad` the fit + hit-test use, so they stay locked.
        let (grid_pad_x, grid_pad_y) = {
            let (w, h) = self
                .content_bounds
                .lock()
                .unwrap()
                .map(|b| (f32::from(b.size.width), f32::from(b.size.height)))
                .unwrap_or((0.0, 0.0));
            let (k1, k2) = theme::warp_coeffs(th.warp);
            grid_pad(w, h, k1, k2)
        };
        div()
            .track_focus(&self.focus_handle(cx))
            .on_key_down(cx.listener(Self::on_key))
            .on_scroll_wheel(cx.listener(Self::on_wheel))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .size_full()
            // Grade the base background too (not just cells): the DISPLAY brightness
            // / contrast / colour sliders dim the whole pane like a dimmer light —
            // crucially the flat/paper themes, whose bright background is the bulk of
            // what you see. Neutral grade short-circuits, so the default is unchanged.
            .bg(graded(th.bg, &th.grade, Channel::Bg))
            .relative()
            .flex()
            .flex_col()
            .font_family(th.font_family.clone())
            // Terminal grid renders at its native size — the scrubber no longer
            // touches it (it sizes the menu bar instead; see the header below).
            .text_size(px(th.font_size))
            .text_color(th.text)
            .pt(px(shake_y.max(0.)))
            .pb(px((-shake_y).max(0.)))
            .pl(px(rumble_dx.max(0.)))
            .pr(px((-rumble_dx).max(0.)))
            .child(header)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .overflow_hidden()
                    .child({
                        let store = self.content_bounds.clone();
                        let weak = cx.entity().downgrade();
                        div().absolute().inset_0().child(
                            canvas(
                                move |bounds, window, cx| {
                                    let sf = window.scale_factor();
                                    // Per-pane warp: this tube bends by THIS pane's
                                    // own resolved curvature (grade.warp → th.warp),
                                    // so a bent pane and a flat pane coexist and
                                    // hit-testing matches each tube's own shader k.
                                    let (k1, k2) = crate::theme::warp_coeffs(th.warp);
                                    // Per-pane crawl: this tube recedes by THIS
                                    // pane's own crawl perspective (grade.crawl →
                                    // th.crawl/angle/depth). Identity when off, so
                                    // a crawling pane and a plain pane coexist.
                                    let crawl = if th.crawl {
                                        let (a, d) = crate::theme::crawl_coeffs(
                                            th.crawl_angle,
                                            th.crawl_depth,
                                        );
                                        [1.0, a, d]
                                    } else {
                                        [0.0, 1.0, 1.0]
                                    };
                                    crate::warp::register_tube(
                                        [
                                            f32::from(bounds.origin.x) * sf,
                                            f32::from(bounds.origin.y) * sf,
                                            f32::from(bounds.size.width) * sf,
                                            f32::from(bounds.size.height) * sf,
                                        ],
                                        th.screen_glare,
                                        k1,
                                        k2,
                                        crawl,
                                    );
                                    let changed = {
                                        let mut slot = store.lock().unwrap();
                                        let changed = slot.is_none_or(|b| b != bounds);
                                        if changed {
                                            *slot = Some(bounds);
                                        }
                                        changed
                                    };
                                    if changed {
                                        let weak = weak.clone();
                                        cx.defer(move |cx| {
                                            let _ = weak.update(cx, |_, cx| cx.notify());
                                        });
                                    }
                                },
                                |_, _, _, _| {},
                            )
                            .size_full(),
                        )
                    })
                    .child(
                        div()
                            .px(px(grid_pad_x))
                            .py(px(grid_pad_y))
                            .flex()
                            .flex_col()
                            .children(lines.into_iter().map(|(text, runs)| {
                                // Crawl mode centres each row: alacritty fills a row
                                // to full width with blank cells, so we trim the
                                // trailing blanks (clamping the runs to match) and let
                                // the flex row justify-centre the remaining shaped
                                // text. gpui measures the real glyph run, so this
                                // centres correctly even in the proportional crawl
                                // font. The grid model is unchanged (visual only).
                                if th.crawl {
                                    return match crawl_centered_runs(text, runs) {
                                        Some((t, cut)) => div()
                                            .h(px(self.cell_h))
                                            .flex()
                                            .justify_center()
                                            .whitespace_nowrap()
                                            .child(StyledText::new(t).with_runs(cut)),
                                        None => div().h(px(self.cell_h)).whitespace_nowrap(),
                                    };
                                }
                                let line = div().h(px(self.cell_h)).whitespace_nowrap();
                                if text.is_empty() {
                                    line
                                } else {
                                    line.child(StyledText::new(text).with_runs(runs))
                                }
                            })),
                    ),
            )
            .when(std::env::var("TD_NOGLASS").is_err(), |el| {
                el.child(crt::glass(&th, &self.fx))
            })
            // raised bezel frame sits above the glass, framing the whole pane
            .when(th.bezel > 0.001, |el| el.child(crt::bezel(&th)))
            // 🎰 the slot reels ride above the bezel, below the bell modal
            .children(gamba_overlay)
            // the "agent finished" card floats above the content, top-centre
            .children(agent_alert)
            .children(ctx_menu_el)
            .children(overflow_el)
            .children(bell_tray)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A single styled run of `len` bytes (style irrelevant to wrap geometry).
    fn run(len: usize) -> TextRun {
        TextRun {
            len,
            font: font("monospace"),
            color: Hsla {
                h: 0.,
                s: 0.,
                l: 0.,
                a: 1.,
            },
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    #[test]
    fn wrap_breaks_at_words_and_trims_trailing_blanks() {
        // trailing grid blanks are trimmed; a line that fits stays one row
        let lines = vec![("abcdef     ".to_string(), vec![run(11)])];
        let rows = wrap_focus_lines(&lines, 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "abcdef");
        assert_eq!((rows[0].src_row, rows[0].src_col0, rows[0].cols), (0, 0, 6));

        // word-boundary wrap: the break space is swallowed, not carried over
        let lines = vec![("ab cd ef".to_string(), vec![run(8)])];
        let rows = wrap_focus_lines(&lines, 4);
        let got: Vec<(&str, usize)> = rows.iter().map(|r| (r.text.as_str(), r.src_col0)).collect();
        assert_eq!(got, vec![("ab", 0), ("cd", 3), ("ef", 6)]);
    }

    #[test]
    fn wrap_hard_breaks_long_tokens_and_never_overflows() {
        // a single unbreakable token longer than the width splits at the cap, so a
        // wrapped row can NEVER be wider than fit_cols (the reader never scrolls right)
        let fit = 5usize;
        let lines = vec![("0123456789abc".to_string(), vec![run(13)])];
        let rows = wrap_focus_lines(&lines, fit);
        assert_eq!(
            rows.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
            vec!["01234", "56789", "abc"]
        );
        assert!(
            rows.iter().all(|r| r.cols <= fit),
            "no row exceeds fit_cols"
        );
        // src columns are contiguous so a click on any row maps to the right cell
        assert_eq!(
            rows.iter().map(|r| r.src_col0).collect::<Vec<_>>(),
            vec![0, 5, 10]
        );

        // a blank source row survives as one empty visual row (paragraph spacing)
        let rows = wrap_focus_lines(&[("   ".to_string(), vec![run(3)])], 8);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cols, 0);
    }

    #[test]
    fn highlight_tints_only_the_selected_span() {
        // one 5-byte run "hello"; select glyphs [1,3) → three runs, middle tinted
        let bg = Hsla {
            h: 0.5,
            s: 0.5,
            l: 0.5,
            a: 0.3,
        };
        let out = highlight_runs("hello", &[run(5)], 1, 3, bg);
        assert_eq!(out.iter().map(|r| r.len).collect::<Vec<_>>(), vec![1, 2, 2]);
        assert_eq!(
            out.iter()
                .map(|r| r.background_color.is_some())
                .collect::<Vec<_>>(),
            vec![false, true, false]
        );
        // an empty selection is a no-op (returns the runs unchanged)
        let out = highlight_runs("hello", &[run(5)], 2, 2, bg);
        assert_eq!(out.len(), 1);
        assert!(out[0].background_color.is_none());
    }

    #[test]
    fn fuzzy_match_scores_ranks_and_locates() {
        // a contiguous substring outscores the same chars scattered
        let (sub, _) = fuzzy_match("the cargo build finished", "cargo").unwrap();
        let (scattered, _) = fuzzy_match("c-a-r-g-o spread out", "cargo").unwrap();
        assert!(
            sub > scattered,
            "contiguous run beats a scattered subsequence"
        );
        // case-insensitive; positions point at the matched chars (for highlight)
        let (_, pos) = fuzzy_match("Run CARGO now", "cargo").unwrap();
        assert_eq!(pos, vec![4, 5, 6, 7, 8]);
        // a word-start hit outscores a mid-word one
        let (start, _) = fuzzy_match("build run", "run").unwrap();
        let (mid, _) = fuzzy_match("overrunner", "run").unwrap();
        assert!(start > mid, "word-start match ranks above mid-word");
        // non-subsequence and empty needle never match
        assert!(fuzzy_match("hello world", "xyz").is_none());
        assert!(fuzzy_match("anything", "").is_none());
    }

    #[test]
    fn brightness_lights_the_screen_without_whitening_text() {
        use crate::theme::{Grade, GradeKey};
        // a bright, saturated phosphor-green text cell and a near-black screen.
        let text = Hsla {
            h: 0.33,
            s: 0.9,
            l: 0.78,
            a: 1.0,
        };
        let bg = Hsla {
            h: 0.33,
            s: 0.6,
            l: 0.06,
            a: 1.0,
        };

        // Brightness turned UP from neutral.
        let mut g = Grade::neutral();
        g.set(GradeKey::Brightness, 0.85);

        let t = graded(text, &g, Channel::Text);
        let b = graded(bg, &g, Channel::Bg);
        // text must NOT be pushed brighter (toward white) by brightness…
        assert!(
            t.l <= text.l + 1e-6,
            "brightness-up must not raise text lightness (got {} from {})",
            t.l,
            text.l
        );
        assert!(
            t.s > 0.5,
            "text keeps its colour, not bleached to grey/white"
        );
        // …while the screen field DOES brighten (it has the dark headroom).
        assert!(
            b.l > bg.l,
            "brightness-up lights the screen: {} > {}",
            b.l,
            bg.l
        );

        // Brightness turned DOWN still dims BOTH (the existing dimming behaviour).
        let mut d = Grade::neutral();
        d.set(GradeKey::Brightness, 0.2);
        assert!(
            graded(text, &d, Channel::Text).l < text.l,
            "dim still dims text"
        );
        assert!(
            graded(bg, &d, Channel::Bg).l < bg.l,
            "dim still dims the screen"
        );
    }

    #[test]
    fn trim_drag_clamps_and_keeps_a_gap() {
        // drag END: lands on t, but never below start+0.2, never above dur
        assert_eq!(trim_drag_value(true, 7.0, 2.0, 10.0, 12.0), 7.0);
        assert_eq!(trim_drag_value(true, 99.0, 2.0, 10.0, 12.0), 12.0); // clamp to dur
        assert_eq!(trim_drag_value(true, 1.0, 5.0, 10.0, 12.0), 5.2); // keep gap above start
                                                                      // drag START: lands on t, but never above end-0.2, never below 0
        assert_eq!(trim_drag_value(false, 3.0, 0.0, 10.0, 12.0), 3.0);
        assert_eq!(trim_drag_value(false, -5.0, 0.0, 10.0, 12.0), 0.0); // clamp to 0
        assert_eq!(trim_drag_value(false, 11.0, 0.0, 10.0, 12.0), 9.8); // keep gap below end
                                                                        // end<=start means "to the clip end" → start clamps against dur
        assert_eq!(trim_drag_value(false, 20.0, 0.0, 0.0, 12.0), 11.8);
    }

    #[test]
    fn warp_matches_the_shader_and_is_identity_when_flat() {
        // a flat pane (k = 0) maps screen→content 1:1 everywhere
        assert_eq!(warp_screen_to_content(0.3, 0.7, 0.0, 0.0), (0.3, 0.7));

        // the centre is a fixed point under any curvature
        let (cx, cy) = warp_screen_to_content(0.5, 0.5, 0.14, 0.06);
        assert!((cx - 0.5).abs() < 1e-6 && (cy - 0.5).abs() < 1e-6);

        // a known off-centre point, recomputed straight from the shader's own
        // `l2 = 0.5 + c*(1 + k1*r2 + k2*r2*r2)` (crt_pass.wgsl fs_crt)
        let (k1, k2) = (0.14, 0.06);
        let (sx, sy) = (0.85, 0.65);
        let (cu, cv) = (sx - 0.5, sy - 0.5);
        let r2 = cu * cu + cv * cv;
        let f = 1.0 + k1 * r2 + k2 * r2 * r2;
        let (gx, gy) = warp_screen_to_content(sx, sy, k1, k2);
        assert!((gx - (0.5 + cu * f)).abs() < 1e-6);
        assert!((gy - (0.5 + cv * f)).abs() < 1e-6);

        // curvature pushes the sampled content outward (so a click near the edge
        // resolves to a cell further from centre — matching what's drawn there)
        let (ex, _) = warp_screen_to_content(0.8, 0.5, 0.14, 0.06);
        assert!(ex > 0.8);
    }

    #[test]
    fn link_at_finds_urls_and_paths_and_trims_delimiters() {
        // a URL mid-line, clicked anywhere inside it
        let line = "see (https://example.com/x), and more";
        assert_eq!(
            link_at(line, 8),
            Some(Link::Url("https://example.com/x".into()))
        );
        // trailing sentence punctuation is peeled off
        assert_eq!(
            link_at("go to https://a.dev.", 10),
            Some(Link::Url("https://a.dev".into()))
        );
        // www. is promoted to https
        assert_eq!(
            link_at("visit www.brownfamilysports.com today", 8),
            Some(Link::Url("https://www.brownfamilysports.com".into()))
        );
        // absolute + ~ + relative paths are paths
        assert_eq!(
            link_at("open /home/user/notes.md now", 8),
            Some(Link::Path("/home/user/notes.md".into()))
        );
        assert_eq!(
            link_at("~/todo.md", 0),
            Some(Link::Path("~/todo.md".into()))
        );
        assert_eq!(
            link_at("./README.md", 2),
            Some(Link::Path("./README.md".into()))
        );
        // plain words and whitespace are not links
        assert_eq!(link_at("just some words", 5), None);
        assert_eq!(link_at("a b", 1), None); // the space
        assert_eq!(link_at("", 0), None);
    }

    #[test]
    fn stitch_wrapped_line_rejoins_a_url_split_across_rows() {
        // a narrow 8-col terminal; the URL fills row 0 (wraps) and spills into row 1
        let cols = 8;
        let pad = |s: &str| {
            let mut v: Vec<char> = s.chars().collect();
            v.resize(cols, ' ');
            v
        };
        let rows = vec![
            pad("https://"), // wraps into the next row (full width)
            pad("a.dev/x "), // tail of the URL, then padding
            pad("next    "),
        ];
        let wraps = vec![true, false, false];

        // click on the first row → stitched line + adjusted column find the whole URL
        let (line, col) = stitch_wrapped_line(&rows, &wraps, 0, 2);
        assert_eq!(line, "https://a.dev/x ");
        assert_eq!(col, 2);
        assert_eq!(
            link_at(&line, col),
            Some(Link::Url("https://a.dev/x".into()))
        );

        // click on the *continuation* row → walks up, same URL, column offset by cols
        let (line, col) = stitch_wrapped_line(&rows, &wraps, 1, 3);
        assert_eq!(line, "https://a.dev/x ");
        assert_eq!(col, cols + 3);
        assert_eq!(
            link_at(&line, col),
            Some(Link::Url("https://a.dev/x".into()))
        );

        // a non-wrapping row stitches to just itself
        let (line, col) = stitch_wrapped_line(&rows, &wraps, 2, 1);
        assert_eq!(line, "next    ");
        assert_eq!(col, 1);

        // empty grid is harmless
        assert_eq!(stitch_wrapped_line(&[], &[], 0, 4), (String::new(), 4));
    }

    #[test]
    fn bottom_anchor_rows_pushes_content_to_the_bottom() {
        let row = |s: &str| (s.to_string(), Vec::<TextRun>::new());
        let texts =
            |l: &[(String, Vec<TextRun>)]| l.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>();

        // cleared screen: just a prompt on row 0 → it slides to the bottom
        let mut lines = vec![row("$ "), row(""), row(""), row("")];
        bottom_anchor_rows(&mut lines, 4);
        assert_eq!(texts(&lines), vec!["", "", "", "$ "]);

        // partially filled: content hugs the bottom, blank padding on top
        let mut lines = vec![row("ls"), row("a b c"), row("$ "), row("")];
        bottom_anchor_rows(&mut lines, 4);
        assert_eq!(texts(&lines), vec!["", "ls", "a b c", "$ "]);

        // full screen (all rows used) is left exactly as-is (offset 0)
        let mut lines = vec![row("a"), row("b"), row("c"), row("d")];
        bottom_anchor_rows(&mut lines, 4);
        assert_eq!(texts(&lines), vec!["a", "b", "c", "d"]);

        // rows of only trailing spaces count as blank
        let mut lines = vec![row("$ "), row("   "), row("   ")];
        bottom_anchor_rows(&mut lines, 3);
        assert_eq!(texts(&lines), vec!["", "", "$ "]);

        // all-blank is a no-op (nothing to anchor)
        let mut lines = vec![row(""), row("")];
        bottom_anchor_rows(&mut lines, 2);
        assert_eq!(texts(&lines), vec!["", ""]);

        // row count is always preserved
        let mut lines = vec![row("x"), row(""), row(""), row(""), row("")];
        bottom_anchor_rows(&mut lines, 5);
        assert_eq!(lines.len(), 5);
        assert_eq!(texts(&lines).last().unwrap(), "x");
    }

    #[test]
    fn anchor_top_atomic_round_trips_and_gates_the_bottom_pad() {
        let row = |s: &str| (s.to_string(), Vec::<TextRun>::new());
        let texts =
            |l: &[(String, Vec<TextRun>)]| l.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>();

        // default is bottom-anchored (toggle off)
        assert!(!anchor_top(), "default anchors to the bottom");

        // toggle on: the global atomic publishes the live value …
        set_anchor_top(true);
        assert!(anchor_top(), "set_anchor_top(true) is observed");

        // … and the render gate (`th.crawl || !anchor_top()`) skips the bottom pad,
        // so a short session's content stays top-aligned where the grid put it.
        let mut lines = vec![row("$ "), row(""), row(""), row("")];
        let crawl = false;
        if crawl || !anchor_top() {
            bottom_anchor_rows(&mut lines, 4);
        }
        assert_eq!(
            texts(&lines),
            vec!["$ ", "", "", ""],
            "top-anchor leaves the prompt at the top"
        );

        // toggle back off: the same gate now bottom-anchors as before.
        set_anchor_top(false);
        assert!(!anchor_top(), "set_anchor_top(false) restores the default");
        let mut lines = vec![row("$ "), row(""), row(""), row("")];
        if crawl || !anchor_top() {
            bottom_anchor_rows(&mut lines, 4);
        }
        assert_eq!(
            texts(&lines),
            vec!["", "", "", "$ "],
            "bottom-anchor slides the prompt to the bottom"
        );
    }

    #[test]
    fn inverted_anchor_top_puts_the_prompt_on_top_with_older_descending() {
        let row = |s: &str| (s.to_string(), Vec::<TextRun>::new());
        let texts =
            |l: &[(String, Vec<TextRun>)]| l.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>();

        // Inverted read = bottom-anchor THEN reverse. A short session (prompt on
        // grid row 0) should end with the PROMPT at index 0 (top), recent output
        // just under it, older output descending, and the blank pad at the bottom.
        // grid order (top→bottom): ls, a b c, $ , <blank>
        let mut lines = vec![row("ls"), row("a b c"), row("$ "), row("")];
        let offset = bottom_anchor_rows(&mut lines, 4); // → ["", "ls", "a b c", "$ "]
        assert_eq!(offset, 1, "one blank row of bottom-anchor shift");
        lines.reverse(); // → ["$ ", "a b c", "ls", ""]
        assert_eq!(
            texts(&lines),
            vec!["$ ", "a b c", "ls", ""],
            "prompt on top, recent under it, older descending, blank at the bottom"
        );
        // the last non-blank (the prompt) lands at painted index 0
        assert_eq!(lines[0].0, "$ ", "prompt is the top painted row");

        // cleared screen: prompt alone on grid row 0 → bottom-anchor (offset 3)
        // then reverse puts the prompt at index 0 with blanks below it.
        let mut lines = vec![row("$ "), row(""), row(""), row("")];
        let offset = bottom_anchor_rows(&mut lines, 4); // → ["", "", "", "$ "]
        assert_eq!(offset, 3);
        lines.reverse(); // → ["$ ", "", "", ""]
        assert_eq!(texts(&lines), vec!["$ ", "", "", ""]);

        // a full screen (offset 0) just reverses: top↔bottom flip, no padding.
        let mut lines = vec![row("a"), row("b"), row("c"), row("d")];
        let offset = bottom_anchor_rows(&mut lines, 4);
        assert_eq!(offset, 0, "full screen has no bottom-anchor shift");
        lines.reverse();
        assert_eq!(texts(&lines), vec!["d", "c", "b", "a"]);
    }

    #[test]
    fn invert_logical_read_keeps_wrapped_lines_in_order() {
        let row = |s: &str| (s.to_string(), Vec::<TextRun>::new());
        let texts =
            |l: &[(String, Vec<TextRun>)]| l.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>();

        // grid order: row0 old output; rows1-2 a WRAPPED human prompt (row1 soft-
        // wraps into row2); row3 the live prompt; row4 trailing blank.
        let lines = vec![
            row("old output"),
            row("a long human"),
            row("message wrapped"),
            row("> live prompt"),
            row(""),
        ];
        let wraps = vec![false, true, false, false, false]; // row1 → row2 are one line
        let (out, perm) = invert_logical_read(lines, &wraps, false);

        // Logical lines reverse (prompt on top, older descending) BUT the wrapped
        // line's two rows stay in reading order — NOT flipped bottom-to-top.
        assert_eq!(
            texts(&out),
            vec![
                "> live prompt",
                "a long human",
                "message wrapped",
                "old output",
                "",
            ],
            "wrapped human prompt must read top-to-bottom, not reversed"
        );
        // perm maps painted→grid for the hit-test; the wrapped rows ascend (1,2).
        assert_eq!(perm, vec![3, 1, 2, 0, 4]);
        assert_eq!((perm[1], perm[2]), (1, 2), "wrapped rows keep grid order");

        // A non-wrapped screen still fully reverses by logical line.
        let lines = vec![row("a"), row("b"), row("c")];
        let (out, perm) = invert_logical_read(lines, &[false, false, false], false);
        assert_eq!(texts(&out), vec!["c", "b", "a"]);
        assert_eq!(perm, vec![2, 1, 0]);
    }

    #[test]
    fn invert_logical_read_block_mode_keeps_agent_input_box_upright() {
        let row = |s: &str| (s.to_string(), Vec::<TextRun>::new());
        let texts =
            |l: &[(String, Vec<TextRun>)]| l.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>();

        // An agent pane: an OUTPUT message (rows 0-1), a blank, then the live INPUT
        // box (rows 3-5, a 3-row box the agent drew by cursor positioning — NOT
        // soft-wrapped), then a trailing blank. Block mode must keep the input box
        // UPRIGHT and on top (so typing reads top→bottom), with the older message
        // below it — never flipping a box's rows bottom-to-top.
        let lines = vec![
            row("agent: first line"),  // 0  output block
            row("agent: second line"), // 1
            row(""),                   // 2  separator
            row("> a long prompt"),    // 3  input box, line 1
            row("that I am typing"),   // 4  input box, line 2 (grows DOWN)
            row("right now"),          // 5  input box, line 3 (cursor)
            row(""),                   // 6  trailing blank
        ];
        // block mode ignores `wraps`.
        let (out, perm) = invert_logical_read(lines, &[], true);
        let t = texts(&out);
        // input box on top, IN ORDER (not reversed), then a blank, then the older
        // output message in order, then bottom padding.
        assert_eq!(
            &t[0..3],
            &["> a long prompt", "that I am typing", "right now"]
        );
        assert!(
            t[3].is_empty(),
            "a blank separates the reversed blocks for breathing room"
        );
        assert_eq!(&t[4..6], &["agent: first line", "agent: second line"]);
        // hit-test perm: painted rows 0-2 map to grid rows 3-5 (the input box).
        assert_eq!(&perm[0..3], &[3usize, 4, 5]);
    }

    #[test]
    fn paint_row_to_grid_row_inverts_the_paint_transform() {
        // Default un-anchored path (offset 0, not inverted) is the identity:
        // every painted row maps to the same grid row → byte-identical to before.
        for p in 0..6 {
            assert_eq!(
                paint_row_to_grid_row_impl(p, 6, 0, false),
                p,
                "identity in the default path"
            );
        }

        // Bottom-anchored with offset>0 (normal-mode + crawl): g = p - offset.
        // This is the latent pre-existing-bug fix — selection now accounts for
        // the shift instead of being off by `offset`.
        let rows = 4;
        let offset = 1; // content shifted DOWN by one (one blank row on top)
                        // painted row 0 is the blank pad → clamps to grid row 0
        assert_eq!(paint_row_to_grid_row_impl(0, rows, offset, false), 0);
        // painted rows 1..3 map back to grid rows 0..2
        assert_eq!(paint_row_to_grid_row_impl(1, rows, offset, false), 0);
        assert_eq!(paint_row_to_grid_row_impl(2, rows, offset, false), 1);
        assert_eq!(paint_row_to_grid_row_impl(3, rows, offset, false), 2);

        // Inverted, offset 0: g = (rows-1) - p — a pure top↔bottom flip, and it
        // round-trips (applying it twice returns the original row).
        let rows = 5;
        for p in 0..rows {
            let g = paint_row_to_grid_row_impl(p, rows, 0, true);
            assert_eq!(g, rows - 1 - p, "inverted offset-0 flips the row");
            assert_eq!(
                paint_row_to_grid_row_impl(g, rows, 0, true),
                p,
                "the flip is its own inverse"
            );
        }

        // Inverted with offset>0: g = (rows-1 - p) - offset. Reproduces the
        // example from `inverted_anchor_top_puts_the_prompt_on_top_…`:
        //   grid rows 0..3 = [ls, a b c, $ , <blank>], offset 1, painted (after
        //   reverse) = [$ , a b c, ls, <pad>] at indices 0..3.
        let rows = 4;
        let offset = 1;
        // painted index 0 ($ ) is grid row 2 ($ )
        assert_eq!(paint_row_to_grid_row_impl(0, rows, offset, true), 2);
        // painted index 1 (a b c) is grid row 1
        assert_eq!(paint_row_to_grid_row_impl(1, rows, offset, true), 1);
        // painted index 2 (ls) is grid row 0
        assert_eq!(paint_row_to_grid_row_impl(2, rows, offset, true), 0);
        // painted index 3 (the blank pad) underflows → clamps to grid row 0
        assert_eq!(paint_row_to_grid_row_impl(3, rows, offset, true), 0);
    }

    #[test]
    fn resolve_path_expands_home_and_anchors_relatives() {
        // absolute passes through
        assert_eq!(
            resolve_path("/etc/hosts", None).as_deref(),
            Some("/etc/hosts")
        );
        // relative needs a cwd; without one it can't anchor
        assert_eq!(resolve_path("./x.md", None), None);
        assert_eq!(
            resolve_path("./x.md", Some("/home/user/proj")).as_deref(),
            Some("/home/user/proj/x.md")
        );
        // ~ expands against HOME
        let home = std::env::var("HOME").unwrap_or_default();
        if !home.is_empty() {
            assert_eq!(
                resolve_path("~/a.md", None).as_deref(),
                Some(format!("{home}/a.md").as_str())
            );
        }
    }

    #[test]
    fn classify_recognises_the_phosphor_quartet() {
        assert_eq!(
            PaneMode::classify("claude", "claude --resume"),
            PaneMode::Claude
        );
        assert_eq!(
            PaneMode::classify("node", "node /home/x/.local/bin/claude"),
            PaneMode::Claude
        );
        assert_eq!(PaneMode::classify("codex", ""), PaneMode::Codex);
        for remote in ["ssh", "mosh-client", "et", "telnet"] {
            assert_eq!(PaneMode::classify(remote, ""), PaneMode::Remote);
        }
        for sh in ["bash", "zsh", "fish", "sh", "dash", "nu"] {
            assert_eq!(PaneMode::classify(sh, ""), PaneMode::Shell);
        }
        assert_eq!(
            PaneMode::classify("htop\n", ""),
            PaneMode::Other("htop".into())
        );
    }

    #[test]
    fn is_agent_is_true_only_for_claude_and_codex() {
        assert!(PaneMode::Claude.is_agent());
        assert!(PaneMode::Codex.is_agent());
        assert!(!PaneMode::Shell.is_agent());
        assert!(!PaneMode::Remote.is_agent());
        assert!(!PaneMode::Other("vim".into()).is_agent());
    }

    #[test]
    fn human_input_line_detects_the_prompt_caret_only() {
        // the agent CLIs' human-turn carets, with leading indentation tolerated
        assert!(is_human_input_line("❯ hi there"));
        assert!(is_human_input_line("  ❯ tell me the weather"));
        assert!(is_human_input_line("> what is 2+2"));
        assert!(is_human_input_line("▌ codex-style prompt"));
        assert!(is_human_input_line("» fish-ish caret"));
        // a bare caret with nothing after still counts (the live empty input box)
        assert!(is_human_input_line("❯"));
        // NOT human input: the agent's replies, plain output, shell redirects
        assert!(!is_human_input_line(
            "● Hi Parker! What are you working on?"
        ));
        assert!(!is_human_input_line("Compiling aurora v0.3.0"));
        assert!(!is_human_input_line(">> heredoc body")); // doubled '>' is not a prompt
        assert!(!is_human_input_line("cat file > out.txt")); // '>' mid-line
        assert!(!is_human_input_line(""));
        assert!(!is_human_input_line("    "));
    }

    #[test]
    fn human_input_rows_span_the_whole_wrapped_message() {
        // A multi-line user turn: caret row + indented wrapped continuation,
        // then a blank row and the agent's column-0 reply.
        let rows: Vec<String> = [
            "> Great - all the work we had on deck",
            "  is done? Let's get a clean main",
            "  and stand up a CLA across the repos",
            "",
            "● Two things: clean up the git state,",
            "  and stand up a CLA across the OSS repos.",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let marks = human_input_rows(&rows);
        // caret row + both indented continuation rows are the human's turn
        assert_eq!(marks[0..3], [true, true, true]);
        // the blank row closes the turn; the agent's reply is NOT human —
        // including its own indented continuation row after the bullet.
        assert_eq!(marks[3..6], [false, false, false]);

        // A bare/empty caret (live input box) colours just that row.
        let live: Vec<String> = ["❯", ""].iter().map(|s| s.to_string()).collect();
        assert_eq!(human_input_rows(&live), [true, false]);
    }

    #[test]
    fn grid_pad_floors_then_scales_then_compensates_curvature() {
        // Flat + small → the 4px floor wins on both axes.
        assert_eq!(grid_pad(100.0, 100.0, 0.0, 0.0), (4.0, 4.0));
        // Flat + large → 2% of each axis (no overscan term).
        assert_eq!(grid_pad(1000.0, 800.0, 0.0, 0.0), (20.0, 16.0));
        // Curving a pane only ADDS inset (the prompt needs to clear the smear),
        // never removes it — and the inset tracks each axis independently.
        let (k1, k2) = crate::theme::warp_coeffs(crate::theme::WARP_DEFAULT);
        let (fx, fy) = grid_pad(1000.0, 800.0, 0.0, 0.0);
        let (cx, cy) = grid_pad(1000.0, 800.0, k1, k2);
        assert!(cx > fx && cy > fy, "house warp must widen the frame");
        // Symmetric source ⇒ the per-axis pad is purely a function of that axis
        // length (top==bottom, left==right framing reads even).
        let (sx, _) = grid_pad(640.0, 480.0, k1, k2);
        let (_, sy) = grid_pad(480.0, 640.0, k1, k2);
        assert!((sx - sy).abs() < 1e-3, "equal axis lengths ⇒ equal pad");
    }

    /// #88 regression guard: the click→cell hit-test is the EXACT inverse of the
    /// shader gather, including the bottom rows of a TALL pane (where the barrel
    /// bows hardest). We forward-map each cell's content centre to its screen
    /// position (the numerical inverse of `warp_screen_to_content`), then run the
    /// `viewport_cell` math on it and assert we recover the same (row, col). If
    /// this passes, any live drift is a PARAMETER mismatch (stale `warp_k`, a rect
    /// or cell-size disagreement), NOT the formula — so don't "fix" the formula.
    #[test]
    fn warp_hit_test_round_trips_even_at_the_bottom_of_a_tall_pane() {
        // forward map: content-norm (cx,cy) → screen-norm, inverting the radial
        // barrel scale r_c = r_s·(1 + k1·r_s² + k2·r_s⁴) by bisection.
        fn content_to_screen(cx: f32, cy: f32, k1: f32, k2: f32) -> (f32, f32) {
            let (dx, dy) = (cx - 0.5, cy - 0.5);
            let rc = (dx * dx + dy * dy).sqrt();
            if rc < 1e-9 {
                return (cx, cy);
            }
            let (mut lo, mut hi) = (0.0f32, 1.5f32);
            for _ in 0..80 {
                let m = 0.5 * (lo + hi);
                let f = m * (1.0 + k1 * m * m + k2 * m * m * m * m);
                if f < rc {
                    lo = m;
                } else {
                    hi = m;
                }
            }
            let rs = 0.5 * (lo + hi);
            let s = rs / rc;
            (0.5 + dx * s, 0.5 + dy * s)
        }
        let (k1, k2) = crate::theme::warp_coeffs(crate::theme::WARP_DEFAULT);
        // a deliberately TALL pane (the reported failure shape) + a square control.
        for &(bw, bh) in &[(420.0f32, 1400.0f32), (900.0, 520.0), (700.0, 700.0)] {
            let (cell_w, cell_h) = (9.0f32, 20.0f32);
            let (pad_x, pad_y) = grid_pad(bw, bh, k1, k2);
            let cols = (((bw - 2.0 * pad_x) / cell_w).floor() as usize).max(10);
            let rows = (((bh - 2.0 * pad_y) / cell_h).floor() as usize).max(3);
            for &row in &[0usize, rows / 2, rows - 2, rows - 1] {
                for &col in &[0usize, cols / 2, cols - 1] {
                    // where the renderer puts this cell's centre (content-norm)…
                    let cx = (pad_x + (col as f32 + 0.5) * cell_w) / bw;
                    let cy = (pad_y + (row as f32 + 0.5) * cell_h) / bh;
                    // …forward-warped to the screen pixel it's DISPLAYED at…
                    let (sx, sy) = content_to_screen(cx, cy, k1, k2);
                    // …then the viewport_cell math run on that screen pixel.
                    let (lx, ly) = warp_screen_to_content(sx, sy, k1, k2);
                    let rr = ((ly * bh - pad_y) / cell_h).max(0.0) as usize;
                    let cc = ((lx * bw - pad_x) / cell_w).max(0.0) as usize;
                    assert_eq!(
                        (rr.min(rows - 1), cc.min(cols - 1)),
                        (row, col),
                        "round-trip drift at pane {bw}x{bh} cell (r{row},c{col})"
                    );
                }
            }
        }
    }

    #[test]
    fn hue_fold_keeps_colours_inside_the_seed_arc() {
        // wrap01 stays in [0,1); signed_turn is the shortest signed distance.
        assert!((wrap01(1.25) - 0.25).abs() < 1e-6);
        assert!((wrap01(-0.25) - 0.75).abs() < 1e-6);
        assert!((signed_turn(0.9) - (-0.1)).abs() < 1e-6); // 0.9 turns ≈ -0.1
        assert!((signed_turn(0.1) - 0.1).abs() < 1e-6);

        // OnTheme fold (mirrors `shape`): the canonical terminal green lands
        // exactly on the seed, and the full wheel stays within ±ARC/2 of it.
        const ARC: f32 = 0.55;
        const GREEN: f32 = 1.0 / 3.0;
        let seed = 0.6_f32; // arbitrary seed hue
        let folded = |h: f32| wrap01(seed + signed_turn(h - GREEN) * ARC);
        assert!((folded(GREEN) - seed).abs() < 1e-6, "green pins to seed");
        for i in 0..360 {
            let h = i as f32 / 360.0;
            let d = signed_turn(folded(h) - seed).abs();
            assert!(d <= ARC / 2.0 + 1e-4, "hue {h} escaped the arc: {d}");
        }
    }

    #[test]
    fn default_foreground_is_the_text_colour_in_every_mode() {
        use crate::theme::ColorMode;
        // The collision fix: default-fg is the theme's text colour (the wheel's
        // `T` target) in EVERY mode — the mode axis governs program colour only,
        // so an explicit text colour reads in ansi/mono/theme alike.
        let mut th = crate::theme::parse(crate::theme::DEFAULT_THEME_TOML).unwrap();
        let fg = AnsiColor::Named(NamedColor::Foreground);
        for mode in [
            ColorMode::Default,
            ColorMode::Monochrome,
            ColorMode::OnTheme,
        ] {
            th.color_mode = mode;
            assert_eq!(ansi_to_hsla(fg, &th, th.text), th.text);
        }
        // a fresh `T` colour flows straight through, whatever the mode
        th.text = rgb(0xff8800).into();
        assert_eq!(ansi_to_hsla(fg, &th, th.text), th.text);
    }

    #[test]
    fn classify_line_tags_each_token_class() {
        // index a char by hand and assert its class
        let at = |line: &str, i: usize| classify_line(line)[i];
        let line = r#"git commit -m "fix 3" /etc/hosts # done"#;
        //            0123456789...
        assert_eq!(at(line, 0), Tok::Keyword); // "git"
        assert_eq!(at(line, 4), Tok::Word); // "commit"
        assert_eq!(at(line, 11), Tok::Flag); // "-m"
        let q = line.find('"').unwrap();
        assert_eq!(at(line, q), Tok::Str); // opening quote
        assert_eq!(at(line, line.find('3').unwrap()), Tok::Str); // inside the string
        assert_eq!(at(line, line.find("/etc").unwrap()), Tok::Path); // "/etc/hosts"
        assert_eq!(at(line, line.find('#').unwrap()), Tok::Comment); // to EOL
                                                                     // a bare number outside a string is a number; classification is 1:1
        let nums = classify_line("v = 1.5");
        assert_eq!(nums.len(), "v = 1.5".chars().count());
        assert_eq!(nums[2], Tok::Op); // '='
        assert_eq!(nums[4], Tok::Num); // '1'
    }

    #[test]
    fn grade_neutral_is_identity_and_channels_are_independent() {
        use crate::theme::{Grade, GradeKey};
        let c = Hsla {
            h: 0.33,
            s: 0.6,
            l: 0.5,
            a: 1.0,
        };
        // neutral grade leaves a colour untouched (the identity render path)
        let n = Grade::neutral();
        assert_eq!(graded(c, &n, Channel::Text), c);
        assert_eq!(graded(c, &n, Channel::Bg), c);

        // brightness lights the SCREEN: it raises the background channel, but must
        // NOT push text brighter (that bleaches the hue toward white — see
        // brightness_lights_the_screen_without_whitening_text). Below neutral it
        // still dims both.
        let mut up = Grade::neutral();
        up.set(GradeKey::Brightness, 0.75);
        assert!(
            graded(c, &up, Channel::Bg).l > c.l,
            "brightness lifts the screen"
        );
        assert!(
            graded(c, &up, Channel::Text).l <= c.l + 1e-6,
            "brightness must not brighten text toward white"
        );
        let mut down = Grade::neutral();
        down.set(GradeKey::Brightness, 0.25);
        assert!(graded(c, &down, Channel::Text).l < c.l);
        assert!(graded(c, &down, Channel::Bg).l < c.l);

        // colour = 0 desaturates to greyscale
        let mut grey = Grade::neutral();
        grey.set(GradeKey::Colour, 0.0);
        assert!(graded(c, &grey, Channel::Text).s.abs() < 1e-6);

        // text vs background are independent: the text slider moves fg only
        let mut text_only = Grade::neutral();
        text_only.set(GradeKey::Text, 0.8);
        assert!(
            graded(c, &text_only, Channel::Text).l > c.l,
            "text level lifts fg"
        );
        assert_eq!(
            graded(c, &text_only, Channel::Bg),
            c,
            "text level must not touch the background channel"
        );

        // contrast > 0.5 widens the spread around mid-grey (a bright cell brightens)
        let bright = Hsla { l: 0.7, ..c };
        let mut hi = Grade::neutral();
        hi.set(GradeKey::Contrast, 0.75);
        assert!(graded(bright, &hi, Channel::Text).l > bright.l);

        // results always stay in gamut
        let mut extreme = Grade::neutral();
        extreme.set(GradeKey::Brightness, 1.0);
        let g = graded(Hsla { l: 0.95, ..c }, &extreme, Channel::Text);
        assert!((0.0..=1.0).contains(&g.l) && (0.0..=1.0).contains(&g.s));
    }

    #[test]
    fn grade_coeffs_match_graded() {
        use crate::theme::{Grade, GradeKey};
        // `GradeCoeffs` is the per-frame fast path the paint loop uses; it MUST be
        // bit-for-bit identical to `graded`, the canonical per-cell reference.
        // Sweep neutral, the shipped house default, every paint channel pushed to
        // each extreme, a scale-only grade (non-neutral but identity math), and a
        // full mix — across a spread of colours and both channels — asserting
        // EXACT equality (no epsilon: same inputs ⇒ same `powf`/divisions ⇒ no ULP
        // drift). If this ever fails, the fast path diverged and must be fixed.
        let mut grades = vec![Grade::neutral(), Grade::default()];
        for key in [
            GradeKey::Brightness,
            GradeKey::Contrast,
            GradeKey::Colour,
            GradeKey::Text,
            GradeKey::Background,
            GradeKey::Gamma,
        ] {
            for v in [0.0_f32, 0.25, 0.75, 1.0] {
                let mut g = Grade::neutral();
                g.set(key, v);
                grades.push(g);
            }
        }
        // scale moves the `is_neutral` needle but not the paint math.
        let mut scale_only = Grade::neutral();
        scale_only.set(GradeKey::Scale, 1.3);
        grades.push(scale_only);
        // an all-channel mix
        let mut mix = Grade::neutral();
        mix.set(GradeKey::Brightness, 0.23);
        mix.set(GradeKey::Contrast, 0.77);
        mix.set(GradeKey::Colour, 0.41);
        mix.set(GradeKey::Text, 0.62);
        mix.set(GradeKey::Background, 0.18);
        mix.set(GradeKey::Gamma, 0.9);
        grades.push(mix);

        let colours = [
            Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.0,
                a: 1.0,
            },
            Hsla {
                h: 0.33,
                s: 1.0,
                l: 1.0,
                a: 1.0,
            },
            Hsla {
                h: 0.5,
                s: 0.5,
                l: 0.5,
                a: 0.8,
            },
            Hsla {
                h: 0.12,
                s: 0.9,
                l: 0.05,
                a: 1.0,
            },
            Hsla {
                h: 0.78,
                s: 0.3,
                l: 0.95,
                a: 0.5,
            },
            Hsla {
                h: 0.95,
                s: 0.66,
                l: 0.42,
                a: 1.0,
            },
        ];
        for g in &grades {
            let cc = GradeCoeffs::new(g);
            for &c in &colours {
                for ch in [Channel::Text, Channel::Bg] {
                    assert_eq!(
                        cc.apply(c, ch),
                        graded(c, g, ch),
                        "GradeCoeffs::apply must equal graded — grade {g:?}, colour {c:?}, {ch:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn keystroke_bytes_encodes_the_pty_protocol() {
        let bytes = |s: &str| keystroke_bytes(&Keystroke::parse(s).unwrap());
        assert_eq!(bytes("ctrl-c"), Some(vec![3]));
        assert_eq!(bytes("enter"), Some(b"\r".to_vec()));
        // shift/alt+enter = literal newline (line break) for claude/codex multiline
        assert_eq!(bytes("shift-enter"), Some(b"\n".to_vec()));
        assert_eq!(bytes("alt-enter"), Some(b"\n".to_vec()));
        assert_eq!(bytes("up"), Some(b"\x1b[A".to_vec()));
        assert_eq!(bytes("escape"), Some(vec![0x1b]));
        // ctrl+arrows skip by word (xterm CSI 1;5 form)
        assert_eq!(bytes("ctrl-right"), Some(b"\x1b[1;5C".to_vec()));
        assert_eq!(bytes("ctrl-left"), Some(b"\x1b[1;5D".to_vec()));
        // shift+arrows extend selection (CSI 1;2 form)
        assert_eq!(bytes("shift-right"), Some(b"\x1b[1;2C".to_vec()));
        // workspace-owned chords must NOT reach the shell
        assert_eq!(bytes("alt-left"), None);
        assert_eq!(bytes("ctrl-pageup"), None);
    }

    #[test]
    fn idx_color_cube_and_ramp_boundaries() {
        // the hand-rolled 256-colour table feeds every 256-colour TUI, and the
        // cube/ramp arithmetic is off-by-one-prone — pin the corners.
        let c = |hex: u32| Hsla::from(rgb(hex));
        // 0..16: the xterm base palette
        assert_eq!(idx_color(0), c(0x000000));
        assert_eq!(idx_color(7), c(0xe5e5e5));
        assert_eq!(idx_color(15), c(0xffffff));
        // 16..232: the 6x6x6 cube. 16 = black corner, 231 = white corner.
        assert_eq!(idx_color(16), c(0x000000));
        assert_eq!(idx_color(231), c(0xffffff));
        assert_eq!(idx_color(196), c(0xff0000), "cube pure red");
        assert_eq!(idx_color(17), c(0x00005f), "first non-zero cube level = 95");
        // 232..256: the greyscale ramp, v = 8 + 10*(i-232)
        assert_eq!(idx_color(232), c(0x080808), "ramp start");
        assert_eq!(idx_color(255), c(0xeeeeee), "ramp end");
    }

    #[test]
    fn shape_three_modes_and_grey_guard() {
        use crate::theme::ColorMode;
        let mut th = crate::theme::parse(crate::theme::DEFAULT_THEME_TOML).unwrap();
        let red = Hsla {
            h: 0.0,
            s: 0.9,
            l: 0.5,
            a: 1.0,
        };
        // Default: untouched, the honest xterm palette
        th.color_mode = ColorMode::Default;
        assert_eq!(shape(red, &th), red);
        // Monochrome: adopt the text hue+saturation, keep the source lightness
        th.color_mode = ColorMode::Monochrome;
        th.text = Hsla {
            h: 1.0 / 3.0,
            s: 0.8,
            l: 0.4,
            a: 1.0,
        };
        let m = shape(red, &th);
        assert!((m.h - th.text.h).abs() < 1e-6 && (m.s - th.text.s).abs() < 1e-6);
        assert!((m.l - red.l).abs() < 1e-6, "structure (lightness) survives");
        // OnTheme grey guard: a near-grey keeps its low saturation (only the hue
        // breathes the seed) instead of smearing toward the accent.
        th.color_mode = ColorMode::OnTheme;
        th.accent = Hsla {
            h: 0.6,
            s: 0.7,
            l: 0.5,
            a: 1.0,
        };
        let grey = Hsla {
            h: 0.0,
            s: 0.02,
            l: 0.5,
            a: 1.0,
        };
        let g = shape(grey, &th);
        assert!(
            (g.s - grey.s).abs() < 1e-6,
            "near-grey stays low-saturation"
        );
        assert!(
            (g.h - th.accent.h).abs() < 1e-6,
            "grey breathes the seed hue"
        );
    }

    #[test]
    fn mode_theme_per_mode_palette() {
        let base = crate::theme::parse(crate::theme::DEFAULT_THEME_TOML).unwrap();
        // Shell and Other are passthrough — no retint.
        assert_eq!(mode_theme(&base, &PaneMode::Shell).accent, base.accent);
        assert_eq!(
            mode_theme(&base, &PaneMode::Other("vim".into())).accent,
            base.accent
        );
        // Agent modes retint the tube and keep their identity invariants.
        for mode in [PaneMode::Claude, PaneMode::Codex, PaneMode::Remote] {
            let th = mode_theme(&base, &mode);
            assert_ne!(th.accent, base.accent, "{:?} retints the accent", mode);
            assert_eq!(th.ansi[7], th.text, "default-fg slot follows the mode text");
            assert!(th.bg.l < 0.1, "{:?} tube depths stay dark", mode);
        }
    }

    /// Role at the first char of the first occurrence of `needle`.
    fn role_at(line: &str, needle: &str, roles: &[Role]) -> Role {
        let idx = line.find(needle).expect("needle present");
        roles[line[..idx].chars().count()]
    }

    #[test]
    fn agentic_marks_callouts_tools_links_and_structure() {
        let l = "Recommendation: ship it";
        let r = classify_agentic(l);
        assert_eq!(role_at(l, "Recommendation", &r), Role::Primary);
        assert_eq!(role_at(l, "ship", &r), Role::Text); // body after the colon

        let l = "then Bash(ls) runs";
        assert_eq!(role_at(l, "Bash", &classify_agentic(l)), Role::Tertiary);

        let l = "see https://x.io/y and src/main.rs";
        let r = classify_agentic(l);
        assert_eq!(role_at(l, "https", &r), Role::Secondary);
        assert_eq!(role_at(l, "src/main.rs", &r), Role::Secondary);

        assert_eq!(classify_agentic("# Heading")[0], Role::Quaternary);
        assert_eq!(classify_agentic("1. first step")[0], Role::Muted);
    }

    #[test]
    fn logs_marks_levels_timestamps_and_numbers() {
        let l = "12:00:01 ERROR took 45ms at src/x.rs";
        let r = classify_logs(l);
        assert_eq!(role_at(l, "ERROR", &r), Role::Primary);
        assert_eq!(role_at(l, "12:00:01", &r), Role::Muted);
        assert_eq!(role_at(l, "45ms", &r), Role::Tertiary);
        assert_eq!(role_at(l, "src/x.rs", &r), Role::Quaternary);

        let l = "WARN low disk OK ready";
        let r = classify_logs(l);
        assert_eq!(role_at(l, "WARN", &r), Role::Tertiary);
        assert_eq!(role_at(l, "OK", &r), Role::Secondary);
    }

    #[test]
    fn markdown_marks_headings_spans_and_links() {
        assert_eq!(classify_markdown("## Title")[0], Role::Primary);
        assert_eq!(classify_markdown("> quoted")[0], Role::Muted);

        let l = "a **bold** and `code` and [t](u)";
        let r = classify_markdown(l);
        assert_eq!(role_at(l, "**bold**", &r), Role::Secondary);
        assert_eq!(role_at(l, "`code`", &r), Role::Tertiary);
        assert_eq!(role_at(l, "[t]", &r), Role::Secondary);
        assert_eq!(role_at(l, "(u)", &r), Role::Quaternary);
    }

    #[test]
    fn role_color_responds_to_program_color() {
        let base = crate::theme::parse(crate::theme::DEFAULT_THEME_TOML).unwrap();
        let mut mono = base.clone();
        mono.color_mode = crate::theme::ColorMode::Monochrome;
        assert_eq!(role_color(Role::Primary, &mono).h, mono.text.h); // shade of text hue
        let mut on = base.clone();
        on.color_mode = crate::theme::ColorMode::OnTheme;
        assert_eq!(role_color(Role::Secondary, &on), on.complement);
        assert_eq!(role_color(Role::Tertiary, &on), on.human);
        assert_eq!(role_color(Role::Text, &base), base.text); // mode-independent
    }

    #[test]
    fn syntax_colors_match_line_length_for_every_scheme() {
        let base = crate::theme::parse(crate::theme::DEFAULT_THEME_TOML).unwrap();
        let line = "Note: run Bash(ls) at 12:00 OK `x` **y** /a/b 3ms";
        for scheme in crate::theme::SyntaxScheme::ALL {
            let mut th = base.clone();
            th.syntax_scheme = scheme;
            assert_eq!(
                syntax_colors(line, &th).len(),
                line.chars().count(),
                "{scheme:?} must emit one colour per char"
            );
        }
    }
}
