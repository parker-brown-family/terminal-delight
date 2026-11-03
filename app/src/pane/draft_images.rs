//! Which file each `[Image #n]` in an agent's prompt came from, so the ctrl+c
//! that clears the prompt can copy the screenshots back as well as the words.
//!
//! A screenshot reaches an agent as a path: TD saves the clipboard image as
//! `pastes/pasted-<n>.<ext>` and pastes that path, and the agent turns the
//! path into a tag. Both agents do it in the prompt, before anything is sent —
//! captured off the real programs on 2026-11-02: Claude Code draws
//! `[Image #1]` plain, Codex `ESC[38;5;6m[Image #1]`. The agent owns the
//! number and nothing outside it can read the agent's own list. TD does not
//! need to: it wrote the file and typed the path, so it only has to watch
//! which tag appears next.
//!
//! The pairing is conservative, because a wrong file is worse than none. A
//! tag is paired only with a path TD pasted, in the order they were pasted,
//! and only if it appears within [`WAIT`] of that paste. A tag TD did not
//! cause stays a tag. Parker, 2026-11-02: *"we just need to make sure the
//! CORRECT files and CORRECT replacement of the clipboard input happens"*.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::ops::Range;
use std::time::{Duration, Instant};

/// How long a pasted path waits for its tag. An agent draws the tag the
/// moment the paste arrives; a paste whose tag has not appeared by then was
/// not taken as an image, and pairing it later could pair it with the wrong
/// tag.
pub(super) const WAIT: Duration = Duration::from_secs(10);

#[derive(Debug, Default)]
pub(super) struct DraftImages {
    /// Tag number to the file it was made from.
    known: HashMap<u32, String>,
    /// Paths pasted whose tag has not appeared yet, oldest first.
    pending: VecDeque<(String, Instant)>,
    /// The tags already on screen when the oldest pending path was pasted,
    /// and every tag seen since: a tag outside this set is new.
    seen: BTreeSet<u32>,
    /// A watcher is already polling the screen for this pane.
    pub(super) watching: bool,
}

impl DraftImages {
    /// TD is about to paste `path` into a prompt that shows `tags_now`.
    pub(super) fn pasted(&mut self, path: String, tags_now: BTreeSet<u32>, now: Instant) {
        if self.pending.is_empty() {
            self.seen = tags_now;
        }
        self.pending.push_back((path, now));
    }

    /// The prompt now shows `tags_now`. Each tag that has appeared since is
    /// paired with the oldest pasted path still waiting — in number order,
    /// which is the order both agents hand numbers out in. More new tags than
    /// paths waiting means something TD did not do put a picture there, and
    /// which tag is whose can no longer be told: none of them is paired, every
    /// file those numbers used to mean is forgotten, and the waiting paths are
    /// dropped rather than left to pair with a later stranger. Returns whether
    /// a path is still waiting.
    pub(super) fn observe(&mut self, tags_now: &BTreeSet<u32>, now: Instant) -> bool {
        self.pending
            .retain(|(_, at)| now.saturating_duration_since(*at) < WAIT);
        let new: Vec<u32> = tags_now.difference(&self.seen).copied().collect();
        if new.len() > self.pending.len() {
            for tag in &new {
                self.known.remove(tag);
            }
            self.pending.clear();
        } else {
            for tag in new {
                if let Some((path, _)) = self.pending.pop_front() {
                    self.known.insert(tag, path);
                }
            }
        }
        // Grown, never shrunk, while watching: a frame read halfway through
        // the agent redrawing its prompt can miss a tag for one look, and
        // that tag coming back is not a new one.
        self.seen.extend(tags_now.iter().copied());
        !self.pending.is_empty()
    }

    /// `draft` with every tag TD can account for replaced by its file's path,
    /// written as a paste of that path would be. Any other tag is left as it
    /// stands.
    pub(super) fn restore(&self, draft: &str) -> String {
        let mut out = String::with_capacity(draft.len());
        let mut from = 0;
        for (span, tag) in image_tags(draft) {
            out.push_str(&draft[from..span.start]);
            match self.known.get(&tag) {
                Some(path) => out.push_str(&crate::workbench::paths_as_words(&[path.into()])),
                None => out.push_str(&draft[span.clone()]),
            }
            from = span.end;
        }
        out.push_str(&draft[from..]);
        out
    }

    /// The prompt has gone — sent or cleared — and its tags with it.
    pub(super) fn clear(&mut self) {
        self.known.clear();
        self.pending.clear();
        self.seen.clear();
    }
}

/// A paste TD can vouch for: exactly one file, of a kind both agents turn
/// into a tag. More than one path in one paste is pasted but not tracked —
/// how an agent numbers several pictures arriving at once was never
/// measured, and a guessed pairing is the one way to copy the wrong file.
/// The clipboard route always pastes one file.
pub(super) fn trackable(paths: &[std::path::PathBuf]) -> bool {
    let [path] = paths else {
        return false;
    };
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp"
        )
    })
}

/// Every `[Image #n]` in `text`, with its byte span. The space may be any
/// run of whitespace: an agent's own wrap can fall between the two words.
pub(super) fn image_tags(text: &str) -> Vec<(Range<usize>, u32)> {
    const OPEN: &str = "[Image";
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = text[from..].find(OPEN) {
        let start = from + at;
        let mut i = start + OPEN.len();
        from = i;
        let rest = &text[i..];
        let gap = rest.len() - rest.trim_start().len();
        if gap == 0 || !rest[gap..].starts_with('#') {
            continue;
        }
        i += gap + 1;
        let digits = text[i..].chars().take_while(char::is_ascii_digit).count();
        let Ok(tag) = text[i..i + digits].parse::<u32>() else {
            continue;
        };
        i += digits;
        if !text[i..].starts_with(']') {
            continue;
        }
        i += 1;
        out.push((start..i, tag));
        from = i;
    }
    out
}

/// Just the numbers, for [`DraftImages::observe`].
pub(super) fn tag_set(text: &str) -> BTreeSet<u32> {
    image_tags(text).into_iter().map(|(_, n)| n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(ns: &[u32]) -> BTreeSet<u32> {
        ns.iter().copied().collect()
    }

    #[test]
    fn a_tag_is_found_however_the_agent_spaced_or_wrapped_it() {
        let found: Vec<u32> = image_tags("look [Image #1] and [Image  #12]\n[Image\n#3] x")
            .into_iter()
            .map(|(_, n)| n)
            .collect();
        assert_eq!(found, vec![1, 12, 3]);
        assert!(image_tags("[Image#1] [Image #] [Image #2 [Images #4]").is_empty());
    }

    /// The tag that appears after a paste is that paste's file, and Ctrl+C
    /// copies the file's path where the tag was — words kept around it.
    #[test]
    fn the_tag_that_appears_after_a_paste_copies_back_as_its_file() {
        let t0 = Instant::now();
        let mut d = DraftImages::default();
        d.pasted("/p/pasted-1.png".into(), tags(&[]), t0);
        assert!(
            d.observe(&tags(&[]), t0),
            "still waiting before the agent draws it"
        );
        assert!(!d.observe(&tags(&[1]), t0), "paired once the tag is drawn");
        assert_eq!(
            d.restore("look at [Image #1] please"),
            "look at /p/pasted-1.png please"
        );
        assert_eq!(
            d.restore("[Image #1]"),
            "/p/pasted-1.png",
            "a lone screenshot is just its path"
        );
    }

    /// Two pastes before the agent draws either: numbers in order, files in
    /// order. And a path with a space stays one path.
    #[test]
    fn two_quick_pastes_pair_in_order() {
        let t0 = Instant::now();
        let mut d = DraftImages::default();
        d.pasted("/p/pasted-1.png".into(), tags(&[]), t0);
        d.pasted("/p/my shot.png".into(), tags(&[]), t0);
        assert!(!d.observe(&tags(&[3, 4]), t0));
        assert_eq!(
            d.restore("[Image #3] vs [Image #4]"),
            "/p/pasted-1.png vs '/p/my shot.png'"
        );
    }

    /// The failure that would put the WRONG file on the clipboard: a tag TD
    /// did not cause. It stays a tag, and if its number used to mean a file,
    /// that is forgotten rather than reused.
    #[test]
    fn a_tag_td_did_not_cause_is_never_given_a_file() {
        let t0 = Instant::now();
        let mut d = DraftImages::default();
        d.pasted("/p/pasted-1.png".into(), tags(&[]), t0);
        d.observe(&tags(&[1]), t0);
        assert!(!d.observe(&tags(&[1, 2]), t0), "nothing was waiting for #2");
        assert_eq!(
            d.restore("[Image #1] [Image #2]"),
            "/p/pasted-1.png [Image #2]"
        );

        // Number reused for something TD did not paste: the old file is
        // forgotten, not copied.
        let mut d = DraftImages::default();
        d.pasted("/p/pasted-1.png".into(), tags(&[]), t0);
        d.observe(&tags(&[1]), t0);
        d.pasted("/p/pasted-2.png".into(), tags(&[]), t0);
        d.observe(&tags(&[2]), t0);
        d.observe(&tags(&[]), t0);
        assert_eq!(d.restore("[Image #2]"), "/p/pasted-2.png");
    }

    /// One paste, two new tags: which one is the paste cannot be told, so
    /// neither gets the file, and the paste pairs with nothing later.
    #[test]
    fn more_new_tags_than_pastes_pairs_none() {
        let t0 = Instant::now();
        let mut d = DraftImages::default();
        d.pasted("/p/pasted-1.png".into(), tags(&[]), t0);
        assert!(!d.observe(&tags(&[1, 2]), t0), "nothing left waiting");
        assert_eq!(d.restore("[Image #1] [Image #2]"), "[Image #1] [Image #2]");
        d.observe(&tags(&[1, 2, 3]), t0);
        assert_eq!(d.restore("[Image #3]"), "[Image #3]");
    }

    /// A paste the agent never turned into a tag stops waiting, so a tag that
    /// turns up much later is not given its file.
    #[test]
    fn a_paste_that_never_became_a_tag_stops_waiting() {
        let t0 = Instant::now();
        let mut d = DraftImages::default();
        d.pasted("/p/notes.txt".into(), tags(&[]), t0);
        assert!(!d.observe(&tags(&[]), t0 + WAIT), "expired");
        d.observe(&tags(&[1]), t0 + WAIT);
        assert_eq!(d.restore("[Image #1]"), "[Image #1]");
    }

    #[test]
    fn only_one_picture_at_a_time_is_tracked() {
        let one = |p: &str| vec![std::path::PathBuf::from(p)];
        assert!(trackable(&one("/p/pasted-1.png")));
        assert!(trackable(&one("/p/Shot.JPEG")));
        assert!(!trackable(&one("/p/notes.txt")));
        assert!(
            !trackable(&one("/p/diagram.svg")),
            "not a kind both agents attach"
        );
        assert!(
            !trackable(&[one("/p/a.png"), one("/p/b.png")].concat()),
            "two at once"
        );
        assert!(!trackable(&[]));
    }

    /// A tag already on screen when the paste happened is not the paste's.
    #[test]
    fn a_tag_already_there_is_not_the_new_paste() {
        let t0 = Instant::now();
        let mut d = DraftImages::default();
        d.pasted("/p/pasted-2.png".into(), tags(&[1]), t0);
        assert!(d.observe(&tags(&[1]), t0), "#1 was there before");
        d.observe(&tags(&[1, 2]), t0);
        assert_eq!(
            d.restore("[Image #1] [Image #2]"),
            "[Image #1] /p/pasted-2.png"
        );
    }

    /// Sent or cleared, the prompt's files are forgotten: a tag in the next
    /// prompt is never the last prompt's file.
    #[test]
    fn a_cleared_prompt_forgets_its_files() {
        let t0 = Instant::now();
        let mut d = DraftImages::default();
        d.pasted("/p/pasted-1.png".into(), tags(&[]), t0);
        d.observe(&tags(&[1]), t0);
        d.clear();
        assert_eq!(d.restore("[Image #1]"), "[Image #1]");
    }
}
