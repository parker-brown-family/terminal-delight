//! Original review evidence and deterministic reattachment. Resolution is a
//! view of the saved evidence, never a rewrite of what the reader reviewed.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub revision: Option<String>,
    pub anchors: Vec<MdAnchor>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub revision: Option<String>,
    pub original_nid: String,
    pub kind: String,
    pub quote: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct EditContext {
    pub evidence: Option<Evidence>,
    pub stored_nid: Option<String>,
    pub fingerprint: Option<String>,
}

impl Frame {
    pub fn evidence(&self, nid: &str) -> Option<Evidence> {
        let i = self.anchors.iter().position(|a| a.nid == nid)?;
        let a = &self.anchors[i];
        Some(Evidence {
            revision: self.revision.clone(),
            original_nid: nid.into(),
            kind: a.kind.clone(),
            quote: a.quote.clone(),
            before: i.checked_sub(1).map(|j| self.anchors[j].quote.clone()),
            after: self.anchors.get(i + 1).map(|a| a.quote.clone()),
        })
    }
}

fn normalized(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn string<'a>(o: &'a Obj, key: &str) -> Option<&'a str> {
    match o.get(key) {
        Some(Json::Str(s)) => Some(s),
        _ => None,
    }
}

fn evidence(o: &Obj) -> Option<Evidence> {
    serde_json::from_str(&notes::stringify(o.get("_td_md")?)).ok()
}

fn evidence_json(e: &Evidence) -> Json {
    notes::parse_json(&serde_json::to_vec(e).expect("review evidence serializes"))
        .expect("serialized review evidence is JSON")
}

fn persistent(o: &Obj) -> Obj {
    let mut o = o.clone();
    o.remove("_td_stored_nid");
    o.remove("_td_resolution");
    o
}

fn fingerprint(o: &Obj) -> String {
    crate::artifact_archive::digest(notes::stringify(&Json::Obj(persistent(o))).as_bytes())
}

fn context_score(frame: &Frame, i: usize, e: &Evidence) -> usize {
    let before = i
        .checked_sub(1)
        .map(|j| normalized(&frame.anchors[j].quote));
    let after = frame.anchors.get(i + 1).map(|a| normalized(&a.quote));
    usize::from(
        e.before
            .as_ref()
            .is_some_and(|s| Some(normalized(s)) == before),
    ) + usize::from(
        e.after
            .as_ref()
            .is_some_and(|s| Some(normalized(s)) == after),
    )
}

/// A small wording edit needs unchanged neighbouring text as well as a large
/// shared word sequence. This does not infer semantic equivalence.
fn nearby_words(a: &str, b: &str) -> bool {
    let a: Vec<_> = a.split_whitespace().collect();
    let b: Vec<_> = b.split_whitespace().collect();
    if a.len().min(b.len()) < 6 {
        return false;
    }
    let prefix = a.iter().zip(&b).take_while(|(a, b)| a == b).count();
    let suffix = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    prefix.max(suffix) * 5 >= a.len().max(b.len()) * 4
}

fn locate<'a>(frame: &'a Frame, e: &Evidence) -> (Option<&'a str>, &'static str) {
    let quote = normalized(&e.quote);
    if e.revision.is_some() && e.revision == frame.revision {
        if let Some(a) = frame
            .anchors
            .iter()
            .find(|a| a.nid == e.original_nid && normalized(&a.quote) == quote)
        {
            return (Some(&a.nid), "same revision");
        }
    }
    if quote.is_empty() {
        return (None, "quotation unavailable");
    }
    let exact: Vec<_> = frame
        .anchors
        .iter()
        .enumerate()
        .filter(|(_, a)| a.kind == e.kind && normalized(&a.quote) == quote)
        .collect();
    if exact.len() == 1 {
        return (Some(&exact[0].1.nid), "quotation matched");
    }
    let candidates: Vec<_> = if exact.is_empty() {
        frame
            .anchors
            .iter()
            .enumerate()
            .filter(|(i, a)| {
                a.kind == e.kind
                    && nearby_words(&e.quote, &a.quote)
                    && context_score(frame, *i, e) > 0
            })
            .collect()
    } else {
        exact
    };
    let best = candidates
        .iter()
        .map(|(i, _)| context_score(frame, *i, e))
        .max();
    let winners: Vec<_> = candidates
        .iter()
        .filter(|(i, _)| Some(context_score(frame, *i, e)) == best)
        .collect();
    if winners.len() == 1 && best.is_some_and(|n| n > 0) {
        let a = winners[0].1;
        return (
            Some(&a.nid),
            if normalized(&a.quote) == quote {
                "quotation and context matched"
            } else {
                "wording changed; context matched"
            },
        );
    }
    if candidates.is_empty() {
        (None, "passage not found")
    } else {
        (None, "ambiguous passage; no automatic attachment")
    }
}

/// Project notes onto the current parse while keeping their persistent keys in
/// private display metadata. An unresolved note has a separate key, even when
/// its old first-four-words key happens to exist on another passage now.
pub fn display(raw: &NoteMap, frame: &Frame) -> NoteMap {
    let mut shown = NoteMap::default();
    for (stored, value) in raw.0.iter() {
        let Json::Arr(list) = value else {
            shown.0.insert(stored.into(), value.clone());
            continue;
        };
        for item in list {
            let Json::Obj(o) = item else {
                push(&mut shown, stored, item.clone());
                continue;
            };
            let (nid, status) = match evidence(o) {
                Some(e) => {
                    let (found, status) = locate(frame, &e);
                    (
                        found
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("md-unresolved-{}", fingerprint(o))),
                        status,
                    )
                }
                None if o.get("_td_md").is_some() => (
                    format!("md-unresolved-{}", fingerprint(o)),
                    "review evidence unreadable; no automatic attachment",
                ),
                None => (
                    stored.to_string(),
                    "legacy note; quotation and revision unavailable",
                ),
            };
            let mut note = persistent(o);
            note.insert("_td_stored_nid".into(), Json::Str(stored.into()));
            note.insert("_td_resolution".into(), Json::Str(status.into()));
            push(&mut shown, &nid, Json::Obj(note));
        }
    }
    shown
}

fn push(map: &mut NoteMap, nid: &str, note: Json) {
    if !matches!(map.0.get(nid), Some(Json::Arr(_))) {
        map.0.insert(nid.into(), Json::Arr(Vec::new()));
    }
    if let Some(Json::Arr(list)) = map.0.get_mut(nid) {
        list.push(note);
    }
}

pub fn edit_context(
    frame: Option<&Frame>,
    shown: Option<&NoteMap>,
    edit: &notes::NoteEdit,
) -> EditContext {
    match edit {
        notes::NoteEdit::Add { nid, .. } => EditContext {
            evidence: frame.and_then(|frame| frame.evidence(nid)),
            ..Default::default()
        },
        notes::NoteEdit::Delete { nid, text, ts } => {
            let found = shown.and_then(|map| match map.0.get(nid) {
                Some(Json::Arr(list)) => list.iter().find_map(|v| match v {
                    Json::Obj(o)
                        if string(o, "text") == Some(text)
                            && ts.as_deref().is_none_or(|ts| string(o, "ts") == Some(ts)) =>
                    {
                        Some(o)
                    }
                    _ => None,
                }),
                _ => None,
            });
            EditContext {
                stored_nid: found.map(|o| string(o, "_td_stored_nid").unwrap_or(nid).into()),
                fingerprint: found.map(fingerprint),
                evidence: None,
            }
        }
        _ => EditContext::default(),
    }
}

/// Annotate the just-added note, in a pending view or in the persistent map.
pub fn enrich_last(map: &mut NoteMap, edit: &notes::NoteEdit, context: &EditContext) {
    if let (notes::NoteEdit::Add { nid, .. }, Some(e)) = (edit, &context.evidence) {
        if let Some(Json::Arr(list)) = map.0.get_mut(nid) {
            if let Some(Json::Obj(o)) = list.last_mut() {
                o.insert("_td_md".into(), evidence_json(e));
            }
        }
    }
}

/// Preserve the context from the instant of the edit, even if another save or a
/// document reload was in flight. Deletion addresses the saved note, not its
/// temporary position in the new document.
pub fn keep_with_evidence(
    store: &Path,
    doc: &Path,
    edits: &[notes::NoteEdit],
    contexts: &[EditContext],
) -> Result<NoteMap, Words> {
    let _lock = lock_store(store)?;
    let mut raw = read(store).map_err(Words::as_is)?;
    for (i, edit) in edits.iter().enumerate() {
        let context = contexts
            .get(i)
            .ok_or_else(|| Words::as_is("Review context is unavailable; note was not changed."))?;
        match edit {
            notes::NoteEdit::Add { nid, .. } => {
                let e = context.evidence.as_ref().ok_or_else(|| {
                    Words::as_is("The reviewed passage is unavailable; note was not saved.")
                })?;
                if e.original_nid != *nid {
                    return Err(Words::as_is("Review context does not match the note."));
                }
                notes::apply(
                    &mut raw,
                    &mut notes::ConcurMap::default(),
                    std::slice::from_ref(edit),
                    &[nid],
                    false,
                )
                .map_err(|r| Words::said(|l| r.say(l)))?;
                enrich_last(&mut raw, edit, context);
            }
            notes::NoteEdit::Delete { .. } => {
                let key = context
                    .stored_nid
                    .as_deref()
                    .ok_or_else(|| Words::as_is("The saved note could not be identified."))?;
                let empty = if let Some(Json::Arr(list)) = raw.0.get_mut(key) {
                    if let Some(at) = list.iter().position(|v| match v {
                        Json::Obj(o) => {
                            context.fingerprint.as_deref() == Some(fingerprint(o).as_str())
                        }
                        _ => false,
                    }) {
                        list.remove(at);
                    }
                    list.is_empty()
                } else {
                    false
                };
                if empty {
                    raw.0.remove(key);
                }
            }
            _ => {
                return Err(Words::as_is(
                    "Markdown review notes do not support concurrence stamps.",
                ))
            }
        }
    }
    write(store, doc, &raw)?;
    Ok(raw)
}

pub(super) fn lock_store(store: &Path) -> Result<std::fs::File, Words> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    let dir = store
        .parent()
        .ok_or_else(|| Words::as_is("Notes store has no directory."))?;
    std::fs::create_dir_all(dir).map_err(|e| Words::as_is(e.to_string()))?;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(store.with_extension("lock"))
        .map_err(|e| Words::as_is(e.to_string()))?;
    loop {
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == 0 {
            return Ok(file);
        }
        let err = std::io::Error::last_os_error();
        if err.kind() != std::io::ErrorKind::Interrupted {
            return Err(Words::as_is(err.to_string()));
        }
    }
}

/// The exported review includes the original words and revision, not merely the
/// current line number that happens to hold a matching passage.
fn summary(o: &Obj) -> Option<String> {
    let status = string(o, "_td_resolution");
    match evidence(o) {
        Some(e) => Some(format!(
            "Original quotation: {:?}\nRevision SHA-256: {}\nAttachment: {}",
            e.quote,
            e.revision.as_deref().unwrap_or("unavailable"),
            status.unwrap_or("captured at note creation")
        )),
        None => status.map(str::to_owned),
    }
}

pub fn summaries(map: &NoteMap, nid: &str) -> Vec<String> {
    let Some(Json::Arr(list)) = map.0.get(nid) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|v| match v {
            Json::Obj(o) => summary(o),
            _ => None,
        })
        .collect()
}

pub fn summary_at(map: &NoteMap, nid: &str, i: usize) -> Option<String> {
    let Some(Json::Arr(list)) = map.0.get(nid) else {
        return None;
    };
    list.iter()
        .filter_map(|v| match v {
            Json::Obj(o) if string(o, "text").is_some() => Some(o),
            _ => None,
        })
        .nth(i)
        .and_then(summary)
}

#[cfg(test)]
mod tests;
