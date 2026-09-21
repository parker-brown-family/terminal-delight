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
    Answered, Choice_, Kind, Origin, Question, Round as QRound, Step, Surface, SurfaceId, Weight,
};

/// The channel's own version, carried on every record the window writes.
pub const TDAC_VERSION: &str = "0.1";

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
    pub fn to_json(&self, at_ms: u64) -> Value {
        let mut m = Map::new();
        m.insert("td".into(), json!(TDAC_VERSION));
        m.insert("at_ms".into(), json!(at_ms));
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
    /// The harness said the tool returned.
    pub closed: bool,
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
            closed: false,
            sent: false,
        }
    }

    /// The surface id of question `i`. Stable across sweeps, so the harness's
    /// own `answered` lands on the card that asked.
    pub fn surface_id(&self, i: usize) -> SurfaceId {
        SurfaceId(format!("ask-hook-{}-{i}", file_key(&self.tool_use_id)))
    }

    /// Every question has an answer committed.
    pub fn complete(&self) -> bool {
        self.picked
            .iter()
            .all(|p| p.as_ref().is_some_and(|v| !v.is_empty()))
    }

    /// The map the harness's own answer channel takes: question text to the
    /// chosen label, several labels joined by `", "` on a multi-select.
    /// `None` until every question is answered — a partial map would answer
    /// questions nobody has looked at with nothing.
    pub fn answers(&self) -> Option<Map<String, Value>> {
        if !self.complete() {
            return None;
        }
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
            out.insert(q.question.clone(), json!(labels.join(", ")));
        }
        Some(out)
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
                    (_, []) => Answered::Waiting,
                    (false, [one]) => Answered::Chose(*one),
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

/// What the bench should do about one inbound record.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Effect {
    /// Caption the overview with the person's exact words.
    Asked { text: String },
    /// Put these on the bench (present, or re-present with new state).
    Present(Vec<Surface>),
    /// Present the agent's reply as a response, because none arrived itself.
    Reply { text: String },
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
    cursors: BTreeMap<SurfaceId, (usize, Option<usize>)>,
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
                    Some(t) if !t.trim().is_empty() => Effect::Asked { text: t },
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
                let round = Round::new(tool_use_id, at_ms, questions);
                let surfaces = round.surfaces(now_ms);
                self.rounds.push(round);
                if self.rounds.len() > ROUNDS_KEPT {
                    let gone = self.rounds.remove(0);
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
                    return Effect::Nothing;
                };
                r.closed = true;
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
            Inbound::Reply { text, .. } => match text {
                Some(t) if !t.trim().is_empty() && self.responses_since_prompt == 0 => {
                    Effect::Reply { text: t }
                }
                _ => Effect::Nothing,
            },
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
            .any(|r| !r.closed && !r.sent && !r.complete())
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
    pub fn matching(&self, question_text: &str) -> Option<SurfaceId> {
        let want = fold(question_text);
        self.rounds.iter().filter(|r| !r.closed).find_map(|r| {
            r.questions
                .iter()
                .enumerate()
                .find(|(i, q)| r.picked[*i].is_none() && fold(&q.question) == want)
                .map(|(i, _)| r.surface_id(i))
        })
    }

    /// The screen reader saw the picker for this card, and where its highlight is.
    pub fn saw_cursor(&mut self, id: &SurfaceId, cursor: usize, submit: Option<usize>) {
        self.cursors.insert(id.clone(), (cursor, submit));
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
            if let (Some((cursor, _)), Kind::Question(q)) =
                (self.cursors.get(&s.id).copied(), &mut s.kind)
            {
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
        if round.closed || round.sent {
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
            Route::File => match round.answers() {
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
                let (at, submit) = cursor.unwrap_or((0, None));
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
pub fn reply_surface(text: &str, now_ms: u64) -> Option<Surface> {
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
        "id": format!("reply-hook-{now_ms}"),
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
        match st.press(&multi, 2, 2_003) {
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
        st.saw_cursor(&single, 0, None);
        match st.press(&single, 1, 2_000) {
            Press::Keys { bytes, note } => {
                assert_eq!(bytes, crate::workbench::menu_keys(1, 0));
                assert_eq!(note, "chose \"Coffee\"");
            }
            other => panic!("{other:?}"),
        }
        // Answered: no longer matched by the screen reader.
        assert_eq!(st.matching("Which drink?"), None);
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
                text: "done again".into()
            }
        );
        let s = reply_surface("done again\nwith detail", 4).expect("a response");
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
        .to_json(9);
        assert_eq!(say["type"], "say");
        assert_eq!(say["delivery"], "paste");
        assert_eq!(say["at_ms"], 9);
        let keys = Outbound::Keys {
            bytes: vec![0x1b, b'[', b'B', b'\r'],
            why: "picker painted".into(),
        }
        .to_json(9);
        assert_eq!(keys["bytes_hex"], "1b5b420d");
        assert_eq!(keys["why"], "picker painted");
        assert_eq!(Outbound::End.to_json(1)["type"], "end");
        assert_eq!(Outbound::Interrupt.to_json(1)["type"], "interrupt");
        let ans = Outbound::Answer {
            tool_use_id: "t".into(),
            answers: Map::new(),
            route: Route::File,
        }
        .to_json(1);
        assert_eq!(ans["route"], "file");
    }

    #[test]
    fn a_tool_use_id_becomes_a_file_name_that_cannot_walk() {
        assert_eq!(file_key("toolu_01ABC"), "toolu_01ABC");
        assert_eq!(file_key("../../etc/passwd"), "etcpasswd");
        assert_eq!(file_key("a b"), "ab");
    }
}
