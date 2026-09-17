//! Surfaces the agent did not have to send.
//!
//! The declared half of this protocol assumes an agent will call a verb. The
//! first night of real use said otherwise, in the plainest possible way: an
//! agent wrote a page, printed a `Deliverable:` line and a Links table, asked
//! a multiple-choice question — and the bench beside it stayed empty, because
//! none of that was a surface.
//!
//! So this module reads what the agent already produced and makes the
//! surfaces itself. Nothing here needs cooperation, a protocol, an MCP
//! connection or a briefing. It works on a Codex pane, on a Claude pane
//! somebody opened by hand, and on a conversation that started before this
//! feature existed.
//!
//! ```text
//!   the agent's own JSONL transcript
//!            │
//!            ├── AskUserQuestion with no result yet   →  question  (waiting)
//!            ├── AskUserQuestion with a result        →  question  (answered)
//!            └── a "Deliverable:" line in its prose   →  artifact
//! ```
//!
//! # Read the transcript, never the screen
//!
//! [`crate::mcp_tail`] already walks each agent's JSONL for the tab face, and
//! that is the source used here too. Scraping the rendered grid would mean
//! recovering structure from a drawing that threw it away — and a TUI menu
//! repaints, wraps and scrolls, so the same question would arrive three times
//! in three shapes.
//!
//! # Derived surfaces are re-derived, not accumulated
//!
//! Every sweep produces the same posts for the same transcript, with the same
//! stable ids, so presenting them is idempotent. That is the whole reason ids
//! are derived from the tool-use id and the href rather than from a clock:
//! a retry updates one row instead of stacking twenty.

use std::path::Path;

use serde_json::Value;

use crate::surface::{
    Answered, Artifact, Choice_, Kind, Op, Post, Question, Round, Source, Step, Surface, SurfaceId,
    Weight,
};

/// Everything derivable from one transcript, oldest first.
///
/// `now_ms` stamps arrival, as it does for a declared surface: the agent's own
/// clock is one more thing that can be wrong, and ordering a bench by it would
/// let a bad clock jump the queue.
pub fn from_transcript(path: &Path, now_ms: u64) -> Vec<Post> {
    let Some(body) = crate::mcp_tail::read_tail_public(path) else {
        return Vec::new();
    };
    from_jsonl(&body, now_ms)
}

/// The same, over text — the seam every test uses.
pub fn from_jsonl(body: &str, now_ms: u64) -> Vec<Post> {
    let mut asked: Vec<Asked> = Vec::new();
    let mut deliverables: Vec<(String, String)> = Vec::new();

    for line in body.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else {
            continue; // a partial line at the head of a tail read
        };
        for block in content_blocks(&v) {
            match block.get("type").and_then(Value::as_str) {
                Some("tool_use") => {
                    if block.get("name").and_then(Value::as_str) == Some("AskUserQuestion") {
                        if let Some(a) = parse_ask(block) {
                            asked.push(a);
                        }
                    }
                }
                Some("tool_result") => {
                    // The result closes whichever question it names. Matched by
                    // id rather than by position, because a turn can hold
                    // several tool calls and they do not come back in order.
                    let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else {
                        continue;
                    };
                    if let Some(a) = asked.iter_mut().find(|a| a.id == id) {
                        a.answer = answered_with(block, &a.options);
                    }
                }
                // Assistant prose only. The convention lives in AGENTS.md,
                // which is in every system prompt, so the words appear in the
                // transcript far more often than an actual deliverable does.
                Some("text") if v.get("type").and_then(Value::as_str) == Some("assistant") => {
                    if let Some(text) = block.get("text").and_then(Value::as_str) {
                        deliverables.extend(deliverable_lines(text));
                    }
                }
                _ => {}
            }
        }
    }

    let mut out: Vec<Post> = Vec::new();
    for a in asked {
        out.push(a.into_post(now_ms));
    }
    // Only the most recent deliverable. An agent that has declared six over a
    // long session has superseded five of them, and a bench full of the same
    // report at six ages is a worse answer than one row that is current.
    if let Some((label, href)) = deliverables.pop() {
        out.push(deliverable_post(&label, &href, now_ms));
    }
    out
}

// ---------------------------------------------------------------------------
// the live question, which the transcript does not have
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
        .map(|r| r.trim())
        .find(|r| {
            r.len() > 8
                && !r.starts_with('╭')
                && !r.starts_with('│')
                && !r.starts_with('─')
                && !r.ends_with("to cancel")
        })?
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
        let (label, checked) = split_checkbox(&label);
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
    SurfaceId(format!("ask-live-{}", short_hash(&q.question)))
}

/// A question found in the transcript, and whether it has been answered.
struct Asked {
    id: String,
    question: String,
    options: Vec<Choice_>,
    answer: Answered,
}

impl Asked {
    fn into_post(self, now_ms: u64) -> Post {
        let id = SurfaceId(format!("ask-{}", tail_of(&self.id)));
        let title = self.question.chars().take(72).collect::<String>();
        let waiting = matches!(self.answer, Answered::Waiting);
        let kind = Kind::Question(Question {
            question: self.question,
            options: self.options,
            recommend: None,
            answer: self.answer,
            // Where the agent's own menu is sitting. Claude Code opens a
            // picker on its first option, and the bench answers by driving
            // that menu — so a question we merely observed carries the cursor
            // and a question somebody declared does not.
            cursor: waiting.then_some(0),
            round: None,
            submit: None,
        });
        let mut actions = kind.default_actions();
        actions.push(crate::surface::Action::AskAgent);
        Post {
            op: Op::Present,
            pane: None,
            surface: Some(Surface {
                id: id.clone(),
                title,
                kind,
                weight: Weight::default(),
                actions,
                source: None,
                arrived_ms: now_ms,
            }),
            id,
        }
    }
}

fn parse_ask(block: &Value) -> Option<Asked> {
    let id = block.get("id").and_then(Value::as_str)?.to_string();
    // Claude Code sends `questions: [{question, header, options: [{label,
    // description}], multiSelect}]`. Only the first is taken: the bench
    // answers by driving a menu, and a multi-question prompt is several menus
    // in sequence, which is a thing to get right rather than guess at.
    let q = block
        .pointer("/input/questions")
        .and_then(Value::as_array)
        .and_then(|a| a.first())?;
    let question = q.get("question").and_then(Value::as_str)?.to_string();
    let options: Vec<Choice_> = q
        .get("options")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|o| {
                    Some(Choice_ {
                        checked: None,
                        label: match o {
                            Value::String(s) => s.clone(),
                            other => other.get("label").and_then(Value::as_str)?.to_string(),
                        },
                        what_happens: o
                            .get("description")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    (!options.is_empty()).then_some(Asked {
        id,
        question,
        options,
        answer: Answered::Waiting,
    })
}

/// How a result answered, as precisely as it can be read.
///
/// Claude Code writes the answer back in one shape:
///
/// ```text
/// The user answered: "<the question>"="<what they said>". Read the answers…
/// ```
///
/// So the words are there, and they are worth having: the first question this
/// ever derived from a live transcript was answered *"All good - just
/// diagnostics for now"*, which is none of the options and is the only part
/// of the surface worth reading. Three outcomes, in order of how much is
/// known — an option, the words, or the honest nothing.
fn answered_with(block: &Value, options: &[Choice_]) -> Answered {
    let Some(text) = result_text(block) else {
        return Answered::ChoseUnknown;
    };
    let said = quoted_answer(&text);
    let hay = said.as_deref().unwrap_or(&text);
    if let Some(i) = options
        .iter()
        .position(|o| !o.label.is_empty() && hay.contains(&o.label))
    {
        return Answered::Chose(i);
    }
    match said {
        Some(words) if !words.trim().is_empty() => Answered::Typed(words),
        _ => Answered::ChoseUnknown,
    }
}

/// The `="…"` half of an answer line, if it is shaped like one.
fn quoted_answer(text: &str) -> Option<String> {
    let at = text.find("\"=\"")?;
    let rest = &text[at + 3..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn result_text(block: &Value) -> Option<String> {
    match block.get("content") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Array(a)) => Some(
            a.iter()
                .filter_map(|c| c.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(" "),
        ),
        _ => None,
    }
}

/// Every `Deliverable:` line in a piece of assistant prose, as `(label, href)`.
///
/// The house convention, already mandatory in `AGENTS.md` and already written
/// by every agent on this machine:
///
/// ```text
/// Deliverable: The Workbench — file:///home/parker/Work/reports/x.html
/// ```
///
/// The em dash is what the rule specifies and a hyphen is what gets typed, so
/// both split it. A line with no URL is not a deliverable, however it is
/// punctuated.
fn deliverable_lines(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim().trim_start_matches("**").trim_start_matches("- ");
        let Some(rest) = line
            .strip_prefix("Deliverable:")
            .or_else(|| line.strip_prefix("**Deliverable:**"))
        else {
            continue;
        };
        // `**Deliverable:**` loses its opening stars above and its closing
        // ones here. Markdown emphasis is how the rule is written in
        // AGENTS.md, so the bold form is the common one rather than the edge
        // case.
        let rest = rest.trim_start_matches("**").trim();
        let Some(at) = find_href(rest) else { continue };
        let href = rest[at..]
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_end_matches(['.', ',', ')'])
            .to_string();
        let label = rest[..at]
            .trim()
            .trim_end_matches(['—', '-', '–', ':'])
            .trim()
            .to_string();
        let label = if label.is_empty() {
            href.rsplit('/').next().unwrap_or(&href).to_string()
        } else {
            label
        };
        out.push((label, href));
    }
    out
}

/// Turn whatever the agent wrote into a name a person would give the thing.
///
/// A derived surface's title is the first and often the only thing anybody
/// reads about it, and it arrives from two untidy places: a label an agent
/// typed, or a filename. Both carry machine debris — a content hash, an ISO
/// date, an extension, a session id — and the bench was showing it. Parker, on
/// an artifact called *Bench Paste 15868dd2*: *"artifact names are UNHELPFUL!
/// ... These artifacts should be a HUMAN READABLE VERY SHORT NAME!"*
///
/// So the debris is removed here rather than asked for in a prompt. An
/// instruction in `AGENTS.md` telling agents to write short titles is worth
/// having and is not a mechanism: it is advisory, unversioned, and silently
/// absent for every agent that has not read it, whereas every derived title in
/// this window passes through this function.
///
/// Conservative on purpose — it drops tokens that cannot be words and touches
/// nothing else. A real title stays exactly as written.
pub fn human_title(raw: &str) -> String {
    let base = raw.trim();
    // A bare path or URL becomes its filename first; a label is already a
    // label and keeps its spaces.
    let base = if base.contains(' ') {
        base.to_string()
    } else {
        // A path or URL names itself with its LAST segment that is a word.
        // `…/artifact/DhG556CDbtwZHQ3RPbsqR6` is named by the collection it
        // sits in, because the id is the one part of it that says nothing —
        // which is the same rule a person uses reading the URL aloud.
        base.rsplit('/')
            .map(strip_extension)
            .find(|seg| !seg.is_empty() && !seg.split(['-', '_', '.']).all(is_debris))
            .unwrap_or_else(|| base.to_string())
    };
    let words: Vec<String> = base
        .split(['-', '_', ' ', '.'])
        .map(str::trim)
        .filter(|w| !w.is_empty() && !is_debris(w))
        .map(|w| w.to_string())
        .collect();
    if words.is_empty() {
        // Everything was debris. The raw string, clipped, beats an empty
        // heading — a card with no title is a card nobody can refer to.
        return raw.chars().take(48).collect();
    }
    // Six words is the length of a name somebody says out loud. Past that it
    // is a sentence, and a sentence belongs in the summary underneath.
    let mut out = words.iter().take(6).cloned().collect::<Vec<_>>().join(" ");
    if words.len() > 6 {
        out.push('\u{2026}');
    }
    let mut c = out.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => out,
    }
}

/// A token that cannot be a word: a hash, an id, a date, a bare number.
///
/// The digit is what makes this safe. `deadbeef` is hex and is also a word
/// somebody might legitimately title something, so hex alone would eat real
/// titles; hex WITH a digit in it, six characters or more, is a checksum every
/// time. `2026` and `08` go because a date is metadata the bench already has.
fn is_debris(w: &str) -> bool {
    let lower = w.to_ascii_lowercase();
    if lower.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    let hexish = lower.len() >= 6
        && lower.chars().all(|c| c.is_ascii_hexdigit())
        && lower.chars().any(|c| c.is_ascii_digit());
    // A long unbroken run of letters and digits with no vowel is an id, not a
    // word: `DhG556CDbtwZHQ3RPbsqR6`, which is what a published artifact's
    // own URL ends in.
    let idish = w.len() >= 12
        && w.chars().all(|c| c.is_ascii_alphanumeric())
        && w.chars().any(|c| c.is_ascii_digit())
        && !lower.chars().any(|c| "aeiou".contains(c));
    hexish || idish
}

fn strip_extension(s: &str) -> String {
    match s.rsplit_once('.') {
        Some((stem, ext))
            if !stem.is_empty()
                && ext.len() <= 5
                && ext.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            stem.to_string()
        }
        _ => s.to_string(),
    }
}

fn find_href(s: &str) -> Option<usize> {
    ["file:///", "https://", "http://", "/"]
        .iter()
        .filter_map(|p| s.find(p))
        .min()
}

fn deliverable_post(label: &str, href: &str, now_ms: u64) -> Post {
    // `file://` is stripped to a path: the desktop handler takes either, and a
    // path is what the rest of this window means by a file.
    let target = href.strip_prefix("file://").unwrap_or(href).to_string();
    let id = SurfaceId(format!("deliv-{}", short_hash(&target)));
    let kind = Kind::Artifact(Artifact {
        mime: mime_of(&target),
        summary: Some("declared in the agent's own reply".into()),
        href: target.clone(),
    });
    let actions = kind.default_actions();
    Post {
        op: Op::Present,
        pane: None,
        surface: Some(Surface {
            id: id.clone(),
            title: human_title(label),
            kind,
            weight: Weight::default(),
            actions,
            source: Some(Source {
                files: vec![target],
                command: None,
                reference: Some("Deliverable: line".into()),
            }),
            arrived_ms: now_ms,
        }),
        id,
    }
}

fn mime_of(path: &str) -> Option<String> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    Some(
        match ext.as_str() {
            "html" | "htm" => "text/html",
            "md" => "text/markdown",
            "pdf" => "application/pdf",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "svg" => "image/svg+xml",
            "json" => "application/json",
            _ => return None,
        }
        .to_string(),
    )
}

/// Content blocks of a transcript line, whichever shape it is in.
fn content_blocks(v: &Value) -> Vec<&Value> {
    v.pointer("/message/content")
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

fn tail_of(id: &str) -> String {
    id.chars()
        .rev()
        .take(12)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

fn short_hash(s: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:012x}")
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_round_at_its_submit_step_reports_everything_answered() {
        let rows = vec![u_row("← ⊠ Tomorrow ⊠ Mug ✔ Submit →")];
        let r = round_on_screen(&rows).expect("a round");
        assert_eq!((r.answered(), r.total()), (2, 2));
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

    fn u_row(s: &str) -> String {
        s.to_string()
    }

    #[test]
    fn a_title_a_person_would_give_it() {
        // The specimen: what the bench actually showed.
        assert_eq!(human_title("Bench Paste 15868dd2"), "Bench Paste");
        // A filename, with everything a filename carries.
        assert_eq!(
            human_title("/tmp/x/2026-09-17-workbench-brief.html"),
            "Workbench brief"
        );
        // A published artifact's URL ends in an id, not a word.
        assert_eq!(
            human_title("https://claude.ai/artifact/DhG556CDbtwZHQ3RPbsqR6"),
            "Artifact"
        );
        // A real title is left alone. This is the case worth protecting: a
        // cleaner that improves nine titles and mangles the tenth is not an
        // improvement.
        assert_eq!(
            human_title("Why the agent describes meaning"),
            "Why the agent describes meaning"
        );
        // Words that merely LOOK like hex survive, because they have no digit.
        assert_eq!(
            human_title("decade-facade-effaced"),
            "Decade facade effaced"
        );
        // Long ones are cut on a word and say they were cut.
        let long = human_title("one two three four five six seven eight");
        assert!(long.starts_with("One two three four five six"), "{long}");
        assert!(long.ends_with('\u{2026}'), "{long}");
        // And a name made of nothing but debris keeps the raw string rather
        // than becoming an untitled card.
        assert_eq!(human_title("15868dd2"), "15868dd2");
    }

    #[test]
    fn a_derived_deliverable_carries_the_tidied_title() {
        // Through the real path, not just the helper: the tidy has to be
        // wired in, and asserting the function alone would pass either way.
        let post = deliverable_post("Bench Paste 15868dd2", "file:///tmp/pastes/15868dd2.png", 0);
        assert_eq!(
            post.surface.expect("a surface").title,
            "Bench Paste",
            "the hash must not reach the rail"
        );
    }
    use serde_json::json;

    const NOW: u64 = 1_758_000_000_000;

    fn assistant(blocks: Value) -> String {
        json!({ "type": "assistant", "message": { "content": blocks } }).to_string()
    }

    fn user(blocks: Value) -> String {
        json!({ "type": "user", "message": { "content": blocks } }).to_string()
    }

    fn ask_block(id: &str) -> Value {
        json!({
            "type": "tool_use", "id": id, "name": "AskUserQuestion",
            "input": { "questions": [{
                "question": "What should the placeholder page actually hold?",
                "header": "Page content",
                "options": [
                    { "label": "Session status page", "description": "What landed, what's left." },
                    { "label": "Drawn decision brief", "description": "SVG figures, annotatable." },
                    { "label": "Delete it", "description": "It was a misfire." }
                ]
            }]}
        })
    }

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
    fn a_screen_with_no_menu_at_all_yields_nothing() {
        let rows: Vec<String> = ["› cargo test", "running 1077 tests", "ok"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(question_on_screen(&rows).is_none());
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

    #[test]
    fn a_pending_question_becomes_a_waiting_surface() {
        let posts = from_jsonl(&assistant(json!([ask_block("toolu_01ABCDEF")])), NOW);
        assert_eq!(posts.len(), 1);
        let s = posts[0].surface.as_ref().unwrap();
        assert_eq!(s.kind.id(), "question");
        assert!(s.title.starts_with("What should the placeholder"));
        match &s.kind {
            Kind::Question(q) => {
                assert_eq!(q.options.len(), 3);
                assert_eq!(q.options[0].label, "Session status page");
                assert_eq!(
                    q.options[1].what_happens.as_deref(),
                    Some("SVG figures, annotatable.")
                );
                assert_eq!(q.answer, Answered::Waiting);
                assert_eq!(
                    q.cursor,
                    Some(0),
                    "a menu we can drive knows where it starts"
                );
            }
            other => panic!("{}", other.id()),
        }
        assert_eq!(s.subtitle(), "3 options · waiting on you");
    }

    #[test]
    fn a_result_naming_an_option_closes_the_question_with_that_option() {
        let body = format!(
            "{}\n{}",
            assistant(json!([ask_block("toolu_1")])),
            user(json!([{
                "type": "tool_result", "tool_use_id": "toolu_1",
                "content": [{ "type": "text", "text": "Drawn decision brief" }]
            }]))
        );
        let posts = from_jsonl(&body, NOW);
        assert_eq!(posts.len(), 1, "the question is updated, not duplicated");
        match &posts[0].surface.as_ref().unwrap().kind {
            Kind::Question(q) => {
                assert_eq!(q.answer, Answered::Chose(1));
                assert_eq!(q.cursor, None, "an answered menu is not ours to drive");
            }
            other => panic!("{}", other.id()),
        }
    }

    #[test]
    fn a_free_text_answer_keeps_the_words() {
        // The real shape, from the first question this ever derived off a live
        // transcript. The answer was none of the options and was the only
        // interesting thing on the surface.
        let body = format!(
            "{}\n{}",
            assistant(json!([ask_block("toolu_free")])),
            user(json!([{
                "type": "tool_result", "tool_use_id": "toolu_free",
                "content": "The user answered: \"What should the placeholder page actually hold?\"=\"All good - just diagnostics for now\". Read the answers carefully."
            }]))
        );
        let s = from_jsonl(&body, NOW)[0].surface.clone().unwrap();
        match &s.kind {
            Kind::Question(q) => assert_eq!(
                q.answer,
                Answered::Typed("All good - just diagnostics for now".into())
            ),
            other => panic!("{}", other.id()),
        }
        assert_eq!(
            s.subtitle(),
            "answered · All good - just diagnostics for now"
        );
    }

    #[test]
    fn an_answer_line_naming_an_option_still_resolves_to_that_option() {
        let body = format!(
            "{}\n{}",
            assistant(json!([ask_block("toolu_opt")])),
            user(json!([{
                "type": "tool_result", "tool_use_id": "toolu_opt",
                "content": "The user answered: \"Page content\"=\"Drawn decision brief\"."
            }]))
        );
        match &from_jsonl(&body, NOW)[0].surface.as_ref().unwrap().kind {
            Kind::Question(q) => assert_eq!(q.answer, Answered::Chose(1)),
            other => panic!("{}", other.id()),
        }
    }

    #[test]
    fn a_result_naming_nothing_is_answered_unknown_rather_than_option_zero() {
        // A person can reply in prose. Recording that as "they chose the first
        // option" would invent a decision nobody made.
        let body = format!(
            "{}\n{}",
            assistant(json!([ask_block("toolu_2")])),
            user(json!([{
                "type": "tool_result", "tool_use_id": "toolu_2",
                "content": [{ "type": "text", "text": "actually do something else entirely" }]
            }]))
        );
        match &from_jsonl(&body, NOW)[0].surface.as_ref().unwrap().kind {
            Kind::Question(q) => assert_eq!(q.answer, Answered::ChoseUnknown),
            other => panic!("{}", other.id()),
        }
    }

    #[test]
    fn a_result_for_a_different_call_leaves_the_question_waiting() {
        let body = format!(
            "{}\n{}",
            assistant(json!([ask_block("toolu_3")])),
            user(
                json!([{ "type": "tool_result", "tool_use_id": "toolu_OTHER",
                          "content": "ok" }])
            )
        );
        match &from_jsonl(&body, NOW)[0].surface.as_ref().unwrap().kind {
            Kind::Question(q) => assert_eq!(q.answer, Answered::Waiting),
            other => panic!("{}", other.id()),
        }
    }

    #[test]
    fn the_same_transcript_derives_the_same_ids_every_time() {
        // Idempotence is the whole reason derived ids come from the tool-use id
        // rather than from a clock: this runs once a second, forever.
        let body = assistant(json!([ask_block("toolu_stable")]));
        let a = from_jsonl(&body, NOW);
        let b = from_jsonl(&body, NOW + 60_000);
        assert_eq!(a[0].id, b[0].id);
        assert!(a[0].id.as_str().starts_with("ask-"));
    }

    #[test]
    fn a_deliverable_line_becomes_an_artifact() {
        let body = assistant(json!([{
            "type": "text",
            "text": "Here is the thing.\n\nDeliverable: The Workbench — file:///home/parker/Work/reports/x.html\n\nmore prose"
        }]));
        let posts = from_jsonl(&body, NOW);
        assert_eq!(posts.len(), 1);
        let s = posts[0].surface.as_ref().unwrap();
        assert_eq!(s.title, "The Workbench");
        match &s.kind {
            Kind::Artifact(a) => {
                assert_eq!(
                    a.href, "/home/parker/Work/reports/x.html",
                    "file:// is stripped"
                );
                assert_eq!(a.mime.as_deref(), Some("text/html"));
            }
            other => panic!("{}", other.id()),
        }
        assert!(s.actions.contains(&crate::surface::Action::Open));
    }

    #[test]
    fn a_bold_deliverable_with_a_hyphen_also_counts() {
        let body = assistant(json!([{
            "type": "text",
            "text": "**Deliverable:** Quick page - /home/parker/Work/reports/quick.html"
        }]));
        let s = from_jsonl(&body, NOW)[0].surface.clone().unwrap();
        assert_eq!(s.title, "Quick page");
        match s.kind {
            Kind::Artifact(a) => assert_eq!(a.href, "/home/parker/Work/reports/quick.html"),
            other => panic!("{}", other.id()),
        }
    }

    #[test]
    fn only_the_most_recent_deliverable_is_kept() {
        let body = format!(
            "{}\n{}",
            assistant(json!([{ "type": "text", "text": "Deliverable: Old — /tmp/a.html" }])),
            assistant(json!([{ "type": "text", "text": "Deliverable: New — /tmp/b.html" }]))
        );
        let posts = from_jsonl(&body, NOW);
        assert_eq!(posts.len(), 1, "five superseded reports are not five rows");
        assert_eq!(posts[0].surface.as_ref().unwrap().title, "New");
    }

    #[test]
    fn a_deliverable_line_with_no_target_is_not_one() {
        let body = assistant(json!([{ "type": "text", "text": "Deliverable: a feeling" }]));
        assert!(from_jsonl(&body, NOW).is_empty());
    }

    #[test]
    fn a_users_own_message_quoting_the_rule_is_not_a_deliverable() {
        // The convention lives in AGENTS.md, which is in every system prompt —
        // so the words "Deliverable:" appear in the transcript far more often
        // than an actual deliverable does. Only assistant prose counts.
        let body = user(json!([{ "type": "text",
            "text": "remember: Deliverable: <label> — file:///path" }]));
        assert!(from_jsonl(&body, NOW).is_empty());
    }

    #[test]
    fn junk_lines_and_partial_json_are_skipped_not_fatal() {
        let body = format!(
            "not json at all\n{{\"type\":\"assis\n{}\n",
            assistant(json!([ask_block("toolu_9")]))
        );
        assert_eq!(from_jsonl(&body, NOW).len(), 1);
    }

    #[test]
    fn an_empty_transcript_derives_nothing() {
        assert!(from_jsonl("", NOW).is_empty());
        assert!(from_jsonl("\n\n", NOW).is_empty());
    }

    #[test]
    fn a_question_with_no_options_is_not_answerable_and_is_skipped() {
        let block = json!({
            "type": "tool_use", "id": "t", "name": "AskUserQuestion",
            "input": { "questions": [{ "question": "open ended?", "options": [] }]}
        });
        assert!(from_jsonl(&assistant(json!([block])), NOW).is_empty());
    }

    #[test]
    fn a_question_and_a_deliverable_in_one_transcript_both_land() {
        let body = format!(
            "{}\n{}",
            assistant(json!([{ "type": "text", "text": "Deliverable: Page — /tmp/p.html" }])),
            assistant(json!([ask_block("toolu_both")]))
        );
        let kinds: Vec<&str> = from_jsonl(&body, NOW)
            .iter()
            .map(|p| p.surface.as_ref().unwrap().kind.id())
            .collect();
        assert!(
            kinds.contains(&"question") && kinds.contains(&"artifact"),
            "{kinds:?}"
        );
    }
}
