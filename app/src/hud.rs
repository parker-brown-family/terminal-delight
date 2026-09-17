//! Agent-wall HUD — the read-only "scoreboard" over a wall of agent panes.
//!
//! Coding agents print their own progress on the bottom line of their TUI —
//! e.g. Claude: `✷ Accomplishing… (13m 18s · ↓ 59.0k tokens · still thinking
//! with high effort)`. The hard part of watching many agents is knowing, at a
//! glance, *who is working, who is blocked on you, and how much they've spent*.
//! This module turns that already-on-screen line into a compact [`AgentStatus`]
//! the HUD renders.
//!
//! Pure logic on purpose — no gpui — so the parser is unit-tested. `pane.rs`
//! feeds it the live bottom rows (`agent_status`); `main.rs` paints it.

/// What an agent pane is doing right now, in priority order for the wall HUD.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AgentState {
    /// A turn is running (spinner / "esc to interrupt" / "still thinking").
    Working,
    /// Stopped, waiting on the human — a permission prompt or a question.
    Blocked,
    /// A rate-limit / API error is visible.
    Error,
    /// Turn finished with an unacknowledged "agent finished" alert.
    Finished,
    /// Agent at rest with nothing pending (or a plain shell).
    ///
    /// **The parser can now claim this, and the reason it once could not is
    /// worth keeping.** From rows alone a resting agent and a screen this code
    /// cannot read were the same picture — there were positive tests for
    /// working, errored and blocked and none at all for at-rest, so rest was
    /// only ever the absence of evidence. [`is_at_rest`] gives it evidence of
    /// its own: the idle furniture an agent CLI prints at its prompt. Recognised
    /// rest is a finding; unrecognised anything is still [`AgentState::Unknown`],
    /// and the two must never merge again.
    #[default]
    Idle,
    /// The pane is an agent and its screen matched no rule here.
    ///
    /// This used to be `Idle`, and that conflation is the defect this variant
    /// exists to end: an attention surface built on it would answer "nothing
    /// needs you" for a pane it simply could not read. Unknown is shown, is
    /// never counted as wanting a human, and is never decorated as activity.
    Unknown,
}

impl AgentState {
    /// A single-glyph badge for the scoreboard.
    pub fn badge(self) -> &'static str {
        match self {
            AgentState::Working => "\u{25b6}",  // ▶
            AgentState::Blocked => "\u{23f8}",  // ⏸
            AgentState::Error => "\u{2715}",    // ✕
            AgentState::Finished => "\u{2713}", // ✓
            AgentState::Idle => "\u{00b7}",     // ·
            AgentState::Unknown => "?",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AgentState::Working => "working",
            AgentState::Blocked => "blocked",
            AgentState::Error => "error",
            AgentState::Finished => "done",
            AgentState::Idle => "idle",
            AgentState::Unknown => "unknown",
        }
    }

    /// True for the states a human should look at first (the whole point of the
    /// HUD): an agent waiting on you, or one that has errored.
    ///
    /// `Unknown` is deliberately not one of them. "We could not tell" is not a
    /// yes, and a count that treats it as one turns every parser gap into an
    /// alarm. It is shown with its own mark instead, which is the honest shape:
    /// visible, and not counted.
    pub fn needs_you(self) -> bool {
        matches!(self, AgentState::Blocked | AgentState::Error)
    }

    /// Nothing is happening that a person should be shown as movement.
    ///
    /// Both resting and unreadable panes are quiet: a glow or a live badge on a
    /// screen nobody could parse is the interface asserting activity it has not
    /// observed.
    pub fn is_quiet(self) -> bool {
        matches!(self, AgentState::Idle | AgentState::Unknown)
    }
}

/// Fold in the one thing the rows cannot show: whether a finish bell is
/// unacknowledged.
///
/// The bell is a pane's own state, so the parser never sees it and the caller
/// has to apply it. It promotes only a QUIET state, because a bell says a turn
/// ended and says nothing about a screen that is currently working, blocked or
/// errored — those are live and outrank a stale finish.
///
/// **Why this is a function and not two lines at the call site.** It was two
/// lines at the call site, comparing against `Idle`, and adding [`AgentState::Unknown`]
/// made that comparison unreachable: the parser stopped returning `Idle`, so a
/// finished agent whose screen carried nothing matchable reported `Unknown` and
/// the done state quietly went to zero. Nothing failed — no test covered the
/// pairing, because the bell lives on the pane and the parser is pure. Here, it
/// is testable.
pub fn with_bell(state: AgentState, bell: bool) -> AgentState {
    if bell && state.is_quiet() {
        AgentState::Finished
    } else {
        state
    }
}

/// One agent pane's live status, parsed from its bottom-of-screen line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AgentStatus {
    pub state: AgentState,
    /// The gerund the agent shows, e.g. "Accomplishing".
    pub gerund: Option<String>,
    /// Elapsed time on the current turn, kept as the agent's own string
    /// ("13m 18s") — we only display it.
    pub elapsed: Option<String>,
    /// Tokens reported for the current turn (normalised to a count).
    pub turn_tokens: Option<u64>,
    /// Effort level if the agent prints one ("low" / "medium" / "high").
    pub effort: Option<String>,
}

impl AgentStatus {
    /// Is a turn actively running? (drives the live timer + token accrual)
    pub fn working(&self) -> bool {
        self.state == AgentState::Working
    }
}

pub fn parse_status_line(rows: &[String]) -> AgentStatus {
    let lower: Vec<String> = rows.iter().map(|r| r.to_ascii_lowercase()).collect();

    // Working: stock Claude/Codex print "esc to interrupt"; Parker's custom
    // status line says "still thinking …". Accept both (broader than the bell's
    // detector on purpose — this never feeds the bell).
    // One rule, shared with `TerminalView::agent_is_thinking` — they were two
    // copies of the same heuristic and only one of them would have been fixed.
    let working = lower.iter().any(|l| crate::screenread::row_is_working(l));

    // The richest status row: prefer one carrying a "(… tokens …)" group.
    let status_row = rows
        .iter()
        .zip(&lower)
        .find(|(_, l)| {
            l.contains("tokens")
                && (l.contains('\u{00b7}') || l.contains("thinking") || l.contains("interrupt"))
        })
        .or_else(|| {
            rows.iter()
                .zip(&lower)
                .find(|(_, l)| l.contains("esc to interrupt"))
        })
        .map(|(r, _)| r.as_str());

    let mut st = AgentStatus::default();

    if let Some(rowtext) = status_row {
        if let Some(open) = rowtext.find('(') {
            // gerund = the words before the '(' minus the spinner glyph + ellipsis
            let head = rowtext[..open].trim();
            let g = head
                .trim_start_matches(|c: char| !c.is_alphanumeric())
                .trim_end_matches(['\u{2026}', '.', ' '])
                .trim();
            if !g.is_empty() && g.chars().count() <= 24 {
                st.gerund = Some(g.to_string());
            }
            let inner = rowtext[open + 1..].split(')').next().unwrap_or("");
            for field in inner.split('\u{00b7}') {
                let f = field.trim();
                let fl = f.to_ascii_lowercase();
                if fl.contains("tokens") {
                    st.turn_tokens = parse_tokens(f);
                } else if is_time(&fl) {
                    st.elapsed = Some(f.to_string());
                } else if fl.contains("effort") {
                    st.effort = extract_effort(&fl);
                }
            }
        } else if rowtext.to_ascii_lowercase().contains("tokens") {
            st.turn_tokens = parse_tokens(rowtext);
        }
    }

    st.state = if working {
        AgentState::Working
    } else if lower.iter().any(|l| has_error(l)) {
        AgentState::Error
    } else if lower.iter().any(|l| is_blocked_prompt(l)) {
        AgentState::Blocked
    } else if lower.iter().any(|l| is_at_rest(l)) {
        // The agent's own idle furniture is on screen. It is not working, it has
        // not errored and it is not asking — it is sitting at its prompt, which
        // is a thing we can recognise rather than a thing we failed to.
        AgentState::Idle
    } else {
        // Nothing matched. That is not the same as an agent at rest, and saying
        // so was the bug: a queue built on this reported "nothing needs you" for
        // a pane whose screen it could not read. The caller knows whether this
        // pane is even an agent; this function only knows what it failed to
        // recognise.
        AgentState::Unknown
    };
    st
}

/// Does a (lowercased) row look like the agent is waiting on a human decision?
/// Is this the furniture an agent CLI shows while it sits at its prompt doing
/// nothing?
///
/// **The missing positive test.** The parser had one for working, one for
/// errored and one for blocked, and nothing for at-rest — so a resting agent
/// matched none of them and fell to [`AgentState::Unknown`]. That was correct in
/// the narrow sense (nothing was recognised) and useless in practice: on a fleet
/// of two dozen mostly-idle panes the rail's unreadable lane held almost all of
/// them, which is not a report about anything.
///
/// **Safe by ordering, which is why it can be a heuristic at all.** This is
/// consulted LAST, after working, error and blocked have each had their say. A
/// marker that fires wrongly can therefore only turn an Unknown into a rest — it
/// can never hide a prompt, a wall or a live turn, because those are decided
/// before control reaches here. The failure mode of a bad needle is a pane that
/// stops being listed as unreadable, not a pane that stops asking for you.
///
/// Anything unrecognised still falls through to Unknown. Adding a needle can
/// only shrink that set, and the collapsed "N panes could not be read" line is
/// what keeps the remainder visible instead of silently absorbed.
fn is_at_rest(l: &str) -> bool {
    const NEEDLES: [&str; 6] = [
        "? for shortcuts",
        "shift+tab to cycle",
        "auto-accept edits on",
        "plan mode on",
        "bypassing permissions",
        "send with enter",
    ];
    NEEDLES.iter().any(|n| l.contains(n))
}

fn is_blocked_prompt(l: &str) -> bool {
    const NEEDLES: [&str; 8] = [
        "do you want to proceed",
        "do you want to",
        "\u{276f} 1.", // ❯ 1.  (a selected menu option)
        "(y/n)",
        "would you like to",
        "press enter to continue",
        "waiting for your",
        "approve this",
    ];
    NEEDLES.iter().any(|n| l.contains(n))
}

/// Does a (lowercased) row look like a rate-limit / API error?
fn has_error(l: &str) -> bool {
    const NEEDLES: [&str; 6] = [
        "rate limit",
        "api error",
        "overloaded",
        "too many requests",
        "error: connection",
        "529",
    ];
    NEEDLES.iter().any(|n| l.contains(n))
}

/// A field reads as a time if a digit is immediately followed by h/m/s.
fn is_time(s: &str) -> bool {
    let b = s.as_bytes();
    for i in 0..b.len() {
        if b[i].is_ascii_digit() {
            let mut j = i + 1;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j < b.len() && matches!(b[j], b'h' | b'm' | b's') {
                return true;
            }
        }
    }
    false
}

fn extract_effort(fl: &str) -> Option<String> {
    if fl.contains("high") {
        Some("high".into())
    } else if fl.contains("medium") || fl.contains("med ") {
        Some("medium".into())
    } else if fl.contains("low") {
        Some("low".into())
    } else {
        None
    }
}

/// Pull a token count out of a "`↓ 59.0k tokens`"-style field. Handles k/M
/// suffixes, thousands commas, and decimals. No regex dependency.
pub fn parse_tokens(s: &str) -> Option<u64> {
    let low = s.to_ascii_lowercase();
    let idx = low.find("tokens")?;
    let prefix = s[..idx].trim_end();
    let run: String = prefix
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | 'k' | 'K' | 'm' | 'M'))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if run.is_empty() {
        return None;
    }
    let (num_part, mult): (&str, f64) = match run.chars().last() {
        Some('k' | 'K') => (&run[..run.len() - 1], 1_000.),
        Some('m' | 'M') => (&run[..run.len() - 1], 1_000_000.),
        _ => (run.as_str(), 1.),
    };
    let cleaned: String = num_part.chars().filter(|c| *c != ',').collect();
    let val: f64 = cleaned.parse().ok()?;
    if val < 0. {
        return None;
    }
    Some((val * mult).round() as u64)
}

/// Compact a token count for display: `840`, `59.0k`, `1.2M`.
pub fn fmt_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn rows(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn parses_parkers_real_status_line() {
        // The exact format from the live wall (image #2).
        let r = rows(&[
            "  some agent output above",
            "\u{2737} Accomplishing\u{2026} (13m 18s \u{00b7} \u{2193} 59.0k tokens \u{00b7} still thinking with high effort)",
            "\u{276f} ",
        ]);
        let st = parse_status_line(&r);
        assert_eq!(st.state, AgentState::Working);
        assert_eq!(st.gerund.as_deref(), Some("Accomplishing"));
        assert_eq!(st.elapsed.as_deref(), Some("13m 18s"));
        assert_eq!(st.turn_tokens, Some(59_000));
        assert_eq!(st.effort.as_deref(), Some("high"));
    }

    #[test]
    fn stock_esc_to_interrupt_is_working() {
        let st = parse_status_line(&rows(&[
            "\u{273b} Cogitating (45s \u{00b7} 1,234 tokens \u{00b7} esc to interrupt)",
        ]));
        assert_eq!(st.state, AgentState::Working);
        assert_eq!(st.turn_tokens, Some(1_234));
        assert_eq!(st.elapsed.as_deref(), Some("45s"));
    }

    #[test]
    fn bare_interrupt_without_metrics_still_working() {
        let st = parse_status_line(&rows(&["(esc to interrupt)"]));
        assert_eq!(st.state, AgentState::Working);
        assert_eq!(st.turn_tokens, None);
    }

    #[test]
    fn blocked_on_a_permission_prompt() {
        let st = parse_status_line(&rows(&[
            "Bash(rm -rf build)",
            "Do you want to proceed?",
            "\u{276f} 1. Yes",
            "  2. No",
        ]));
        assert_eq!(st.state, AgentState::Blocked);
    }

    #[test]
    fn error_when_rate_limited() {
        let st = parse_status_line(&rows(&["API Error: 529 overloaded, retrying"]));
        assert_eq!(st.state, AgentState::Error);
    }

    /// The test that used to assert the defect.
    ///
    /// It read `idle_when_nothing_matches` and passed a shell prompt, which is
    /// three different claims wearing one value: a shell, an agent at rest, and
    /// a screen this parser cannot read. The parser can only honestly make the
    /// third.
    #[test]
    fn unknown_when_nothing_matches() {
        let st = parse_status_line(&rows(&["pbrown@host:~/proj$ "]));
        assert_eq!(st.state, AgentState::Unknown);
        assert_eq!(st.turn_tokens, None);
    }

    /// An agent sitting at its own prompt is recognised as resting rather than
    /// falling through to unreadable. Without this the rail's unknown lane held
    /// almost every pane on a mostly-idle fleet, which is not a report.
    #[test]
    fn an_agent_at_its_own_prompt_is_resting_not_unreadable() {
        for footer in [
            "? for shortcuts",
            "\u{23f5}\u{23f5} auto-accept edits on (shift+tab to cycle)",
            "\u{23f8} plan mode on",
            "send with enter",
        ] {
            let st = parse_status_line(&rows(&["> ", footer]));
            assert_eq!(
                st.state,
                AgentState::Idle,
                "{footer:?} is an agent at rest, not a screen we failed to read"
            );
        }
    }

    /// **The ordering is what makes the rest test safe**, so it is asserted
    /// rather than left to the reader of the `if` chain. Every louder state is
    /// decided first, so a rest marker that fires wrongly can only turn an
    /// Unknown into a rest — it can never hide a live turn, a wall, or a
    /// question. Each case below carries the idle footer AND something louder,
    /// and the louder thing must win.
    #[test]
    fn a_rest_marker_never_outranks_a_turn_a_wall_or_a_question() {
        let idle = "? for shortcuts";
        let cases = [
            (
                vec![
                    "\u{2733} Refactoring\u{2026} (2m \u{b7} esc to interrupt)",
                    idle,
                ],
                AgentState::Working,
            ),
            (vec!["API Error: overloaded_error", idle], AgentState::Error),
            (vec!["Do you want to proceed?", idle], AgentState::Blocked),
        ];
        for (screen, want) in cases {
            assert_eq!(
                parse_status_line(&rows(&screen)).state,
                want,
                "the idle footer must not outrank {want:?}"
            );
        }
    }

    #[test]
    fn an_unreadable_screen_is_not_counted_as_wanting_you() {
        let st = parse_status_line(&rows(&["\u{2588}\u{2588} garbled \u{2588}\u{2588}"]));
        assert_eq!(st.state, AgentState::Unknown);
        assert!(!st.state.needs_you(), "we could not tell is not a yes");
        assert!(st.state.is_quiet(), "and it is not drawn as movement");
    }

    /// The three states the issue says must stop being one value.
    #[test]
    fn rest_unreadable_and_working_are_three_different_answers() {
        let working = parse_status_line(&rows(&[
            "\u{2733} Refactoring\u{2026} (2m \u{b7} esc to interrupt)",
        ]));
        let unreadable = parse_status_line(&rows(&["wat"]));
        assert_eq!(working.state, AgentState::Working);
        assert_eq!(unreadable.state, AgentState::Unknown);
        assert_ne!(
            unreadable.state,
            AgentState::Idle,
            "a screen we cannot read must never assert rest"
        );
        assert_eq!(AgentState::default(), AgentState::Idle);
    }

    #[test]
    fn a_bell_promotes_a_quiet_state_whatever_kind_of_quiet_it_is() {
        // The case the Unknown change broke: nothing matched, bell ringing.
        assert_eq!(
            with_bell(AgentState::Unknown, true),
            AgentState::Finished,
            "an unreadable screen with a finish bell is finished, not unknown"
        );
        assert_eq!(with_bell(AgentState::Idle, true), AgentState::Finished);
    }

    #[test]
    fn a_bell_never_overrides_something_live() {
        for live in [AgentState::Working, AgentState::Blocked, AgentState::Error] {
            assert_eq!(
                with_bell(live, true),
                live,
                "a stale finish must not mask what the screen says now"
            );
        }
    }

    #[test]
    fn no_bell_changes_nothing() {
        for st in [
            AgentState::Working,
            AgentState::Blocked,
            AgentState::Error,
            AgentState::Finished,
            AgentState::Idle,
            AgentState::Unknown,
        ] {
            assert_eq!(with_bell(st, false), st);
        }
    }

    #[test]
    fn token_suffixes_and_fmt() {
        assert_eq!(parse_tokens("\u{2193} 59.0k tokens"), Some(59_000));
        assert_eq!(parse_tokens("2.1M tokens"), Some(2_100_000));
        assert_eq!(parse_tokens("840 tokens"), Some(840));
        assert_eq!(parse_tokens("1,234,567 tokens"), Some(1_234_567));
        assert_eq!(parse_tokens("no number here tokens"), None);
        assert_eq!(fmt_tokens(840), "840");
        assert_eq!(fmt_tokens(59_000), "59.0k");
        assert_eq!(fmt_tokens(2_100_000), "2.1M");
    }
}
