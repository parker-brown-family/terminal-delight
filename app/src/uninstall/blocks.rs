//! The Hyprland key-sheet block `install-keys-hotkey.sh` writes into
//! `bindings.lua`, removed only when it is exactly the block TD wrote: one
//! opening marker, one closing marker after it, and the seven generated lines
//! between them. A missing marker, a second copy or an edited line leaves the
//! whole file alone, so a damaged block can never become "delete to the end".
//! The same port of `scripts/lib/uninstall.sh` serves people with no checkout.

pub(crate) const OPEN: &str = "-- >>> td-keybindings hotkey >>>";
pub(crate) const SHUT: &str = "-- <<< td-keybindings hotkey <<<";

const BODY: [&str; 6] = [
    "-- SUPER+ALT+T shows every Terminal Delight key in Omarchy's searchable key",
    "-- menu, the way SUPER+K shows Omarchy's own and SUPER+CTRL+K shows Herdr's.",
    "-- The list is the docs site's Keys page, copied beside the script by",
    "-- scripts/install-keys-hotkey.sh in the terminal-delight repo. The seat was",
    "-- td-send's, whose hotkey is retired; T reads for Terminal Delight, and the K",
    "-- family on SUPER, SUPER+ALT and SUPER+CTRL is all Omarchy's already.",
];

#[derive(Debug, PartialEq)]
pub(crate) enum KeyBlock {
    /// No marker at all.
    Absent,
    /// Exactly TD's block; the text without it.
    Owned(String),
    /// Markers present but not exactly TD's block, or the helper is referenced
    /// elsewhere in the file. The reason is for the person.
    Refused(String),
}

/// `dest` is the directory the helper was installed into (`~/.local/bin`).
pub(crate) fn strip_key_block(text: &str, dest: &str) -> KeyBlock {
    let lines: Vec<&str> = text.split('\n').collect();
    let opens: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| **l == OPEN)
        .map(|(i, _)| i)
        .collect();
    let shuts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| **l == SHUT)
        .map(|(i, _)| i)
        .collect();
    if opens.is_empty() && shuts.is_empty() {
        return KeyBlock::Absent;
    }
    if opens.len() != 1 || shuts.len() != 1 || shuts[0] != opens[0] + 8 {
        return KeyBlock::Refused(
            "the td-keybindings markers are missing, repeated or out of order".into(),
        );
    }
    let o = opens[0];
    let bind = format!(
        "o.bind(\"SUPER + ALT + T\", \"Terminal Delight keybindings\", \"{dest}/td-keybindings\")"
    );
    let body_ok = BODY
        .iter()
        .enumerate()
        .all(|(i, want)| lines[o + 1 + i] == *want)
        && lines[o + 7] == bind;
    if !body_ok {
        return KeyBlock::Refused("the td-keybindings block was edited after installation".into());
    }
    let rest: Vec<&str> = lines[..o]
        .iter()
        .chain(lines[o + 9..].iter())
        .copied()
        .collect();
    let out = rest.join("\n");
    if out.contains("td-keybindings") {
        return KeyBlock::Refused("another line in the file still refers to td-keybindings".into());
    }
    KeyBlock::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(dest: &str) -> String {
        let mut s = format!("{OPEN}\n");
        for l in BODY {
            s.push_str(l);
            s.push('\n');
        }
        s.push_str(&format!(
            "o.bind(\"SUPER + ALT + T\", \"Terminal Delight keybindings\", \"{dest}/td-keybindings\")\n{SHUT}\n"
        ));
        s
    }

    #[test]
    fn exact_block_goes_and_the_tail_stays() {
        let text = format!("-- before\n\n{}-- user after", block("/h/.local/bin"));
        assert_eq!(
            strip_key_block(&text, "/h/.local/bin"),
            KeyBlock::Owned("-- before\n\n-- user after".into())
        );
    }

    #[test]
    fn damaged_duplicate_edited_or_foreign_blocks_are_refused() {
        let b = block("/h/.local/bin");
        let cases = [
            b.replace(SHUT, "-- missing"),
            format!("{b}{b}"),
            b.replace("SUPER + ALT + T", "SUPER + ALT + U"),
            format!("{b}o.bind('X', 'custom', 'td-keybindings')\n"),
            format!("{SHUT}\n{}", b.replace(SHUT, "")),
        ];
        for c in cases {
            assert!(
                matches!(strip_key_block(&c, "/h/.local/bin"), KeyBlock::Refused(_)),
                "{c}"
            );
        }
        assert!(matches!(
            strip_key_block(&b, "/elsewhere"),
            KeyBlock::Refused(_)
        ));
    }

    #[test]
    fn no_marker_is_absent() {
        assert_eq!(strip_key_block("-- nothing here\n", "/h"), KeyBlock::Absent);
    }
}
