//! Taking TD's entries out of Claude Code's `settings.json` without touching
//! anyone else's. An entry is TD's only when its command is one TD wrote: the
//! exact path of an owned helper, or the briefing hook's own marker. A group
//! that also holds someone else's hook keeps that hook and its metadata; a
//! group, an event or the `hooks` object is dropped only when this edit is
//! what emptied it. Anything else that mentions a TD helper is reported, and
//! keeps that helper on disk, because removing the helper would break it.
use super::jsonorder::{self, J};

/// The marker `briefhook` appends to the SessionStart command it writes.
pub(crate) const BRIEFING_MARKER: &str = "# terminal-delight:briefing";

/// Names that, appearing in a command TD does not own, mean a hook somebody
/// wrote by hand still depends on a TD helper.
const HELPER_NAMES: [&str; 2] = ["td-agent-hooks", "td-agent-ledger"];

#[derive(Debug, Default, PartialEq)]
pub(crate) struct Stripped {
    /// The new file text, when anything changed.
    pub(crate) text: Option<String>,
    /// `event: command` for each hook removed.
    pub(crate) removed_hooks: Vec<String>,
    pub(crate) removed_allow: Vec<String>,
    /// Commands that mention a TD helper but are not TD's: kept, and they keep
    /// the helper they name.
    pub(crate) foreign_refs: Vec<String>,
}

/// `owned_commands`: exact commands TD wrote (from receipts, and the default
/// helper paths). `briefing`: also remove the briefing hook by its marker.
pub(crate) fn strip(
    text: &str,
    owned_commands: &[String],
    briefing: bool,
) -> Result<Stripped, String> {
    let mut root = jsonorder::parse(text).map_err(|e| format!("not valid JSON: {e}"))?;
    if !matches!(root, J::Obj(_)) {
        return Err("settings are not a JSON object".into());
    }
    // Only a file this module can print back byte for byte is edited, so an
    // edit changes TD's entries and nothing else: escapes, line endings and
    // spacing elsewhere stay exactly as the person or their tools wrote them.
    let faithful = jsonorder::print_like(&root, text) == text;
    let owned = |cmd: &str| {
        owned_commands.iter().any(|c| c == cmd)
            || (briefing && cmd.trim_end().ends_with(BRIEFING_MARKER))
    };
    let mut out = Stripped::default();
    let mut changed = false;

    if let Some(hooks) = root.get_mut("hooks") {
        let J::Obj(events) = hooks else {
            return Err("\"hooks\" is not an object".into());
        };
        let mut emptied_events = Vec::new();
        for (event, groups) in events.iter_mut() {
            let J::Arr(list) = groups else {
                return Err(format!("hooks.{event} is not an array"));
            };
            let before = list.len();
            let mut keep = Vec::with_capacity(before);
            for mut group in std::mem::take(list) {
                let Some(J::Arr(entries)) = group.get_mut("hooks") else {
                    return Err(format!("a hooks.{event} group has no \"hooks\" array"));
                };
                let had = entries.len();
                let mut kept = Vec::with_capacity(had);
                for entry in std::mem::take(entries) {
                    let cmd = match entry.get("command") {
                        None => None,
                        Some(J::Str(s)) => Some(s.clone()),
                        Some(_) => return Err(format!("a hooks.{event} command is not a string")),
                    };
                    let is_command = entry.get("type").and_then(J::as_str) == Some("command");
                    match cmd {
                        Some(c) if is_command && owned(&c) => {
                            out.removed_hooks.push(format!("{event}: {c}"));
                            changed = true;
                        }
                        Some(c) => {
                            if HELPER_NAMES.iter().any(|n| c.contains(n))
                                || c.contains(BRIEFING_MARKER)
                            {
                                out.foreign_refs.push(format!("{event}: {c}"));
                            }
                            kept.push(entry);
                        }
                        None => kept.push(entry),
                    }
                }
                let emptied_here = had > 0 && kept.is_empty();
                *entries = kept;
                if !emptied_here {
                    keep.push(group);
                }
            }
            *list = keep;
            if before > 0 && list.is_empty() {
                emptied_events.push(event.clone());
            }
        }
        events.retain(|(k, _)| !emptied_events.contains(k));
        if events.is_empty() && !emptied_events.is_empty() {
            root.remove("hooks");
        }
    }

    if let Some(J::Arr(allow)) = root.get_mut("permissions").and_then(|p| p.get_mut("allow")) {
        let before = allow.len();
        allow.retain(|v| match v.as_str() {
            Some(s) if s == "mcp__terminal-delight" || s.starts_with("mcp__terminal-delight__") => {
                out.removed_allow.push(s.to_string());
                false
            }
            _ => true,
        });
        changed |= allow.len() != before;
    }

    if changed {
        if !faithful {
            return Err("TD's entries are in it, but its formatting cannot be kept byte for byte, so TD will not rewrite it".into());
        }
        out.text = Some(jsonorder::print_like(&root, text));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OURS: &str = "/h/.local/bin/td-agent-hooks";

    fn cmds() -> Vec<String> {
        vec![
            OURS.to_string(),
            "/h/.local/bin/td-agent-ledger".to_string(),
        ]
    }

    #[test]
    fn removes_only_owned_entries_and_keeps_order_and_neighbours() {
        let text = r#"{
  "theme": "dark",
  "hooks": {
    "Stop": [
      {
        "matcher": "*",
        "timeout": 42,
        "hooks": [
          {
            "type": "command",
            "command": "/h/.local/bin/td-agent-hooks"
          },
          {
            "type": "command",
            "command": "unrelated-tool"
          }
        ]
      }
    ],
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "'/h/.local/bin/terminal-delight' briefing 2>/dev/null || true  # terminal-delight:briefing",
            "timeout": 15
          }
        ]
      }
    ]
  },
  "permissions": {
    "allow": [
      "Bash(ls)",
      "mcp__terminal-delight"
    ]
  }
}
"#;
        let s = strip(text, &cmds(), true).unwrap();
        let want = r#"{
  "theme": "dark",
  "hooks": {
    "Stop": [
      {
        "matcher": "*",
        "timeout": 42,
        "hooks": [
          {
            "type": "command",
            "command": "unrelated-tool"
          }
        ]
      }
    ]
  },
  "permissions": {
    "allow": [
      "Bash(ls)"
    ]
  }
}
"#;
        assert_eq!(s.text.as_deref(), Some(want));
        assert_eq!(s.removed_hooks.len(), 2);
        assert_eq!(s.removed_allow, vec!["mcp__terminal-delight"]);
        assert!(s.foreign_refs.is_empty());
    }

    #[test]
    fn unfaithful_formatting_refuses_only_when_there_is_something_to_remove() {
        let ours = "{\"hooks\":{\"Stop\":[{\"hooks\":[{\"type\":\"command\",\"command\":\"/h/.local/bin/td-agent-hooks\"}]}]},\"k\":\"\\u0061\"}";
        assert!(strip(ours, &cmds(), false).is_err());
        let crlf = "{\r\n  \"hooks\": {\"Stop\": [{\"hooks\": [{\"type\": \"command\", \"command\": \"/h/.local/bin/td-agent-hooks\"}]}]}\r\n}\r\n";
        assert!(strip(crlf, &cmds(), false).is_err());
        assert_eq!(
            strip("{ \"k\": \"\\u0061\" }\r\n", &cmds(), false)
                .unwrap()
                .text,
            None
        );
    }

    #[test]
    fn nothing_owned_means_no_rewrite() {
        let text = "{ \"theme\": \"keep\" }\n";
        assert_eq!(strip(text, &cmds(), true).unwrap().text, None);
    }

    #[test]
    fn a_lookalike_is_kept_and_reported() {
        let text = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"/h/.local/bin/td-agent-hooks --custom"}]}]}}"#;
        let s = strip(text, &cmds(), false).unwrap();
        assert_eq!(s.text, None);
        assert_eq!(s.foreign_refs.len(), 1);
    }

    #[test]
    fn an_event_that_was_already_empty_stays() {
        let text = r#"{"hooks":{"Stop":[],"Notification":[{"hooks":[{"type":"command","command":"/h/.local/bin/td-agent-hooks"}]}]}}"#;
        let s = strip(text, &cmds(), false).unwrap();
        assert_eq!(s.text.as_deref(), Some(r#"{"hooks":{"Stop":[]}}"#));
    }

    #[test]
    fn malformed_shapes_are_refused() {
        for text in [
            "[]",
            "{\"hooks\":[]}",
            "{\"hooks\":{\"Stop\":{}}}",
            "{\"hooks\":{\"Stop\":[{}]}}",
            "{bad",
        ] {
            assert!(strip(text, &cmds(), true).is_err(), "{text}");
        }
    }

    #[test]
    fn briefing_is_left_unless_asked() {
        let text = r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"x  # terminal-delight:briefing"}]}]}}"#;
        let s = strip(text, &cmds(), false).unwrap();
        assert_eq!(s.text, None);
        assert_eq!(s.foreign_refs.len(), 1);
    }
}
