//! Word labels for native controls. This is deliberately not a Unicode filter:
//! terminal output, names, filenames and authored content never pass through it.

/// Icons embedded beside an existing word label add no information. An icon
/// alone needs its action spelled out instead.
pub(super) fn label_in(source: &str, l: crate::lang::Lang) -> String {
    let source = source.trim();
    let source = source.strip_prefix("</>").unwrap_or(source).trim();
    if let Some((_, word)) = ICONS.iter().find(|(icon, _)| *icon == source) {
        return word(l).to_string();
    }
    if source.is_ascii() {
        return source.to_string();
    }
    let mut text = source.to_string();
    for (icon, _) in ICONS {
        if !icon.is_ascii() && text.contains(icon) {
            text = text.replace(icon, "");
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

type Caption = fn(crate::lang::Lang) -> &'static str;

const ICONS: &[(&str, Caption)] = &[
    ("📜", |l| l.strings().parts.ctl_artifacts),
    ("⚓", |l| l.strings().parts.ctl_anchor),
    ("↻", |l| l.strings().parts.ctl_refresh),
    ("‖", |l| l.strings().parts.ctl_paused),
    ("⏸", |l| l.strings().parts.ctl_paused),
    ("☾", |l| l.strings().parts.ctl_rest),
    ("⬇", |l| l.strings().parts.ctl_download),
    ("⏎", |_| "Enter"),
    ("↵", |_| "Enter"),
    ("🎨", |l| l.strings().parts.ctl_look),
    ("👓", |l| l.strings().parts.pane_overflow_focus),
    ("👤", |l| l.strings().parts.ctl_turns),
    ("🤖", |l| l.strings().parts.ctl_agents),
    ("\u{f06a9}", |l| l.strings().parts.ctl_agents),
    ("\u{f04b2}", |l| l.strings().parts.ctl_asleep),
    ("\u{f0594}", |l| l.strings().parts.ctl_rest),
    ("🪦", |l| l.strings().parts.ctl_recover),
    ("🔔", |l| l.strings().parts.ctl_notices),
    ("🐞", |l| l.strings().parts.ctl_report),
    ("🐛", |l| l.strings().parts.ctl_report),
    ("🧩", |l| l.strings().parts.ctl_plugins),
    ("📦", |l| l.strings().parts.ctl_artifacts),
    ("🗂", |l| l.strings().parts.ctl_artifacts),
    ("▤", |l| l.strings().parts.ctl_artifacts),
    ("📊", |l| l.strings().parts.ctl_usage),
    ("⛭", |l| l.strings().parts.ctl_gauges),
    ("⚙", |l| l.strings().parts.ctl_settings),
    ("🖥", |l| l.strings().parts.ctl_display),
    ("☀", |l| l.strings().parts.ctl_light),
    ("🌐", |l| l.strings().parts.ctl_language),
    ("🔍", |l| l.strings().parts.ctl_find),
    ("🔎", |l| l.strings().parts.ctl_lookup),
    ("🖼", |l| l.strings().parts.ctl_images),
    ("🔒", |l| l.strings().parts.ctl_locked),
    ("📝", |l| l.strings().parts.ctl_notes),
    ("💬", |l| l.strings().parts.ctl_comment),
    ("📌", |l| l.strings().parts.ctl_pinned),
    ("🔥", |l| l.strings().parts.ctl_pace),
    ("🗑", |l| l.strings().parts.ctl_delete),
    ("🍡", |l| l.strings().parts.ctl_notices),
    ("🌙", |l| l.strings().parts.ctl_rest),
    ("💤", |l| l.strings().parts.ctl_sleep),
    ("☕", |l| l.strings().parts.ctl_awake),
    ("✨", |l| l.strings().parts.ctl_effects),
    ("✦", |l| l.strings().parts.ctl_new),
    ("✅", |l| l.strings().parts.ctl_done),
    ("❌", |l| l.strings().parts.ctl_blocked),
    ("⚠", |l| l.strings().parts.ctl_warning),
    ("✔", |l| l.strings().parts.ctl_done),
    ("✓", |l| l.strings().parts.ctl_selected),
    ("✕", |l| l.strings().parts.ctl_close),
    ("×", |l| l.strings().parts.ctl_close),
    ("⋯", |l| l.strings().parts.ctl_menu),
    ("…", |l| l.strings().parts.ctl_more),
    ("⌁", |l| l.strings().parts.ctl_launch),
    ("⎘", |l| l.strings().parts.ctl_copy),
    ("◳", |l| l.strings().parts.ctl_open),
    ("◧", |l| l.strings().parts.ctl_split_right),
    ("⬓", |l| l.strings().parts.ctl_split_down),
    ("◨", |l| l.strings().parts.ctl_split_right),
    ("◩", |l| l.strings().parts.ctl_split_down),
    ("＋", |l| l.strings().parts.ctl_add),
    ("+", |l| l.strings().parts.ctl_add),
    ("−", |l| l.strings().parts.ctl_remove),
    ("▾", |l| l.strings().parts.ctl_hide),
    ("▸", |l| l.strings().parts.ctl_show),
    ("▿", |l| l.strings().parts.ctl_hide),
    ("▹", |l| l.strings().parts.ctl_show),
    ("▼", |l| l.strings().parts.ctl_next),
    ("▲", |l| l.strings().parts.ctl_previous),
    ("◀", |l| l.strings().parts.ctl_hide),
    ("▶", |l| l.strings().parts.ctl_show),
    ("☐", |l| l.strings().parts.ctl_maximize),
    ("▭", |l| l.strings().parts.ctl_size),
    ("▣", |l| l.strings().parts.ctl_window),
    ("◉", |l| l.strings().parts.ctl_on),
    ("●", |l| l.strings().parts.ctl_on),
    ("○", |l| l.strings().parts.ctl_off),
    ("◯", |l| l.strings().parts.ctl_off),
    ("◆", |l| l.strings().parts.ctl_active),
    ("◇", |l| l.strings().parts.ctl_available),
    ("⤓", |l| l.strings().parts.ctl_adopt),
    ("⟲", |l| l.strings().parts.ctl_reset),
    ("↺", |l| l.strings().parts.ctl_reset),
    ("↪", |l| l.strings().parts.ctl_send),
    ("↗", |l| l.strings().parts.ctl_open),
    ("⇲", |l| l.strings().parts.ctl_beside),
    ("→", |l| l.strings().parts.ctl_right),
    ("↓", |l| l.strings().parts.ctl_down),
    ("←", |l| l.strings().parts.ctl_left),
    ("↑", |l| l.strings().parts.ctl_up),
    ("★", |l| l.strings().parts.ctl_score),
    ("Σ", |l| l.strings().parts.ctl_total),
    ("≈", |l| l.strings().parts.ctl_about),
    ("☰", |l| l.strings().parts.ctl_menu),
    ("\u{fe0f}", |_| ""),
    ("\u{fe0e}", |_| ""),
];

#[cfg(test)]
mod tests {
    use super::*;
    fn label(source: &str) -> String {
        label_in(source, crate::lang::Lang::En)
    }

    #[test]
    fn every_icon_uses_the_readers_language_and_authored_words_survive() {
        for l in crate::lang::Lang::ALL {
            assert_eq!(label_in("+", l), l.strings().parts.ctl_add);
            assert_eq!(label_in("🎨", l), l.strings().parts.ctl_look);
            assert_eq!(label_in("⚓", l), l.strings().parts.ctl_anchor);
            assert_eq!(label_in("↪ send to agent", l), "send to agent");
            assert_eq!(label_in("🌐 über", l), "über");
            assert_eq!(label_in("⏎", l), "Enter");
            for (icon, caption) in ICONS {
                assert_eq!(label_in(icon, l), caption(l));
            }
        }
    }

    #[test]
    fn actions_are_words_and_prefixes_do_not_repeat_the_label() {
        assert_eq!(label("🎨"), "look");
        assert_eq!(label("🎨  Appearance"), "Appearance");
        assert_eq!(label("↪ send to agent"), "send to agent");
        assert_eq!(label("×"), "close");
        assert_eq!(label(""), "");
    }

    #[test]
    fn localized_words_and_unknown_readings_survive() {
        assert_eq!(label("🎨 修复权限"), "修复权限");
        assert_eq!(label("🌐 über"), "über");
        assert_eq!(label("—"), "—");
        assert_eq!(label("0%"), "0%");
        assert_eq!(label("unavailable"), "unavailable");
    }
}
