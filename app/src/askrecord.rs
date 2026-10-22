//! The record of every ask one agent made of another, one JSON line per stage.
//!
//! Append-only, one file per window session under
//! `$XDG_STATE_HOME/terminal-delight/warroom/<session>/asks.jsonl`. It keeps
//! who asked whom, along which kind of line, at which stage, and what reading
//! the answer cost — and never the question's words or the answer, only the
//! question's length and SHA-256, so the record cannot become a second copy of
//! what agents say to each other. The two numbers Gate 1 promised Parker (asks
//! answered with no line, which must be zero, and tokens per answered ask) are
//! read back from it by [`numbers`].

use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::warroom::{AskId, SeatId};

/// One stage of one ask.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: AskId,
    pub at_ms: u64,
    /// The asking seat, when the asker is seated. `None` is an unseated or
    /// unknown asker, never seat zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<SeatId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<SeatId>,
    /// `line`, `bypass`, or `none` when the ask had no line at all.
    pub via: String,
    /// `cheap`, `priced`, `pressed`, `declined`, `copy_started`, `answered`,
    /// `refused` or `failed`.
    pub stage: String,
    /// Why it was refused, by name, on a `refused` stage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    pub question_len: usize,
    pub question_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// What a copy would read, when it was priced; `None` is unknown, not 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reads_tokens: Option<u64>,
    /// What answering actually read; `None` is not measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_read: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

/// The question's fingerprint: what the record keeps instead of its words.
pub fn fingerprint(question: &str) -> String {
    let digest = Sha256::digest(question.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Where one window session's record lives.
pub fn path_for(session: &str) -> PathBuf {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::session::home_dir().join(".local/state"));
    state
        .join("terminal-delight")
        .join("warroom")
        .join(session)
        .join("asks.jsonl")
}

/// Append one stage. A record that cannot be written is an error the caller
/// reports; it never silently drops the stage.
pub fn append(path: &Path, entry: &Entry) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
    line.push('\n');
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    f.write_all(line.as_bytes())
}

/// The two numbers Gate 1 promised, read back from a record.
#[derive(Clone, Debug, PartialEq)]
pub struct Numbers {
    /// Asks answered with no line. Zero by construction; counted so that a
    /// regression shows up as a number rather than as nothing.
    pub unlined_answered: u64,
    /// Asks answered from the cheap reads.
    pub cheap_answers: u64,
    /// Asks a copy answered, and the tokens those copies read (`None` when no
    /// copy reported what it read — unknown, not zero).
    pub copy_answers: u64,
    pub copy_tokens: Option<u64>,
}

/// Read a record back. Lines this build cannot read are skipped and counted
/// out of the numbers rather than guessed into them.
pub fn numbers(path: &Path) -> std::io::Result<Numbers> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let mut n = Numbers {
        unlined_answered: 0,
        cheap_answers: 0,
        copy_answers: 0,
        copy_tokens: None,
    };
    for e in text
        .lines()
        .filter_map(|l| serde_json::from_str::<Entry>(l).ok())
    {
        let answered = matches!(e.stage.as_str(), "cheap" | "answered");
        if answered && e.via == "none" {
            n.unlined_answered += 1;
        }
        match e.stage.as_str() {
            "cheap" => n.cheap_answers += 1,
            "answered" => {
                n.copy_answers += 1;
                if let Some(t) = e.tokens_read {
                    n.copy_tokens = Some(n.copy_tokens.unwrap_or(0) + t);
                }
            }
            _ => {}
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(stage: &str, via: &str, question: &str) -> Entry {
        Entry {
            id: AskId(1),
            at_ms: 1,
            from: Some(SeatId(1)),
            to: Some(SeatId(2)),
            via: via.into(),
            stage: stage.into(),
            refusal: None,
            question_len: question.len(),
            question_sha256: fingerprint(question),
            program: None,
            model: None,
            reads_tokens: None,
            tokens_read: None,
            cost_usd: None,
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("td-askrecord-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("asks.jsonl")
    }

    #[test]
    fn the_record_never_holds_the_question_text() {
        let path = scratch("words");
        let q = "what is left on the refresh path, secret-word-kumquat?";
        append(&path, &entry("cheap", "line", q)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            !text.contains("kumquat") && !text.contains("refresh path"),
            "{text}"
        );
        assert!(text.contains(&fingerprint(q)), "the fingerprint is kept");
        assert!(text.contains(&format!("\"question_len\":{}", q.len())));
    }

    #[test]
    fn every_stage_writes_one_line() {
        let path = scratch("stages");
        for stage in ["priced", "pressed", "copy_started", "answered"] {
            append(&path, &entry(stage, "bypass", "q")).unwrap();
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 4);
    }

    #[test]
    fn numbers_count_unlined_answers_as_zero_when_none() {
        let path = scratch("numbers");
        assert_eq!(
            numbers(&path).unwrap().unlined_answered,
            0,
            "no file is no asks"
        );
        append(&path, &entry("cheap", "line", "a")).unwrap();
        let mut copy = entry("answered", "bypass", "b");
        copy.tokens_read = Some(360_090);
        append(&path, &copy).unwrap();
        append(&path, &entry("refused", "none", "c")).unwrap();
        append(&path, &entry("answered", "line", "d")).unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{not a line this build reads}\n")
            .unwrap();
        let n = numbers(&path).unwrap();
        assert_eq!(
            n.unlined_answered, 0,
            "a refused ask with no line answered nothing"
        );
        assert_eq!(n.cheap_answers, 1);
        assert_eq!(n.copy_answers, 2);
        assert_eq!(
            n.copy_tokens,
            Some(360_090),
            "the copy that reported nothing adds nothing"
        );
        append(&path, &entry("cheap", "none", "e")).unwrap();
        assert_eq!(
            numbers(&path).unwrap().unlined_answered,
            1,
            "a regression shows up as a number"
        );
    }
}
