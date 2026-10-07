//! What an agent handed over, kept per pane and shown from the pane's header.
//!
//! An agent hands a person something it made in one of three ways, and all
//! three arrive here as a [`Handover`]:
//!
//! ```text
//!   declare_deliverable (MCP verb)     ─┐
//!   a printed `Deliverable:` line      ─┼→  Ledger  →  the header chip
//!   an artifact surface on the bench   ─┘      │        and the history page
//!                                              └→  filed into the conversation's record
//! ```
//!
//! # Why a ledger beside the bench, when the bench already shelves artifacts
//!
//! The bench keeps the newest 64 surfaces a pane was shown, and a turn's
//! response takes one of them every turn. One agent handed over sixteen links in
//! twenty-six hours on 5–6 October while answering twenty-three turns, so on a
//! long-running agent the oldest handovers would leave the bench before anybody
//! went looking for them — which is exactly when somebody does. The ledger is
//! rebuilt from the conversation's whole record and has no such cap; the bench
//! keeps showing what it shows.
//!
//! # Why the screen is read at all
//!
//! Claude Code here draws on the alternate screen, so a terminal's scrollback
//! never holds an old reply, and of 51 agent transcripts that printed a
//! `Deliverable:` line in the week to 6 October, 19 never called the verb. A
//! list fed only by the verb would miss a third of what was handed over. The
//! rule for which rows count is [`handover_rows`], and it is narrow on purpose:
//! a row that starts with `Deliverable:` and carries a link, on that row or as
//! the first thing on the next. A Links table is never read — it lists
//! everything referenced, and choosing which of those was made is the guess the
//! attention rail was built to refuse.
//!
//! # Nothing here knows about gpui
//!
//! The pane reads the screen, stats files and opens documents; every decision
//! about what those mean is a pure function here with a table test, in the
//! shape [`crate::attention`] and [`crate::workbench`] established.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// How a handover reached this window. Also how strongly it was claimed: a
/// declaration outranks a surface, which outranks a line read off a screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    /// A `Deliverable:` line on the pane's screen.
    Said,
    /// An artifact surface the agent put on its own bench.
    Presented,
    /// The `declare_deliverable` verb.
    Declared,
}

impl Source {
    /// The word a row says. Lower case, because it reads as a fact after a
    /// time, not as a label.
    pub fn word(self) -> &'static str {
        match self {
            Source::Said => "said",
            Source::Presented => "presented",
            Source::Declared => "declared",
        }
    }

    fn from_word(w: &str) -> Option<Source> {
        match w {
            "said" => Some(Source::Said),
            "presented" => Some(Source::Presented),
            "declared" => Some(Source::Declared),
            _ => None,
        }
    }
}

/// One thing an agent handed over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Handover {
    /// What to call it: the agent's own words, or the target's last segment
    /// when it gave none.
    pub label: String,
    /// An absolute path, a `file://` URL or a web address, as given.
    pub href: String,
    /// When this window first saw it handed over, in Unix milliseconds. Not
    /// the file's time: ten of the sixteen files in the measured pane were
    /// rewritten after they were handed over, three in the same minute, and a
    /// list ordered by file time reshuffles itself on every rebuild.
    pub at_ms: u64,
    pub source: Source,
}

impl Handover {
    /// The identity a handover is kept under. Two spellings of one file —
    /// `/a/b.html` and `file:///a/b.html`, or one percent-encoded — are one
    /// handover.
    pub fn key(&self) -> String {
        key_of(&self.href)
    }

    /// The file on this machine it names, if it names one.
    pub fn path(&self) -> Option<PathBuf> {
        file_path(&self.href)
    }

    /// A short word for what it is: the file's extension, or `web`.
    pub fn kind(&self) -> String {
        kind_of(&self.href)
    }
}

/// The file an href names on this machine: an absolute path, or a `file://`
/// URL with no host or `localhost`. Percent-decoded, fragment and query
/// dropped, `.` and `..` folded — the same reading a document link gets.
pub fn file_path(href: &str) -> Option<PathBuf> {
    let href = href.trim();
    // A bare path is taken as written: `#` and `?` are ordinary characters in
    // a file name, and only a URL has a fragment.
    if href.starts_with('/') {
        return Some(PathBuf::from(href));
    }
    if !href
        .get(..5)
        .is_some_and(|s| s.eq_ignore_ascii_case("file:"))
    {
        return None;
    }
    match crate::docview::resolve_link(Path::new("/"), href) {
        crate::docview::LinkTarget::File { path, .. } => Some(path),
        _ => None,
    }
}

/// See [`Handover::key`].
pub fn key_of(href: &str) -> String {
    match file_path(href) {
        Some(p) => p.to_string_lossy().into_owned(),
        None => href.trim().to_string(),
    }
}

/// See [`Handover::kind`].
pub fn kind_of(href: &str) -> String {
    let Some(path) = file_path(href) else {
        return "web".into();
    };
    path.extension()
        .and_then(|e| e.to_str())
        .filter(|e| !e.is_empty() && e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_else(|| "file".into())
}

/// The name a handover with no label is shown under: the target's last path
/// segment, as the verb names an unlabelled declaration.
pub fn fallback_label(href: &str) -> String {
    crate::mcp::deliverable_fallback_label(href)
}

// ---------------------------------------------------------------------------
// the screen rule
// ---------------------------------------------------------------------------

/// The word a handover row opens with.
const MARK: &str = "Deliverable:";

/// How many rows one handover may run across. A link longer than four full
/// rows of a pane is not one a person reads off a screen either.
const MAX_ROWS: usize = 4;

/// Every handover on a screen, as `(label, href)`, top to bottom.
///
/// `rows` are the screen's rows exactly as drawn — **not** trimmed — because
/// whether a row reaches the pane's last column is what says an application
/// wrapped it. `wraps[r]` is the terminal's own word that row `r` carries on
/// into `r + 1`: its soft-wrap flag, which survives the pane growing wider
/// afterwards, when the row stops reaching the edge.
///
/// A row is a handover when, after its margin, it starts with `Deliverable:`
/// and a link follows: on that row, carried across the rows a long link was
/// wrapped onto, or as the first thing on the next row (the house format puts
/// the label and a dash on one line and the URL on the next). Nothing else is:
///
/// - a tool call echoing the word (`⎿  "Deliverable:"`) does not start with it;
/// - prose that happened to wrap onto the start of a row has no link after it;
/// - a Links table row does not start with it.
///
/// **The wrap join is margin-aware.** Claude Code draws the rest of a wrapped
/// row two spaces in, at the message's own margin, so the join the copy chip
/// uses — which wants the next row to start flush — would cut a link at the
/// pane's edge. Here a row continues onto the next when it reaches the last
/// column and the next row starts at the same margin; measured on the HC Video
/// pane on 2026-10-06, a 100-character row in a 100-column pane.
pub fn handover_rows(rows: &[String], wraps: &[bool]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        let row = &rows[i];
        let body = row.trim_start();
        let margin = row.chars().count() - body.chars().count();
        let Some(rest) = strip_bullet(body).strip_prefix(MARK) else {
            i += 1;
            continue;
        };
        // The handover's own text: this row, and the rows a link that did not
        // fit was carried onto.
        let (text, mut last, cut) = carry(rows, wraps, i, margin, rest.trim_start().to_string());
        let (label, href) = match find_link(&text) {
            Some((at, href)) => (text[..at].to_string(), Some(href)),
            None => (text.clone(), None),
        };
        let mut cut = cut && href.is_some();
        // No link on the row: the house format's second line, the URL alone,
        // and its own wrapped continuation.
        let href = href.or_else(|| {
            let j = (last + 1..rows.len()).find(|&j| !rows[j].trim().is_empty())?;
            let first = rows[j].trim();
            let (at, _) = find_link(first)?;
            if at != 0 {
                return None;
            }
            let next_margin = rows[j].chars().count() - rows[j].trim_start().chars().count();
            let (t, k, c) = carry(rows, wraps, j, next_margin, first.to_string());
            last = k;
            cut = c;
            find_link(&t).map(|(_, h)| h)
        });
        // A link that runs to the pane's edge with nowhere found to carry it
        // was cut short, and a cut link opens nothing. It is refused rather
        // than kept: a row that is missing says less than a row that lies.
        if cut {
            i = last + 1;
            continue;
        }
        if let Some(href) = href {
            let label = clean_label(&label);
            let label = if label.is_empty() {
                fallback_label(&href)
            } else {
                label
            };
            out.push((label, href));
        }
        i = last + 1;
    }
    out
}

/// Carry a link that reached the pane's edge onto the rows it was wrapped
/// onto, starting from row `start` whose text so far is `text`. Two wraps are
/// followed: Claude Code's own, which carries on at the message's margin, and
/// the terminal's, which carries on at column 0 — the shape when an agent that
/// does not draw fullscreen prints a line wider than the pane.
///
/// Answers the text, the last row used, and whether the link still ends at an
/// edge with nothing found to carry it: cut, which the caller refuses. A row
/// that opens a handover of its own is never a continuation, and a link that
/// reaches the edge just before one ended there.
fn carry(
    rows: &[String],
    wraps: &[bool],
    start: usize,
    margin: usize,
    mut text: String,
) -> (String, usize, bool) {
    let mut last = start;
    loop {
        let flagged = wraps.get(last).copied().unwrap_or(false);
        if !(flagged || fills(&rows[last])) || !ends_in_link(&text) {
            return (text, last, false);
        }
        let Some(next) = rows.get(last + 1) else {
            return (text, last, true);
        };
        // A row that opens a handover of its own, or a blank one, carries
        // nothing on: the link ended exactly at the edge.
        if next.trim().is_empty() || strip_bullet(next.trim_start()).starts_with(MARK) {
            return (text, last, false);
        }
        if last + 1 - start >= MAX_ROWS
            || !(flagged || at_margin(next, margin) || at_margin(next, 0))
        {
            return (text, last, true);
        }
        text.push_str(next.trim());
        last += 1;
    }
}

/// A leading bullet the renderer drew in front of a message's first line.
fn strip_bullet(s: &str) -> &str {
    for b in ["⏺ ", "● ", "• ", "- ", "* "] {
        if let Some(rest) = s.strip_prefix(b) {
            return rest.trim_start();
        }
    }
    s
}

/// The row runs to the pane's last column: the shape of a wrap.
fn fills(row: &str) -> bool {
    row.chars().last().is_some_and(|c| !c.is_whitespace())
}

/// The row starts at `margin` columns and has something there.
fn at_margin(row: &str, margin: usize) -> bool {
    let body = row.trim_start();
    !body.is_empty() && row.chars().count() - body.chars().count() == margin
}

/// The text so far ends inside a link: the only case a row is carried on.
/// A label that wrapped is not followed, because a link several rows down in
/// prose is not this line's.
fn ends_in_link(text: &str) -> bool {
    let last = text.rsplit(char::is_whitespace).next().unwrap_or("");
    find_link(last).is_some_and(|(at, _)| at == 0) || is_link_prefix(last)
}

/// A token that is the start of a scheme a link would have, cut at the edge.
fn is_link_prefix(token: &str) -> bool {
    let t = token.trim_start_matches(['<', '(']);
    !t.is_empty()
        && ["file://", "https://", "http://"]
            .iter()
            .any(|s| s.starts_with(t) || t.starts_with(s))
}

/// The first link in `text`: its byte offset and the link itself, trimmed of
/// the punctuation a sentence puts round it.
fn find_link(text: &str) -> Option<(usize, String)> {
    let mut at = 0;
    for token in text.split_inclusive(char::is_whitespace) {
        let word = token.trim_end();
        let lead = word.len() - word.trim_start_matches(['<', '(', '`', '"']).len();
        let candidate = word[lead..].trim_end_matches(['>', ')', '`', '"', ',', '.', ';', ':']);
        if is_link(candidate) {
            return Some((at + lead, candidate.to_string()));
        }
        at += token.len();
    }
    None
}

/// A link worth opening: a file URL or absolute path with a real path, or a
/// web address with a host. The template agents are taught to write —
/// `<file:// or https:// URL>` — is not one.
fn is_link(s: &str) -> bool {
    if let Some(rest) = s.strip_prefix("file://") {
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        return rest.len() > 1 && rest.starts_with('/');
    }
    for scheme in ["https://", "http://"] {
        if let Some(rest) = s.strip_prefix(scheme) {
            return rest.split('/').next().is_some_and(|h| {
                h.contains('.') || h.starts_with("127.") || h.starts_with("localhost")
            });
        }
    }
    // A bare absolute path: two segments at least and a file name with an
    // extension, so `/` in a sentence is not mistaken for one.
    s.starts_with('/')
        && s[1..].contains('/')
        && Path::new(s)
            .extension()
            .is_some_and(|e| !e.is_empty() && e.len() <= 5)
}

/// A label as written before its link: trailing dashes and colons are the
/// joint, not the name.
fn clean_label(s: &str) -> String {
    s.trim()
        .trim_end_matches(['—', '–', '-', ':', '·'])
        .trim()
        .trim_matches('*')
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// the ledger
// ---------------------------------------------------------------------------

/// What [`Ledger::record`] did with a handover.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recorded {
    /// Never seen before: a new row.
    New,
    /// Already known, now claimed more strongly — a line read off the screen
    /// that the agent then declared. Same row, same time, the stronger word.
    Upgraded,
    /// Already known as well as this says it. Nothing changed.
    Known,
}

/// One pane's handovers, and which of them a person has opened.
#[derive(Clone, Debug, Default)]
pub struct Ledger {
    items: Vec<Handover>,
    /// Key → when it was opened, in Unix milliseconds.
    opened: HashMap<String, u64>,
}

impl Ledger {
    /// Take a handover. The same link handed over again keeps its first time
    /// and its row: an agent re-surfaces its main deliverable every turn until
    /// a person acts on it, and every one of those is the same thing.
    pub fn record(&mut self, h: Handover) -> Recorded {
        let key = h.key();
        match self.items.iter_mut().find(|e| e.key() == key) {
            None => {
                self.items.push(h);
                Recorded::New
            }
            Some(e) if h.source > e.source => {
                e.source = h.source;
                // The stronger claim brings its own name with it, unless it
                // gave none and the weaker one did.
                if !h.label.is_empty() && h.label != fallback_label(&h.href) {
                    e.label = h.label;
                }
                e.at_ms = e.at_ms.min(h.at_ms);
                Recorded::Upgraded
            }
            Some(e) => {
                if h.at_ms < e.at_ms {
                    e.at_ms = h.at_ms;
                }
                Recorded::Known
            }
        }
    }

    /// Every handover, newest first. Ties keep the order they were recorded
    /// in, newest-recorded first.
    pub fn newest_first(&self) -> Vec<&Handover> {
        let mut v: Vec<(usize, &Handover)> = self.items.iter().enumerate().collect();
        v.sort_by(|(ia, a), (ib, b)| b.at_ms.cmp(&a.at_ms).then(ib.cmp(ia)));
        v.into_iter().map(|(_, h)| h).collect()
    }

    /// The handover the chip names.
    pub fn newest(&self) -> Option<&Handover> {
        self.newest_first().into_iter().next()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn contains(&self, key: &str) -> bool {
        self.items.iter().any(|h| h.key() == key)
    }

    /// The handover kept under `key`, as it stands after every merge.
    pub fn get(&self, key: &str) -> Option<&Handover> {
        self.items.iter().find(|h| h.key() == key)
    }

    /// Whether a person has opened it. Once opened it stays opened: a file
    /// rewritten afterwards is shown as rewritten, not relit, because the
    /// measured rewrites were rebuilds far more often than revisions.
    pub fn is_opened(&self, key: &str) -> bool {
        self.opened.contains_key(key)
    }

    /// Mark a handover opened. Answers whether that changed anything, so a
    /// caller writes the mark to disk once and not on every open.
    pub fn mark_opened(&mut self, key: &str, at_ms: u64) -> bool {
        if !self.contains(key) || self.opened.contains_key(key) {
            return false;
        }
        self.opened.insert(key.to_string(), at_ms);
        true
    }

    /// Marks read back from disk. Kept even for keys not (yet) in the list,
    /// because the record and the marks are read in either order.
    pub fn restore_opened(&mut self, marks: HashMap<String, u64>) {
        for (k, at) in marks {
            self.opened.entry(k).or_insert(at);
        }
    }
}

/// What the tests count. The window never needs a number: the chip names the
/// newest handover rather than counting them, and the page counts its rows.
#[cfg(test)]
impl Ledger {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn unopened(&self) -> usize {
        self.items
            .iter()
            .filter(|h| !self.is_opened(&h.key()))
            .count()
    }
}

// ---------------------------------------------------------------------------
// the filed surface
// ---------------------------------------------------------------------------

/// The id a handover is filed under. Derived from the link, so the same link
/// filed twice is one record line updated, not a second row.
pub fn surface_id(key: &str) -> String {
    // FNV-1a, 64 bits: stable across builds and machines, which `DefaultHasher`
    // does not promise, and the id outlives this build in the record.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in key.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("handover-{h:016x}")
}

/// The artifact surface a handover is filed as.
///
/// An ordinary `artifact`, so the bench's ARTIFACTS shelf shows it with no
/// code of its own, plus one key the shelf draws as a fact on the card and
/// [`from_surface`] reads back: how it was handed over.
pub fn surface_doc(h: &Handover) -> Value {
    json!({
        "td": "0.4",
        "kind": "artifact",
        "id": surface_id(&h.key()),
        "title": h.label,
        "model": {
            "href": h.href,
            "handover": h.source.word(),
        },
    })
}

/// A handover read back out of a surface in the record: one TD filed, or an
/// artifact the agent presented itself. `at_ms` is when the record says it
/// was first said.
pub fn from_surface(doc: &Value, at_ms: u64) -> Option<Handover> {
    if doc.get("kind").and_then(Value::as_str) != Some("artifact") {
        return None;
    }
    let post = crate::surface::parse_lenient(doc, at_ms, "");
    let surface = post.surface?;
    let crate::surface::Kind::Artifact(a) = surface.kind else {
        return None;
    };
    let source = doc
        .get("model")
        .and_then(|m| m.get("handover"))
        .and_then(Value::as_str)
        .and_then(Source::from_word)
        .unwrap_or(Source::Presented);
    let label = if surface.title.trim().is_empty() {
        fallback_label(&a.href)
    } else {
        surface.title
    };
    Some(Handover {
        label,
        href: a.href,
        at_ms,
        source,
    })
}

// ---------------------------------------------------------------------------
// the history page
// ---------------------------------------------------------------------------

/// What is known about a handed-over file now, beside when it was handed over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileFact {
    /// It is there and has not changed since.
    Same,
    /// It is there and was written after it was handed over, at this time.
    Rewritten(u64),
    /// It is not there any more.
    Gone,
    /// Not a file, or a file whose state could not be read. Says nothing,
    /// rather than claiming it did not change.
    Unknown,
}

/// A wall-clock reading in this machine's local time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
    pub year: i32,
    /// 1–12.
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    /// 0 = Sunday.
    pub weekday: u32,
}

impl Clock {
    fn date(self) -> (i32, u32, u32) {
        (self.year, self.month, self.day)
    }

    fn hhmm(self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }
}

/// `ms` in local time, or `None` when the C library cannot place it — a
/// handover dated 1 January 1970 would be a clock failure wearing a plausible
/// answer.
pub fn local(ms: u64) -> Option<Clock> {
    let secs: libc::time_t = (ms / 1000).try_into().ok()?;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: `localtime_r` is the reentrant form: it writes into the `tm` we
    // own and holds no state another thread could race for. Both pointers are
    // valid for the call.
    if unsafe { libc::localtime_r(&secs, &mut tm) }.is_null() {
        return None;
    }
    let year = 1900i32.checked_add(tm.tm_year)?;
    if !(1900..=2999).contains(&year) {
        return None;
    }
    Some(Clock {
        year,
        month: u32::try_from(tm.tm_mon).ok()? + 1,
        day: u32::try_from(tm.tm_mday).ok()?,
        hour: u32::try_from(tm.tm_hour).ok()?,
        minute: u32::try_from(tm.tm_min).ok()?,
        weekday: u32::try_from(tm.tm_wday).ok()?,
    })
}

/// One row of the history page, with everything the page needs already
/// worked out, so the page is a pure function of its rows.
#[derive(Clone, Debug)]
pub struct Row {
    pub label: String,
    pub href: String,
    pub kind: String,
    pub source: Source,
    pub opened: bool,
    pub when: Option<Clock>,
    pub fact: FileFact,
}

/// The history page: every handover in a pane, newest first, grouped by day.
///
/// Markdown, because Terminal Delight draws Markdown itself in the floating
/// square, where a plain click on a link opens the linked file in the square
/// in this page's place — the open-into-the-pane gesture, with no list code of
/// its own. It is also a file an agent can read and a person can keep.
///
/// `today` and `yesterday` are the local dates the headings are named
/// against; `None` names every day by its date.
pub fn history_markdown(
    pane: &str,
    rows: &[Row],
    today: Option<(i32, u32, u32)>,
    yesterday: Option<(i32, u32, u32)>,
) -> String {
    let mut out = String::new();
    out.push_str("# Handed over\n\n");
    let pane = escape(pane.trim());
    let unopened = rows.iter().filter(|r| !r.opened).count();
    if rows.is_empty() {
        out.push_str(&format!(
            "**{pane}** has not handed anything over in this conversation yet.\n\n\
             An agent hands something over with the `declare_deliverable` verb, \
             or with a line that starts `Deliverable:` and carries a link. \
             Each one is listed here, newest first, and opens over the pane with one click.\n"
        ));
        return out;
    }
    let count = match rows.len() {
        1 => "1 artifact".to_string(),
        n => format!("{n} artifacts"),
    };
    let fresh = match unopened {
        0 => "all opened".to_string(),
        n => format!("**{n} new**"),
    };
    // The pane's name on a line of its own, so a long one wraps as a name
    // rather than pushing the counts onto a ragged second line.
    out.push_str(&format!(
        "**{pane}**\n\n\
         {count}, newest first · {fresh} · \
         click a name to open it over this pane, Esc puts this page away.\n"
    ));
    let mut day: Option<Option<(i32, u32, u32)>> = None;
    for r in rows {
        let this = r.when.map(Clock::date);
        if day != Some(this) {
            day = Some(this);
            out.push_str(&format!("\n## {}\n", day_heading(r.when, today, yesterday)));
        }
        // One marker per row: the list's own. "New" is a word in the line
        // under the name, where the eye that has found the name reads on.
        out.push_str(&format!(
            "\n- **[{}]({})**\n\n  ",
            escape(&r.label),
            link_target(&r.href)
        ));
        let mut meta = Vec::new();
        if !r.opened {
            meta.push("**new**".to_string());
        }
        meta.push(format!("`{}`", r.kind));
        meta.push(
            r.when
                .map(Clock::hhmm)
                .unwrap_or_else(|| "time unavailable".into()),
        );
        meta.push(r.source.word().to_string());
        match r.fact {
            FileFact::Rewritten(ms) => meta.push(match local(ms) {
                Some(c) if Some(c.date()) == r.when.map(Clock::date) => {
                    format!("rewritten {}", c.hhmm())
                }
                Some(c) => format!("rewritten {} {}", short_date(c), c.hhmm()),
                None => "rewritten since".into(),
            }),
            FileFact::Gone => meta.push("**file gone**".into()),
            FileFact::Same | FileFact::Unknown => {}
        }
        out.push_str(&meta.join(" · "));
        out.push('\n');
    }
    out.push_str(
        "\n---\n\nWritten by Terminal Delight from what this agent handed over, \
         and rewritten whenever something new arrives.\n",
    );
    out
}

fn day_heading(
    when: Option<Clock>,
    today: Option<(i32, u32, u32)>,
    yesterday: Option<(i32, u32, u32)>,
) -> String {
    let Some(c) = when else {
        return "Time unavailable".into();
    };
    if Some(c.date()) == today {
        return "Today".into();
    }
    if Some(c.date()) == yesterday {
        return "Yesterday".into();
    }
    const DAY: [&str; 7] = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];
    let name = DAY.get(c.weekday as usize).copied().unwrap_or("");
    format!("{name} {}", short_date(c)).trim().to_string()
}

fn short_date(c: Clock) -> String {
    const MONTH: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let m = MONTH
        .get(c.month.saturating_sub(1) as usize)
        .copied()
        .unwrap_or("");
    format!("{} {m}", c.day)
}

/// Markdown's own characters in a label, made literal.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(
            c,
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '|'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A link target a Markdown parser takes whole: a file as a `file://` URL
/// with the characters that would end or break the link percent-encoded, a
/// web address with only those.
fn link_target(href: &str) -> String {
    let encode = |s: &str| {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            match c {
                ' ' => out.push_str("%20"),
                '(' => out.push_str("%28"),
                ')' => out.push_str("%29"),
                '<' => out.push_str("%3C"),
                '>' => out.push_str("%3E"),
                '%' => out.push_str("%25"),
                '#' => out.push_str("%23"),
                '?' => out.push_str("%3F"),
                c => out.push(c),
            }
        }
        out
    };
    match file_path(href) {
        Some(p) => format!("file://{}", encode(&p.to_string_lossy())),
        None => href
            .trim()
            .replace(' ', "%20")
            .replace('(', "%28")
            .replace(')', "%29"),
    }
}

// ---------------------------------------------------------------------------
// where it lives
// ---------------------------------------------------------------------------

/// The directory this feature writes into, beside the surfaces and the
/// conversations: `$XDG_STATE_HOME/terminal-delight/handovers`.
///
/// Under test it is a folder of this test process's own, because a pane test
/// that presents an artifact writes a page, and a test run must not leave
/// pages in the state directory of the person running it.
pub fn dir() -> PathBuf {
    if cfg!(test) {
        return std::env::temp_dir().join(format!("td-handovers-test-{}", std::process::id()));
    }
    crate::surfacefeed::surfaces_root()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(crate::surfacefeed::surfaces_root)
        .join("handovers")
}

/// The page's own file name. The floating square names a document by its file
/// in its strip, so the conversation is the folder and this is what reads.
pub const PAGE: &str = "handed-over.md";

/// The history page for a conversation, or for a pane not yet bound to one.
/// `None` when the conversation's name could not safely be a file name.
pub fn history_path(root: Option<&str>, session: &str, pane: u64) -> Option<PathBuf> {
    match root {
        Some(r) if crate::benchstore::safe_segment(r) => Some(dir().join(r).join(PAGE)),
        Some(_) => None,
        None => {
            let session: String = session
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect();
            Some(dir().join(format!("pane-{session}-{pane}")).join(PAGE))
        }
    }
}

/// Where a conversation's opened marks are kept: one JSON line per open.
pub fn opened_path(root: &str) -> Option<PathBuf> {
    crate::benchstore::safe_segment(root).then(|| dir().join(root).join("opened.jsonl"))
}

/// Read the opened marks back. A missing file is no marks; a line that does
/// not parse is skipped, because one torn line must not relight every row.
pub fn read_opened(path: &Path) -> HashMap<String, u64> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let (Some(k), Some(at)) = (
            v.get("key").and_then(Value::as_str),
            v.get("at_ms").and_then(Value::as_u64),
        ) {
            out.entry(k.to_string()).or_insert(at);
        }
    }
    out
}

/// Append one opened mark.
pub fn append_opened(path: &Path, key: &str, at_ms: u64) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{}", json!({ "key": key, "at_ms": at_ms }))
}

/// Write the page only when it changed, so an open square showing it is not
/// told to reload for nothing. Answers whether it wrote.
pub fn write_if_changed(path: &Path, text: &str) -> std::io::Result<bool> {
    if std::fs::read_to_string(path).is_ok_and(|old| old == text) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Written beside and renamed over, so a reader never sees half a page.
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(true)
}

/// What is true of a handed-over file now. `mtime_ms` is the file's
/// modification time, `None` when the file is not there, and the outer
/// `None` when it could not be read at all.
pub fn file_fact(h: &Handover, mtime_ms: Option<Option<u64>>) -> FileFact {
    if h.path().is_none() {
        return FileFact::Unknown;
    }
    match mtime_ms {
        None => FileFact::Unknown,
        Some(None) => FileFact::Gone,
        // A minute of grace: an agent declares a file in the same breath as it
        // finishes writing it, and the write can land after the declaration.
        Some(Some(m)) if m > h.at_ms.saturating_add(60_000) => FileFact::Rewritten(m),
        Some(Some(_)) => FileFact::Same,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|s| s.to_string()).collect()
    }

    /// A screen with no row flagged as the terminal's own wrap.
    fn read(screen: &[String]) -> Vec<(String, String)> {
        handover_rows(screen, &vec![false; screen.len()])
    }

    /// Pad a row to a pane's width, the way the grid hands it over.
    fn padded(line: &str, cols: usize) -> String {
        let n = line.chars().count();
        assert!(n <= cols, "{line:?} is wider than {cols}");
        format!("{line}{}", " ".repeat(cols - n))
    }

    #[test]
    fn the_house_format_carries_its_link_on_the_next_row() {
        // Read off the Terminal delight pane on 2026-10-06.
        let screen = rows(&[
            &padded("  Deliverable: The File Drop —", 90),
            &padded(
                "  file:///home/parker/Work/terminal-delight/reports/2026-10-06-the-file-drop.html",
                90,
            ),
        ]);
        assert_eq!(
            read(&screen),
            vec![(
                "The File Drop".to_string(),
                "file:///home/parker/Work/terminal-delight/reports/2026-10-06-the-file-drop.html"
                    .to_string()
            )]
        );
    }

    #[test]
    fn a_link_cut_at_the_pane_edge_is_joined_at_the_messages_margin() {
        // The HC Video pane: 100 columns, and this row is exactly 100
        // characters. Claude carries the rest two spaces in.
        let first = "  Deliverable: Club HC VIDEO spec — file:///home/parker/BROWN-FAMILY-SPORTS/hc-video/software/hc-vid";
        assert_eq!(first.chars().count(), 100);
        let screen = rows(&[
            first,
            &padded(
                "  eo-web/docs/plans/club-hc-video/2026-09-29-club-hc-video.html",
                100,
            ),
        ]);
        assert_eq!(
            read(&screen),
            vec![(
                "Club HC VIDEO spec".to_string(),
                "file:///home/parker/BROWN-FAMILY-SPORTS/hc-video/software/hc-video-web/docs/plans/club-hc-video/2026-09-29-club-hc-video.html"
                    .to_string()
            )]
        );
    }

    #[test]
    fn a_link_that_ends_short_of_the_edge_is_not_joined_to_the_next_row() {
        let screen = rows(&[
            &padded("  Deliverable: Report — file:///tmp/a/report.html", 80),
            &padded("  and something else entirely", 80),
        ]);
        assert_eq!(
            read(&screen),
            vec![("Report".into(), "file:///tmp/a/report.html".into())]
        );
    }

    #[test]
    fn what_is_not_a_handover_is_refused() {
        // Every row here is a real one from a pane open on 2026-10-06.
        let screen = rows(&[
            &padded(
                "  Deliverable: line, that's where the actual artifacts live. I'll review",
                90,
            ),
            &padded("  the workbench next.", 90),
            &padded("  ⎿  \"Deliverable:\"", 90),
            &padded(
                "  │ file:///home/parker/x/td-movements-symphony-final-flat.mp4 │ The symphony, flat │",
                90,
            ),
            &padded("Deliverable: <short label> — <file:// or https:// URL>", 90),
        ]);
        assert_eq!(read(&screen), vec![]);
    }

    #[test]
    fn the_terminals_own_wrap_carries_a_link_on_at_column_zero() {
        // The photograph's case: printed while the pane was narrower than the line,
        // so the terminal itself wrapped the link and carried it on at the
        // left edge, not at the message's margin.
        let url = "  file:///home/parker/Work/cinema-delight/campaigns/2026-10-05-terminal-delight-movements/sound/README.md";
        let head: String = url.chars().take(100).collect();
        let tail: String = url.chars().skip(100).collect();
        assert!(head.ends_with("READ"), "{head}");
        let screen = rows(&[
            &padded("  Deliverable: the four voicings —", 100),
            &head,
            &padded(&tail, 100),
        ]);
        assert_eq!(
            read(&screen),
            vec![("the four voicings".into(), url.trim().to_string())]
        );
    }

    #[test]
    fn a_row_the_terminal_wrapped_is_carried_on_after_the_pane_grows_wider() {
        // The photograph's second case: printed narrow, so the terminal wrapped
        // the link and flagged the row; then the window grew, the alternate
        // screen was not reflowed, and the row now ends in blanks. Only the
        // flag still says it carries on.
        let screen = rows(&[
            &padded("  Deliverable: the four voicings —", 140),
            &padded(
                "  file:///home/parker/Work/cinema-delight/campaigns/2026-10-05-terminal-delight-movements/sound/READ",
                140,
            ),
            &padded("ME.md", 140),
        ]);
        let href = "file:///home/parker/Work/cinema-delight/campaigns/2026-10-05-terminal-delight-movements/sound/README.md";
        assert_eq!(
            handover_rows(&screen, &[false, true, false]),
            vec![("the four voicings".into(), href.into())]
        );
        // Without the flag the same rows are a link that simply ends there.
        assert_eq!(read(&screen)[0].1, href.replace("README.md", "READ"));
    }

    #[test]
    fn a_link_cut_at_the_edge_with_nothing_to_carry_it_is_refused_not_kept_short() {
        // Full to the last column, and the next row is somewhere else
        // entirely: what is on screen is a link cut short, which would open
        // nothing. Neither a short link nor a guess is recorded.
        let cut = "  Deliverable: A page — file:///home/parker/reports/2026-10-06-a-very-long-n";
        let screen = rows(&[cut, &padded("      unrelated text, indented further", 77)]);
        assert_eq!(read(&screen), vec![]);
        // At the very bottom of the screen it is just as cut.
        assert_eq!(read(&rows(&[cut])), vec![]);
    }

    #[test]
    fn a_web_address_and_a_bare_path_are_links_too() {
        let screen = rows(&[
            &padded(
                "  Deliverable: PR 902 — https://github.com/parker-brown-family/terminal-delight/pull/902",
                100,
            ),
            &padded("  Deliverable: /tmp/graphics-protocols.md", 100),
        ]);
        assert_eq!(
            read(&screen),
            vec![
                (
                    "PR 902".into(),
                    "https://github.com/parker-brown-family/terminal-delight/pull/902".into()
                ),
                (
                    "graphics-protocols.md".into(),
                    "/tmp/graphics-protocols.md".into()
                ),
            ]
        );
    }

    #[test]
    fn a_bullet_the_renderer_drew_does_not_hide_the_line() {
        let screen = rows(&[&padded(
            "⏺ Deliverable: The page — file:///tmp/p/page.html",
            80,
        )]);
        assert_eq!(read(&screen).len(), 1);
    }

    #[test]
    fn two_handovers_on_one_screen_are_both_read_in_order() {
        let screen = rows(&[
            &padded("  Deliverable: One — file:///tmp/a/one.html", 60),
            &padded("  some prose between", 60),
            &padded("  Deliverable: Two —", 60),
            &padded("  file:///tmp/a/two.md", 60),
        ]);
        let got: Vec<String> = read(&screen).into_iter().map(|(l, _)| l).collect();
        assert_eq!(got, vec!["One", "Two"]);
    }

    #[test]
    fn a_link_that_happens_to_end_at_the_edge_does_not_swallow_the_next_handover() {
        // Both rows exactly fill a 46-column pane, so the first looks wrapped;
        // the second opens a handover of its own and is never a continuation.
        let a = "  Deliverable: A — file:///tmp/a/first-one.md";
        let b = "  Deliverable: B — file:///tmp/a/second-2a.md";
        assert_eq!(a.chars().count(), b.chars().count());
        let screen = rows(&[a, b, &padded("", 46)]);
        assert_eq!(
            read(&screen),
            vec![
                ("A".into(), "file:///tmp/a/first-one.md".into()),
                ("B".into(), "file:///tmp/a/second-2a.md".into()),
            ]
        );
    }

    fn h(label: &str, href: &str, at: u64, source: Source) -> Handover {
        Handover {
            label: label.into(),
            href: href.into(),
            at_ms: at,
            source,
        }
    }

    #[test]
    fn one_file_under_two_spellings_is_one_handover() {
        let mut l = Ledger::default();
        assert_eq!(
            l.record(h("A", "/tmp/a b/x.html", 10, Source::Said)),
            Recorded::New
        );
        assert_eq!(
            l.record(h("A", "file:///tmp/a%20b/x.html", 20, Source::Said)),
            Recorded::Known
        );
        assert_eq!(l.len(), 1);
        assert_eq!(
            l.newest().unwrap().at_ms,
            10,
            "the first sighting's time stays"
        );
    }

    #[test]
    fn a_said_line_the_agent_then_declares_is_upgraded_in_place() {
        let mut l = Ledger::default();
        l.record(h("x.html", "/tmp/x.html", 10, Source::Said));
        assert_eq!(
            l.record(h("The real name", "/tmp/x.html", 12, Source::Declared)),
            Recorded::Upgraded
        );
        let n = l.newest().unwrap();
        assert_eq!(
            (n.label.as_str(), n.source, n.at_ms),
            ("The real name", Source::Declared, 10)
        );
        // A weaker claim afterwards changes nothing.
        assert_eq!(
            l.record(h("other", "/tmp/x.html", 30, Source::Said)),
            Recorded::Known
        );
        assert_eq!(l.newest().unwrap().label, "The real name");
    }

    #[test]
    fn newest_first_orders_by_handover_time_not_by_arrival() {
        let mut l = Ledger::default();
        l.record(h("old", "/tmp/old.html", 100, Source::Declared));
        l.record(h("new", "/tmp/new.html", 300, Source::Declared));
        l.record(h("middle", "/tmp/mid.html", 200, Source::Declared));
        let got: Vec<&str> = l.newest_first().iter().map(|h| h.label.as_str()).collect();
        assert_eq!(got, vec!["new", "middle", "old"]);
    }

    #[test]
    fn opened_stays_opened_and_only_marks_what_is_listed() {
        let mut l = Ledger::default();
        l.record(h("a", "/tmp/a.html", 1, Source::Declared));
        assert_eq!(l.unopened(), 1);
        assert!(!l.mark_opened("/tmp/nope.html", 5), "not in the list");
        assert!(l.mark_opened("/tmp/a.html", 5));
        assert!(
            !l.mark_opened("/tmp/a.html", 9),
            "a second open changes nothing"
        );
        assert!(l.is_opened("/tmp/a.html"));
        assert_eq!(l.unopened(), 0);
    }

    #[test]
    fn a_filed_handover_reads_back_as_itself() {
        let one = h(
            "The File Drop",
            "/tmp/r/the-file-drop.html",
            1_000,
            Source::Said,
        );
        let doc = surface_doc(&one);
        assert_eq!(from_surface(&doc, 1_000), Some(one.clone()));
        assert_eq!(
            doc["id"].as_str().unwrap(),
            surface_id(&key_of("file:///tmp/r/the-file-drop.html")),
            "the id comes from the file, not from how it was spelled"
        );
        // And the bench parses it as an ordinary artifact.
        let post = crate::surface::parse_lenient(&doc, 1_000, "");
        assert!(matches!(
            post.surface.unwrap().kind,
            crate::surface::Kind::Artifact(_)
        ));
    }

    #[test]
    fn an_artifact_the_agent_presented_reads_as_presented() {
        let doc = json!({
            "td": "0.4", "kind": "artifact", "id": "plan", "title": "Plan",
            "model": { "href": "/tmp/plan.html", "summary": "four decisions" }
        });
        let got = from_surface(&doc, 7).unwrap();
        assert_eq!(
            (got.source, got.label.as_str()),
            (Source::Presented, "Plan")
        );
        let response =
            json!({ "td": "0.4", "kind": "response", "title": "t", "model": { "layman": "x" } });
        assert_eq!(from_surface(&response, 7), None);
    }

    #[test]
    fn the_surface_id_is_stable_and_names_the_feature() {
        assert_eq!(surface_id("/tmp/a.html"), surface_id("/tmp/a.html"));
        assert_ne!(surface_id("/tmp/a.html"), surface_id("/tmp/b.html"));
        assert!(surface_id("/tmp/a.html").starts_with("handover-"));
        // The parser keeps it as the id rather than minting one of its own.
        let doc = surface_doc(&h("a", "/tmp/a.html", 1, Source::Said));
        let post = crate::surface::parse_lenient(&doc, 1, "");
        assert_eq!(post.id.0, surface_id("/tmp/a.html"));
    }

    fn clock(day: u32, hour: u32, minute: u32) -> Clock {
        Clock {
            year: 2026,
            month: 10,
            day,
            hour,
            minute,
            weekday: (day + 3) % 7,
        }
    }

    fn row(
        label: &str,
        href: &str,
        when: Option<Clock>,
        source: Source,
        opened: bool,
        fact: FileFact,
    ) -> Row {
        Row {
            label: label.into(),
            href: href.into(),
            kind: kind_of(href),
            source,
            opened,
            when,
            fact,
        }
    }

    #[test]
    fn the_page_groups_by_day_and_says_how_each_was_handed_over() {
        let rows = vec![
            row(
                "The symphony, curved glass (review cut)",
                "/tmp/s/glass.mp4",
                Some(clock(6, 15, 31)),
                Source::Declared,
                false,
                FileFact::Same,
            ),
            row(
                "Movement IV's final score",
                "/tmp/s/long-IV.wav",
                Some(clock(5, 16, 9)),
                Source::Said,
                true,
                FileFact::Same,
            ),
            row(
                "the four voicings",
                "/tmp/s/README.md",
                Some(clock(4, 13, 43)),
                Source::Said,
                true,
                FileFact::Gone,
            ),
        ];
        let page = history_markdown(
            "Cinema Delight",
            &rows,
            Some((2026, 10, 6)),
            Some((2026, 10, 5)),
        );
        let today = page.find("## Today").expect(&page);
        let yesterday = page.find("## Yesterday").expect(&page);
        let older = page.find("## ").and(page.find("4 Oct")).expect(&page);
        assert!(today < yesterday && yesterday < older, "{page}");
        assert!(
            page.contains(
                "- **[The symphony, curved glass (review cut)](file:///tmp/s/glass.mp4)**"
            ),
            "{page}"
        );
        assert!(
            page.contains("**new** · `mp4` · 15:31 · declared"),
            "{page}"
        );
        assert!(page.contains("\n  `wav` · 16:09 · said"), "{page}");
        assert!(page.contains("**file gone**"), "{page}");
        assert!(
            page.contains("3 artifacts, newest first · **1 new**"),
            "{page}"
        );
    }

    #[test]
    fn a_link_with_spaces_and_brackets_survives_the_markdown() {
        let page = history_markdown(
            "p",
            &[row(
                "x",
                "/tmp/a dir/(v2) #1.html",
                Some(clock(6, 1, 2)),
                Source::Said,
                false,
                FileFact::Same,
            )],
            None,
            None,
        );
        let target = "file:///tmp/a%20dir/%28v2%29%20%231.html";
        assert!(page.contains(target), "{page}");
        // And the document view reads it back to the same file.
        match crate::docview::resolve_link(Path::new("/"), target) {
            crate::docview::LinkTarget::File { path, .. } => {
                assert_eq!(path, PathBuf::from("/tmp/a dir/(v2) #1.html"))
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_handover_whose_time_is_unknown_says_so_and_is_never_dated_1970() {
        let page = history_markdown(
            "p",
            &[row(
                "x",
                "/tmp/x.html",
                None,
                Source::Declared,
                false,
                FileFact::Unknown,
            )],
            Some((2026, 10, 6)),
            None,
        );
        assert!(page.contains("## Time unavailable"), "{page}");
        assert!(page.contains("time unavailable · declared"), "{page}");
        assert!(!page.contains("1970"), "{page}");
    }

    #[test]
    fn an_empty_page_says_how_things_arrive_rather_than_drawing_nothing() {
        let page = history_markdown("Terminal delight", &[], None, None);
        assert!(page.contains("has not handed anything over"), "{page}");
        assert!(page.contains("Deliverable:"), "{page}");
    }

    #[test]
    fn markdown_in_a_label_is_drawn_as_written() {
        let page = history_markdown(
            "p",
            &[row(
                "IV: *arpeggios* [draft]",
                "/tmp/x.html",
                Some(clock(6, 1, 2)),
                Source::Said,
                true,
                FileFact::Same,
            )],
            None,
            None,
        );
        assert!(page.contains("IV: \\*arpeggios\\* \\[draft\\]"), "{page}");
    }

    #[test]
    fn a_rewrite_is_a_fact_on_the_row_and_a_minute_of_grace_is_not_one() {
        let one = h("a", "/tmp/a.html", 1_000_000, Source::Declared);
        assert_eq!(
            file_fact(&one, Some(Some(1_000_000 + 30_000))),
            FileFact::Same
        );
        assert_eq!(
            file_fact(&one, Some(Some(1_000_000 + 600_000))),
            FileFact::Rewritten(1_600_000)
        );
        assert_eq!(file_fact(&one, Some(None)), FileFact::Gone);
        assert_eq!(file_fact(&one, None), FileFact::Unknown);
        let web = h("pr", "https://github.com/x/y/pull/1", 1, Source::Declared);
        assert_eq!(
            file_fact(&web, Some(None)),
            FileFact::Unknown,
            "a web page is not a file"
        );
    }

    #[test]
    fn kinds_come_from_the_extension_and_a_web_page_is_web() {
        assert_eq!(kind_of("/tmp/a/b.HTML"), "html");
        assert_eq!(kind_of("file:///tmp/a/b.mp4"), "mp4");
        assert_eq!(kind_of("/tmp/a/Makefile"), "file");
        assert_eq!(kind_of("https://example.com/x.html"), "web");
    }

    #[test]
    fn opened_marks_survive_a_round_trip_and_a_torn_line() {
        let dir = std::env::temp_dir().join(format!("td-handover-{}", std::process::id()));
        let path = dir.join("root.opened.jsonl");
        let _ = std::fs::remove_file(&path);
        append_opened(&path, "/tmp/a.html", 5).unwrap();
        append_opened(&path, "/tmp/b.html", 6).unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, b"{\"key\": \"/tmp/c"))
            .unwrap();
        let got = read_opened(&path);
        assert_eq!(got.len(), 2);
        assert_eq!(got.get("/tmp/a.html"), Some(&5));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_conversation_name_that_is_not_a_safe_file_name_gets_no_page() {
        assert!(history_path(Some("../../etc/passwd"), "1", 3).is_none());
        assert!(opened_path("a/b").is_none());
        let p = history_path(Some("9fc192a8-e8cc-42a0-ba4f-e5afb310ee35"), "1", 3).unwrap();
        assert!(p.ends_with("9fc192a8-e8cc-42a0-ba4f-e5afb310ee35/handed-over.md"));
        let unbound = history_path(None, "td/x", 3).unwrap();
        assert!(
            unbound.ends_with("pane-td-x-3/handed-over.md"),
            "{unbound:?}"
        );
    }

    #[test]
    fn the_page_is_written_only_when_it_changes() {
        let dir = std::env::temp_dir().join(format!("td-handover-w-{}", std::process::id()));
        let path = dir.join("x.md");
        assert!(write_if_changed(&path, "one").unwrap());
        assert!(!write_if_changed(&path, "one").unwrap());
        assert!(write_if_changed(&path, "two").unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
