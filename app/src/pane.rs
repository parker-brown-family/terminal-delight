//! TerminalView — one pane: a real shell with themed rendering, selection,
//! scrollback, clipboard, CRT-lite effects, and the TD_LATENCY probe.

use std::cell::RefCell;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::crt;
use crate::doc::{wrap_join, DocLine, Document, DocumentSource, RowBudget, WrapJoin};
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
    px, rgb, Animation, AnimationExt, AnyElement, App, Bounds, BoxShadow, ClipboardItem, Context,
    FocusHandle, Focusable, Font, FontStyle, FontWeight, Hsla, KeyDownEvent, Keystroke,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, ScrollWheelEvent,
    StyledText, TextRun, UnderlineStyle, Window,
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

/// Strip a row down to its sentence: the surrounding whitespace plus the box
/// drawing the agent CLI frames a dialog in, so `│ Do you want to proceed?   │`
/// reads as the question it is. `>`/`❯` are deliberately NOT stripped — they
/// mark the human's own echoed input, and a line the HUMAN typed must never
/// read as the CLI asking something.
fn prompt_sentence(text: &str) -> &str {
    // U+2500..U+257F is the whole Box Drawing block — │ ─ ╭ ╰ and the rest.
    text.trim_matches(|c: char| c.is_whitespace() || ('\u{2500}'..='\u{257f}').contains(&c))
}

/// Does this rendered row read as part of an INTERACTION PROMPT — the agent
/// stopped to ask the human something (an option picker, a permission gate, a
/// trust dialog), as opposed to being done? Matches the stable footer/header
/// phrases Claude Code and Codex print with their pickers. Deliberately
/// STRICT, like the copy gate: a false "come interact" cries wolf, a miss just
/// means the plain done-bell semantics. "esc to interrupt" (the WORKING
/// footer) must never match — only "esc to cancel" (a prompt's footer).
///
/// The question forms are ANCHORED: the phrase has to OPEN the row and the row
/// has to be the question (ends in `?`). A dialog header is its own line inside
/// a box; the identical words in the agent's own prose arrive mid-sentence —
/// "What do you want to work on?" is the agent talking, and matching it pinned
/// the blinker on forever, because a finished reply just sits there on screen.
pub fn row_wants_human(text: &str) -> bool {
    let t = prompt_sentence(text).to_ascii_lowercase();
    if t.is_empty() {
        return false;
    }
    // Footer furniture: the CLI prints these only under a LIVE picker, so they
    // stand alone wherever on the row they land.
    if t.contains("enter to select") || t.contains("esc to cancel") {
        return true;
    }
    t.ends_with('?')
        && (t.starts_with("do you want to")
            || t.starts_with("do you trust")
            || t.starts_with("would you like to proceed"))
}

/// Is an interaction prompt on screen RIGHT NOW? Scans the last few live rows
/// (prompts sit at the bottom of an agent TUI). Pure over the rows for tests.
pub fn wants_human(recent_rows: &[String]) -> bool {
    recent_rows.iter().any(|r| row_wants_human(r))
}

/// Did the agent stop because it hit a WALL rather than the end of its turn?
/// Matches the error banners the agent CLI itself prints — API failures,
/// exhausted limits, expired auth — NOT the word "error", which appears
/// constantly in the agent's own tool output (a failing build is the agent
/// WORKING, not the agent blocked). Classifies the finish badge: ✅ clean, ❌
/// blocked. Strict for the same reason as [`row_wants_human`].
pub fn looks_blocked(recent_rows: &[String]) -> bool {
    recent_rows.iter().any(|r| {
        let t = r.trim().to_ascii_lowercase();
        !t.is_empty()
            && (t.contains("api error")
                || t.contains("usage limit")
                || t.contains("credit balance")
                || t.contains("oauth token")
                || t.contains("rate_limit_error")
                || t.contains("overloaded_error")
                || t.contains("request timed out"))
    })
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

/// The Alt-held copy affordance: one reconstructed logical line and the PAINTED
/// rows it occupies. The text is what lands on the clipboard — wrap seams already
/// healed — and the row span is where the border is drawn, in painted order so it
/// frames the text where the eye actually sees it.
#[derive(Debug, Clone, PartialEq)]
struct CopyHint {
    text: String,
    first_paint: usize,
    last_paint: usize,
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

/// Does grid row `r` flow into `r+1` as one logical token? Two signals: the
/// terminal's own soft-wrap (`wraps[r]`, the `WRAPLINE` flag), OR a *width-wrap*
/// — an app (Claude Code, and our own Links tables) that hard-wraps a long
/// URL/path to the pane width emits real rows with NO `WRAPLINE`, but the token
/// runs edge-to-edge: row `r` has no trailing space (its last cell is filled)
/// and row `r+1` begins with a non-space char. Links/paths carry no interior
/// spaces, so an edge-filled boundary is exactly a mid-token break. A row that
/// wrapped at a word boundary keeps its trailing space, so prose never trips this.
fn row_flows_into_next(rows: &[Vec<char>], wraps: &[bool], r: usize) -> bool {
    if r + 1 >= rows.len() {
        return false;
    }
    if wraps.get(r).copied().unwrap_or(false) {
        return true;
    }
    let filled = rows[r].last().is_some_and(|c| !c.is_whitespace());
    let continues = rows[r + 1].first().is_some_and(|c| !c.is_whitespace());
    filled && continues
}

/// Stitch a click on a wrapped row back into its full logical line. A terminal
/// wraps a long URL/path mid-token with no space; the break is carried either by
/// the `WRAPLINE` flag (`wraps[r]`) or, for app-hard-wrapped output, by the token
/// running edge-to-edge (see `row_flows_into_next`). We walk up while the row
/// above flows into us and down while we keep flowing, concatenate those rows,
/// and return the stitched line together with the absolute column of the original
/// click within it — so `link_at` sees the whole token instead of a truncated
/// fragment. Pure: testable without a live grid.
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
    // first row of the logical line: walk up while the row above flows into us
    let mut top = vrow;
    while top > 0 && row_flows_into_next(rows, wraps, top - 1) {
        top -= 1;
    }
    // last row: walk down while the current row flows into the next
    let mut bot = vrow;
    while bot + 1 < rows.len() && row_flows_into_next(rows, wraps, bot) {
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

/// Smart-reflow selected terminal text for the clipboard. TUI agents like Claude
/// Code wrap their own prose and commands to the pane width and emit *real* line
/// breaks; those breaks land in the grid as separate rows, so a naïve copy
/// pastes one paragraph (or one long command) as a stack of short lines. This
/// rejoins rows that were broken purely *by width* back into one logical line,
/// while leaving genuine structure alone.
///
/// Precision is the width test: a row counts as a wrap only when it reaches
/// ~`cols` — i.e. the first word of the next row could not have fit after it —
/// so text wrapped at a *narrower* fixed column (email at 72, source at 100) is
/// left untouched. A row filled to exactly `cols` is a mid-token wrap and is
/// glued with no space; a shorter-but-still-full row wrapped at a word boundary
/// (its trailing space trimmed) is rejoined with a single space. Blank lines
/// stay as paragraph breaks; indented, code-fenced, and rule/box-drawing rows
/// are never merged. The native `selection_to_string` already strips genuine
/// terminal soft-wrap, so those rows arrive pre-joined and pass through here
/// unchanged. Pure: unit-tested without a live grid.
fn reflow_wrapped_copy(text: &str, cols: usize) -> String {
    let rows: Vec<&str> = text.split('\n').collect();
    reflow_wrapped_copy_spans(&rows, cols)
        .into_iter()
        .map(|l| l.text)
        .collect::<Vec<_>>()
        .join("\n")
}

/// One reconstructed logical line, plus the inclusive range of input rows it was
/// assembled from. The row span is what makes the reflow usable for anything
/// other than a drag-selection: it says *which rows on screen* a copyable line
/// occupies, so a hover affordance can be drawn around exactly those.
#[derive(Debug, Clone, PartialEq)]
struct LogicalLine {
    text: String,
    /// First input row of this logical line (inclusive).
    first: usize,
    /// Last input row of this logical line (inclusive). Equal to `first` for an
    /// unwrapped line.
    last: usize,
}

/// The span-reporting core of [`reflow_wrapped_copy`] — same joining rules, but
/// it also reports which rows produced each logical line. `reflow_wrapped_copy`
/// is a thin wrapper over this, so the two can never disagree about where a
/// logical line begins and ends.
///
/// Blank rows are emitted as empty lines (paragraph breaks) with their own
/// one-row span, exactly as the text path does, so the wrapper's `join("\n")`
/// reproduces its previous output byte for byte. Callers that want *hoverable*
/// regions must skip them.
///
/// IMPORTANT: rows must arrive with trailing padding trimmed. A terminal grid is
/// space-padded to the full column count, and `wrap_join`'s width test reads a
/// padded row as full-width — so feeding raw grid rows in makes every row look
/// like a wrap and glues the whole screen into one line. Pure: testable without
/// a live grid.
fn reflow_wrapped_copy_spans(rows: &[&str], cols: usize) -> Vec<LogicalLine> {
    let one_per_row = |rows: &[&str]| -> Vec<LogicalLine> {
        rows.iter()
            .enumerate()
            .map(|(i, r)| LogicalLine {
                text: (*r).to_string(),
                first: i,
                last: i,
            })
            .collect()
    };
    if cols == 0 {
        return one_per_row(rows);
    }
    let mut out: Vec<LogicalLine> = Vec::new();
    // The logical line under construction: its text, the char-width of the last
    // raw row appended (the row the wrap test is applied against), and the span
    // of input rows it has consumed so far.
    let mut cur: Option<(String, usize, usize, usize)> = None;
    for (i, raw) in rows.iter().enumerate() {
        let raw = *raw;
        let row_len = raw.chars().count();
        if raw.trim().is_empty() {
            if let Some((text, _, first, last)) = cur.take() {
                out.push(LogicalLine { text, first, last });
            }
            out.push(LogicalLine {
                text: String::new(),
                first: i,
                last: i,
            });
            continue;
        }
        match cur.take() {
            None => cur = Some((raw.to_string(), row_len, i, i)),
            Some((mut acc, prev_len, first, last)) => match wrap_join(&acc, prev_len, raw, cols) {
                WrapJoin::Glue => {
                    acc.push_str(raw);
                    cur = Some((acc, row_len, first, i));
                }
                WrapJoin::Space => {
                    acc.push(' ');
                    acc.push_str(raw);
                    cur = Some((acc, row_len, first, i));
                }
                WrapJoin::Break => {
                    out.push(LogicalLine {
                        text: acc,
                        first,
                        last,
                    });
                    cur = Some((raw.to_string(), row_len, i, i));
                }
            },
        }
    }
    if let Some((text, _, first, last)) = cur.take() {
        out.push(LogicalLine { text, first, last });
    }
    out
}

/// Shell verbs common enough in agent output that seeing one in first position
/// is strong evidence the line is a command rather than prose. Deliberately a
/// closed list: the copy affordance is opt-in per line, and a false positive
/// (a chip over an English sentence) is more corrosive than a false negative.
const COMMAND_VERBS: &[&str] = &[
    "awk",
    "bash",
    "bun",
    "bunx",
    "cargo",
    "cat",
    "cd",
    "chmod",
    "chown",
    "cp",
    "curl",
    "diff",
    "docker",
    "echo",
    "env",
    "find",
    "gh",
    "git",
    "grep",
    "gzip",
    "head",
    "hyprctl",
    "jq",
    "kubectl",
    "ln",
    "ls",
    "make",
    "mkdir",
    "mv",
    "node",
    "npm",
    "npx",
    "pip",
    "pip3",
    "python",
    "python3",
    "rg",
    "rm",
    "rsync",
    "scp",
    "sed",
    "sh",
    "ssh",
    "sudo",
    "systemctl",
    "tail",
    "tar",
    "tmux",
    "touch",
    "wget",
    "xargs",
    "zsh",
];

/// Whether a reconstructed logical line earns a one-click copy affordance.
///
/// v1 is deliberately STRICT — commands only. Offering a chip on every logical
/// line is never *wrong*, but it makes the pane restless, and the whole value of
/// the affordance is that its presence means "this is a thing you run". JSON and
/// prose are out of scope: JSON carries real line breaks, so the reflow leaves
/// it alone and there is nothing to reconstruct.
///
/// The elision rule is not a heuristic, it is a hard guarantee. A line carrying
/// `…` is an *illustration* — an agent showing the shape of a command with the
/// boring parts cut out. Copying it produces something that looks authoritative
/// and cannot work. Such lines get no chip at all: not a warning, not a disabled
/// state. Silence is the correct affordance for text that must not be copied.
fn is_copyable_command(line: &str) -> bool {
    let t = line.trim();
    if t.chars().count() < 4 {
        return false;
    }
    // an elided illustration is never runnable — never offer it
    if t.contains('…') {
        return false;
    }
    // `! cmd` (the Claude Code run prefix) and `$ cmd` (the docs convention) are
    // explicit "this is a command" markers, and settle it on their own.
    let body = match t.strip_prefix('!').or_else(|| t.strip_prefix('$')) {
        Some(rest) if rest.starts_with(' ') => return rest.trim().chars().count() >= 2,
        _ => t,
    };
    // Otherwise the first token has to look like something you can execute.
    let Some(first) = body.split_whitespace().next() else {
        return false;
    };
    // A path to an executable: ./script.sh, /usr/bin/thing, ~/bin/td-send
    if first.starts_with("./") || first.starts_with('/') || first.starts_with("~/") {
        return !first.ends_with('.') && body.split_whitespace().count() >= 1;
    }
    // A line-initial URL: the other string you constantly need WHOLE. Link
    // tables and agent replies wrap them, and half a URL is as dead as half a
    // command. First-token-only keeps prose ("see https://…") chip-free.
    let lower = first.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("file://")
    {
        return true;
    }
    // An env assignment prefix (FOO=1 cmd …) reads as a command line.
    if first.contains('=') && !first.contains(' ') && body.split_whitespace().count() >= 2 {
        return true;
    }
    COMMAND_VERBS.contains(&first)
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

/// Spawn a helper detached from us — its own session, no inherited stdio — so
/// it outlives the click and never blocks the UI.
fn spawn_detached(program: &str, args: &[&str]) {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(program);
    cmd.args(args)
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

/// Is this session managed by uwsm (Omarchy's, and any `uwsm start`ed Wayland
/// session)? There, a GUI app is expected to be launched through `uwsm-app --`,
/// which puts it in its OWN systemd scope under `app.slice` instead of leaving
/// it a child of whatever spawned it — Omarchy's launchers are literally
/// `exec setsid uwsm-app -- nautilus …`, and its Quickshell panels reveal files
/// the same way. `uwsm-app` is a client for `wayland-wm-app-daemon.service` and
/// talks to it through a FIFO in `$XDG_RUNTIME_DIR`; with no daemon it restarts
/// units and blocks on its own timeouts, so the FIFO is the honest test of
/// whether this route is available. Anywhere else — GNOME, KDE, X11, a bare
/// compositor — this is false and we spawn the program directly, as before.
fn session_uses_uwsm() -> bool {
    let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") else {
        return false;
    };
    std::path::Path::new(&format!("{dir}/uwsm-app-daemon-in")).exists()
}

/// Hand a URL/path to the system default tool (`xdg-open`), detached so it
/// outlives the click and never blocks the UI. On a uwsm session the launch
/// goes through `uwsm-app` so the opened app is scoped to the desktop rather
/// than to this terminal — closing the pane that printed a link should not be
/// able to take the PDF it opened with it.
fn open_with_system(target: &str) {
    if session_uses_uwsm() {
        spawn_detached("uwsm-app", &["--", "xdg-open", target]);
    } else {
        spawn_detached("xdg-open", &[target]);
    }
}

/// The filesystem item a clicked link points at, or None when there is nothing
/// on this disk to show — an `http(s)` link, or a `file://` URI naming another
/// host. Accepts what `link_under` hands back: an already-absolute path, or a
/// URL. Pure, so the tricky half (percent-decoding, the `localhost` authority)
/// is testable without a file manager.
fn reveal_target(target: &str) -> Option<String> {
    if target.starts_with('/') {
        return Some(target.to_string());
    }
    let rest = target.strip_prefix("file://")?;
    // file://<authority>/path — empty or "localhost" means this machine; any
    // other authority is someone else's disk and we have nothing to reveal.
    let path = if let Some(p) = rest.strip_prefix("localhost/") {
        format!("/{p}")
    } else if rest.starts_with('/') {
        rest.to_string()
    } else {
        return None;
    };
    Some(percent_decode(&path))
}

/// Decode `%XX` escapes in a URI path. Leaves a malformed escape as written —
/// a literal `%` in a filename is likelier than a truncated escape.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Encode an absolute path as a `file://` URI. Everything outside the URI
/// unreserved set is escaped, `/` excepted — a space or a `#` in a filename
/// otherwise truncates the URI at the receiving end.
fn path_to_file_uri(path: &str) -> String {
    let mut out = String::from("file://");
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Wrap a string so a POSIX shell reads it as one literal argument.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// The shell one-liner that reveals `path`. Pure, so the quoting and both
/// branches are testable without a desktop.
///
/// `org.freedesktop.FileManager1.ShowItems` is the desktop-wide interface for
/// showing an item *selected* in its folder — Nautilus, Dolphin, Nemo, Thunar
/// and PCManFM all export it, and it is what "reveal in folder" means on every
/// other OS. It is also D-Bus *activation*: the manager is started by the
/// session, not by us, so it is correctly scoped whatever desktop this is. Only
/// the fallback — a desktop exporting no such manager, which gets the containing
/// directory opened instead — is ours to place, and there `use_uwsm` routes it
/// through `uwsm-app` the way the rest of an Omarchy session launches apps.
///
/// The fallback rides inside the same `sh` because whether the D-Bus call failed
/// is only knowable after it returns, and the UI thread will not wait for it.
fn reveal_script(path: &str, use_uwsm: bool) -> String {
    let parent = std::path::Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "/".to_string());
    let opener = if use_uwsm {
        "uwsm-app -- xdg-open"
    } else {
        "xdg-open"
    };
    format!(
        "dbus-send --session --print-reply --dest=org.freedesktop.FileManager1 \
         /org/freedesktop/FileManager1 org.freedesktop.FileManager1.ShowItems \
         array:string:{} string:'' >/dev/null 2>&1 || {opener} {} >/dev/null 2>&1",
        shell_quote(&path_to_file_uri(path)),
        shell_quote(&parent),
    )
}

/// Show `path` in the system file manager with the item itself selected, rather
/// than opening it — the "where does this actually live?" answer to a path a
/// pane just printed.
fn reveal_with_system(path: &str) {
    spawn_detached("sh", &["-c", &reveal_script(path, session_uses_uwsm())]);
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

/// Cache key for [`TerminalView::mirror_document`]. Matching keys guarantee a
/// byte-identical document: `generation` moves on every terminal event, the
/// grid dimensions cover resizes, and the remaining fields are exactly what
/// [`TerminalView::resolved_theme`]'s memo keys on — its guarantee is what makes
/// comparing these enough, with no hash over the resolved palette.
#[derive(Clone, PartialEq)]
struct MirrorDocKey {
    generation: u64,
    cols: usize,
    rows: usize,
    eff: theme::ThemeChoice,
    mode: PaneMode,
    inherit: bool,
    theme_gen: u64,
}

pub struct TerminalView {
    focus_handle: FocusHandle,
    session: term::Session,
    /// The OSC-driven shell title (apps overwrite it via the title sequence).
    pub title: String,
    /// A user-set name (right-click the header to rename). Wins over `title`
    /// and survives OSC title updates; persisted per leaf in the state file.
    pub name: Option<String>,
    /// Filesystem path to an EXPLICIT per-pane header logo image (MCP
    /// `set_pane_config`, or one saved by an older session). Shown to the left
    /// of the program label; click the logo (or the `＋ logo`
    /// placeholder when unset) to pick one. Persisted per leaf in the state file.
    pub logo: Option<String>,
    /// The logo INHERITED from the pane's cwd via the per-directory map
    /// (`dirlogo`), resolved by the workspace's sweep — display state, never
    /// persisted (the map itself is the durable record). Shadowed by `logo`.
    pub dir_logo: Option<String>,
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
    /// When this pane came into being — drives the one-shot CRT ignition, and
    /// nothing else. Set at construction rather than on first render so a pane
    /// created behind an inactive tab has already "fired" by the time you look
    /// at it, instead of ambushing you with a flash when you switch over.
    born: Instant,
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
    /// A prompt seek (Alt+↑/↓ in an alt-screen agent pane) is walking the AGENT's
    /// own scrollback right now. Guards against a second press stacking another
    /// walk on top of the first and doubling every synthetic wheel step.
    seeking: bool,
    /// Per-pane memo for `resolved_theme`, keyed on (effective choice, mode,
    /// inherit_theme, theme generation). resolve() deep-clones, recolours and
    /// grade-transforms the palette, and render() calls it every frame; this
    /// reuses the last result until one of those inputs actually changes.
    theme_cache: RefCell<ThemeMemo>,
    /// Memoised full-scrollback document for the FOCUS reader — see
    /// [`Self::mirror_document`] for the key's correctness argument. The `u64`
    /// is the document REVISION: bumped on every rebuild, never reused, so
    /// downstream caches (the reader's layout) can key on it safely where an
    /// `Arc` pointer would be unsound (a freed allocation's address can recur).
    mirror_doc: RefCell<Option<(MirrorDocKey, u64, Arc<Document>)>>,
    /// Right-click context menu (Copy / Paste / Open link) anchor, window-space.
    ctx_menu: Option<gpui::Point<Pixels>>,
    /// An agent in this pane finished and nobody has looked yet: the ping has
    /// played, the tab wears its 🔔 badge and the header reads "● done" until
    /// the focus-in edge acknowledges it ([`Self::ack_bell`]).
    bell: bool,
    /// The agent is stopped WAITING ON A HUMAN — a picker or permission prompt
    /// is on screen (see [`wants_human`]). Live state maintained by the 120ms
    /// scan, never latched: true exactly while the prompt is up. Drives the
    /// tab ❓ pulse and the "needs you" notification flavour.
    needs_input: bool,
    /// How the latched finish classified at ring time: `true` = the agent hit
    /// a wall (an error banner was on screen — see [`looks_blocked`]) → the ❌
    /// badge; `false` = a clean finish → ✅. Meaningless while `bell` is off;
    /// cleared with it.
    bell_blocked: bool,
    /// The live player child for this pane's ping (hard-killed on stop/drop).
    bell_player: crate::bell::BellPlayer,
    /// Responsive header: when the pane narrows, controls tuck into a ⋯ overflow
    /// menu. `Some(pos)` = that menu is open, anchored at the ⋯ click. None = shut.
    hdr_overflow: Option<gpui::Point<Pixels>>,
    /// The Alt-held copy affordance for the line under the pointer, or None when
    /// Alt is up or the line does not read as a command. Recomputed only while
    /// Alt is down, so ordinary mousing costs nothing.
    copy_hint: Option<CopyHint>,
    /// When the last Alt+click copy landed — drives the brief "copied"
    /// confirmation in the chip. No timer: the next pointer move repaints it.
    copy_flash: Option<Instant>,
    /// Last-known OS focus, for edge-detected focus reporting (CSI I / CSI O).
    was_focused: bool,
    /// 🎰 GAMBA slot-machine reels — rolled while an agent in this pane is
    /// "thinking", on the gamba DESIGN texture only (not any colour set). Satire.
    gamba: crate::gamba::Reels,
    /// Throttle for the (cheap) grid scan that detects the agent spinner.
    last_think_scan: Instant,
    /// True while this pane is the one mirrored in the FOCUS modal — a plain Esc
    /// then closes the modal instead of reaching the PTY. Set by the workspace.
    being_read: bool,
    /// The sticky note pinned to this pane's glass, if any. See [`crate::sticky`]
    /// — in particular why Esc does NOT take it down.
    note: Option<crate::sticky::Sticky>,
    /// The text of the note last peeled off, so `alt+s` re-opens the composer
    /// holding it (selected). This is what makes intercepting `alt+backspace`
    /// defensible: the chord already means word-erase in a shell, so a note up
    /// changes what it does, and the cost of being wrong has to be one keystroke.
    peeled: Option<String>,
    /// Which part of the note the pointer is over. Drives the peel corner's
    /// curl, which is the only thing that teaches the mouse gesture.
    note_hover: Option<crate::sticky::Hit>,
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

/// A sticky note was posted or peeled off — the workspace persists the layout so
/// the note is still there tomorrow morning (same contract as [`PaneRenamed`]).
/// Emitted only on COMMIT, never per keystroke: a note being typed rewrites the
/// state file once, when you press Enter.
pub struct StickyChanged;
impl gpui::EventEmitter<StickyChanged> for TerminalView {}

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

/// A paint-overlay tile was clicked on this pane — the recolour is already
/// applied to `appearance`; the workspace just persists the layout so the new
/// coat survives a restart (same contract as [`PaneRenamed`]).
pub struct PaintApplied;
impl gpui::EventEmitter<PaintApplied> for TerminalView {}

/// What [`TerminalView::paint_key`] decided about a keystroke offered to the
/// raised PAINT overlay. Named rather than a bool pair because the three
/// outcomes are genuinely different promises, and getting them confused is
/// exactly how a modal starts typing into the agent behind it.
enum PaintKey {
    /// The overlay acted (or deliberately swallowed a printable miss). Halt
    /// propagation — nothing below may see this key.
    Took,
    /// Declined ON PURPOSE: return without halting, so the Workspace still gets
    /// it. Bare arrows walk the wall, and only the Workspace knows the geometry.
    Bubble,
    /// Not a paint key at all — fall through to the rest of `on_key`.
    Pass,
}

/// The inclusive grid line range a [`RowBudget`] selects: the newest `lines`
/// rows, clamped to what history actually holds.
///
/// `oldest` is `grid.topmost_line()` — zero when nothing has scrolled off yet, and
/// increasingly negative as history accumulates. `newest` is the last row of the
/// visible screen. Pure so the arithmetic is testable: the reader's whole vertical
/// fill rests on this being right, and an off-by-one here shows up as one missing
/// or one duplicated line at the top of the document, which is invisible until you
/// go looking for it.
fn budget_range(oldest: i32, newest: i32, lines: usize) -> (i32, i32) {
    // i64 internally: `RowBudget::all()` passes usize::MAX, which `as i32` would
    // wrap NEGATIVE — want = -1, first = newest + 2, an inverted range, and the
    // reader would render an empty document precisely when asked for everything.
    let want = (lines.max(1)).min(i64::MAX as usize) as i64;
    let first = (newest as i64 - want + 1).max(oldest as i64);
    (first as i32, newest)
}

/// A pane as a [`DocumentSource`]: the terminal grid, scrollback included, run
/// through the hard-wrap recovery.
///
/// This is the seam the FOCUS reader talks to. It replaced "mirror the pane's
/// rendered rows and try to undo the rendering", which could only ever show one
/// screenful — so shrinking the reader's text just zoomed out instead of revealing
/// more. Here the budget decides how far back to read, so a smaller glyph genuinely
/// gets more content.
///
/// A source is a *bound* thing — a pane plus the theme its cells are coloured
/// through — which is why this is a struct rather than an impl on the pane itself.
/// It keeps `doc.rs` free of any dependency on theming.
pub struct PaneSource<'a> {
    pub pane: &'a TerminalView,
    pub theme: &'a Theme,
}

impl DocumentSource for PaneSource<'_> {
    fn document(&self, budget: RowBudget) -> Document {
        self.pane.document_with(budget, self.theme)
    }
}

impl TerminalView {
    /// Build a [`Document`] from this pane's grid, reading back up to
    /// `budget.lines` rows into scrollback. Clamped to what history actually
    /// holds, so a fresh pane simply yields fewer lines rather than blank filler.
    pub fn document_with(&self, budget: RowBudget, th: &Theme) -> Document {
        let (first, last) = {
            let term = self.session.term.lock();
            // Line 0 is the top of the screen; history runs negative from there.
            let oldest = term.grid().topmost_line().0;
            let newest = (self.grid.rows as i32 - 1).max(0);
            budget_range(oldest, newest, budget.lines)
        };
        let rows = self.grid_rows_in(first, last, th);
        Document::from_grid_rows(&rows, self.grid.cols)
    }
}

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

/// Header glyphs retired in favour of hotkeys, so a narrow pane spends its
/// width on the terminal instead of on controls. The pane header now keeps only
/// 🎨 (theme) and 📊 (display).
///
/// The BEHAVIOUR is untouched in both cases — only the glyph goes. Alt+↑/↓ still
/// walks your own messages in an agent pane, and Alt+R still opens the FOCUS
/// reader. Flip either back to `true` to restore its glyph and its ⋯ entry.
const SHOW_HUMAN_NAV_GLYPH: bool = false;
const SHOW_FOCUS_GLYPH: bool = false;

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

/// An agent in this pane just finished a real turn (the bell edge, both the
/// thinking-scan and a BEL from the app itself). The workspace turns it into a
/// system notification when the pane isn't the one being watched — the pane
/// can't decide that alone, and the tab/window naming lives up there too.
pub struct AgentDone;
impl gpui::EventEmitter<AgentDone> for TerminalView {}

/// A cached attention state flipped (either direction): "agent is working"
/// (the 🤖 pulse) or "agent needs a human" (the ❓ pulse). The workspace
/// listens so the mother bar repaints on the actual edge — the pulse
/// animations drive their own repaints only while already on screen, so
/// something must repaint the tab bar to START one.
pub struct AgentWorkingChanged;
impl gpui::EventEmitter<AgentWorkingChanged> for TerminalView {}

/// Where a paging key asks the FOCUS reader's view to go. `Top`/`Bottom` are the
/// ends of the whole document (ctrl+Home / ctrl+End); the pages overlap slightly
/// so context carries across a press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadNav {
    PageUp,
    PageDown,
    Top,
    Bottom,
}

/// A paging key pressed while this pane is mirrored in the FOCUS modal — the
/// workspace moves the READER's view. Routed through the pane for the same
/// reason as [`CloseFocusRead`]: the mirrored pane keeps keyboard focus.
pub struct FocusReadNav(pub ReadNav);
impl gpui::EventEmitter<FocusReadNav> for TerminalView {}

/// A read-only snapshot the workspace paints into the FOCUS modal. It's just the
/// same styled rows [`styled_lines`] already builds for the live pane, plus the
/// metrics needed to scale them up to fill the modal — so the mirror costs one
/// extra (cheap) grid scan of a single pane, never a second terminal or PTY.
pub struct MirrorSnapshot {
    /// The reader's document: logical lines with scrollback, width-breaks healed.
    /// This is what the FOCUS reader lays out. `lines` below is the raw mirrored
    /// viewport, still needed by the crawl path (which never wraps or joins).
    pub doc: Arc<Document>,
    /// Revision of `doc` — bumps on every rebuild, for downstream layout caches.
    pub doc_rev: u64,
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
    /// The reader's document — this pane's ENTIRE retained scrollback as logical
    /// lines — memoised against a key that can only match when the result would
    /// be byte-identical: the terminal's content generation (bumped by the I/O
    /// thread on every event), the grid dimensions, and exactly the inputs
    /// [`Self::resolved_theme`]'s own memo keys on. Same key ⇒ same colours went
    /// into every run, by that memo's guarantee — no hashing of the theme itself.
    ///
    /// A mouse-move frame therefore costs one Arc clone; only real PTY output,
    /// a resize, or a theme change pays for the 10k-line rebuild.
    fn mirror_document(&self, th: &Theme, cx: &App) -> (Arc<Document>, u64) {
        let key = MirrorDocKey {
            generation: self.session.content_generation(),
            cols: self.grid.cols,
            rows: self.grid.rows,
            eff: self.appearance.effective(&theme::outer_choice(cx)),
            mode: self.mode.clone(),
            inherit: self.appearance.inherit_theme,
            theme_gen: theme::theme_gen(cx),
        };
        let next_rev = {
            let cache = self.mirror_doc.borrow();
            match &*cache {
                Some((k, rev, doc)) if *k == key => return (doc.clone(), *rev),
                Some((_, rev, _)) => rev + 1,
                None => 1,
            }
        };
        let doc = Arc::new(
            PaneSource {
                pane: self,
                theme: th,
            }
            .document(RowBudget::all()),
        );
        *self.mirror_doc.borrow_mut() = Some((key, next_rev, doc.clone()));
        (doc, next_rev)
    }

    pub fn mirror_snapshot(&self, cx: &App) -> MirrorSnapshot {
        let th = self.resolved_theme(cx);
        // Mirror the live pane's anchor-to-top inverted read: bottom-anchor the
        // rows (prompt to the bottom) THEN reverse, so the FOCUS reader shows the
        // prompt on TOP with older output flowing down, exactly like the pane.
        // Crawl keeps its own bottom-anchor look (handled in the modal), so it is
        // excluded — matching the live render's `anchor_top() && !th.crawl` gate.
        // Off (the default) leaves `lines` untouched → byte-identical to before.
        // Alt-screen shell TUIs (vim/htop) must not be inverted in the mirror
        // either, but agent TUIs (Codex) still need the prompt-first read.
        let alt_screen_active = self
            .session
            .term
            .lock()
            .mode()
            .contains(TermMode::ALT_SCREEN);
        let agent_mode = self.mode.is_agent();
        let inverted = should_invert(anchor_top(), th.crawl, alt_screen_active, agent_mode);
        let mut lines = self.styled_lines(&th, inverted);
        if inverted {
            let wraps = if agent_mode {
                Vec::new()
            } else {
                self.row_wraps()
            };
            let (new_lines, perm) = invert_logical_read(lines, &wraps, agent_mode);
            lines = new_lines;
            // #149: the FOCUS mirror gets the same visual-order selection as the
            // live pane (the highlight was skipped in styled_lines).
            self.apply_visual_selection(&mut lines, &perm, &th);
        }
        // The pane's own resolved CRT curvature + glare, so the FOCUS reader can
        // inherit the look on demand (flat 0/0/0 for a flat pane → no-op).
        let (k1, k2) = crate::theme::warp_coeffs(th.warp);
        // The reader's document — scrollback included, so shrinking its text can
        // reveal MORE content instead of merely smaller content. The budget is a
        // whole retained scrollback ("the entire terminal convo" — operator
        // decision, 2026-08-31), rebuilt only when the terminal's content
        // generation moves. Building a 10k-line document with styled runs every
        // render frame would jank the modal; the cache makes a mouse-move frame
        // an Arc clone, and only real PTY output (or a theme/width change) pays
        // for a rebuild. The key mirrors `resolved_theme`'s own memo exactly, so
        // "same key" *guarantees* the same colours went into the runs.
        let (doc, doc_rev) = self.mirror_document(&th, cx);
        MirrorSnapshot {
            doc,
            doc_rev,
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

    /// The pane's live cwd, cheaply (no agent-session scan) — polled by the
    /// workspace's dir-logo sweep and read at picker-open / pick time.
    pub fn current_cwd(&self) -> Option<String> {
        crate::session::capture_cwd(self.session.master.as_ref(), self.session.shell_pid)
    }

    /// This pane's shell pid — the kernel handle behind its identity. Ephemeral
    /// (recycles across a resume); the durable key is the agent session. Read by
    /// the read-only MCP snapshot.
    pub fn shell_pid(&self) -> u32 {
        self.session.shell_pid
    }

    /// Whether this pane is floating a click-target popup of its OWN over the
    /// glass — the ⋯ header-overflow menu, the right-click context menu, or the
    /// BELL+ tray. The workspace's overlay flags can't see pane-local state, so
    /// `Workspace::render` asks each visible leaf this and folds the answer into
    /// [`crate::warp::set_suppressed`]: a pane popup bows with the barrel warp
    /// while gpui keeps hit-testing its flat layout box, so the glass must read
    /// flat while one is up. Any new pane-owned floating MENU belongs in this OR
    /// — decoration that carries no click target (the 🎰 reels, the bell toast)
    /// deliberately does not, since bending costs nothing there.
    pub fn popup_open(&self) -> bool {
        self.hdr_overflow.is_some() || self.ctx_menu.is_some()
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
        // A restored note comes back POSTED, never composing: the window just
        // opened and the cursor belongs to the shell, not to a piece of paper.
        let note = restore
            .note
            .clone()
            .filter(|(text, _)| !text.trim().is_empty())
            .map(|(text, seed)| crate::sticky::Sticky {
                text,
                seed,
                edit: None,
            });
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
                        let detected = foreground_mode(master, view.session.shell_pid);
                        // Sticky agent detection (spec §4): an agent runs child
                        // processes (bash/node/rg) as the terminal's foreground group
                        // while working, momentarily reclassifying the pane as Shell —
                        // which FLICKERS the anchor-top inversion frame to frame (the
                        // "sometimes / middle-screen prompt" bug). Keep the agent mode
                        // through that, but only while the ALTERNATE SCREEN is active
                        // (the agent's live TUI); when the agent exits and the plain
                        // shell returns on the normal screen, we correctly demote.
                        let on_alt = view
                            .session
                            .term
                            .lock()
                            .mode()
                            .contains(TermMode::ALT_SCREEN);
                        let mode = if view.mode.is_agent() && !detected.is_agent() && on_alt {
                            view.mode.clone()
                        } else {
                            detected
                        };
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
                                // repaint the mother bar: the tab 🤖 pulse
                                // starts/stops on this edge
                                cx.emit(AgentWorkingChanged);
                                if thinking {
                                    view.think_since = Some(Instant::now());
                                    view.not_thinking_since = None;
                                } else {
                                    // Transitioned to not-thinking; debounce to avoid false
                                    // positives from transient state changes (error messages, etc).
                                    view.not_thinking_since = Some(Instant::now());
                                }
                            }
                            // COME INTERACT: the agent stopped because it needs a
                            // HUMAN — a picker or permission prompt is on screen,
                            // not a finished turn. Live state, never latched: it
                            // clears the moment the prompt is answered. Drives
                            // the tab ❓ pulse, the header "❓ your turn", and
                            // upgrades the finish notification to "needs you".
                            // ONE bottom-rows scan feeds this and the ✅/❌
                            // classification when the bell latches below.
                            let recent = view.recent_lines(14);
                            let needs = view.mode.is_agent() && !thinking && wants_human(&recent);
                            if needs != view.needs_input {
                                view.needs_input = needs;
                                cx.emit(AgentWorkingChanged);
                                cx.notify();
                            }
                            // Only ring the bell if we've been not-thinking for 300ms+ AND
                            // the original thinking period was real (> 1200ms).
                            if !thinking && view.mode.is_agent() {
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
                                            // classify the finish while the
                                            // stop-state is still on screen
                                            view.bell_blocked = looks_blocked(&recent);
                                            view.bell_player.play();
                                            view.think_since = None;
                                            view.not_thinking_since = None;
                                            // the workspace decides whether this
                                            // becomes a system notification
                                            cx.emit(AgentDone);
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
            dir_logo: None,
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
            born: Instant::now(),
            paint_offset: 0,
            paint_inverted: false,
            paint_to_grid: None,
            mode: PaneMode::Shell,
            appearance: PaneTheme::default(),
            ctx_menu: None,
            bell: false,
            needs_input: false,
            bell_blocked: false,
            bell_player: crate::bell::BellPlayer::default(),
            hdr_overflow: None,
            copy_hint: None,
            copy_flash: None,
            was_focused: false,
            pending_grid: None,
            last_scroll_offset: None,
            seeking: false,
            theme_cache: RefCell::new(None),
            mirror_doc: RefCell::new(None),
            gamba: crate::gamba::Reels::new(seed),
            last_think_scan: Instant::now()
                .checked_sub(std::time::Duration::from_secs(1))
                .unwrap_or_else(Instant::now),
            being_read: false,
            note,
            peeled: None,
            note_hover: None,
            kbd_sel: None,
            think_since: None,
            not_thinking_since: None,
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

    /// The most recent USER prompt visible in this pane — its first line (marker
    /// stripped) plus any wrapped continuation, up to `max_lines`. Empty if none.
    /// Skips the empty live-input box (a prompt marker with no text). Used by the
    /// agent-wall card so an IDLE agent shows what you last ASKED it instead of a
    /// blank input window.
    pub fn last_human_message(&self, max_lines: usize) -> Vec<String> {
        let term = self.session.term.lock();
        let grid = term.grid();
        let cols = grid.columns();
        let screen = grid.screen_lines() as i32;
        let hist = grid.history_size() as i32;
        // Read a grid line by absolute index — NEGATIVE indices are SCROLLBACK, so
        // the prompt is found even when the agent's reply has scrolled it off the
        // visible screen (the whole point: an idle agent's last ask).
        let read = |line: i32| -> String {
            let row = &grid[Line(line)];
            let mut s = String::with_capacity(cols);
            for col in 0..cols {
                let cell = &row[Column(col)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                s.push(if cell.c == '\0' { ' ' } else { cell.c });
            }
            s.trim_end().to_string()
        };
        let strip = |t: &str| -> String {
            t.trim_start()
                .trim_start_matches(|c| {
                    matches!(c, '\u{276f}' | '>' | '\u{258c}' | '\u{00b7}' | ' ')
                })
                .trim()
                .to_string()
        };
        // walk UP from the bottom (incl. scrollback) to the last human-input line
        // that actually carries text (skip the empty live input box).
        let start = (-hist..screen)
            .rev()
            .find(|&l| is_human_input_line(&read(l)) && !strip(&read(l)).is_empty());
        let Some(start) = start else {
            return Vec::new();
        };
        let mut out = vec![strip(&read(start))];
        for l in (start + 1)..screen {
            let s = read(l);
            let t = s.trim();
            if t.is_empty()
                || is_human_input_line(&s)
                || t.starts_with('?')
                || t.starts_with("esc ")
            {
                break;
            }
            if out.len() >= max_lines {
                break;
            }
            out.push(t.to_string());
        }
        out
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
    /// the workspace to badge the owning tab; cleared by [`Self::ack_bell`] on
    /// the focus-in edge (looking at the pane is the acknowledgement).
    pub fn has_bell(&self) -> bool {
        self.bell
    }

    /// Is the agent in this pane actively working RIGHT NOW? Reads the cached
    /// thinking state the 120ms effects-clock scan maintains — the same signal
    /// that rolls the GAMBA reels and arms the bell edge — so the mother-bar 🤖
    /// pulse can ask every frame for free. Flips are announced via
    /// [`AgentWorkingChanged`].
    /// This pane's screen rect as it was last painted, in window coordinates.
    /// The workspace reads it on the way OUT — a closing pane has to leave its
    /// stage behind, because by the time the shutdown plays the pane is gone.
    pub fn screen_rect(&self) -> Option<Bounds<Pixels>> {
        *self.content_bounds.lock().unwrap()
    }

    pub fn agent_working(&self) -> bool {
        self.mode.is_agent() && self.gamba.is_thinking()
    }

    /// Is the agent stopped WAITING ON A HUMAN right now (picker / permission
    /// prompt on screen)? Cached by the same 120ms scan; live, never latched.
    pub fn needs_input(&self) -> bool {
        self.needs_input
    }

    /// Did the latched finish classify as BLOCKED (an error banner was on
    /// screen at ring time)? Only meaningful while [`Self::has_bell`] is true.
    pub fn bell_blocked(&self) -> bool {
        self.bell && self.bell_blocked
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
            //
            // A BEL is an ACCELERATOR for a finish, never an independent trigger.
            // This arm used to take every bell byte as a completed turn, carrying
            // none of the guards the thinking-scan path below has — no real-spell
            // test, no debounce, no scroll-settle gate — so two ordinary things
            // both rang it falsely:
            //
            //   - a TUI beeping mid-turn (scrolling past the end of its own
            //     history is the one that gets hit), and
            //   - a NEW window, where restoring the session replays each resumed
            //     agent's transcript and with it every bell that transcript
            //     already contains, so opening a window announced a burst of
            //     completions for turns that finished hours ago.
            //
            // Two conditions, one for each. `think_since` is Some only while a
            // turn has STARTED in this pane and not yet produced a finish (it is
            // set when the spinner appears and cleared when a finish rings), so
            // it is exactly "there is an outstanding turn to complete" — a
            // freshly restored pane has none, and a replayed bell is ignored. And
            // if the spinner is still on the LIVE bottom screen the agent is
            // demonstrably working, so the beep is UI noise. It also drops a
            // duplicate bell arriving after a finish already rang.
            //
            // Suppressing a REAL bell costs nothing: the 120ms scan rings the
            // same finish ~300ms later through the debounced path, so the failure
            // mode is a slightly later notification rather than a lost one. That
            // asymmetry is the whole argument — a false finish interrupts Parker,
            // clears the tab's badge, and lies about an agent that is still going.
            TermEvent::Bell
                if self.mode.is_agent()
                    && self.think_since.is_some()
                    && !self.agent_is_thinking() =>
            {
                self.bell = true;
                self.bell_blocked = looks_blocked(&self.recent_lines(14));
                self.bell_player.play();
                cx.emit(AgentDone);
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
    /// Read grid rows as styled text over an arbitrary line range, INCLUDING
    /// scrollback. `first`/`last` are alacritty grid line indices, where negative
    /// lines are history — `grid.topmost_line()` is the oldest row retained.
    ///
    /// This is the scrollback counterpart to [`Self::styled_lines`], which can only
    /// ever see the visible viewport because it walks `display_iter`. Colour comes
    /// from the cells themselves, so history keeps the ANSI colours the program
    /// emitted; TD's own overlays (the syntax pass, human-input tinting, selection,
    /// cursor) are viewport-only and deliberately not reproduced here — they are
    /// decoration on the live screen, not properties of the text.
    fn grid_rows_in(&self, first: i32, last: i32, th: &Theme) -> Vec<DocLine> {
        let term = self.session.term.lock();
        let grid = term.grid();
        let cols = self.grid.cols;
        let mut out = Vec::with_capacity((last - first + 1).max(0) as usize);
        for l in first..=last {
            let row = &grid[alacritty_terminal::index::Line(l)];
            let mut text = String::with_capacity(cols);
            let mut runs: Vec<TextRun> = Vec::new();
            for c in 0..cols {
                let cell = &row[alacritty_terminal::index::Column(c)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                let ch = if cell.c == '\0' { ' ' } else { cell.c };
                let color = ansi_to_hsla(cell.fg, th, th.text);
                let len = ch.len_utf8();
                text.push(ch);
                // coalesce identical adjacent styles so a row is a handful of runs
                match runs.last_mut() {
                    Some(prev) if prev.color == color => prev.len += len,
                    _ => runs.push(TextRun {
                        len,
                        font: grid_font(th, FontWeight::default()),
                        color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }),
                }
            }
            out.push(DocLine::new(text, runs));
        }
        out
    }

    /// Read the whole visible grid as characters plus per-row soft-wrap flags,
    /// both in grid-viewport order. One term lock, one `display_iter` pass.
    ///
    /// Rows are space-padded to the full column count — that is what
    /// `stitch_wrapped_line` wants, since it maps a click column into the
    /// concatenated line. Anything doing width arithmetic (the copy reflow) must
    /// trim the padding first; see [`grid_logical_lines`].
    /// Resolve the copy affordance for a pointer position: the logical line under
    /// it, if that line reads as a command, plus the painted rows it covers.
    /// `None` whenever nothing should be offered. Callers gate on Alt being held.
    fn copy_hint_at(&self, pos: gpui::Point<Pixels>) -> Option<CopyHint> {
        // A drag-selection owns the pointer — never compete with it.
        if self.selecting || self.has_selection() {
            return None;
        }
        // Alt-screen apps (vim, htop) lay their rows out as a canvas, not as
        // flowed text, so "rejoin what the width broke" means nothing there.
        if self
            .session
            .term
            .lock()
            .mode()
            .contains(TermMode::ALT_SCREEN)
        {
            return None;
        }
        let (prow, _, _) = self.viewport_cell(pos);
        let grow = self.paint_row_to_grid_row(prow);
        let line = self
            .grid_logical_lines()
            .into_iter()
            .find(|l| l.first <= grow && grow <= l.last)?;
        if !is_copyable_command(&line.text) {
            return None;
        }
        // Map the GRID span back to painted rows. The paint transform can be an
        // arbitrary permutation (anchor-to-top reverses in groups), so invert it
        // by scanning every row rather than assuming the identity mapping.
        let painted: Vec<usize> = (0..self.grid.rows)
            .filter(|p| {
                let g = self.paint_row_to_grid_row(*p);
                line.first <= g && g <= line.last
            })
            .collect();
        Some(CopyHint {
            text: line.text,
            first_paint: *painted.first()?,
            last_paint: *painted.last()?,
        })
    }

    fn grid_snapshot(&self) -> (Vec<Vec<char>>, Vec<bool>) {
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
        (grid, wraps)
    }

    /// The visible grid reconstructed into logical lines, each carrying the grid
    /// rows it spans. Trailing padding is trimmed per row FIRST — without that
    /// every row reads as full-width to `wrap_join`'s width test and the whole
    /// screen glues into a single line.
    fn grid_logical_lines(&self) -> Vec<LogicalLine> {
        let (grid, _) = self.grid_snapshot();
        let rows: Vec<String> = grid
            .iter()
            .map(|r| r.iter().collect::<String>().trim_end().to_string())
            .collect();
        let borrowed: Vec<&str> = rows.iter().map(String::as_str).collect();
        reflow_wrapped_copy_spans(&borrowed, self.grid.cols)
    }

    fn link_under(&self, pos: gpui::Point<Pixels>) -> Option<String> {
        let (vrow, vcol, _) = self.viewport_cell(pos);
        // Map the painted/visual row back to the grid viewport row it shows
        // (identity in the default un-anchored path; inverts the anchor-to-top
        // flip + any bottom-anchor offset otherwise).
        let vrow = self.paint_row_to_grid_row(vrow);
        // Read the whole visible grid plus per-row soft-wrap flags, then stitch
        // the clicked row to its neighbours so a URL/path wrapped across rows is
        // recognised as one token (see `stitch_wrapped_line`).
        let (grid, wraps) = self.grid_snapshot();
        let (line, col) = stitch_wrapped_line(&grid, &wraps, vrow, vcol);
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

    /// Type `bytes` at the PTY: drop any selection, snap the view back to the
    /// prompt, and mark the pane as having pending input.
    fn send(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
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

    /// Apply a paint-overlay pick to THIS pane. `None` re-attaches the pane to
    /// the outer/desktop look (the live inherit link); `Some(set)` pins the
    /// pane's theme group with that colour set and clears the wheel overrides
    /// (seed/T/C/human) so the set's own signature palette paints cleanly.
    /// Identity — texture, CRT effects, grade, warp — is deliberately left
    /// alone: paint is colour, not look.
    fn paint_pick(&mut self, pick: Option<theme::Dynamic>, cx: &mut Context<Self>) {
        match pick {
            None => self.appearance.inherit_theme = true,
            Some(d) => {
                let outer = theme::outer_choice(cx);
                let mut g = theme::ThemeGroup::of(&self.appearance.effective(&outer));
                g.dynamic = d;
                g.seed = None;
                g.text = None;
                g.complement = None;
                g.human = None;
                // A colour set works FROM the theme's own colours, so a desktop
                // palette still painted over them would silently win. The two
                // shelves are ONE choice: picking on either clears the other.
                g.palette = None;
                self.appearance.set_theme(g);
            }
        }
        cx.emit(PaintApplied);
        cx.notify();
    }

    /// Paint this pane with a DESKTOP palette — one of Omarchy's on-board colour
    /// schemes ([`crate::palette`]). The mirror of [`Self::paint_pick`] for the
    /// other shelf, and it clears the same overrides for the same reason: one
    /// pick is meant to be the whole statement, not a layer on a pile.
    fn paint_palette(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        let outer = theme::outer_choice(cx);
        let mut g = theme::ThemeGroup::of(&self.appearance.effective(&outer));
        g.palette = id;
        // The seed/set/T/C machinery would re-derive colours ON TOP of the
        // borrowed ones — exactly what "looks like the rest of the desktop" must
        // not do. Stand the palette up clean; the tray can still tweak it after.
        g.dynamic = theme::Dynamic::Plain;
        g.seed = None;
        g.text = None;
        g.complement = None;
        g.human = None;
        self.appearance.set_theme(g);
        cx.emit(PaintApplied);
        cx.notify();
    }

    /// One keystroke offered to the raised PAINT overlay.
    ///
    /// Split out of [`Self::on_key`] so the overlay's whole keyboard reads in one
    /// place, and so `on_key` keeps exactly ONE `stop_propagation` — the thing
    /// `pane_on_key_only_stops_propagation_when_it_consumes_the_key` guards.
    ///
    /// PAINT mode owns the keyboard while it is up: it is the topmost surface
    /// across ALL panes at once, so nothing it handles may reach the PTY
    /// underneath (an ESC byte into a running agent kills it; a stray `w` lands
    /// in someone's shell). Only the FOCUSED pane runs this, which is what makes
    /// "the letter paints the selected terminal" true with no selection state to
    /// keep — the spotlight in the overlay and the focus here are one fact.
    fn paint_key(&mut self, ks: &Keystroke, cx: &mut Context<Self>) -> PaintKey {
        let m = &ks.modifiers;
        let plain = !m.control && !m.alt && !m.platform;
        let key = ks.key.as_str();
        if key == "escape" {
            theme::set_paint_mode(cx, false);
            cx.notify();
            return PaintKey::Bubble;
        }
        if !plain {
            // Modified chords still pass, so the overlay is never a trap.
            return PaintKey::Pass;
        }
        // Bare arrows walk the wall — bubbled on purpose to the Workspace, which
        // owns the geometry and does the directional focus move.
        if matches!(key, "left" | "right" | "up" | "down") {
            return PaintKey::Bubble;
        }
        // `z` turns the SHELF (colour sets ⇄ desktop palettes), `shift+z` turns
        // it back. It is the one letter allowed to be a verb rather than a name,
        // because nothing on either shelf is spelled with one — guarded by
        // `the_shelf_key_is_not_a_chord_on_either_shelf`.
        if key.eq_ignore_ascii_case("z") {
            theme::cycle_paint_shelf(cx, if m.shift { -1 } else { 1 });
            return PaintKey::Took;
        }
        if !m.shift {
            // On the COLOUR SETS shelf, `d` and a set's first letter come out of
            // ONE table (`Dynamic::paint_chord`), so the tiles, the legend and
            // this handler cannot drift apart; those letters are unique and never
            // `d`/`s` (`named_sets_spell_a_unique_paint_alphabet`).
            //
            // On the DESKTOP PALETTES shelf the names belong to Omarchy and DO
            // collide — `catppuccin` beside `catppuccin-latte`, three `r`s — so a
            // letter CYCLES through the palettes sharing it, painting each one on
            // the way past. `d` still hands the pane back to the desktop on both
            // shelves, which is why it is checked before the cycle.
            if theme::paint_shelf(cx) == 1 && !key.eq_ignore_ascii_case("d") {
                let mut ch = key.chars();
                if let (Some(c), None) = (ch.next(), ch.next()) {
                    let worn = self.worn_palette(cx);
                    if let Some(id) = crate::palette::next_for_letter(cx, c, worn.as_deref()) {
                        self.paint_palette(Some(id), cx);
                        return PaintKey::Took;
                    }
                }
            } else if let Some(pick) = theme::Dynamic::paint_chord(key) {
                self.paint_pick(pick, cx);
                return PaintKey::Took;
            }
        }
        // Anything else printable is swallowed rather than typed: the overlay is
        // modal, and a miss should be a no-op, not a keystroke into whatever is
        // running behind it.
        if key.chars().count() == 1 {
            return PaintKey::Took;
        }
        PaintKey::Pass
    }

    /// The desktop palette this pane is actually WEARING, if any — the cursor the
    /// letter-cycle walks from. A pane that follows the outer scope wears nothing
    /// of its own, so the next `r` starts the `r` group from the top rather than
    /// from wherever the mother happens to sit.
    fn worn_palette(&self, cx: &App) -> Option<String> {
        if self.appearance.inherit_theme {
            return None;
        }
        self.appearance.effective(&theme::outer_choice(cx)).palette
    }

    /// This pane's note box on the glass — recomputed from the LIVE content rect
    /// rather than cached, so a resize can never leave the paper and the thing
    /// you click in different places.
    fn note_layout(&self) -> Option<crate::sticky::Layout> {
        let note = self.note.as_ref()?;
        let bounds = (*self.content_bounds.lock().ok()?)?;
        crate::sticky::layout(bounds, note.tilt())
    }

    /// `alt+s`, or a click on the paper: stick a note on, or pick the pen back up
    /// on the one already here.
    ///
    /// Alt+S never DESTROYS. One chord that both creates and destroys makes a
    /// blind press a coin flip over thirty seconds of typing, so taking a note
    /// down is always an explicit second gesture.
    pub fn sticky_open(&mut self, cx: &mut Context<Self>) {
        match self.note.as_mut() {
            Some(note) => note.reopen(),
            None => {
                // A note peeled off a moment ago comes back SELECTED, so this is
                // both "write a note" and the undo for a stray alt+backspace,
                // and the first keystroke still replaces it either way.
                let prefill = self.peeled.clone().unwrap_or_default();
                self.note = Some(crate::sticky::Sticky::composing(
                    &prefill,
                    crate::sticky::seed(),
                ));
            }
        }
        cx.notify();
    }

    /// Take the note down. `false` when there was none, so the caller can let the
    /// keystroke through to the PTY instead of silently eating it.
    pub fn sticky_peel(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(note) = self.note.take() else {
            return false;
        };
        let text = match note.edit {
            Some(edit) => edit.buf.text(),
            None => note.text,
        };
        if !text.trim().is_empty() {
            self.peeled = Some(text);
        }
        self.note_hover = None;
        cx.emit(StickyChanged);
        cx.notify();
        true
    }

    /// This pane's note as `(text, seed)` for the state file.
    ///
    /// A note being COMPOSED saves what has been typed, not the draft's
    /// ancestor: an unattended save — a window closing, a crash — should not
    /// throw away the sentence you were half way through.
    pub fn saved_note(&self) -> Option<(String, u32)> {
        let note = self.note.as_ref()?;
        let text = match &note.edit {
            Some(edit) => edit.buf.text(),
            None => note.text.clone(),
        };
        let text = text.trim().to_string();
        (!text.is_empty()).then_some((text, note.seed))
    }

    pub fn sticky_composing(&self) -> bool {
        self.note.as_ref().is_some_and(|n| n.is_editing())
    }

    /// Post what is being written and hand the cursor back.
    ///
    /// The single commit path: Enter, a second `alt+s`, and a click away from the
    /// note all land here, so "done" cannot come to mean three different things.
    /// A note emptied first is thrown away instead — the one way a commit can
    /// remove a note, and it takes deleting the text to reach it.
    pub fn sticky_commit(&mut self, cx: &mut Context<Self>) {
        let Some(note) = self.note.as_mut() else {
            return;
        };
        let Some(edit) = note.edit.as_ref() else {
            return;
        };
        let text = edit.buf.text().trim().to_string();
        if text.is_empty() {
            self.sticky_peel(cx);
            return;
        }
        note.text = text;
        note.edit = None;
        cx.emit(StickyChanged);
        cx.notify();
    }

    /// `alt+s`. A toggle: it gives the note the cursor and takes it back.
    pub fn sticky_toggle(&mut self, cx: &mut Context<Self>) {
        match crate::sticky::alt_s(self.sticky_composing()) {
            crate::sticky::Act::Post => self.sticky_commit(cx),
            _ => self.sticky_open(cx),
        }
    }

    /// One keystroke offered to the note's composer. `true` when the note took
    /// it, which is EVERY key while composing: the composer is modal, and a
    /// letter that leaked past it would land in whatever is running behind.
    ///
    /// Esc lives here and only here. While composing it reverts the draft and
    /// the PTY never sees it — same contract as the inline rename box. Once the
    /// note is posted this function isn't reached at all, so a posted note
    /// cannot swallow the Esc that stops a running agent.
    fn sticky_key(&mut self, ks: &Keystroke, cx: &mut Context<Self>) -> bool {
        let Some(note) = self.note.as_mut() else {
            return false;
        };
        let Some(edit) = note.edit.as_mut() else {
            return false;
        };
        match crate::sticky::press(true, ks.key.as_str()) {
            crate::sticky::Press::Pass => return false,
            crate::sticky::Press::Post => self.sticky_commit(cx),
            crate::sticky::Press::Revert => {
                match edit.restore.take() {
                    Some(previous) => {
                        note.text = previous;
                        note.edit = None;
                    }
                    None => {
                        self.note = None;
                    }
                }
                cx.notify();
            }
            crate::sticky::Press::Write => {
                edit.buf.apply(
                    ks.key.as_str(),
                    &ks.modifiers,
                    ks.key_char.as_deref(),
                    crate::sticky::MAX_CHARS,
                );
                cx.notify();
            }
        }
        true
    }

    /// A click resolved against the note, inverting its tilt.
    ///
    /// `true` when the note SWALLOWED the click, so the pane's own selection
    /// never starts under the paper. Committing does not swallow it: a click
    /// away from a note being written posts the note AND lands in the terminal
    /// where you aimed it, the same as clicking out of the tab-rename box.
    fn sticky_click(&mut self, at: gpui::Point<Pixels>, cx: &mut Context<Self>) -> bool {
        let hit = self
            .note_layout()
            .and_then(|l| crate::sticky::Hit::at(at, &l));
        match crate::sticky::click(self.sticky_composing(), hit) {
            crate::sticky::Act::Peel => {
                self.sticky_peel(cx);
                true
            }
            crate::sticky::Act::Open => {
                self.sticky_open(cx);
                true
            }
            crate::sticky::Act::Post => {
                self.sticky_commit(cx);
                false
            }
            crate::sticky::Act::Pass => false,
        }
    }

    /// Track what the pointer is over so the peel corner can curl under it —
    /// the only thing that teaches the mouse gesture.
    fn sticky_hover(&mut self, at: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let was = self.note_hover;
        self.note_hover = self
            .note_layout()
            .and_then(|l| crate::sticky::Hit::at(at, &l));
        if was != self.note_hover {
            cx.notify();
        }
    }

    // INVARIANT: a key this handler DECLINES must bubble to the Workspace.
    // Every workspace chord — alt+arrows (pane nav), alt+v/h and ctrl+alt+r/d
    // (split), ctrl+pgup/pgdn (tabs) — reaches the Workspace only by bubbling out of
    // here while a pane holds focus, so swallowing the fall-through kills all
    // of them at once with no compile error and nothing else failing.
    //
    // Consuming a key is different from swallowing one: `cx.stop_propagation()`
    // is correct where this handler OWNS the key and returns immediately (F1
    // does exactly that — the workspace root also binds it, and a bubbled F1
    // toggled the modal twice in one frame). The rule is therefore not "never
    // stop propagation" but "never stop it without returning".
    // Guarded by `pane_on_key_only_stops_propagation_when_it_consumes_the_key`.
    fn on_key(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        // Typing into the pane is attention too — covers the frozen-badge case
        // where the pane kept idle keyboard focus across the whole latch, so
        // no focus-in edge ever fired. No-op when nothing is latched.
        self.ack_bell(cx);
        // F1 opens the help modal (handled by the workspace), never the PTY.
        // STOP the event here: the workspace root also binds F1 (its no-pane-
        // focused fallback), and a bubbled F1 toggled `help_open` a SECOND time
        // in the same frame — the modal opened and closed instantly, so F1 read
        // as dead everywhere except the outer bar's `?` button.
        if ks.key.as_str() == "f1" {
            cx.emit(OpenHelp);
            cx.stop_propagation();
            return;
        }
        // PAINT mode owns the keyboard while it is up — it is the topmost
        // surface across ALL panes at once, so nothing it handles may reach the
        // PTY underneath (an ESC byte into a running agent kills it; a stray
        // `w` lands in someone's shell).
        //
        // Only the FOCUSED pane runs this handler, which is what makes "the
        // letter paints the selected terminal" true without any selection state
        // to keep: the spotlight in the overlay and the focus this handler
        // rides are the same fact.
        if theme::paint_mode(cx) {
            match self.paint_key(ks, cx) {
                PaintKey::Took => {
                    cx.stop_propagation();
                    return;
                }
                PaintKey::Bubble => return,
                PaintKey::Pass => {}
            }
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
        // While this pane is mirrored in the FOCUS modal, a plain Esc closes the
        // modal (the workspace handles it) rather than reaching the PTY — every
        // OTHER keystroke still flows straight to this terminal, so you keep
        // directing the agent while you read it big.
        if self.being_read && ks.key.as_str() == "escape" {
            cx.emit(CloseFocusRead);
            return;
        }
        // Same contract for the paging keys: while the modal is up they drive the
        // READER's view (page through the mirrored convo, jump to its ends), not
        // the pane's own scrollback — that's the surface you are actually reading.
        // Every other keystroke still flows to the PTY below.
        if self.being_read {
            if let Some(nav) = read_nav_key(ks.key.as_str(), &ks.modifiers) {
                cx.emit(FocusReadNav(nav));
                return;
            }
        }
        // The note's OWN chords, ahead of the composer that would otherwise eat
        // them. A composer that swallows the chord for "put the pen down" leaves
        // Enter as the only way out, which is exactly the bug this ordering
        // exists to prevent: alt+s reached `EditBuffer::apply`, which drops
        // alt-modified keys, so pressing it again did nothing at all.
        //
        // Alt+S sticks a note to this pane, picks the pen back up on the one
        // already there, and — pressed again while writing — posts it. Taken in
        // the pane rather than at the Workspace because the note belongs to the
        // pane and the pane's handler runs first: routing it through the
        // Workspace would let `keystroke_bytes` send `ESC s` on the way past. It
        // costs the shell alt+s, which nothing standard binds.
        if ks.modifiers.alt
            && !ks.modifiers.control
            && !ks.modifiers.shift
            && ks.key.as_str() == "s"
        {
            self.sticky_toggle(cx);
            cx.stop_propagation();
            return;
        }
        // Alt+Backspace peels it off — but ONLY when a note is actually stuck
        // here. With no note the chord falls through untouched and readline still
        // gets its backward-kill-word, so the shell loses the binding exactly
        // when the pane is visibly carrying a note and not otherwise. Peeling by
        // accident costs one keystroke: alt+s brings the text straight back.
        if ks.modifiers.alt
            && !ks.modifiers.control
            && ks.key.as_str() == "backspace"
            && self.sticky_peel(cx)
        {
            cx.stop_propagation();
            return;
        }
        // A note holding the cursor owns the keyboard, rename-box style: every
        // OTHER keystroke writes on the paper instead of reaching the PTY, Enter
        // posts it, Esc reverts it. This runs ONLY while composing — see
        // `sticky_key` for why a posted note must never see a key.
        if self.sticky_composing() && self.sticky_key(ks, cx) {
            cx.stop_propagation();
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
        // Paging the pane itself. PageUp/PageDown page the scrollback in AGENT
        // panes — a Claude/Codex session keeps its whole convo in our history and
        // never binds the keys itself, while a shell keeps them (Arch's inputrc
        // binds PageUp to history-search, and `send` would snap the view to the
        // prompt anyway). ctrl+Home / ctrl+End jump to the ends of the scrollback
        // in EVERY pane — no shell or readline binding wants those chords. Both
        // defer to an app that owns its own view (alt screen or mouse reporting:
        // less, vim, tmux), where our scrollback is not the surface on screen.
        // In an inverted (anchor-top) pane the keys keep their MEANING — PageUp
        // steps toward older, ctrl+Home is the oldest row — wherever older is
        // painted; the wheel's per-gesture flip is about physical direction,
        // which a named key doesn't have.
        if let Some(nav) = read_nav_key(ks.key.as_str(), m) {
            let paging = matches!(nav, ReadNav::PageUp | ReadNav::PageDown);
            if !paging || self.mode.is_agent() {
                let tmode = *self.session.term.lock().mode();
                if !tmode.contains(TermMode::ALT_SCREEN) && !tmode.intersects(TermMode::MOUSE_MODE)
                {
                    let scroll = match nav {
                        ReadNav::PageUp => Scroll::PageUp,
                        ReadNav::PageDown => Scroll::PageDown,
                        ReadNav::Top => Scroll::Top,
                        ReadNav::Bottom => Scroll::Bottom,
                    };
                    self.session.term.lock().scroll_display(scroll);
                    cx.notify();
                    return;
                }
            }
        }
        if let Some(bytes) = keystroke_bytes(ks) {
            self.send(bytes, cx);
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
        if lines == 0 {
            return;
        }
        self.scroll_accum -= lines as f32;
        // Inverted (anchor-top) panes read "older is DOWN". Compute the gesture's
        // intent ONCE — "reveal older?" — so EVERY scroll leg honours the inversion,
        // not just the local-scrollback leg (Leg 3). This is the Claude alt-screen
        // fix: Leg 2 (arrow keys) previously sent the un-flipped arrow, so a physical
        // scroll-DOWN scrolled toward NEWER. See docs/spec/anchor-top-read.md §5.
        // Leg 3 keeps its own `-lines` flip below (it uses the exact line count).
        let up = (lines > 0) ^ self.paint_inverted; // true = reveal OLDER (app "up")
        let count = (lines.unsigned_abs() as usize).clamp(1, 8);

        let mode = *self.session.term.lock().mode();
        // Diagnostic for the anchor-top read (see docs/spec/anchor-top-read.md §7).
        if std::env::var("TD_ANCHORDEBUG").is_ok() {
            eprintln!(
                "[anchor] scroll pane={:?} paint_inverted={} reveal_older={} mouse_mode={} alt_screen={} alt_scroll={} (spec §5)",
                self.mode,
                self.paint_inverted,
                up,
                mode.intersects(TermMode::MOUSE_MODE),
                mode.contains(TermMode::ALT_SCREEN),
                mode.contains(TermMode::ALTERNATE_SCROLL),
            );
        }
        // 1) The app has mouse reporting on (tmux, some TUIs) → send wheel button
        //    events so it handles the scroll itself.
        // 2) The app is on the ALTERNATE SCREEN with alternate-scroll (the default
        //    for Claude Code, less, man, vim) → translate the wheel to arrow keys,
        //    because the alt screen has NO scrollback for us to move. THIS is the
        //    fix: without it the wheel does nothing inside a full-screen app.
        // 3) Otherwise (a normal-screen shell) → scroll OUR scrollback as before;
        //    inverted anchor-top panes flip "older is down".
        if mode.intersects(TermMode::MOUSE_MODE) {
            let (vrow, vcol, _) = self.viewport_cell(self.last_mouse);
            let grow = self.paint_row_to_grid_row(vrow);
            let button: u8 = if up { 64 } else { 65 };
            let sgr = mode.contains(TermMode::SGR_MOUSE);
            let mut out = Vec::new();
            for _ in 0..count {
                if sgr {
                    out.extend_from_slice(
                        format!("\u{1b}[<{};{};{}M", button, vcol + 1, grow + 1).as_bytes(),
                    );
                } else {
                    let enc = |v: usize| (32 + (v + 1).min(223)) as u8;
                    out.extend_from_slice(&[0x1b, b'[', b'M', 32 + button, enc(vcol), enc(grow)]);
                }
            }
            self.session.notifier.notify(out);
        } else if mode.contains(TermMode::ALT_SCREEN) && mode.contains(TermMode::ALTERNATE_SCROLL) {
            let app_cursor = mode.contains(TermMode::APP_CURSOR);
            let seq: &[u8] = match (up, app_cursor) {
                (true, false) => b"\x1b[A",
                (false, false) => b"\x1b[B",
                (true, true) => b"\x1bOA",
                (false, true) => b"\x1bOB",
            };
            let mut out = Vec::with_capacity(seq.len() * count);
            for _ in 0..count {
                out.extend_from_slice(seq);
            }
            self.session.notifier.notify(out);
        } else {
            // In inverted (anchor-to-top) mode "older is DOWN", so flip the sign.
            let d = if self.paint_inverted { -lines } else { lines };
            self.session.term.lock().scroll_display(Scroll::Delta(d));
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
        // A full-screen agent TUI (Claude Code 2.x paints on the ALTERNATE
        // screen) keeps its own scrollback and leaves the terminal none: our
        // grid history is empty, so there are no `❯` lines to walk and this
        // used to be a silent no-op. Drive the AGENT's scrollback instead.
        if self
            .session
            .term
            .lock()
            .mode()
            .contains(TermMode::ALT_SCREEN)
        {
            self.seek_agent_prompt(next, cx);
            return;
        }
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

    /// `Alt+↑/↓` (and the ▲/▼ header buttons) in an agent pane whose TUI owns the
    /// whole screen. On the alternate screen there is no terminal-side history to
    /// move a viewport through, so we ask the AGENT to scroll and watch what it
    /// repaints, stepping until one of the human's own prompt lines (`❯`/`>`)
    /// reaches the top row. The input is a synthetic WHEEL notch, never an arrow
    /// key: Claude Code binds ↑/↓ to prompt-history recall, so arrows would
    /// rewrite the composer instead of scrolling. If the app never asked for
    /// mouse reports we fall back to PageUp/PageDown, which it maps to a
    /// half-screen scroll — a coarser landing, but still nothing typed.
    fn seek_agent_prompt(&mut self, next: bool, cx: &mut Context<Self>) {
        if self.seeking {
            return;
        }
        let mode = *self.session.term.lock().mode();
        let up = !next; // "previous message" = scroll toward OLDER output
        let (step, coarse) = if mode.intersects(TermMode::MOUSE_MODE) {
            (
                wheel_step_bytes(up, mode.contains(TermMode::SGR_MOUSE)),
                false,
            )
        } else if up {
            (b"\x1b[5~".to_vec(), true)
        } else {
            (b"\x1b[6~".to_vec(), true)
        };
        // A line-at-a-time walk needs a long leash (one agent turn can be hundreds
        // of rows); half-screen jumps need very few. Either way the walk is
        // bounded, and a screen that stops changing means we reached the end of
        // the agent's own scrollback.
        let max_steps = if coarse { 24 } else { 400 };
        self.seeking = true;
        cx.spawn(async move |this, cx| {
            let mut stalls = 0;
            for _ in 0..max_steps {
                let before = match this.update(cx, |view: &mut TerminalView, _cx| {
                    view.session.notifier.notify(step.clone());
                    view.screen_signature()
                }) {
                    Ok(sig) => sig,
                    Err(_) => return, // pane closed mid-walk
                };
                // Wait for the agent to actually repaint before reading the top
                // row — a busy turn must not let us race ahead of its redraw.
                let mut painted = false;
                for _ in 0..14 {
                    cx.background_executor()
                        .timer(Duration::from_millis(6))
                        .await;
                    match this.update(cx, |view: &mut TerminalView, _cx| view.screen_signature()) {
                        Ok(sig) if sig != before => {
                            painted = true;
                            break;
                        }
                        Ok(_) => {}
                        Err(_) => return,
                    }
                }
                if !painted {
                    stalls += 1;
                    if stalls >= 3 {
                        break; // top (or bottom) of the agent's scrollback
                    }
                    continue;
                }
                stalls = 0;
                match this.update(cx, |view: &mut TerminalView, _cx| view.top_is_human(coarse)) {
                    Ok(true) => break,
                    Ok(false) => {}
                    Err(_) => return,
                }
            }
            let _ = this.update(cx, |view: &mut TerminalView, cx| {
                view.seeking = false;
                cx.notify();
            });
        })
        .detach();
    }

    /// A fingerprint of the visible screen — cheap enough to poll during a seek,
    /// and all we need to tell "the agent repainted" from "nothing moved".
    fn screen_signature(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for row in self.live_rows() {
            row.hash(&mut h);
        }
        h.finish()
    }

    /// Has one of the human's own prompt lines arrived at the top of the screen?
    /// The line walk tests the first two rows (a one-line step lands the prompt
    /// exactly at the top); the coarse PageUp fallback accepts a hit anywhere in
    /// the upper half, since a half-screen jump cannot place it precisely. Empty
    /// carets (the live composer, which every agent paints) never count.
    fn top_is_human(&self, coarse: bool) -> bool {
        let rows = self.live_rows();
        let depth = if coarse { (rows.len() / 2).max(1) } else { 2 };
        rows.iter().take(depth).any(|r| {
            is_human_input_line(r)
                && !r
                    .trim_start()
                    .trim_start_matches(|c| {
                        matches!(c, '\u{276f}' | '>' | '\u{258c}' | '\u{00b7}' | ' ')
                    })
                    .trim()
                    .is_empty()
        })
    }

    /// Map a logical grid `point` to a `(painted_row, col)` visual position under
    /// the current inverted permutation `perm` (`perm[p]` = grid viewport row drawn
    /// at painted row `p`). The viewport row is `point.line + display_offset`,
    /// clamped on-screen; the painted row is the `perm` slot that draws it.
    fn point_to_painted(
        &self,
        point: TermPoint,
        perm: &[usize],
        display_offset: usize,
    ) -> (usize, usize) {
        let rows = self.grid.rows.max(1);
        let vr = (point.line.0 + display_offset as i32).clamp(0, rows as i32 - 1) as usize;
        let painted = perm
            .iter()
            .position(|&g| g == vr)
            .unwrap_or_else(|| vr.min(rows - 1));
        (painted, point.column.0)
    }

    /// #149: re-apply the current selection to already-permuted `lines` in VISUAL
    /// (painted) order. styled_lines skipped the logical highlight in inverted
    /// mode; here every painted row in the visual span between the two drag
    /// endpoints is inverted, so a cross-section drag fills the cells you SEE
    /// (not the whole reversed blocks a logical range would cover).
    fn apply_visual_selection(
        &self,
        lines: &mut [(String, Vec<TextRun>)],
        perm: &[usize],
        th: &Theme,
    ) {
        let (start, end, is_block, display_offset) = {
            let term = self.session.term.lock();
            let Some(range) = term.selection.as_ref().and_then(|s| s.to_range(&*term)) else {
                return;
            };
            (
                range.start,
                range.end,
                range.is_block,
                term.grid().display_offset(),
            )
        };
        let a = self.point_to_painted(start, perm, display_offset);
        let b = self.point_to_painted(end, perm, display_offset);
        let last_col = self.grid.cols.saturating_sub(1);
        // Match styled_lines' INVERSE default background (graded th.bg) so the
        // visual highlight is the same colour as the non-inverted selection.
        let default_bg = GradeCoeffs::new(&th.grade).apply(th.bg, Channel::Bg);
        for (p, c_lo, c_hi) in visual_selection_spans(a, b, last_col, is_block) {
            if let Some(row) = lines.get_mut(p) {
                invert_run_range(row, c_lo, c_hi, default_bg);
            }
        }
    }

    /// #149: the selected text in VISUAL reading order for an inverted pane — walk
    /// the painted-row span and read each row's grid cells through `paint_to_grid`,
    /// so what-you-copy == what-you-see. `None` when nothing is selected. The
    /// default (non-inverted) path keeps alacritty's logical `selection_to_string`.
    fn visual_selection_to_string(&self) -> Option<String> {
        let perm = self.paint_to_grid.as_ref()?;
        let rows = self.grid.rows;
        let cols = self.grid.cols;
        let (start, end, is_block, display_offset, grid) = {
            let term = self.session.term.lock();
            let range = term.selection.as_ref().and_then(|s| s.to_range(&*term))?;
            let content = term.renderable_content();
            let display_offset = content.display_offset;
            let mut grid = vec![vec![' '; cols]; rows];
            for indexed in content.display_iter {
                let r = indexed.point.line.0 + display_offset as i32;
                if r < 0 || r as usize >= rows {
                    continue;
                }
                if indexed.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                let c = indexed.point.column.0;
                if c < cols {
                    grid[r as usize][c] = if indexed.cell.c == '\0' {
                        ' '
                    } else {
                        indexed.cell.c
                    };
                }
            }
            (range.start, range.end, range.is_block, display_offset, grid)
        };
        let a = self.point_to_painted(start, perm, display_offset);
        let b = self.point_to_painted(end, perm, display_offset);
        let last_col = cols.saturating_sub(1);
        let mut out = String::new();
        for (p, c_lo, c_hi) in visual_selection_spans(a, b, last_col, is_block) {
            let g = *perm.get(p).unwrap_or(&p);
            if g >= rows {
                continue;
            }
            let line: String = (c_lo..=c_hi).map(|c| grid[g][c]).collect();
            out.push_str(line.trim_end());
            out.push('\n');
        }
        let out = out.trim_end_matches('\n').to_string();
        (!out.is_empty()).then_some(out)
    }

    /// Copy the current selection to the system clipboard (no-op if empty). In an
    /// inverted (anchor-to-top) pane the copy follows VISUAL reading order (#149).
    /// The current selection as clipboard-ready text: pick the right extractor
    /// for the pane's paint mode (inverted read vs. native logical order), then
    /// smart-reflow so app-hard-wrapped agent output pastes as logical lines
    /// (see `reflow_wrapped_copy`). Returns `None` for an empty selection. One
    /// entry point so every copy surface — Ctrl+C, cut, select-to-PRIMARY —
    /// yields identical text.
    fn selection_clipboard_text(&self) -> Option<String> {
        let raw = if self.paint_inverted {
            self.visual_selection_to_string()
        } else {
            self.session.term.lock().selection_to_string()
        }?;
        if raw.is_empty() {
            return None;
        }
        Some(reflow_wrapped_copy(&raw, self.grid.cols))
    }

    fn copy_selection(&self, cx: &mut Context<Self>) {
        let text = self.selection_clipboard_text();
        if let Some(text) = text {
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
        let text = match self.selection_clipboard_text() {
            Some(t) => t,
            None => return,
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

    /// Acknowledge the "agent finished" bell: clear the flag (tab badge and
    /// "● done" status revert) and stop a ping still sounding. Fired on the
    /// focus-in edge — looking at the pane IS the acknowledgement — and by the
    /// workspace when a notification click jumps here (the jump focuses the
    /// pane, so this is also just the focus edge arriving).
    pub fn ack_bell(&mut self, cx: &mut Context<Self>) {
        if !self.bell {
            return;
        }
        self.bell = false;
        self.bell_blocked = false;
        self.bell_player.stop();
        self.not_thinking_since = None;
        cx.notify();
    }

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if std::env::var("TD_KEYDEBUG").is_ok() {
            eprintln!("pane mousedown at {:?}", ev.position);
        }
        // Clicking into a terminal makes it the focused leaf, so keystrokes and
        // the split buttons (which target the focused pane) follow the pane the
        // user is actually working in — not whichever pane happened to start focused.
        window.focus(&self.focus_handle, cx);
        // A click is attention even when it makes no focus EDGE (a bell that
        // latched while this pane already held idle focus froze the ✅ badge —
        // the edge never came). ack_bell is a no-op when nothing is latched.
        self.ack_bell(cx);
        // The note is a physical object lying on the glass, so a click lands on
        // it before anything underneath: the bottom-left corner tears it off,
        // anywhere else picks the pen back up. Resolved here rather than with a
        // gpui click target for the same reason as the copy chip below — the
        // paper is tilted, and gpui would hit-test its flat layout box.
        if ev.button == MouseButton::Left && self.sticky_click(ev.position, cx) {
            cx.stop_propagation();
            return;
        }
        // Alt+click on the armed copy chip takes the reconstructed line. Handled
        // here rather than as a click target on the chip element: the chip is
        // painted inside the warped tube, so gpui would hit-test it flat and land
        // beside it, whereas `viewport_cell` (which `copy_hint_at` goes through)
        // already inverts the warp. Falls through untouched when no chip is armed,
        // so Alt+click still reaches an app that asked for mouse reporting.
        if ev.button == MouseButton::Left && ev.modifiers.alt {
            if let Some(hint) = self.copy_hint_at(ev.position) {
                cx.write_to_clipboard(ClipboardItem::new_string(hint.text.clone()));
                cx.write_to_primary(ClipboardItem::new_string(hint.text.clone()));
                self.copy_flash = Some(Instant::now());
                self.copy_hint = Some(hint);
                cx.stop_propagation();
                cx.notify();
                return;
            }
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
        // Super+Ctrl-click REVEALS the file instead of opening it: the file
        // manager comes up with the item selected, which is the other question a
        // printed path provokes ("where does this live?", "what else is beside
        // it?"). Tested before the plain Ctrl-click branch below, because that
        // one also matches when Super is held. A target with nothing on disk to
        // show — an http link — falls through and simply opens, as before.
        if ev.button == MouseButton::Left && ev.modifiers.platform && ev.modifiers.control {
            if let Some(item) = self
                .link_under(ev.position)
                .as_deref()
                .and_then(reveal_target)
            {
                reveal_with_system(&item);
                cx.notify();
                return;
            }
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

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, _w: &mut Window, cx: &mut Context<Self>) {
        // The peel corner curls under the pointer. No-op with no note stuck here,
        // and it only notifies on a change, so ordinary mousing costs nothing.
        self.sticky_hover(ev.position, cx);
        // The Alt-held copy affordance. Gated on the modifier so the grid is only
        // re-read while Alt is actually down — ordinary mousing costs nothing, and
        // nothing can arm by accident.
        let hint = if ev.modifiers.alt {
            self.copy_hint_at(ev.position)
        } else {
            None
        };
        if hint != self.copy_hint {
            if hint.is_none() || self.copy_hint.is_none() {
                self.copy_flash = None;
            }
            self.copy_hint = hint;
            cx.notify();
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
        self.selecting = false;
        self.autoscroll = 0.;
        // Finishing a drag publishes the selection to the X11 PRIMARY selection
        // (classic select-to-copy → middle-click paste). Empty selections (plain
        // clicks) are skipped; no-op on platforms without a primary selection.
        if let Some(text) = self.selection_clipboard_text() {
            cx.write_to_primary(ClipboardItem::new_string(text));
        }
    }

    /// Snapshot the viewport into one styled line per row.
    /// Build the per-row styled text from the grid. `sel_visual` ⇒ inverted
    /// (anchor-to-top) mode: SKIP the logical selection highlight here, because the
    /// caller re-applies it in painted/visual order after the permutation (#149).
    /// In the default path `sel_visual` is false and this is byte-identical.
    fn styled_lines(&self, th: &Theme, sel_visual: bool) -> Vec<(String, Vec<TextRun>)> {
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
            if !sel_visual && selection.is_some_and(|s| s.contains(indexed.point)) {
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

/// Whether the inverted anchor-to-top read should apply. The read reverses row
/// order so the prompt lands on TOP. That is correct for scrolling shells and
/// conversational agents, including Codex's alternate-screen TUI, but corrupts
/// non-agent full-screen TUIs (vim/htop/less) whose box drawing assumes a fixed
/// top-to-bottom layout.
///
/// TARGET BEHAVIOUR SPEC: `docs/spec/anchor-top-read.md`. Read it before changing
/// the inverted read or the wheel handling — the behaviour must hold for shell,
/// Codex, AND Claude, and a fix for one client must not regress another (PR #142
/// was closed for regressing Codex). `TD_ANCHORDEBUG=1` dumps the per-pane state.
fn should_invert(anchor_top: bool, crawl: bool, alt_screen: bool, agent_mode: bool) -> bool {
    anchor_top && !crawl && (!alt_screen || agent_mode)
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

/// Per-PAINTED-row inclusive column spans for a VISUAL (painted-order) selection
/// between two viewport endpoints `a` and `b` (each `(painted_row, col)`, in any
/// order). This is the fix for #149: in an inverted (anchor-to-top) pane the
/// display is a group-reversed permutation, so a logically-contiguous range paints
/// as a visually DISJOINT set. Defining the selection in painted-row order instead
/// makes "drag from here to there" fill exactly the cells you see between them.
///
/// - `block` ⇒ rectangular: every row clipped to the same `[c0..=c1]` column band.
/// - else (linear): the first visual row runs from the anchor column to the row
///   end, whole rows fill the middle, and the last visual row runs from the start
///   to the active column — i.e. crossing into a section first highlights only its
///   TOP visual line, then fills downward. Pure + unit-tested.
fn visual_selection_spans(
    a: (usize, usize),
    b: (usize, usize),
    last_col: usize,
    block: bool,
) -> Vec<(usize, usize, usize)> {
    if block {
        let (r0, r1) = (a.0.min(b.0), a.0.max(b.0));
        let (c0, c1) = (a.1.min(b.1).min(last_col), a.1.max(b.1).min(last_col));
        return (r0..=r1).map(|r| (r, c0, c1)).collect();
    }
    // Linear: order the endpoints in visual reading order (row, then column).
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    if lo.0 == hi.0 {
        return vec![(lo.0, lo.1.min(last_col), hi.1.min(last_col))];
    }
    let mut out = Vec::with_capacity(hi.0 - lo.0 + 1);
    out.push((lo.0, lo.1.min(last_col), last_col)); // anchor row → end
    for r in (lo.0 + 1)..hi.0 {
        out.push((r, 0, last_col)); // middle rows: whole row
    }
    out.push((hi.0, 0, hi.1.min(last_col))); // start → active row
    out
}

/// Flip INVERSE (swap fg/bg) on char columns `c_lo..=c_hi` of one already-built
/// painted row, in place: expand the row's coalesced runs to per-cell styles, swap
/// the selected cells (a cell with no background takes `default_bg`), then
/// re-coalesce. Char column == cell here (one char pushed per non-spacer cell).
/// This applies a selection highlight AFTER the inverted permutation, where rows
/// are already in visual order, so the highlight is visually contiguous (#149).
fn invert_run_range(row: &mut (String, Vec<TextRun>), c_lo: usize, c_hi: usize, default_bg: Hsla) {
    let (text, runs) = row;
    let nchars = text.chars().count();
    if nchars == 0 || c_lo >= nchars {
        return;
    }
    let c_hi = c_hi.min(nchars - 1);
    // Expand coalesced runs → one TextRun per char (run.len is BYTES, so walk the
    // text slice each run covers and clone the style per char).
    let mut cells: Vec<TextRun> = Vec::with_capacity(nchars);
    let mut byte = 0usize;
    for run in runs.iter() {
        let end = (byte + run.len).min(text.len());
        for ch in text[byte..end].chars() {
            cells.push(TextRun {
                len: ch.len_utf8(),
                font: run.font.clone(),
                color: run.color,
                background_color: run.background_color,
                underline: run.underline,
                strikethrough: run.strikethrough,
            });
        }
        byte = end;
    }
    for (i, c) in cells.iter_mut().enumerate() {
        if i >= c_lo && i <= c_hi {
            let new_fg = c.background_color.unwrap_or(default_bg);
            c.background_color = Some(c.color);
            c.color = new_fg;
        }
    }
    // Re-coalesce adjacent cells that share a style (mirrors styled_lines).
    let mut merged: Vec<TextRun> = Vec::with_capacity(cells.len());
    for c in cells {
        if let Some(last) = merged.last_mut() {
            if last.color == c.color
                && last.background_color == c.background_color
                && last.font.weight == c.font.weight
                && last.underline.is_some() == c.underline.is_some()
            {
                last.len += c.len;
                continue;
            }
        }
        merged.push(c);
    }
    *runs = merged;
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
        // Group by conversational TURN, not paragraph (spec §3a). A turn opens at a
        // human-input line (`is_human_input_line`) and runs until the NEXT human-input
        // line — the human prompt plus the agent's full multi-step reply, keeping the
        // blank lines *within* a message in natural order. Reversing TURN order then
        // puts the newest turn on top while each message still reads top→bottom. The
        // old "split on every blank line" reversed a message's own paragraphs/steps
        // (the "out of order" bug on Claude, whose replies blank-separate their steps).
        // Rows before the first human turn form a leading group.
        let mut i = 0;
        while i <= last {
            if is_blank[i] {
                i += 1;
                continue;
            }
            let start = i;
            i += 1;
            while i <= last && !is_human_input_line(&lines[i].0) {
                i += 1;
            }
            // Trailing blanks become inter-turn separators / bottom padding.
            let mut end = i;
            while end > start && is_blank[end - 1] {
                end -= 1;
            }
            groups.push((start..end).collect());
        }
        // Fallback: if the client's human-prompt caret wasn't recognised, turn
        // detection yields a SINGLE group → reversing is a no-op → the pane reads
        // BOTTOM-anchored instead of inverted (the regression). Split on blank-line
        // blocks so anchor-top still inverts (prompt on top); the finer turn-grouping
        // resumes once the caret is recognised (`is_human_input_line`). Spec §3a/§6.
        if groups.len() < 2 {
            groups.clear();
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
    // Blanks INSIDE a turn group are kept in place (a message's own paragraph
    // breaks); only the ungrouped blanks are free to become inter-turn separators
    // and bottom padding.
    let grouped: Vec<bool> = {
        let mut g = vec![false; n];
        for grp in &groups {
            for &r in grp {
                g[r] = true;
            }
        }
        g
    };
    let mut blanks: Vec<usize> = (0..n).filter(|&i| is_blank[i] && !grouped[i]).collect();
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
    if std::env::var("TD_ANCHORDEBUG").is_ok() {
        // Group boundaries reveal §3a over-splitting (one message → many blocks).
        let sizes: Vec<usize> = groups.iter().map(|g| g.len()).collect();
        eprintln!(
            "[anchor] invert_logical_read block_mode={} groups={} sizes={:?}",
            block_mode,
            groups.len(),
            sizes
        );
    }
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

/// Startup diagnostic: if the ACTIVE font family isn't installed, describe the
/// fallback that will be used (so a silent substitution can never hide again).
/// Returns None when the wanted family is present. Call after
/// `init_font_registry`.
///
/// `want` is the family the running theme actually asks for — NOT the ship
/// default. It used to be the hardcoded literal "JetBrains Mono", so a user who
/// had deliberately configured an installed family still got told, on every
/// single launch, that a font they never asked for was missing. That is the
/// first thing the program says to you, and it was crying wolf. Guarded by
/// `the_font_diagnostic_is_silent_about_a_family_that_resolves`.
pub fn font_diagnostic(want: &str) -> Option<String> {
    let got = resolve_family(want);
    if got == want {
        return None;
    }
    let n = AVAILABLE_FONTS.get().map(|v| v.len()).unwrap_or(0);
    Some(format!(
        "font '{want}' not installed; falling back to '{got}' ({n} families available). \
         Install {want} for the intended look."
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

/// Cursor & nav keys carry modifiers in xterm's CSI 1;<mod> form, so ctrl+→/←
/// skip by word, shift+→/← extend selection, and so on. Split out of
/// `keystroke_bytes` so the workspace can re-encode a chord it declined.
pub(crate) fn cursor_key_bytes(ks: &Keystroke) -> Option<Vec<u8>> {
    let fin = match ks.key.as_str() {
        "up" => b'A',
        "down" => b'B',
        "right" => b'C',
        "left" => b'D',
        "home" => b'H',
        "end" => b'F',
        _ => return None,
    };
    let m = &ks.modifiers;
    // xterm modifier code: 1 + shift(1) + alt(2) + ctrl(4)
    let code = 1 + u8::from(m.shift) + u8::from(m.alt) * 2 + u8::from(m.control) * 4;
    Some(if code == 1 {
        vec![0x1b, b'[', fin]
    } else {
        format!("\x1b[1;{code}{}", fin as char).into_bytes()
    })
}

/// One synthetic wheel notch, encoded the way the running app asked for its
/// mouse reports: SGR when it negotiated 1006, otherwise the legacy X10 form.
/// Row and column are reported as 1,1 — the app scrolls its own view, so where
/// the pointer happens to sit is irrelevant. Used by the agent prompt seek (see
/// [`TerminalView::seek_agent_prompt`]), which scrolls a full-screen TUI that
/// keeps its scrollback to itself.
fn wheel_step_bytes(up: bool, sgr: bool) -> Vec<u8> {
    let button: u8 = if up { 64 } else { 65 };
    if sgr {
        format!("\u{1b}[<{button};1;1M").into_bytes()
    } else {
        vec![0x1b, b'[', b'M', 32 + button, 33, 33]
    }
}

/// Map a paging keystroke to a [`ReadNav`], or `None` for anything else. Plain
/// PageUp/PageDown page; ctrl+Home / ctrl+End jump to the ends. Any other
/// modifier combination is someone else's chord (ctrl+PageUp switches tabs,
/// plain Home/End belong to the shell), so it must NOT match here.
fn read_nav_key(key: &str, m: &gpui::Modifiers) -> Option<ReadNav> {
    if m.alt || m.shift || m.platform || m.function {
        return None;
    }
    match (key, m.control) {
        ("pageup", false) => Some(ReadNav::PageUp),
        ("pagedown", false) => Some(ReadNav::PageDown),
        ("home", true) => Some(ReadNav::Top),
        ("end", true) => Some(ReadNav::Bottom),
        _ => None,
    }
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
        // alt+arrows move pane focus by direction; alt+r opens the FOCUS reader (the 👓 header
        // glyph it replaces is gone); alt+v / alt+h and the ctrl+alt chords
        // split — all owned by the Workspace. Taking alt+r costs readline's
        // revert-line, alt+v its page-scroll and alt+h its mark-paragraph —
        // fair trades for one-hand pane chords.
        if matches!(
            ks.key.as_str(),
            "left" | "right" | "up" | "down" | "r" | "v" | "h"
        ) || m.control
        {
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
    // ctrl+arrows belong to the SHELL, not the workspace: they fall through to
    // `cursor_key_bytes` as CSI 1;5 so readline/zsh word-jump works in every
    // pane, split or not. Pane focus is alt+arrows, which costs the terminal
    // nothing it had a use for.
    if let Some(bytes) = cursor_key_bytes(ks) {
        return Some(bytes);
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
                // Reveal is offered only where there is a file to point at, so
                // the item never appears dead on an http link.
                let item = reveal_target(&l);
                menu = menu.child(row("Open link  ↗", true).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |v, _, _, cx| {
                        open_with_system(&l);
                        v.ctx_menu = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ));
                if let Some(item) = item {
                    menu = menu.child(row("Reveal in folder  ⌖", true).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |v, _, _, cx| {
                            reveal_with_system(&item);
                            v.ctx_menu = None;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ));
                }
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
        // PAINT mode — the wall-wide palette overlay (raised by `terminal-delight
        // ctl paint …`, e.g. the Omarchy bar's palette widget). EVERY pane draws
        // its own glyph grid at once, so a wall of terminals recolours like
        // dipping a brush: paint this one, arrow to the next, Esc when the wall
        // reads right. The tiles are the theme tray's own colour-set vocabulary
        // (Dynamic::NAMED + signatures) — which is also, deliberately, the
        // desktop's variant set — so a paint pick produces exactly what the tray
        // would have, and exactly what the bar would have.
        //
        // It plays like Omarchy's own picker, mouse optional: the SELECTED pane
        // is spotlit (everything else keeps the scrim), bare arrows walk the
        // selection, and a tile's FIRST LETTER paints it. That letter is drawn
        // the way it is pressed — bigger, bolder, underlined — so the keyboard
        // is legible from the tile itself instead of a legend somewhere else.
        //
        // TWO SHELVES of vocabulary, cycled with `z` (`shift+z` back) or by
        // clicking a pill:
        //  · COLOUR SETS — `Dynamic::NAMED`, which is also the desktop's variant
        //    set, so a pick produces what the tray and the bar would have.
        //  · DESKTOP PALETTES — every Omarchy theme installed on the machine
        //    ([`crate::palette`]), which replaces the colours outright and leaves
        //    the theme's texture, so a pane can match every other window.
        // `z` can be the shelf key precisely because no set and no theme is
        // spelled with one (guarded by `named_sets_spell_a_unique_paint_alphabet`
        // and `the_shelf_key_is_not_a_chord_on_either_shelf`).
        let paint_el = theme::paint_mode(cx).then(|| {
            let outer = theme::outer_choice(cx);
            let eff = self.appearance.effective(&outer);
            let following = self.appearance.inherit_theme;
            // the spotlight: only the focused pane is lit, and only IT answers
            // the letters — so the keyboard always has one unambiguous target.
            let sel = self.focus_handle(cx).is_focused(window);
            let shelf = theme::paint_shelf(cx);
            let shelf_count = theme::shelf_count(cx);
            let palettes = crate::palette::chips(cx);
            let (acc, surf, txt, faint) = (th.accent, th.surface, th.text, th.faint);
            let ff = th.font_family.clone();
            // ONE tile shape serves both shelves: a face (a set's glyph, or a
            // palette's own screen in miniature), the chord letter with the rest
            // of the name beside it, an optional second name line, and the colour
            // the pick paints with.
            let tile = move |face: AnyElement,
                             key: char,
                             rest: String,
                             second: String,
                             swatch: Option<Hsla>,
                             lit: bool| {
                div()
                    .w(px(62.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(3.))
                    .py(px(6.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(if lit { acc } else { acc.alpha(0.28) })
                    .bg(if lit {
                        acc.alpha(0.16)
                    } else {
                        surf.alpha(0.92)
                    })
                    .cursor_pointer()
                    .hover(move |s| s.bg(acc.alpha(0.20)))
                    // Face and name sit in FIXED-height boxes so every tile is the
                    // same height whether its name takes one line or two —
                    // otherwise the rows stagger and the grid reads as scrunched.
                    .child(div().h(px(21.)).flex().items_center().child(face))
                    .child(
                        div()
                            .h(px(21.))
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .child(
                                // The name, with its chord worn loud — bigger,
                                // heavier, underlined, and inked in the TILE'S OWN
                                // colour, so the key you press also previews the
                                // colour it applies. Two children on a shared
                                // baseline rather than one styled string: the
                                // initial needs its own size, weight and underline,
                                // and gpui styles per element, not per run.
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_baseline()
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(gpui::FontWeight::BLACK)
                                            .text_color(swatch.unwrap_or(acc))
                                            .underline()
                                            .text_decoration_2()
                                            .text_decoration_color(swatch.unwrap_or(acc))
                                            .child(key.to_string()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(8.))
                                            .text_color(if lit { txt.alpha(0.9) } else { faint })
                                            .child(rest),
                                    ),
                            )
                            // A desktop palette's name is the desktop's, not ours:
                            // it breaks on the hyphen onto a second line rather
                            // than folding mid-word (CATPPUCCIN / LATTE).
                            .when(!second.is_empty(), |d| {
                                d.child(
                                    div()
                                        .text_size(px(8.))
                                        .text_color(if lit { txt.alpha(0.9) } else { faint })
                                        .child(second),
                                )
                            }),
                    )
                    .child(
                        div()
                            .h(px(3.))
                            .w(px(36.))
                            .rounded(px(2.))
                            .bg(swatch.unwrap_or(acc.alpha(0.0))),
                    )
            };
            let glyph_face = |g: &str| {
                div()
                    .text_size(px(17.))
                    .child(g.to_string())
                    .into_any_element()
            };
            // A palette's face is a 30×18 mock screen filled with the scheme's OWN
            // background, carrying three of its own hues as pips. Omarchy themes
            // ship no emoji, and a name alone cannot tell gruvbox from everforest
            // — the miniature can.
            let screen_face = move |bg: Hsla, chips: [Hsla; 3], light: bool| {
                div()
                    .relative()
                    .w(px(30.))
                    .h(px(18.))
                    .rounded(px(3.))
                    .border_1()
                    .border_color(txt.alpha(0.25))
                    .bg(bg)
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(3.))
                    .children(chips.map(|c| div().w(px(4.)).h(px(4.)).rounded_full().bg(c)))
                    // A LIGHT scheme turns the whole pane into a bright screen —
                    // worth knowing BEFORE the key is pressed, not after.
                    .when(light, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(-1.))
                                .right(px(1.))
                                .text_size(px(7.))
                                .text_color(chips[0])
                                .child("☀"),
                        )
                    })
                    .into_any_element()
            };
            let mut grid = div()
                .flex()
                .flex_row()
                .flex_wrap()
                .justify_center()
                .items_start()
                .gap(px(6.))
                .max_w(px(430.));
            // ⟲ D leads every shelf: "stop deciding, follow the desktop again".
            grid = grid.child(
                tile(
                    glyph_face("⟲"),
                    'D',
                    "ESKTOP".into(),
                    String::new(),
                    None,
                    following,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|v, _, _, cx| {
                        v.paint_pick(None, cx);
                        cx.stop_propagation();
                    }),
                ),
            );
            if shelf == 0 {
                for d in theme::Dynamic::NAMED.iter() {
                    // A colour set is only "the one you're on" when no palette has
                    // since painted over it — otherwise every set would read lit.
                    let lit = !following && eff.palette.is_none() && eff.dynamic.same_kind(d);
                    let pick = d.clone();
                    grid = grid.child(
                        tile(
                            glyph_face(d.glyph()),
                            d.paint_letter(),
                            d.label()[1..].to_uppercase(),
                            String::new(),
                            d.swatch(),
                            lit,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |v, _, _, cx| {
                                v.paint_pick(Some(pick.clone()), cx);
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
            } else {
                for p in palettes {
                    let lit = !following && eff.palette.as_deref() == Some(p.id.as_str());
                    let id = p.id.clone();
                    grid = grid.child(
                        tile(
                            screen_face(p.bg, p.chips, p.light),
                            p.letter,
                            p.rest.clone(),
                            p.second.clone(),
                            Some(p.chips[0]),
                            lit,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |v, _, _, cx| {
                                v.paint_palette(Some(id.clone()), cx);
                                cx.stop_propagation();
                            }),
                        ),
                    );
                }
            }
            // The shelf pills — the visible half of `z`. Shown only when there is
            // somewhere to switch TO (no Omarchy installed → no second shelf).
            let pills = (shelf_count > 1).then(|| {
                let mut row = div().flex().flex_row().gap(px(4.));
                for (i, name) in theme::PAINT_SHELVES.iter().enumerate() {
                    let on = i as u8 == shelf;
                    row = row.child(
                        div()
                            .px(px(9.))
                            .py(px(2.))
                            .rounded(px(9.))
                            .border_1()
                            .border_color(if on { acc } else { acc.alpha(0.25) })
                            .bg(if on { acc.alpha(0.18) } else { surf.alpha(0.5) })
                            .text_size(px(8.))
                            .text_color(if on { txt } else { faint })
                            .cursor_pointer()
                            .hover(move |s| s.bg(acc.alpha(0.28)))
                            .child(*name)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |_v, _, _, cx| {
                                    theme::set_paint_shelf(cx, i as u8);
                                    cx.stop_propagation();
                                }),
                            ),
                    );
                }
                row
            });
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                // the spotlight, and the whole reason arrows are worth pressing:
                // the pane the letters will hit sits under a THIN scrim, every
                // other pane under a heavy one. Which terminal you are painting
                // is answered from across the room, without reading a word.
                .bg(gpui::hsla(0., 0., 0., if sel { 0.42 } else { 0.78 }))
                .flex()
                .items_center()
                .justify_center()
                .when(sel, |d| {
                    // …and the selected pane is FRAMED, inset so the band reads
                    // as "this window" rather than as another card border.
                    d.border(px(3.)).border_color(txt.alpha(0.9))
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(8.))
                        // unselected cards fade back: same layout, quieter ink,
                        // so the wall still shows what every pane is wearing.
                        .opacity(if sel { 1.0 } else { 0.34 })
                        .font_family(ff)
                        .child(
                            div()
                                .text_size(px(11.))
                                .font_weight(if sel {
                                    gpui::FontWeight::EXTRA_BOLD
                                } else {
                                    gpui::FontWeight::NORMAL
                                })
                                .text_color(txt)
                                .child("PAINT THIS PANE"),
                        )
                        .children(pills)
                        .child(grid)
                        .child(
                            // The legend is the contract: everything named here
                            // works, and nothing that works is unnamed.
                            div()
                                .text_size(px(9.))
                                .text_color(faint)
                                .child(if shelf_count > 1 {
                                    "↔ select · letter paints · z shelf · d desktop · esc done"
                                } else {
                                    "↔ select · letter paints · d desktop · esc done"
                                }),
                        ),
                )
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
        // Does this pane's tube fire on its way in? Only a BENT tube ignites —
        // read off the resolved coefficients rather than the dial, so the gate
        // follows whatever `warp_coeffs` decides "bent" means. The elapsed test
        // is what makes it one-shot: once the window has passed the overlay
        // stops being built at all, for the rest of the pane's life.
        let frozen_ignition = crt::ignition_freeze();
        let ignites = {
            let (k1, k2) = self.warp_k;
            (k1.abs() > 1e-5 || k2.abs() > 1e-5)
                && (frozen_ignition.is_some()
                    || self.born.elapsed() < Duration::from_millis(crt::IGNITION_MS))
        };
        // edge-detected focus reporting (CSI I / CSI O) for apps that ask for it.
        // The bell persists until you actually LOOK at the pane: the focus-in
        // edge below is the acknowledgement (a click, alt+arrows, or the
        // notification jump all land here), so you never miss which agent
        // finished while you were away, and there is nothing to dismiss.
        let focused_now = self.focus_handle(cx).is_focused(window);
        if focused_now != self.was_focused {
            self.was_focused = focused_now;
            if focused_now {
                self.ack_bell(cx);
            }
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
        // The inverted-read decision is needed up-front: styled_lines SKIPS the
        // logical selection highlight when inverted, because #149 re-applies the
        // selection in painted/visual order after the permutation below (a logical
        // range paints as a visually disjoint set once groups are reversed).
        let alt_screen_active = self
            .session
            .term
            .lock()
            .mode()
            .contains(TermMode::ALT_SCREEN);
        let agent_mode = self.mode.is_agent();
        let inverted = should_invert(anchor_top(), th.crawl, alt_screen_active, agent_mode);
        // Diagnostic for the anchor-top read (see docs/spec/anchor-top-read.md §7).
        if std::env::var("TD_ANCHORDEBUG").is_ok() {
            eprintln!(
                "[anchor] render pane={:?} agent={} alt_screen={} anchor_top={} crawl={} inverted={}",
                self.mode,
                agent_mode,
                alt_screen_active,
                anchor_top(),
                th.crawl,
                inverted
            );
        }
        let mut lines = self.styled_lines(&th, inverted);
        if std::env::var("TD_ANCHORDEBUG").is_ok() {
            // The last few non-blank grid rows + whether each is detected as the
            // human prompt line — reveals a caret we don't recognise or a leading
            // box-border char that defeats detection (spec §3a/§4).
            for (t, _) in lines
                .iter()
                .rev()
                .filter(|(t, _)| !t.trim().is_empty())
                .take(3)
            {
                let head: String = t.chars().take(12).collect();
                eprintln!(
                    "[anchor]   row human_input={} head={:?}",
                    is_human_input_line(t),
                    head
                );
            }
        }
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
            let wraps = if agent_mode {
                Vec::new()
            } else {
                self.row_wraps()
            };
            let (new_lines, perm) = invert_logical_read(lines, &wraps, agent_mode);
            lines = new_lines;
            // #149: paint the selection in VISUAL order now that rows are permuted,
            // so a cross-section drag highlights the visually-contiguous span (not
            // whole reversed blocks). styled_lines skipped the logical highlight.
            self.apply_visual_selection(&mut lines, &perm, &th);
            self.paint_to_grid = Some(perm);
            self.paint_inverted = true;
            self.paint_offset = 0;
        } else {
            self.paint_offset = 0;
            self.paint_inverted = false;
            self.paint_to_grid = None;
        }
        let ps = crate::lang::current().strings();
        let status = if self.needs_input {
            // waiting on the HUMAN outranks "done": the turn isn't over, it's
            // yours. (English literal for now — the prompt phrases it detects
            // are English-only anyway; i18n rides along when they do.)
            "❓ your turn".to_string()
        } else if self.bell_blocked() {
            "✘ blocked".to_string()
        } else if self.bell {
            format!("✔ {}", ps.ph_done)
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
        let show_human = SHOW_HUMAN_NAV_GLYPH && pane_w >= 470.; // 1st: 👤 ▲▼ nav
        let show_eq = pane_w >= 410.; //    2nd: EQ / display
        let show_theme = pane_w >= 360.; //  3rd: 🎨 theme
        let show_focus = SHOW_FOCUS_GLYPH && pane_w >= 264.; // 4th & last: 👓 FOCUS
                                                             // ⋯ shows only once something is actually tucked (👤-nav is agent-only).
        let overflow = (SHOW_FOCUS_GLYPH && !show_focus)
            || !show_theme
            || !show_eq
            || (SHOW_HUMAN_NAV_GLYPH && !show_human && self.mode.is_agent());

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
            if SHOW_HUMAN_NAV_GLYPH && !show_human && self.mode.is_agent() {
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
            if SHOW_FOCUS_GLYPH && !show_focus {
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
            if let Some(path) = self.logo.clone().or_else(|| self.dir_logo.clone()) {
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

        let jiggle = self.fx.jiggle_px;
        // 🎰 GAMBA reels — shown only on the gamba DESIGN texture while the agent
        // thinks. The colour set (incl. RETRO/🎰) never triggers the reels.
        let gamba_look = crate::gamba::look_active(&th);
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
        // The Alt-held copy affordance: a border around the logical line under the
        // pointer with a ⎘ chip at its right edge. Painted INSIDE the tube, so it
        // bends with the glass like the text it frames — and deliberately carries
        // no gpui mouse handler, because gpui would hit-test it flat and miss.
        // The click is taken in `on_mouse_down`, which resolves through
        // `viewport_cell` and so already inverts the warp. That is why this needs
        // no `warp::set_suppressed` entry and never flattens the screen.
        let copy_el = self.copy_hint.clone().map(|hint| {
            let rows = (hint.last_paint - hint.first_paint + 1) as f32;
            let copied = self
                .copy_flash
                .is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(1200));
            let (acc, surf) = (th.accent, th.surface);
            div()
                .absolute()
                .left(px(grid_pad_x - 2.))
                .top(px(grid_pad_y + hint.first_paint as f32 * self.cell_h - 1.))
                .right(px(grid_pad_x - 2.))
                .h(px(rows * self.cell_h + 2.))
                .border_1()
                .border_color(acc.alpha(0.75))
                .rounded(px(4.))
                .child(
                    div()
                        .absolute()
                        .right(px(2.))
                        .top(px(-1.))
                        .px(px(6.))
                        .bg(surf)
                        .border_1()
                        .border_color(acc.alpha(0.75))
                        .rounded(px(4.))
                        .text_color(acc)
                        .text_size(px(11.))
                        .child(if copied {
                            "✓ copied"
                        } else {
                            "⎘ alt+click"
                        }),
                )
        });
        // The sticky note. Its geometry comes from the same `content_bounds` the
        // warp tube is registered from, because the note is drawn through the
        // INVERSE of that tube's distortion and the two must be measuring the
        // same rectangle or the cancellation is against the wrong curve.
        let note_el = self.note.clone().map(|note| {
            let store = self.content_bounds.clone();
            let pal = crate::sticky::paper(th.text, th.accent);
            let peeling = self.note_hover == Some(crate::sticky::Hit::Peel);
            // The pane's own curvature, and zero while a modal has flattened the
            // whole screen — the note un-bends with everything else that frame
            // rather than being the one thing that stayed compensated.
            let (k1, k2) = if crate::warp::is_suppressed() || th.crawl {
                (0.0, 0.0)
            } else {
                crate::theme::warp_coeffs(th.warp)
            };
            div().absolute().inset_0().child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, cx| {
                        let Some(content) = store.lock().ok().and_then(|b| *b) else {
                            return;
                        };
                        if let Some(mut lay) = crate::sticky::layout(content, note.tilt()) {
                            lay.pre_warp(content, k1, k2);
                            crate::sticky::paint(&note, &lay, &pal, peeling, window, cx);
                        }
                    },
                )
                .size_full(),
            )
        });

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
                    )
                    .children(copy_el)
                    // The sticky note, INSIDE the screen and therefore inside the
                    // registered warp tube. It has to be: the note is drawn
                    // pre-distorted so the barrel pass straightens it out, and
                    // any of that overdraw that lands outside the tube — over the
                    // header, say — is never straightened and shows as a smear of
                    // paper where no paper is. The screen's `overflow_hidden` is
                    // what guarantees that can't happen. Being under the glass is
                    // right too, now that the note shares the tube's curve: it
                    // takes the same scanlines and glare as everything else on it.
                    .children(note_el)
                    // The tube fires. Last child of the SCREEN, so it paints
                    // over the grid but stays inside the registered warp tube —
                    // the curvature and scanlines in the effect are the shader
                    // and the glass acting on it, not something drawn here.
                    // Gated on the barrel warp: a flat pane is not pretending to
                    // be a CRT, and a flash on it would read as a glitch.
                    .when(ignites, |el| {
                        el.child(div().absolute().inset_0().with_animation(
                            "crt-ignition",
                            Animation::new(Duration::from_millis(crt::IGNITION_MS)),
                            move |el, t| match crt::ignition(frozen_ignition.unwrap_or(t)) {
                                Some(ign) => el.child(crt::ignition_flash(ign)),
                                None => el,
                            },
                        ))
                    }),
            )
            .when(std::env::var("TD_NOGLASS").is_err(), |el| {
                el.child(crt::glass(&th, &self.fx))
            })
            // raised bezel frame sits above the glass, framing the whole pane
            .when(th.bezel > 0.001, |el| el.child(crt::bezel(&th)))
            // 🎰 the slot reels ride above the bezel, below the menus
            .children(gamba_overlay)
            .children(ctx_menu_el)
            .children(overflow_el)
            // the paint overlay is the topmost surface — painted last, above
            // every menu and tray, matching its Esc-first place in on_key
            .children(paint_el)
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
    fn the_font_diagnostic_is_silent_about_a_family_that_resolves() {
        // #163: the diagnostic used to hardcode the ship default, so configuring
        // an installed family still produced "JetBrains Mono not installed" on
        // every launch. It must speak only about the family it was ASKED about.
        //
        // AVAILABLE_FONTS is a process-wide OnceLock, so this reads back whatever
        // is actually registered rather than assuming this test set it — that
        // keeps the assertions true whatever order the suite runs in.
        let _ = AVAILABLE_FONTS.set(MONO_FALLBACKS.iter().map(|s| s.to_string()).collect());
        let installed = AVAILABLE_FONTS.get().expect("registry seeded");
        let present = installed.first().expect("at least one family").clone();

        assert_eq!(
            font_diagnostic(&present),
            None,
            "a family that resolves must not warn (got a warning for {present})"
        );

        // A family that is absent must name ITSELF, never the ship default.
        let msg = font_diagnostic("Nonexistent Family XYZ")
            .expect("an absent family that falls back must warn");
        assert!(
            msg.contains("Nonexistent Family XYZ"),
            "the warning must name the family actually wanted, got: {msg}"
        );
        assert!(
            msg.contains("Install Nonexistent Family XYZ"),
            "the ADVICE must name the wanted family, not the ship default: {msg}"
        );
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

    // Super+Ctrl-click reveals the ITEM, so the target has to survive the round
    // trip out of a URI and back into one — the shapes a Links table prints.
    #[test]
    fn reveal_target_accepts_paths_and_local_file_uris() {
        assert_eq!(
            reveal_target("/home/parker/notes.md"),
            Some("/home/parker/notes.md".into())
        );
        assert_eq!(
            reveal_target("file:///home/parker/notes.md"),
            Some("/home/parker/notes.md".into())
        );
        // the localhost authority names THIS machine
        assert_eq!(
            reveal_target("file://localhost/home/parker/notes.md"),
            Some("/home/parker/notes.md".into())
        );
        // escapes are decoded — a space in a filename is the common one
        assert_eq!(
            reveal_target("file:///home/parker/my%20file.md"),
            Some("/home/parker/my file.md".into())
        );
        // nothing on this disk to show
        assert_eq!(reveal_target("https://example.com/x"), None);
        assert_eq!(reveal_target("mailto:a@b.c"), None);
        // someone else's disk
        assert_eq!(reveal_target("file://otherhost/home/x"), None);
    }

    #[test]
    fn file_uri_escapes_what_would_truncate_it() {
        assert_eq!(
            path_to_file_uri("/home/parker/a b#c.md"),
            "file:///home/parker/a%20b%23c.md"
        );
        // the unreserved set survives untouched
        assert_eq!(path_to_file_uri("/a/B-9_x.~y"), "file:///a/B-9_x.~y");
        // and a decode of our own encoding is the identity
        let p = "/home/parker/BROWN FAMILY/kf-aero (2026).pdf";
        assert_eq!(
            percent_decode(path_to_file_uri(p).strip_prefix("file://").unwrap()),
            p
        );
    }

    #[test]
    fn shell_quote_survives_a_quote_in_the_name() {
        assert_eq!(shell_quote("/a/b"), "'/a/b'");
        assert_eq!(shell_quote("/a/it's"), r"'/a/it'\''s'");
    }

    // The reveal always asks the desktop's file manager first; only the fallback
    // is ours to place, and on a uwsm session it goes through `uwsm-app` so the
    // opened window is scoped to the desktop rather than to this terminal.
    #[test]
    fn reveal_script_asks_dbus_first_then_falls_back_to_the_folder() {
        let plain = reveal_script("/home/parker/kf-aero/letter.pdf", false);
        assert!(plain.contains("org.freedesktop.FileManager1.ShowItems"));
        assert!(plain.contains("array:string:'file:///home/parker/kf-aero/letter.pdf'"));
        // the fallback opens the CONTAINING folder, not the file
        assert!(plain.ends_with("|| xdg-open '/home/parker/kf-aero' >/dev/null 2>&1"));
        assert!(!plain.contains("uwsm-app"));

        let uwsm = reveal_script("/home/parker/kf-aero/letter.pdf", true);
        assert!(uwsm.ends_with("|| uwsm-app -- xdg-open '/home/parker/kf-aero' >/dev/null 2>&1"));

        // a file at the root still has a folder to open
        assert!(reveal_script("/passwd", false).contains("xdg-open '/'"));
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
    fn stitch_rejoins_an_app_hard_wrapped_link_without_wrapline() {
        // Claude Code / our Links tables hard-wrap a long file:// path to the pane
        // width: real rows, NO WRAPLINE flag, but the token runs edge-to-edge.
        let cols = 12;
        let pad = |s: &str| {
            let mut v: Vec<char> = s.chars().collect();
            v.resize(cols, ' ');
            v
        };
        let rows = vec![
            pad("file:///home"), // filled to the edge → flows into next
            pad("/pbrown/a.js"), // filled to the edge → flows into next
            pad("onl next"),     // ends with a space before "next"
        ];
        let wraps = vec![false, false, false]; // <-- the app hard-wrapped; no flag

        // click the FIRST row → stitched to the whole path, click column preserved
        let (line, col) = stitch_wrapped_line(&rows, &wraps, 0, 3);
        assert_eq!(line.trim_end(), "file:///home/pbrown/a.jsonl next");
        assert_eq!(col, 3);
        assert_eq!(
            link_at(&line, col),
            Some(Link::Url("file:///home/pbrown/a.jsonl".into()))
        );

        // click a CONTINUATION row (row 1) → walks up to the same full link
        let (line, col) = stitch_wrapped_line(&rows, &wraps, 1, 2);
        assert_eq!(col, cols + 2);
        assert_eq!(
            link_at(&line, col),
            Some(Link::Url("file:///home/pbrown/a.jsonl".into()))
        );

        // a word-boundary wrap (trailing space) does NOT over-stitch: two short
        // distinct rows stay separate.
        let prose = vec![pad("hello "), pad("world ")];
        let (line, _) = stitch_wrapped_line(&prose, &[false, false], 0, 1);
        assert_eq!(line.trim_end(), "hello");
    }

    #[test]
    fn reflow_rejoins_word_wrapped_prose_at_pane_width() {
        // cols=40. An agent hard-wrapped one sentence across two rows at a word
        // boundary (the trailing space before the break is trimmed away).
        let cols = 40;
        let text = "So the reliable path is to run the\ngather in your terminal.";
        // row 0 len == 34; first word of row 1 ("gather", 6): 34+1+6=41 > 40 ⇒ wrap
        assert_eq!(
            reflow_wrapped_copy(text, cols),
            "So the reliable path is to run the gather in your terminal."
        );
    }

    #[test]
    fn reflow_glues_mid_token_softwrap_with_no_space() {
        // A row filled to exactly cols is a mid-token wrap → glue without a space.
        let cols = 10;
        let text = "abcdefghij\nklmno world"; // row 0 len == 10 == cols
        assert_eq!(reflow_wrapped_copy(text, cols), "abcdefghijklmno world");
    }

    #[test]
    fn reflow_preserves_blank_lines_and_short_distinct_lines() {
        let cols = 80;
        // Two short lines whose next word clearly fits ⇒ NOT joined; blank kept.
        let text = "line one\nline two\n\nfile1.txt\nfile2.txt";
        assert_eq!(reflow_wrapped_copy(text, cols), text);
    }

    /// The reader's vertical fill rests entirely on this arithmetic: ask for N
    /// lines, get the newest N, never reach past what history holds. An off-by-one
    /// here is one missing or one duplicated line at the top of the document —
    /// invisible unless you go looking, which is why it is pinned.
    #[test]
    fn budget_range_takes_the_newest_lines_and_clamps_to_history() {
        // a 47-row screen with 200 rows of history retained
        let (oldest, newest) = (-200, 46);

        // a budget inside history: exactly `want` rows, ending at the newest
        let (a, b) = budget_range(oldest, newest, 100);
        assert_eq!((a, b), (-53, 46));
        assert_eq!((b - a + 1) as usize, 100, "exactly the budget, inclusive");

        // a budget larger than history clamps to the oldest retained row rather
        // than indexing past it — reading a line the grid lacks would panic
        assert_eq!(budget_range(oldest, newest, 10_000), (-200, 46));

        // one screenful asks for exactly the screen, no history
        assert_eq!(budget_range(oldest, newest, 47), (0, 46));

        // a fresh pane with no history never reaches above line 0
        assert_eq!(budget_range(0, 46, 500), (0, 46));

        // a zero budget still yields one line, never an inverted range
        let (a, b) = budget_range(oldest, newest, 0);
        assert_eq!((a, b), (46, 46));
        assert!(a <= b, "the range is never inverted");

        // RowBudget::all() passes usize::MAX. Cast naively to i32 that wraps to
        // -1 and the range inverts — the reader would go BLANK exactly when
        // asked for the whole scrollback. Must clamp to history instead.
        assert_eq!(budget_range(oldest, newest, usize::MAX), (-200, 46));
        assert_eq!(budget_range(0, 0, usize::MAX), (0, 0), "empty fresh pane");
    }

    /// The headline case: the command that failed to paste four times on
    /// 2026-08-31 because the terminal broke it mid-argument. It must come back
    /// as ONE logical line, reporting the two rows it was assembled from.
    #[test]
    fn spans_rejoin_the_command_that_kept_failing_to_paste() {
        let cols = 60;
        let rows = [
            "cd ~/.claude && jq --slurpfile e /tmp/x/automode-env.json",
            "'.autoMode.environment = $e[0]' settings.json > s.tmp",
        ];
        let out = reflow_wrapped_copy_spans(&rows, cols);
        assert_eq!(out.len(), 1, "the two rows are one logical line");
        assert_eq!(out[0].first, 0);
        assert_eq!(out[0].last, 1, "the span covers both rows it came from");
        assert!(
            out[0]
                .text
                .contains("automode-env.json '.autoMode.environment"),
            "the wrap seam is healed, got {:?}",
            out[0].text
        );
        assert!(
            !out[0].text.contains('\n'),
            "a copied command must carry no interior line break"
        );
    }

    /// A mid-token wrap (the row filled the pane exactly) glues with no space,
    /// and still reports both rows.
    #[test]
    fn spans_report_rows_for_a_mid_token_glue() {
        let cols = 20;
        // row 0 is filled to EXACTLY cols — that is what marks a mid-token wrap,
        // and it is why the join adds no space.
        let rows = ["curl https://example", ".com/a/very/long/pa"];
        assert_eq!(rows[0].chars().count(), cols, "row 0 must fill the pane");
        let out = reflow_wrapped_copy_spans(&rows, cols);
        assert_eq!(out.len(), 1);
        assert_eq!((out[0].first, out[0].last), (0, 1));
        assert_eq!(out[0].text, "curl https://example.com/a/very/long/pa");
    }

    /// Every distinct short line keeps its own one-row span, and a blank row
    /// stays a paragraph break rather than being swallowed.
    #[test]
    fn spans_keep_distinct_lines_and_blank_rows_apart() {
        let out = reflow_wrapped_copy_spans(&["git status", "", "git log"], 80);
        assert_eq!(out.len(), 3);
        assert_eq!((out[0].first, out[0].last), (0, 0));
        assert_eq!(out[1].text, "", "the blank row survives as a break");
        assert_eq!((out[2].first, out[2].last), (2, 2));
    }

    /// The wrapper must stay byte-identical to the span core, or the drag-select
    /// copy path silently changes behaviour underneath a shipped feature.
    #[test]
    fn the_text_wrapper_agrees_with_the_span_core() {
        for (text, cols) in [
            ("one\ntwo\nthree", 80),
            (
                "a full width heading line here\nbody paragraph text follows on",
                30,
            ),
            ("", 80),
            ("anything at all", 0), // the cols==0 short circuit
        ] {
            let rows: Vec<&str> = text.split('\n').collect();
            let joined = reflow_wrapped_copy_spans(&rows, cols)
                .into_iter()
                .map(|l| l.text)
                .collect::<Vec<_>>()
                .join("\n");
            assert_eq!(reflow_wrapped_copy(text, cols), joined, "text {text:?}");
        }
    }

    /// THE PADDING TRAP. A terminal grid is space-padded to full width, so every
    /// row reads as "filled to cols" and the width test glues the entire screen
    /// into one line. This test pins both halves: padded input collapses (which
    /// is why the trim exists) and trimmed input does not.
    #[test]
    fn padded_grid_rows_must_be_trimmed_before_reflow() {
        let cols = 20;
        let padded = ["git status         ", "git log            "];
        let padded: Vec<String> = padded
            .iter()
            .map(|r| format!("{r:width$}", width = cols))
            .collect();
        let padded_refs: Vec<&str> = padded.iter().map(String::as_str).collect();
        let glued = reflow_wrapped_copy_spans(&padded_refs, cols);
        assert_eq!(
            glued.len(),
            1,
            "padded rows collapse into one line — this is the trap"
        );

        let trimmed: Vec<&str> = padded_refs.iter().map(|r| r.trim_end()).collect();
        let ok = reflow_wrapped_copy_spans(&trimmed, cols);
        assert_eq!(ok.len(), 2, "trimmed rows stay two distinct commands");
        assert_eq!(ok[0].text, "git status");
        assert_eq!(ok[1].text, "git log");
    }

    /// The copyability gate is deliberately strict: commands get a chip, prose
    /// does not, and an elided illustration never does however command-shaped it
    /// looks. The last case is the one that burned a real operator.
    #[test]
    fn the_copy_gate_offers_commands_and_refuses_prose_and_elisions() {
        for cmd in [
            "cd ~/.claude && jq --slurpfile e /tmp/x.json",
            "git status",
            "! bash /tmp/apply-automode.sh",
            "$ gh pr merge 207 --merge --admin",
            "./scripts/td-send --dry-run",
            "/usr/bin/env bash -c 'echo hi'",
            "~/bin/gh auth status",
            "TD_DEMO=1 cargo run --release",
        ] {
            assert!(is_copyable_command(cmd), "should offer a chip: {cmd:?}");
        }
        for prose in [
            // the exact elision that got pasted and failed, twice
            "cd ~/.claude && jq --slurpfile e /tmp/…/automode-env.json",
            "! bash /tmp/…/apply-automode.sh",
            "This adds a bordered box with a copy button around the line.",
            "Target: https://github.com/parker-brown-family/terminal-delight",
            "abc",
            "",
            "!important",
        ] {
            assert!(!is_copyable_command(prose), "must stay silent: {prose:?}");
        }
    }

    /// The COME-INTERACT detector matches the agent CLI's own prompt furniture
    /// — picker footers, permission questions, the trust dialog — and nothing
    /// else. "esc to interrupt" is the WORKING footer and must stay silent, and
    /// ordinary prose (even about wanting things) must never summon Parker.
    #[test]
    fn interaction_prompts_are_detected_and_working_footers_are_not() {
        let rows = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        for prompt in [
            "  Enter to select · ↑/↓ to navigate · Esc to cancel",
            "  Do you want to proceed?",
            "  Do you trust the files in this folder?",
            "  Would you like to proceed with this plan?",
            // the same headers as the CLI actually draws them: inside a box
            "│ Do you want to make this edit to pane.rs?                    │",
        ] {
            assert!(wants_human(&rows(&[prompt])), "should summon: {prompt:?}");
        }
        for quiet in [
            "✶ Crunching… (esc to interrupt)",
            "I want to refactor the reader next.",
            "error[E0425]: cannot find function `ensure_seeded`",
            "",
            // THE regression: an agent that finished its turn by asking a
            // conversational question left the blinker lit forever, because a
            // finished reply just sits in the live rows. The phrase is only a
            // prompt when it OPENS the row and the row IS the question.
            "  Hi Parker. Nothing is blocked. What do you want to work on?",
            "  and test/run. What do you want to work on?",
            "  Say the word and I'll commit — do you want the .gitignore in?",
            // the human's own echoed input must never read as the CLI asking
            "> do you want to proceed?",
        ] {
            assert!(!wants_human(&rows(&[quiet])), "must stay quiet: {quiet:?}");
        }
    }

    /// The ✅/❌ split: the CLI's own failure banners classify a stop as
    /// blocked; the agent's tool output failing (a red cargo error) is the
    /// agent WORKING and must classify as a clean finish when it stops.
    #[test]
    fn blocked_finishes_are_the_clis_banners_not_tool_errors() {
        let rows = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        for wall in [
            "  ⎿  API Error: 529 overloaded_error",
            "  You've reached your usage limit — resets 3pm",
            "  Credit balance too low",
            "  OAuth token has expired",
            "  Request timed out after 60s",
        ] {
            assert!(
                looks_blocked(&rows(&[wall])),
                "should read blocked: {wall:?}"
            );
        }
        for fine in [
            "error[E0308]: mismatched types",
            "test result: FAILED. 3 passed; 1 failed",
            "Done — merged #227 and deployed.",
            "",
        ] {
            assert!(!looks_blocked(&rows(&[fine])), "must read clean: {fine:?}");
        }
    }

    /// A line-INITIAL URL earns a chip — link tables and agent replies wrap
    /// them, and half a URL is as dead as half a command. Mid-prose URLs stay
    /// silent (the strictness rule), and the elision guarantee still wins even
    /// over a URL shape.
    #[test]
    fn the_copy_gate_offers_line_initial_urls() {
        for url in [
            "https://github.com/parker-brown-family/terminal-delight/pull/222",
            "http://localhost:631/printers/",
            "file:///home/parker/.local/bin/terminal-delight",
            "HTTPS://EXAMPLE.COM/CASED",
        ] {
            assert!(is_copyable_command(url), "should offer a chip: {url:?}");
        }
        for no in [
            "see https://example.com for details",
            "Target: https://github.com/x",
            "https://example.com/…/elided/path",
        ] {
            assert!(!is_copyable_command(no), "must stay silent: {no:?}");
        }
    }

    /// The paging-key map: plain PageUp/PageDown and ctrl+Home/ctrl+End match;
    /// every other modifier combination belongs to someone else (ctrl+PageUp is
    /// tab switching, plain Home/End are the shell's, alt/super are chords) and
    /// must fall through.
    #[test]
    fn paging_keys_map_and_modified_ones_fall_through() {
        let m = |ctrl: bool, alt: bool, shift: bool| gpui::Modifiers {
            control: ctrl,
            alt,
            shift,
            ..Default::default()
        };
        assert_eq!(
            read_nav_key("pageup", &m(false, false, false)),
            Some(ReadNav::PageUp)
        );
        assert_eq!(
            read_nav_key("pagedown", &m(false, false, false)),
            Some(ReadNav::PageDown)
        );
        assert_eq!(
            read_nav_key("home", &m(true, false, false)),
            Some(ReadNav::Top)
        );
        assert_eq!(
            read_nav_key("end", &m(true, false, false)),
            Some(ReadNav::Bottom)
        );
        // ctrl+PageUp/PageDown = tab switching; plain Home/End = the shell's.
        assert_eq!(read_nav_key("pageup", &m(true, false, false)), None);
        assert_eq!(read_nav_key("pagedown", &m(true, false, false)), None);
        assert_eq!(read_nav_key("home", &m(false, false, false)), None);
        assert_eq!(read_nav_key("end", &m(false, false, false)), None);
        // any alt/shift decoration falls through too
        assert_eq!(read_nav_key("pageup", &m(false, true, false)), None);
        assert_eq!(read_nav_key("end", &m(true, false, true)), None);
        assert_eq!(read_nav_key("q", &m(false, false, false)), None);
    }

    #[test]
    fn reflow_leaves_narrow_wrapped_text_untouched() {
        // Prose wrapped at a fixed 72 cols inside a wide 200-col pane: the next
        // word always fits, so no row trips the width test.
        let cols = 200;
        let text = "A paragraph wrapped by an email client at seventy-two\ncolumns stays exactly as many lines.";
        assert_eq!(reflow_wrapped_copy(text, cols), text);
    }

    #[test]
    fn reflow_does_not_merge_indented_or_rule_rows() {
        let cols = 30;
        // row 0 is full-width prose, but row 1 is indented (code) ⇒ break kept.
        let indented = "this line runs right up to edge\n    let x = code_block();";
        assert_eq!(reflow_wrapped_copy(indented, cols), indented);
        // a box-drawing rule between two full rows is never absorbed.
        let ruled = "a full width heading line here\n──────────────────────────────\nbody paragraph text follows on";
        assert_eq!(reflow_wrapped_copy(ruled, cols), ruled);
    }

    #[test]
    fn reflow_rebuilds_a_wrapped_shell_command() {
        // The command block from the report: wrapped at a word boundary rejoins
        // into a single runnable line.
        let cols = 30;
        let text = "gcloud compute instances\ndescribe internal-tools now";
        // row 0 len 24; next word "describe"(8): 24+1+8=33 > 30 ⇒ join w/ space
        assert_eq!(
            reflow_wrapped_copy(text, cols),
            "gcloud compute instances describe internal-tools now"
        );
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
    fn should_invert_truth_table_preserves_codex_alt_screen() {
        // Invert when anchor_top is on and this is not crawl. Alternate-screen
        // shell TUIs are guarded off, but agent TUIs such as Codex still invert
        // so they get the same prompt-first top-anchor flow as Claude.
        // (anchor_top, crawl, alt_screen, agent_mode) -> expected
        let cases = [
            (false, false, false, false, false), // toggle off -> never invert
            (false, false, true, true, false),
            (true, true, false, true, false), // crawl keeps its own bottom-anchor
            (true, true, true, true, false),
            (true, false, false, false, true), // shell, normal screen
            (true, false, false, true, true),  // agent, normal screen
            (true, false, true, false, false), // vim/htop/less alt-screen guard
            (true, false, true, true, true),   // Codex/Claude alt-screen agent
        ];
        for (anchor, crawl, alt, agent, expected) in cases {
            assert_eq!(
                should_invert(anchor, crawl, alt, agent),
                expected,
                "should_invert(anchor_top={anchor}, crawl={crawl}, alt_screen={alt}, agent_mode={agent})"
            );
        }
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
    fn invert_logical_read_block_mode_keeps_a_multistep_turn_in_order() {
        // §3a regression: an agent TURN whose reply blank-separates its steps
        // (Claude's shape) must NOT have its steps reversed. Group by the human-
        // prompt turn boundary, not by every blank line, so the reply reads
        // top→bottom while whole TURNS reverse (newest on top).
        let row = |s: &str| (s.to_string(), Vec::<TextRun>::new());
        let texts =
            |l: &[(String, Vec<TextRun>)]| l.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>();
        let lines = vec![
            row("> old question"),      // 0  turn 1: human
            row("\u{25cf} old answer"), // 1  turn 1: reply
            row(""),                    // 2
            row("> new question"),      // 3  turn 2: human
            row("\u{25cf} step one"),   // 4  turn 2: reply step 1
            row(""),                    // 5  internal blank — must NOT reorder steps
            row("\u{25cf} step two"),   // 6  turn 2: reply step 2
            row(""),                    // 7  internal blank
            row("\u{25cf} step three"), // 8  turn 2: reply step 3
        ];
        let (out, _perm) = invert_logical_read(lines, &[], true);
        let t = texts(&out);
        // Newest turn on top, its steps IN ORDER (one, two, three).
        assert_eq!(t[0], "> new question");
        assert_eq!(t[1], "\u{25cf} step one");
        assert_eq!(t[3], "\u{25cf} step two"); // t[2] is the internal blank
        assert_eq!(t[5], "\u{25cf} step three"); // t[4] is the internal blank
                                                 // Older turn sits BELOW the newest one, still top→bottom.
        let old_q = t.iter().position(|s| s == "> old question").unwrap();
        let old_a = t.iter().position(|s| s == "\u{25cf} old answer").unwrap();
        assert!(old_q < old_a, "older turn reads top→bottom");
        assert!(old_q > 5, "older turn sits below the newest turn");
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
    fn visual_selection_spans_fills_visually_contiguous_rows() {
        // #149: a drag from visual (row 1, col 5) into (row 3, col 2) selects the
        // anchor row from col 5 to the end, the whole middle row, and the last row
        // from the start to col 2 — i.e. crossing a section first highlights only
        // its TOP visual line, then fills down. NOT whole reversed blocks.
        let spans = visual_selection_spans((1, 5), (3, 2), 9, false);
        assert_eq!(spans, vec![(1, 5, 9), (2, 0, 9), (3, 0, 2)]);
        // Endpoint order is irrelevant — dragging up gives the identical span.
        assert_eq!(visual_selection_spans((3, 2), (1, 5), 9, false), spans);
    }

    #[test]
    fn visual_selection_spans_single_row_is_a_column_range() {
        assert_eq!(
            visual_selection_spans((2, 7), (2, 3), 20, false),
            vec![(2, 3, 7)]
        );
        // columns clamp to last_col so a span can't run off the row.
        assert_eq!(
            visual_selection_spans((2, 3), (2, 99), 9, false),
            vec![(2, 3, 9)]
        );
    }

    #[test]
    fn visual_selection_spans_block_is_rectangular() {
        // Block (alt-drag): every row clipped to the same column band, ordered.
        assert_eq!(
            visual_selection_spans((3, 8), (1, 2), 20, true),
            vec![(1, 2, 8), (2, 2, 8), (3, 2, 8)]
        );
    }

    #[test]
    fn invert_run_range_swaps_only_selected_cells() {
        let fg = gpui::hsla(0.1, 0.5, 0.5, 1.0);
        let bg = gpui::hsla(0.6, 0.5, 0.2, 1.0);
        let mk = |s: &str| TextRun {
            len: s.len(),
            font: gpui::font("monospace"),
            color: fg,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        // Row "ABCDE", one run, no background. Invert cols 1..=3 → those cells take
        // fg=default_bg, bg=old fg; cols 0 and 4 are untouched. Text is preserved.
        let mut row = ("ABCDE".to_string(), vec![mk("ABCDE")]);
        invert_run_range(&mut row, 1, 3, bg);
        assert_eq!(row.0, "ABCDE", "text content never changes");
        // Re-coalesced into [A][BCD][E]: untouched / inverted / untouched.
        assert_eq!(row.1.len(), 3);
        assert_eq!(row.1[0].len, 1); // "A" untouched
        assert_eq!(row.1[0].color, fg);
        assert_eq!(row.1[0].background_color, None);
        assert_eq!(row.1[1].len, 3); // "BCD" inverted
        assert_eq!(row.1[1].color, bg); // fg ← default_bg (cell had no bg)
        assert_eq!(row.1[1].background_color, Some(fg)); // bg ← old fg
        assert_eq!(row.1[2].len, 1); // "E" untouched
        assert_eq!(row.1[2].color, fg);
        // A column range past the row end is a no-op (no panic, no change).
        let mut short = ("AB".to_string(), vec![mk("AB")]);
        invert_run_range(&mut short, 5, 9, bg);
        assert_eq!(short.1[0].color, fg);
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

    /// The prompt seek drives a full-screen agent by synthesising wheel notches,
    /// so the bytes must be exactly what a real wheel would have produced — and
    /// they must never be cursor keys, which agents read as history recall.
    #[test]
    fn a_synthetic_wheel_notch_is_a_mouse_report_not_a_cursor_key() {
        // SGR (1006): button 64 = wheel up, 65 = wheel down, at cell 1,1.
        assert_eq!(wheel_step_bytes(true, true), b"\x1b[<64;1;1M".to_vec());
        assert_eq!(wheel_step_bytes(false, true), b"\x1b[<65;1;1M".to_vec());
        // legacy X10: ESC [ M <32+button> <col+32> <row+32>
        assert_eq!(
            wheel_step_bytes(true, false),
            vec![0x1b, b'[', b'M', 96, 33, 33]
        );
        assert_eq!(
            wheel_step_bytes(false, false),
            vec![0x1b, b'[', b'M', 97, 33, 33]
        );
        for bytes in [
            wheel_step_bytes(true, true),
            wheel_step_bytes(false, true),
            wheel_step_bytes(true, false),
            wheel_step_bytes(false, false),
        ] {
            assert_ne!(bytes, b"\x1b[A".to_vec());
            assert_ne!(bytes, b"\x1b[B".to_vec());
        }
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
    fn pane_on_key_only_stops_propagation_when_it_consumes_the_key() {
        // The bubbling invariant above has no compile-time or runtime signal —
        // break it and every workspace chord silently dies while all other
        // tests stay green. The source is the only place it is observable.
        //
        // Stopping propagation is legitimate where the handler owns the key and
        // returns on the spot; it is a bug on the fall-through path, where a
        // chord this handler declined would never reach the Workspace. So the
        // assertion is not "no stop_propagation" — that would reject the
        // correct F1 fix — but "every stop_propagation returns".
        let src = include_str!("pane.rs");
        let at = src
            .find("fn on_key(&mut self, ev: &KeyDownEvent")
            .expect("TerminalView::on_key");
        let body = &src[at..];
        let end = body.find("\n    }\n").expect("end of on_key");
        let body = &body[..end];
        for (i, _) in body.match_indices("stop_propagation") {
            let tail = &body[i..(i + 120).min(body.len())];
            assert!(
                tail.contains("return"),
                "stop_propagation in TerminalView::on_key must belong to a \
                 branch that returns, or the chords the handler declines never \
                 reach the Workspace — see the INVARIANT comment above on_key"
            );
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
        // shift+arrows extend selection (CSI 1;2 form)
        assert_eq!(bytes("shift-right"), Some(b"\x1b[1;2C".to_vec()));
        // workspace-owned chords must NOT reach the shell
        assert_eq!(bytes("alt-left"), None);
        assert_eq!(bytes("ctrl-pageup"), None);
        // alt+<char> needs a key_char to encode, and Keystroke::parse doesn't
        // synthesise one — gpui fills it at runtime. Supply it, or an assertion
        // here passes for the wrong reason.
        let alt_char = |c: &str| {
            let mut k = Keystroke::parse(&format!("alt-{c}")).unwrap();
            k.key_char = Some(c.to_string());
            keystroke_bytes(&k)
        };
        // alt+r opens the FOCUS reader now that the 👓 glyph is retired, so the
        // Workspace owns it — it must NOT reach the shell as readline's
        // revert-line, even with a key_char present.
        assert_eq!(alt_char("r"), None);
        // alt+v / alt+h are the Workspace's split chords — never PTY bytes.
        assert_eq!(alt_char("v"), None);
        assert_eq!(alt_char("h"), None);
        // ...while every OTHER alt+<char> still goes through ESC-prefixed, so
        // alt+b / alt+f keep their readline meaning.
        assert_eq!(alt_char("b"), Some(vec![0x1b, b'b']));
        assert_eq!(alt_char("f"), Some(vec![0x1b, b'f']));
        // ctrl+arrows are the SHELL's word-jump, straight through (CSI 1;5).
        // The workspace does not contend for them — pane focus is alt+arrows —
        // so these must be real bytes here, not a None handed back later.
        assert_eq!(bytes("ctrl-right"), Some(b"\x1b[1;5C".to_vec()));
        assert_eq!(bytes("ctrl-left"), Some(b"\x1b[1;5D".to_vec()));
        assert_eq!(bytes("ctrl-up"), Some(b"\x1b[1;5A".to_vec()));
        assert_eq!(bytes("ctrl-down"), Some(b"\x1b[1;5B".to_vec()));
        // alt+arrows, by contrast, ARE the workspace's — never PTY bytes
        assert_eq!(bytes("alt-left"), None);
        assert_eq!(bytes("alt-right"), None);
        // ctrl+shift+arrows are nobody's chord — straight through (CSI 1;6)
        assert_eq!(bytes("ctrl-shift-right"), Some(b"\x1b[1;6C".to_vec()));
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
