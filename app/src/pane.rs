//! TerminalView — one pane: a real shell with themed rendering, selection,
//! scrollback, clipboard, CRT-lite effects, and the TD_LATENCY probe.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::crt;
use crate::term;
use crate::theme::{self, Theme, ThemeChoice};
use alacritty_terminal::{
    event::{Event as TermEvent, Notify},
    grid::Scroll,
    index::{Column, Point as TermPoint, Side},
    selection::{Selection, SelectionType},
    term::{cell::Flags, viewport_to_point, TermMode},
    vte::ansi::{Color as AnsiColor, NamedColor},
};
use futures::StreamExt;
use gpui::{
    canvas, div, font, linear_color_stop, linear_gradient, point, prelude::*, px, rgb, App, Bounds,
    BoxShadow, ClipboardItem, Context, FocusHandle, Focusable, Font, FontWeight, Hsla,
    KeyDownEvent, Keystroke, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    ScrollWheelEvent, StyledText, TextRun, UnderlineStyle, Window,
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

const HEADER_H: f32 = 28.0;
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

/// The colour unstyled, default-foreground text takes — the bulk of the
/// screen. It varies by mode so each [`ColorMode`] has a distinct identity even
/// on plain text, not only on programs that emit ANSI colour:
/// - `Default` — the honest xterm default foreground (light grey).
/// - `Monochrome` — the theme's phosphor (`text`), the classic look.
/// - `OnTheme` — the seed accent, so plain text glows on-theme too.
///
/// (When the `syntax` overlay is on, default-fg text is instead recoloured by
/// token class in [`syntax_colors`]; this is the no-overlay fallback.)
fn default_fg(mode: crate::theme::ColorMode, text: Hsla, accent: Hsla) -> Hsla {
    use crate::theme::ColorMode;
    match mode {
        ColorMode::Default => rgb(0xe5e5e5).into(),
        ColorMode::Monochrome => text,
        ColorMode::OnTheme => accent,
    }
}

fn ansi_to_hsla(color: AnsiColor, th: &Theme, default: Hsla) -> Hsla {
    match color {
        AnsiColor::Named(named) => match named {
            // unstyled text follows the active mode (honest grey / phosphor /
            // seed); bg + cursor stay structural so the UI never loses contrast
            NamedColor::Foreground => default_fg(th.color_mode, th.text, th.accent),
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
    /// Explicit per-pane appearance; None = follow the outer theme (+ mode tint).
    pub theme_override: Option<ThemeChoice>,
    /// Debounced PTY resize: (target grid, when it stabilized).
    pending_grid: Option<(term::GridSize, Instant)>,
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

/// This sub-tab's name just changed (rename committed) — the workspace
/// persists the layout so the custom name survives a restart.
pub struct PaneRenamed;
impl gpui::EventEmitter<PaneRenamed> for TerminalView {}

impl TerminalView {
    /// The theme this pane actually renders with: an explicit override wins;
    /// otherwise the outer theme tinted by what's running (mode).
    pub fn resolved_theme(&self, cx: &App) -> Theme {
        match &self.theme_override {
            Some(choice) => (*theme::resolve(cx, choice)).clone(),
            None => mode_theme(&theme::theme(cx), &self.mode),
        }
    }

    /// What this pane is doing right now — cwd + resumable agent session —
    /// captured from the kernel for the workspace snapshot.
    pub fn runtime(&self) -> crate::session::PaneRuntime {
        crate::session::capture(self.session.master.as_ref(), self.session.shell_pid)
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
                let active = this
                    .update(cx, |view: &mut TerminalView, cx| {
                        let th = theme::theme(cx);
                        if view.fx.tick(&th) {
                            cx.notify();
                        }
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
                        view.fx.active()
                    })
                    .unwrap_or(false);
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
            theme_override: None,
            pending_grid: None,
        }
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
    fn sync_size(&mut self, th: &Theme, scale: f32, window: &mut Window) {
        self.cell_h = th.cell_h * scale;
        let font = grid_font(th, FontWeight::NORMAL);
        if let Ok(w) = window.text_system().advance(
            window.text_system().resolve_font(&font),
            px(th.font_size * scale),
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

    fn cell_at(&self, pos: gpui::Point<Pixels>, display_offset: usize) -> (TermPoint, Side) {
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
        // Invert the tube's barrel warp: a screen point displays content
        // sampled from warped(point), so selection must follow the glass.
        let mut sx = f32::from(pos.x) - bx;
        let mut sy = f32::from(pos.y) - by;
        let (k1, k2) = self.warp_k;
        if k1.abs() > 0.0005 {
            let cu = sx / bw - 0.5;
            let cv = sy / bh - 0.5;
            let r2 = cu * cu + cv * cv;
            let f = 1.0 + k1 * r2 + k2 * r2 * r2;
            sx = (0.5 + cu * f) * bw;
            sy = (0.5 + cv * f) * bh;
        }
        let fx = (sx - PAD_X) / self.cell_w;
        let y = ((sy - PAD_Y) / self.cell_h).max(0.) as usize;
        let col = (fx.max(0.) as usize).min(self.grid.cols.saturating_sub(1));
        let row = y.min(self.grid.rows.saturating_sub(1));
        let side = if fx.fract() < 0.5 {
            Side::Left
        } else {
            Side::Right
        };
        (
            viewport_to_point(display_offset, TermPoint::new(row, Column(col))),
            side,
        )
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
        if m.control && m.shift {
            match ks.key.as_str() {
                // workspace chords: new tab
                "t" => return,
                "c" => {
                    let text = self.session.term.lock().selection_to_string();
                    if let Some(text) = text {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                    return;
                }
                "v" => {
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
                    return;
                }
                _ => {}
            }
        }
        if let Some(bytes) = keystroke_bytes(ks) {
            {
                let mut term = self.session.term.lock();
                term.selection = None;
                term.scroll_display(Scroll::Bottom);
            }
            self.pending_input = Some(Instant::now());
            self.session.notifier.notify(bytes);
            cx.notify();
        }
    }

    fn on_wheel(&mut self, ev: &ScrollWheelEvent, _w: &mut Window, cx: &mut Context<Self>) {
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

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if std::env::var("TD_KEYDEBUG").is_ok() {
            eprintln!("pane mousedown at {:?}", ev.position);
        }
        // Clicking into a terminal makes it the focused leaf, so keystrokes and
        // the split buttons (which target the focused pane) follow the pane the
        // user is actually working in — not whichever pane happened to start focused.
        window.focus(&self.focus_handle, cx);
        let offset = self.session.term.lock().grid().display_offset();
        let (point, side) = self.cell_at(ev.position, offset);
        let ty = match ev.click_count {
            2 => SelectionType::Semantic,
            n if n >= 3 => SelectionType::Lines,
            _ => SelectionType::Simple,
        };
        self.session.term.lock().selection = Some(Selection::new(ty, point, side));
        self.selecting = true;
        self.last_mouse = ev.position;
        self.autoscroll = 0.;
        cx.notify();
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, _w: &mut Window, cx: &mut Context<Self>) {
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

    fn on_mouse_up(&mut self, _ev: &MouseUpEvent, _w: &mut Window, _cx: &mut Context<Self>) {
        self.selecting = false;
        self.autoscroll = 0.;
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
        let palettes: Vec<Vec<Hsla>> = if syntax {
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
            rows_text.iter().map(|t| syntax_colors(t, th)).collect()
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

fn grid_font(th: &Theme, weight: FontWeight) -> Font {
    let mut f = font(th.font_family.clone());
    f.weight = weight;
    f
}

/// gpui Keystroke → PTY bytes.
fn keystroke_bytes(ks: &Keystroke) -> Option<Vec<u8>> {
    let m = &ks.modifiers;
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
        let scale = cx
            .try_global::<crate::UiScale>()
            .map(|s| s.0)
            .unwrap_or(1.0);
        self.sync_size(&th, scale, window);
        self.warp_k = (th.curvature * 0.14, th.curvature * 0.06);
        let lines = self.styled_lines(&th);
        let status = if self.exited { "exited" } else { "live" };
        let grid_label = format!("{}×{}", self.grid.cols, self.grid.rows);
        let glow = th.glow;

        // Sub-tab header text + glow: the complement of the theme seed
        // (accent), the opposite hue but kept legible. ~30% more vibrant than
        // the old muting (s 0.55→0.72, l 0.8→0.9) so the bar reads less faded.
        let bar_fg = Hsla {
            h: wrap01(th.accent.h + 0.5),
            s: (th.accent.s * 0.72).clamp(0., 1.),
            l: (th.accent.l * 0.9).clamp(0., 1.),
            a: th.accent.a,
        };

        // solid, reflective header: gradient face + crisp top reflection line
        let mut lighter = th.surface;
        lighter.l = (lighter.l * 1.9).min(0.9);
        let mut header = div()
            .h(px(HEADER_H))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_3()
            .bg(linear_gradient(
                180.,
                linear_color_stop(lighter, 0.),
                linear_color_stop(th.surface, 1.),
            ))
            .border_b_1()
            .border_color(th.accent.alpha(0.5))
            .text_color(bar_fg)
            .child(if let Some(buf) = self.renaming.clone() {
                // inline rename box: a left-click anywhere else commits via
                // focus loss is not wired, so enter/escape (in on_key) close it
                div()
                    .flex_1()
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
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .child(format!("▸ {} · {label}", self.mode.label()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, ev: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            cx.emit(DragPaneStart { at: ev.position });
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
                    .gap_2()
                    .child(grid_label)
                    .child(
                        // the theme icon IS the theme UI: click for the breakout
                        div()
                            .px_1()
                            .rounded_sm()
                            .border_1()
                            .border_color(th.accent.alpha(0.5))
                            .cursor_pointer()
                            .child(th.icon.clone())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|_, ev: &MouseDownEvent, _w, cx| {
                                    cx.stop_propagation();
                                    cx.emit(OpenThemeMenu { at: ev.position });
                                }),
                            ),
                    )
                    .child(
                        // the display icon: click for this pane's monitor-OSD tray
                        div()
                            .px_1()
                            .rounded_sm()
                            .border_1()
                            .border_color(th.accent.alpha(0.5))
                            .cursor_pointer()
                            .child("⛭")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|_, ev: &MouseDownEvent, _w, cx| {
                                    cx.stop_propagation();
                                    cx.emit(OpenDisplayMenu { at: ev.position });
                                }),
                            ),
                    )
                    .child(status)
                    .child(
                        // close just this sub-tab (×): ends this pane's shell
                        div()
                            .px_1()
                            .rounded_sm()
                            .border_1()
                            .border_color(th.accent.alpha(0.3))
                            .text_color(bar_fg)
                            .cursor_pointer()
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

        let jiggle = self.fx.jiggle_px;
        div()
            .track_focus(&self.focus_handle(cx))
            .on_key_down(cx.listener(Self::on_key))
            .on_scroll_wheel(cx.listener(Self::on_wheel))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .size_full()
            .bg(th.bg)
            .relative()
            .flex()
            .flex_col()
            .font_family(th.font_family.clone())
            .text_size(px(th.font_size * scale))
            .text_color(th.text)
            .pt(px(jiggle.max(0.)))
            .pb(px((-jiggle).max(0.)))
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
                                    crate::warp::register_tube(
                                        [
                                            f32::from(bounds.origin.x) * sf,
                                            f32::from(bounds.origin.y) * sf,
                                            f32::from(bounds.size.width) * sf,
                                            f32::from(bounds.size.height) * sf,
                                        ],
                                        th.screen_glare,
                                        th.curvature * 0.14,
                                        th.curvature * 0.06,
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn default_foreground_is_distinct_per_mode() {
        use crate::theme::ColorMode;
        let text: Hsla = rgb(0x33ff66).into(); // a theme phosphor
        let accent: Hsla = rgb(0x00ffaa).into(); // its seed
        let ansi = default_fg(ColorMode::Default, text, accent);
        let mono = default_fg(ColorMode::Monochrome, text, accent);
        let ontheme = default_fg(ColorMode::OnTheme, text, accent);
        // The fix: plain text used to be `text` in EVERY mode, so ansi/mono/
        // theme looked identical and only the syntax overlay appeared to work.
        assert_eq!(
            ansi,
            Hsla::from(rgb(0xe5e5e5)),
            "ansi shows honest xterm grey"
        );
        assert_eq!(mono, text, "mono keeps the classic phosphor look");
        assert_eq!(ontheme, accent, "on-theme paints plain text in the seed");
        assert_ne!(ansi, mono, "ansi must differ from mono on plain text");
        assert_ne!(
            ansi, ontheme,
            "ansi must differ from on-theme on plain text"
        );
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
        // neutral grade leaves a colour untouched (the default render path)
        let n = Grade::default();
        assert_eq!(graded(c, &n, Channel::Text), c);
        assert_eq!(graded(c, &n, Channel::Bg), c);

        // brightness > 0.5 raises lightness, < 0.5 lowers it
        let mut up = Grade::default();
        up.set(GradeKey::Brightness, 0.75);
        assert!(graded(c, &up, Channel::Text).l > c.l);
        let mut down = Grade::default();
        down.set(GradeKey::Brightness, 0.25);
        assert!(graded(c, &down, Channel::Text).l < c.l);

        // colour = 0 desaturates to greyscale
        let mut grey = Grade::default();
        grey.set(GradeKey::Colour, 0.0);
        assert!(graded(c, &grey, Channel::Text).s.abs() < 1e-6);

        // text vs background are independent: the text slider moves fg only
        let mut text_only = Grade::default();
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
        let mut hi = Grade::default();
        hi.set(GradeKey::Contrast, 0.75);
        assert!(graded(bright, &hi, Channel::Text).l > bright.l);

        // results always stay in gamut
        let mut extreme = Grade::default();
        extreme.set(GradeKey::Brightness, 1.0);
        let g = graded(Hsla { l: 0.95, ..c }, &extreme, Channel::Text);
        assert!((0.0..=1.0).contains(&g.l) && (0.0..=1.0).contains(&g.s));
    }

    #[test]
    fn keystroke_bytes_encodes_the_pty_protocol() {
        let bytes = |s: &str| keystroke_bytes(&Keystroke::parse(s).unwrap());
        assert_eq!(bytes("ctrl-c"), Some(vec![3]));
        assert_eq!(bytes("enter"), Some(b"\r".to_vec()));
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
}
