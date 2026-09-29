//! Pictures pasted onto a document's elements: kept by TD, listed in the note
//! box, handed to an agent by their path, and never drawn.
//!
//! # Attached to an element, not written into a note
//!
//! A person writing a note on a brief or a Markdown file pastes a screenshot,
//! and the picture is attached to the element the note box is open on. Parker,
//! 2026-09-29: *"Pasting an image into a comment shouldn't be in line. It will
//! attach that image to the element … It gets added as a list item underneath
//! the comments"*. The note box lists an element's pictures as
//! `[doc-image #1]`, `[doc-image #2]` and never shows where they are; the map
//! an agent is handed gives each its full path. Nothing is drawn and nothing
//! opens on a click: *"a one-way pass of a screenshot"*.
//!
//! # Where
//!
//! `<notes root>/pictures/<doc>/doc-image-<n>.<ext>`, where `<doc>` is the name
//! the document's notes file already has ([`md_notes::stem`]), with the
//! document's list beside the pictures in `images.json`. Briefs and Markdown
//! files alike. A brief keeps its notes inside itself, but a picture cannot go
//! there (one median screenshot is five times the median brief), so its list
//! lives here with the pictures, and the island a brief shares with its own
//! `notes.js` does not change.
//!
//! # A number is never given out twice
//!
//! Pictures are numbered per document, `#1` onwards, and `next` only counts
//! up. A note saying "see doc-image #2" was written about one picture; when #2
//! is deleted, no later paste becomes #2 and changes what the note meant.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::md_notes;

/// The list's format. A list of a later one is read-only here.
pub const FORMAT: u32 = 1;

/// The document's list of pictures, in its pictures folder.
pub const LIST: &str = "images.json";

/// One picture pasted onto an element.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocImage {
    /// Its number in the document: `[doc-image #n]`.
    pub n: u32,
    /// Its file, in the document's pictures folder.
    pub file: String,
    /// When it was pasted, written as a note's time is.
    pub ts: String,
    /// FNV-1a of its bytes, so the same picture pasted onto the same element
    /// twice stays one picture.
    pub hash: String,
    /// Whether its file was there when the list was last read or written.
    /// Not kept in the list: it is a fact about this disk, found by looking.
    #[serde(skip)]
    pub here: bool,
}

impl DocImage {
    /// How the note box and the map name it.
    pub fn label(&self) -> String {
        format!("[doc-image #{}]", self.n)
    }
}

/// Every picture pasted onto a document, by the element it is on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocImages {
    pub format: u32,
    /// The document, as TD names it, for a person reading the folder.
    pub file: String,
    /// The number the next picture gets. Only ever counts up.
    pub next: u32,
    pub on: BTreeMap<String, Vec<DocImage>>,
}

impl DocImages {
    /// A document nothing was ever pasted onto.
    pub fn empty(doc: &Path) -> DocImages {
        DocImages {
            format: FORMAT,
            file: md_notes::canonical(doc).to_string_lossy().into_owned(),
            next: 1,
            on: BTreeMap::new(),
        }
    }

    /// The pictures on one element, oldest first.
    pub fn on(&self, nid: &str) -> &[DocImage] {
        self.on.get(nid).map_or(&[], Vec::as_slice)
    }

    /// Every picture, on every element.
    pub fn count(&self) -> usize {
        self.on.values().map(Vec::len).sum()
    }
}

/// The folder a document's pictures are kept in, under TD's notes root.
pub fn folder(root: &Path, doc: &Path) -> PathBuf {
    root.join("pictures").join(md_notes::stem(doc))
}

/// The file a picture is kept in.
pub fn path_of(folder: &Path, img: &DocImage) -> PathBuf {
    folder.join(&img.file)
}

/// The pictures kept for a document. No list yet is no pictures, which is an
/// answer: nothing was ever pasted. A list that cannot be read, or was written
/// by a later TD, is an error in words, and nothing is written over it. Each
/// picture is looked for on disk, so one whose file went is shown as gone
/// rather than dropped.
pub fn read(folder: &Path, doc: &Path) -> Result<DocImages, String> {
    let list = folder.join(LIST);
    let bytes = match std::fs::read(&list) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(DocImages::empty(doc)),
        Err(e) => return Err(format!("the pictures kept for it could not be read: {e}")),
    };
    let mut images: DocImages = serde_json::from_slice(&bytes).map_err(|e| {
        format!(
            "the pictures kept for it at {} cannot be read: {e}",
            list.display()
        )
    })?;
    if images.format != FORMAT {
        return Err(format!(
            "its pictures were kept by a newer TD (format {}), so this one only reads them",
            images.format
        ));
    }
    for img in images.on.values_mut().flatten() {
        img.here = path_of(folder, img).is_file();
    }
    Ok(images)
}

/// What a paste onto an element came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attached {
    /// A new picture, with this number.
    New(u32),
    /// The same bytes were already on this element, as this number, and
    /// nothing was written.
    Already(u32),
}

/// Attach a picture to the element `nid`: its file written first, then the
/// list that names it, each a new file renamed into place so a reader never
/// sees half of one. Applied to what the list holds now, not to a copy in
/// memory. `ext` is the picture's file extension; anything but a short run of
/// letters and digits is refused, since it becomes part of a file name.
pub fn attach(
    folder: &Path,
    doc: &Path,
    nid: &str,
    bytes: &[u8],
    ext: &str,
    ts: &str,
) -> Result<(DocImages, Attached), String> {
    if ext.is_empty()
        || ext.len() > 5
        || !ext
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        return Err(format!("“{ext}” is not a picture's file extension"));
    }
    let mut images = read(folder, doc)?;
    let hash = format!("{:016x}", md_notes::fnv_bytes(bytes));
    if let Some(same) = images.on(nid).iter().find(|i| i.hash == hash) {
        return Ok((images.clone(), Attached::Already(same.n)));
    }
    std::fs::create_dir_all(folder)
        .map_err(|e| format!("the pictures folder could not be made: {e}"))?;
    let n = images.next;
    let file = format!("doc-image-{n}.{ext}");
    put(folder, &file, bytes).map_err(|e| format!("the picture could not be kept: {e}"))?;
    images.next = n + 1;
    images
        .on
        .entry(nid.to_string())
        .or_default()
        .push(DocImage {
            n,
            file,
            ts: ts.to_string(),
            hash,
            here: true,
        });
    write_list(folder, &images)?;
    Ok((images, Attached::New(n)))
}

/// Take picture `n` off the element `nid`: out of the list, and its file
/// deleted. A picture no longer in the list is not an error, and a file
/// already gone is not one either: afterwards the list says what the person
/// asked it to.
pub fn remove(folder: &Path, doc: &Path, nid: &str, n: u32) -> Result<DocImages, String> {
    let mut images = read(folder, doc)?;
    let Some(list) = images.on.get_mut(nid) else {
        return Ok(images);
    };
    let Some(i) = list.iter().position(|img| img.n == n) else {
        return Ok(images);
    };
    let gone = list.remove(i);
    if list.is_empty() {
        images.on.remove(nid);
    }
    write_list(folder, &images)?;
    match std::fs::remove_file(path_of(folder, &gone)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(format!(
                "{} was taken off the list, but its file could not be deleted: {e}",
                gone.label()
            ))
        }
    }
    Ok(images)
}

fn write_list(folder: &Path, images: &DocImages) -> Result<(), String> {
    let mut out = serde_json::to_string_pretty(images)
        .map_err(|e| format!("the list of pictures could not be written: {e}"))?;
    out.push('\n');
    put(folder, LIST, out.as_bytes())
        .map_err(|e| format!("the list of pictures could not be written: {e}"))
}

/// Write `bytes` to `folder/name` by way of a new file renamed into place.
fn put(folder: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = folder.join(format!(".put-{}-{stamp}", std::process::id()));
    let written = (|| {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, folder.join(name))
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "td-doc-images-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Nothing pasted yet is an empty list, written nowhere; the folder is
    /// named as the document's notes file is.
    #[test]
    fn a_document_nothing_was_pasted_onto_has_an_empty_list_and_no_folder() {
        let root = tmp("empty");
        let doc = root.join("plan.md");
        let folder = folder(&root, &doc);
        assert_eq!(
            folder,
            root.join("pictures").join(md_notes::stem(&doc)),
            "the name the notes file has, without .json"
        );
        let images = read(&folder, &doc).expect("no list is an answer");
        assert_eq!((images.next, images.count()), (1, 0));
        assert!(!folder.exists(), "reading writes nothing");
    }

    /// A paste writes the picture and the list, numbers from 1, and a second
    /// picture on another element takes the next number: numbers are the
    /// document's, not the element's.
    #[test]
    fn pictures_are_numbered_by_the_document_and_kept_as_files() {
        let root = tmp("attach");
        let doc = root.join("brief.html");
        let folder = folder(&root, &doc);
        let (_, a) =
            attach(&folder, &doc, "fig-01", b"first", "png", "2026-09-29 11:02").expect("kept");
        let (images, b) =
            attach(&folder, &doc, "ask-2", b"second", "jpg", "2026-09-29 11:03").expect("kept");
        assert_eq!((a, b), (Attached::New(1), Attached::New(2)));
        assert_eq!(images.next, 3);
        assert_eq!(images.on("fig-01")[0].label(), "[doc-image #1]");
        assert_eq!(
            std::fs::read(folder.join("doc-image-2.jpg")).expect("the file"),
            b"second"
        );
        let again = read(&folder, &doc).expect("read back");
        assert_eq!(again, images, "the list on disk is the list in hand");
        assert!(again.on("ask-2")[0].here);
    }

    /// The same bytes pasted onto the same element again are the picture
    /// already there: nothing is written and no number is spent. Onto another
    /// element they are a new picture.
    #[test]
    fn the_same_picture_pasted_twice_on_one_element_is_one_picture() {
        let root = tmp("again");
        let doc = root.join("plan.md");
        let folder = folder(&root, &doc);
        attach(&folder, &doc, "p-one", b"shot", "png", "t").expect("kept");
        let (images, again) = attach(&folder, &doc, "p-one", b"shot", "png", "t").expect("kept");
        assert_eq!(again, Attached::Already(1));
        assert_eq!((images.next, images.count()), (2, 1));
        let (_, elsewhere) = attach(&folder, &doc, "p-two", b"shot", "png", "t").expect("kept");
        assert_eq!(elsewhere, Attached::New(2));
    }

    /// Deleting a picture takes it off the list and deletes its file, and its
    /// number is never given out again, so a note that named it still names
    /// nothing else.
    #[test]
    fn a_deleted_pictures_number_is_never_given_out_again() {
        let root = tmp("remove");
        let doc = root.join("plan.md");
        let folder = folder(&root, &doc);
        attach(&folder, &doc, "p-one", b"a", "png", "t").expect("kept");
        attach(&folder, &doc, "p-one", b"b", "png", "t").expect("kept");
        let images = remove(&folder, &doc, "p-one", 2).expect("removed");
        assert_eq!(images.on("p-one").len(), 1);
        assert!(!folder.join("doc-image-2.png").exists(), "its file went");
        let (_, next) = attach(&folder, &doc, "p-one", b"c", "png", "t").expect("kept");
        assert_eq!(next, Attached::New(3), "not 2");
        // Deleting what is already gone changes nothing and is no error.
        let same = remove(&folder, &doc, "p-one", 2).expect("nothing to remove");
        assert_eq!(same.count(), 2);
        let emptied = remove(&folder, &doc, "p-gone", 1).expect("no such element");
        assert_eq!(emptied.count(), 2);
    }

    /// A picture whose file went is still listed, and says it is not here: a
    /// picture nobody can see is not a picture that was never pasted.
    #[test]
    fn a_picture_whose_file_went_is_listed_as_not_here() {
        let root = tmp("gone");
        let doc = root.join("plan.md");
        let folder = folder(&root, &doc);
        attach(&folder, &doc, "p-one", b"a", "png", "t").expect("kept");
        std::fs::remove_file(folder.join("doc-image-1.png")).unwrap();
        let images = read(&folder, &doc).expect("read");
        assert_eq!(images.count(), 1);
        assert!(!images.on("p-one")[0].here);
    }

    /// A list this TD cannot read, or one a later TD wrote, is refused in
    /// words, and a paste onto it writes nothing over it.
    #[test]
    fn a_list_that_cannot_be_read_is_never_written_over() {
        let root = tmp("unreadable");
        let doc = root.join("plan.md");
        let folder = folder(&root, &doc);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(LIST), b"{ not json").unwrap();
        assert!(read(&folder, &doc).unwrap_err().contains("cannot be read"));
        assert!(attach(&folder, &doc, "p", b"a", "png", "t").is_err());
        assert_eq!(std::fs::read(folder.join(LIST)).unwrap(), b"{ not json");
        std::fs::write(
            folder.join(LIST),
            br#"{"format":2,"file":"x","next":1,"on":{}}"#,
        )
        .unwrap();
        assert!(read(&folder, &doc).unwrap_err().contains("newer TD"));
    }

    /// The extension becomes part of a file name, so only a short run of
    /// lowercase letters and digits is taken.
    #[test]
    fn an_extension_that_is_not_a_plain_word_is_refused() {
        let root = tmp("ext");
        let doc = root.join("plan.md");
        let folder = folder(&root, &doc);
        for bad in ["", "../x", "p/ng", "PNG", "toolong"] {
            assert!(
                attach(&folder, &doc, "p", b"a", bad, "t").is_err(),
                "{bad:?}"
            );
        }
        assert!(!folder.exists(), "nothing was written for any of them");
    }
}
