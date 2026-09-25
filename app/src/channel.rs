//! The agent channel — how the bench and the agent PROCESS talk with no screen
//! read and no keystroke between them. TDAC 0.1; the contract is
//! `docs/spec/td-agent-channel.md`, and this file is its types, its parsers,
//! its encoders and the per-pane state that turns records into effects.
//!
//! No gpui, no pane, no window. It tests the way `surface.rs` does: against
//! values, and against a scratch directory when a file is the point.
//!
//! # The shape, in one picture
//!
//! ```text
//!   agent process           the pane's mailbox              the bench
//!  ───────────────         ──────────────────              ───────────
//!   a hook fires     →     inbound.jsonl            →      Effect::Present / Asked / Reply
//!   a hook waits     ←     answers/<tool_use_id>.json  ←   a press on a card
//!   its stdin        ←     one bracketed paste      ←      SEND on the composer
//!                          outbound.jsonl           ←      every one of the above, FIRST
//! ```
//!
//! # Why records and not keystrokes
//!
//! The composer used to be a MIRROR of the agent's own line editor: every key
//! went down the pseudoterminal first and the local copy existed only to draw a
//! caret. It inherited the terminal's meaning for every key, which is how
//! `ctrl+c` in a text box ended a person's session (Gate 1 of
//! `docs/plans/workbench-drives-the-agent/`). A question the agent asked was a
//! menu read off the grid one screen at a time, and the answer was arrow keys
//! aimed at a cursor the bench believed it could see.
//!
//! Every exchange is now a typed record with a source. A send is done when the
//! journal has it and the bytes have left; an answer is done when the file
//! exists; whether the agent took either arrives later as another record, or
//! does not, and is drawn as such. Where a record cannot be had — an agent with
//! no hooks, an older window — the bench falls back to the screen and to keys
//! and **says so on the card**.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::surface::{
    Answered, Choice_, Kind, MenuButton, Origin, Question, Round as QRound, Step, Surface,
    SurfaceId, Weight,
};

/// The channel's own version, carried on every record the window writes.
///
/// 0.2 (2026-09-21): `at_ms` on the hook's `waiting` and `released` records,
/// `released.why` gains `closed` and `missing`, outbound records carry
/// `session` and `pane`, and the adapter rotates its journal. All additive.
pub const TDAC_VERSION: &str = "0.2";

/// How often the window refreshes a pane's marker while its bench is OPEN.
/// Closed benches write once, on the change, and never again: a closed face
/// is not a claim that ages, and twenty panes at one hertz was disk churn for
/// a fact that only changes when a person flips a face.
pub const BEACON_MS: u64 = 1_000;

/// How recently the window must have refreshed a pane's marker for a hook to
/// hold a picker on the strength of it — and for how long a hook keeps
/// trusting it between re-reads. Four seconds is four sweeps: a window that
/// dies mid-question releases the picker inside that, rather than at the
/// hook's own ten-minute timeout.
///
/// The LIVE reader of the marker is `scripts/td-agent-hooks`, in bash; this
/// constant and [`marker_holds`] are the same rule in Rust, kept so a test can
/// pin what the window writes against what the hook will accept.
#[cfg(test)]
pub const MARKER_FRESH_MS: u64 = 4_000;

/// How many rounds a pane remembers. A record, not a queue: an answered round
/// stays so a late `answered` from the harness can still find it.
pub const ROUNDS_KEPT: usize = 16;

/// How many sent messages the composer recalls with the up key.
pub const HISTORY_KEPT: usize = 32;

// ---------------------------------------------------------------------------
// what a hook carries
// ---------------------------------------------------------------------------

/// One way a question could be answered, as the harness's tool input spells it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AskedOption {
    pub label: String,
    pub description: Option<String>,
    /// The long form the picker shows beside an option. The part the screen
    /// reader could never see, and kept whole for that reason.
    pub preview: Option<String>,
}

/// One question of a round, verbatim from the tool input.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Asked {
    pub question: String,
    /// The picker's short tab label for this step.
    pub header: Option<String>,
    /// Pick as many as apply, committed by a Submit.
    pub multi: bool,
    pub options: Vec<AskedOption>,
}

impl Asked {
    /// Parse one entry of `tool_input.questions`. `None` when there is no
    /// question text — an option list with nothing asked is not a question.
    pub fn parse(v: &Value) -> Option<Asked> {
        let question = v.get("question")?.as_str()?.trim().to_string();
        if question.is_empty() {
            return None;
        }
        let options = v
            .get("options")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|o| match o {
                        Value::String(s) => Some(AskedOption {
                            label: s.clone(),
                            description: None,
                            preview: None,
                        }),
                        other => Some(AskedOption {
                            label: other.get("label")?.as_str()?.to_string(),
                            description: other
                                .get("description")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            preview: other
                                .get("preview")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                        }),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(Asked {
            question,
            header: v.get("header").and_then(Value::as_str).map(str::to_string),
            multi: v
                .get("multiSelect")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            options,
        })
    }
}

/// One line of `inbound.jsonl`.
///
/// `at_ms` is the WRITER's clock and is an `Option`: a record that did not say
/// when it was written is ordered by its place in the file, not by a zero that
/// would sort it before everything.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Inbound {
    /// The person submitted a turn. `text` is `None` when the harness did not
    /// hand the hook the words — a fact the caption draws as such.
    Prompt {
        at_ms: Option<u64>,
        prompt_id: Option<String>,
        text: Option<String>,
    },
    /// The agent called its question tool. The whole round, before any picker.
    Question {
        at_ms: Option<u64>,
        tool_use_id: String,
        questions: Vec<Asked>,
        deadline_ms: Option<u64>,
    },
    /// The hook is holding the picker and waiting for the bench.
    Waiting { tool_use_id: String, until_ms: u64 },
    /// The hook stopped waiting: `answered`, `timeout`, `stale`, `no-bench`.
    Released { tool_use_id: String, why: String },
    /// The tool returned. `answers` is whatever the harness's result carried,
    /// or `None` when it carried nothing this build could read.
    Answered {
        tool_use_id: String,
        answers: Option<Value>,
    },
    /// The person refused the tool, so it never ran and no answer is coming.
    ///
    /// Read out of the transcript rather than written by the hook — see
    /// [`Ending::Cancelled`] for why the hook is structurally unable to say
    /// this. Parsed here as well so a future adapter that CAN say it needs no
    /// change on this side.
    Cancelled { tool_use_id: String },
    /// The agent's turn ended, with its last message when the harness gave it.
    Reply {
        at_ms: Option<u64>,
        text: Option<String>,
    },
    /// The harness raised a notification.
    Notify {
        at_ms: Option<u64>,
        kind: Option<String>,
        message: Option<String>,
    },
    /// A record this build does not know. Kept and counted, never dropped.
    Unknown { type_name: String },
}

impl Inbound {
    /// Parse one record. `None` for a value that is not an object with a
    /// `type` — which is what a line caught mid-write parses to, and the reason
    /// the reader retries it rather than skipping past it.
    pub fn parse(v: &Value) -> Option<Inbound> {
        let m = v.as_object()?;
        let ty = m.get("type")?.as_str()?;
        let s = |k: &str| m.get(k).and_then(Value::as_str).map(str::to_string);
        let n = |k: &str| m.get(k).and_then(Value::as_u64);
        let malformed = |what: &str| Inbound::Unknown {
            type_name: format!("{ty} (malformed: {what})"),
        };
        Some(match ty {
            "prompt" => Inbound::Prompt {
                at_ms: n("at_ms"),
                prompt_id: s("prompt_id"),
                text: s("text"),
            },
            "question" => {
                let Some(tool_use_id) = s("tool_use_id") else {
                    return Some(malformed("no tool_use_id"));
                };
                let questions: Vec<Asked> = m
                    .get("questions")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Asked::parse).collect())
                    .unwrap_or_default();
                if questions.is_empty() {
                    return Some(malformed("no questions"));
                }
                Inbound::Question {
                    at_ms: n("at_ms"),
                    tool_use_id,
                    questions,
                    deadline_ms: n("deadline_ms"),
                }
            }
            "waiting" => match (s("tool_use_id"), n("until_ms")) {
                (Some(tool_use_id), Some(until_ms)) => Inbound::Waiting {
                    tool_use_id,
                    until_ms,
                },
                _ => malformed("needs tool_use_id and until_ms"),
            },
            "released" => match s("tool_use_id") {
                Some(tool_use_id) => Inbound::Released {
                    tool_use_id,
                    why: s("why").unwrap_or_else(|| "unsaid".into()),
                },
                None => malformed("no tool_use_id"),
            },
            "answered" => match s("tool_use_id") {
                Some(tool_use_id) => Inbound::Answered {
                    tool_use_id,
                    answers: m.get("answers").filter(|v| !v.is_null()).cloned(),
                },
                None => malformed("no tool_use_id"),
            },
            "cancelled" => match s("tool_use_id") {
                Some(tool_use_id) => Inbound::Cancelled { tool_use_id },
                None => malformed("no tool_use_id"),
            },
            "reply" => Inbound::Reply {
                at_ms: n("at_ms"),
                text: s("text"),
            },
            "notify" => Inbound::Notify {
                at_ms: n("at_ms"),
                kind: s("notification_type").or_else(|| s("kind")),
                message: s("message"),
            },
            other => Inbound::Unknown {
                type_name: other.to_string(),
            },
        })
    }

    /// One line of the journal, or `None` for a line that is not JSON yet.
    pub fn parse_line(line: &str) -> Option<Inbound> {
        let value: Value = serde_json::from_str(line.trim()).ok()?;
        Inbound::parse(&value)
    }
}

// ---------------------------------------------------------------------------
// what the window records before it acts
// ---------------------------------------------------------------------------

/// How a `say` left, or did not yet.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Delivery {
    /// A bracketed paste and a return: line breaks survive.
    Paste,
    /// The terminal has no bracketed paste; line breaks became spaces.
    Flat,
    /// Held by the bench's visibility rules; goes when the pane is looked at.
    Held,
}

impl Delivery {
    pub fn id(self) -> &'static str {
        match self {
            Delivery::Paste => "paste",
            Delivery::Flat => "flat",
            Delivery::Held => "held",
        }
    }
}

/// Which road an answer took to the agent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Route {
    /// The hook was waiting: a file, and the picker never paints.
    File,
    /// The picker painted: arrow keys and a return, as before.
    Keys,
    /// Neither: the TDSP §7 line, for the agent to read on its next turn.
    Sentence,
}

impl Route {
    pub fn id(self) -> &'static str {
        match self {
            Route::File => "file",
            Route::Keys => "keys",
            Route::Sentence => "sentence",
        }
    }
}

/// One line of `outbound.jsonl`, written before the thing it describes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Outbound {
    Say {
        id: String,
        text: String,
        delivery: Delivery,
    },
    Answer {
        tool_use_id: String,
        answers: Map<String, Value>,
        route: Route,
    },
    Interrupt,
    End,
    /// Raw bytes into the pseudoterminal — the ONE impure verb, journaled so
    /// the fallback is visible in the record and never silent.
    Keys {
        bytes: Vec<u8>,
        why: String,
    },
}

impl Outbound {
    /// The journal line. `session` and `pane` name the mailbox it was written
    /// in, so a reader that multiplexes many panes — a relay for another
    /// device, say — needs nothing from the path the line was found at.
    pub fn to_json(&self, at_ms: u64, session: &str, pane: u64) -> Value {
        let mut m = Map::new();
        m.insert("td".into(), json!(TDAC_VERSION));
        m.insert("at_ms".into(), json!(at_ms));
        m.insert("session".into(), json!(session));
        m.insert("pane".into(), json!(pane));
        match self {
            Outbound::Say { id, text, delivery } => {
                m.insert("type".into(), json!("say"));
                m.insert("id".into(), json!(id));
                m.insert("text".into(), json!(text));
                m.insert("delivery".into(), json!(delivery.id()));
            }
            Outbound::Answer {
                tool_use_id,
                answers,
                route,
            } => {
                m.insert("type".into(), json!("answer"));
                m.insert("tool_use_id".into(), json!(tool_use_id));
                m.insert("answers".into(), Value::Object(answers.clone()));
                m.insert("route".into(), json!(route.id()));
            }
            Outbound::Interrupt => {
                m.insert("type".into(), json!("interrupt"));
            }
            Outbound::End => {
                m.insert("type".into(), json!("end"));
            }
            Outbound::Keys { bytes, why } => {
                m.insert("type".into(), json!("keys"));
                let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
                m.insert("bytes_hex".into(), json!(hex));
                m.insert("why".into(), json!(why));
            }
        }
        Value::Object(m)
    }
}

/// The bytes that put one MESSAGE in front of the agent, and how they did it.
///
/// One write. With bracketed paste on — every agent TUI this house runs — the
/// text travels inside `ESC[200~ … ESC[201~` so a line break inside it stays a
/// line break and nothing inside it can submit early; the return after the
/// closing bracket is the submit. Without it, line breaks become spaces, which
/// is what the terminal face's own paste does on such a terminal.
pub fn say_bytes(text: &str, bracketed: bool) -> (Vec<u8>, Delivery) {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if bracketed {
        let mut out = b"\x1b[200~".to_vec();
        out.extend_from_slice(text.as_bytes());
        out.extend_from_slice(b"\x1b[201~");
        out.push(b'\r');
        (out, Delivery::Paste)
    } else {
        let mut out = text.replace('\n', " ").into_bytes();
        out.push(b'\r');
        (out, Delivery::Flat)
    }
}

/// What the screen reader has told us about one card's picker, kept together
/// because the two facts are read off the same rows in the same pass.
///
/// The button is one `Option` rather than a position and a word that could
/// disagree: either a row was read, in which case both halves are known, or
/// none was, in which case neither is. Keeping them apart is how a consumer
/// ends up with a position and no idea what pressing it does.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SeenPicker {
    /// Where the highlight sits, in the picker's own up/down order.
    pub at: usize,
    /// The picker's own button row — where it sits, and what it does.
    /// [`None`] when the picker has no such row.
    pub button: Option<(usize, MenuButton)>,
}

impl SeenPicker {
    /// The button's POSITION, for navigation arithmetic, which needs it
    /// whichever word the picker drew there.
    pub fn slot(&self) -> Option<usize> {
        self.button.map(|(at, _)| at)
    }

    /// The button's position, but ONLY if pressing it ends the round.
    ///
    /// The guard for the keys road. A `None` here is either "no button" or
    /// "a button that steps on", and neither is a row a round may be sent by.
    pub fn ending_slot(&self) -> Option<usize> {
        self.button
            .filter(|(_, kind)| *kind == MenuButton::EndsRound)
            .map(|(at, _)| at)
    }
}

/// Which road an answer takes, in the order the roads are tried.
///
/// `waiting_until_ms` and `released` are what the hook said about itself;
/// `cursor_known` is whether the screen reader has supplied a cursor for this
/// question — which it can only do once the picker has painted.
pub fn route_for(
    waiting_until_ms: Option<u64>,
    released: bool,
    now_ms: u64,
    cursor_known: bool,
) -> Route {
    match (waiting_until_ms, released) {
        (Some(until), false) if now_ms < until => Route::File,
        _ if cursor_known => Route::Keys,
        _ => Route::Sentence,
    }
}

// ---------------------------------------------------------------------------
// the marker
// ---------------------------------------------------------------------------

/// Is it time to write the marker again?
///
/// `prev` is the last marker this pane wrote — whether its bench was open,
/// and when. An open bench refreshes every [`BEACON_MS`] so a hook can tell a
/// live window from a dead one; a closed bench writes on the change only,
/// because "terminal" does not go stale — the hook reads the face before it
/// reads the clock.
pub fn beacon_due(prev: Option<(bool, u64)>, open: bool, now_ms: u64) -> bool {
    match prev {
        None => true,
        Some((was, at)) => was != open || (open && now_ms.saturating_sub(at) >= BEACON_MS),
    }
}

/// The window's liveness marker for one pane.
pub fn marker(bench_open: bool, at_ms: u64, window_pid: u32) -> Value {
    json!({
        "td": TDAC_VERSION,
        "face": if bench_open { "workbench" } else { "terminal" },
        "at_ms": at_ms,
        "window": window_pid,
    })
}

/// Does this marker license a hook to hold a picker right now?
///
/// Both halves are required — a bench face, and a clock within
/// [`MARKER_FRESH_MS`] of now in EITHER direction, because a marker from the
/// future is a clock that cannot be trusted rather than one that is very fresh.
/// The hook applies exactly this rule (`fresh()` in `scripts/td-agent-hooks`).
#[cfg(test)]
pub fn marker_holds(v: &Value, now_ms: u64) -> bool {
    let face = v.get("face").and_then(Value::as_str) == Some("workbench");
    let Some(at) = v.get("at_ms").and_then(Value::as_u64) else {
        return false;
    };
    face && now_ms.abs_diff(at) < MARKER_FRESH_MS
}

/// The answer file's own contents.
pub fn answers_json(tool_use_id: &str, answers: &Map<String, Value>) -> Value {
    json!({
        "td": TDAC_VERSION,
        "tool_use_id": tool_use_id,
        "answers": Value::Object(answers.clone()),
    })
}

/// A tool-use id as a file name. The hook applies the same filter, so the two
/// sides name the same file; anything else in the id is dropped rather than
/// escaped, because a name made only of these cannot walk anywhere.
/// The surface id of question `i` of the round `tool_use_id` names.
///
/// **One function, called by every reader of a round.** It used to be two: the
/// channel formatting `ask-hook-<key>-<i>` from the hook's record, and the
/// transcript reader formatting `ask-<last twelve characters>` from the very
/// same tool-use id. Two spellings of one id is how a single question arrived
/// on one bench as two cards that then disagreed about whether it had been
/// answered — and nothing about the ids made it visible, because each was
/// stable and sensible on its own.
pub fn question_surface_id(tool_use_id: &str, i: usize) -> SurfaceId {
    SurfaceId(format!("ask-hook-{}-{i}", file_key(tool_use_id)))
}

pub fn file_key(tool_use_id: &str) -> String {
    tool_use_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .take(96)
        .collect()
}

// ---------------------------------------------------------------------------
// rounds
// ---------------------------------------------------------------------------

/// How a round ended.
///
/// `None` on [`Round::ending`] is *it has not ended*, which is a different
/// fact from ending in nothing — and the one a `bool` could not hold.
///
/// The two endings are not shades of each other. An answered round produced
/// something, even when this build cannot read what; a cancelled round
/// produced nothing and never will. Everything downstream — whether the chips
/// are live, whether the tab badge stays up, whether the row belongs in a
/// review — turns on which of those happened.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ending {
    /// The tool returned. Whatever was chosen is in `picked`.
    Answered,
    /// The person refused the tool, so it never ran.
    ///
    /// **The hook cannot report this.** A rejected tool skips `PostToolUse`
    /// entirely, so the adapter is never called and the journal simply stops
    /// after `released` — measured on pane 11 on 2026-09-21, where a refused
    /// round was still drawing three live cards and holding the tab badge up
    /// six minutes later. It is read out of the transcript instead, where the
    /// refusal arrives as a `tool_result` carrying `is_error`.
    Cancelled,
    /// Over, and nothing still readable says how.
    ///
    /// Inferred rather than reported: the agent finished a turn, which it
    /// cannot do while an `AskUserQuestion` is holding one open. The reason it
    /// exists is that both REPORTED endings can go out of reach. The hook
    /// never writes one for a refusal at all, and the transcript reader sees
    /// only the last 256 KiB of a session — so on a window restart an hour
    /// later the journal replays `question` / `waiting` / `released` with
    /// nothing closing it, and the refusal it would have been closed by has
    /// scrolled out of view.
    ///
    /// Measured 2026-09-21: a round refused at 21:48 came back live on a
    /// window relaunched at 22:43, and Parker answered two of its cards —
    /// *"the old stale questions were still hanging around - I smash clicked
    /// them a bunch"*. The refusal was 5.2 MB behind the reader's window.
    ///
    /// **Not a guess at WHICH ending happened.** The card says it does not
    /// know, because it does not.
    Unknown,
}

/// One `AskUserQuestion` call as the bench tracks it: every question, what has
/// been picked so far, and what the hook has said about itself.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Round {
    pub tool_use_id: String,
    pub at_ms: Option<u64>,
    pub questions: Vec<Asked>,
    /// Per question, the option indexes picked so far. `None` is unanswered;
    /// an empty list is a multi-select nobody has ticked yet, which is a
    /// different fact.
    pub picked: Vec<Option<Vec<usize>>>,
    pub waiting_until_ms: Option<u64>,
    pub released: bool,
    /// How this round ended, once it has. `None` while it is still open.
    pub ending: Option<Ending>,
    /// The bench has already sent its answer, whichever road it took.
    pub sent: bool,
}

impl Round {
    pub fn new(tool_use_id: String, at_ms: Option<u64>, questions: Vec<Asked>) -> Round {
        let picked = vec![None; questions.len()];
        Round {
            tool_use_id,
            at_ms,
            questions,
            picked,
            waiting_until_ms: None,
            released: false,
            ending: None,
            sent: false,
        }
    }

    /// Has this round ended, however it ended.
    pub fn closed(&self) -> bool {
        self.ending.is_some()
    }

    /// The surface id of question `i`. Stable across sweeps, so the harness's
    /// own `answered` lands on the card that asked.
    pub fn surface_id(&self, i: usize) -> SurfaceId {
        question_surface_id(&self.tool_use_id, i)
    }

    /// Every question has an answer committed.
    pub fn complete(&self) -> bool {
        self.picked
            .iter()
            .all(|p| p.as_ref().is_some_and(|v| !v.is_empty()))
    }

    /// Not one question of this round got an answer.
    ///
    /// The opposite end of [`Self::complete`], and a DIFFERENT fact from "not
    /// complete": a round with two of three answered is incomplete and carries
    /// real decisions, while this one carries none at all. Submitting the
    /// first is a person sending what they had an opinion about; submitting
    /// the second is a person declining the whole round, and the bench draws
    /// them differently because they are different acts.
    pub fn nothing_picked(&self) -> bool {
        self.picked
            .iter()
            .all(|p| p.as_ref().is_none_or(|v| v.is_empty()))
    }

    /// The map the harness's own answer channel takes: question text to the
    /// chosen label, several labels joined by `", "` on a multi-select.
    /// `None` until every question is answered — a partial map would answer
    /// questions nobody has looked at with nothing.
    pub fn answers(&self) -> Option<Map<String, Value>> {
        self.complete().then(|| self.answers_so_far())
    }

    /// The same map, however far through the round the person got.
    ///
    /// What SUBMIT ANSWERS sends. [`Self::answers`] refuses to build one until
    /// every question is committed, and that refusal is right for the
    /// automatic road — a round that sends itself the instant it fills up must
    /// not send itself early. It is wrong for a button somebody pressed on
    /// purpose. Parker: *"NO CONFIRMATION if the person FAILED to answer
    /// questions... a blank question is common practice, this will not add
    /// friction"*.
    ///
    /// **An unanswered question is OMITTED, never sent as `""`.** The two are
    /// not the same fact on the other side of the wire: an empty string is an
    /// answer whose content happens to be nothing, and an absent key is a
    /// question nobody answered. The agent reading this map can act on the
    /// second and can only be misled by the first.
    pub fn answers_so_far(&self) -> Map<String, Value> {
        let mut out = Map::new();
        for (q, picks) in self.questions.iter().zip(&self.picked) {
            let labels: Vec<&str> = picks
                .as_ref()
                .map(|v| {
                    v.iter()
                        .filter_map(|i| q.options.get(*i))
                        .map(|o| o.label.as_str())
                        .collect()
                })
                .unwrap_or_default();
            if labels.is_empty() {
                continue;
            }
            out.insert(q.question.clone(), json!(labels.join(", ")));
        }
        out
    }

    /// Every question's answer as one line, for the road where the bench can
    /// only talk to the agent in words.
    ///
    /// A blank is SAID rather than dropped here, and that is not a
    /// contradiction of [`Self::answers_so_far`] — a map has a shape that can
    /// carry absence and a sentence does not, so a question omitted from a
    /// sentence is a question the agent never learns was asked.
    fn spoken_answers(&self) -> String {
        self.questions
            .iter()
            .zip(&self.picked)
            .map(|(q, picks)| {
                let name = q
                    .header
                    .clone()
                    .unwrap_or_else(|| fold(&q.question).chars().take(40).collect::<String>());
                let labels: Vec<&str> = picks
                    .as_ref()
                    .map(|v| {
                        v.iter()
                            .filter_map(|i| q.options.get(*i))
                            .map(|o| o.label.as_str())
                            .collect()
                    })
                    .unwrap_or_default();
                if labels.is_empty() {
                    format!("{name}: left blank")
                } else {
                    format!("{name}: {}", labels.join(", "))
                }
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// The round's steps, for the progress strip on every card in it.
    fn steps(&self) -> Option<QRound> {
        if self.questions.len() < 2 {
            // A round of one reports nothing: a one-segment bar would invent a
            // sequence that does not exist (TDSP §5).
            return None;
        }
        Some(QRound {
            steps: self
                .questions
                .iter()
                .zip(&self.picked)
                .enumerate()
                .map(|(i, (q, p))| Step {
                    label: q
                        .header
                        .clone()
                        .unwrap_or_else(|| q.question.chars().take(12).collect()),
                    done: p.as_ref().is_some_and(|v| !v.is_empty()),
                    // Every question of a hook-carried round IS a card, so
                    // every step of the navigator can be pressed. The screen
                    // path cannot say that and leaves it `None`.
                    id: Some(self.surface_id(i)),
                })
                .collect(),
            submitting: false,
            // Filled per card by [`Self::surfaces`], which knows which question
            // it is drawing. A ROUND has no single current step; a CARD does.
            current: None,
        })
    }

    /// The TDSP `question` surfaces this round draws as, one per question.
    pub fn surfaces(&self, now_ms: u64) -> Vec<Surface> {
        let round = self.steps();
        self.questions
            .iter()
            .enumerate()
            .map(|(i, q)| {
                let picks = self.picked[i].as_deref().unwrap_or(&[]);
                let options: Vec<Choice_> = q
                    .options
                    .iter()
                    .enumerate()
                    .map(|(n, o)| Choice_ {
                        label: o.label.clone(),
                        what_happens: o.description.clone(),
                        checked: q.multi.then_some(picks.contains(&n)),
                    })
                    .collect();
                let answer = match (q.multi, picks) {
                    // NOTHING PICKED, AND THE BENCH SENT ANYWAY — somebody
                    // pressed SUBMIT ANSWERS with this one still blank.
                    //
                    // Read ABOVE every ending, because it is the strongest
                    // thing known about this question and the only one of the
                    // blank states that records a decision. The `answered`
                    // record for the round lands a moment later and would
                    // otherwise overwrite a deliberate blank with "how is
                    // unavailable" — the bench forgetting something it watched
                    // a person do, and replacing it with a shrug.
                    (_, []) if self.sent => {
                        // NOTHING AT ALL IS A REFUSAL, not three separate
                        // decisions to leave a question blank.
                        //
                        // `Skipped` says a person looked at this question and
                        // chose to send without it, which is true of a blank
                        // beside an answer and false of a round where nothing
                        // was picked — there the one decision was *go on
                        // without me*, and drawing it as three considered
                        // omissions invents deliberation that did not happen.
                        // [`Answered::Cancelled`] already means exactly this
                        // and already reads "cancelled · nothing was
                        // answered".
                        //
                        // Read here rather than off `ending`, for the same
                        // reason the whole arm is: the harness's own record
                        // lands a moment later and sets `ending` to
                        // `Answered`, which would overwrite what the bench
                        // watched a person do with a shrug.
                        if self.nothing_picked() {
                            Answered::Cancelled
                        } else {
                            Answered::Skipped
                        }
                    }
                    // Nothing picked. WHY nothing was picked is the whole
                    // reading: a round still open is waiting on a person, a
                    // round that returned answered this question in a shape we
                    // could not read, and a refused round will never answer it
                    // at all. Drawn as one state, the last two invite a press
                    // that cannot reach the agent.
                    (_, []) => match self.ending {
                        Some(Ending::Cancelled) => Answered::Cancelled,
                        Some(Ending::Answered) => Answered::ChoseUnknown,
                        Some(Ending::Unknown) => Answered::Ended,
                        None => Answered::Waiting,
                    },
                    (false, [one]) => Answered::Chose(*one),
                    // TICKS ARE NOT AN ANSWER. On a multi-select the picker
                    // stays on the question while boxes go on and off; the
                    // answer is the Submit below them. Reported as answered
                    // the moment the first box was ticked, the card treated
                    // itself as finished and took its own Submit away — so a
                    // one-question multi-select became unanswerable from the
                    // card that asked it, and the only live Submit left on the
                    // bench was the screen-read twin's. Nothing is lost by
                    // calling it open: the ticks are drawn on the options.
                    (true, _) if !self.sent && self.ending.is_none() => Answered::Waiting,
                    (_, many) => Answered::Typed(
                        many.iter()
                            .filter_map(|n| q.options.get(*n))
                            .map(|o| o.label.clone())
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                };
                let kind = Kind::Question(Question {
                    question: q.question.clone(),
                    options,
                    recommend: None,
                    answer,
                    cursor: None,
                    // A multi-select needs a Submit, and it sits after the
                    // options in the navigation order — the same slot the
                    // picker's own would take. A single choice commits on the
                    // press.
                    submit: q.multi.then_some(q.options.len()),
                    submit_kind: None,
                    // The same steps on every card of the round, each one
                    // knowing which step it is. That is what lets the navigator
                    // mark where you are standing without the renderer having
                    // to work out which surface it is inside.
                    round: round.clone().map(|mut r| {
                        r.current = Some(i);
                        r
                    }),
                });
                let actions = kind.default_actions();
                Surface {
                    id: self.surface_id(i),
                    title: q.question.chars().take(72).collect(),
                    kind,
                    weight: Weight::default(),
                    actions,
                    source: None,
                    arrived_ms: now_ms,
                    origin: Origin::Hook,
                }
            })
            .collect()
    }
}

/// A turn the HARNESS opened, in nobody's voice but its own.
///
/// Claude Code fires `UserPromptSubmit` for turns the person never typed — a
/// background task finishing, a sibling session's message, a scheduled
/// wake-up — and hands the hook the whole envelope as `prompt`. The hook
/// cannot tell those from typing, so the bench drew a tool-use id and a
/// `/tmp` path under the word YOU. Parker, on a pane doing exactly that:
/// *"BUG! I can be certain that I did not type any of this crazy machine
/// talk!"*
///
/// Counted across this box's mailboxes the day it was found: of 126 records
/// typed `prompt`, 59 were task notifications and 10 were a peer session
/// talking. More than half of everything the bench called YOU was written by
/// a machine.
///
/// The envelope is KEPT rather than dropped. Something did open the turn, and
/// a caption that fell back to the person's older words would be the same lie
/// pointing the other way — the reply underneath is real, and it is an answer
/// to this.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Woken {
    /// A background task or command this agent started has finished.
    /// `summary` is the harness's own sentence for it, where it gave one.
    Task { summary: Option<String> },
    /// Another session sent this one a message. `from` is the peer's NAME
    /// where the envelope carried one, never the socket path — an address is
    /// not a who.
    Peer { from: Option<String> },
    /// An envelope this build has no name for. The tag travels, because a
    /// wake-up shape nobody here has met is itself worth drawing: a variant
    /// carrying nothing would be indistinguishable from not having looked.
    Other { tag: String },
}

/// The text inside the first `<name>…</name>` of an envelope, trimmed.
/// `None` when it is absent OR empty — an element that said nothing and an
/// element that was not there are both "the harness gave no sentence".
fn element(t: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let from = t.find(&open)? + open.len();
    let to = t[from..].find(&close)? + from;
    let v = t[from..to].trim();
    (!v.is_empty()).then(|| v.to_string())
}

/// A double-quoted attribute off an envelope's opening tag.
fn attribute(t: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let from = t.find(&key)? + key.len();
    let to = t[from..].find('"')? + from;
    let v = t[from..to].trim();
    (!v.is_empty()).then(|| v.to_string())
}

/// Did the HARNESS open this turn rather than the person? The envelope, if so.
///
/// The whole text must BE one element — it opens with a tag and ends with that
/// tag's close — because a person QUOTING a notification inside a message of
/// their own is still the person talking, and a substring match would eat
/// their words. Everything this rule gets wrong should cost a label, never a
/// sentence.
///
/// Two envelopes are known by name; any other whose tag is KEBAB is taken as
/// one this build has not met. The hyphen is the entire discriminator and it
/// is deliberately crude: every wrapper the harness injects carries one
/// (`task-notification`, `cross-session-message`, `system-reminder`,
/// `local-command-stdout`), and no bare HTML element a person might paste
/// does. A wake-up spelled as a single word is therefore MISSED — which
/// leaves the behaviour that was already shipping instead of swallowing a
/// real message, and missing is the safe direction for a rule about whose
/// words these are.
pub fn woken_by(text: &str) -> Option<Woken> {
    let t = text.trim();
    let rest = t.strip_prefix('<')?;
    let tag: String = rest
        .chars()
        .take_while(|c| c.is_ascii_lowercase() || *c == '-')
        .collect();
    if !tag.contains('-') {
        return None;
    }
    // The tag has to END where it stops being the tag: `>` closes the element,
    // whitespace opens its attributes. Without this, `<task-notifications>`
    // and `<task-notification-v2>` would both read as the name they merely
    // begin with, and the summary drawn would be another envelope's.
    let after = &rest[tag.len()..];
    if !(after.starts_with('>') || after.starts_with(char::is_whitespace)) {
        return None;
    }
    if !t.ends_with(&format!("</{tag}>")) {
        return None;
    }
    Some(match tag.as_str() {
        "task-notification" => Woken::Task {
            summary: element(t, "summary"),
        },
        "cross-session-message" => Woken::Peer {
            from: attribute(t, "from-name").or_else(|| attribute(t, "from")),
        },
        _ => Woken::Other { tag },
    })
}

/// What the bench should do about one inbound record.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Effect {
    /// Caption the overview with the person's exact words.
    Asked { text: String },
    /// The harness opened this turn, not the person. Caption the reply with
    /// what woke it, and leave whatever they last said alone — including the
    /// screen latch, which is a pane's only caption when no real prompt has
    /// come through the channel yet.
    Woken(Woken),
    /// Put these on the bench (present, or re-present with new state).
    Present(Vec<Surface>),
    /// Present the agent's reply as a response, because none arrived itself.
    /// `n` counts this pane's hook replies, so two in one millisecond are two.
    ///
    /// `ended` is every round this same reply proved over — see
    /// [`State::abandon_open_rounds`]. Carried on the reply rather than sent
    /// as its own effect because it IS the same event: one record arrived and
    /// it says two things, and an effect that could only report one of them
    /// would have to drop the other.
    Reply {
        text: String,
        n: u32,
        ended: Vec<Surface>,
    },
    /// Recorded; nothing to draw.
    Nothing,
}

/// What a press on a hook-carried card should do next.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Press {
    /// The round is complete and the hook is waiting: write this map.
    WriteAnswers {
        tool_use_id: String,
        answers: Map<String, Value>,
    },
    /// The picker has painted: drive it with these bytes.
    Keys { bytes: Vec<u8>, note: String },
    /// Say it in a line the agent reads on its next turn.
    Sentence { label: String },
    /// Recorded, more to answer before anything is sent.
    Recorded,
    /// Not a pressable thing.
    Refused(String),
}

/// One pane's channel state.
#[derive(Clone, Debug, Default)]
pub struct State {
    rounds: Vec<Round>,
    /// Records this build could not type. Counted, because a bench that said
    /// nothing about them would be hiding the version skew they signal.
    pub unknown: usize,
    /// The harness in this pane has spoken through the channel at least once.
    /// Decides whether the screen latch may still overwrite the ask caption:
    /// a pane whose hooks deliver exact words does not want a scraped copy.
    pub heard: bool,
    /// `response` surfaces the agent itself presented since the last prompt.
    responses_since_prompt: usize,
    /// Screen cursors the reader has supplied for open hook questions, so the
    /// keys road knows where the picker's highlight is.
    cursors: BTreeMap<SurfaceId, SeenPicker>,
    /// Rounds whose ENDING reached us before their question did — a journal
    /// replayed from an offset, two hooks racing, a transcript read before the
    /// hook's own record landed. The question, when it arrives, is presented
    /// already settled rather than as a live ask.
    ///
    /// A map rather than a set, because *which* ending arrived is the part
    /// that decides how the card draws: a set could say only that the round
    /// was over, which is the distinction this whole slice exists to keep.
    closed_early: BTreeMap<String, Ending>,
    /// How many hook replies this pane has presented, for their ids.
    replies: u32,
}

impl State {
    pub fn new() -> State {
        State::default()
    }

    /// Take one record and say what it means for the bench.
    pub fn take(&mut self, ev: Inbound, now_ms: u64) -> Effect {
        self.heard = true;
        match ev {
            Inbound::Prompt { text, .. } => {
                self.responses_since_prompt = 0;
                match text {
                    // A turn is a turn whoever opened it, so the counter above
                    // resets either way — what changes is whose voice the
                    // caption is drawn in.
                    Some(t) if !t.trim().is_empty() => match woken_by(&t) {
                        Some(w) => Effect::Woken(w),
                        None => Effect::Asked { text: t },
                    },
                    _ => Effect::Nothing,
                }
            }
            Inbound::Question {
                at_ms,
                tool_use_id,
                questions,
                ..
            } => {
                // The same round arriving twice — a hook re-run, a journal
                // re-read — is one round.
                if let Some(r) = self.rounds.iter().find(|r| r.tool_use_id == tool_use_id) {
                    return Effect::Present(r.surfaces(now_ms));
                }
                let mut round = Round::new(tool_use_id, at_ms, questions);
                // Its answer got here first: a settled question, not a live one.
                if let Some(ending) = self.closed_early.remove(&round.tool_use_id) {
                    round.ending = Some(ending);
                }
                let surfaces = round.surfaces(now_ms);
                self.rounds.push(round);
                if self.rounds.len() > ROUNDS_KEPT {
                    // Make room by dropping the oldest SETTLED round. Only when
                    // every round is still open does the oldest open one go —
                    // a bench holding sixteen unanswered questions has a
                    // different problem than this cap can solve.
                    let at = self
                        .rounds
                        .iter()
                        .position(|r| r.closed() || r.sent)
                        .unwrap_or(0);
                    let gone = self.rounds.remove(at);
                    for i in 0..gone.questions.len() {
                        self.cursors.remove(&gone.surface_id(i));
                    }
                }
                Effect::Present(surfaces)
            }
            Inbound::Waiting {
                tool_use_id,
                until_ms,
            } => {
                if let Some(r) = self.round_mut(&tool_use_id) {
                    r.waiting_until_ms = Some(until_ms);
                    r.released = false;
                }
                Effect::Nothing
            }
            Inbound::Released { tool_use_id, .. } => {
                if let Some(r) = self.round_mut(&tool_use_id) {
                    r.released = true;
                }
                Effect::Nothing
            }
            Inbound::Answered {
                tool_use_id,
                answers,
            } => {
                let Some(r) = self.round_mut(&tool_use_id) else {
                    // Answered before asked, as far as this reader has seen.
                    // Remembered, so the question is not presented live when
                    // its record catches up.
                    self.closed_early.insert(tool_use_id, Ending::Answered);
                    return Effect::Nothing;
                };
                // The same transcript tail is re-read every sweep, so this
                // record arrives again and again for as long as the refusal
                // stays in it. The first is the event; the rest are the same
                // fact, and re-presenting on each would repaint every card in
                // the round once a second.
                if r.ending == Some(Ending::Answered) && answers.is_none() {
                    return Effect::Nothing;
                }
                r.ending = Some(Ending::Answered);
                // The harness's own record of what was chosen outranks ours:
                // a person may have answered in the terminal instead.
                if let Some(map) = answers.as_ref().and_then(answers_of) {
                    for (i, q) in r.questions.iter().enumerate() {
                        let Some(chosen) = map.get(&q.question) else {
                            continue;
                        };
                        let picks: Vec<usize> = chosen
                            .split(", ")
                            .filter_map(|label| q.options.iter().position(|o| o.label == label))
                            .collect();
                        if !picks.is_empty() {
                            r.picked[i] = Some(picks);
                        }
                    }
                }
                let out = r.surfaces(now_ms);
                let ids: Vec<SurfaceId> = (0..r.questions.len()).map(|i| r.surface_id(i)).collect();
                for id in ids {
                    self.cursors.remove(&id);
                }
                Effect::Present(out)
            }
            Inbound::Cancelled { tool_use_id } => {
                let Some(r) = self.round_mut(&tool_use_id) else {
                    // Refused before its question reached us. Remembered, so
                    // the question is not presented live when its record
                    // catches up — the transcript is swept on its own clock
                    // and can easily overtake the journal.
                    self.closed_early.insert(tool_use_id, Ending::Cancelled);
                    return Effect::Nothing;
                };
                // Re-read of the same transcript tail, sweep after sweep. The
                // first arrival is the event; every later one is the same fact
                // again, and presenting on each would repaint every card in
                // the round about once a second for as long as the refusal
                // stays in the tail.
                if r.ending == Some(Ending::Cancelled) {
                    return Effect::Nothing;
                }
                r.ending = Some(Ending::Cancelled);
                let out = r.surfaces(now_ms);
                // The picker is gone with the tool that drew it, so a cursor
                // kept for the keys road now points at nothing.
                let ids: Vec<SurfaceId> = (0..r.questions.len()).map(|i| r.surface_id(i)).collect();
                for id in ids {
                    self.cursors.remove(&id);
                }
                Effect::Present(out)
            }
            Inbound::Reply { text, .. } => {
                let ended = self.abandon_open_rounds(now_ms);
                match text {
                    Some(t) if !t.trim().is_empty() && self.responses_since_prompt == 0 => {
                        self.replies += 1;
                        Effect::Reply {
                            text: t,
                            n: self.replies,
                            ended,
                        }
                    }
                    // No reply surface to draw, but the rounds still ended.
                    _ if !ended.is_empty() => Effect::Present(ended),
                    _ => Effect::Nothing,
                }
            }
            Inbound::Notify { .. } => Effect::Nothing,
            Inbound::Unknown { .. } => {
                self.unknown += 1;
                Effect::Nothing
            }
        }
    }

    /// The agent presented a `response` of its own this turn, so a hook reply
    /// would be a second copy of it.
    pub fn saw_response(&mut self) {
        self.responses_since_prompt += 1;
    }

    /// Is a hook-carried question still waiting on a person?
    pub fn has_open_question(&self) -> bool {
        self.rounds
            .iter()
            .any(|r| !r.closed() && !r.sent && !r.complete())
    }

    /// Can this card's round still be sent from the bench?
    ///
    /// What the SUBMIT ANSWERS button is drawn under. It asks about the ROUND
    /// and not about the card, which is the whole point: a round whose first
    /// question is answered is still sendable, and the card somebody happens
    /// to be reading has no idea whether its neighbours are done.
    ///
    /// A card the channel does not carry answers `false` — there is no
    /// `tool_use_id` to write against and nothing accumulating to send, so the
    /// button is absent there rather than present and refusing.
    pub fn submittable(&self, id: &SurfaceId) -> bool {
        self.owns(id)
            .map(|(ri, _)| &self.rounds[ri])
            .is_some_and(|r| !r.closed() && !r.sent)
    }

    /// Which round and question a card belongs to, if it is ours.
    pub fn owns(&self, id: &SurfaceId) -> Option<(usize, usize)> {
        self.rounds.iter().enumerate().find_map(|(ri, r)| {
            (0..r.questions.len())
                .find(|i| r.surface_id(*i) == *id)
                .map(|qi| (ri, qi))
        })
    }

    /// An open hook question with these words, for the screen reader to merge
    /// its cursor into rather than presenting a second card.
    ///
    /// # A SUFFIX, not an equality
    ///
    /// The hook has a paragraph; the screen reader has one ROW.
    /// [`crate::screenread::question_on_screen`] takes the nearest line above
    /// the first option, so on any question the pane is too narrow to draw
    /// whole it holds the LAST visual row and nothing else. The last row of a
    /// wrapped string is a suffix of that string whether the wrap fell on a
    /// space or inside a word, once [`fold`] has collapsed the whitespace —
    /// so a suffix is the strongest test that can actually be made here, and
    /// equality was a test that could only pass on short questions.
    ///
    /// Under equality the fold simply never happened on a long one: a second
    /// card stood beside the first for the life of the round and both drew a
    /// live Submit. Measured on a 254-character question in a pane about sixty
    /// columns wide. Parker: *"I hit submit --- and then it asked AGAIN if I
    /// would like to submit... JUST ONE SUBMIT please!"*
    ///
    /// The caller guarantees the reading is more than eight characters, so
    /// there is no length floor here; a floor no caller can trip is a case
    /// that cannot fail.
    ///
    /// # An ANSWERED question of an open round still matches
    ///
    /// A preference, not a filter, and the difference is the duplicate card
    /// Parker kept meeting: *"STILL REPEATING A QUESTION ON THE RIGHT SPINE
    /// CARDS ... or maybe sometimes not!!!"*
    ///
    /// This used to SKIP any question already picked, on the reasoning that
    /// pressing an option advances the picker, so a reading of a picked
    /// question must be stale. **That reasoning holds on the keys road and
    /// fails on the file road**, which is the road a held picker actually
    /// takes: there the bench types nothing at all — it records the pick and
    /// waits for the round to fill up — so the picker goes on painting the
    /// question it was painting, unchanged, for as long as the round is open.
    /// The next screen sweep read that question, found it picked, called it a
    /// stranger, and minted a second card for it. Timing is the whole reason
    /// it came and went: on the keys road the picker really does move on, and
    /// the twin never appeared.
    ///
    /// So a picked question is now the SECOND choice rather than no choice.
    /// An unpicked one still wins where both match, which keeps the old
    /// behaviour everywhere it was right — the reading most likely belongs to
    /// the question the picker has moved to. Falling back costs nothing a
    /// twin did not cost more of: folding into an answered card sets a cursor
    /// on it and nothing else, and the card's armed-ness is read from
    /// `picked`, never from the screen, so a stale reading cannot re-arm
    /// anything.
    pub fn matching(&self, question_text: &str) -> Option<SurfaceId> {
        let want = fold(question_text);
        if want.is_empty() {
            return None;
        }
        let mut fallback = None;
        for r in self.rounds.iter().filter(|r| !r.closed() && !r.sent) {
            for (i, q) in r.questions.iter().enumerate() {
                if !fold(&q.question).ends_with(&want) {
                    continue;
                }
                if q.multi || r.picked[i].is_none() {
                    return Some(r.surface_id(i));
                }
                fallback.get_or_insert_with(|| r.surface_id(i));
            }
        }
        fallback
    }

    /// Every round still open is over, because the agent has finished a turn.
    ///
    /// `AskUserQuestion` holds the turn open until it returns, so an agent that
    /// replied cannot still be blocked on one. That makes a `reply` record a
    /// proof of ending — and, unlike the two REPORTED endings, a proof that
    /// lives in the pane's own journal, which is replayed whole when a window
    /// restarts. The hook writes nothing at all for a refusal, and the
    /// transcript reader sees only the last 256 KiB of the session; both are
    /// out of reach by the time a window comes back an hour later.
    ///
    /// Falsified before it was shipped, not after: 11 rounds across 9 panes on
    /// this machine, checked for a `reply` landing between a `question` and its
    /// `answered`. **Zero.** The eight answered rounds had none in between, and
    /// the only round this would have ended was the dead one.
    ///
    /// A round with picks already recorded keeps them — the ending only
    /// decides how a question with NOTHING picked draws, so this cannot
    /// overwrite an answer the person gave.
    fn abandon_open_rounds(&mut self, now_ms: u64) -> Vec<Surface> {
        let mut out = Vec::new();
        for r in self.rounds.iter_mut().filter(|r| r.ending.is_none()) {
            r.ending = Some(Ending::Unknown);
            out.extend(r.surfaces(now_ms));
        }
        // The picker went with the turn, so a cursor kept for the keys road
        // now points at nothing.
        for s in &out {
            self.cursors.remove(&s.id);
        }
        out
    }

    /// Read the picker off the screen AT THE MOMENT OF A PRESS, for the card
    /// being pressed, and record its cursor. Returns whether it did.
    ///
    /// The once-a-second sweep is the usual source of a cursor, and a press
    /// that arrived when the sweep had not supplied one fell to
    /// [`Route::Sentence`]: the choice was written down for the agent to read
    /// on its next turn. An agent blocked inside that very picker has no next
    /// turn. Measured 2026-09-25 on pane 102, a two-question round after its
    /// hook was released as stale: `Coffee` went down as keys and the picker
    /// ticked it, then `Dogs` and SUBMIT both went to `actions.jsonl` as
    /// sentences while the terminal sat on `❯ 1. Cats` with Pet unanswered.
    /// The bench said "2 of 2 answered"; the agent never moved.
    ///
    /// Strict for the same reason the sweep is: the screen's question has to
    /// be THIS card's by [`State::matching`], so a picker for some other
    /// question is never driven on this card's behalf.
    pub fn read_picker(&mut self, id: &SurfaceId, rows: &[String]) -> bool {
        let Some(q) = crate::screenread::question_on_screen(rows) else {
            return false;
        };
        let Some(cursor) = q.cursor else {
            return false;
        };
        if self.matching(&q.question).as_ref() != Some(id) {
            return false;
        }
        self.saw_cursor(id, cursor, q.submit, q.submit_kind);
        true
    }

    /// The screen reader saw the picker for this card, and where its highlight is.
    pub fn saw_cursor(
        &mut self,
        id: &SurfaceId,
        cursor: usize,
        submit: Option<usize>,
        submit_kind: Option<MenuButton>,
    ) {
        // Both halves of the button row or neither. A position whose word was
        // not read is not a button this channel will press — see
        // [`SeenPicker::ending_slot`].
        let button = submit.zip(submit_kind);
        self.cursors
            .insert(id.clone(), SeenPicker { at: cursor, button });
    }

    /// The round a card is in, whole, for re-presenting after a change — with
    /// any cursor the screen reader has supplied put back on its card, so a
    /// redraw does not throw away the one thing the keys road needs.
    pub fn round_surfaces(&self, id: &SurfaceId, now_ms: u64) -> Vec<Surface> {
        let Some((ri, _)) = self.owns(id) else {
            return Vec::new();
        };
        let mut out = self.rounds[ri].surfaces(now_ms);
        for s in out.iter_mut() {
            if let (Some(seen), Kind::Question(q)) = (self.cursors.get(&s.id).copied(), &mut s.kind)
            {
                let cursor = seen.at;
                // The cursor only. The card's own Submit slot stays where the
                // round put it — after the options — and the picker's Submit
                // position is kept beside the cursor for the keys road.
                q.cursor = Some(cursor);
            }
        }
        out
    }

    /// A press on option `nav` of a hook-carried card, in NAVIGATION order —
    /// the same index the chips carry, where a multi-select's Submit sits
    /// after the options.
    pub fn press(&mut self, id: &SurfaceId, nav: usize, now_ms: u64) -> Press {
        let Some((ri, qi)) = self.owns(id) else {
            return Press::Refused("not a question this channel carried".into());
        };
        let cursor = self.cursors.get(id).copied();
        let round = &mut self.rounds[ri];
        if round.closed() || round.sent {
            return Press::Refused("this question has already been answered".into());
        }
        let q = &round.questions[qi];
        let submit_slot = q.multi.then_some(q.options.len());
        let is_submit = submit_slot == Some(nav);
        if !is_submit && nav >= q.options.len() {
            return Press::Refused(format!("this question has no option {}", nav + 1));
        }
        let label = if is_submit {
            String::new()
        } else {
            q.options[nav].label.clone()
        };
        // Record first. This is the only claim the card makes — *this was
        // pressed* — and it stays true whichever road the answer takes.
        if q.multi {
            let picks = round.picked[qi].get_or_insert_with(Vec::new);
            if !is_submit {
                if let Some(at) = picks.iter().position(|p| *p == nav) {
                    picks.remove(at);
                } else {
                    picks.push(nav);
                    picks.sort_unstable();
                }
                return Press::Recorded;
            }
            if picks.is_empty() {
                return Press::Refused("tick at least one option before submitting".into());
            }
        } else {
            round.picked[qi] = Some(vec![nav]);
        }
        match route_for(
            round.waiting_until_ms,
            round.released,
            now_ms,
            cursor.is_some(),
        ) {
            // A ROUND WITH A NAVIGATOR IS SENT BY ITS SUBMIT TAB, never by
            // running out of questions.
            //
            // Filling in the last answer used to send the round on the spot,
            // which was the only way it could ever go and is why there was no
            // submit at all. Now that there is one, auto-sending is worse than
            // redundant: it takes the round away at the exact moment a person
            // has finished and might want to look back over it, and it makes
            // the tabs a lie — they invite you to revisit question one, and
            // answering question three had already posted the lot. Parker, on
            // the state that produces: *"Once answered a question, it is
            // locked and I cannot change it - this is wrong."*
            //
            // A round of ONE still commits on the press, because a round of
            // one draws no navigator (`Round::steps` returns `None` below two)
            // and therefore has no submit tab. Leaving it to a button that is
            // not there would make a single question unanswerable, which is
            // the same class of bug pointing the other way.
            Route::File => match round.answers().filter(|_| round.questions.len() < 2) {
                Some(answers) => {
                    round.sent = true;
                    Press::WriteAnswers {
                        tool_use_id: round.tool_use_id.clone(),
                        answers,
                    }
                }
                None => Press::Recorded,
            },
            Route::Keys => {
                // `nav` is the CARD's order — options, then a Submit after
                // them. The picker's order may put its Submit earlier, so the
                // screen's position is what the keys aim at.
                //
                // THE POSITION, not the word. A single press is a step inside
                // the round, and stepping is what the picker's row does under
                // either spelling: `Next` on a middle question, `Submit` on the
                // last one, and a multi-select's ticks commit against both. It
                // is [`Channel::submit`] — ending the whole round — that may
                // only aim at a row which really ends it.
                let seen = cursor.unwrap_or_default();
                let (at, submit) = (seen.at, seen.slot());
                let target = if is_submit {
                    submit.unwrap_or(nav)
                } else {
                    crate::workbench::nav_index(nav, submit)
                };
                let bytes = crate::workbench::menu_keys(target, at);
                self.cursors.remove(id);
                Press::Keys {
                    bytes,
                    note: if is_submit {
                        "submitted".into()
                    } else {
                        format!("chose {label:?}")
                    },
                }
            }
            Route::Sentence => Press::Sentence { label },
        }
    }

    /// SUBMIT ANSWERS: send this card's whole round, however much of it was
    /// answered.
    ///
    /// The round was already the unit — one decision node, one card, one
    /// navigator across its questions — in every way except the one that
    /// mattered, which is that there was no way to END it. A round only ever
    /// went out when its last question was answered, so a person who meant to
    /// leave one blank had no move at all. Parker, on a bench with three
    /// questions on it: *"there is STILL NO WAY TO SUBMIT THE QUESTIONS!!!!"*
    ///
    /// **Nothing is refused for being incomplete, and nothing asks twice.**
    /// A confirmation here would be the friction the button exists to remove —
    /// *"a blank question is common practice, this will not add friction"* —
    /// and the state it would protect is recorded honestly instead: every
    /// question left blank comes back as [`Answered::Skipped`], which says a
    /// person decided rather than that a person has not arrived yet.
    ///
    /// The road is picked exactly as a single press picks it ([`route_for`]),
    /// because the question of who can hear the bench right now does not
    /// change with which button was pressed:
    ///
    /// - **File** — the hook is holding the picker, so the map goes to disk
    ///   and the tool returns it. The road this button was built for.
    /// - **Keys** — no hold, but the picker is painted and the screen reader
    ///   knows where its own button sits. Every answer already went down the
    ///   pseudoterminal as it was made, so on a COMPLETE round that button is
    ///   the round's own Submit and this is the last keystroke. On a partial
    ///   round it is not — see below.
    /// - **Sentence** — nobody is holding anything and there is no picker to
    ///   drive. Say what was chosen in a line the agent reads next turn, with
    ///   the blanks named aloud (see [`Round::spoken_answers`]).
    ///
    /// # A PARTIAL ROUND NEVER GOES BY KEYS
    ///
    /// The picker calls its own button `Submit` on the last question of a
    /// round and `Next` on every other one — [`crate::screenread`] matches all
    /// three spellings and records only the POSITION. So a keystroke aimed at
    /// it from question two of three presses `Next`: the picker advances, the
    /// round does not go out, and the bench has just set the flag that turns
    /// every remaining blank into [`Answered::Skipped`] and takes the chips
    /// away. The agent stays blocked on question three with nothing left on
    /// the bench that can answer it.
    ///
    /// Measured on `cdb64925` before this clause existed: `submit` on a round
    /// with both questions blank returned
    /// `Keys { bytes: [1b 5b 42 × 3, 0d], note: "submitted the round" }` and
    /// both questions came back `Skipped`.
    ///
    /// So the keystroke is used only where it really is the round's ending,
    /// and a partial round falls to words — which the sentence road already
    /// names blanks in, and which cannot half-press anybody's menu.
    pub fn submit(&mut self, id: &SurfaceId, now_ms: u64) -> Press {
        let Some((ri, _)) = self.owns(id) else {
            return Press::Refused("not a question this channel carried".into());
        };
        let cursor = self.cursors.get(id).copied();
        let round = &mut self.rounds[ri];
        if round.closed() || round.sent {
            return Press::Refused("this round has already been answered".into());
        }
        let route = route_for(
            round.waiting_until_ms,
            round.released,
            now_ms,
            cursor.is_some(),
        );
        // SENT IS SET ON EVERY ROAD, unlike a single press — which leaves it
        // alone on the keys road because one answer of several is not the
        // round going out. This IS the round going out, whichever way it
        // travelled, and the flag is what turns the blanks into
        // [`Answered::Skipped`] and stops the card offering its chips again.
        round.sent = true;
        match route {
            Route::File => Press::WriteAnswers {
                tool_use_id: round.tool_use_id.clone(),
                answers: round.answers_so_far(),
            },
            Route::Keys => {
                let seen = cursor.unwrap_or_default();
                let at = seen.at;
                // BOTH, or words — and the second half now ASKS THE PICKER.
                //
                // `ending_slot` is a row the screen reader read the word off:
                // it is `Some` only where the picker drew `Submit`, never where
                // it drew `Next`. Completeness of the bench's own record stays
                // as the other half, because a partial round must not go by
                // keys whatever the picker says.
                //
                // Completeness used to be the WHOLE test, standing in for "the
                // picker is on its last question". The two agree until the hook
                // releases a round: the tool proceeds, the picker paints from
                // question one, the bench still holds every answer, and the
                // proxy said Submit at a row that said Next — pressing it
                // advanced the menu, marked the rest Skipped, and left the
                // agent blocked (#698).
                match seen.ending_slot().filter(|_| round.complete()) {
                    Some(target) => {
                        let bytes = crate::workbench::menu_keys(target, at);
                        self.cursors.remove(id);
                        Press::Keys {
                            bytes,
                            note: "submitted the round".into(),
                        }
                    }
                    // The picker is up and either has no button we can see or
                    // is not on the last question. Guessing a position would
                    // drive somebody's menu to a row nobody chose, so this
                    // falls to words instead.
                    None => Press::Sentence {
                        label: round.spoken_answers(),
                    },
                }
            }
            Route::Sentence => Press::Sentence {
                label: round.spoken_answers(),
            },
        }
    }
}

impl State {
    fn round_mut(&mut self, tool_use_id: &str) -> Option<&mut Round> {
        self.rounds
            .iter_mut()
            .find(|r| r.tool_use_id == tool_use_id)
    }
}

/// The `answers` map out of whatever the harness's result carried: an object
/// of question to label, or an object holding one under `answers`.
fn answers_of(v: &Value) -> Option<BTreeMap<String, String>> {
    let obj = v
        .get("answers")
        .and_then(Value::as_object)
        .or_else(|| v.as_object())?;
    let mut out = BTreeMap::new();
    for (k, v) in obj {
        if let Some(s) = v.as_str() {
            out.insert(k.clone(), s.to_string());
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Question text as the screen reader would see it: one line, one space,
/// lower case, no trailing punctuation the picker might wrap away.
fn fold(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['?', '.', ':', '…'])
        .to_ascii_lowercase()
}

/// A hook-carried reply as a TDSP `response`, when the agent sent none.
///
/// `n` is the pane's own count of these (see [`Effect::Reply`]); with the
/// clock it makes the id unique even when two replies land in one millisecond.
pub fn reply_surface(text: &str, now_ms: u64, n: u32) -> Option<Surface> {
    let title: String = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("reply")
        .chars()
        .take(72)
        .collect();
    let value = json!({
        "td": crate::surface::TDSP_VERSION,
        "kind": "response",
        "id": format!("reply-hook-{now_ms}-{n}"),
        "title": title,
        "model": { "layman": text },
    });
    let mut post = crate::surface::parse_lenient(&value, now_ms, "reply");
    let mut s = post.surface.take()?;
    s.origin = Origin::Hook;
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question_event() -> Value {
        json!({
            "td": "0.1", "type": "question", "at_ms": 1000, "pid": 42,
            "tool_use_id": "toolu_01ABC",
            "questions": [
                {"question": "Which drink?", "header": "Drink", "multiSelect": false,
                 "options": [{"label": "Tea", "description": "hot leaves"},
                             {"label": "Coffee", "description": "hot beans", "preview": "a long preview"}]},
                {"question": "Which sizes?", "header": "Sizes", "multiSelect": true,
                 "options": ["Small", "Large"]}
            ],
            "deadline_ms": 600000
        })
    }

    /// Verbatim off this box's own mailboxes, newlines and all — the record
    /// that put a tool-use id under the word YOU.
    const A_REAL_TASK_NOTIFICATION: &str = "<task-notification>\n<task-id>b8owss7vp</task-id>\n<tool-use-id>toolu_01NfPEhjaXvPWkCWWBhMS7Mn</tool-use-id>\n<output-file>/tmp/claude-1000/-home-parker-Work-terminal-delight/55584eab/tasks/b8owss7vp.output</output-file>\n<status>completed</status>\n<summary>Background command \"Wait for PR 627 CI checks to complete\" completed (exit code 0)</summary>\n</task-notification>";

    /// Also verbatim: a sibling session talking, which arrives at the same
    /// hook by the same road and is equally not the person typing.
    const A_REAL_PEER_MESSAGE: &str = "<cross-session-message from=\"uds:/run/user/1000/cc-socks/2073136.sock\" from-name=\"terminal-delight-05\" from-mode=\"prompting\">\nIf you are the pane in docs/plans/workbench-drives-the-agent — Parker has told me to form the bench-tenancy work into your architecture.\n</cross-session-message>";

    #[test]
    fn a_turn_the_harness_opened_is_never_drawn_as_the_persons_own_words() {
        // The bug, in one assertion. The hook cannot tell a wake-up from
        // typing, so the classifier is the only thing standing between a
        // `/tmp` path and the word YOU.
        assert_eq!(
            woken_by(A_REAL_TASK_NOTIFICATION),
            Some(Woken::Task {
                summary: Some(
                    "Background command \"Wait for PR 627 CI checks to complete\" completed (exit code 0)"
                        .into()
                )
            }),
            "the harness's own sentence is what the block has to draw"
        );
        assert_eq!(
            woken_by(A_REAL_PEER_MESSAGE),
            Some(Woken::Peer {
                // The NAME, not the socket path that sits in front of it in
                // the same tag. An address is not a who.
                from: Some("terminal-delight-05".into())
            })
        );

        // An envelope with no `summary` is still a wake-up. `None` here is the
        // harness having said nothing, which the block draws as such — it is
        // not an excuse to fall back to calling it theirs.
        assert_eq!(
            woken_by("<task-notification><status>completed</status></task-notification>"),
            Some(Woken::Task { summary: None })
        );
        // A shape this build has never met is still not the person. It is
        // carried by NAME so that what arrived can be chased.
        assert_eq!(
            woken_by("<scheduled-wake>at 0600</scheduled-wake>"),
            Some(Woken::Other {
                tag: "scheduled-wake".into()
            })
        );

        // ── and everything that IS them, which must survive untouched ──────
        for theirs in [
            "BUG! I can be certain that I did not type any of this crazy machine talk!",
            "/code-review high 627",
            // The case the whole-element rule exists for: a person QUOTING a
            // notification is a person talking, and a substring match would
            // have eaten this entire message.
            "why did <task-notification> show up under YOU? fix it",
            // Ends with a close tag but does not open with one.
            "here is the envelope I mean: </task-notification>",
            // Opens with one and runs on into their own words.
            "<task-notification>ignore that, I typed this myself",
            // A tag with no hyphen is not a harness envelope — pasted markup
            // stays the person's.
            "<div>hand me the markup for this</div>",
            // A near-miss on the name. It must not read as the tag it merely
            // begins with, or the summary drawn would be another envelope's.
            "<task-notifications>two of them</task-notifications>ish",
            "",
            "   ",
        ] {
            assert_eq!(
                woken_by(theirs),
                None,
                "this is the person talking: {theirs:?}"
            );
        }
    }

    #[test]
    fn a_wake_up_captions_the_reply_without_ever_claiming_the_person_spoke() {
        let mut st = State::new();
        let prompt = |t: &str| Inbound::Prompt {
            at_ms: None,
            prompt_id: None,
            text: Some(t.into()),
        };

        // What the bench used to do with this: caption the overview with it.
        match st.take(prompt(A_REAL_TASK_NOTIFICATION), 1) {
            Effect::Woken(Woken::Task { summary }) => {
                assert!(summary.is_some(), "the harness gave a sentence")
            }
            other => panic!("a task notification is not a person asking: {other:?}"),
        }
        // Their own next turn is theirs again, exactly as before — the
        // classifier must not cost a real prompt its caption.
        assert_eq!(
            st.take(prompt("now merge it"), 2),
            Effect::Asked {
                text: "now merge it".into()
            }
        );
        // A peer session is the other half of the same defect.
        assert!(matches!(
            st.take(prompt(A_REAL_PEER_MESSAGE), 3),
            Effect::Woken(Woken::Peer { .. })
        ));
        // And a turn the harness handed over with no words at all is still
        // nothing to draw, rather than an empty wake-up.
        assert_eq!(
            st.take(
                Inbound::Prompt {
                    at_ms: None,
                    prompt_id: None,
                    text: None
                },
                4
            ),
            Effect::Nothing
        );
    }

    #[test]
    fn every_inbound_type_parses_and_an_unknown_one_is_kept() {
        let p = Inbound::parse(&json!({"type":"prompt","at_ms":5,"prompt_id":"p1","text":"hi"}));
        assert_eq!(
            p,
            Some(Inbound::Prompt {
                at_ms: Some(5),
                prompt_id: Some("p1".into()),
                text: Some("hi".into())
            })
        );
        // A prompt whose words the harness withheld is still a prompt, with
        // `None` where the words would be — not an empty string.
        assert_eq!(
            Inbound::parse(&json!({"type":"prompt"})),
            Some(Inbound::Prompt {
                at_ms: None,
                prompt_id: None,
                text: None
            })
        );
        match Inbound::parse(&question_event()) {
            Some(Inbound::Question {
                tool_use_id,
                questions,
                deadline_ms,
                ..
            }) => {
                assert_eq!(tool_use_id, "toolu_01ABC");
                assert_eq!(questions.len(), 2);
                assert_eq!(
                    questions[0].options[1].preview.as_deref(),
                    Some("a long preview")
                );
                assert!(questions[1].multi);
                assert_eq!(questions[1].options[1].label, "Large");
                assert_eq!(deadline_ms, Some(600000));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            Inbound::parse(&json!({"type":"waiting","tool_use_id":"t","until_ms":9})),
            Some(Inbound::Waiting {
                tool_use_id: "t".into(),
                until_ms: 9
            })
        );
        assert_eq!(
            Inbound::parse(&json!({"type":"released","tool_use_id":"t","why":"stale"})),
            Some(Inbound::Released {
                tool_use_id: "t".into(),
                why: "stale".into()
            })
        );
        assert_eq!(
            Inbound::parse(&json!({"type":"answered","tool_use_id":"t","answers":null})),
            Some(Inbound::Answered {
                tool_use_id: "t".into(),
                answers: None
            })
        );
        assert_eq!(
            Inbound::parse(&json!({"type":"reply","at_ms":1,"text":"done"})),
            Some(Inbound::Reply {
                at_ms: Some(1),
                text: Some("done".into())
            })
        );
        assert_eq!(
            Inbound::parse(
                &json!({"type":"notify","notification_type":"idle_prompt","message":"m"})
            ),
            Some(Inbound::Notify {
                at_ms: None,
                kind: Some("idle_prompt".into()),
                message: Some("m".into())
            })
        );
        assert_eq!(
            Inbound::parse(&json!({"type":"hologram"})),
            Some(Inbound::Unknown {
                type_name: "hologram".into()
            })
        );
        // A question with no id cannot be answered and says so, rather than
        // vanishing.
        assert!(matches!(
            Inbound::parse(&json!({"type":"question","questions":[{"question":"?","options":["a","b"]}]})),
            Some(Inbound::Unknown { type_name }) if type_name.contains("tool_use_id")
        ));
        // Not JSON yet: the reader retries, so the parser says nothing.
        assert_eq!(Inbound::parse_line("{\"type\":\"pro"), None);
        assert_eq!(Inbound::parse(&json!("a string")), None);
    }

    #[test]
    fn a_say_is_one_bracketed_paste_and_a_return_or_flat_when_the_terminal_cannot() {
        let (bytes, how) = say_bytes("first\r\nsecond\nthird", true);
        assert_eq!(how, Delivery::Paste);
        assert_eq!(
            bytes,
            b"\x1b[200~first\nsecond\nthird\x1b[201~\r".to_vec(),
            "line breaks survive inside the brackets, CRLF folded to LF"
        );
        let (flat, how) = say_bytes("first\nsecond", false);
        assert_eq!(how, Delivery::Flat);
        assert_eq!(flat, b"first second\r".to_vec());
        // Exactly one carriage return either way, and it is the last byte.
        for b in [bytes, flat] {
            assert_eq!(b.iter().filter(|c| **c == b'\r').count(), 1);
            assert_eq!(b.last(), Some(&b'\r'));
        }
    }

    #[test]
    fn an_answer_takes_the_first_open_road() {
        // The hook is waiting and its deadline is ahead: a file.
        assert_eq!(route_for(Some(10_000), false, 5_000, true), Route::File);
        assert_eq!(route_for(Some(10_000), false, 5_000, false), Route::File);
        // Released, or past the deadline: keys if the picker painted.
        assert_eq!(route_for(Some(10_000), true, 5_000, true), Route::Keys);
        assert_eq!(route_for(Some(10_000), false, 10_000, true), Route::Keys);
        assert_eq!(route_for(None, false, 5_000, true), Route::Keys);
        // Neither: a sentence.
        assert_eq!(route_for(None, false, 5_000, false), Route::Sentence);
        assert_eq!(route_for(Some(10_000), true, 5_000, false), Route::Sentence);
    }

    #[test]
    fn the_marker_holds_only_on_a_fresh_workbench_face() {
        let m = marker(true, 100_000, 7);
        assert!(marker_holds(&m, 100_000));
        assert!(marker_holds(&m, 100_000 + MARKER_FRESH_MS - 1));
        assert!(!marker_holds(&m, 100_000 + MARKER_FRESH_MS), "stale");
        assert!(
            !marker_holds(&m, 100_000 - MARKER_FRESH_MS),
            "from the future"
        );
        assert!(
            !marker_holds(&marker(false, 100_000, 7), 100_000),
            "terminal face"
        );
        assert!(
            !marker_holds(&json!({"face":"workbench"}), 100_000),
            "no clock"
        );
    }

    #[test]
    fn a_round_becomes_one_card_per_question_with_the_round_drawn_on_each() {
        let Some(Inbound::Question {
            tool_use_id,
            questions,
            ..
        }) = Inbound::parse(&question_event())
        else {
            panic!()
        };
        let round = Round::new(tool_use_id, Some(1000), questions);
        let cards = round.surfaces(5);
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].id.as_str(), "ask-hook-toolu_01ABC-0");
        assert_eq!(cards[1].id.as_str(), "ask-hook-toolu_01ABC-1");
        for c in &cards {
            assert_eq!(c.origin, Origin::Hook);
            let Kind::Question(q) = &c.kind else { panic!() };
            assert_eq!(q.answer, Answered::Waiting);
            assert_eq!(q.cursor, None, "no picker to drive");
            let r = q.round.as_ref().expect("two questions make a round");
            assert_eq!(r.total(), 2);
            assert_eq!(r.answered(), 0);
            assert_eq!(r.steps[0].label, "Drink");
        }
        let Kind::Question(single) = &cards[0].kind else {
            panic!()
        };
        assert_eq!(single.submit, None, "a single choice commits on the press");
        assert_eq!(single.options[0].checked, None, "not a checkbox");
        let Kind::Question(multi) = &cards[1].kind else {
            panic!()
        };
        assert_eq!(multi.submit, Some(2), "Submit sits after the two options");
        assert_eq!(multi.options[0].checked, Some(false));
        // A round of one draws no progress bar.
        let lone = Round::new("t".into(), None, vec![round.questions[0].clone()]);
        let Kind::Question(q) = &lone.surfaces(1)[0].kind else {
            panic!()
        };
        assert!(q.round.is_none());
    }

    #[test]
    fn every_card_of_a_round_knows_which_step_it_is_and_how_to_reach_the_others() {
        // The navigator is drawn on EVERY card of the round, so each copy has
        // to say two different things: the same list of steps, and a different
        // "you are here". A shared `Round` with one `current` would mark the
        // same step on all of them, which is the bug this asserts against.
        let Some(Inbound::Question {
            tool_use_id,
            questions,
            ..
        }) = Inbound::parse(&question_event())
        else {
            panic!()
        };
        let round = Round::new(tool_use_id, Some(1000), questions);
        let cards = round.surfaces(5);
        for (i, c) in cards.iter().enumerate() {
            let Kind::Question(q) = &c.kind else { panic!() };
            let r = q.round.as_ref().expect("two questions make a round");
            assert_eq!(r.current, Some(i), "card {i} should know it is step {i}");
            // Every step reaches a real card, and the ids are the ones the
            // bench will look those cards up by.
            for (n, step) in r.steps.iter().enumerate() {
                assert_eq!(
                    step.id.as_ref(),
                    Some(&round.surface_id(n)),
                    "step {n} on card {i} must point at question {n}'s card"
                );
                assert!(
                    cards.iter().any(|c| c.id == *step.id.as_ref().unwrap()),
                    "step {n} points at a card that is not in the round"
                );
            }
        }
    }

    #[test]
    fn answering_a_step_marks_it_done_and_names_the_next_one_still_open() {
        // The whole auto-advance move, at the level that decides it: answer one
        // question and the round must say which one is owed next. Before the
        // channel this could not be asked at all — the bench only ever held the
        // question the picker was painting.
        let Some(Inbound::Question {
            tool_use_id,
            questions,
            ..
        }) = Inbound::parse(&question_event())
        else {
            panic!()
        };
        let mut st = State::new();
        st.take(
            Inbound::Question {
                tool_use_id: tool_use_id.clone(),
                questions,
                at_ms: Some(1000),
                deadline_ms: None,
            },
            1_000,
        );
        // The hook has to be HOLDING the picker for a press to be recorded
        // rather than said in a sentence. Without this the press takes the
        // sentence road, which is a different feature and answers the whole
        // round in one line — it would have passed a weaker assertion.
        st.take(
            Inbound::Waiting {
                tool_use_id: tool_use_id.clone(),
                until_ms: 600_000,
            },
            1_001,
        );
        let first = SurfaceId(format!("ask-hook-{}-0", file_key(&tool_use_id)));
        assert_eq!(st.press(&first, 0, 2_000), Press::Recorded);
        let cards = st.round_surfaces(&first, 3_000);
        let Kind::Question(q) = &cards[0].kind else {
            panic!()
        };
        let r = q.round.as_ref().expect("a round of two");
        assert!(r.steps[0].done, "the step just answered is done");
        assert!(!r.steps[1].done, "the other one is still owed");
        assert_eq!(
            r.next_open(0),
            Some(1),
            "answering step 0 should send a person to step 1"
        );
        // And the far side of the wrap: with only step 0 open, standing on the
        // last step still finds it rather than walking off the end.
        let back = crate::surface::Round {
            steps: vec![
                crate::surface::Step {
                    label: "a".into(),
                    done: false,
                    id: None,
                },
                crate::surface::Step {
                    label: "b".into(),
                    done: true,
                    id: None,
                },
            ],
            submitting: false,
            current: Some(1),
        };
        assert_eq!(back.next_open(1), Some(0), "the search wraps");
        let all_done = crate::surface::Round {
            steps: vec![crate::surface::Step {
                label: "a".into(),
                done: true,
                id: None,
            }],
            submitting: false,
            current: Some(0),
        };
        assert_eq!(
            all_done.next_open(0),
            None,
            "a finished round moves nobody anywhere"
        );
    }

    /// The round Parker refused on 2026-09-21, and what has to become of it.
    ///
    /// A rejected tool never runs, so `PostToolUse` never fires and the hook
    /// writes nothing at all — the journal for that pane simply stops after
    /// `released`. The round therefore stayed open forever: three cards still
    /// offering chips that could no longer reach the agent, and a tab badge
    /// that could not go down. The transcript is the only reader that sees the
    /// refusal, and this is what it has to be able to say.
    #[test]
    fn a_refused_round_ends_and_its_cards_stop_asking() {
        let mut st = State::new();
        let Effect::Present(open) = st.take(Inbound::parse(&question_event()).unwrap(), 10) else {
            panic!("a round presents one card per question")
        };
        assert_eq!(open.len(), 2);
        assert!(st.has_open_question(), "asked, and nobody has answered");

        let Effect::Present(settled) = st.take(
            Inbound::Cancelled {
                tool_use_id: "toolu_01ABC".into(),
            },
            20,
        ) else {
            panic!("a refusal settles the round it names")
        };

        assert!(
            !st.has_open_question(),
            "the badge predicate: nothing waits on a person once the round is refused"
        );
        assert_eq!(
            settled.iter().map(|c| c.id.clone()).collect::<Vec<_>>(),
            open.iter().map(|c| c.id.clone()).collect::<Vec<_>>(),
            "the SAME cards settle in place; a second set would be the duplicate again"
        );
        for c in &settled {
            let Kind::Question(q) = &c.kind else { panic!() };
            assert_eq!(
                q.answer,
                Answered::Cancelled,
                "a refused question is cancelled — not still waiting, and not answered"
            );
        }
    }

    /// The transcript tail is re-read on every sweep, so the refusal in it
    /// arrives again and again. It is one event.
    #[test]
    fn an_ending_read_again_next_sweep_presents_nothing() {
        let cancel = || Inbound::Cancelled {
            tool_use_id: "toolu_01ABC".into(),
        };
        let mut st = State::new();
        st.take(Inbound::parse(&question_event()).unwrap(), 10);
        assert!(matches!(st.take(cancel(), 20), Effect::Present(_)));
        assert!(
            matches!(st.take(cancel(), 21), Effect::Nothing),
            "the same refusal is the same fact, not a second one"
        );
        assert!(matches!(st.take(cancel(), 22), Effect::Nothing));

        let answered = || Inbound::Answered {
            tool_use_id: "toolu_01ABC".into(),
            answers: None,
        };
        let mut st = State::new();
        st.take(Inbound::parse(&question_event()).unwrap(), 10);
        assert!(matches!(st.take(answered(), 20), Effect::Present(_)));
        assert!(
            matches!(st.take(answered(), 21), Effect::Nothing),
            "re-reporting an ending is not a second ending — without this every card \
             in the round repaints about once a second for as long as it is in the tail"
        );
    }

    /// The transcript is swept on its own clock and can overtake the journal.
    #[test]
    fn a_refusal_that_arrives_before_its_question_settles_it_on_arrival() {
        let mut st = State::new();
        assert!(matches!(
            st.take(
                Inbound::Cancelled {
                    tool_use_id: "toolu_01ABC".into()
                },
                5
            ),
            Effect::Nothing
        ));
        let Effect::Present(cards) = st.take(Inbound::parse(&question_event()).unwrap(), 10) else {
            panic!()
        };
        assert!(!st.has_open_question(), "it was over before we heard of it");
        for c in &cards {
            let Kind::Question(q) = &c.kind else { panic!() };
            assert_eq!(
                q.answer,
                Answered::Cancelled,
                "WHICH ending arrived early is the part that decides how the card draws"
            );
        }
    }

    /// Exactly what a window restart replays for the round Parker refused.
    ///
    /// `question` / `waiting` / `released`, and nothing closing it — a rejected
    /// tool never fires `PostToolUse`, so the hook writes no ending, and the
    /// refusal that would have closed it lives only in the transcript, 5.2 MB
    /// behind the 256 KiB the reader looks at. Before this, the round came back
    /// live with pressable cards an hour after it died, and he pressed them.
    #[test]
    fn a_round_the_agent_walked_away_from_is_over_even_when_nothing_says_how() {
        let mut st = State::new();
        st.take(Inbound::parse(&question_event()).unwrap(), 10);
        st.take(
            Inbound::Released {
                tool_use_id: "toolu_01ABC".into(),
                why: "stale".into(),
            },
            11,
        );
        assert!(
            st.has_open_question(),
            "so far this is indistinguishable from a question still being asked"
        );

        // The agent finished a turn. It cannot do that while an
        // AskUserQuestion is holding one open, so the round is over.
        let Effect::Reply { ended, .. } = st.take(
            Inbound::Reply {
                at_ms: None,
                text: Some("here is what I found".into()),
            },
            12,
        ) else {
            panic!("a reply with words presents a reply")
        };

        assert!(!st.has_open_question(), "the badge lets go");
        assert_eq!(
            ended.len(),
            2,
            "every card of the round, not just the first"
        );
        for c in &ended {
            let Kind::Question(q) = &c.kind else { panic!() };
            assert_eq!(
                q.answer,
                Answered::Ended,
                "it says it does not know how it ended, rather than guessing \
                 answered or cancelled"
            );
        }
        assert!(
            matches!(
                st.press(&SurfaceId("ask-hook-toolu_01ABC-0".into()), 0, 13),
                Press::Refused(_)
            ),
            "and the cards stop taking presses"
        );
    }

    /// The ending decides only how an UNANSWERED question draws.
    #[test]
    fn walking_away_does_not_overwrite_an_answer_already_given() {
        let first = SurfaceId("ask-hook-toolu_01ABC-0".into());
        let mut st = State::new();
        st.take(Inbound::parse(&question_event()).unwrap(), 10);
        st.press(&first, 1, 11);
        st.take(
            Inbound::Reply {
                at_ms: None,
                text: Some("done".into()),
            },
            12,
        );
        let Kind::Question(q) = &st.round_surfaces(&first, 13)[0].kind else {
            panic!()
        };
        assert_eq!(
            q.answer,
            Answered::Chose(1),
            "a press the person made outranks an ending nobody recorded"
        );
    }

    /// A reply with nothing to draw still ends what it proves over.
    #[test]
    fn a_reply_the_agent_already_presented_still_ends_the_round() {
        let mut st = State::new();
        st.take(Inbound::parse(&question_event()).unwrap(), 10);
        // The agent sent its own `response` surface, so the hook's copy of the
        // same turn is not wanted — but the turn still ENDED.
        st.saw_response();
        let Effect::Present(ended) = st.take(
            Inbound::Reply {
                at_ms: None,
                text: Some("done".into()),
            },
            11,
        ) else {
            panic!("no reply surface, but the rounds it ended still have to be drawn")
        };
        assert_eq!(ended.len(), 2);
        assert!(!st.has_open_question());
    }

    /// The predicate the tab badge and the keystroke edge both consult.
    ///
    /// [`TerminalView::ack_needs_input`] asks this before a keystroke is
    /// allowed to drop the needs-you flag, so this test is the specification
    /// of Parker's *"it should PERSIST WITHOUT BLINKING ON KEYSTROKE.. until
    /// the question SET is actually submitted"* — the whole of it, in the one
    /// place it can be executed.
    #[test]
    fn the_badge_predicate_holds_from_the_question_to_its_ending() {
        let first = SurfaceId("ask-hook-toolu_01ABC-0".into());

        let mut st = State::new();
        assert!(!st.has_open_question(), "nothing has been asked");
        st.take(Inbound::parse(&question_event()).unwrap(), 10);
        assert!(st.has_open_question(), "asked");
        st.press(&first, 0, 11);
        assert!(
            st.has_open_question(),
            "one of two answered is not a SET the person is done with"
        );
        st.take(
            Inbound::Cancelled {
                tool_use_id: "toolu_01ABC".into(),
            },
            12,
        );
        assert!(!st.has_open_question(), "refused counts as ended");

        let mut st = State::new();
        st.take(Inbound::parse(&question_event()).unwrap(), 10);
        st.take(
            Inbound::Answered {
                tool_use_id: "toolu_01ABC".into(),
                answers: None,
            },
            11,
        );
        assert!(!st.has_open_question(), "answered counts as ended");
    }

    #[test]
    fn a_complete_round_answers_with_labels_keyed_by_question_text() {
        let Some(Inbound::Question {
            tool_use_id,
            questions,
            ..
        }) = Inbound::parse(&question_event())
        else {
            panic!()
        };
        let mut round = Round::new(tool_use_id, None, questions);
        assert_eq!(round.answers(), None, "nothing answered yet");
        round.picked[0] = Some(vec![1]);
        assert_eq!(round.answers(), None, "the second question is open");
        round.picked[1] = Some(vec![]);
        assert_eq!(
            round.answers(),
            None,
            "an unticked multi-select is not an answer"
        );
        round.picked[1] = Some(vec![0, 1]);
        let map = round.answers().expect("complete");
        assert_eq!(map.get("Which drink?"), Some(&json!("Coffee")));
        assert_eq!(map.get("Which sizes?"), Some(&json!("Small, Large")));
        assert_eq!(
            answers_json("toolu_01ABC", &map)["answers"]["Which drink?"],
            json!("Coffee")
        );
    }

    #[test]
    fn a_press_records_first_then_takes_the_road_the_hook_left_open() {
        let mut st = State::new();
        let ev = Inbound::parse(&question_event()).unwrap();
        let Effect::Present(cards) = st.take(ev, 1_000) else {
            panic!()
        };
        assert_eq!(cards.len(), 2);
        assert!(st.has_open_question());
        let single = cards[0].id.clone();
        let multi = cards[1].id.clone();
        // The hook is holding the picker.
        st.take(
            Inbound::Waiting {
                tool_use_id: "toolu_01ABC".into(),
                until_ms: 600_000,
            },
            1_001,
        );
        // Picking on the first card records and waits for the rest.
        assert_eq!(st.press(&single, 1, 2_000), Press::Recorded);
        // Ticking two boxes records each; submitting the second completes the
        // round and the answer goes as a FILE.
        assert_eq!(st.press(&multi, 0, 2_001), Press::Recorded);
        assert_eq!(st.press(&multi, 1, 2_002), Press::Recorded);
        // FILLING IN THE LAST ANSWER NO LONGER SENDS. This used to be a
        // `WriteAnswers`, and that was the only way a round could go — which
        // is why there was no submit at all. A round with a navigator now has
        // a SUBMIT tab, and posting the round the instant its last box was
        // ticked made the other tabs a lie: they invite a person back to an
        // earlier question and the round had already gone.
        assert_eq!(st.press(&multi, 2, 2_003), Press::Recorded);
        assert!(st.submittable(&single), "and it is waiting to be sent");
        match st.submit(&single, 2_004) {
            Press::WriteAnswers {
                tool_use_id,
                answers,
            } => {
                assert_eq!(tool_use_id, "toolu_01ABC");
                assert_eq!(answers["Which drink?"], json!("Coffee"));
                assert_eq!(answers["Which sizes?"], json!("Small, Large"));
            }
            other => panic!("{other:?}"),
        }
        assert!(!st.has_open_question());
        // Pressing again does nothing: the answer has gone.
        assert!(matches!(st.press(&single, 0, 3_000), Press::Refused(_)));
        // The cards now draw as answered.
        let redrawn = st.round_surfaces(&single, 4_000);
        let Kind::Question(q) = &redrawn[0].kind else {
            panic!()
        };
        assert_eq!(q.answer, Answered::Chose(1));
        let Kind::Question(q) = &redrawn[1].kind else {
            panic!()
        };
        assert_eq!(q.answer, Answered::Typed("Small, Large".into()));
        assert_eq!(q.options[0].checked, Some(true));
    }

    #[test]
    fn once_the_picker_has_painted_a_press_drives_it_with_keys() {
        let mut st = State::new();
        let ev = Inbound::parse(&question_event()).unwrap();
        let Effect::Present(cards) = st.take(ev, 1_000) else {
            panic!()
        };
        let single = cards[0].id.clone();
        // The hook gave up (no bench was open) and the picker painted; the
        // screen reader found the same question with its cursor on row 0.
        st.take(
            Inbound::Released {
                tool_use_id: "toolu_01ABC".into(),
                why: "no-bench".into(),
            },
            1_001,
        );
        assert_eq!(st.matching("Which drink?  "), Some(single.clone()));
        assert_eq!(st.matching("which drink"), Some(single.clone()), "folded");
        assert_eq!(st.matching("Which size?"), None);
        st.saw_cursor(&single, 0, None, None);
        match st.press(&single, 1, 2_000) {
            Press::Keys { bytes, note } => {
                assert_eq!(bytes, crate::workbench::menu_keys(1, 0));
                assert_eq!(note, "chose \"Coffee\"");
            }
            other => panic!("{other:?}"),
        }
        // ANSWERED, AND STILL MATCHED — which is the change that killed the
        // duplicate card, and it reads as a reversal until you ask what
        // matching DOES. It hands the screen reader a card to fold its cursor
        // into; the alternative is not "no card", it is a second card for a
        // question that already has one. This assertion used to read `None`,
        // and that `None` is what the twin came out of.
        //
        // Folding into an answered card changes nothing about it: the chips
        // are armed from `picked`, never from the screen.
        assert_eq!(st.matching("Which drink?"), Some(single.clone()));
        // The PREFERENCE is intact, which is the half worth protecting: the
        // round's other question is untouched and still matches on its own.
        assert_eq!(st.matching("Which sizes?"), Some(cards[1].id.clone()));
        // With neither a hook nor a picker, the answer is a sentence.
        let multi = cards[1].id.clone();
        assert_eq!(st.press(&multi, 0, 2_001), Press::Recorded);
        assert_eq!(
            st.press(&multi, 2, 2_002),
            Press::Sentence {
                label: String::new()
            }
        );
    }

    /// SUBMIT ANSWERS sends a round nobody finished, and says so afterwards.
    ///
    /// The button exists for this shape and only this shape — a person who
    /// answered what they had an opinion about and wants the agent to get on
    /// with it. Parker: *"a blank question is common practice, this will not
    /// add friction"*.
    ///
    /// Three things are asserted because each could be wrong on its own: what
    /// goes out, what is left behind, and what a second press does.
    #[test]
    fn submitting_a_half_answered_round_sends_what_there_is_and_marks_the_rest() {
        let mut st = State::new();
        let ev = Inbound::parse(&question_event()).unwrap();
        let Effect::Present(cards) = st.take(ev, 1_000) else {
            panic!()
        };
        let first = cards[0].id.clone();
        st.take(
            Inbound::Waiting {
                tool_use_id: "toolu_01ABC".into(),
                until_ms: 600_000,
            },
            1_001,
        );
        // One of two answered, so the round would never have gone out on its
        // own — which is the whole state the button was missing for.
        assert_eq!(st.press(&first, 1, 1_500), Press::Recorded);

        match st.submit(&first, 1_600) {
            Press::WriteAnswers { answers, .. } => {
                assert_eq!(
                    answers.get("Which drink?").and_then(Value::as_str),
                    Some("Coffee")
                );
                // OMITTED, not empty. An absent key is a question nobody
                // answered; `""` is an answer whose content is nothing. The
                // agent reading this map can act on the first and can only be
                // misled by the second.
                assert!(
                    !answers.contains_key("Which sizes?"),
                    "the blank question was sent as a value: {answers:?}"
                );
            }
            other => panic!("the hook is holding, so this is the file road: {other:?}"),
        }

        // WHAT IS LEFT BEHIND. Nobody is coming back to the blank one, and
        // saying so is the difference between a card that records what
        // happened and one that goes on offering chips that reach nothing.
        let after = st.round_surfaces(&first, 1_700);
        let Kind::Question(blank) = &after[1].kind else {
            panic!()
        };
        assert_eq!(blank.answer, Answered::Skipped);
        let Kind::Question(given) = &after[0].kind else {
            panic!()
        };
        assert_eq!(
            given.answer,
            Answered::Chose(1),
            "and the answer that was given still stands"
        );

        // AND IT DOES NOT GO TWICE.
        assert!(!st.submittable(&first));
        assert!(matches!(st.submit(&first, 1_800), Press::Refused(_)));
    }

    /// THE DUPLICATE CARD, in the state that produced it.
    ///
    /// A hook holding the picker takes the FILE road, and the file road types
    /// nothing: a press is recorded and the round waits to fill up. So the
    /// picker goes on painting the question it was painting — the one the
    /// bench has just answered — and the screen reader keeps reading it.
    ///
    /// Under the old rule that reading matched nothing, because the question
    /// was picked, so the pane minted a second card for it and the rail grew
    /// a twin. Parker: *"STILL REPEATING A QUESTION ON THE RIGHT SPINE CARDS
    /// ... or maybe sometimes not!!!"* — the *sometimes* is the two roads. On
    /// the keys road the picker really does advance, the next reading is of a
    /// different question, and no twin ever appeared.
    ///
    /// Asserted about `matching` rather than about the pane because this is
    /// the decision: a reading that finds a card folds into it, and only a
    /// reading that finds NOTHING becomes a new surface.
    #[test]
    fn a_question_answered_on_the_file_road_is_still_found_by_the_screen() {
        let mut st = State::new();
        let ev = Inbound::parse(&question_event()).unwrap();
        let Effect::Present(cards) = st.take(ev, 1_000) else {
            panic!()
        };
        let first = cards[0].id.clone();
        // The hook is holding the picker, which is what makes this the file
        // road — and the file road is the one that types nothing.
        st.take(
            Inbound::Waiting {
                tool_use_id: "toolu_01ABC".into(),
                until_ms: 600_000,
            },
            1_001,
        );
        assert_eq!(st.press(&first, 1, 1_500), Press::Recorded, "nothing typed");
        assert_eq!(
            st.matching("Which drink?"),
            Some(first),
            "the picker is still painting it, and the reading must land on the \
             card that already answered it rather than making another"
        );
    }

    #[test]
    fn the_harnesss_own_answer_closes_the_round_and_names_what_was_chosen() {
        let mut st = State::new();
        st.take(Inbound::parse(&question_event()).unwrap(), 1_000);
        let ef = st.take(
            Inbound::Answered {
                tool_use_id: "toolu_01ABC".into(),
                answers: Some(json!({"answers": {"Which drink?": "Tea", "Which sizes?": "Large"}})),
            },
            2_000,
        );
        let Effect::Present(cards) = ef else { panic!() };
        let Kind::Question(q) = &cards[0].kind else {
            panic!()
        };
        assert_eq!(q.answer, Answered::Chose(0));
        let Kind::Question(q) = &cards[1].kind else {
            panic!()
        };
        assert_eq!(q.answer, Answered::Typed("Large".into()));
        assert!(!st.has_open_question());
        // A result this build cannot read still closes the round; the cards
        // stay as they were, which is the honest reading.
        let mut st2 = State::new();
        st2.take(Inbound::parse(&question_event()).unwrap(), 1_000);
        let ef = st2.take(
            Inbound::Answered {
                tool_use_id: "toolu_01ABC".into(),
                answers: Some(json!("some string")),
            },
            2_000,
        );
        assert!(matches!(ef, Effect::Present(_)));
        assert!(!st2.has_open_question());
    }

    #[test]
    fn a_reply_is_presented_only_when_the_agent_presented_nothing_itself() {
        let mut st = State::new();
        st.take(
            Inbound::Prompt {
                at_ms: None,
                prompt_id: None,
                text: Some("do it".into()),
            },
            1,
        );
        st.saw_response();
        assert_eq!(
            st.take(
                Inbound::Reply {
                    at_ms: None,
                    text: Some("done".into())
                },
                2
            ),
            Effect::Nothing,
            "the agent's own response is already on the overview"
        );
        // The next prompt opens a new turn with nothing presented yet.
        assert_eq!(
            st.take(
                Inbound::Prompt {
                    at_ms: None,
                    prompt_id: None,
                    text: Some("again".into())
                },
                3
            ),
            Effect::Asked {
                text: "again".into()
            }
        );
        assert_eq!(
            st.take(
                Inbound::Reply {
                    at_ms: None,
                    text: Some("done again".into())
                },
                4
            ),
            Effect::Reply {
                text: "done again".into(),
                n: 1,
                // No rounds were open, so this reply ended nothing.
                ended: Vec::new()
            }
        );
        let s = reply_surface("done again\nwith detail", 4, 1).expect("a response");
        assert_eq!(s.origin, Origin::Hook);
        assert_eq!(s.title, "done again");
        assert!(matches!(s.kind, Kind::Response(_)));
        // A reply the harness did not hand over is nothing, not an empty card.
        assert_eq!(
            st.take(
                Inbound::Reply {
                    at_ms: None,
                    text: None
                },
                5
            ),
            Effect::Nothing
        );
    }

    #[test]
    fn unknown_records_are_counted_and_the_pane_knows_it_has_been_heard_from() {
        let mut st = State::new();
        assert!(!st.heard);
        st.take(
            Inbound::Unknown {
                type_name: "hologram".into(),
            },
            1,
        );
        assert_eq!(st.unknown, 1);
        assert!(st.heard);
    }

    #[test]
    fn an_outbound_record_carries_its_road_and_the_keys_road_is_never_silent() {
        let say = Outbound::Say {
            id: "say-1".into(),
            text: "hi".into(),
            delivery: Delivery::Paste,
        }
        .to_json(9, "attention", 22);
        assert_eq!(say["type"], "say");
        assert_eq!(say["delivery"], "paste");
        assert_eq!(say["at_ms"], 9);
        // The mailbox it was written in travels on the line, so a reader that
        // multiplexes panes needs nothing from the path it found it at.
        assert_eq!(say["session"], "attention");
        assert_eq!(say["pane"], 22);
        assert_eq!(say["td"], TDAC_VERSION);
        let keys = Outbound::Keys {
            bytes: vec![0x1b, b'[', b'B', b'\r'],
            why: "picker painted".into(),
        }
        .to_json(9, "attention", 22);
        assert_eq!(keys["bytes_hex"], "1b5b420d");
        assert_eq!(keys["why"], "picker painted");
        assert_eq!(Outbound::End.to_json(1, "s", 1)["type"], "end");
        assert_eq!(Outbound::Interrupt.to_json(1, "s", 1)["type"], "interrupt");
        let ans = Outbound::Answer {
            tool_use_id: "t".into(),
            answers: Map::new(),
            route: Route::File,
        }
        .to_json(1, "s", 1);
        assert_eq!(ans["route"], "file");
    }

    #[test]
    fn a_tool_use_id_becomes_a_file_name_that_cannot_walk() {
        assert_eq!(file_key("toolu_01ABC"), "toolu_01ABC");
        assert_eq!(file_key("../../etc/passwd"), "etcpasswd");
        assert_eq!(file_key("a b"), "ab");
    }

    #[test]
    fn an_answer_that_arrives_before_its_question_closes_the_round_on_arrival() {
        // A journal replayed from an offset, or two hooks racing: the harness's
        // `answered` can reach the reader before the `question` it answers.
        // Presenting that question live would ask a person something already
        // settled, so the answer is remembered and the question arrives closed.
        let mut st = State::new();
        assert_eq!(
            st.take(
                Inbound::Answered {
                    tool_use_id: "toolu_01ABC".into(),
                    answers: Some(json!({"answers": {"Which drink?": "Tea"}})),
                },
                1
            ),
            Effect::Nothing
        );
        assert!(!st.has_open_question());
        let Effect::Present(cards) = st.take(Inbound::parse(&question_event()).unwrap(), 2) else {
            panic!()
        };
        assert_eq!(cards.len(), 2);
        assert!(!st.has_open_question(), "closed on arrival");
        assert!(matches!(st.press(&cards[0].id, 0, 3), Press::Refused(_)));
        assert_eq!(
            st.matching("Which drink?"),
            None,
            "not the screen reader's either"
        );
    }

    #[test]
    fn the_cap_evicts_a_settled_round_before_an_open_one() {
        let mut st = State::new();
        let round = |id: &str| Inbound::Question {
            at_ms: None,
            tool_use_id: id.into(),
            questions: vec![Asked {
                question: format!("Q for {id}?"),
                header: None,
                multi: false,
                options: vec![
                    AskedOption {
                        label: "a".into(),
                        description: None,
                        preview: None,
                    },
                    AskedOption {
                        label: "b".into(),
                        description: None,
                        preview: None,
                    },
                ],
            }],
            deadline_ms: None,
        };
        // The oldest round stays OPEN; the second is answered by the harness.
        st.take(round("open-0"), 1);
        st.take(round("done-1"), 2);
        st.take(
            Inbound::Answered {
                tool_use_id: "done-1".into(),
                answers: None,
            },
            3,
        );
        for i in 2..=ROUNDS_KEPT {
            st.take(round(&format!("r-{i}")), 10 + i as u64);
        }
        // One past the cap: the settled one goes, the open one is still here.
        let open = SurfaceId("ask-hook-open-0-0".into());
        let done = SurfaceId("ask-hook-done-1-0".into());
        assert!(
            st.owns(&open).is_some(),
            "an open question outlives the cap"
        );
        assert!(st.owns(&done).is_none(), "the settled round made room");
    }

    #[test]
    fn replaying_the_journal_leaves_the_state_where_it_was() {
        // A window that restarts re-reads the journal from the start. Every
        // record is idempotent: the same question is one round, a waiting
        // whose deadline has passed opens no file road, and a round the bench
        // already answered refuses a second press.
        let journal = |st: &mut State, now: u64| {
            st.take(Inbound::parse(&question_event()).unwrap(), now);
            st.take(
                Inbound::Waiting {
                    tool_use_id: "toolu_01ABC".into(),
                    until_ms: 5_000,
                },
                now,
            );
        };
        let mut st = State::new();
        journal(&mut st, 1_000);
        let single = SurfaceId("ask-hook-toolu_01ABC-0".into());
        let multi = SurfaceId("ask-hook-toolu_01ABC-1".into());
        assert_eq!(st.press(&single, 1, 2_000), Press::Recorded);
        assert_eq!(st.press(&multi, 0, 2_000), Press::Recorded);
        assert_eq!(st.press(&multi, 2, 2_000), Press::Recorded);
        // The round of two is sent by its SUBMIT tab, not by running out of
        // questions.
        assert!(matches!(
            st.submit(&multi, 2_000),
            Press::WriteAnswers { .. }
        ));
        let before = st.round_surfaces(&single, 0);
        st.take(
            Inbound::Answered {
                tool_use_id: "toolu_01ABC".into(),
                answers: None,
            },
            3_000,
        );
        // The restart: the same records again, long after the hook's deadline.
        journal(&mut st, 9_000);
        assert!(
            !st.has_open_question(),
            "a replayed question is not a new one"
        );
        assert!(matches!(st.press(&single, 0, 9_001), Press::Refused(_)));
        let after = st.round_surfaces(&single, 0);
        assert_eq!(before.len(), after.len());
        for (b, a) in before.iter().zip(&after) {
            assert_eq!(b.kind, a.kind, "the same cards, answered the same way");
        }
        // A fresh reader that only ever saw the replay: the waiting is stale
        // (its deadline is behind it), so a press would not take the file road.
        let mut cold = State::new();
        journal(&mut cold, 9_000);
        // A single choice with no road but the sentence goes out at once, per
        // question — there is no round to wait for on that road.
        assert_eq!(
            cold.press(&single, 1, 9_001),
            Press::Sentence {
                label: "Coffee".into()
            }
        );
        assert_eq!(cold.press(&multi, 0, 9_001), Press::Recorded);
        assert_eq!(
            cold.press(&multi, 2, 9_001),
            Press::Sentence {
                label: String::new()
            },
            "no hook is waiting and no picker painted: a sentence"
        );
    }

    /// Verbatim off pane 13 on 2026-09-21 — the question that produced two
    /// Submit buttons. 254 characters, in a pane about sixty columns wide.
    const A_QUESTION_TOO_LONG_FOR_THE_PANE: &str = "On 2026-09-15 (`dbb39cf`, the incinerator-bay commit) the name box was deliberately stopped from opening on creation, with the reason \"the next move is usually using the terminal it just made\". Which creation should open straight into its name box again?";

    fn a_long_multi_select() -> Inbound {
        Inbound::Question {
            at_ms: None,
            tool_use_id: "toolu_01LONG".into(),
            questions: vec![Asked {
                question: A_QUESTION_TOO_LONG_FOR_THE_PANE.into(),
                header: Some("Which one".into()),
                multi: true,
                options: [
                    "A new project and a new group",
                    "Also a plain new tab",
                    "Only a plain new tab",
                ]
                .into_iter()
                .map(|label| AskedOption {
                    label: label.into(),
                    description: None,
                    preview: None,
                })
                .collect(),
            }],
            deadline_ms: None,
        }
    }

    /// The screen reader gets one ROW of a paragraph, so the fold has to be a
    /// suffix test. Under equality it missed on every question the pane was
    /// too narrow to draw whole, and the twin it minted carried a second
    /// Submit.
    #[test]
    fn a_question_too_long_for_the_pane_still_matches_the_card_that_asked_it() {
        let mut st = State::new();
        st.take(a_long_multi_select(), 10);
        let card = SurfaceId("ask-hook-toolu_01LONG-0".into());

        // A terminal breaks on a space when it can...
        assert_eq!(
            st.matching("Which creation should open straight into its name box again?"),
            Some(card.clone()),
            "the last wrapped row is the only thing the screen reader has"
        );
        // ...and inside a word when it cannot. Both are suffixes.
        assert_eq!(
            st.matching("ht into its name box again?"),
            Some(card.clone()),
            "a wrap mid-word is still a suffix once the whitespace is folded"
        );
        // The whole thing, for a pane wide enough to show it.
        assert_eq!(
            st.matching(A_QUESTION_TOO_LONG_FOR_THE_PANE),
            Some(card),
            "equality is a special case of suffix, not a different rule"
        );
        // And it is still a TEST: a reading of something else finds nothing.
        assert_eq!(st.matching("Which drink?"), None);
        assert_eq!(st.matching("   "), None, "an empty reading matches nothing");
    }

    /// Ticking a box leaves the picker on the same question, so the card must
    /// stay open and keep its Submit — and the fold must keep finding it.
    ///
    /// Both halves of one bug. Reported as answered, the card took its own
    /// Submit away; skipped by the fold, a twin appeared carrying one. Between
    /// them, the only live Submit on the bench belonged to a card the channel
    /// did not own.
    #[test]
    fn a_multi_select_is_not_answered_until_it_is_submitted() {
        let mut st = State::new();
        st.take(a_long_multi_select(), 10);
        let card = SurfaceId("ask-hook-toolu_01LONG-0".into());

        let answer_now = |st: &State| {
            let Kind::Question(q) = &st.round_surfaces(&card, 0)[0].kind else {
                panic!()
            };
            q.answer.clone()
        };

        assert_eq!(answer_now(&st), Answered::Waiting, "nothing ticked");
        assert_eq!(st.press(&card, 0, 11), Press::Recorded, "a tick records");
        assert_eq!(
            answer_now(&st),
            Answered::Waiting,
            "a tick is not an answer — the card keeps its Submit"
        );
        assert_eq!(
            st.matching(A_QUESTION_TOO_LONG_FOR_THE_PANE),
            Some(card.clone()),
            "the picker has not moved on, so neither has the fold"
        );
        // The tick IS visible — on the option, where it belongs.
        let Kind::Question(q) = &st.round_surfaces(&card, 0)[0].kind else {
            panic!()
        };
        assert_eq!(q.options[0].checked, Some(true));
        assert_eq!(q.options[1].checked, Some(false));

        // Ending the round is what settles it.
        st.take(
            Inbound::Answered {
                tool_use_id: "toolu_01LONG".into(),
                answers: None,
            },
            12,
        );
        assert!(
            !matches!(answer_now(&st), Answered::Waiting),
            "once the round has ended the card is finished"
        );
        assert_eq!(
            st.matching(A_QUESTION_TOO_LONG_FOR_THE_PANE),
            None,
            "and a stale reading cannot re-arm it"
        );
    }

    #[test]
    fn two_questions_with_one_text_in_a_round_are_matched_in_order() {
        // The screen reader matches by words. Two questions that read the
        // same — an agent asking "Are you sure?" twice — must resolve to the
        // FIRST unanswered card, then the next once that one is answered.
        let mut st = State::new();
        let q = |label: &str| Asked {
            question: "Are you sure?".into(),
            header: Some(label.into()),
            multi: false,
            options: vec![
                AskedOption {
                    label: "yes".into(),
                    description: None,
                    preview: None,
                },
                AskedOption {
                    label: "no".into(),
                    description: None,
                    preview: None,
                },
            ],
        };
        st.take(
            Inbound::Question {
                at_ms: None,
                tool_use_id: "twice".into(),
                questions: vec![q("first"), q("second")],
                deadline_ms: None,
            },
            1,
        );
        let first = SurfaceId("ask-hook-twice-0".into());
        let second = SurfaceId("ask-hook-twice-1".into());
        assert_eq!(st.matching("Are you sure"), Some(first.clone()));
        st.saw_cursor(&first, 0, None, None);
        assert!(matches!(st.press(&first, 0, 2), Press::Keys { .. }));
        assert_eq!(st.matching("Are you sure?"), Some(second));
    }

    #[test]
    fn the_beacon_refreshes_an_open_bench_and_writes_a_closed_one_once() {
        assert!(beacon_due(None, false, 0), "the first marker always goes");
        assert!(beacon_due(None, true, 0));
        // Open: every BEACON_MS, so a hook can tell a live window from a dead one.
        assert!(!beacon_due(
            Some((true, 1_000)),
            true,
            1_000 + BEACON_MS - 1
        ));
        assert!(beacon_due(Some((true, 1_000)), true, 1_000 + BEACON_MS));
        // Closed: on the change, and then never — "terminal" does not age.
        assert!(
            beacon_due(Some((true, 1_000)), false, 1_001),
            "the close is written at once"
        );
        assert!(!beacon_due(Some((false, 1_000)), false, 1_000 + 60_000));
        assert!(
            beacon_due(Some((false, 1_000)), true, 1_001),
            "and so is the open"
        );
    }

    #[test]
    fn hook_replies_get_distinct_ids_inside_one_millisecond() {
        let mut st = State::new();
        let reply = |st: &mut State| {
            st.take(
                Inbound::Prompt {
                    at_ms: None,
                    prompt_id: None,
                    text: Some("go".into()),
                },
                7,
            );
            match st.take(
                Inbound::Reply {
                    at_ms: None,
                    text: Some("done".into()),
                },
                7,
            ) {
                Effect::Reply { text, n, .. } => reply_surface(&text, 7, n).unwrap().id,
                other => panic!("{other:?}"),
            }
        };
        let a = reply(&mut st);
        let b = reply(&mut st);
        assert_ne!(a, b, "two replies at the same clock are two surfaces");
        assert_eq!(a.as_str(), "reply-hook-7-1");
        assert_eq!(b.as_str(), "reply-hook-7-2");
    }

    /// The real adapter, driven the way the harness drives it, read the way
    /// the window reads it, answered the way a press answers it. Bash, jq and
    /// flock have to be on the path — they are on this machine and on CI's
    /// runner; anywhere else the test says so rather than failing for a reason
    /// that is not the code's.
    #[test]
    fn a_question_round_trips_through_the_real_adapter_and_the_reader() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let have = Command::new("bash")
            .args([
                "-c",
                "command -v jq >/dev/null && command -v flock >/dev/null",
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !have {
            eprintln!("skipping the adapter round trip: jq or flock is not on the path");
            return;
        }
        let script =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/td-agent-hooks");
        let state = std::env::temp_dir().join(format!(
            "tdac-rt-{}-{}",
            std::process::id(),
            crate::surfacefeed::now_ms()
        ));
        let dir = state.join("terminal-delight/surfaces/attention/22");
        std::fs::create_dir_all(&dir).unwrap();
        let payload = json!({
            "session_id": "s1", "hook_event_name": "PreToolUse",
            "tool_name": "AskUserQuestion", "tool_use_id": "toolu_RT1",
            "tool_input": { "questions": [ { "question": "Which drink?", "header": "Drink",
                "multiSelect": false,
                "options": [ {"label": "Tea", "description": "leaves"},
                             {"label": "Coffee", "description": "beans"} ] } ] }
        });
        // The window says its bench is open on this pane, before the hook looks.
        crate::surfacefeed::write_marker(&dir, true, crate::surfacefeed::now_ms()).unwrap();
        let mut child = Command::new("bash")
            .arg(&script)
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", std::env::var("HOME").unwrap_or_default())
            .env("XDG_STATE_HOME", &state)
            .env("TD_SESSION", "attention")
            .env("TD_PANE_ID", "22")
            .env("TD_ASK_WAIT_S", "20")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn the adapter");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.to_string().as_bytes())
            .unwrap();
        // The window's side: sweep the journal until the question and the
        // hook's `waiting` are both there, keeping the marker fresh meanwhile.
        let mut feed = crate::surfacefeed::Feed::new();
        let mut st = State::new();
        let mut card = None;
        let mut waiting = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline && !(card.is_some() && waiting) {
            crate::surfacefeed::write_marker(&dir, true, crate::surfacefeed::now_ms()).unwrap();
            for ev in feed.tail_inbound(&dir) {
                waiting |= matches!(ev, Inbound::Waiting { .. });
                if let Effect::Present(cards) = st.take(ev, crate::surfacefeed::now_ms()) {
                    card = cards.first().map(|c| c.id.clone());
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let card = card.expect("the question reached the reader");
        assert!(waiting, "the hook held the picker");
        assert!(st.has_open_question());
        // The press takes the file road, because the hook is waiting.
        match st.press(&card, 1, crate::surfacefeed::now_ms()) {
            Press::WriteAnswers {
                tool_use_id,
                answers,
            } => {
                crate::surfacefeed::write_answers(&dir, &tool_use_id, &answers).unwrap();
            }
            other => panic!("{other:?}"),
        }
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "the adapter exits 0");
        let decision: Value =
            serde_json::from_slice(&out.stdout).expect("the pre-answer on stdout");
        assert_eq!(
            decision["hookSpecificOutput"]["permissionDecision"],
            "allow"
        );
        assert_eq!(
            decision["hookSpecificOutput"]["updatedInput"]["answers"]["Which drink?"],
            "Coffee"
        );
        // And the reader sees the release, with its reason and its clock.
        let mut released = None;
        for ev in feed.tail_inbound(&dir) {
            if let Inbound::Released { why, .. } = &ev {
                released = Some(why.clone());
            }
            st.take(ev, 0);
        }
        assert_eq!(released.as_deref(), Some("answered"));
        let _ = std::fs::remove_dir_all(&state);
    }

    /// A PARTIAL ROUND NEVER GOES OUT AS ONE KEYSTROKE.
    ///
    /// The picker calls its own button `Submit` on the last question of a
    /// round and `Next` on every other one, and the screen reader records only
    /// where it sits. So a keystroke aimed at it from an unanswered question
    /// presses `Next`: the picker advances, the round does not go, and the
    /// bench has just set the flag that blanks every remaining question and
    /// takes the chips away. The agent stays blocked with nothing on the bench
    /// that can answer it.
    ///
    /// Measured on `cdb64925`, before the completeness clause:
    /// `Keys { bytes: [1b 5b 42 × 3, 0d], note: "submitted the round" }`, and
    /// both questions came back `Skipped`.
    ///
    /// Two halves, because either alone would pass on the broken code: that a
    /// partial round says it in words, and that a COMPLETE one still sends the
    /// keystroke — a fix that refused the keys road outright would take away
    /// the only exit a finished round has on that road.
    #[test]
    fn a_partial_round_is_spoken_rather_than_half_pressed_into_the_picker() {
        let mut st = State::new();
        let ev = Inbound::parse(&question_event()).unwrap();
        let Effect::Present(cards) = st.take(ev, 1_000) else {
            panic!()
        };
        let first = cards[0].id.clone();
        // No hook hold, so this is the keys road; the screen reader has seen
        // the picker's button at row 3.
        st.saw_cursor(&first, 0, Some(3), Some(MenuButton::EndsRound));
        match st.submit(&first, 5_000) {
            Press::Sentence { label } => {
                assert!(
                    label.contains("left blank"),
                    "the blanks have to be named aloud: {label}"
                );
            }
            other => panic!("a partial round must not drive the picker: {other:?}"),
        }
        // And nothing of it reads as a considered omission, because nothing
        // was considered — see `nothing_picked`.
        let after = st.round_surfaces(&first, 5_100);
        for s in &after {
            let Kind::Question(q) = &s.kind else { panic!() };
            assert_eq!(q.answer, Answered::Cancelled);
        }

        // THE COMPLETE ROUND STILL GOES BY KEYS. Every answer already went
        // down the pseudoterminal as it was made, so the picker is on the last
        // question and its button really is the round's Submit.
        let mut st = State::new();
        let Effect::Present(cards) = st.take(Inbound::parse(&question_event()).unwrap(), 1_000)
        else {
            panic!()
        };
        let single = cards[0].id.clone();
        let multi = cards[1].id.clone();
        st.saw_cursor(&single, 0, Some(3), Some(MenuButton::EndsRound));
        st.saw_cursor(&multi, 0, Some(3), Some(MenuButton::EndsRound));
        assert!(matches!(st.press(&single, 1, 1_100), Press::Keys { .. }));
        assert_eq!(st.press(&multi, 0, 1_101), Press::Recorded, "a tick");
        assert!(matches!(st.press(&multi, 2, 1_102), Press::Keys { .. }));
        // A press spends its cursor; the next sweep supplies another, which is
        // what makes the round sendable by keys at all.
        st.saw_cursor(&multi, 0, Some(3), Some(MenuButton::EndsRound));
        assert!(
            matches!(st.submit(&multi, 1_200), Press::Keys { .. }),
            "a finished round still ends with the picker's own Submit"
        );
    }

    /// A COMPLETE ROUND WILL NOT BE SENT BY A ROW THAT SAYS `Next`.
    ///
    /// The failure this closes, measured and filed as `terminal-delight#698`:
    /// the hook's deadline fires, the tool proceeds — a `PreToolUse` hook that
    /// exits without `updatedInput` lets the call run — and the picker paints
    /// from question ONE, while the bench still holds every answer.
    ///
    /// [`Round::complete`] is true of the BENCH's record and says nothing
    /// about where the picker is. It used to be the whole test for "that row
    /// is the round's Submit", so the round went out as a single keystroke
    /// aimed at a row spelling `Next`: the menu advanced one question, `sent`
    /// turned every remaining blank into [`Answered::Skipped`] and took the
    /// chips away, and the agent stayed blocked on the rest with nothing left
    /// that could answer it.
    ///
    /// The reader now keeps the WORD beside the place, so the guard asks the
    /// picker what its button does instead of inferring it from the bench. The
    /// round falls to words, which is the road that cannot half-press anybody's
    /// menu — and the answers survive as a sentence the agent reads.
    #[test]
    fn a_complete_round_will_not_press_a_button_that_says_next() {
        let mut st = State::new();
        let Effect::Present(cards) = st.take(Inbound::parse(&question_event()).unwrap(), 1_000)
        else {
            panic!()
        };
        let single = cards[0].id.clone();
        let multi = cards[1].id.clone();

        // The whole round answered on the bench, exactly as the passing case
        // above does it.
        st.saw_cursor(&single, 0, Some(3), Some(MenuButton::EndsRound));
        assert!(matches!(st.press(&single, 1, 1_100), Press::Keys { .. }));
        st.saw_cursor(&multi, 0, Some(3), Some(MenuButton::EndsRound));
        assert_eq!(st.press(&multi, 0, 1_101), Press::Recorded, "a tick");
        assert!(matches!(st.press(&multi, 2, 1_102), Press::Keys { .. }));

        // The hook gives up. Nothing about the bench's own record changes —
        // which is the whole trap.
        st.take(
            Inbound::Released {
                tool_use_id: "toolu_01ABC".into(),
                why: "timeout".into(),
            },
            1_150,
        );

        // The picker repaints from question one, where the button reads `Next`.
        st.saw_cursor(&multi, 0, Some(3), Some(MenuButton::StepsOn));
        match st.submit(&multi, 1_200) {
            Press::Sentence { label } => assert!(
                !label.is_empty(),
                "the answers still travel, as words the agent reads"
            ),
            other => panic!("a round must not go out on a row that says Next: {other:?}"),
        }
    }

    /// The same round, the same release, a row that really does end it.
    ///
    /// The other half of the guard: this must NOT have been closed by refusing
    /// the keys road whenever a round has been released. A release is why the
    /// file road is shut, not a reason the picker cannot be driven — and if
    /// this test ever starts failing, the fix above has been widened into a
    /// feature nobody asked to lose.
    #[test]
    fn a_released_round_still_goes_by_keys_when_the_row_really_submits() {
        let mut st = State::new();
        let Effect::Present(cards) = st.take(Inbound::parse(&question_event()).unwrap(), 1_000)
        else {
            panic!()
        };
        let single = cards[0].id.clone();
        let multi = cards[1].id.clone();
        st.saw_cursor(&single, 0, Some(3), Some(MenuButton::EndsRound));
        assert!(matches!(st.press(&single, 1, 1_100), Press::Keys { .. }));
        st.saw_cursor(&multi, 0, Some(3), Some(MenuButton::EndsRound));
        assert_eq!(st.press(&multi, 0, 1_101), Press::Recorded, "a tick");
        assert!(matches!(st.press(&multi, 2, 1_102), Press::Keys { .. }));
        st.take(
            Inbound::Released {
                tool_use_id: "toolu_01ABC".into(),
                why: "timeout".into(),
            },
            1_150,
        );
        st.saw_cursor(&multi, 0, Some(3), Some(MenuButton::EndsRound));
        assert!(
            matches!(st.submit(&multi, 1_200), Press::Keys { .. }),
            "the picker is on its last question and says so"
        );
    }

    /// A press the sweep supplied no cursor for reads the picker itself.
    ///
    /// Parker's round on 2026-09-25, rows as his terminal showed them: Drink
    /// answered by keys, then the picker on Pet with `❯ 1. Cats`. Without a
    /// cursor, `Dogs` took the sentence road the blocked agent never reads —
    /// the first half below pins that it still would, so this test fails if
    /// `read_picker` is not what changed the outcome.
    #[test]
    fn a_press_with_no_swept_cursor_reads_the_picker_on_the_screen() {
        let event = json!({
            "td": "0.1", "type": "question", "at_ms": 1000, "pid": 42,
            "tool_use_id": "toolu_01PET",
            "questions": [
                {"question": "Coffee or tea?", "header": "Drink", "multiSelect": false,
                 "options": [{"label": "Coffee", "description": "Coffee, every time"},
                             {"label": "Tea", "description": "Tea, every time"},
                             {"label": "Neither", "description": "Something else, or nothing"}]},
                {"question": "Cats or dogs?", "header": "Pet", "multiSelect": false,
                 "options": [{"label": "Cats", "description": "Cats"},
                             {"label": "Dogs", "description": "Dogs"},
                             {"label": "Both", "description": "No need to pick"}]}
            ]
        });
        let pet_screen: Vec<String> = [
            "\u{2190}  \u{2612} Drink  \u{2610} Pet  \u{2714} Submit  \u{2192}",
            "",
            "Cats or dogs?",
            "",
            "\u{276f} 1. Cats",
            "     Cats",
            "  2. Dogs",
            "     Dogs",
            "  3. Both",
            "     No need to pick",
            "  4. Type something.",
            "  5. Chat about this",
            "",
            "Enter to select \u{b7} Tab/Arrow keys to navigate \u{b7} Esc to cancel",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        // The failure, as it happened: no cursor for Pet, so Dogs is spoken.
        let mut st = State::new();
        let Effect::Present(cards) = st.take(Inbound::parse(&event).unwrap(), 1_000) else {
            panic!()
        };
        let (drink, pet) = (cards[0].id.clone(), cards[1].id.clone());
        st.saw_cursor(&drink, 0, None, None);
        assert!(matches!(st.press(&drink, 0, 1_100), Press::Keys { .. }));
        assert!(
            matches!(st.press(&pet, 1, 1_200), Press::Sentence { .. }),
            "without a cursor the press is spoken — the bug this guards"
        );

        // The repair: the press reads the screen first, and goes by keys.
        let mut st = State::new();
        let Effect::Present(cards) = st.take(Inbound::parse(&event).unwrap(), 1_000) else {
            panic!()
        };
        let (drink, pet) = (cards[0].id.clone(), cards[1].id.clone());
        st.saw_cursor(&drink, 0, None, None);
        assert!(matches!(st.press(&drink, 0, 1_100), Press::Keys { .. }));
        assert!(
            st.read_picker(&pet, &pet_screen),
            "the screen is Pet's picker"
        );
        match st.press(&pet, 1, 1_200) {
            Press::Keys { bytes, .. } => assert_eq!(bytes, b"\x1b[B\r", "one down to Dogs"),
            other => panic!("Dogs must reach the picker: {other:?}"),
        }
        // And a screen showing some other question drives nothing here.
        let mut st = State::new();
        let Effect::Present(cards) = st.take(Inbound::parse(&event).unwrap(), 1_000) else {
            panic!()
        };
        assert!(
            !st.read_picker(&cards[0].id, &pet_screen),
            "Pet's picker is not Drink's"
        );
    }

    /// A position with no word read off it is not a button this channel presses.
    ///
    /// `None` here is *nobody looked*, and it is a different fact from *the row
    /// says Next* — but both are unsafe to send a round on, and the type is
    /// what makes them behave the same at the one place it matters. A card the
    /// hook declared and no screen reading ever touched has no button at all.
    #[test]
    fn a_button_whose_word_was_never_read_is_not_pressed() {
        assert_eq!(
            SeenPicker {
                at: 0,
                button: None
            }
            .ending_slot(),
            None,
            "no row was read"
        );
        assert_eq!(
            SeenPicker {
                at: 0,
                button: Some((3, MenuButton::StepsOn))
            }
            .ending_slot(),
            None,
            "the row steps on"
        );
        assert_eq!(
            SeenPicker {
                at: 0,
                button: Some((3, MenuButton::EndsRound))
            }
            .ending_slot(),
            Some(3),
            "the row ends the round"
        );
        // And the POSITION is still available to navigation arithmetic under
        // either word, which is the reason the two halves are separate
        // accessors rather than one.
        assert_eq!(
            SeenPicker {
                at: 0,
                button: Some((3, MenuButton::StepsOn))
            }
            .slot(),
            Some(3),
        );
    }

    /// A ROUND SENT WITH NOTHING IN IT IS A REFUSAL, not three omissions.
    ///
    /// `Skipped` says a person looked at this question and chose to send
    /// without it. That is true of a blank beside an answer and false of a
    /// round where nothing was picked, where the one decision was *go on
    /// without me*. Drawing it as three considered omissions invents
    /// deliberation nobody performed, and `Cancelled` already says exactly the
    /// right thing.
    #[test]
    fn a_round_submitted_with_nothing_picked_reads_as_refused() {
        let mut st = State::new();
        let ev = Inbound::parse(&question_event()).unwrap();
        let Effect::Present(cards) = st.take(ev, 1_000) else {
            panic!()
        };
        let first = cards[0].id.clone();
        st.take(
            Inbound::Waiting {
                tool_use_id: "toolu_01ABC".into(),
                until_ms: 600_000,
            },
            1_001,
        );
        // The tab is still offered — a person saying "none of these" in one
        // press is a real gesture and counting answers would hide it.
        assert!(st.submittable(&first));
        assert!(matches!(
            st.submit(&first, 1_500),
            Press::WriteAnswers { .. }
        ));
        for s in st.round_surfaces(&first, 1_600) {
            let Kind::Question(q) = &s.kind else { panic!() };
            assert_eq!(q.answer, Answered::Cancelled, "{:?}", s.id);
        }

        // AND THE PARTIAL ROUND IS UNCHANGED, which is the half that keeps
        // this from being a rename: one answer given and one left means the
        // blank really was skipped, and it must not read as a refusal.
        let mut st = State::new();
        let Effect::Present(cards) = st.take(Inbound::parse(&question_event()).unwrap(), 1_000)
        else {
            panic!()
        };
        let first = cards[0].id.clone();
        st.take(
            Inbound::Waiting {
                tool_use_id: "toolu_01ABC".into(),
                until_ms: 600_000,
            },
            1_001,
        );
        assert_eq!(st.press(&first, 1, 1_500), Press::Recorded);
        assert!(matches!(
            st.submit(&first, 1_600),
            Press::WriteAnswers { .. }
        ));
        let after = st.round_surfaces(&first, 1_700);
        let Kind::Question(given) = &after[0].kind else {
            panic!()
        };
        let Kind::Question(blank) = &after[1].kind else {
            panic!()
        };
        assert_eq!(given.answer, Answered::Chose(1));
        assert_eq!(blank.answer, Answered::Skipped);
    }
}
