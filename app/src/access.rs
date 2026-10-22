//! What an agent may ask a person for, and the one line it is told back.
//!
//! Some settings decide what agents can see and do: whether a pane is
//! off-limits to them, whether they may write, whether plain shells and other
//! agents' transcripts are shown to them. Those are a person's to change.
//! Before this module an agent that needed one of them changed got a refusal
//! and nothing else; now it can ASK, the ask waits on the attention rail with
//! Yes and No, and only a person's press answers it. Parker, 2026-10-15:
//! *"the agent spine will now allow requests to show up with a SIMPEL YES NO if
//! they are trying to modify TERMINAL DELIGHT SETTING THAt require human
//! provenance (agent behaviour and access)... but are not blocked by human
//! decision for simple things like themeing etc."*
//!
//! Appearance is not here, and that is the second half of his sentence: an
//! agent with writes on restyles a pane through `set_pane_config` and asks
//! nobody.
//!
//! ## What this module is
//!
//! The rules and the words, with no window in them: which changes exist, which
//! of them name a pane, when there is nothing to ask, what Yes does to the MCP
//! switches, and the line typed back. The verb that raises an ask is
//! `request_access` in `mcp.rs`; the row and its buttons are the rail's, in
//! `main.rs`. **Nothing here answers an ask** — the only callers of an answer
//! are the rail's own press and key handlers, and a test in this file keeps the
//! two agent-facing surfaces, `mcp.rs` and `ctl.rs`, from ever naming them.

use std::time::Instant;

use crate::mcp::{Expose, McpConfig};

/// The settings an agent may ask a person to change. Six, and only these.
///
/// The narrowing twins of the last three are left out on purpose: an agent
/// that wants less access can simply not use it, and every extra verb is one
/// more thing waiting on the rail. [`Change::ClosePane`] stays, because an
/// agent that sees a pane full of credentials should be able to say so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Lift off-limits from a pane, so agents can see it again.
    OpenPane,
    /// Mark a pane off-limits to agents.
    ClosePane,
    /// Turn MCP writes on.
    WritesOn,
    /// Show plain shells to agents, not only agent panes.
    ExposeShells,
    /// Let agents tail other agents' transcripts.
    EventsOn,
    /// A bypass line in the War Room: ask one pane directly, past the chart,
    /// for a few prompts (three unless the person picks another count).
    AskLine,
}

impl Change {
    /// Every change, in the order the tool description lists them.
    pub const ALL: [Change; 6] = [
        Change::OpenPane,
        Change::ClosePane,
        Change::WritesOn,
        Change::ExposeShells,
        Change::EventsOn,
        Change::AskLine,
    ];

    /// The name on the wire, which is also the name in the answer line.
    pub fn wire(self) -> &'static str {
        match self {
            Change::OpenPane => "open_pane",
            Change::ClosePane => "close_pane",
            Change::WritesOn => "writes_on",
            Change::ExposeShells => "expose_shells",
            Change::EventsOn => "events_on",
            Change::AskLine => "ask_line",
        }
    }

    pub fn parse(s: &str) -> Option<Change> {
        Change::ALL.into_iter().find(|c| c.wire() == s)
    }

    /// Whether the change is about one pane, and so needs that pane's pid.
    pub fn needs_pane(self) -> bool {
        matches!(self, Change::OpenPane | Change::ClosePane | Change::AskLine)
    }

    /// What a person reads on the rail row, in their words rather than ours.
    ///
    /// `title` is the pane's name as the person sees it, read when the ask
    /// arrived; it is `None` for the window-wide changes and for a pane that
    /// had no name to read.
    ///
    /// Drawn and nowhere else, so it is in the reader's language; the answer
    /// line the agent gets back ([`Self::done_words`], [`Self::kept_words`])
    /// stays English.
    pub fn ask_words(self, title: Option<&str>) -> String {
        let t = &crate::lang::current().strings().parts;
        let pane = match title {
            Some(name) => format!("\u{201c}{}\u{201d}", one_line(name)),
            None => t.access_ask_a_pane.to_string(),
        };
        match self {
            Change::OpenPane => crate::lang::fill(t.access_ask_open_pane, &[("pane", &pane)]),
            Change::ClosePane => crate::lang::fill(t.access_ask_close_pane, &[("pane", &pane)]),
            Change::WritesOn => t.access_ask_writes_on.into(),
            Change::ExposeShells => t.access_ask_expose_shells.into(),
            Change::EventsOn => t.access_ask_events_on.into(),
            Change::AskLine => crate::lang::fill(t.access_ask_ask_line, &[("pane", &pane)]),
        }
    }

    /// What is now true after a Yes, for the agent's line.
    fn done_words(self, pid: Option<u32>) -> String {
        let pane = pane_words(pid);
        match self {
            Change::OpenPane => format!("{pane} is open to agents"),
            Change::ClosePane => format!("{pane} is off-limits to agents"),
            Change::WritesOn => "MCP writes are on".into(),
            Change::ExposeShells => "plain shells are shown to agents".into(),
            Change::EventsOn => "agents' tool calls are shown to agents".into(),
            Change::AskLine => {
                format!("you may ask {pane} directly, for the prompts on the bypass line")
            }
        }
    }

    /// What is still true after a No, for the agent's line.
    fn kept_words(self, pid: Option<u32>) -> String {
        let pane = pane_words(pid);
        match self {
            Change::OpenPane => format!("{pane} stays off-limits"),
            Change::ClosePane => format!("{pane} stays open to agents"),
            Change::WritesOn => "MCP writes stay off".into(),
            Change::ExposeShells => "only agent panes are shown to agents".into(),
            Change::EventsOn => "agents' tool calls stay hidden".into(),
            Change::AskLine => format!("no line reaches {pane} from your seat"),
        }
    }
}

fn pane_words(pid: Option<u32>) -> String {
    match pid {
        Some(p) => format!("pane {p}"),
        None => "the pane".to_string(),
    }
}

/// The most words a reason may run to.
pub const WHY_MAX_WORDS: usize = 40;

/// The agent's reason, made safe to draw and to type: required, one line, at
/// most [`WHY_MAX_WORDS`] words.
///
/// Required because a bare "let me in" is not something a person can judge;
/// the reason is most of what the row is for. It is the agent's text, so it is
/// flattened like every other agent string that reaches a person's screen.
pub fn validate_why(why: Option<&str>) -> Result<String, String> {
    let flat = one_line(why.unwrap_or(""));
    if flat.is_empty() {
        return Err(
            "`why` is required: say in a sentence what you need this for. The \
                    person reads it on the rail before answering."
                .to_string(),
        );
    }
    let words = flat.split_whitespace().count();
    if words > WHY_MAX_WORDS {
        return Err(format!(
            "`why` is {words} words; keep it to {WHY_MAX_WORDS}. The person reads it on a \
             row 300 pixels wide."
        ));
    }
    Ok(flat)
}

/// One ask, waiting on the pane it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    pub change: Change,
    /// The shell pid of the pane the ask is about — the number agents name
    /// panes by. `None` for the window-wide changes.
    pub pid: Option<u32>,
    /// That pane's name as the person sees it, read by the window when the ask
    /// arrived. `None` until then, and for a window-wide change.
    pub title: Option<String>,
    /// The agent's reason, through [`validate_why`].
    pub why: String,
    /// When the ask arrived, so the row can say how long it has waited.
    pub at: Instant,
    /// How serious the ask is — the rail's colour and glyph. Assessed by the
    /// window when the ask arrives (`warroom::risk`); `None` until then.
    pub risk: Option<crate::coordination::Severity>,
    /// For a bypass: the prompts it grants, three unless the person picks
    /// another count on the card before pressing Yes.
    pub prompts: u8,
}

/// The prompts a bypass grants unless the person picks another count.
pub const BYPASS_PROMPTS: u8 = 3;
/// The counts the rail offers for a bypass.
pub const BYPASS_CHOICES: [u8; 3] = [1, 3, 10];

/// A person's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Yes,
    No,
}

/// What came of an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Yes, and the setting changed.
    Done,
    /// No, and nothing changed.
    Kept,
    /// Yes, but it was already so by the time of the press — a person changed
    /// it by hand while the ask waited.
    AlreadySo,
    /// Yes, but the pane the ask named has closed.
    Gone,
}

/// What the window knows about the setting an ask names, at the moment of
/// asking: the target pane's mark (`None` when no such pane exists) and the
/// MCP switches.
pub struct Now<'a> {
    pub pane_off_limits: Option<bool>,
    pub mcp: &'a McpConfig,
    /// For `ask_line`: `None` when the asker or the pane is not seated in the
    /// War Room (a line runs between seats), else whether a line from the
    /// asker's seat already reaches the pane as far as ask.
    pub line_reaches_ask: Option<bool>,
}

/// Why there is nothing to ask, or `None` when the ask can stand.
///
/// An ask for what is already so would put a row on the rail whose Yes does
/// nothing, which teaches a person that the buttons are decoration.
pub fn nothing_to_ask(change: Change, pid: Option<u32>, now: &Now) -> Option<String> {
    let pane = pane_words(pid);
    match change {
        Change::OpenPane | Change::ClosePane => match now.pane_off_limits {
            None => Some(match pid {
                Some(p) => format!("no pane with pid {p} in this window"),
                None => "this change is about one pane, and none was named".to_string(),
            }),
            Some(false) if change == Change::OpenPane => Some(format!(
                "{pane} is not off-limits, so there is nothing to ask"
            )),
            Some(true) if change == Change::ClosePane => {
                Some(format!("{pane} is already off-limits"))
            }
            Some(_) => None,
        },
        Change::WritesOn => now
            .mcp
            .writable
            .then(|| "MCP writes are already on".to_string()),
        Change::ExposeShells => matches!(now.mcp.expose, Expose::All)
            .then(|| "plain shells are already shown to agents".to_string()),
        Change::EventsOn => now
            .mcp
            .events
            .then(|| "agents' tool calls are already shown".to_string()),
        Change::AskLine => match (now.pane_off_limits, now.line_reaches_ask) {
            (None, _) => Some(match pid {
                Some(p) => format!("no pane with pid {p} in this window"),
                None => "a bypass is to one pane, and none was named".to_string(),
            }),
            (Some(true), _) => Some(format!("{pane} is off-limits; no line can reach it")),
            (Some(false), None) => Some(format!(
                "a line runs between War Room seats, and your pane or {pane} is not seated; \
                 only a person seats panes, in the War Room"
            )),
            (Some(false), Some(true)) => Some(format!(
                "a line from your seat already reaches {pane} as far as ask"
            )),
            (Some(false), Some(false)) => None,
        },
    }
}

/// A Yes, applied to the window-wide switches. `None` for a pane change, which
/// the window applies to the pane itself through [`pane_mark`].
pub fn grant_config(change: Change, mcp: &mut McpConfig) -> Option<Outcome> {
    let flip = |on: &mut bool| {
        if *on {
            Outcome::AlreadySo
        } else {
            *on = true;
            Outcome::Done
        }
    };
    match change {
        Change::OpenPane | Change::ClosePane | Change::AskLine => None,
        Change::WritesOn => Some(flip(&mut mcp.writable)),
        Change::EventsOn => Some(flip(&mut mcp.events)),
        Change::ExposeShells => Some(if matches!(mcp.expose, Expose::All) {
            Outcome::AlreadySo
        } else {
            mcp.expose = Expose::All;
            Outcome::Done
        }),
    }
}

/// The mark a Yes leaves on the pane a pane change names, and whether that is
/// a change: `(off_limits, outcome)`.
pub fn pane_mark(change: Change, off_limits_now: bool) -> (bool, Outcome) {
    let want = change == Change::ClosePane;
    let outcome = if off_limits_now == want {
        Outcome::AlreadySo
    } else {
        Outcome::Done
    };
    (want, outcome)
}

/// The line typed into the asking agent's terminal when a person answers.
///
/// Signed with the session's tag, like every line the bench types, so a
/// briefed agent can tell an answer its operator gave from text that merely
/// claims to be one. ONE line whatever is in it.
pub fn answer_line(tag: Option<&str>, ask: &Ask, answer: Answer, outcome: Outcome) -> String {
    let sign = match tag {
        Some(t) => format!("[workbench:{t}]"),
        None => "[workbench]".to_string(),
    };
    let what = match ask.pid {
        Some(p) => format!("{} {p}", ask.change.wire()),
        None => ask.change.wire().to_string(),
    };
    let said = match answer {
        Answer::Yes => "yes",
        Answer::No => "no",
    };
    let result = match outcome {
        Outcome::Done => ask.change.done_words(ask.pid),
        Outcome::Kept => ask.change.kept_words(ask.pid),
        Outcome::AlreadySo => format!("{} already", ask.change.done_words(ask.pid)),
        Outcome::Gone => format!("{} has closed, so nothing changed", pane_words(ask.pid)),
    };
    one_line(&format!("{sign} request_access {what}: {said}, {result}"))
}

/// Every control character becomes a space and runs of whitespace become one.
///
/// The reason and the pane title are text from outside the window; a newline
/// in either would be a second line on the rail, and in the answer line a
/// second command typed into a terminal.
fn one_line(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ask_line_is_a_wire_change_about_one_pane() {
        assert_eq!(Change::parse("ask_line"), Some(Change::AskLine));
        assert!(Change::AskLine.needs_pane());
        assert_eq!(
            grant_config(Change::AskLine, &mut McpConfig::default()),
            None,
            "not a window switch"
        );
    }

    /// A bypass runs between two War Room seats: no pane, an off-limits pane,
    /// an unseated pane and a line that already reaches each leave nothing to
    /// ask, each in its own words.
    #[test]
    fn an_ask_line_needs_two_seats_and_no_line_that_reaches() {
        let on = McpConfig {
            enabled: true,
            ..McpConfig::default()
        };
        let now = |off: Option<bool>, reaches: Option<bool>| Now {
            pane_off_limits: off,
            mcp: &on,
            line_reaches_ask: reaches,
        };
        let why = |off, reaches| nothing_to_ask(Change::AskLine, Some(7), &now(off, reaches));
        assert!(why(None, None).unwrap().contains("no pane with pid 7"));
        assert!(why(Some(true), Some(false)).unwrap().contains("off-limits"));
        assert!(why(Some(false), None).unwrap().contains("not seated"));
        assert!(why(Some(false), Some(true))
            .unwrap()
            .contains("already reaches"));
        assert_eq!(
            why(Some(false), Some(false)),
            None,
            "two seats and no line: the ask stands"
        );
    }

    fn ask(change: Change, pid: Option<u32>) -> Ask {
        Ask {
            change,
            pid,
            title: None,
            why: "reason".into(),
            at: Instant::now(),
            risk: None,
            prompts: crate::access::BYPASS_PROMPTS,
        }
    }

    #[test]
    fn every_change_round_trips_its_wire_name_and_nothing_else_parses() {
        for c in Change::ALL {
            assert_eq!(Change::parse(c.wire()), Some(c));
        }
        // Appearance is not an ask, and neither is a narrowing twin.
        for s in ["writes_off", "theme", "brightness", "", "OPEN_PANE"] {
            assert_eq!(Change::parse(s), None, "{s}");
        }
    }

    #[test]
    fn only_the_pane_changes_and_the_bypass_name_a_pane() {
        let pane: Vec<_> = Change::ALL.into_iter().filter(|c| c.needs_pane()).collect();
        assert_eq!(pane, [Change::OpenPane, Change::ClosePane, Change::AskLine]);
    }

    #[test]
    fn a_reason_is_required_flattened_and_short() {
        assert!(validate_why(None).is_err());
        assert!(validate_why(Some("  \n\t ")).is_err());
        assert_eq!(
            validate_why(Some("read the\nbuild\x1b[31m log")).unwrap(),
            "read the build [31m log"
        );
        let long = vec!["word"; WHY_MAX_WORDS + 1].join(" ");
        assert!(validate_why(Some(&long)).unwrap_err().contains("41 words"));
        let ok = vec!["word"; WHY_MAX_WORDS].join(" ");
        assert!(validate_why(Some(&ok)).is_ok());
    }

    #[test]
    fn an_ask_for_what_is_already_so_is_refused_before_it_reaches_the_rail() {
        let mut cfg = McpConfig::default();
        let now = |off: Option<bool>, cfg: &McpConfig| -> Option<String> {
            nothing_to_ask(
                Change::OpenPane,
                Some(7),
                &Now {
                    pane_off_limits: off,
                    mcp: cfg,
                    line_reaches_ask: None,
                },
            )
        };
        assert!(now(None, &cfg).unwrap().contains("no pane with pid 7"));
        assert!(now(Some(false), &cfg).unwrap().contains("not off-limits"));
        assert_eq!(now(Some(true), &cfg), None);
        let close = |off| {
            nothing_to_ask(
                Change::ClosePane,
                Some(7),
                &Now {
                    pane_off_limits: Some(off),
                    mcp: &McpConfig::default(),
                    line_reaches_ask: None,
                },
            )
        };
        assert!(close(true).unwrap().contains("already off-limits"));
        assert_eq!(close(false), None);
        let window = |c, cfg: &McpConfig| {
            nothing_to_ask(
                c,
                None,
                &Now {
                    pane_off_limits: None,
                    mcp: cfg,
                    line_reaches_ask: None,
                },
            )
        };
        assert_eq!(window(Change::WritesOn, &cfg), None);
        assert_eq!(window(Change::ExposeShells, &cfg), None);
        cfg.events = false;
        assert_eq!(window(Change::EventsOn, &cfg), None);
        cfg.writable = true;
        cfg.expose = Expose::All;
        cfg.events = true;
        for c in [Change::WritesOn, Change::ExposeShells, Change::EventsOn] {
            assert!(window(c, &cfg).is_some(), "{c:?}");
        }
    }

    /// Mutation-tested: `writes_on` flipping the events switch instead fails
    /// this — an earlier version only counted how many switches moved, and
    /// one wrong switch is still one.
    #[test]
    fn a_yes_turns_on_exactly_the_switch_it_names() {
        let off = McpConfig {
            events: false,
            ..McpConfig::default()
        };
        let want = |c: Change| {
            let mut w = off.clone();
            match c {
                Change::WritesOn => w.writable = true,
                Change::ExposeShells => w.expose = Expose::All,
                Change::EventsOn => w.events = true,
                Change::OpenPane | Change::ClosePane | Change::AskLine => unreachable!(),
            }
            w
        };
        for c in [Change::WritesOn, Change::ExposeShells, Change::EventsOn] {
            let mut cfg = off.clone();
            assert_eq!(grant_config(c, &mut cfg), Some(Outcome::Done), "{c:?}");
            assert_eq!(cfg, want(c), "{c:?} moved the wrong switch");
            assert_eq!(grant_config(c, &mut cfg), Some(Outcome::AlreadySo), "{c:?}");
        }
        let mut cfg = McpConfig::default();
        assert_eq!(grant_config(Change::OpenPane, &mut cfg), None);
        assert_eq!(cfg, McpConfig::default());
    }

    #[test]
    fn a_pane_yes_sets_the_mark_the_change_names() {
        assert_eq!(pane_mark(Change::OpenPane, true), (false, Outcome::Done));
        assert_eq!(
            pane_mark(Change::OpenPane, false),
            (false, Outcome::AlreadySo)
        );
        assert_eq!(pane_mark(Change::ClosePane, false), (true, Outcome::Done));
        assert_eq!(
            pane_mark(Change::ClosePane, true),
            (true, Outcome::AlreadySo)
        );
    }

    #[test]
    fn the_answer_is_one_signed_line_that_says_what_is_now_true() {
        let a = ask(Change::OpenPane, Some(4412));
        assert_eq!(
            answer_line(Some("k7f2q9ax"), &a, Answer::Yes, Outcome::Done),
            "[workbench:k7f2q9ax] request_access open_pane 4412: yes, pane 4412 is open to agents"
        );
        assert_eq!(
            answer_line(None, &a, Answer::No, Outcome::Kept),
            "[workbench] request_access open_pane 4412: no, pane 4412 stays off-limits"
        );
        assert!(answer_line(None, &a, Answer::Yes, Outcome::Gone).contains("has closed"));
        let w = ask(Change::WritesOn, None);
        assert_eq!(
            answer_line(Some("t"), &w, Answer::Yes, Outcome::AlreadySo),
            "[workbench:t] request_access writes_on: yes, MCP writes are on already"
        );
        // Nothing in the ask can break the line: the reason is not in it, and
        // the words are ours.
        for c in Change::ALL {
            for o in [
                Outcome::Done,
                Outcome::Kept,
                Outcome::AlreadySo,
                Outcome::Gone,
            ] {
                let line = answer_line(Some("t"), &ask(c, Some(1)), Answer::Yes, o);
                assert!(!line.chars().any(char::is_control), "{line:?}");
            }
        }
    }

    #[test]
    fn a_pane_title_cannot_put_a_second_line_on_the_rail() {
        let words = Change::OpenPane.ask_words(Some("prod\n\x1b[2Jdb"));
        assert_eq!(words, "Let agents see \u{201c}prod [2Jdb\u{201d}");
        assert_eq!(
            Change::ClosePane.ask_words(None),
            "Mark a pane off-limits to agents"
        );
    }

    /// The provenance rule, held where an agent could break it. An ask is
    /// answered by a person's press on the rail and by nothing else; the two
    /// surfaces an agent drives, the MCP server and the control socket, must
    /// never be able to name the functions that answer one.
    #[test]
    fn no_agent_facing_surface_can_answer_an_ask() {
        for (file, source) in [
            ("mcp.rs", include_str!("mcp.rs")),
            ("ctl.rs", include_str!("ctl.rs")),
        ] {
            // Code, not prose: a comment may say where answering lives, and a
            // test may name a key it proves cannot be sent.
            let live = &source[..source.find("\n#[cfg(test)]").unwrap_or(source.len())];
            let code: String = live
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            for forbidden in [
                "rail_answer",
                "grant_config",
                "pane_mark",
                "Answer::",
                "set_off_limits",
                "toggle_off_limits",
                "access_ask",
            ] {
                assert!(
                    !code.contains(forbidden),
                    "{file} names `{forbidden}` — an agent-facing surface can answer an ask \
                     or move the mark"
                );
            }
        }
    }
}
