//! Agent-finished system notifications, omarchy-shaped.
//!
//! When an agent pane rings while you are NOT looking at it, TD posts a desktop
//! notification — `tab → pane` as the title, a recap of the agent's last words
//! as the body — through `notify-send`, which omarchy's Quickshell daemon
//! renders (its capabilities include `actions`). Clicking the notification
//! invokes the `default` action; `notify-send` then prints the action name and
//! exits, the workspace sees it and ZIPS there: focuses this TD window via the
//! Hyprland socket, activates the tab, focuses the pane.
//!
//! The recap comes from the agent's own transcript (Claude Code's JSONL — the
//! last assistant message), because the grid under the status line is a poor
//! summary of a finished turn. Panes without a resolvable transcript fall back
//! to their last non-empty rows.
use std::path::Path;

/// Max bytes of transcript tail to scan for the last assistant message. Turns
/// are appended, so the last assistant entry lives at the end; 256KB spans even
/// a verbose multi-block reply.
const TAIL: u64 = 256 * 1024;

/// The last assistant utterance in a Claude Code transcript, recap-trimmed.
pub fn recap_from_transcript(path: &Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let start = len.saturating_sub(TAIL);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = String::new();
    f.read_to_string(&mut buf).ok()?;
    // A mid-line seek can open on a torn line; the line iterator just yields a
    // JSON fragment that fails to parse, which is the behaviour we want anyway.
    last_assistant_text(&buf).map(|t| trim_recap(&t, 220))
}

/// The text of the LAST assistant message in a chunk of Claude Code JSONL.
/// Concatenates the entry's `text` content blocks (tool_use blocks carry no
/// prose); skips entries of any other type. Pure, so the parse is testable.
pub fn last_assistant_text(jsonl: &str) -> Option<String> {
    for line in jsonl.lines().rev() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if v["type"].as_str() != Some("assistant") {
            continue;
        }
        let mut out = String::new();
        if let Some(blocks) = v["message"]["content"].as_array() {
            for b in blocks {
                if b["type"].as_str() == Some("text") {
                    if let Some(t) = b["text"].as_str() {
                        if !out.is_empty() {
                            out.push(' ');
                        }
                        out.push_str(t);
                    }
                }
            }
        }
        if !out.trim().is_empty() {
            return Some(out);
        }
        // an assistant entry that was pure tool_use: keep walking back
    }
    None
}

/// Squash a recap for a one-line-ish notification body: collapse all
/// whitespace, drop markdown heading/emphasis clutter, cap at `max` chars on a
/// char boundary with an ellipsis.
pub fn trim_recap(s: &str, max: usize) -> String {
    let mut flat = String::with_capacity(s.len().min(max * 2));
    let mut last_ws = true;
    for ch in s.chars() {
        let ch = if ch.is_whitespace() { ' ' } else { ch };
        if ch == ' ' {
            if last_ws {
                continue;
            }
            last_ws = true;
        } else {
            last_ws = false;
        }
        if !matches!(ch, '#' | '*' | '`') {
            flat.push(ch);
        }
    }
    let flat = flat.trim();
    if flat.chars().count() <= max {
        return flat.to_string();
    }
    let cut: String = flat.chars().take(max).collect();
    format!("{}…", cut.trim_end())
}

/// The `notify-send` argv (after the program name). `-A default=…` makes the
/// daemon offer a default action; when the user clicks, notify-send prints
/// `default` on stdout and exits — that is the click-to-zip signal. Pure for
/// tests; the caller appends nothing.
pub fn notify_args(title: &str, body: &str) -> Vec<String> {
    vec![
        "-a".into(),
        "Terminal Delight".into(),
        "-i".into(),
        "utilities-terminal".into(),
        "-t".into(),
        "12000".into(),
        "-A".into(),
        "default=Jump to pane".into(),
        "--".into(),
        title.into(),
        body.into(),
    ]
}

/// The notification title: `tab → pane`, Parker's juggling format — which tab
/// group to look at, then which pane inside it.
pub fn notify_title(tab: &str, pane: &str) -> String {
    format!("{tab} → {pane}")
}

/// Flavour the title with WHY the agent stopped, matching the tab glyphs:
/// ❓ it needs a human (a prompt is up — outranks everything: the turn is not
/// over, it's yours), ❌ it hit a wall (error banner at ring time), ✅ a clean
/// finish. The glyph leads so the toast is scannable from across the room.
pub fn flavor_title(base: &str, needs_input: bool, blocked: bool) -> String {
    if needs_input {
        format!("❓ {base} — your move")
    } else if blocked {
        format!("❌ {base} — blocked")
    } else {
        format!("✅ {base}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"type":"user","message":{"content":[{"type":"text","text":"do the thing"}]}}
{"type":"assistant","message":{"content":[{"type":"text","text":"Working on it."}]}}
{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{}}]}}
{"type":"assistant","message":{"content":[{"type":"text","text":"Done — merged #223"},{"type":"text","text":"and deployed."}]}}
{"type":"user","message":{"content":[{"type":"text","text":"ty"}]}}"#;

    /// The recap is the LAST assistant prose — not the first, not the user's
    /// message, and a trailing pure-tool_use assistant entry doesn't blank it.
    #[test]
    fn recap_takes_the_last_assistant_prose() {
        let t = last_assistant_text(SAMPLE).unwrap();
        assert_eq!(t, "Done — merged #223 and deployed.");
        // a torn first line (mid-seek) parses as garbage and is skipped
        let torn = format!("ge\":{{\"content\":[]}}}}\n{SAMPLE}");
        assert_eq!(last_assistant_text(&torn).unwrap(), t);
        assert_eq!(last_assistant_text("{\"type\":\"user\"}"), None);
    }

    #[test]
    fn recap_trim_flattens_and_caps() {
        let s = "## Big\n\n*win*:  the   `thing`\nshipped";
        assert_eq!(trim_recap(s, 220), "Big win: the thing shipped");
        let long = "x".repeat(300);
        let t = trim_recap(&long, 220);
        assert_eq!(t.chars().count(), 221); // 220 + ellipsis
        assert!(t.ends_with('…'));
    }

    /// The argv shape notify-send needs: an action named `default` (that is the
    /// name mako/Quickshell invoke on a plain click), title and body last,
    /// behind a `--` so a title starting with `-` can't read as a flag.
    #[test]
    fn notify_args_carry_the_default_action() {
        let a = notify_args("TD → CLAUDE", "done");
        let i = a.iter().position(|s| s == "-A").unwrap();
        assert_eq!(a[i + 1], "default=Jump to pane");
        assert_eq!(&a[a.len() - 3..], ["--", "TD → CLAUDE", "done"]);
        assert_eq!(notify_title("RESEARCH", "CLAUDE"), "RESEARCH → CLAUDE");
    }

    /// The stop-reason flavour mirrors the tab glyphs, and needs-input outranks
    /// blocked: a prompt IS the wall, and answering it is the fix.
    #[test]
    fn stop_reason_flavours_the_title() {
        assert_eq!(flavor_title("A → B", false, false), "✅ A → B");
        assert_eq!(flavor_title("A → B", false, true), "❌ A → B — blocked");
        assert_eq!(flavor_title("A → B", true, false), "❓ A → B — your move");
        assert_eq!(flavor_title("A → B", true, true), "❓ A → B — your move");
    }
}
