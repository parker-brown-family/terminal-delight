//! Notes on a Markdown document: which blocks take one, what each is called,
//! the map an agent is handed, and where the notes are kept.
//!
//! # Kept by TD, never in the file
//!
//! A brief keeps its notes inside itself, in an island its own script reads.
//! A Markdown file has nowhere like that, and the files TD opens are mostly
//! tracked in git: plans, handoffs, READMEs, the `AGENTS.md` every agent
//! loads. A note written into one of those is a diff somebody else's commit
//! sweeps up, or a line in every agent's context. So a Markdown file's notes
//! are kept in TD's state directory, one file per document, keyed by the
//! document's canonical path — the choice Parker made on 2026-09-25 ("TD's
//! own store"), and the one markdown-delight made before it. They are written
//! as each is added, so there is nothing to save. The Markdown file itself is
//! only ever read.
//!
//! # A block's id
//!
//! Made the way the brief's own `notes.js` makes one: a short prefix for what
//! the block is, then the first four words of its text as a slug, and `-2`,
//! `-3` for a repeat. An id is a name, so a note stays on its block when the
//! file moves around it. Change a block's opening words and its notes lose
//! it; they are kept, counted and mapped as notes on words no longer in the
//! file (see [`build_map`]), never dropped.
//!
//! # The map
//!
//! The brief's map names element ids, which an agent searches for. A Markdown
//! file is opened at a line, so this map names the line each block starts on
//! in the file as it is now, and the file by its whole path.

use std::path::{Path, PathBuf};

use super::markdown::{block_plain, Block, MdDoc};
use super::notes::{self, Json, NoteMap, Obj};

/// One block that takes notes: its id, the words that name it, the line it
/// starts on (1-based, as an editor counts; `None` if the parse did not say),
/// which top-level block it is, and which item of it for a list.
#[derive(Clone, Debug, PartialEq)]
pub struct MdAnchor {
    pub nid: String,
    pub title: String,
    pub line: Option<u32>,
    pub block: usize,
    pub item: Option<usize>,
}

/// notes.js `slug()`: lower case, every run of anything but a–z and 0–9 one
/// dash, no dash at either end, at most 44 characters, `x` for nothing.
fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in s.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            if dash && !out.is_empty() {
                out.push('-');
            }
            dash = false;
            out.push(c);
        } else {
            dash = true;
        }
    }
    // Built with no dash at either end, then cut, as notes.js trims first.
    let cut: String = out.chars().take(44).collect();
    if cut.is_empty() {
        "x".into()
    } else {
        cut
    }
}

/// notes.js `titleOf()`: whitespace runs as one space, cut at 72 characters.
fn title_of(s: &str) -> String {
    let t = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() > 72 {
        format!("{}…", t.chars().take(69).collect::<String>())
    } else {
        t
    }
}

/// What a block is called in its id, and how its title opens so a reader
/// of the map knows the kind of block without the file open. `None` for a
/// block that takes no note of its own.
fn kind(block: &Block) -> Option<(&'static str, String)> {
    Some(match block {
        Block::Heading { level, .. } => ("h", format!("{} ", "#".repeat(*level as usize))),
        Block::Paragraph(_) => ("p", String::new()),
        Block::Code(_) => ("code", "``` ".into()),
        Block::Quote(_) => ("quote", "> ".into()),
        Block::Table(_) => ("table", String::new()),
        Block::Html(_) => ("html", String::new()),
        Block::Image { .. } => ("img", "picture: ".into()),
        // A list's items take notes, each on its own; a rule has no words.
        Block::List(_) | Block::Rule => return None,
    })
}

/// An id for a block of this text: the prefix and four words of slug, with
/// a count on a repeat, as notes.js makes one.
fn nid_for(pre: &str, plain: &str, used: &mut std::collections::HashSet<String>) -> String {
    let words = slug(plain).split('-').take(4).collect::<Vec<_>>().join("-");
    let base = format!("{pre}-{words}");
    let mut nid = base.clone();
    let mut n = 2;
    while !used.insert(nid.clone()) {
        nid = format!("{base}-{n}");
        n += 1;
    }
    nid
}

/// A 0-based source line as an editor counts it.
fn editor_line(line: usize) -> Option<u32> {
    u32::try_from(line + 1).ok()
}

/// Every block that takes notes, with its id, title and line: each
/// top-level block, and each item of a top-level list in place of the list,
/// because a note on a list is about one of its items. A nested item's note
/// goes on the top-level item that holds it.
pub fn anchors(doc: &MdDoc) -> Vec<MdAnchor> {
    let mut used = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (i, block) in doc.blocks.iter().enumerate() {
        let meta = doc.meta.get(i);
        if let Block::List(items) = block {
            for (j, (marker, blocks)) in items.iter().enumerate() {
                let plain = blocks.iter().map(block_plain).collect::<Vec<_>>().join(" ");
                let first = plain.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
                out.push(MdAnchor {
                    nid: nid_for("li", &plain, &mut used),
                    title: title_of(&format!("{marker} {first}")),
                    line: meta
                        .and_then(|m| m.items.get(j))
                        .and_then(|&l| editor_line(l)),
                    block: i,
                    item: Some(j),
                });
            }
            continue;
        }
        let Some((pre, mark)) = kind(block) else {
            continue;
        };
        let plain = block_plain(block);
        let first = plain.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        out.push(MdAnchor {
            nid: nid_for(pre, &plain, &mut used),
            title: title_of(&format!("{mark}{first}")),
            line: meta.and_then(|m| editor_line(m.line)),
            block: i,
            item: None,
        });
    }
    out
}

/// The map a Markdown document's notes make: the brief's map in its shape,
/// with each block named by the line it starts on. Notes on a block the file
/// no longer has come last, under the words they were written against.
/// `lines` is each anchor's id, title and line, in the order of the file. A
/// block whose line is not known is named by its id instead: a line nobody
/// counted is not line 0.
pub fn build_map(file: &Path, notes: &NoteMap, lines: &[(&str, &str, Option<u32>)]) -> String {
    let (count, els) = (notes.count(), notes.elements());
    let mut out = vec![
        format!("NOTES — {}", file.display()),
        format!(
            "{count} {} on {els} {}.",
            if count == 1 { "note" } else { "notes" },
            if els == 1 { "block" } else { "blocks" }
        ),
        "Each [L<n>] is the line in that file where the block starts.".into(),
        String::new(),
    ];
    for &(nid, title, line) in lines {
        let list = notes.on(nid);
        if list.is_empty() {
            continue;
        }
        match line {
            Some(line) => out.push(format!("[L{line}] {title}")),
            None => out.push(format!("[{nid}] {title}")),
        }
        for n in list {
            out.push(format!("  · {}", one_line(n.text)));
        }
        out.push(String::new());
    }
    let gone: Vec<&str> = notes
        .0
        .iter()
        .map(|(nid, _)| nid)
        .filter(|nid| !lines.iter().any(|(n, _, _)| n == nid))
        .filter(|nid| !notes.on(nid).is_empty())
        .collect();
    if !gone.is_empty() {
        out.push("On words no longer in the file:".into());
        out.push(String::new());
        for nid in gone {
            let list = notes.on(nid);
            let title = list.iter().find_map(|n| n.title).unwrap_or(nid);
            out.push(format!("[{nid}] {title}"));
            for n in list {
                out.push(format!("  · {}", one_line(n.text)));
            }
            out.push(String::new());
        }
    }
    out.join("\n")
}

/// Each run of line feeds one space, as the brief's map does.
fn one_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_run = false;
    for c in text.chars() {
        if c == '\n' {
            if !in_run {
                out.push(' ');
            }
            in_run = true;
        } else {
            out.push(c);
            in_run = false;
        }
    }
    out
}

// ── the store ───────────────────────────────────────────────────────────────

/// The store's format. A file of a later one is read-only here.
pub const FORMAT: u32 = 1;

/// `$XDG_STATE_HOME/terminal-delight/notes`, the state directory resolved as
/// the brief's backups resolve it: the variable when it is set and absolute,
/// else `~/.local/state`.
fn store_dir() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state")
        })
        .join("terminal-delight/notes")
}

/// FNV-1a over the path's bytes: stable across builds and machines, which
/// the standard library's hasher does not promise.
fn fnv(path: &Path) -> u64 {
    use std::os::unix::ffi::OsStrExt;
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in path.as_os_str().as_bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The document's own path, links followed, so two ways of naming one file
/// keep one set of notes. The path as given when it cannot be resolved.
pub fn canonical(doc: &Path) -> PathBuf {
    std::fs::canonicalize(doc).unwrap_or_else(|_| doc.to_path_buf())
}

/// Where one document's notes are kept.
pub fn store_for(doc: &Path) -> PathBuf {
    store_in(&store_dir(), doc)
}

fn store_in(dir: &Path, doc: &Path) -> PathBuf {
    let doc = canonical(doc);
    let name = doc
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // The name first, for a person listing the directory; the hash is what
    // makes it one file per path.
    dir.join(format!("{}-{:016x}.json", slug(&name), fnv(&doc)))
}

/// The notes kept for a document. No file yet is no notes, which is an
/// answer: nothing was ever written. A file that cannot be read, or was
/// written by a later TD, is an error in words, and nothing is written over
/// it.
pub fn read(store: &Path) -> Result<NoteMap, String> {
    let bytes = match std::fs::read(store) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(NoteMap::default()),
        Err(e) => return Err(format!("the notes kept for it could not be read: {e}")),
    };
    let unreadable = |why: String| {
        format!(
            "the notes kept for it at {} cannot be read: {why}",
            store.display()
        )
    };
    let Json::Obj(top) = notes::parse_json(&bytes).map_err(|e| unreadable(e.0))? else {
        return Err(unreadable("not an object".into()));
    };
    match top.get("format") {
        Some(Json::Num(n)) if *n == f64::from(FORMAT) => {}
        Some(Json::Num(n)) => {
            return Err(format!(
                "its notes were kept by a newer TD (format {}), so this one only reads them",
                notes::js_number(*n)
            ))
        }
        _ => return Err(unreadable("no format".into())),
    }
    match top.get("notes") {
        Some(Json::Obj(map)) => Ok(NoteMap(map.clone())),
        None => Ok(NoteMap::default()),
        Some(_) => Err(unreadable("its notes are not an object".into())),
    }
}

/// Keep a document's notes: a new file beside the old one in the store,
/// renamed into its place, so a reader never sees half of one. The only
/// write in this module, and it never touches the document.
pub fn write(store: &Path, doc: &Path, notes: &NoteMap) -> Result<(), String> {
    use std::io::Write;
    let dir = store.parent().ok_or("the store has no folder")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut top = Obj::default();
    top.insert("format".into(), Json::Num(f64::from(FORMAT)));
    top.insert(
        "file".into(),
        Json::Str(canonical(doc).to_string_lossy().into_owned()),
    );
    top.insert("notes".into(), Json::Obj(notes.0.clone()));
    let out = notes::stringify(&Json::Obj(top)) + "\n";
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(".keep-{}-{stamp}", std::process::id()));
    let written = (|| {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(out.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, store)
    })();
    written.map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("the note could not be kept: {e}")
    })
}

/// Apply edits to what the store holds NOW, not to a copy in memory, and
/// keep the result: two TD windows on one file both keep their notes. Answers
/// what the store holds afterwards. `known` is every block id the file has,
/// so a note is never added to a block that is not there.
pub fn keep(
    store: &Path,
    doc: &Path,
    edits: &[notes::NoteEdit],
    known: &[&str],
) -> Result<NoteMap, String> {
    let mut map = read(store)?;
    let mut concurs = notes::ConcurMap::default();
    notes::apply(&mut map, &mut concurs, edits, known, false).map_err(|r| r.sentence())?;
    write(store, doc, &map)?;
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docview::markdown::parse;
    use crate::docview::notes::NoteEdit;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "td-md-notes-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const PLAN: &str = "# The plan\n\nFirst we read the file.\n\n---\n\n## Slice 2 — the agent side\n\n- one\n- two\n\nFirst we read the file.\n\n```rust\nfn main() {}\n```\n";

    /// Ids are made as notes.js makes them — a prefix, four words of slug,
    /// a count on a repeat — lines are counted from 1, and a rule takes no
    /// note.
    #[test]
    fn every_block_but_a_rule_is_an_anchor_named_as_notes_js_names_one() {
        let doc = parse(PLAN, None);
        let a = anchors(&doc);
        let got: Vec<(&str, &str, u32)> = a
            .iter()
            .map(|a| {
                (
                    a.nid.as_str(),
                    a.title.as_str(),
                    a.line.expect("a parsed block has a line"),
                )
            })
            .collect();
        assert_eq!(
            got,
            vec![
                ("h-the-plan", "# The plan", 1),
                ("p-first-we-read-the", "First we read the file.", 3),
                ("h-slice-2-the-agent", "## Slice 2 — the agent side", 7),
                ("li-one", "• one", 9),
                ("li-two", "• two", 10),
                ("p-first-we-read-the-2", "First we read the file.", 12),
                ("code-fn-main", "``` fn main() {}", 14),
            ]
        );
        assert_eq!(slug("  ¿Qué? Sí  "), "qu-s");
        assert_eq!(slug("—"), "x");
    }

    /// Each item of a top-level list is an anchor of its own, at its own
    /// line; a nested list rides on the item that holds it.
    #[test]
    fn a_list_takes_notes_item_by_item() {
        let doc = parse(
            "Intro.\n\n1. first\n2. second\n   - nested\n3. third\n",
            None,
        );
        let got: Vec<(String, String, Option<u32>, Option<usize>)> = anchors(&doc)
            .into_iter()
            .map(|a| (a.nid, a.title, a.line, a.item))
            .collect();
        assert_eq!(
            got,
            vec![
                ("p-intro".into(), "Intro.".into(), Some(1), None),
                ("li-first".into(), "1. first".into(), Some(3), Some(0)),
                (
                    "li-second-nested".into(),
                    "2. second • nested".into(),
                    Some(4),
                    Some(1)
                ),
                ("li-third".into(), "3. third".into(), Some(6), Some(2)),
            ]
        );
    }

    /// The map names the file whole and each block by its line, counts notes
    /// and blocks in words, and keeps a note whose block has gone, under the
    /// title it was written against.
    #[test]
    fn the_map_names_lines_and_keeps_notes_on_words_no_longer_there() {
        let doc = parse(PLAN, None);
        let a = anchors(&doc);
        let lines: Vec<(&str, &str, Option<u32>)> = a
            .iter()
            .map(|a| (a.nid.as_str(), a.title.as_str(), a.line))
            .collect();
        let mut map = NoteMap::default();
        let mut c = notes::ConcurMap::default();
        let known: Vec<&str> = a.iter().map(|a| a.nid.as_str()).chain(["p-gone"]).collect();
        notes::apply(
            &mut map,
            &mut c,
            &[
                NoteEdit::Add {
                    nid: "h-slice-2-the-agent".into(),
                    title: "## Slice 2 — the agent side".into(),
                    text: "before slice 1\nplease".into(),
                    ts: "2026-09-25 10:00".into(),
                },
                NoteEdit::Add {
                    nid: "p-gone".into(),
                    title: "A sentence since cut".into(),
                    text: "why was this cut?".into(),
                    ts: "2026-09-25 10:01".into(),
                },
            ],
            &known,
            false,
        )
        .unwrap();
        let out = build_map(Path::new("/r/plan.md"), &map, &lines);
        assert_eq!(
            out,
            "NOTES — /r/plan.md\n2 notes on 2 blocks.\nEach [L<n>] is the line in that file where the block starts.\n\n[L7] ## Slice 2 — the agent side\n  · before slice 1 please\n\nOn words no longer in the file:\n\n[p-gone] A sentence since cut\n  · why was this cut?\n"
        );
    }

    /// A note kept is in the store and not in the document, which is byte
    /// for byte what it was; a second keep adds to what the store holds on
    /// disk; a missing store is no notes, and a later format is refused
    /// rather than written over.
    #[test]
    fn keeping_a_note_writes_the_store_and_never_the_document() {
        let dir = tmp("keep");
        let doc = dir.join("plan.md");
        std::fs::write(&doc, PLAN).unwrap();
        let store = store_in(&dir.join("store"), &doc);
        assert_eq!(read(&store).unwrap().count(), 0, "nothing kept yet");
        let add = |nid: &str, text: &str| NoteEdit::Add {
            nid: nid.into(),
            title: "t".into(),
            text: text.into(),
            ts: "2026-09-25 10:00".into(),
        };
        let known = ["h-the-plan", "p-first-we-read-the"];
        keep(&store, &doc, &[add("h-the-plan", "one")], &known).unwrap();
        let after = keep(&store, &doc, &[add("p-first-we-read-the", "two")], &known).unwrap();
        assert_eq!(after.count(), 2);
        assert_eq!(read(&store).unwrap().count(), 2, "both on disk");
        assert_eq!(
            std::fs::read_to_string(&doc).unwrap(),
            PLAN,
            "the document is untouched"
        );
        assert!(
            keep(&store, &doc, &[add("p-not-a-block", "x")], &known).is_err(),
            "no note on a block the file does not have"
        );
        std::fs::write(&store, "{\"format\":2,\"notes\":{}}").unwrap();
        let why = read(&store).unwrap_err();
        assert!(why.contains("newer TD"), "{why}");
        assert!(keep(&store, &doc, &[add("h-the-plan", "x")], &known).is_err());
        assert_eq!(
            std::fs::read_to_string(&store).unwrap(),
            "{\"format\":2,\"notes\":{}}",
            "a later format is never written over"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// One document, one store file, however it is named.
    #[test]
    fn a_document_has_one_store_file_by_its_canonical_path() {
        let dir = tmp("canon");
        let doc = dir.join("plan.md");
        std::fs::write(&doc, PLAN).unwrap();
        let link = dir.join("alias.md");
        std::os::unix::fs::symlink(&doc, &link).unwrap();
        let s = dir.join("store");
        assert_eq!(store_in(&s, &doc), store_in(&s, &link));
        assert_ne!(store_in(&s, &doc), store_in(&s, &dir.join("other.md")));
        assert!(store_in(&s, &doc)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("plan-md-"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
