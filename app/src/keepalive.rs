//! Cache keepalive — type into an idle agent's prompt before its cache expires.
//!
//! An hour of silence expires a session's prompt cache, and the next message
//! re-writes the whole conversation at 2.0x base input instead of reading it at
//! 0.1x. Measured across 32,037 billed turns in thirty days on this machine: 87
//! such cold returns, 24.2M tokens re-written, average gap 5.0 hours — about
//! $242, of which roughly $230 buys nothing.
//!
//! A message typed into a live pane before the hour is up avoids all of it.
//! Measured on real panes in this machine's own history, at gaps of 51-58
//! minutes:
//!
//! ```text
//! gap 57.5 min   read 341,693   paid $0.176   cold would have cost $3.42
//! gap 57.3 min   read 510,815   paid $0.320   cold would have cost $5.17
//! gap 51.7 min   read 570,879   paid $0.340   cold would have cost $5.77
//! ```
//!
//! # What this replaced, and why that was wrong
//!
//! The first version of this module did not type into the terminal. It spawned
//! a headless `claude --resume <id> --fork-session` ping instead, on the theory
//! that an idle pane and a pane holding a permission prompt look identical on
//! screen, so writing to a terminal was unsafe and a forked process was the
//! careful way round it.
//!
//! **It does not work, and it was not what was asked for.** A headless process
//! builds a *different prefix* than the interactive one, so it warms a cache
//! entry the live pane will never read. Measured on one session, same minute:
//!
//! ```text
//! the pane's own interactive turns:   prefix 127,741
//! a headless resume of that session:  prefix 114,737   (13,004 apart)
//! ```
//!
//! A resume of a session idle only 24 minutes — cache alive, well inside the
//! TTL — still wrote 94,668 tokens and cost $0.99. There is no out-of-band
//! version of this feature. Only the live interactive process can produce the
//! live interactive prefix, so the message has to go in through the terminal.
//!
//! That detour also invented a second problem and then solved it at length: the
//! ping was given `--disallowed-tools "*"` to make it harmless, tool definitions
//! turned out to be part of the cached prefix, and the "safety" flag cost
//! $0.4200 instead of $0.0276. None of that apparatus survives here. It was
//! avoidance of a plainly stated instruction, and the instruction was right.
//!
//! # The sequence, per idle pane
//!
//! | at | what happens |
//! |---|---|
//! | 50 min | 💤 appears on the tab — the first warning, one per tab |
//! | 55 min | the message is typed into the prompt, **unsent**, without focus |
//! | +5 s | a desktop notification offers to send it; clicking sends |
//! | 57 min | it sends itself |
//!
//! **Focus is never taken.** TD owns the pty, so [`Act::Type`] and [`Act::Send`]
//! are writes to the pty master — the same path a keystroke takes, minus the
//! keyboard. The human keeps working in whatever pane they are in.
//!
//! # Enter is not pressed on a prediction
//!
//! `needs_input` and `bell_blocked` are screen-row heuristics, and heuristics
//! are allowed to be wrong. The sibling herdr plugin — same design, different
//! runtime — hit the case they miss: its runtime reported a pane as idle and
//! ready for input while Claude Code sat on its "Do you trust the files in this
//! folder?" dialog. A status describes the agent PROCESS, not the screen, and
//! Enter on that screen answers the dialog.
//!
//! So [`Act::Send`] is gated on a read-back rather than on a prediction. Before
//! the carriage return reaches the pty, the pane's own grid is searched for
//! [`PROBE`]; if the message it was given two minutes ago is not there, the
//! characters went somewhere that is not a visible text field and the pane goes
//! to [`Stage::Refused`] instead. TD reads its own grid, so unlike a runtime
//! queried over a socket there is no stale-snapshot question — the answer is
//! the screen. See `TerminalView::keepalive_step`.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// The campfire, lit and out. The menu-bar toggle and nothing else.
///
/// A fire reads the right way round without a legend: burning means somebody is
/// tending the camp and the caches stay warm; out means everyone has gone home.
/// Deliberately NOT 💤, which already means "this one pane is going drowsy" on
/// the tab bar — one glyph, one meaning.
pub const GLYPH_TENDING: &str = "🔥";
pub const GLYPH_AWAY: &str = "🌙";

/// 💤 on the tab. Far enough ahead of the typing to be a warning rather than a
/// narration of something already done.
pub const DROWSY_AT: Duration = Duration::from_secs(50 * 60);

/// The message goes into the prompt, unsent, with five minutes of TTL left.
pub const TYPE_AT: Duration = Duration::from_secs(55 * 60);

/// Long enough after typing that the notification describes a prompt that is
/// already sitting there, short enough to still be about this minute.
pub const NOTIFY_AFTER_TYPING: Duration = Duration::from_secs(5);

/// Auto-send, two minutes after entry and three minutes before the cache dies.
pub const SEND_AT: Duration = Duration::from_secs(57 * 60);

/// Typed verbatim into the agent's prompt. Long enough to explain itself to
/// whoever finds it sitting there unsent, and it asks for a one-word reply so
/// the answering turn costs almost nothing on top of the cache read it is for.
pub const MESSAGE: &str = "55 Minutes have elapsed since human interaction. \
Warming cache to save 20x costs of refreshing cache. Please respond with simple \
yes - cache is still warm if the cache is still warm for another hour.";

/// A distinctive slice of [`MESSAGE`], searched for on the pane's own grid
/// before Enter is ever pressed.
///
/// The TAIL of the message, not the head, and the difference is not cosmetic.
/// An input box is a few lines tall and this message does not fit in one: the
/// box shows the end, where the cursor is, and scrolls the beginning out of
/// sight. A probe matching "55 Minutes have elapsed" would be looking at the
/// one region guaranteed to be off-screen, and would refuse every pane that had
/// received the message perfectly. Measured on live panes 2026-09-04, in the
/// sibling herdr plugin that shares this design.
pub const PROBE: &str = "another hour";

/// How many Ctrl+U kills [`Act::Clear`] sends. Enough to empty a wrapped
/// message of this length several times over; see the note in [`bytes`] for why
/// it is a fixed count rather than a loop.
pub const CLEAR_KILLS: usize = 12;

/// AWAY — the campfire is out, and nothing warms anything.
///
/// The keepalive is built for the gap between "stepped away for a coffee" and
/// "the cache is about to expire". It is exactly wrong for the other kind of
/// leaving: finishing for the day, or putting six agents in flight and going
/// out for six hours. Every one of those sessions gets warmed on the hour,
/// forever, for a return that is not coming — which is the one configuration
/// where this feature costs money instead of saving it.
///
/// A **file** rather than a field, because the flag has two readers in two
/// processes. TD's menu bar owns the switch, and the sibling herdr plugin
/// (`herdr-auto-warm-cache`) reads the same path, so one press governs every
/// agent on the machine rather than only the ones inside this terminal. Its
/// presence is the whole protocol: no format to agree on, no parse to get
/// wrong, and a stray file fails safe by warming nothing.
pub fn away_path(home: &Path) -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/state"))
        .join("cache-keepalive/away")
}

/// True when the campfire is out. An unreadable path reads as "not away": the
/// failure mode of a missing state directory should be the feature working, not
/// the feature silently disabling itself.
pub fn is_away(home: &Path) -> bool {
    away_path(home).exists()
}

/// Put the fire out, or light it. Creating the parent is part of the job — the
/// first press must not fail because nothing has ever written here before.
pub fn set_away(home: &Path, away: bool) -> std::io::Result<()> {
    let p = away_path(home);
    if away {
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&p, b"away\n")
    } else {
        match std::fs::remove_file(&p) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

/// Where a pane is in the sequence. Reset to `Awake` by any human keystroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stage {
    #[default]
    Awake,
    /// 💤 is up.
    Drowsy,
    /// The message is sitting in the prompt, unsent.
    Loaded,
    /// ...and the desktop has been told about it.
    Notified,
    /// Sent. Nothing further until a human comes back.
    Sent,
    /// The message was typed and could not then be found on this pane's grid,
    /// so Enter was never pressed. Terminal: something other than a text prompt
    /// took those characters, and this pane is not ours to keep poking.
    Refused,
}

/// What to do to one pane on this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Nothing,
    /// Put 💤 on the tab.
    Drowse,
    /// Write [`MESSAGE`] to the pty, no newline.
    Type,
    /// `notify-send`, with a default action that sends.
    Notify,
    /// Write `\r`.
    Send,
    /// The human came back to a pane holding an unsent message: kill the line
    /// before their keystroke lands on the end of it.
    Clear,
}

/// One pane's inputs. `busy`, `needs_input` and `blocked` all come from state
/// TD already tracks for the tab badges.
#[derive(Debug, Clone, Copy)]
pub struct Pane {
    /// Since the last keystroke a *human* sent to this pane.
    pub idle: Duration,
    pub stage: Stage,
    /// A turn is in flight. Its completion refreshes the cache for free.
    pub busy: bool,
    /// A permission prompt, a plan approval, a `/` menu — anything where the
    /// agent is waiting on a person.
    pub needs_input: bool,
    /// An error banner is on screen.
    pub blocked: bool,
    /// The human is typing here right now.
    pub human_active: bool,
}

/// The whole policy. No I/O, no clock, no side effects.
pub fn act(p: &Pane) -> Act {
    // The human came back. Clear a staged message before their text lands on
    // the end of it; otherwise there is nothing to do.
    if p.human_active {
        return match p.stage {
            Stage::Loaded | Stage::Notified => Act::Clear,
            _ => Act::Nothing,
        };
    }

    // Never type at a pane that is waiting on a person. An idle pane and a pane
    // holding a permission prompt look identical on screen, and "yes" into a
    // confirmation dialog is an approval. A cold cache is the cheaper mistake.
    if p.needs_input || p.blocked {
        return Act::Nothing;
    }

    // A pane mid-turn will refresh its own cache when the turn lands.
    if p.busy {
        return Act::Nothing;
    }

    match p.stage {
        Stage::Awake if p.idle >= DROWSY_AT => Act::Drowse,
        Stage::Drowsy if p.idle >= TYPE_AT => Act::Type,
        Stage::Loaded if p.idle >= TYPE_AT + NOTIFY_AFTER_TYPING => Act::Notify,
        Stage::Notified if p.idle >= SEND_AT => Act::Send,
        // A pane that crossed 55 minutes while blocked, and became unblocked
        // after: catch it up rather than leaving it stuck a stage behind.
        Stage::Awake if p.idle >= TYPE_AT => Act::Drowse,
        _ => Act::Nothing,
    }
}

/// The stage a pane is in once `act` has been carried out.
pub fn advance(stage: Stage, done: Act) -> Stage {
    match done {
        Act::Drowse => Stage::Drowsy,
        Act::Type => Stage::Loaded,
        Act::Notify => Stage::Notified,
        Act::Send => Stage::Sent,
        Act::Clear => Stage::Awake,
        Act::Nothing => stage,
    }
}

/// The bytes an [`Act`] puts on the pty. `bracketed` is the pane's own
/// `BRACKETED_PASTE` mode, exactly as a clipboard paste consults it — without
/// the wrapper a multi-word line can be interpreted rather than inserted.
pub fn bytes(a: Act, bracketed: bool) -> Option<Vec<u8>> {
    match a {
        Act::Type if bracketed => Some([b"\x1b[200~", MESSAGE.as_bytes(), b"\x1b[201~"].concat()),
        Act::Type => Some(MESSAGE.as_bytes().to_vec()),
        // Carriage return, not newline: this is a pty, and \n on a line-buffered
        // TUI is not the same key.
        Act::Send => Some(b"\r".to_vec()),
        // Ctrl+U, several times. One press is not enough: Claude Code's input
        // box is multi-line and Ctrl+U kills a single line, so one byte against
        // a message this long leaves most of it in the prompt — measured, on a
        // live pane, where the first press took it from "still warm for another
        // hour" down to "Please respond with" and no further.
        //
        // Bounded rather than repeated-until-clean, and that bound is the
        // lesson. The sibling herdr plugin tried pressing until a read-back
        // agreed the box was empty, and no such read-back is trustworthy:
        // a tail probe reports clear while the middle survives, a list of
        // slices only proves the slices you thought of are absent, and the
        // structural "empty box is a bare prompt glyph" test holds right up
        // until the box IS empty, at which point Claude collapses it and draws
        // no glyph to match. A fixed, generous number of kills does most of the
        // work; the human clears any remainder with one keystroke, and nobody
        // hammers a live pane forever waiting for an oracle that cannot answer.
        Act::Clear => Some(vec![0x15; CLEAR_KILLS]),
        _ => None,
    }
}

/// The desktop notification. Its `default` action is what a click invokes, and
/// TD answers that by sending the message — from wherever the human happens to
/// be, without raising or focusing the pane it belongs to.
pub fn notification(tab: &str, pane: &str) -> (String, String) {
    (
        format!("💤 {tab} — {pane} going cold"),
        "An inactive terminal is about to lose its prompt cache. Click to send the \
         cache-warm message now."
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u64 = 60;

    /// A private HOME for the away-flag tests. `XDG_STATE_HOME` is consulted
    /// first by `away_path`, and these tests must not touch the real one — a
    /// test that put the developer's own machine into AWAY and left it there
    /// would be a very quiet way to stop the feature working.
    fn tmp_home(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("td-away-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn the_campfire_starts_lit() {
        let home = tmp_home("fresh");
        std::env::remove_var("XDG_STATE_HOME");
        assert!(
            !is_away(&home),
            "a machine that has never been told is not away"
        );
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn putting_the_fire_out_and_lighting_it_again_round_trips() {
        let home = tmp_home("toggle");
        std::env::remove_var("XDG_STATE_HOME");
        set_away(&home, true).unwrap();
        assert!(is_away(&home));
        set_away(&home, false).unwrap();
        assert!(!is_away(&home));
        // ...and lighting an already-lit fire is not an error, because a menu
        // bar can be clicked twice and a missing file is the desired state.
        set_away(&home, false).unwrap();
        assert!(!is_away(&home));
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn the_flag_is_a_path_two_processes_can_agree_on() {
        // The herdr plugin reads this exact location. If the shape changes here
        // the two halves silently stop governing each other, and the only
        // symptom is agents being warmed after somebody pressed away.
        let home = tmp_home("path");
        std::env::remove_var("XDG_STATE_HOME");
        assert!(away_path(&home).ends_with(".local/state/cache-keepalive/away"));
        std::env::set_var("XDG_STATE_HOME", "/tmp/xdg-state-probe");
        assert_eq!(
            away_path(&home),
            PathBuf::from("/tmp/xdg-state-probe/cache-keepalive/away")
        );
        std::env::remove_var("XDG_STATE_HOME");
        std::fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn the_two_campfire_glyphs_are_distinct_and_neither_is_the_drowsy_badge() {
        // 💤 already means "this one pane is going drowsy" on the tab bar, and
        // one glyph carrying two meanings is how a bar stops being readable.
        assert_ne!(GLYPH_TENDING, GLYPH_AWAY);
        assert_ne!(GLYPH_TENDING, "💤");
        assert_ne!(GLYPH_AWAY, "💤");
    }

    fn at(mins: u64, stage: Stage) -> Pane {
        Pane {
            idle: Duration::from_secs(mins * MIN),
            stage,
            busy: false,
            needs_input: false,
            blocked: false,
            human_active: false,
        }
    }

    #[test]
    fn nothing_happens_for_the_first_fifty_minutes() {
        assert_eq!(act(&at(49, Stage::Awake)), Act::Nothing);
    }

    #[test]
    fn fifty_minutes_puts_the_sleep_emoji_up_as_the_first_warning() {
        assert_eq!(act(&at(50, Stage::Awake)), Act::Drowse);
    }

    #[test]
    fn fifty_five_minutes_types_the_message_without_sending_it() {
        assert_eq!(act(&at(55, Stage::Drowsy)), Act::Type);
    }

    #[test]
    fn the_notification_follows_five_seconds_after_the_typing() {
        let mut p = at(55, Stage::Loaded);
        assert_eq!(act(&p), Act::Nothing);
        p.idle = TYPE_AT + NOTIFY_AFTER_TYPING;
        assert_eq!(act(&p), Act::Notify);
    }

    #[test]
    fn it_sends_itself_two_minutes_after_it_was_entered() {
        assert_eq!(act(&at(57, Stage::Notified)), Act::Send);
        assert_eq!(SEND_AT - TYPE_AT, Duration::from_secs(2 * MIN));
    }

    #[test]
    fn every_stage_lands_with_time_left_on_the_hour() {
        // The whole sequence is worthless if it finishes after the cache dies.
        assert!(SEND_AT < Duration::from_secs(60 * MIN));
        assert!(Duration::from_secs(60 * MIN) - SEND_AT >= Duration::from_secs(3 * MIN));
        assert!(DROWSY_AT < TYPE_AT && TYPE_AT < SEND_AT);
    }

    #[test]
    fn a_sent_pane_does_not_send_again() {
        assert_eq!(act(&at(90, Stage::Sent)), Act::Nothing);
    }

    #[test]
    fn a_pane_waiting_on_a_person_is_never_typed_into() {
        // The one way this feature can do damage: "yes" into a confirmation
        // dialog is an approval. A cold cache is the cheaper mistake.
        for stage in [Stage::Awake, Stage::Drowsy, Stage::Loaded, Stage::Notified] {
            let mut p = at(58, stage);
            p.needs_input = true;
            assert_eq!(act(&p), Act::Nothing, "{stage:?} with a prompt up");
            let mut p = at(58, stage);
            p.blocked = true;
            assert_eq!(act(&p), Act::Nothing, "{stage:?} with an error banner");
        }
    }

    #[test]
    fn a_pane_mid_turn_is_left_alone_because_its_own_turn_refreshes_the_cache() {
        let mut p = at(58, Stage::Drowsy);
        p.busy = true;
        assert_eq!(act(&p), Act::Nothing);
    }

    #[test]
    fn the_human_coming_back_clears_a_message_they_did_not_type() {
        // Otherwise their first keystroke lands on the end of 190 characters
        // they did not write.
        for stage in [Stage::Loaded, Stage::Notified] {
            let mut p = at(56, stage);
            p.human_active = true;
            assert_eq!(act(&p), Act::Clear);
        }
    }

    #[test]
    fn the_human_coming_back_before_anything_was_typed_clears_nothing() {
        let mut p = at(51, Stage::Drowsy);
        p.human_active = true;
        assert_eq!(act(&p), Act::Nothing);
    }

    #[test]
    fn a_pane_unblocked_after_the_deadline_catches_up_rather_than_sticking() {
        assert_eq!(act(&at(56, Stage::Awake)), Act::Drowse);
        assert_eq!(act(&advance_to(56, Stage::Awake)), Act::Type);
    }

    fn advance_to(mins: u64, stage: Stage) -> Pane {
        let p = at(mins, stage);
        at(mins, advance(stage, act(&p)))
    }

    #[test]
    fn clearing_returns_the_pane_to_the_start_of_the_sequence() {
        assert_eq!(advance(Stage::Notified, Act::Clear), Stage::Awake);
    }

    #[test]
    fn typing_is_a_paste_when_the_pane_asked_for_bracketed_paste() {
        let b = bytes(Act::Type, true).unwrap();
        assert!(b.starts_with(b"\x1b[200~") && b.ends_with(b"\x1b[201~"));
        assert_eq!(bytes(Act::Type, false).unwrap(), MESSAGE.as_bytes());
    }

    #[test]
    fn typing_carries_no_newline_of_its_own() {
        // The two minutes between entry and send exist so a person can see it
        // and stop it. A stray \r here would delete that window.
        for bracketed in [true, false] {
            let b = bytes(Act::Type, bracketed).unwrap();
            assert!(!b.contains(&b'\r') && !b.contains(&b'\n'));
        }
    }

    #[test]
    fn send_is_a_carriage_return_and_clear_is_repeated_ctrl_u() {
        assert_eq!(bytes(Act::Send, false).unwrap(), b"\r");
        let clear = bytes(Act::Clear, false).unwrap();
        assert!(
            clear.len() > 1,
            "one Ctrl+U kills one line of a multi-line box"
        );
        assert!(clear.iter().all(|b| *b == 0x15));
        assert_eq!(clear.len(), CLEAR_KILLS);
    }

    #[test]
    fn the_probe_matches_the_tail_of_the_message_not_the_head() {
        // An input box shows the end, where the cursor is, and scrolls the
        // start out of sight. A head probe refuses panes that received the
        // message perfectly.
        assert!(MESSAGE.ends_with(&format!("{PROBE}.")) || MESSAGE.contains(PROBE));
        let head = &MESSAGE[..PROBE.len().min(MESSAGE.len())];
        assert!(
            !head.contains(PROBE),
            "PROBE must not match the opening of MESSAGE"
        );
        // ...and it has to be findable at all, or Send is refused every time.
        assert!(MESSAGE.contains(PROBE));
    }

    #[test]
    fn a_refused_pane_is_terminal_and_is_never_typed_at_again() {
        // Set when the message could not be found on the grid after typing.
        // Something that is not a text prompt took those characters; poking it
        // further is how a keepalive answers a dialog.
        for mins in [56, 58, 90, 600] {
            assert_eq!(act(&at(mins, Stage::Refused)), Act::Nothing);
        }
    }

    #[test]
    fn a_human_returning_to_a_refused_pane_is_not_sent_a_clear() {
        // Nothing verifiably staged there, and the pane already declined to
        // behave like a text box once.
        let mut p = at(56, Stage::Refused);
        p.human_active = true;
        assert_eq!(act(&p), Act::Nothing);
    }

    #[test]
    fn the_warning_stages_put_nothing_on_the_pty() {
        // 💤 is a tab badge and the notification is a desktop toast; neither may
        // touch the terminal.
        assert!(bytes(Act::Drowse, true).is_none());
        assert!(bytes(Act::Notify, true).is_none());
        assert!(bytes(Act::Nothing, true).is_none());
    }

    #[test]
    fn the_message_asks_for_a_one_word_answer() {
        // The reply is billed as output. The cache read is the point; the answer
        // should cost nothing next to it.
        assert!(MESSAGE.contains("simple yes"));
        assert!(MESSAGE.len() < 256);
    }
}
