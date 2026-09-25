//! A decision brief's notes, read from its bytes: where the notes live in the
//! file, what they say, and the map the brief's own notes.js would build from
//! them.
//!
//! # One format, two writers
//!
//! A brief keeps its reader's notes inside its own HTML file, in two JSON
//! islands — `<script id="report-notes">` for the notes and
//! `<script id="report-concurs">` for the CONCUR stamps — and a plain-text
//! `READER NOTES` comment that mirrors them for grep. The brief's own notes.js
//! writes that format when a reader saves in a browser; TD reads it here and,
//! from the notes-write slice on, writes it too. What both must do is pinned
//! byte for byte by the skill's shared fixtures, which TD vendors under
//! `tests/fixtures/decision-brief/notes-format/`, and by the skill's reference
//! writer (`writer.mjs` there), which every rule below follows.
//!
//! # Bytes, never a decoded page
//!
//! Markers are matched as ASCII in the raw bytes, the way the reference writer
//! matches them in a latin1 view of the file, so an offset here is an offset
//! in the file and nothing outside the islands is ever decoded. The island is
//! found by the pattern the reference writer uses, character for character; it
//! picked the same element as `document.getElementById` in all 94 islands of
//! Parker's corpus. Only an island's own text is decoded, as UTF-8.
//!
//! # What a browser shows, not merely what the file holds
//!
//! TD shows a brief's notes where a browser opened fresh would show them, so a
//! file whose island sits after its notes script — which notes.js reads before
//! the island is parsed, so no browser ever shows it — reads as notes nobody
//! can see, not as notes. A file with no island at all is read-only, which is
//! not the same as a file with no notes, and the type says so.
//!
//! No `crate::` paths, so the engine tests can compile this file on its own.

use std::ops::Range;

const NOTES_ID: &[u8] = b"report-notes";
const CONCURS_ID: &[u8] = b"report-concurs";

// ── the bytes ───────────────────────────────────────────────────────────────

/// One `<script … id="report-…">` element as found in the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Island {
    /// The `<` of its open tag.
    pub tag_start: usize,
    /// Where its text starts: one past the open tag's `>`.
    pub start: usize,
    /// Where its text ends: the `<` of the first `</script` after it.
    pub end: usize,
    /// One past the `>` that closes `</script`. `None` when the file ends
    /// before one: a torn file, which nothing may write into.
    pub close_end: Option<usize>,
}

impl Island {
    pub fn open_tag<'a>(&self, bytes: &'a [u8]) -> &'a [u8] {
        &bytes[self.tag_start..self.start]
    }

    pub fn text<'a>(&self, bytes: &'a [u8]) -> &'a [u8] {
        &bytes[self.start..self.end]
    }
}

/// Where a brief keeps its notes.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Regions {
    /// The FIRST notes island: the element `getElementById` returns.
    pub notes: Option<Island>,
    /// How many open tags the finder's pattern matches. One brief in the
    /// corpus has two, the markup pasted twice; only the first is ever read.
    pub notes_count: usize,
    /// The first concurs island.
    pub concurs: Option<Island>,
    /// The inline script that is notes.js: the one holding both
    /// `function tag()` and `reader notes`, from its `<script` to its
    /// `</script`.
    pub notes_js: Option<Range<usize>>,
}

impl Regions {
    /// The notes island opens after notes.js starts: notes.js reads the island
    /// at the top of its script, before the parser has reached it, so a
    /// browser never shows what it holds.
    pub fn island_after_script(&self) -> bool {
        match (&self.notes, &self.notes_js) {
            (Some(island), Some(js)) => island.tag_start > js.start,
            _ => false,
        }
    }
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// JavaScript's `\s` over a latin1 view: ASCII whitespace, the vertical tab,
/// and U+00A0, which a latin1 view shows for a lone 0xA0 byte.
fn is_js_space(b: u8) -> bool {
    matches!(b, 0x09..=0x0d | b' ' | 0xa0)
}

fn starts_with_ci(hay: &[u8], at: usize, needle: &[u8]) -> bool {
    hay.get(at..at + needle.len())
        .is_some_and(|s| s.eq_ignore_ascii_case(needle))
}

fn find_ci(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    (from..hay.len().saturating_sub(needle.len() - 1)).find(|&i| starts_with_ci(hay, i, needle))
}

fn rfind_ci(hay: &[u8], needle: &[u8], at_or_before: usize) -> Option<usize> {
    let last = hay.len().checked_sub(needle.len())?;
    (0..=at_or_before.min(last))
        .rev()
        .find(|&i| starts_with_ci(hay, i, needle))
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from > hay.len() {
        return None;
    }
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| i + from)
}

/// `\bid\s*=\s*["']?<id>` at `q`, inside a tag that ends at `gt`.
fn id_at(b: &[u8], q: usize, gt: usize, id: &[u8]) -> bool {
    if q == 0 || is_word(b[q - 1]) || !starts_with_ci(b, q, b"id") {
        return false;
    }
    let mut i = q + 2;
    while i < gt && is_js_space(b[i]) {
        i += 1;
    }
    if i >= gt || b[i] != b'=' {
        return false;
    }
    i += 1;
    while i < gt && is_js_space(b[i]) {
        i += 1;
    }
    if i < gt
        && (b[i] == b'"' || b[i] == b'\'')
        && starts_with_ci(b, i + 1, id)
        && i + 1 + id.len() <= gt
    {
        return true;
    }
    starts_with_ci(b, i, id) && i + id.len() <= gt
}

/// Every open tag `<script\b[^>]*\bid\s*=\s*["']?<id>["']?[^>]*>` matches, as
/// `(tag_start, one past its >)`, leftmost first and never overlapping —
/// the reference writer's pattern, ASCII case-insensitive.
fn open_tags(b: &[u8], id: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut p = 0;
    while let Some(at) = find_ci(b, b"<script", p) {
        let after = at + b"<script".len();
        let bounded = b.get(after).is_some_and(|&c| !is_word(c));
        let gt = find(b, b">", after);
        match (bounded, gt) {
            (true, Some(gt)) if (after..gt).any(|q| id_at(b, q, gt, id)) => {
                out.push((at, gt + 1));
                p = gt + 1;
            }
            _ => p = at + 1,
        }
    }
    out
}

fn find_island(b: &[u8], id: &[u8]) -> (Option<Island>, usize) {
    let tags = open_tags(b, id);
    let first = tags.first().and_then(|&(tag_start, start)| {
        let end = find_ci(b, b"</script", start)?;
        let close_end = find(b, b">", end).map(|g| g + 1);
        Some(Island {
            tag_start,
            start,
            end,
            close_end,
        })
    });
    (first, tags.len())
}

fn find_notes_js(b: &[u8]) -> Option<Range<usize>> {
    let mut from = 0;
    while let Some(at) = find(b, b"function tag()", from) {
        from = at + 1;
        let (Some(open), Some(close)) = (rfind_ci(b, b"<script", at), find_ci(b, b"</script", at))
        else {
            continue;
        };
        if find(&b[open..close], b"reader notes", 0).is_some() {
            return Some(open..close);
        }
    }
    None
}

/// Where the notes, the concurs and notes.js sit in a file.
pub fn find_regions(bytes: &[u8]) -> Regions {
    let (notes, notes_count) = find_island(bytes, NOTES_ID);
    let (concurs, _) = find_island(bytes, CONCURS_ID);
    Regions {
        notes,
        notes_count,
        concurs,
        notes_js: find_notes_js(bytes),
    }
}

/// An attribute's value in an open tag, the first `\s<name>\s*=\s*` followed
/// by a double-quoted, single-quoted or bare value, ASCII case-insensitive on
/// the name. Bytes are read as latin1, as the reference writer reads them.
pub fn attr(open_tag: &[u8], name: &str) -> Option<String> {
    attr_span(open_tag, name).map(|(_, value)| value.iter().map(|&b| b as char).collect())
}

/// The span `\s<name>\s*=\s*<value>` covers from the name, and the value.
fn attr_span<'a>(tag: &'a [u8], name: &str) -> Option<(Range<usize>, &'a [u8])> {
    let name = name.as_bytes();
    for s in 0..tag.len() {
        if !is_js_space(tag[s]) || !starts_with_ci(tag, s + 1, name) {
            continue;
        }
        let mut i = s + 1 + name.len();
        while i < tag.len() && is_js_space(tag[i]) {
            i += 1;
        }
        if tag.get(i) != Some(&b'=') {
            continue;
        }
        i += 1;
        while i < tag.len() && is_js_space(tag[i]) {
            i += 1;
        }
        let v0 = i;
        match tag.get(i) {
            Some(&q @ (b'"' | b'\'')) => {
                if let Some(close) = find(tag, &[q], i + 1) {
                    return Some((s + 1..close + 1, &tag[v0 + 1..close]));
                }
            }
            Some(_) => {
                let end = (i..tag.len())
                    .find(|&j| is_js_space(tag[j]) || matches!(tag[j], b'"' | b'\'' | b'>'))
                    .unwrap_or(tag.len());
                if end > i {
                    return Some((s + 1..end, &tag[v0..end]));
                }
            }
            None => {}
        }
    }
    None
}

// ── JSON, the way a browser parses and prints it ────────────────────────────

/// A JSON value as JavaScript holds one: numbers are doubles, and an
/// object's keys keep the order `JSON.parse` gave them.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Obj),
}

/// An object's properties in the order they were created. JavaScript lists
/// array-index keys (`"0"`, `"12"`) first, ascending, whatever the order they
/// were made in; [`Obj::iter`] does the same.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Obj(Vec<(String, Json)>);

/// A canonical array index: what JavaScript orders ahead of every other key.
fn array_index(k: &str) -> Option<u32> {
    let n: u32 = k.parse().ok()?;
    (n != u32::MAX && n.to_string() == k).then_some(n)
}

impl Obj {
    pub fn get(&self, k: &str) -> Option<&Json> {
        self.0.iter().find(|(key, _)| key == k).map(|(_, v)| v)
    }

    /// Set a property: replaced where it is, or created last.
    pub fn insert(&mut self, k: String, v: Json) {
        match self.0.iter_mut().find(|(key, _)| *key == k) {
            Some(slot) => slot.1 = v,
            None => self.0.push((k, v)),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    /// In JavaScript's property order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Json)> {
        let mut idx: Vec<(u32, &(String, Json))> = self
            .0
            .iter()
            .filter_map(|p| array_index(&p.0).map(|n| (n, p)))
            .collect();
        idx.sort_by_key(|(n, _)| *n);
        idx.into_iter()
            .map(|(_, p)| p)
            .chain(self.0.iter().filter(|p| array_index(&p.0).is_none()))
            .map(|(k, v)| (k.as_str(), v))
    }
}

/// What a JSON text could not be read as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unreadable(pub String);

impl std::fmt::Display for Unreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    depth: usize,
}

/// Deeper than any note map, shallower than the stack.
const MAX_DEPTH: usize = 256;

impl Parser<'_> {
    fn fail<T>(&self, what: &str) -> Result<T, Unreadable> {
        Err(Unreadable(format!("{what} at byte {}", self.i)))
    }

    fn ws(&mut self) {
        while self
            .s
            .get(self.i)
            .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
        {
            self.i += 1;
        }
    }

    fn eat(&mut self, lit: &[u8]) -> bool {
        if self.s[self.i..].starts_with(lit) {
            self.i += lit.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Result<Json, Unreadable> {
        self.ws();
        match self.s.get(self.i) {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(Json::Str),
            Some(b't') if self.eat(b"true") => Ok(Json::Bool(true)),
            Some(b'f') if self.eat(b"false") => Ok(Json::Bool(false)),
            Some(b'n') if self.eat(b"null") => Ok(Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => self.fail("unexpected character"),
            None => self.fail("unexpected end"),
        }
    }

    fn nest(&mut self) -> Result<(), Unreadable> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return self.fail("nested too deep");
        }
        Ok(())
    }

    fn object(&mut self) -> Result<Json, Unreadable> {
        self.nest()?;
        self.i += 1;
        let mut obj = Obj::default();
        self.ws();
        if self.eat(b"}") {
            self.depth -= 1;
            return Ok(Json::Obj(obj));
        }
        loop {
            self.ws();
            if self.s.get(self.i) != Some(&b'"') {
                return self.fail("expected a key");
            }
            let k = self.string()?;
            self.ws();
            if !self.eat(b":") {
                return self.fail("expected ':'");
            }
            let v = self.value()?;
            obj.insert(k, v);
            self.ws();
            if self.eat(b",") {
                continue;
            }
            if self.eat(b"}") {
                self.depth -= 1;
                return Ok(Json::Obj(obj));
            }
            return self.fail("expected ',' or '}'");
        }
    }

    fn array(&mut self) -> Result<Json, Unreadable> {
        self.nest()?;
        self.i += 1;
        let mut out = Vec::new();
        self.ws();
        if self.eat(b"]") {
            self.depth -= 1;
            return Ok(Json::Arr(out));
        }
        loop {
            out.push(self.value()?);
            self.ws();
            if self.eat(b",") {
                continue;
            }
            if self.eat(b"]") {
                self.depth -= 1;
                return Ok(Json::Arr(out));
            }
            return self.fail("expected ',' or ']'");
        }
    }

    fn hex4(&mut self) -> Result<u32, Unreadable> {
        let h = self
            .s
            .get(self.i..self.i + 4)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u32::from_str_radix(h, 16).ok());
        match h {
            Some(v) => {
                self.i += 4;
                Ok(v)
            }
            None => self.fail("bad \\u escape"),
        }
    }

    fn string(&mut self) -> Result<String, Unreadable> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let run = self.i;
            while self
                .s
                .get(self.i)
                .is_some_and(|&b| b != b'"' && b != b'\\' && b >= 0x20)
            {
                self.i += 1;
            }
            match std::str::from_utf8(&self.s[run..self.i]) {
                Ok(t) => out.push_str(t),
                Err(_) => return self.fail("not UTF-8"),
            }
            match self.s.get(self.i) {
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.i += 1;
                    let esc = self.s.get(self.i).copied();
                    self.i += 1;
                    match esc {
                        Some(b'"') => out.push('"'),
                        Some(b'\\') => out.push('\\'),
                        Some(b'/') => out.push('/'),
                        Some(b'b') => out.push('\u{8}'),
                        Some(b'f') => out.push('\u{c}'),
                        Some(b'n') => out.push('\n'),
                        Some(b'r') => out.push('\r'),
                        Some(b't') => out.push('\t'),
                        Some(b'u') => {
                            let hi = self.hex4()?;
                            let c = if (0xd800..0xdc00).contains(&hi) {
                                // A lone half of a pair is a string JavaScript
                                // can hold and Rust cannot: unreadable here.
                                if !self.eat(b"\\u") {
                                    return self.fail("a lone surrogate");
                                }
                                let lo = self.hex4()?;
                                if !(0xdc00..0xe000).contains(&lo) {
                                    return self.fail("a lone surrogate");
                                }
                                0x10000 + ((hi - 0xd800) << 10) + (lo - 0xdc00)
                            } else {
                                hi
                            };
                            match char::from_u32(c) {
                                Some(c) => out.push(c),
                                None => return self.fail("a lone surrogate"),
                            }
                        }
                        _ => return self.fail("bad escape"),
                    }
                }
                Some(_) => return self.fail("a control character in a string"),
                None => return self.fail("an unterminated string"),
            }
        }
    }

    fn number(&mut self) -> Result<Json, Unreadable> {
        let start = self.i;
        self.eat(b"-");
        let digits = |p: &mut Self| {
            let d = p.i;
            while p.s.get(p.i).is_some_and(u8::is_ascii_digit) {
                p.i += 1;
            }
            p.i - d
        };
        if self.s.get(self.i) == Some(&b'0') {
            self.i += 1;
        } else if digits(self) == 0 {
            return self.fail("bad number");
        }
        if self.eat(b".") && digits(self) == 0 {
            return self.fail("bad number");
        }
        if matches!(self.s.get(self.i), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.s.get(self.i), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if digits(self) == 0 {
                return self.fail("bad number");
            }
        }
        let text = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("x");
        match text.parse::<f64>() {
            Ok(v) if v.is_finite() => Ok(Json::Num(v)),
            _ => self.fail("a number out of range"),
        }
    }
}

/// `JSON.parse`, strictly: a JSON text and nothing after it.
pub fn parse_json(text: &[u8]) -> Result<Json, Unreadable> {
    let mut p = Parser {
        s: text,
        i: 0,
        depth: 0,
    };
    let v = p.value()?;
    p.ws();
    if p.i != text.len() {
        return p.fail("something after the JSON");
    }
    Ok(v)
}

/// JavaScript's `String.prototype.trim` whitespace, over UTF-8 text.
fn js_trim(s: &str) -> &str {
    let ws = |c: char| {
        matches!(
            c,
            '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
                ..='\u{200a}'
                    | '\u{2028}'
                    | '\u{2029}'
                    | '\u{202f}'
                    | '\u{205f}'
                    | '\u{3000}'
                    | '\u{feff}'
        )
    };
    s.trim_matches(ws)
}

// ── the maps ────────────────────────────────────────────────────────────────

/// One note as the brief stores it. Fields other than these three ride along
/// untouched in the object they came in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteRef<'a> {
    pub text: &'a str,
    pub title: Option<&'a str>,
    pub ts: Option<&'a str>,
}

/// `{ "<anchor>": [ { "text", "title", "ts", … }, … ] }`, as the island holds
/// it, in the island's own key order.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct NoteMap(pub Obj);

impl NoteMap {
    /// The notes on one anchor, oldest first.
    pub fn on(&self, nid: &str) -> Vec<NoteRef<'_>> {
        let Some(Json::Arr(list)) = self.0.get(nid) else {
            return Vec::new();
        };
        list.iter()
            .filter_map(|n| match n {
                Json::Obj(o) => {
                    let s = |k: &str| match o.get(k) {
                        Some(Json::Str(s)) => Some(s.as_str()),
                        _ => None,
                    };
                    Some(NoteRef {
                        text: s("text")?,
                        title: s("title"),
                        ts: s("ts"),
                    })
                }
                _ => None,
            })
            .collect()
    }

    /// Every note under every key, anchored or not: the map's header count.
    pub fn count(&self) -> usize {
        self.0
            .iter()
            .map(|(_, v)| match v {
                Json::Arr(a) => a.len(),
                _ => 0,
            })
            .sum()
    }

    /// Every key: the map header's "elements", orphans included.
    pub fn elements(&self) -> usize {
        self.0.len()
    }
}

/// `{ "<anchor>": "YYYY-MM-DD HH:MM" }`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ConcurMap(pub Obj);

impl ConcurMap {
    pub fn has(&self, nid: &str) -> bool {
        self.0.get(nid).is_some()
    }

    pub fn count(&self) -> usize {
        self.0.len()
    }
}

/// An island's text as notes.js and the reference writer read it:
/// `JSON.parse(text.trim() || '{}')`, and only a map of the right shape.
fn island_json(bytes: &[u8], island: &Island, what: &str) -> Result<Obj, Unreadable> {
    let text = std::str::from_utf8(island.text(bytes))
        .map_err(|_| Unreadable(format!("the {what} island is not UTF-8")))?;
    let text = js_trim(text);
    let value = if text.is_empty() {
        Json::Obj(Obj::default())
    } else {
        parse_json(text.as_bytes())
            .map_err(|e| Unreadable(format!("the {what} island is not JSON: {e}")))?
    };
    match value {
        Json::Obj(o) => Ok(o),
        _ => Err(Unreadable(format!("the {what} island is not a map"))),
    }
}

fn notes_of(bytes: &[u8], island: &Island) -> Result<NoteMap, Unreadable> {
    let o = island_json(bytes, island, "notes")?;
    let shaped = o.iter().all(|(_, v)| match v {
        Json::Arr(list) => list.iter().all(|n| match n {
            Json::Obj(n) => matches!(n.get("text"), Some(Json::Str(_))),
            _ => false,
        }),
        _ => false,
    });
    if !shaped {
        return Err(Unreadable(
            "the notes island is not a map of the expected shape".into(),
        ));
    }
    Ok(NoteMap(o))
}

fn concurs_of(bytes: &[u8], island: &Island) -> Result<ConcurMap, Unreadable> {
    let o = island_json(bytes, island, "concurs")?;
    if !o.iter().all(|(_, v)| matches!(v, Json::Str(_))) {
        return Err(Unreadable(
            "the concurs island is not a map of the expected shape".into(),
        ));
    }
    Ok(ConcurMap(o))
}

/// Everything a reader of the file can know without a browser.
#[derive(Clone, Debug, PartialEq)]
pub struct NotesRead {
    pub regions: Regions,
    /// `None`: the file has no notes island. Read-only, which is not empty.
    pub notes: Option<Result<NoteMap, Unreadable>>,
    /// Empty when the file has no concurs island.
    pub concurs: Result<ConcurMap, Unreadable>,
    /// The notes island's `data-format`: `None` for a brief from before
    /// format 1, which is a real answer and not a default.
    pub format: Option<String>,
    /// The notes island's `data-rev`, the time of its last format-1 write.
    pub rev: Option<String>,
}

/// Read a brief's notes from its bytes. Writes nothing and decodes nothing
/// outside the islands.
pub fn read(bytes: &[u8]) -> NotesRead {
    let regions = find_regions(bytes);
    let notes = regions.notes.map(|i| notes_of(bytes, &i));
    let concurs = match &regions.concurs {
        Some(i) => concurs_of(bytes, i),
        None => Ok(ConcurMap::default()),
    };
    let tag = regions.notes.map(|i| i.open_tag(bytes).to_vec());
    let format = tag.as_deref().and_then(|t| attr(t, "data-format"));
    let rev = tag.as_deref().and_then(|t| attr(t, "data-rev"));
    NotesRead {
        regions,
        notes,
        concurs,
        format,
        rev,
    }
}

/// The island's text as a browser's `textContent` gives it, for comparing
/// with what the fixtures recorded. `None` with no island.
#[cfg(test)]
pub fn notes_text(bytes: &[u8], regions: &Regions) -> Option<String> {
    regions
        .notes
        .map(|i| String::from_utf8_lossy(i.text(bytes)).into_owned())
}

// ── the map notes.js builds ─────────────────────────────────────────────────

/// notes.js `buildMap()`, unchanged since the concur release: a header that
/// counts every key, orphans included, then one block per anchor in document
/// order that has a note or a concur, each note on one line.
///
/// `anchors` is `(nid, title)` in document order, from the brief's own
/// notes.js. `file_label` is the page's `NOTES_FILE`, never TD's path. Pass
/// an empty `concurs` for a brief whose notes.js predates concurs.
pub fn build_map(
    file_label: &str,
    notes: &NoteMap,
    concurs: &ConcurMap,
    anchors: &[(&str, &str)],
) -> String {
    let (count, els, cc) = (notes.count(), notes.elements(), concurs.count());
    let mut out = vec![
        format!("NOTES — {file_label}"),
        format!(
            "{count} notes on {els} elements{}.",
            match cc {
                0 => String::new(),
                1 => " · 1 concur".into(),
                n => format!(" · {n} concurs"),
            }
        ),
        "Each [anchor] is an element id in that file — search it to find the passage.".into(),
        String::new(),
    ];
    for &(nid, title) in anchors {
        let list = notes.on(nid);
        let agreed = concurs.has(nid);
        if list.is_empty() && !agreed {
            continue;
        }
        out.push(format!(
            "[{nid}] {title}{}",
            if agreed { "  ✓ concur" } else { "" }
        ));
        for n in list {
            out.push(format!("  · {}", one_line(n.text)));
        }
        out.push(String::new());
    }
    out.join("\n")
}

/// `text.replace(/\n+/g, ' ')`: each run of line feeds one space. A carriage
/// return stays, as it does in the browser.
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

// ── the concur stamp's pose ─────────────────────────────────────────────────

/// notes.js `seed()`: FNV-1a over UTF-16 code units, scaled to 0..=1.
pub fn stamp_seed(s: &str) -> f64 {
    let mut h: u32 = 2_166_136_261;
    for unit in s.encode_utf16() {
        h ^= u32::from(unit);
        h = h.wrapping_mul(16_777_619);
    }
    f64::from(h) / 4_294_967_295.0
}

/// Where and how a CONCUR stamp lands on a decision, in CSS terms: rotated by
/// `degrees` about its centre and moved by `(dx, dy)` CSS px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StampPose {
    pub degrees: f32,
    pub dx: f32,
    pub dy: f32,
}

/// `toFixed(1)`, which is what the browser's stylesheet is given: the half
/// rounds away from zero.
fn fixed1(x: f64) -> f32 {
    let r = (x.abs() * 10.0).round() / 10.0;
    (if x < 0.0 { -r } else { r }) as f32
}

/// notes.js `stampSvg()`: `-12 + 9·seed(id)` degrees, and
/// `(seed(id + ':x') − ½)·6`, `(seed(id + ':y') − ½)·6` px, each to one
/// decimal as the page writes them into its style. Seeded from the anchor, so
/// a stamp lands the same way on every reload and in every program.
pub fn stamp_pose(nid: &str) -> StampPose {
    StampPose {
        degrees: fixed1(-12.0 + 9.0 * stamp_seed(nid)),
        dx: fixed1((stamp_seed(&format!("{nid}:x")) - 0.5) * 6.0),
        dy: fixed1((stamp_seed(&format!("{nid}:y")) - 0.5) * 6.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/decision-brief/notes-format")
    }

    /// Every case the skill's fixtures carry, by name, sorted.
    fn cases() -> Vec<(String, PathBuf)> {
        let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(fixtures().join("cases"))
            .expect("the vendored fixtures")
            .flatten()
            .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
            .collect();
        out.sort();
        assert_eq!(out.len(), 15, "the skill's fifteen cases");
        out
    }

    fn json(path: &Path) -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    struct Anchor {
        nid: String,
        title: String,
        concurrable: bool,
        stamp: Option<(f64, f64, f64)>,
    }

    fn anchors(case: &Path) -> Vec<Anchor> {
        let v = json(&case.join("anchors.json"));
        v.as_array()
            .unwrap()
            .iter()
            .map(|a| Anchor {
                nid: a["nid"].as_str().unwrap().into(),
                title: a["title"].as_str().unwrap().into(),
                concurrable: a["concurrable"].as_bool().unwrap(),
                stamp: a["stamp"].as_object().map(|s| {
                    (
                        s["r"].as_f64().unwrap(),
                        s["dx"].as_f64().unwrap(),
                        s["dy"].as_f64().unwrap(),
                    )
                }),
            })
            .collect()
    }

    fn pairs(a: &[Anchor]) -> Vec<(&str, &str)> {
        a.iter()
            .map(|a| (a.nid.as_str(), a.title.as_str()))
            .collect()
    }

    /// The island TD reads is the island a browser reads: its text equals
    /// the `textContent` of `getElementById('report-notes')` that the skill's
    /// check recorded in a real Chromium — in `two-islands` the first of two,
    /// and in `no-island` none at all.
    #[test]
    fn the_island_is_found_where_the_browser_finds_it() {
        for (name, case) in cases() {
            let bytes = std::fs::read(case.join("brief.html")).unwrap();
            let expect = json(&case.join("expect.json"));
            let got = notes_text(&bytes, &find_regions(&bytes));
            assert_eq!(
                got.as_deref(),
                expect["notes_text"].as_str(),
                "{name}: the island's text"
            );
        }
        // The concurs island too, in the brief that carries two of each: the
        // first, which sits before the second notes island.
        let two = std::fs::read(fixtures().join("cases/two-islands/brief.html")).unwrap();
        let r = find_regions(&two);
        assert_eq!(r.notes_count, 2);
        let second_notes = open_tags(&two, NOTES_ID)[1].0;
        assert!(r.concurs.unwrap().tag_start < second_notes);
        assert_eq!(open_tags(&two, CONCURS_ID).len(), 2);
    }

    /// The map is byte for byte what the brief's own `exportNotes()` put in
    /// its text box once the written file was reopened in a browser, read
    /// here from that same written file.
    #[test]
    fn the_map_is_the_one_notes_js_builds() {
        let mut checked = 0;
        for (name, case) in cases() {
            let Ok(bytes) = std::fs::read(case.join("expected.html")) else {
                continue;
            };
            let expect = json(&case.join("expect.json"));
            let label = expect["notes_file"].as_str().unwrap();
            let supported = expect["concur_support"] == "supported";
            let read = read(&bytes);
            let notes = read.notes.expect("an island").expect("readable");
            let concurs = if supported {
                read.concurs.unwrap()
            } else {
                ConcurMap::default()
            };
            let a = anchors(&case);
            let map = build_map(label, &notes, &concurs, &pairs(&a));
            let want = std::fs::read_to_string(case.join("expected-map.txt")).unwrap();
            assert_eq!(map, want, "{name}");
            checked += 1;
        }
        assert_eq!(checked, 11, "every case that is written has a map");
    }

    #[test]
    fn a_note_whose_anchor_is_gone_is_counted_but_not_listed() {
        let case = fixtures().join("cases/orphan-note");
        let bytes = std::fs::read(case.join("brief.html")).unwrap();
        let read = read(&bytes);
        let notes = read.notes.unwrap().unwrap();
        let a = anchors(&case);
        assert!(!a.iter().any(|a| a.nid == "finding-a-passage-since-cut"));
        let map = build_map("brief.html", &notes, &read.concurs.unwrap(), &pairs(&a));
        assert!(map.contains("\n4 notes on 3 elements.\n"), "{map}");
        assert!(!map.contains("finding-a-passage-since-cut"), "{map}");
        assert!(!map.contains("This passage was cut"), "{map}");
    }

    #[test]
    fn a_concur_rides_on_its_decisions_line_even_without_a_note() {
        let case = fixtures().join("cases/concur-era-pristine");
        let bytes = std::fs::read(case.join("expected.html")).unwrap();
        let read = read(&bytes);
        let map = build_map(
            "brief.html",
            &read.notes.unwrap().unwrap(),
            &read.concurs.unwrap(),
            &pairs(&anchors(&case)),
        );
        assert!(
            map.contains(
                "\n[ask-stamp-a-revision-on] Stamp a revision on the island?  ✓ concur\n\n"
            ),
            "{map}"
        );
        assert!(map.contains("1 notes on 1 elements · 2 concurs."), "{map}");
    }

    #[test]
    fn the_map_names_the_briefs_notes_file_not_the_path() {
        let case = fixtures().join("cases/notes-file-override");
        let want = std::fs::read_to_string(case.join("expected-map.txt")).unwrap();
        assert!(
            want.starts_with("NOTES — 2026-09-09-the-original-brief.html\n"),
            "{want}"
        );
        let bytes = std::fs::read(case.join("expected.html")).unwrap();
        let read = read(&bytes);
        let map = build_map(
            "2026-09-09-the-original-brief.html",
            &read.notes.unwrap().unwrap(),
            &read.concurs.unwrap(),
            &pairs(&anchors(&case)),
        );
        assert_eq!(map, want);
    }

    /// The stamp's angle and offset are seeded from the anchor exactly as
    /// notes.js seeds them, so TD's stamp lands at the browser's angle.
    #[test]
    fn a_concur_stamp_lands_at_the_angle_the_browser_gives_it() {
        let mut seen = 0;
        for (name, case) in cases() {
            for a in anchors(&case) {
                let Some((r, dx, dy)) = a.stamp else {
                    assert!(!a.concurrable, "{name}: {} takes a stamp", a.nid);
                    continue;
                };
                let p = stamp_pose(&a.nid);
                for (got, want, what) in
                    [(p.degrees, r, "angle"), (p.dx, dx, "dx"), (p.dy, dy, "dy")]
                {
                    assert!(
                        (f64::from(got) - want).abs() < 0.05,
                        "{name} {}: {what} {got} vs {want}",
                        a.nid
                    );
                }
                seen += 1;
            }
        }
        assert!(seen >= 20, "stamps compared: {seen}");
        // Outside the fixtures: FNV-1a over UTF-16, so a character outside
        // the basic plane counts as the two units JavaScript sees.
        assert_eq!(stamp_seed(""), 2_166_136_261.0 / 4_294_967_295.0);
        let mut h: u32 = 2_166_136_261;
        for unit in [u32::from(b'a'), 0xd83c, 0xdfaf] {
            h = (h ^ unit).wrapping_mul(16_777_619);
        }
        assert_eq!(stamp_seed("a🎯"), f64::from(h) / 4_294_967_295.0);
    }

    /// What TD draws as badges and stamps is what a browser opened fresh
    /// draws: per anchor, the count on its button; the notebar's total, which
    /// counts only anchored notes; which decisions carry a stamp; and the
    /// concur count, which exists only where the brief offers concurs.
    #[test]
    fn the_badges_are_the_ones_a_browser_shows() {
        for (name, case) in cases() {
            let expect = json(&case.join("expect.json"));
            let supported = expect["concur_support"] == "supported";
            let a = anchors(&case);
            for (file, shown) in [
                ("brief.html", "shown_before"),
                ("expected.html", "shown_after"),
            ] {
                let Ok(bytes) = std::fs::read(case.join(file)) else {
                    continue;
                };
                let shown = &expect[shown];
                let seen = shown_by_browser(&read(&bytes));
                let mut counts = serde_json::Map::new();
                let mut total = 0;
                for anchor in &a {
                    let n = seen.notes.on(&anchor.nid).len();
                    total += n;
                    if n > 0 {
                        counts.insert(anchor.nid.clone(), n.to_string().into());
                    }
                }
                assert_eq!(
                    serde_json::Value::Object(counts),
                    shown["notes"],
                    "{name} {file}: badges"
                );
                let stamps: Vec<&str> = a
                    .iter()
                    .filter(|x| x.concurrable && supported && seen.concurs.has(&x.nid))
                    .map(|x| x.nid.as_str())
                    .collect();
                let want: Vec<&str> = shown["concurs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect();
                assert_eq!(stamps, want, "{name} {file}: stamps");
                match shown["note_count"].as_str() {
                    Some(n) => assert_eq!(total.to_string(), n, "{name} {file}: the count"),
                    None => assert!(seen.none, "{name}: no island means no counter at all"),
                }
                match shown["concur_count"].as_str() {
                    Some(n) => {
                        assert!(supported, "{name}");
                        assert_eq!(seen.concurs.count().to_string(), n, "{name} {file}")
                    }
                    None => assert!(!supported, "{name}: a concur count only with concurs"),
                }
            }
        }
    }

    /// What a fresh browser would show from these bytes: nothing from an
    /// island it can never read, nothing from an unreadable one.
    struct Seen {
        notes: NoteMap,
        concurs: ConcurMap,
        none: bool,
    }

    fn shown_by_browser(r: &NotesRead) -> Seen {
        let hidden = r.regions.island_after_script();
        let notes = match &r.notes {
            Some(Ok(n)) if !hidden => n.clone(),
            _ => NoteMap::default(),
        };
        let concurs = match &r.concurs {
            Ok(c) if !hidden => c.clone(),
            _ => ConcurMap::default(),
        };
        Seen {
            notes,
            concurs,
            none: r.notes.is_none(),
        }
    }

    /// No island is read-only, which is a different answer from an island
    /// holding nothing — and the refusals the skill pins are all visible from
    /// the bytes alone.
    #[test]
    fn a_brief_with_no_island_is_read_only_not_empty() {
        let bytes =
            |c: &str| std::fs::read(fixtures().join("cases").join(c).join("brief.html")).unwrap();
        let none = read(&bytes("no-island"));
        assert_eq!(none.notes, None, "no island at all");
        let empty = read(&bytes("current-pristine"));
        assert_eq!(
            empty.notes,
            Some(Ok(NoteMap::default())),
            "an island holding {{}}"
        );
        assert_eq!(empty.format.as_deref(), Some("1"));
        let text_less = read(&bytes("no-body-close"));
        assert_eq!(
            text_less.notes,
            Some(Ok(NoteMap::default())),
            "no text reads as {{}}"
        );
        assert!(matches!(
            read(&bytes("unreadable-island")).notes,
            Some(Err(_))
        ));
        assert!(find_regions(&bytes("island-after-script")).island_after_script());
        assert!(!find_regions(&bytes("current-pristine")).island_after_script());
        assert_eq!(read(&bytes("future-format")).format.as_deref(), Some("2"));
        let legacy = read(&bytes("b689671-saved"));
        assert_eq!(
            (legacy.format, legacy.rev),
            (None, None),
            "undeclared, not format 1"
        );
        assert_eq!(
            read(&bytes("stale-mirrors")).rev.as_deref(),
            Some("2026-09-21T09:01:00.000Z")
        );
    }

    /// Every file under a directory, relative to it, sorted.
    fn files(root: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.push(p.strip_prefix(root).unwrap().to_path_buf());
                }
            }
        }
        out.sort();
        out
    }

    /// TD's copy of the skill's fixtures is the skill's, byte for byte, when
    /// the skill is installed on this machine. Without it, the test says it
    /// compared nothing rather than passing as if it had.
    #[test]
    fn the_vendored_fixtures_match_the_skill_when_it_is_installed() {
        use std::io::Write;
        let ours = fixtures();
        let source = std::fs::read_to_string(ours.join("SOURCE")).unwrap();
        assert!(
            source.starts_with("parker-brown-family/agent-skills "),
            "{source}"
        );
        let home = std::env::var_os("HOME").unwrap_or_default();
        let skill = Path::new(&home).join(".claude/skills/decision-brief/fixtures/notes-format");
        if !skill.is_dir() {
            let _ = writeln!(
                std::io::stderr().lock(),
                "the_vendored_fixtures_match_the_skill_when_it_is_installed: no skill at {}, so nothing was compared",
                skill.display()
            );
            return;
        }
        let mine: Vec<PathBuf> = files(&ours)
            .into_iter()
            .filter(|p| p != Path::new("SOURCE"))
            .collect();
        let theirs = files(&skill);
        assert_eq!(mine, theirs, "the same files, SOURCE aside");
        for f in &mine {
            assert!(
                std::fs::read(ours.join(f)).unwrap() == std::fs::read(skill.join(f)).unwrap(),
                "{} differs from the skill's: copy the fixtures again and name the commit in SOURCE",
                f.display()
            );
        }
    }

    /// The finder follows the reference writer's pattern, looseness and all.
    #[test]
    fn the_finder_matches_the_writers_pattern() {
        let island = |s: &str| {
            find_regions(s.as_bytes())
                .notes
                .map(|i| i.text(s.as_bytes()).to_vec())
        };
        assert_eq!(
            island("<script type=x id=\"report-notes\">{}</script>"),
            Some(b"{}".to_vec())
        );
        assert_eq!(
            island("<SCRIPT ID = 'Report-Notes' >{}</Script >"),
            Some(b"{}".to_vec())
        );
        assert_eq!(
            island("<script id=report-notes>{}</script>"),
            Some(b"{}".to_vec())
        );
        // Shared with the reference: a data-id matches too; the read-back
        // after every write is what catches it.
        assert_eq!(
            island("<script data-id=\"report-notes\">x</script>"),
            Some(b"x".to_vec())
        );
        assert_eq!(island("<scripts id=\"report-notes\">x</script>"), None);
        assert_eq!(island("<script xid=\"report-notes\">x</script>"), None);
        assert_eq!(
            island("<script id=\"other\">x</script><p id=\"report-notes\">"),
            None
        );
        // The first tag that matches, not the first <script.
        assert_eq!(
            island("<script>a</script><script id=\"report-notes\">b</script>"),
            Some(b"b".to_vec())
        );
        assert_eq!(
            attr(b"<script data-format=\"1\" data-rev='x'>", "data-format").as_deref(),
            Some("1")
        );
        assert_eq!(
            attr(b"<script data-rev='x y'>", "data-rev").as_deref(),
            Some("x y")
        );
        assert_eq!(
            attr(b"<script\tDATA-FORMAT = 2>", "data-format").as_deref(),
            Some("2")
        );
        assert_eq!(attr(b"<script xdata-format=\"1\">", "data-format"), None);
        assert_eq!(attr(b"<script>", "data-format"), None);
    }

    #[test]
    fn json_is_parsed_the_way_a_browser_parses_it() {
        let o = |s: &str| match parse_json(s.as_bytes()).unwrap() {
            Json::Obj(o) => o,
            other => panic!("{other:?}"),
        };
        let keys = |s: &str| o(s).iter().map(|(k, _)| k.to_string()).collect::<Vec<_>>();
        assert_eq!(keys(r#"{"z":1,"a":2}"#), ["z", "a"], "the file's own order");
        assert_eq!(
            keys(r#"{"b":1,"10":2,"2":3,"01":4}"#),
            ["2", "10", "b", "01"]
        );
        assert_eq!(o(r#"{"a":1,"a":2}"#).get("a"), Some(&Json::Num(2.0)));
        // Escapes spelled out with a separate backslash, so no tool between
        // this file and the compiler can turn them into the characters.
        let bs = '\\';
        let escaped = format!("\"{bs}ud83c{bs}udfaf {bs}u00e9{bs}n\"");
        assert_eq!(
            parse_json(escaped.as_bytes()).unwrap(),
            Json::Str("🎯 é\n".into())
        );
        let lone = format!("\"{bs}ud800\"");
        assert!(parse_json(lone.as_bytes()).is_err(), "a lone surrogate");
        for bad in [
            r#"{"a":1,}"#,
            r#"'a'"#,
            "\"\u{7}\"",
            r#""\ud800""#,
            "01",
            "1.",
            "{} x",
            "",
        ] {
            assert!(parse_json(bad.as_bytes()).is_err(), "{bad:?}");
        }
        assert_eq!(js_trim("\u{feff}\u{a0} {} \n"), "{}");
    }
}
