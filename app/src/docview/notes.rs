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

/// Where `\s<name>\s*=\s*<value>` matches: the value's span, quotes
/// included, and the value itself.
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
                    return Some((v0..close + 1, &tag[v0 + 1..close]));
                }
            }
            Some(_) => {
                let end = (i..tag.len())
                    .find(|&j| is_js_space(tag[j]) || matches!(tag[j], b'"' | b'\'' | b'>'))
                    .unwrap_or(tag.len());
                if end > i {
                    return Some((v0..end, &tag[v0..end]));
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

    pub fn get_mut(&mut self, k: &str) -> Option<&mut Json> {
        self.0.iter_mut().find(|(key, _)| key == k).map(|(_, v)| v)
    }

    /// Set a property: replaced where it is, or created last.
    pub fn insert(&mut self, k: String, v: Json) {
        match self.0.iter_mut().find(|(key, _)| *key == k) {
            Some(slot) => slot.1 = v,
            None => self.0.push((k, v)),
        }
    }

    /// `delete obj[k]`: gone, and the others keep their order. Made again
    /// later, it goes last.
    pub fn remove(&mut self, k: &str) {
        self.0.retain(|(key, _)| key != k);
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
fn island_map(bytes: &[u8], island: &Island, what: &str) -> Result<Obj, Unreadable> {
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
    let o = island_map(bytes, island, "notes")?;
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
    let o = island_map(bytes, island, "concurs")?;
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

// ── writing: JSON as a browser prints it ────────────────────────────────────

/// A backslash, kept apart from the letters after it wherever an escape is
/// spelled, so no tool between this file and the compiler can read the pair
/// as an escape of its own.
const BS: char = '\\';

/// `JSON.stringify(value, null, 1)`: one space of indent per level, `": "`
/// between a key and its value, empty containers as `{}` and `[]`.
pub fn stringify(v: &Json) -> String {
    let mut out = String::new();
    write_json(v, 0, &mut out);
    out
}

fn indent(depth: usize, out: &mut String) {
    out.extend(std::iter::repeat_n(' ', depth));
}

fn write_json(v: &Json, depth: usize, out: &mut String) {
    match v {
        Json::Null => out.push_str("null"),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Num(n) => out.push_str(&js_number(*n)),
        Json::Str(s) => quote(s, out),
        Json::Arr(a) if a.is_empty() => out.push_str("[]"),
        Json::Arr(a) => {
            out.push_str("[\n");
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                indent(depth + 1, out);
                write_json(x, depth + 1, out);
            }
            out.push('\n');
            indent(depth, out);
            out.push(']');
        }
        Json::Obj(o) if o.len() == 0 => out.push_str("{}"),
        Json::Obj(o) => {
            out.push_str("{\n");
            for (i, (k, x)) in o.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                indent(depth + 1, out);
                quote(k, out);
                out.push_str(": ");
                write_json(x, depth + 1, out);
            }
            out.push('\n');
            indent(depth, out);
            out.push('}');
        }
    }
}

/// JSON.stringify's string quoting: the two-character escapes it has, the
/// six-character one, lowercase, for every other control character, and
/// everything else — U+2028 and U+2029 included — as it is.
fn quote(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => {
                out.push(BS);
                out.push('"');
            }
            '\\' => {
                out.push(BS);
                out.push(BS);
            }
            '\u{8}' => out.push_str(&format!("{BS}b")),
            '\u{c}' => out.push_str(&format!("{BS}f")),
            '\n' => out.push_str(&format!("{BS}n")),
            '\r' => out.push_str(&format!("{BS}r")),
            '\t' => out.push_str(&format!("{BS}t")),
            c if (c as u32) < 0x20 => out.push_str(&format!("{BS}u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// `Number.prototype.toString()`: the shortest digits that read back as the
/// same double, in positional notation from 1e-7 up to 1e21 and in
/// exponent notation (`1e+21`, `1.5e-7`) outside it.
pub fn js_number(x: f64) -> String {
    if x == 0.0 {
        return "0".into();
    }
    if x < 0.0 {
        return format!("-{}", js_number(-x));
    }
    // Rust's shortest round-trip form, `d.ddde±n`: the digits and where the
    // point goes are exactly what the spec's algorithm starts from.
    let sci = format!("{x:e}");
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let k = digits.len() as i32;
    let n = exp.parse::<i32>().unwrap_or(0) + 1;
    if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let sign = if e < 0 { '-' } else { '+' };
        match digits.len() {
            1 => format!("{digits}e{sign}{}", e.abs()),
            _ => format!("{}.{}e{sign}{}", &digits[..1], &digits[1..], e.abs()),
        }
    }
}

/// An island's text as format 1 writes it: `JSON.stringify(map, null, 1)`,
/// then every `</` written `<\/` and every `<!` written with its `<` as the
/// JSON escape backslash-u-003c. Still JSON, the same strings when parsed,
/// and the HTML tokenizer can no longer leave the script element early or
/// enter its double-escaped state.
pub fn island_json(map: &Obj) -> String {
    stringify(&Json::Obj(map.clone()))
        .replace("</", "<\\/")
        .replace("<!", &format!("{BS}u003c!"))
}

/// The only two sequences that end an HTML comment, broken with a space:
/// `--!>` first, then `-->`. A reader's own `---` stays as written.
pub fn comment_safe(s: &str) -> String {
    s.replace("--!>", "--! >").replace("-->", "-- >")
}

/// `<!--\nREADER NOTES —\n` + the map, made safe, + `\n-->`.
pub fn mirror_comment(map: &str) -> String {
    format!("<!--\nREADER NOTES —\n{}\n-->", comment_safe(map))
}

/// The first bytes of a mirror comment, as the file holds them.
pub const MIRROR_MARK: &[u8] = "<!--\nREADER NOTES —".as_bytes();

// ── writing: times, as notes.js writes them ─────────────────────────────────

/// Days since 1970-01-01 to a proleptic Gregorian date.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// A time as `(days since the epoch, ms into that day)`, UTC.
fn utc_parts(t: std::time::SystemTime) -> (i64, i64) {
    let ms = match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(e) => -(e.duration().as_millis() as i64),
    };
    (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000))
}

/// `new Date().toISOString().slice(0, 16).replace('T', ' ')`: a note's time,
/// UTC, to the minute.
pub fn utc_minute(t: std::time::SystemTime) -> String {
    let (days, ms) = utc_parts(t);
    let (y, m, d) = civil(days);
    let min = ms / 60_000;
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", min / 60, min % 60)
}

/// `new Date().toISOString()`: a write's revision, UTC, to the millisecond.
pub fn iso_millis(t: std::time::SystemTime) -> String {
    let (days, ms) = utc_parts(t);
    let (y, m, d) = civil(days);
    let (s, milli) = (ms / 1000, ms % 1000);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{milli:03}Z",
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}

fn is_rev(rev: &str) -> bool {
    let b = rev.as_bytes();
    let digit_at = |i: &[usize]| i.iter().all(|&i| b.get(i).is_some_and(u8::is_ascii_digit));
    b.len() == 24
        && digit_at(&[0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18, 20, 21, 22])
        && [
            (4, b'-'),
            (7, b'-'),
            (10, b'T'),
            (13, b':'),
            (16, b':'),
            (19, b'.'),
            (23, b'Z'),
        ]
        .iter()
        .all(|&(i, c)| b[i] == c)
}

// ── writing: the plan ───────────────────────────────────────────────────────

/// The newest notes format TD knows. A brief declaring any other refuses
/// every write: TD never writes a format it does not know.
pub const FORMAT: &str = "1";

/// One change a reader made, applied to the file's islands as they are when
/// the save runs, never as they were when the brief was opened.
#[derive(Clone, Debug, PartialEq)]
pub enum NoteEdit {
    Add {
        nid: String,
        title: String,
        text: String,
        ts: String,
    },
    /// Found by its words (and its time, when given), not its position:
    /// another writer may have put notes ahead of it since.
    Delete {
        nid: String,
        text: String,
        ts: Option<String>,
    },
    Concur {
        nid: String,
        ts: String,
    },
    Unconcur {
        nid: String,
    },
}

/// Why a write was not made. Every one leaves the file as it was.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// No notes island: the page is read-only, not empty.
    NoIsland,
    /// The island sits after notes.js, which reads it before it is parsed.
    IslandAfterScript,
    /// An island on a page where no notes script runs.
    NoScript,
    /// The islands hold something that is not a map of the right shape.
    Unreadable(String),
    /// The island declares a format TD does not know.
    UnknownFormat(String),
    /// A file whose name starts with `_`: a build input an agent assembles.
    BuildInput(String),
    /// A note or a concur on an anchor the page does not have.
    AnchorGone(String),
    /// A concur on a brief whose notes.js predates concurs.
    ConcursUnsupported(String),
    /// A `READER NOTES` comment that never closes.
    UnterminatedMirror,
    /// A notes island whose `</script` never closes.
    TornIsland,
    /// Two of the regions a write touches overlap.
    Overlap,
    /// The path is a symbolic link.
    Symlink,
    /// The file has another name on disk; a save by rename would split them.
    HardLinked,
    /// The path is not a regular file.
    NotAFile,
    /// The file's own mode says nobody writes it.
    ReadOnly,
    /// A revision that is not an ISO time to the millisecond. A bug, not a
    /// file: TD makes its own.
    BadRevision(String),
}

impl Refusal {
    /// The name the skill's fixtures give it, where they give it one.
    pub fn kind(&self) -> &'static str {
        match self {
            Refusal::NoIsland => "NoIsland",
            Refusal::IslandAfterScript => "IslandAfterScript",
            Refusal::NoScript => "NoScript",
            Refusal::Unreadable(_) => "Unreadable",
            Refusal::UnknownFormat(_) => "UnknownFormat",
            Refusal::BuildInput(_) => "BuildInput",
            Refusal::AnchorGone(_) => "AnchorGone",
            Refusal::ConcursUnsupported(_) => "ConcursUnsupported",
            Refusal::UnterminatedMirror => "UnterminatedMirror",
            Refusal::TornIsland => "TornIsland",
            Refusal::Overlap => "Overlap",
            Refusal::Symlink => "Symlink",
            Refusal::HardLinked => "HardLinked",
            Refusal::NotAFile => "NotAFile",
            Refusal::ReadOnly => "ReadOnly",
            Refusal::BadRevision(_) => "BadRevision",
        }
    }

    /// The refusal as one plain sentence, for the person who pressed save.
    pub fn sentence(&self) -> String {
        match self {
            Refusal::NoIsland => {
                "This page has no notes island, so there is nowhere in the file to save a note.".into()
            }
            Refusal::IslandAfterScript => "This brief's notes island comes after its notes script, so a browser could never show a note saved there.".into(),
            Refusal::NoScript => "No notes script runs on this page, so a note saved here would be one no browser shows.".into(),
            Refusal::Unreadable(why) => format!("The notes already in this file cannot be read ({why}), and TD will not overwrite what it cannot read."),
            Refusal::UnknownFormat(f) => format!("This brief's notes are in format {f}, which is newer than this TD; it will not write a format it does not know."),
            Refusal::BuildInput(name) => format!("{name} is a build input (its name starts with _), and TD never writes one."),
            Refusal::AnchorGone(nid) => format!("The passage [{nid}] is no longer in this brief, so a note on it would be one no browser shows; its text is still in the note box to copy."),
            Refusal::ConcursUnsupported(_) => "This brief's notes script is older than CONCUR stamps, so a stamp saved here would never show.".into(),
            Refusal::UnterminatedMirror => "The READER NOTES comment in this file never closes, and TD will not guess where it ends.".into(),
            Refusal::TornIsland => "This brief's notes island never closes its script tag, and TD will not guess where it ends.".into(),
            Refusal::Overlap => "The places this brief keeps its notes overlap, and TD will not guess which bytes belong to which.".into(),
            Refusal::Symlink => "This brief is a symbolic link; TD saves only into a file itself.".into(),
            Refusal::HardLinked => "This brief has another name on disk, and a save would split the two apart.".into(),
            Refusal::NotAFile => "This is not a regular file, so TD will not save into it.".into(),
            Refusal::ReadOnly => "This brief is marked read-only on disk, so TD will not save into it.".into(),
            Refusal::BadRevision(rev) => format!("The revision {rev} is not a time TD can write."),
        }
    }
}

/// One region a write replaces, and what it puts there.
#[derive(Clone, Debug, PartialEq)]
pub struct Splice {
    pub what: &'static str,
    pub range: Range<usize>,
    pub bytes: Vec<u8>,
}

/// A write worked out in memory, before anything touches the disk.
#[derive(Clone, Debug, PartialEq)]
pub struct WritePlan {
    pub out: Vec<u8>,
    /// In file order, never overlapping.
    pub splices: Vec<Splice>,
    pub notes: NoteMap,
    /// `None` on a brief that does not take concurs: its concurs island, if
    /// any, is never read or written.
    pub concurs: Option<ConcurMap>,
}

/// What a write needs to know about the page besides its bytes.
pub struct WriteArgs<'a> {
    /// `(nid, title)` in document order, from the brief's own notes.js.
    pub anchors: &'a [(&'a str, &'a str)],
    /// The page's `NOTES_FILE`, for the map's header.
    pub label: &'a str,
    /// Whether the brief's notes.js takes concurs.
    pub concurs: bool,
    /// The revision this write stamps: `new Date().toISOString()`.
    pub rev: &'a str,
    /// Only to refuse a build input.
    pub path: &'a std::path::Path,
}

/// Apply edits to maps parsed fresh from the file, the reference writer's
/// `applyEdits`: an add appends, a delete finds its note by its words, and a
/// concur goes only where the brief takes concurs and the anchor exists.
pub fn apply(
    notes: &mut NoteMap,
    concurs: &mut ConcurMap,
    edits: &[NoteEdit],
    known: &[&str],
    concurs_supported: bool,
) -> Result<(), Refusal> {
    for e in edits {
        match e {
            NoteEdit::Add {
                nid,
                title,
                text,
                ts,
            } => {
                if !known.contains(&nid.as_str()) {
                    return Err(Refusal::AnchorGone(nid.clone()));
                }
                let mut note = Obj::default();
                note.insert("text".into(), Json::Str(text.clone()));
                note.insert("title".into(), Json::Str(title.clone()));
                note.insert("ts".into(), Json::Str(ts.clone()));
                if !matches!(notes.0.get(nid), Some(Json::Arr(_))) {
                    notes.0.insert(nid.clone(), Json::Arr(Vec::new()));
                }
                if let Some(Json::Arr(list)) = notes.0.get_mut(nid) {
                    list.push(Json::Obj(note));
                }
            }
            NoteEdit::Delete { nid, text, ts } => {
                // `!e.ts`: an empty time is no time.
                let ts = ts.as_deref().filter(|t| !t.is_empty());
                let emptied = match notes.0.get_mut(nid) {
                    Some(Json::Arr(list)) => {
                        let found = list.iter().position(|n| match n {
                            Json::Obj(o) => {
                                o.get("text") == Some(&Json::Str(text.clone()))
                                    && ts.is_none_or(|t| o.get("ts") == Some(&Json::Str(t.into())))
                            }
                            _ => false,
                        });
                        if let Some(i) = found {
                            list.remove(i);
                        }
                        list.is_empty()
                    }
                    _ => false,
                };
                if emptied {
                    notes.0.remove(nid);
                }
            }
            NoteEdit::Concur { nid, ts } => {
                if !concurs_supported {
                    return Err(Refusal::ConcursUnsupported(nid.clone()));
                }
                if !known.contains(&nid.as_str()) {
                    return Err(Refusal::AnchorGone(nid.clone()));
                }
                concurs.0.insert(nid.clone(), Json::Str(ts.clone()));
            }
            NoteEdit::Unconcur { nid } => {
                if !concurs_supported {
                    return Err(Refusal::ConcursUnsupported(nid.clone()));
                }
                concurs.0.remove(nid);
            }
        }
    }
    Ok(())
}

/// `setAttribute` as the browser serialises it: an existing value replaced
/// where it stands, double-quoted, the name's own spelling kept; a new
/// attribute last, after any space before the `>`.
pub fn set_attr(tag: &[u8], name: &str, value: &str) -> Vec<u8> {
    match attr_span(tag, name) {
        Some((span, _)) => {
            let mut out = tag[..span.start].to_vec();
            out.extend_from_slice(format!("\"{value}\"").as_bytes());
            out.extend_from_slice(&tag[span.end..]);
            out
        }
        None => {
            let mut end = tag.len();
            if tag.last() == Some(&b'>') {
                end -= 1;
            }
            while end > 0 && is_js_space(tag[end - 1]) {
                end -= 1;
            }
            let mut out = tag[..end].to_vec();
            out.extend_from_slice(format!(" {name}=\"{value}\">").as_bytes());
            out
        }
    }
}

fn retag(tag: &[u8], rev: &str) -> Vec<u8> {
    set_attr(&set_attr(tag, "data-format", FORMAT), "data-rev", rev)
}

/// The whole write, worked out in memory: the reference writer's
/// `planWrite`, rule for rule. Refuses, and writes nothing, on anything it
/// cannot write exactly.
///
/// It rewrites at most three regions and carries every other byte over:
/// the first notes island (its text, and on a format-1 brief its open tag,
/// which takes the revision); the first concurs island the same way, or a
/// new one straight after the notes island's `</script>` when there are
/// concurs to write and none exists; and the last `READER NOTES` comment
/// after the notes island, else a new one before the last `</body>`, else at
/// the end of the file.
pub fn plan_write(
    bytes: &[u8],
    edits: &[NoteEdit],
    args: &WriteArgs,
) -> Result<WritePlan, Refusal> {
    let name = args
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if name.starts_with('_') {
        return Err(Refusal::BuildInput(name));
    }
    let regions = find_regions(bytes);
    let island = regions.notes.ok_or(Refusal::NoIsland)?;
    if regions.island_after_script() {
        return Err(Refusal::IslandAfterScript);
    }
    let format = attr(island.open_tag(bytes), "data-format");
    if let Some(f) = format.as_deref().filter(|f| *f != FORMAT) {
        return Err(Refusal::UnknownFormat(f.to_string()));
    }
    let stamp = format.is_some();
    if stamp && !is_rev(args.rev) {
        return Err(Refusal::BadRevision(args.rev.to_string()));
    }
    let close_end = island.close_end.ok_or(Refusal::TornIsland)?;

    let mut notes = notes_of(bytes, &island).map_err(|e| Refusal::Unreadable(e.0))?;
    let mut concurs = match (args.concurs, &regions.concurs) {
        (true, Some(c)) => concurs_of(bytes, c).map_err(|e| Refusal::Unreadable(e.0))?,
        _ => ConcurMap::default(),
    };
    let known: Vec<&str> = args.anchors.iter().map(|(n, _)| *n).collect();
    apply(&mut notes, &mut concurs, edits, &known, args.concurs)?;

    let rewrite = |i: &Island, map: &Obj, what: &'static str| {
        let json = island_json(map);
        match stamp {
            true => {
                let mut b = retag(i.open_tag(bytes), args.rev);
                b.extend_from_slice(json.as_bytes());
                Splice {
                    what,
                    range: i.tag_start..i.end,
                    bytes: b,
                }
            }
            false => Splice {
                what,
                range: i.start..i.end,
                bytes: json.into_bytes(),
            },
        }
    };
    let mut splices = vec![rewrite(&island, &notes.0, "notes-island")];
    if args.concurs {
        match &regions.concurs {
            Some(c) => splices.push(rewrite(c, &concurs.0, "concurs-island")),
            None if concurs.count() > 0 => {
                let tag = b"<script type=\"application/json\" id=\"report-concurs\">";
                let mut b = if stamp {
                    retag(tag, args.rev)
                } else {
                    tag.to_vec()
                };
                b.extend_from_slice(island_json(&concurs.0).as_bytes());
                b.extend_from_slice(b"</script>");
                splices.push(Splice {
                    what: "concurs-island-inserted",
                    range: close_end..close_end,
                    bytes: b,
                });
            }
            None => {}
        }
    }

    let empty = ConcurMap::default();
    let map = build_map(
        args.label,
        &notes,
        if args.concurs { &concurs } else { &empty },
        args.anchors,
    );
    let mirror = mirror_comment(&map).into_bytes();
    match last_mirror(bytes, close_end)? {
        Some(range) => splices.push(Splice {
            what: "mirror-replaced",
            range,
            bytes: mirror,
        }),
        None => {
            let body = rfind_ci(bytes, b"</body>", bytes.len());
            let at = body.unwrap_or(bytes.len());
            splices.push(Splice {
                what: if body.is_some() {
                    "mirror-inserted"
                } else {
                    "mirror-appended"
                },
                range: at..at,
                bytes: mirror,
            });
        }
    }

    splices.sort_by_key(|s| s.range.start);
    if splices
        .windows(2)
        .any(|w| w[1].range.start < w[0].range.end)
    {
        return Err(Refusal::Overlap);
    }
    let mut out = Vec::with_capacity(bytes.len() + 1024);
    let mut at = 0;
    for s in &splices {
        out.extend_from_slice(&bytes[at..s.range.start]);
        out.extend_from_slice(&s.bytes);
        at = s.range.end;
    }
    out.extend_from_slice(&bytes[at..]);
    Ok(WritePlan {
        out,
        splices,
        notes,
        concurs: args.concurs.then_some(concurs),
    })
}

/// The last mirror comment, when it starts after the notes island closes:
/// the one a write replaces. `Ok(None)` when there is none there.
fn last_mirror(bytes: &[u8], after: usize) -> Result<Option<Range<usize>>, Refusal> {
    let at = bytes
        .windows(MIRROR_MARK.len())
        .rposition(|w| w == MIRROR_MARK);
    match at {
        Some(at) if at > after => {
            let close = find(bytes, b"-->", at + 4).ok_or(Refusal::UnterminatedMirror)?;
            Ok(Some(at..close + 3))
        }
        _ => Ok(None),
    }
}

/// The two checks made before a write is committed: every byte outside the
/// splices equals the original, and the islands in the result read back as
/// the maps meant to be written.
pub fn verify(before: &[u8], plan: &WritePlan) -> Result<(), String> {
    let (mut o, mut n) = (0usize, 0usize);
    for s in &plan.splices {
        let len = s.range.start - o;
        if before.get(o..s.range.start) != plan.out.get(n..n + len) {
            return Err(format!("bytes moved before the {}", s.what));
        }
        n += len + s.bytes.len();
        o = s.range.end;
    }
    if before.get(o..) != plan.out.get(n..) {
        return Err("bytes moved after the last region written".into());
    }
    let back = read(&plan.out);
    let notes = match back.notes {
        Some(Ok(notes)) => notes,
        _ => return Err("the notes island does not read back".into()),
    };
    if stringify(&Json::Obj(notes.0)) != stringify(&Json::Obj(plan.notes.0.clone())) {
        return Err("the notes island does not read back as the notes written".into());
    }
    if let Some(want) = &plan.concurs {
        match back.concurs {
            Ok(c)
                if stringify(&Json::Obj(c.0.clone())) == stringify(&Json::Obj(want.0.clone())) => {}
            _ => return Err("the concurs island does not read back as the concurs written".into()),
        }
    }
    Ok(())
}

/// What the written file, reopened fresh in the engine, must show: a note
/// on exactly the anchors the plan put notes on, and a stamp on exactly the
/// decisions it concurred, as the brief's own notes.js marks them. The first
/// anchor that disagrees is named. An anchor the page did not say anything
/// about is a disagreement too: unknown is not "no".
pub fn confirm(plan: &WritePlan, shown: &[super::engine::Anchor]) -> Result<(), String> {
    for a in shown {
        let want = !plan.notes.on(&a.nid).is_empty();
        match a.has_note {
            Some(got) if got == want => {}
            Some(got) => {
                return Err(format!(
                    "[{}] {} a note",
                    a.nid,
                    if got { "shows" } else { "does not show" }
                ))
            }
            None => {
                return Err(format!(
                    "the page did not say whether [{}] has a note",
                    a.nid
                ))
            }
        }
        if let (Some(concurs), Some(_)) = (&plan.concurs, a.concur_zone) {
            let want = concurs.has(&a.nid);
            if a.has_concur != Some(want) {
                return Err(format!(
                    "[{}] {} its stamp",
                    a.nid,
                    if want { "does not show" } else { "still shows" }
                ));
            }
        }
    }
    Ok(())
}

// ── the layout's hash, with the notes left out ──────────────────────────────

/// The byte ranges a notes write can change: the notes island's open tag and
/// text, the whole concurs island, and the last mirror after the notes
/// island. Sorted, never overlapping.
pub fn notes_regions(bytes: &[u8]) -> Vec<Range<usize>> {
    let r = find_regions(bytes);
    let mut out = Vec::new();
    if let Some(i) = r.notes {
        out.push(i.tag_start..i.end);
        if let Ok(Some(m)) = last_mirror(bytes, i.close_end.unwrap_or(i.end)) {
            out.push(m);
        }
    }
    if let Some(c) = r.concurs {
        out.push(c.tag_start..c.close_end.unwrap_or(c.end));
    }
    out.sort_by_key(|r| r.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for r in out {
        match merged.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => merged.push(r),
        }
    }
    merged
}

/// Every byte of the file outside [`notes_regions`], in order: what a page
/// is laid out from, as far as notes are concerned. A save changes only
/// what this leaves out, so a saved note keeps the render — which is least-
/// confident decision 2, notes are data, not layout, and is measured.
pub fn outside_notes(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    let masks = notes_regions(bytes);
    let mut at = 0;
    let mut pieces = Vec::with_capacity(masks.len() + 1);
    for m in masks {
        pieces.push(&bytes[at..m.start]);
        at = m.end;
    }
    pieces.push(&bytes[at..]);
    pieces.into_iter()
}

// ── writing: to disk ────────────────────────────────────────────────────────

/// Where a brief's backups are kept, and how many.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Backups {
    pub dir: std::path::PathBuf,
    pub keep: usize,
}

/// The last eight copies of a brief as it was before each save.
pub const KEEP_BACKUPS: usize = 8;

/// `$XDG_STATE_HOME/terminal-delight/brief-backups/<hash of the path>/`,
/// the state directory resolved as the surface feed resolves it: the
/// variable when it is set and absolute, else `~/.local/state`. Never
/// beside the brief: a backup there would be a second brief in the folder.
pub fn backups_for(path: &std::path::Path) -> Backups {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                .join(".local/state")
        });
    use std::os::unix::ffi::OsStrExt;
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in path.as_os_str().as_bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    Backups {
        dir: base
            .join("terminal-delight/brief-backups")
            .join(format!("{h:016x}")),
        keep: KEEP_BACKUPS,
    }
}

/// Why a commit did not happen, or did not finish.
#[derive(Clone, Debug, PartialEq)]
pub enum WriteError {
    Refused(Refusal),
    /// Somebody else wrote the file after it was read: nothing was written.
    Changed,
    /// The disk said no. `backup` is where the file's bytes from before are,
    /// when the backup was made before it did.
    Io {
        why: String,
        backup: Option<std::path::PathBuf>,
    },
}

impl WriteError {
    pub fn sentence(&self) -> String {
        match self {
            WriteError::Refused(r) => r.sentence(),
            WriteError::Changed => "The brief changed on disk while it was being saved, so nothing was written; your notes are still here to save again.".into(),
            WriteError::Io { why, backup: Some(b) } => format!("Could not save ({why}); the file is as it was, and a copy of it is at {}.", b.display()),
            WriteError::Io { why, backup: None } => format!("Could not save ({why}); the file is as it was."),
        }
    }
}

/// A save that landed: where the copy from before it is, and the file as
/// the disk now has it.
#[derive(Clone, Debug, PartialEq)]
pub struct Written {
    pub backup: std::path::PathBuf,
    pub mtime: std::time::SystemTime,
    pub len: u64,
}

/// Write a plan into the file: backup, then a temporary file in the same
/// directory with the original's mode, synced, a last look that nobody wrote
/// meanwhile, a rename over the original (atomic on one filesystem), the
/// directory synced, and the bytes on disk read back and compared with the
/// plan. `read` is what the plan was made from.
pub fn commit(
    path: &std::path::Path,
    read: &[u8],
    plan: &WritePlan,
    backups: &Backups,
) -> Result<Written, WriteError> {
    commit_with(path, read, plan, backups, &mut || {})
}

/// [`commit`], with a hook run just before the rename, where a test can
/// play the other writer.
pub fn commit_with(
    path: &std::path::Path,
    read: &[u8],
    plan: &WritePlan,
    backups: &Backups,
    before_rename: &mut dyn FnMut(),
) -> Result<Written, WriteError> {
    use std::io::Write;
    use std::os::unix::fs::MetadataExt;
    let io = |why: String, backup: Option<&std::path::Path>| WriteError::Io {
        why,
        backup: backup.map(std::path::Path::to_path_buf),
    };
    let meta = std::fs::symlink_metadata(path).map_err(|e| io(e.to_string(), None))?;
    if meta.file_type().is_symlink() {
        return Err(WriteError::Refused(Refusal::Symlink));
    }
    if !meta.is_file() {
        return Err(WriteError::Refused(Refusal::NotAFile));
    }
    if meta.nlink() > 1 {
        return Err(WriteError::Refused(Refusal::HardLinked));
    }
    if meta.permissions().readonly() {
        return Err(WriteError::Refused(Refusal::ReadOnly));
    }
    let now = std::fs::read(path).map_err(|e| io(e.to_string(), None))?;
    if now != read {
        return Err(WriteError::Changed);
    }
    let backup = back_up(path, read, backups).map_err(|e| io(format!("the backup: {e}"), None))?;
    let dir = path.parent().unwrap_or(std::path::Path::new("."));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(".{name}.td-save-{}-{stamp}", std::process::id()));
    let written = (|| {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(&plan.out)?;
        f.set_permissions(meta.permissions())?;
        f.sync_all()
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(io(e.to_string(), Some(&backup)));
    }
    before_rename();
    match std::fs::read(path) {
        Ok(again) if again == read => {}
        Ok(_) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(WriteError::Changed);
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(io(e.to_string(), Some(&backup)));
        }
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(io(e.to_string(), Some(&backup)));
    }
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
    let on_disk = std::fs::read(path).map_err(|e| io(e.to_string(), Some(&backup)))?;
    if on_disk != plan.out {
        return Err(io(
            "what is on disk is not what was written".into(),
            Some(&backup),
        ));
    }
    let after = std::fs::metadata(path).map_err(|e| io(e.to_string(), Some(&backup)))?;
    let mtime = after
        .modified()
        .map_err(|e| io(e.to_string(), Some(&backup)))?;
    Ok(Written {
        backup,
        mtime,
        len: after.len(),
    })
}

/// Keep a copy of the bytes as they were, newest last by name, and only the
/// last [`Backups::keep`] of them. The folder also names the brief it holds.
fn back_up(
    path: &std::path::Path,
    bytes: &[u8],
    backups: &Backups,
) -> std::io::Result<std::path::PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&backups.dir)?;
    std::fs::write(
        backups.dir.join("brief"),
        path.as_os_str().to_string_lossy().as_bytes(),
    )?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Named `<ms>-<n>-<name>`, padded, and always after the newest already
    // there: the ring's order is its names' order, whatever the clock does
    // and however many saves land in one millisecond.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let newest = std::fs::read_dir(&backups.dir)?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let mut parts = name.splitn(3, '-');
            let ms = parts.next()?.parse::<u128>().ok()?;
            let n = parts.next()?.parse::<u32>().ok()?;
            Some((ms, n))
        })
        .max();
    let (millis, n) = match newest {
        Some((ms, n)) if ms >= now => (ms, n + 1),
        _ => (now, 0),
    };
    let file = backups.dir.join(format!("{millis:015}-{n:04}-{name}"));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file)?;
    f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    f.write_all(bytes)?;
    f.sync_all()?;
    let mut kept: Vec<std::path::PathBuf> = std::fs::read_dir(&backups.dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n != "brief"))
        .collect();
    kept.sort();
    let extra = kept.len().saturating_sub(backups.keep.max(1));
    for old in &kept[..extra] {
        let _ = std::fs::remove_file(old);
    }
    Ok(file)
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

    // ── the writer ──────────────────────────────────────────────────────────

    /// A directory of its own for one test, gone when the test is.
    struct Dir(PathBuf);

    impl Dir {
        fn new(tag: &str) -> Dir {
            let d = std::env::temp_dir().join(format!(
                "td-notes-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            Dir(d)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A case's edits as TD would make them, and the revision the write stamps.
    fn edits(case: &Path) -> (String, Vec<NoteEdit>) {
        let v = json(&case.join("edits.json"));
        let s = |e: &serde_json::Value, k: &str| e[k].as_str().map(str::to_string);
        let list = v["edits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                let nid = s(e, "nid").unwrap();
                match e["op"].as_str().unwrap() {
                    "add" => NoteEdit::Add {
                        nid,
                        title: s(e, "title").unwrap(),
                        text: s(e, "text").unwrap(),
                        ts: s(e, "ts").unwrap(),
                    },
                    "delete" => NoteEdit::Delete {
                        nid,
                        text: s(e, "text").unwrap(),
                        ts: s(e, "ts"),
                    },
                    "concur" => NoteEdit::Concur {
                        nid,
                        ts: s(e, "ts").unwrap(),
                    },
                    "unconcur" => NoteEdit::Unconcur { nid },
                    op => panic!("unknown op {op}"),
                }
            })
            .collect();
        (v["rev"].as_str().unwrap().to_string(), list)
    }

    /// Plan a case's write the way the skill's check does: its edits, its
    /// anchors, its NOTES_FILE and its concur support, from `file`.
    fn plan_case(
        case: &Path,
        file: &str,
        edits_: &[NoteEdit],
        rev: &str,
    ) -> Result<WritePlan, Refusal> {
        let expect = json(&case.join("expect.json"));
        let a = anchors(case);
        let pairs = pairs(&a);
        let bytes = std::fs::read(case.join(file)).unwrap();
        plan_write(
            &bytes,
            edits_,
            &WriteArgs {
                anchors: &pairs,
                label: expect["notes_file"].as_str().unwrap_or("brief.html"),
                concurs: expect["concur_support"] == "supported",
                rev,
                path: Path::new("/r/brief.html"),
            },
        )
    }

    /// The whole contract: for every case the skill pins, TD's writer makes
    /// exactly the bytes the reference writer made, or refuses exactly as it
    /// refused.
    #[test]
    fn adding_notes_gives_the_skills_expected_bytes() {
        let (mut written, mut refused) = (0, 0);
        for (name, case) in cases() {
            let (rev, list) = edits(&case);
            let expect = json(&case.join("expect.json"));
            let got = plan_case(&case, "brief.html", &list, &rev);
            match expect["refuse"].as_str() {
                Some(kind) => {
                    let r = got.expect_err(&name);
                    assert_eq!(r.kind(), kind, "{name}: {r:?}");
                    assert!(!case.join("expected.html").exists());
                    refused += 1;
                }
                None => {
                    let plan = got.unwrap_or_else(|r| panic!("{name}: refused {r:?}"));
                    let want = std::fs::read(case.join("expected.html")).unwrap();
                    assert!(
                        plan.out == want,
                        "{name}: the written bytes differ from expected.html at byte {:?}",
                        plan.out.iter().zip(&want).position(|(a, b)| a != b)
                    );
                    let whats: Vec<&str> = plan.splices.iter().map(|s| s.what).collect();
                    let mut want_whats: Vec<&str> = expect["writes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|w| w.as_str().unwrap())
                        .collect();
                    let mut sorted = whats.clone();
                    sorted.sort();
                    want_whats.sort();
                    assert_eq!(sorted, want_whats, "{name}: the regions written");
                    written += 1;
                }
            }
        }
        assert_eq!((written, refused), (11, 4));
    }

    #[test]
    fn a_format_newer_than_td_is_refused() {
        let case = fixtures().join("cases/future-format");
        let (rev, list) = edits(&case);
        assert_eq!(
            plan_case(&case, "brief.html", &list, &rev),
            Err(Refusal::UnknownFormat("2".into()))
        );
        // Even with no edits at all: TD never rewrites a format it does not know.
        assert_eq!(
            plan_case(&case, "brief.html", &[], &rev).map(|_| ()),
            Err(Refusal::UnknownFormat("2".into()))
        );
    }

    /// `verify` holds on every plan, and catches one byte moved outside the
    /// regions a write owns.
    #[test]
    fn nothing_outside_the_three_regions_moves() {
        for (name, case) in cases() {
            let (rev, list) = edits(&case);
            let Ok(plan) = plan_case(&case, "brief.html", &list, &rev) else {
                continue;
            };
            let before = std::fs::read(case.join("brief.html")).unwrap();
            assert_eq!(verify(&before, &plan), Ok(()), "{name}");
            assert!(plan.splices.len() <= 3, "{name}: at most three regions");
            let mut tampered = plan.clone();
            let first = tampered.splices[0].range.start;
            let at = if first > 10 {
                5
            } else {
                tampered.out.len() - 2
            };
            tampered.out[at] ^= 0x20;
            assert!(
                verify(&before, &tampered).is_err(),
                "{name}: a byte moved at {at}"
            );
            // And a plan whose island does not read back as the notes meant.
            let mut lying = plan.clone();
            lying
                .notes
                .0
                .insert("invented".into(), Json::Arr(Vec::new()));
            assert!(verify(&before, &lying).is_err(), "{name}");
        }
    }

    /// Writing a written brief back with no edits, under the revision it
    /// already carries, changes not one byte.
    #[test]
    fn writing_the_same_notes_back_changes_no_byte() {
        let mut n = 0;
        for (name, case) in cases() {
            if !case.join("expected.html").exists() {
                continue;
            }
            let (rev, _) = edits(&case);
            let plan = plan_case(&case, "expected.html", &[], &rev).unwrap();
            let want = std::fs::read(case.join("expected.html")).unwrap();
            assert!(plan.out == want, "{name}");
            n += 1;
        }
        assert_eq!(n, 11);
    }

    /// The island a browser's own save wrote is reproduced exactly from its
    /// own parse: TD's JSON is `JSON.stringify(map, null, 1)`.
    #[test]
    fn the_island_is_written_the_way_json_stringify_writes_it() {
        let bytes = std::fs::read(fixtures().join("cases/b689671-saved/brief.html")).unwrap();
        let r = find_regions(&bytes);
        let island = r.notes.unwrap();
        let text = std::str::from_utf8(island.text(&bytes)).unwrap();
        let parsed = notes_of(&bytes, &island).unwrap();
        assert_eq!(island_json(&parsed.0), text);
        // Numbers the way JavaScript prints them.
        for (x, s) in [
            (1.0, "1"),
            (-0.0, "0"),
            (0.1, "0.1"),
            (123.456, "123.456"),
            (1e21, "1e+21"),
            (1.5e-7, "1.5e-7"),
            (0.000001, "0.000001"),
            (2.0_f64.powi(53) + 2.0, "9007199254740994"),
            (-12.5, "-12.5"),
        ] {
            assert_eq!(js_number(x), s, "{x}");
        }
        let bs = '\\';
        let v = Json::Str("a\"b\\c\u{8}\u{c}\n\r\t\u{7}\u{1b}\u{2028}é".into());
        assert_eq!(
            stringify(&v),
            format!("\"a{bs}\"b{bs}{bs}c{bs}b{bs}f{bs}n{bs}r{bs}t{bs}u0007{bs}u001b\u{2028}é\"")
        );
        let mut o = Obj::default();
        o.insert(
            "k".into(),
            Json::Arr(vec![Json::Obj(Obj::default()), Json::Arr(vec![])]),
        );
        assert_eq!(stringify(&Json::Obj(o)), "{\n \"k\": [\n  {},\n  []\n ]\n}");
    }

    /// Text that would end the island, open a comment, enter the script's
    /// double-escaped state or end the mirror is written so it cannot, and
    /// reads back exactly as it was typed.
    #[test]
    fn a_note_cannot_end_the_island_or_the_comment() {
        let case = fixtures().join("cases/hostile-text");
        let (rev, list) = edits(&case);
        let plan = plan_case(&case, "brief.html", &list, &rev).unwrap();
        let island = find_regions(&plan.out).notes.unwrap();
        let text = String::from_utf8(island.text(&plan.out).to_vec()).unwrap();
        let bs = '\\';
        assert!(!text.contains("</"), "no </ left in the island");
        assert!(!text.contains("<!"), "no <! left in the island");
        assert!(text.contains(&format!("<{bs}/script>")));
        assert!(text.contains(&format!("{bs}u003c!--")));
        let mirror = find(&plan.out, MIRROR_MARK, island.end).unwrap();
        let close = find(&plan.out, b"-->", mirror + 4).unwrap();
        let comment = String::from_utf8(plan.out[mirror..close + 3].to_vec()).unwrap();
        assert!(comment.contains("closes a comment -- > LEAKED-2 and --! > LEAKED-3, keeps ---"));
        assert_eq!(comment.matches("-->").count(), 1, "only its own end");
        // Every note reads back as the words that were written.
        let back = read(&plan.out).notes.unwrap().unwrap();
        for e in &list {
            if let NoteEdit::Add { nid, text, .. } = e {
                assert!(back.on(nid).iter().any(|n| n.text == text), "{text:?}");
            }
        }
        // Including what another writer baked in with a CR, a bell and U+2028.
        let baked = back.on("callout-a-callout-that-takes");
        assert!(baked[0].text.contains("\r\n") && baked[0].text.contains('\u{7}'));
        assert!(baked[0].text.contains('\u{2028}'));
    }

    #[test]
    fn only_the_last_reader_notes_comment_is_replaced() {
        let case = fixtures().join("cases/stale-mirrors");
        let before = std::fs::read(case.join("brief.html")).unwrap();
        let (rev, list) = edits(&case);
        let plan = plan_case(&case, "brief.html", &list, &rev).unwrap();
        let marks = |b: &[u8]| {
            b.windows(MIRROR_MARK.len())
                .enumerate()
                .filter(|(_, w)| *w == MIRROR_MARK)
                .map(|(i, _)| i)
                .collect::<Vec<_>>()
        };
        let (was, now) = (marks(&before), marks(&plan.out));
        assert_eq!(was.len(), now.len(), "replaced, never appended");
        let comment = |b: &[u8], at: usize| {
            let close = find(b, b"-->", at + 4).unwrap();
            b[at..close + 3].to_vec()
        };
        let (was_last, now_last) = (was[was.len() - 1], now[now.len() - 1]);
        for (&a, &b) in was.iter().zip(&now).take(was.len() - 1) {
            if a <= find_regions(&before).notes.unwrap().end {
                continue;
            }
            assert_eq!(
                comment(&before, a),
                comment(&plan.out, b),
                "an earlier mirror moved"
            );
        }
        assert_ne!(comment(&before, was_last), comment(&plan.out, now_last));
        assert!(plan.splices.iter().any(|s| s.what == "mirror-replaced"));
    }

    #[test]
    fn a_mirror_goes_before_the_last_body_close_or_at_the_end() {
        let case = fixtures().join("cases/current-pristine");
        let (rev, list) = edits(&case);
        let plan = plan_case(&case, "brief.html", &list, &rev).unwrap();
        let before = std::fs::read(case.join("brief.html")).unwrap();
        let body = rfind_ci(&before, b"</body>", before.len()).unwrap();
        let s = plan
            .splices
            .iter()
            .find(|s| s.what == "mirror-inserted")
            .unwrap();
        assert_eq!(s.range, body..body);
        let case = fixtures().join("cases/no-body-close");
        let (rev, list) = edits(&case);
        let plan = plan_case(&case, "brief.html", &list, &rev).unwrap();
        let before = std::fs::read(case.join("brief.html")).unwrap();
        let s = plan
            .splices
            .iter()
            .find(|s| s.what == "mirror-appended")
            .unwrap();
        assert_eq!(s.range, before.len()..before.len());
        assert!(plan.out.ends_with(b"\n-->"));
    }

    /// A brief whose concurs island was never written: no island appears for
    /// no concurs, and one concur puts one straight after the notes island.
    #[test]
    fn a_concurs_island_is_inserted_after_the_notes_island_only_when_needed() {
        let pristine = std::fs::read(fixtures().join("cases/current-pristine/brief.html")).unwrap();
        let c = find_regions(&pristine).concurs.unwrap();
        let mut bytes = pristine[..c.tag_start].to_vec();
        bytes.extend_from_slice(&pristine[c.close_end.unwrap()..]);
        assert!(find_regions(&bytes).concurs.is_none());
        let case = fixtures().join("cases/current-pristine");
        let a = anchors(&case);
        let p = pairs(&a);
        let args = WriteArgs {
            anchors: &p,
            label: "brief.html",
            concurs: true,
            rev: "2026-09-24T20:05:00.000Z",
            path: Path::new("/r/brief.html"),
        };
        let note = NoteEdit::Add {
            nid: "finding-the-island-is-the".into(),
            title: "The island is the record".into(),
            text: "no concurs".into(),
            ts: "2026-09-24 20:00".into(),
        };
        let plan = plan_write(&bytes, std::slice::from_ref(&note), &args).unwrap();
        assert!(!plan.splices.iter().any(|s| s.what.starts_with("concurs")));
        assert!(find_regions(&plan.out).concurs.is_none());
        let concur = NoteEdit::Concur {
            nid: "ask-stamp-a-revision-on".into(),
            ts: "2026-09-24 20:01".into(),
        };
        let plan = plan_write(&bytes, &[note, concur], &args).unwrap();
        let inserted = plan
            .splices
            .iter()
            .find(|s| s.what == "concurs-island-inserted")
            .unwrap();
        let notes_island = find_regions(&bytes).notes.unwrap();
        assert_eq!(inserted.range.start, notes_island.close_end.unwrap());
        assert!(inserted.bytes.starts_with(
            b"<script type=\"application/json\" id=\"report-concurs\" data-format=\"1\" data-rev=\"2026-09-24T20:05:00.000Z\">"
        ));
        let back = read(&plan.out).concurs.unwrap();
        assert!(back.has("ask-stamp-a-revision-on"));
    }

    #[test]
    fn a_brief_older_than_concurs_never_gets_a_concurs_island() {
        let case = fixtures().join("cases/b689671-saved");
        let concur = NoteEdit::Concur {
            nid: "ask-stamp-a-revision-on".into(),
            ts: "2026-09-24 20:01".into(),
        };
        let rev = "2026-09-24T20:05:00.000Z";
        assert_eq!(
            plan_case(&case, "brief.html", std::slice::from_ref(&concur), rev),
            Err(Refusal::ConcursUnsupported(
                "ask-stamp-a-revision-on".into()
            ))
        );
        let (_, list) = edits(&case);
        let plan = plan_case(&case, "brief.html", &list, rev).unwrap();
        assert!(!plan.splices.iter().any(|s| s.what.starts_with("concurs")));
        assert_eq!(plan.concurs, None, "its concurs are never read either");
    }

    #[test]
    fn a_file_without_an_island_is_refused_not_invented() {
        for (case, want) in [
            ("no-island", "NoIsland"),
            ("island-after-script", "IslandAfterScript"),
            ("unreadable-island", "Unreadable"),
        ] {
            let dir = fixtures().join("cases").join(case);
            let (rev, list) = edits(&dir);
            let got = plan_case(&dir, "brief.html", &list, &rev);
            assert_eq!(got.map(|_| ()).unwrap_err().kind(), want, "{case}");
            // Even a write of nothing: no island is not an empty island.
            let none = plan_case(&dir, "brief.html", &[], &rev);
            assert_eq!(none.map(|_| ()).unwrap_err().kind(), want, "{case}");
        }
    }

    #[test]
    fn a_build_input_is_never_written() {
        let case = fixtures().join("cases/current-pristine");
        let bytes = std::fs::read(case.join("brief.html")).unwrap();
        let a = anchors(&case);
        let p = pairs(&a);
        let got = plan_write(
            &bytes,
            &[],
            &WriteArgs {
                anchors: &p,
                label: "x",
                concurs: true,
                rev: "2026-09-24T20:05:00.000Z",
                path: Path::new("/r/_x_body.html"),
            },
        );
        assert_eq!(
            got.map(|_| ()),
            Err(Refusal::BuildInput("_x_body.html".into()))
        );
    }

    fn add(nid: &str, text: &str, ts: &str) -> NoteEdit {
        NoteEdit::Add {
            nid: nid.into(),
            title: "t".into(),
            text: text.into(),
            ts: ts.into(),
        }
    }

    const FIG: &str = "fig-01-two-writers-one";

    /// Plan against current-pristine with a hand-made notes island.
    fn plan_island(island: &str, list: &[NoteEdit]) -> Result<WritePlan, Refusal> {
        let pristine = std::fs::read(fixtures().join("cases/current-pristine/brief.html")).unwrap();
        let i = find_regions(&pristine).notes.unwrap();
        let mut bytes = pristine[..i.start].to_vec();
        bytes.extend_from_slice(island.as_bytes());
        bytes.extend_from_slice(&pristine[i.end..]);
        let a = anchors(&fixtures().join("cases/current-pristine"));
        let p = pairs(&a);
        plan_write(
            &bytes,
            list,
            &WriteArgs {
                anchors: &p,
                label: "brief.html",
                concurs: true,
                rev: "2026-09-24T20:05:00.000Z",
                path: Path::new("/r/brief.html"),
            },
        )
    }

    fn texts(plan: &WritePlan, nid: &str) -> Vec<String> {
        plan.notes
            .on(nid)
            .iter()
            .map(|n| n.text.to_string())
            .collect()
    }

    #[test]
    fn a_deleted_note_is_found_by_its_words_not_its_position() {
        // TD read [a, b] and deleted b; meanwhile another writer put x first.
        let now = format!(
            r#"{{"{FIG}":[{{"text":"x","ts":"2026-09-24 09:00"}},{{"text":"a","ts":"2026-09-24 10:00"}},{{"text":"b","ts":"2026-09-24 11:00"}}]}}"#
        );
        let del = NoteEdit::Delete {
            nid: FIG.into(),
            text: "b".into(),
            ts: Some("2026-09-24 11:00".into()),
        };
        let plan = plan_island(&now, &[del]).unwrap();
        assert_eq!(texts(&plan, FIG), ["x", "a"]);
        // The same words at another time are another note.
        let wrong_time = NoteEdit::Delete {
            nid: FIG.into(),
            text: "a".into(),
            ts: Some("2026-09-24 23:59".into()),
        };
        assert_eq!(
            texts(&plan_island(&now, &[wrong_time]).unwrap(), FIG),
            ["x", "a", "b"]
        );
        // A last note deleted takes its key with it.
        let only = format!(r#"{{"{FIG}":[{{"text":"a"}}]}}"#);
        let del = NoteEdit::Delete {
            nid: FIG.into(),
            text: "a".into(),
            ts: None,
        };
        assert!(plan_island(&only, &[del])
            .unwrap()
            .notes
            .0
            .get(FIG)
            .is_none());
    }

    #[test]
    fn pending_notes_join_whatever_the_file_holds_now() {
        // TD's note is a delta: applied to the file as it is at save time, it
        // keeps the note another writer added after TD read the file.
        let now = format!(r#"{{"{FIG}":[{{"text":"theirs","ts":"2026-09-24 09:00"}}]}}"#);
        let plan = plan_island(&now, &[add(FIG, "mine", "2026-09-24 10:00")]).unwrap();
        assert_eq!(texts(&plan, FIG), ["theirs", "mine"]);
    }

    #[test]
    fn a_note_on_a_passage_that_is_gone_is_not_written() {
        let got = plan_island(
            "{}",
            &[add("finding-cut-since", "orphaned", "2026-09-24 10:00")],
        );
        assert_eq!(
            got.map(|_| ()),
            Err(Refusal::AnchorGone("finding-cut-since".into()))
        );
        let concur = NoteEdit::Concur {
            nid: "ask-gone".into(),
            ts: "2026-09-24 10:00".into(),
        };
        assert_eq!(
            plan_island("{}", &[concur]).map(|_| ()),
            Err(Refusal::AnchorGone("ask-gone".into()))
        );
    }

    #[test]
    fn the_notes_island_keeps_its_keys_in_the_order_the_file_had_them() {
        let island = format!(
            r#"{{"{FIG}":[{{"text":"z first"}}],"finding-the-island-is-the":[{{"text":"a second"}}]}}"#
        );
        let plan = plan_island(
            &island,
            &[add("finding-the-island-is-the", "more", "2026-09-24 10:00")],
        )
        .unwrap();
        let text = String::from_utf8(
            find_regions(&plan.out)
                .notes
                .unwrap()
                .text(&plan.out)
                .to_vec(),
        )
        .unwrap();
        assert!(
            text.find(FIG).unwrap() < text.find("finding-the-island-is-the").unwrap(),
            "{text}"
        );
    }

    #[test]
    fn a_field_another_writer_added_survives_a_save() {
        let case = fixtures().join("cases/orphan-note");
        let (rev, list) = edits(&case);
        let plan = plan_case(&case, "brief.html", &list, &rev).unwrap();
        let text = String::from_utf8(
            find_regions(&plan.out)
                .notes
                .unwrap()
                .text(&plan.out)
                .to_vec(),
        )
        .unwrap();
        assert!(text.contains("\"author\": \"terminal-delight\""), "{text}");
        let island = format!(
            r#"{{"{FIG}":[{{"ts":"x","text":"order kept","pinned":1.50,"by":{{"n":null}}}}]}}"#
        );
        let plan = plan_island(&island, &[]).unwrap();
        let text = String::from_utf8(
            find_regions(&plan.out)
                .notes
                .unwrap()
                .text(&plan.out)
                .to_vec(),
        )
        .unwrap();
        assert!(
            text.contains("\"ts\": \"x\",\n   \"text\": \"order kept\",\n   \"pinned\": 1.5,\n   \"by\": {\n    \"n\": null\n   }"),
            "{text}"
        );
    }

    #[test]
    fn a_notes_time_is_written_in_utc_as_notes_js_writes_it() {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_790_278_680);
        assert_eq!(utc_minute(t), "2026-09-24 19:38");
        assert_eq!(
            iso_millis(t + std::time::Duration::from_millis(45)),
            "2026-09-24T19:38:00.045Z"
        );
        assert_eq!(utc_minute(std::time::UNIX_EPOCH), "1970-01-01 00:00");
        // A leap day, and the last minute of a year.
        let leap = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_709_164_800);
        assert_eq!(utc_minute(leap), "2024-02-29 00:00");
        let eve = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_798_761_599);
        assert_eq!(iso_millis(eve), "2026-12-31T23:59:59.000Z");
        assert!(is_rev(&iso_millis(std::time::SystemTime::now())));
    }

    /// A pristine brief in a directory of its own, and its plan.
    fn staged(dir: &Path) -> (PathBuf, Vec<u8>, WritePlan) {
        let case = fixtures().join("cases/current-pristine");
        let path = dir.join("brief.html");
        std::fs::copy(case.join("brief.html"), &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let (rev, list) = edits(&case);
        let plan = plan_case(&case, "brief.html", &list, &rev).unwrap();
        (path, bytes, plan)
    }

    #[test]
    fn a_save_is_a_rename_and_leaves_a_backup() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let dir = Dir::new("rename");
        let (path, bytes, plan) = staged(&dir.0);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        let inode = std::fs::metadata(&path).unwrap().ino();
        let backups = Backups {
            dir: dir.0.join("state/brief-backups/x"),
            keep: KEEP_BACKUPS,
        };
        let w = commit(&path, &bytes, &plan, &backups).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), plan.out);
        let meta = std::fs::metadata(&path).unwrap();
        assert_ne!(
            meta.ino(),
            inode,
            "a new file renamed into place, not the old one rewritten"
        );
        assert_eq!(meta.permissions().mode() & 0o777, 0o640, "the mode kept");
        assert_eq!((w.len, w.mtime), (meta.len(), meta.modified().unwrap()));
        assert_eq!(
            std::fs::read(&w.backup).unwrap(),
            bytes,
            "the backup is the file as it was"
        );
        assert!(
            w.backup.starts_with(&backups.dir),
            "in the ring, not beside the brief"
        );
        let left: Vec<String> = std::fs::read_dir(&dir.0)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            left.len(),
            2,
            "the brief and the state folder, no temporary file: {left:?}"
        );
        assert_eq!(
            std::fs::read_to_string(backups.dir.join("brief")).unwrap(),
            path.to_string_lossy(),
            "the ring says whose it is"
        );
    }

    #[test]
    fn a_backup_ring_keeps_the_last_eight() {
        let dir = Dir::new("ring");
        let (path, _, _) = staged(&dir.0);
        let backups = Backups {
            dir: dir.0.join("ring"),
            keep: KEEP_BACKUPS,
        };
        for i in 0..11u8 {
            back_up(&path, &[b'0' + i], &backups).unwrap();
        }
        let mut kept: Vec<Vec<u8>> = std::fs::read_dir(&backups.dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name() != "brief")
            .map(|e| (e.path(), std::fs::read(e.path()).unwrap()))
            .collect::<std::collections::BTreeMap<_, _>>()
            .into_values()
            .collect();
        assert_eq!(kept.len(), 8);
        assert_eq!(kept.remove(0), b"3", "the three oldest went");
        assert_eq!(kept.pop().unwrap(), b":", "the newest stayed");
        let b = backups_for(Path::new("/home/p/r/a.html"));
        assert!(b
            .dir
            .to_string_lossy()
            .contains("terminal-delight/brief-backups/"));
        assert_ne!(b.dir, backups_for(Path::new("/home/p/r/b.html")).dir);
    }

    #[test]
    fn a_file_written_by_someone_else_mid_save_is_not_overwritten() {
        let dir = Dir::new("changed");
        let (path, bytes, plan) = staged(&dir.0);
        let backups = Backups {
            dir: dir.0.join("ring"),
            keep: KEEP_BACKUPS,
        };
        // Between the read and the save.
        std::fs::write(&path, b"an agent rewrote it").unwrap();
        assert_eq!(
            commit(&path, &bytes, &plan, &backups),
            Err(WriteError::Changed)
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"an agent rewrote it");
        // Between the temporary file and the rename.
        std::fs::write(&path, &bytes).unwrap();
        let p = path.clone();
        let got = commit_with(&path, &bytes, &plan, &backups, &mut || {
            std::fs::write(&p, b"rewritten mid-save").unwrap();
        });
        assert_eq!(got, Err(WriteError::Changed));
        assert_eq!(std::fs::read(&path).unwrap(), b"rewritten mid-save");
        let names: Vec<String> = std::fs::read_dir(&dir.0)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            !names.iter().any(|n| n.contains("td-save")),
            "no temporary file left: {names:?}"
        );
    }

    #[test]
    fn a_symlinked_brief_is_refused() {
        let dir = Dir::new("symlink");
        let (path, bytes, plan) = staged(&dir.0);
        let link = dir.0.join("link.html");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        let backups = Backups {
            dir: dir.0.join("ring"),
            keep: KEEP_BACKUPS,
        };
        assert_eq!(
            commit(&link, &bytes, &plan, &backups),
            Err(WriteError::Refused(Refusal::Symlink))
        );
        let hard = dir.0.join("hard.html");
        std::fs::hard_link(&path, &hard).unwrap();
        assert_eq!(
            commit(&path, &bytes, &plan, &backups),
            Err(WriteError::Refused(Refusal::HardLinked))
        );
        std::fs::remove_file(&hard).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
        assert_eq!(
            commit(&path, &bytes, &plan, &backups),
            Err(WriteError::Refused(Refusal::ReadOnly))
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes, "untouched throughout");
        assert!(!backups.dir.exists(), "a refusal makes no backup");
    }

    #[test]
    fn every_refusal_is_said_in_plain_words() {
        for r in [
            Refusal::NoIsland,
            Refusal::IslandAfterScript,
            Refusal::NoScript,
            Refusal::Unreadable("x".into()),
            Refusal::UnknownFormat("2".into()),
            Refusal::BuildInput("_x.html".into()),
            Refusal::AnchorGone("fig-1".into()),
            Refusal::ConcursUnsupported("ask-1".into()),
            Refusal::UnterminatedMirror,
            Refusal::TornIsland,
            Refusal::Overlap,
            Refusal::Symlink,
            Refusal::HardLinked,
            Refusal::NotAFile,
            Refusal::ReadOnly,
            Refusal::BadRevision("x".into()),
        ] {
            let s = r.sentence();
            assert!(s.ends_with('.') && s.len() > 30, "{s}");
            assert!(!s.contains(r.kind()), "a sentence, not a name: {s}");
        }
        for e in [
            WriteError::Changed,
            WriteError::Io {
                why: "disk full".into(),
                backup: Some("/s/b".into()),
            },
        ] {
            assert!(e.sentence().ends_with('.'), "{}", e.sentence());
        }
    }

    /// A save changes only the regions the layout's hash leaves out, so a
    /// saved note keeps the render and its cache entry.
    #[test]
    fn saving_notes_keeps_the_cached_render() {
        use super::super::engine::layout_hash;
        for (name, case) in cases() {
            let Ok(written) = std::fs::read(case.join("expected.html")) else {
                continue;
            };
            let before = std::fs::read(case.join("brief.html")).unwrap();
            assert_ne!(before, written);
            assert_eq!(layout_hash(&before), layout_hash(&written), "{name}");
        }
    }

    /// The read-back after a save holds the written file to the plan: a
    /// note shown where none was written, a note missing, a stamp missing, or
    /// an anchor the page said nothing about, each fails it by name.
    #[test]
    fn a_read_back_that_does_not_show_the_write_is_caught() {
        use super::super::engine::{Anchor as PageAnchor, RectCss};
        let case = fixtures().join("cases/current-pristine");
        let (rev, list) = edits(&case);
        let plan = plan_case(&case, "brief.html", &list, &rev).unwrap();
        let zone = Some(RectCss {
            x: 0.0,
            y: 0.0,
            w: 120.0,
            h: 120.0,
        });
        let shown: Vec<PageAnchor> = anchors(&case)
            .iter()
            .map(|a| PageAnchor {
                nid: a.nid.clone(),
                title: a.title.clone(),
                tag: "div".into(),
                dialog: None,
                rect: None,
                button: None,
                concur_zone: if a.concurrable { zone } else { None },
                has_note: Some(!plan.notes.on(&a.nid).is_empty()),
                has_concur: Some(plan.concurs.as_ref().unwrap().has(&a.nid)),
            })
            .collect();
        assert_eq!(confirm(&plan, &shown), Ok(()));
        let mut wrong = shown.clone();
        wrong[0].has_note = Some(false);
        assert!(confirm(&plan, &wrong).unwrap_err().contains(&wrong[0].nid));
        let mut extra = shown.clone();
        extra[2].has_note = Some(true);
        assert!(
            confirm(&plan, &extra).is_err(),
            "a note shown where none was written"
        );
        let mut unstamped = shown.clone();
        let decision = unstamped
            .iter()
            .position(|a| a.concur_zone.is_some())
            .unwrap();
        unstamped[decision].has_concur = Some(false);
        assert!(confirm(&plan, &unstamped).unwrap_err().contains("stamp"));
        let mut silent = shown;
        silent[1].has_note = None;
        assert!(confirm(&plan, &silent).is_err(), "unknown is not no");
    }

    #[test]
    fn any_other_change_misses_the_cache() {
        use super::super::engine::layout_hash;
        let bytes = std::fs::read(fixtures().join("cases/current-pristine/brief.html")).unwrap();
        let island = find_regions(&bytes).notes.unwrap();
        let mut outside = bytes.clone();
        outside[island.tag_start - 3] ^= 0x01;
        assert_ne!(layout_hash(&bytes), layout_hash(&outside));
        let mut inside = bytes.clone();
        inside[island.start] = b' ';
        assert_eq!(
            layout_hash(&bytes),
            layout_hash(&inside),
            "the island's text is notes"
        );
    }
}
