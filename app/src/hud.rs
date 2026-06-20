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
    #[default]
    Idle,
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
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AgentState::Working => "working",
            AgentState::Blocked => "blocked",
            AgentState::Error => "error",
            AgentState::Finished => "done",
            AgentState::Idle => "idle",
        }
    }

    /// True for the states a human should look at first (the whole point of the
    /// HUD): an agent waiting on you, or one that has errored.
    pub fn needs_you(self) -> bool {
        matches!(self, AgentState::Blocked | AgentState::Error)
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

/// Parse the visible bottom rows of an agent pane into an [`AgentStatus`].
///
/// `rows` is the live screen, top-to-bottom. We do not assume a fixed format:
/// we look for the richest status row and pull whatever is present, degrading
/// gracefully (a bare "esc to interrupt" still reads as Working, just without
/// metrics). `Finished` is *not* decided here — the caller layers it on from
/// the pane's unacknowledged bell.
pub fn parse_status_line(rows: &[String]) -> AgentStatus {
    let lower: Vec<String> = rows.iter().map(|r| r.to_ascii_lowercase()).collect();

    // Working: stock Claude/Codex print "esc to interrupt"; Parker's custom
    // status line says "still thinking …". Accept both (broader than the bell's
    // detector on purpose — this never feeds the bell).
    let working = lower.iter().any(|l| {
        l.contains("esc to interrupt")
            || l.contains("interrupt)")
            || l.contains("still thinking")
            || (l.contains("tokens") && l.contains("thinking"))
    });

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
    } else {
        AgentState::Idle
    };
    st
}

/// Does a (lowercased) row look like the agent is waiting on a human decision?
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

    #[test]
    fn idle_when_nothing_matches() {
        let st = parse_status_line(&rows(&["pbrown@host:~/proj$ "]));
        assert_eq!(st.state, AgentState::Idle);
        assert_eq!(st.turn_tokens, None);
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
