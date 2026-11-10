//! A small multi-line text editor's model: the lines, a caret, typing, the
//! motions, and undo. No window, no font: the side panel draws it
//! (`flyout_view.rs`) and hands it keys.
//!
//! Parker, 2026-11-10, of a Markdown file open in the right-bar side panel:
//! *"would be nice to be able to edit from the main OR source by just clicking
//! around and editing like a regular markedown editor! Let's get that in!"*
//!
//! The caret is a line and a column counted in characters, never bytes, so a
//! line with an accent or an emoji moves one character per press. Every edit
//! that changes the text is one undo step, except that typing a run of
//! letters is one step, as editors do: an undo takes back a word, not a key.

#[derive(Clone, Debug, PartialEq)]
pub struct TextEdit {
    lines: Vec<String>,
    /// The caret: a line, and a column in characters within it.
    pub line: usize,
    pub col: usize,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// The last edit was typing a letter, so the next letter joins its step.
    typing: bool,
    /// Whether the text differs from what it was made from.
    changed: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct Snapshot {
    lines: Vec<String>,
    line: usize,
    col: usize,
}

/// The most undo steps kept; older ones are let go.
const UNDO_KEPT: usize = 400;

fn chars(s: &str) -> usize {
    s.chars().count()
}

/// The byte offset of character `col` in `s`, or its end.
fn byte_at(s: &str, col: usize) -> usize {
    s.char_indices().nth(col).map(|(b, _)| b).unwrap_or(s.len())
}

impl TextEdit {
    /// The text split into lines. A trailing newline is kept as an empty last
    /// line, so writing it back gives the same bytes.
    pub fn new(text: &str) -> TextEdit {
        TextEdit {
            lines: text.split('\n').map(str::to_string).collect(),
            line: 0,
            col: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            typing: false,
            changed: false,
        }
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn changed(&self) -> bool {
        self.changed
    }

    /// The text has been written where it came from: it is the new start.
    pub fn saved(&mut self) {
        self.changed = false;
    }

    /// Put the caret at a place, clamped to the text.
    pub fn place(&mut self, line: usize, col: usize) {
        self.line = line.min(self.lines.len().saturating_sub(1));
        self.col = col.min(chars(&self.lines[self.line]));
        self.typing = false;
    }

    fn remember(&mut self, typing: bool) {
        if !(typing && self.typing) {
            self.undo.push(Snapshot {
                lines: self.lines.clone(),
                line: self.line,
                col: self.col,
            });
            if self.undo.len() > UNDO_KEPT {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.typing = typing;
        self.changed = true;
    }

    /// Type text at the caret. A newline in it splits the line.
    pub fn insert(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let word = !text.contains('\n') && !text.chars().any(char::is_whitespace);
        self.remember(word && chars(text) == 1);
        let cur = &self.lines[self.line];
        let at = byte_at(cur, self.col);
        let (head, tail) = (cur[..at].to_string(), cur[at..].to_string());
        let mut parts: Vec<&str> = text.split('\n').collect();
        let last = parts.pop().unwrap_or("");
        if parts.is_empty() {
            self.lines[self.line] = format!("{head}{last}{tail}");
            self.col += chars(last);
            return;
        }
        self.lines[self.line] = format!("{head}{}", parts[0]);
        let mut at_line = self.line;
        for p in &parts[1..] {
            at_line += 1;
            self.lines.insert(at_line, (*p).to_string());
        }
        at_line += 1;
        self.lines.insert(at_line, format!("{last}{tail}"));
        self.line = at_line;
        self.col = chars(last);
    }

    /// Enter: split the line, keeping the indent and a list marker, as a
    /// Markdown editor does. An empty list item ends the list instead.
    pub fn newline(&mut self) {
        let cur = self.lines[self.line].clone();
        let indent: String = cur
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        let rest = &cur[indent.len()..];
        let marker = ["- [ ] ", "- [x] ", "- ", "* ", "+ ", "> "]
            .into_iter()
            .find(|m| rest.starts_with(m))
            .map(|m| if m.starts_with("- [") { "- [ ] " } else { m })
            .map(str::to_string)
            .or_else(|| {
                let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
                (!digits.is_empty() && rest[digits.len()..].starts_with(". "))
                    .then(|| format!("{}. ", digits.parse::<u64>().unwrap_or(0) + 1))
            });
        if let Some(m) = &marker {
            let body = rest.trim_start_matches(|c: char| c.is_ascii_digit());
            let item_empty = rest.trim_end() == m.trim_end()
                || (body.starts_with(". ") && body[2..].trim().is_empty());
            if item_empty && self.col >= chars(&cur) {
                self.remember(false);
                self.lines[self.line] = indent;
                self.col = chars(&self.lines[self.line]);
                return;
            }
        }
        let carry = format!("{indent}{}", marker.unwrap_or_default());
        self.remember(false);
        self.typing = false;
        let at = byte_at(&cur, self.col);
        self.lines[self.line] = cur[..at].to_string();
        self.lines
            .insert(self.line + 1, format!("{carry}{}", &cur[at..]));
        self.line += 1;
        self.col = chars(&carry);
    }

    /// Backspace: the character before the caret, or the line break.
    pub fn backspace(&mut self) {
        if self.col > 0 {
            self.remember(false);
            let cur = &mut self.lines[self.line];
            let (a, b) = (byte_at(cur, self.col - 1), byte_at(cur, self.col));
            cur.replace_range(a..b, "");
            self.col -= 1;
        } else if self.line > 0 {
            self.remember(false);
            let cur = self.lines.remove(self.line);
            self.line -= 1;
            self.col = chars(&self.lines[self.line]);
            self.lines[self.line].push_str(&cur);
        }
    }

    /// Delete: the character after the caret, or the line break.
    pub fn delete(&mut self) {
        let len = chars(&self.lines[self.line]);
        if self.col < len {
            self.remember(false);
            let cur = &mut self.lines[self.line];
            let (a, b) = (byte_at(cur, self.col), byte_at(cur, self.col + 1));
            cur.replace_range(a..b, "");
        } else if self.line + 1 < self.lines.len() {
            self.remember(false);
            let next = self.lines.remove(self.line + 1);
            self.lines[self.line].push_str(&next);
        }
    }

    pub fn left(&mut self) {
        self.typing = false;
        if self.col > 0 {
            self.col -= 1;
        } else if self.line > 0 {
            self.line -= 1;
            self.col = chars(&self.lines[self.line]);
        }
    }

    pub fn right(&mut self) {
        self.typing = false;
        if self.col < chars(&self.lines[self.line]) {
            self.col += 1;
        } else if self.line + 1 < self.lines.len() {
            self.line += 1;
            self.col = 0;
        }
    }

    pub fn up(&mut self) {
        self.typing = false;
        if self.line > 0 {
            self.line -= 1;
            self.col = self.col.min(chars(&self.lines[self.line]));
        }
    }

    pub fn down(&mut self) {
        self.typing = false;
        if self.line + 1 < self.lines.len() {
            self.line += 1;
            self.col = self.col.min(chars(&self.lines[self.line]));
        }
    }

    pub fn home(&mut self) {
        self.typing = false;
        self.col = 0;
    }

    pub fn end(&mut self) {
        self.typing = false;
        self.col = chars(&self.lines[self.line]);
    }

    pub fn undo(&mut self) -> bool {
        let Some(s) = self.undo.pop() else {
            return false;
        };
        self.redo.push(Snapshot {
            lines: std::mem::replace(&mut self.lines, s.lines),
            line: self.line,
            col: self.col,
        });
        self.line = s.line;
        self.col = s.col;
        self.typing = false;
        self.changed = true;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(s) = self.redo.pop() else {
            return false;
        };
        self.undo.push(Snapshot {
            lines: std::mem::replace(&mut self.lines, s.lines),
            line: self.line,
            col: self.col,
        });
        self.line = s.line;
        self.col = s.col;
        self.typing = false;
        self.changed = true;
        true
    }
}

/// How many columns the first `col` characters of `line` take when a tab is
/// drawn as four, as the side panel draws it.
pub fn shown_col(line: &str, col: usize) -> usize {
    line.chars()
        .take(col)
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
}

/// The caret column for a click `shown` columns into `line`: the nearest
/// gap between characters, a tab counting four.
pub fn col_at_shown(line: &str, shown: f32) -> usize {
    let mut x = 0.0;
    for (i, c) in line.chars().enumerate() {
        let w = if c == '\t' { 4.0 } else { 1.0 };
        if shown < x + w / 2.0 {
            return i;
        }
        x += w;
    }
    line.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_moves_the_caret_by_characters_and_round_trips_the_bytes() {
        let mut e = TextEdit::new("héllo\nwörld\n");
        assert_eq!(
            e.lines().len(),
            3,
            "a trailing newline is an empty last line"
        );
        e.place(0, 2);
        e.insert("X");
        assert_eq!(e.lines()[0], "héXllo", "inserted after the accented letter");
        e.place(1, 99);
        e.insert("!");
        assert_eq!(
            e.text(),
            "héXllo\nwörld!\n",
            "the trailing newline survives"
        );
        assert!(e.changed());
        e.saved();
        assert!(!e.changed());
    }

    #[test]
    fn enter_backspace_and_delete_join_and_split_lines() {
        let mut e = TextEdit::new("abcd");
        e.place(0, 2);
        e.newline();
        assert_eq!(e.lines(), ["ab", "cd"]);
        assert_eq!((e.line, e.col), (1, 0));
        e.backspace();
        assert_eq!(e.lines(), ["abcd"]);
        assert_eq!((e.line, e.col), (0, 2));
        e.end();
        e.insert("\nef");
        e.place(0, 4);
        e.delete();
        assert_eq!(e.lines(), ["abcdef"]);
    }

    /// Enter carries a list marker and the indent, numbers the next item,
    /// and on an empty item ends the list, as a Markdown editor does.
    #[test]
    fn enter_continues_a_list_and_an_empty_item_ends_it() {
        let mut e = TextEdit::new("  - first");
        e.end();
        e.newline();
        assert_eq!(e.lines()[1], "  - ");
        e.newline();
        assert_eq!(e.lines()[1], "  ", "an empty item ends the list");
        let mut n = TextEdit::new("3. three");
        n.end();
        n.newline();
        assert_eq!(n.lines()[1], "4. ");
        let mut t = TextEdit::new("- [x] done");
        t.end();
        t.newline();
        assert_eq!(t.lines()[1], "- [ ] ", "a new task starts unticked");
    }

    /// A run of letters is one undo step; a space or Enter starts another.
    ///
    /// Mutation-tested: making every key its own step fails the first undo.
    #[test]
    fn undo_takes_back_a_word_and_redo_brings_it_again() {
        let mut e = TextEdit::new("");
        for c in "hello".chars() {
            e.insert(&c.to_string());
        }
        e.insert(" ");
        for c in "you".chars() {
            e.insert(&c.to_string());
        }
        assert_eq!(e.text(), "hello you");
        assert!(e.undo());
        assert_eq!(e.text(), "hello ");
        assert!(e.undo());
        assert_eq!(e.text(), "hello");
        assert!(e.redo());
        assert_eq!(e.text(), "hello ");
    }

    /// A click lands in the nearer gap, and a tab is four columns wide, as
    /// drawn. Mutation-tested: counting a tab as one puts the caret a
    /// character late after a tab.
    #[test]
    fn a_click_lands_in_the_nearer_gap_with_a_tab_four_wide() {
        assert_eq!(col_at_shown("abc", 0.4), 0);
        assert_eq!(col_at_shown("abc", 0.6), 1);
        assert_eq!(col_at_shown("abc", 9.0), 3);
        assert_eq!(col_at_shown("\tb", 3.0), 1);
        assert_eq!(col_at_shown("\tb", 4.4), 1);
        assert_eq!(col_at_shown("\tb", 4.6), 2);
        assert_eq!(shown_col("\tb", 2), 5);
        assert_eq!(shown_col("é\t", 1), 1);
    }
}
