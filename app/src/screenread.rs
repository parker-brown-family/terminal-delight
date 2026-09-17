//! Everything that reads the terminal grid AS TEXT, in one place.
//!
//! The bench derives most of what it knows about an agent from the agent's own
//! screen — is it asking, is it working, what is it asking, which row is the
//! picker on — and every one of those readings is a heuristic over characters
//! that the CLI never promised to keep stable. Before this module they lived in
//! three files with no shared discipline, and in one working day every single
//! one of them was found either wrong or rotted: a phrase the CLI stopped
//! printing, a blink read as a state change, a second column read as part of
//! the first, a checkbox read as part of a name.
//!
//! So this module has one law, and a test at the bottom enforces it on every
//! public function here:
//!
//! **Every reader carries a test transcribed from a real screen.** Not a
//! plausible string — the actual rows off a photograph or a captured grid,
//! glyphs and spacing intact. A reader that has never been shown the thing it
//! reads has only been shown what its author imagined, and that is how the
//! `esc to interrupt` detector ran for months after the string it wanted had
//! left the product.
//!
//! What a screen read may NOT conclude is written in the protocol doc under
//! *What a surface is not told*, and the rules there are the reasons the
//! functions here are shaped the way they are.

use crate::surface::{Answered, Choice_, Question, Round, Step, SurfaceId};

// ---------------------------------------------------------------------------
// from pane.rs
// ---------------------------------------------------------------------------

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
    // The review step at the end of a multi-select or a round, which draws no
    // footer at all — so the picker was live, the person was being waited on,
    // and the bench said Idle. Pressing the bench's Submit reached exactly
    // this screen and then appeared to do nothing, because the surface that
    // would have shown the second half of the commit was never raised.
    //
    // Matched as the CLI's own literal rather than by loosening the rule to
    // "any line ending in a question mark": the agent's prose ends in question
    // marks constantly, and a detector that fires on those would report a
    // working agent as blocked. Verified against the strings in the binary —
    // `Ready to submit your answers?` beside `Review your answers` and the
    // `Submit answers` / `Cancel` pair.
    if t == "ready to submit your answers?" || t == "review your answers" {
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
    wants_human_row(recent_rows).is_some()
}

/// The row that made [`wants_human`] say yes — the evidence behind a decision
/// row in the rail.
///
/// **The predicate is defined in terms of this, and not the other way round.**
/// A second scan that re-derived "which line was it" would be free to disagree
/// with the one that set the flag, and the disagreement would appear only on the
/// screens where two rows both match — precisely the ambiguous screens where a
/// person most needs the quote to be the real one. One walk, one answer, and
/// `wants_human` is now a question about whether that answer exists.
///
/// The LAST match wins, because an agent TUI prints downward: with a stale
/// question still on screen above a live picker, the live one is lower.
pub fn wants_human_row(recent_rows: &[String]) -> Option<&str> {
    recent_rows
        .iter()
        .rev()
        .find(|r| row_wants_human(r))
        .map(|r| r.trim())
}

/// The row that made [`looks_blocked`] say yes — the evidence behind a failure
/// row. Same one-walk contract as [`wants_human_row`].
pub fn blocked_row(recent_rows: &[String]) -> Option<&str> {
    recent_rows
        .iter()
        .rev()
        .find(|r| row_blocked(r))
        .map(|r| r.trim())
}

/// The longest quote a rail row will carry.
///
/// A queue row is 300 pixels wide and the line it quotes is a terminal row that
/// may be two hundred columns of box-drawing. Clipped at capture rather than at
/// paint: the stored string is what a later reader gets, and storing a kilobyte
/// per pane to draw sixty characters of it is a cost paid every scan.
const EVIDENCE_CHARS: usize = 96;

/// Tidy one screen row into something quotable on a 300-pixel row, or `None`
/// when nothing quotable is left.
///
/// Box-drawing furniture goes (the CLI draws its prompts inside a frame, and
/// `│ Do you want to proceed? │` quotes the frame as much as the question), runs
/// of spaces collapse, and the result is clipped with an ellipsis so a clipped
/// quote can never be mistaken for a short one.
///
/// **`None` rather than an empty string**, because a row of pure furniture —
/// `╭──────────╮`, or a framed blank line — tidies down to nothing at all, and
/// the renderer would draw that as an empty grey quote box: a row appearing to
/// quote its screen and quoting nothing. Today's predicates cannot hand one in
/// (both demand real text before they match), so this is the boundary being
/// closed one predicate change ahead of needing it, not a bug being fixed.
pub fn clip_evidence(row: &str) -> Option<String> {
    let cleaned: String = row
        .chars()
        .map(|c| {
            if ('\u{2500}'..='\u{257f}').contains(&c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    let mut out = String::with_capacity(cleaned.len());
    let mut space = false;
    for c in cleaned.trim().chars() {
        if c.is_whitespace() {
            space = true;
            continue;
        }
        if space && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        out.push(c);
    }
    if out.chars().count() > EVIDENCE_CHARS {
        out = out.chars().take(EVIDENCE_CHARS - 1).collect::<String>() + "\u{2026}";
    }
    (!out.is_empty()).then_some(out)
}

/// How many rows of the visible tail the prompt predicates read.
///
/// One constant because the scan and the clearing edge must agree: a
/// fingerprint taken over a different window than the predicate ran on would
/// suppress the wrong screen, and the bug would only show on a pane whose
/// fifteenth-from-last row happened to change.
pub const PROMPT_TAIL_ROWS: usize = 14;

/// A cheap identity for the rows a predicate was just evaluated over.
///
/// Not a checksum and not a diff — only enough to answer "is this the same
/// screen I was already told about". Fourteen trimmed rows, so it costs nothing
/// on the 120ms scan that already has them in hand.
pub fn rows_fingerprint(recent_rows: &[String]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for r in recent_rows {
        r.trim_end().hash(&mut h);
    }
    h.finish()
}

/// [`wants_human`], minus a screen the person has already answered to.
///
/// The predicate reads the visible tail, and on a quiet pane an answered prompt
/// does not scroll away — which is how the needs-you blinker once pinned on
/// permanently. Narrowing [`row_wants_human`] fixed the loud half of that;
/// this is the clearing edge for the rest. A keystroke in the pane records the
/// screen it was typed at, and the flag cannot re-arm while that is still what
/// is showing.
///
/// **Keyed on the SCREEN, deliberately, and not on a clock.** A stale prompt and
/// a fresh one are indistinguishable to a timer, so a timeout would either keep
/// hiding a real question or start showing an answered one again. Somebody who
/// types and is then genuinely asked something new has a different screen, and
/// gets told.
pub fn wants_human_unless_answered(recent_rows: &[String], answered_on: Option<u64>) -> bool {
    wants_human(recent_rows) && answered_on != Some(rows_fingerprint(recent_rows))
}

/// Did the agent stop because it hit a WALL rather than the end of its turn?
/// Matches the error banners the agent CLI itself prints — API failures,
/// exhausted limits, expired auth — NOT the word "error", which appears
/// constantly in the agent's own tool output (a failing build is the agent
/// WORKING, not the agent blocked). Classifies the finish badge: ✅ clean, ❌
/// blocked. Strict for the same reason as [`row_wants_human`].
pub fn looks_blocked(recent_rows: &[String]) -> bool {
    blocked_row(recent_rows).is_some()
}

/// Does this one row read as a wall? The row-level half of [`looks_blocked`],
/// split out so the predicate and the evidence come from one walk — see
/// [`wants_human_row`] for why that matters.
pub fn row_blocked(text: &str) -> bool {
    let t = text.trim().to_ascii_lowercase();
    !t.is_empty()
        && (t.contains("api error")
            || t.contains("usage limit")
            || t.contains("credit balance")
            || t.contains("oauth token")
            || t.contains("rate_limit_error")
            || t.contains("overloaded_error")
            || t.contains("request timed out"))
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

// ---------------------------------------------------------------------------
// from hud.rs
// ---------------------------------------------------------------------------

/// Parse the visible bottom rows of an agent pane into an [`AgentStatus`].
///
/// `rows` is the live screen, top-to-bottom. We do not assume a fixed format:
/// we look for the richest status row and pull whatever is present, degrading
/// gracefully (a bare "esc to interrupt" still reads as Working, just without
/// metrics). `Finished` is *not* decided here — the caller layers it on from
/// the pane's unacknowledged bell.
/// Does this row read as the agent's own working spinner?
///
/// **The shape, not the words.** The detector this replaces looked for `esc to
/// interrupt`, and that string has almost left the CLI: in 2.1.270 it survives
/// only in the low-priority retry path, so an agent that had been composing for
/// forty-nine seconds was reported as Idle. Parker, with the spinner on screen
/// beside a bench reading Idle: *"the agent state is also not accurate"*.
///
/// The gerund is hopeless to match — Composing, Churned, Cooked, Cerebrating,
/// Sautéed — and that is the point: the CLI varies the word on purpose and
/// keeps the STRUCTURE. A working line is a parenthesised group carrying both
/// an elapsed time and a token count:
///
/// ```text
/// \u{2733} Composing\u{2026} (49s \u{b7} \u{2193} 7.5k tokens)
/// ```
///
/// A finished line has neither the parentheses nor the tokens — `\u{2733}
/// Churned for 13s \u{b7} done 11:22 AM` — so the two do not collide, and prose
/// that merely mentions tokens has no seconds inside a bracket.
///
/// The old strings are kept rather than replaced. They still appear on older
/// builds and on the retry path, and a detector that stops recognising the
/// previous format is how this broke in the first place.
pub fn row_is_working(text: &str) -> bool {
    let low = text.to_ascii_lowercase();
    if low.contains("esc to interrupt")
        || low.contains("interrupt)")
        || low.contains("still thinking")
    {
        return true;
    }
    // A bracketed group holding both an elapsed time and a token count.
    let mut rest = low.as_str();
    while let Some(open) = rest.find('(') {
        let after = &rest[open + 1..];
        let close = after.find(')').unwrap_or(after.len());
        let group = &after[..close];
        if group.contains("tokens") && has_elapsed(group) {
            return true;
        }
        rest = &after[close.min(after.len())..];
        if rest.is_empty() {
            break;
        }
        rest = &rest[1.min(rest.len())..];
    }
    // The spinner line — `✻ Forming… (6m 34s · ↓ 50.0k tokens)` — is the needle
    // that survives a NARROW pane, where the footer carrying every string above
    // has been truncated away (#477). Case does not matter to it, so the row
    // this function was handed is the row it gets.
    crate::hud::is_live_spinner(text)
}

/// A run of digits followed by a time unit — `49s`, `2m`, `1h`.
///
/// Deliberately not a regex and deliberately strict about what follows the
/// unit: `7.5k tokens` must not read as an elapsed time just because it has a
/// digit and a letter in it.
fn has_elapsed(group: &str) -> bool {
    let b = group.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            continue;
        }
        let unit = b.get(i).copied();
        let after = b.get(i + 1).copied();
        let ends = after.is_none_or(|c| !c.is_ascii_alphanumeric() && c != b'.');
        if matches!(unit, Some(b's') | Some(b'm') | Some(b'h')) && ends {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// from derive.rs
// ---------------------------------------------------------------------------

/// The question an agent is asking **right now**, read off its screen.
///
/// This is the one place this module looks at pixels rather than at the
/// record, and it is here because the record does not have it. Measured on
/// 2026-09-17: a Claude Code transcript's last line was written at 07:15:53
/// while a picker had been on screen since ~07:16 — the assistant message
/// carrying an `AskUserQuestion` is buffered until its result arrives, so a
/// **pending** question is never in the file. Deriving only from the
/// transcript therefore produces questions that are always already answered:
/// good history, useless for answering.
///
/// So the transcript stays authoritative for what happened, and the screen is
/// the only source for what is happening. The two do not fight because the
/// live row is **retired** the moment the pane stops waiting: the recorded
/// one then arrives carrying the answer, and the bench holds one row rather
/// than a live copy and a historical copy of the same question.
///
/// Pure over rows, so the whole of it is testable against text captured from
/// a real pane.
/// Read the picker's own tab bar, if it is running a round of questions.
///
/// The strip looks like this, with the arrows only present when there is
/// somewhere to scroll to:
///
/// ```text
/// ←  ⊠ Tomorrow   ⊡ Mug   ✔ Submit  →
/// ```
///
/// `⊠` is a question already answered and `⊡` one still open; the tick is the
/// Submit step, which ends the round rather than being a question in it. The
/// glyphs are the measurement — the highlight that marks the CURRENT step is a
/// colour, and a colour does not survive being read off a character grid, so
/// this reports what was answered rather than guessing where the cursor is.
///
/// [`None`] when there is no strip, which is the ordinary single-question
/// case and not a failure.
pub fn round_on_screen(rows: &[String]) -> Option<Round> {
    let bar = rows.iter().find(|r| {
        let t = r.trim();
        t.contains('\u{2714}') && (t.contains('\u{22a0}') || t.contains('\u{22a1}'))
    })?;
    let mut steps = Vec::new();
    let mut submitting = false;
    // Split on the marks themselves: the labels between them are whatever the
    // agent called each question, and they may contain spaces.
    let rest0 = bar.trim();
    // Drop the scroll arrows; they say only that the strip is wider than the
    // pane.
    let rest = rest0
        .trim_start_matches('\u{2190}')
        .trim_end_matches('\u{2192}')
        .trim();
    let mut cur: Option<bool> = None;
    let mut label = String::new();
    let push = |cur: &mut Option<bool>, label: &mut String, steps: &mut Vec<Step>| {
        if let Some(done) = cur.take() {
            let text = label.trim().to_string();
            if !text.is_empty() {
                steps.push(Step { label: text, done });
            }
        }
        label.clear();
    };
    for ch in rest.chars() {
        match ch {
            '\u{22a0}' => {
                push(&mut cur, &mut label, &mut steps);
                cur = Some(true);
            }
            '\u{22a1}' => {
                push(&mut cur, &mut label, &mut steps);
                cur = Some(false);
            }
            '\u{2714}' => {
                push(&mut cur, &mut label, &mut steps);
                submitting = true;
                cur = None;
            }
            _ => {
                if cur.is_some() {
                    label.push(ch);
                }
            }
        }
    }
    push(&mut cur, &mut label, &mut steps);
    if steps.is_empty() {
        return None;
    }
    // A round of one is a question with a Submit button, not a workflow, and
    // the bench should draw it as the former.
    (steps.len() > 1).then_some(Round { steps, submitting })
}

pub fn question_on_screen(rows: &[String]) -> Option<Question> {
    let numbered = collect_options(rows)?;
    let (first_line, options, cursor, submit) = numbered;
    // The question is the nearest non-empty line above the first option that
    // is not the picker's own header chrome.
    let question = rows[..first_line]
        .iter()
        .rev()
        .map(|r| without_side_panel(r.trim()))
        .find(|r| r.len() > 8 && !r.starts_with(is_box_drawing) && !r.ends_with("to cancel"))?
        .to_string();
    Some(Question {
        question,
        options,
        recommend: None,
        answer: Answered::Waiting,
        cursor: Some(cursor),
        submit,
        round: round_on_screen(rows),
    })
}

/// The consecutive `N. label` block, its first row, and which one the cursor
/// is on.
fn collect_options(rows: &[String]) -> Option<(usize, Vec<Choice_>, usize, Option<usize>)> {
    let mut first_line = None;
    let mut options: Vec<Choice_> = Vec::new();
    let mut cursor = 0usize;
    let mut submit_at: Option<usize> = None;
    for (i, row) in rows.iter().enumerate() {
        let Some((n, label, marked)) = numbered_option(row) else {
            // A description line belongs to the option above it: indented,
            // non-empty, and we are already inside the block.
            if let Some(last) = options.last_mut() {
                let t = row.trim();
                // `Submit` indented under the last option is the
                // picker's own button, not something that option does, and
                // reading it as a description put the word where a sentence
                // about the choice belongs.
                // The picker's own button, which it calls `Submit` on the
                // last question of a round and `Next` on the others. It is
                // not an option and it is not a description of one — it is a
                // POSITION in the up/down order, and recording where it sits
                // is the only way an answer sent from here lands on the row
                // the person pointed at.
                if matches!(t, "Submit" | "Submit answers" | "Next") {
                    submit_at.get_or_insert(options.len());
                    continue;
                }
                // A row that is nothing BUT the side panel is not a
                // description of the option above it. Cut it first, and if
                // what is left is empty then this row belonged to the panel
                // and never to the option.
                let t = without_side_panel(t);
                if !t.is_empty()
                    && row.starts_with("    ")
                    && last.what_happens.is_none()
                    && numbered_option(row).is_none()
                {
                    last.what_happens = Some(t.chars().take(120).collect());
                    continue;
                }
                // A blank line does not end the block — the picker puts one
                // before "Type something." — but two in a row do.
                if t.is_empty() {
                    continue;
                }
                if t.starts_with("Enter to select") || t.starts_with("Esc to") {
                    break;
                }
            }
            continue;
        };
        // Options are numbered from one and in order; anything else is prose
        // that happens to start with a digit and a dot.
        if n != options.len() + 1 {
            continue;
        }
        if first_line.is_none() {
            first_line = Some(i);
        }
        if marked {
            cursor = options.len();
        }
        let (label, checked) = split_checkbox(without_side_panel(&label));
        options.push(Choice_ {
            label,
            what_happens: None,
            checked,
        });
    }
    let first = first_line?;
    (options.len() >= 2).then_some((first, options, cursor, submit_at))
}

/// `  1. Cast it into the fire` → `(1, "Cast it into the fire", false)`.
/// A non-space glyph before the number means the cursor is on that row.
/// Cut a row where a side panel starts.
///
/// A picker may draw a PREVIEW beside its options rather than under them:
///
/// ```text
/// ) 1. Stacked rows        \u{250c}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2510}
///   2. Two columns         \u{2502} LINES HELD  \u{2502}
/// ```
///
/// This reader works a row at a time, so the panel was arriving inside the
/// labels — an option came out as `Stacked rows \u{250c}\u{2500}\u{2500}` and a
/// description as `\u{2502} \u{2502} 4 \u{2502}`. It looked right in the terminal,
/// where the columns line up, and completely wrong on the bench, where each row
/// is a separate element and the second column has nowhere to be.
///
/// Box-drawing is the signal because it is unambiguous: an option's label is
/// prose an agent wrote, and prose does not contain `\u{2502}`. Cutting at the
/// first one loses at worst a decorative character; not cutting imports a
/// whole panel into a button. The gutter marks and checkboxes this reader
/// depends on all sit outside the range — `\u{276f}`, `\u{203a}`, `\u{22a0}`,
/// `\u{22a1}`, `\u{2713}`, `\u{2714}` — so none of them is caught by it.
fn without_side_panel(text: &str) -> &str {
    match text.find(is_box_drawing) {
        Some(at) => text[..at].trim_end(),
        None => text,
    }
}

/// Box Drawing (U+2500..U+257F) and Block Elements (U+2580..U+259F).
fn is_box_drawing(c: char) -> bool {
    matches!(c, '\u{2500}'..='\u{259f}')
}

/// Split a multi-select checkbox off the front of an option label.
///
/// The picker writes `[ ] A plant, thriving` for an unticked box and `[✓] …`
/// for a ticked one, and those five characters were landing in the LABEL — so
/// a chip read `1 · [✓] A plant, thriving`, and the instant the terminal
/// redrew mid-click it read `1 · A plant, thriving` instead. Nothing was
/// loading; the same option was being parsed two different ways one second
/// apart. Parker, watching a click flicker through a state he had not asked
/// for: *"if we are UNABLE to make it respond INSTANTLY then we must not
/// CHANGE anything"*.
///
/// The tick is real information — it is the answer so far — so it comes out of
/// the text and becomes state the chip can draw. [`None`] for a row with no
/// box, which is every ordinary single-choice option.
fn split_checkbox(label: &str) -> (String, Option<bool>) {
    let t = label.trim_start();
    if !t.starts_with('[') {
        return (label.to_string(), None);
    }
    let Some(close) = t.find(']') else {
        return (label.to_string(), None);
    };
    let inside = t[1..close].trim();
    // Measured in CHARACTERS. `\u{2713}` is three bytes, so a byte-length
    // guard rejected the one case this exists for — a ticked box — and let
    // the empty one through, which is the most confusing half to get right.
    if inside.chars().count() > 1 {
        return (label.to_string(), None);
    }
    let ticked = match inside {
        "" => false,
        "✓" | "✔" | "x" | "X" | "*" => true,
        // A bracket that is not a checkbox — `[1]`, `[note]` — is part of what
        // the option is called, and taking it off would rename it.
        _ => return (label.to_string(), None),
    };
    (t[close + 1..].trim().to_string(), Some(ticked))
}

fn numbered_option(row: &str) -> Option<(usize, String, bool)> {
    let trimmed = row.trim_start();
    let lead = &row[..row.len() - trimmed.len()];
    // The picker draws its cursor in the gutter — `❯`, `>` or `)` depending
    // on the build and the font. Anything that is not whitespace counts.
    let marked_gutter = lead.chars().any(|c| !c.is_whitespace());
    let mut rest = trimmed;
    let mut marked = marked_gutter;
    for glyph in ['❯', '›', '>', ')', '*'] {
        if let Some(r) = rest.strip_prefix(glyph) {
            rest = r.trim_start();
            marked = true;
        }
    }
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() || digits.len() > 2 {
        return None;
    }
    let rest = rest[digits.len()..].strip_prefix('.')?;
    let label = rest.trim();
    if label.is_empty() {
        return None;
    }
    Some((
        digits.parse().ok()?,
        label.chars().take(90).collect(),
        marked,
    ))
}

/// A stable id for a question read off the screen, so the transcript's copy
/// of the same question lands on the same row rather than beside it.
pub fn screen_question_id(q: &Question) -> SurfaceId {
    SurfaceId(format!(
        "ask-live-{}",
        crate::derive::short_hash(&q.question)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real thing, transcribed from a pane on 2026-09-17 — the question
    /// that exposed the whole gap, because it was on screen and nowhere in
    /// the transcript.
    fn mordor_rows() -> Vec<String> {
        [
            "› pose me a question WORTHY OF MORDOR",
            "",
            "□ The Ring",
            "",
            "The Ring is in your hand at the Cracks of Doom. What do you actually do?",
            "",
            "❯ 1. Cast it into the fire",
            "     Destroy the thing itself. Every power built on it falls with it.",
            "  2. Claim it",
            "     Put it on and be the one who decides.",
            "  3. Give it to the Wise",
            "     Hand it to whoever is older, stronger, better counselled.",
            "  4. Carry it, undecided",
            "     Refuse the choice. Keep walking, keep it hidden, decide later.",
            "  5. Type something.",
            "",
            "  6. Chat about this",
            "",
            "Enter to select · ↑/↓ to navigate · Esc to cancel",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn rows(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|s| s.to_string()).collect()
    }

    fn u_row(s: &str) -> String {
        s.to_string()
    }

    /// The pin, and the edge that clears it.
    ///
    /// A permission prompt is the visible tail of a stopped pane and does not
    /// scroll away, so the 120ms scan re-asserts it forever — that is the
    /// blinker that once stayed on permanently. Answering suppresses exactly
    /// that screen, and nothing else.
    #[test]
    fn an_answered_prompt_stops_re_arming_until_the_screen_changes() {
        let asking = rows(&["Do you want to proceed?", "  1. Yes", "  esc to cancel"]);

        // Positive control: without the edge this is a live question, and it
        // stays one however many times the scan looks at it.
        assert!(wants_human(&asking));
        assert!(wants_human_unless_answered(&asking, None));

        // The person types at it. That screen is now answered.
        let answered = rows_fingerprint(&asking);
        assert!(!wants_human_unless_answered(&asking, Some(answered)));

        // The agent asks something NEW. A different screen is a different
        // question, and suppressing it would be the failure this whole rail
        // exists to prevent.
        let asking_again = rows(&["Do you trust the files in this folder?", "  esc to cancel"]);
        assert!(wants_human_unless_answered(&asking_again, Some(answered)));
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

    /// The suppression is keyed on the screen, so a picker the person is
    /// steering re-arms the moment the selection moves — one row differing is
    /// enough. Without this the down-arrow through a permission picker would
    /// silence a prompt that is still waiting.
    #[test]
    fn moving_a_pickers_selection_re_arms_the_prompt() {
        let before = rows(&[
            "Do you want to proceed?",
            "> 1. Yes",
            "  2. No",
            "  esc to cancel",
        ]);
        let after = rows(&[
            "Do you want to proceed?",
            "  1. Yes",
            "> 2. No",
            "  esc to cancel",
        ]);
        let answered = rows_fingerprint(&before);
        assert_ne!(rows_fingerprint(&before), rows_fingerprint(&after));
        assert!(!wants_human_unless_answered(&before, Some(answered)));
        assert!(wants_human_unless_answered(&after, Some(answered)));
    }

    /// A screen nobody has answered to suppresses nothing, and a fingerprint is
    /// stable across the trailing whitespace a terminal pads rows with — the
    /// cells to the right of the text change as the grid resizes, and a prompt
    /// that "changed" because the pane got wider is not a new question.
    #[test]
    fn no_answer_suppresses_nothing_and_padding_is_not_a_change() {
        let asking = rows(&["Do you want to proceed?", "  esc to cancel"]);
        let padded = rows(&["Do you want to proceed?   ", "  esc to cancel      "]);
        assert!(wants_human_unless_answered(&asking, None));
        assert_eq!(rows_fingerprint(&asking), rows_fingerprint(&padded));
    }

    /// The review step at the end of a round draws no footer, so the rule
    /// that catches every other picker misses this one entirely.
    #[test]
    fn the_review_step_is_a_question_even_with_no_footer_under_it() {
        // Transcribed from the screen after pressing the picker's Submit.
        // There is no "Enter to select" line here — that is the whole
        // problem, and it is why the bench read Idle while the agent was
        // plainly waiting on somebody.
        let review = rows(&[
            "Review your answers",
            "  \u{25cf} Round 3. Which of these belong on a desk?",
            "     \u{2192} A second keyboard",
            "Ready to submit your answers?",
            "  ) 1. Submit answers",
            "    2. Cancel",
        ]);
        assert!(
            wants_human(&review),
            "the second half of a commit is still a question"
        );

        // And the strictness it must not cost: an agent WRITING about
        // submitting answers is an agent working, not an agent blocked.
        let prose = rows(&[
            "I will review your answers and then submit them.",
            "Should I go ahead and submit your answers?",
            "Running the suite now.",
        ]);
        assert!(
            !wants_human(&prose),
            "prose about submitting is not the picker asking"
        );
    }

    #[test]
    fn a_finished_line_and_ordinary_prose_are_not_working() {
        // The done line from the same screen. It has an elapsed time and no
        // tokens and no bracket, and confusing the two would make every
        // finished agent look busy forever.
        assert!(!row_is_working("✳ Churned for 13s · done 11:22 AM"));
        // Prose that mentions tokens. No bracket, no elapsed time, not a
        // spinner — and the agent talks about tokens constantly.
        assert!(!row_is_working("we spent 7.5k tokens on that turn"));
        assert!(!row_is_working(
            "the context window is 200k tokens and we used half"
        ));
        // A bracket with tokens but no duration: a caption, not a spinner.
        assert!(!row_is_working("Prompt cache (main): 12k tokens"));
        // And a bracket with a duration but no tokens.
        assert!(!row_is_working("finished (49s)"));
        assert!(!row_is_working(""));
    }

    #[test]
    fn a_working_spinner_is_recognised_by_its_shape_not_its_verb() {
        // Transcribed from a photograph of the running agent. It had been
        // composing for forty-nine seconds while the bench read Idle, because
        // the old detector wanted `esc to interrupt` and this build prints it
        // almost nowhere.
        assert!(row_is_working("✳ Composing… (49s · ↓ 7.5k tokens)"));
        // The CLI varies the gerund on purpose and keeps the structure, so
        // the structure is what this matches.
        for gerund in ["Churning", "Cerebrating", "Sautéing", "Baking", "Noodling"] {
            let row = format!("✳ {gerund}… (3s · ↓ 120 tokens)");
            assert!(row_is_working(&row), "{row}");
        }
        // Older builds and the retry path still say it, and dropping support
        // for the previous format is how this broke the first time.
        assert!(row_is_working("✳ Thinking… (12s · esc to interrupt)"));
        assert!(row_is_working(
            "  · next try in 4s · attempt 2 · esc to interrupt"
        ));
    }

    #[test]
    fn a_lone_question_is_not_a_round_and_says_so() {
        // A single question with a Submit button is a question, not a
        // workflow, and a one-segment progress bar under it would invent a
        // sequence that does not exist. `None` is the honest answer.
        let one = vec![u_row("← ⊡ Desk items ✔ Submit →")];
        assert!(round_on_screen(&one).is_none());
        // And a screen with no strip at all.
        let plain = vec![
            u_row("Where should this page live?"),
            u_row("  1. Artifact only"),
        ];
        assert!(round_on_screen(&plain).is_none());
    }

    #[test]
    fn a_multi_select_parses_its_boxes_as_state_and_its_submit_as_a_position() {
        // Transcribed from a photograph of the running picker. Two things
        // here were landing in the wrong place: the checkbox was part of the
        // LABEL, so the chip read "1 · [✓] A second keyboard" until the
        // terminal redrew mid-click and it read "1 · A second keyboard"
        // instead — the flicker Parker saw was one option parsed two ways.
        // And Submit was being read as a description of option five.
        let rows: Vec<String> = [
            ") 1. [✓] A second keyboard",
            "    Not plugged in. Kept for the day the first one dies.",
            "  2. [ ] A plant, thriving",
            "    Watered on a schedule somebody actually keeps.",
            "  3. [ ] A plant, not thriving",
            "  4. [ ] Cable you cannot identify",
            "  5. [ ] Type something",
            "      Submit",
            "  6. Chat about this",
            "Enter to select · ↑/↓ to navigate · Esc to cancel",
        ]
        .iter()
        .map(|r| r.to_string())
        .collect();

        let (_, options, _, submit) = collect_options(&rows).expect("a multi-select");
        assert_eq!(options.len(), 6, "six options, Submit is not one of them");
        assert_eq!(options[0].label, "A second keyboard", "no box in the name");
        assert_eq!(options[0].checked, Some(true), "the box is state");
        assert_eq!(options[1].checked, Some(false));
        assert_eq!(
            options[5].checked, None,
            "the trailing option has no box at all, which is not the same as an empty one"
        );
        assert_eq!(
            options[4].what_happens, None,
            "Submit is the picker's button, not a description of option five"
        );

        // And it sits at five in the up/down order, between option five and
        // the trailing `Chat about this`.
        assert_eq!(submit, Some(5));
        assert_eq!(
            crate::workbench::nav_index(5, submit),
            6,
            "option six shifts"
        );
    }

    #[test]
    fn a_picker_that_draws_a_preview_beside_its_options_keeps_it_out_of_them() {
        // Transcribed from a photograph of the running picker. The agent drew
        // a preview panel in a second column, which lines up perfectly in a
        // terminal and falls apart on a bench, where every row becomes its own
        // element and the second column has nowhere to be.
        let rows: Vec<String> = [
            "Round 4. Which layout reads better in a narrow pane?",
            ") 1. Stacked rows        ┌────────┐",
            "  2. Two columns         │ ┌────┐ │",
            "  3. Inline run          │ │ LINES HELD │ │",
            "                         │ │ 4          │ │",
            "                         └────────┘",
            "  4. Chat about this",
            "Enter to select \u{b7} \u{2191}/\u{2193} to navigate \u{b7} Esc to cancel",
        ]
        .iter()
        .map(|r| r.to_string())
        .collect();

        let q = question_on_screen(&rows).expect("a question");
        assert_eq!(
            q.question, "Round 4. Which layout reads better in a narrow pane?",
            "the question keeps its own words"
        );
        assert_eq!(q.options.len(), 4);
        assert_eq!(q.options[0].label, "Stacked rows", "no panel in the button");
        assert_eq!(q.options[1].label, "Two columns");
        assert_eq!(q.options[2].label, "Inline run");
        assert_eq!(q.options[3].label, "Chat about this");
        // And the panel's own rows are not descriptions of anything. A row
        // that is nothing but box-drawing belonged to the panel, never to the
        // option above it.
        for (i, o) in q.options.iter().enumerate() {
            assert_eq!(
                o.what_happens, None,
                "option {i} picked up a slice of the preview: {:?}",
                o.what_happens
            );
        }
    }

    #[test]
    fn a_round_at_its_submit_step_reports_everything_answered() {
        let rows = vec![u_row("← ⊠ Tomorrow ⊠ Mug ✔ Submit →")];
        let r = round_on_screen(&rows).expect("a round");
        assert_eq!((r.answered(), r.total()), (2, 2));
    }

    #[test]
    fn a_screen_with_no_menu_at_all_yields_nothing() {
        let rows: Vec<String> = ["› cargo test", "running 1077 tests", "ok"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(question_on_screen(&rows).is_none());
    }

    #[test]
    fn cutting_at_a_side_panel_leaves_ordinary_labels_alone() {
        // The strictness it must not cost. None of these contains
        // box-drawing, and every one of them has to survive untouched —
        // including the marks this reader depends on.
        for plain in [
            "Stacked rows",
            "A plant, thriving",
            "Artifact only (Recommended)",
            "\u{2713} already ticked",
            "\u{276f} a gutter mark",
            "100% \u{b7} nothing to cut here",
        ] {
            assert_eq!(without_side_panel(plain), plain, "{plain}");
        }
        // And the cut itself, on the one shape it exists for.
        assert_eq!(
            without_side_panel("Stacked rows   \u{250c}\u{2500}\u{2510}"),
            "Stacked rows"
        );
        assert_eq!(without_side_panel("\u{2502} \u{2502} 4 \u{2502}"), "");
    }

    #[test]
    fn prose_that_merely_contains_numbers_is_not_a_question() {
        let rows: Vec<String> = [
            "I made three changes:",
            "1. renamed the field",
            "then ran the tests and they passed.",
            "42. is not an option either",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        // One option is not a menu, and an out-of-sequence number is prose.
        assert!(question_on_screen(&rows).is_none());
    }

    #[test]
    fn the_cursor_is_read_from_the_gutter_not_assumed() {
        let mut rows = mordor_rows();
        rows[6] = "  1. Cast it into the fire".into();
        rows[10] = "❯ 3. Give it to the Wise".into();
        let q = question_on_screen(&rows).expect("a question");
        assert_eq!(
            q.cursor,
            Some(2),
            "answering has to know where the highlight is, or it walks the wrong way"
        );
    }

    #[test]
    fn the_live_question_is_read_off_the_screen_because_the_file_does_not_have_it() {
        let q = question_on_screen(&mordor_rows()).expect("a question");
        assert_eq!(
            q.question,
            "The Ring is in your hand at the Cracks of Doom. What do you actually do?"
        );
        assert_eq!(
            q.options.len(),
            6,
            "including 'Type something' and 'Chat about this'"
        );
        assert_eq!(q.options[0].label, "Cast it into the fire");
        assert_eq!(
            q.options[1].what_happens.as_deref(),
            Some("Put it on and be the one who decides.")
        );
        assert_eq!(q.answer, Answered::Waiting);
        assert_eq!(q.cursor, Some(0), "the picker opens on its first option");
    }

    #[test]
    fn the_pickers_own_tab_bar_says_how_far_through_the_round_we_are() {
        // Transcribed from a photograph of the running picker, glyphs and
        // spacing intact. The strip is the ONLY place the shape of a round is
        // stated, so this is the measurement the progress bar rests on.
        let rows = vec![
            u_row("←  ⊠ Tomorrow   ⊡ Mug   ✔ Submit  →"),
            u_row("Round 2, second of two. Where does the coffee sit?"),
        ];
        let r = round_on_screen(&rows).expect("a two-question round");
        assert_eq!(
            r.total(),
            2,
            "Submit is the end of a round, not a step in it"
        );
        assert_eq!(r.answered(), 1);
        assert_eq!(r.steps[0].label, "Tomorrow");
        assert!(r.steps[0].done);
        assert_eq!(r.steps[1].label, "Mug");
        assert!(!r.steps[1].done);
        assert!(r.submitting, "the strip carries a Submit step");
    }

    #[test]
    fn the_same_live_question_keeps_one_id_while_the_cursor_moves() {
        // The id is the question's wording, not the screen's state — so
        // arrowing up and down the menu updates one row instead of stacking
        // a row per keystroke.
        let a = question_on_screen(&mordor_rows()).unwrap();
        let mut rows = mordor_rows();
        rows[6] = "  1. Cast it into the fire".into();
        rows[10] = "❯ 3. Give it to the Wise".into();
        let b = question_on_screen(&rows).unwrap();
        assert_ne!(a.cursor, b.cursor, "the cursor did move");
        assert_eq!(screen_question_id(&a), screen_question_id(&b));
    }

    // The five the law caught on its first run. Each had been exercised only
    // through a wrapper, which is coverage of the wrapper and not of the
    // reader — a wrapper can be right about a reader that is wrong.

    #[test]
    fn a_single_row_says_whether_it_wants_a_person() {
        // The picker's footer and a permission prompt, as the CLI prints them.
        assert!(row_wants_human(
            "Enter to select \u{b7} \u{2191}/\u{2193} to navigate \u{b7} Esc to cancel"
        ));
        assert!(row_wants_human("Do you want to proceed?"));
        assert!(row_wants_human("Ready to submit your answers?"));
        // The strictness it must not cost: the agent's own prose ends in
        // question marks constantly and none of it is a picker.
        assert!(!row_wants_human(
            "Should I go ahead and submit your answers?"
        ));
        assert!(!row_wants_human(
            "\u{2733} Composing\u{2026} (49s \u{b7} \u{2193} 7.5k tokens)"
        ));
        assert!(!row_wants_human(""));
    }

    #[test]
    fn the_matching_row_is_returned_not_just_the_fact_of_a_match() {
        // `wants_human_row` is what the rail QUOTES, so it has to hand back
        // the row that matched and not the question above it.
        let screen = rows(&[
            "Do you want to proceed?",
            "  1. Yes",
            "  2. No",
            "Enter to select \u{b7} Esc to cancel",
        ]);
        assert_eq!(
            wants_human_row(&screen),
            Some("Enter to select \u{b7} Esc to cancel"),
            "the newest matching row, walking up from the bottom"
        );
        assert_eq!(
            wants_human_row(&rows(&["just output", "more output"])),
            None
        );
    }

    #[test]
    fn a_wall_is_recognised_one_row_at_a_time() {
        // The CLI's own banners — verbatim — and the tool output that must
        // never be mistaken for one, because a failing build is the agent
        // WORKING.
        for wall in [
            "  \u{23bf}  API Error: 529 overloaded_error",
            "  You've reached your usage limit \u{2014} resets 3pm",
            "  Credit balance too low",
            "  OAuth token has expired",
            "  Request timed out after 60s",
        ] {
            assert!(row_blocked(wall), "{wall:?}");
        }
        for busy in [
            "error[E0308]: mismatched types",
            "test result: FAILED. 3 passed; 1 failed",
            "",
        ] {
            assert!(!row_blocked(busy), "{busy:?}");
        }
        // And the row-returning form hands back the banner itself.
        let screen = rows(&[
            "building...",
            "  \u{23bf}  API Error: 529 overloaded_error",
            "",
        ]);
        assert_eq!(
            blocked_row(&screen),
            Some("\u{23bf}  API Error: 529 overloaded_error")
        );
        assert_eq!(blocked_row(&rows(&["all fine"])), None);
    }

    #[test]
    fn evidence_is_quoted_clean_and_clipped() {
        // A permission prompt as it sits inside the CLI's box-drawn frame.
        // The frame is furniture and the quote must not carry it.
        assert_eq!(
            clip_evidence("\u{2502}  Do you want to proceed?   \u{2502}"),
            Some("Do you want to proceed?".to_string())
        );
        // Runs of whitespace collapse: a row is padded to the terminal's
        // width and the rail has no width to spare.
        assert_eq!(
            clip_evidence("  Enter to select     \u{b7}     Esc to cancel  "),
            Some("Enter to select \u{b7} Esc to cancel".to_string())
        );
        // Longer than the rail can carry: clipped at the limit and marked.
        let long = "x".repeat(200);
        let got = clip_evidence(&long).expect("something survives");
        assert_eq!(got.chars().count(), 96, "95 characters and an ellipsis");
        assert!(got.ends_with('\u{2026}'));
        // Nothing but furniture is nothing.
        assert_eq!(clip_evidence("\u{2502}\u{2500}\u{2500}\u{2502}"), None);
        assert_eq!(clip_evidence("   "), None);
    }

    /// The law of this module, enforced.
    ///
    /// Every public reader here must be exercised by at least one test in this
    /// module. This scans the SOURCE for the public function names and the
    /// test module for calls to them, so a reader added without a test fails
    /// here rather than in front of somebody a month later — and it is a scan
    /// rather than a list, because a list guards only the names on it.
    #[test]
    fn every_public_reader_has_a_test_in_this_module() {
        let src = include_str!("screenread.rs");
        let (code, tests) = src.split_once("#[cfg(test)]").expect("a test module");
        let mut missing = Vec::new();
        for line in code.lines() {
            let Some(rest) = line.strip_prefix("pub fn ") else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                continue;
            }
            let called = tests.contains(&format!("{name}(")) || tests.contains(&format!("{name}("));
            if !called {
                missing.push(name);
            }
        }
        assert!(
            missing.is_empty(),
            "public readers with no test in screenread::tests: {missing:?}"
        );
    }
}
