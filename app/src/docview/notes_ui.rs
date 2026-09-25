//! The gpui half of a brief's notes: the buttons, the concur stamps, the note
//! box and the notes bar TD draws over a page, and the notes a reader adds
//! there until they are saved into the file.
//!
//! # TD's layer over the page, the page's own hidden
//!
//! The page engine hides the brief's own notes chrome before it takes the
//! picture (`extract.js`), because a picture of it would be a picture of
//! buttons that do nothing and badges that go stale. What the brief would draw
//! there, TD draws here instead, from the file's islands: a button on every
//! anchor, where the brief's own hidden button keeps its box, with the count
//! of its notes, shown while the pointer is over the anchor or always once it
//! has a note; the rule down an anchor's left edge that the brief's stylesheet
//! gives an anchor with notes; the dashed space that takes a CONCUR stamp,
//! and the stamp at the angle the brief's own notes.js would give it; and a
//! bar that counts them, copies the map and saves.
//!
//! # What a browser shows
//!
//! The layer shows what a browser opened fresh shows from the same file (the
//! pure rules are in [`super::notes`] and held to the skill's fixtures). A
//! file with no notes island takes no notes, and the bar says so in those
//! words rather than counting nothing: read-only is not empty.
//!
//! # Notes are deltas until they are saved
//!
//! A note added, a note deleted, a stamp put down or peeled off is held here
//! as an edit ([`NoteEdit`]) on top of what the file said, and the page shows
//! the two together. Nothing reaches the disk until the bar's save is pressed;
//! the bar says how many edits are waiting. A save reads the file fresh and
//! applies the edits to what it holds then, so a note another writer added
//! meanwhile survives, and a note deleted is found by its words rather than
//! its place. A crash loses what was not saved, which is least-confident
//! decision 6, taken with open eyes.
//!
//! # No handlers
//!
//! Like everything under `docview`, this registers no mouse handler. The pane
//! un-bends a press and hands it to the view, which asks [`hit`] and
//! [`NotesLayer::press`]; the bar and the note box are laid out by gpui, so
//! where they landed is recorded at paint by canvases that listen to nothing.
//! Keys reach the note box through the pane too, which hands every key to the
//! view while [`NotesLayer::has_caret`] says a draft is being written.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::SystemTime;

use gpui::{
    canvas, div, hsla, prelude::*, px, radians, rgb, svg, AnyElement, App, Bounds, ClipboardItem,
    FontWeight, Hsla, Keystroke, Pixels, Point, SharedString, Transformation,
};

use super::engine::{Anchor, ConcurSupport, RectCss};
use super::notes::{
    apply, build_map, stamp_pose, utc_minute, ConcurMap, NoteEdit, NoteMap, NotesRead, Refusal,
    FORMAT,
};
use super::page::{place, PageToView};
use crate::theme::Theme;
use crate::EditBuffer;

/// notes.css: the brief's note button is 26 × 26 CSS px, 8 px in from its
/// anchor's top-right corner. A Markdown block's is the same size, in the
/// column's gutter.
pub const BUTTON_CSS: f32 = 26.0;
const BUTTON_INSET_CSS: f32 = 8.0;
/// notes.css: the rule down the left edge of an anchor with notes.
const RULE_CSS: f32 = 3.0;
/// notes.css: a stamp overhangs its concur space by 12 CSS px on every side.
const STAMP_OVERHANG_CSS: f32 = 12.0;
/// notes.js: the stamp's ink.
const STAMP_INK: u32 = 0x35c27a;
/// A note longer than this is a paste gone wrong, not a note.
const MAX_NOTE_CHARS: usize = 20_000;

const STAMP_SVG: &[u8] = include_bytes!("../../assets/img/concur-stamp.svg");

/// What the layer can show of a brief's notes, from the file as a browser
/// opened fresh would show them.
#[derive(Clone, Debug, PartialEq)]
pub enum Shown {
    /// The file has no notes island: it takes no notes. Read-only, which is
    /// a different thing from a brief with none.
    NoIsland,
    /// The island sits after the page's notes script, which reads it before
    /// the parser gets there: no browser ever shows what it holds.
    AfterScript,
    /// The page has an island and no notes script ran, so nothing on it is
    /// annotatable in a browser either.
    NoScript,
    /// The island is there and cannot be read.
    Unreadable(String),
    Notes {
        notes: NoteMap,
        concurs: ConcurMap,
    },
}

/// The note box: one anchor's notes, open over the page, and the note being
/// written there.
#[derive(Clone, Debug)]
pub struct NoteBox {
    pub nid: String,
    pub title: String,
    pub draft: EditBuffer,
    /// Open on notes whose words a Markdown file no longer has: they can be
    /// read and deleted, and nothing can be added to them.
    pub gone: bool,
}

/// A part of the bar or the note box, as the last paint laid it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    CopyMap,
    /// ↪: the map, into the prompt of the agent the brief sits beside.
    Send,
    Save,
    /// Notes on words a Markdown file no longer has: opens them.
    Gone,
    /// Anywhere on the bar that is not a button.
    Bar,
    /// Anywhere inside the note box.
    Box,
    CloseBox,
    AddNote,
    /// The delete on the note box's `n`th note.
    Delete(usize),
}

/// Something the layer draws for one anchor, in the view's own flat
/// coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    /// The rule down the left edge of an anchor with notes.
    Rule { at: Bounds<Pixels> },
    /// The note button: the count on it, or none while it only shows because
    /// the pointer is over its anchor.
    Button {
        nid: String,
        title: String,
        at: Bounds<Pixels>,
        count: usize,
        open: bool,
    },
    /// An empty space on a decision that takes a CONCUR stamp.
    ConcurSpace { nid: String, at: Bounds<Pixels> },
    /// A stamp: its box before it is turned, and how far it is turned.
    Stamp {
        nid: String,
        at: Bounds<Pixels>,
        degrees: f32,
    },
}

/// What a press on the marks landed on.
#[derive(Clone, Debug, PartialEq)]
pub enum MarkHit {
    Open { nid: String, title: String },
    Concur(String),
}

/// What the bar last said, and in which voice.
#[derive(Clone, Debug, PartialEq)]
pub enum Said {
    /// Something happened as asked.
    Done(String),
    /// Something is under way.
    Working(String),
    /// Something was refused or failed: nothing was written.
    Refused(String),
}

impl Said {
    pub fn text(&self) -> &str {
        match self {
            Said::Done(s) | Said::Working(s) | Said::Refused(s) => s,
        }
    }
}

/// Where each part of the bar and the note box was painted, by the canvases
/// that recorded it.
type Zones = Rc<RefCell<Vec<(Bounds<Pixels>, Zone)>>>;

/// Where a layer's notes are written.
#[derive(Clone, Debug, PartialEq)]
pub enum Keeping {
    /// Into the brief itself, by 💾 save into file (and by ↪).
    File,
    /// Into TD's own store as each is written, for a Markdown document whose
    /// notes never go into it (see `md_notes.rs`). There is nothing to save,
    /// so the bar has no 💾, and the map names this path and lines.
    Store(std::path::PathBuf),
}

/// A document's notes, as TD shows them over it: a brief's over its page,
/// a Markdown file's over its column.
pub struct NotesLayer {
    /// The page's `NOTES_FILE`, what the map's header names.
    label: String,
    keeping: Keeping,
    /// What the file held when it was last read.
    base: Shown,
    concur: ConcurSupport,
    /// The edits made here and not yet saved, oldest first.
    pending: Vec<NoteEdit>,
    /// `base` with `pending` applied: what the page shows. `None` unless the
    /// file's notes can be shown at all.
    applied: Option<(NoteMap, ConcurMap)>,
    /// Whether a save into this file could be made, and in words why not.
    writable: Result<(), Refusal>,
    note_box: Option<NoteBox>,
    said: Option<Said>,
    /// What came of the last ↪, kept apart from `said` because a send saves:
    /// the save's own progress would otherwise write over where the notes
    /// landed before anyone had read it.
    sent: Option<Said>,
    /// A save is running: another waits for it.
    saving: bool,
    /// The file is not on disk any more. The page stays up; saving is off.
    gone: bool,
    /// Escape was pressed once with edits unsaved, and the bar said so: the
    /// next Escape closes the document without them.
    close_warned: bool,
    zones: Zones,
}

/// What a press on the layer did.
#[derive(Debug, PartialEq)]
pub enum LayerPress {
    /// Not the layer's: the page under it may have it.
    Pass,
    /// Taken, with nothing more for the view to do.
    Took,
    /// The bar's save: the view saves, since it holds the engine and the file.
    Save,
    /// The bar's ↪: the map goes to the pane the brief sits beside, which
    /// only the workspace can reach, and the view saves as it goes when
    /// [`Sending::saves`] says so.
    Send(Sending),
}

/// What a press on ↪ does: the map it pastes, and whether it saves.
#[derive(Clone, Debug, PartialEq)]
pub struct Sending {
    /// The map as shown, unsaved edits and all: copy map's text.
    pub map: String,
    /// The press saves the waiting edits into the file as it sends them.
    pub saves: bool,
    /// Edits in the map that will not be in the file after the press: all of
    /// them where no save can be made, none where one is under way.
    pub unsaved: usize,
}

fn rect_css(x: f32, y: f32, w: f32, h: f32) -> RectCss {
    RectCss { x, y, w, h }
}

/// Pure. The anchor a pointer is over, by its own box or its button's.
fn hot(anchor: &Anchor, map: &PageToView, at: Option<Point<Pixels>>) -> bool {
    let Some(at) = at else { return false };
    [anchor.rect, button_rect(anchor)]
        .into_iter()
        .flatten()
        .filter_map(|r| place(r, map))
        .any(|b| b.contains(&at))
}

/// The page's own (hidden) button, else where notes.css would put it.
fn button_rect(anchor: &Anchor) -> Option<RectCss> {
    anchor.button.or_else(|| {
        anchor.rect.map(|r| {
            rect_css(
                r.x + r.w - BUTTON_INSET_CSS - BUTTON_CSS,
                r.y + BUTTON_INSET_CSS,
                BUTTON_CSS,
                BUTTON_CSS,
            )
        })
    })
}

/// notes.js's `ta.value.trim()`.
fn trimmed(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

impl NotesLayer {
    /// What the page's own notes would show, read from the bytes the page
    /// was drawn from. `notes_file` and `concur` come from the brief's own
    /// notes.js as it ran in the engine; `tagged` is how many anchors it made.
    pub fn new(
        read: NotesRead,
        notes_file: Option<String>,
        concur: ConcurSupport,
        tagged: u32,
        path: &Path,
    ) -> NotesLayer {
        let label = notes_file.unwrap_or_else(|| {
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        let (base, writable) = Self::judge(read, tagged, path);
        let mut layer = NotesLayer {
            label,
            keeping: Keeping::File,
            base,
            concur,
            pending: Vec::new(),
            applied: None,
            writable,
            note_box: None,
            said: None,
            sent: None,
            saving: false,
            gone: false,
            close_warned: false,
            zones: Rc::new(RefCell::new(Vec::new())),
        };
        layer.refresh();
        layer
    }

    /// A Markdown document's notes, as TD's store holds them for `doc`. A
    /// store that cannot be read shows its reason and takes no notes: writing
    /// over it would lose whatever it holds.
    pub fn for_markdown(doc: &Path, read: Result<NoteMap, String>) -> NotesLayer {
        let label = doc
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (base, writable) = match read {
            Ok(notes) => (
                Shown::Notes {
                    notes,
                    concurs: ConcurMap::default(),
                },
                Ok(()),
            ),
            Err(why) => (
                Shown::Unreadable(why.clone()),
                Err(Refusal::Unreadable(why)),
            ),
        };
        let mut layer = NotesLayer {
            label,
            keeping: Keeping::Store(doc.to_path_buf()),
            base,
            concur: ConcurSupport::NotSupported,
            pending: Vec::new(),
            applied: None,
            writable,
            note_box: None,
            said: None,
            sent: None,
            saving: false,
            gone: false,
            close_warned: false,
            zones: Rc::new(RefCell::new(Vec::new())),
        };
        layer.refresh();
        layer
    }

    /// Whether the notes go into TD's store rather than the file.
    pub fn in_store(&self) -> bool {
        matches!(self.keeping, Keeping::Store(_))
    }

    /// The document whose notes TD's store keeps, when it does.
    pub fn store_doc(&self) -> Option<&Path> {
        match &self.keeping {
            Keeping::Store(doc) => Some(doc),
            Keeping::File => None,
        }
    }

    /// TD's store, read again with the document: what it holds is what the
    /// edits still waiting are shown on top of, as a brief's are rebased. A
    /// keep in flight is left to land, since it read the store after this did
    /// or will.
    pub fn rekept(&mut self, read: Result<NoteMap, String>) {
        if self.saving {
            return;
        }
        match read {
            Ok(notes) => {
                self.base = Shown::Notes {
                    notes,
                    concurs: ConcurMap::default(),
                };
                self.writable = Ok(());
            }
            Err(why) => {
                self.base = Shown::Unreadable(why.clone());
                self.writable = Err(Refusal::Unreadable(why));
            }
        }
        self.refresh();
    }

    /// Whether the bar is drawn. A brief's always is, as the browser draws
    /// its notebar. A Markdown document's only once there is something on
    /// it — a note, the last word of a ↪ or a keep, or why it takes none —
    /// so a plan with nothing written on it reads as the plan. Its blocks'
    /// 💬 under the pointer is the way in.
    pub fn shows_bar(&self, anchors: &[Anchor]) -> bool {
        !self.in_store()
            || self.counts(anchors).0 > 0
            || self.said.is_some()
            || self.sent.is_some()
            || self.read_only().is_some()
    }

    /// What the bytes show, and whether a save could be made into them.
    fn judge(read: NotesRead, tagged: u32, path: &Path) -> (Shown, Result<(), Refusal>) {
        let hidden = read.regions.island_after_script();
        let no_script = read.regions.notes_js.is_none() && tagged == 0;
        let torn = read.regions.notes.is_some_and(|i| i.close_end.is_none());
        let base = match read.notes {
            None => Shown::NoIsland,
            Some(_) if hidden => Shown::AfterScript,
            Some(_) if no_script => Shown::NoScript,
            Some(Err(e)) => Shown::Unreadable(e.0),
            // A concurs island a browser cannot read is an empty one to it.
            Some(Ok(notes)) => Shown::Notes {
                notes,
                concurs: read.concurs.unwrap_or_default(),
            },
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let writable = match &base {
            Shown::NoIsland => Err(Refusal::NoIsland),
            Shown::AfterScript => Err(Refusal::IslandAfterScript),
            Shown::NoScript => Err(Refusal::NoScript),
            Shown::Unreadable(why) => Err(Refusal::Unreadable(why.clone())),
            Shown::Notes { .. } => match read.format {
                Some(f) if f != FORMAT => Err(Refusal::UnknownFormat(f)),
                _ if name.starts_with('_') => Err(Refusal::BuildInput(name)),
                _ if torn => Err(Refusal::TornIsland),
                _ => Ok(()),
            },
        };
        (base, writable)
    }

    /// A new render of the same brief took this one's place: what was being
    /// written here comes along, the waiting edits, the open note box and
    /// its draft, and what the bar last said.
    pub fn carry_from(&mut self, old: NotesLayer) {
        self.pending = old.pending;
        self.said = old.said;
        self.sent = old.sent;
        self.saving = old.saving;
        self.note_box = old.note_box;
        self.refresh();
    }

    /// The file changed on disk and its layout did not: another writer
    /// changed only its notes. They become what the page is shown on top
    /// of; the edits waiting here stay, because they are deltas.
    pub fn rebase(&mut self, read: NotesRead, tagged: u32, path: &Path) {
        let (base, writable) = Self::judge(read, tagged, path);
        self.base = base;
        self.writable = writable;
        self.gone = false;
        self.refresh();
    }

    /// The file is gone from disk, or back.
    pub fn set_gone(&mut self, gone: bool) {
        self.gone = gone;
    }

    /// Recompute what the page shows: the file's notes with the waiting
    /// edits on top, leniently — an edit the next save would refuse still
    /// shows here, so the words are there to read and copy.
    fn refresh(&mut self) {
        self.close_warned = false;
        self.applied = match &self.base {
            Shown::Notes { notes, concurs } => {
                let (mut n, mut c) = (notes.clone(), concurs.clone());
                let supported = self.concur == ConcurSupport::Supported;
                for e in &self.pending {
                    let _ = apply(
                        &mut n,
                        &mut c,
                        std::slice::from_ref(e),
                        &[nid_of(e)],
                        supported,
                    );
                }
                Some((n, c))
            }
            _ => None,
        };
    }

    #[cfg(test)]
    pub fn shown(&self) -> &Shown {
        &self.base
    }

    #[cfg(test)]
    pub fn note_box(&self) -> Option<&NoteBox> {
        self.note_box.as_ref()
    }

    fn notes(&self) -> Option<&NoteMap> {
        self.applied.as_ref().map(|(n, _)| n)
    }

    /// The concurs a browser draws: none on a brief whose notes.js predates
    /// them, whatever an island holds.
    fn concurs(&self) -> Option<&ConcurMap> {
        match (&self.applied, self.concur) {
            (Some((_, c)), ConcurSupport::Supported) => Some(c),
            _ => None,
        }
    }

    /// The notes on one anchor, saved and waiting.
    pub fn count_on(&self, nid: &str) -> usize {
        self.notes().map_or(0, |n| n.on(nid).len())
    }

    /// Edits made here and not yet in the file.
    pub fn unsaved(&self) -> usize {
        self.pending.len()
    }

    /// Why this page takes no notes, in words, or `None` when it shows them.
    pub fn read_only(&self) -> Option<String> {
        match &self.base {
            Shown::NoIsland => Some("read-only · this page has no notes island".into()),
            Shown::AfterScript => Some(
                "read-only · its notes island sits after the notes script, where no browser reads it"
                    .into(),
            ),
            Shown::NoScript => Some("read-only · no notes script ran on this page".into()),
            Shown::Unreadable(why) => Some(format!("read-only · {why}")),
            Shown::Notes { .. } => None,
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    /// Why no note can be added or taken away here, in words, or `Ok`.
    pub fn can_edit(&self) -> Result<(), String> {
        if let Err(r) = &self.writable {
            return Err(r.sentence());
        }
        if self.gone {
            return Err("The file is no longer on disk, so there is nothing to save into.".into());
        }
        Ok(())
    }

    /// Why a save cannot be made now, in words, or `Ok` when it can.
    pub fn can_save(&self) -> Result<(), String> {
        if let Err(r) = &self.writable {
            return Err(r.sentence());
        }
        if self.gone {
            return Err("The file is no longer on disk, so there is nothing to save into.".into());
        }
        if self.saving {
            return Err("A save is already running.".into());
        }
        if self.pending.is_empty() {
            return Err("Nothing to save: every note here is already in the file.".into());
        }
        Ok(())
    }

    /// Whether a draft is open to type into: every key goes to it.
    pub fn has_caret(&self) -> bool {
        self.note_box.as_ref().is_some_and(|b| !b.gone) && self.can_delete()
    }

    /// Whether the open box's notes can be deleted: an edit can be made, on
    /// words still in the file or gone from it.
    fn can_delete(&self) -> bool {
        self.note_box.is_some() && self.writable.is_ok() && !self.gone
    }

    /// The bar's counts, as a browser's notebar counts them: notes on the
    /// page's anchors, and every concur, where the brief takes concurs. A
    /// Markdown document counts every note it keeps, those on words the file
    /// no longer has included, because its map carries them too.
    pub fn counts(&self, anchors: &[Anchor]) -> (usize, Option<usize>) {
        let notes = match (&self.keeping, self.notes()) {
            (Keeping::Store(_), Some(n)) => n.count(),
            _ => anchors.iter().map(|a| self.count_on(&a.nid)).sum(),
        };
        (notes, self.concurs().map(ConcurMap::count))
    }

    /// The map notes.js's copy map would give for this page, unsaved notes
    /// included as the brief's own copy map includes them. `None` when the
    /// page shows no notes; empty of blocks when it has none yet.
    pub fn map(&self, anchors: &[Anchor]) -> Option<String> {
        let notes = self.notes()?;
        if let Keeping::Store(doc) = &self.keeping {
            let lines: Vec<(&str, &str, Option<u32>)> = anchors
                .iter()
                .map(|a| (a.nid.as_str(), a.title.as_str(), a.line))
                .collect();
            return Some(super::md_notes::build_map(doc, notes, &lines));
        }
        let empty = ConcurMap::default();
        let pairs: Vec<(&str, &str)> = anchors
            .iter()
            .map(|a| (a.nid.as_str(), a.title.as_str()))
            .collect();
        Some(build_map(
            &self.label,
            notes,
            self.concurs().unwrap_or(&empty),
            &pairs,
        ))
    }

    /// Whether there is anything to copy: the brief disables its own copy
    /// map until a note or a concur exists.
    fn mappable(&self, anchors: &[Anchor]) -> bool {
        let (notes, concurs) = self.counts(anchors);
        self.notes().is_some() && notes + concurs.unwrap_or(0) > 0
    }

    /// The bar's ↪, when it is drawn: its words, and whether it can be
    /// pressed. `beside` names the agent pane the brief sits beside — "agent",
    /// or that pane's name when the tab holds more than one — and `None`
    /// means there is none, so there is no button: a brief with nobody beside
    /// it has nobody to send to. A page that shows no notes has no map to
    /// send, and draws none either. With edits waiting that a press would
    /// save, the words say it saves too.
    pub fn send_button(&self, anchors: &[Anchor], beside: Option<&str>) -> Option<(String, bool)> {
        let who = beside?;
        self.notes()?;
        let verb = if self.saves_on_send() {
            "save & send"
        } else {
            "send"
        };
        Some((format!("↪ {verb} to {who}"), self.mappable(anchors)))
    }

    /// A press on ↪ now would save: there are edits waiting, and a save
    /// could be made.
    fn saves_on_send(&self) -> bool {
        !self.pending.is_empty() && self.can_save().is_ok()
    }

    /// What ↪ does: the map exactly as copy map would give it, notes not yet
    /// saved included, and whether the press saves them. `None` with nothing
    /// to send.
    ///
    /// A send saves. Parker, 2026-09-25: *"The Send to Agent button should
    /// also save the doc."* Notes that reached the agent's prompt and not the
    /// file were gone the next time the brief opened, and the agent reading
    /// the file found none of them. Where no save can be made — a build
    /// input, a format newer than TD, a file gone from disk, a save already
    /// running — the map still goes as shown, and `unsaved` counts what the
    /// file will not hold, so the bar can say so.
    pub fn send(&self, anchors: &[Anchor]) -> Option<Sending> {
        if !self.mappable(anchors) {
            return None;
        }
        let saves = self.saves_on_send();
        Some(Sending {
            map: self.map(anchors)?,
            saves,
            unsaved: if saves { 0 } else { self.unsaved() },
        })
    }

    /// What came of a ↪, said on its own line of the bar.
    pub fn say_sent(&mut self, said: Said) {
        self.sent = Some(said);
    }

    /// Pure. Everything the layer draws for these anchors through this
    /// mapping, and which of them the pointer lights. Nothing for an anchor
    /// with no place (inside a closed dialog), nothing scrolled out of the
    /// view, and nothing at all on a page that shows no notes.
    pub fn marks(
        &self,
        anchors: &[Anchor],
        map: &PageToView,
        pointer: Option<Point<Pixels>>,
    ) -> Vec<Mark> {
        let mut out = Vec::new();
        if self.notes().is_none() {
            return out;
        }
        for a in anchors {
            let Some(rect) = a.rect else { continue };
            let count = self.count_on(&a.nid);
            if count > 0 {
                if let Some(at) = place(rect_css(rect.x, rect.y, RULE_CSS, rect.h), map) {
                    out.push(Mark::Rule { at });
                }
            }
            if let (Some(zone), Some(concurs)) = (a.concur_zone, self.concurs()) {
                if concurs.has(&a.nid) {
                    let pose = stamp_pose(&a.nid);
                    let o = STAMP_OVERHANG_CSS;
                    let r = rect_css(
                        zone.x - o + pose.dx,
                        zone.y - o + pose.dy,
                        zone.w + 2.0 * o,
                        zone.h + 2.0 * o,
                    );
                    if let Some(at) = place(r, map) {
                        out.push(Mark::Stamp {
                            nid: a.nid.clone(),
                            at,
                            degrees: pose.degrees,
                        });
                    }
                } else if let Some(at) = place(zone, map) {
                    out.push(Mark::ConcurSpace {
                        nid: a.nid.clone(),
                        at,
                    });
                }
            }
            let open = self.note_box.as_ref().is_some_and(|b| b.nid == a.nid);
            if count > 0 || open || hot(a, map, pointer) {
                if let Some(at) = button_rect(a).and_then(|r| place(r, map)) {
                    out.push(Mark::Button {
                        nid: a.nid.clone(),
                        title: a.title.clone(),
                        at,
                        count,
                        open,
                    });
                }
            }
        }
        out
    }

    /// The nids the pointer lights: a change here is a change in what is
    /// drawn, and nothing else about a move is.
    pub fn lit(
        anchors: &[Anchor],
        map: &PageToView,
        pointer: Option<Point<Pixels>>,
    ) -> Vec<String> {
        anchors
            .iter()
            .filter(|a| hot(a, map, pointer))
            .map(|a| a.nid.clone())
            .collect()
    }

    // ── notes, added and taken away ─────────────────────────────────────────

    /// Open the note box on an anchor, with an empty draft.
    pub fn open(&mut self, nid: String, title: String) {
        if self.note_box.as_ref().is_some_and(|b| b.nid == nid) {
            return;
        }
        self.note_box = Some(NoteBox {
            nid,
            title,
            draft: EditBuffer::default(),
            gone: false,
        });
    }

    /// Open the note box on notes whose words the file no longer has, to
    /// read them and delete them.
    pub fn open_gone(&mut self, nid: String, title: String) {
        self.note_box = Some(NoteBox {
            nid,
            title,
            draft: EditBuffer::default(),
            gone: true,
        });
    }

    /// Notes kept on words a Markdown file no longer has, by id, with the
    /// title each was written against: none on a brief, whose notes on a
    /// missing anchor are the page's business.
    pub fn orphans(&self, anchors: &[Anchor]) -> Vec<(String, String)> {
        let (Keeping::Store(_), Some(notes)) = (&self.keeping, self.notes()) else {
            return Vec::new();
        };
        notes
            .0
            .iter()
            .map(|(nid, _)| nid)
            .filter(|nid| !anchors.iter().any(|a| a.nid == *nid))
            .filter_map(|nid| {
                let list = notes.on(nid);
                let first = list.first()?;
                Some((nid.to_string(), first.title.unwrap_or(nid).to_string()))
            })
            .collect()
    }

    /// A close asked for with edits not saved — Escape, the ✕, the control
    /// socket, a link or a click that would put another document in its
    /// place: the first says so and keeps the document open, the second lets
    /// it go. A habit of closing is not a decision to throw notes away.
    /// Answers whether it kept the document open.
    pub fn guard_close(&mut self) -> bool {
        if self.pending.is_empty() || self.close_warned {
            return false;
        }
        self.close_warned = true;
        let n = self.pending.len();
        self.said = Some(Said::Refused(if self.in_store() {
            format!("{n} not kept · close again to leave without them")
        } else {
            format!("{n} unsaved · save them into the file, or close again to leave without them")
        }));
        true
    }

    /// Escape: the note box closes before anything else does. Answers
    /// whether it was open.
    pub fn escape(&mut self) -> bool {
        self.note_box.take().is_some()
    }

    /// A key, while the note box is open. Escape closes it; Ctrl+Enter adds
    /// the draft as a note, as the brief's own dialog does; Enter starts a
    /// new line; anything else edits the draft. Answers whether the layer
    /// took the key, which it always does while the box is open.
    pub fn key(&mut self, ks: &Keystroke, now: SystemTime) -> bool {
        if self.note_box.is_none() {
            return false;
        }
        let m = &ks.modifiers;
        if ks.key == "escape" {
            self.note_box = None;
            return true;
        }
        if !self.has_caret() {
            return true;
        }
        if ks.key == "enter" && (m.control || m.platform) {
            self.add(now);
            return true;
        }
        let Some(b) = self.note_box.as_mut() else {
            return true;
        };
        if ks.key == "enter" && !m.alt {
            b.draft.insert("\n");
        } else {
            b.draft
                .apply(&ks.key, m, ks.key_char.as_deref(), MAX_NOTE_CHARS);
        }
        true
    }

    /// Add the draft as a note on the open anchor, stamped with the time as
    /// notes.js stamps it. Nothing is added for a draft of only whitespace.
    pub fn add(&mut self, now: SystemTime) -> bool {
        let Some(b) = self.note_box.as_mut() else {
            return false;
        };
        let text = trimmed(&b.draft.text()).to_string();
        if text.is_empty() || b.gone || self.writable.is_err() {
            return false;
        }
        b.draft = EditBuffer::default();
        let (nid, title) = (b.nid.clone(), b.title.clone());
        self.add_note(nid, title, text, utc_minute(now));
        true
    }

    /// Add a note to an anchor, open or not: the control socket's way in.
    pub fn add_note(&mut self, nid: String, title: String, text: String, ts: String) {
        self.pending.push(NoteEdit::Add {
            nid,
            title,
            text,
            ts,
        });
        self.said = None;
        self.sent = None;
        self.refresh();
    }

    /// Delete the open anchor's `i`th note as the box lists it. One that was
    /// added here and never saved simply goes; one the file holds becomes a
    /// delete the next save makes.
    pub fn delete_shown(&mut self, i: usize) {
        let Some(nid) = self.note_box.as_ref().map(|b| b.nid.clone()) else {
            return;
        };
        let Some(note) = self.notes().and_then(|n| n.on(&nid).get(i).copied()) else {
            return;
        };
        let (text, ts) = (note.text.to_string(), note.ts.map(str::to_string));
        self.delete_note(nid, text, ts);
    }

    /// Delete a note by its words: the control socket's way in.
    pub fn delete_note(&mut self, nid: String, text: String, ts: Option<String>) {
        let unsaved = self.pending.iter().rposition(|e| {
            matches!(e, NoteEdit::Add { nid: n, text: t, ts: s, .. }
                if *n == nid && *t == text && ts.as_ref().is_none_or(|ts| ts == s))
        });
        match unsaved {
            Some(i) => {
                self.pending.remove(i);
            }
            None => self.pending.push(NoteEdit::Delete { nid, text, ts }),
        }
        self.said = None;
        self.sent = None;
        self.refresh();
    }

    /// Put a stamp down on a decision, or peel it off. Only where the brief
    /// takes concurs; undoing an unsaved one takes it back rather than
    /// saving a pair of edits that cancel.
    pub fn toggle_concur(&mut self, nid: &str, now: SystemTime) -> Result<(), String> {
        if let Err(r) = &self.writable {
            return Err(r.sentence());
        }
        if self.concur != ConcurSupport::Supported {
            return Err(Refusal::ConcursUnsupported(nid.into()).sentence());
        }
        let on = self.concurs().is_some_and(|c| c.has(nid));
        let undo = self.pending.iter().rposition(|e| match e {
            NoteEdit::Concur { nid: n, .. } => on && n == nid,
            NoteEdit::Unconcur { nid: n } => !on && n == nid,
            _ => false,
        });
        match (undo, on) {
            (Some(i), _) => {
                self.pending.remove(i);
            }
            (None, true) => self.pending.push(NoteEdit::Unconcur { nid: nid.into() }),
            (None, false) => self.pending.push(NoteEdit::Concur {
                nid: nid.into(),
                ts: utc_minute(now),
            }),
        }
        self.said = None;
        self.sent = None;
        self.refresh();
        Ok(())
    }

    // ── saving ──────────────────────────────────────────────────────────────

    /// The edits a save starts with: everything waiting now. What is added
    /// while it runs waits for the next one.
    pub fn begin_save(&mut self) -> Vec<NoteEdit> {
        self.saving = true;
        // A note kept as it is written says nothing while it is kept.
        if !self.in_store() {
            self.said = Some(Said::Working(format!("saving into {}…", self.label)));
        }
        self.pending.clone()
    }

    /// A Markdown document's notes were kept: the store now holds `notes`,
    /// and the first `made` edits are in it. Nothing is said: keeping is
    /// what writing a note does, and only a failure is news.
    pub fn kept(&mut self, made: usize, notes: NoteMap) {
        self.saving = false;
        self.pending.drain(..made.min(self.pending.len()));
        self.base = Shown::Notes {
            notes,
            concurs: ConcurMap::default(),
        };
        self.refresh();
    }

    /// Whether edits are waiting to be kept and none is being kept now: a
    /// Markdown document keeps them at once.
    pub fn wants_keeping(&self) -> bool {
        self.in_store() && !self.pending.is_empty() && !self.saving && self.can_edit().is_ok()
    }

    /// The save landed. The file now holds `notes` and `concurs`, the first
    /// `made` edits are in it, and the page is being read back.
    pub fn saved(&mut self, made: usize, notes: NoteMap, concurs: Option<ConcurMap>, file: &str) {
        self.saving = false;
        self.pending.drain(..made.min(self.pending.len()));
        let concurs = match (concurs, &self.base) {
            (Some(c), _) => c,
            (None, Shown::Notes { concurs, .. }) => concurs.clone(),
            (None, _) => ConcurMap::default(),
        };
        self.base = Shown::Notes { notes, concurs };
        self.said = Some(Said::Working(format!(
            "saved into {file} · reading it back…"
        )));
        self.refresh();
    }

    /// The written file, reopened fresh, shows what was written.
    pub fn confirmed(&mut self, file: &str) {
        self.said = Some(Said::Done(format!("saved into {file} ✓")));
    }

    /// The written file, reopened fresh, does not show what was written; it
    /// stays written, and the bar says where the copy from before is.
    pub fn unconfirmed(&mut self, why: String) {
        self.said = Some(Said::Refused(why));
    }

    /// The save did not happen: nothing was written, the edits wait.
    pub fn refused(&mut self, why: String) {
        self.saving = false;
        self.said = Some(Said::Refused(why));
    }

    pub fn say(&mut self, said: Said) {
        self.said = Some(said);
    }

    /// A press, `at` in window pixels as the last paint laid things out.
    /// The note box first, which takes every press while it is open — one
    /// outside it puts it away, as a click on a browser dialog's backdrop
    /// does — then the bar. `beside` is whether an agent pane is beside the
    /// brief now: ↪ is only ever a press while it is drawn.
    pub fn press(
        &mut self,
        at: Point<Pixels>,
        anchors: &[Anchor],
        now: SystemTime,
        beside: bool,
        cx: &mut App,
    ) -> LayerPress {
        let zones = self.zones.borrow().clone();
        let under = |z: Zone| zones.iter().any(|(b, k)| *k == z && b.contains(&at));
        let hit = zones
            .iter()
            .rev()
            .find(|(b, z)| b.contains(&at) && !matches!(z, Zone::Box | Zone::Bar))
            .map(|(_, z)| *z);
        if self.note_box.is_some() {
            match hit {
                Some(Zone::CloseBox) => self.note_box = None,
                Some(Zone::AddNote) => {
                    self.add(now);
                }
                Some(Zone::Delete(i)) if self.can_delete() => self.delete_shown(i),
                _ if !under(Zone::Box) => self.note_box = None,
                _ => {}
            }
            return LayerPress::Took;
        }
        match hit {
            Some(Zone::CopyMap) => {
                self.copy_map(anchors, cx);
                LayerPress::Took
            }
            Some(Zone::Send) if beside => match self.send(anchors) {
                Some(sending) => LayerPress::Send(sending),
                None => LayerPress::Took,
            },
            Some(Zone::Save) => LayerPress::Save,
            Some(Zone::Gone) => {
                if let Some((nid, title)) = self.orphans(anchors).into_iter().next() {
                    self.open_gone(nid, title);
                }
                LayerPress::Took
            }
            _ if under(Zone::Bar) => LayerPress::Took,
            _ => LayerPress::Pass,
        }
    }

    /// Put the map on the clipboard, exactly as notes.js builds it.
    pub fn copy_map(&mut self, anchors: &[Anchor], cx: &mut App) {
        if !self.mappable(anchors) {
            return;
        }
        let Some(map) = self.map(anchors) else { return };
        cx.write_to_clipboard(ClipboardItem::new_string(map.clone()));
        cx.write_to_primary(ClipboardItem::new_string(map.clone()));
        self.said = Some(Said::Done(format!(
            "map copied · {} characters ≈ {} tokens",
            map.chars().count(),
            map.len().div_ceil(4)
        )));
    }

    /// What the bar says, for the control socket and the tests.
    pub fn report(&self, anchors: &[Anchor]) -> serde_json::Value {
        let (notes, concurs) = self.counts(anchors);
        let state = match &self.base {
            Shown::NoIsland => "no-island",
            Shown::AfterScript => "after-script",
            Shown::NoScript => "no-script",
            Shown::Unreadable(_) => "unreadable",
            Shown::Notes { .. } => "notes",
        };
        serde_json::json!({
            "state": state,
            "label": self.label,
            "notes": notes,
            "concurs": concurs,
            "anchors": anchors.len(),
            "read_only": self.read_only(),
            "writable": self.writable.as_ref().err().map(Refusal::sentence),
            "refusal": self.writable.as_ref().err().map(Refusal::kind),
            "unsaved": self.unsaved(),
            "saving": self.saving,
            "gone": self.gone,
            "said": self.said.as_ref().map(|s| s.text().to_string()),
            "sent": self.sent.as_ref().map(|s| s.text().to_string()),
            "map": self.map(anchors),
            "open": self.note_box.as_ref().map(|b| b.nid.clone()),
            "kept": match &self.keeping {
                Keeping::File => "file",
                Keeping::Store(_) => "store",
            },
        })
    }

    // ── drawing ─────────────────────────────────────────────────────────────

    /// Forget where things were drawn: called at the top of every render,
    /// before the canvases below record the new places.
    pub fn clear_zones(&self) {
        self.zones.borrow_mut().clear();
    }

    /// A canvas that records where its parent was painted as `zone`.
    fn record(&self, zone: Zone) -> impl IntoElement {
        let zones = self.zones.clone();
        canvas(
            move |bounds, _, _| zones.borrow_mut().push((bounds, zone)),
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
    }

    /// The marks, drawn: under the bar and the note box.
    pub fn draw_marks(&self, marks: &[Mark], th: &Theme) -> Vec<AnyElement> {
        let accent = th.accent;
        marks
            .iter()
            .map(|m| match m {
                Mark::Rule { at } => at_rect(div(), *at).bg(accent).into_any_element(),
                Mark::Button {
                    at, count, open, ..
                } => {
                    let side = f32::from(at.size.height);
                    let (bg, fg, label): (Hsla, Hsla, SharedString) = if *count > 0 {
                        (accent, th.bg, count.to_string().into())
                    } else {
                        (th.surface, th.text.alpha(0.8), "💬".into())
                    };
                    at_rect(div(), *at)
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(side * 0.23))
                        .bg(bg)
                        .border_1()
                        .border_color(if *open { th.text } else { accent })
                        .text_color(fg)
                        .text_size(px((side * 0.5).max(8.0)))
                        .font_weight(FontWeight::BOLD)
                        .child(label)
                        .into_any_element()
                }
                Mark::ConcurSpace { at, .. } => {
                    let side = f32::from(at.size.width);
                    let ink = th.text.alpha(0.38);
                    at_rect(div(), *at)
                        .rounded(px(side * 0.117))
                        .border_1()
                        .border_dashed()
                        .border_color(ink)
                        .text_color(ink)
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .child(div().text_size(px(side * 0.38)).child("?"))
                        .child(div().text_size(px((side * 0.085).max(7.0))).child("CONCUR"))
                        .into_any_element()
                }
                Mark::Stamp { at, degrees, .. } => svg()
                    .external_path(stamp_path())
                    .absolute()
                    .left(at.origin.x)
                    .top(at.origin.y)
                    .w(at.size.width)
                    .h(at.size.height)
                    .text_color(rgb(STAMP_INK))
                    .with_transformation(Transformation::rotate(radians(degrees.to_radians())))
                    .into_any_element(),
            })
            .collect()
    }

    /// A bar button: a word in a thin border, lit when it can be pressed.
    fn button(&self, label: &str, zone: Zone, live: bool, th: &Theme) -> gpui::Div {
        div()
            .relative()
            .px(px(7.))
            .py(px(1.))
            .rounded(px(4.))
            .border_1()
            .border_color(th.accent.alpha(if live { 1.0 } else { 0.3 }))
            .text_color(th.accent.alpha(if live { 1.0 } else { 0.4 }))
            .child(label.to_string())
            .child(self.record(zone))
    }

    fn said_colour(said: &Said, th: &Theme) -> Hsla {
        match said {
            Said::Refused(_) => th.ansi[9],
            Said::Working(_) => th.text.alpha(0.7),
            Said::Done(_) => th.accent,
        }
    }

    /// The bar, bottom-right and fixed in the view. `beside` names the agent
    /// pane ↪ would send to; see [`Self::send_button`].
    pub fn draw_bar(&self, anchors: &[Anchor], th: &Theme, beside: Option<&str>) -> AnyElement {
        let text = th.font_size * 0.85;
        let (notes, concurs) = self.counts(anchors);
        let mut row = div()
            .relative()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .px(px(10.))
            .py(px(5.))
            .rounded(px(6.))
            .bg(th.surface.alpha(0.96))
            .border_1()
            .border_color(th.accent.alpha(0.45))
            .shadow(crate::float_shadows(th.accent))
            .font_family(SharedString::from(th.font_family.clone()))
            .text_size(px(text))
            .text_color(th.text.alpha(0.8))
            .child(self.record(Zone::Bar));
        match self.read_only() {
            Some(why) => row = row.child(div().child(why)),
            None => {
                row = row.child(
                    div()
                        .flex()
                        .gap(px(4.))
                        .child(div().text_color(th.accent).child(notes.to_string()))
                        .child(if notes == 1 { "note" } else { "notes" }),
                );
                if let Some(c) = concurs {
                    row = row.child(format!(
                        "· {c} {}",
                        if c == 1 { "concur" } else { "concurs" }
                    ));
                }
                row =
                    row.child(self.button("⎘ copy map", Zone::CopyMap, self.mappable(anchors), th));
                if let Some((label, live)) = self.send_button(anchors, beside) {
                    row = row.child(self.button(&label, Zone::Send, live, th));
                }
                let gone: usize = self
                    .orphans(anchors)
                    .iter()
                    .map(|(nid, _)| self.count_on(nid))
                    .sum();
                if gone > 0 {
                    let label = format!("{gone} on words no longer here");
                    row = row.child(self.button(&label, Zone::Gone, true, th));
                }
                match &self.writable {
                    // Kept as written: nothing to save, and nothing to say
                    // unless keeping failed, which `said` carries.
                    Ok(()) if self.in_store() => {}
                    Ok(()) if !self.gone => {
                        row = row.child(self.button(
                            "💾 save into file",
                            Zone::Save,
                            self.can_save().is_ok(),
                            th,
                        ));
                        if !self.pending.is_empty() {
                            row = row.child(
                                div()
                                    .text_color(th.accent)
                                    .child(format!("{} unsaved", self.pending.len())),
                            );
                        }
                    }
                    Ok(()) => row = row.child("the file is gone · saving is off"),
                    Err(r) => row = row.child(format!("read-only · {}", r.sentence())),
                }
            }
        }
        // Where the notes went, then the save that went with them.
        for said in [&self.sent, &self.said].into_iter().flatten() {
            row = row.child(
                div()
                    .text_color(Self::said_colour(said, th))
                    .child(said.text().to_string()),
            );
        }
        div()
            .absolute()
            .right(px(10.))
            .bottom(px(10.))
            .max_w(px(900.))
            .child(row)
            .into_any_element()
    }

    /// The draft as lines with a caret, split where the note has a newline:
    /// the single-line box TD draws elsewhere cannot hold a note.
    fn draw_draft(draft: &EditBuffer, th: &Theme) -> gpui::Div {
        let text = draft.text();
        let caret_at = draft.caret();
        let caret = || div().w(px(2.)).h(px(th.font_size * 1.1)).bg(th.accent);
        let mut col = div().flex().flex_col().min_h(px(th.font_size * 4.5));
        let mut start = 0usize;
        for line in text.split('\n') {
            let len = line.chars().count();
            let mut row = div()
                .flex()
                .flex_row()
                .flex_wrap()
                .min_h(px(th.font_size * 1.3));
            if (start..=start + len).contains(&caret_at) {
                let split = caret_at - start;
                let before: String = line.chars().take(split).collect();
                let after: String = line.chars().skip(split).collect();
                row = row.child(before).child(caret()).child(after);
            } else {
                row = row.child(line.to_string());
            }
            col = col.child(row);
            start += len + 1;
        }
        col
    }

    /// The note box, over everything, when it is open: the anchor's title,
    /// its id, its notes oldest first (each with its delete), the draft, and
    /// Add note, Close and the hint, as the brief's own dialog lays them out.
    pub fn draw_box(&self, view: gpui::Size<Pixels>, th: &Theme) -> Option<AnyElement> {
        let b = self.note_box.as_ref()?;
        let editable = self.has_caret();
        let deletable = self.can_delete();
        let notes = self.notes().map(|n| n.on(&b.nid)).unwrap_or_default();
        let (vw, vh) = (f32::from(view.width), f32::from(view.height));
        let w = (vw * 0.76).clamp(220.0_f32.min(vw), 620.0);
        let body = th.font_size;
        let mut list = div().flex().flex_col().gap(px(8.));
        if notes.is_empty() {
            list = list.child(
                div()
                    .text_color(th.text.alpha(0.6))
                    .child("No notes on this yet."),
            );
        }
        for (i, n) in notes.iter().enumerate() {
            let mut head = div()
                .flex()
                .flex_row()
                .justify_between()
                .text_size(px(body * 0.72))
                .text_color(th.faint)
                .child(n.ts.unwrap_or("").to_string());
            if deletable {
                head = head.child(
                    div()
                        .relative()
                        .text_color(th.text.alpha(0.6))
                        .child("delete")
                        .child(self.record(Zone::Delete(i))),
                );
            }
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .pl(px(9.))
                    .pr(px(9.))
                    .py(px(6.))
                    .border_l_2()
                    .border_color(th.accent)
                    .bg(th.bg.alpha(0.5))
                    .child(head)
                    .child(div().child(n.text.to_string())),
            );
        }
        let pill = |label: &str, zone: Zone, primary: bool| {
            div()
                .relative()
                .px(px(9.))
                .py(px(2.))
                .rounded(px(4.))
                .border_1()
                .border_color(th.accent)
                .when(primary, |d| d.bg(th.accent).text_color(th.bg))
                .when(!primary, |d| d.text_color(th.accent))
                .child(label.to_string())
                .child(self.record(zone))
        };
        let mut actions = div().flex().flex_row().items_center().gap(px(8.));
        if editable {
            actions = actions.child(pill("Add note", Zone::AddNote, true));
        }
        actions = actions.child(pill("Close", Zone::CloseBox, false));
        if editable {
            actions = actions.child(
                div()
                    .ml_auto()
                    .text_size(px(body * 0.72))
                    .text_color(th.faint)
                    .child("ctrl+enter to add"),
            );
        }
        let mut content = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .px(px(12.))
            .py(px(10.))
            .child(list);
        if editable {
            content = content.child(
                div()
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(5.))
                    .border_1()
                    .border_color(th.accent)
                    .bg(th.bg.alpha(0.6))
                    .child(Self::draw_draft(&b.draft, th)),
            );
        } else if let Some(why) = self.writable.as_ref().err() {
            content = content.child(
                div()
                    .text_size(px(body * 0.8))
                    .text_color(th.text.alpha(0.6))
                    .child(why.sentence()),
            );
        } else if b.gone {
            content = content.child(
                div()
                    .text_size(px(body * 0.8))
                    .text_color(th.text.alpha(0.6))
                    .child("These were written on words no longer in the file."),
            );
        }
        content = content.child(actions);
        let panel = div()
            .absolute()
            .left(px(((vw - w) / 2.0).max(0.0)))
            .top(px((vh * 0.1).min(110.0)))
            .w(px(w))
            .max_h(px(vh * 0.8))
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(th.surface)
            .border_2()
            .border_color(th.accent)
            .rounded(px(10.))
            .shadow(crate::float_shadows(th.accent))
            .text_size(px(body))
            .text_color(th.text)
            .child(self.record(Zone::Box))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .px(px(12.))
                    .py(px(8.))
                    .border_b_1()
                    .border_color(th.faint)
                    .child(div().font_weight(FontWeight::BOLD).child(b.title.clone()))
                    .child(
                        div()
                            .text_size(px(body * 0.75))
                            .text_color(th.faint)
                            .font_family(SharedString::from(th.font_family.clone()))
                            .child(format!("#{}", b.nid)),
                    ),
            )
            .child(content);
        Some(
            div()
                .absolute()
                .inset_0()
                .bg(hsla(0., 0., 0., 0.45))
                .child(panel)
                .into_any_element(),
        )
    }
}

/// The anchor an edit is about.
fn nid_of(e: &NoteEdit) -> &str {
    match e {
        NoteEdit::Add { nid, .. }
        | NoteEdit::Delete { nid, .. }
        | NoteEdit::Concur { nid, .. }
        | NoteEdit::Unconcur { nid } => nid,
    }
}

/// Pure. What a press on the marks lands on: a button first, which the
/// brief draws above everything in its anchor, then a concur space or stamp.
pub fn hit(marks: &[Mark], at: Point<Pixels>) -> Option<MarkHit> {
    let button = marks.iter().rev().find_map(|m| match m {
        Mark::Button {
            nid, title, at: b, ..
        } if b.contains(&at) => Some(MarkHit::Open {
            nid: nid.clone(),
            title: title.clone(),
        }),
        _ => None,
    });
    button.or_else(|| {
        marks.iter().rev().find_map(|m| match m {
            Mark::ConcurSpace { nid, at: b } | Mark::Stamp { nid, at: b, .. }
                if b.contains(&at) =>
            {
                Some(MarkHit::Concur(nid.clone()))
            }
            _ => None,
        })
    })
}

fn at_rect(d: gpui::Div, at: Bounds<Pixels>) -> gpui::Div {
    d.absolute()
        .left(at.origin.x)
        .top(at.origin.y)
        .w(at.size.width)
        .h(at.size.height)
}

/// The stamp's drawing, written once to the runtime directory: gpui draws an
/// SVG from a path.
fn stamp_path() -> SharedString {
    static PATH: OnceLock<SharedString> = OnceLock::new();
    PATH.get_or_init(|| {
        crate::art::runtime_asset("terminal-delight-concur-stamp.svg", STAMP_SVG)
            .to_string_lossy()
            .into_owned()
            .into()
    })
    .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docview::notes;
    use gpui::{point, size, Modifiers};

    fn anchor(nid: &str, rect: Option<RectCss>, concur: bool) -> Anchor {
        Anchor {
            nid: nid.into(),
            title: format!("{nid} title"),
            tag: "div".into(),
            dialog: None,
            rect,
            button: rect.map(|r| rect_css(r.x + r.w - 34.0, r.y + 8.0, 26.0, 26.0)),
            concur_zone: (concur && rect.is_some()).then(|| {
                let r = rect.unwrap();
                rect_css(r.x + r.w - 134.0, r.y + 42.0, 120.0, 120.0)
            }),
            has_note: None,
            has_concur: None,
            line: None,
        }
    }

    fn view() -> PageToView {
        PageToView {
            origin: point(px(0.), px(0.)),
            px_per_css: 1.0,
            scroll: px(0.),
            clip: Bounds {
                origin: point(px(0.), px(0.)),
                size: size(px(800.), px(2000.)),
            },
        }
    }

    fn layer(island: &str, concurs: &str, support: ConcurSupport) -> NotesLayer {
        let html = format!(
            "<script type=\"application/json\" id=\"report-notes\">{island}</script>\
             <script type=\"application/json\" id=\"report-concurs\">{concurs}</script>\
             <script>function tag() {{}} // reader notes</script>"
        );
        NotesLayer::new(
            notes::read(html.as_bytes()),
            Some("b.html".into()),
            support,
            3,
            Path::new("/r/b.html"),
        )
    }

    const ONE_NOTE: &str = r#"{"a":[{"text":"hello","title":"a title","ts":"2026-09-24 10:00"}]}"#;

    #[test]
    fn concur_is_offered_only_where_the_brief_offers_it() {
        let r = |y| Some(rect_css(0.0, y, 700.0, 200.0));
        let anchors = vec![anchor("a", r(0.0), false), anchor("ask-b", r(300.0), true)];
        let l = layer(ONE_NOTE, r#"{}"#, ConcurSupport::Supported);
        let marks = l.marks(&anchors, &view(), None);
        let spaces: Vec<&str> = marks
            .iter()
            .filter_map(|m| match m {
                Mark::ConcurSpace { nid, .. } => Some(nid.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            spaces,
            ["ask-b"],
            "only the decision the page made a space on"
        );
        let inside = point(px(700. - 134. + 60.), px(300. + 42. + 60.));
        assert_eq!(hit(&marks, inside), Some(MarkHit::Concur("ask-b".into())));
        assert_eq!(hit(&marks, point(px(600.), px(100.))), None);
        // A brief whose notes.js predates concurs draws no space and no
        // stamp, even with a concurs island holding one, and takes none.
        let mut old = layer(
            ONE_NOTE,
            r#"{"ask-b":"2026-09-24 10:00"}"#,
            ConcurSupport::NotSupported,
        );
        let marks = old.marks(&anchors, &view(), None);
        assert!(!marks
            .iter()
            .any(|m| matches!(m, Mark::ConcurSpace { .. } | Mark::Stamp { .. })));
        assert_eq!(old.counts(&anchors), (1, None), "no concur count at all");
        assert!(old.toggle_concur("ask-b", SystemTime::now()).is_err());
        assert_eq!(old.unsaved(), 0);
        // Where it offers them, a concurred decision shows its stamp at the
        // browser's angle instead of the empty space.
        let stamped = layer(
            ONE_NOTE,
            r#"{"ask-b":"2026-09-24 10:00"}"#,
            ConcurSupport::Supported,
        );
        let marks = stamped.marks(&anchors, &view(), None);
        let pose = notes::stamp_pose("ask-b");
        assert!(marks.iter().any(|m| matches!(m,
            Mark::Stamp { nid, degrees, at } if nid == "ask-b" && *degrees == pose.degrees
                && at.size == size(px(144.), px(144.)))));
        assert_eq!(stamped.counts(&anchors), (1, Some(1)));
    }

    #[test]
    fn an_anchor_in_a_closed_dialog_has_no_button() {
        let anchors = vec![anchor("a", None, true)];
        let l = layer(ONE_NOTE, "{}", ConcurSupport::Supported);
        let marks = l.marks(&anchors, &view(), Some(point(px(0.), px(0.))));
        assert!(marks.is_empty(), "{marks:?}");
        // Its notes still count, as they do in the browser's notebar.
        assert_eq!(l.counts(&anchors), (1, Some(0)));
    }

    /// A button shows always once its anchor has a note, and otherwise only
    /// while the pointer is over the anchor, as notes.css shows it.
    #[test]
    fn a_note_button_shows_its_count_and_otherwise_only_under_the_pointer() {
        let r = |y| Some(rect_css(10.0, y, 600.0, 100.0));
        let anchors = vec![anchor("a", r(0.0), false), anchor("b", r(200.0), false)];
        let l = layer(ONE_NOTE, "{}", ConcurSupport::NotSupported);
        let buttons = |p: Option<Point<Pixels>>| {
            l.marks(&anchors, &view(), p)
                .into_iter()
                .filter_map(|m| match m {
                    Mark::Button { nid, count, .. } => Some((nid, count)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(buttons(None), [("a".to_string(), 1)]);
        assert_eq!(
            buttons(Some(point(px(50.), px(250.)))),
            [("a".to_string(), 1), ("b".to_string(), 0)]
        );
        let marks = l.marks(&anchors, &view(), None);
        assert!(
            marks.iter().any(|m| matches!(m, Mark::Rule { .. })),
            "a has a note"
        );
        let on_a = point(px(10. + 600. - 34. + 5.), px(8. + 5.));
        assert_eq!(
            hit(&marks, on_a),
            Some(MarkHit::Open {
                nid: "a".into(),
                title: "a title".into()
            })
        );
    }

    fn ks(key: &str, ctrl: bool, ch: Option<&str>) -> Keystroke {
        Keystroke {
            modifiers: Modifiers {
                control: ctrl,
                ..Default::default()
            },
            key: key.into(),
            key_char: ch.map(str::to_string),
        }
    }

    #[test]
    fn escape_closes_the_note_box_before_anything_else() {
        let mut l = layer(ONE_NOTE, "{}", ConcurSupport::Supported);
        assert!(!l.escape(), "nothing open: Escape is not the layer's");
        assert!(!l.key(&ks("escape", false, None), SystemTime::now()));
        l.open("a".into(), "a title".into());
        assert_eq!(l.note_box().map(|b| b.nid.as_str()), Some("a"));
        assert!(
            l.key(&ks("escape", false, None), SystemTime::now()),
            "the note box takes Escape"
        );
        assert!(l.note_box().is_none());
        assert!(!l.escape());
    }

    /// Enter is a new line in the note; Ctrl+Enter adds it, as the brief's
    /// own dialog does, stamped with the time as notes.js stamps it.
    #[test]
    fn ctrl_enter_adds_and_enter_starts_a_new_line() {
        let mut l = layer("{}", "{}", ConcurSupport::Supported);
        l.open("a".into(), "a title".into());
        assert!(l.has_caret());
        let now = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_790_278_680);
        for c in ["o", "n", "e"] {
            assert!(l.key(&ks(c, false, Some(c)), now));
        }
        assert!(l.key(&ks("enter", false, None), now));
        assert!(l.key(&ks("t", false, Some("t")), now));
        assert_eq!(l.note_box().unwrap().draft.text(), "one\nt");
        assert_eq!(l.unsaved(), 0, "Enter added nothing");
        assert!(l.key(&ks("enter", true, None), now));
        assert_eq!(
            l.note_box().unwrap().draft.text(),
            "",
            "the draft is emptied"
        );
        assert_eq!(
            l.pending,
            [NoteEdit::Add {
                nid: "a".into(),
                title: "a title".into(),
                text: "one\nt".into(),
                ts: "2026-09-24 19:38".into()
            }]
        );
        assert_eq!(l.count_on("a"), 1, "shown before it is saved");
        // Whitespace alone is not a note.
        l.key(&ks("space", false, Some(" ")), now);
        l.key(&ks("enter", true, None), now);
        assert_eq!(l.unsaved(), 1);
    }

    /// Unsaved edits are deltas: deleting a note added here takes the add
    /// back, deleting one the file holds is a delete to save, and a stamp put
    /// down and peeled off again leaves nothing to save.
    #[test]
    fn unsaved_edits_are_deltas_that_cancel_where_they_should() {
        let mut l = layer(ONE_NOTE, "{}", ConcurSupport::Supported);
        let now = SystemTime::now();
        l.open("a".into(), "a title".into());
        l.add_note(
            "a".into(),
            "a title".into(),
            "mine".into(),
            "2026-09-24 11:00".into(),
        );
        assert_eq!(l.count_on("a"), 2);
        l.delete_shown(1);
        assert_eq!(l.unsaved(), 0, "an unsaved note deleted leaves nothing");
        l.delete_shown(0);
        assert_eq!(l.count_on("a"), 0);
        assert!(matches!(&l.pending[..], [NoteEdit::Delete { text, .. }] if text == "hello"));
        l.toggle_concur("ask-b", now).unwrap();
        assert_eq!(l.unsaved(), 2);
        l.toggle_concur("ask-b", now).unwrap();
        assert_eq!(l.unsaved(), 1, "put down and peeled off: nothing to save");
        assert!(l.can_save().is_ok());
    }

    /// Escape out of a square holding unsaved notes is kept once, with the
    /// bar saying what the next one throws away; with nothing unsaved, or on
    /// the second press, it is not the layer's.
    #[test]
    fn escape_with_unsaved_notes_keeps_the_square_open_once() {
        let mut l = layer("{}", "{}", ConcurSupport::Supported);
        assert!(!l.guard_close(), "nothing unsaved: Escape closes");
        l.add_note(
            "a".into(),
            "t".into(),
            "kept".into(),
            "2026-09-24 11:00".into(),
        );
        assert!(l.guard_close(), "the first Escape is kept");
        assert!(l.report(&[])["said"]
            .as_str()
            .unwrap()
            .contains("1 unsaved"));
        assert!(!l.guard_close(), "the second closes");
        l.add_note(
            "a".into(),
            "t".into(),
            "more".into(),
            "2026-09-24 11:01".into(),
        );
        assert!(l.guard_close(), "a new edit asks again");
    }

    /// A save takes what is waiting; what is added while it runs waits for
    /// the next; a refusal keeps every edit.
    #[test]
    fn a_save_keeps_what_was_added_while_it_ran() {
        let mut l = layer("{}", "{}", ConcurSupport::Supported);
        l.add_note(
            "a".into(),
            "t".into(),
            "first".into(),
            "2026-09-24 11:00".into(),
        );
        let edits = l.begin_save();
        assert_eq!(edits.len(), 1);
        assert!(l.can_save().is_err(), "one save at a time");
        l.add_note(
            "a".into(),
            "t".into(),
            "second".into(),
            "2026-09-24 11:01".into(),
        );
        l.refused("the disk said no".into());
        assert_eq!(l.unsaved(), 2, "a refusal keeps every edit");
        let edits = l.begin_save();
        let mut written = NoteMap::default();
        let mut c = ConcurMap::default();
        apply(&mut written, &mut c, &edits[..1], &["a"], true).unwrap();
        l.saved(1, written, Some(c), "b.html");
        assert_eq!(l.unsaved(), 1, "the second waits for the next save");
        assert_eq!(l.count_on("a"), 2, "one in the file, one on top of it");
        l.confirmed("b.html");
        assert_eq!(l.report(&[])["said"], "saved into b.html ✓");
    }

    /// No island is read-only and says so; it never reads as "0 notes".
    #[test]
    fn a_page_without_an_island_says_it_is_read_only() {
        let none = NotesLayer::new(
            notes::read(b"<html><body><p>just a page</p></body></html>"),
            None,
            ConcurSupport::Unknown,
            0,
            Path::new("/r/page.html"),
        );
        assert_eq!(none.shown(), &Shown::NoIsland);
        let why = none.read_only().expect("read-only");
        assert!(why.contains("no notes island"), "{why}");
        assert_eq!(none.map(&[]), None, "nothing to map, not an empty map");
        assert!(none
            .marks(
                &[anchor("a", Some(rect_css(0., 0., 9., 9.)), true)],
                &view(),
                None
            )
            .is_empty());
        assert_eq!(none.report(&[])["state"], "no-island");
        assert_eq!(none.report(&[])["label"], "page.html");
        assert!(none.can_save().unwrap_err().contains("no notes island"));
        // An island whose script never ran is read-only too, and so is one
        // the browser cannot read.
        let unread = layer("{\"a\":", "{}", ConcurSupport::Supported);
        assert!(matches!(unread.shown(), Shown::Unreadable(_)));
        assert!(unread.read_only().is_some());
        let script_less = NotesLayer::new(
            notes::read(b"<script id=\"report-notes\">{}</script>"),
            None,
            ConcurSupport::Unknown,
            0,
            Path::new("/r/p.html"),
        );
        assert_eq!(script_less.shown(), &Shown::NoScript);
    }

    /// ↪ is drawn only when an agent pane is beside the brief, names it when
    /// the pane says there is a choice, and never on a page with no notes to
    /// send. It can be pressed once there is something in the map.
    ///
    /// Mutation-tested: drawing the button whenever the page shows notes
    /// fails this.
    #[test]
    fn the_send_button_is_absent_when_no_agent_pane_is_beside_the_brief() {
        let anchors = vec![anchor("a", Some(rect_css(0.0, 0.0, 700.0, 200.0)), false)];
        let l = layer(ONE_NOTE, "{}", ConcurSupport::Supported);
        assert_eq!(
            l.send_button(&anchors, None),
            None,
            "nobody beside: no button"
        );
        assert_eq!(
            l.send_button(&anchors, Some("agent")),
            Some(("↪ send to agent".to_string(), true))
        );
        assert_eq!(
            l.send_button(&anchors, Some("CLAUDE")).map(|b| b.0),
            Some("↪ send to CLAUDE".to_string())
        );
        let empty = layer("{}", "{}", ConcurSupport::Supported);
        assert_eq!(
            empty.send_button(&anchors, Some("agent")),
            Some(("↪ send to agent".to_string(), false)),
            "drawn, and dim until there is a note"
        );
        assert_eq!(empty.send(&anchors), None);
        let read_only = NotesLayer::new(
            notes::read(b"<html><body><p>just a page</p></body></html>"),
            None,
            ConcurSupport::Unknown,
            0,
            Path::new("/r/page.html"),
        );
        assert_eq!(read_only.send_button(&anchors, Some("agent")), None);
    }

    /// ↪ sends what the bar shows, unsaved notes included, and saves them
    /// as it goes; the button says so while there is something to save.
    ///
    /// Mutation-tested: `saves_on_send` answering false, and `send` counting
    /// the pending edits as unsaved when it saves, each fail this.
    #[test]
    fn send_carries_the_map_as_shown_and_saves_it() {
        let anchors = vec![anchor("a", Some(rect_css(0.0, 0.0, 700.0, 200.0)), false)];
        let mut l = layer(ONE_NOTE, "{}", ConcurSupport::Supported);
        let clean = l.send(&anchors).expect("a saved note is a map");
        assert!(!clean.saves, "nothing waiting: a send is only a send");
        l.add_note(
            "a".into(),
            "a title".into(),
            "not saved yet".into(),
            "2026-09-25 09:00".into(),
        );
        let s = l.send(&anchors).expect("a map to send");
        assert!(
            s.map.contains("hello") && s.map.contains("not saved yet"),
            "{}",
            s.map
        );
        assert_eq!(s.map, l.map(&anchors).unwrap(), "the copy map's text");
        assert!(s.saves, "an unsaved note is saved by the send");
        assert_eq!(s.unsaved, 0, "none left out of the file");
        assert_eq!(
            l.send_button(&anchors, Some("agent")).map(|b| b.0),
            Some("↪ save & send to agent".to_string())
        );
    }

    /// Where the file cannot be saved into, ↪ still sends and counts what
    /// the file will not hold; and what the send said stays on the bar
    /// through the save's own progress, until the next edit.
    #[test]
    fn send_without_a_save_counts_the_unsaved_and_its_answer_outlives_the_save() {
        let anchors = vec![anchor("a", Some(rect_css(0.0, 0.0, 700.0, 200.0)), false)];
        let mut l = layer(ONE_NOTE, "{}", ConcurSupport::Supported);
        l.add_note(
            "a".into(),
            "t".into(),
            "one".into(),
            "2026-09-25 09:00".into(),
        );
        l.set_gone(true);
        let s = l.send(&anchors).expect("still a map");
        assert!(!s.saves, "a file gone from disk takes no save");
        assert_eq!(s.unsaved, 1);
        assert_eq!(
            l.send_button(&anchors, Some("agent")).map(|b| b.0),
            Some("↪ send to agent".to_string())
        );
        l.set_gone(false);
        let _ = l.begin_save();
        l.say_sent(Said::Done("in the agent's prompt, not sent".into()));
        l.confirmed("brief.html");
        let bar = l.report(&anchors);
        assert_eq!(bar["sent"], "in the agent's prompt, not sent");
        assert_eq!(bar["said"], "saved into brief.html ✓");
        l.add_note(
            "a".into(),
            "t".into(),
            "two".into(),
            "2026-09-25 09:01".into(),
        );
        assert_eq!(l.report(&anchors)["sent"], serde_json::Value::Null);
    }

    /// Every way a page cannot be saved into is said in words before
    /// anyone presses save: a format newer than TD, a build input.
    #[test]
    fn a_page_says_why_it_cannot_be_saved_into() {
        let future = NotesLayer::new(
            notes::read(
                b"<script id=\"report-notes\" data-format=\"2\">{}</script><script>function tag() {} // reader notes</script>",
            ),
            Some("f.html".into()),
            ConcurSupport::Supported,
            3,
            Path::new("/r/f.html"),
        );
        let mut future = future;
        assert!(future.can_save().unwrap_err().contains("format 2"));
        future.open("a".into(), "t".into());
        assert!(!future.has_caret(), "no draft to type into");
        assert!(!future.add(SystemTime::now()));
        let build = NotesLayer::new(
            notes::read(b"<script id=\"report-notes\">{}</script><script>function tag() {} // reader notes</script>"),
            None,
            ConcurSupport::Supported,
            3,
            Path::new("/r/_x_body.html"),
        );
        assert!(build.can_save().unwrap_err().contains("build input"));
    }

    fn kept_map(json: &str) -> NoteMap {
        match notes::parse_json(json.as_bytes()).expect("a map") {
            notes::Json::Obj(o) => NoteMap(o),
            _ => panic!("an object"),
        }
    }

    fn md_anchor(nid: &str, title: &str, line: u32) -> Anchor {
        let mut a = anchor(nid, Some(rect_css(0.0, 0.0, 700.0, 40.0)), false);
        a.title = title.into();
        a.line = Some(line);
        a
    }

    /// A Markdown document's layer counts every note TD keeps for it, one on
    /// words the file no longer has included, and its map names lines; its
    /// bar shows only with something on it; and keeping a note says nothing.
    ///
    /// Mutation-tested: counting only the notes on present anchors, and
    /// drawing the bar whatever it holds, each fail this.
    #[test]
    fn a_markdown_layer_counts_every_kept_note_and_maps_lines() {
        let anchors = vec![md_anchor("h-a", "# A", 7)];
        let doc = Path::new("/r/plan.md");
        let mut l = NotesLayer::for_markdown(
            doc,
            Ok(kept_map(
                r##"{"h-a":[{"text":"one","title":"# A","ts":"2026-09-25 10:00"}],"p-gone":[{"text":"two","title":"Gone words","ts":"2026-09-25 10:01"}]}"##,
            )),
        );
        assert_eq!(
            l.counts(&anchors),
            (2, None),
            "the one on gone words counts"
        );
        let map = l.map(&anchors).expect("a map");
        assert!(map.starts_with("NOTES — /r/plan.md\n"), "{map}");
        assert!(map.contains("[L7] # A\n  · one"), "{map}");
        assert!(
            map.contains("On words no longer in the file:\n\n[p-gone] Gone words\n  · two"),
            "{map}"
        );
        assert_eq!(l.report(&anchors)["kept"], "store");
        assert!(l.shows_bar(&anchors));
        assert_eq!(l.store_doc(), Some(doc));

        let empty = NotesLayer::for_markdown(doc, Ok(NoteMap::default()));
        assert!(
            !empty.shows_bar(&anchors),
            "nothing written: no bar over the plan"
        );
        assert_eq!(empty.send(&anchors), None, "and nothing to send");
        let unreadable =
            NotesLayer::for_markdown(doc, Err("its notes were kept by a newer TD".into()));
        assert!(
            unreadable.shows_bar(&anchors),
            "a reason is something on the bar"
        );
        assert!(
            unreadable.can_edit().is_err(),
            "and nothing is written over it"
        );

        l.add_note(
            "h-a".into(),
            "# A".into(),
            "three".into(),
            "2026-09-25 10:02".into(),
        );
        assert!(l.wants_keeping());
        let edits = l.begin_save();
        assert_eq!(edits.len(), 1);
        assert_eq!(
            l.report(&anchors)["said"],
            serde_json::Value::Null,
            "keeping says nothing"
        );
        assert!(!l.wants_keeping(), "one keep at a time");
        let mut after = l.notes().cloned().expect("notes");
        let mut c = ConcurMap::default();
        apply(&mut after, &mut c, &[], &[], false).unwrap();
        l.kept(1, after);
        assert_eq!(l.unsaved(), 0);
        assert_eq!(l.counts(&anchors).0, 3);
    }

    /// Closing over a note not yet kept is held once, in the words of a
    /// store rather than a file.
    #[test]
    fn a_markdown_note_not_yet_kept_holds_a_close_once() {
        let mut l = NotesLayer::for_markdown(Path::new("/r/plan.md"), Ok(NoteMap::default()));
        l.add_note(
            "h-a".into(),
            "# A".into(),
            "x".into(),
            "2026-09-25 10:00".into(),
        );
        assert!(l.guard_close());
        let said = l.report(&[])["said"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        assert!(said.contains("not kept"), "{said}");
        assert!(!l.guard_close(), "the second close goes");
    }

    /// Notes on words a Markdown file no longer has are found, open to be
    /// read and deleted, and take no new note: there is nothing left to
    /// hang one on.
    ///
    /// Mutation-tested: letting a box on gone words take a caret fails this.
    #[test]
    fn notes_on_gone_words_can_be_deleted_and_not_added_to() {
        let anchors = vec![md_anchor("h-a", "# A", 7)];
        let mut l = NotesLayer::for_markdown(
            Path::new("/r/plan.md"),
            Ok(kept_map(
                r##"{"h-a":[{"text":"one","title":"# A","ts":"2026-09-25 10:00"}],"p-gone":[{"text":"two","title":"Gone words","ts":"2026-09-25 10:01"}]}"##,
            )),
        );
        assert_eq!(
            l.orphans(&anchors),
            vec![("p-gone".to_string(), "Gone words".to_string())]
        );
        l.open_gone("p-gone".into(), "Gone words".into());
        assert!(!l.has_caret(), "no draft on gone words");
        assert!(!l.add(SystemTime::now()), "and nothing added");
        l.delete_shown(0);
        assert_eq!(
            l.orphans(&anchors),
            Vec::<(String, String)>::new(),
            "deleted"
        );
        assert_eq!(l.unsaved(), 1, "a delete waiting to be kept");
        let brief = layer(ONE_NOTE, "{}", ConcurSupport::Supported);
        assert!(brief.orphans(&[]).is_empty(), "a brief has no such notes");
    }
}
