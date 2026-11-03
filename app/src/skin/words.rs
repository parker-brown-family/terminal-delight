//! Word labels for native controls. This is deliberately not a Unicode filter:
//! terminal output, names, filenames and authored content never pass through it.

/// Icons embedded beside an existing word label add no information. An icon
/// alone needs its action spelled out instead.
pub(super) fn label(source: &str) -> String {
    let source = source.trim();
    let source = source.strip_prefix("</>").unwrap_or(source).trim();
    if let Some((_, word)) = ICONS.iter().find(|(icon, _)| *icon == source) {
        return (*word).to_string();
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

const ICONS: &[(&str, &str)] = &[
    ("📜", "artifacts"),
    ("⚓", "anchor"),
    ("↻", "refresh"),
    ("‖", "paused"),
    ("⏸", "paused"),
    ("☾", "rest"),
    ("⬇", "download"),
    ("⏎", "enter"),
    ("↵", "enter"),
    ("🎨", "look"),
    ("👓", "focus"),
    ("👤", "turns"),
    ("🤖", "agents"),
    ("\u{f06a9}", "agents"),
    ("\u{f04b2}", "asleep"),
    ("\u{f0594}", "rest"),
    ("🪦", "recover"),
    ("🔔", "notices"),
    ("🐞", "report"),
    ("🐛", "report"),
    ("🧩", "plugins"),
    ("📦", "artifacts"),
    ("🗂", "artifacts"),
    ("▤", "artifacts"),
    ("📊", "usage"),
    ("⛭", "gauges"),
    ("⚙", "settings"),
    ("🖥", "display"),
    ("☀", "light"),
    ("🌐", "language"),
    ("🔍", "find"),
    ("🔎", "lookup"),
    ("🖼", "images"),
    ("🔒", "locked"),
    ("📝", "notes"),
    ("💬", "comment"),
    ("📌", "pinned"),
    ("🔥", "pace"),
    ("🗑", "delete"),
    ("🍡", "notices"),
    ("🌙", "rest"),
    ("💤", "sleep"),
    ("☕", "awake"),
    ("✨", "effects"),
    ("✦", "new"),
    ("✅", "done"),
    ("❌", "blocked"),
    ("⚠", "warning"),
    ("✔", "done"),
    ("✓", "selected"),
    ("✕", "close"),
    ("×", "close"),
    ("⋯", "menu"),
    ("…", "more"),
    ("⌁", "launch"),
    ("⎘", "copy"),
    ("◳", "open"),
    ("◧", "split right"),
    ("⬓", "split down"),
    ("◨", "split right"),
    ("◩", "split down"),
    ("＋", "add"),
    ("+", "add"),
    ("−", "remove"),
    ("▾", "hide"),
    ("▸", "show"),
    ("▿", "hide"),
    ("▹", "show"),
    ("▼", "next"),
    ("▲", "previous"),
    ("◀", "hide"),
    ("▶", "show"),
    ("☐", "maximize"),
    ("▭", "size"),
    ("▣", "window"),
    ("◉", "on"),
    ("●", "on"),
    ("○", "off"),
    ("◯", "off"),
    ("◆", "active"),
    ("◇", "available"),
    ("⤓", "adopt"),
    ("⟲", "reset"),
    ("↺", "reset"),
    ("↪", "send"),
    ("↗", "open"),
    ("⇲", "beside"),
    ("→", "right"),
    ("↓", "down"),
    ("←", "left"),
    ("↑", "up"),
    ("★", "score"),
    ("Σ", "total"),
    ("≈", "about"),
    ("☰", "menu"),
    ("\u{fe0f}", ""),
    ("\u{fe0e}", ""),
];

#[cfg(test)]
mod tests {
    use super::*;
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
