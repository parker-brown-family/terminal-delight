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
                .child(div().w(px(3.5 * s)).h(px(3.5 * s)).rounded_full().bg(accent))
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
                .child(div().flex().flex_row().gap(px(4. * s)).child(eye()).child(eye()))
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
fn warp_screen_to_content(sx: f32, sy: f32, k1: f32, k2: f32) -> (f32, f32) {
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
const PAD_X: f32 = 8.0;
const PAD_Y: f32 = 4.0;

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
    // Each class is a fixed offset on the seed arc — distinct, but unmistakably
    // one family. Lightness flips so it reads on light themes too.
    let dark = th.bg.l < 0.5;
    let l = if dark { 0.72 } else { 0.40 };
    let hue = |off: f32| -> Hsla {
        Hsla {
            h: wrap01(th.accent.h + off),
            s: th.accent.s.clamp(0.45, 0.95),
            l,
            a: 1.0,
        }
    };
    let comment = Hsla { a: 0.7, ..th.faint };
    classify_line(line)
        .into_iter()
        .map(|t| match t {
            Tok::Word => th.text,
            Tok::Num => hue(0.09),
            Tok::Str => hue(0.17),
            Tok::Path => hue(-0.09),
            Tok::Flag => hue(-0.17),
            Tok::Op => hue(0.32),
            Tok::Punct => hue(0.24),
            Tok::Keyword => th.accent,
            Tok::Comment => comment,
        })
        .collect()
}

/// Which independent level a graded cell takes: foreground text vs background.
#[derive(Clone, Copy, PartialEq, Eq)]
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
    // …then master brightness and the per-channel text/background level scale it.
    l *= f(g.brightness);
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

/// F1 was pressed in this pane — ask the workspace to open the help modal.
pub struct OpenHelp;
impl gpui::EventEmitter<OpenHelp> for TerminalView {}

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
        let lines = self.styled_lines(&th);
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
        let stored = *self.content_bounds.lock().unwrap();
        let (avail_w, avail_h) = match stored {
            Some(b) => (
                f32::from(b.size.width) - PAD_X * 2.,
                f32::from(b.size.height) - PAD_Y * 2.,
            ),
            None => {
                let viewport = window.viewport_size();
                (
                    f32::from(viewport.width) - PAD_X * 2.,
                    f32::from(viewport.height) - HEADER_H - PAD_Y * 2.,
                )
            }
        };
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
        let fx = (lx * bw - PAD_X) / self.cell_w;
        let y = ((ly * bh - PAD_Y) / self.cell_h).max(0.) as usize;
        let col = (fx.max(0.) as usize).min(self.grid.cols.saturating_sub(1));
        let row = y.min(self.grid.rows.saturating_sub(1));
        if std::env::var("TD_HITDEBUG").is_ok() {
            eprintln!(
                "hit pos=({:.0},{:.0}) rect=({:.0},{:.0},{:.0},{:.0}) k={:?} local=({:.3},{:.3}) cell=(r{row},c{col})",
                f32::from(pos.x),
                f32::from(pos.y),
                bx,
                by,
                bw,
                bh,
                self.warp_k,
                lx,
                ly,
            );
        }
        let side = if fx.fract() < 0.5 {
            Side::Left
        } else {
            Side::Right
        };
        (row, col, side)
    }

    fn cell_at(&self, pos: gpui::Point<Pixels>, display_offset: usize) -> (TermPoint, Side) {
        let (row, col, side) = self.viewport_cell(pos);
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
        let line = {
            let term = self.session.term.lock();
            let content = term.renderable_content();
            let display_offset = content.display_offset;
            let mut row = vec![' '; self.grid.cols];
            for indexed in content.display_iter {
                if indexed.point.line.0 + display_offset as i32 != vrow as i32 {
                    continue;
                }
                if indexed.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                let c = indexed.point.column.0;
                if c < row.len() {
                    row[c] = if indexed.cell.c == '\0' {
                        ' '
                    } else {
                        indexed.cell.c
                    };
                }
            }
            row.into_iter().collect::<String>()
        };
        match link_at(&line, vcol)? {
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
        let dy = match ev.delta {
            gpui::ScrollDelta::Lines(l) => l.y * 3.0,
            gpui::ScrollDelta::Pixels(p) => f32::from(p.y) / self.cell_h,
        };
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

        let mut lines: Vec<(String, Vec<TextRun>)> = (0..self.grid.rows)
            .map(|_| (String::new(), vec![]))
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
        let human_rows: Vec<bool> = if agent {
            rows_text.iter().map(|t| is_human_input_line(t)).collect()
        } else {
            Vec::new()
        };
        let mut ords = vec![0usize; self.grid.rows];

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
            fg = graded(fg, &th.grade, Channel::Text);
            bg = bg.map(|c| graded(c, &th.grade, Channel::Bg));

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
        let lines = self.styled_lines(&th);
        let status = if self.bell {
            "● done"
        } else if self.exited {
            "exited"
        } else {
            "live"
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
                    .child(format!("▸ {} · {buf}", self.mode.label()))
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
                    .gap_1()
                    .cursor_pointer()
                    .child(format!("▸ {} · {label}", self.mode.label()))
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
                            .px(px(PAD_X))
                            .py(px(PAD_Y))
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

        // brightness > 0.5 raises lightness, < 0.5 lowers it
        let mut up = Grade::neutral();
        up.set(GradeKey::Brightness, 0.75);
        assert!(graded(c, &up, Channel::Text).l > c.l);
        let mut down = Grade::neutral();
        down.set(GradeKey::Brightness, 0.25);
        assert!(graded(c, &down, Channel::Text).l < c.l);

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
}
