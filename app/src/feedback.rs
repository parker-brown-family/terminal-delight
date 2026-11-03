//! Whether a person has left feedback on an artifact: what the Threshing Floor's
//! cards say beside each agent's newest artifact.
//!
//! Parker, 2026-10-27: *"Needs a save in file --- no feedback -> human annotaed
//! means that a READ or OPEN of the file is not enough to trigger the human
//! provenance because there was no INTERACTION."* So opening an artifact is not
//! feedback, however it was opened; a note or a concur saved for it is. A brief
//! keeps those in its own file, in the islands [`crate::docview::notes`] reads.
//! A Markdown file's notes are kept in TD's own store
//! ([`crate::docview::md_notes`]). Nothing else an artifact can be — a picture,
//! a PDF, a web page — takes notes here, and that is not the same as having
//! none, so it answers [`Feedback::Unknown`].
//!
//! Read at most once per revision: each answer is kept with the size and
//! modification time of the file it was read from, and a card re-reads only
//! when those change.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::docview::{md_notes, notes};

/// The largest brief read for its notes; anything bigger answers unknown.
const MAX_BRIEF: u64 = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feedback {
    /// A person has saved a note or a concur on it.
    Annotated,
    /// It can take notes, and none has been saved.
    None,
    /// Nothing is claimed: this kind of artifact takes no notes here, or its
    /// notes could not be read.
    Unknown,
}

/// A brief's bytes. A note or a concur in its islands is feedback. A file with
/// no notes island cannot take notes, which is not the same as having none.
pub fn of_brief(bytes: &[u8]) -> Feedback {
    let read = notes::read(bytes);
    let notes = match &read.notes {
        Some(Ok(map)) => Some(map.count()),
        Some(Err(_)) | None => None,
    };
    let concurs = read.concurs.as_ref().ok().map(|c| c.count());
    if notes.unwrap_or(0) > 0 || concurs.unwrap_or(0) > 0 {
        return Feedback::Annotated;
    }
    match (read.notes.is_some(), notes, concurs) {
        (true, Some(0), Some(0)) => Feedback::None,
        _ => Feedback::Unknown,
    }
}

/// The notes TD keeps for a Markdown file, as [`md_notes::read`] answers.
pub fn of_markdown(kept: &Result<notes::NoteMap, String>) -> Feedback {
    match kept {
        Ok(map) if map.count() > 0 => Feedback::Annotated,
        Ok(_) => Feedback::None,
        Err(_) => Feedback::Unknown,
    }
}

/// The size and modification time of `path`, or `None` when it is not there.
fn stamp(path: &Path) -> Option<(u64, u128)> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some((meta.len(), mtime))
}

type Stamp = Option<(u64, u128)>;

fn cache() -> &'static Mutex<HashMap<PathBuf, (Stamp, Feedback)>> {
    static CACHE: std::sync::OnceLock<Mutex<HashMap<PathBuf, (Stamp, Feedback)>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// `read` the answer from `source` unless the cached one was read from the
/// same revision of it.
fn cached(source: &Path, read: impl FnOnce(Stamp) -> Feedback) -> Feedback {
    let now = stamp(source);
    if let Some((was, answer)) = cache().lock().ok().and_then(|c| c.get(source).copied()) {
        if was == now {
            return answer;
        }
    }
    let answer = read(now);
    if let Ok(mut c) = cache().lock() {
        c.insert(source.to_path_buf(), (now, answer));
    }
    answer
}

/// The feedback on the artifact at `href`, as a card shows it.
pub fn of(href: &str) -> Feedback {
    of_in(href, &md_notes::store_dir())
}

fn of_in(href: &str, md_root: &Path) -> Feedback {
    let Some(path) = crate::handover::file_path(href) else {
        return Feedback::Unknown;
    };
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("html" | "htm") => cached(&path, |now| match now {
            Some((len, _)) if len <= MAX_BRIEF => std::fs::read(&path)
                .map(|bytes| of_brief(&bytes))
                .unwrap_or(Feedback::Unknown),
            _ => Feedback::Unknown,
        }),
        Some("md" | "markdown") => {
            let store = md_notes::store_in(md_root, &path);
            cached(&store, |_| of_markdown(&md_notes::read(&store)))
        }
        _ => Feedback::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brief(notes: &str, concurs: &str) -> Vec<u8> {
        format!(
            "<html><body><p id=\"x\">x</p>\
             <script type=\"application/json\" id=\"report-notes\" data-format=\"1\">{notes}</script>\
             <script type=\"application/json\" id=\"report-concurs\" data-format=\"1\">{concurs}</script>\
             <script>/* notes.js */</script></body></html>"
        )
        .into_bytes()
    }

    #[test]
    fn a_brief_with_a_saved_note_or_concur_is_annotated() {
        assert_eq!(
            of_brief(&brief(
                r#"{"fig-01":[{"text":"keep this","ts":"2026-10-27 12:00"}]}"#,
                "{}"
            )),
            Feedback::Annotated
        );
        assert_eq!(
            of_brief(&brief("{}", r#"{"ask-1":"2026-10-27 12:00"}"#)),
            Feedback::Annotated,
            "a concur is feedback too"
        );
    }

    #[test]
    fn a_brief_with_empty_islands_has_no_feedback() {
        assert_eq!(of_brief(&brief("{}", "{}")), Feedback::None);
    }

    #[test]
    fn a_page_that_cannot_take_notes_claims_nothing() {
        assert_eq!(
            of_brief(b"<html><body><p>a plain report</p></body></html>"),
            Feedback::Unknown,
            "no island is read-only, not empty"
        );
        assert_eq!(
            of_brief(&brief("not json", "{}")),
            Feedback::Unknown,
            "an unreadable island claims nothing"
        );
    }

    #[test]
    fn markdown_notes_count_and_a_missing_store_is_no_feedback() {
        let dir = std::env::temp_dir().join(format!("td-feedback-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let doc = dir.join("plan.md");
        std::fs::write(&doc, "# Plan\n\nA paragraph.\n").unwrap();
        let href = doc.display().to_string();
        assert_eq!(of_in(&href, &dir), Feedback::None, "nothing was ever kept");
        let store = md_notes::store_in(&dir, &doc);
        std::fs::write(
            &store,
            r#"{"format":1,"notes":{"p-a-paragraph":[{"text":"yes","ts":"2026-10-27 12:00"}]}}"#,
        )
        .unwrap();
        assert_eq!(
            of_in(&href, &dir),
            Feedback::Annotated,
            "a kept note is feedback"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn opening_a_brief_is_not_feedback_and_a_save_is_read_again() {
        let dir = std::env::temp_dir().join(format!("td-feedback-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let page = dir.join("brief.html");
        std::fs::write(&page, brief("{}", "{}")).unwrap();
        let href = format!("file://{}", page.display());
        assert_eq!(of_in(&href, &dir), Feedback::None);
        let _ = std::fs::read(&page).unwrap();
        assert_eq!(
            of_in(&href, &dir),
            Feedback::None,
            "reading it is not feedback"
        );
        std::fs::write(&page, brief("{}", r#"{"ask-2":"2026-10-27 12:01"}"#)).unwrap();
        assert_eq!(
            of_in(&href, &dir),
            Feedback::Annotated,
            "the save is a new revision"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_picture_or_a_web_page_claims_nothing() {
        assert_eq!(
            of_in("/tmp/render.png", Path::new("/nowhere")),
            Feedback::Unknown
        );
        assert_eq!(
            of_in("https://example.com/report", Path::new("/nowhere")),
            Feedback::Unknown
        );
    }
}
