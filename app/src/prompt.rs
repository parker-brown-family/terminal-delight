//! Prompts: one agent's words typed into another agent's own conversation as
//! its next turn, along a line a person drew at `prompt` in the Threshing
//! Floor. An ask reads a worker; a prompt gives it work.
//!
//! This file is the timing and the rules, and none of the terminal: it holds
//! every prompt from its send to its end and decides, from what the window
//! observes of the worker on each tick, what happens next — wait, wake it,
//! paste, press Enter, or report a stage. The window does the typing
//! (`Workspace::prompt_tick`), so every write this feature makes is one of
//! the [`Act`]s below, and every one of them is decided here, where a test
//! can hold it.
//!
//! Three rules carry the safety, and each is stated where it is enforced:
//!
//! - **Typed only at a recognised rest.** A paste goes in only while the
//!   worker's screen shows its own idle prompt, nothing a person typed is in
//!   the last [`QUIET_MS`], and its prompt has asked for bracketed paste. A
//!   permission prompt, a dialog, an error or a screen nobody can read is
//!   never typed into ([`Seen`]).
//! - **Enter only on proof the paste landed.** After the paste, Enter waits
//!   until a row that changed on the worker's screen shows this prompt's own
//!   number or the program's paste placeholder ([`shows_paste`]). No proof in
//!   [`SHOW_PASTE_MS`], no Enter: the prompt fails and says where its text may
//!   be sitting. Enter is pressed once and never again.
//! - **One at a time per worker.** Prompts to one worker go in send order, the
//!   next only after the last has ended, so two senders never interleave.

use std::collections::VecDeque;

use serde::Serialize;

use crate::threshing_floor::{AskId, Refusal, SeatId};

// The clocks below are slow on purpose. A coordinator's workers run in the
// background while their person is away, so a prompt that waits costs
// seconds and a prompt typed on a misread costs the hand-off. Parker,
// 2026-10-27: "slower and safer is basically better ... SPEED is NOT the
// priority at all."

/// How long a prompt waits for its worker to come to rest before it fails.
pub const WAIT_FOR_REST_MS: u64 = 10 * 60 * 1000;
/// A key a person pressed in the worker's pane holds a prompt back this long:
/// they may have a line half-typed, and a paste would join it.
pub const QUIET_MS: u64 = 60_000;
/// The pause between the paste and Enter, so a program that takes its paste
/// on the next frame has drawn it before it is looked for.
pub const ENTER_AFTER_MS: u64 = 1_000;
/// How long the paste has to show on the worker's screen before Enter is
/// given up.
pub const SHOW_PASTE_MS: u64 = 4_000;
/// After Enter, how long the worker has to be seen working.
pub const TAKE_UP_MS: u64 = 2 * 60 * 1000;
/// How long a worker must read at rest, look after look, with no turn
/// beginning, before it is typed into or its prompt is called finished. Two
/// seconds held through a streamed story in rig run 5; five leaves room for
/// a slower stream.
pub const SETTLE_MS: u64 = 5_000;
/// A worker back at rest, never seen working, has its transcript looked at
/// this long after Enter, and as often after that.
pub const PROBE_AFTER_MS: u64 = 3_000;
/// An error that stays on the worker's screen this long after Enter ends the
/// prompt.
pub const ERROR_ENDS_MS: u64 = 60_000;
/// A woken worker that is still asleep or resting this long after the wake is
/// woken again.
pub const REWAKE_MS: u64 = 60_000;
/// How long a woken agent's fresh pane may run a plain shell, while that
/// shell starts the agent again, before the prompt calls the worker gone.
pub const WAKE_GRACE_MS: u64 = 90_000;
/// How many ended prompts the book keeps for `prompt_status`, and for how
/// long.
pub const KEEP_ENDED: usize = 64;
pub const KEEP_ENDED_MS: u64 = 60 * 60 * 1000;
/// How many rows from the bottom of the worker's screen the paste is looked
/// for in.
pub const LOOK_ROWS: usize = 16;
/// The longest key a sender may name a prompt by.
pub const KEY_MAX_CHARS: usize = 64;

/// What the window sees of a worker on one tick, from its own screen and its
/// own clocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seen {
    /// At its own prompt, at rest: the idle furniture is on screen.
    Ready,
    /// A turn is running.
    Working,
    /// Waiting on a person: a permission prompt, a question, a dialog.
    Blocked,
    /// A rate limit or an API error is on screen.
    Errored,
    /// Resting (frozen) or asleep (ended, conversation kept): must be woken.
    Dormant,
    /// An agent whose screen matched no rule.
    Unreadable,
    /// The pane is gone, or is no longer an agent.
    Gone,
}

/// One tick's look at a worker.
#[derive(Clone, Debug, PartialEq)]
pub struct Worker {
    pub seen: Seen,
    /// The worker's state in the person's words, for `prompt_status`.
    pub label: String,
    /// Whether the program at the worker's prompt has asked for bracketed
    /// paste. Without it every newline would be an Enter, so nothing is typed.
    pub takes_paste: bool,
    /// A picker the workbench's dial raised is on screen.
    pub picker: bool,
    /// When a person last pressed a key in this pane, in ms since the epoch.
    pub person_key_ms: Option<u64>,
    /// When the pane's own scan last saw a turn begin.
    pub turn_began_ms: Option<u64>,
    /// The bottom [`LOOK_ROWS`] rows of its screen, as text.
    pub rows: Vec<String>,
    /// Whether a line a person drew still lets this sender prompt this pane,
    /// checked again on every tick: an erased line stops a prompt that has not
    /// been typed.
    pub allowed: Result<(), Refusal>,
    /// The worker's shell pid as the window found it: a woken worker has a
    /// new one, and the prompt follows it.
    pub pid: Option<u32>,
}

impl Worker {
    /// A worker the window could not find.
    pub fn gone() -> Self {
        Self {
            seen: Seen::Gone,
            label: "gone".to_string(),
            takes_paste: false,
            picker: false,
            person_key_ms: None,
            turn_began_ms: None,
            rows: Vec::new(),
            allowed: Err(Refusal::Gone),
            pid: None,
        }
    }
}

/// How far a prompt has got, on the wire as `stage` with what each stage
/// carries beside it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum PromptStage {
    /// Not typed: the worker is not at rest. `worker` is what it is doing.
    Waiting { worker: String },
    /// Pasted at the worker's prompt; Enter not yet pressed.
    Typing,
    /// Enter pressed; the worker has not yet been seen working.
    Delivered,
    /// The worker took it. `worker` is what it is doing now.
    Working { worker: String },
    /// The worker came back to rest. `reply` is its last message when its
    /// conversation is proven its own; `None` otherwise, with the reason.
    Finished {
        reply: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reply_unknown: Option<String>,
    },
    /// Ended without the worker finishing it.
    Failed { why: String },
}

impl PromptStage {
    pub fn is_open(&self) -> bool {
        !matches!(self, Self::Finished { .. } | Self::Failed { .. })
    }

    /// Typed into the worker: past the point where it can be withdrawn.
    pub fn is_typed(&self) -> bool {
        !matches!(self, Self::Waiting { .. })
    }

    /// The record's word for the stage.
    pub fn word(&self) -> &'static str {
        match self {
            Self::Waiting { .. } => "waiting",
            Self::Typing => "typing",
            Self::Delivered => "delivered",
            Self::Working { .. } => "working",
            Self::Finished { .. } => "finished",
            Self::Failed { .. } => "failed",
        }
    }
}

/// What `prompt_agent` and `prompt_status` hand back.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PromptStatus {
    pub prompt_id: AskId,
    /// The sender's own key, when it named one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// The worker's shell pid, as `list_panes` names it.
    pub pid: u32,
    /// Milliseconds since the prompt was sent.
    pub age_ms: u64,
    #[serde(flatten)]
    pub stage: PromptStage,
}

/// One prompt, held by the window from its send to its end.
#[derive(Clone, Debug, PartialEq)]
pub struct Sent {
    pub id: AskId,
    pub sender_pane: u64,
    pub from: Option<SeatId>,
    pub to: Option<SeatId>,
    pub target_pid: u32,
    pub key: Option<String>,
    /// The words pasted: the provenance header and the brief, cleaned.
    pub text: String,
    pub brief_len: usize,
    pub brief_sha256: String,
    pub sent_ms: u64,
    pub stage: PromptStage,
    /// When it was last woken, while asleep or resting.
    pub woke_ms: Option<u64>,
    pub pasted_ms: Option<u64>,
    /// The bottom rows of the worker's screen just before the paste.
    pub before: Vec<String>,
    pub entered_ms: Option<u64>,
    pub errored_ms: Option<u64>,
    /// The reply is being read off the window's thread.
    pub reading_reply: bool,
    /// When the current unbroken run of at-rest looks began.
    pub ready_since: Option<u64>,
    /// That read is a probe ([`Act::Probe`]), and when the last one went.
    pub probing: bool,
    pub probed_ms: Option<u64>,
    pub ended_ms: Option<u64>,
}

impl Sent {
    pub fn status(&self, now_ms: u64) -> PromptStatus {
        PromptStatus {
            prompt_id: self.id,
            key: self.key.clone(),
            pid: self.target_pid,
            age_ms: now_ms.saturating_sub(self.sent_ms),
            stage: self.stage.clone(),
        }
    }
}

/// What the window must do for a prompt, decided by [`PromptBook::tick`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    /// Wake the resting or asleep worker.
    Wake(AskId),
    /// Paste the prompt's text, bracketed, at the worker's prompt.
    Paste(AskId),
    /// Press Enter once in the worker's pane.
    Enter(AskId),
    /// Read the worker's last message off the window's thread, and hand it to
    /// [`PromptBook::replied`].
    ReadReply(AskId),
    /// The same read, once, for a worker back at rest that was never seen
    /// taking the prompt: a reply found finishes it, anything else changes
    /// nothing.
    Probe(AskId),
    /// The prompt reached a new stage: append it to the record.
    Record(AskId),
}

/// What a new prompt carries in.
#[derive(Clone, Debug, PartialEq)]
pub struct NewPrompt {
    pub id: AskId,
    pub sender_pane: u64,
    pub from: Option<SeatId>,
    pub to: Option<SeatId>,
    pub target_pid: u32,
    pub key: Option<String>,
    pub text: String,
    pub brief_len: usize,
    pub brief_sha256: String,
    pub now_ms: u64,
}

/// Every prompt the window holds, open and recently ended, in send order.
#[derive(Default, Debug)]
pub struct PromptBook {
    sent: VecDeque<Sent>,
}

/// What [`PromptBook::send`] did with a prompt.
#[derive(Clone, Debug, PartialEq)]
pub enum Taken {
    /// A new prompt, waiting for its worker.
    New(PromptStatus),
    /// The sender's key named an earlier prompt: that one, untouched.
    Again(PromptStatus),
}

impl PromptBook {
    pub fn get(&self, id: AskId) -> Option<&Sent> {
        self.sent.iter().find(|s| s.id == id)
    }

    fn get_mut(&mut self, id: AskId) -> Option<&mut Sent> {
        self.sent.iter_mut().find(|s| s.id == id)
    }

    /// A prompt the sender may read: only its own.
    pub fn of_sender(&self, id: AskId, sender_pane: u64) -> Option<&Sent> {
        self.get(id).filter(|s| s.sender_pane == sender_pane)
    }

    /// Every prompt still open.
    pub fn open(&self) -> impl Iterator<Item = &Sent> {
        self.sent.iter().filter(|s| s.stage.is_open())
    }

    /// Take a prompt the rules allowed. A key this sender used before names
    /// that prompt, which is returned as it is, so a retried call never types
    /// twice. One prompt in flight from a sender to a worker: a second while
    /// the first is open is refused.
    pub fn send(&mut self, p: NewPrompt) -> Result<Taken, Refusal> {
        if let Some(key) = &p.key {
            if let Some(s) = self
                .sent
                .iter()
                .find(|s| s.sender_pane == p.sender_pane && s.key.as_deref() == Some(key))
            {
                return Ok(Taken::Again(s.status(p.now_ms)));
            }
        }
        if self
            .open()
            .any(|s| s.sender_pane == p.sender_pane && s.target_pid == p.target_pid)
        {
            return Err(Refusal::PromptBusy);
        }
        let s = Sent {
            id: p.id,
            sender_pane: p.sender_pane,
            from: p.from,
            to: p.to,
            target_pid: p.target_pid,
            key: p.key,
            text: p.text,
            brief_len: p.brief_len,
            brief_sha256: p.brief_sha256,
            sent_ms: p.now_ms,
            stage: PromptStage::Waiting {
                worker: String::new(),
            },
            woke_ms: None,
            pasted_ms: None,
            before: Vec::new(),
            entered_ms: None,
            errored_ms: None,
            reading_reply: false,
            ready_since: None,
            probing: false,
            probed_ms: None,
            ended_ms: None,
        };
        let status = s.status(p.now_ms);
        self.sent.push_back(s);
        Ok(Taken::New(status))
    }

    /// The window could not do what an [`Act`] asked — the pane went between
    /// the decision and the write.
    pub fn fail(&mut self, id: AskId, why: &str, now_ms: u64) -> bool {
        match self.get_mut(id) {
            Some(s) if s.stage.is_open() => {
                end(s, failed(why), now_ms);
                true
            }
            _ => false,
        }
    }

    /// The worker's last message, read after it came back to rest. `Err` says
    /// why it is not known.
    pub fn replied(&mut self, id: AskId, reply: Result<String, String>, now_ms: u64) -> bool {
        let Some(s) = self.get_mut(id) else {
            return false;
        };
        if !s.reading_reply || !s.stage.is_open() {
            return false;
        }
        s.reading_reply = false;
        // A probe proves a turn only by finding the reply; anything else
        // leaves the prompt where it was, still waiting to be seen working.
        if std::mem::take(&mut s.probing) && reply.is_err() {
            return false;
        }
        let stage = match reply {
            Ok(text) => PromptStage::Finished {
                reply: Some(text),
                reply_unknown: None,
            },
            Err(why) => PromptStage::Finished {
                reply: None,
                reply_unknown: Some(why),
            },
        };
        end(s, stage, now_ms);
        true
    }

    /// A sender's pane closed: what it sent and has not been typed is dropped
    /// with it. A prompt already typed stays to the end, for the record.
    pub fn sender_gone(&mut self, sender_pane: u64, now_ms: u64) -> Vec<AskId> {
        let mut ended = Vec::new();
        for s in self.sent.iter_mut() {
            if s.sender_pane == sender_pane && s.stage.is_open() && !s.stage.is_typed() {
                end(s, failed("the pane that sent it closed"), now_ms);
                ended.push(s.id);
            }
        }
        ended
    }

    /// One look at every worker with an open prompt. `look` is asked once per
    /// worker, for its head prompt only, so a worker's queue moves one prompt
    /// at a time; the answer is what the window must do, in order.
    pub fn tick(&mut self, now_ms: u64, mut look: impl FnMut(&Sent) -> Worker) -> Vec<Act> {
        let mut acts = Vec::new();
        let mut seen_workers: Vec<u32> = Vec::new();
        for i in 0..self.sent.len() {
            let pid = self.sent[i].target_pid;
            if !self.sent[i].stage.is_open() || seen_workers.contains(&pid) {
                continue;
            }
            seen_workers.push(pid);
            let w = look(&self.sent[i]);
            let s = &mut self.sent[i];
            let before = s.stage.word();
            step(s, &w, now_ms, &mut acts);
            if s.stage.word() != before {
                acts.push(Act::Record(s.id));
            }
        }
        self.prune(now_ms);
        acts
    }

    fn prune(&mut self, now_ms: u64) {
        self.sent.retain(|s| {
            s.ended_ms
                .is_none_or(|t| now_ms.saturating_sub(t) < KEEP_ENDED_MS)
        });
        let mut ended = self.sent.iter().filter(|s| !s.stage.is_open()).count();
        while ended > KEEP_ENDED {
            let Some(i) = self.sent.iter().position(|s| !s.stage.is_open()) else {
                break;
            };
            self.sent.remove(i);
            ended -= 1;
        }
    }
}

fn failed(why: &str) -> PromptStage {
    PromptStage::Failed {
        why: why.to_string(),
    }
}

fn end(s: &mut Sent, stage: PromptStage, now_ms: u64) {
    s.stage = stage;
    s.ended_ms = Some(now_ms);
    s.reading_reply = false;
    s.probing = false;
}

/// One prompt's move on one tick.
fn step(s: &mut Sent, w: &Worker, now: u64, acts: &mut Vec<Act>) {
    // A probe that never answers must not hold the prompt open: past the
    // take-up limit it is dropped, and a late answer changes nothing.
    let past_take_up = s
        .entered_ms
        .is_some_and(|e| now.saturating_sub(e) > TAKE_UP_MS);
    if s.probing && past_take_up {
        s.probing = false;
        s.reading_reply = false;
    }
    if s.reading_reply {
        return;
    }
    if let Some(pid) = w.pid {
        s.target_pid = pid;
    }
    // At rest is a run of looks, not one: a streaming agent's screen can read
    // at rest between two chunks (rig run 4, Claude Code writing a story). The
    // run restarts on any look that is not at rest, and on any turn the
    // pane's own scan saw begin inside it, even one that began and ended
    // between two looks.
    if w.seen == Seen::Ready {
        let since = s.ready_since.get_or_insert(now);
        if w.turn_began_ms.is_some_and(|t| t >= *since) {
            *since = now;
        }
    } else {
        s.ready_since = None;
    }
    let settled = s
        .ready_since
        .is_some_and(|t| now.saturating_sub(t) >= SETTLE_MS);
    if w.seen == Seen::Gone {
        let why = if s.stage.is_typed() {
            "the worker's pane closed, or stopped being an agent, before it finished"
        } else {
            "the worker's pane closed, or stopped being an agent, before it was typed"
        };
        end(s, failed(why), now);
        return;
    }
    match s.stage.clone() {
        PromptStage::Waiting { .. } => {
            // A line erased, a pane made read-only or off-limits, MCP off:
            // nothing typed yet, so nothing will be.
            if let Err(r) = &w.allowed {
                end(s, failed(r.why()), now);
                return;
            }
            if now.saturating_sub(s.sent_ms) > WAIT_FOR_REST_MS {
                let why = format!(
                    "the worker did not come to rest at its prompt within ten minutes (it was {})",
                    w.label
                );
                end(s, failed(&why), now);
                return;
            }
            let quiet = w
                .person_key_ms
                .is_none_or(|t| now.saturating_sub(t) >= QUIET_MS);
            let worker = match w.seen {
                Seen::Ready if w.picker => "showing a picker for its person".to_string(),
                Seen::Ready if !quiet => "being typed in by its person".to_string(),
                Seen::Ready if !w.takes_paste => {
                    "at a prompt that does not take a paste".to_string()
                }
                Seen::Ready if !settled => "coming to rest".to_string(),
                Seen::Ready => {
                    s.before = w.rows.clone();
                    s.pasted_ms = Some(now);
                    s.stage = PromptStage::Typing;
                    acts.push(Act::Paste(s.id));
                    return;
                }
                Seen::Dormant => {
                    if s.woke_ms.is_none_or(|t| now.saturating_sub(t) >= REWAKE_MS) {
                        s.woke_ms = Some(now);
                        acts.push(Act::Wake(s.id));
                    }
                    format!("{}; waking it", w.label)
                }
                _ => w.label.clone(),
            };
            s.stage = PromptStage::Waiting { worker };
        }
        PromptStage::Typing => {
            let since = now.saturating_sub(s.pasted_ms.unwrap_or(now));
            if since < ENTER_AFTER_MS {
                return;
            }
            // The line is read again before Enter, not only before the paste:
            // a line erased in the second between them means no Enter, so no
            // turn is ever sent along a line that is gone (cross-provider
            // review, 2026-10-27).
            if let Err(r) = &w.allowed {
                end(
                    s,
                    failed(&format!(
                        "pasted, not sent: {} before Enter. The words may be sitting at its prompt.",
                        r.why()
                    )),
                    now,
                );
                return;
            }
            let hold = match w.seen {
                Seen::Working => Some("it started working on something else before Enter"),
                Seen::Blocked => Some("a prompt for its person came up before Enter"),
                Seen::Errored => Some("an error came up on its screen before Enter"),
                Seen::Dormant => Some("it went to rest before Enter"),
                Seen::Ready | Seen::Unreadable | Seen::Gone => None,
            };
            let typed_over = w
                .person_key_ms
                .is_some_and(|t| t >= s.pasted_ms.unwrap_or(now));
            let hold = hold.or(typed_over.then_some("its person typed in the pane before Enter"));
            if let Some(why) = hold {
                end(
                    s,
                    failed(&format!(
                        "pasted, not sent: {why}. The words may be sitting at its prompt; the next prompt to this pane is pasted after them, and Enter sends both."
                    )),
                    now,
                );
                return;
            }
            if shows_paste(&s.before, &w.rows, s.id) {
                s.entered_ms = Some(now);
                s.stage = PromptStage::Delivered;
                acts.push(Act::Enter(s.id));
            } else if since >= SHOW_PASTE_MS {
                end(
                    s,
                    failed(
                        "pasted, not sent: the words never showed at the worker's prompt, so Enter \
                         was not pressed. They may be sitting at its prompt; the next prompt to \
                         this pane is pasted after them, and Enter sends both.",
                    ),
                    now,
                );
            }
        }
        PromptStage::Delivered | PromptStage::Working { .. } => {
            let entered = s.entered_ms.unwrap_or(now);
            // Taken: seen working on a look, or a turn the pane's own scan saw
            // begin after Enter — one that began and ended between two looks.
            let worked = matches!(s.stage, PromptStage::Working { .. })
                || w.turn_began_ms.is_some_and(|t| t >= entered);
            if w.seen == Seen::Errored {
                let since = *s.errored_ms.get_or_insert(now);
                if now.saturating_sub(since) >= ERROR_ENDS_MS {
                    end(
                        s,
                        failed("an error stayed on the worker's screen for a minute (a rate limit or an API error); it may not have finished"),
                        now,
                    );
                    return;
                }
            } else {
                s.errored_ms = None;
            }
            match w.seen {
                Seen::Working | Seen::Blocked | Seen::Errored => {
                    s.stage = PromptStage::Working {
                        worker: w.label.clone(),
                    };
                }
                Seen::Ready if worked && settled => {
                    s.reading_reply = true;
                    acts.push(Act::ReadReply(s.id));
                }
                Seen::Ready if worked => {
                    s.stage = PromptStage::Working {
                        worker: "coming to rest".to_string(),
                    };
                }
                Seen::Dormant if worked => {
                    end(
                        s,
                        failed(
                            "the worker was put to rest or ended before it came back to its prompt",
                        ),
                        now,
                    );
                }
                // At rest and never seen taking it: its own transcript can
                // still show the turn happened, so look there now and then.
                Seen::Ready
                    if !worked
                        && settled
                        && now.saturating_sub(entered) >= PROBE_AFTER_MS
                        && s.probed_ms
                            .is_none_or(|t| now.saturating_sub(t) >= PROBE_AFTER_MS)
                        && now.saturating_sub(entered) <= TAKE_UP_MS =>
                {
                    s.probed_ms = Some(now);
                    s.reading_reply = true;
                    s.probing = true;
                    acts.push(Act::Probe(s.id));
                }
                _ if !worked && now.saturating_sub(entered) > TAKE_UP_MS => {
                    end(
                        s,
                        failed("Enter was pressed, but the worker was never seen working on it; look at the pane"),
                        now,
                    );
                }
                _ if worked => {
                    s.stage = PromptStage::Working {
                        worker: w.label.clone(),
                    };
                }
                _ => {}
            }
        }
        PromptStage::Finished { .. } | PromptStage::Failed { .. } => {}
    }
}

/// Whether the paste shows on the worker's screen: some row near the bottom
/// that was not there before the paste carries this prompt's own number, or
/// one more of the placeholders the program draws in place of a long paste
/// than the screen held before. A number in an unchanged row is an earlier
/// prompt in its history and proves nothing. A placeholder is counted rather
/// than looked for in a changed row, because an earlier prompt's unsent paste
/// can sit in the composer and change shape (rewrap, a cursor) without this
/// paste having landed; only a new one is this paste. A count that cannot
/// grow, because the screen scrolled one away, means no Enter: the safe side.
pub fn shows_paste(before: &[String], after: &[String], id: AskId) -> bool {
    let tag = tag(id);
    let placeholders = |rows: &[String]| -> usize {
        rows.iter()
            .map(|row| {
                PLACEHOLDERS
                    .iter()
                    .map(|p| row.matches(p).count())
                    .sum::<usize>()
            })
            .sum()
    };
    after
        .iter()
        .filter(|row| !before.contains(row))
        .any(|row| row.contains(&tag))
        || placeholders(after) > placeholders(before)
}

/// How the agent programs draw a paste too long to show: Claude Code's
/// `[Pasted text #1 +40 lines]`, Codex's `[Pasted Content 1204 chars]`.
const PLACEHOLDERS: [&str; 2] = ["[Pasted text", "[Pasted Content"];

/// The prompt's number as its header writes it, the string looked for on the
/// worker's screen and in its transcript. It ends at the comma, so prompt 17
/// is never found inside prompt 171.
pub fn tag(id: AskId) -> String {
    format!("Threshing Floor prompt {},", id.0)
}

/// The words as they are pasted: every control character but newline and tab
/// taken out, so nothing in a brief can close the paste's bracket or press a
/// key, and line endings made newlines. Trailing whitespace goes: Enter is
/// the window's to press.
pub fn clean(text: &str) -> String {
    let unified = text.replace("\r\n", "\n").replace('\r', "\n");
    let kept: String = unified
        .chars()
        .filter(|c| matches!(c, '\n' | '\t') || !c.is_control())
        .collect();
    kept.trim_end().to_string()
}

/// A name for the header, one line and short: a title is another program's
/// words.
fn one_line(name: &str, max: usize) -> String {
    let flat: String = clean(name)
        .chars()
        .map(|c| if c == '\n' || c == '\t' { ' ' } else { c })
        .collect();
    let flat = flat.trim();
    if flat.chars().count() <= max {
        flat.to_string()
    } else {
        let cut: String = flat.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}\u{2026}")
    }
}

/// The line the worker reads first: that this turn came from another agent,
/// which one, by what right, and what happens to its answer. `returns` is
/// whether the worker's last message will be handed back — only when its
/// conversation is proven its own.
pub fn header(id: AskId, sender: &str, sender_pid: Option<u32>, returns: bool) -> String {
    let who = match sender_pid {
        Some(pid) => format!("{} (pid {pid})", one_line(sender, 80)),
        None => one_line(sender, 80),
    };
    let back = if returns {
        "Your last message this turn is returned to the sender."
    } else {
        "The sender is told when you finish."
    };
    format!(
        "[{} from {who}: a person drew a prompt line from that agent to you. {back}]",
        tag(id)
    )
}

/// The header, a blank line, and the brief.
pub fn compose(header: &str, brief: &str) -> String {
    format!("{header}\n\n{brief}")
}

/// A sender's key, checked: short, one line.
pub fn key_of(raw: Option<&str>) -> Result<Option<String>, &'static str> {
    match raw.map(str::trim) {
        None | Some("") => Ok(None),
        Some(k) if k.chars().count() > KEY_MAX_CHARS => Err("`key` is longer than 64 characters"),
        Some(k) if k.chars().any(char::is_control) => Err("`key` must be one line of text"),
        Some(k) => Ok(Some(k.to_string())),
    }
}

/// Milliseconds since the epoch, the clock every stage is stamped in.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The two things a prompt ever types, and nothing else can be named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keys<'a> {
    /// The prompt's words, as one bracketed paste.
    Paste(&'a str),
    /// One carriage return.
    Enter,
}

/// The bytes of a prompt's paste, or `None` when nothing may be written: a
/// prompt that has not asked for bracketed paste (every newline would be an
/// Enter there), or no words left once they are cleaned. Cleaned again here,
/// whatever the caller did, so the bracket can only be closed by its own
/// closer.
pub fn paste(text: &str, bracketed: bool) -> Option<Vec<u8>> {
    if !bracketed {
        return None;
    }
    let clean = clean(text);
    if clean.is_empty() {
        return None;
    }
    Some([b"\x1b[200~", clean.as_bytes(), b"\x1b[201~"].concat())
}

/// How much of a transcript's end is read for a reply. A turn that ran many
/// tools writes a lot after the prompt; past this, the prompt is not found and
/// the reply is said to be unknown rather than guessed.
pub const REPLY_TAIL: u64 = 8 * 1024 * 1024;

/// The worker's last message after this prompt, from a chunk of its own
/// transcript, Claude Code's or Codex's. The prompt's number must be in a
/// line before the message: that is what proves the file is the conversation
/// that took this prompt, and that the message answers it rather than an
/// earlier turn.
pub fn last_reply(jsonl: &str, id: AskId) -> Result<String, &'static str> {
    let tag = tag(id);
    let mut lines: Vec<&str> = jsonl.lines().collect();
    // The prompt's own turn, not any line that repeats it: Claude Code writes
    // a `last-prompt` entry carrying the prompt's words after the reply, and
    // anchoring there found "no message after" a prompt it had answered.
    let Some(at) = lines.iter().rposition(|l| {
        l.contains(&tag)
            && serde_json::from_str::<serde_json::Value>(l).is_ok_and(|v| is_user_turn(&v))
    }) else {
        return Err(NOT_SHOWN);
    };
    lines.drain(..=at);
    for line in lines.iter().rev() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(text) = reply_text(&v) {
            return Ok(text);
        }
    }
    Err(NOTHING_AFTER)
}

/// A turn the person, or a prompt, typed: Claude Code's `user` entry whose
/// content is words rather than a tool's result, or Codex's `response_item`
/// message from the user.
fn is_user_turn(v: &serde_json::Value) -> bool {
    match v["type"].as_str() {
        Some("user") => match &v["message"]["content"] {
            serde_json::Value::String(_) => true,
            serde_json::Value::Array(blocks) => blocks.iter().any(|b| b["type"] == "text"),
            _ => false,
        },
        Some("response_item") => {
            v["payload"]["type"] == "message" && v["payload"]["role"] == "user"
        }
        _ => false,
    }
}

/// An assistant message's words: Claude Code's `assistant` entry with `text`
/// blocks, or Codex's `response_item` message from the assistant with
/// `output_text` blocks. Tool calls carry no words and are passed over.
fn reply_text(v: &serde_json::Value) -> Option<String> {
    let blocks = match v["type"].as_str()? {
        "assistant" => v["message"]["content"].as_array()?,
        "response_item"
            if v["payload"]["type"].as_str() == Some("message")
                && v["payload"]["role"].as_str() == Some("assistant") =>
        {
            v["payload"]["content"].as_array()?
        }
        _ => return None,
    };
    let words: Vec<&str> = blocks
        .iter()
        .filter(|b| matches!(b["type"].as_str(), Some("text" | "output_text")))
        .filter_map(|b| b["text"].as_str())
        .collect();
    let text = words.join("\n");
    (!text.trim().is_empty()).then(|| text.trim().to_string())
}

/// How many times a reply is looked for, [`REPLY_RETRY_MS`] apart, while the
/// program may still be writing it.
pub const REPLY_TRIES: u32 = 10;
pub const REPLY_RETRY_MS: u64 = 500;

/// What a reply is unknown for when the worker's pane names no conversation.
pub const NO_CONVERSATION: &str = "its pane names no conversation that could be read";

/// Whether a reply that was not found may yet be: the transcript has not
/// caught up with the screen. A file that cannot be opened, or a pane with no
/// conversation, will not change in a few seconds.
pub fn worth_another_look(why: &str) -> bool {
    why == NOT_SHOWN || why == NOTHING_AFTER
}

const NOT_SHOWN: &str = "its conversation does not show this prompt";
const NOTHING_AFTER: &str = "it wrote no message after this prompt";

/// [`last_reply`] from the end of the transcript at `path`.
pub fn read_reply(path: &std::path::Path, id: AskId) -> Result<String, String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path)
        .map_err(|e| format!("its conversation could not be opened: {e}"))?;
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    f.seek(SeekFrom::Start(len.saturating_sub(REPLY_TAIL)))
        .map_err(|e| format!("its conversation could not be read: {e}"))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)
        .map_err(|e| format!("its conversation could not be read: {e}"))?;
    last_reply(&String::from_utf8_lossy(&buf), id).map_err(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_800_000_000_000;

    fn new(id: u64, sender: u64, pid: u32, key: Option<&str>) -> NewPrompt {
        NewPrompt {
            id: AskId(id),
            sender_pane: sender,
            from: Some(SeatId(1)),
            to: Some(SeatId(2)),
            target_pid: pid,
            key: key.map(str::to_string),
            text: compose(
                &header(AskId(id), "planner", Some(10), true),
                "do the thing",
            ),
            brief_len: 12,
            brief_sha256: "x".into(),
            now_ms: T0,
        }
    }

    /// The first look after the pause before Enter.
    const E: u64 = ENTER_AFTER_MS + 200;

    fn worker(seen: Seen) -> Worker {
        Worker {
            seen,
            label: format!("{seen:?}").to_lowercase(),
            takes_paste: true,
            picker: false,
            person_key_ms: None,
            turn_began_ms: None,
            rows: vec!["\u{2500}".repeat(20), "> ".into(), "? for shortcuts".into()],
            allowed: Ok(()),
            pid: None,
        }
    }

    fn with_rows(mut w: Worker, rows: &[&str]) -> Worker {
        w.rows = rows.iter().map(|r| r.to_string()).collect();
        w
    }

    fn stage(book: &PromptBook, id: u64) -> PromptStage {
        book.get(AskId(id)).unwrap().stage.clone()
    }

    /// Paste at rest, Enter once the paste shows, working, back at rest, the
    /// reply read: the whole road, one act at a time.
    #[test]
    fn a_prompt_goes_paste_enter_working_finished() {
        let mut b = PromptBook::default();
        assert!(matches!(b.send(new(7, 1, 500, None)), Ok(Taken::New(_))));
        // One look at rest is not rest: the first starts the run.
        assert_eq!(b.tick(T0 + 10, |_| worker(Seen::Ready)), vec![]);
        assert_eq!(
            stage(&b, 7),
            PromptStage::Waiting {
                worker: "coming to rest".into()
            }
        );
        let acts = b.tick(T0 + 10 + SETTLE_MS, |_| worker(Seen::Ready));
        assert_eq!(acts, vec![Act::Paste(AskId(7)), Act::Record(AskId(7))]);
        assert_eq!(stage(&b, 7), PromptStage::Typing);
        let t = T0 + 10 + SETTLE_MS;
        // Too soon to look for the paste.
        assert_eq!(b.tick(t + 100, |_| worker(Seen::Ready)), vec![]);
        let pasted = with_rows(
            worker(Seen::Unreadable),
            &["> [Threshing Floor prompt 7, from planner"],
        );
        let acts = b.tick(t + E, |_| pasted.clone());
        assert_eq!(acts, vec![Act::Enter(AskId(7)), Act::Record(AskId(7))]);
        let acts = b.tick(t + E + 1_000, |_| worker(Seen::Working));
        assert_eq!(acts, vec![Act::Record(AskId(7))]);
        assert!(matches!(stage(&b, 7), PromptStage::Working { .. }));
        // Back at rest: settled first, then read, so a pause in a streaming
        // turn is never taken for its end.
        assert_eq!(b.tick(t + 9_000, |_| worker(Seen::Ready)), vec![]);
        assert_eq!(b.tick(t + 9_500, |_| worker(Seen::Working)), vec![]);
        assert_eq!(b.tick(t + 10_000, |_| worker(Seen::Ready)), vec![]);
        let acts = b.tick(t + 10_000 + SETTLE_MS, |_| worker(Seen::Ready));
        assert_eq!(acts, vec![Act::ReadReply(AskId(7))]);
        // While the reply is read the prompt is still open, and nothing moves.
        assert_eq!(
            b.tick(t + 10_000 + SETTLE_MS + 500, |_| worker(Seen::Gone)),
            vec![]
        );
        assert!(b.replied(AskId(7), Ok("done".into()), t + 10_000 + SETTLE_MS + 600));
        assert_eq!(
            stage(&b, 7),
            PromptStage::Finished {
                reply: Some("done".into()),
                reply_unknown: None
            }
        );
    }

    /// Nothing is typed into a worker that is not at its own prompt at rest.
    #[test]
    fn nothing_is_typed_unless_the_worker_is_at_rest() {
        for seen in [
            Seen::Working,
            Seen::Blocked,
            Seen::Errored,
            Seen::Unreadable,
        ] {
            let mut b = PromptBook::default();
            b.send(new(1, 1, 500, None)).unwrap();
            let acts = b.tick(T0 + 10, |_| worker(seen));
            assert!(
                !acts
                    .iter()
                    .any(|a| matches!(a, Act::Paste(_) | Act::Enter(_))),
                "{seen:?} was typed into: {acts:?}"
            );
            assert!(
                matches!(stage(&b, 1), PromptStage::Waiting { .. }),
                "{seen:?}"
            );
        }
    }

    /// At rest is not enough: a picker, a person mid-line, or a prompt that
    /// never asked for bracketed paste each hold it back.
    #[test]
    fn rest_with_a_person_or_a_picker_or_no_paste_waits() {
        let cases: [(&str, fn(&mut Worker)); 3] = [
            ("picker", |w| w.picker = true),
            ("typing", |w| w.person_key_ms = Some(T0 + 5)),
            ("no paste", |w| w.takes_paste = false),
        ];
        for (name, tweak) in cases {
            let mut b = PromptBook::default();
            b.send(new(1, 1, 500, None)).unwrap();
            let mut w = worker(Seen::Ready);
            tweak(&mut w);
            b.tick(T0 + 10 - SETTLE_MS, |_| worker(Seen::Ready));
            let acts = b.tick(T0 + 10, |_| w.clone());
            assert!(
                acts.iter().all(|a| !matches!(a, Act::Paste(_))),
                "{name}: {acts:?}"
            );
        }
        // The person's key ages out after the quiet period.
        let mut b = PromptBook::default();
        b.send(new(1, 1, 500, None)).unwrap();
        let mut w = worker(Seen::Ready);
        w.person_key_ms = Some(T0);
        b.tick(T0 + QUIET_MS - SETTLE_MS, |_| w.clone());
        assert!(!b
            .tick(T0 + QUIET_MS - 1, |_| w.clone())
            .contains(&Act::Paste(AskId(1))));
        assert!(b
            .tick(T0 + QUIET_MS, |_| w.clone())
            .contains(&Act::Paste(AskId(1))));
    }

    /// No proof the paste landed, no Enter: the prompt fails and says where
    /// its words may be.
    #[test]
    fn enter_waits_for_proof_and_gives_up_without_it() {
        let mut b = PromptBook::default();
        b.send(new(3, 1, 500, None)).unwrap();
        b.tick(T0 - SETTLE_MS, |_| worker(Seen::Ready));
        b.tick(T0, |_| worker(Seen::Ready));
        // The screen did not change: no Enter yet, then none ever.
        let acts = b.tick(T0 + 1_000, |_| worker(Seen::Ready));
        assert!(acts.is_empty(), "{acts:?}");
        let acts = b.tick(T0 + SHOW_PASTE_MS, |_| worker(Seen::Ready));
        assert_eq!(acts, vec![Act::Record(AskId(3))]);
        let PromptStage::Failed { why } = stage(&b, 3) else {
            panic!("not failed");
        };
        assert!(why.contains("Enter was not pressed"), "{why}");
    }

    /// A tag already in the worker's history (an unchanged row) is an earlier
    /// prompt and proves nothing; the same tag on a new row, or a placeholder
    /// on a new row, does.
    #[test]
    fn only_a_changed_row_proves_the_paste() {
        let before = vec![
            "[Threshing Floor prompt 9, from x".to_string(),
            "> ".to_string(),
        ];
        assert!(!shows_paste(&before, &before, AskId(9)));
        let after = vec![
            before[0].clone(),
            "> [Threshing Floor prompt 9, from x".into(),
        ];
        assert!(shows_paste(&before, &after, AskId(9)));
        let after = vec![before[0].clone(), "> [Pasted text #1 +40 lines]".into()];
        assert!(shows_paste(&before, &after, AskId(9)));
        let after = vec![
            before[0].clone(),
            "\u{203a} [Pasted Content 1204 chars]".into(),
        ];
        assert!(shows_paste(&before, &after, AskId(9)));
        // Another prompt's number is not this one's.
        let after = vec![
            before[0].clone(),
            "> [Threshing Floor prompt 8, from x".into(),
        ];
        assert!(!shows_paste(&before, &after, AskId(9)));
        // An earlier prompt's unsent paste changing shape is not this paste;
        // a second placeholder beside it is.
        let before = vec!["> [Pasted text #1 +40 lines]".to_string()];
        let reshaped = vec!["> [Pasted text #1 +40 lines]\u{2588}".to_string()];
        assert!(!shows_paste(&before, &reshaped, AskId(9)));
        let joined = vec!["> [Pasted text #1 +40 lines][Pasted text #2 +12 lines]".to_string()];
        assert!(shows_paste(&before, &joined, AskId(9)));
    }

    /// Something that comes up between the paste and Enter stops Enter: a
    /// dialog is never answered by it.
    #[test]
    fn a_dialog_between_paste_and_enter_stops_enter() {
        for seen in [Seen::Blocked, Seen::Working, Seen::Errored, Seen::Dormant] {
            let mut b = PromptBook::default();
            b.send(new(4, 1, 500, None)).unwrap();
            b.tick(T0 - SETTLE_MS, |_| worker(Seen::Ready));
            b.tick(T0, |_| worker(Seen::Ready));
            let pasted = with_rows(worker(seen), &["> [Threshing Floor prompt 4, from x"]);
            let acts = b.tick(T0 + E, |_| pasted.clone());
            assert!(!acts.contains(&Act::Enter(AskId(4))), "{seen:?}");
            assert_eq!(stage(&b, 4).word(), "failed", "{seen:?}");
        }
        // A person's key after the paste stops it too.
        let mut b = PromptBook::default();
        b.send(new(4, 1, 500, None)).unwrap();
        b.tick(T0 - SETTLE_MS, |_| worker(Seen::Ready));
        b.tick(T0, |_| worker(Seen::Ready));
        let mut w = with_rows(
            worker(Seen::Ready),
            &["> [Threshing Floor prompt 4, from x"],
        );
        w.person_key_ms = Some(T0 + 200);
        assert!(!b
            .tick(T0 + E, |_| w.clone())
            .contains(&Act::Enter(AskId(4))));
    }

    /// Words a failed prompt left at the worker's prompt do not hold the
    /// worker: the next prompt is pasted after them once the worker settles,
    /// and Enter sends both. Parker, 2026-10-27: "pasting the next prompt on
    /// top feels fine to me."
    #[test]
    fn a_paste_left_unsent_is_followed_by_the_next_prompt() {
        let mut b = PromptBook::default();
        b.send(new(1, 1, 500, None)).unwrap();
        b.tick(T0 - SETTLE_MS, |_| worker(Seen::Ready));
        b.tick(T0, |_| worker(Seen::Ready));
        let blocked = with_rows(worker(Seen::Blocked), &["> [Threshing Floor prompt 1, x"]);
        b.tick(T0 + ENTER_AFTER_MS + 200, |_| blocked.clone());
        let PromptStage::Failed { why } = stage(&b, 1) else {
            panic!("not failed");
        };
        assert!(why.contains("pasted after them"), "{why}");
        b.send(new(2, 2, 500, None)).unwrap();
        let t = T0 + 10_000;
        b.tick(t, |_| worker(Seen::Ready));
        assert!(!b
            .tick(t + SETTLE_MS - 1, |_| worker(Seen::Ready))
            .contains(&Act::Paste(AskId(2))));
        assert!(b
            .tick(t + SETTLE_MS, |_| worker(Seen::Ready))
            .contains(&Act::Paste(AskId(2))));
    }

    /// Enter is pressed once. Whatever the worker shows after, no second Enter
    /// — and the probes it gets meanwhile, never answered here, do not hold
    /// it open past the take-up limit.
    #[test]
    fn enter_is_pressed_once() {
        let mut b = PromptBook::default();
        b.send(new(5, 1, 500, None)).unwrap();
        b.tick(T0 - SETTLE_MS, |_| worker(Seen::Ready));
        b.tick(T0, |_| worker(Seen::Ready));
        let pasted = with_rows(
            worker(Seen::Ready),
            &["> [Threshing Floor prompt 5, from x"],
        );
        let mut enters = 0;
        for t in (600..TAKE_UP_MS + 5_000).step_by(1_000) {
            enters += b
                .tick(T0 + t, |_| pasted.clone())
                .iter()
                .filter(|a| matches!(a, Act::Enter(_)))
                .count();
        }
        assert_eq!(enters, 1);
        let PromptStage::Failed { why } = stage(&b, 5) else {
            panic!("not failed");
        };
        assert!(why.contains("never seen working"), "{why}");
    }

    /// A turn the pane's own scan saw begin after Enter counts, even if no
    /// look caught it working; and a worker back at rest that was never seen
    /// working is finished by its transcript, or by nothing else.
    #[test]
    fn a_turn_between_looks_is_still_seen() {
        let entered = |b: &mut PromptBook| {
            b.send(new(1, 1, 500, None)).unwrap();
            b.tick(T0 - SETTLE_MS, |_| worker(Seen::Ready));
            b.tick(T0, |_| worker(Seen::Ready));
            b.tick(T0 + E, |_| {
                with_rows(worker(Seen::Ready), &["> [Threshing Floor prompt 1, f"])
            });
        };
        let mut b = PromptBook::default();
        entered(&mut b);
        let mut w = worker(Seen::Ready);
        w.turn_began_ms = Some(T0 + E + 100);
        // Taken, but a turn that began in the last SETTLE_MS is not over.
        assert_eq!(
            b.tick(T0 + E + 1_400, |_| w.clone()),
            vec![Act::Record(AskId(1))]
        );
        assert_eq!(
            stage(&b, 1),
            PromptStage::Working {
                worker: "coming to rest".into()
            }
        );
        assert!(b.tick(T0 + E + 2_100, |_| w.clone()).is_empty());
        assert_eq!(
            b.tick(T0 + E + 2_100 + SETTLE_MS, |_| w.clone()),
            vec![Act::ReadReply(AskId(1))]
        );
        // A turn that began before Enter is somebody else's.
        let mut b = PromptBook::default();
        entered(&mut b);
        w.turn_began_ms = Some(T0 - SETTLE_MS - 3_000);
        assert!(b.tick(T0 + E + 1_400, |_| w.clone()).is_empty());
        // The probe: after PROBE_AFTER_MS at rest, once per PROBE_AFTER_MS.
        let at = T0 + E + PROBE_AFTER_MS;
        assert_eq!(b.tick(at, |_| w.clone()), vec![Act::Probe(AskId(1))]);
        assert!(
            b.tick(at + 500, |_| w.clone()).is_empty(),
            "one probe at a time"
        );
        // Not found: nothing changes, and the next probe waits its turn.
        assert!(!b.replied(AskId(1), Err(NOTHING_AFTER.into()), at + 600));
        assert_eq!(stage(&b, 1), PromptStage::Delivered);
        assert!(b.tick(at + 1_000, |_| w.clone()).is_empty());
        assert_eq!(
            b.tick(at + PROBE_AFTER_MS, |_| w.clone()),
            vec![Act::Probe(AskId(1))]
        );
        // Found: finished, with the reply.
        assert!(b.replied(AskId(1), Ok("done".into()), at + PROBE_AFTER_MS + 100));
        assert_eq!(
            stage(&b, 1),
            PromptStage::Finished {
                reply: Some("done".into()),
                reply_unknown: None
            }
        );
    }

    /// A resting or asleep worker is woken, once a minute at most, and typed
    /// into when it is back at rest.
    #[test]
    fn a_dormant_worker_is_woken_then_typed() {
        let mut b = PromptBook::default();
        b.send(new(6, 1, 500, None)).unwrap();
        assert!(b
            .tick(T0, |_| worker(Seen::Dormant))
            .contains(&Act::Wake(AskId(6))));
        assert!(!b
            .tick(T0 + 1_000, |_| worker(Seen::Dormant))
            .contains(&Act::Wake(AskId(6))));
        assert!(b
            .tick(T0 + REWAKE_MS, |_| worker(Seen::Dormant))
            .contains(&Act::Wake(AskId(6))));
        b.tick(T0 + REWAKE_MS + 5_000, |_| worker(Seen::Ready));
        assert!(b
            .tick(T0 + REWAKE_MS + 5_000 + SETTLE_MS, |_| worker(Seen::Ready))
            .contains(&Act::Paste(AskId(6))));
    }

    /// Ten minutes without rest ends the prompt, naming what the worker was
    /// doing.
    #[test]
    fn waiting_ends_after_ten_minutes() {
        let mut b = PromptBook::default();
        b.send(new(8, 1, 500, None)).unwrap();
        b.tick(T0 + WAIT_FOR_REST_MS, |_| worker(Seen::Working));
        assert_eq!(stage(&b, 8).word(), "waiting");
        b.tick(T0 + WAIT_FOR_REST_MS + 1, |_| worker(Seen::Working));
        let PromptStage::Failed { why } = stage(&b, 8) else {
            panic!("not failed");
        };
        assert!(
            why.contains("ten minutes") && why.contains("working"),
            "{why}"
        );
    }

    /// A line erased, or the pane made read-only, stops a prompt not yet
    /// sent, and that includes the second between the paste and Enter: no
    /// Enter is pressed along a line that is gone. Once Enter is pressed the
    /// worker has the turn, and it runs to its end.
    ///
    /// Mutation-tested: dropping the recheck before Enter fails this.
    #[test]
    fn a_revoked_line_stops_only_what_is_not_typed() {
        let mut b = PromptBook::default();
        b.send(new(9, 1, 500, None)).unwrap();
        let mut w = worker(Seen::Working);
        w.allowed = Err(Refusal::NoPromptLine);
        b.tick(T0 + 10, |_| w.clone());
        assert_eq!(stage(&b, 9).word(), "failed");

        let mut b = PromptBook::default();
        b.send(new(9, 1, 500, None)).unwrap();
        b.tick(T0 - SETTLE_MS, |_| worker(Seen::Ready));
        b.tick(T0, |_| worker(Seen::Ready));
        let mut w = with_rows(
            worker(Seen::Ready),
            &["> [Threshing Floor prompt 9, from x"],
        );
        w.allowed = Err(Refusal::NoPromptLine);
        assert!(!b
            .tick(T0 + E, |_| w.clone())
            .contains(&Act::Enter(AskId(9))));
        let PromptStage::Failed { why } = stage(&b, 9) else {
            panic!("not failed");
        };
        assert!(why.starts_with("pasted, not sent"), "{why}");

        // Erased after Enter: the turn is the worker's, and it finishes.
        let mut b = PromptBook::default();
        b.send(new(9, 1, 500, None)).unwrap();
        b.tick(T0 - SETTLE_MS, |_| worker(Seen::Ready));
        b.tick(T0, |_| worker(Seen::Ready));
        let pasted = with_rows(
            worker(Seen::Ready),
            &["> [Threshing Floor prompt 9, from x"],
        );
        assert!(b
            .tick(T0 + E, |_| pasted.clone())
            .contains(&Act::Enter(AskId(9))));
        let mut w = worker(Seen::Working);
        w.allowed = Err(Refusal::NoPromptLine);
        b.tick(T0 + E + 1_000, |_| w.clone());
        assert!(stage(&b, 9).is_open(), "{:?}", stage(&b, 9));
    }

    /// The same key from the same sender names the first prompt; another
    /// sender's same key is its own.
    #[test]
    fn a_key_makes_a_retry_safe() {
        let mut b = PromptBook::default();
        assert!(matches!(
            b.send(new(1, 1, 500, Some("k"))),
            Ok(Taken::New(_))
        ));
        let again = b.send(new(2, 1, 500, Some("k"))).unwrap();
        assert!(matches!(again, Taken::Again(s) if s.prompt_id == AskId(1)));
        assert!(b.get(AskId(2)).is_none());
        // Ended, the key still names it.
        b.fail(AskId(1), "x", T0);
        assert!(matches!(
            b.send(new(3, 1, 500, Some("k"))),
            Ok(Taken::Again(_))
        ));
        assert!(matches!(
            b.send(new(4, 2, 501, Some("k"))),
            Ok(Taken::New(_))
        ));
    }

    /// One in flight from a sender to a worker; another worker is fine.
    #[test]
    fn one_prompt_in_flight_per_sender_and_worker() {
        let mut b = PromptBook::default();
        b.send(new(1, 1, 500, None)).unwrap();
        assert_eq!(b.send(new(2, 1, 500, None)), Err(Refusal::PromptBusy));
        assert!(b.send(new(3, 1, 501, None)).is_ok());
        b.fail(AskId(1), "x", T0);
        assert!(b.send(new(4, 1, 500, None)).is_ok());
    }

    /// Two senders to one worker go in send order: the second is not looked
    /// at, so not typed, until the first has ended.
    #[test]
    fn a_worker_takes_its_prompts_one_at_a_time() {
        let mut b = PromptBook::default();
        b.send(new(1, 1, 500, None)).unwrap();
        b.send(new(2, 2, 500, None)).unwrap();
        let mut looked = Vec::new();
        let acts = b.tick(T0, |s| {
            looked.push(s.id);
            worker(Seen::Ready)
        });
        assert_eq!(looked, vec![AskId(1)]);
        assert!(!acts.contains(&Act::Paste(AskId(2))));
        b.fail(AskId(1), "x", T0);
        assert!(!b
            .tick(T0 + 10, |_| worker(Seen::Ready))
            .contains(&Act::Paste(AskId(2))));
        assert!(b
            .tick(T0 + 10 + SETTLE_MS, |_| worker(Seen::Ready))
            .contains(&Act::Paste(AskId(2))));
    }

    /// A worker that leaves while working ends the prompt; one that comes to
    /// rest after working has its reply read, never twice.
    #[test]
    fn gone_or_dormant_while_working_fails() {
        let typed = |b: &mut PromptBook| {
            b.send(new(1, 1, 500, None)).unwrap();
            b.tick(T0 - SETTLE_MS, |_| worker(Seen::Ready));
            b.tick(T0, |_| worker(Seen::Ready));
            b.tick(T0 + E, |_| {
                with_rows(worker(Seen::Ready), &["> [Threshing Floor prompt 1, f"])
            });
            b.tick(T0 + E + 1_000, |_| worker(Seen::Working));
        };
        let mut b = PromptBook::default();
        typed(&mut b);
        b.tick(T0 + E + 1_400, |_| worker(Seen::Gone));
        assert_eq!(stage(&b, 1).word(), "failed");
        let mut b = PromptBook::default();
        typed(&mut b);
        b.tick(T0 + E + 1_400, |_| worker(Seen::Dormant));
        assert_eq!(stage(&b, 1).word(), "failed");
        let mut b = PromptBook::default();
        typed(&mut b);
        assert_eq!(b.tick(T0 + E + 1_400, |_| worker(Seen::Ready)), vec![]);
        assert_eq!(
            b.tick(T0 + E + 1_400 + SETTLE_MS, |_| worker(Seen::Ready)),
            vec![Act::ReadReply(AskId(1))]
        );
        assert_eq!(
            b.tick(T0 + E + 1_400 + SETTLE_MS + 500, |_| worker(Seen::Ready)),
            vec![]
        );
        assert!(b.replied(
            AskId(1),
            Err("not proven".into()),
            T0 + E + 1_400 + SETTLE_MS + 600
        ));
        assert!(!b.replied(
            AskId(1),
            Ok("late".into()),
            T0 + E + 1_400 + SETTLE_MS + 700
        ));
    }

    /// An error that clears is weather; one that stays a minute ends it.
    #[test]
    fn an_error_that_stays_ends_the_prompt() {
        let mut b = PromptBook::default();
        b.send(new(1, 1, 500, None)).unwrap();
        b.tick(T0 - SETTLE_MS, |_| worker(Seen::Ready));
        b.tick(T0, |_| worker(Seen::Ready));
        b.tick(T0 + E, |_| {
            with_rows(worker(Seen::Ready), &["> [Threshing Floor prompt 1, f"])
        });
        b.tick(T0 + E + 400, |_| worker(Seen::Errored));
        b.tick(T0 + 30_000, |_| worker(Seen::Working));
        b.tick(T0 + 40_000, |_| worker(Seen::Errored));
        b.tick(T0 + 40_000 + ERROR_ENDS_MS - 1, |_| worker(Seen::Errored));
        assert!(stage(&b, 1).is_open());
        b.tick(T0 + 40_000 + ERROR_ENDS_MS, |_| worker(Seen::Errored));
        assert_eq!(stage(&b, 1).word(), "failed");
    }

    /// A brief cannot carry a key: every control character but newline and
    /// tab goes, the paste bracket's closer included, and line endings unify.
    #[test]
    fn clean_takes_out_every_key() {
        let raw = "a\x1b[201~b\r\nc\rd\te\x07\x03\x7f\u{9b}f\n\n  ";
        let c = clean(raw);
        assert_eq!(c, "a[201~b\nc\nd\tef");
        assert!(!c
            .chars()
            .any(|ch| ch.is_control() && ch != '\n' && ch != '\t'));
    }

    /// The header is one line, names the sender and says by what right, and
    /// a title cannot break it onto two lines or run it long.
    #[test]
    fn the_header_is_one_line_and_names_the_right() {
        let h = header(AskId(42), "evil\nline\x1b[2J", Some(9), false);
        assert!(!h.contains('\n') && !h.contains('\x1b'), "{h}");
        assert!(
            h.starts_with("[Threshing Floor prompt 42, from evil line[2J (pid 9)"),
            "{h}"
        );
        assert!(h.contains("a person drew a prompt line"));
        assert!(h.contains("told when you finish"));
        assert!(header(AskId(1), "p", None, true).contains("returned to the sender"));
        let long = header(AskId(1), &"x".repeat(500), None, true);
        assert!(long.chars().count() < 260, "{}", long.chars().count());
    }

    #[test]
    fn a_key_is_short_and_one_line() {
        assert_eq!(key_of(None), Ok(None));
        assert_eq!(key_of(Some("  ")), Ok(None));
        assert_eq!(key_of(Some(" job-1 ")), Ok(Some("job-1".into())));
        assert!(key_of(Some(&"k".repeat(65))).is_err());
        assert!(key_of(Some("a\nb")).is_err());
    }

    /// Ended prompts are kept for status, but not forever and not without
    /// bound.
    #[test]
    fn ended_prompts_are_pruned() {
        let mut b = PromptBook::default();
        for i in 0..(KEEP_ENDED as u64 + 10) {
            b.send(new(i + 1, i + 1, 500, None)).unwrap();
            b.fail(AskId(i + 1), "x", T0);
        }
        b.tick(T0 - SETTLE_MS, |_| worker(Seen::Ready));
        b.tick(T0, |_| worker(Seen::Ready));
        assert_eq!(b.sent.len(), KEEP_ENDED);
        b.tick(T0 + KEEP_ENDED_MS, |_| worker(Seen::Ready));
        assert!(b.sent.is_empty());
    }

    /// A sender closing drops what it has not had typed, and only that.
    #[test]
    fn a_sender_closing_drops_its_untyped_prompts() {
        let mut b = PromptBook::default();
        b.send(new(1, 1, 500, None)).unwrap();
        b.send(new(2, 1, 501, None)).unwrap();
        b.tick(T0 - SETTLE_MS, |s| {
            if s.target_pid == 501 {
                worker(Seen::Ready)
            } else {
                worker(Seen::Working)
            }
        });
        b.tick(T0, |s| {
            if s.target_pid == 501 {
                worker(Seen::Ready)
            } else {
                worker(Seen::Working)
            }
        });
        assert_eq!(b.sender_gone(1, T0 + 10), vec![AskId(1)]);
        assert_eq!(stage(&b, 2), PromptStage::Typing);
    }

    /// The paste is bracketed, cleaned again, and never written to a prompt
    /// that did not ask for bracketing.
    #[test]
    fn the_paste_is_bracketed_and_cannot_close_early() {
        assert_eq!(paste("hi", false), None);
        assert_eq!(paste("\x1b\x07  ", true), None);
        let out = paste("a\x1b[201~\rb\n", true).unwrap();
        assert_eq!(out, b"\x1b[200~a[201~\nb\x1b[201~".to_vec());
        let inner = &out[6..out.len() - 6];
        assert!(
            !inner.contains(&0x1b) && !inner.contains(&b'\r'),
            "{inner:?}"
        );
    }

    /// The reply is the last assistant message after the prompt's own line,
    /// in either program's transcript; without the prompt's number in the
    /// file, or with nothing said after it, it is unknown.
    #[test]
    fn the_reply_is_read_after_the_prompt_only() {
        let id = AskId(17);
        let claude = [
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"before"}]}}"#,
            r#"{"type":"user","message":{"content":"[Threshing Floor prompt 17, from p"}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"first"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"done, 3 files"}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result"}]}}"#,
        ]
        .join("\n");
        assert_eq!(last_reply(&claude, id), Ok("done, 3 files".into()));
        // Claude Code's bookkeeping repeats the prompt after the reply; the
        // reply is still the reply (measured in rig run 3).
        let booked = format!(
            "{claude}\n{}",
            r#"{"type":"last-prompt","lastPrompt":"[Threshing Floor prompt 17, from p…","leafUuid":"x"}"#
        );
        assert_eq!(last_reply(&booked, id), Ok("done, 3 files".into()));
        // A tool's result that happens to quote the number is not the turn.
        let quoted = format!(
            "{claude}\n{}",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"Threshing Floor prompt 17,"}]}}"#
        );
        assert_eq!(last_reply(&quoted, id), Ok("done, 3 files".into()));
        let codex = [
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"[Threshing Floor prompt 17, from p"}]}}"#,
            r#"{"type":"response_item","payload":{"type":"function_call","name":"shell"}}"#,
            r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"all green"}]}}"#,
            r#"{"type":"event_msg","payload":{"type":"token_count"}}"#,
        ]
        .join("\n");
        assert_eq!(last_reply(&codex, id), Ok("all green".into()));
        // Another prompt's number is not this one, even one it is a prefix of.
        assert!(last_reply(&claude.replace("prompt 17,", "prompt 171,"), id).is_err());
        assert_eq!(
            last_reply(&claude.replace("prompt 17", "prompt 18"), id),
            Err("its conversation does not show this prompt")
        );
        let silent = r#"{"type":"user","message":{"content":"[Threshing Floor prompt 17, x"}}"#;
        assert_eq!(
            last_reply(silent, id),
            Err("it wrote no message after this prompt")
        );
        // A message before the prompt is never its reply.
        let only_before = [claude.lines().next().unwrap(), silent].join("\n");
        assert!(last_reply(&only_before, id).is_err());
    }

    /// Only a reply the transcript may not have caught up with is looked for
    /// again.
    #[test]
    fn only_a_reply_still_being_written_is_looked_for_again() {
        assert!(worth_another_look(NOT_SHOWN) && worth_another_look(NOTHING_AFTER));
        assert!(!worth_another_look(NO_CONVERSATION));
        assert!(!worth_another_look(
            "its conversation could not be opened: x"
        ));
    }

    /// `prompt 17` is not found inside `prompt 171`: the tag ends at a comma.
    #[test]
    fn a_tag_is_not_a_prefix_of_another() {
        let h = header(AskId(171), "p", None, true);
        assert!(!h.contains(&tag(AskId(17))), "{h}");
        assert!(h.contains(&tag(AskId(171))), "{h}");
        let rows = |r: &str| vec![r.to_string()];
        assert!(!shows_paste(&[], &rows(&h), AskId(17)));
        assert!(shows_paste(&[], &rows(&h), AskId(171)));
    }
}
